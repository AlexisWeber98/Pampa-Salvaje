#!/usr/bin/env python3
"""Convert a MakeHuman character (.mhm) into a skinned glTF 2.0 model.

Standard library plus numpy; run it isolated: ``python3 -I tools/mh2gltf.py``.
It needs MakeHuman 1.x's *data* folder (the compiled ``.npz`` / ``.mhpxy``
assets, rigs and targets) but none of its code: what MakeHuman does with a
character is reproduced here from its sources (see tools/README.md), and
the GUI is never started. The data is local, but every file is read as
untrusted text or arrays: bounded sizes, ``allow_pickle=False``, every
index checked, every path kept inside the data folder or the .mhm's own
folder.

What it reproduces:

- the base mesh morphed by every modifier: macro modifiers (gender, age,
  ethnics, muscle, weight, height, proportions, breast) through
  MakeHuman's macro variables and target factors (apps/humanmodifier.py,
  apps/human.py, lib/targets.py), other modifiers through their
  decr/incr targets;
- the body *proxy* (a low-poly body mesh) and every other proxy (eyes,
  eyebrows, eyelashes, hair, clothes) fitted to the morphed mesh by their
  reference vertices, weights, offsets and scale matrix (shared/proxy.py);
- clothes hiding what is under them (``deleteVerts``, outermost layer
  first by ``z_depth``), with those faces left out of the output;
- the skeleton (``skeleton`` line, e.g. ``game_engine.mhskel``): joints
  from its reference vertices on the morphed mesh, bone orientation copied
  from the default skeleton's plane normals, vertex weights from the
  default weights remapped through the bones' weight references and then
  through each proxy's reference vertices (shared/skeleton.py,
  shared/animation.py); at most 4 influences, normalized;
- materials (.mhmat): the diffuse texture and colour, the normal map,
  transparency (alpha mask), back-face culling and a roughness from the
  shininess. A ``material`` line may name a .mhmat in the .mhm's own
  folder, so a character can recolour a garment with ``diffuseColor``.

What it writes, into the output folder: ``model.gltf``, ``model.bin`` and
``textures/`` (PNG, downscaled to ``--max-texture``). Units are meters, Y
up, facing +Z, the lowest point on y = 0. One skinned mesh node named after
the character, one primitive per proxy, and the joint hierarchy.
"""

import argparse
import json
import math
import os
import re
import struct
import sys
import urllib.parse
import zipfile
import zlib

try:
    import numpy as np
except ImportError:  # pragma: no cover - reported by main()
    np = None

DEFAULT_DATA = "~/.local/opt/makehuman-1.3.0/makehuman/data"
MAX_TEXT_BYTES = 4 * 1024 * 1024
MAX_JSON_BYTES = 16 * 1024 * 1024
MAX_NPZ_BYTES = 64 * 1024 * 1024
MAX_ARRAY_ITEMS = 8_000_000
MAX_IMAGE_PIXELS = 4096 * 4096
MAX_INFLUENCES = 4
# MakeHuman's units are decimeters.
METERS = 0.1
SAFE_NAME = re.compile(r"^[A-Za-z0-9][A-Za-z0-9._ -]{0,127}$")
UUID = re.compile(r"^[0-9a-fA-F-]{8,64}$")
# Proxy kinds of an .mhm and the data folder each lives in.
PROXY_KINDS = {
    "proxy": "proxymeshes",
    "eyes": "eyes",
    "eyebrows": "eyebrows",
    "eyelashes": "eyelashes",
    "hair": "hair",
    "clothes": "clothes",
}
# Lines MakeHuman writes that change nothing here (camera, UI state).
IGNORED_KEYS = {
    "version", "tags", "camera", "subdivide", "status", "background",
    "expression", "pose", "teeth", "tongue", "custom", "skeleton_pose",
}
WEIGHT_THRESHOLD = 1e-4

# lib/targets.py: macro variable categories and their values.
CATEGORIES = [
    ("gender", ["male", "female"]),
    ("age", ["baby", "child", "young", "old"]),
    ("race", ["caucasian", "asian", "african"]),
    ("muscle", ["maxmuscle", "averagemuscle", "minmuscle"]),
    ("weight", ["minweight", "averageweight", "maxweight"]),
    ("height", ["minheight", "averageheight", "maxheight"]),
    ("breastsize", ["mincup", "averagecup", "maxcup"]),
    ("breastfirmness", ["minfirmness", "averagefirmness", "maxfirmness"]),
    ("bodyproportions", ["uncommonproportions", "regularproportions", "idealproportions"]),
]
VALUE_CATEGORY = {v: c for c, values in CATEGORIES for v in values}
# Macro modifier variable -> human attribute it sets (apps/human.py).
MACRO_VARIABLES = {
    "Gender": "gender", "Age": "age", "Muscle": "muscle", "Weight": "weight",
    "Height": "height", "BodyProportions": "bodyproportions",
    "BreastSize": "breastsize", "BreastFirmness": "breastfirmness",
    "African": "african", "Asian": "asian", "Caucasian": "caucasian",
}


class ConvertError(Exception):
    """The input cannot be converted safely."""


def f32(x):
    """The value a float32 stores for x (accessor bounds must match it)."""
    return struct.unpack("<f", struct.pack("<f", x))[0]


# --- safe reading ------------------------------------------------------------


def inside(path, root):
    path, root = os.path.realpath(path), os.path.realpath(root)
    return path == root or path.startswith(root + os.sep)


def read_bytes(path, limit):
    size = os.path.getsize(path)
    if size > limit:
        raise ConvertError(f"{os.path.basename(path)} is {size} bytes; the limit is {limit}")
    with open(path, "rb") as f:
        return f.read()


def read_text(path, limit=MAX_TEXT_BYTES):
    try:
        return read_bytes(path, limit).decode("utf-8")
    except UnicodeDecodeError as e:
        raise ConvertError(f"{os.path.basename(path)} is not UTF-8 text") from e


def read_json(path):
    data = json.loads(read_text(path, MAX_JSON_BYTES), parse_constant=_no_constant)
    if not isinstance(data, dict):
        raise ConvertError(f"{os.path.basename(path)}: not a JSON object")
    return data


def _no_constant(name):
    raise ConvertError(f"non-finite JSON number {name}")


def number(text, what):
    try:
        v = float(text)
    except ValueError as e:
        raise ConvertError(f"{what}: {text!r} is not a number") from e
    if not math.isfinite(v):
        raise ConvertError(f"{what}: {text!r} is not finite")
    return v


class Npz:
    """A numpy archive read without pickles, with checked members."""

    def __init__(self, path):
        if os.path.getsize(path) > MAX_NPZ_BYTES:
            raise ConvertError(f"{os.path.basename(path)} is too large")
        self.name = os.path.basename(path)
        try:
            self.z = np.load(path, allow_pickle=False)
        except (ValueError, OSError, zipfile.BadZipFile) as e:
            raise ConvertError(f"{self.name}: not a numpy archive ({e})") from e
        self.files = set(self.z.files)

    def array(self, key, kinds, ndim=None, width=None):
        if key not in self.files:
            raise ConvertError(f"{self.name}: missing {key!r}")
        try:
            a = self.z[key]
        except ValueError as e:
            raise ConvertError(f"{self.name}: {key!r} is not a plain array") from e
        if a.dtype.kind not in kinds or a.size > MAX_ARRAY_ITEMS:
            raise ConvertError(f"{self.name}: {key!r} has type {a.dtype} or is too large")
        if ndim is not None and a.ndim != ndim:
            raise ConvertError(f"{self.name}: {key!r} has {a.ndim} dimensions, not {ndim}")
        if width is not None and a.shape[-1] != width:
            raise ConvertError(f"{self.name}: {key!r} rows are {a.shape[-1]} wide, not {width}")
        if a.dtype.kind == "f" and not np.all(np.isfinite(a)):
            raise ConvertError(f"{self.name}: {key!r} holds non-finite numbers")
        return a

    def text(self, key, default=None):
        if key not in self.files:
            if default is None:
                raise ConvertError(f"{self.name}: missing {key!r}")
            return default
        a = self.array(key, "SU")
        raw = a.tobytes() if a.dtype.kind == "S" else "".join(a.tolist()).encode()
        return raw.decode("utf-8", "replace")

    def scalar(self, key, default):
        if key not in self.files:
            return default
        a = self.array(key, "iuf")
        return a.reshape(-1)[0].item() if a.size else default


def checked_indices(a, limit, what):
    if a.size and (a.min() < 0 or a.max() >= limit):
        raise ConvertError(f"{what}: an index is out of range (0..{limit - 1})")
    return a.astype(np.int64)


# --- the .mhm -------------------------------------------------------------------


class Character:
    """What an .mhm file says."""

    def __init__(self):
        self.name = None
        self.modifiers = {}
        self.skeleton = None
        self.skin = None
        self.proxies = []  # (kind, name, uuid) in file order
        self.materials = {}  # proxy uuid -> material path
        self.hide_faces = True
        self.ignored = []


def read_mhm(path):
    c = Character()
    for number_, line in enumerate(read_text(path).splitlines(), 1):
        line = line.strip()
        if not line or line.startswith("#"):
            continue
        words = line.split()
        key, values = words[0], words[1:]
        where = f"{os.path.basename(path)} line {number_}"
        if key == "name":
            c.name = " ".join(values)
        elif key == "modifier":
            if len(values) != 2:
                raise ConvertError(f"{where}: modifier needs a name and a value")
            c.modifiers[values[0]] = number(values[1], where)
        elif key == "skeleton":
            c.skeleton = " ".join(values)
        elif key == "skinMaterial":
            c.skin = " ".join(values)
        elif key in PROXY_KINDS:
            if len(values) < 2 or not UUID.match(values[-1]):
                raise ConvertError(f"{where}: {key} needs a name and a UUID")
            c.proxies.append((key, " ".join(values[:-1]), values[-1].lower()))
        elif key == "material":
            if len(values) < 2 or not UUID.match(values[-2]):
                raise ConvertError(f"{where}: material needs [name] UUID path")
            c.materials[values[-2].lower()] = values[-1]
        elif key == "clothesHideFaces":
            c.hide_faces = bool(values) and values[0].lower() in ("true", "yes")
        elif key in IGNORED_KEYS:
            continue
        else:
            c.ignored.append(key)
    if c.name is None:
        c.name = os.path.splitext(os.path.basename(path))[0]
    if not SAFE_NAME.match(c.name):
        raise ConvertError(f"the character name {c.name!r} is not a plain name")
    return c


class Files:
    """Resolves the paths an .mhm or .mhmat names: inside the .mhm's own
    folder or the MakeHuman data folder, never elsewhere."""

    def __init__(self, data, local):
        self.data = os.path.realpath(data)
        self.local = os.path.realpath(local)

    def find(self, name, folders):
        if not name or "\\" in name or "\x00" in name or os.path.isabs(name):
            raise ConvertError(f"unsafe path {name!r}")
        tries = []
        for folder in folders:
            tries.append(os.path.join(folder, name))
        if name.startswith("data/"):
            tries.append(os.path.join(self.data, name[5:]))
        tries.append(os.path.join(self.data, name))
        for t in tries:
            real = os.path.realpath(t)
            if (inside(real, self.data) or inside(real, self.local)) and os.path.isfile(real):
                return real
        raise ConvertError(f"{name!r} not found in the .mhm's folder or the data folder")


# --- targets and modifiers ----------------------------------------------------


class Targets:
    """The compiled targets (data/targets.npz), grouped like lib/targets.py
    groups them: a key of name parts, and the macro values the path names."""

    def __init__(self, data):
        path = os.path.join(data, "targets.npz")
        self.npz = Npz(path)
        self.groups = {}
        for member in sorted(self.npz.files):
            if not member.endswith(".index") or not member.startswith("targets/"):
                continue
            base = member[: -len(".index")]
            if base + ".vector" not in self.npz.files:
                continue
            key, values = [], {}
            parts_of_path = base.split("/")
            parts_of_path[-1] += ".target"
            for component in parts_of_path:
                for i, part in enumerate(re.split(r"[-_.]", component)):
                    if i == 0 and part == "targets":
                        continue
                    category = VALUE_CATEGORY.get(part)
                    if category is not None:
                        values[category] = part
                    elif part != "target":
                        key.append(part)
            self.groups.setdefault("-".join(key), []).append((base, sorted(values.values())))

    def load(self, base, vertex_count):
        index = self.npz.array(base + ".index", "iu", ndim=1)
        vector = self.npz.array(base + ".vector", "if", ndim=2, width=3)
        if len(index) != len(vector):
            raise ConvertError(f"target {base}: index and vector lengths differ")
        return checked_indices(index, vertex_count, f"target {base}"), vector.astype(np.float64) * 1e-3


def macro_values(settings):
    """apps/human.py's _set*Vals: the factor of every macro value."""
    v = {}
    g = settings["gender"]
    v["male"], v["female"] = g, 1 - g
    age = settings["age"]
    if age < 0.5:
        v["old"] = 0.0
        v["baby"] = max(0.0, 1 - age * 5.333)
        v["young"] = max(0.0, (age - 0.1875) * 3.2)
        v["child"] = max(0.0, min(1.0, 5.333 * age) - v["young"])
    else:
        v["child"] = v["baby"] = 0.0
        v["old"] = max(0.0, age * 2 - 1)
        v["young"] = 1 - v["old"]
    for var, (lo, mid, hi) in (
        ("muscle", ("minmuscle", "averagemuscle", "maxmuscle")),
        ("weight", ("minweight", "averageweight", "maxweight")),
        ("height", ("minheight", "averageheight", "maxheight")),
        ("breastsize", ("mincup", "averagecup", "maxcup")),
        ("breastfirmness", ("minfirmness", "averagefirmness", "maxfirmness")),
        ("bodyproportions", ("uncommonproportions", "regularproportions", "idealproportions")),
    ):
        x = settings[var]
        v[hi] = max(0.0, x * 2 - 1)
        v[lo] = max(0.0, 1 - x * 2)
        v[mid] = 1 - (v[hi] + v[lo])
    total = sum(settings[e] for e in ("african", "asian", "caucasian"))
    for e in ("african", "asian", "caucasian"):
        v[e] = settings[e] / total if total > 0 else 1.0 / 3
    return v


def load_modifiers(data):
    """Every modifier MakeHuman defines, in its order: (full name, kind,
    group, definition)."""
    out = []
    for file in ("modeling_modifiers.json", "measurement_modifiers.json", "bodyshapes_modifiers.json"):
        path = os.path.join(data, "modifiers", file)
        if not os.path.isfile(path):
            continue
        groups = json.loads(read_text(path, MAX_JSON_BYTES), parse_constant=_no_constant)
        if not isinstance(groups, list):
            raise ConvertError(f"{file}: not a list of groups")
        for group in groups:
            name = str(group.get("group", "")).replace("/", "-")
            for m in group.get("modifiers", []):
                if "macrovar" in m:
                    out.append((f"{name}/{m['macrovar']}", "macro", name, m))
                    continue
                target = str(m["target"])
                left, right, mid = m.get("min"), m.get("max"), m.get("mid")
                if left and right:
                    label = f"{target}-{left}|{mid}|{right}" if mid else f"{target}-{left}|{right}"
                else:
                    label = target
                out.append((f"{name}/{label}", "universal", name, m))
    return out


def target_weights(character, targets, data):
    """The weight of every target (MakeHuman's 'details'), from the .mhm's
    modifier values and the defaults of the rest."""
    modifiers = load_modifiers(data)
    settings = {
        "gender": 0.5, "age": 0.5, "muscle": 0.5, "weight": 0.5, "height": 0.5,
        "bodyproportions": 0.5, "breastsize": 0.5, "breastfirmness": 0.5,
        "african": 1 / 3, "asian": 1 / 3, "caucasian": 1 / 3,
    }
    known = {full for full, _, _, _ in modifiers}
    unknown = sorted(set(character.modifiers) - known)
    if unknown:
        raise ConvertError(f"unknown modifiers: {', '.join(unknown[:5])}")
    for full, kind, _, m in modifiers:
        if kind == "macro" and full in character.modifiers:
            var = MACRO_VARIABLES.get(m["macrovar"])
            if var is None:
                raise ConvertError(f"unsupported macro variable {m['macrovar']!r}")
            settings[var] = min(max(character.modifiers[full], 0.0), 1.0)
    factors = macro_values(settings)
    details = {}
    for full, kind, group, m in modifiers:
        if kind == "macro":
            f = dict(factors)
            f[group] = 1.0
            for base, values in targets.groups.get(group, []):
                details[base] = product(f, values + [group])
            continue
        value = character.modifiers.get(full, float(m.get("defaultValue", 0.0)))
        stem = f"{group}-{m['target']}"
        left = f"{stem}-{m['min']}" if m.get("min") and m.get("max") else None
        right = f"{stem}-{m['max']}" if left else stem
        center = f"{stem}-{m['mid']}" if left and m.get("mid") else None
        value = min(1.0, max(-1.0 if left else 0.0, value))
        f = dict(factors)
        if left:
            f[left] = -min(value, 0.0)
        if center:
            f[center] = 1.0 - abs(value)
        f[right] = max(0.0, value)
        for side in (left, right, center):
            if side is None:
                continue
            for base, values in targets.groups.get(side, []):
                details[base] = product(f, values + [side])
    return details


def product(factors, names):
    p = 1.0
    for n in names:
        p *= factors[n]
    return p


def morph(base, details, targets):
    coords = base.astype(np.float64)
    for path in sorted(details):
        w = details[path]
        if w == 0.0:
            continue
        index, vector = targets.load(path, len(coords))
        np.add.at(coords, index, w * vector)
    return coords


# --- skeleton -----------------------------------------------------------------


def normalize(v):
    n = np.linalg.norm(v)
    return v / n if n > 1e-12 else v * 0.0


def bone_matrix(head, tail, normal):
    """shared/skeleton.py getMatrix: Y along the bone, X close to normal."""
    direction = normalize(tail - head)
    normal = normalize(normal)
    z = normalize(np.cross(normal, direction))
    if not np.any(z):
        # The normal lies along the bone: any perpendicular will do.
        z = normalize(np.cross([1.0, 0.0, 0.0] if abs(direction[0]) < 0.9 else [0.0, 0.0, 1.0], direction))
    x = normalize(np.cross(direction, z))
    m = np.identity(4)
    m[:3, 0], m[:3, 1], m[:3, 2], m[:3, 3] = x, direction, z, head
    return m


class Rig:
    """A .mhskel: bones in MakeHuman's breadth-first order."""

    def __init__(self, path, vertex_count):
        d = read_json(path)
        self.path = path
        self.name = str(d.get("name", ""))
        bones = d.get("bones")
        if not isinstance(bones, dict) or not bones:
            raise ConvertError(f"{os.path.basename(path)}: no bones")
        self.joints = {}
        for name, idx in d.get("joints", {}).items():
            if isinstance(idx, list) and idx:
                a = np.asarray(idx)
                if a.dtype.kind not in "iu":
                    raise ConvertError(f"joint {name}: vertex indices are not integers")
                self.joints[name] = checked_indices(a, vertex_count, f"joint {name}")
        self.planes = d.get("planes", {}) or {}
        self.weights_file = d.get("weights_file")
        order, previous = [], -1
        while len(order) != len(bones) and previous != len(order):
            previous = len(order)
            for name, b in bones.items():
                if name not in order and (not b.get("parent") or b["parent"] in order):
                    order.append(name)
        self.order = order
        self.bones = {}
        for name in order:
            b = bones[name]
            plane = b.get("rotation_plane", 0)
            if plane == [None, None, None]:
                plane = 0
            self.bones[name] = {
                "parent": b.get("parent") or None,
                "head": b["head"],
                "tail": b["tail"],
                "plane": plane,
                "reference": list(b.get("reference") or []),
                "weights_reference": b.get("weights_reference"),
            }
        self.children = {n: [c for c in order if self.bones[c]["parent"] == n] for n in order}

    def joint(self, name, coords):
        if name not in self.joints:
            raise ConvertError(f"{os.path.basename(self.path)}: joint {name!r} has no vertices")
        return coords[self.joints[name]].mean(axis=0)

    def plane_normal(self, plane, coords):
        if plane not in self.planes or len(self.planes[plane]) != 3:
            return np.array([0.0, 1.0, 0.0])
        p1, p2, p3 = (self.joint(j, coords) for j in self.planes[plane])
        return normalize(np.cross(normalize(p3 - p2), normalize(p2 - p1)))

    def own_normal(self, bone, coords):
        plane = self.bones[bone]["plane"]
        up = np.array([0.0, 1.0, 0.0])
        if isinstance(plane, list):
            normals = [self.plane_normal(p, coords) for p in plane]
            normals = [n for n in normals if not np.allclose(n, 0, atol=1e-5)]
            total = sum(normals) if normals else np.zeros(3)
            return total / len(normals) if normals and not np.allclose(total, 0, atol=1e-5) else up
        if isinstance(plane, str):
            n = self.plane_normal(plane, coords)
            return up if np.allclose(n, 0, atol=1e-5) else n
        return up

    def rest(self, coords, normals):
        """Global rest matrices by bone, given each bone's normal."""
        out = {}
        for name in self.order:
            b = self.bones[name]
            head, tail = self.joint(b["head"], coords), self.joint(b["tail"], coords)
            if np.linalg.norm(tail - head) < 1e-9:
                raise ConvertError(f"bone {name} has no length")
            out[name] = bone_matrix(head, tail, normals[name])
        return out


def copied_normals(rig, reference, reference_rest):
    """shared/skeleton.py copy_normal: each bone's X axis from its
    reference bones in the default skeleton."""
    out = {}
    for name in rig.order:
        refs = rig.bones[name]["reference"]
        axes = [reference_rest[r][:3, 0] for r in refs if r in reference_rest]
        if refs:
            axes = [a for a in axes if not np.allclose(a, 0, atol=1e-5)]
            total = sum(axes) if axes else np.zeros(3)
            ok = axes and not np.allclose(total, 0, atol=1e-5)
            out[name] = total / len(axes) if ok else np.array([0.0, 1.0, 0.0])
        elif name in reference_rest:
            out[name] = reference_rest[name][:3, 0]
        else:
            out[name] = np.array([0.0, 1.0, 0.0])
    return out


def weight_references(rig, reference):
    """shared/skeleton.py autoBuildWeightReferences: a bone without explicit
    weight references takes its pose references, plus every chain of the
    default skeleton below them that no bone of this rig references."""
    included, refs = {}, {}
    for name in rig.order:
        explicit = rig.bones[name]["weights_reference"]
        included[name] = explicit is None
        refs[name] = list(rig.bones[name]["reference"] if explicit is None else explicit)

    def reverse_map():
        out = {}
        for name in rig.order:
            for r in refs[name]:
                out.setdefault(r, []).append(name)
        return out

    def unreferenced_tail(bone, reverse, first=False):
        if not first and bone in reverse and reverse:
            return False
        kids = reference.children.get(bone, [])
        return any(unreferenced_tail(k, reverse) for k in kids) if kids else True

    def descendants(bone):
        out = []
        for k in reference.children.get(bone, []):
            out.append(k)
            out.extend(descendants(k))
        return out

    reverse = reverse_map()
    for rbone in reversed(reference.order):
        if rbone not in reverse:
            continue
        if unreferenced_tail(rbone, reverse, True):
            extra = [b for b in descendants(rbone) if unreferenced_tail(b, reverse)]
            for name in reverse[rbone]:
                if included[name]:
                    refs[name] = sorted(set(refs[name]) | set(extra))
            reverse = reverse_map()
    return refs


def read_weights(path, vertex_count):
    """A .mhw file: {bone: [(vertex, weight), ...]}."""
    d = read_json(path).get("weights")
    if not isinstance(d, dict):
        raise ConvertError(f"{os.path.basename(path)}: no weights")
    out = {}
    for bone, pairs in d.items():
        a = np.asarray(pairs if pairs else np.zeros((0, 2)), dtype=np.float64).reshape(-1, 2)
        if not np.all(np.isfinite(a)):
            raise ConvertError(f"{os.path.basename(path)}: non-finite weight")
        out[bone] = (checked_indices(a[:, 0].astype(np.int64), vertex_count, f"weights of {bone}"), a[:, 1])
    return out


def remap_weights(rig, refs, source, vertex_count):
    """skeleton.getVertexWeights + VertexBoneWeights.create: per-vertex
    weights on the rig's bones (a dense vertices x bones array), from
    weights on the default skeleton's bones."""
    raw = np.zeros((vertex_count, len(rig.order)))
    for j, name in enumerate(rig.order):
        names = refs[name] if refs[name] else [name]
        for r in names:
            if r in source:
                v, w = source[r]
                np.add.at(raw[:, j], v, w)
    return finish_weights(raw)


def finish_weights(raw):
    """Normalized per vertex, small weights dropped, unweighted vertices on
    the root bone (column 0)."""
    total = raw.sum(axis=1)
    out = np.divide(raw, total[:, None], out=np.zeros_like(raw), where=total[:, None] > 0)
    out[out <= WEIGHT_THRESHOLD] = 0.0
    out[total == 0, 0] = 1.0
    return out, int((total == 0).sum())


# --- proxies ------------------------------------------------------------------


class Proxy:
    """A compiled proxy (.mhpxy) and its mesh (.npz)."""

    def __init__(self, path, kind, base_count):
        self.path = path
        self.kind = kind
        z = Npz(path)
        self.name = z.text("name")
        self.uuid = z.text("uuid").lower()
        self.z_depth = int(z.scalar("z_depth", 50))
        self.tags = set()
        if "tags_str" in z.files and "tags_idx" in z.files:
            raw = z.array("tags_str", "S").tobytes()
            cuts = [int(i) for i in z.array("tags_idx", "iu", ndim=1)] + [len(raw)]
            self.tags = {raw[a:b].decode("utf-8", "replace").lower() for a, b in zip(cuts, cuts[1:]) if 0 <= a <= b <= len(raw)}
        self.material_file = z.text("material_file", "") or None
        self.weights_file = z.text("vertexBoneWeights_file", "") or None
        n = int(z.scalar("num_refverts", 3))
        if n == 3:
            self.refs = z.array("ref_vIdxs", "iu", ndim=2, width=3)
            self.weights = z.array("weights", "f", ndim=2, width=3).astype(np.float64)
            self.offsets = z.array("offsets", "f", ndim=2, width=3).astype(np.float64)
        elif n == 1:
            r = z.array("ref_vIdxs", "iu", ndim=1)
            self.refs = np.zeros((len(r), 3), np.int64)
            self.refs[:, 0] = r
            self.weights = np.zeros((len(r), 3))
            self.weights[:, 0] = z.array("weights", "f", ndim=1)
            self.offsets = np.zeros((len(r), 3))
        else:
            raise ConvertError(f"{self.name}: {n} reference vertices per vertex")
        self.refs = checked_indices(self.refs, base_count, f"{self.name} references")
        if not (len(self.refs) == len(self.weights) == len(self.offsets)):
            raise ConvertError(f"{self.name}: reference arrays differ in length")
        self.scale = None
        if "tmat_scale" in z.files:
            scales = z.array("tmat_scale", "f", ndim=1)
            idx = checked_indices(z.array("tmat_scale_idx", "iu", ndim=1), base_count, f"{self.name} scale")
            if len(scales) >= 3 and len(idx) >= 6:
                self.scale = [
                    None if math.isnan(scales[i]) or scales[i] == 0 else (idx[2 * i], idx[2 * i + 1], float(scales[i]))
                    for i in range(3)
                ]
        if any(k in z.files for k in ("tmat_shear", "tmat_lshear", "tmat_rshear")) and self.scale is None:
            raise ConvertError(f"{self.name}: shear fitting is not supported")
        self.delete = None
        if "deleteVerts" in z.files:
            d = z.array("deleteVerts", "b", ndim=1)
            if len(d) != base_count:
                raise ConvertError(f"{self.name}: deleteVerts does not match the base mesh")
            self.delete = d.astype(bool)
        mesh = Npz(os.path.splitext(path)[0] + ".npz")
        coord = mesh.array("coord", "f", ndim=2, width=3)
        if len(coord) != len(self.refs):
            raise ConvertError(f"{self.name}: mesh and references differ in vertex count")
        self.vertex_count = len(coord)
        self.texco = mesh.array("texco", "f", ndim=2, width=2).astype(np.float64)
        self.fvert = checked_indices(mesh.array("fvert", "iu", ndim=2, width=4), self.vertex_count, f"{self.name} faces")
        self.fuvs = checked_indices(mesh.array("fuvs", "iu", ndim=2, width=4), len(self.texco), f"{self.name} uvs")
        if len(self.fuvs) != len(self.fvert):
            raise ConvertError(f"{self.name}: faces and face UVs differ in count")

    def fit(self, coords):
        """shared/proxy.py getCoords: the proxy's vertices on the morphed
        base mesh."""
        m = np.identity(3)
        if self.scale:
            for axis, entry in enumerate(self.scale):
                if entry is not None:
                    a, b, den = entry
                    m[axis, axis] = abs(coords[a][axis] - coords[b][axis]) / den
        out = self.offsets @ m.T
        for k in range(3):
            out += coords[self.refs[:, k]] * self.weights[:, k, None]
        return out

    def vertex_mask(self, base_mask):
        """proxy.transferVertexMaskToProxy: which of its vertices show."""
        exact = ~np.any(self.weights[:, 1:] != 0, axis=1)
        shown = base_mask[self.refs].astype(int).sum(axis=1) >= 2
        return np.where(exact, base_mask[self.refs[:, 0]], shown)

    def triangles(self, vertex_shown=None):
        """(vertex, uv) index triangles of its faces: a quad is split along
        its first diagonal, a triangle is stored with its first vertex
        repeated. A face hides when all its vertices are hidden."""
        quad = self.fvert[:, 3] != self.fvert[:, 0]
        keep = np.ones(len(self.fvert), bool)
        if vertex_shown is not None:
            keep = vertex_shown[self.fvert].any(axis=1)
        tris = []
        for f, t in zip(self.fvert[keep], self.fuvs[keep]):
            tris.append(((f[0], t[0]), (f[1], t[1]), (f[2], t[2])))
            if f[3] != f[0]:
                tris.append(((f[0], t[0]), (f[2], t[2]), (f[3], t[3])))
        return tris, int(keep.sum()), int(quad.sum())

    def bone_weights(self, human):
        """proxy.getVertexWeights: the human's weights carried through the
        proxy's reference vertices (terms of 1e-4 or less dropped)."""
        raw = np.zeros((self.vertex_count, human.shape[1]))
        for k in range(3):
            term = self.weights[:, k, None] * human[self.refs[:, k]]
            term[term <= WEIGHT_THRESHOLD] = 0.0
            raw += term
        return finish_weights(raw)


def proxy_index(data):
    """Every proxy file of a kind folder, by UUID."""
    found = {}
    for kind, folder in PROXY_KINDS.items():
        root = os.path.join(data, folder)
        for dirpath, dirnames, files in os.walk(root):
            dirnames.sort()
            for f in sorted(files):
                if f.endswith(".mhpxy"):
                    path = os.path.join(dirpath, f)
                    try:
                        uuid = Npz(path).text("uuid").lower()
                    except ConvertError:
                        continue
                    found.setdefault((kind, uuid), path)
    return found


# --- materials ----------------------------------------------------------------


class Material:
    def __init__(self, name):
        self.name = name
        self.color = [1.0, 1.0, 1.0, 1.0]
        self.diffuse = None
        self.normal = None
        self.transparent = False
        self.double_sided = False
        self.roughness = 0.8


def read_material(path, files):
    m = Material(os.path.splitext(os.path.basename(path))[0])
    folder = os.path.dirname(path)
    for line in read_text(path).splitlines():
        words = line.split()
        if not words or words[0].startswith("#"):
            continue
        key, rest = words[0], words[1:]
        if key == "name" and rest:
            m.name = " ".join(rest)
        elif key == "diffuseColor" and len(rest) >= 3:
            m.color[:3] = [min(max(number(x, path), 0.0), 1.0) for x in rest[:3]]
        elif key == "opacity" and rest:
            m.color[3] = min(max(number(rest[0], path), 0.0), 1.0)
        elif key == "diffuseTexture" and rest:
            m.diffuse = files.find(" ".join(rest), [folder])
        elif key == "normalmapTexture" and rest:
            m.normal = files.find(" ".join(rest), [folder])
        elif key == "transparent" and rest:
            m.transparent = rest[0].lower() == "true"
        elif key == "backfaceCull" and rest:
            m.double_sided = rest[0].lower() == "false"
        elif key == "shininess" and rest:
            # Shininess 0..1; the hair's 0.96 would read as wet plastic,
            # so roughness keeps to 0.35..0.95.
            m.roughness = min(max(1.0 - number(rest[0], path), 0.35), 0.95)
    return m


# --- PNG ----------------------------------------------------------------------


def png_decode(path):
    """An 8-bit PNG (grey, RGB, palette, grey+alpha, RGBA; not interlaced)
    as an (h, w, channels) uint8 array."""
    data = read_bytes(path, 64 * 1024 * 1024)
    if data[:8] != b"\x89PNG\r\n\x1a\n":
        raise ConvertError(f"{os.path.basename(path)} is not a PNG")
    pos, idat, header, palette = 8, [], None, None
    while pos + 12 <= len(data):
        (length,) = struct.unpack(">I", data[pos : pos + 4])
        kind = data[pos + 4 : pos + 8]
        body = data[pos + 8 : pos + 8 + length]
        if len(body) != length:
            raise ConvertError(f"{os.path.basename(path)}: truncated chunk")
        pos += 12 + length
        if kind == b"IHDR" and length == 13:
            header = struct.unpack(">IIBBBBB", body)
        elif kind == b"PLTE":
            palette = np.frombuffer(body, np.uint8).reshape(-1, 3)
        elif kind == b"IDAT":
            idat.append(body)
        elif kind == b"IEND":
            break
    if header is None:
        raise ConvertError(f"{os.path.basename(path)} has no header")
    w, h, depth, colour, _, _, interlace = header
    channels = {0: 1, 2: 3, 3: 1, 4: 2, 6: 4}.get(colour)
    if depth not in (8, 16) or channels is None or interlace != 0 or not 0 < w * h <= MAX_IMAGE_PIXELS:
        raise ConvertError(f"{os.path.basename(path)}: unsupported PNG (8- or 16-bit, non-interlaced only)")
    if depth == 16 and colour == 3:
        raise ConvertError(f"{os.path.basename(path)}: a 16-bit palette is not a PNG")
    size = depth // 8
    stride = w * channels * size
    expected = (stride + 1) * h
    raw = zlib.decompressobj().decompress(b"".join(idat), expected + 1)
    if len(raw) != expected:
        raise ConvertError(f"{os.path.basename(path)}: truncated image data")
    img = unfilter(np.frombuffer(raw, np.uint8).reshape(h, stride + 1), h, w, channels * size)
    if size == 2:
        # Filters work on bytes, a pixel's bpp bytes apart; keep the high
        # byte of each 16-bit sample.
        img = np.ascontiguousarray(img[:, :, 0::2])
    if colour == 3:
        if palette is None or img.max() >= len(palette):
            raise ConvertError(f"{os.path.basename(path)}: bad palette")
        img = palette[img[:, :, 0]]
    return img


def unfilter(rows, h, w, bpp):
    """Undoes PNG's per-row filters. Each byte depends on the bytes left of,
    above and above-left of it, so the pixels are rebuilt along
    anti-diagonals, each one a vectorized step."""
    kind = rows[:, 0].astype(np.int16)
    if kind.max(initial=0) > 4:
        raise ConvertError("bad PNG filter type")
    data = rows[:, 1:].reshape(h, w, bpp).astype(np.int16)
    if not np.any(kind >= 3):
        # None, Sub and Up only: rows in order, Sub as a running sum.
        out = np.zeros((h, w, bpp), np.int16)
        prev = np.zeros((w, bpp), np.int16)
        for y in range(h):
            line = data[y]
            if kind[y] == 1:
                line = np.cumsum(line, axis=0)
            elif kind[y] == 2:
                line = line + prev
            prev = line & 255
            out[y] = prev
        return out.astype(np.uint8)
    out = np.zeros((h + 1, w + 1, bpp), np.int16)
    for d in range(h + w - 1):
        ys = np.arange(max(0, d - w + 1), min(h - 1, d) + 1)
        xs = d - ys
        a = out[ys + 1, xs]
        b = out[ys, xs + 1]
        c = out[ys, xs]
        t = kind[ys][:, None]
        p = a + b - c
        pa, pb, pc = np.abs(p - a), np.abs(p - b), np.abs(p - c)
        paeth = np.where((pa <= pb) & (pa <= pc), a, np.where(pb <= pc, b, c))
        pred = np.select([t == 1, t == 2, t == 3, t == 4], [a, b, (a + b) >> 1, paeth], 0)
        out[ys + 1, xs + 1] = (data[ys, xs] + pred) & 255
    return out[1:, 1:].astype(np.uint8)


def png_encode(img):
    """PNG bytes of an (h, w, channels) uint8 array, each row with the
    filter whose output has the smallest sum (deterministic)."""
    h, w, c = img.shape
    x = img.reshape(h, w * c).astype(np.int16)
    up = np.vstack([np.zeros((1, w * c), np.int16), x[:-1]])
    left = np.hstack([np.zeros((h, c), np.int16), x[:, :-c]])
    upleft = np.hstack([np.zeros((h, c), np.int16), up[:, :-c]])
    p = left + up - upleft
    pa, pb, pc = np.abs(p - left), np.abs(p - up), np.abs(p - upleft)
    paeth = np.where((pa <= pb) & (pa <= pc), left, np.where(pb <= pc, up, upleft))
    options = np.stack([x, x - left, x - up, x - ((left + up) >> 1), x - paeth]) & 255
    signed = np.where(options > 127, 256 - options, options).sum(axis=2)
    choice = signed.argmin(axis=0)
    rows = options[choice, np.arange(h)].astype(np.uint8)
    raw = np.hstack([choice.astype(np.uint8)[:, None], rows]).tobytes()

    def chunk(kind, body):
        return struct.pack(">I", len(body)) + kind + body + struct.pack(">I", zlib.crc32(kind + body) & 0xFFFFFFFF)

    colour = {1: 0, 2: 4, 3: 2, 4: 6}[c]
    return (
        b"\x89PNG\r\n\x1a\n"
        + chunk(b"IHDR", struct.pack(">IIBBBBB", w, h, 8, colour, 0, 0, 0))
        + chunk(b"IDAT", zlib.compress(raw, 9))
        + chunk(b"IEND", b"")
    )


def downscale(img, limit, normal_map=False):
    """Box-filters img by a whole factor until no side exceeds limit. Colour
    under transparent texels does not bleed in (alpha-weighted), and a
    normal map's averaged normals are renormalized."""
    h, w, c = img.shape
    k = 1
    while max(h, w) / k > limit:
        k += 1
    if k == 1:
        return img
    ph, pw = -h % k, -w % k
    x = np.pad(img, ((0, ph), (0, pw), (0, 0)), mode="edge").astype(np.float64)
    x = x.reshape((h + ph) // k, k, (w + pw) // k, k, c)
    if c in (2, 4):
        alpha = x[..., -1:]
        weight = alpha.sum(axis=(1, 3))
        colour = (x[..., :-1] * alpha).sum(axis=(1, 3))
        plain = x[..., :-1].mean(axis=(1, 3))
        colour = np.where(weight > 0, colour / np.maximum(weight, 1e-9), plain)
        out = np.concatenate([colour, x[..., -1:].mean(axis=(1, 3))], axis=-1)
    else:
        out = x.mean(axis=(1, 3))
    if normal_map and c >= 3:
        n = out[..., :3] / 127.5 - 1.0
        n /= np.maximum(np.linalg.norm(n, axis=-1, keepdims=True), 1e-9)
        out[..., :3] = (n + 1.0) * 127.5
    return np.clip(np.rint(out), 0, 255).astype(np.uint8)


def opaque(img):
    """Drops an alpha channel that is fully opaque."""
    if img.shape[2] in (2, 4) and img[..., -1].min() == 255:
        return img[..., :-1]
    return img


# --- conversion -----------------------------------------------------------------


class Piece:
    """One output primitive: a proxy's visible triangles."""

    def __init__(self, label, proxy, material, coords, triangles, weights):
        self.label = label
        self.proxy = proxy
        self.material = material
        self.coords = coords
        self.triangles = triangles
        self.weights = weights


def convert(args):
    data = os.path.realpath(os.path.expanduser(args.data or os.environ.get("MAKEHUMAN_DATA") or DEFAULT_DATA))
    if not os.path.isfile(os.path.join(data, "targets.npz")):
        raise ConvertError(f"{data} is not a MakeHuman data folder (no targets.npz)")
    mhm = os.path.realpath(args.input)
    character = read_mhm(mhm)
    files = Files(data, os.path.dirname(mhm))
    report = []

    base = Npz(os.path.join(data, "3dobjs", "base.npz")).array("coord", "f", ndim=2, width=3)
    targets = Targets(data)
    details = target_weights(character, targets, data)
    coords = morph(base, details, targets)
    report.append(f"  targets applied: {sum(1 for w in details.values() if w)}")

    # Skeleton, bone orientation and the human's weights.
    if not character.skeleton:
        raise ConvertError("the .mhm names no skeleton (a 'skeleton' line, e.g. game_engine.mhskel)")
    reference = Rig(os.path.join(data, "rigs", "default.mhskel"), len(base))
    reference_rest = reference.rest(coords, {b: reference.own_normal(b, coords) for b in reference.order})
    rig = Rig(files.find(character.skeleton, [os.path.join(data, "rigs")]), len(base))
    rest = rig.rest(coords, copied_normals(rig, reference, reference_rest))
    refs = weight_references(rig, reference)
    weights_path = os.path.join(data, "rigs", reference.weights_file or "default_weights.mhw")
    default_weights = read_weights(weights_path, len(base))
    human, _ = remap_weights(rig, refs, default_weights, len(base))

    # Proxies.
    index = proxy_index(data)
    proxies = []
    for kind, name, uuid in character.proxies:
        path = index.get((kind, uuid))
        if path is None:
            raise ConvertError(f"{kind} {name!r} ({uuid}) not found in {PROXY_KINDS[kind]}/")
        proxies.append(Proxy(path, kind, len(base)))
    bodies = [p for p in proxies if p.kind == "proxy"]
    if len(bodies) != 1:
        raise ConvertError("the .mhm needs exactly one body proxy (a 'proxy' line): the base mesh is too dense for a game")
    body = bodies[0]

    # Face hiding: outermost clothes first (clothes_chooser.updateFaceMasks).
    shown = {}
    mask = np.ones(len(base), bool)
    clothes = sorted((p for p in proxies if p.kind == "clothes"), key=lambda p: (p.z_depth, p.uuid))
    if character.hide_faces:
        for p in reversed(clothes):
            shown[p.uuid] = p.vertex_mask(mask)
            if p.delete is not None:
                mask[p.delete] = False
        shown[body.uuid] = body.vertex_mask(mask)

    # Hats: hair above a hat's brim is under its crown, and would poke
    # through it (MakeHuman leaves that to the user). Not in MakeHuman.
    hats = [p for p in clothes if "hats" in p.tags]
    if hats and not args.keep_hair_under_hats:
        brim = min(np.percentile(h.fit(coords)[:, 1], 10) for h in hats)
        for p in proxies:
            if p.kind == "hair":
                low = p.fit(coords)[:, 1] < brim
                shown[p.uuid] = low if p.uuid not in shown else shown[p.uuid] & low
                report.append(f"  {p.name}: {int((~low).sum())} vertices under the hat left out")

    pieces = []
    order = [body] + [p for p in proxies if p.kind in ("eyes", "eyebrows", "eyelashes", "hair")] + clothes
    for p in order:
        if p is body:
            if not character.skin:
                raise ConvertError("the .mhm names no skinMaterial")
            material = read_material(files.find(character.skin, [os.path.join(data, "skins")]), files)
        else:
            path = character.materials.get(p.uuid) or p.material_file
            if not path:
                raise ConvertError(f"{p.name} has no material")
            material = read_material(files.find(path, [os.path.dirname(mhm), os.path.dirname(p.path)]), files)
        tris, faces, _ = p.triangles(shown.get(p.uuid))
        if not tris:
            report.append(f"  {p.name}: every face hidden")
            continue
        if p.weights_file:
            custom = read_weights(files.find(p.weights_file, [os.path.dirname(p.path)]), p.vertex_count)
            w, unweighted = remap_weights(rig, refs, custom, p.vertex_count)
        else:
            w, unweighted = p.bone_weights(human)
        if unweighted:
            report.append(f"  {p.name}: {unweighted} vertices without weights, on the root bone")
        pieces.append(Piece(p.kind if p is body else p.name, p, material, p.fit(coords), tris, w))

    # Meters, the lowest point on the ground.
    used = np.concatenate([pc.coords[np.unique([v for t in pc.triangles for v, _ in t])] for pc in pieces])
    floor = used[:, 1].min()
    offset = np.array([0.0, -floor, 0.0])
    for pc in pieces:
        pc.coords = (pc.coords + offset) * METERS
    for name in rest:
        rest[name] = rest[name].copy()
        rest[name][:3, 3] = (rest[name][:3, 3] + offset) * METERS
    return write(args, character, rig, rest, pieces, report)


def smooth_normals(coords, triangles):
    n = np.zeros_like(coords)
    t = np.array([[v for v, _ in tri] for tri in triangles])
    face = np.cross(coords[t[:, 1]] - coords[t[:, 0]], coords[t[:, 2]] - coords[t[:, 0]])
    for k in range(3):
        np.add.at(n, t[:, k], face)
    length = np.linalg.norm(n, axis=1, keepdims=True)
    return np.divide(n, length, out=np.tile([0.0, 1.0, 0.0], (len(n), 1)), where=length > 1e-12)


def quaternion(m):
    """A rotation matrix (orthonormal 3x3) as a unit quaternion x, y, z, w."""
    t = np.trace(m)
    if t > 0:
        s = math.sqrt(t + 1.0) * 2
        q = [(m[2, 1] - m[1, 2]) / s, (m[0, 2] - m[2, 0]) / s, (m[1, 0] - m[0, 1]) / s, 0.25 * s]
    elif m[0, 0] > m[1, 1] and m[0, 0] > m[2, 2]:
        s = math.sqrt(1.0 + m[0, 0] - m[1, 1] - m[2, 2]) * 2
        q = [0.25 * s, (m[0, 1] + m[1, 0]) / s, (m[0, 2] + m[2, 0]) / s, (m[2, 1] - m[1, 2]) / s]
    elif m[1, 1] > m[2, 2]:
        s = math.sqrt(1.0 + m[1, 1] - m[0, 0] - m[2, 2]) * 2
        q = [(m[0, 1] + m[1, 0]) / s, 0.25 * s, (m[1, 2] + m[2, 1]) / s, (m[0, 2] - m[2, 0]) / s]
    else:
        s = math.sqrt(1.0 + m[2, 2] - m[0, 0] - m[1, 1]) * 2
        q = [(m[0, 2] + m[2, 0]) / s, (m[1, 2] + m[2, 1]) / s, 0.25 * s, (m[1, 0] - m[0, 1]) / s]
    q = np.array(q) / np.linalg.norm(q)
    if q[3] < 0:
        q = -q
    return [round(float(c), 7) for c in q]


def write(args, character, rig, rest, pieces, report):
    out_dir = os.path.realpath(args.out)
    os.makedirs(os.path.join(out_dir, "textures"), exist_ok=True)
    blob = bytearray()
    gltf = {
        "asset": {"version": "2.0", "generator": "pampa-seca tools/mh2gltf.py"},
        "scene": 0,
        "scenes": [{"name": character.name, "nodes": []}],
        "nodes": [],
        "meshes": [],
        "skins": [],
        "materials": [],
        "textures": [],
        "images": [],
        "samplers": [{"magFilter": 9729, "minFilter": 9987, "wrapS": 10497, "wrapT": 10497}],
        "accessors": [],
        "bufferViews": [],
        "buffers": [],
    }

    def view(raw, target=None):
        while len(blob) % 4:
            blob.append(0)
        v = {"buffer": 0, "byteOffset": len(blob), "byteLength": len(raw)}
        if target:
            v["target"] = target
        gltf["bufferViews"].append(v)
        blob.extend(raw)
        return len(gltf["bufferViews"]) - 1

    def accessor(array, component, kind, target=34962, bounds=False):
        entry = {
            "bufferView": view(array.tobytes(), target),
            "componentType": component,
            "count": int(array.shape[0]),
            "type": kind,
        }
        if bounds:
            entry["min"] = [f32(float(v)) for v in array.min(axis=0)]
            entry["max"] = [f32(float(v)) for v in array.max(axis=0)]
        gltf["accessors"].append(entry)
        return len(gltf["accessors"]) - 1

    # Joints: node 0 is the mesh, then the bones in breadth-first order.
    joint_node = {name: i + 1 for i, name in enumerate(rig.order)}
    gltf["nodes"].append({"name": character.name, "mesh": 0, "skin": 0})
    for name in rig.order:
        parent = rig.bones[name]["parent"]
        local = rest[name] if parent is None else np.linalg.inv(rest[parent]) @ rest[name]
        node = {"name": name}
        t = [round(float(c), 6) for c in local[:3, 3]]
        if any(t):
            node["translation"] = t
        q = quaternion(local[:3, :3])
        if q != [0.0, 0.0, 0.0, 1.0]:
            node["rotation"] = q
        kids = [joint_node[c] for c in rig.children[name]]
        if kids:
            node["children"] = kids
        gltf["nodes"].append(node)
    roots = [joint_node[n] for n in rig.order if rig.bones[n]["parent"] is None]
    gltf["scenes"][0]["nodes"] = [0] + roots
    # Inverse bind matrices from the nodes' own (rounded) transforms, so the
    # rest pose binds exactly.
    world = {}
    for name in rig.order:
        node = gltf["nodes"][joint_node[name]]
        m = np.identity(4)
        m[:3, :3] = rotation_matrix(node.get("rotation", [0, 0, 0, 1]))
        m[:3, 3] = node.get("translation", [0, 0, 0])
        parent = rig.bones[name]["parent"]
        world[name] = m if parent is None else world[parent] @ m
    inverse = np.stack([np.linalg.inv(world[n]).T for n in rig.order]).astype(np.float32)
    gltf["skins"].append({
        "name": rig.name or "skeleton",
        "joints": [joint_node[n] for n in rig.order],
        "skeleton": roots[0],
        "inverseBindMatrices": accessor(inverse.reshape(len(rig.order), 16), 5126, "MAT4", target=None),
    })

    images, prims, totals, texture_notes = {}, [], [], []

    def texture(path, normal_map, limit):
        key = (path, normal_map)
        if key in images:
            return images[key]
        stem = os.path.splitext(os.path.basename(path))[0]
        name = re.sub(r"[^A-Za-z0-9._-]", "_", stem) + ".png"
        taken = {img["uri"] for img in gltf["images"]}
        n = 2
        while "textures/" + name in taken:
            name = f"{stem}_{n}.png"
            n += 1
        img = png_decode(path)
        before = img.shape
        img = opaque(downscale(img, limit, normal_map))
        raw = png_encode(img)
        with open(os.path.join(out_dir, "textures", name), "wb") as f:
            f.write(raw)
        texture_notes.append(
            f"  texture textures/{name}: {img.shape[1]}x{img.shape[0]} from {before[1]}x{before[0]}, {len(raw)} bytes"
        )
        gltf["images"].append({"uri": "textures/" + name, "mimeType": "image/png"})
        gltf["textures"].append({"sampler": 0, "source": len(gltf["images"]) - 1})
        images[key] = len(gltf["textures"]) - 1
        return images[key]

    for pc in pieces:
        m = pc.material
        # The skin and the garments that cover the body get the large
        # textures; hats, shoes, hair and the face's bits the small ones.
        main = pc.label == "proxy" or pc.proxy.kind == "clothes" and pc.proxy.z_depth >= 50 and pc.proxy.delete is not None
        limit = args.max_texture if main else args.max_small_texture
        pbr = {"baseColorFactor": [round(c, 4) for c in m.color], "metallicFactor": 0.0, "roughnessFactor": round(m.roughness, 3)}
        if m.diffuse:
            pbr["baseColorTexture"] = {"index": texture(m.diffuse, False, limit)}
        entry = {"name": m.name, "pbrMetallicRoughness": pbr}
        if m.normal and not args.no_normal_maps:
            entry["normalTexture"] = {"index": texture(m.normal, True, args.max_small_texture)}
        if m.transparent:
            entry["alphaMode"] = "MASK"
            entry["alphaCutoff"] = 0.5
        if m.double_sided or m.transparent:
            entry["doubleSided"] = True
        gltf["materials"].append(entry)

        # Split vertices by (vertex, uv); MakeHuman's V runs up.
        normals = smooth_normals(pc.coords, pc.triangles)
        index, corners, indices = {}, [], []
        for tri in pc.triangles:
            for corner in tri:
                if corner not in index:
                    index[corner] = len(corners)
                    corners.append(corner)
                indices.append(index[corner])
        v = np.array([c[0] for c in corners])
        t = np.array([c[1] for c in corners])
        positions = pc.coords[v].astype(np.float32)
        uvs = pc.proxy.texco[t].copy()
        uvs[:, 1] = 1.0 - uvs[:, 1]
        joints, weights = top_influences(pc.weights[v])
        big = len(corners) > 65535
        idx = np.array(indices, dtype=np.uint32 if big else np.uint16)
        prims.append({
            "attributes": {
                "POSITION": accessor(positions, 5126, "VEC3", bounds=True),
                "NORMAL": accessor(normals[v].astype(np.float32), 5126, "VEC3"),
                "TEXCOORD_0": accessor(uvs.astype(np.float32), 5126, "VEC2"),
                "JOINTS_0": accessor(joints, 5121 if len(rig.order) < 256 else 5123, "VEC4"),
                "WEIGHTS_0": accessor(weights, 5126, "VEC4"),
            },
            "indices": accessor(idx, 5125 if big else 5123, "SCALAR", target=34963),
            "material": len(gltf["materials"]) - 1,
        })
        totals.append((pc.label, len(pc.triangles), len(corners), positions.min(axis=0), positions.max(axis=0)))
    gltf["meshes"].append({"name": character.name, "primitives": prims})
    for key in ("textures", "images"):
        if not gltf[key]:
            del gltf[key]
    gltf["buffers"].append({"uri": args.name + ".bin", "byteLength": len(blob)})
    with open(os.path.join(out_dir, args.name + ".bin"), "wb") as f:
        f.write(blob)
    with open(os.path.join(out_dir, args.name + ".gltf"), "w", encoding="utf-8") as f:
        json.dump(gltf, f, indent=1)
        f.write("\n")

    lo = np.min([pc.coords.min(axis=0) for pc in pieces], axis=0)
    hi = np.max([pc.coords.max(axis=0) for pc in pieces], axis=0)
    print(f"wrote {os.path.join(args.out, args.name + '.gltf')} ({len(blob)} bytes of buffer)")
    for label, tris, verts, low, high in totals:
        print(f"  {label}: {tris} triangles, {verts} vertices, bounds {fmt3(low)} {fmt3(high)}")
    print(f"  total: {sum(t[1] for t in totals)} triangles, {len(rig.order)} joints")
    print("  bounds (m, Y up): min " + fmt3(lo) + "  max " + fmt3(hi))
    for line in report + texture_notes:
        print(line)
    return 0


def rotation_matrix(q):
    x, y, z, w = q
    return np.array([
        [1 - 2 * (y * y + z * z), 2 * (x * y - z * w), 2 * (x * z + y * w)],
        [2 * (x * y + z * w), 1 - 2 * (x * x + z * z), 2 * (y * z - x * w)],
        [2 * (x * z - y * w), 2 * (y * z + x * w), 1 - 2 * (x * x + y * y)],
    ])


def top_influences(w):
    """The 4 strongest joints of each vertex and their weights, summing to
    1 as float32 (ties keep the lower joint)."""
    order = np.argsort(-w, axis=1, kind="stable")[:, :MAX_INFLUENCES]
    picked = np.take_along_axis(w, order, axis=1)
    total = picked.sum(axis=1, keepdims=True)
    picked = np.where(total > 0, picked / np.where(total > 0, total, 1), [[1.0, 0, 0, 0]])
    picked = picked.astype(np.float32)
    # Float32 rounding: put the remainder on the strongest influence.
    picked[:, 0] += np.float32(1.0) - picked.sum(axis=1, dtype=np.float32)
    joints = np.where(picked > 0, order, 0)
    return joints.astype(np.uint8 if w.shape[1] < 256 else np.uint16), picked


def fmt3(v):
    return "(" + ", ".join(f"{float(c):.3f}" for c in v) + ")"


# --- checking a written file ---------------------------------------------------

IMAGE_TYPES = {".png": "image/png", ".jpg": "image/jpeg", ".jpeg": "image/jpeg"}
IMAGE_MAGIC = {"image/png": b"\x89PNG\r\n\x1a\n", "image/jpeg": b"\xff\xd8\xff"}
COMPONENTS = {5120: ("b", 1), 5121: ("B", 1), 5122: ("h", 2), 5123: ("H", 2), 5125: ("I", 4), 5126: ("f", 4)}
WIDTHS = {"SCALAR": 1, "VEC2": 2, "VEC3": 3, "VEC4": 4, "MAT4": 16}


def check(path):
    """Validates a written glTF: buffers, views, accessors, indices and
    images in range, every URI inside the model's folder, images PNG or
    JPEG; and its skin: joints are nodes, inverse bind matrices one per
    joint and invertible, every vertex's joints in range and its weights
    non-negative and summing to 1. Prints the triangles, joints and
    bounds; returns non-zero on a fault."""
    folder = os.path.dirname(os.path.realpath(path))
    with open(path, encoding="utf-8") as f:
        g = json.load(f)
    faults = []
    if g.get("asset", {}).get("version") != "2.0":
        faults.append("asset.version is not 2.0")

    def local(uri):
        target = os.path.realpath(os.path.join(folder, urllib.parse.unquote(uri)))
        if uri.startswith("data:") or os.path.isabs(uri) or not target.startswith(folder + os.sep):
            faults.append(f"uri {uri!r} is not a file inside the model's folder")
            return None
        return target

    def item(items, i, what):
        if not isinstance(i, int) or isinstance(i, bool) or not 0 <= i < len(items):
            faults.append(f"{what} index {i!r} is missing or out of range")
            return None
        return items[i]

    blobs = []
    for b in g.get("buffers", []):
        target = local(b.get("uri", ""))
        raw = open(target, "rb").read() if target and os.path.isfile(target) else b""
        if len(raw) < b.get("byteLength", 0):
            faults.append(f"buffer {b.get('uri')} is shorter than its byteLength")
        blobs.append(raw)
    views = g.get("bufferViews", [])
    for k, v in enumerate(views):
        raw = item(blobs, v.get("buffer"), f"bufferView {k} buffer")
        if raw is not None and v.get("byteOffset", 0) + v.get("byteLength", 0) > len(raw):
            faults.append(f"bufferView {k} is out of its buffer")
    accessors = g.get("accessors", [])

    def read(i, what):
        """An accessor's values as a (count, width) list of tuples, or None."""
        a = item(accessors, i, what)
        if a is None:
            return None
        comp, width = COMPONENTS.get(a.get("componentType")), WIDTHS.get(a.get("type"))
        v = item(views, a.get("bufferView"), f"{what} bufferView")
        if comp is None or width is None or v is None or item(blobs, v.get("buffer"), what) is None:
            faults.append(f"{what}: bad accessor")
            return None
        fmt, size = comp
        start = v.get("byteOffset", 0) + a.get("byteOffset", 0)
        count = a.get("count", 0)
        if a.get("byteOffset", 0) + count * size * width > v.get("byteLength", 0):
            faults.append(f"{what} is out of its view")
            return None
        flat = struct.unpack_from("<%d%s" % (count * width, fmt), blobs[v["buffer"]], start)
        return [flat[k * width : (k + 1) * width] for k in range(count)]

    nodes = g.get("nodes", [])
    skins = g.get("skins", [])
    joint_counts = []
    for s, skin in enumerate(skins):
        joints = skin.get("joints", [])
        for j in joints:
            item(nodes, j, f"skin {s} joint")
        joint_counts.append(len(joints))
        if "inverseBindMatrices" in skin:
            mats = read(skin["inverseBindMatrices"], f"skin {s} inverseBindMatrices")
            if mats is not None:
                if len(mats) != len(joints):
                    faults.append(f"skin {s}: {len(mats)} inverse bind matrices for {len(joints)} joints")
                for k, m in enumerate(mats):
                    if np is not None and abs(np.linalg.det(np.array(m).reshape(4, 4))) < 1e-9:
                        faults.append(f"skin {s}: inverse bind matrix {k} is singular")
                        break
    tris = 0
    lo, hi = [math.inf] * 3, [-math.inf] * 3
    for n in nodes:
        if "mesh" in n:
            item(g.get("meshes", []), n["mesh"], "node mesh")
        if "skin" in n:
            item(skins, n["skin"], "node skin")
    skinned = {n["mesh"]: n.get("skin") for n in nodes if "mesh" in n}
    for mi, m in enumerate(g.get("meshes", [])):
        for p in m.get("primitives", []):
            attrs = p.get("attributes", {})
            positions = read(attrs.get("POSITION"), "POSITION accessor")
            idx = read(p.get("indices"), "indices accessor")
            if positions is None or idx is None:
                continue
            flat = [i[0] for i in idx]
            if flat and max(flat) >= len(positions):
                faults.append(f"mesh {m.get('name')} indexes past its vertices")
            tris += len(flat) // 3
            for pos in positions:
                for k in range(3):
                    lo[k], hi[k] = min(lo[k], pos[k]), max(hi[k], pos[k])
            skin = skinned.get(mi)
            if skin is None or not isinstance(skin, int) or not 0 <= skin < len(skins):
                continue
            joints, weights = read(attrs.get("JOINTS_0"), "JOINTS_0"), read(attrs.get("WEIGHTS_0"), "WEIGHTS_0")
            if joints is None or weights is None or len(joints) != len(positions) or len(weights) != len(positions):
                faults.append(f"mesh {m.get('name')}: a skinned primitive lacks JOINTS_0/WEIGHTS_0 per vertex")
                continue
            count = joint_counts[skin]
            for j, w in zip(joints, weights):
                if max(j) >= count:
                    faults.append(f"mesh {m.get('name')}: a joint index is out of range")
                    break
                if min(w) < 0 or abs(sum(w) - 1.0) > 2e-3:
                    faults.append(f"mesh {m.get('name')}: weights {w} do not sum to 1")
                    break
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
                faults.append(f"image {uri} does not hold what its type says")
    print(f"{path}: {tris} triangles, {sum(joint_counts)} joints, bounds min {fmt3(lo)} max {fmt3(hi)}")
    for fault in faults:
        print("  FAULT: " + fault)
    return 1 if faults else 0


def main(argv=None):
    parser = argparse.ArgumentParser(
        description="Convert a MakeHuman character (.mhm) into a skinned glTF 2.0 model "
        "(standard library and numpy; run with python3 -I).",
    )
    parser.add_argument("input", nargs="?", help="the .mhm file")
    parser.add_argument("-o", "--out", help="output folder")
    parser.add_argument("--name", default="model", help="output base name (default: model)")
    parser.add_argument("--data", help=f"MakeHuman's data folder (default: $MAKEHUMAN_DATA, else {DEFAULT_DATA})")
    parser.add_argument("--max-texture", type=int, default=1024, help="largest side of the skin and outfit textures (default 1024)")
    parser.add_argument("--max-small-texture", type=int, default=512, help="largest side of other textures and normal maps (default 512)")
    parser.add_argument("--keep-hair-under-hats", action="store_true", help="keep the hair above a hat's brim (hidden by default)")
    parser.add_argument("--no-normal-maps", action="store_true", help="leave the normal maps out")
    parser.add_argument("--check", metavar="GLTF", help="only validate a written glTF file")
    args = parser.parse_args(argv)
    try:
        if args.check:
            return check(args.check)
        if np is None:
            parser.error("numpy is required to convert")
        if not args.input or not args.out:
            parser.error("input and --out are required")
        if not SAFE_NAME.match(args.name) or "/" in args.name:
            parser.error("--name must be a plain file name")
        if not (16 <= args.max_texture <= 4096 and 16 <= args.max_small_texture <= 4096):
            parser.error("texture limits must be 16..4096")
        return convert(args)
    except (ConvertError, ValueError, KeyError, OSError) as e:
        # Refusals, unparsable numbers or JSON (ValueError), a missing key in
        # a data file and unreadable files are the input's fault: one line.
        # Anything else is a bug in this script and keeps its traceback.
        print(f"mh2gltf: {e}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    sys.exit(main())
