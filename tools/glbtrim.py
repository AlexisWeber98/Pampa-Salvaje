#!/usr/bin/env python3
"""Trim an animated GLB down to the clips the game plays.

Standard library only; run it isolated: ``python3 -I tools/glbtrim.py``.
The input is treated as untrusted data: it is parsed, never executed, with
bounded sizes and counts and every index checked. Only a self-contained
GLB (one embedded binary buffer, no external URIs) is accepted.

What it does: keeps the animations named with ``--keep``, in that order,
and drops every other one (Quaternius' FBX2glTF exports carry each clip
twice, once bare and once as ``AnimalArmature|<clip>``, and clips the game
never plays, such as Attack, Death and Jump). Then it drops every accessor
and buffer view nothing kept refers to any more, and the binary data only
they used, and writes the rest unchanged: nodes, meshes, skins and
materials keep their indices, so the scene is the same. Texture samplers
are dropped when there are no textures.

It also recomputes smooth vertex normals (``--smooth-angle``, 60 degrees by
default; 0 keeps the file's). The exports are flat-shaded, so low-poly
bodies read faceted. Vertices are welded, for the normals only, by
position and by equal skin joints and weights, never across different
skinning, and across all primitives of a mesh (so no seam shows where two
materials meet). Around each welded point, faces that share an edge
bending less than the angle join one smooth group, as auto smooth does;
each vertex gets the sum of its group's face normals, weighted by their
corner angles. Sharper edges (hoof rims, ear and horn tips) stay sharp. Only the
NORMAL data changes: no vertex is merged, so UV and colour seams stay.

The output is deterministic: the same input, clip list and angle give the
same bytes. See tools/README.md.
"""

import argparse
import json
import math
import os
import struct
import sys

MAX_INPUT_BYTES = 64 * 1024 * 1024
MAX_JSON_BYTES = 16 * 1024 * 1024
MAX_ITEMS = 100_000
GLB_MAGIC = b"glTF"
CHUNK_JSON = 0x4E4F534A
CHUNK_BIN = 0x004E4942
TRIMMER = "trimmed by pampa-seca tools/glbtrim.py"
SMOOTH_ANGLE = 60.0
COMPONENTS = {5120: "b", 5121: "B", 5122: "h", 5123: "H", 5125: "I", 5126: "f"}
WIDTHS = {"SCALAR": 1, "VEC2": 2, "VEC3": 3, "VEC4": 4, "MAT2": 4, "MAT3": 9, "MAT4": 16}


class TrimError(Exception):
    """The input cannot be trimmed safely."""


def read_glb(data):
    """The JSON document and binary chunk of GLB bytes."""
    if len(data) > MAX_INPUT_BYTES:
        raise TrimError(f"input is {len(data)} bytes, over {MAX_INPUT_BYTES}")
    if len(data) < 20 or data[:4] != GLB_MAGIC:
        raise TrimError("not a GLB file (bad magic)")
    version, length = struct.unpack_from("<II", data, 4)
    if version != 2:
        raise TrimError(f"GLB version {version}, only 2 is supported")
    if length != len(data):
        raise TrimError(f"GLB header says {length} bytes, file has {len(data)}")
    offset = 12
    chunks = []
    while offset < length:
        if offset + 8 > length:
            raise TrimError("truncated chunk header")
        size, kind = struct.unpack_from("<II", data, offset)
        start = offset + 8
        if size > length - start:
            raise TrimError("chunk runs past the end of the file")
        chunks.append((kind, data[start : start + size]))
        offset = start + size
    if not chunks or chunks[0][0] != CHUNK_JSON:
        raise TrimError("the first chunk is not JSON")
    if len(chunks[0][1]) > MAX_JSON_BYTES:
        raise TrimError("JSON chunk too large")
    try:
        doc = json.loads(chunks[0][1].decode("utf-8"))
    except (UnicodeDecodeError, json.JSONDecodeError) as e:
        raise TrimError(f"JSON chunk is not valid JSON: {e}") from None
    if not isinstance(doc, dict):
        raise TrimError("JSON chunk is not an object")
    binary = b""
    if len(chunks) > 1:
        if chunks[1][0] != CHUNK_BIN:
            raise TrimError("the second chunk is not BIN")
        binary = chunks[1][1]
    return doc, binary


def items(doc, key):
    """The list `doc[key]`, checked: absent means empty."""
    value = doc.get(key, [])
    if not isinstance(value, list) or len(value) > MAX_ITEMS:
        raise TrimError(f"{key!r} is not a list of at most {MAX_ITEMS}")
    if not all(isinstance(v, dict) for v in value):
        raise TrimError(f"{key!r} holds something other than objects")
    return value


def index(value, count, what):
    """`value` checked as an index into a list of `count` items."""
    if not isinstance(value, int) or isinstance(value, bool) or not 0 <= value < count:
        raise TrimError(f"{what} index {value!r} out of range (0..{count})")
    return value


def check_buffers(doc, binary):
    """One embedded buffer, as large as the BIN chunk allows."""
    buffers = items(doc, "buffers")
    if len(buffers) != 1:
        raise TrimError(f"{len(buffers)} buffers; only one embedded buffer is supported")
    if "uri" in buffers[0]:
        raise TrimError("the buffer has an external URI; only self-contained GLB is supported")
    length = buffers[0].get("byteLength")
    if not isinstance(length, int) or not 0 <= length <= len(binary):
        raise TrimError(f"buffer byteLength {length!r} does not fit the BIN chunk")
    for k, view in enumerate(items(doc, "bufferViews")):
        index(view.get("buffer"), 1, f"bufferView {k} buffer")
        offset = view.get("byteOffset", 0)
        size = view.get("byteLength")
        if not all(isinstance(v, int) and v >= 0 for v in (offset, size)):
            raise TrimError(f"bufferView {k} has a bad offset or length")
        if offset + size > length:
            raise TrimError(f"bufferView {k} runs past its buffer")


def used_accessors(doc, animations):
    """Indices of the accessors the meshes, skins and `animations` use."""
    count = len(items(doc, "accessors"))
    used = set()

    def use(value, what):
        used.add(index(value, count, what))

    for m, mesh in enumerate(items(doc, "meshes")):
        for p, prim in enumerate(mesh.get("primitives", [])):
            where = f"mesh {m} primitive {p}"
            for name, value in prim.get("attributes", {}).items():
                use(value, f"{where} {name}")
            if "indices" in prim:
                use(prim["indices"], f"{where} indices")
            for target in prim.get("targets", []):
                for name, value in target.items():
                    use(value, f"{where} target {name}")
    for s, skin in enumerate(items(doc, "skins")):
        if "inverseBindMatrices" in skin:
            use(skin["inverseBindMatrices"], f"skin {s} inverseBindMatrices")
    for animation in animations:
        name = animation.get("name")
        samplers = animation.get("samplers", [])
        for k, sampler in enumerate(samplers):
            use(sampler.get("input"), f"animation {name!r} sampler {k} input")
            use(sampler.get("output"), f"animation {name!r} sampler {k} output")
        for k, channel in enumerate(animation.get("channels", [])):
            index(channel.get("sampler"), len(samplers), f"animation {name!r} channel {k}")
    return used


def layout(doc, binary, a):
    """Where accessor `a`'s elements lie in `binary`: (start, stride,
    struct format of one element, components, count), all checked."""
    accessors = items(doc, "accessors")
    accessor = accessors[index(a, len(accessors), "accessor")]
    if "sparse" in accessor:
        raise TrimError(f"accessor {a} is sparse; not supported")
    kind = COMPONENTS.get(accessor.get("componentType"))
    width = WIDTHS.get(accessor.get("type"))
    count = accessor.get("count")
    if kind is None or width is None or not isinstance(count, int) or count < 0:
        raise TrimError(f"accessor {a} has a bad type or count")
    views = items(doc, "bufferViews")
    view = views[index(accessor.get("bufferView"), len(views), f"accessor {a} bufferView")]
    fmt = "<" + kind * width
    size = struct.calcsize(fmt)
    stride = view.get("byteStride", size)
    offset = accessor.get("byteOffset", 0)
    if not isinstance(stride, int) or stride < size or not isinstance(offset, int) or offset < 0:
        raise TrimError(f"accessor {a} has a bad stride or offset")
    start = view.get("byteOffset", 0) + offset
    end = start + stride * (count - 1) + size if count else start
    if end > view.get("byteOffset", 0) + view["byteLength"] or end > len(binary):
        raise TrimError(f"accessor {a} runs past its buffer view")
    return start, stride, fmt, width, count


def read_accessor(doc, binary, a):
    """Accessor `a`'s elements, as tuples."""
    start, stride, fmt, _, count = layout(doc, binary, a)
    return [struct.unpack_from(fmt, binary, start + stride * k) for k in range(count)]


def raw_accessor(doc, binary, a):
    """Accessor `a`'s elements, as raw bytes (exact equality)."""
    start, stride, fmt, _, count = layout(doc, binary, a)
    size = struct.calcsize(fmt)
    return [bytes(binary[start + stride * k : start + stride * k + size]) for k in range(count)]


def sub(a, b):
    return (a[0] - b[0], a[1] - b[1], a[2] - b[2])


def dot(a, b):
    return a[0] * b[0] + a[1] * b[1] + a[2] * b[2]


def cross(a, b):
    return (a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0])


def unit(v):
    """`v` normalized, or None when it has no length."""
    length = math.sqrt(dot(v, v))
    return None if length < 1e-20 else (v[0] / length, v[1] / length, v[2] / length)


def corner_angle(at, a, b):
    """The angle at `at` between the edges to `a` and `b`."""
    u, v = unit(sub(a, at)), unit(sub(b, at))
    if u is None or v is None:
        return 0.0
    return math.acos(max(-1.0, min(1.0, dot(u, v))))


def smooth_normals(doc, binary, degrees):
    """`binary` with every mesh's NORMAL data recomputed smooth within
    `degrees` (see the module docs); everything else unchanged."""
    out = bytearray(binary)
    limit = math.cos(math.radians(degrees))
    claimed = set()
    for m, mesh in enumerate(items(doc, "meshes")):
        # Every vertex of the mesh: its weld key, position and where its
        # normal is written; every triangle by those global indices.
        keys, positions, targets, faces = [], [], [], []
        for p, prim in enumerate(mesh.get("primitives", [])):
            attributes = prim.get("attributes", {})
            if prim.get("mode", 4) != 4 or "NORMAL" not in attributes:
                continue
            normal = attributes["NORMAL"]
            if normal in claimed:
                raise TrimError(f"mesh {m} primitive {p} shares its NORMAL accessor")
            claimed.add(normal)
            nstart, nstride, nfmt, _, ncount = layout(doc, binary, normal)
            if nfmt != "<fff":
                raise TrimError(f"mesh {m} primitive {p} NORMAL is not float VEC3")
            if "POSITION" not in attributes:
                raise TrimError(f"mesh {m} primitive {p} has no POSITION")
            if layout(doc, binary, attributes["POSITION"])[2] != "<fff":
                raise TrimError(f"mesh {m} primitive {p} POSITION is not float VEC3")
            points = read_accessor(doc, binary, attributes["POSITION"])
            if len(points) != ncount:
                raise TrimError(f"mesh {m} primitive {p} POSITION does not match NORMAL")
            skin = [
                raw_accessor(doc, binary, attributes[name])
                for name in ("JOINTS_0", "WEIGHTS_0")
                if name in attributes
            ]
            if any(len(column) != ncount for column in skin):
                raise TrimError(f"mesh {m} primitive {p} skin data does not match POSITION")
            base = len(positions)
            for k, point in enumerate(points):
                keys.append((point,) + tuple(column[k] for column in skin))
                positions.append(point)
                targets.append((normal, nstart + nstride * k))
            if "indices" in prim:
                corners = [i[0] for i in read_accessor(doc, binary, prim["indices"])]
            else:
                corners = list(range(len(points)))
            if any(not 0 <= i < len(points) for i in corners):
                raise TrimError(f"mesh {m} primitive {p} has an index out of range")
            for t in range(0, len(corners) - 2, 3):
                faces.append(tuple(base + i for i in corners[t : t + 3]))
        if not positions:
            continue
        # Weld ids, each face's normal, and each vertex's faces with its
        # corner angle.
        ids = {}
        weld = [ids.setdefault(key, len(ids)) for key in keys]
        own = [[] for _ in positions]
        fans = [[] for _ in ids]
        normals = []
        for f, (a, b, c) in enumerate(faces):
            pa, pb, pc = positions[a], positions[b], positions[c]
            n = unit(cross(sub(pb, pa), sub(pc, pa)))
            normals.append(n)
            if n is None:
                continue
            for v, angle in (
                (a, corner_angle(pa, pb, pc)),
                (b, corner_angle(pb, pc, pa)),
                (c, corner_angle(pc, pa, pb)),
            ):
                own[v].append((f, angle))
                fans[weld[v]].append((f, angle))
        # Around each welded point, faces sharing an edge that bends less
        # than the angle join one smooth group (like auto smooth): a corner
        # takes its own face's group.
        groups = []
        for w, fan in enumerate(fans):
            parent = {f: f for f, _ in fan}

            def root(f):
                while parent[f] != f:
                    parent[f] = parent[parent[f]]
                    f = parent[f]
                return f

            edges = {}
            for f, _ in fan:
                for other in {weld[v] for v in faces[f]} - {w}:
                    edges.setdefault(other, []).append(f)
            for shared in edges.values():
                for x in range(len(shared)):
                    for y in range(x + 1, len(shared)):
                        f, g = shared[x], shared[y]
                        if dot(normals[f], normals[g]) >= limit:
                            parent[root(f)] = root(g)
            groups.append({f: root(f) for f, _ in fan})

        def weighted(corners):
            """The unit sum of the corners' face normals, by angle."""
            x = y = z = 0.0
            for f, angle in corners:
                n = normals[f]
                x, y, z = x + n[0] * angle, y + n[1] * angle, z + n[2] * angle
            return unit((x, y, z))

        for v in range(len(positions)):
            if not own[v]:
                continue
            group = groups[weld[v]]
            mine = {group[f] for f, _ in own[v]}
            near = [(f, a) for f, a in fans[weld[v]] if group[f] in mine]
            smooth = weighted(near) or normals[own[v][0][0]]
            _, offset = targets[v]
            struct.pack_into("<fff", out, offset, *smooth)
        # Keep the bounds of every rewritten NORMAL accessor true.
        accessors = items(doc, "accessors")
        for normal in {t[0] for t in targets}:
            accessor = accessors[normal]
            if "min" in accessor or "max" in accessor:
                values = read_accessor(doc, out, normal)
                accessor["min"] = [min(v[k] for v in values) for k in range(3)]
                accessor["max"] = [max(v[k] for v in values) for k in range(3)]
    return bytes(out)


def trim(data, keep, smooth_angle=SMOOTH_ANGLE):
    """The GLB `data` with only the animations named in `keep`, in that
    order, and only the data they and the scene still use; with smooth
    normals within `smooth_angle` degrees, unless it is 0."""
    doc, binary = read_glb(data)
    if items(doc, "images") or doc.get("extensionsRequired"):
        raise TrimError("images and required extensions are not supported")
    check_buffers(doc, binary)
    if not 0 <= smooth_angle <= 180:
        raise TrimError(f"smooth angle {smooth_angle} is not within 0..180 degrees")
    if smooth_angle > 0:
        binary = smooth_normals(doc, binary, smooth_angle)
    animations = items(doc, "animations")
    by_name = {}
    for animation in animations:
        name = animation.get("name")
        if isinstance(name, str):
            by_name.setdefault(name, animation)
    if len(set(keep)) != len(keep):
        raise TrimError("a clip is named twice in --keep")
    missing = [name for name in keep if name not in by_name]
    if missing:
        raise TrimError(f"no animation named {', '.join(missing)}")
    kept = [by_name[name] for name in keep]

    accessors = items(doc, "accessors")
    views = items(doc, "bufferViews")
    used = sorted(used_accessors(doc, kept))
    for a in used:
        if "sparse" in accessors[a]:
            raise TrimError(f"accessor {a} is sparse; not supported")
        index(accessors[a].get("bufferView"), len(views), f"accessor {a} bufferView")
    used_views = sorted({accessors[a]["bufferView"] for a in used})

    # The kept views' bytes, in their old order, each 4-byte aligned.
    view_map, new_views, chunks, size = {}, [], [], 0
    for v in used_views:
        view = dict(views[v])
        start = view.get("byteOffset", 0)
        piece = binary[start : start + view["byteLength"]]
        pad = -size % 4
        chunks.append(b"\0" * pad + piece)
        size += pad
        view["byteOffset"] = size
        size += len(piece)
        view_map[v] = len(new_views)
        new_views.append(view)
    accessor_map = {}
    new_accessors = []
    for a in used:
        accessor = dict(accessors[a])
        accessor["bufferView"] = view_map[accessor["bufferView"]]
        accessor_map[a] = len(new_accessors)
        new_accessors.append(accessor)

    for mesh in items(doc, "meshes"):
        for prim in mesh.get("primitives", []):
            prim["attributes"] = {
                k: accessor_map[v] for k, v in prim.get("attributes", {}).items()
            }
            if "indices" in prim:
                prim["indices"] = accessor_map[prim["indices"]]
            prim_targets = prim.get("targets")
            if prim_targets:
                prim["targets"] = [{k: accessor_map[v] for k, v in t.items()} for t in prim_targets]
    for skin in items(doc, "skins"):
        if "inverseBindMatrices" in skin:
            skin["inverseBindMatrices"] = accessor_map[skin["inverseBindMatrices"]]
    for animation in kept:
        for sampler in animation["samplers"]:
            sampler["input"] = accessor_map[sampler["input"]]
            sampler["output"] = accessor_map[sampler["output"]]

    doc["animations"] = kept
    doc["accessors"] = new_accessors
    doc["bufferViews"] = new_views
    blob = b"".join(chunks)
    blob += b"\0" * (-len(blob) % 4)
    doc["buffers"] = [{"byteLength": len(blob)}]
    if not items(doc, "textures"):
        doc.pop("samplers", None)
    asset = doc.setdefault("asset", {})
    generator = str(asset.get("generator", "")).strip()
    if TRIMMER not in generator:
        asset["generator"] = f"{generator}; {TRIMMER}" if generator else TRIMMER
    return write_glb(doc, blob)


def write_glb(doc, blob):
    """GLB bytes of `doc` and its binary chunk `blob`."""
    text = json.dumps(doc, separators=(",", ":"), ensure_ascii=False).encode("utf-8")
    text += b" " * (-len(text) % 4)
    parts = [struct.pack("<II", len(text), CHUNK_JSON), text]
    if blob:
        parts += [struct.pack("<II", len(blob), CHUNK_BIN), blob]
    body = b"".join(parts)
    return GLB_MAGIC + struct.pack("<II", 2, 12 + len(body)) + body


def summary(data):
    """Bytes, triangles, joints and clip names of GLB `data`."""
    doc, _ = read_glb(data)
    accessors = items(doc, "accessors")
    triangles = 0
    for mesh in items(doc, "meshes"):
        for prim in mesh.get("primitives", []):
            if "indices" in prim:
                triangles += accessors[prim["indices"]].get("count", 0) // 3
    joints = sum(len(s.get("joints", [])) for s in items(doc, "skins"))
    clips = [a.get("name") for a in items(doc, "animations")]
    return len(data), triangles, joints, clips


def main(argv=None):
    parser = argparse.ArgumentParser(
        description="Keep only the named animation clips of a GLB and drop the data nothing uses."
    )
    parser.add_argument("input", help="the GLB to read (left unchanged)")
    parser.add_argument("output", help="the trimmed GLB to write")
    parser.add_argument(
        "--keep", nargs="+", required=True, metavar="CLIP", help="the clips to keep, in order"
    )
    parser.add_argument(
        "--smooth-angle",
        type=float,
        default=SMOOTH_ANGLE,
        metavar="DEGREES",
        help=f"recompute smooth normals, keeping edges sharper than this (default {SMOOTH_ANGLE:g}; 0 keeps the file's)",
    )
    args = parser.parse_args(argv)
    try:
        with open(args.input, "rb") as f:
            data = f.read(MAX_INPUT_BYTES + 1)
        out = trim(data, args.keep, args.smooth_angle)
    except (OSError, TrimError) as e:
        print(f"glbtrim: {e}", file=sys.stderr)
        return 1
    folder = os.path.dirname(os.path.abspath(args.output))
    os.makedirs(folder, exist_ok=True)
    with open(args.output, "wb") as f:
        f.write(out)
    before, after = summary(data), summary(out)
    print(
        f"{os.path.basename(args.output)}: {before[0]} -> {after[0]} bytes, "
        f"{after[1]} triangles, {after[2]} joints, clips {', '.join(after[3])} "
        f"(from {len(before[3])})"
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
