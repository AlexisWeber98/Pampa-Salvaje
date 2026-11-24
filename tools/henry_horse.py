#!/usr/bin/env python3
"""Turn the Sketchfab "Horse" by henrysteve973 (CC-BY-4.0) into a game GLB.

Standard library only (plus ImageMagick 7 ``magick`` for the textures); run it
isolated: ``python3 -I tools/henry_horse.py SOURCE OUT``. SOURCE is the
extracted Sketchfab glTF download (``scene.gltf``, ``scene.bin``,
``textures/``); it is treated as untrusted data: parsed with bounded sizes,
every index checked, never executed.

What it does:

- keeps the body, the hair cards, the corneas and the horseshoes as one
  skinned mesh ``Horse`` (one primitive per material, in ``MATERIALS``
  order) and drops the saddle and the bridle (``DROP_MATERIALS``), and the
  stirrup bones nothing else is weighted to (``DROP_JOINTS``);
- bakes the Sketchfab wrapper nodes and the armature's 0.1 scale into the
  skeleton: every joint has unit scale and its translations are in metres,
  the root joint is the identity, and the inverse bind matrices are rigid;
  the skinned result is the source's in metres, turned to face +Z with Y up
  and moved so that the hooves stand on y = 0 around the origin (the first
  frame of ``Idle_1``), like our own horse;
- keeps all 28 clips, renamed (``clean_clip_name``) and written in the fixed
  order ``CLIPS``, and drops their scale channels (all 1 in the source);
  closes the loop of ``Gallop``, a frame short in the source
  (``CLOSE_LOOPS``);
- writes the texture sources the game converts to KTX2 with
  ``textures_to_ktx2.sh`` (``TEXTURES``: 2048 px for the body and the
  hair, 512 for the horseshoes);
- prints, per clip, its length and root motion, and for the walks and the
  gallops the speed measured on the planted hooves.

The output is deterministic. See tools/README.md.
"""

import argparse
import json
import math
import os
import re
import struct
import subprocess
import sys
import tempfile

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import glbtrim  # noqa: E402

MAX_JSON_BYTES = 16 * 1024 * 1024
MAX_BIN_BYTES = 64 * 1024 * 1024

# The materials kept, in primitive order, and the ones dropped (S5).
MATERIALS = ("Horse", "Hair", "Cornea", "Horseshoe")
DROP_MATERIALS = ("Saddle",)
# Bones that only carry the saddle's stirrups: dropped with their children
# when no kept vertex is weighted to them.
DROP_JOINTS = re.compile(r"Stirrup")

# The clips, in the order the GLB holds them. The first seven fill the slots
# of our horse's clips (Idle, Idle_2, Idle_Headlow, Eating, Walk, Gallop,
# Gallop_Jump), so the game can switch with the fewest changes.
CLIPS = (
    "Idle_1", "Idle_2", "Idle_3", "Eat", "Walk", "Gallop", "Jump",
    "Jump_Land", "Jump_Run", "S_Jump",
    "Walk_L", "Walk_R", "Gallop_L", "Gallop_R", "Run_Stop",
    "Attack_1", "Attack_2", "Buck_1", "Buck_2", "Buck_3",
    "Arise", "Sleep", "Falling", "Fallen", "Death",
    "Swim", "Swim_L", "Swim_R",
)  # fmt: skip

# Loops whose last key is one frame short of the cycle: the source stops
# them a frame before they come back to their first pose, so played on a
# loop they jump that frame at the seam (the lead fore of `Gallop` pops
# 21 cm every stride, the planted hooves of `Walk` 3 cm). Each gets a closing
# key, its first, one frame after its last: `Gallop` becomes 1.000 s and
# `Walk` 1.042 s. `Gallop_L`, `Gallop_R` and the swims miss one as well and
# stay open: the game does not play them where it shows.
CLOSE_LOOPS = ("Gallop", "Walk")
LOOP_FRAME = 1 / 24

# The bones the placement and the measurements read.
HEAD = "BN_Head_00_016_015"
PELVIS = "BN_Pelvis_060_066"
ROOT = "BN_Root_01_01"
# The hooves, each the centroid of the horseshoe skinned to the leg below
# its bone: left fore, right fore, left hind, right hind.
LEGS = ("BN_L_Hand_041_042", "BN_R_Hand_046_048", "BN_L_Foot_054_057", "BN_R_Foot_00_063")
REST_CLIP = "Idle_1"
# The Sketchfab wrappers leave the horse in decimetres (the mesh is in
# centimetres, the armature scales it by 0.1): the poll stands 2.03 units
# high. This takes it to metres.
METRES = 0.1
# A hoof is planted while it is within this of its lowest point in the clip
# and sliding back. The fore hooves rise some 7 cm through a walk's stance
# (the heel lifts long before breakover), so a tighter band misses most of it.
PLANTED = 0.05
SAMPLE_HZ = 96
# The cycles played in place, whose hoof slide is their ground speed.
CYCLES = ("Walk", "Walk_L", "Walk_R", "Gallop", "Gallop_L", "Gallop_R", "Swim", "Swim_L", "Swim_R")

# Texture sets: folder, then file -> (source image, longest side, kind).
# 2048 for the body (from 4096) and the hair (its own size): one model dresses
# every horse in the game, so it gets twice our own horse's 1024; the
# horseshoes keep their 512.
TEXTURES = {
    "horse_body": {
        "color.jpg": ("Horse_baseColor.png", 2048, "color"),
        "normal_gl.jpg": ("Horse_normal.png", 2048, "normal"),
        "roughness.jpg": ("Horse_baseColor.png", 2048, "roughness"),
    },
    "horse_hair": {
        "color.jpg": ("Hair_baseColor.png", 2048, "color"),
        "opacity.jpg": ("Hair_baseColor.png", 2048, "opacity"),
        "normal_gl.jpg": ("Hair_normal.png", 2048, "normal"),
        "roughness.jpg": ("Hair_baseColor.png", 2048, "roughness"),
    },
    "horse_shoe": {
        "color.jpg": ("Horseshoe_baseColor.png", 512, "color"),
        "normal_gl.jpg": ("Horseshoe_normal.png", 512, "normal"),
        "roughness.jpg": ("Horseshoe_baseColor.png", 512, "roughness"),
    },
}
# The source has no roughness maps, one factor per material: 0.82.
ROUGHNESS = 0.82
# The hair atlas is grey strands on black, with no alpha: the grey is the
# coverage. Grey up to the first level is clear, from the second opaque.
HAIR_LEVELS = ("4%", "30%")


class ConvertError(Exception):
    """The source cannot be converted safely."""


# --- names -------------------------------------------------------------


def clean_clip_name(raw):
    """`Skeleton|1 Ilde` -> `Idle_1`, `Skeleton|4 Attack_1` -> `Attack_1`,
    `Skeleton|S.Jump` -> `S_Jump`."""
    name = raw.rsplit("|", 1)[-1].strip()
    number = None
    match = re.fullmatch(r"(\d+) (.+)", name)
    if match:
        number, name = match.groups()
    name = name.replace("Ilde", "Idle").replace(".", "_").replace(" ", "_")
    if name == "Idle" and number is not None:
        name = f"Idle_{number}"
    return name


# --- maths ---------------------------------------------------------------
# Matrices are 4x4 row-major lists; glTF stores them column-major.


def identity():
    return [[float(r == c) for c in range(4)] for r in range(4)]


def mul(a, b):
    return [[sum(a[r][k] * b[k][c] for k in range(4)) for c in range(4)] for r in range(4)]


def from_gltf(m):
    return [[m[c * 4 + r] for c in range(4)] for r in range(4)]


def to_gltf(m):
    return [m[r][c] for c in range(4) for r in range(4)]


def apply(m, p):
    return tuple(m[r][0] * p[0] + m[r][1] * p[1] + m[r][2] * p[2] + m[r][3] for r in range(3))


def linear(m, v):
    return tuple(m[r][0] * v[0] + m[r][1] * v[1] + m[r][2] * v[2] for r in range(3))


def inverse(m):
    """Inverse of an affine matrix."""
    a = [row[:3] for row in m[:3]]
    det = (
        a[0][0] * (a[1][1] * a[2][2] - a[1][2] * a[2][1])
        - a[0][1] * (a[1][0] * a[2][2] - a[1][2] * a[2][0])
        + a[0][2] * (a[1][0] * a[2][1] - a[1][1] * a[2][0])
    )
    if abs(det) < 1e-12:
        raise ConvertError("a singular transform")
    inv = [[0.0] * 3 for _ in range(3)]
    for r in range(3):
        for c in range(3):
            r1, r2 = [i for i in range(3) if i != c]
            c1, c2 = [i for i in range(3) if i != r]
            minor = a[r1][c1] * a[r2][c2] - a[r1][c2] * a[r2][c1]
            inv[r][c] = (-1) ** (r + c) * minor / det
    t = [-sum(inv[r][k] * m[k][3] for k in range(3)) for r in range(3)]
    return [inv[r] + [t[r]] for r in range(3)] + [[0.0, 0.0, 0.0, 1.0]]


def quat_mul(a, b):
    ax, ay, az, aw = a
    bx, by, bz, bw = b
    return (
        aw * bx + ax * bw + ay * bz - az * by,
        aw * by - ax * bz + ay * bw + az * bx,
        aw * bz + ax * by - ay * bx + az * bw,
        aw * bw - ax * bx - ay * by - az * bz,
    )


def quat_matrix(q):
    x, y, z, w = q
    n = math.sqrt(x * x + y * y + z * z + w * w) or 1.0
    x, y, z, w = x / n, y / n, z / n, w / n
    return [
        [1 - 2 * (y * y + z * z), 2 * (x * y - z * w), 2 * (x * z + y * w)],
        [2 * (x * y + z * w), 1 - 2 * (x * x + z * z), 2 * (y * z - x * w)],
        [2 * (x * z - y * w), 2 * (y * z + x * w), 1 - 2 * (x * x + y * y)],
    ]


def matrix_quat(r):
    """Unit quaternion of a 3x3 rotation, w >= 0."""
    t = r[0][0] + r[1][1] + r[2][2]
    if t > 0:
        s = math.sqrt(t + 1.0) * 2
        q = ((r[2][1] - r[1][2]) / s, (r[0][2] - r[2][0]) / s, (r[1][0] - r[0][1]) / s, s / 4)
    elif r[0][0] > r[1][1] and r[0][0] > r[2][2]:
        s = math.sqrt(1.0 + r[0][0] - r[1][1] - r[2][2]) * 2
        q = (s / 4, (r[0][1] + r[1][0]) / s, (r[0][2] + r[2][0]) / s, (r[2][1] - r[1][2]) / s)
    elif r[1][1] > r[2][2]:
        s = math.sqrt(1.0 + r[1][1] - r[0][0] - r[2][2]) * 2
        q = ((r[0][1] + r[1][0]) / s, s / 4, (r[1][2] + r[2][1]) / s, (r[0][2] - r[2][0]) / s)
    else:
        s = math.sqrt(1.0 + r[2][2] - r[0][0] - r[1][1]) * 2
        q = ((r[0][2] + r[2][0]) / s, (r[1][2] + r[2][1]) / s, s / 4, (r[1][0] - r[0][1]) / s)
    n = math.sqrt(sum(c * c for c in q))
    q = tuple(c / n for c in q)
    return q if q[3] >= 0 else tuple(-c for c in q)


def trs(t=(0, 0, 0), r=(0, 0, 0, 1), s=(1, 1, 1)):
    m3 = quat_matrix(r)
    return [[m3[i][j] * s[j] for j in range(3)] + [t[i]] for i in range(3)] + [[0, 0, 0, 1.0]]


def similarity(m, what):
    """(uniform scale, 3x3 rotation, translation) of `m`, which must be a
    rotation with a uniform positive scale."""
    cols = [[m[r][c] for r in range(3)] for c in range(3)]
    scales = [math.sqrt(sum(v * v for v in col)) for col in cols]
    s = sum(scales) / 3
    rot = [[m[r][c] / s for c in range(3)] for r in range(3)]
    ortho = max(
        abs(sum(rot[k][i] * rot[k][j] for k in range(3)) - (i == j))
        for i in range(3)
        for j in range(3)
    )
    if s <= 0 or max(abs(x - s) for x in scales) > 1e-4 * s or ortho > 1e-4:
        raise ConvertError(f"{what} is not a rotation with a uniform scale")
    det = (
        rot[0][0] * (rot[1][1] * rot[2][2] - rot[1][2] * rot[2][1])
        - rot[0][1] * (rot[1][0] * rot[2][2] - rot[1][2] * rot[2][0])
        + rot[0][2] * (rot[1][0] * rot[2][1] - rot[1][1] * rot[2][0])
    )
    if det < 0:
        raise ConvertError(f"{what} mirrors the model")
    return s, rot, tuple(m[r][3] for r in range(3))


def slerp(a, b, f):
    d = sum(x * y for x, y in zip(a, b))
    if d < 0:
        b, d = tuple(-x for x in b), -d
    if d > 0.9995:
        q = tuple(x + (y - x) * f for x, y in zip(a, b))
    else:
        th = math.acos(min(1.0, d))
        sa, sb = math.sin((1 - f) * th), math.sin(f * th)
        q = tuple((x * sa + y * sb) / math.sin(th) for x, y in zip(a, b))
    n = math.sqrt(sum(x * x for x in q))
    return tuple(x / n for x in q)


# --- reading the source --------------------------------------------------


def load_source(folder):
    """The glTF document and its binary buffer, checked."""
    path = os.path.join(folder, "scene.gltf")
    with open(path, "rb") as f:
        text = f.read(MAX_JSON_BYTES + 1)
    if len(text) > MAX_JSON_BYTES:
        raise ConvertError(f"{path} is over {MAX_JSON_BYTES} bytes")
    try:
        doc = json.loads(text.decode("utf-8"))
    except (UnicodeDecodeError, json.JSONDecodeError) as e:
        raise ConvertError(f"{path} is not valid JSON: {e}") from None
    if not isinstance(doc, dict):
        raise ConvertError(f"{path} is not a JSON object")
    buffers = items(doc, "buffers")
    if len(buffers) != 1:
        raise ConvertError(f"{len(buffers)} buffers; the source has one")
    uri = buffers[0].pop("uri", None)
    if not isinstance(uri, str) or not re.fullmatch(r"[\w.-]+\.bin", uri) or uri.startswith("."):
        raise ConvertError(f"buffer uri {uri!r} is not a plain .bin file name")
    with open(os.path.join(folder, uri), "rb") as f:
        binary = f.read(MAX_BIN_BYTES + 1)
    if len(binary) > MAX_BIN_BYTES:
        raise ConvertError(f"{uri} is over {MAX_BIN_BYTES} bytes")
    check(glbtrim.check_buffers, doc, binary)
    return doc, binary


def check(fn, *args):
    """`fn(*args)`, with glbtrim's errors as ours."""
    try:
        return fn(*args)
    except glbtrim.TrimError as e:
        raise ConvertError(str(e)) from None


def items(doc, key):
    return check(glbtrim.items, doc, key)


def index(value, count, what):
    return check(glbtrim.index, value, count, what)


def accessor(doc, binary, a):
    return check(glbtrim.read_accessor, doc, binary, a)


def node_matrix(node, what):
    if "matrix" in node:
        m = node["matrix"]
        if not (isinstance(m, list) and len(m) == 16 and all(finite(v) for v in m)):
            raise ConvertError(f"{what} has a bad matrix")
        return from_gltf(m)
    t = vector(node.get("translation", [0, 0, 0]), 3, f"{what} translation")
    r = vector(node.get("rotation", [0, 0, 0, 1]), 4, f"{what} rotation")
    s = vector(node.get("scale", [1, 1, 1]), 3, f"{what} scale")
    return trs(t, r, s)


def finite(v):
    return isinstance(v, (int, float)) and not isinstance(v, bool) and math.isfinite(v)


def vector(v, n, what):
    if not (isinstance(v, list) and len(v) == n and all(finite(x) for x in v)):
        raise ConvertError(f"{what} is not {n} finite numbers")
    return tuple(float(x) for x in v)


class Source:
    """The parts of the download the conversion reads."""

    def __init__(self, doc, binary):
        self.doc, self.binary = doc, binary
        nodes = items(doc, "nodes")
        self.names = [n.get("name", f"node{k}") for k, n in enumerate(nodes)]
        self.parent = {}
        for k, n in enumerate(nodes):
            children = n.get("children", [])
            if not isinstance(children, list):
                raise ConvertError(f"node {k} children is not a list")
            for c in children:
                c = index(c, len(nodes), f"node {k} child")
                if c in self.parent:
                    raise ConvertError(f"node {c} has two parents")
                self.parent[c] = k
        self.local = [node_matrix(n, f"node {k}") for k, n in enumerate(nodes)]
        self._world = {}
        for k in range(len(nodes)):
            self.world(k)
        skins = items(doc, "skins")
        if len(skins) != 1:
            raise ConvertError(f"{len(skins)} skins; the horse has one")
        joints = skins[0].get("joints")
        if not isinstance(joints, list) or not joints or len(joints) > 256:
            raise ConvertError("the skin has no joints, or too many")
        self.joints = [index(j, len(nodes), "skin joint") for j in joints]
        if len(set(self.joints)) != len(self.joints):
            raise ConvertError("a joint is listed twice")
        ibm = accessor(doc, binary, skins[0].get("inverseBindMatrices"))
        if len(ibm) != len(self.joints):
            raise ConvertError("one inverse bind matrix per joint is needed")
        self.ibm = [from_gltf(m) for m in ibm]
        roots = [j for j in self.joints if self.parent.get(j) not in self.joints]
        if len(roots) != 1:
            raise ConvertError(f"the skeleton has {len(roots)} roots; one is needed")
        self.root = roots[0]
        for j in self.joints:
            if j == self.root:
                continue
            p = self.parent[j]
            if p not in self.joints:
                raise ConvertError(f"joint {self.names[j]!r} hangs off a non-joint")
            _, _, s = node_trs(nodes[j], self.names[j])
            if max(abs(x - 1) for x in s) > 1e-4:
                raise ConvertError(f"joint {self.names[j]!r} is scaled")
        self.trs = {j: node_trs(nodes[j], self.names[j]) for j in self.joints}
        self.prims = self._primitives()
        self.clips = self._clips()

    def world(self, k):
        if k not in self._world:
            seen, chain = set(), []
            n = k
            while n is not None:
                if n in seen:
                    raise ConvertError("the node tree has a cycle")
                seen.add(n)
                chain.append(n)
                n = self.parent.get(n)
            m = identity()
            for n in reversed(chain):
                m = mul(m, self.local[n])
            self._world[k] = m
        return self._world[k]

    def _primitives(self):
        """(material name, mesh node world, attributes) of the kept
        primitives, in MATERIALS order; checks the dropped ones."""
        doc, binary = self.doc, self.binary
        meshes, materials = items(doc, "meshes"), items(doc, "materials")
        found = {}
        for k, node in enumerate(items(doc, "nodes")):
            if "mesh" not in node:
                continue
            mesh = meshes[index(node["mesh"], len(meshes), f"node {k} mesh")]
            if node.get("skin") != 0:
                raise ConvertError(f"mesh node {k} is not skinned to the skeleton")
            for p, prim in enumerate(mesh.get("primitives", [])):
                name = materials[index(prim.get("material"), len(materials), "material")].get("name")
                if name in DROP_MATERIALS:
                    continue
                if name not in MATERIALS or name in found:
                    raise ConvertError(f"unexpected primitive with material {name!r}")
                if prim.get("mode", 4) != 4:
                    raise ConvertError(f"material {name!r} is not triangles")
                attrs = prim.get("attributes", {})
                data = {}
                for key in ("POSITION", "NORMAL", "TANGENT", "TEXCOORD_0", "JOINTS_0", "WEIGHTS_0"):
                    if key in attrs:
                        data[key] = accessor(doc, binary, attrs[key])
                for key in ("POSITION", "NORMAL", "TEXCOORD_0", "JOINTS_0", "WEIGHTS_0"):
                    if key not in data:
                        raise ConvertError(f"material {name!r} has no {key}")
                count = len(data["POSITION"])
                if any(len(v) != count for v in data.values()):
                    raise ConvertError(f"material {name!r} attributes differ in length")
                indices = [i[0] for i in accessor(doc, binary, prim.get("indices"))]
                if len(indices) % 3 or any(not 0 <= i < count for i in indices):
                    raise ConvertError(f"material {name!r} has bad indices")
                for j4 in data["JOINTS_0"]:
                    for j in j4:
                        index(j, len(self.joints), f"material {name!r} JOINTS_0")
                for key in ("POSITION", "NORMAL", "TANGENT", "TEXCOORD_0", "WEIGHTS_0"):
                    for v in data.get(key, []):
                        if not all(math.isfinite(x) for x in v):
                            raise ConvertError(f"material {name!r} {key} is not finite")
                data["indices"] = indices
                found[name] = (self.world(k), data)
        missing = [m for m in MATERIALS if m not in found]
        if missing:
            raise ConvertError(f"no primitive with material {', '.join(missing)}")
        return [(m, *found[m]) for m in MATERIALS]

    def _clips(self):
        """clean name -> list of (joint index, path, times, values)."""
        doc, binary = self.doc, self.binary
        where = {node: k for k, node in enumerate(self.joints)}
        clips = {}
        for a, anim in enumerate(items(doc, "animations")):
            raw = anim.get("name")
            if not isinstance(raw, str):
                raise ConvertError(f"animation {a} has no name")
            name = clean_clip_name(raw)
            if name in clips:
                raise ConvertError(f"two clips are named {name!r}")
            samplers = anim.get("samplers", [])
            channels = []
            for c, channel in enumerate(anim.get("channels", [])):
                sampler = samplers[index(channel.get("sampler"), len(samplers), f"{raw} channel {c}")]
                if sampler.get("interpolation", "LINEAR") != "LINEAR":
                    raise ConvertError(f"clip {raw!r} has a non-linear sampler")
                target = channel.get("target", {})
                node = target.get("node")
                if node not in where:
                    raise ConvertError(f"clip {raw!r} animates a node that is not a joint")
                path = target.get("path")
                times = [t[0] for t in accessor(doc, binary, sampler.get("input"))]
                values = accessor(doc, binary, sampler.get("output"))
                width = {"translation": 3, "rotation": 4, "scale": 3}.get(path)
                if width is None or len(values) != len(times) or not times:
                    raise ConvertError(f"clip {raw!r} channel {c} is malformed")
                if any(len(v) != width or not all(math.isfinite(x) for x in v) for v in values):
                    raise ConvertError(f"clip {raw!r} channel {c} has bad values")
                if any(b < a for a, b in zip(times, times[1:])) or times[0] < 0:
                    raise ConvertError(f"clip {raw!r} channel {c} times go backwards")
                if path == "scale":
                    if max(abs(x - 1) for v in values for x in v) > 1e-4:
                        raise ConvertError(f"clip {raw!r} scales {self.names[node]!r}")
                    continue
                if node == self.root:
                    raise ConvertError(f"clip {raw!r} animates the skeleton root")
                channels.append((where[node], path, times, values))
            clips[name] = channels
        return clips


def node_trs(node, what):
    if "matrix" in node:
        raise ConvertError(f"joint {what!r} has a matrix")
    return (
        vector(node.get("translation", [0, 0, 0]), 3, f"{what} translation"),
        vector(node.get("rotation", [0, 0, 0, 1]), 4, f"{what} rotation"),
        vector(node.get("scale", [1, 1, 1]), 3, f"{what} scale"),
    )


# --- the baked model -----------------------------------------------------


class Model:
    """The horse after the bake: `G` (rotation and uniform scale, plus a
    translation) is the transform from the source's skinned result to ours.

    Every joint world becomes ``G * world * 1/s``, so every joint is rigid:
    children's translations scale by `s`, and the root joint's children
    take ``G * root world`` into their own transform (the root joint itself
    becomes the identity). The vertices are moved to ``G * mesh world`` and
    the inverse bind matrices to ``s * IBM * (G * mesh world)^-1``, so the
    skinned result is ``G`` times the source's, frame by frame."""

    def __init__(self, src, g, clips=CLIPS):
        rigid_root = mul(g, src.world(src.root))
        s, rot_r, t_r = similarity(rigid_root, "the skeleton root")
        q_r = matrix_quat(rot_r)
        self.scale = s
        self.src = src
        self.joints = list(src.joints)
        self.parents = [
            None if j == src.root else src.joints.index(src.parent[j]) for j in src.joints
        ]
        under_root = {k for k, p in enumerate(self.parents) if p == 0}

        def root_child(tr, rq):
            # The joint's new local is rigid_root * (t, r) * 1/s: translation
            # rigid_root(t), rotation q(rigid_root) * r.
            moved = None if tr is None else tuple(
                t_r[i] + s * sum(rot_r[i][k] * tr[k] for k in range(3)) for i in range(3)
            )
            return moved, None if rq is None else quat_mul(q_r, rq)

        self.rest = []
        for k, j in enumerate(src.joints):
            tr, rq, _ = src.trs[j]
            if k == 0:
                if j != src.joints[0]:
                    raise ConvertError("the skeleton root must be the first joint")
                self.rest.append(((0.0, 0.0, 0.0), (0.0, 0.0, 0.0, 1.0)))
            elif k in under_root:
                self.rest.append(root_child(tr, rq))
            else:
                self.rest.append((tuple(x * s for x in tr), rq))
        self.clips = []
        for name in clips:
            if name not in src.clips:
                raise ConvertError(f"the source has no clip {name!r}")
            channels = []
            for k, path, times, values in src.clips[name]:
                if k in under_root:
                    if path == "translation":
                        values = [root_child(v, None)[0] for v in values]
                    else:
                        values = [root_child(None, v)[1] for v in values]
                elif path == "translation":
                    values = [tuple(x * s for x in v) for v in values]
                channels.append((k, path, times, values))
            if name in CLOSE_LOOPS:
                channels = close_loop(name, channels)
            self.clips.append((name, channels))
        extra = sorted(set(src.clips) - set(clips))
        if extra:
            raise ConvertError(f"clips not in the order list: {', '.join(extra)}")
        self.prims = []
        weighted = set()
        for material, world, data in src.prims:
            c = mul(g, world)
            _, c_rot, _ = similarity(c, f"the {material} mesh node")
            ibm_fix = inverse(c)
            out = dict(data)
            out["POSITION"] = [apply(c, p) for p in data["POSITION"]]
            out["NORMAL"] = [unit(linear(c_rot, n)) for n in data["NORMAL"]]
            if "TANGENT" in data:
                out["TANGENT"] = [unit(linear(c_rot, v[:3])) + (v[3],) for v in data["TANGENT"]]
            for j4, w4 in zip(data["JOINTS_0"], data["WEIGHTS_0"]):
                weighted.update(j for j, w in zip(j4, w4) if w > 0)
            self.prims.append((material, out, ibm_fix))
        self.weighted = weighted
        # The inverse bind matrices: one per joint, the mesh nodes must agree.
        fixes = [p[2] for p in self.prims]
        if any(max(abs(a - b) for ra, rb in zip(f, fixes[0]) for a, b in zip(ra, rb)) > 1e-6 for f in fixes):
            raise ConvertError("the mesh nodes do not share one transform")
        unscale = [[s * float(r == c) for c in range(4)] for r in range(3)] + [[0, 0, 0, 1.0]]
        rest_world = self.pose()
        self.ibm = []
        for k in range(len(self.joints)):
            if k in weighted:
                self.ibm.append(mul(mul(unscale, src.ibm[k]), fixes[0]))
            else:
                # Nothing is weighted to it: the inverse of its rest world.
                self.ibm.append(inverse(rest_world[k]))

    def pose(self, clip=None, time=0.0):
        """Joint world matrices at `time` of clip `clip` (None: the rest)."""
        local = list(self.rest)
        if clip is not None:
            local = [list(x) for x in local]
            for k, path, times, values in dict(self.clips)[clip]:
                local[k][0 if path == "translation" else 1] = sample(times, values, time, path)
        worlds = []
        for k, (t, r) in enumerate(local):
            m = trs(t, r)
            p = self.parents[k]
            worlds.append(m if p is None else mul(worlds[p], m))
        return worlds

    def skinned(self, worlds, materials=MATERIALS):
        """Skinned vertex positions of the primitives with `materials`."""
        skin = [mul(w, ibm) for w, ibm in zip(worlds, self.ibm)]
        out = []
        for material, data, _ in self.prims:
            if material not in materials:
                continue
            for p, j4, w4 in zip(data["POSITION"], data["JOINTS_0"], data["WEIGHTS_0"]):
                acc = [0.0, 0.0, 0.0]
                for j, w in zip(j4, w4):
                    if w > 0:
                        q = apply(skin[j], p)
                        acc = [a + w * b for a, b in zip(acc, q)]
                out.append(tuple(acc))
        return out

    def hooves(self, worlds):
        """The four hoof centroids (`LEGS` order) for joint `worlds`."""
        if not hasattr(self, "_shoes"):
            legs = [self.joint(name) for name in LEGS]
            self._shoes = [[] for _ in legs]
            data = next(d for m, d, _ in self.prims if m == "Horseshoe")
            for v, (j4, w4) in enumerate(zip(data["JOINTS_0"], data["WEIGHTS_0"])):
                k = max(zip(w4, j4))[1]
                while k is not None and k not in legs:
                    k = self.parents[k]
                if k is None:
                    raise ConvertError(f"horseshoe vertex {v} is on no leg")
                self._shoes[legs.index(k)].append(v)
            if not all(self._shoes):
                raise ConvertError("a leg has no horseshoe")
        points = self.skinned(worlds, ("Horseshoe",))
        return [
            tuple(sum(points[v][i] for v in shoe) / len(shoe) for i in range(3))
            for shoe in self._shoes
        ]

    def duration(self, clip):
        return max(times[-1] for _, _, times, _ in dict(self.clips)[clip])

    def joint(self, name):
        for k, j in enumerate(self.joints):
            if self.src.names[j] == name:
                return k
        raise ConvertError(f"no joint {name!r}")


def close_loop(name, channels):
    """`channels` with a closing key, the first, `LOOP_FRAME` after the
    clip's last (see `CLOSE_LOOPS`); a rotation on the side of the last key,
    so it turns the short way."""
    end = max(times[-1] for _, _, times, _ in channels)
    out = []
    for k, path, times, values in channels:
        if abs(times[-1] - end) > 1e-6:
            raise ConvertError(f"clip {name!r} channels end at different times")
        first = values[0]
        if path == "rotation" and sum(a * b for a, b in zip(first, values[-1])) < 0:
            first = tuple(-x for x in first)
        out.append((k, path, [*times, end + LOOP_FRAME], [*values, tuple(first)]))
    return out


def unit(v):
    n = math.sqrt(sum(x * x for x in v)) or 1.0
    return tuple(x / n for x in v)


def sample(times, values, t, path):
    if t <= times[0]:
        return values[0]
    if t >= times[-1]:
        return values[-1]
    lo, hi = 0, len(times) - 1
    while hi - lo > 1:
        mid = (lo + hi) // 2
        if times[mid] <= t:
            lo = mid
        else:
            hi = mid
    f = (t - times[lo]) / (times[hi] - times[lo]) if times[hi] > times[lo] else 0.0
    if path == "rotation":
        return slerp(values[lo], values[hi], f)
    return tuple(a + (b - a) * f for a, b in zip(values[lo], values[hi]))


def placement(src, clips=CLIPS):
    """The transform G that takes the source's skinned result to ours: the
    source's own wrappers and scale, then a turn about Y so the head faces
    +Z, then a shift so the hooves stand on y = 0 around the origin, in the
    first frame of `REST_CLIP`."""
    g = [[METRES * float(r == c) for c in range(3)] + [0.0] for r in range(3)] + [[0, 0, 0, 1.0]]
    model = Model(src, g, clips)
    worlds = model.pose(REST_CLIP, 0.0)
    head = worlds[model.joint(HEAD)]
    tail = worlds[model.joint(PELVIS)]
    dx, dz = head[0][3] - tail[0][3], head[2][3] - tail[2][3]
    yaw = math.atan2(dx, dz)  # turning by -yaw about Y faces +Z
    quarter = round(yaw / (math.pi / 2)) * (math.pi / 2)
    if abs(yaw - quarter) < math.radians(2):
        yaw = quarter
    c, s = math.cos(-yaw), math.sin(-yaw)
    turn = [[c, 0, s, 0], [0, 1, 0, 0], [-s, 0, c, 0], [0, 0, 0, 1.0]]
    for row in turn:
        for i, v in enumerate(row):
            if abs(v) < 1e-12:
                row[i] = 0.0
            elif abs(abs(v) - 1) < 1e-12:
                row[i] = math.copysign(1.0, v)
    hooves = [apply(turn, p) for p in model.hooves(worlds)]
    feet = [apply(turn, p) for p in model.skinned(worlds, ("Horse", "Horseshoe"))]
    cx = sum(p[0] for p in hooves) / len(hooves)
    cz = sum(p[2] for p in hooves) / len(hooves)
    floor = min(p[1] for p in feet)
    shift = [[1, 0, 0, -cx], [0, 1, 0, -floor], [0, 0, 1, -cz], [0, 0, 0, 1.0]]
    return mul(mul(shift, turn), g)


def convert(src, clips=CLIPS):
    """The baked model, placed."""
    return Model(src, placement(src, clips), clips)


# --- dropping the stirrups -----------------------------------------------


def kept_joints(model):
    """Indices of the joints to keep: all but the `DROP_JOINTS` bones and
    their children, unless a kept vertex is weighted to one of them."""
    drop = set()
    for k, j in enumerate(model.joints):
        p = model.parents[k]
        if DROP_JOINTS.search(model.src.names[j]) or p in drop:
            drop.add(k)
    if drop & model.weighted:
        names = sorted(model.src.names[model.joints[k]] for k in drop & model.weighted)
        raise ConvertError(f"kept vertices are weighted to {', '.join(names)}")
    return [k for k in range(len(model.joints)) if k not in drop]


# --- writing the GLB -----------------------------------------------------


class Writer:
    def __init__(self):
        self.blob = bytearray()
        self.views, self.accessors = [], []
        self._times = {}

    def add(self, values, kind, component=5126, minmax=False, target=None):
        fmt = {5126: "f", 5123: "H", 5125: "I", 5121: "B"}[component]
        width = glbtrim.WIDTHS[kind]
        flat = [x for v in values for x in (v if isinstance(v, (tuple, list)) else (v,))]
        data = struct.pack(f"<{len(flat)}{fmt}", *flat)
        self.blob += b"\0" * (-len(self.blob) % 4)
        view = {"buffer": 0, "byteOffset": len(self.blob), "byteLength": len(data)}
        if target:
            view["target"] = target
        self.views.append(view)
        self.blob += data
        acc = {"bufferView": len(self.views) - 1, "componentType": component, "count": len(values), "type": kind}
        if minmax:
            stored = struct.unpack(f"<{len(flat)}{fmt}", data)
            cols = [stored[i::width] for i in range(width)]
            acc["min"] = [min(c) for c in cols]
            acc["max"] = [max(c) for c in cols]
        self.accessors.append(acc)
        return len(self.accessors) - 1

    def times(self, times):
        key = struct.pack(f"<{len(times)}f", *times)
        if key not in self._times:
            self._times[key] = self.add(times, "SCALAR", minmax=True)
        return self._times[key]


def write(model):
    """GLB bytes of the placed model."""
    keep = kept_joints(model)
    new = {k: i for i, k in enumerate(keep)}
    w = Writer()
    names = [model.src.names[model.joints[k]] for k in keep]
    # Nodes: 0 the armature, 1 the mesh, then the joints in skin order.
    nodes = [{"name": "Armature", "children": [1, 2]}, {"name": "Horse", "mesh": 0, "skin": 0}]
    for i, k in enumerate(keep):
        t, r = model.rest[k]
        node = {"name": names[i]}
        if any(abs(x) > 0 for x in t):
            node["translation"] = [f32(x) for x in t]
        if any(abs(x) > 0 for x in r[:3]):
            node["rotation"] = [f32(x) for x in r]
        children = [2 + new[c] for c, p in enumerate(model.parents) if p == k and c in new]
        if children:
            node["children"] = children
        nodes.append(node)
    primitives, materials = [], []
    for m, (material, data, _) in enumerate(model.prims):
        attrs = {
            "POSITION": w.add(data["POSITION"], "VEC3", minmax=True, target=34962),
            "NORMAL": w.add(data["NORMAL"], "VEC3", target=34962),
        }
        if "TANGENT" in data:
            attrs["TANGENT"] = w.add(data["TANGENT"], "VEC4", target=34962)
        attrs["TEXCOORD_0"] = w.add(data["TEXCOORD_0"], "VEC2", target=34962)
        attrs["JOINTS_0"] = w.add(
            [tuple(new[j] if wt > 0 else 0 for j, wt in zip(j4, w4)) for j4, w4 in zip(data["JOINTS_0"], data["WEIGHTS_0"])],
            "VEC4",
            component=5121,
            target=34962,
        )
        attrs["WEIGHTS_0"] = w.add(data["WEIGHTS_0"], "VEC4", target=34962)
        big = len(data["POSITION"]) > 65535
        indices = w.add(data["indices"], "SCALAR", component=5125 if big else 5123, target=34963)
        primitives.append({"attributes": attrs, "indices": indices, "material": m})
        source = model.src.doc["materials"]
        props = next(x for x in source if x.get("name") == material)
        pbr = props.get("pbrMetallicRoughness", {})
        out = {"name": material, "doubleSided": True, "pbrMetallicRoughness": {
            "metallicFactor": 0.0, "roughnessFactor": round(float(pbr.get("roughnessFactor", 1.0)), 4)}}
        if "baseColorFactor" in pbr:
            out["pbrMetallicRoughness"]["baseColorFactor"] = [round(float(x), 6) for x in pbr["baseColorFactor"]]
        if material == "Hair":
            out["alphaMode"] = "MASK"
            out["alphaCutoff"] = 0.5
        materials.append(out)
    ibm = w.add([to_gltf(model.ibm[k]) for k in keep], "MAT4")
    animations = []
    for name, channels in model.clips:
        samplers, chans = [], []
        for k, path, times, values in channels:
            if k not in new:
                continue
            samplers.append({"input": w.times(times), "output": w.add(values, "VEC3" if path == "translation" else "VEC4"), "interpolation": "LINEAR"})
            chans.append({"sampler": len(samplers) - 1, "target": {"node": 2 + new[k], "path": path}})
        animations.append({"name": name, "channels": chans, "samplers": samplers})
    doc = {
        "asset": {
            "version": "2.0",
            "generator": "pampa-seca tools/henry_horse.py",
            "copyright": "\"Horse\" by henrysteve973, CC-BY-4.0",
            "extras": {
                "title": "Horse",
                "author": "henrysteve973 (https://sketchfab.com/henrysteve973)",
                "license": "CC-BY-4.0 (http://creativecommons.org/licenses/by/4.0/)",
                "source": "https://sketchfab.com/3d-models/horse-a6f860e43e364619bccb174a1ac7d0c9",
            },
        },
        "scene": 0,
        "scenes": [{"name": "Scene", "nodes": [0]}],
        "nodes": nodes,
        "meshes": [{"name": "Horse", "primitives": primitives}],
        "materials": materials,
        "skins": [{"name": "Horse", "inverseBindMatrices": ibm, "joints": [2 + i for i in range(len(keep))], "skeleton": 2}],
        "animations": animations,
        "accessors": w.accessors,
        "bufferViews": w.views,
        "buffers": [{"byteLength": len(w.blob)}],
    }
    return glbtrim.write_glb(doc, bytes(w.blob))


def f32(x):
    return struct.unpack("<f", struct.pack("<f", x))[0]


# --- the report ----------------------------------------------------------


def clip_report(model, clip):
    """Length, root motion and the speed measured on the planted hooves:
    a hoof is planted while its centroid is within `PLANTED` of its lowest
    point in the clip and slides back; the speed is how fast the planted hooves slide back
    (an in-place cycle) plus how fast the root moves forward."""
    length = model.duration(clip)
    root, pelvis = model.joint(ROOT), model.joint(PELVIS)
    steps = max(2, int(round(length * SAMPLE_HZ)))
    dt = length / steps
    frames = [model.pose(clip, dt * i) for i in range(steps + 1)]
    path = [tuple(f[root][r][3] for r in range(3)) for f in frames]
    hips = [(f[pelvis][1][3], f[pelvis][2][3]) for f in frames]
    feet = list(zip(*(model.hooves(f) for f in frames)))
    slid = time = planted = 0.0
    for foot in feet:
        low = min(p[1] for p in foot)
        for a, b in zip(foot, foot[1:]):
            if a[1] < low + PLANTED and b[1] < low + PLANTED and b[2] < a[2]:
                slid += a[2] - b[2]
                time += dt
                planted += 1 / steps / len(feet)
    first, last = path[0], path[-1]
    return {
        "clip": clip,
        "length": length,
        "root": tuple(b - a for a, b in zip(first, last)),
        "hips": (min(h[0] for h in hips), max(h[0] for h in hips)),
        "hips_dz": hips[-1][1] - hips[0][1],
        "lowest": (min(min(p[1] for p in f) for f in zip(*feet)), max(min(p[1] for p in f) for f in zip(*feet))),
        "duty": planted if clip in CYCLES else None,
        "slide": slid / time if time and clip in CYCLES else None,
        "root_speed": (last[2] - first[2]) / length,
    }


def report(model, out=sys.stdout):
    rest = model.skinned(model.pose(REST_CLIP, 0.0))
    withers = model.pose(REST_CLIP, 0.0)[model.joint("BN_Spine_03_05_05")]
    tris = sum(len(d["indices"]) // 3 for _, d, _ in model.prims)
    parts = ", ".join(f"{m} {len(d['indices']) // 3}" for m, d, _ in model.prims)
    print(f"triangles {tris} ({parts})", file=out)
    print(f"joints {len(kept_joints(model))} (of {len(model.joints)}); scale baked {model.scale:.6g}", file=out)
    lo = [min(p[i] for p in rest) for i in range(3)]
    hi = [max(p[i] for p in rest) for i in range(3)]
    print(f"{REST_CLIP} frame 0 bounds min {fmt(lo)} max {fmt(hi)}; withers joint y {withers[1][3]:.3f}", file=out)
    print(
        "clip | length (s) | root dx, dy, dz (m) | pelvis y (m) | pelvis dz (m) | lowest hoof y (m)"
        " | planted share | hoof slide (m/s) | root speed (m/s)",
        file=out,
    )
    for name, _ in model.clips:
        r = clip_report(model, name)
        slide = "-" if r["slide"] is None else f"{r['slide']:.2f}"
        duty = "-" if r["duty"] is None else f"{r['duty']:.2f}"
        root = ", ".join(f"{x:+.2f}" for x in r["root"])
        print(
            f"{name} | {r['length']:.3f} | {root} | {r['hips'][0]:.2f}..{r['hips'][1]:.2f} | "
            f"{r['hips_dz']:+.2f} | {r['lowest'][0]:.2f}..{r['lowest'][1]:.2f} | {duty} | {slide} | "
            f"{r['root_speed']:.2f}",
            file=out,
        )


def fmt(v):
    return "(" + ", ".join(f"{x:.3f}" for x in v) + ")"


# --- textures --------------------------------------------------------------


def image_path(folder, name):
    path = os.path.join(folder, "textures", name)
    if not os.path.isfile(path):
        raise ConvertError(f"missing texture {name}")
    return path


def write_textures(folder, out):
    """The JPG sources of the KTX2 sets, with ImageMagick."""
    import dae2gltf  # the normal-map green check

    for set_name, files in TEXTURES.items():
        target = os.path.join(out, set_name)
        os.makedirs(target, exist_ok=True)
        for file, (image, size, kind) in files.items():
            src = "png:" + image_path(folder, image)
            dst = os.path.join(target, file)
            resize = ["-resize", f"{size}x{size}>"]
            if kind == "color":
                cmd = [src, *resize, "-colorspace", "sRGB", "-type", "TrueColor"]
            elif kind == "opacity":
                cmd = [src, *resize, "-colorspace", "Gray", "-level", ",".join(HAIR_LEVELS)]
            elif kind == "roughness":
                grey = round(ROUGHNESS * 255)
                cmd = [src, *resize, "-fill", f"rgb({grey},{grey},{grey})", "-colorize", "100", "-colorspace", "Gray"]
            else:
                cmd = [src, *resize, "-colorspace", "sRGB", "-type", "TrueColor"]
                with tempfile.TemporaryDirectory() as tmp:
                    probe = os.path.join(tmp, "n.png")
                    magick([src, "-resize", "512x512>", "-depth", "8", "-type", "TrueColor", probe])
                    convention, ratio = dae2gltf.green_convention(*dae2gltf.png_read(probe))
                print(f"{set_name}/{file}: green {convention.upper()} (curl ratio {ratio:.2f})")
                if convention == "dx":
                    cmd += ["-channel", "G", "-negate", "+channel"]
            magick([*cmd, "-strip", "-quality", "92", "-sampling-factor", "1x1", "jpg:" + dst])
            print(f"{set_name}/{file}: from {image}")


def magick(args):
    try:
        subprocess.run(["magick", *args], check=True, stdin=subprocess.DEVNULL)
    except (OSError, subprocess.CalledProcessError) as e:
        raise ConvertError(f"magick failed: {e}") from None


# --- main ------------------------------------------------------------------


def main(argv=None):
    parser = argparse.ArgumentParser(description="Convert the Sketchfab Horse by henrysteve973 to the game's GLB.")
    parser.add_argument("source", help="the extracted download (scene.gltf, scene.bin, textures/)")
    parser.add_argument("out", help="the asset folder: horse.glb and the texture sets go here")
    parser.add_argument("--no-textures", action="store_true", help="write only horse.glb")
    parser.add_argument("--no-report", action="store_true", help="skip the per-clip measurements")
    args = parser.parse_args(argv)
    try:
        src = Source(*load_source(args.source))
        model = convert(src)
        data = write(model)
        os.makedirs(args.out, exist_ok=True)
        with open(os.path.join(args.out, "horse.glb"), "wb") as f:
            f.write(data)
        size, tris, joints, clips = glbtrim.summary(data)
        print(f"horse.glb: {size} bytes, {tris} triangles, {joints} joints, {len(clips)} clips: {', '.join(clips)}")
        if not args.no_report:
            report(model)
        if not args.no_textures:
            write_textures(args.source, args.out)
    except (OSError, ConvertError) as e:
        print(f"henry_horse: {e}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
