#!/usr/bin/env python3
"""Dress a skinned body that is already on the game's ``game_engine``
skeleton in a MakeHuman outfit (.mhm ``piece`` lines), with
``tools/mh2gltf.py``'s own garment code, and write the game's glTF.

Standard library plus numpy; run it isolated: ``python3 -I tools/dress.py``.
It needs MakeHuman 1.x's *data* folder only for the skeleton's bone order
(``rigs/game_engine.mhskel``).

The body is a glTF 2.0 file (``.gltf`` with external ``.bin`` and PNG
textures, as mh2gltf writes them; e.g. the Rocketbox protagonist built on
the game skeleton): one skinned mesh whose primitives share one vertex
layout (POSITION, TEXCOORD_0, JOINTS_0, WEIGHTS_0) and differ only in
their material. Its animations, cameras and other meshes are ignored. Every
file is read as untrusted data: bounded sizes, every index, offset and
number checked, every referenced file kept inside the body's own folder.

What it does, the way mh2gltf.convert() does it for a MakeHuman body:

- the body's vertices are welded by position (one vertex per place; the
  UV corners stay apart), converted to MakeHuman's decimetres, V flipped
  (MakeHuman's V runs up; write() flips it back), and given dense weights
  in the skeleton's bone order; the joints' rest matrices come from the
  glTF's own nodes;
- ``dress()`` cuts the .mhm's garments from the body and hides the skin
  under them; ``generate()`` fits the generated pieces (hat, poncho,
  neckerchief, faja, rastra...) to the dressed body;
- the skin is split back into the body's own materials, the lowest point
  is put on y = 0 and ``write()`` writes ``model.gltf``, ``model.bin`` and
  ``textures/``, the same layout and skeleton as mh2gltf's.

The output is deterministic: the same inputs give the same bytes.
"""

import argparse
import math
import os
import re
import sys

if __package__ in (None, ""):
    # Run as a script with -I the tools folder is not on the path; it is
    # our own code, never the input's folder.
    sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))

import mh2gltf as M  # noqa: E402

np = M.np
MAX_BUFFER_BYTES = 64 * 1024 * 1024
WELD = 4  # decimals of a decimetre: vertices this close are one place
COMPONENTS = {5126: ("f", 4), 5125: ("I", 4), 5123: ("H", 2), 5121: ("B", 1)}
WIDTHS = {"SCALAR": 1, "VEC2": 2, "VEC3": 3, "VEC4": 4, "MAT4": 16}
PLAIN_FILE = re.compile(r"^[A-Za-z0-9][A-Za-z0-9._-]{0,127}(/[A-Za-z0-9][A-Za-z0-9._-]{0,127}){0,3}$")


class Gltf:
    """A .gltf document and the buffers its accessors read, checked."""

    def __init__(self, path):
        self.path = path
        self.folder = os.path.dirname(os.path.realpath(path))
        self.doc = M.read_json(path)
        self.buffers = {}

    def items(self, key):
        value = self.doc.get(key, [])
        if not isinstance(value, list) or len(value) > 100_000 or not all(isinstance(v, dict) for v in value):
            raise M.ConvertError(f"body: {key!r} is not a list of objects")
        return value

    def item(self, key, i):
        items = self.items(key)
        if not isinstance(i, int) or isinstance(i, bool) or not 0 <= i < len(items):
            raise M.ConvertError(f"body: {key} index {i!r} is out of range")
        return items[i]

    def file(self, uri, what):
        """A file the document names, inside its own folder."""
        if not isinstance(uri, str) or not PLAIN_FILE.match(uri) or ".." in uri.split("/"):
            raise M.ConvertError(f"body: {what} {uri!r} is not a plain relative file name")
        path = os.path.join(self.folder, *uri.split("/"))
        if not M.inside(path, self.folder) or not os.path.isfile(path):
            raise M.ConvertError(f"body: {what} {uri!r} is not a file in the body's folder")
        return os.path.realpath(path)

    def buffer(self, i):
        if i not in self.buffers:
            b = self.item("buffers", i)
            data = M.read_bytes(self.file(b.get("uri"), "buffer"), MAX_BUFFER_BYTES)
            length = b.get("byteLength")
            if not isinstance(length, int) or not 0 <= length <= len(data):
                raise M.ConvertError(f"body: buffer {i} is shorter than its byteLength")
            self.buffers[i] = data[:length]
        return self.buffers[i]

    def read(self, i, kind, components):
        """Accessor i as a numpy array (count, width), or (count,) for a
        SCALAR; of one of ``components``; floats finite."""
        a = self.item("accessors", i)
        if a.get("type") != kind or a.get("componentType") not in components or "sparse" in a:
            raise M.ConvertError(f"body: accessor {i} is not a {kind} of {sorted(components)}")
        fmt, size = COMPONENTS[a["componentType"]]
        width = WIDTHS[kind]
        count = a.get("count")
        if not isinstance(count, int) or not 0 < count * width <= M.MAX_ARRAY_ITEMS:
            raise M.ConvertError(f"body: accessor {i} has a bad count")
        view = self.item("bufferViews", a.get("bufferView"))
        stride = view.get("byteStride", size * width)
        if stride != size * width:
            raise M.ConvertError(f"body: accessor {i} is interleaved (byteStride)")
        data = self.buffer(view.get("buffer"))
        start = view.get("byteOffset", 0) + a.get("byteOffset", 0)
        length = view.get("byteLength")
        if not all(isinstance(x, int) and x >= 0 for x in (start, length, view.get("byteOffset", 0))) or \
                a.get("byteOffset", 0) + count * size * width > length or \
                view.get("byteOffset", 0) + length > len(data):
            raise M.ConvertError(f"body: accessor {i} runs past its buffer")
        arr = np.frombuffer(data, f"<{fmt}", count * width, start).astype(np.float64 if fmt == "f" else np.int64)
        if fmt == "f" and not np.isfinite(arr).all():
            raise M.ConvertError(f"body: accessor {i} has a number that is not finite")
        return arr.reshape(count, width) if width > 1 else arr


def number_list(value, n, what):
    if not isinstance(value, list) or len(value) != n or not all(
            isinstance(x, (int, float)) and not isinstance(x, bool) and math.isfinite(x) for x in value):
        raise M.ConvertError(f"body: {what} is not {n} finite numbers")
    return [float(x) for x in value]


def local_matrix(node):
    """A node's local transform, its translation in decimetres."""
    if "matrix" in node or "scale" in node and number_list(node["scale"], 3, "a scale") != [1.0, 1.0, 1.0]:
        raise M.ConvertError(f"body: joint {node.get('name')!r} is scaled or given as a matrix")
    m = np.identity(4)
    m[:3, :3] = M.rotation_matrix(number_list(node.get("rotation", [0, 0, 0, 1]), 4, "a rotation"))
    m[:3, 3] = np.array(number_list(node.get("translation", [0, 0, 0]), 3, "a translation")) / M.METERS
    return m


def srgb(c):
    """A linear colour channel (glTF's baseColorFactor) as the display
    value mh2gltf's materials hold (write() turns it back)."""
    c = min(max(c, 0.0), 1.0)
    return c * 12.92 if c <= 0.0031308 else 1.055 * c ** (1 / 2.4) - 0.055


def skin_material(g, i):
    """The body's material i as an mh2gltf Material: its colour and normal
    textures (files in the body's folder), roughness, metallic, alpha mask
    and double-sidedness."""
    src = g.item("materials", i)
    m = M.Material(str(src.get("name", f"skin_{i}")))
    pbr = src.get("pbrMetallicRoughness", {})
    m.color = [srgb(c) for c in number_list(pbr.get("baseColorFactor", [1, 1, 1, 1]), 4, "a baseColorFactor")[:3]] + \
              [min(max(float(pbr.get("baseColorFactor", [1, 1, 1, 1])[3]), 0.0), 1.0)]
    m.roughness = min(max(float(number_list([pbr.get("roughnessFactor", 1.0)], 1, "a roughness")[0]), 0.0), 1.0)
    m.metallic = min(max(float(number_list([pbr.get("metallicFactor", 1.0)], 1, "a metallic")[0]), 0.0), 1.0)

    def image(ref):
        if ref is None:
            return None
        if not isinstance(ref, dict):
            raise M.ConvertError("body: a texture reference is not an object")
        img = g.item("images", g.item("textures", ref.get("index")).get("source"))
        return g.file(img.get("uri"), "image")

    m.diffuse = image(pbr.get("baseColorTexture"))
    m.normal = image(src.get("normalTexture"))
    m.transparent = src.get("alphaMode") == "MASK"
    m.double_sided = src.get("doubleSided") is True
    return m


def read_body(g, rig):
    """The body's skin as one mh2gltf Piece (decimetres, welded, dense
    weights in rig order), the joints' rest matrices by name, and each
    triangle's material index."""
    doc = g.doc
    nodes = g.items("nodes")
    skinned = [n for n in nodes if "mesh" in n and "skin" in n]
    if len(skinned) != 1:
        raise M.ConvertError("body: it needs exactly one skinned mesh node")
    prims = g.item("meshes", skinned[0]["mesh"]).get("primitives")
    skin = g.item("skins", skinned[0]["skin"])
    if not isinstance(prims, list) or not prims or not all(isinstance(p, dict) for p in prims):
        raise M.ConvertError("body: its mesh has no primitives")
    attrs = prims[0].get("attributes", {})
    if any(p.get("attributes") != attrs or p.get("mode", 4) != 4 for p in prims):
        raise M.ConvertError("body: its primitives must be triangles sharing one vertex layout")
    pos = g.read(attrs.get("POSITION"), "VEC3", {5126}) / M.METERS
    n = len(pos)
    uv = g.read(attrs.get("TEXCOORD_0"), "VEC2", {5126})
    uv[:, 1] = 1.0 - uv[:, 1]  # MakeHuman's V runs up; write() flips it back
    joints = g.read(attrs.get("JOINTS_0"), "VEC4", {5121, 5123})
    wts = g.read(attrs.get("WEIGHTS_0"), "VEC4", {5126})
    if len(uv) != n or len(joints) != n or len(wts) != n:
        raise M.ConvertError("body: its vertex attributes differ in count")

    # Joints: every bone of the skeleton is a node of the same name, under
    # the same parent; the skin's joints are among them.
    index = {}
    for i, node in enumerate(nodes):
        name = node.get("name")
        if isinstance(name, str) and name in rig.order:
            if name in index:
                raise M.ConvertError(f"body: two nodes are named {name!r}")
            index[name] = i
    missing = [b for b in rig.order if b not in index]
    if missing:
        raise M.ConvertError(f"body: the skeleton's {missing[0]!r} is not a node (not on the game skeleton?)")
    parent = {}
    for i, node in enumerate(nodes):
        for c in node.get("children", []):
            g.item("nodes", c)
            if c in parent:
                raise M.ConvertError("body: a node has two parents")
            parent[c] = i
    for b in rig.order:
        p = rig.bones[b]["parent"]
        if p is not None and parent.get(index[b]) != index[p]:
            raise M.ConvertError(f"body: {b!r} is not under {p!r} as on the skeleton")
    rest = {}
    for b in rig.order:
        m, i = local_matrix(nodes[index[b]]), index[b]
        while i in parent:  # the skeleton's root may sit under other nodes
            i = parent[i]
            m = local_matrix(nodes[i]) @ m
        rest[b] = m
    skin_joints = skin.get("joints")
    if not isinstance(skin_joints, list) or not skin_joints:
        raise M.ConvertError("body: its skin has no joints")
    column = []
    for j in skin_joints:
        name = g.item("nodes", j).get("name")
        if name not in rig.order:
            raise M.ConvertError(f"body: skin joint {name!r} is not on the skeleton")
        column.append(rig.order.index(name))
    M.checked_indices(joints, len(column), "body JOINTS_0")
    if (wts < 0).any():
        raise M.ConvertError("body: a negative skin weight")

    # Weld by position: one vertex per place (the garments are cut along
    # shared edges), the UV corners kept apart.
    welded, where = np.unique(np.round(pos, WELD), axis=0, return_inverse=True)
    where = where.reshape(-1)
    first = np.full(len(welded), n)
    np.minimum.at(first, where, np.arange(n))  # each place's first corner
    coords = pos[first]
    dense = np.zeros((len(welded), len(rig.order)))
    cols = np.array(column)[joints[first]]
    np.add.at(dense, (np.repeat(np.arange(len(welded)), 4), cols.reshape(-1)), wts[first].reshape(-1))
    total = dense.sum(axis=1)
    if (total <= 0).any():
        raise M.ConvertError("body: a vertex without skin weights")
    dense /= total[:, None]

    triangles, material = [], []
    for p in prims:
        idx = M.checked_indices(g.read(p.get("indices"), "SCALAR", {5121, 5123, 5125}), n, "body indices")
        if len(idx) % 3:
            raise M.ConvertError("body: a primitive's indices are not triangles")
        mat = p.get("material")
        g.item("materials", mat)
        for t in idx.reshape(-1, 3):
            triangles.append(tuple((int(where[c]), int(c)) for c in t))
            material.append(mat)
    piece = M.Piece("skin", "proxy", None, coords, triangles, dense, uv, large=True)
    piece.all_triangles = list(triangles)
    return piece, rest, dict(zip(triangles, material))


def dress_body(args):
    data = os.path.realpath(os.path.expanduser(args.data or os.environ.get("MAKEHUMAN_DATA") or M.DEFAULT_DATA))
    character = M.read_mhm(os.path.realpath(args.outfit))
    if not character.skeleton:
        raise M.ConvertError("the .mhm names no skeleton (a 'skeleton' line, e.g. game_engine.mhskel)")
    rig = M.Rig(os.path.join(data, "rigs", os.path.basename(character.skeleton)), M.MAX_ARRAY_ITEMS)
    g = Gltf(args.body)
    skin, rest, material_of = read_body(g, rig)
    report = []

    tile = M.FABRIC_TILE if args.fabrics else M.CLOTH_TILE
    garments, covered = M.dress(character, M.Figure(rig, rest, skin), rig, tile)
    before = len(skin.triangles)
    skin.triangles = [t for t in skin.triangles if tuple(sorted(v for v, _ in t)) not in covered]
    report.append(f"  skin: {before - len(skin.triangles)} triangles under the garments left out")
    for pc in garments:
        report.append(f"  {pc.label}: cut from the body, {len(pc.triangles)} triangles")
    made, _ = M.generate(character, M.Body(rig, rest, [skin] + garments), rig)
    for pc in made:
        report.append(f"  {pc.label}: generated, {len(pc.triangles)} triangles")

    # The skin back in the body's own materials, in their order.
    split = []
    for mat in sorted(set(material_of.values())):
        tris = [t for t in skin.triangles if material_of[t] == mat]
        if tris:
            m = skin_material(g, mat)
            split.append(M.Piece(m.name, "proxy", m, skin.coords, tris, skin.weights, skin.texco, large=True))
    pieces = split + garments + made

    # Meters, the lowest point on the ground.
    used = np.concatenate([pc.coords[np.unique([v for t in pc.triangles for v, _ in t])] for pc in pieces])
    offset = np.array([0.0, -used[:, 1].min(), 0.0])
    for pc in pieces:
        pc.coords = (pc.coords + offset) * M.METERS
    for b in rest:
        rest[b] = rest[b].copy()
        rest[b][:3, 3] = (rest[b][:3, 3] + offset) * M.METERS
    out = argparse.Namespace(out=args.out, name=args.name, max_texture=args.max_texture,
                             max_small_texture=args.max_texture, no_normal_maps=False,
                             keep_hair_under_hats=False, fabrics=args.fabrics)
    return M.write(out, character, rig, rest, pieces, report)


def main(argv=None):
    parser = argparse.ArgumentParser(
        description="Dress a skinned glTF body on the game_engine skeleton in a .mhm outfit with "
        "mh2gltf's garment code and write the game's glTF (standard library and numpy; run with python3 -I).",
    )
    parser.add_argument("body", help="the body's .gltf (on the game_engine skeleton)")
    parser.add_argument("outfit", help="the .mhm whose skeleton and piece lines dress it")
    parser.add_argument("-o", "--out", required=True, help="output folder")
    parser.add_argument("--name", default="model", help="output base name (default: model)")
    parser.add_argument("--data", help=f"MakeHuman's data folder, for the skeleton (default: $MAKEHUMAN_DATA, else {M.DEFAULT_DATA})")
    parser.add_argument("--fabrics", metavar="DIR", help="Poly Haven fabrics for the cloth (see mh2gltf --fabrics)")
    parser.add_argument("--max-texture", type=int, default=2048, help="largest side of every texture (default 2048)")
    args = parser.parse_args(argv)
    if np is None:
        parser.error("numpy is required")
    if not M.SAFE_NAME.match(args.name) or "/" in args.name:
        parser.error("--name must be a plain file name")
    if not 16 <= args.max_texture <= 4096:
        parser.error("--max-texture must be 16..4096")
    try:
        if args.fabrics:
            M.fabric_files(args.fabrics)
        return dress_body(args)
    except (M.ConvertError, ValueError, KeyError, OSError) as e:
        print(f"dress: {e}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    sys.exit(main())
