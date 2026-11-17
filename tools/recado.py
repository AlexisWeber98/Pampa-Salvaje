"""Model the recado's seat (the two bastos, the cojinillo of curly long wool
and the sobrepuesto of carpincho) and the bridle (the cabezada of braided
rawhide and the curb bit) on the game's criollo horse, in Blender, by
script, and skin them to its rig.

    blender -b --factory-startup --python tools/recado.py -- OUT.glb \\
        --textures assets/models/tack [--preview DIR]
    FORCE=1 sh tools/textures_to_ktx2.sh assets/models/tack/tack_recado assets/models/tack/tack_wool

It loads henry's horse (``assets/models/fauna/henry/horse.glb``) standing
as in the first frame of ``Idle_1`` (``REST_CLIP``: its glTF rest pose has the
head down by the hooves) and builds every piece on its skin, in the frame the
game fits its own tack in (``pampa_render::character::tack``: +X the horse's
front, +Y up, +Z its right, in game metres, ``SCALE`` times the file's; in
Blender, after the glTF import, that is -Y, +Z and -X). The exported
armature rests in that pose, so the game fits its own tack on the same
standing horse (``tack::Recado::pose``).

- the **bastos**: two stuffed suela rolls along the spine, resting on the
  carona (``UNDER`` off the skin, where the game's carona ends), longer than
  the cojinillo so their round, tooled ends show front and back, with a
  stitched seam down each;
- the **cojinillo**: a sheepskin draped over them, its wool thick over the
  rolls and the spine and thinning to the hem, its front and back curving
  down, with soft lumps in its surface and the curls of the wool in its maps,
  plus clumps of long wool cards hanging unevenly from its hem and ends and
  short tufts on its exposed top (random places, lengths and curls: never in
  rows);
- the **sobrepuesto**: carpincho laid over the middle of the cojinillo, its
  wool pressed flat under it, following the two rolls: the seat;
- the **cabezada**: testera over the poll behind the ears, frontalera across
  the forehead below the ears and above the eyes, a quijera down each cheek
  behind and below the eye to the bit, the muserola round the nose below the
  cheekbones, and the fiador under the throat, each a flat strap laid on the
  skin;
- the **freno**: the mouthpiece across the mouth at the corners of the lips,
  a curved iron shank flat against the lips on each side (a top eye for the
  cheek piece, a ring below for the reins) and the barbada under the chin
  groove.

Every piece is skinned like the horse's skin underneath it
(``animal_rig.transfer_weights``), so it bends with every clip; then every
clip a saddled horse plays (``SADDLED_CLIPS``) is posed in numpy exactly as
the game poses it (``Glb``) and each vertex the moving skin came too close
to is lifted off it. Small
marker meshes (``Mark_ReinL``, ``Mark_ReinR``, ``Mark_Seat``) tell the game
where the reins start and where the rider sits; it does not draw them.

Materials: ``Recado`` (the cojinillo, the bastos and the sobrepuesto, on one
1024² atlas: ``tack_recado``), ``RecadoWool`` (the alpha-tested wool cards:
``tack_wool``, 512²), ``Cabezada`` (the game's ``tack_rawhide`` braid) and
``Freno`` (``tack_iron``). The atlas is painted here with numpy over the
pieces' own UVs (the curls of the wool, the leather with its seams and
tooled ends, the carpincho), from the same functions as ``tools/tack.py``.
``--preview DIR`` renders PNGs of the recado on the horse.

The part above ``build`` needs only numpy (``tools/test_recado.py``).
"""

import math
import os
import sys

import numpy as np

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import tack as T  # noqa: E402

HERE = os.path.dirname(os.path.abspath(__file__))
HORSE = os.path.join(HERE, "..", "assets", "models", "fauna", "henry", "horse.glb")
# The game draws henry's horse at 1.5 m to the withers, 1.60 in its file
# (``livestock::HORSE_WITHERS``): the body frame is the file's times this.
SCALE = 1.5 / 1.60
# The pose the recado is modelled on and its armature rests in: the horse
# standing, at the start of its idle.
REST_CLIP = "Idle_1"
# The clips a saddled horse plays (never ``Eat``: saddled horses do not
# graze; ``Jump_Land`` is not used).
SADDLED_CLIPS = ("Idle_1", "Idle_2", "Idle_3", "Walk", "Gallop", "Jump", "S_Jump", "Jump_Run")
FPS = 24
# The ones that are not the jump.
GAITS = tuple(c for c in SADDLED_CLIPS if "Jump" not in c)

# ---------------------------------------------------------------------------
# Profiles, in the game's body frame: ``t`` metres along the barrel axis
# (0 at the girth, + forward), ``theta`` radians round it from the top
# (+ to the right), heights in metres off the skin along the ray.

# The barrel axis (game ``Axis::BARREL``): from the girth, a hand behind the
# lowest point of the back and about four behind the elbow, at the height of
# the middle of the barrel (top 1.459 m, belly 0.870 m there); and a mean
# radius of the barrel under the saddle, to turn angles into metres.
GIRTH_X = 0.1
AXIS_HEIGHT = 1.19
REACH = 0.33
# Where the game's carona ends: the recado rests on it.
UNDER = 0.062
BASTO_THETA = 0.36
BASTO_R = 0.034
BASTO_ALONG = (-0.162, 0.255)
COJINILLO_ALONG = (-0.14, 0.21)
COJINILLO_AROUND = 1.38
SOBREPUESTO_ALONG = (-0.11, 0.18)
SOBREPUESTO_AROUND = 0.8
SOBREPUESTO_THICK = 0.004
# Where the rider sits (game ``tack::SEAT_X``).
SEAT_X = -0.02


# ---------------------------------------------------------------------------
# The horse posed as the game poses it: the GLB's nodes, skin and clips, in
# numpy (glTF semantics: linear keys, slerped rotations, linear blend
# skinning). Points come out in the file's frame (x the horse's left, y up,
# z forward, file metres).

_COMPONENTS = {5126: np.float32, 5125: np.uint32, 5123: np.uint16, 5121: np.uint8}
_WIDTH = {"SCALAR": 1, "VEC2": 2, "VEC3": 3, "VEC4": 4, "MAT4": 16}


def quat_matrix(q):
    x, y, z, w = q
    return np.array(
        [
            [1 - 2 * (y * y + z * z), 2 * (x * y - z * w), 2 * (x * z + y * w)],
            [2 * (x * y + z * w), 1 - 2 * (x * x + z * z), 2 * (y * z - x * w)],
            [2 * (x * z - y * w), 2 * (y * z + x * w), 1 - 2 * (x * x + y * y)],
        ]
    )


def slerp(a, b, f):
    a, b = np.asarray(a, float), np.asarray(b, float)
    d = float(a @ b)
    if d < 0:
        b, d = -b, -d
    if d > 0.9995:
        q = a + (b - a) * f
    else:
        th = math.acos(d)
        q = (math.sin((1 - f) * th) * a + math.sin(f * th) * b) / math.sin(th)
    return q / np.linalg.norm(q)


class Glb:
    """A skinned GLB (one skin), posed like the game poses it."""

    def __init__(self, path):
        import json
        import struct

        with open(path, "rb") as f:
            data = f.read()
        size = struct.unpack("<I", data[12:16])[0]
        self.doc = json.loads(data[20 : 20 + size])
        at = 20 + size
        self.bin = data[at + 8 : at + 8 + struct.unpack("<I", data[at : at + 4])[0]]
        self.nodes = self.doc["nodes"]
        self.names = [n.get("name", "") for n in self.nodes]
        self.parent = {c: i for i, n in enumerate(self.nodes) for c in n.get("children", [])}
        skin = self.doc["skins"][0]
        self.joints = skin["joints"]
        self.ibm = self._read(skin["inverseBindMatrices"]).reshape(-1, 4, 4).transpose(0, 2, 1)
        self.prims = []
        for mesh in self.doc["meshes"]:
            for p in mesh["primitives"]:
                a = p["attributes"]
                self.prims.append(
                    (
                        self.doc["materials"][p["material"]]["name"],
                        self._read(a["POSITION"]).astype(float),
                        self._read(a["JOINTS_0"]).astype(int),
                        self._read(a["WEIGHTS_0"]).astype(float),
                    )
                )
        self.clips = {a["name"]: a for a in self.doc.get("animations", [])}

    def _read(self, i):
        a = self.doc["accessors"][i]
        view = self.doc["bufferViews"][a["bufferView"]]
        kind, width = _COMPONENTS[a["componentType"]], _WIDTH[a["type"]]
        at = view.get("byteOffset", 0) + a.get("byteOffset", 0)
        out = np.frombuffer(self.bin, kind, a["count"] * width, at)
        if a.get("normalized") and kind != np.float32:
            out = out / np.iinfo(kind).max
        return out.reshape(a["count"], width) if width > 1 else out

    def duration(self, clip):
        a = self.clips[clip]
        return max(float(self._read(s["input"])[-1]) for s in a["samplers"])

    def worlds(self, clip=None, time=0.0):
        """Every node's world matrix (4x4, file frame) at ``time`` of
        ``clip``; ``None``: the nodes' rest."""
        local = [
            [
                np.array(n.get("translation", (0, 0, 0)), float),
                np.array(n.get("rotation", (0, 0, 0, 1)), float),
                np.array(n.get("scale", (1, 1, 1)), float),
            ]
            for n in self.nodes
        ]
        if clip is not None:
            a = self.clips[clip]
            for ch in a["channels"]:
                s = a["samplers"][ch["sampler"]]
                times, values = self._read(s["input"]), self._read(s["output"])
                k = int(np.clip(np.searchsorted(times, time), 1, len(times) - 1))
                f = float(np.clip((time - times[k - 1]) / max(times[k] - times[k - 1], 1e-9), 0, 1))
                path = ch["target"]["path"]
                slot = {"translation": 0, "rotation": 1, "scale": 2}[path]
                if path == "rotation":
                    local[ch["target"]["node"]][slot] = slerp(values[k - 1], values[k], f)
                else:
                    local[ch["target"]["node"]][slot] = values[k - 1] * (1 - f) + values[k] * f
        out = [None] * len(self.nodes)

        def world(i):
            if out[i] is None:
                t, r, sc = local[i]
                m = np.eye(4)
                m[:3, :3] = quat_matrix(r) * sc
                m[:3, 3] = t
                p = self.parent.get(i)
                out[i] = m if p is None else world(p) @ m
            return out[i]

        return [world(i) for i in range(len(self.nodes))]

    def skinned(self, worlds, materials=None):
        """The vertices of the primitives with ``materials`` (all: ``None``)
        skinned by the node ``worlds``, in primitive order."""
        skin = np.stack([worlds[j] @ self.ibm[k] for k, j in enumerate(self.joints)])
        out = []
        for name, pos, joints, weights in self.prims:
            if materials is not None and name not in materials:
                continue
            ph = np.c_[pos, np.ones(len(pos))]
            acc = np.zeros((len(pos), 3))
            for c in range(4):
                acc += weights[:, c : c + 1] * np.einsum("nij,nj->ni", skin[joints[:, c]], ph)[:, :3]
            out.append(acc)
        return np.concatenate(out)

    def joint_world(self, name, worlds):
        return worlds[self.names.index(name)]


def to_body(raw):
    """File points (x the horse's left, y up, z forward) in the game's body
    frame (``livestock::body_point``)."""
    raw = np.asarray(raw, float)
    return np.stack([raw[..., 2], raw[..., 1], -raw[..., 0]], -1) * SCALE


def _smooth(e0, e1, x):
    t = np.clip((x - e0) / (e1 - e0), 0.0, 1.0)
    return t * t * (3 - 2 * t)


def basto_radius(t):
    """A basto's radius along it: full, rounding over a short shoulder into
    the flat cap at each end (the cap is two thirds of the full radius)."""
    e = np.clip(np.minimum(t - BASTO_ALONG[0], BASTO_ALONG[1] - t) / 0.012, 0.0, 1.0)
    return BASTO_R * (2 / 3 + 1 / 3 * np.sqrt(1.0 - (1.0 - e) ** 2))


def _ends(t, lo, hi, run):
    """A quarter circle from 0 at the ends of ``lo..hi`` to 1 at ``run``
    in from them."""
    e = np.clip(np.minimum(t - lo, hi - t) / run, 0.0, 1.0)
    return np.sqrt(1.0 - (1.0 - e) ** 2)


def basto_envelope(t, theta):
    """How far above ``UNDER`` the bastos (and the wool draped from them)
    reach: each roll's round top, sloping off to the sides."""
    t, theta = np.asarray(t, float), np.asarray(theta, float)
    out = np.zeros(np.broadcast(t, theta).shape)
    inside = (t >= BASTO_ALONG[0]) & (t <= BASTO_ALONG[1])
    r = np.where(inside, basto_radius(np.clip(t, *BASTO_ALONG)), 0.0)
    for side in (-1.0, 1.0):
        d = np.abs(theta - side * BASTO_THETA) * REACH
        top = BASTO_R + np.sqrt(np.clip(r * r - d * d, 0.0, None))
        # The wool rolls over each basto and falls softly to the sides.
        slope = (BASTO_R + r) * (1.0 - _smooth(0.0, 0.05, d - r)) * (r > 0)
        out = np.maximum(out, np.where(d < r, top, slope))
    return out


def cojinillo(t, theta):
    """The cojinillo's underside and its wool's (smooth) top. Between the
    bastos the wool is packed (a folded fleece fills the channel over the
    spine), so the seat is a gentle dome with the two rolls just showing:
    the rider sits on them, not between them."""
    t, theta = np.asarray(t, float), np.asarray(theta, float)
    inside = (t >= BASTO_ALONG[0]) & (t <= BASTO_ALONG[1])
    r = np.where(inside, basto_radius(np.clip(t, *BASTO_ALONG)), 0.0)
    a_m = np.abs(theta) * REACH
    fill = 1.4 * r * (1.0 - _smooth(BASTO_THETA * REACH, BASTO_THETA * REACH + 0.05, a_m))
    inner = UNDER + np.maximum(basto_envelope(t, theta), fill)
    a = np.clip(np.abs(theta) / COJINILLO_AROUND, 0.0, 1.0)
    # Thick over the seat, thinner down the flanks where the thighs lie.
    wool = 0.008 + 0.034 * np.cos(a * math.pi / 2) ** 1.8
    end = np.maximum(_ends(t, *COJINILLO_ALONG, 0.09), 0.15)
    return inner, inner + np.maximum(wool * end, 0.005)


def sobrepuesto(t, theta):
    """The sobrepuesto's underside: on the cojinillo's smooth top."""
    return cojinillo(t, theta)[1] + 0.001


# ---------------------------------------------------------------------------
# The atlas (game UVs: u right, v down; rows top down) and the wool cards.

REGIONS = {
    # u0, v0, u1, v1. The cojinillo: u round the barrel, v along it.
    "cojinillo": (0.0, 0.0, 1.0, 0.6),
    # Each basto: u along it, v round it.
    "basto_l": (0.0, 0.6, 0.5, 0.8),
    "basto_r": (0.5, 0.6, 1.0, 0.8),
    # The sobrepuesto: u round the barrel, v along it.
    "sobrepuesto": (0.0, 0.8, 1.0, 1.0),
}

WOOL_COLUMNS = 8


def _region(size, name):
    u0, v0, u1, v1 = REGIONS[name]
    return slice(int(round(v0 * size)), int(round(v1 * size))), slice(int(round(u0 * size)), int(round(u1 * size)))


def _resample(img, h, w):
    """``img`` resized to ``h`` × ``w`` by nearest texel."""
    rows = (np.arange(h) * img.shape[0] / h).astype(int)
    cols = (np.arange(w) * img.shape[1] / w).astype(int)
    return img[rows][:, cols]


def _tile(img, h, w):
    """``img`` repeated to fill ``h`` × ``w``."""
    reps = (h // img.shape[0] + 1, w // img.shape[1] + 1) + ((1,) if img.ndim == 3 else ())
    return np.tile(img, reps)[:h, :w]


def curls(h, w, scale, seed):
    """The relief of curly wool on an ``h`` × ``w`` image, ``scale``
    texels a millimetre: packed ringlets a centimetre or so across, each a
    rounded lock with a spiral groove, and their colour mask (1 on top)."""
    rng = np.random.default_rng(seed)
    height = np.zeros((h, w))
    radius = max(1.2, 7.0 * scale)
    count = int(2.4 * h * w / (math.pi * radius * radius))
    r = int(math.ceil(radius * 1.6))
    oy, ox = np.mgrid[-r : r + 1, -r : r + 1]
    for _ in range(count):
        cy, cx = rng.random() * h, rng.random() * w
        rad = radius * rng.uniform(0.7, 1.4)
        iy, ix = int(cy), int(cx)
        dy, dx = oy + iy - cy, ox + ix - cx
        d = np.sqrt(dx * dx + dy * dy) / rad
        ang = np.arctan2(dy, dx)
        spiral = 0.8 + 0.2 * np.cos(2 * ang + d * 7.0 + rng.uniform(0, 6.3))
        lock = np.clip(1 - d * d, 0, 1) ** 0.5 * spiral + 0.05 * rng.random()
        rows, cols = (oy + iy) % h, (ox + ix) % w
        np.maximum.at(height, (rows, cols), lock)
    return height


def atlas(size):
    """The ``tack_recado`` maps: colour, height, roughness and AO arrays
    (rows top down)."""
    colour = np.zeros((size, size, 3))
    height = np.zeros((size, size))
    rough = np.ones((size, size)) * 0.9
    ao = np.ones((size, size))
    # The cojinillo: creamy curls, darker in the crevices, a little dirty.
    rs, cs = _region(size, "cojinillo")
    h, w = rs.stop - rs.start, cs.stop - cs.start
    hc = curls(h, w, size / 1024, 81)
    dirt = T.fbm(max(h, w), 5, 82, 4)[:h, :w]
    top = _smooth(0.15, 0.85, hc)
    c = T.mix((0.64, 0.56, 0.43), (0.87, 0.80, 0.65), top)
    c = T.mix(c, c * np.array((0.82, 0.74, 0.62)), _smooth(0.5, 0.8, dirt) * 0.7)
    colour[rs, cs] = np.clip(c, 0, 0.92)
    height[rs, cs] = hc
    rough[rs, cs] = 0.85 + 0.12 * (1 - top)
    ao[rs, cs] = 0.5 + 0.5 * top
    # The bastos: suela, a stitched seam down the middle of each roll and a
    # tooled double ring near each end.
    leather = T.leather(size)
    mid = leather["color"][size // 5 : 4 * size // 5, size // 5 : 4 * size // 5]
    mid_h = leather["height"][size // 5 : 4 * size // 5, size // 5 : 4 * size // 5]
    for name in ("basto_l", "basto_r"):
        rs, cs = _region(size, name)
        h, w = rs.stop - rs.start, cs.stop - cs.start
        c = _resample(mid, h, w).copy()
        hh = 0.2 * _resample(mid_h, h, w)
        y = (np.arange(h)[:, None] + 0.5) / h
        x = (np.arange(w)[None, :] + 0.5) / w
        px = 1.0 / h
        seam = 1 - _smooth(0.6 * px, 2.0 * px, np.abs(y - 0.5))
        stitch = (np.abs(y - 0.5) < 5 * px) & (np.abs(y - 0.5) > 2 * px) & ((x * w / max(2, w // 60)) % 1.0 < 0.5)
        rings = np.zeros((1, w))
        for at in (0.05, 0.075, 0.925, 0.95):
            rings = np.maximum(rings, 1 - _smooth(0.6 / w, 2.5 / w, np.abs(x - at)))
        hh = hh - 0.6 * seam + 0.35 * stitch - 0.6 * rings
        c = T.mix(c, (0.22, 0.13, 0.07), np.maximum(seam * 0.8, rings * 0.8))
        c = T.mix(c, (0.72, 0.62, 0.45), stitch * 0.8)
        colour[rs, cs] = c
        height[rs, cs] = hh
        rough[rs, cs] = 0.6
        ao[rs, cs] = 1 - 0.4 * np.maximum(seam, rings)
    # The sobrepuesto: the carpincho's pebbled grain, its edges darkened.
    rs, cs = _region(size, "sobrepuesto")
    h, w = rs.stop - rs.start, cs.stop - cs.start
    carp = T.carpincho(max(64, size // 2), edges=False)
    c = _tile(carp["color"], h, w)
    hh = _tile(carp["height"], h, w)
    yy = (np.arange(h)[:, None] + 0.5) / h
    xx = (np.arange(w)[None, :] + 0.5) / w
    d = np.minimum(np.minimum(xx * w / h, (1 - xx) * w / h), np.minimum(yy, 1 - yy))
    edge = 1 - _smooth(0.0, 0.18, d)
    colour[rs, cs] = T.mix(c, c * 0.55, edge)
    height[rs, cs] = hh
    rough[rs, cs] = 0.75 - 0.1 * edge
    return {
        "color": np.clip(colour, 0, 1),
        "normal_gl": T.normals(height, 2.5 * size / 1024 + 1.0),
        "roughness": np.clip(rough, 0, 1),
        "ao": np.clip(ao, 0, 1),
        "height": height,
    }


def wool_atlas(size):
    """The ``tack_wool`` maps for the wool cards: ``WOOL_COLUMNS`` broad
    locks hanging from the top of the image, each its own length: many curly
    strands gathered into one wavy clump that frays at the tip into strands
    of uneven length (a soft fringe, never a point). Shaded like the fleece:
    darker at the root (its occlusion), creamy along the lock, some tips
    dirtier (rows top down)."""
    rng = np.random.default_rng(91)
    w = size // WOOL_COLUMNS
    colour = np.zeros((size, size, 3))
    alpha = np.zeros((size, size))
    height = np.zeros((size, size))
    v = (np.arange(size)[:, None] + 0.5) / size
    u = (np.arange(w)[None, :] + 0.5) / w
    for c in range(WOOL_COLUMNS):
        length = rng.uniform(0.55, 0.95)
        sway, freq, phase = rng.uniform(0.06, 0.14), rng.uniform(1.2, 2.2), rng.uniform(0, 6.3)
        # The lock's wave: the whole clump swings side to side down its length.
        centre = 0.5 + sway * np.sin(freq * v * 6.28 + phase) * np.clip(v * 2, 0, 1)
        col_h = np.zeros((size, w))
        for _ in range(60):
            x0 = rng.uniform(-0.38, 0.38)
            own = length * rng.uniform(0.55, 1.0)
            curl, cphase = rng.uniform(0.01, 0.03), rng.uniform(0, 6.3)
            # Strands start spread across the root and gather into the lock.
            path = centre + x0 * (1 - 0.55 * np.clip(v / own, 0, 1)) + curl * np.sin(v * rng.uniform(25, 45) + cphase)
            width = rng.uniform(0.03, 0.06) * (1 - 0.6 * np.clip(v / own, 0, 1))
            dist = np.abs(u - path) / width
            on = (dist < 1) & (v < own)
            col_h = np.maximum(col_h, np.where(on, (1 - dist * dist) * (0.75 + 0.25 * rng.random()), 0.0))
        sl = slice(c * w, (c + 1) * w)
        height[:, sl] = col_h
        alpha[:, sl] = (col_h > 0.08).astype(float)
        tip = np.clip(v / length, 0, 1)
        dirty = rng.uniform(0.0, 0.6)
        cc = T.mix((0.55, 0.47, 0.36), (0.86, 0.79, 0.64), _smooth(0.0, 0.35, tip))
        cc = T.mix(cc, (0.58, 0.48, 0.35), _smooth(0.65, 1.0, tip) * dirty)
        colour[:, sl] = cc * (0.8 + 0.2 * col_h[..., None])
    mean = colour[alpha > 0.5].mean(axis=0) if (alpha > 0.5).any() else np.array((0.8, 0.7, 0.6))
    colour[alpha < 0.5] = mean
    return {
        "color": np.clip(colour, 0, 0.92),
        "normal_gl": T.normals(height, 1.5),
        "roughness": np.full((size, size), 0.9),
        "ao": np.clip(0.55 + 0.45 * np.clip(v * 3, 0, 1) * np.ones_like(height), 0, 1),
        "opacity": alpha,
        "height": height,
    }


def write_maps(out):
    """Writes the JPEG sources of ``tack_recado`` and ``tack_wool``."""
    for name, maps in (("tack_recado", atlas(1024)), ("tack_wool", wool_atlas(512))):
        folder = os.path.join(out, name)
        os.makedirs(folder, exist_ok=True)
        for key in ("color", "normal_gl", "roughness", "ao", "opacity"):
            if key in maps:
                T.save(maps[key], os.path.join(folder, f"{key}.jpg"))
        print(f"recado: wrote {folder}")


# ---------------------------------------------------------------------------
# Blender: the pieces on the horse.


def build(out_glb, textures=None, preview=None):
    import bpy
    from mathutils import Matrix, Vector
    from mathutils.bvhtree import BVHTree

    import animal_rig as rig

    rig.reset()
    # The nodes' own rest (henry's bind pose is not one the game ever shows).
    bpy.ops.import_scene.gltf(filepath=os.path.abspath(HORSE), guess_original_bind_pose=False)
    arm = next(o for o in bpy.data.objects if o.type == "ARMATURE")
    body = next(o for o in bpy.data.objects if o.type == "MESH")
    if not (_identity(arm.matrix_world) and _identity(body.matrix_world)):
        raise SystemExit("recado: the horse's armature and body must sit at the origin")
    # The armature rests standing as at the start of REST_CLIP, and the body
    # stands so with it, in game metres (``_scaled``) while the pieces are
    # built; they go back to the file's metres before the export.
    horse = Glb(HORSE)
    stand = horse.worlds(REST_CLIP, 0.0)
    _rest_on(arm, horse, stand)
    arm.data.pose_position = "REST"
    co = _scaled(horse.skinned(stand))
    if len(co) != len(body.data.vertices):
        raise SystemExit("recado: the horse's vertices are not the GLB's")
    body.data.vertices.foreach_set("co", co.ravel())
    body.data.update()
    skin_mats = SKIN
    mats = [s.material.name if s.material else "" for s in body.material_slots]
    mesh = body.data
    verts = [body.matrix_world @ v.co for v in mesh.vertices]
    polys = [tuple(p.vertices) for p in mesh.polygons if mats[p.material_index] in skin_mats]
    bvh = BVHTree.FromPolygons(verts, polys)

    def blender(t, up, right):
        """A game body-frame point (forward, up, right) in Blender."""
        return Vector((-right, -t, up))

    def ray(t, theta):
        origin = Vector((0.0, -(GIRTH_X + t), AXIS_HEIGHT))
        d = Vector((-math.sin(theta), 0.0, math.cos(theta)))
        hit = bvh.ray_cast(origin, d)
        return (hit[0] if hit[0] is not None else origin + d * 0.3), d

    def on_skin(p, lift):
        loc, n, _, _ = bvh.find_nearest(p)
        return loc + n * lift, n

    def out_of_skin(p, gap):
        loc, n, _, dist = bvh.find_nearest(p)
        inside = (p - loc).dot(n) < 0
        if inside or dist < gap:
            return loc + n * gap
        return p

    objects = []

    def make(name, verts, faces, uvs, material):
        m = bpy.data.meshes.new(name)
        m.from_pydata([tuple(v) for v in verts], [], faces)
        m.update()
        layer = m.uv_layers.new(name="UVMap")
        for poly in m.polygons:
            poly.use_smooth = True
            for li in poly.loop_indices:
                u, v = uvs[m.loops[li].vertex_index]
                layer.data[li].uv = (u, 1.0 - v)
        obj = bpy.data.objects.new(name, m)
        bpy.context.collection.objects.link(obj)
        mat = bpy.data.materials.get(material) or bpy.data.materials.new(material)
        obj.data.materials.append(mat)
        objects.append(obj)
        return obj

    def region_uv(name, a, b):
        u0, v0, u1, v1 = REGIONS[name]
        return (u0 + (u1 - u0) * a, v0 + (v1 - v0) * b)

    def grid_shell(name, along, around, cells, top, bottom, region, swap, corner=0.0):
        """A shell over the barrel between ``bottom(t, theta)`` and
        ``top(t, theta)`` (heights off the skin): its top face and the
        walls round its edge, UVs on ``region`` (``swap``: u round, v
        along)."""
        na, nr = cells
        rows = []
        for i in range(na + 1):
            row = []
            for k in range(nr + 1):
                a, b = i / na, k / nr
                t = along[0] + (along[1] - along[0]) * a
                th = around[0] + (around[1] - around[0]) * b
                if corner > 0:
                    t, th = _round_corner(t, th, along, around, corner)
                p, d = ray(t, th)
                uv = region_uv(region, b, a) if swap else region_uv(region, a, b)
                row.append((p + d * float(top(t, th)), p + d * float(bottom(t, th)), uv))
            rows.append(row)
        verts, faces, uvs = [], [], []
        index = {}
        for i in range(na + 1):
            for k in range(nr + 1):
                outer, inner, uv = rows[i][k]
                index[(0, i, k)] = len(verts)
                verts.append(outer)
                uvs.append(uv)
                index[(1, i, k)] = len(verts)
                verts.append(inner)
                uvs.append(uv)
        for i in range(na):
            for k in range(nr):
                a, b, c, d = (index[(0, i, k)], index[(0, i + 1, k)], index[(0, i + 1, k + 1)], index[(0, i, k + 1)])
                faces.append((a, b, c, d))
        ring = [(i, 0) for i in range(na + 1)] + [(na, k) for k in range(1, nr + 1)]
        ring += [(i, nr) for i in range(na - 1, -1, -1)] + [(0, k) for k in range(nr - 1, 0, -1)]
        for (i0, k0), (i1, k1) in zip(ring, ring[1:] + ring[:1]):
            faces.append((index[(0, i0, k0)], index[(1, i0, k0)], index[(1, i1, k1)], index[(0, i1, k1)]))
        obj = make(name, verts, faces, uvs, "Recado")
        _outward(obj)
        return obj

    # --- the bastos ------------------------------------------------------
    for side, name in ((1.0, "basto_l"), (-1.0, "basto_r")):
        nr = 16
        # Dense at the ends for the rounded shoulder into the flat cap.
        span = BASTO_ALONG[1] - BASTO_ALONG[0]
        cuts = [0.0, 0.004, 0.012, 0.03] + [0.03 + (span - 0.06) * k / 10 for k in range(1, 10)]
        cuts += [span - 0.03, span - 0.012, span - 0.004, span]
        verts, faces, uvs = [], [], []
        centres = []
        for i, cut in enumerate(cuts):
            a = cut / span
            t = BASTO_ALONG[0] + cut
            r = float(basto_radius(np.array(t)))
            p, d = ray(t, side * BASTO_THETA)
            centre = p + d * (UNDER + BASTO_R)
            centres.append((centre, d, r, a))
            fore = Vector((0.0, -1.0, 0.0))
            b = fore.cross(d).normalized()
            for k in range(nr + 1):
                ang = k / nr * 2 * math.pi
                verts.append(centre + (d * math.cos(ang) + b * math.sin(ang)) * r)
                uvs.append(region_uv(name, a, k / nr))
        na = len(cuts) - 1
        for i in range(na):
            for k in range(nr):
                q = i * (nr + 1) + k
                faces.append((q, q + nr + 1, q + nr + 2, q + 1))
        # The flat, tooled caps: on the end rings of the atlas region.
        for i, a_cap in ((0, 0.06), (na, 0.94)):
            centre, d, r, _ = centres[i]
            hub = len(verts)
            verts.append(centre)
            uvs.append(region_uv(name, a_cap, 0.5))
            ring = [i * (nr + 1) + k for k in range(nr)]
            for k in range(nr):
                a0, a1 = ring[k], ring[(k + 1) % nr]
                faces.append((hub, a1, a0) if i == 0 else (hub, a0, a1))
        obj = make("Basto_L" if side > 0 else "Basto_R", verts, faces, uvs, "Recado")
        _outward(obj)
    left, right = objects[-2], objects[-1]
    objects.remove(right)
    _join([left, right], "Bastos")

    # --- the cojinillo ----------------------------------------------------
    rng = np.random.default_rng(7)
    lumps = [
        (rng.uniform(*COJINILLO_ALONG), rng.uniform(-COJINILLO_AROUND, COJINILLO_AROUND), rng.uniform(0.025, 0.045), rng.uniform(0.004, 0.009))
        for _ in range(70)
    ]

    def pressed(t, th):
        """1 where the sobrepuesto presses the wool flat."""
        inside_t = _smooth(-0.02, 0.0, min(t - SOBREPUESTO_ALONG[0], SOBREPUESTO_ALONG[1] - t))
        inside_a = _smooth(-0.08, 0.0, SOBREPUESTO_AROUND - abs(th))
        return float(inside_t * inside_a)

    def wool_top(t, th):
        smooth_top = cojinillo(np.array(t), np.array(th))[1]
        bump = 0.0
        for lt, la, lr, lh in lumps:
            dd = math.hypot(t - lt, (th - la) * REACH) / lr
            if dd < 1:
                bump = max(bump, lh * (1 - dd * dd) ** 2)
        edge = float(_ends(np.array(t), *COJINILLO_ALONG, 0.04)) * float(_smooth(0.0, 0.08, COJINILLO_AROUND - abs(th)))
        return float(smooth_top) + bump * (1 - pressed(t, th)) * edge

    grid_shell(
        "Cojinillo",
        COJINILLO_ALONG,
        (-COJINILLO_AROUND, COJINILLO_AROUND),
        (20, 40),
        wool_top,
        lambda t, th: float(cojinillo(np.array(t), np.array(th))[0]),
        "cojinillo",
        swap=True,
        # Round at its corners, clear of the shoulder and the stifle, which
        # move under them at the gallop.
        corner=0.1,
    )

    # --- the sobrepuesto ---------------------------------------------------
    grid_shell(
        "Sobrepuesto",
        SOBREPUESTO_ALONG,
        (-SOBREPUESTO_AROUND, SOBREPUESTO_AROUND),
        (12, 20),
        lambda t, th: float(sobrepuesto(np.array(t), np.array(th))) + SOBREPUESTO_THICK,
        lambda t, th: float(sobrepuesto(np.array(t), np.array(th))),
        "sobrepuesto",
        swap=True,
        corner=0.03,
    )

    # --- the wool cards ----------------------------------------------------
    strands = []
    down = Vector((0.0, 0.0, -1.0))

    def lock(t, th, length, width, column, inward=0.06):
        """A broad lock rooted ``inward`` (radians) above the hem on the
        fleece's surface, lying along it down to the hem and hanging a
        little past it with gravity, waving."""
        side_sign = 1.0 if th >= 0 else -1.0
        root_th = th - side_sign * inward
        fore = Vector((0.0, -1.0, 0.0))
        phase = rng.uniform(0, 6.3)
        pts, widths, outs = [], [], []
        hem_p, hem_d = ray(t, th)
        hem = hem_p + hem_d * (wool_top(t, th) + 0.004)
        for j in range(6):
            s = j / 5
            if s <= 0.4:
                # Lying on the fleece down to its hem...
                ang = root_th + side_sign * inward * (s / 0.4)
                p, d = ray(t, ang)
                q = p + d * (wool_top(t, ang) + 0.004)
            else:
                # ...then hanging from it with gravity, a little out.
                d = hem_d
                q = hem + down * (length * (s - 0.4) / 0.6) + d * (0.008 * (s - 0.4))
            side = fore.cross(d).normalized()
            q = q + side * (math.sin(phase + s * 4.0) * width * 0.4 * s)
            q = out_of_skin(q, UNDER + 0.012)
            pts.append(q)
            widths.append(width * (1.0 - 0.25 * s))
            outs.append(d)
        strands.append((pts, widths, outs, column))

    def clump(count, place):
        """``count`` clumps of two or three broad locks each, from
        ``place``."""
        for _ in range(count):
            t, th = place()
            base_len = rng.uniform(0.06, 0.13)
            col = int(rng.integers(0, WOOL_COLUMNS))
            for _ in range(int(rng.integers(2, 4))):
                jt = t + rng.uniform(-0.012, 0.012)
                lock(jt, th, base_len * rng.uniform(0.8, 1.2), rng.uniform(0.03, 0.05), col)

    def hem():
        s = rng.choice((-1.0, 1.0))
        return rng.uniform(COJINILLO_ALONG[0] + 0.02, COJINILLO_ALONG[1] - 0.02), s * COJINILLO_AROUND

    clump(30, hem)
    cards = rig.strand_cards("Vellon", strands, WOOL_COLUMNS)
    cards.data.materials.append(bpy.data.materials.new("RecadoWool"))
    # strand_cards' UVs are Blender's (v up): already as the game expects.
    objects.append(cards)

    # --- the cabezada -------------------------------------------------------
    # The corners of the mouth: where the line between the skin the jaw
    # carries (the lower lip and the chin) and the skin of the head turns
    # from the mouth's slit up the cheek, on the outside of the lips.
    gi = {g.name: g.index for g in body.vertex_groups}
    jaw = {gi[n] for n in JAW}
    lower = [sum(g.weight for g in v.groups if g.group in jaw) > 0.5 for v in mesh.vertices]
    at = [body.matrix_world @ v.co for v in mesh.vertices]
    tip = max((at[i] for poly in polys for i in poly), key=lambda p: -p.y)
    corners = {}
    for s in (-1.0, 1.0):
        line = []
        for poly in mesh.polygons:
            if mats[poly.material_index] not in skin_mats:
                continue
            ring = list(poly.vertices)
            for i, j in zip(ring, ring[1:] + ring[:1]):
                mid = (at[i] + at[j]) / 2
                if lower[i] != lower[j] and mid.x * s > 0.035 and (mid - tip).length < 0.2:
                    line.append(mid)
        joint = min(line, key=lambda p: p.z)
        hit = bvh.ray_cast(Vector((0.0, joint.y, joint.z)), Vector((s, 0.0, 0.0)))[0]
        corners[s] = hit if hit is not None else joint
        print(f"recado: mouth corner {s:+.0f} at {tuple(round(c, 3) for c in corners[s])}")
    # The guides below were measured on the standing horse: the line of the
    # face from the forehead to the nose, the cheekbones and the throat.
    face = (Vector((0.0, -1.421, 1.324)) - Vector((0.0, -1.2, 1.769))).normalized()
    up = Vector((0.0, -face.z, face.y)) if face.y < 0 else Vector((0.0, face.z, -face.y))
    if up.z < 0:
        up = -up
    back = Vector((0.0, 1.0, 0.0))

    def strap(name, guides, width, thick, lift=0.0025, loop=False, material="Cabezada", ends_free=(), samples=6):
        pts = [p for p, _ in rig._catmull([Vector(g) for g in guides], samples)]
        if loop:
            pts = pts[:-1]
        for _ in range(3):
            proj = []
            for j, p in enumerate(pts):
                if j in ends_free or (j - len(pts)) in ends_free:
                    proj.append(p)
                else:
                    proj.append(on_skin(p, lift)[0])
            n = len(proj)
            pts = [
                proj[j]
                if (not loop and (j == 0 or j == n - 1))
                else (proj[(j - 1) % n] + proj[j] * 2 + proj[(j + 1) % n]) / 4
                for j in range(n)
            ]
        pts = [on_skin(p, lift)[0] if not (j in ends_free or (j - len(pts)) in ends_free) else p for j, p in enumerate(pts)]
        return ribbon(name, pts, width, thick, loop, material, lift=lift)

    def ribbon(name, pts, width, thick, loop, material, conform=True, lift=0.003):
        verts, faces, uvs = [], [], []
        n = len(pts)
        run = 0.0
        for j, p in enumerate(pts):
            prev, nxt = pts[(j - 1) % n if loop else max(j - 1, 0)], pts[(j + 1) % n if loop else min(j + 1, n - 1)]
            tan = (nxt - prev).normalized()
            _, nrm = on_skin(p, 0.0)
            nrm = (nrm - tan * nrm.dot(tan)).normalized()
            side = tan.cross(nrm).normalized()
            if j:
                run += (p - pts[j - 1]).length
            v = run / 0.48
            # Each edge laid on the skin itself, so the strap bends across
            # its width with the head instead of bridging it.
            edges = {}
            for off in (-0.5, 0.5):
                q = p + side * (width * off)
                edges[off] = on_skin(q, lift) if conform else (q, nrm)
            for off, out, u in ((-0.5, 0.0, 0.0), (0.5, 0.0, 0.5), (0.5, 1.0, 0.5), (-0.5, 1.0, 0.0)):
                loc, normal = edges[off]
                verts.append(loc + normal * (thick * out))
                uvs.append((u, v))
        segs = n if loop else n - 1
        for j in range(segs):
            a, b = 4 * j, 4 * ((j + 1) % n)
            for q in range(4):
                faces.append((a + q, b + q, b + (q + 1) % 4, a + (q + 1) % 4))
        if not loop:
            faces.append((0, 1, 2, 3))
            last = 4 * (n - 1)
            faces.append((last + 3, last + 2, last + 1, last))
        obj = make(name, verts, faces, uvs, material)
        _outward(obj)
        return obj

    # The bit's top eye, on the skin above the corner, where the quijera
    # buckles.
    top_eye = {s: on_skin(corners[s] + up * 0.03, 0.004)[0] for s in (-1.0, 1.0)}
    # Behind the eye and below the ear, on each side of the poll.
    cheek_top = {s: Vector((s * 0.104, -1.19, 1.686)) for s in (-1.0, 1.0)}
    strap(
        "Testera",
        [cheek_top[-1], Vector((-0.084, -1.192, 1.816)), Vector((0.0, -1.196, 1.896)), Vector((0.084, -1.192, 1.816)), cheek_top[1]],
        0.018,
        0.003,
    )
    strap(
        "Frontalera",
        # Across the forehead between the ears, clear of their hollows.
        [Vector((-0.033, -1.268, 1.748)), Vector((-0.019, -1.27, 1.755)), Vector((0.0, -1.271, 1.759)), Vector((0.019, -1.27, 1.755)), Vector((0.033, -1.268, 1.748))],
        0.022,
        0.003,
    )
    for s in (-1.0, 1.0):
        strap(
            "Quijera_" + ("L" if s > 0 else "R"),
            [cheek_top[s], Vector((s * 0.116, -1.228, 1.611)), Vector((s * 0.107, -1.282, 1.524)), Vector((s * 0.084, -1.324, 1.458)), top_eye[s]],
            0.022,
            0.004,
        )
    # The muserola: round the nose, about two fingers below the cheekbones.
    centre = Vector((0.0, -1.33, 1.543))
    a_dir = Vector((1.0, 0.0, 0.0))
    b_dir = face.cross(a_dir).normalized()
    ring = []
    for k in range(28):
        ang = k / 28 * 2 * math.pi
        d = (a_dir * math.sin(ang) + b_dir * math.cos(ang)).normalized()
        hit = bvh.ray_cast(centre, d)
        ring.append(hit[0] if hit[0] is not None else centre + d * 0.1)
    strap("Muserola", ring + ring[:1], 0.025, 0.005, loop=True, samples=1)
    strap(
        "Fiador",
        [cheek_top[-1], Vector((-0.079, -1.156, 1.639)), Vector((0.0, -1.143, 1.6)), Vector((0.079, -1.156, 1.639)), cheek_top[1]],
        0.015,
        0.003,
    )

    # --- the freno --------------------------------------------------------------
    def tube(points, radii, side_hint, sides, closed=False):
        if closed or len(points) <= 2:
            pts = [Vector(p) for p in points]
        else:
            pts = [p for p, _ in rig._catmull([Vector(p) for p in points], 4)]
        verts, faces, uvs = [], [], []
        n = len(pts)
        run = 0.0
        rings = n + 1 if closed else n
        for j in range(rings):
            p = pts[j % n]
            prev, nxt = pts[(j - 1) % n if closed else max(j - 1, 0)], pts[(j + 1) % n if closed else min(j + 1, n - 1)]
            tan = (nxt - prev).normalized()
            a = (side_hint - tan * side_hint.dot(tan)).normalized()
            b = tan.cross(a)
            if j:
                run += (p - pts[(j - 1) % n]).length
            for k in range(sides):
                ang = k / sides * 2 * math.pi
                verts.append(p + a * (radii[0] * math.cos(ang)) + b * (radii[1] * math.sin(ang)))
                uvs.append((k / sides, run / 0.05))
        for j in range(rings - 1):
            for k in range(sides):
                q, r = j * sides, (j + 1) * sides
                faces.append((q + k, r + k, r + (k + 1) % sides, q + (k + 1) % sides))
        if not closed:
            faces.append(tuple(range(sides - 1, -1, -1)))
            last = (rings - 1) * sides
            faces.append(tuple(last + k for k in range(sides)))
        return verts, faces, uvs

    def torus(centre, normal, radius, bar):
        normal = Vector(normal).normalized()
        a = normal.cross(Vector((0.0, 0.0, 1.0)))
        if a.length < 1e-3:
            a = normal.cross(Vector((0.0, 1.0, 0.0)))
        a.normalize()
        b = normal.cross(a)
        pts = [centre + (a * math.cos(k / 12 * 2 * math.pi) + b * math.sin(k / 12 * 2 * math.pi)) * radius for k in range(12)]
        return tube(pts, (bar / 2, bar / 2), normal, 5, closed=True)

    def merge(parts):
        verts, faces, uvs = [], [], []
        for v, f, u in parts:
            base = len(verts)
            verts += v
            uvs += u
            faces += [tuple(i + base for i in face) for face in f]
        return verts, faces, uvs

    freno_parts = []
    rein_rings = {}
    for s in (-1.0, 1.0):
        x = Vector((s, 0.0, 0.0))
        c = corners[s] + x * 0.007
        path = [c + up * 0.03, c, c - up * 0.025 + back * 0.004, c - up * 0.045 + back * 0.01, c - up * 0.062 + back * 0.014]
        # The shank lies on the lips; only its ring hangs free below them.
        # Its inner face a few millimetres off the lips, which work under it
        # in the idles.
        path = [on_skin(p, 0.0075)[0] for p in path[:-1]] + [out_of_skin(path[-1], 0.0075)]
        freno_parts.append(tube(path, (0.0035, 0.0065), x, 8))
        freno_parts.append(torus(top_eye[s] + x * 0.002, x, 0.009, 0.005))
        ring_c = out_of_skin(c - up * (0.062 + 0.016) + back * 0.016, 0.012)
        rein_rings[s] = ring_c
        freno_parts.append(torus(ring_c, x, 0.017, 0.006))
    v, f, u = merge(freno_parts)
    # None of the iron inside the lips: every vertex at least 4.5 mm off the
    # skin (the lips work under it in the idles).
    v = [out_of_skin(p, 0.0045) if (p - bvh.find_nearest(p)[0]).length < 0.03 else p for p in v]
    obj = make("Freno", v, f, u, "Freno")
    v, f, u = tube([corners[-1], corners[1]], (0.006, 0.006), up, 8)
    make("Bocado", v, f, u, "Freno")
    # The barbada: under the chin groove from one top eye to the other.
    chin = bvh.ray_cast(Vector((0.0, -1.314, 1.06)), Vector((0.0, 0.0, 1.0)))[0]
    strap(
        "Barbada",
        [top_eye[-1], Vector((-0.046, -1.314, chin.z + 0.012)), chin, Vector((0.046, -1.314, chin.z + 0.012)), top_eye[1]],
        0.012,
        0.004,
        ends_free=(0, -1),
    )

    # --- the markers ------------------------------------------------------------
    def marker(name, at):
        verts = [at + Vector(o) * 0.003 for o in ((1, 0, 0), (0, 1, 0), (0, 0, 1))]
        make(name, verts, [(0, 1, 2)], [(0.0, 0.0)] * 3, "Marker")

    # Blender's +X is the horse's left.
    marker("Mark_ReinL", rein_rings[1])
    marker("Mark_ReinR", rein_rings[-1])
    p, d = ray(SEAT_X, 0.0)
    marker("Mark_Seat", p + d * (float(sobrepuesto(np.array(SEAT_X), np.array(0.0))) + SOBREPUESTO_THICK))

    # --- skin them ---------------------------------------------------------------
    # From the skin alone: never the weights of the mane's or the forelock's
    # cards, nor the eyes'.
    import bmesh

    skin = body.copy()
    skin.data = body.data.copy()
    bpy.context.collection.objects.link(skin)
    bm = bmesh.new()
    bm.from_mesh(skin.data)
    bmesh.ops.delete(bm, geom=[f for f in bm.faces if mats[f.material_index] not in skin_mats], context="FACES")
    bm.to_mesh(skin.data)
    bm.free()
    for obj in objects:
        rig.transfer_weights(skin, obj, arm)
        if obj.name in BACK:
            stray = {}
            for v in obj.data.vertices:
                for g in v.groups:
                    name = obj.vertex_groups[g.group].name
                    if name not in TRUNK and g.weight > 0.05:
                        stray[name] = stray.get(name, 0) + 1
            if stray:
                print(f"recado: {obj.name}: vertices weighted off the trunk: {stray}")
            _trunk_only(obj, body)
        if obj.name == "Testera":
            # On the poll and the forehead: the head and the top of the neck
            # in the proportions of the skin under them, never the ears nor
            # the mane, which move on their own beside and behind them.
            for g in [g for g in obj.vertex_groups if g.name.startswith(LOOSE)]:
                obj.vertex_groups.remove(g)
            for v in obj.data.vertices:
                if sum(g.weight for g in v.groups) < 1e-4:
                    obj.vertex_groups[HEAD].add([v.index], 1.0, "REPLACE")
            bpy.ops.object.select_all(action="DESELECT")
            obj.select_set(True)
            bpy.context.view_layer.objects.active = obj
            bpy.ops.object.vertex_group_normalize_all(lock_active=False)
        if obj.name in SKULL:
            # On the skull, between the ears: the head carries them whole.
            for g in list(obj.vertex_groups):
                obj.vertex_groups.remove(g)
            obj.vertex_groups.new(name=HEAD).add(list(range(len(obj.data.vertices))), 1.0, "REPLACE")
        bpy.ops.object.select_all(action="DESELECT")
        obj.select_set(True)
        bpy.context.view_layer.objects.active = obj
        bpy.ops.object.vertex_group_limit_total(group_select_mode="ALL", limit=4)
        bpy.ops.object.vertex_group_normalize_all(lock_active=False)
    bpy.data.objects.remove(skin)
    straps = [o for o in objects if o.data.materials and o.data.materials[0].name in ("Cabezada", "Freno") and o.name != "Bocado"]
    pose = (horse, stand, skin_mats)
    # A few millimetres more than the clips need: the game also turns the
    # head and the neck by code, toward the reins.
    _settle([o for o in straps if o.name == "Testera"], body, pose, 0.0035, most=0.006)
    # The browband's ends sit in front of the ears, which turn forward.
    _settle([o for o in straps if o.name == "Frontalera"], body, pose, 0.0035)
    _settle([o for o in straps if o.name not in ("Testera", "Frontalera", "Fiador")], body, pose, 0.0035)
    # The shoulder blades and the loins move under the seat at the gallop:
    # the recado gives way to them where they rise.
    seat = [o for o in objects if o.name in ("Bastos", "Cojinillo", "Sobrepuesto", "Vellon")]
    _settle_pass(seat, body, pose, 0.014, reweight=False)
    # Under the throat the neck folds more when ridden than in any clip; in
    # the jump the throat folds shut over the strap for a moment, as it
    # would over a real one (clearing that would hang it 5 cm off the throat).
    _settle([o for o in straps if o.name == "Fiador"], body, pose, 0.0035, clips=GAITS)
    for obj in objects:
        empty = sum(1 for v in obj.data.vertices if sum(g.weight for g in v.groups) < 1e-4)
        if empty:
            print(f"recado: {obj.name}: {empty} vertices without weights")
    kept = list(objects)
    total = sum(rig.triangles(o) for o in kept if not o.name.startswith("Mark_"))
    print(f"recado: {total} triangles: " + ", ".join(f"{o.name} {rig.triangles(o)}" for o in kept))

    if textures:
        write_maps(textures)
    if preview:
        _preview(preview, arm, body, kept)
    # Back to the file's metres, where the armature rests.
    for obj in kept:
        if not _identity(obj.matrix_world):
            raise SystemExit(f"recado: {obj.name} must sit at the origin")
        co = np.empty(len(obj.data.vertices) * 3)
        obj.data.vertices.foreach_get("co", co)
        obj.data.vertices.foreach_set("co", co / SCALE)
        obj.data.update()
    bpy.data.objects.remove(body)
    rig.export(os.path.abspath(out_glb), [])


# The horse's skin (not its hair, its corneas nor its shoes).
SKIN = {"Horse"}
# The bone of the head, and the ones that move on their own on it and the
# neck: the ears and the mane.
HEAD = "BN_Head_00_016_015"
LOOSE = ("BN_L_Ear", "BN_R_Ear", "BN_Hair")
# The bones of the lower jaw: the skin they carry is the lower lip and the
# chin.
JAW = ("BN_Jaw_027_026", "BN_BT_Lip_028_027")
# The straps that ride the skull alone.
SKULL = ("Frontalera",)
# The pieces on the back, and the only bones that carry them: the spine from
# the croup to behind the withers, never the neck (its first bone, at the
# withers, lifts with the head) nor the belly's.
BACK = ("Bastos", "Cojinillo", "Sobrepuesto", "Vellon")
TRUNK = ("BN_Spine_00_02_02", "BN_Spine_01_03_03", "BN_Spine_02_04_04")
# The saddle tree itself (the bastos and the sobrepuesto on them) is stiff,
# every vertex with the same weights: it rides the bone under the seat, with a
# tenth of the one in front, whose withers rise under its front ends. With the
# mean weights of the skin under it (a fifth of the croup's bone, nearly half
# the withers') its ends rock 3.3 cm over the skin in henry's idles; so, 1.1.
TREE = ("Bastos", "Sobrepuesto")
TREE_WEIGHTS = {"BN_Spine_01_03_03": 0.9, "BN_Spine_02_04_04": 0.1}
# Blender's frame from the glTF file's: (x, y, z) -> (x, -z, y).
_TO_BLENDER = np.array([[1.0, 0, 0, 0], [0, 0, -1, 0], [0, 1, 0, 0], [0, 0, 0, 1]])


def _identity(m):
    return all(abs(m[i][j] - float(i == j)) < 1e-6 for i in range(4) for j in range(4))


def _scaled(raw):
    """File points in Blender's frame, in game metres."""
    raw = np.asarray(raw, float)
    return np.stack([raw[:, 0], -raw[:, 2], raw[:, 1]], 1) * SCALE


def _unscaled(points):
    """Blender points in game metres back in the file's frame."""
    p = np.asarray(points, float) / SCALE
    return np.stack([p[:, 0], p[:, 2], -p[:, 1]], 1)


def _rest_on(arm, horse, worlds):
    """Moves every bone of ``arm`` (as imported, resting in the nodes' rest)
    to rest in the pose of the node ``worlds``: each bone as its joint
    moves from one to the other."""
    import bpy
    from mathutils import Matrix

    rest = horse.worlds()
    to_b, from_b = _TO_BLENDER, np.linalg.inv(_TO_BLENDER)
    bpy.ops.object.select_all(action="DESELECT")
    bpy.context.view_layer.objects.active = arm
    bpy.ops.object.mode_set(mode="EDIT")
    bones = arm.data.edit_bones
    for b in bones:
        b.use_connect = False
    was = {b.name: np.array(b.matrix) for b in bones}
    for b in bones:
        k = horse.names.index(b.name)
        move = to_b @ worlds[k] @ np.linalg.inv(rest[k]) @ from_b
        b.matrix = Matrix((move @ was[b.name]).tolist())
    bpy.ops.object.mode_set(mode="OBJECT")


def _trunk_skin(body):
    """The skin vertices the trunk bones carry: each one's place and its trunk
    weights."""
    names = {g.index: g.name for g in body.vertex_groups}
    out = []
    for v in body.data.vertices:
        w = {names[g.group]: g.weight for g in v.groups if names[g.group] in TRUNK and g.weight > 0}
        if w:
            out.append((body.matrix_world @ v.co, w))
    return out


def _trunk_only(obj, body):
    """Keeps only the trunk bones' weights on ``obj``, normalised; a vertex
    left with none takes the weights of the nearest skin vertex that has
    trunk weights. The saddle tree's pieces (``TREE``) move whole with
    ``TREE_WEIGHTS``; the wool lies on the tree over the back and on the skin
    down the flanks, so its weights go from the one to the other between the
    bastos and the hem."""
    tree = TREE_WEIGHTS
    groups = {g.index: g.name for g in obj.vertex_groups}
    trunk_skin = _trunk_skin(body)
    weights = []
    for v in obj.data.vertices:
        w = {groups[g.group]: g.weight for g in v.groups if groups[g.group] in TRUNK and g.weight > 0}
        p = obj.matrix_world @ v.co
        if not w:
            w = min(trunk_skin, key=lambda s: (s[0] - p).length_squared)[1]
        total = sum(w.values())
        w = {k: x / total for k, x in w.items()}
        if obj.name in TREE:
            w = dict(tree)
        else:
            # Round the barrel from the top (game ``Axis::BARREL``); the locks
            # of wool hang from the hem, and move with it.
            theta = abs(math.atan2(-p.x, p.z - AXIS_HEIGHT))
            if obj.name == "Vellon":
                theta = min(theta, COJINILLO_AROUND)
            free = float(_smooth(1.0, 1.6, theta))
            w = {k: tree.get(k, 0.0) * (1 - free) + w.get(k, 0.0) * free for k in set(tree) | set(w)}
        weights.append(w)
    for v, w in zip(obj.data.vertices, weights):
        for g in list(v.groups):
            obj.vertex_groups[g.group].remove([v.index])
        for name, weight in sorted(w.items(), key=lambda kv: -kv[1])[:4]:
            group = obj.vertex_groups.get(name) or obj.vertex_groups.new(name=name)
            group.add([v.index], weight, "REPLACE")


def _settle(objects, body, pose, gap, most=1.0, clips=SADDLED_CLIPS, reweight=True):
    """Poses the horse in every clip a saddled horse plays under ``objects``
    (skinned to its rig). First, a vertex the moving skin ever came closer
    to than ``gap`` takes the weights of the skin that came closest, so it
    rides on it (the ears turn and the jaw chews under the straps and the
    bit); then each vertex still too close is lifted off the standing skin,
    along its normal, by as much as it was, ``most`` in all."""
    if reweight and not any(o.name in SKULL for o in objects):
        _settle_pass(objects, body, pose, gap, reweight=True, clips=clips)
    # A lift along the standing skin's normal is not always the way out of
    # the skin of another pose: lift again until no vertex needs more.
    lifted = {o.name: np.zeros(len(o.data.vertices)) for o in objects}
    for _ in range(4):
        if _settle_pass(objects, body, pose, gap, reweight=False, most=most, lifted=lifted, clips=clips) < 0.0005:
            break


def _skin_of(obj, horse):
    """``obj``'s four heaviest joints (indices into the horse's skin) and
    their weights, per vertex, normalised."""
    joint = {horse.names[j]: k for k, j in enumerate(horse.joints)}
    groups = {g.index: joint.get(g.name) for g in obj.vertex_groups}
    n = len(obj.data.vertices)
    joints, weights = np.zeros((n, 4), int), np.zeros((n, 4))
    for i, v in enumerate(obj.data.vertices):
        heavy = sorted(((g.weight, groups[g.group]) for g in v.groups if g.weight > 0 and groups[g.group] is not None), reverse=True)
        for c, (w, k) in enumerate(heavy[:4]):
            joints[i, c], weights[i, c] = k, w
    total = weights.sum(1, keepdims=True)
    return joints, np.divide(weights, total, out=np.zeros_like(weights), where=total > 0)


def _settle_pass(objects, body, pose, gap, reweight, most=1.0, lifted=None, clips=SADDLED_CLIPS):
    from mathutils import Vector
    from mathutils.bvhtree import BVHTree

    horse, stand, skin_mats = pose
    mats = [s.material.name if s.material else "" for s in body.material_slots]
    skin_polys = [p for p in body.data.polygons if mats[p.material_index] in skin_mats]
    polys = [tuple(p.vertices) for p in skin_polys]
    poly_of = [p.index for p in skin_polys]
    unstand = np.stack([np.linalg.inv(stand[j]) for j in horse.joints])
    pieces = []
    for obj in objects:
        at = np.array([tuple(obj.matrix_world @ v.co) for v in obj.data.vertices])
        pieces.append((obj, np.c_[_unscaled(at), np.ones(len(at))], *_skin_of(obj, horse)))
    margin = {o.name: np.ones(len(o.data.vertices)) for o in objects}
    closest = {o.name: [None] * len(o.data.vertices) for o in objects}
    poses = 0
    for clip in clips:
        for frame in range(0, int(round(horse.duration(clip) * FPS)) + 1, 2):
            worlds = horse.worlds(clip, frame / FPS)
            bvh = BVHTree.FromPolygons([Vector(p) for p in _scaled(horse.skinned(worlds))], polys)
            moved = np.stack([worlds[j] for j in horse.joints]) @ unstand
            poses += 1
            for obj, at, joints, weights in pieces:
                m = np.einsum("nc,ncij->nij", weights, moved[joints])
                now = _scaled(np.einsum("nij,nj->ni", m, at)[:, :3])
                worst = margin[obj.name]
                for i, p in enumerate(now):
                    loc, n, face, d = bvh.find_nearest(Vector(p))
                    if loc is None:
                        continue
                    sd = d if (Vector(p) - loc).dot(n) >= 0 else -d
                    if sd < worst[i]:
                        worst[i] = sd
                        closest[obj.name][i] = poly_of[face]
    verts = [body.matrix_world @ v.co for v in body.data.vertices]
    bvh = BVHTree.FromPolygons(verts, polys)
    if reweight:
        names = {g.index: g.name for g in body.vertex_groups}
        count = 0
        for obj in objects:
            for i, v in enumerate(obj.data.vertices):
                face = closest[obj.name][i]
                if margin[obj.name][i] >= gap or face is None:
                    continue
                weights = {}
                corners = body.data.polygons[face].vertices
                for vi in corners:
                    for g in body.data.vertices[vi].groups:
                        name = names[g.group]
                        weights[name] = weights.get(name, 0.0) + g.weight / len(corners)
                for g in list(v.groups):
                    obj.vertex_groups[g.group].remove([i])
                for name, w in sorted(weights.items(), key=lambda kv: -kv[1])[:4]:
                    group = obj.vertex_groups.get(name) or obj.vertex_groups.new(name=name)
                    group.add([i], w, "REPLACE")
                count += 1
        print(f"recado: {count} vertices now ride the skin that moves under them")
        return 0.0
    if lifted is None:
        lifted = {o.name: np.zeros(len(o.data.vertices)) for o in objects}
    count, most_now = 0, 0.0
    for obj in objects:
        done = lifted[obj.name]
        for i, v in enumerate(obj.data.vertices):
            need = min(gap - margin[obj.name][i], most - done[i])
            if need > 0:
                p = obj.matrix_world @ v.co
                _, n, _, _ = bvh.find_nearest(p)
                v.co = obj.matrix_world.inverted() @ (p + n * need)
                done[i] += need
                count += 1
                most_now = max(most_now, need)
    still = max((gap - float(m.min()) for m in margin.values()), default=0.0)
    print(f"recado: lifted {count} vertices of {', '.join(o.name for o in objects)} clear of the moving skin over {poses} poses by at most {most_now * 100:.1f} cm (the skin came {still * 100:.1f} cm too close)")
    return most_now


def _round_corner(t, th, along, around, r):
    """``(t, th)`` pulled onto a rounded corner of radius ``r`` metres."""
    t0, t1 = min(along), max(along)
    v0, v1 = min(around) * REACH, max(around) * REACH
    px, py = t, th * REACH
    cx, cy = min(max(px, t0 + r), t1 - r), min(max(py, v0 + r), v1 - r)
    dx, dy = px - cx, py - cy
    d = math.hypot(dx, dy)
    if dx != 0 and dy != 0 and d > r:
        px, py = cx + dx * r / d, cy + dy * r / d
    return px, py / REACH


def _outward(obj):
    import bmesh

    bm = bmesh.new()
    bm.from_mesh(obj.data)
    bmesh.ops.recalc_face_normals(bm, faces=bm.faces)
    bm.to_mesh(obj.data)
    bm.free()


def _join(parts, name):
    import bpy

    bpy.ops.object.select_all(action="DESELECT")
    for p in parts:
        p.select_set(True)
    bpy.context.view_layer.objects.active = parts[0]
    bpy.ops.object.join()
    obj = bpy.context.view_layer.objects.active
    obj.name = name
    obj.data.name = name


def _preview(out_dir, arm, body, objects):
    import bpy

    import animal_rig as rig

    rec = atlas(1024)
    wool = wool_atlas(512)
    rawhide = T.rawhide(512)
    iron = T.iron(256)

    def image(name, rgb):
        return rig.image_from_array(name, np.ascontiguousarray(rgb[::-1]))

    def normal(name, rgb):
        return rig.image_from_array(name, np.ascontiguousarray(rgb[::-1]), data=True)

    looks = {
        "Recado": rig.textured_material("PrevRecado", image("rc", rec["color"]), normal("rn", rec["normal_gl"])),
        "RecadoWool": rig.textured_material(
            "PrevWool",
            image("wc", wool["color"]),
            normal("wn", wool["normal_gl"]),
            alpha=image("wa", np.repeat(wool["opacity"][..., None], 3, axis=2)),
        ),
        "Cabezada": rig.textured_material("PrevRaw", image("hc", rawhide["color"]), normal("hn", rawhide["normal_gl"]), rough=0.6),
        "Freno": rig.textured_material("PrevIron", image("ic", iron["color"]), normal("in", iron["normal_gl"]), rough=0.45),
    }
    for obj in objects:
        name = obj.data.materials[0].name if obj.data.materials else ""
        if name in looks:
            obj.data.materials[0] = looks[name]
        if obj.name.startswith("Mark_"):
            obj.hide_render = True
    coat = bpy.data.materials.new("Coat")
    coat.use_nodes = True
    coat.node_tree.nodes["Principled BSDF"].inputs["Base Color"].default_value = (0.18, 0.11, 0.07, 1)
    for i in range(len(body.data.materials)):
        body.data.materials[i] = coat
    shots = [
        ("side.png", None, 1, (2.6, 0.0, 1.45), (0.0, -0.15, 1.3), 50),
        ("front3q.png", None, 1, (1.9, -2.1, 1.75), (0.0, -0.3, 1.3), 50),
        ("rear3q.png", None, 1, (1.8, 2.0, 1.85), (0.0, -0.1, 1.3), 50),
        ("seat.png", None, 1, (0.9, 0.25, 1.85), (0.0, -0.13, 1.35), 50),
        ("seat_low.png", None, 1, (0.85, -0.1, 1.25), (0.0, -0.13, 1.35), 45),
        ("head.png", None, 1, (1.15, -1.6, 1.8), (0.0, -1.3, 1.6), 45),
        ("head_front.png", None, 1, (0.75, -2.3, 1.85), (0.0, -1.3, 1.6), 45),
        ("head_low.png", None, 1, (0.9, -1.75, 1.3), (0.0, -1.3, 1.45), 40),
        ("mouth.png", None, 1, (0.55, -1.5, 1.42), (0.0, -1.34, 1.42), 50),
        ("mouth_r.png", None, 1, (-0.55, -1.5, 1.42), (0.0, -1.34, 1.42), 50),
    ]
    rig.render_previews(out_dir, arm, shots, size=(900, 700))


if __name__ == "__main__":
    argv = sys.argv[sys.argv.index("--") + 1 :] if "--" in sys.argv else sys.argv[1:]
    out = argv[0]
    textures = argv[argv.index("--textures") + 1] if "--textures" in argv else None
    preview = argv[argv.index("--preview") + 1] if "--preview" in argv else None
    try:
        import bpy  # noqa: F401
    except ImportError:
        write_maps(textures or out)
    else:
        build(out, textures, preview)
