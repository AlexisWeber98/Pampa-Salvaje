#!/usr/bin/env python3
"""Convert a COLLADA (.dae) model into a glTF 2.0 model slot.

Standard library only; run it isolated: ``python3 -I tools/dae2gltf.py``.
The input is treated as untrusted: no DTDs or entities, bounded sizes and
counts, every index checked, and texture files resolved by base name inside
the model's own folder only. See tools/README.md for usage.

What it reads: <triangles>, <polylist> and <polygons> (fanned into
triangles); positions, normals and the first bound texture coordinate set;
node transforms (matrix, translate, rotate, scale); <up_axis> and
<unit meter>; per material the diffuse texture (or colour), the normal map
(a <bump> texture, else a sidecar ``<diffuse>_NRM.png``) and the shininess.
Specular maps are ignored: glTF has no Blinn specular, and a roughness
guessed from shininess reads better than a repacked spec map.

What it writes, into the output folder: ``model.gltf``, its ``model.bin``
and a ``textures/`` folder. Every node's transform is baked into the
vertices (negative scales flip the winding), so the scene is one static
``body`` node plus, with ``--rotor``, a ``rotor`` node: the geometry under
that COLLADA node, its origin at the node's pivot, to be spun about the
axis this tool prints (glTF space: Y up, meters).
"""

import argparse
import json
import math
import os
import re
import shutil
import struct
import sys
import urllib.parse
import xml.etree.ElementTree as ET
import zlib

NS = "{http://www.collada.org/2005/11/COLLADASchema}"
MAX_INPUT_BYTES = 64 * 1024 * 1024
MAX_FLOATS = 8_000_000
MAX_INDICES = 16_000_000
MAX_IMAGE_PIXELS = 4096 * 4096
SAFE_NAME = re.compile(r"^[A-Za-z0-9][A-Za-z0-9._ -]{0,127}$")


class ConvertError(Exception):
    """The input cannot be converted safely."""


# Internal geometry group keys; the glTF node names are "body" and
# --rotor-name, which must differ.
BODY = "__body__"
ROTOR = "__rotor__"
IMAGE_TYPES = {".png": "image/png", ".jpg": "image/jpeg", ".jpeg": "image/jpeg"}
IMAGE_MAGIC = {"image/png": b"\x89PNG\r\n\x1a\n", "image/jpeg": b"\xff\xd8\xff"}


def f32(x):
    """The value a float32 stores for x (accessor bounds must match it)."""
    return struct.unpack("<f", struct.pack("<f", x))[0]


def image_type(path):
    """The glTF MIME type of a texture file, by extension and content; PNG
    and JPEG are the only ones glTF 2.0 core allows."""
    mime = IMAGE_TYPES.get(os.path.splitext(path)[1].lower())
    if mime is None:
        raise ConvertError(f"texture {os.path.basename(path)!r} is not a PNG or JPEG")
    with open(path, "rb") as f:
        head = f.read(8)
    if not head.startswith(IMAGE_MAGIC[mime]):
        raise ConvertError(f"texture {os.path.basename(path)!r} is not a PNG or JPEG by its content")
    return mime


# --- small matrix helpers (row-major 4x4, column vectors) -------------------


def identity():
    return [1.0, 0, 0, 0, 0, 1.0, 0, 0, 0, 0, 1.0, 0, 0, 0, 0, 1.0]


def mul(a, b):
    return [
        sum(a[r * 4 + k] * b[k * 4 + c] for k in range(4))
        for r in range(4)
        for c in range(4)
    ]


def translation(x, y, z):
    m = identity()
    m[3], m[7], m[11] = x, y, z
    return m


def scaling(x, y, z):
    m = identity()
    m[0], m[5], m[10] = x, y, z
    return m


def rotation(ax, ay, az, degrees):
    n = math.sqrt(ax * ax + ay * ay + az * az)
    if n == 0:
        return identity()
    x, y, z = ax / n, ay / n, az / n
    a = math.radians(degrees)
    c, s, t = math.cos(a), math.sin(a), 1 - math.cos(a)
    return [
        t * x * x + c, t * x * y - s * z, t * x * z + s * y, 0,
        t * x * y + s * z, t * y * y + c, t * y * z - s * x, 0,
        t * x * z - s * y, t * y * z + s * x, t * z * z + c, 0,
        0, 0, 0, 1.0,
    ]


def apply(m, p):
    x, y, z = p
    return (
        m[0] * x + m[1] * y + m[2] * z + m[3],
        m[4] * x + m[5] * y + m[6] * z + m[7],
        m[8] * x + m[9] * y + m[10] * z + m[11],
    )


def linear(m, v):
    x, y, z = v
    return (
        m[0] * x + m[1] * y + m[2] * z,
        m[4] * x + m[5] * y + m[6] * z,
        m[8] * x + m[9] * y + m[10] * z,
    )


def det3(m):
    return (
        m[0] * (m[5] * m[10] - m[6] * m[9])
        - m[1] * (m[4] * m[10] - m[6] * m[8])
        + m[2] * (m[4] * m[9] - m[5] * m[8])
    )


def normal_matrix(m):
    """The inverse transpose of the linear part, as a 3x3 row-major list."""
    d = det3(m)
    if abs(d) < 1e-20:
        raise ConvertError("a node transform is singular")
    a, b, c, e, f, g, h, i, j = m[0], m[1], m[2], m[4], m[5], m[6], m[8], m[9], m[10]
    # Cofactor matrix over the determinant is the inverse transpose.
    return [
        (f * j - g * i) / d, -(e * j - g * h) / d, (e * i - f * h) / d,
        -(b * j - c * i) / d, (a * j - c * h) / d, -(a * i - b * h) / d,
        (b * g - c * f) / d, -(a * g - c * e) / d, (a * f - b * e) / d,
    ]


def normalized(v):
    n = math.sqrt(sum(c * c for c in v))
    return tuple(c / n for c in v) if n > 1e-12 else (0.0, 1.0, 0.0)


# --- safe parsing ------------------------------------------------------------


def read_xml(path):
    size = os.path.getsize(path)
    if size > MAX_INPUT_BYTES:
        raise ConvertError(f"{path} is {size} bytes; the limit is {MAX_INPUT_BYTES}")
    with open(path, "rb") as f:
        data = f.read()
    # The DTD check below is a byte search, so it only sees ASCII-compatible
    # text: refuse UTF-16/32 (BOM-detected by the parser, and full of NULs)
    # and any declared encoding but UTF-8 or ASCII.
    if b"\x00" in data or data[:2] in (b"\xff\xfe", b"\xfe\xff"):
        raise ConvertError("the document is not UTF-8; only UTF-8 encoding is accepted")
    declared = re.match(rb"(?:\xef\xbb\xbf)?<\?xml[^>]*encoding=[\"']([A-Za-z0-9._-]+)", data)
    if declared and declared.group(1).lower() not in (b"utf-8", b"utf8", b"us-ascii", b"ascii"):
        raise ConvertError(f"the document declares encoding {declared.group(1).decode()!r}; only UTF-8 is accepted")
    # No DTDs: they carry entities (expansion bombs, external files).
    if b"<!DOCTYPE" in data or b"<!ENTITY" in data:
        raise ConvertError("the document declares a DTD or entities; refusing it")
    try:
        return ET.fromstring(data)
    except ET.ParseError as e:
        raise ConvertError(f"not well-formed XML: {e}") from e


def floats(text, limit=MAX_FLOATS):
    parts = (text or "").split()
    if len(parts) > limit:
        raise ConvertError("an array is too large")
    out = []
    for p in parts:
        v = float(p)
        if not math.isfinite(v):
            raise ConvertError("an array holds a non-finite number")
        out.append(v)
    return out


def ints(text, limit=MAX_INDICES):
    parts = (text or "").split()
    if len(parts) > limit:
        raise ConvertError("an index list is too large")
    out = []
    for p in parts:
        v = int(p)
        if v < 0:
            raise ConvertError("a negative index")
        out.append(v)
    return out


def ref(url):
    """The id a local '#id' URL points at."""
    if not url or not url.startswith("#"):
        raise ConvertError(f"unsupported reference {url!r} (only local '#id')")
    return url[1:]


def by_id(root):
    return {e.get("id"): e for e in root.iter() if e.get("id")}


# --- textures ----------------------------------------------------------------


def image_file(init_from, folder):
    """The texture file an <init_from> names, looked up by base name in the
    model's own folder (authoring paths are absolute and foreign). The exact
    name wins; otherwise a single case-insensitive match is taken (Windows
    exports), and several are refused as ambiguous."""
    path = urllib.parse.unquote((init_from or "").strip())
    name = re.split(r"[\\/]", path)[-1]
    if not SAFE_NAME.match(name) or name in (".", ".."):
        raise ConvertError(f"unsafe texture name {name!r}")
    names = os.listdir(folder)
    if name in names:
        candidates = [name]
    else:
        candidates = [n for n in names if n.lower() == name.lower()]
    if not candidates:
        return None
    if len(candidates) > 1:
        raise ConvertError(f"texture {name!r} is ambiguous: {sorted(candidates)} differ only in case")
    full = os.path.realpath(os.path.join(folder, candidates[0]))
    if os.path.dirname(full) != os.path.realpath(folder) or not os.path.isfile(full):
        return None
    return full


def png_read(path):
    """Decodes an 8-bit, non-interlaced grey/RGB/RGBA PNG: (w, h, channels,
    rows of bytes)."""
    with open(path, "rb") as f:
        data = f.read()
    if data[:8] != b"\x89PNG\r\n\x1a\n":
        raise ConvertError(f"{path} is not a PNG")
    pos, idat, header = 8, [], None
    while pos + 8 <= len(data):
        (length,) = struct.unpack(">I", data[pos : pos + 4])
        kind = data[pos + 4 : pos + 8]
        body = data[pos + 8 : pos + 8 + length]
        pos += 12 + length
        if kind == b"IHDR":
            header = struct.unpack(">IIBBBBB", body)
        elif kind == b"IDAT":
            idat.append(body)
        elif kind == b"IEND":
            break
    if header is None:
        raise ConvertError(f"{path} has no header")
    w, h, depth, colour, _, _, interlace = header
    channels = {0: 1, 2: 3, 4: 2, 6: 4}.get(colour)
    if depth != 8 or channels is None or interlace != 0 or w * h > MAX_IMAGE_PIXELS:
        raise ConvertError(f"{path}: unsupported PNG (8-bit, non-interlaced only)")
    stride = w * channels
    expected = (stride + 1) * h
    raw = zlib.decompressobj().decompress(b"".join(idat), expected + 1)
    if len(raw) != expected:
        raise ConvertError(f"{path}: truncated image data")
    rows, prev = [], bytearray(stride)
    for y in range(h):
        kind = raw[y * (stride + 1)]
        line = bytearray(raw[y * (stride + 1) + 1 : (y + 1) * (stride + 1)])
        for x in range(stride):
            a = line[x - channels] if x >= channels else 0
            b = prev[x]
            c = prev[x - channels] if x >= channels else 0
            if kind == 1:
                line[x] = (line[x] + a) & 255
            elif kind == 2:
                line[x] = (line[x] + b) & 255
            elif kind == 3:
                line[x] = (line[x] + (a + b) // 2) & 255
            elif kind == 4:
                p = a + b - c
                pa, pb, pc = abs(p - a), abs(p - b), abs(p - c)
                pred = a if pa <= pb and pa <= pc else (b if pb <= pc else c)
                line[x] = (line[x] + pred) & 255
            elif kind != 0:
                raise ConvertError(f"{path}: bad filter {kind}")
        rows.append(line)
        prev = line
    return w, h, channels, rows


def png_write(path, w, h, channels, rows):
    colour = {1: 0, 2: 4, 3: 2, 4: 6}[channels]

    def chunk(kind, body):
        crc = zlib.crc32(kind + body) & 0xFFFFFFFF
        return struct.pack(">I", len(body)) + kind + body + struct.pack(">I", crc)

    raw = b"".join(b"\x00" + bytes(r) for r in rows)
    with open(path, "wb") as f:
        f.write(b"\x89PNG\r\n\x1a\n")
        f.write(chunk(b"IHDR", struct.pack(">IIBBBBB", w, h, 8, colour, 0, 0, 0)))
        f.write(chunk(b"IDAT", zlib.compress(raw, 9)))
        f.write(chunk(b"IEND", b""))


def green_convention(w, h, channels, rows):
    """'gl' (green up, as glTF wants) or 'dx' (green down), and how sure: a
    real height field's normals are curl-free, which holds for only one
    sign of green. Returns the ratio of the GL residual to the DX one."""
    if channels < 3:
        return "gl", 1.0
    step = max(1, min(w, h) // 256)
    gl = dx = 0.0
    for y in range(step, h - step, step):
        up, here, down = rows[y - step], rows[y], rows[y + step]
        for x in range(step, w - step, step):
            dr = (down[x * channels] - up[x * channels]) / 127.5
            dg = (here[(x + step) * channels + 1] - here[(x - step) * channels + 1]) / 127.5
            gl += (dr + dg) ** 2
            dx += (dr - dg) ** 2
    ratio = gl / dx if dx > 0 else 1.0
    return ("gl" if ratio <= 1.0 else "dx"), ratio


# --- the model -----------------------------------------------------------


class Material:
    def __init__(self, name):
        self.name = name
        self.color = (0.8, 0.8, 0.8, 1.0)
        self.diffuse = None  # texture file
        self.normal = None  # texture file
        self.roughness = 0.85


def read_effect(effect, elements, folder):
    """The diffuse texture or colour, normal map and roughness of an effect."""
    m = Material(effect.get("name") or effect.get("id"))
    params = {}
    for p in effect.iter(NS + "newparam"):
        params[p.get("sid")] = p

    def texture_file(tex):
        name = tex.get("texture")
        param = params.get(name)
        if param is not None and param.find(NS + "sampler2D") is not None:
            source = param.find(NS + "sampler2D/" + NS + "source")
            surface = params.get(source.text.strip() if source is not None else "")
            init = surface.find(".//" + NS + "init_from") if surface is not None else None
            name = init.text.strip() if init is not None and init.text else None
        image = elements.get(name)
        if image is None or image.tag != NS + "image":
            return None
        init = image.find(NS + "init_from")
        return image_file(init.text if init is not None else "", folder)

    technique = effect.find(".//" + NS + "technique[@sid='common']")
    shader = None
    if technique is not None:
        for kind in ("blinn", "phong", "lambert", "constant"):
            shader = technique.find(NS + kind)
            if shader is not None:
                break
    if shader is not None:
        diffuse = shader.find(NS + "diffuse")
        if diffuse is not None:
            tex = diffuse.find(NS + "texture")
            colour = diffuse.find(NS + "color")
            if tex is not None:
                m.diffuse = texture_file(tex)
            elif colour is not None:
                c = floats(colour.text, 4)
                if len(c) >= 3:
                    m.color = (c[0], c[1], c[2], c[3] if len(c) > 3 else 1.0)
        shine = shader.find(NS + "shininess/" + NS + "float")
        if shine is not None:
            # 3ds Max writes its glossiness percentage here. Roughness is
            # its complement, clamped: below 0.45 the worn wood and rusty
            # steel of these models read as wet plastic under the sun, and
            # above 0.95 they lose every highlight and look like chalk.
            g = min(max(floats(shine.text, 1)[0], 0.0), 100.0)
            m.roughness = min(max(1.0 - g / 100.0, 0.45), 0.95)
    bump = effect.find(".//" + NS + "bump/" + NS + "texture")
    if bump is not None:
        m.normal = texture_file(bump)
    if m.normal is None and m.diffuse is not None:
        stem, _ = os.path.splitext(m.diffuse)
        for suffix in ("_NRM.png", "_nrm.png", "_N.png", "_normal.png"):
            if os.path.isfile(stem + suffix):
                m.normal = stem + suffix
                break
    return m


class Geometry:
    """One COLLADA mesh: its float sources by id ((values, stride)), its
    <vertices> inputs by id, and its primitives as (material symbol,
    element, kind) parts, triangulated on demand by ``triangles``."""

    def __init__(self, element, elements):
        mesh = element.find(NS + "mesh")
        if mesh is None:
            raise ConvertError(f"geometry {element.get('id')} is not a mesh")
        self.sources = {}
        for s in mesh.findall(NS + "source"):
            array = s.find(NS + "float_array")
            accessor = s.find(".//" + NS + "accessor")
            if array is None or accessor is None:
                continue
            stride = int(accessor.get("stride", "1"))
            values = floats(array.text)
            if stride < 1 or stride > 16 or len(values) < stride * int(accessor.get("count", "0")):
                raise ConvertError(f"source {s.get('id')} is shorter than its accessor")
            self.sources[s.get("id")] = (values, stride)
        self.vertices = {}
        for v in mesh.findall(NS + "vertices"):
            self.vertices[v.get("id")] = {
                i.get("semantic"): ref(i.get("source")) for i in v.findall(NS + "input")
            }
        self.parts = []
        for prim in mesh:
            kind = prim.tag.replace(NS, "")
            if kind in ("triangles", "polylist", "polygons"):
                self.parts.append((prim.get("material"), prim, kind))
            elif kind not in ("source", "vertices", "extra"):
                print(f"  skipping <{kind}> (unsupported)", file=sys.stderr)

    def source(self, sid, minimum):
        if sid not in self.sources:
            raise ConvertError(f"missing source {sid}")
        values, stride = self.sources[sid]
        if stride < minimum:
            raise ConvertError(f"source {sid} has stride {stride}")
        return values, stride

    def triangles(self, prim, kind, texcoord_set):
        """(positions, normals, uvs, [((p, n, t), ...) triangle corners])."""
        inputs = prim.findall(NS + "input")
        width = max(int(i.get("offset", "0")) for i in inputs) + 1
        pos = nrm = uv = None
        uv_set = None
        for i in inputs:
            semantic, offset = i.get("semantic"), int(i.get("offset", "0"))
            if semantic == "VERTEX":
                vertices = self.vertices.get(ref(i.get("source")), {})
                pos = (offset, vertices.get("POSITION"))
                if "NORMAL" in vertices and nrm is None:
                    nrm = (offset, vertices["NORMAL"])
            elif semantic == "NORMAL":
                nrm = (offset, ref(i.get("source")))
            elif semantic == "TEXCOORD":
                s = int(i.get("set", "0"))
                if uv is None or s == texcoord_set or (uv_set != texcoord_set and s < uv_set):
                    uv, uv_set = (offset, ref(i.get("source"))), s
        if pos is None or pos[1] is None:
            raise ConvertError("a primitive has no positions")
        if kind == "polygons":
            polys = [ints(p.text) for p in prim.findall(NS + "p")]
            if prim.find(NS + "ph") is not None:
                print("  skipping <ph> polygons with holes", file=sys.stderr)
        else:
            p = prim.find(NS + "p")
            flat = ints(p.text if p is not None else "")
            if kind == "triangles":
                counts = [3] * int(prim.get("count", "0"))
            else:
                vcount = prim.find(NS + "vcount")
                if vcount is None:
                    raise ConvertError("a <polylist> has no <vcount>")
                counts = ints(vcount.text)
            polys, at = [], 0
            for n in counts:
                polys.append(flat[at : at + n * width])
                at += n * width
            if at > len(flat):
                raise ConvertError("a primitive's index list is too short")
        positions = self.source(pos[1], 3)
        normals = self.source(nrm[1], 3) if nrm else None
        uvs = self.source(uv[1], 2) if uv else None
        corners = []
        for poly in polys:
            n = len(poly) // width
            if n < 3:
                continue
            refs = []
            for k in range(n):
                group = poly[k * width : (k + 1) * width]
                refs.append(
                    (
                        group[pos[0]],
                        group[nrm[0]] if nrm else None,
                        group[uv[0]] if uv else None,
                    )
                )
            for k in range(1, n - 1):
                corners.append((refs[0], refs[k], refs[k + 1]))
        # Indices are checked against their sources as they are used.
        ids = (pos[1], nrm[1] if nrm else None, uv[1] if uv else None)
        return positions, normals, uvs, corners, ids


def node_matrix(node):
    """The node's local transform, and its elements in order."""
    m = identity()
    elements = []
    for e in node:
        tag = e.tag.replace(NS, "")
        if tag == "matrix":
            v = floats(e.text, 16)
            if len(v) != 16:
                raise ConvertError("a matrix needs 16 numbers")
            local = v
        elif tag == "translate":
            local = translation(*floats(e.text, 3)[:3])
        elif tag == "rotate":
            local = rotation(*floats(e.text, 4)[:4])
        elif tag == "scale":
            local = scaling(*floats(e.text, 3)[:3])
        elif tag in ("lookat", "skew"):
            raise ConvertError(f"<{tag}> transforms are not supported")
        else:
            continue
        elements.append((e, local))
        m = mul(m, local)
    return m, elements


def axis_root(up_axis, unit):
    """From the document's axes and units to glTF's: Y up, meters."""
    if up_axis == "Z_UP":
        c = [1.0, 0, 0, 0, 0, 0, 1.0, 0, 0, -1.0, 0, 0, 0, 0, 0, 1.0]
    elif up_axis == "X_UP":
        c = [0, -1.0, 0, 0, 1.0, 0, 0, 0, 0, 0, 1.0, 0, 0, 0, 0, 1.0]
    else:
        c = identity()
    return mul(scaling(unit, unit, unit), c)


class Batch:
    """Deduplicated vertices and triangle indices of one glTF primitive."""

    def __init__(self):
        self.index = {}
        self.positions, self.normals, self.uvs, self.triangles = [], [], [], []

    def vertex(self, key, p, n, t):
        if key not in self.index:
            self.index[key] = len(self.positions)
            self.positions.append(p)
            self.normals.append(n)
            self.uvs.append(t)
        return self.index[key]


def convert(args):
    src = os.path.realpath(args.input)
    folder = os.path.dirname(src)
    root = read_xml(src)
    if root.tag != NS + "COLLADA":
        raise ConvertError("not a COLLADA 1.4 document")
    elements = by_id(root)
    asset = root.find(NS + "asset")
    up = asset.findtext(NS + "up_axis", "Y_UP").strip() if asset is not None else "Y_UP"
    unit_el = asset.find(NS + "unit") if asset is not None else None
    unit = float(unit_el.get("meter", "1")) if unit_el is not None else 1.0
    if not (math.isfinite(unit) and unit > 0):
        raise ConvertError("bad <unit>")
    base = mul(scaling(args.scale, args.scale, args.scale), axis_root(up, unit))

    # Animated targets: '<node id>/<sid>' of every animation channel (its
    # target minus any '.ANGLE'-style member); the rotor's axis is the
    # first of its rotations found here.
    animated = set()
    for channel in root.iter(NS + "channel"):
        target = (channel.get("target") or "").split(".")[0]
        animated.add(target)

    materials = {}

    def material(symbol, bindings):
        mid = bindings.get(symbol, symbol)
        if mid not in materials:
            m = elements.get(mid)
            effect = None
            if m is not None and m.find(NS + "instance_effect") is not None:
                effect = elements.get(ref(m.find(NS + "instance_effect").get("url")))
            if effect is None:
                materials[mid] = Material(mid)
            else:
                materials[mid] = read_effect(effect, elements, folder)
                materials[mid].name = (m.get("name") or mid)
        return mid

    # Internal keys, independent of the output node names (``write``).
    groups = {BODY: {}, ROTOR: {}}
    pivot, axis = None, None
    geometries = {}

    def visit(node, parent, in_rotor):
        nonlocal pivot, axis
        local, steps = node_matrix(node)
        world = mul(parent, local)
        is_rotor = args.rotor is not None and args.rotor in (node.get("name"), node.get("id"))
        if is_rotor:
            if pivot is not None:
                raise ConvertError(f"two nodes are named {args.rotor!r}")
            pivot = apply(world, (0.0, 0.0, 0.0))
            before = parent
            for e, m in steps:
                sid = e.get("sid")
                if e.tag == NS + "rotate" and sid and f"{node.get('id')}/{sid}" in animated:
                    axis = normalized(linear(before, floats(e.text, 4)[:3]))
                    break
                before = mul(before, m)
        rotor = in_rotor or is_rotor
        for inst in node.findall(NS + "instance_geometry"):
            gid = ref(inst.get("url"))
            if gid not in geometries:
                element = elements.get(gid)
                if element is None or element.tag != NS + "geometry":
                    raise ConvertError(f"missing geometry {gid}")
                geometries[gid] = Geometry(element, elements)
            bindings, sets = {}, {}
            for im in inst.iter(NS + "instance_material"):
                bindings[im.get("symbol")] = ref(im.get("target"))
                bvi = im.find(NS + "bind_vertex_input")
                if bvi is not None:
                    sets[im.get("symbol")] = int(bvi.get("input_set", "0"))
            add_geometry(geometries[gid], world, rotor, bindings, sets)
        for child in node.findall(NS + "node"):
            visit(child, world, rotor)
        if node.find(NS + "instance_node") is not None:
            print("  skipping <instance_node> (unsupported)", file=sys.stderr)

    instances = [0]

    def add_geometry(geometry, world, rotor, bindings, sets):
        # Vertices are shared within one placed instance of a geometry only.
        instances[0] += 1
        instance = instances[0]
        flip = det3(world) < 0
        nm = normal_matrix(world)
        group = groups[ROTOR if rotor else BODY]
        for symbol, prim, kind in geometry.parts:
            mid = material(symbol, bindings)
            batch = group.setdefault(mid, Batch())
            positions, normals, uvs, corners, source_ids = geometry.triangles(
                prim, kind, sets.get(symbol, 0)
            )
            pv, nvals, tvals = positions[0], normals, uvs
            for tri in corners:
                pts = []
                for p_, _, _ in tri:
                    if (p_ + 1) * positions[1] > len(pv):
                        raise ConvertError("a position index is out of range")
                    o = p_ * positions[1]
                    pts.append(apply(world, pv[o : o + 3]))
                face = normalized(
                    cross(sub(pts[1], pts[0]), sub(pts[2], pts[0]))
                )
                if flip:
                    face = tuple(-c for c in face)
                ids = []
                for (p_, n_, t_), p in zip(tri, pts):
                    if nvals is not None and n_ is not None:
                        vals, stride = nvals
                        if (n_ + 1) * stride > len(vals):
                            raise ConvertError("a normal index is out of range")
                        raw = vals[n_ * stride : n_ * stride + 3]
                        n = normalized(tuple(sum(nm[r * 3 + k] * raw[k] for k in range(3)) for r in range(3)))
                        key_n = n_
                    else:
                        n, key_n = face, ("face", len(batch.triangles))
                    if tvals is not None and t_ is not None:
                        vals, stride = tvals
                        if (t_ + 1) * stride > len(vals):
                            raise ConvertError("a texcoord index is out of range")
                        t = (vals[t_ * stride], 1.0 - vals[t_ * stride + 1])
                    else:
                        t = (0.0, 0.0)
                    # Primitives of one material share a batch but may read
                    # different sources, so the source ids are in the key.
                    key = (instance, source_ids, p_, key_n, t_)
                    ids.append(batch.vertex(key, p, n, t))
                if flip:
                    ids[1], ids[2] = ids[2], ids[1]
                batch.triangles.append(tuple(ids))

    scene = root.find(".//" + NS + "instance_visual_scene")
    visual = elements.get(ref(scene.get("url"))) if scene is not None else None
    if visual is None:
        visual = root.find(".//" + NS + "visual_scene")
    if visual is None:
        raise ConvertError("no visual scene")
    for node in visual.findall(NS + "node"):
        visit(node, base, False)

    if args.rotor is not None:
        if pivot is None:
            raise ConvertError(f"no node named {args.rotor!r}")
        if args.rotor_axis:
            axis = normalized(tuple(float(c) for c in args.rotor_axis.split(",")))
        if axis is None:
            raise ConvertError("the rotor has no animated rotation: pass --rotor-axis")
        for batch in groups[ROTOR].values():
            batch.positions = [sub(p, pivot) for p in batch.positions]
    return write(args, groups, materials, pivot, axis)


def sub(a, b):
    return (a[0] - b[0], a[1] - b[1], a[2] - b[2])


def cross(a, b):
    return (
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    )


# --- writing -----------------------------------------------------------------


def copy_texture(path, out_dir, normal, convention):
    """Copies a texture into ``textures/``, flipping a DirectX normal map's
    green channel. Returns its URI, MIME type and what was done, naming the
    convention actually applied to a normal map and whether it was forced."""
    mime = image_type(path)
    name = os.path.basename(path)
    dest_dir = os.path.join(out_dir, "textures")
    os.makedirs(dest_dir, exist_ok=True)
    dest = os.path.join(dest_dir, name)
    uri = "textures/" + name
    if not normal:
        shutil.copyfile(path, dest)
        return uri, mime, "copied"
    if convention == "gl":
        # Nothing to flip, so the map is not even decoded.
        shutil.copyfile(path, dest)
        return uri, mime, "normal map, applied GL (forced), copied"
    if mime != "image/png":
        if convention == "dx":
            raise ConvertError(f"{name}: only PNG normal maps can be flipped from DX")
        shutil.copyfile(path, dest)
        return uri, mime, "normal map, applied GL (assumed: a JPEG is not inspected), copied"
    w, h, channels, rows = png_read(path)
    found, ratio = green_convention(w, h, channels, rows)
    use = found if convention == "auto" else convention
    how = "detected" if convention == "auto" else f"forced; its curl says {found.upper()}"
    note = f"normal map, applied {use.upper()} ({how}, GL/DX residual {ratio:.2f})"
    if use == "dx":
        for r in rows:
            for x in range(1, len(r), channels):
                r[x] = 255 - r[x]
        png_write(dest, w, h, channels, rows)
        return uri, mime, note + ", green flipped to GL"
    shutil.copyfile(path, dest)
    return uri, mime, note + ", copied"


def write(args, groups, materials, pivot, axis):
    out_dir = os.path.realpath(args.out)
    os.makedirs(out_dir, exist_ok=True)
    blob = bytearray()
    gltf = {
        "asset": {"version": "2.0", "generator": "pampa-seca tools/dae2gltf.py"},
        "scene": 0,
        "scenes": [{"name": args.name, "nodes": []}],
        "nodes": [],
        "meshes": [],
        "materials": [],
        "textures": [],
        "images": [],
        "samplers": [{"magFilter": 9729, "minFilter": 9987, "wrapS": 10497, "wrapT": 10497}],
        "accessors": [],
        "bufferViews": [],
        "buffers": [],
    }

    def view(data, target):
        while len(blob) % 4:
            blob.append(0)
        gltf["bufferViews"].append(
            {"buffer": 0, "byteOffset": len(blob), "byteLength": len(data), "target": target}
        )
        blob.extend(data)
        return len(gltf["bufferViews"]) - 1

    def accessor(values, kind, width):
        data = b"".join(struct.pack("<%df" % width, *v) for v in values)
        entry = {
            "bufferView": view(data, 34962),
            "componentType": 5126,
            "count": len(values),
            "type": kind,
        }
        if kind == "VEC3":
            # Bounds of the stored float32 values, not of the float64 ones.
            entry["min"] = [f32(min(v[k] for v in values)) for k in range(3)]
            entry["max"] = [f32(max(v[k] for v in values)) for k in range(3)]
        gltf["accessors"].append(entry)
        return len(gltf["accessors"]) - 1

    images, material_index, report = {}, {}, []

    def texture(path, normal):
        key = (path, normal)
        if key not in images:
            uri, mime, note = copy_texture(path, out_dir, normal, args.normal)
            report.append(f"  texture {uri}: {note}")
            gltf["images"].append({"uri": uri, "mimeType": mime})
            gltf["textures"].append({"sampler": 0, "source": len(gltf["images"]) - 1})
            images[key] = len(gltf["textures"]) - 1
        return images[key]

    def gltf_material(mid):
        if mid not in material_index:
            m = materials[mid]
            pbr = {"metallicFactor": args.metallic, "roughnessFactor": round(m.roughness, 3)}
            if m.diffuse:
                pbr["baseColorTexture"] = {"index": texture(m.diffuse, False)}
            else:
                pbr["baseColorFactor"] = list(m.color)
            entry = {"name": m.name, "pbrMetallicRoughness": pbr, "doubleSided": not args.single_sided}
            if m.normal:
                entry["normalTexture"] = {"index": texture(m.normal, True)}
            gltf["materials"].append(entry)
            material_index[mid] = len(gltf["materials"]) - 1
        return material_index[mid]

    totals = {}
    names = {BODY: "body", ROTOR: args.rotor_name}
    for key, group in groups.items():
        name = names[key]
        prims, tris = [], 0
        for mid, batch in group.items():
            if not batch.triangles:
                continue
            indices = [i for t in batch.triangles for i in t]
            big = len(batch.positions) > 65535
            fmt = "<%dI" if big else "<%dH"
            data = struct.pack(fmt % len(indices), *indices)
            gltf["accessors"].append(
                {
                    "bufferView": view(data, 34963),
                    "componentType": 5125 if big else 5123,
                    "count": len(indices),
                    "type": "SCALAR",
                }
            )
            idx = len(gltf["accessors"]) - 1
            prims.append(
                {
                    "attributes": {
                        "POSITION": accessor(batch.positions, "VEC3", 3),
                        "NORMAL": accessor(batch.normals, "VEC3", 3),
                        "TEXCOORD_0": accessor(batch.uvs, "VEC2", 2),
                    },
                    "indices": idx,
                    "material": gltf_material(mid),
                }
            )
            tris += len(batch.triangles)
        if not prims:
            continue
        gltf["meshes"].append({"name": name, "primitives": prims})
        node = {"name": name, "mesh": len(gltf["meshes"]) - 1}
        if key == ROTOR and pivot is not None:
            node["translation"] = [round(c, 6) for c in pivot]
        gltf["nodes"].append(node)
        gltf["scenes"][0]["nodes"].append(len(gltf["nodes"]) - 1)
        totals[name] = tris
    if not gltf["meshes"]:
        raise ConvertError("no triangles found")
    for key in ("textures", "images", "materials"):
        if not gltf[key]:
            del gltf[key]
    bin_name = args.name + ".bin"
    gltf["buffers"].append({"uri": bin_name, "byteLength": len(blob)})
    with open(os.path.join(out_dir, bin_name), "wb") as f:
        f.write(blob)
    with open(os.path.join(out_dir, args.name + ".gltf"), "w", encoding="utf-8") as f:
        json.dump(gltf, f, indent=1)
        f.write("\n")

    lo = [math.inf] * 3
    hi = [-math.inf] * 3
    for key, group in groups.items():
        offset = pivot if key == ROTOR and pivot is not None else (0, 0, 0)
        for batch in group.values():
            for p in batch.positions:
                for k in range(3):
                    lo[k] = min(lo[k], p[k] + offset[k])
                    hi[k] = max(hi[k], p[k] + offset[k])
    print(f"wrote {os.path.join(args.out, args.name + '.gltf')} ({len(blob)} bytes of buffer)")
    for name, tris in totals.items():
        print(f"  {name}: {tris} triangles")
    print(f"  total: {sum(totals.values())} triangles")
    print("  bounds (m, Y up): min " + fmt3(lo) + "  max " + fmt3(hi))
    if pivot is not None:
        print(f"  rotor node {args.rotor_name!r}: pivot {fmt3(pivot)}, axis {fmt3(axis)}")
        radius = 0.0
        for batch in groups[ROTOR].values():
            for p in batch.positions:
                along = sum(p[k] * axis[k] for k in range(3))
                radius = max(radius, math.sqrt(max(sum(c * c for c in p) - along * along, 0.0)))
        print(f"  rotor radius: {radius:.3f} m")
    for line in report:
        print(line)
    return 0


def fmt3(v):
    return "(" + ", ".join(f"{c:.3f}" for c in v) + ")"


# --- checking a written file ---------------------------------------------------


def check(path):
    """Validates a glTF file's structure: buffers, views, accessors, indices
    and images in range (every index a non-negative integer that exists),
    images PNG or JPEG by type, extension and content. Prints its
    triangles; exits non-zero on a fault."""
    folder = os.path.dirname(os.path.realpath(path))
    with open(path, encoding="utf-8") as f:
        g = json.load(f)
    faults = []
    if g.get("asset", {}).get("version") != "2.0":
        faults.append("asset.version is not 2.0")

    def local(uri):
        if uri.startswith("data:"):
            return None
        target = os.path.realpath(os.path.join(folder, urllib.parse.unquote(uri)))
        if os.path.isabs(uri) or not target.startswith(folder + os.sep):
            faults.append(f"uri {uri!r} leaves the model's folder")
            return None
        return target

    def item(items, i, what):
        """items[i] when i is a valid index; a fault (and None) otherwise.
        Python's negative indices would silently wrap, so they are refused."""
        if not isinstance(i, int) or isinstance(i, bool) or not 0 <= i < len(items):
            faults.append(f"{what} index {i!r} is missing or out of range")
            return None
        return items[i]

    blobs = []
    for b in g.get("buffers", []):
        target = local(b.get("uri", ""))
        data = open(target, "rb").read() if target and os.path.isfile(target) else b""
        if len(data) < b.get("byteLength", 0):
            faults.append(f"buffer {b.get('uri')} is shorter than its byteLength")
        blobs.append(data)
    views = g.get("bufferViews", [])
    for k, v in enumerate(views):
        data = item(blobs, v.get("buffer"), f"bufferView {k} buffer")
        if data is not None and v.get("byteOffset", 0) + v.get("byteLength", 0) > len(data):
            faults.append(f"bufferView {k} is out of its buffer")
    sizes = {5120: 1, 5121: 1, 5122: 2, 5123: 2, 5125: 4, 5126: 4}
    widths = {"SCALAR": 1, "VEC2": 2, "VEC3": 3, "VEC4": 4, "MAT4": 16}
    accessors = g.get("accessors", [])
    good = set()
    for k, a in enumerate(accessors):
        v = item(views, a.get("bufferView"), f"accessor {k} bufferView")
        size, width = sizes.get(a.get("componentType")), widths.get(a.get("type"))
        if size is None or width is None:
            faults.append(f"accessor {k} has an unknown componentType or type")
            continue
        need = a.get("byteOffset", 0) + a.get("count", 0) * size * width
        if v is not None and need > v.get("byteLength", 0):
            faults.append(f"accessor {k} is out of its view")
        elif v is not None and isinstance(v.get("buffer"), int) and 0 <= v["buffer"] < len(blobs):
            good.add(k)
    tris = 0
    for m in g.get("meshes", []):
        for p in m.get("primitives", []):
            position = item(accessors, p.get("attributes", {}).get("POSITION"), "POSITION accessor")
            ii = p.get("indices")
            a = item(accessors, ii, "indices accessor")
            if position is None or a is None or ii not in good:
                continue
            fmt = {5121: "B", 5123: "H", 5125: "I"}.get(a["componentType"])
            if fmt is None:
                faults.append(f"mesh {m.get('name')} has float indices")
                continue
            v = views[a["bufferView"]]
            start = v.get("byteOffset", 0) + a.get("byteOffset", 0)
            idx = struct.unpack_from("<%d%s" % (a["count"], fmt), blobs[v["buffer"]], start)
            if idx and max(idx) >= position.get("count", 0):
                faults.append(f"mesh {m.get('name')} indexes past its vertices")
            tris += a["count"] // 3
    for image in g.get("images", []):
        uri = image.get("uri", "")
        mime = image.get("mimeType") or IMAGE_TYPES.get(os.path.splitext(uri)[1].lower())
        if mime not in IMAGE_MAGIC or IMAGE_TYPES.get(os.path.splitext(uri)[1].lower()) != mime:
            faults.append(f"image {uri} is not a PNG or JPEG by its type or extension")
            continue
        target = local(uri)
        if target is None:
            continue
        if not os.path.isfile(target):
            faults.append(f"image {uri} is missing")
            continue
        with open(target, "rb") as f:
            if not f.read(8).startswith(IMAGE_MAGIC[mime]):
                kind = "PNG" if mime == "image/png" else "JPEG"
                faults.append(f"image {uri} is not a {kind} by its content")
    print(f"{path}: {tris} triangles, nodes {[n.get('name') for n in g.get('nodes', [])]}")
    for fault in faults:
        print("  FAULT: " + fault)
    return 1 if faults else 0


def main(argv=None):
    parser = argparse.ArgumentParser(
        description="Convert a COLLADA (.dae) model into a glTF 2.0 model slot "
        "(standard library only; run with python3 -I).",
    )
    parser.add_argument("input", nargs="?", help="the .dae file")
    parser.add_argument("-o", "--out", help="output folder (the model slot)")
    parser.add_argument("--name", default="model", help="output base name (default: model)")
    parser.add_argument("--rotor", help="COLLADA node (name or id) to keep apart as a spinning rotor")
    parser.add_argument("--rotor-name", default="rotor", help="glTF node name of the rotor (default: rotor)")
    parser.add_argument("--rotor-axis", help="spin axis x,y,z in glTF space, when the rotor is not animated")
    parser.add_argument("--scale", type=float, default=1.0, help="extra uniform scale (default: 1)")
    parser.add_argument("--metallic", type=float, default=0.0, help="metallic factor of every material")
    parser.add_argument(
        "--normal", choices=("auto", "gl", "dx"), default="auto",
        help="green convention of the normal maps (default: detect per map)",
    )
    parser.add_argument("--single-sided", action="store_true", help="cull back faces")
    parser.add_argument("--check", metavar="GLTF", help="only validate a written glTF file")
    args = parser.parse_args(argv)
    try:
        if args.check:
            return check(args.check)
        if not args.input or not args.out:
            parser.error("input and --out are required")
        if not (math.isfinite(args.scale) and args.scale > 0):
            parser.error("--scale must be positive")
        if not SAFE_NAME.match(args.name) or "/" in args.name:
            parser.error("--name must be a plain file name")
        if args.rotor_name == "body" or not SAFE_NAME.match(args.rotor_name):
            parser.error("--rotor-name must be a plain name other than 'body'")
        return convert(args)
    except (ConvertError, ValueError, OSError) as e:
        # Refusals, unparsable numbers or JSON (ValueError) and unreadable
        # files are the input's fault: one line. Anything else is a bug in
        # this script and keeps its traceback.
        print(f"dae2gltf: {e}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    sys.exit(main())
