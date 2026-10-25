#!/usr/bin/env python3
"""Convert a MakeHuman character (.mhm) into a skinned glTF 2.0 model.

Standard library plus numpy; run it isolated: ``python3 -I tools/mh2gltf.py``.
It needs MakeHuman 1.x's *data* folder (the compiled ``.npz`` / ``.mhpxy``
assets, rigs and targets; downloaded ``.mhclo`` / ``.obj`` assets there or
in the user data folder) but none of its code: what MakeHuman does with a
character is reproduced here from its sources (see tools/README.md), and
the GUI is never started. The data is local, but every file is read as
untrusted text or arrays: bounded sizes, ``allow_pickle=False``, every
index checked, every path kept inside the data folders or the .mhm's own
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
  shininess and specular strength. A ``material`` line may name a .mhmat in
  the .mhm's own folder, so a character can recolour a garment with
  ``diffuseColor``.

What it adds, from lines MakeHuman skips (see tools/README.md): quadric
decimation of dense garments (``decimate``), greyscale textures
(``greyscaleTexture``), the arms lowered into the rest pose, garments cut
from the body proxy and skinned like it (shirt, waistcoat, jacket,
trousers, bombacha, boots), and generated pieces (hats, kepi,
neckerchief, faja, rastra, buttons, watch chain, strap, poncho, lapels,
collar), all from ``piece`` lines, with small cloth textures made in code.
The pieces on one bone (the hats on the head, the neckerchief and collar
on the neck) are rigidly skinned to it; those that lie over several (the
poncho and strap from the shoulder to the hip, the faja and rastra round
the waist, the buttons, chain and lapels down the chest) take their skin
from the dressed body under them (``inherit``).

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
# Where MakeHuman 1.x installs downloaded assets (searched first, like
# MakeHuman's getpath does, when it exists).
DEFAULT_USER_DATA = "~/Documents/makehuman/v1py3/data"
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
        # Converter lines, not MakeHuman's (it warns and skips them):
        self.decimate = {}  # proxy uuid -> triangle target
        self.pieces = []  # (kind, {key: [numbers]}) generated pieces and garments


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
        elif key == "decimate":
            if len(values) < 2 or not UUID.match(values[-2]) or not re.fullmatch(r"\d{2,6}", values[-1]):
                raise ConvertError(f"{where}: decimate needs [name] UUID triangles")
            c.decimate[values[-2].lower()] = int(values[-1])
        elif key == "piece":
            c.pieces.append(read_piece(values, where))
        elif key in IGNORED_KEYS:
            continue
        else:
            c.ignored.append(key)
    if c.name is None:
        c.name = os.path.splitext(os.path.basename(path))[0]
    if not SAFE_NAME.match(c.name):
        raise ConvertError(f"the character name {c.name!r} is not a plain name")
    return c


def read_piece(values, where):
    """A ``piece KIND key=value ...`` line: a generated piece and its
    settings, each a number or a comma-separated list of numbers."""
    if not values or values[0] not in PIECES:
        raise ConvertError(f"{where}: piece needs a kind: {', '.join(sorted(PIECES))}")
    kind, settings = values[0], {}
    for word in values[1:]:
        key, eq, text = word.partition("=")
        if not eq or key not in PIECES[kind]:
            raise ConvertError(f"{where}: {kind} takes {', '.join(sorted(PIECES[kind]))}=value, not {word!r}")
        settings[key] = [number(x, where) for x in text.split(",")]
        if len(settings[key]) != len(PIECES[kind][key]):
            raise ConvertError(f"{where}: {kind} {key} takes {len(PIECES[kind][key])} numbers")
    return kind, settings


class Files:
    """Resolves the paths an .mhm or .mhmat names: inside the .mhm's own
    folder or a MakeHuman data folder (the user's, then the system's),
    never elsewhere."""

    def __init__(self, roots, local):
        roots = [roots] if isinstance(roots, str) else roots
        self.roots = [os.path.realpath(r) for r in roots]
        self.local = os.path.realpath(local)

    def find(self, name, folders):
        if not name or "\\" in name or "\x00" in name or os.path.isabs(name):
            raise ConvertError(f"unsafe path {name!r}")
        tries = []
        for folder in folders:
            tries.append(os.path.join(folder, name))
        for root in self.roots:
            if name.startswith("data/"):
                tries.append(os.path.join(root, name[5:]))
            tries.append(os.path.join(root, name))
        for t in tries:
            real = os.path.realpath(t)
            allowed = inside(real, self.local) or any(inside(real, r) for r in self.roots)
            if allowed and os.path.isfile(real):
                return real
        raise ConvertError(f"{name!r} not found in the .mhm's folder or the data folders")


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

    def __init__(self, path, kind, base_count, files=None):
        self.path = path
        self.kind = kind
        self.obj_file = None
        if path.endswith(".mhclo"):
            self.read_text(path, base_count)
        else:
            self.read_compiled(path, base_count)
        self.refs = checked_indices(self.refs, base_count, f"{self.name} references")
        if not (len(self.refs) == len(self.weights) == len(self.offsets)):
            raise ConvertError(f"{self.name}: reference arrays differ in length")
        # The mesh: MakeHuman's compiled .npz beside the proxy, else the
        # .obj it names (assets downloaded but never loaded by MakeHuman).
        compiled = os.path.splitext(path)[0] + ".npz"
        if os.path.isfile(compiled):
            mesh = Npz(compiled)
            coord = mesh.array("coord", "f", ndim=2, width=3)
            self.texco = mesh.array("texco", "f", ndim=2, width=2).astype(np.float64)
            fvert, fuvs = mesh.array("fvert", "iu", ndim=2, width=4), mesh.array("fuvs", "iu", ndim=2, width=4)
        else:
            if not self.obj_file:
                raise ConvertError(f"{self.name}: no mesh (.npz) and no obj_file")
            folder = os.path.dirname(path)
            obj = (files or Files([folder], folder)).find(self.obj_file, [folder])
            coord, self.texco, fvert, fuvs = read_obj(obj)
        if len(coord) != len(self.refs):
            raise ConvertError(f"{self.name}: mesh and references differ in vertex count")
        self.vertex_count = len(coord)
        self.fvert = checked_indices(fvert, self.vertex_count, f"{self.name} faces")
        self.fuvs = checked_indices(fuvs, len(self.texco), f"{self.name} uvs")
        if len(self.fuvs) != len(self.fvert):
            raise ConvertError(f"{self.name}: faces and face UVs differ in count")

    def read_text(self, path, base_count):
        """shared/proxy.py loadTextProxy: an .mhclo as MakeClothes writes it."""
        self.name = os.path.splitext(os.path.basename(path))[0]
        self.uuid, self.z_depth = "", 50
        self.material_file = self.weights_file = None
        self.scale, self.delete = None, None
        refs, status = [], None
        for number_, line in enumerate(read_text(path, MAX_JSON_BYTES).splitlines(), 1):
            words = line.split()
            if not words or words[0].startswith("#"):
                continue
            key, where = words[0], f"{os.path.basename(path)} line {number_}"
            if key == "name":
                self.name = " ".join(words[1:])
            elif key == "uuid" and len(words) > 1:
                self.uuid = words[1].lower()
            elif key == "z_depth" and len(words) > 1:
                self.z_depth = int(number(words[1], where))
            elif key == "obj_file" and len(words) > 1:
                self.obj_file = " ".join(words[1:])
            elif key == "material" and len(words) > 1:
                self.material_file = " ".join(words[1:])
            elif key == "vertexboneweights_file" and len(words) > 1:
                self.weights_file = " ".join(words[1:])
            elif key in ("x_scale", "y_scale", "z_scale"):
                if len(words) != 4:
                    raise ConvertError(f"{where}: {key} needs two vertices and a length")
                if self.scale is None:
                    self.scale = [None, None, None]
                a, b = (int(number(w, where)) for w in words[1:3])
                self.scale["xyz".index(key[0])] = tuple(checked_indices(np.array([a, b]), base_count, where).tolist()) + (number(words[3], where),)
            elif "shear" in key:
                raise ConvertError(f"{self.name}: shear fitting is not supported")
            elif key == "verts":
                status = "verts"
            elif key == "delete_verts":
                status = "delete"
                self.delete = np.zeros(base_count, bool)
            elif key == "weights":
                status = "weights"  # legacy per-proxy weights: unused
            elif status == "verts":
                if len(words) == 1:
                    refs.append((int(number(words[0], where)), 0, 0, 1.0, 0.0, 0.0, 0.0, 0.0, 0.0))
                elif len(words) in (6, 9):
                    v = [number(w, where) for w in words]
                    refs.append(tuple(v) + (0.0,) * (9 - len(v)))
                else:
                    raise ConvertError(f"{where}: a reference vertex needs 1, 6 or 9 numbers")
            elif status == "delete":
                previous, run = None, False
                for w in words:
                    if w == "-":
                        run = True
                        continue
                    v = int(number(w, where))
                    if not 0 <= v < base_count or (run and previous is None):
                        raise ConvertError(f"{where}: delete_verts index {v} is out of range")
                    if run:
                        self.delete[previous : v + 1] = True
                    else:
                        self.delete[v] = True
                    previous, run = v, False
        if not refs or not self.uuid:
            raise ConvertError(f"{os.path.basename(path)}: no uuid or no reference vertices")
        a = np.array(refs, dtype=np.float64)
        self.refs = a[:, :3].astype(np.int64)
        self.weights, self.offsets = a[:, 3:6], a[:, 6:9]

    def read_compiled(self, path, base_count):
        z = Npz(path)
        self.name = z.text("name")
        self.uuid = z.text("uuid").lower()
        self.z_depth = int(z.scalar("z_depth", 50))
        self.material_file = z.text("material_file", "") or None
        self.weights_file = z.text("vertexBoneWeights_file", "") or None
        self.obj_file = z.text("obj_file", "") or None
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


def proxy_index(roots):
    """Every proxy file of a kind folder, by UUID: the data folders in
    order, and in each a compiled .mhpxy before an .mhclo."""
    found = {}
    for data in [roots] if isinstance(roots, str) else roots:
        for kind, folder in PROXY_KINDS.items():
            for suffix, uuid_of in ((".mhpxy", compiled_uuid), (".mhclo", text_uuid)):
                for dirpath, dirnames, files in os.walk(os.path.join(data, folder)):
                    dirnames.sort()
                    for f in sorted(files):
                        if f.endswith(suffix):
                            path = os.path.realpath(os.path.join(dirpath, f))
                            try:
                                uuid = uuid_of(path)
                            except ConvertError:
                                continue
                            if uuid:
                                found.setdefault((kind, uuid), path)
    return found


def compiled_uuid(path):
    return Npz(path).text("uuid").lower()


def text_uuid(path):
    for line in read_text(path, MAX_JSON_BYTES).splitlines():
        words = line.split()
        if len(words) == 2 and words[0] == "uuid":
            return words[1].lower()
        if words and words[0] == "verts":
            break
    return None


def read_obj(path):
    """A Wavefront .obj as MakeHuman reads it (shared/wavefront.py): the
    coordinates, UVs and (vertex, uv) faces of 4 corners, a triangle with
    its first corner repeated. Larger polygons are fanned into triangles."""
    coords, uvs, fvert, fuvs = [], [], [], []
    name = os.path.basename(path)
    for number_, line in enumerate(read_text(path, MAX_NPZ_BYTES).splitlines(), 1):
        words = line.split()
        if not words:
            continue
        where = f"{name} line {number_}"
        if words[0] == "v":
            if len(words) < 4:
                raise ConvertError(f"{where}: a vertex needs 3 numbers")
            coords.append([number(w, where) for w in words[1:4]])
        elif words[0] == "vt":
            if len(words) < 3:
                raise ConvertError(f"{where}: a UV needs 2 numbers")
            uvs.append([number(w, where) for w in words[1:3]])
        elif words[0] == "f":
            corners = []
            for w in words[1:]:
                parts = w.split("/")
                v = int(number(parts[0], where))
                t = int(number(parts[1], where)) if len(parts) > 1 and parts[1] else 0
                corners.append((v - 1, t - 1 if t else 0))
            if len(corners) < 3:
                continue
            if len(corners) == 4:
                polys = [corners]
            else:
                polys = [[corners[0], corners[k], corners[k + 1], corners[0]] for k in range(1, len(corners) - 1)]
            for poly in polys:
                fvert.append([c[0] for c in poly])
                fuvs.append([c[1] for c in poly])
        if len(coords) + len(fvert) > MAX_ARRAY_ITEMS:
            raise ConvertError(f"{name} is too large")
    if not uvs:
        uvs = [[0.0, 0.0]]
        fuvs = [[0, 0, 0, 0] for _ in fvert]
    coord = np.array(coords, dtype=np.float64).reshape(-1, 3)
    fvert = checked_indices(np.array(fvert, dtype=np.int64).reshape(-1, 4), len(coord), f"{name} faces")
    fuvs = checked_indices(np.array(fuvs, dtype=np.int64).reshape(-1, 4), len(uvs), f"{name} uvs")
    return coord, np.array(uvs, dtype=np.float64), fvert, fuvs


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
        self.metallic = 0.0
        self.greyscale = False
        self.pattern = None  # a cloth() texture instead of a file


def linear(c):
    """A display (sRGB) colour channel as the linear value glTF's
    baseColorFactor holds. MakeHuman's diffuseColor and the .mhm's piece
    colours are display colours: written as they are, a dark suit (0.2)
    shows as a light grey (0.48) in a glTF viewer or the game."""
    return c / 12.92 if c <= 0.04045 else ((c + 0.055) / 1.055) ** 2.4


def phong_roughness(shininess, specular):
    """A roughness for MakeHuman's Phong shininess (0..1), weighted by how
    strong its specular colour is: cloth and felt with a faint specular
    (the fedoras' 0.04) read matte, not as polished metal. MakeHuman's
    defaults (white specular, shininess 0.2) give 0.8. Kept to 0.35..0.95."""
    gloss = shininess * min(1.0, 4.0 * specular)
    return min(max(1.0 - gloss, 0.35), 0.95)


def read_material(path, files):
    m = Material(os.path.splitext(os.path.basename(path))[0])
    folder = os.path.dirname(path)
    shininess, specular, roughness = 0.2, 1.0, None
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
            shininess = min(max(number(rest[0], path), 0.0), 1.0)
        elif key == "specularColor" and len(rest) >= 3:
            specular = max(min(max(number(x, path), 0.0), 1.0) for x in rest[:3])
        elif key == "roughness" and rest:
            roughness = min(max(number(rest[0], path), 0.0), 1.0)
        elif key == "metallic" and rest:
            m.metallic = min(max(number(rest[0], path), 0.0), 1.0)
        elif key == "greyscaleTexture" and rest:
            # Not MakeHuman's (it warns and skips it): the diffuse texture
            # as grey, stretched to white, so diffuseColor can lighten it.
            m.greyscale = rest[0].lower() == "true"
    m.roughness = roughness if roughness is not None else phong_roughness(shininess, specular)
    return m


def greyscale(img):
    """The colour channels of an RGB(A) image as their luminance, stretched
    so its brighter opaque texels (the 95th percentile) are white; alpha is
    kept."""
    c = img.shape[2]
    colour = img[..., :3].astype(np.float64) if c >= 3 else img[..., :1].astype(np.float64)
    lum = colour @ [0.299, 0.587, 0.114] if c >= 3 else colour[..., 0]
    seen = img[..., -1] > 0 if c in (2, 4) else np.ones(lum.shape, bool)
    top = np.percentile(lum[seen], 95) if seen.any() else 0.0
    lum = np.clip(np.rint(lum * (255.0 / top if top > 0 else 1.0)), 0, 255).astype(np.uint8)
    out = img.copy()
    out[..., : 3 if c >= 3 else 1] = lum[..., None]
    return out


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


# --- decimation -----------------------------------------------------------------


def decimate(coords, triangles, weights, target):
    """Quadric edge-collapse simplification (Garland and Heckbert) down to
    at most ``target`` triangles, for a garment too dense for the game.

    It runs on a fitted, skinned piece: ``triangles`` are ((vertex, uv) x 3)
    and ``weights`` a vertices x joints array. Collapses are half-edge: one
    vertex merges into a neighbour and keeps its position and UV, so no UV
    is ever interpolated. Vertices on a boundary or a UV seam (more than one
    UV) are locked: those edges stay exactly as they were. A collapse is
    refused if it would turn a triangle over or break the surface (the link
    condition). The survivor takes both vertices' weights, averaged and cut
    to the strongest MAX_INFLUENCES, renormalized. Cheapest collapse first,
    ties by vertex index: the result is deterministic. Returns compacted
    coords, triangles and weights; unused vertices are dropped."""
    import heapq

    tris = np.array([[v for v, _ in t] for t in triangles], dtype=np.int64)
    uvs = np.array([[u for _, u in t] for t in triangles], dtype=np.int64)
    pos = coords.astype(np.float64)
    w = weights.astype(np.float64).copy()
    if len(tris) > target:
        # Plane quadrics, area-weighted.
        e1, e2 = pos[tris[:, 1]] - pos[tris[:, 0]], pos[tris[:, 2]] - pos[tris[:, 0]]
        n = np.cross(e1, e2)
        area = np.linalg.norm(n, axis=1)
        unit = np.divide(n, area[:, None], out=np.zeros_like(n), where=area[:, None] > 1e-12)
        plane = np.hstack([unit, -(unit * pos[tris[:, 0]]).sum(axis=1, keepdims=True)])
        q = np.zeros((len(pos), 4, 4))
        k = plane[:, :, None] * plane[:, None, :] * (0.5 * area)[:, None, None]
        for c in range(3):
            np.add.at(q, tris[:, c], k)
        faces = [set() for _ in range(len(pos))]
        for t, (a, b, c) in enumerate(tris.tolist()):
            for v in (a, b, c):
                faces[v].add(t)
        uv_sets = [set() for _ in range(len(pos))]
        for t, row in enumerate(uvs.tolist()):
            for v, u in zip(tris[t].tolist(), row):
                uv_sets[v].add(u)
        edge_count = {}
        for a, b, c in tris.tolist():
            for e in ((a, b), (b, c), (c, a)):
                key = (min(e), max(e))
                edge_count[key] = edge_count.get(key, 0) + 1
        locked = np.zeros(len(pos), bool)
        for (a, b), count in edge_count.items():
            if count != 2:
                locked[a] = locked[b] = True
        for v, us in enumerate(uv_sets):
            if len(us) > 1:
                locked[v] = True
        alive = np.ones(len(tris), bool)
        version = np.zeros(len(pos), np.int64)

        def neighbours(v):
            return {x for t in faces[v] for x in tris[t].tolist()} - {v}

        def cost(src, dst):
            h = np.append(pos[dst], 1.0)
            return float(h @ (q[src] + q[dst]) @ h)

        heap = []

        def push(a, b):
            for src, dst in ((a, b), (b, a)):
                if not locked[src]:
                    heapq.heappush(heap, (cost(src, dst), src, dst, int(version[src]), int(version[dst])))

        for a, b in sorted(edge_count):
            push(a, b)
        count = len(tris)
        while count > target and heap:
            _, src, dst, vs, vd = heapq.heappop(heap)
            if version[src] != vs or version[dst] != vd:
                continue
            shared = faces[src] & faces[dst]
            if len(shared) != 2:
                continue
            opposite = {x for t in shared for x in tris[t].tolist()} - {src, dst}
            if neighbours(src) & neighbours(dst) != opposite:
                continue  # the link condition: the collapse would pinch the surface
            flips = False
            for t in faces[src] - shared:
                corners = tris[t].copy()
                before = np.cross(pos[corners[1]] - pos[corners[0]], pos[corners[2]] - pos[corners[0]])
                corners[corners == src] = dst
                after = np.cross(pos[corners[1]] - pos[corners[0]], pos[corners[2]] - pos[corners[0]])
                if np.dot(before, after) <= 1e-12 * max(np.dot(before, before), 1e-30) or np.linalg.norm(after) < 1e-12:
                    flips = True
                    break
            if flips:
                continue
            # src has one UV (it is not on a seam), so all its triangles lie
            # on one side of any seam, as do the two that go: dst's UV in
            # those is its UV on that side, and src's corners take it.
            first = min(shared)
            dst_uv = int(uvs[first][tris[first] == dst][0])
            for t in shared:
                alive[t] = False
                for v in tris[t].tolist():
                    faces[v].discard(t)
            count -= len(shared)
            for t in list(faces[src]):
                corner = int(np.flatnonzero(tris[t] == src)[0])
                tris[t, corner] = dst
                uvs[t, corner] = dst_uv
                faces[dst].add(t)
            faces[src] = set()
            q[dst] += q[src]
            merged = w[dst] + w[src]
            keep = np.argsort(-merged, kind="stable")[:MAX_INFLUENCES]
            w[dst] = 0.0
            w[dst, keep] = merged[keep]
            w[dst] /= w[dst].sum() if w[dst].sum() > 0 else 1.0
            locked[src] = True
            version[src] += 1
            version[dst] += 1
            for x in sorted(neighbours(dst)):
                version[x] += 1
            for x in sorted(neighbours(dst)):
                push(dst, x)
                for y in sorted(neighbours(x)):
                    push(x, y)
        tris, uvs = tris[alive], uvs[alive]
    used = np.unique(tris)
    remap = np.full(len(pos), -1, np.int64)
    remap[used] = np.arange(len(used))
    out_w = w[used]
    # Every row at most MAX_INFLUENCES, summing to 1.
    order = np.argsort(-out_w, axis=1, kind="stable")[:, MAX_INFLUENCES:]
    np.put_along_axis(out_w, order, 0.0, axis=1)
    total = out_w.sum(axis=1, keepdims=True)
    out_w = np.divide(out_w, total, out=np.zeros_like(out_w), where=total > 0)
    out_w[total[:, 0] == 0, 0] = 1.0
    out_tris = [tuple((int(remap[v]), int(u)) for v, u in zip(t, uv)) for t, uv in zip(tris.tolist(), uvs.tolist())]
    return pos[used], out_tris, out_w


class Piece:
    """One output primitive: a proxy's visible triangles, or a generated
    piece. ``triangles`` are ((vertex, uv) x 3), ``weights`` vertices x
    joints, and ``large`` asks for the large texture size."""

    def __init__(self, label, kind, material, coords, triangles, weights, texco, large=False):
        self.label = label
        self.kind = kind
        self.material = material
        self.coords = coords
        self.triangles = triangles
        self.weights = weights
        self.texco = texco
        self.large = large


# --- the arms' rest pose --------------------------------------------------------

# The upper arms hang along this direction (x away from the body, y up,
# z forward; MakeHuman's units), the forearms a little forward of it.
ARM_HANG = (0.2, -1.0, 0.05)
FOREARM_FORWARD = 0.18
ARM_CHAINS = (("upperarm_l", "lowerarm_l"), ("upperarm_r", "lowerarm_r"))


def rotation_between(a, b):
    """The rotation matrix turning unit vector a onto unit vector b."""
    a, b = normalize(a), normalize(b)
    v, c = np.cross(a, b), float(np.dot(a, b))
    if c < -1 + 1e-9:
        axis = normalize(np.cross(a, [1.0, 0.0, 0.0] if abs(a[0]) < 0.9 else [0.0, 1.0, 0.0]))
        return 2 * np.outer(axis, axis) - np.identity(3)
    k = np.array([[0, -v[2], v[1]], [v[2], 0, -v[0]], [-v[1], v[0], 0]])
    return np.identity(3) + k + k @ k / (1 + c)


def pose_arms(rig, rest, pieces):
    """Lowers the arms out of MakeHuman's rest A-pose, into the rest pose
    written: each upper arm turns about its shoulder to hang along
    ARM_HANG on its own side, and each forearm about its elbow to follow
    it, bent FOREARM_FORWARD forward. The bones below follow, and every
    vertex moves by its weights (linear blend skinning). Returns the new
    rest matrices; the pieces' coords are moved in place."""
    delta = {}
    turned = {}
    for upper, lower in ARM_CHAINS:
        if upper in rig.bones and lower in rig.bones:
            turned[upper] = None
            turned[lower] = upper
    out = {}
    for name in rig.order:
        parent = rig.bones[name]["parent"]
        d = delta[parent].copy() if parent else np.identity(4)
        if name in turned:
            head = (d @ rest[name])[:3, 3]
            along = d[:3, :3] @ rest[name][:3, 1]
            side = 1.0 if head[0] > 0 else -1.0
            hang = normalize(np.array([ARM_HANG[0] * side, ARM_HANG[1], ARM_HANG[2]]))
            if turned[name] is not None:
                hang = normalize(hang + np.array([0.0, 0.0, FOREARM_FORWARD]))
            turn = np.identity(4)
            turn[:3, :3] = rotation_between(along, hang)
            pivot, back = np.identity(4), np.identity(4)
            pivot[:3, 3], back[:3, 3] = head, -head
            d = pivot @ turn @ back @ d
        delta[name] = d
        out[name] = d @ rest[name]
    moving = [j for j, name in enumerate(rig.order) if not np.allclose(delta[name], np.identity(4))]
    for pc in pieces:
        h = np.hstack([pc.coords, np.ones((len(pc.coords), 1))])
        moved = pc.coords.copy()
        for j in moving:
            w = pc.weights[:, j]
            if np.any(w):
                moved += w[:, None] * ((h @ delta[rig.order[j]].T)[:, :3] - pc.coords)
        pc.coords = moved
    return out


# --- generated pieces -------------------------------------------------------------

# Each kind's settings and their defaults (MakeHuman's units: decimeters).
FELT = [0.30, 0.25, 0.20]
# The garments cut from the body (see dress()) share these settings:
# where they end (top, bottom: stations, see Figure), the sleeves' cuff (a
# reach, 0 for none), loose (a factor on the offsets), a V opening at the
# front (vneck: the station of its point, 0 for none; vwidth: its half
# width at the neck) and a skirt below the waist (skirt: the station of its
# hem, 0 for none; open: the front sectors left open; flare: its hem's
# extra width) and padded shoulders (pad: how far they stand out).
GARMENT_SETTINGS = {"top": [6.0], "bottom": [3.2], "cuff": [0.0], "loose": [1.0], "vneck": [0.0],
                    "vwidth": [0.75], "skirt": [0.0], "open": [0], "flare": [0.25], "pad": [0.0]}
PIECES = {
    # A felt hat with a flat crown and a flat brim, and a ribbon round the
    # band. crown: its top above the head's; taper: top width / band width;
    # brim: width; droop and back: how far the brim's front and back edge
    # drop (negative: up); sides: the same at the sides; dent: a crease
    # down the top, front to back; pinch: narrows the crown's front; round:
    # softens the crown's top edge (0 to 1); thick: the brim's thickness;
    # curl: how far its edge turns up; tilt:
    # how much lower the band sits at the back; fit: the band below the
    # head's top; band: the ribbon's colour.
    "hat": {"color": FELT, "band": [0.08, 0.07, 0.06], "crown": [0.25], "taper": [0.97], "brim": [0.75],
            "droop": [0.05], "back": [0.0], "sides": [0.0], "dent": [0.0], "pinch": [0.0], "round": [0.0],
            "thick": [0.07], "curl": [0.04], "tilt": [0.15], "fit": [1.05]},
    # A kepi: a cloth crown with a flat top a little wider than its band,
    # sloping down to the front, a leather band, a stiff visor and a chin
    # strap with a button at each end.
    "kepi": {"color": [0.07, 0.08, 0.15], "visor": [0.03, 0.03, 0.035], "button": [0.80, 0.62, 0.26],
             "crown": [0.02], "taper": [1.06], "slope": [0.3], "width": [0.6], "droop": [0.3],
             "tilt": [0.0], "fit": [0.62]},
    # A neckerchief (panuelo) knotted round the neck, a point on the chest.
    "neckerchief": {"color": [0.86, 0.84, 0.78], "drop": [0.9], "knot": [0.11]},
    # A cloth sash (faja) wound round the waist, faintly striped.
    "sash": {"color": [0.45, 0.08, 0.07], "width": [1.1], "height": [0.0]},
    # A leather belt over the sash (rastra), its front set with coins and a
    # central plate; with coins=0, a plain belt and its buckle.
    "rastra": {"color": [0.17, 0.10, 0.06], "coin": [0.80, 0.80, 0.76], "coins": [8], "height": [0.0]},
    # A column of brass buttons down the front (two with spread > 0).
    "buttons": {"color": [0.80, 0.62, 0.26], "count": [6], "spread": [0.0]},
    # A watch chain across the waistcoat, from its middle to the pockets:
    # height: from spine_01 (0) to spine_02 (1); reach: to each side; sag.
    "chain": {"color": [0.83, 0.68, 0.32], "height": [0.5], "reach": [0.55], "sag": [0.18]},
    # Garments cut from the body (see GARMENT_CUTS for their shape).
    "shirt": {**GARMENT_SETTINGS, "color": [0.88, 0.86, 0.80], "top": [6.15], "bottom": [3.2], "cuff": [2.08]},
    "waistcoat": {**GARMENT_SETTINGS, "color": [0.15, 0.13, 0.12], "top": [6.0], "bottom": [3.6], "vneck": [5.0]},
    "jacket": {**GARMENT_SETTINGS, "color": [0.15, 0.14, 0.13], "top": [6.1], "bottom": [3.9], "cuff": [2.07],
               "vneck": [4.7], "skirt": [2.8], "open": [1]},
    "trousers": {**GARMENT_SETTINGS, "color": [0.20, 0.18, 0.16], "top": [4.3], "bottom": [1.2]},
    "bombacha": {**GARMENT_SETTINGS, "color": [0.40, 0.36, 0.30], "top": [4.3], "bottom": [1.8]},
    "boots": {**GARMENT_SETTINGS, "color": [0.07, 0.06, 0.05], "top": [2.0], "bottom": [-1.0], "sole": [0.1]},
    "alpargatas": {**GARMENT_SETTINGS, "color": [0.74, 0.70, 0.60], "top": [1.1], "bottom": [-1.0], "sole": [0.15]},
    # A shawl (see shawl_cut()): over the shoulders and the upper arms (to
    # ``cuff``), to a point down the back (``point``), its two ends
    # ``width`` wide crossed over the chest (make_shawl_ends).
    "shawl": {**GARMENT_SETTINGS, "color": [0.32, 0.22, 0.18], "top": [6.05], "bottom": [3.0], "cuff": [0.85],
              "point": [3.9], "width": [1.9]},
    # A headscarf (panuelo de cabeza) framing the face, or hair worn up:
    # cut from the head's skin (see head_cut()).
    "headscarf": {**GARMENT_SETTINGS, "color": [0.70, 0.62, 0.45], "loose": [0.12], "crown": [0.08],
                  "brow": [0.3], "face": [0.5], "knot": [0.12], "drop": [1.1]},
    "hair": {**GARMENT_SETTINGS, "color": [0.16, 0.11, 0.08], "loose": [0.05], "crown": [0.05], "brow": [0.2]},
    # A coat's lapels, laid on the front either side of its opening: from
    # their point (point: from spine_02 at 0 to the neck at 1) up to the
    # collar, ``open`` apart at the top, ``width`` wide.
    "lapels": {"color": [0.15, 0.14, 0.13], "point": [0.3], "open": [0.6], "width": [0.4]},
    # A stiff band collar round the neck, ``height`` tall, with small wing
    # tips folded down at the front (none with ``wings=0``: a blouse's).
    "collar": {"color": [0.93, 0.92, 0.89], "height": [0.28], "wings": [0.12]},
    # A leather strap from one shoulder (side 1 the left, -1 the right)
    # across the chest and the back to the other hip, ``width`` wide.
    "strap": {"color": [0.07, 0.06, 0.05], "side": [-1], "width": [0.5]},
    # A poncho folded long and laid the same way over one shoulder,
    # ``width`` wide and ``thick`` deep, its ends hanging ``hang`` below the
    # hip; woven dark red with cream bands (the colour tints it).
    "poncho": {"color": [1.0, 1.0, 1.0], "side": [1], "width": [1.8], "thick": [0.16], "hang": [1.4]},
    # A boina (beret): a soft flat crown ``width`` times as wide as its
    # band, its top ``height`` above the head's, pulled down to one side
    # (``lean``, its rim dropping ``droop`` there), with a short stalk
    # (txistor) on top.
    "boina": {"color": [0.12, 0.13, 0.17], "width": [1.3], "height": [0.18], "lean": [0.25], "droop": [0.25],
              "tilt": [0.1], "fit": [0.95]},
    # Hair worn up in a bun (rodete) at the back of the head, ``size``
    # across, ``high`` up from the nape (0) to the crown (1).
    "bun": {"color": [0.16, 0.11, 0.08], "size": [0.85], "high": [0.5]},
    # A woman's ankle-length skirt (pollera), a bell from the waist (see
    # make_long_skirt): its waistband's and its hem's stations, the hem's
    # extra width, the folds' depth, the most of a thigh's turn it takes,
    # and the station from which the skin under it is left out.
    "skirt": {"color": [0.28, 0.22, 0.18], "top": [3.9], "hem": [1.15], "flare": [1.2], "folds": [0.08],
              "follow": [0.8], "kick": [0.9], "hide": [1.6]},
    # An apron over the skirt's front: down to station ``hem``, over
    # ``width`` of the front half.
    "apron": {"color": [0.80, 0.77, 0.70], "hem": [1.7], "width": [0.8]},
}
SEGMENTS = 20  # around a band, a crown or a brim
ARM_JOINTS = re.compile(r"^(upperarm|lowerarm|hand|thumb|index|middle|ring|pinky)_")


class Body:
    """What the generated pieces are fitted to: the visible vertices of
    every piece, their weights, strongest joint and piece's kind, and the
    joints' rest matrices."""

    def __init__(self, rig, rest, pieces):
        self.rig, self.rest = rig, rest
        used, faces = [], []
        for pc in pieces:
            v = np.unique([v for t in pc.triangles for v, _ in t])
            used.append((pc.coords[v], pc.weights[v].argmax(axis=1), pc.kind, pc.weights[v]))
            t = np.array([[v for v, _ in tri] for tri in pc.triangles], np.int64).reshape(-1, 3)
            faces.append((pc.coords[t], pc.weights[t[:, 0]].argmax(axis=1), pc.kind))
        self.points = np.concatenate([u[0] for u in used])
        self.joint = np.concatenate([u[1] for u in used])
        self.kind = np.concatenate([[u[2]] * len(u[0]) for u in used])
        self.weights = np.concatenate([u[3] for u in used])
        # Every triangle (corners), its first corner's strongest joint and kind.
        self.faces = np.concatenate([f[0] for f in faces])
        self.face_joint = np.concatenate([f[1] for f in faces])
        self.face_kind = np.concatenate([[f[2]] * len(f[0]) for f in faces])

    def surface(self, x, y, names, kind=None, back=False):
        """The z where a line along z through (x, y) meets the triangles of
        those joints (and kind) first from the front (from the back with
        ``back``), or None if it misses them."""
        ok = np.array([n in names for n in self.rig.order])[self.face_joint]
        if kind is not None:
            ok &= self.face_kind == kind
        t = self.faces[ok]
        a, b, c = t[:, 0], t[:, 1], t[:, 2]
        d = (b[:, 0] - a[:, 0]) * (c[:, 1] - a[:, 1]) - (c[:, 0] - a[:, 0]) * (b[:, 1] - a[:, 1])
        safe = np.where(np.abs(d) > 1e-12, d, 1.0)
        u = ((x - a[:, 0]) * (c[:, 1] - a[:, 1]) - (c[:, 0] - a[:, 0]) * (y - a[:, 1])) / safe
        v = ((b[:, 0] - a[:, 0]) * (y - a[:, 1]) - (x - a[:, 0]) * (b[:, 1] - a[:, 1])) / safe
        hit = (np.abs(d) > 1e-12) & (u >= 0) & (v >= 0) & (u + v <= 1)
        if not hit.any():
            return None
        z = (a[:, 2] + u * (b[:, 2] - a[:, 2]) + v * (c[:, 2] - a[:, 2]))[hit]
        return float(z.min() if back else z.max())

    def at(self, name):
        return self.rest[name][:3, 3]

    def select(self, names=None, arms=False, kind=None):
        """Points whose strongest joint is one of ``names`` (or any but the
        arms' when names is None), of one kind of piece if given."""
        order = self.rig.order
        if names is None:
            ok = np.array([arms or not ARM_JOINTS.match(n) for n in order])
        else:
            ok = np.array([n in names for n in order])
        keep = ok[self.joint]
        if kind is not None:
            keep = keep & (self.kind == kind)
        return self.points[keep]


def outline(points, center, y0, y1, margin, n=SEGMENTS):
    """The radius (from a vertical axis through ``center``'s x, z) of the
    points between heights y0 and y1, at n angles round it (angle 0 to the
    front, +z): each sector's farthest point, widened to its neighbours'
    and with empty sectors interpolated from the nearest full ones, plus
    margin."""
    band = points[(points[:, 1] >= y0) & (points[:, 1] <= y1)]
    if not len(band):
        raise ConvertError("a generated piece found nothing to sit on")
    dx, dz = band[:, 0] - center[0], band[:, 2] - center[2]
    sector = np.floor((np.arctan2(dx, dz) / (2 * math.pi) + 0.5 / n) * n).astype(np.int64) % n
    r = np.zeros(n)
    np.maximum.at(r, sector, np.hypot(dx, dz))
    full = np.flatnonzero(r > 0)
    if len(full) < 2:
        raise ConvertError("a generated piece found too little to sit on")
    r = np.interp(np.arange(n), np.concatenate([full - n, full, full + n]), np.tile(r[full], 3))
    return np.maximum(r, np.maximum(np.roll(r, 1), np.roll(r, -1))) + margin


def axis(points, y0, y1):
    """The middle (x, z) of the points between heights y0 and y1."""
    band = points[(points[:, 1] >= y0) & (points[:, 1] <= y1)]
    if not len(band):
        raise ConvertError("a generated piece found nothing to sit on")
    middle = (band.min(axis=0) + band.max(axis=0)) / 2
    return np.array([middle[0], 0.0, middle[2]])


def angles(n=SEGMENTS):
    a = np.arange(n) * (2 * math.pi / n)
    return np.sin(a), np.cos(a)  # x, z of each direction


def ring(center, radius, y, n=SEGMENTS):
    sx, cz = angles(n)
    y = np.broadcast_to(y, (n,))
    return np.stack([center[0] + radius * sx, y, center[2] + radius * cz], axis=1)


def strip(rows, closed=True):
    """Triangles joining rows of vertex indices (each a ring or a line),
    counter-clockwise from the side the rows run anticlockwise round."""
    tris = []
    for lo, hi in zip(rows, rows[1:]):
        count = len(lo) if closed else len(lo) - 1
        for k in range(count):
            a, b = lo[k], lo[(k + 1) % len(lo)]
            c, d = hi[(k + 1) % len(hi)], hi[k]
            tris += [(a, c, b), (a, d, c)]
    return tris


def fan(center, ring_, closed=True):
    count = len(ring_) if closed else len(ring_) - 1
    return [(center, ring_[(k + 1) % len(ring_)], ring_[k]) for k in range(count)]


def disc(center, normal, radius, sides=6):
    """A flat polygon facing ``normal``: its vertices and triangles."""
    normal = normalize(np.asarray(normal, float))
    u = normalize(np.cross([0.0, 1.0, 0.0], normal) if abs(normal[1]) < 0.9 else np.cross([1.0, 0.0, 0.0], normal))
    v = np.cross(normal, u)
    a = np.arange(sides) * (2 * math.pi / sides)
    rim = center + radius * (np.cos(a)[:, None] * u + np.sin(a)[:, None] * v)
    coords = np.vstack([center, rim])
    return coords, [(0, 1 + k, 1 + (k + 1) % sides) for k in range(sides)]


def facing(coords, tris, outward):
    """Turns each triangle to face ``outward`` (a function of its centre)."""
    out = []
    for t in tris:
        p = coords[list(t)]
        n = np.cross(p[1] - p[0], p[2] - p[0])
        out.append(t if np.dot(n, outward(p.mean(axis=0))) >= 0 else (t[0], t[2], t[1]))
    return out


def joined(parts):
    """One mesh of several (coords, triangles) parts."""
    coords, tris, base = [], [], 0
    for c, t in parts:
        coords.append(np.asarray(c, float))
        tris += [tuple(v + base for v in tri) for tri in t]
        base += len(c)
    return np.vstack(coords), tris


def made(label, material, coords, tris, joint, rig, texco=None):
    """A generated Piece rigidly skinned to one joint (weight 1). With
    ``texco`` (one UV per vertex) it can carry a texture."""
    if joint not in rig.order:
        raise ConvertError(f"{label}: the skeleton has no {joint} joint")
    w = np.zeros((len(coords), len(rig.order)))
    w[:, rig.order.index(joint)] = 1.0
    if texco is None:
        return Piece(label, "piece", material, np.asarray(coords, float), [tuple((v, 0) for v in t) for t in tris], w, np.zeros((1, 2)))
    return Piece(label, "piece", material, np.asarray(coords, float), [tuple((v, v) for v in t) for t in tris], w, np.asarray(texco, float))


# The generated pieces that lie over several bones, and the joints each may
# follow; the rest are rigid on their one bone.
TRUNK = ("pelvis", "spine_01", "spine_02", "spine_03")
SKIN_FROM = {
    "sash": ("pelvis", "spine_01"),
    "rastra": ("pelvis", "spine_01"),
    "buttons": TRUNK,
    "chain": TRUNK,
    "strap": TRUNK + ("clavicle_l", "clavicle_r"),
    "poncho": TRUNK + ("clavicle_l", "clavicle_r"),
    "shawl": TRUNK + ("clavicle_l", "clavicle_r"),
    "lapels": TRUNK + ("clavicle_l", "clavicle_r", "neck_01"),
}
SKIN_NEAREST = 8


def inherit(piece, body, joints):
    """``piece`` skinned like the dressed body under it, the way MakeHuman's
    garments take the body's skin: each vertex gets the inverse-distance
    mean of the weights of its ``SKIN_NEAREST`` nearest body points, of
    those that ``joints`` carry most of, kept to ``joints``. So the poncho
    turns with the clavicle at the shoulder and with the pelvis at the
    hip, and blends between them along the way. Unchanged when no body
    point is on those joints."""
    idx = [body.rig.order.index(j) for j in joints if j in body.rig.order]
    w = body.weights[:, idx]
    share = w.sum(axis=1)
    on = share > 0.5
    if not idx or not on.any():
        return piece
    points, w = body.points[on], w[on] / share[on, None]
    k = min(SKIN_NEAREST, len(points))
    mixed = np.zeros((len(piece.coords), len(idx)))
    for start in range(0, len(piece.coords), 256):
        p = piece.coords[start : start + 256]
        d = np.linalg.norm(p[:, None, :] - points[None, :, :], axis=2)
        near = np.argsort(d, axis=1, kind="stable")[:, :k]
        inv = 1.0 / (np.take_along_axis(d, near, axis=1) + 0.01)
        mixed[start : start + len(p)] = (inv[..., None] * w[near]).sum(axis=1) / inv.sum(axis=1)[:, None]
    piece.weights = np.zeros((len(piece.coords), len(body.rig.order)))
    piece.weights[:, idx] = mixed
    return piece


# How the poncho clears the arm it lies over: how far outside the upper
# arm's surface it is kept (decimeters), and how much of the upper arm's
# turn it takes where it lies on it, fading out over how far from it.
ARM_CLEARANCE = 0.25
ARM_SHARE = 0.9
ARM_REACH = 1.5


def clear_arms(piece, body, sides=("l", "r")):
    """``piece`` kept off the upper arms and over the shoulders: every
    vertex within ``ARM_CLEARANCE`` of an upper arm's surface (a capsule
    from the shoulder joint to just past the elbow, as thick as the dressed
    arm round its middle) is pushed straight out to it, and the vertices near it take
    up to ``ARM_SHARE`` of the upper arm's weight, fading over
    ``ARM_REACH``, so the folded poncho rides on the deltoid as the arm
    swings under it instead of going through it. Only what lies out from
    the shoulder joint moves (the chest is left as it was). Pieces near no
    arm are unchanged."""
    order = list(body.rig.order)
    coords, weights = piece.coords.copy(), piece.weights.copy()
    for side in sides:
        upper, lower = f"upperarm_{side}", f"lowerarm_{side}"
        if upper not in order or lower not in order:
            continue
        a, b = body.at(upper), body.at(lower)
        along = b - a
        arm = body.select({upper})
        if not len(arm) or not along.any():
            continue

        def place(p):
            t = (p - a) @ along / (along @ along)
            near = a + np.clip(t, 0.0, 1.0)[:, None] * along
            return t, near, np.linalg.norm(p - near, axis=1)

        t, _, d = place(arm)
        middle = (t > 0.3) & (t < 0.7)
        if not middle.any():
            continue
        radius = float(np.median(d[middle]))
        t, near, d = place(coords)
        # Past the elbow a little too (the poncho's end hangs by it), and
        # only out from the shoulder joint (within 7 cm of it):
        # the chest in front stays as it is.
        outer = np.abs(coords[:, 0]) >= abs(a[0]) - 0.7
        on = (t >= 0.0) & (t <= 1.2) & outer
        push = on & (d < radius + ARM_CLEARANCE) & (d > 1e-9)
        coords[push] = near[push] + (coords[push] - near[push]) * ((radius + ARM_CLEARANCE) / d[push])[:, None]
        d = np.where(push, radius + ARM_CLEARANCE, d)
        share = ARM_SHARE * np.clip(1.0 - (d - radius - ARM_CLEARANCE) / ARM_REACH, 0.0, 1.0)
        share = np.where((t >= -0.2) & (t <= 1.3) & outer, share, 0.0)
        k = order.index(upper)
        weights *= (1.0 - share)[:, None]
        weights[:, k] += share
    piece.coords, piece.weights = coords, weights
    return piece


def plain(name, color, roughness=0.9, metallic=0.0, double_sided=True, pattern=None):
    m = Material(name)
    m.color = list(color) + [1.0]
    m.roughness, m.metallic, m.double_sided = roughness, metallic, double_sided
    m.pattern = pattern
    return m


def setting(settings, kind, key):
    v = settings.get(key, PIECES[kind][key])
    return v if len(v) > 1 else v[0]


def head_frame(body):
    """The head's points (skin and hair), the top of the head and the
    vertical axis the hats sit on."""
    head = body.select({"head"})
    top = head[:, 1].max()
    upper = head[head[:, 1] > top - 1.2]
    center = (upper.min(axis=0) + upper.max(axis=0)) / 2
    return head, top, center


def hat_band(body, settings, kind):
    """The band where a hat meets the head: its heights round the head
    (lower at the back by tilt), its radii, its axis and the head's top."""
    head, top, center = head_frame(body)
    fit, tilt = setting(settings, kind, "fit"), setting(settings, kind, "tilt")
    _, cz = angles()
    y = top - fit - tilt * (1 - cz) / 2
    # Snug on the skull: the skin's outline (the hair under the band is cut
    # away), with room for the felt; the hair's when there is no skin.
    skull = body.select({"head"}, kind="proxy")
    lo, hi = y.min() - 0.1, y.max() + 0.25
    if np.count_nonzero((skull[:, 1] >= lo) & (skull[:, 1] <= hi)) >= 3:
        radius = outline(skull, center, lo, hi, 0.04)
    else:
        radius = outline(head, center, lo, hi, 0.08)
    return y, radius, center, top


def radial(center, up=0.0):
    """An outward() for facing(): away from a vertical axis, tipped up."""
    def outward(p):
        d = p - center
        d[1] = 0.0
        return d + np.array([0.0, up, 0.0])
    return outward


def make_hat(body, settings, n):
    """A felt hat: crown sides up to a flat top (creased front to back by
    dent, its edge softened by round), a brim with a thickness (thick) and
    an edge turned up by curl, and a ribbon round the band. Returns (felt
    coords, triangles), (ribbon coords, triangles) and the band's lowest
    height."""
    k = "hat"
    y, r, center, head_top = hat_band(body, settings, k)
    crown, taper, brim = (setting(settings, k, x) for x in ("crown", "taper", "brim"))
    droop, back, sides = (setting(settings, k, x) for x in ("droop", "back", "sides"))
    dent, pinch, soft = (setting(settings, k, x) for x in ("dent", "pinch", "round"))
    thick, curl = setting(settings, k, "thick"), setting(settings, k, "curl")
    sx, cz = angles(n)
    top_y = head_top + max(crown, 0.05)
    dent = min(max(dent, 0.0), max(crown, 0.05) - 0.03)  # the crease never reaches the head
    squeeze = 1 - pinch * np.clip(cz, 0, 1) ** 2
    rt = r * taper * squeeze
    height = top_y - y
    crease = dent * (1 - 0.7 * sx**2)  # deepest along the front-back line
    crown_rows = [
        ring(center, r, y),
        ring(center, r + (rt - r) * 0.6, y + 0.6 * height),
        ring(center, rt, top_y - 0.04 - 0.25 * soft),  # the shoulder of the crown
        ring(center, rt * (0.93 - 0.2 * soft), top_y - 0.6 * crease * soft),
        ring(center, rt * 0.5, top_y - 0.6 * crease),
    ]
    middle = np.array([center[0], top_y - dent, center[2]])
    # The brim, its edge dropped at the front by droop, at the back by back
    # and at the sides by sides, a lip at its edge turned up by curl.
    drop = droop * np.clip(cz, 0, 1) ** 2 + back * np.clip(-cz, 0, 1) ** 2 + sides * sx**2
    out = np.stack([sx, np.zeros_like(sx), cz], axis=1)
    up = np.array([0.0, 1.0, 0.0])
    inner = ring(center, r, y)
    mid = inner + out * (brim * 0.5) - np.outer(0.3 * drop, up)
    edge = inner + out * brim - np.outer(drop, up)
    lip = edge + out * 0.06 + up * curl
    brim_rows = [inner, mid, edge, lip, lip - up * thick * 0.6, edge - up * thick, mid - up * thick, inner - up * thick]
    coords = np.vstack(crown_rows + [middle[None]] + brim_rows)
    ids = [list(range(i * n, (i + 1) * n)) for i in range(len(crown_rows))]
    m = len(crown_rows) * n
    brim_ids = [list(range(m + 1 + i * n, m + 1 + (i + 1) * n)) for i in range(len(brim_rows))]
    crown_tris = facing(coords, strip(ids[:3]), radial(center, 0.3))
    top_tris = facing(coords, strip(ids[2:5]) + fan(m, ids[4]), radial(center, 6.0))
    up_tris = facing(coords, strip(brim_ids[:4]), lambda p: up)
    edge_tris = facing(coords, strip(brim_ids[3:5]), radial(center))
    under_tris = facing(coords, strip(brim_ids[4:]), lambda p: -up)
    felt = (coords, crown_tris + top_tris + up_tris + edge_tris + under_tris)
    rows = [ring(center, r * 1.015 + 0.015, y + 0.01), ring(center, r * 1.015 + 0.015, y + 0.3)]
    ribbon = (np.vstack(rows), facing(np.vstack(rows), strip([list(range(n)), list(range(n, 2 * n))]), radial(center)))
    return felt, ribbon, y.min()


def make_kepi(body, settings, n):
    """A kepi: cloth sides from the band up to a flat top a little wider
    (taper), sloping down to the front (slope); a leather band, a stiff
    visor and a chin strap over it, with a brass button at each end.
    Returns the cloth, the leather and the buttons (coords, triangles each)
    and the band's lowest height."""
    k = "kepi"
    y, r, center, head_top = hat_band(body, settings, k)
    crown, taper, width = (setting(settings, k, x) for x in ("crown", "taper", "width"))
    droop, slope = setting(settings, k, "droop"), setting(settings, k, "slope")
    sx, cz = angles(n)
    top = ring(center, r * taper, 0.0)
    # One plane, sloping down to the front by slope from back to front.
    depth = (top[:, 2] - center[2]) / np.abs(top[:, 2] - center[2]).max()
    top[:, 1] = head_top + max(crown, 0.0) - slope * depth / 2
    top_y = top[:, 1]
    rows = [ring(center, r, y), top]
    middle = np.array([center[0], top_y.mean(), center[2]])
    coords = np.vstack(rows + [middle[None]])
    ids = [list(range(n)), list(range(n, 2 * n))]
    cloth = (coords, facing(coords, strip(ids), radial(center)) + facing(coords, fan(2 * n, ids[1]), lambda p: np.array([0.0, 1.0, 0.0])))
    # The leather band round the bottom of the crown.
    lr = r * 1.01 + 0.012
    band = np.vstack([ring(center, lr, y - 0.01), ring(center, lr, y + 0.25)])
    band_tris = facing(band, strip(ids), radial(center))
    # The visor: the front band's span, out and down.
    front = sorted((i for i in range(n) if cz[i] > 0.5), key=lambda i: sx[i])
    base = rows[0][front] + np.array([0.0, 0.01, 0.0])
    # Straight out to the front, longest in the middle: a half disc.
    reach = width * np.sqrt(np.clip((cz[front] - 0.45) / 0.55, 0, 1))
    edge = base + np.outer(reach, [0.0, 0.0, 1.0]) - np.outer(droop * reach / width, [0, 1, 0])
    visor = np.vstack([base, edge])
    m = len(front)
    vtris = facing(visor, strip([list(range(m)), list(range(m, 2 * m))], closed=False), lambda p: np.array([0.0, 1.0, 0.0]))
    # The chin strap across the front of the band, just above the visor.
    strap_at = sorted((i for i in range(n) if cz[i] > -0.05), key=lambda i: sx[i])
    sr = lr + 0.02
    strap = np.vstack([ring(center, sr, y + 0.05)[strap_at], ring(center, sr, y + 0.13)[strap_at]])
    s = len(strap_at)
    stris = facing(strap, strip([list(range(s)), list(range(s, 2 * s))], closed=False), radial(center))
    leather = joined([(band, band_tris), (visor, vtris), (strap, stris)])
    buttons = []
    for end in (strap_at[0], strap_at[-1]):
        normal = np.array([sx[end], 0.0, cz[end]])
        buttons.append(disc(ring(center, sr + 0.015, y + 0.09)[end], normal, 0.05))
    return cloth, leather, joined(buttons), y.min()


def neck_band(body, n=SEGMENTS):
    neck, head = body.at("neck_01"), body.at("head")
    pts = body.select({"neck_01", "head", "spine_03", "clavicle_l", "clavicle_r"})
    pts = pts[np.hypot(pts[:, 0] - neck[0], pts[:, 2] - neck[2]) < 0.9]
    y = neck[1] + 0.25 * (head[1] - neck[1])
    return axis(pts, y - 0.3, y + 0.3), y, pts


def make_neckerchief(body, settings, n):
    k = "neckerchief"
    neck, y, pts = neck_band(body)
    lo, hi = y - 0.13, y + 0.13
    r_lo = outline(pts, neck, lo - 0.1, lo + 0.1, 0.05)
    r_hi = outline(pts, neck, hi - 0.1, hi + 0.1, 0.04)
    rows = [ring(neck, r_lo, lo), ring(neck, r_hi, hi)]
    # The point: from the knot's front stretch down over the chest.
    sx, cz = angles()
    front = sorted((i for i in range(n) if cz[i] > 0.55), key=lambda i: sx[i])
    drop = setting(settings, k, "drop")
    torso = body.select({"spine_03", "spine_02", "neck_01"})

    trunk = {nm for nm in body.rig.order if not ARM_JOINTS.match(nm) and nm != "head"}

    def chest(x, yy):
        z = body.surface(x, yy, trunk)
        if z is None:
            near = torso[(abs(torso[:, 0] - x) < 0.2) & (abs(torso[:, 1] - yy) < 0.15)]
            z = near[:, 2].max() if len(near) else rows[0][front, 2].max()
        return z + 0.04

    top = rows[0][front]
    mid_y = lo - drop * 0.5
    mid = np.array([[neck[0] + (p[0] - neck[0]) * 0.5, mid_y, 0.0] for p in top])
    mid[:, 2] = [chest(p[0], mid_y) for p in mid]
    tip = np.array([neck[0], lo - drop, chest(neck[0], lo - drop)])
    coords = np.vstack(rows + [mid, tip[None]])
    m = len(front)
    band = strip([list(range(n)), list(range(n, 2 * n))])
    flap_top = [front[i] for i in range(m)]
    flap_mid = list(range(2 * n, 2 * n + m))
    flap = strip([flap_top, flap_mid], closed=False) + fan(2 * n + m, flap_mid, closed=False)
    # The knot: a small lump at the front of the band, over the point.
    # (a little to the left of the middle, so it never reads as a tie).
    kx, ky = neck[0] + 0.15, lo - 0.08
    at = np.array([kx, ky, chest(kx, ky) + 0.06])
    size = setting(settings, k, "knot")
    knot = at + np.array([[size, 0, 0], [-size, 0, 0], [0, size, 0], [0, -size, 0], [0, 0, size * 0.8], [0, 0, -size * 0.5]])
    ktris = [(0, 2, 4), (2, 1, 4), (1, 3, 4), (3, 0, 4), (2, 0, 5), (1, 2, 5), (3, 1, 5), (0, 3, 5)]
    # Its two short ends, splayed out from the knot.
    ends = []
    for side in (1, -1):
        e = at + np.array([[0.0, 0.3 * size, size * 0.6], [0.0, -0.3 * size, size * 0.6],
                           [side * 1.6 * size, -2.0 * size, size * 0.4]])
        ends.append((e, facing(e, [(0, 1, 2)], lambda p: np.array([0.0, 0.0, 1.0]))))
    return joined([(coords, facing(coords, band + flap, radial(neck))), (knot, facing(knot, ktris, lambda p: p - at))] + ends)


def waist(body, settings, kind):
    pelvis, spine = body.at("spine_01"), body.at("spine_02")
    pts = body.select()
    pts = pts[np.hypot(pts[:, 0] - pelvis[0], pts[:, 2] - pelvis[2]) < 3.0]
    y = pelvis[1] + 0.35 * (spine[1] - pelvis[1]) + setting(settings, kind, "height")
    return axis(pts, y - 0.6, y + 0.6), y, pts


def make_sash(body, settings, n):
    """The faja: three rings round the waist, and a UV per vertex (round
    it, and across it for the stripes)."""
    k = "sash"
    center, y, pts = waist(body, settings, k)
    half = setting(settings, k, "width") / 2
    levels = [y - half, y, y + half]
    rows = [ring(center, outline(pts, center, h - 0.25, h + 0.25, 0.05 + (0.02 if h == y else 0.0)), h) for h in levels]
    coords = np.vstack(rows)
    tris = strip([list(range(i * n, (i + 1) * n)) for i in range(3)])
    # The last sector repeats the texture's edge; tiling hides the seam.
    texco = np.array([[4.0 * i / n, row / 2] for row in range(3) for i in range(n)])
    return coords, facing(coords, tris, lambda p: (p - center) * [1, 0, 1]), texco


def make_rastra(body, settings, n, sash):
    k = "rastra"
    center, y, pts = waist(body, settings, k)
    if sash is not None:
        pts = np.vstack([pts, sash])
    half = 0.2
    r = outline(pts, center, y - half - 0.1, y + half + 0.1, 0.03)
    rows = [ring(center, r, y - half), ring(center, r, y + half)]
    coords = np.vstack(rows)
    tris = facing(coords, strip([list(range(n)), list(range(n, 2 * n))]), lambda p: (p - center) * [1, 0, 1])
    # The coins and the plate, just proud of the belt at the front.
    count = int(setting(settings, k, "coins"))
    sx, cz = angles(64)
    rr = np.interp(np.arange(64) / 64 * n, np.arange(n + 1), np.append(r, r[0]))
    parts = []
    slots = [(0.0, 0.26)] + [(s * a, 0.13) for a in np.linspace(0.35, 1.05, max(count // 2, 1)) for s in (-1, 1)][:count]
    for angle, size in slots:
        i = int(round(angle / (2 * math.pi) * 64)) % 64
        normal = np.array([math.sin(angle), 0.0, math.cos(angle)])
        at = np.array([center[0], y, center[2]]) + normal * (rr[i] + 0.02)
        parts.append(disc(at, normal, size, 8 if size > 0.2 else 6))
    return (coords, tris), joined(parts)


def make_buttons(body, settings, n):
    k = "buttons"
    count, spread = int(setting(settings, k, "count")), setting(settings, k, "spread")
    top = body.at("spine_03")[1] + 0.55 * (body.at("neck_01")[1] - body.at("spine_03")[1])
    bottom = body.at("spine_01")[1] + 0.3 * (body.at("spine_02")[1] - body.at("spine_01")[1])
    torso = body.select({"spine_01", "spine_02", "spine_03", "pelvis", "neck_01"})
    x0 = body.at("spine_02")[0]
    parts = []
    columns = [x0] if spread <= 0 else [x0 - spread / 2, x0 + spread / 2]
    for y in np.linspace(top, bottom, count):
        for x in columns:
            near = torso[(abs(torso[:, 0] - x) < 0.25) & (abs(torso[:, 1] - y) < 0.25)]
            if not len(near):
                continue
            parts.append(disc(np.array([x, y, near[:, 2].max() + 0.025]), [0.0, 0.0, 1.0], 0.075))
    if not parts:
        raise ConvertError("buttons: no chest to sew them on")
    return joined(parts)


def make_chain(body, settings, n):
    """A watch chain: a thin ribbon across the front of the waistcoat (the
    outermost torso garment without it), sagging from the middle to a
    pocket on each side."""
    k = "chain"
    low, high = body.at("spine_01"), body.at("spine_02")
    y0 = low[1] + setting(settings, k, "height") * (high[1] - low[1])
    reach, sag = setting(settings, k, "reach"), setting(settings, k, "sag")
    joints = {"spine_01", "spine_02", "spine_03", "pelvis"}
    t = np.linspace(-1.0, 1.0, 17)
    xs = high[0] + reach * t
    ys = y0 - sag * np.sin(math.pi * np.abs(t))
    path = []
    for x, y in zip(xs, ys):
        z = body.surface(x, y, joints, "waistcoat")
        z = body.surface(x, y, joints) if z is None else z
        if z is None:
            raise ConvertError("chain: no waistcoat to hang it on")
        path.append([x, y, z + 0.02])
    path = np.array(path)
    half = np.array([0.0, 0.016, 0.0])
    coords = np.vstack([path - half, path + half])
    m = len(path)
    tris = facing(coords, strip([list(range(m)), list(range(m, 2 * m))], closed=False), lambda p: np.array([0.0, 0.0, 1.0]))
    return coords, tris


def diagonal(body, side):
    """A path across the dressed torso in the plane from one shoulder (side
    >= 0 the left) to the other hip: up the back, over the shoulder and
    down the front, just proud of what it lies on. Returns its points, each
    point's outward direction, the direction across it in that plane, and
    the axis it goes round."""
    near, far = ("l", "r") if side >= 0 else ("r", "l")
    p0 = body.at(f"upperarm_{near}") * [1.0, 1.0, 0.0] + [0.0, 0.5, 0.0]
    p1 = body.at(f"thigh_{far}") * [1.0, 1.0, 0.0] + [0.0, 0.9, 0.0]
    across = normalize(np.cross(p1 - p0, [0.0, 0.0, 1.0]))
    trunk = {n for n in body.rig.order if not ARM_JOINTS.match(n) and n != "head"}
    front, back = [], []
    for t in np.linspace(0.0, 1.0, 24):
        q = p0 + t * (p1 - p0)
        zf, zb = body.surface(q[0], q[1], trunk), body.surface(q[0], q[1], trunk, back=True)
        if zf is None or zb is None:
            continue
        front.append([q[0], q[1], zf + 0.03])
        back.append([q[0], q[1], zb - 0.03])
    if len(front) < 3:
        raise ConvertError("strap: no torso to lay it across")
    torso = body.select()
    first = np.array(front[0])
    middle = (front[0][2] + back[0][2]) / 2
    top = torso[np.hypot(torso[:, 0] - first[0], torso[:, 2] - middle) < 0.35]
    over = [first[0], (top[:, 1].max() if len(top) else first[1]) + 0.03, middle]
    path = np.array(back[::-1] + [over] + front)
    m = len(back)
    out = np.array([[0.0, 0.0, -1.0]] * m + [[0.0, 1.0, 0.0]] + [[0.0, 0.0, 1.0]] * len(front))
    # Round the shoulder: each direction the mean of its neighbours'.
    out = np.array([normalize(out[max(i - 1, 0) : i + 2].sum(axis=0)) for i in range(len(out))])
    return path, out, across, np.array([p0[0], 0.0, middle])


def make_strap(body, settings, n):
    """A flat leather strap along diagonal()."""
    k = "strap"
    path, _, across, center = diagonal(body, setting(settings, k, "side"))
    half = across * setting(settings, k, "width") / 2
    coords = np.vstack([path - half, path + half])
    m = len(path)
    tris = facing(coords, strip([list(range(m)), list(range(m, 2 * m))], closed=False), radial(center, 0.5))
    return coords, tris


def make_shawl_ends(body, settings, steps=10, across=7):
    """A shawl's two ends: each a band ``width`` wide from under the shawl
    at one shoulder down across the chest to the other side of the waist,
    where it is tucked in; the right one laid over the left. Each point is
    laid on what is frontmost there, the bumps along the band smoothed.
    Returns (coords, triangles) for each."""
    width = setting(settings, "shawl", "width")
    low, high = body.at("spine_01")[1], body.at("spine_02")[1]
    waist = low + 0.75 * (high - low)
    x0 = body.at("spine_02")[0]
    wide = abs(body.at("upperarm_l")[0] - x0)
    shoulder = body.at("upperarm_l")[1] + 0.3
    trunk = {nm for nm in body.rig.order if not ARM_JOINTS.match(nm) and nm != "head"}
    out = []
    for k, side in enumerate((1, -1)):
        a = np.array([x0 + side * 0.5 * wide, shoulder])
        b = np.array([x0 - side * 0.45 * wide, waist])
        along = normalize(np.append(b - a, 0.0))[:2]
        normal = np.array([-along[1], along[0]])
        grid = np.zeros((across, steps, 3))
        for i, u in enumerate(np.linspace(-0.5, 0.5, across) * width):
            for j, t in enumerate(np.linspace(0.0, 1.0, steps)):
                x, y = a + t * (b - a) + normal * u
                z = body.surface(x, y, trunk)
                grid[i, j] = [x, y, np.nan if z is None else z]
        z = grid[:, :, 2]
        if np.isnan(z).all():
            raise ConvertError("shawl: no chest to cross its ends on")
        # Off the chest's side, as far forward as the nearest point on it.
        seen = np.argwhere(~np.isnan(z))
        for i, j in np.argwhere(np.isnan(z)):
            near = seen[np.argmin(np.abs(seen[:, 0] - i) + 2 * np.abs(seen[:, 1] - j))]
            z[i, j] = z[near[0], near[1]]
        padded = np.pad(z, ((0, 0), (1, 1)), mode="edge")
        smooth = (padded[:, :-2] + 2 * z + padded[:, 2:]) / 4  # along the band, never into the chest
        grid[:, :, 2] = np.maximum(smooth, z) + 0.08 + 0.04 * k
        coords = grid.reshape(-1, 3)
        tris = strip([list(range(i * steps, (i + 1) * steps)) for i in range(across)], closed=False)
        out.append((coords, facing(coords, tris, lambda p: np.array([0.0, 0.0, 1.0]))))
    return out


def make_poncho(body, settings, n, sides=8):
    """A poncho folded long and laid along diagonal(): a flat oval section
    ``width`` across and ``thick`` deep, its two ends hanging ``hang``
    below the hip, front and back, a little wider, closed at the ends. Its
    UVs run round the section (v), so the woven stripes run along it."""
    k = "poncho"
    path, out, across, _ = diagonal(body, setting(settings, k, "side"))
    width, thick, hang = (setting(settings, k, x) for x in ("width", "thick", "hang"))
    steps = np.linspace(1 / 3, 1.0, 3)[:, None]
    down = np.array([0.0, -1.0, 0.0])
    tail_back = path[0] + steps * hang * down + steps * out[0] * 0.15
    tail_front = path[-1] + steps * hang * down + steps * out[-1] * 0.15
    path = np.vstack([tail_back[::-1], path, tail_front])
    out = np.vstack([np.repeat(out[:1], 3, 0), out, np.repeat(out[-1:], 3, 0)])
    spread = np.concatenate([1 + 0.15 * steps[::-1, 0], np.ones(len(path) - 6), 1 + 0.15 * steps[:, 0]])
    # One more vertex than sides round each section: the first again, at
    # the far edge of the texture, so its seam does not squeeze it.
    a = (np.arange(sides + 1) / sides + 0.75) * 2 * math.pi
    rows, texco = [], []
    for p, o, w in zip(path, out, spread):
        rows.append(p + np.outer(np.cos(a), across * width * w / 2) + np.outer(1 + np.sin(a), o * thick / 2))
        texco += [[0.0, i / sides] for i in range(sides + 1)]
    coords = np.vstack(rows + [path[0] + out[0] * thick / 2, path[-1] + out[-1] * thick / 2])
    texco += [[0.0, 0.5], [0.0, 0.5]]
    m = sides + 1
    ids = [list(range(i * m, (i + 1) * m)) for i in range(len(path))]
    ends = fan(len(coords) - 2, ids[0], closed=False) + fan(len(coords) - 1, ids[-1], closed=False)
    tris = strip(ids, closed=False) + ends
    centers = path + out * thick / 2

    def outward(p):
        return p - centers[np.argmin(np.linalg.norm(centers - p, axis=1))]

    return coords, facing(coords, tris, outward), np.array(texco)


def make_lapels(body, settings, n):
    """A coat's two lapels: flat pieces laid on what is frontmost there,
    from their point up and out to the collar, widest two thirds of the
    way up and notched at the top."""
    k = "lapels"
    spine, neck = body.at("spine_02"), body.at("neck_01")
    low = spine[1] + setting(settings, k, "point") * (neck[1] - spine[1])
    high = neck[1] - 0.15
    trunk = {nm for nm in body.rig.order if not ARM_JOINTS.match(nm) and nm != "head"}
    open_, width = setting(settings, k, "open"), setting(settings, k, "width")
    parts = []
    for side in (1, -1):
        rows = []
        for t in np.linspace(0.0, 1.0, 8):
            y = low + t * (high - low)
            w = width * min(1.0, t * 1.5) * (1 - 0.5 * max(0.0, t - 0.7) / 0.3)
            row = []
            for x in (spine[0] + side * open_ * t / 2, spine[0] + side * (open_ * t / 2 + max(w, 0.02))):
                z = body.surface(x, y, trunk)
                if z is None:
                    raise ConvertError("lapels: no coat to lay them on")
                row.append([x, y, z + 0.025])
            rows.append(row)
        rows = np.array(rows)
        coords = np.vstack([rows[:, 0], rows[:, 1]])
        m = len(rows)
        parts.append((coords, facing(coords, strip([list(range(m)), list(range(m, 2 * m))], closed=False), lambda p: np.array([0.0, 0.0, 1.0]))))
    return joined(parts)


def make_collar(body, settings, n):
    """A stiff band collar round the neck and its two wing tips, folded
    down and out at the front."""
    k = "collar"
    neck, y, _ = neck_band(body)
    height = setting(settings, k, "height")
    lo = y - 0.3
    hi = lo + height
    # Round the neck's skin (what shows of it above the shirt).
    r = outline(body.select({"neck_01"}, kind="proxy"), neck, lo - 0.05, hi + 0.3, 0.05)
    rows = [ring(neck, r, lo), ring(neck, r, hi)]
    coords = np.vstack(rows)
    tris = facing(coords, strip([list(range(n)), list(range(n, 2 * n))]), radial(neck))
    sx, cz = angles(n)
    wing = setting(settings, k, "wings")
    parts = [(coords, tris)]
    for side in (1, -1) if wing > 0 else ():
        i = int(np.argmin(np.abs(sx - side * 0.3) + (cz < 0)))
        top = rows[1][i]
        tip = top + np.array([side * wing, -wing * 1.2, wing * 0.4])
        foot = rows[0][i] + np.array([0.0, 0.0, 0.01])
        c = np.array([top + [0.0, 0.0, 0.01], tip, foot])
        parts.append((c, facing(c, [(0, 1, 2)], lambda p: np.array([0.0, 0.0, 1.0]))))
    return joined(parts)


def make_boina(body, settings, n):
    """A boina: a soft crown from the band out to a wide, flat top that
    overhangs it, pulled down to the left (``lean``), and a short stalk in
    the middle. Returns (coords, triangles) and the band's lowest
    height."""
    k = "boina"
    y, r, center, head_top = hat_band(body, settings, k)
    width, height = setting(settings, k, "width"), setting(settings, k, "height")
    lean, droop = setting(settings, k, "lean"), setting(settings, k, "droop")
    sx, _ = angles(n)
    rows = []
    for frac, scale in ((0.0, 1.0), (0.35, 0.5 * (1 + width)), (0.65, width), (0.9, width * 0.8), (1.0, width * 0.45)):
        row = ring(center, r * scale, y + frac * (head_top + height - y), n)
        row[:, 0] += lean * frac
        row[:, 1] -= droop * frac * np.clip(sx, 0, 1) * (frac < 1.0)
        rows.append(row)
    top_y = rows[-1][:, 1].mean()
    middle = np.array([center[0] + lean, top_y + 0.03, center[2]])
    stalk = np.array([middle + [0.0, 0.12, 0.0]])
    coords = np.vstack(rows + [middle[None], stalk])
    ids = [list(range(i * n, (i + 1) * n)) for i in range(len(rows))]
    tris = facing(coords, strip(ids), radial(center, 0.5))
    tris += facing(coords, fan(len(rows) * n, ids[-1]), lambda p: np.array([0.0, 1.0, 0.0]))
    # The stalk: a thin spike from three points round the middle.
    base = [ids[-1][0], ids[-1][n // 3], ids[-1][2 * n // 3]]
    tip = len(coords) - 1
    tris += facing(coords, [(base[0], base[1], tip), (base[1], base[2], tip), (base[2], base[0], tip)], radial(center, 1.0))
    return coords, tris, y.min()


def make_bun(body, settings, n, sides=10, levels=5):
    """A bun at the back of the head: a squashed ball ``size`` across, set
    just behind the hair at ``high`` between the nape and the crown."""
    k = "bun"
    head, top, center = head_frame(body)
    size, high = setting(settings, k, "size"), setting(settings, k, "high")
    nape = head[:, 1].min()
    y = nape + high * (top - nape)
    band = head[np.abs(head[:, 1] - y) < 0.25]
    back = band[:, 2].min() if len(band) else head[:, 2].min()
    at = np.array([center[0], y, back - 0.3 * size])
    radii = np.array([size / 2, size * 0.42, size * 0.38])
    a = np.arange(sides) * (2 * math.pi / sides)
    rows = []
    for lat in np.linspace(-math.pi / 2, math.pi / 2, levels + 2)[1:-1]:
        # Round the bun's axis, which points out of the back of the head (-z).
        rows.append(at + np.stack([np.cos(lat) * np.cos(a) * radii[0], np.cos(lat) * np.sin(a) * radii[1],
                                   np.full(sides, -np.sin(lat) * radii[2])], axis=1))
    poles = np.array([at + [0.0, 0.0, radii[2]], at - [0.0, 0.0, radii[2]]])
    coords = np.vstack(rows + [poles])
    ids = [list(range(i * sides, (i + 1) * sides)) for i in range(len(rows))]
    tris = strip(ids) + fan(len(rows) * sides, ids[0][::-1]) + fan(len(rows) * sides + 1, ids[-1])
    return coords, facing(coords, tris, lambda p: p - at)


def generate(character, body, rig):
    """The .mhm's generated pieces, in their order, and the lowest hat band
    (None without a hat). The garments are cut by dress()."""
    out, band = [], None
    sash = None
    n = SEGMENTS
    for kind, settings in character.pieces:
        if kind == "shawl":
            for coords, tris in make_shawl_ends(body, settings):
                piece = made("shawl end", plain("shawl", setting(settings, kind, "color"), ROUGHNESS["wool"], pattern="wool"),
                             coords, tris, "spine_03", rig, coords[:, [0, 1]] * 0.6)
                out.append(inherit(piece, body, SKIN_FROM["shawl"]))
        if kind in GARMENT_CUTS or kind in DRESSED:
            continue
        color = setting(settings, kind, "color")
        low = None
        if kind == "hat":
            (coords, tris), (ribbon, rtris), low = make_hat(body, settings, n)
            out.append(made("hat", plain("hat_felt", color, 0.92), coords, tris, "head", rig))
            out.append(made("hat band", plain("hat_band", setting(settings, kind, "band"), 0.8), ribbon, rtris, "head", rig))
        elif kind == "kepi":
            (coords, tris), (leather, ltris), (buttons, btris), low = make_kepi(body, settings, n)
            out.append(made("kepi", plain("kepi_cloth", color, 0.9), coords, tris, "head", rig))
            out.append(made("kepi leather", plain("kepi_leather", setting(settings, kind, "visor"), 0.45), leather, ltris, "head", rig))
            out.append(made("kepi buttons", plain("kepi_buttons", setting(settings, kind, "button"), 0.35, 1.0), buttons, btris, "head", rig))
        elif kind == "boina":
            coords, tris, low = make_boina(body, settings, n)
            out.append(made("boina", plain("boina", color, 0.95), coords, tris, "head", rig))
        elif kind == "bun":
            coords, tris = make_bun(body, settings, n)
            out.append(made("bun", plain("bun", color, 0.7), coords, tris, "head", rig))
        elif kind == "neckerchief":
            coords, tris = make_neckerchief(body, settings, n)
            out.append(made("neckerchief", plain("neckerchief", color, 0.9), coords, tris, "neck_01", rig))
        elif kind == "sash":
            coords, tris, texco = make_sash(body, settings, n)
            sash = coords
            out.append(made("sash", plain("sash", color, 0.92, pattern="stripe"), coords, tris, "pelvis", rig, texco))
            out[-1] = inherit(out[-1], body, SKIN_FROM["sash"])
        elif kind == "rastra":
            (coords, tris), (coins, ctris) = make_rastra(body, settings, n, sash)
            out.append(made("rastra", plain("rastra_leather", color, 0.6), coords, tris, "pelvis", rig))
            out.append(made("rastra coins", plain("rastra_coins", setting(settings, kind, "coin"), 0.35, 1.0), coins, ctris, "pelvis", rig))
            out[-2:] = [inherit(pc, body, SKIN_FROM["rastra"]) for pc in out[-2:]]
        elif kind == "poncho":
            coords, tris, texco = make_poncho(body, settings, n)
            out.append(made("poncho", plain("poncho", color, 0.92, pattern="poncho"), coords, tris, "spine_03", rig, texco))
            out[-1] = clear_arms(inherit(out[-1], body, SKIN_FROM["poncho"]), body)
        elif kind == "lapels":
            coords, tris = make_lapels(body, settings, n)
            out.append(made("lapels", plain("lapels", color, 0.6), coords, tris, "spine_03", rig))
            out[-1] = inherit(out[-1], body, SKIN_FROM["lapels"])
        elif kind == "collar":
            coords, tris = make_collar(body, settings, n)
            out.append(made("collar", plain("collar", color, 0.8), coords, tris, "neck_01", rig))
        elif kind == "strap":
            coords, tris = make_strap(body, settings, n)
            out.append(made("strap", plain("strap_leather", color, 0.55), coords, tris, "spine_03", rig))
            out[-1] = inherit(out[-1], body, SKIN_FROM["strap"])
        elif kind == "chain":
            coords, tris = make_chain(body, settings, n)
            out.append(made("watch chain", plain("watch_chain", color, 0.35, 1.0), coords, tris, "spine_02", rig))
            out[-1] = inherit(out[-1], body, SKIN_FROM["chain"])
        else:
            coords, tris = make_buttons(body, settings, n)
            out.append(made("buttons", plain("brass_buttons", color, 0.35, 1.0), coords, tris, "spine_02", rig))
            out[-1] = inherit(out[-1], body, SKIN_FROM["buttons"])
        if low is not None:
            band = low if band is None else min(band, low)
    return out, band


# --- garments cut from the body ---------------------------------------------------

BODY, ARM, HEAD = 0, 1, 2
# The joints a body vertex's station counts from: 0 at the soles, 1 at the
# ankles (foot), 2 the knees (calf), 3 the hips (thigh), 4 the waist
# (spine_02), 5 the shoulders (upperarm), 6 the neck and 7 the head,
# linear in height between them. Left and right joints are averaged.
STATION_JOINTS = (None, "foot", "calf", "thigh", "spine_02", "upperarm", "neck_01", "head")
RIM_INSET = 0.005  # a garment's rims reach down to this above the skin
CLOTH_TILE = 6.0  # the cloth textures repeat this often across the skin's UVs
# Each garment's cloth texture and its offset out from the skin (decimeters),
# piecewise linear in the station on the trunk and legs, and in the reach
# on the arms (None: no sleeves).
GARMENT_CUTS = {
    # Tucked in: close at the bottom, under the trousers or the bombacha.
    "shirt": ("cotton", ((3.0, 0.02), (4.0, 0.035), (5.8, 0.045), (6.0, 0.05), (6.5, 0.07)),
              ((-1.0, 0.05), (1.0, 0.06), (2.0, 0.055))),
    "waistcoat": ("wool", ((3.0, 0.09), (4.5, 0.10), (5.5, 0.10), (6.3, 0.08)), None),
    "jacket": ("wool", ((3.0, 0.15), (5.0, 0.16), (5.9, 0.15), (6.6, 0.12)),
               ((-1.0, 0.12), (1.0, 0.12), (2.0, 0.10))),
    # Close below the knee, where boots go over them.
    "trousers": ("wool", ((1.0, 0.04), (1.9, 0.045), (2.1, 0.08), (3.0, 0.10), (4.4, 0.07)), None),
    # Wide and loose from the calf to the hip, gathered at the bottom into
    # the boot tops.
    "bombacha": ("cotton", ((1.55, 0.05), (1.7, 0.1), (1.85, 0.38), (2.1, 0.46), (3.0, 0.48), (3.4, 0.4),
                            (3.8, 0.22), (4.4, 0.07)), None),
    # Snug from the sole up (flared at the top, see cut()).
    "boots": ("leather", ((-1.0, 0.06), (1.0, 0.07), (2.0, 0.075)), None),
    # Canvas shoes with a rope sole, snug to the ankle.
    "alpargatas": ("cotton", ((-1.0, 0.04), (1.0, 0.045), (1.3, 0.04)), None),
    # A woollen shawl over the shoulders, outside the blouse (its shape in
    # shawl_cut()).
    "shawl": ("wool", ((3.5, 0.16), (5.0, 0.17), (6.0, 0.15), (6.4, 0.12)), ((-1.0, 0.17), (1.0, 0.15))),
    # Cut from the head (see head_cut(); their offsets are set there).
    "headscarf": ("cotton", ((0.0, 0.0),), None),
    "hair": (None, ((0.0, 0.0),), None),
}
# The pieces dress() makes besides GARMENT_CUTS' shells.
DRESSED = ("skirt", "apron")
ROUGHNESS = {"cotton": 0.85, "wool": 0.92, "stripe": 0.92, "leather": 0.55, None: 0.7}


class Figure:
    """The body proxy, posed, as the garments are cut from it: every
    vertex's smooth normal, its limb (BODY: the trunk, neck and legs; ARM;
    HEAD) by its strongest joint, its side (1 left, -1 right, 0 middle),
    its station (a height, see STATION_JOINTS) and, on the arms, its reach
    (0 at the shoulder, 1 at the elbow, 2 at the wrist, more on the hand)."""

    def __init__(self, rig, rest, skin):
        self.rig, self.rest = rig, rest
        self.coords, self.weights, self.texco = skin.coords, skin.weights, skin.texco
        self.triangles = skin.all_triangles
        self.faces = np.array([[v for v, _ in t] for t in self.triangles], np.int64).reshape(-1, 3)
        self.normals = smooth_normals(self.coords, self.triangles)
        names = rig.order
        strongest = self.weights.argmax(axis=1)
        arm = np.array([bool(ARM_JOINTS.match(n)) for n in names])[strongest]
        head = np.array([n == "head" for n in names])[strongest]
        self.limb = np.where(head, HEAD, np.where(arm, ARM, BODY))
        self.side = np.array([1 if n.endswith("_l") else -1 if n.endswith("_r") else 0 for n in names])[strongest]
        heights = [self.coords[:, 1].min()] + [self.joint(j)[1] for j in STATION_JOINTS[1:]]
        if np.any(np.diff(heights) <= 0):
            raise ConvertError("garments: the joints are not stacked from the feet up")
        self.heights = np.array(heights)
        self.station = np.interp(self.coords[:, 1], self.heights, np.arange(len(heights)))
        self.reach = np.zeros(len(self.coords))
        for suffix, side in (("l", 1), ("r", -1)):
            on = (self.limb == ARM) & (self.side == side)
            if not on.any():
                continue
            s, e, w = (self.joint(f"{b}_{suffix}") for b in ("upperarm", "lowerarm", "hand"))
            p = self.coords[on]

            def segment(a, b):
                ab = b - a
                t = (p - a) @ ab / (ab @ ab)
                return t, np.linalg.norm(p - (a + np.clip(t, 0, 1)[:, None] * ab), axis=1)

            t1, d1 = segment(s, e)
            t2, d2 = segment(e, w)
            self.reach[on] = np.where(d1 <= d2, np.minimum(t1, 1.0), 1.0 + np.maximum(t2, 0.0))

    def joint(self, name):
        """A joint's rest position; for 'foot' and the like, the mean of
        its left and right."""
        if name in self.rest:
            return self.rest[name][:3, 3]
        pair = [f"{name}_l", f"{name}_r"]
        if not all(p in self.rest for p in pair):
            raise ConvertError(f"garments: the skeleton has no {name} joint")
        return (self.rest[pair[0]][:3, 3] + self.rest[pair[1]][:3, 3]) / 2

    def height(self, station):
        return float(np.interp(station, np.arange(len(self.heights)), self.heights))


def cut(figure, kind, settings):
    """Which skin vertices a garment covers and how far out it sits over
    each (zero where it does not cover)."""
    f = figure
    g = lambda key: setting(settings, kind, key)  # noqa: E731
    _, body_profile, arm_profile = GARMENT_CUTS[kind]
    if kind in HEAD_CUTS:
        return head_cut(f, kind, g)
    body = (f.limb == BODY) & (f.station >= g("bottom")) & (f.station <= g("top"))
    if kind == "shawl":
        body &= shawl_cut(f, g)
    if g("vneck") > 0:
        # The V at the front, widening from its point up to the neck.
        span = max(6.0 - g("vneck"), 0.1)
        width = g("vwidth") * np.clip((f.station - g("vneck")) / span, 0, None)
        body &= ~((f.normals[:, 2] > 0.3) & (np.abs(f.coords[:, 0] - f.joint("spine_02")[0]) < width))
    offset = np.where(body, np.interp(f.station, *zip(*body_profile)), 0.0)
    if arm_profile and g("cuff") > 0:
        arm = (f.limb == ARM) & (f.reach <= g("cuff"))
        offset = np.where(arm, np.interp(f.reach, *zip(*arm_profile)), offset)
        body |= arm
    if kind in ("boots", "alpargatas"):
        offset = offset + g("sole") * (f.normals[:, 1] < -0.5)  # the soles
    if kind == "boots":
        offset = offset + 0.075 * np.clip((f.station - (g("top") - 0.15)) / 0.15, 0, 1)  # the flared top
    if g("pad") > 0:
        # Padded shoulders: the tops of the shoulders stand out, squared.
        top = (f.normals[:, 1] > 0.35) & (
            ((f.limb == BODY) & (f.station > 5.2) & (np.abs(f.coords[:, 0] - f.joint("spine_02")[0]) > 0.7))
            | ((f.limb == ARM) & (f.reach < 0.25)))
        offset = offset + g("pad") * (top & body)
    return body, offset * g("loose")


def shawl_cut(f, g):
    """Where a shawl (chal) lies on the trunk: round the shoulders above
    station 5.0, and down the back to a point at ``point`` in the middle.
    Its two ends, crossed over the chest, are generated (make_shawl_ends)."""
    x0 = f.joint("spine_02")[0]
    x = f.coords[:, 0] - x0
    wide = abs(f.joint("upperarm_l")[0] - x0)
    shoulders = f.station >= 5.0
    back = (f.normals[:, 2] < 0.0) & (f.station >= g("point") + np.abs(x) / wide * (4.8 - g("point")))
    return shoulders | back


# The pieces cut from the head's skin, and what of it they leave bare.
HEAD_CUTS = ("headscarf", "hair")


def head_measures(f):
    """The head's top, its chin's height, its middle (x, z) and its height,
    from the head's skin."""
    on = f.limb == HEAD
    p = f.coords[on]
    top = p[:, 1].max()
    chin = p[f.normals[on][:, 2] > 0.3][:, 1].min()
    middle = (p.min(axis=0) + p.max(axis=0)) / 2
    return top, chin, middle, top - chin


def head_cut(f, kind, g):
    """A piece cut from the head's skin (and the back of the neck): a
    headscarf framing the face (an oval ``face`` wide, from the chin to
    ``brow``, as a share of the head's height down from its top), or hair
    worn up, from a hairline ``brow`` down at the front to the nape at the
    back, above the ears at the sides. Returns the vertices covered and
    their offsets (``loose`` out, more at the crown by ``crown``)."""
    top, chin, middle, height = head_measures(f)
    y, nz = f.coords[:, 1], f.normals[:, 2]
    x = f.coords[:, 0] - middle[0]
    head = f.limb == HEAD
    brow = top - g("brow") * height
    if kind == "headscarf":
        # The face's oval, open from the chin to the brow.
        mid = (brow + chin) / 2
        half = (brow - chin) / 2
        face = (nz > 0.0) & ((x / g("face")) ** 2 + ((y - mid) / half) ** 2 < 1.0)
        # Round the top of the neck too: tied under the chin.
        neck = (f.limb == BODY) & (f.station > 6.5)
        covered = (head & ~face) | neck
    else:
        # Down to the hairline at the front, to the ears at the sides and
        # to the nape at the back.
        line = np.interp(nz, [-1.0, -0.3, 0.3, 1.0], [chin + 0.3 * height, chin + 0.45 * height, chin + 0.62 * height, brow])
        covered = head & (y >= line)
    lift = np.clip((y - (top - 0.4 * height)) / (0.4 * height), 0.0, 1.0)
    offset = np.where(covered, g("loose") + g("crown") * lift, 0.0)
    return covered, offset


def shell(figure, label, kind, material, selected, kept, offset):
    """A garment's mesh: the kept skin faces pushed out along the normals
    by ``offset``, and a rim down to the skin along every edge where the
    selection ends (on a kept face), so no end is see-through. Every
    vertex keeps the weights of the skin vertex it comes from, and its UV,
    tiled CLOTH_TILE times for the cloth texture."""
    f = figure
    faces = f.faces
    edges = {}
    for fi in np.flatnonzero(selected):
        for k in range(3):
            a, b = int(faces[fi, k]), int(faces[fi, (k + 1) % 3])
            edges.setdefault((min(a, b), max(a, b)), []).append((fi, k))
    source, local = [], {}

    def vertex(v, inset=None):
        key = (v, inset)
        if inset == "rim" or key not in local:
            local[key] = len(source)
            source.append((v, inset))
        return local[key]

    tris = []
    for fi in np.flatnonzero(kept):
        tri = f.triangles[fi]
        tris.append(tuple((vertex(v), u) for v, u in tri))
    for key in sorted(edges):
        if len(edges[key]) != 1:
            continue
        fi, k = edges[key][0]
        if not kept[fi]:
            continue
        (a, ua), (b, ub), (c, _) = (f.triangles[fi][(k + i) % 3] for i in range(3))
        ao, bo, ai, bi = vertex(a, "rim"), vertex(b, "rim"), vertex(a, RIM_INSET), vertex(b, RIM_INSET)
        quad = [((ao, ua), (bo, ub), (bi, ub)), ((ao, ua), (bi, ub), (ai, ua))]
        across = (f.coords[a] + f.coords[b]) / 2 - f.coords[c]
        e1 = f.coords[b] - f.coords[a]
        normal = np.cross(e1, f.normals[a] * (RIM_INSET - offset[a]))
        if np.dot(normal, across) < 0:
            quad = [(t[0], t[2], t[1]) for t in quad]
        tris += quad
    v = np.array([s for s, _ in source], np.int64)
    push = np.array([offset[s] if inset in (None, "rim") else inset for s, inset in source])
    coords = f.coords[v] + f.normals[v] * push[:, None]
    pc = Piece(label, kind, material, coords, tris, f.weights[v].copy(), f.texco * CLOTH_TILE)
    pc.source = v
    return pc


def dress(character, figure, rig):
    """The garments cut from the body, from the .mhm's garment pieces
    (GARMENT_CUTS): each covers the skin faces whose vertices it all
    covers, pushed out by its offset. Where garments overlap,
    the face goes to the one that sits furthest out (the shirt under the
    waistcoat, the trousers tucked into the boots) and the others leave it
    out. Returns the garment pieces (and their skirts) and the skin
    triangles they cover, as sorted vertex triples, to hide."""
    cuts = []
    for kind, settings in character.pieces:
        if kind in GARMENT_CUTS:
            cuts.append((kind, settings, GARMENT_CUTS[kind][0]) + cut(figure, kind, settings))
    faces = figure.faces
    owner = np.full(len(faces), -1)
    best = np.full(len(faces), -np.inf)
    selections = []
    for i, (_, _, _, covered, offset) in enumerate(cuts):
        selected = covered[faces].all(axis=1)
        mean = offset[faces].mean(axis=1)
        wins = selected & (mean >= best)
        owner[wins] = i
        best[wins] = mean[wins]
        selections.append(selected)
    out = []
    for i, (kind, settings, texture, covered, offset) in enumerate(cuts):
        material = plain(kind, setting(settings, kind, "color"), ROUGHNESS[texture], pattern=texture)
        kept = selections[i] & (owner == i)
        if kept.any():
            out.append(shell(figure, kind, kind, material, selections[i], kept, offset))
    points = np.vstack([figure.coords[figure.limb == BODY]] + [pc.coords[figure.limb[pc.source] == BODY] for pc in out])
    for kind, settings, *_ in cuts:
        if kind in GARMENT_CUTS and setting(settings, kind, "skirt") > 0:
            material = next(pc.material for pc in out if pc.kind == kind)
            out.append(make_skirt(figure, kind, settings, points, material, rig))
    hidden = owner >= 0
    long_skirt = None
    for kind, settings in character.pieces:
        if kind == "skirt":
            if long_skirt is not None:
                raise ConvertError("skirt: one long skirt only")
            long_skirt, under = make_long_skirt(figure, settings, points, rig)
            out.append(long_skirt)
            hidden |= under
    for kind, settings in character.pieces:
        if kind == "headscarf" and setting(settings, kind, "knot") > 0:
            material = next(pc.material for pc in out if pc.kind == kind)
            out.append(make_scarf_knot(figure, settings, material, rig))
            out.append(make_scarf_point(figure, settings, material, rig))
        if kind == "apron":
            if long_skirt is None:
                raise ConvertError("apron: it is tied over a skirt (a 'piece skirt' line)")
            out.append(make_apron(figure, settings, long_skirt))
    hide = {tuple(sorted(faces[fi].tolist())) for fi in np.flatnonzero(hidden)}
    return out, hide


def make_skirt(figure, kind, settings, points, material, rig, n=SEGMENTS):
    """A coat's skirt: a tube from just above the garment's bottom down to
    its hem, round the dressed hips and thighs, hanging straight and flared
    by ``flare`` at the hem, open at the front by ``open`` sectors each
    side. Its top rows go with the pelvis, its lower ones partly with the
    thigh of their side."""
    g = lambda key: setting(settings, kind, key)  # noqa: E731
    top, hem = g("bottom") + 0.3, g("skirt")
    if hem >= top:
        raise ConvertError(f"{kind}: the skirt's hem is above its waist")
    levels = np.linspace(top, hem, 4)
    ys = [figure.height(s) for s in levels]
    center = axis(points, ys[0] - 0.3, ys[0] + 0.3)
    rows, previous = [], None
    for i, y in enumerate(ys):
        r = outline(points, center, y - 0.3, y + 0.3, 0.03)
        r = r if previous is None else np.maximum(r, previous)
        previous = r
        rows.append(ring(center, r + g("flare") * (i / (len(ys) - 1)) ** 1.5, y))
    gap = int(g("open"))
    keep = list(range(gap + 1, n - gap)) if gap > 0 else list(range(n))
    coords = np.vstack([row[keep] for row in rows])
    m = len(keep)
    ids = [list(range(i * m, (i + 1) * m)) for i in range(len(rows))]
    tris = facing(coords, strip(ids, closed=gap == 0), radial(center))
    sx, _ = angles(n)
    w = np.zeros((len(coords), len(rig.order)))
    for i in range(len(rows)):
        share = 0.5 * (i / (len(rows) - 1)) * np.abs(sx[keep])
        w[i * m : (i + 1) * m, rig.order.index("pelvis")] = 1 - share
        for side, name in ((1, "thigh_l"), (-1, "thigh_r")):
            w[i * m : (i + 1) * m, rig.order.index(name)] += np.where(np.sign(sx[keep]) == side, share, 0.0)
    texco = np.array([[2.0 * k / n, 0.5 * i] for i in range(len(rows)) for k in keep])
    pc = Piece(f"{kind} skirt", kind, material, coords, [tuple((v, v) for v in t) for t in tris], w, texco)
    return pc


# The long skirt (pollera): how many directions round it, how many soft
# folds it hangs in, and the rows from its waistband to its hem (0 at the
# waist, 1 at the hem).
SKIRT_SEGMENTS = 32
SKIRT_FOLDS = 8
SKIRT_ROWS = (0.0, 0.06, 0.14, 0.25, 0.38, 0.52, 0.66, 0.8, 0.92, 1.0)
SKIRT_TRUNK = ("pelvis", "spine_01")
# The skirt's vertex layout, which the apron reads: rings of
# SKIRT_SEGMENTS vertices each, top down; the waistband's top and the
# SKIRT_ROWS (its outside, SKIRT_OUTER_RINGS), then the hem turned in.
SKIRT_OUTER_RINGS = len(SKIRT_ROWS) + 1
SKIRT_RINGS = SKIRT_OUTER_RINGS + 1
# The dressed body points a row of the skirt needs round its height.
SKIRT_MIN_POINTS = 8


def nearest_weights(coords, figure, joints, limbs=(BODY,)):
    """Each of ``coords``' skin weights as the body proxy's under it
    (``inherit``, from its trunk and legs, or ``limbs``)."""
    on = np.isin(figure.limb, limbs)
    body = Piece("", "", None, coords, [], np.zeros((len(coords), len(figure.rig.order))), None)
    under = type("Under", (), {})()
    under.rig, under.points, under.weights = figure.rig, figure.coords[on], figure.weights[on]
    w = inherit(body, under, joints).weights
    # No body point on those joints: rigid on the first of them there is.
    bare = w.sum(axis=1) < 1e-9
    if bare.any():
        first = next((j for j in joints if j in figure.rig.order), None)
        if first is None:
            raise ConvertError(f"the skeleton has none of {', '.join(joints)}")
        w[bare, figure.rig.order.index(first)] = 1.0
    return w


def make_long_skirt(figure, settings, points, rig, n=SKIRT_SEGMENTS):
    """An ankle-length skirt (pollera) gathered at the waist: a tube from
    its waistband (station ``top``) down to its hem (``hem``), round the
    dressed hips and legs together, hanging as a bell: below the hips it
    never narrows, and it widens by ``flare`` at the hem, in ``SKIRT_FOLDS``
    soft folds ``folds`` deep. Its hem turns in, so it reads as cloth with a
    thickness.

    The skin: the waistband goes with the trunk under it (``SKIRT_TRUNK``);
    lower down, each vertex takes up to ``follow`` of the thighs' turn, the
    share growing toward the hem and split between the two thighs by how
    far to each side of the middle it hangs (the front's middle takes half
    of each). A stride swings the skirt with the legs, short of them, so it
    neither tears between the legs nor lets a knee through.

    Returns the piece and the skin faces under it, which are left out: the
    trunk and legs from station ``hide`` up to its waistband."""
    k = "skirt"
    g = lambda key: setting(settings, k, key)  # noqa: E731
    top, hem = g("top"), g("hem")
    if not hem < top:
        raise ConvertError("skirt: its hem is above its waist")
    y_top, y_hem = figure.height(top), figure.height(hem)
    center = axis(points, y_top - 0.3, y_top + 0.3)
    sx, cz = angles(n)
    theta = np.arange(n) * (2 * math.pi / n)
    rows, previous = [], None
    hip = figure.height(3.0)
    tall = float(figure.heights[-1] - figure.heights[0])  # no row looks further than the figure
    for t in SKIRT_ROWS:
        y = y_top + t * (y_hem - y_top)
        # The proxy's shins have few rings: look wider until it finds some.
        reach = 0.25
        while np.count_nonzero(np.abs(points[:, 1] - y) <= reach) < SKIRT_MIN_POINTS:
            reach *= 2
            if reach > tall:
                raise ConvertError(f"skirt: fewer than {SKIRT_MIN_POINTS} body points to hang it round")
        r = outline(points, center, y - reach, y + reach, 0.06, n)
        if previous is not None and y < hip:
            r = np.maximum(r, previous)  # below the hips it hangs, never narrows
        previous = r
        # Rounder toward the hem: the cloth falls in a bell, not round the legs.
        round_ = np.maximum(r, (1 - 0.6 * t) * r + 0.6 * t * r.max())
        bell = g("flare") * t**1.6
        fold = g("folds") * t * (1 + np.cos(SKIRT_FOLDS * theta)) / 2
        rows.append(ring(center, round_ + bell + fold, y, n))
    # The waistband's top, and the hem turned in.
    band = ring(center, outline(points, center, y_top - 0.1, y_top + 0.3, 0.06, n), y_top + 0.25, n)
    inner = rows[-1].copy()
    inner[:, [0, 2]] = center[[0, 2]] + (inner[:, [0, 2]] - center[[0, 2]]) * 0.93
    inner[:, 1] += 0.08
    rows = [band] + rows + [inner]
    if len(rows) != SKIRT_RINGS:
        raise ConvertError("skirt: its rings are not the layout the apron reads")
    coords = np.vstack(rows)
    ids = [list(range(i * n, (i + 1) * n)) for i in range(len(rows))]
    tris = facing(coords, strip(ids[:-1]), radial(center))
    tris += facing(coords, strip(ids[-2:]), lambda p: np.array([0.0, -1.0, 0.0]))
    # The skin: the trunk's at the waist, then the thighs' share.
    w = nearest_weights(coords, figure, SKIRT_TRUNK)
    depth = np.clip((y_top - coords[:, 1]) / (y_top - y_hem), 0.0, 1.0)
    share = g("follow") * np.clip((depth - 0.05) / 0.5, 0.0, 1.0) ** 2 * (3 - 2 * np.clip((depth - 0.05) / 0.5, 0.0, 1.0))
    left, right = figure.joint("thigh_l"), figure.joint("thigh_r")
    half = max(abs(left[0] - right[0]) / 2, 0.1) * 1.5
    to_left = np.clip(0.5 + (coords[:, 0] - center[0]) / (2 * half), 0.0, 1.0)
    w *= (1 - share)[:, None]
    w[:, rig.order.index("thigh_l")] += share * to_left
    w[:, rig.order.index("thigh_r")] += share * (1 - to_left)
    # The back of the hem also takes some of the shins' turn: a heel coming
    # up behind lifts it instead of going through it.
    out = coords[:, [0, 2]] - center[[0, 2]]
    back = np.clip(-out[:, 1] / np.maximum(np.linalg.norm(out, axis=1), 1e-9), 0.0, 1.0)
    kick = g("kick") * back * np.clip((depth - 0.35) / 0.65, 0.0, 1.0)
    w *= (1 - kick)[:, None]
    w[:, rig.order.index("calf_l")] += kick * to_left
    w[:, rig.order.index("calf_r")] += kick * (1 - to_left)
    texco = np.array([[2.0 * j / n, 1.5 * i / (len(rows) - 1)] for i in range(len(rows)) for j in range(n)])
    material = plain(k, g("color"), ROUGHNESS["wool"], pattern="wool")
    pc = Piece("skirt", k, material, coords, [tuple((v, v) for v in t) for t in tris], w, texco)
    pc.rings = (SKIRT_RINGS, n)
    under = (figure.limb[figure.faces] == BODY).all(axis=1) & (figure.station[figure.faces] >= g("hide")).all(axis=1) & (
        figure.station[figure.faces] <= top).all(axis=1)
    return pc, under


def make_scarf_knot(figure, settings, material, rig):
    """The headscarf's knot under the chin, ``knot`` across, and its two
    short ends hanging from it; on the head."""
    top, chin, middle, height = head_measures(figure)
    head = figure.coords[figure.limb == HEAD]
    jaw = head[np.abs(head[:, 1] - chin) < 0.2]
    size = setting(settings, "headscarf", "knot")
    at = np.array([middle[0], chin - 0.6 * size, jaw[:, 2].max() - 0.15])
    knot = at + np.array([[size, 0, 0], [-size, 0, 0], [0, size * 0.7, 0], [0, -size * 0.7, 0], [0, 0, size * 0.8], [0, 0, -size * 0.6]])
    ktris = facing(knot, [(0, 2, 4), (2, 1, 4), (1, 3, 4), (3, 0, 4), (2, 0, 5), (1, 2, 5), (3, 1, 5), (0, 3, 5)], lambda p: p - at)
    parts = [(knot, ktris)]
    for side in (1, -1):
        e = at + np.array([[side * 0.3 * size, -0.4 * size, size * 0.7], [side * 0.9 * size, -0.3 * size, size * 0.5],
                           [side * 1.2 * size, -2.4 * size, size * 0.9]])
        parts.append((e, facing(e, [(0, 1, 2)], lambda p: np.array([0.0, 0.0, 1.0]))))
    coords, tris = joined(parts)
    return made("headscarf knot", material, coords, tris, "head", rig)


SCARF_POINT_FROM = ("head", "neck_01", "spine_03")


def make_scarf_point(figure, settings, material, rig):
    """The headscarf's folded point hanging down the back of the neck onto
    the shoulders, ``drop`` long, skinned like the body under it (the
    head, the neck and the chest's top)."""
    top, chin, middle, height = head_measures(figure)
    drop = setting(settings, "headscarf", "drop")
    loose = setting(settings, "headscarf", "loose")
    neck = figure.coords[(figure.limb != ARM) & (figure.normals[:, 2] < -0.2)]

    def back(x, y):
        near = neck[(np.abs(neck[:, 0] - x) < 0.3) & (np.abs(neck[:, 1] - y) < 0.3)]
        return (near[:, 2].min() if len(near) else neck[:, 2].min()) - loose - 0.03

    y0 = chin + 0.15 * height
    rows = []
    for t, half in ((0.0, 0.75), (0.5, 0.45), (1.0, 0.0)):
        y = y0 - t * drop
        xs = [middle[0] - half, middle[0], middle[0] + half] if half else [middle[0]]
        rows.append([[x, y, back(x, y)] for x in xs])
    coords = np.array([p for row in rows for p in row])
    tris = [(0, 1, 4), (0, 4, 3), (1, 2, 4), (2, 5, 4), (3, 4, 6), (4, 5, 6)]
    tris = facing(coords, tris, lambda p: np.array([0.0, 0.0, -1.0]))
    piece = Piece("headscarf point", "piece", material, coords, [tuple((v, v) for v in t) for t in tris],
                  np.zeros((len(coords), len(rig.order))), coords[:, [0, 1]] * 0.6)
    piece.weights = nearest_weights(coords, figure, SCARF_POINT_FROM, limbs=(BODY, HEAD))
    return piece


def make_apron(figure, settings, skirt):
    """An apron (delantal) tied over the long skirt: the skirt's front
    (``width``: the share of its front half it covers) from the waist down
    to station ``hem``, laid just outside it and skinned like it, with its
    own hem turned in."""
    k = "apron"
    g = lambda key: setting(settings, k, key)  # noqa: E731
    rings, n = getattr(skirt, "rings", (None, None))
    if rings != SKIRT_RINGS or n is None or len(skirt.coords) != rings * n:
        raise ConvertError("apron: the skirt is not laid out in the rings it reads")
    rows = SKIRT_OUTER_RINGS  # the waistband and the skirt's rows, not its turned-in hem
    y_hem = figure.height(g("hem"))
    sx, cz = angles(n)
    front = [j for j in range(n) if cz[j] > 1 - g("width") * 1.0 and cz[j] > 0]
    front = sorted(front, key=lambda j: sx[j])
    if len(front) < 2:
        raise ConvertError("apron: too narrow")
    coords = skirt.coords[: rows * n]
    center = np.array([coords[:, 0].mean(), 0.0, coords[:, 2].mean()])
    keep = [i for i in range(rows) if coords[i * n, 1] >= y_hem - 1e-9]
    if len(keep) < 2:
        raise ConvertError("apron: its hem is above the waist")
    # Cut to the hem: the last row kept slid down to it along the next one.
    picked, weights = [], []
    for i in keep:
        picked.append(coords[[i * n + j for j in front]])
        weights.append(skirt.weights[[i * n + j for j in front]])
    last = keep[-1]
    if last + 1 < rows and coords[last * n, 1] > y_hem:
        a, b = coords[[last * n + j for j in front]], coords[[(last + 1) * n + j for j in front]]
        f = (a[:, 1] - y_hem) / np.maximum(a[:, 1] - b[:, 1], 1e-9)
        picked.append(a + (b - a) * f[:, None])
        wa, wb = skirt.weights[[last * n + j for j in front]], skirt.weights[[(last + 1) * n + j for j in front]]
        weights.append(wa + (wb - wa) * f[:, None])
    out = []
    for p in picked:
        d = p - center
        d[:, 1] = 0.0
        out.append(p + d / np.maximum(np.linalg.norm(d, axis=1, keepdims=True), 1e-9) * 0.05)
    m = len(front)
    coords = np.vstack(out)
    ids = [list(range(i * m, (i + 1) * m)) for i in range(len(out))]
    tris = facing(coords, strip(ids, closed=False), radial(center))
    texco = np.array([[1.0 * j / m, 1.0 * i / len(out)] for i in range(len(out)) for j in range(m)])
    material = plain(k, g("color"), ROUGHNESS["cotton"], pattern="cotton")
    return Piece("apron", k, material, coords, [tuple((v, v) for v in t) for t in tris], np.vstack(weights), texco)


def cloth(name, size=64):
    """A small tiling grey texture, made in code and the same every time,
    that a material's colour tints: 'cotton' (a fine plain weave), 'wool'
    (a coarse twill), 'stripe' (the twill with dark and light stripes
    across v) and 'leather' (soft mottling); and 'poncho', in colour (dark
    red with cream bands across v). RGB, uint8."""
    y, x = np.mgrid[0:size, 0:size].astype(np.uint64)
    h = (x * np.uint64(73856093)) ^ (y * np.uint64(19349663))
    h = (h ^ (h >> np.uint64(13))) * np.uint64(1274126177)
    noise = ((h ^ (h >> np.uint64(16))) & np.uint64(0xFFFF)).astype(np.float64) / 65535.0

    def blur(a, times):
        for _ in range(times):
            a = (a + np.roll(a, 1, 0) + np.roll(a, -1, 0) + np.roll(a, 1, 1) + np.roll(a, -1, 1)) / 5
        return a

    xi, yi = x.astype(np.int64), y.astype(np.int64)
    if name == "cotton":
        v = 0.94 + 0.04 * ((xi + yi) % 2) + 0.04 * (noise - 0.5)
    elif name in ("wool", "stripe"):
        v = 0.86 + 0.1 * (((xi + yi) // 2) % 2) + 0.1 * (blur(noise, 1) - 0.5)
        if name == "stripe":
            rows = np.full(size, 0.86)
            for a, b, k in ((0.06, 0.15, 0.55), (0.2, 0.23, 1.15), (0.77, 0.8, 1.15), (0.85, 0.94, 0.55)):
                rows[int(a * size) : int(b * size)] = k
            v = v * rows[yi]
    elif name == "leather":
        soft = blur(noise, 6)
        v = 0.9 + 1.2 * (soft - soft.mean()) + 0.04 * (noise - 0.5)
    elif name == "poncho":
        # Woven bands across v, plain along u: dark red, cream bands edged
        # in black, a fine weave on top.
        red, cream, black = np.array([0.42, 0.07, 0.06]), np.array([0.84, 0.78, 0.64]), np.array([0.08, 0.06, 0.05])
        bands = np.tile(red, (size, 1))
        for a, b, colour in ((0.2, 0.27, cream), (0.17, 0.19, black), (0.28, 0.3, black), (0.47, 0.53, cream),
                             (0.73, 0.8, cream), (0.7, 0.72, black), (0.81, 0.83, black)):
            bands[int(a * size) : int(b * size)] = colour
        weave = 0.94 + 0.06 * ((xi + yi) % 2) + 0.03 * (noise - 0.5)
        return np.clip(np.rint(bands[yi] * weave[..., None] * 255), 0, 255).astype(np.uint8)
    else:
        raise ConvertError(f"no cloth texture {name!r}")
    grey = np.clip(np.rint(v * 255), 0, 255).astype(np.uint8)
    return np.repeat(grey[..., None], 3, axis=2)


# --- conversion -----------------------------------------------------------------


def data_roots(args):
    """The data folders searched for assets: the user's (when it exists),
    then the system's (which also holds the targets and rigs)."""
    data = os.path.realpath(os.path.expanduser(args.data or os.environ.get("MAKEHUMAN_DATA") or DEFAULT_DATA))
    if not os.path.isfile(os.path.join(data, "targets.npz")):
        raise ConvertError(f"{data} is not a MakeHuman data folder (no targets.npz)")
    user = os.path.realpath(os.path.expanduser(args.user_data or os.environ.get("MAKEHUMAN_USER_DATA") or DEFAULT_USER_DATA))
    roots = [user] if os.path.isdir(user) and user != data and not args.no_user_data else []
    return data, roots + [data]


def convert(args):
    data, roots = data_roots(args)
    mhm = os.path.realpath(args.input)
    character = read_mhm(mhm)
    files = Files(roots, os.path.dirname(mhm))
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
    index = proxy_index(roots)
    proxies = []
    for kind, name, uuid in character.proxies:
        path = index.get((kind, uuid))
        if path is None:
            raise ConvertError(f"{kind} {name!r} ({uuid}) not found in {PROXY_KINDS[kind]}/")
        proxies.append(Proxy(path, kind, len(base), files))
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
        if not tris and p is not body:
            report.append(f"  {p.name}: every face hidden")
            continue
        if p.weights_file:
            custom = read_weights(files.find(p.weights_file, [os.path.dirname(p.path)]), p.vertex_count)
            w, unweighted = remap_weights(rig, refs, custom, p.vertex_count)
        else:
            w, unweighted = p.bone_weights(human)
        if unweighted:
            report.append(f"  {p.name}: {unweighted} vertices without weights, on the root bone")
        # The skin and the pack clothes that cover the body get the large
        # textures; shoes, hair and the face's bits the small ones.
        large = p is body or p.kind == "clothes" and p.z_depth >= 50 and p.delete is not None
        pc = Piece(p.kind if p is body else p.name, p.kind, material, p.fit(coords), tris, w, p.texco, large)
        if p is body:
            # Every face, for the garments cut from it (dress()).
            pc.all_triangles = p.triangles()[0]
            skin = pc
        target = character.decimate.get(p.uuid)
        if target and len(tris) > target:
            pc.coords, pc.triangles, pc.weights = decimate(pc.coords, pc.triangles, pc.weights, target)
            report.append(f"  {p.name}: decimated from {len(tris)} to {len(pc.triangles)} triangles")
        pieces.append(pc)

    # The arms lowered into the rest pose; the garments cut from the posed
    # body, which hides the skin under them; then the generated pieces
    # fitted to the dressed body.
    rest = pose_arms(rig, rest, pieces)
    garments, covered = dress(character, Figure(rig, rest, skin), rig)
    if covered:
        before = len(skin.triangles)
        skin.triangles = [t for t in skin.triangles if tuple(sorted(v for v, _ in t)) not in covered]
        report.append(f"  {skin.label}: {before - len(skin.triangles)} triangles under the garments left out")
    pieces = [pc for pc in pieces if pc.triangles] + garments
    for pc in garments:
        report.append(f"  {pc.label}: cut from the body, {len(pc.triangles)} triangles")
    made_pieces, band = generate(character, Body(rig, rest, pieces), rig)

    # Hats: hair above a hat's band is under its crown, and would poke
    # through it (MakeHuman leaves that to the user). Not in MakeHuman.
    if band is not None and not args.keep_hair_under_hats:
        for pc in pieces:
            if pc.kind == "hair":
                low = pc.coords[:, 1] < band
                kept = [t for t in pc.triangles if any(low[v] for v, _ in t)]
                report.append(f"  {pc.label}: {len(pc.triangles) - len(kept)} triangles under the hat left out")
                pc.triangles = kept
        pieces = [pc for pc in pieces if pc.triangles]
    pieces += made_pieces
    for pc in made_pieces:
        report.append(f"  {pc.label}: generated, {len(pc.triangles)} triangles")

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

    def texture(path, normal_map, limit, grey=False, pattern=None):
        key = (path, normal_map, grey, pattern)
        if key in images:
            return images[key]
        stem = pattern or os.path.splitext(os.path.basename(path))[0]
        name = re.sub(r"[^A-Za-z0-9._-]", "_", stem) + ".png"
        taken = {img["uri"] for img in gltf["images"]}
        n = 2
        while "textures/" + name in taken:
            name = f"{stem}_{n}.png"
            n += 1
        img = cloth(pattern) if pattern else png_decode(path)
        if grey:
            img = greyscale(img)
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
        limit = args.max_texture if pc.large else args.max_small_texture
        factor = [linear(c) for c in m.color[:3]] + [m.color[3]]
        pbr = {"baseColorFactor": [round(c, 6) for c in factor], "metallicFactor": round(m.metallic, 3), "roughnessFactor": round(m.roughness, 3)}
        if m.diffuse:
            pbr["baseColorTexture"] = {"index": texture(m.diffuse, False, limit, m.greyscale)}
        elif m.pattern:
            pbr["baseColorTexture"] = {"index": texture(None, False, limit, pattern=m.pattern)}
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
        uvs = pc.texco[t].copy()
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
        raw = b""
        if target and os.path.isfile(target):
            with open(target, "rb") as f:
                raw = f.read()
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
    parser.add_argument("--user-data", help=f"MakeHuman's user data folder, searched first (default: $MAKEHUMAN_USER_DATA, else {DEFAULT_USER_DATA}, when it exists)")
    parser.add_argument("--no-user-data", action="store_true", help="search the system data folder only")
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
