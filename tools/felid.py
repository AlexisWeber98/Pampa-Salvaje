"""Model, rig, paint and animate the pampa's wild cats in Blender, by
script: the gato montés (Leopardus geoffroyi), the puma (Puma concolor) and
the yaguareté (Panthera onca), from one cat's anatomy and rig.

Run it headless, once per species:

    blender -b --factory-startup --python tools/felid.py -- OUT.glb \
        --species gato|puma|yagua [--textures DIR] [--preview DIR] \
        [--shots REGEX] [--draft] [--no-clips]

Writes OUT.glb: one skinned mesh (``GatoMontes``, ``Puma`` or
``Yaguarete``; and its light mesh for far off, the same name with
``_LOD``: the skin alone decimated to ``LIGHT_TRIS``, ``light_mesh``) with the materials ``Main`` (the whole skin: the coat with
its spots, the bare nose and the pads, drawn by the body's own colour map),
``Hair`` (the fur's tufts, the tail's short fur and the whiskers,
alpha-tested cards whose atlas carries their own colours) and the eyes
``Eye_Black`` and ``Eye_Iris``; its armature ``AnimalArmature`` keeps the
Quaternius quadruped's joint names (as the fox, ``tools/fox.py``: ``Body``,
``Back``, ``Torso``-``Torso3``, ``Neck1``-``Neck3``, ``Head``, the legs, the
tail, the IK bones and their ``_end`` leaves) with two bones per ear, the
jaw and nine for the tail; and the seven clips ``Idle``, ``Idle_2``,
``Idle_Headlow``, ``Eating``, ``Walk``, ``Gallop`` (a feline rotary gallop,
the spine flexing and extending with every stride) and ``Gallop_Jump`` (a
pounce) as the cattle's and the fox's (the exporter sorts them by name:
``tools/glbtrim.py --keep`` puts them back in this order). Life size, the
pads at the origin, facing -Y (glTF +Z).

It is the horse (``tools/horse.py``) with a cat's anatomy, as the fox is:
the same steps (the sculpt baked onto a decimated mesh, the hair cards, the
skinning, the analytic leg IK, the gaits, the previews) run through
``animal_rig.Steps`` with the names below replaced. The three species share
one layout (a puma's, in metres) made into each by ``warp``: a scale, the
legs shortened between the paws and the elbows, the trunk shortened
between the shoulders and the hips; the head grows about its joint, and
the girth, the limbs, the paws, the neck, the ears and the tail are each
species' own (``SPECIES``). Each has one coat, painted on the sculpt
(``coat_at``: the spots, rosettes, bands and rings are a pattern in the
layout) and baked into the body's colour map: the game draws ``Main`` and
``Hair`` with their maps as they are.

The coats (the reference photographs: Commons, the three species): the gato
montés grey buff with small solid black spots all over, bands on the legs,
rings on the tail, two dark stripes across each cheek and black ears with a
white spot; the puma a plain greyish tawny, the muzzle, the chin and the
throat white, a dark patch behind each whisker pad, the backs of the ears
and the tail's tip dark; the yaguareté golden with rosettes (dark rings with
spots inside) on the back and the flanks, solid spots on the head, the neck,
the legs and the white belly, the tail spotted then ringed toward its black
tip. See tools/README.md.
"""

import math
import os
import re
import sys

try:
    import bpy
except ImportError:  # outside Blender: the layout, the coats, the cards' colours and the plans (tests)
    bpy = None

HERE = os.path.dirname(os.path.abspath(__file__))


def smooth(t):
    t = max(0.0, min(1.0, t))
    return t * t * (3 - 2 * t)


def _mix(a, b, f):
    return tuple(a[i] + (b[i] - a[i]) * f for i in range(3))


def _dist(a, b):
    return math.sqrt(sum((a[i] - b[i]) ** 2 for i in range(3)))


def _norm(v):
    n = math.sqrt(sum(c * c for c in v)) or 1.0
    return tuple(c / n for c in v)


# ---------------------------------------------------------------------------
# The species. ``S`` scales the whole layout (a puma's, in metres); ``LEG``
# shortens the legs between the paws and the elbows (``warp``), ``TRUNK``
# the trunk between the shoulders and the hips; ``HEAD`` grows the head
# about its joint, ``HEAD_W`` widens it, ``MUZZLE`` lengthens the face ahead
# of the eyes; ``GIRTH``, ``LIMB``, ``PAW``, ``NECK`` and ``EAR`` are the
# trunk's, the legs', the paws', the neck's and the ears' thickness or size;
# the tail is ``TAIL_SEG`` (each of nine segments, layout metres) by
# ``TAIL_R`` (root, tip) carried down at ``TAIL_ANGLES`` (degrees below
# level, segment by segment); ``POUCH`` the loose skin of the belly in
# front of the thighs (the primordial pouch). Then the gaits' seconds and
# ground speeds (m/s), the gallop's share of the cycle a fore paw stands
# (a sprint: short, the paw sweeping far back) and the eyes' iris.

SPECIES = {
    "gato": dict(
        mesh="GatoMontes", S=0.42, LEG=0.95, TRUNK=1.0, HEAD=1.42, HEAD_W=1.04, MUZZLE=0.86, GIRTH=0.93, LIMB=0.92,
        PAW=0.95, NECK=0.95, EAR=0.82, EYE=0.75, HIND=1.0, JAW=1.0, BROW=1.0, TAIL_SEG=0.074, TAIL_R=(0.034, 0.028),
        TAIL_ANGLES=(-28, -48, -60, -68, -72, -70, -60, -42, -18), POUCH=0.6, BODY_TRIS=5200,
        walk=(0.56, 0.55), gallop=(0.32, 5.6, 0.18), iris=(0.52, 0.50, 0.24), pupil=(0.07, 0.26),
    ),
    "puma": dict(
        mesh="Puma", S=1.0, LEG=1.0, TRUNK=1.06, HEAD=1.04, HEAD_W=0.96, MUZZLE=1.08, GIRTH=1.0, LIMB=1.0, PAW=1.0,
        NECK=1.0, EAR=0.76, EYE=0.64, HIND=1.12, JAW=0.88, BROW=1.0, TAIL_SEG=0.090, TAIL_R=(0.042, 0.038),
        TAIL_ANGLES=(-38, -62, -76, -83, -86, -76, -48, -4, 34), POUCH=1.0, BODY_TRIS=6800,
        walk=(0.92, 0.82), gallop=(0.38, 12.5, 0.15), iris=(0.60, 0.50, 0.26), pupil=(0.22, 0.22),
    ),
    "yagua": dict(
        mesh="Yaguarete", S=1.12, LEG=0.91, TRUNK=0.92, HEAD=1.50, HEAD_W=1.30, MUZZLE=0.78, GIRTH=1.36, LIMB=1.42,
        PAW=1.28, NECK=1.40, EAR=0.70, EYE=0.65, HIND=1.08, JAW=1.15, BROW=1.15, TAIL_SEG=0.054, TAIL_R=(0.044, 0.036),
        TAIL_ANGLES=(-30, -55, -68, -74, -72, -60, -40, -16, 8), POUCH=1.3, BODY_TRIS=6800,
        walk=(0.98, 0.76), gallop=(0.40, 12.0, 0.15), iris=(0.66, 0.50, 0.16), pupil=(0.22, 0.22),
    ),
}

NAME = "puma"
SP = SPECIES[NAME]


# ---------------------------------------------------------------------------
# The build: the layout (a puma's) made into each species. ``warp`` scales
# it by ``S``, shortens the legs between ``LEG_FROM`` and ``LEG_TO`` (the
# paws and the body above keep their shape) and the trunk between
# ``TRUNK_FROM`` and ``TRUNK_TO`` (the hindquarters and the tail come
# forward). Both are piecewise linear, so ``unwarp`` is exact.

LEG_FROM, LEG_TO = 0.06, 0.34
TRUNK_FROM, TRUNK_TO = -0.22, 0.22


def _wz(z):
    k = SP["LEG"]
    if z <= LEG_FROM:
        return z
    if z <= LEG_TO:
        return LEG_FROM + (z - LEG_FROM) * k
    return z - (LEG_TO - LEG_FROM) * (1.0 - k)


def _wy(y):
    k = SP["TRUNK"]
    if y <= TRUNK_FROM:
        return y
    if y <= TRUNK_TO:
        return TRUNK_FROM + (y - TRUNK_FROM) * k
    return y - (TRUNK_TO - TRUNK_FROM) * (1.0 - k)


def _uz(z):
    k = SP["LEG"]
    top = LEG_FROM + (LEG_TO - LEG_FROM) * k
    if z <= LEG_FROM:
        return z
    if z <= top:
        return LEG_FROM + (z - LEG_FROM) / k
    return z + (LEG_TO - LEG_FROM) * (1.0 - k)


def _uy(y):
    k = SP["TRUNK"]
    top = TRUNK_FROM + (TRUNK_TO - TRUNK_FROM) * k
    if y <= TRUNK_FROM:
        return y
    if y <= top:
        return TRUNK_FROM + (y - TRUNK_FROM) / k
    return y + (TRUNK_TO - TRUNK_FROM) * (1.0 - k)


def warp(p):
    """A point of the layout (Blender metres) to the model."""
    s = SP["S"]
    return (s * p[0], s * _wy(p[1]), s * _wz(p[2]))


def unwarp(p):
    """A point of the model back to the layout."""
    s = SP["S"]
    return (p[0] / s, _uy(p[1] / s), _uz(p[2] / s))


# ---------------------------------------------------------------------------
# The head's frame: from the back of the skull, at the eyes' height, to the
# nose.
# ``_at(t, u, s)`` is ``t`` metres along it, ``u`` up from it (toward the
# forehead) and ``s`` to the left, for a puma's head; the species' head is
# that grown ``HEAD`` times about the head's joint (``HEAD_PIVOT``), ``HEAD_W``
# wider and its face ahead of the eyes ``MUZZLE`` as long. Plain tuples, so
# the coat works without Blender.

_A = (0.0, -0.966, -0.259)  # the head's length, from the back of the skull to the nose, 15 degrees nose down
_n = math.sqrt(sum(c * c for c in _A))
HEAD_A = tuple(c / _n for c in _A)
HEAD_U = (0.0, HEAD_A[2], -HEAD_A[1])  # perpendicular, forward and up
HEAD_PIVOT = (0.0, -0.500, 0.590)
# The frame's origin: the back of the skull at the height of the eyes.
HEAD_O = tuple(HEAD_PIVOT[i] + HEAD_A[i] * 0.012 + HEAD_U[i] * 0.042 for i in range(3))
FACE_T = 0.13  # the face (ahead of the eyes) starts this far along the head


def _face(t):
    m = SP["MUZZLE"]
    return t if t <= FACE_T else FACE_T + (t - FACE_T) * m


def _unface(t):
    m = SP["MUZZLE"]
    return t if t <= FACE_T else FACE_T + (t - FACE_T) / m


def _at(t, u=0.0, s=0.0):
    k, w = SP["HEAD"], SP["HEAD_W"]
    t = _face(t)
    local = (s * w, HEAD_O[1] - HEAD_PIVOT[1] + HEAD_A[1] * t + HEAD_U[1] * u, HEAD_O[2] - HEAD_PIVOT[2] + HEAD_A[2] * t + HEAD_U[2] * u)
    return (HEAD_PIVOT[0] + k * local[0], HEAD_PIVOT[1] + k * local[1], HEAD_PIVOT[2] + k * local[2])


def head_coords(p):
    """(t, u, s) of the layout point ``p`` in the head's frame."""
    k, w = SP["HEAD"], SP["HEAD_W"]
    r = tuple((p[i] - HEAD_PIVOT[i]) / k for i in range(3))
    r = (r[0], r[1] - (HEAD_O[1] - HEAD_PIVOT[1]), r[2] - (HEAD_O[2] - HEAD_PIVOT[2]))
    t = r[1] * HEAD_A[1] + r[2] * HEAD_A[2]
    return (_unface(t), r[1] * HEAD_U[1] + r[2] * HEAD_U[2], r[0] / w)


def hr(rs, rt, ru):
    """Radii (across, along, up) in the head's frame, grown with the head."""
    k = SP["HEAD"]
    return (rs * k * SP["HEAD_W"], rt * k, ru * k)


# ---------------------------------------------------------------------------
# The skeleton: the Quaternius quadruped's names and parents (the clips of
# ``horse.py`` drive them by name), set on a cat, in the layout. The fore
# leg: ``FrontShoulder`` the shoulder blade and the arm down to the elbow,
# ``FrontUpperLeg`` the forearm (elbow to wrist), ``FrontLowerLeg`` the
# metacarpus (wrist to the knuckles over the pad); the hind: ``BackLeg``
# the femur, ``BackUpperLeg`` the tibia, ``BackLowerLeg`` the long
# metatarsus; the paws hang from the IK bones, as the horse's hooves.

FORE_X, HIND_X = 0.074, 0.080

# The trunk's profile (layout metres): a tube through these points with an
# elliptical section (half width, half height) at each, and the point of
# the chest. A cat's chest is deep and narrow but flat in front: from the
# side the neck runs down into the forelegs between the points of the
# shoulders, the brisket no lower than the elbows; behind the ribs the
# belly rises to the flank, where the loose skin of the pouch hangs in front
# of the thighs. The back dips a little behind the shoulder blades and
# rises to the hips (``back_line``, ``belly_line``, ``chest_front``).
TRUNK = [((0, -0.320, 0.515), (0.088, 0.088)), ((0, -0.255, 0.500), (0.135, 0.152)), ((0, -0.160, 0.500), (0.150, 0.158)),
         ((0, -0.040, 0.500), (0.148, 0.150)), ((0, 0.080, 0.510), (0.138, 0.135)), ((0, 0.200, 0.530), (0.128, 0.120)),
         ((0, 0.300, 0.555), (0.118, 0.105)), ((0, 0.380, 0.570), (0.092, 0.082))]
BRISKET = ((0.0, -0.360, 0.445), (0.050, 0.034, 0.050))
WITHERS = ((0.0, -0.250, 0.614), (0.050, 0.110, 0.046))
CROUP = ((0.0, 0.295, 0.612), (0.080, 0.100, 0.052))
POUCH = ((0.0, 0.160, 0.410), (0.090, 0.130, 0.050))
SHOULDER_POINT = (-0.405, 0.470)  # (y, z): the front of the point of the shoulder
ELBOW_Z = 0.355


def _tube_nodes():
    """The trunk's nodes with the species' girth."""
    g = SP["GIRTH"]
    return [(c, (r[0] * g, r[1] * (0.5 + 0.5 * g))) for c, r in TRUNK]


def chest_front(z):
    """How far forward (the least ``y``) the trunk's profile and the point
    of the chest reach at height ``z`` (layout); None if neither does."""
    best = None
    shapes = [(cy, cz, b, b) for (_, cy, cz), (_, b) in _tube_nodes()]
    shapes.append((BRISKET[0][1], BRISKET[0][2], BRISKET[1][1], BRISKET[1][2]))
    for cy, cz, ry, rz in shapes:
        d = (z - cz) / rz
        if abs(d) < 1.0:
            y = cy - ry * math.sqrt(1.0 - d * d)
            best = y if best is None else min(best, y)
    return best


def chest_bottom(ahead_of=-0.10):
    """The lowest point of the trunk's profile ahead of ``ahead_of`` (layout)."""
    return min(min(cz - b for (_, cy, cz), (_, b) in _tube_nodes() if cy < ahead_of), BRISKET[0][2] - BRISKET[1][2])


def _profile(y, top):
    """The highest (``top``) or lowest point of the trunk's side profile at
    ``y`` (layout): the tube between its nodes, the withers, the croup and
    the pouch."""
    nodes = _tube_nodes()
    out = None
    for ((_, y0, z0), (_, b0)), ((_, y1, z1), (_, b1)) in zip(nodes, nodes[1:]):
        if y0 <= y <= y1:
            f = (y - y0) / (y1 - y0)
            zc, b = z0 + (z1 - z0) * f, b0 + (b1 - b0) * f
            out = zc + b if top else zc - b
    if out is None:
        return None
    blobs = (WITHERS, CROUP) if top else (POUCH,)
    for (_, cy, cz), (_, ry, rz) in blobs:
        k = SP["POUCH"] if blobs[0] is POUCH else 1.0
        d = (y - cy) / ry
        if abs(d) < 1.0:
            z = cz + rz * math.sqrt(1.0 - d * d) if top else cz - k * rz * math.sqrt(1.0 - d * d)
            out = max(out, z) if top else min(out, z)
    return out


def back_line(y):
    """The top of the back at ``y`` (layout), in metres of the model."""
    return SP["S"] * _wz(_profile(y, True))


def belly_line(y):
    """The bottom of the trunk at ``y`` (layout), in metres of the model."""
    return SP["S"] * _wz(_profile(y, False))


def withers_height():
    """The model's height over the withers: the top of the back over the
    shoulder blades."""
    return max(back_line(-0.25 + 0.01 * k) for k in range(-6, 7))


def hips_height():
    return max(back_line(0.30 + 0.01 * k) for k in range(-6, 7))


def _tail_points():
    """The tail's axis: nine segments carried back and down from the root,
    each ``TAIL_ANGLES`` degrees below level (the last curling up)."""
    pts = [(0.0, 0.420, 0.600)]
    for a in SP["TAIL_ANGLES"]:
        r = math.radians(a)
        y, z = pts[-1][1], pts[-1][2]
        pts.append((0.0, y + SP["TAIL_SEG"] * math.cos(r), z + SP["TAIL_SEG"] * math.sin(r)))
    return pts


TAIL = _tail_points()
TAIL_CHAIN = [f"Tail{k}" for k in range(1, 10)]

NOSE_T = 0.202  # the tip of the nose, along the head (a puma's face)
EYE = (0.140, -0.004, 0.031)  # head frame (t, u, |s|) of the eyeball centre
EYE_RADIUS = 0.0160
# The ears: two bones each, from the base to the rounded top (head frame).
EAR_BASE, EAR_MID, EAR_TIP = (0.064, 0.020, 0.062), (0.060, 0.046, 0.073), (0.056, 0.070, 0.077)


# The head's proportions measured on front views of the photographs
# (Commons: puma r02, gato montés r07, yaguareté r05; on 800-pixel copies
# with a 50-pixel grid, to about a hundredth): the eye's opening across the
# skull's width at the cheekbones, and the ear's height over its base
# against the skull's height (crown to chin).
HEAD_RATIOS = {"gato": (0.18, 0.28), "puma": (0.165, 0.26), "yagua": (0.125, 0.24)}


def head_ratios():
    """The model's (eye width / skull width, ear height / skull height),
    from the head's shapes (``head``, ``ears``)."""
    eye = 2 * 0.0175 * SP["EYE"]
    skull_w = 2 * (0.050 + 0.020) * SP["HEAD_W"]
    skull_h = (0.012 + 0.047) - (-0.095 - 0.028)
    crown = 0.020  # the skull's surface under the ears
    top = EAR_BASE[1] + (EAR_TIP[1] - EAR_BASE[1] + 0.012) * SP["EAR"]
    return eye / skull_w, (top - crown) / skull_h


# The yaguareté's legs against its chest, measured from the side on photos
# (800-px copies with a 10-px grid): the height of the bottom of the chest
# (with the belly's fur) over the ground, over the distance from the withers
# down to it. Reference photos (shape only; no pixel is used): Commons
# "Jaguar full.jpg" 0.69, "Jaguar (Panthera onca palustris) male Three
# Brothers River 2.jpg" 0.65 and "Jaguar (Panthera onca) male walking on the
# riverbank ... (48416282622)" 0.79. The model takes the longest-legged one
# (``LEG_RATIO_TARGET``): the user found it squat at 0.71 (L107).
LEG_RATIOS = {"yagua": (0.69, 0.65, 0.79)}
LEG_RATIO_TARGET = {"yagua": max(LEG_RATIOS["yagua"])}


def leg_ratio():
    """The chest's height over the ground over the chest's depth (from the
    withers down to its bottom), in the model."""
    w = withers_height()
    low = SP["S"] * _wz(chest_bottom())
    return low / (w - low)


def _ear(point, sx):
    """A point of an ear (head frame), grown with the species' ``EAR``
    about the ear's base."""
    e = SP["EAR"]
    t, u, s = (EAR_BASE[i] + (point[i] - EAR_BASE[i]) * e for i in range(3))
    return _at(t, u, sx * s)


def _bones():
    return [
        ("Body", None, (0.0, 0.0, 0.20)),
        ("Back", "Body", (0.0, 0.300, 0.590)),
        ("Torso", "Back", (0.0, 0.140, 0.585)),
        ("Torso2", "Torso", (0.0, -0.060, 0.575)),
        ("Torso3", "Torso2", (0.0, -0.220, 0.575)),
        ("Neck1", "Torso3", (0.0, -0.370, 0.560)),
        ("Neck2", "Neck1", (0.0, -0.415, 0.568)),
        ("Neck3", "Neck2", (0.0, -0.458, 0.578)),
        ("Head", "Neck3", HEAD_PIVOT),
        ("FrontShoulder.L", "Torso2", (0.050, -0.290, 0.560)),
        ("FrontUpperLeg.L", "FrontShoulder.L", (FORE_X, -0.272, 0.362)),
        ("FrontLowerLeg.L", "FrontUpperLeg.L", (FORE_X, -0.300, 0.112)),
        ("FrontShoulder.R", "Torso2", (-0.050, -0.290, 0.560)),
        ("FrontUpperLeg.R", "FrontShoulder.R", (-FORE_X, -0.272, 0.362)),
        ("FrontLowerLeg.R", "FrontUpperLeg.R", (-FORE_X, -0.300, 0.112)),
        ("BackShoulder.L", "Back", (0.050, 0.300, 0.590)),
        ("BackLeg.L", "BackShoulder.L", (HIND_X, 0.300, 0.560)),
        ("BackUpperLeg.L", "BackLeg.L", (0.086, 0.205, 0.370)),
        ("BackLowerLeg.L", "BackUpperLeg.L", (HIND_X, 0.383, 0.168)),
        ("BackShoulder.R", "Back", (-0.050, 0.300, 0.590)),
        ("BackLeg.R", "BackShoulder.R", (-HIND_X, 0.300, 0.560)),
        ("BackUpperLeg.R", "BackLeg.R", (-0.086, 0.205, 0.370)),
        ("BackLowerLeg.R", "BackUpperLeg.R", (-HIND_X, 0.383, 0.168)),
    ] + [(f"Tail{k + 1}", "Back" if k == 0 else f"Tail{k}", TAIL[k]) for k in range(9)] + [
        ("PoleTargetBack.L", "Body", (HIND_X, -0.80, 0.20)),
        ("PoleTarget.L", "Body", (FORE_X, -0.75, 0.20)),
        ("PoleTargetBack.R", "Body", (-HIND_X, -0.80, 0.20)),
        ("PoleTarget.R", "Body", (-FORE_X, -0.75, 0.20)),
        ("IKBackLeg.L", None, (HIND_X, 0.346, 0.034)),
        ("FFB.L", "IKBackLeg.L", (HIND_X, 0.318, 0.014)),
        ("IKFrontLeg.L", None, (FORE_X, -0.318, 0.034)),
        ("FF.L", "IKFrontLeg.L", (FORE_X, -0.348, 0.014)),
        ("IKBackLeg.R", None, (-HIND_X, 0.346, 0.034)),
        ("FFB.R", "IKBackLeg.R", (-HIND_X, 0.318, 0.014)),
        ("IKFrontLeg.R", None, (-FORE_X, -0.318, 0.034)),
        ("FF.R", "IKFrontLeg.R", (-FORE_X, -0.348, 0.014)),
    ]


def _ends():
    return [
        ("FrontLowerLeg.L_end", "FrontLowerLeg.L", (FORE_X, -0.318, 0.034)),
        ("FrontLowerLeg.R_end", "FrontLowerLeg.R", (-FORE_X, -0.318, 0.034)),
        ("BackLowerLeg.L_end", "BackLowerLeg.L", (HIND_X, 0.346, 0.034)),
        ("BackLowerLeg.R_end", "BackLowerLeg.R", (-HIND_X, 0.346, 0.034)),
        ("Tail9_end", "Tail9", TAIL[9]),
        ("PoleTargetBack.L_end", "PoleTargetBack.L", (HIND_X, -0.80, 0.28)),
        ("PoleTarget.L_end", "PoleTarget.L", (FORE_X, -0.75, 0.28)),
        ("PoleTargetBack.R_end", "PoleTargetBack.R", (-HIND_X, -0.80, 0.28)),
        ("PoleTarget.R_end", "PoleTarget.R", (-FORE_X, -0.75, 0.28)),
        ("FFB.L_end", "FFB.L", (HIND_X, 0.290, 0.0)),
        ("FF.L_end", "FF.L", (FORE_X, -0.378, 0.0)),
        ("FFB.R_end", "FFB.R", (-HIND_X, 0.290, 0.0)),
        ("FF.R_end", "FF.R", (-FORE_X, -0.378, 0.0)),
    ]


def _extra():
    out = []
    for side, sx in (("L", 1.0), ("R", -1.0)):
        out += [(f"Ear1.{side}", "Head", _ear(EAR_BASE, sx), _ear(EAR_MID, sx)),
                (f"Ear2.{side}", f"Ear1.{side}", _ear(EAR_MID, sx), _ear(EAR_TIP, sx))]
    out.append(("Jaw", "Head", _at(0.050, -0.088), _at(0.160, -0.092)))
    return out


# The names the game's livestock path and ``horse.py``'s clips need.
QUATERNIUS_NAMES = ["Body", "Back", "Torso", "Torso2", "Torso3", "Neck1", "Neck2", "Neck3", "Head"] + [
    f"{b}.{s}" for s in ("L", "R") for b in ("FrontShoulder", "FrontUpperLeg", "FrontLowerLeg", "BackShoulder", "BackLeg",
                                              "BackUpperLeg", "BackLowerLeg", "PoleTargetBack", "PoleTarget", "IKBackLeg",
                                              "FFB", "IKFrontLeg", "FF", "Ear1")] + [f"Tail{k}" for k in range(1, 9)]

BONES, ENDS, EXTRA = _bones(), _ends(), _extra()


def joint_names():
    return [b[0] for b in BONES] + [e[0] for e in EXTRA]


# ---------------------------------------------------------------------------
# The coats, painted on the sculpt (``coat_at``) and baked into the colour
# map. sRGB 0..255. ``grizzle`` is how much of the agouti speckle shows
# (guard hairs with a pale band and a dark tip). The spots, the rosettes,
# the bands and the rings are a pattern in the layout (``_cells``): spheres
# round jittered points of a grid, cut by the skin, so they come in many
# sizes and shapes, as on the animals.

COATS = {
    "gato": dict(
        ground=(150, 140, 112), back=(132, 122, 96), warm=(164, 144, 104), pale=(220, 214, 196), white=(234, 231, 222),
        spot=(28, 25, 23), nose=(96, 66, 58), pad=(40, 34, 32), ear=(30, 27, 25),
    ),
    "puma": dict(
        ground=(160, 142, 120), back=(138, 122, 102), warm=(170, 140, 104), pale=(222, 214, 198), white=(238, 235, 228),
        spot=(44, 36, 31), nose=(146, 94, 80), pad=(46, 38, 34), ear=(66, 56, 48),
    ),
    "yagua": dict(
        ground=(192, 150, 96), back=(176, 130, 76), warm=(158, 102, 48), pale=(232, 222, 198), white=(240, 236, 226),
        spot=(24, 21, 19), nose=(132, 84, 70), pad=(42, 34, 30), ear=(26, 23, 21),
    ),
}
COAT = COATS[NAME]


def _hash(ix, iy, iz, salt):
    """A deterministic pseudo-random number in [0, 1) per grid cell."""
    v = math.sin(ix * 127.1 + iy * 311.7 + iz * 74.7 + salt * 53.3) * 43758.5453
    return v - math.floor(v)


def _cells(q, size, salt, stretch=(1.0, 1.0, 1.0)):
    """The nearest jittered grid point to ``q`` (a grid of ``size`` metres,
    squeezed by ``stretch`` per axis: spots longer along an axis with a
    smaller value): the distance to it in grid units, the offset to it
    (grid units) and its cell's hash."""
    g = tuple(q[i] * stretch[i] / size for i in range(3))
    base = tuple(math.floor(c) for c in g)
    best = None
    for dx in (-1, 0, 1):
        for dy in (-1, 0, 1):
            for dz in (-1, 0, 1):
                c = (base[0] + dx, base[1] + dy, base[2] + dz)
                p = (c[0] + _hash(*c, salt), c[1] + _hash(*c, salt + 1.7), c[2] + _hash(*c, salt + 3.1))
                off = (g[0] - p[0], g[1] - p[1], g[2] - p[2])
                d = math.sqrt(off[0] ** 2 + off[1] ** 2 + off[2] ** 2)
                if best is None or d < best[0]:
                    best = (d, off, _hash(*c, salt + 5.3))
    return best


def solid_spots(q, size, radius, salt, stretch=(1.0, 1.0, 1.0), keep=0.85, edge=0.06, ragged=0.0):
    """How dark (0..1) a pattern of solid spots is at ``q``: one round each
    of most cells (``keep``), ``radius`` grid units across, each its own
    size; ``ragged`` makes their edges irregular."""
    d, _, h = _cells(q, size, salt, stretch)
    if h > keep:
        return 0.0
    r = radius * (0.7 + 0.6 * ((h * 7.31) % 1.0))
    if ragged:
        r *= 1.0 + ragged * _wobble(q, 2.6 / size)
    return smooth((r - d) / edge)


def graded_spots(q, size_at, sizes, radius, salt, keep=0.85):
    """Solid spots whose size follows ``size_at(point)`` (layout metres)
    smoothly: layers of ``sizes`` (largest first), each spot kept or not
    by the size wanted at its own centre (each cell's own draw), so the
    spots are whole and their size changes gradually over the surface
    (the yaguareté's neck, from the body's spots to the face's)."""
    logs = [math.log(s) for s in sizes]
    dark = 0.0
    for i, s in enumerate(sizes):
        d, off, h = _cells(q, s, salt + 17.0 * i)
        if h > keep:
            continue
        centre = tuple(q[j] - off[j] * s for j in range(3))
        x = math.log(size_at(centre))
        # This layer's share where the size wanted is ``x`` (a tent over
        # the layers' sizes, the end layers taking everything beyond).
        lo = logs[i + 1] if i + 1 < len(logs) else -1e9
        hi = logs[i - 1] if i > 0 else 1e9
        if x >= logs[i]:
            share = 1.0 if i == 0 else max(0.0, (hi - x) / (hi - logs[i]))
        else:
            share = 1.0 if i + 1 == len(logs) else max(0.0, (x - lo) / (logs[i] - lo))
        if (h * 13.17) % 1.0 >= share:
            continue
        r = radius * (0.7 + 0.6 * ((h * 7.31) % 1.0))
        dark = max(dark, smooth((r - d) / 0.06))
    return dark


def neck_spot_size(q):
    """The yaguareté's spots' size (layout metres) along the neck: the
    body's (0.040) behind the shoulders, the face's (``_head_coat``)
    behind the jaw."""
    f = smooth((-0.33 - q[1]) / 0.15)
    return math.exp(math.log(0.040) + (math.log(0.012 * SP["HEAD"]) - math.log(0.040)) * f)


ROSETTE_CELL = 0.068  # layout metres between the rosettes' centres, along the body and round it
BLOTCH = (0.080, 0.035)  # a ring's blotch radius (cell units): least, and how much more at most


def rosettes(q, size, salt):
    """A yaguareté's rosettes at ``q``: (dark, inside), both 0..1. On the
    trunk's surface, unrolled round its long axis (along it, and the arc
    round it), each cell's rosette is a broken ring of black blotches of
    many sizes, one to three spots inside it, and the ground inside it a
    deeper gold."""
    x, y, z = q
    arc = math.atan2(x, z - 0.50) * 0.15
    g = (y / size, arc / size)
    base = (math.floor(g[0]), math.floor(g[1]))
    dark, inside = 0.0, 0.0
    for dx in (-1, 0, 1):
        for dy in (-1, 0, 1):
            c = (base[0] + dx, base[1] + dy)
            h = _hash(c[0], c[1], 0, salt + 5.3)
            p = (c[0] + 0.15 + 0.7 * _hash(c[0], c[1], 0, salt), c[1] + 0.15 + 0.7 * _hash(c[0], c[1], 0, salt + 1.7))
            off = (g[0] - p[0], g[1] - p[1])
            d = math.hypot(*off)
            r = 0.33 * (0.85 + 0.3 * h)
            if d > r + 0.25:
                continue
            inside = max(inside, smooth((r - 0.06 - d) / 0.06))
            n = 16 + int(h * 4)
            for k in range(n):
                hk = _hash(c[0] + k, c[1], 1, salt + 7.7)
                if math.sin(2.0 * math.pi * k / n * (2 + int(h * 2)) + 9.0 * h) > 0.72:
                    continue  # a gap in the ring, an arc of it
                ang = 2 * math.pi * (k + 0.3 * hk) / n + 6.3 * h
                e = math.hypot(off[0] - r * math.cos(ang), off[1] - r * math.sin(ang))
                rb = BLOTCH[0] + BLOTCH[1] * hk
                dark = max(dark, smooth((rb - e) / 0.03))
            for k in range(1 + int(h * 2.9)):
                cc = (0.35 * r * math.cos(4.1 * h + 2.4 * k), 0.35 * r * math.sin(4.1 * h + 2.4 * k))
                e = math.hypot(off[0] - cc[0], off[1] - cc[1])
                dark = max(dark, smooth((0.045 + 0.03 * ((h * 13.0 + k) % 1.0) - e) / 0.025))
    return dark, inside


def is_ear(q):
    """On an ear: near its axis (from its base, inside the skull, to its
    top), above the lowest fifth of it; not the crown between the ears."""
    if q[1] > -0.45 or q[2] < 0.50:
        return False
    sx = 1.0 if q[0] >= 0 else -1.0
    a, b = _ear(EAR_BASE, sx), _ear(EAR_TIP, sx)
    ab = tuple(b[i] - a[i] for i in range(3))
    ll = sum(c * c for c in ab)
    f = sum((q[i] - a[i]) * ab[i] for i in range(3)) / ll
    if f < 0.30 or f > 1.4:
        return False
    c = tuple(a[i] + ab[i] * min(f, 1.0) for i in range(3))
    return _dist(q, c) < 0.034 * SP["EAR"] * SP["HEAD"]


def tail_place(q):
    """How far along the tail ``q`` is (0 at its root, 1 at its tip) and
    how far from the tail's axis, or None off the tail."""
    if q[1] < 0.40 or abs(q[0]) > 0.12:
        return None
    best = None
    run, total = 0.0, sum(_dist(a, b) for a, b in zip(TAIL, TAIL[1:]))
    for a, b in zip(TAIL, TAIL[1:]):
        ab = tuple(b[i] - a[i] for i in range(3))
        ll = sum(c * c for c in ab)
        f = max(0.0, min(1.0, sum((q[i] - a[i]) * ab[i] for i in range(3)) / ll))
        c = tuple(a[i] + ab[i] * f for i in range(3))
        d = _dist(q, c)
        if best is None or d < best[0]:
            best = (d, (run + math.sqrt(ll) * f) / total, ab)
        run += math.sqrt(ll)
    d, f, ab = best
    if d > 0.075 or (f <= 0.0 and q[1] < 0.43):
        return None
    return f, d, ab


def _up_of(ab, m):
    """How much the normal ``m`` faces up, across the tail's segment ``ab``."""
    ln = math.sqrt(sum(v * v for v in ab))
    a = tuple(v / ln for v in ab)
    up = (0.0, -a[2], a[1]) if a[1] > 0 else (0.0, a[2], -a[1])
    if up[2] < 0:
        up = tuple(-c for c in up)
    return sum(m[i] * up[i] for i in range(3))


def coat_at(q, m):
    """The coat at the layout point ``q`` facing ``m``: its colour (sRGB
    0..255) and how much the agouti grizzle shows."""
    tail = tail_place(q)
    if tail is not None:
        return _tail_coat(q, m, *tail)
    rgb, g = _body_coat(q, m)
    head = smooth((-0.44 - q[1]) / 0.05) * smooth((q[2] - 0.40) / 0.06)
    if head > 0.0:
        h_rgb, h_g = _head_coat(q, m)
        rgb, g = _mix(rgb, h_rgb, head), g + (h_g - g) * head
    return rgb, g


def _under(q, m):
    """The pale underside: the belly, the inside of the legs, the throat
    and the chest between the forelegs."""
    x, y, z = q
    belly = smooth((-m[2] - 0.30) / 0.4) * smooth((0.47 - z) / 0.06)
    sx = 1.0 if x >= 0 else -1.0
    inner = 0.8 * smooth((-m[0] * sx + 0.1) / 0.8) * smooth((0.42 - z) / 0.06)
    chest = smooth((-0.30 - y) / 0.05) * smooth((-m[2] - 0.1) / 0.4) * smooth((0.50 - z) / 0.05)
    return max(belly, inner, chest)


def _is_leg(q):
    """How much ``q`` is on a leg below the body: the forelegs below the
    elbows, the hinds below the stifles."""
    x, y, z = q
    fore = smooth((0.36 - z) / 0.04) * smooth((-0.18 - y) / 0.04)
    hind = smooth((0.37 - z) / 0.06) * smooth((y - 0.16) / 0.04)
    return max(fore, hind)


def _body_coat(q, m):
    x, y, z = q
    c = COATS[NAME]
    # The back darker, the flanks the ground colour, warmer low down.
    top = smooth((m[2] - 0.45) / 0.4) * smooth((z - 0.55) / 0.06)
    rgb = _mix(c["ground"], c["back"], top)
    rgb = _mix(rgb, c["warm"], 0.25 * smooth((0.50 - z) / 0.08) * (1.0 - top))
    under = _under(q, m)
    g = {"gato": 0.55, "puma": 0.25, "yagua": 0.0}[NAME]
    if NAME == "puma":
        # Plain: the belly and the inside of the legs cream, the chest white.
        rgb = _mix(rgb, c["pale"], under)
        rgb = _mix(rgb, c["white"], smooth((-0.36 - y) / 0.05) * smooth((-m[2] - 0.0) / 0.4) * smooth((0.50 - z) / 0.05))
        # The backs of the forearms and the hocks a little darker, the
        # paws pale.
        rgb = _mix(rgb, c["back"], 0.4 * smooth((m[1] - 0.4) / 0.4) * _is_leg(q) * smooth((z - 0.06) / 0.04))
        rgb = _mix(rgb, c["pale"], 0.5 * smooth((0.05 - z) / 0.03))
        return rgb, g * (1.0 - under)
    if NAME == "yagua":
        rgb = _mix(rgb, c["white"], under)
        leg = _is_leg(q)
        # Rosettes on the back and the flanks; solid spots, smaller, on the
        # legs and the neck; big blotches on the belly; a line of spots
        # down the spine.
        body = (1.0 - leg) * (1.0 - under) * smooth((y + 0.36) / 0.06)
        ring, inside = rosettes(q, ROSETTE_CELL, 1.0) if body > 0.0 else (0.0, 0.0)
        rgb = _mix(rgb, c["warm"], 0.6 * inside * body * (1.0 - ring))
        neck = smooth((-0.30 - y) / 0.06) * (1.0 - under)
        # On the neck they shrink toward the face's small spots (v5 review:
        # the scale changed at once behind the jaw).
        small = solid_spots(q, 0.040, 0.32, 2.0) * leg if leg > 0.0 else 0.0
        if neck > 0.0:
            sizes = (0.040, 0.031, 0.024, 0.012 * SP["HEAD"])
            small = max(small, graded_spots(q, neck_spot_size, sizes, 0.34, 2.0, keep=0.9) * neck)
        blot = solid_spots(q, 0.080, 0.38, 3.0, (1.1, 0.5, 1.3), keep=0.75, ragged=0.35) * under if under > 0.0 else 0.0
        along = smooth((0.03 - abs(x)) / 0.02) * smooth((m[2] - 0.6) / 0.2)
        spine = solid_spots(q, 0.045, 0.40, 4.0, (1.6, 0.7, 1.0)) * along if along > 0.0 else 0.0
        # Small solid spots between the rosettes.
        filler = solid_spots(q, 0.034, 0.34, 9.0, keep=0.8) * body if body > 0.0 else 0.0
        dark = max(ring * body, filler * (1.0 - inside), small, blot * 0.95, spine)
        dark *= smooth((z - 0.045) / 0.02)  # the paws plain
        return _mix(rgb, c["spot"], dark), 0.0
    # The gato montés: grey buff, small solid spots all over (smaller on
    # the legs, where they run into bands across), the belly white and
    # spotted.
    rgb = _mix(rgb, c["white"], under)
    leg = _is_leg(q)
    spots = solid_spots(q, 0.019, 0.40, 11.0, (1.0, 0.7, 1.0), keep=0.96) if under < 1.0 else 0.0
    bands = 0.0
    if leg > 0.0:
        # Broken bands round the legs, about 3 cm apart (layout), slanted.
        w = math.sin((z + 0.12 * y) / 0.046 * 2 * math.pi + 1.3 * math.sin(7.0 * x + 3.0 * y))
        broken = 0.5 + 0.5 * _wobble(q, 60.0)
        bands = smooth((w - 0.55 + 0.25 * broken) / 0.15) * smooth((z - 0.07) / 0.03) * smooth((broken - 0.15) / 0.2)
    belly = solid_spots(q, 0.070, 0.33, 12.0, keep=0.75) if under > 0.0 else 0.0
    # Along the flanks and the back many spots run together into short
    # streaks lying with the body.
    flank = (1.0 - leg) * (1.0 - under) * smooth((y + 0.30) / 0.05)
    streaks = solid_spots(q, 0.032, 0.30, 13.0, (1.0, 0.33, 1.0), keep=0.6, ragged=0.3) * flank if flank > 0.0 else 0.0
    dark = max(streaks, spots * (1.0 - leg) * (1.0 - under), max(bands, 0.6 * spots) * leg * (1.0 - 0.5 * under), belly * under)
    dark *= smooth((z - 0.045) / 0.02)
    return _mix(rgb, c["spot"], dark), g * (1.0 - under) * (1.0 - dark)


def _tail_coat(q, m, f, d, ab):
    c = COATS[NAME]
    top = _up_of(ab, m)
    rgb = _mix(c["ground"], c["back"], smooth((top - 0.1) / 0.6))
    rgb = _mix(rgb, c["pale"], 0.6 * smooth((-top - 0.3) / 0.5))
    if NAME == "puma":
        tip = smooth((f - 0.86) / 0.06)
        rgb = _mix(rgb, c["spot"], tip)
        return rgb, 0.25 * (1.0 - tip)
    if NAME == "yagua":
        # Spots near the root, rings toward the tip, the tip black.
        spots = solid_spots(q, 0.050, 0.36, 5.0) * (1.0 - smooth((f - 0.45) / 0.15))
        rings = smooth((math.sin(f * 2 * math.pi * 5.0 + 0.8) - 0.15) / 0.2) * smooth((f - 0.40) / 0.12)
        tip = smooth((f - 0.90) / 0.05)
        return _mix(rgb, c["spot"], max(spots, rings, tip)), 0.0
    # The gato montés: about ten dark rings, close toward the tip.
    phase = f ** 0.85 * 10.0
    rings = smooth((math.sin(phase * 2 * math.pi + 0.5) - 0.05) / 0.2) * smooth((f - 0.04) / 0.06)
    rings = max(rings, 0.4 * solid_spots(q, 0.05, 0.3, 6.0) * (1.0 - smooth(f / 0.3)))
    tip = smooth((f - 0.93) / 0.04)
    return _mix(rgb, c["spot"], max(rings, tip)), 0.4 * (1.0 - max(rings, tip))


def _wobble(q, k):
    """A cheap smooth noise in about [-1, 1] at ``q`` (``k`` per metre)."""
    x, y, z = (c * k for c in q)
    return (math.sin(1.7 * x + 2.3 * y + 0.4) * math.sin(1.3 * y - 2.1 * z + 1.1) + 0.6 * math.sin(2.9 * z + 1.9 * x - 0.7)) / 1.6


def _head_coat(q, m):
    c = COATS[NAME]
    t, u, s = head_coords(q)
    if is_ear(q):
        # The back of the ear dark (black on the spotted cats, with a white
        # spot in the middle); inside pale, its rim dark.
        inside = smooth((-m[1] - 0.2) / 0.4)
        back = c["ear"]
        if NAME != "puma":
            e = SP["EAR"]
            mid_u = EAR_BASE[1] + (EAR_MID[1] - EAR_BASE[1]) * e * 0.9
            spot = smooth((0.010 * e - math.hypot(u - mid_u, abs(s) - EAR_BASE[2] - (EAR_MID[2] - EAR_BASE[2]) * e)) / 0.005)
            back = _mix(back, c["white"], 0.9 * spot)
        else:
            # The puma's: dark at the rim, the middle a greyer tawny.
            rim = smooth((u - EAR_BASE[1] - 0.030 * SP["EAR"]) / 0.015)
            back = _mix(_mix(c["ground"], c["ear"], 0.45), c["ear"], rim)
        rgb = _mix(back, _mix(c["pale"], c["ground"], 0.25), inside)
        return rgb, 0.0
    rgb = _mix(c["ground"], c["back"], 0.4 * smooth((u - 0.03) / 0.03))
    g = {"gato": 0.4, "puma": 0.2, "yagua": 0.0}[NAME]
    sx = 1.0 if s >= 0 else -1.0
    a = abs(s)
    # The upper lip and the muzzle under the nose, the chin, white.
    lip = smooth((t - 0.145) / 0.02) * smooth((-0.056 - u) / 0.010) * smooth((0.036 - abs(s)) / 0.010)
    chin = smooth((-0.088 - u) / 0.010) * smooth((t - 0.05) / 0.03) * smooth((0.032 - abs(s) + 0.4 * (-0.088 - u)) / 0.010)
    rgb = _mix(rgb, c["white"], max(lip, chin))
    # Pale fur under each eye.
    under_eye = smooth((0.012 - math.hypot(t - EYE[0] - 0.004, u + 0.020)) / 0.008) * smooth((0.016 - abs(a - EYE[2] + 0.002)) / 0.008)
    rgb = _mix(rgb, c["pale"], 0.4 * under_eye)
    # The dark rim round each eye, and the tear line from its inner corner
    # down beside the nose.
    eye_d = _dist(q, _at(EYE[0], EYE[1], sx * EYE[2]))
    k = SP["HEAD"]
    ke = k * SP["EYE"]
    rgb = _mix(rgb, c["spot"], 0.85 * smooth((0.0200 * ke - eye_d) / (0.002 * ke)) * smooth((eye_d - 0.0145 * ke) / (0.001 * ke)))
    tear = _line(t, u, (0.146, -0.010), (0.176, -0.046), 0.0034) * smooth((0.030 - a) / 0.006) * smooth((a - 0.010) / 0.004)
    # The dark line running back from the outer corner of each eye.
    eye_line = _line(t, u, (0.136, -0.004), (0.112, -0.010), 0.0026) * smooth((a - 0.040) / 0.006)
    if NAME == "puma":
        # The tear line only: no dark line back from the eye (the v5
        # review: a stray line no puma has).
        rgb = _mix(rgb, c["spot"], 0.75 * tear)
        # The dark moustache patch behind each whisker pad, up to the lip.
        patch = smooth((0.016 - math.hypot((t - 0.158) * 0.8, u + 0.072)) / 0.008) * smooth((a - 0.022) / 0.006)
        rgb = _mix(rgb, c["spot"], 0.9 * patch)
        return rgb, g * (1.0 - max(lip, chin))
    rgb = _mix(rgb, c["spot"], _whisker_rows(t, u, a))
    if NAME == "yagua":
        rgb = _mix(rgb, c["spot"], 0.6 * tear)
        # Small solid spots over the head and the cheeks, none on the
        # muzzle's white or the bridge of the nose; a line along the cheek
        # back from the corner of the mouth.
        # Finer toward the face (the photograph's spots), the neck's size at
        # the back of the head.
        spots = graded_spots(q, lambda p: 0.012 * k * (1.0 - 0.5 * smooth((head_coords(p)[0] - 0.06) / 0.05)),
                             (0.012 * k, 0.0095 * k, 0.0075 * k, 0.006 * k), 0.38, 7.0, keep=0.95)
        # Over the head and the cheeks down to the whisker pads; the bridge
        # of the nose and the muzzle's white plain.
        bridge = smooth((0.016 - a) / 0.004) * smooth((t - 0.140) / 0.01)
        spots *= smooth((0.172 - t) / 0.01) * (1.0 - max(lip, chin)) * (1.0 - 0.8 * under_eye) * (1.0 - bridge)
        line = _line(t, u, (0.145, -0.082), (0.070, -0.060), 0.0032) * smooth((a - 0.03) / 0.01)
        return _mix(rgb, c["spot"], max(spots, line)), 0.0
    # The gato montés: two dark stripes across each cheek from the eye
    # back, lines and spots over the crown, the tear line dark.
    rgb = _mix(rgb, c["spot"], 0.85 * max(tear, eye_line))
    # A white ring round each eye.
    # (Thin and soft, the fur showing through: v6 review.)
    ring = smooth((0.0022 * ke - abs(eye_d - 0.0210 * ke)) / (0.0016 * ke)) * (1.0 - smooth((u + 0.004) / 0.006))
    rgb = _mix(rgb, c["white"], 0.45 * ring)
    stripes = max(_line(t, u, (0.122, -0.010), (0.058, -0.022), 0.0030), _line(t, u, (0.122, -0.032), (0.064, -0.052), 0.0030))
    stripes *= smooth((a - 0.030) / 0.008)
    lines = max(smooth((0.0030 - abs(a - off)) / 0.0018) for off in (0.004, 0.012, 0.021)) * (0.55 + 0.45 * smooth(_wobble(q, 300.0) + 0.6)) * \
        smooth((u - 0.012) / 0.006) * smooth((0.128 - t) / 0.012) * smooth((t - 0.02) / 0.02)
    spots = solid_spots(q, 0.011 * k, 0.36, 8.0, keep=0.94)  # small, as the photographs' (v6) * smooth((0.140 - t) / 0.02) * (1.0 - max(lip, chin))
    # Rows of small spots over the cheeks, under the stripes.
    rows = solid_spots(q, 0.008 * k, 0.40, 14.0, (1.0, 0.6, 1.0), keep=0.9) * smooth((0.120 - t) / 0.01) * smooth((-0.030 - u) / 0.006) * \
        smooth((a - 0.030) / 0.006) * (1.0 - max(lip, chin))
    dark = max(stripes, 0.9 * lines, rows, spots * (1.0 - under_eye)) * (1.0 - ring)
    return _mix(rgb, c["spot"], dark), g * (1.0 - max(lip, chin, dark))


def _whisker_rows(t, u, a):
    """The rows of small dark spots the whiskers grow from, on the spotted
    cats' whisker pads (head frame)."""
    if NAME == "puma" or not (0.010 < a < 0.036 and 0.160 < t < 0.190):
        return 0.0
    row = (-0.062 - u) / 0.006
    col = (t - 0.168) / 0.005
    d = math.hypot(row - round(row), col - round(col))
    inside = 0 <= round(row) <= 3 and 0 <= round(col) <= 4
    return 0.85 * smooth((0.28 - d) / 0.1) if inside else 0.0


def _line(t, u, a, b, width):
    """How much (t, u) is on the stroke from ``a`` to ``b`` (head frame),
    ``width`` across."""
    ab = (b[0] - a[0], b[1] - a[1])
    f = max(0.0, min(1.0, ((t - a[0]) * ab[0] + (u - a[1]) * ab[1]) / (ab[0] ** 2 + ab[1] ** 2)))
    d = math.hypot(t - a[0] - ab[0] * f, u - a[1] - ab[1] * f)
    return smooth((width - d) / (0.7 * width))


# The hair cards' atlas: eight columns (``horse.py``'s ``HAIR_COLUMNS``: six
# of full locks, two of wispy ones), each with its own colours from the
# root (``f`` 0) to the tip (1).
GROUND, GROUND_2, PALE, SPOT, BACK, WHITE, WISP_WHITE, WISP_BLACK = range(8)


def card_colour(column, f, shift=0.0):
    """The colour (sRGB 0..255) of one hair of the cards' atlas ``column``
    at ``f`` along it (0 at the root): paler underfur at the root, then
    the coat's colour; the grizzled cats' guard hairs with a pale band and
    a dark tip. ``shift`` moves its band and its shade (each hair has its
    own, so no stripe runs across a tuft)."""
    c = COATS[NAME]
    if column in (GROUND, GROUND_2, BACK):
        base = c["back"] if column == BACK else c["ground"]
        root = _mix(base, c["pale"], 0.35)
        if NAME == "yagua":  # golden, the same to the tip
            rgb = _mix(root, base, smooth((f - 0.15 - shift) / 0.25))
            return _mix(rgb, c["warm"], 0.25 + shift) if column == GROUND_2 else rgb
        band = (0.50 if column != GROUND_2 else 0.58) + shift
        rgb = _mix(root, base, smooth((f - 0.2 - shift) / 0.2))
        tipped = 0.75 if NAME == "gato" else 0.35
        rgb = _mix(rgb, _mix(base, c["white"], 0.35), smooth((f - band) / 0.05) * (1.0 - smooth((f - band - 0.18) / 0.05)))
        return _mix(rgb, c["spot"], tipped * smooth((f - band - 0.22) / 0.06))
    if column == PALE:
        return _mix(_mix(c["pale"], c["ground"], 0.2), c["pale"], smooth(f / 0.4 - shift))
    if column == WHITE:
        return _mix(_mix(c["white"], c["pale"], 0.6), _mix(c["white"], c["pale"], 0.25), smooth(f / 0.4))
    if column == SPOT:
        return _mix(_mix(c["spot"], c["ground"], 0.25), c["spot"], smooth(f / 0.3))
    if column == WISP_WHITE:
        return (226, 222, 212)
    return (24, 22, 20)


# ---------------------------------------------------------------------------
# The face from a photograph. The procedural face stops short of the real
# animals' (L94): a photograph of each species, seen from the front, is
# projected onto the head's colour map. Its landmarks (pixels of the photo)
# are matched to the same points of the head; an affine camera fitted to
# them (``fit_affine``) carries each texel of the face to the photo; the
# photo's own shading is divided out (its luminance blurred wide) and its
# colour matched to the baked coat, and it is blended into the coat with a
# feathered weight (``face_weight``): full on the face, nothing on the ears,
# behind the jaw or where the photo sees the skin edge-on. The modelled
# eyes stay; the photo's lids and tear lines fall round them.
#
# Every photograph projected is from Wikimedia Commons or iNaturalist under
# CC0, public domain or CC BY (the user's authorization, L94: no CC BY-SA,
# NC or ND), checked on its page; credit them where the baked maps are used
# (tools/README.md). Photographs under other licences serve only as
# references (proportions, colours, patterns: L96), never as pixels.

FACE_PHOTOS = {
    "puma": dict(
        file="Cougar_17575969543_jpg.jpg", page="https://commons.wikimedia.org/wiki/File:Cougar_(17575969543).jpg",
        # Its shading leaves dark smudges over the cheeks and muzzle, and its
        # face is greyer than the coat (v6 review: a mask pasted on): the
        # middle scales flattened, the colour's saturation matched.
        author="Pete G", licence="CC BY 2.0", flatten=0.45, saturation=True,
        points=dict(eye_L=(655, 561), eye_R=(423, 566), nose_top=(550, 727), nose_bottom=(554, 780), lip=(554, 832),
                    ear_L=(677, 369), ear_R=(388, 373)),
        face=((552, 650), (235, 300)),
    ),
    "yagua": dict(
        file="Jaguar_Tenn_ji_Zoo_33686534416_jpg.jpg", page="https://commons.wikimedia.org/wiki/File:Jaguar,_Tenn%C5%8Dji_Zoo_(33686534416).jpg",
        author="Jin Kemoole", licence="CC BY 2.0",
        points=dict(eye_L=(1524, 1010), eye_R=(823, 1000), nose_top=(1163, 1660), nose_bottom=(1163, 1814), lip=(1153, 1928),
                    ear_L=(1720, 392), ear_R=(668, 433)),
        face=((1170, 1150), (760, 1000)),
    ),
    # The gato montés's from iNaturalist (v6, L96): a wild one in Argentina,
    # square to the camera, evenly lit, black stripes on a grey buff.
    "gato": dict(
        file="Leopardus_geoffroyi_inat_235096757.jpg", page="https://www.inaturalist.org/photos/235096757",
        author="Ben Costamagna", licence="CC BY 4.0", eyes=(82, 58),
        points=dict(eye_L=(1270, 830), eye_R=(875, 800), nose_top=(1055, 1010), nose_bottom=(1050, 1095), lip=(1040, 1160),
                    ear_L=(1330, 340), ear_R=(830, 330)),
        face=((1060, 800), (480, 560)),
    ),
}
FACE_LICENCES = ("CC0", "Public domain", "CC BY 2.0", "CC BY 2.5", "CC BY 3.0", "CC BY 4.0")


def face_landmarks():
    """The head's points matching ``FACE_PHOTOS``' ``points``, in the
    layout: the eyes, the top and the bottom of the nose leather, the upper
    lip under the philtrum, the inner base of each ear."""
    e = SP["EAR"]
    out = {}
    for side, sx in (("L", 1.0), ("R", -1.0)):
        out[f"eye_{side}"] = _at(EYE[0], EYE[1], sx * EYE[2])
        out[f"ear_{side}"] = _at(EAR_BASE[0], EAR_BASE[1] + 0.010, sx * (EAR_BASE[2] - 0.024 * e))
    out["nose_top"] = _at(NOSE_T - 0.004, -0.036)
    out["nose_bottom"] = _at(NOSE_T - 0.004, -0.056)
    out["lip"] = _at(NOSE_T - 0.014, -0.078)
    return out


def fit_affine(points3d, points2d, weights=None):
    """The affine camera (a 2 x 4 matrix) that best carries the 3D points
    to the 2D ones (least squares, each pair weighted)."""
    import numpy as np

    X = np.array([list(p) + [1.0] for p in points3d], float)
    Y = np.array(points2d, float)
    w = np.ones(len(X)) if weights is None else np.asarray(weights, float)
    sol, *_ = np.linalg.lstsq(X * w[:, None], Y * w[:, None], rcond=None)
    return sol.T


def fit_mapping(points3d, points2d, sigma, weights=None):
    """The camera from the 3D points to the photo's pixels: the affine fit
    (``fit_affine``) and a smooth correction (Gaussian bumps of width
    ``sigma`` round each point) that carries every landmark exactly onto
    its pixel; the head's proportions are never quite the photo's. Returns
    the affine matrix and the mapping ``f(points) -> pixels`` (numpy)."""
    import numpy as np

    A = fit_affine(points3d, points2d, weights)
    P = np.array(points3d, float)
    R = np.array(points2d, float) - np.concatenate([P, np.ones((len(P), 1))], axis=1) @ A.T
    G = np.exp(-((P[:, None, :] - P[None, :, :]) ** 2).sum(-1) / (2 * sigma * sigma))
    C = np.linalg.solve(G + 1e-9 * np.eye(len(P)), R)

    def f(X):
        X = np.atleast_2d(np.asarray(X, float))
        K = np.exp(-((X[:, None, :] - P[None, :, :]) ** 2).sum(-1) / (2 * sigma * sigma))
        return np.concatenate([X, np.ones((len(X), 1))], axis=1) @ A.T + K @ C

    return A, f


def view_direction(A):
    """Toward the camera of the affine camera ``A`` (unit, the side of -Y,
    where the animal faces)."""
    import numpy as np

    d = np.cross(A[0, :3], A[1, :3])
    d /= np.linalg.norm(d)
    return d if d[1] < 0 else -d


def face_weight(q, n, view):
    """How much of the photo the coat takes at the layout point ``q`` facing
    ``n``: the face forward of the ears and down to the throat, none on the
    ears, fading where the photo sees the skin edge-on (``view``)."""
    if q[1] > -0.44 or q[2] < 0.40 or is_ear(q):
        return 0.0
    t, u, s = head_coords(q)
    region = smooth((t - 0.045) / 0.045) * smooth((u + 0.125) / 0.025)
    facing = sum(n[i] * view[i] for i in range(3))
    # Off by about 70 degrees from the photo's view: further round, a front
    # photo's pixels are smeared across the cheek (the v5 3/4 views).
    return region * smooth((facing - 0.30) / 0.30)


def ellipse_weight(px, centre, radii):
    """Inside the photo's face ellipse (1), feathered to 0 at its rim."""
    r = math.hypot((px[0] - centre[0]) / radii[0], (px[1] - centre[1]) / radii[1])
    return smooth((1.0 - r) / 0.18)


# The head's share of the body's colour map. Smart projection gives every
# island its share of the surface, so the head (4 to 10 per cent of it)
# got about 5,000 texels of face on the puma (v5, L95): soft at the
# distances the game shows it. Before packing, the head's islands are
# scaled up until they hold ``HEAD_UV_SHARE`` of the islands' area, the rest
# of the body giving up a little (about 6 per cent of its resolution on the
# puma): one map still, no second material or draw call (L96).
HEAD_UV_SHARE = 0.17


def on_head(q):
    """Whether the layout point ``q`` is on the head (forward of the neck)."""
    return q[1] < -0.40 and q[2] > 0.38


def head_uv_scale(head, total):
    """How much (each way) the head's UV islands, ``head`` of the islands'
    ``total`` area, are scaled so they hold ``HEAD_UV_SHARE`` of it; never
    shrunk."""
    if head <= 0.0 or head >= total:
        return 1.0
    h = head / total
    return max(1.0, math.sqrt(HEAD_UV_SHARE * (1.0 - h) / (h * (1.0 - HEAD_UV_SHARE))))


def head_island(head_area, area):
    """Whether a UV island is the head's: most of its surface (``head_area``
    of ``area``) on the head."""
    return area > 0.0 and head_area > 0.5 * area


def box_blur(a, radius, passes=3):
    """A wide blur (three box passes, about a Gaussian) of the 2D array ``a``."""
    import numpy as np

    r = max(1, int(radius))
    for _ in range(passes):
        for axis in (0, 1):
            pad = [(0, 0), (0, 0)]
            pad[axis] = (r + 1, r)
            c = np.cumsum(np.pad(a, pad, mode="edge"), axis=axis)
            a = (np.take(c, range(2 * r + 1, c.shape[axis]), axis=axis) - np.take(c, range(0, c.shape[axis] - 2 * r - 1), axis=axis)) / (2 * r + 1)
    return a


# ---------------------------------------------------------------------------
# The gaits (``horse.py``'s plans): seconds, duty factor, stance sweep (m),
# the phase each leg lands, the swing lift (fore, hind) and the paw's fold.
# A cat walks in a lateral four-beat walk, the head low, each hind paw
# landing in the track of the fore; it gallops in a rotary gallop (the
# hinds land close together, then the fores; the spine curls under as the
# hinds reach forward and stretches as they push). The plans are a puma's
# shape made to each species' size and seconds; ``SPECIES`` gives the
# ground speed each is built to.


def plans():
    """The species' walk and gallop."""
    s, leg = SP["S"], SP["LEG"]
    reach = s * (0.6 + 0.4 * leg)
    ws, wv = SP["walk"]
    gs, gv, gd = SP["gallop"]
    walk = dict(seconds=ws, duty=0.64, sweep=wv * 0.64 * ws, phase=dict(LH=0.0, LF=0.25, RH=0.5, RF=0.75),
                lift=(0.075 * reach, 0.065 * reach), fold=(80.0, 62.0), centre=dict(F=0.02 * s, H=0.0), strike=6.0,
                breakover=30.0, sling=0.6)
    fore = gv * gd * gs
    gallop = dict(seconds=gs, duty=(gd, gd * 1.12), sweep=(fore, fore * 1.12 * 1.0), phase=dict(RH=0.0, LH=0.07, LF=0.40, RF=0.49),
                  lift=(0.15 * reach, 0.11 * reach), fold=(105.0, 90.0), centre=dict(F=-0.03 * s, H=-0.03 * s), strike=8.0,
                  breakover=40.0, reach=0.04 * reach, hip=0.6, hip_swing=0.78, hind_lead=0.8, hind_peak=0.6, fore_peak=1.2,
                  sling=1.0)
    return walk, gallop


def ground_speed(gait):
    """The speed the standing paws slide back at, m/s (fore legs)."""
    duty = gait["duty"][0] if isinstance(gait["duty"], tuple) else gait["duty"]
    sweep = gait["sweep"][0] if isinstance(gait["sweep"], tuple) else gait["sweep"]
    return sweep / (duty * gait["seconds"])


WALK, GALLOP = plans()


def configure(name):
    """Makes the module build ``name`` (``gato``, ``puma`` or ``yagua``)."""
    global NAME, SP, COAT, TAIL, BONES, ENDS, EXTRA, WALK, GALLOP
    NAME, SP, COAT = name, SPECIES[name], COATS[name]
    TAIL = _tail_points()
    BONES, ENDS, EXTRA = _bones(), _ends(), _extra()
    WALK, GALLOP = plans()


def _species_from_argv(argv):
    if "--species" in argv:
        i = argv.index("--species")
        if i + 1 < len(argv) and argv[i + 1] in SPECIES:
            return argv[i + 1]
    return "puma"


configure(_species_from_argv(sys.argv))


# ---------------------------------------------------------------------------
# Everything below runs in Blender.

if bpy is not None:
    from mathutils import Matrix, Quaternion, Vector

    sys.path.insert(0, HERE)
    import animal_rig as rig  # noqa: E402

    H = rig.Steps(os.path.join(HERE, "horse.py"), prefix="felid:")
    HORSE_BODY_MATRIX = H.body_matrix
    HORSE_WEIGHT_HAIR = H.weight_hair
    RIG_UNWRAP = rig.unwrap

    def head_unwrap(obj, margin=0.004):
        """``animal_rig.unwrap``, then the head's islands scaled up
        (``head_uv_scale``) and every island packed again."""
        import bmesh
        from bpy_extras import bmesh_utils

        RIG_UNWRAP(obj, margin)
        bm = bmesh.new()
        bm.from_mesh(obj.data)
        layer = bm.loops.layers.uv.active
        islands = []
        for island in bmesh_utils.bmesh_linked_uv_islands(bm, layer):
            area = sum(f.calc_area() for f in island)
            head = sum(f.calc_area() for f in island if on_head(unwarp(tuple(f.calc_center_median()))))
            islands.append((island, head_island(head, area), sum(abs(_uv_area(f, layer)) for f in island)))
        head_uv = sum(a for _, h, a in islands if h)
        total = sum(a for _, _, a in islands)
        k = head_uv_scale(head_uv, total)
        for island, is_head, _ in islands:
            if not is_head or k == 1.0:
                continue
            loops = [lp for f in island for lp in f.loops]
            cu = sum(lp[layer].uv[0] for lp in loops) / len(loops)
            cv = sum(lp[layer].uv[1] for lp in loops) / len(loops)
            for lp in loops:
                u, v = lp[layer].uv
                lp[layer].uv = (cu + (u - cu) * k, cv + (v - cv) * k)
        after = head_uv * k * k / (total + head_uv * (k * k - 1.0))
        print(f"felid: {NAME}: the head's islands {100 * head_uv / total:.1f}% of the map's islands, now {100 * after:.1f}% (x{k:.2f} each way)")
        bm.to_mesh(obj.data)
        bm.free()
        bpy.ops.object.mode_set(mode="EDIT")
        bpy.ops.mesh.select_all(action="SELECT")
        bpy.ops.uv.select_all(action="SELECT")
        bpy.ops.uv.pack_islands(rotate=True, margin=margin)
        bpy.ops.object.mode_set(mode="OBJECT")

    def _uv_area(face, layer):
        uv = [lp[layer].uv for lp in face.loops]
        return 0.5 * sum(uv[i][0] * uv[i - 1][1] - uv[i - 1][0] * uv[i][1] for i in range(len(uv)))

    rig.unwrap = head_unwrap


def at(t, u=0.0, s=0.0):
    return Vector(_at(t, u, s))


def head_matrix():
    return Matrix((Vector((1, 0, 0)), Vector(HEAD_A), Vector(HEAD_U))).transposed()


def bone_table():
    """Every bone: (name, parent, head, rotation, length): pointing at its
    first child (or its ``TAILS`` point, or its ``_end``), the extra bones
    from head to tail; on the model (``warp``)."""
    tails = {"Head": _at(NOSE_T - 0.01, -0.045), "Body": (0.0, 0.0, 0.40)}
    heads = {name: Vector(h) for name, _, h in BONES}
    ends = {bone: Vector(p) for _, bone, p in ENDS}
    first = {}
    for name, parent, _ in BONES:
        if parent and parent not in first:
            first[parent] = name
    out = []
    for name, parent, head in BONES:
        h = heads[name]
        if name in tails:
            tail = Vector(tails[name])
        elif name in first and (heads[first[name]] - h).length > 0.015:
            tail = heads[first[name]]
        elif name in ends:
            tail = ends[name]
        else:
            tail = h + Vector((0.0, 0.0, 0.04))
        h, tail = Vector(warp(h)), Vector(warp(tail))
        d = tail - h
        up = "Z" if abs(d.normalized().z) < 0.95 else "X"
        out.append((name, parent, tuple(h), tuple(d.to_track_quat("Y", up)), d.length))
    for name, parent, head, tail in EXTRA:
        head, tail = warp(head), warp(tail)
        d = Vector(tail) - Vector(head)
        up = "Z" if abs(d.normalized().z) < 0.95 else "X"
        out.append((name, parent, head, tuple(d.to_track_quat("Y", up)), d.length))
    return out


# ---------------------------------------------------------------------------
# The sculpt, in the layout: the furred silhouette (the coat's volume is part
# of it; the cards only break its outline).

BOUNDS = ((-0.27, -0.88, -0.004), (0.27, 1.12, 0.90))


def rot_x(deg):
    return Matrix.Rotation(math.radians(deg), 3, "X")


def paw(sdf, x, heel_y, toe_y, fore):
    """A cat's paw, round and broad: the main pad behind (three-lobed at
    its back edge), four oval toes in an arc in front, each with its pad,
    the knuckles arched over them and a groove between each two, so the
    toes read from the side as from above; the claws drawn in (none
    shows); the fore paw broader, its dewclaw high on the inside."""
    E = rig.sdf_ellipsoid
    back = 1.0 if toe_y < heel_y else -1.0  # +1: the toes are toward -Y
    k = SP["PAW"] * (1.0 if fore else 0.88)
    mid = heel_y - back * 0.018 * k
    sdf.add(E((x, mid, 0.021), (0.036 * k, 0.030 * k, 0.020)), 0.008)  # the paw's body, low behind the toes
    sdf.add(E((x, heel_y - back * 0.010 * k, 0.017), (0.030 * k, 0.022 * k, 0.017)), 0.008)  # the main pad
    for i, (dx, dy) in enumerate(((-0.035, 0.052), (-0.0125, 0.066), (0.0125, 0.066), (0.035, 0.052))):
        c = (x + dx * k, heel_y - back * dy * k, 0.016)
        sdf.add(E(c, (0.0145 * k, 0.019 * k, 0.016)), 0.006)  # the toe and its pad
        sdf.add(E((c[0], c[1] + back * 0.011 * k, 0.031), (0.0115 * k, 0.014 * k, 0.012)), 0.004)  # the knuckle, arched over it
    # The grooves between the toes, over the top and the front.
    for dx in (-0.024, 0.0, 0.024):
        sdf.carve(E((x + dx * k, heel_y - back * 0.070 * k, 0.024), (0.0034 * k, 0.026 * k, 0.022)), 0.003)
    if fore:
        sx = 1.0 if x > 0 else -1.0
        dew = (x - sx * 0.022 * k, heel_y + back * 0.010, 0.072)
        sdf.add(E(dew, (0.009, 0.010, 0.012)), 0.006)
    lo, hi = (x - 0.06, mid - 0.08, -0.05), (x + 0.06, mid + 0.08, 0.0)
    sdf.carve((lambda X, Y, Z: Z + 0.0 * X + 0.0 * Y, lo, hi), 0.003)


def legs(sdf, sx):
    """One side's legs: the shoulder blade riding high on the chest, the
    point of the shoulder, the heavy arm and the elbow, the thick forearm
    tapering to the wrist (its carpal pad behind), the short metacarpus and
    the paw; the long thigh and the hamstrings, the stifle well forward,
    the lean gaskin, the Achilles tendon, the hock, the long metatarsus and
    the paw."""
    E, R, T = rig.sdf_ellipsoid, rig.sdf_round_cone, rig.sdf_tube
    li = SP["LIMB"]
    x = FORE_X * sx
    sdf.add(E((sx * 0.072, -0.262, 0.560), (0.040 * li, 0.080, 0.100), rot_x(-22)), 0.05)  # shoulder blade
    sdf.add(E((sx * 0.080, -0.370, 0.470), (0.042 * li, 0.040, 0.050)), 0.04)  # point of the shoulder
    sdf.add(E((sx * 0.084, -0.330, 0.420), (0.056 * li, 0.070, 0.086), rot_x(32)), 0.05)  # the arm
    sdf.add(E((sx * 0.080, -0.268, 0.375), (0.038 * li, 0.040, 0.040)), 0.03)  # elbow
    sdf.add(T([(x, -0.290, 0.360), (x, -0.298, 0.290), (x, -0.302, 0.200), (x, -0.300, 0.135)],
              [(0.054 * li, 0.064 * li), (0.047 * li, 0.055 * li), (0.036 * li, 0.041 * li), (0.031 * li, 0.034 * li)]), 0.03)  # forearm
    sdf.add(E((x, -0.300, 0.112), (0.031 * li, 0.032 * li, 0.030)), 0.014)  # wrist
    sdf.add(E((x, -0.270, 0.100), (0.012, 0.010, 0.013)), 0.008)  # the carpal pad
    sdf.add(R((x, -0.303, 0.100), (x, -0.315, 0.050), 0.029 * li, 0.033 * li), 0.014)  # metacarpus
    paw(sdf, x, -0.305, -0.400, True)
    x = HIND_X * sx
    hi = li * SP["HIND"]
    sdf.add(E((sx * 0.078, 0.268, 0.520), (0.082 * hi, 0.130 * SP["HIND"], 0.140), rot_x(16)), 0.08)  # thigh
    sdf.add(E((sx * 0.068, 0.345, 0.455), (0.064 * hi, 0.068 * SP["HIND"], 0.120), rot_x(-10)), 0.07)  # hamstrings
    sdf.add(E((sx * 0.088, 0.205, 0.375), (0.032 * li, 0.034, 0.034)), 0.03)  # stifle
    sdf.add(T([(sx * 0.088, 0.218, 0.370), (sx * 0.088, 0.292, 0.272), (x, 0.368, 0.182)],
              [(0.058 * li, 0.070 * li), (0.042 * li, 0.052 * li), (0.025 * li, 0.028 * li)]), 0.035)  # gaskin
    sdf.add(R((x, 0.335, 0.330), (x, 0.397, 0.180), 0.014 * li, 0.013), 0.016)  # the Achilles tendon
    sdf.add(E((x, 0.386, 0.165), (0.025 * li, 0.030, 0.028)), 0.014)  # hock
    sdf.add(R((x, 0.381, 0.152), (x, 0.350, 0.050), 0.024 * li, 0.027 * li), 0.014)  # metatarsus
    paw(sdf, x, 0.354, 0.272, False)


def trunk(sdf):
    E, T = rig.sdf_ellipsoid, rig.sdf_tube
    nodes = _tube_nodes()
    sdf.add(T([p for p, _ in nodes], [r for _, r in nodes]))
    sdf.add(E(*WITHERS), 0.06)
    sdf.add(E(*BRISKET), 0.04)
    sdf.add(E(*CROUP), 0.05)
    k = SP["POUCH"]
    (c, r) = POUCH
    sdf.add(E(c, (r[0], r[1], r[2] * k)), 0.06)  # the loose skin of the belly
    # The neck, thick and short, rising to the head.
    n = SP["NECK"]
    sdf.add(T([(0, -0.300, 0.550), (0, -0.400, 0.562), (0, -0.455, 0.575), (0, -0.495, 0.592)],
              [(0.100 * n, 0.112 * n), (0.094 * n, 0.104 * n), (0.086 * n, 0.094 * n), (0.076 * n, 0.084 * n)]), 0.07)
    sdf.add(E((0, -0.440, 0.490), (0.066 * n, 0.056, 0.062), rot_x(-25)), 0.06)  # the throat
    # The tail: long, thick and even, its fur part of the volume.
    r0, r1 = SP["TAIL_R"]
    radii = [r0 * 1.15] + [r0 + (r1 - r0) * k / 9 for k in range(1, 10)]
    radii[-1] *= 0.7
    sdf.add(T(TAIL, [(0.95 * r, r) for r in radii]), 0.03)


def head(sdf):
    """A cat's round head: the domed cranium, the broad cheekbones and the
    jowls, the short muzzle with its swollen whisker pads, the nose
    leather, the strong chin, the forward eyes under the brow; the ears."""
    E, T = rig.sdf_ellipsoid, rig.sdf_tube
    Hx = head_matrix()
    n = NOSE_T
    k, w = SP["HEAD"], SP["HEAD"] * SP["HEAD_W"]
    j, br = SP["JAW"], SP["BROW"]
    sdf.add(E(at(0.072, 0.012), hr(0.063, 0.076, 0.047), Hx), 0.03)  # cranium, low and broad
    sdf.add(E(at(0.116, -0.004), hr(0.054, 0.028, 0.036), Hx), 0.025)  # the face: broad and flat round the eyes
    for sx in (1, -1):
        sdf.add(E(at(0.096, -0.020, sx * 0.050), hr(0.020, 0.050, 0.024), Hx), 0.03)  # zygomatic arch
        sdf.add(E(at(0.070, -0.052 - 0.006 * (j - 1), sx * (0.038 + 0.010 * (j - 1))), hr(0.028, 0.046 * j, 0.040 * j), Hx), 0.03)  # cheek over the jaw muscle
        sdf.add(E(at(0.170, -0.070, sx * 0.020), hr(0.020, 0.020, 0.020), Hx), 0.008)  # whisker pad
        # The brow ridge, overhanging the eye.
        sdf.add(E(at(0.132, 0.015, sx * 0.030), hr(0.020 * br, 0.018, 0.008 * br), Hx), 0.012)
    # The face falls steeply from the eyes: a stop under the brows, the
    # bridge of the nose down to the nose leather, a nose's height under
    # the eyes; the muzzle box with the whisker pads under it, the upper lip
    # parted by the philtrum, the mouth and the chin under them.
    sdf.add(T([at(0.125, -0.036), at(0.155, -0.056), at(0.170, -0.066)], [(0.038 * w, 0.040 * k), (0.033 * w, 0.034 * k), (0.028 * w, 0.028 * k)]), 0.02)  # muzzle
    sdf.add(T([at(0.146, 0.000), at(0.168, -0.016), at(n - 0.010, -0.040)], [(0.020 * w, 0.012 * k), (0.019 * w, 0.011 * k), (0.017 * w, 0.010 * k)]), 0.012)  # the bridge of the nose
    for sx in (1, -1):  # the face beside the bridge, from under the eyes down to the whisker pads
        sdf.add(E(at(0.150, -0.032, sx * 0.026), hr(0.020, 0.028, 0.026), Hx), 0.016)
    sdf.add(E(at(n - 0.008, -0.046), hr(0.015, 0.007, 0.011), Hx), 0.006)  # nose
    sdf.add(E(at(0.152, -0.100 - 0.010 * (j - 1)), hr(0.016 * j, 0.012, 0.013 * j), Hx), 0.008)  # the point of the chin
    sdf.add(T([at(0.050, -0.090), at(0.125, -0.095), at(0.156, -0.092)],
              [(0.036 * w * j, 0.028 * k * j), (0.026 * w * j, 0.021 * k * j), (0.015 * w * j, 0.014 * k * j)]), 0.016)  # chin and lower jaw
    for sx in (1, -1):  # the mouth's line, from under the whisker pads back to its corner
        mouth = E(at(0.140, -0.083, sx * 0.020), hr(0.013, 0.036, 0.0016), Hx)
        sdf.carve(mouth, 0.0015)
        OPENINGS.append(mouth[0])
        # The upper lip's edge: from the philtrum down and out round each
        # whisker pad to the mouth.
        lip = rig.sdf_tube([at(n - 0.013, -0.068, sx * 0.002), at(0.166, -0.084, sx * 0.012), at(0.150, -0.086, sx * 0.024)],
                           [(0.0018 * k, 0.0018 * k)] * 3)
        sdf.carve(lip, 0.0012)
        OPENINGS.append(lip[0])
    philtrum = E(at(n - 0.013, -0.062), hr(0.0018, 0.010, 0.010), Hx)
    sdf.carve(philtrum, 0.002)
    for sx in (1, -1):
        nostril = E(at(n - 0.002, -0.049, sx * 0.0060), hr(0.0040, 0.004, 0.0035), Hx)
        sdf.carve(nostril, 0.0015)
        OPENINGS.append(nostril[0])
        c = at(*EYE[:2], sx * EYE[2])
        look = eye_look(sx)
        lid = eye_frame(sx)
        ke = k * SP["EYE"]
        sdf.carve(E(c + look * 0.0105 * ke, (0.0175 * ke, 0.0090 * ke, 0.0100 * ke), lid), 0.004 * SP["EYE"])  # the opening in the face, almond
        ears(sdf, sx)


def eye_look(sx):
    """Where an eye looks: forward, a little out (layout)."""
    return Vector((sx * 0.20, -1.0, 0.02)).normalized()


def eye_frame(sx):
    """The eye's axes as a matrix's columns: across, along the look, up."""
    look = eye_look(sx)
    up = Vector((0.0, 0.0, 1.0))
    up = (up - look * up.dot(look)).normalized()
    side = look.cross(up)
    # The almond slants: its inner corner lower than its outer.
    tilt = Matrix.Rotation(math.radians(-sx * 18.0), 3, look)
    side, up = tilt @ side, tilt @ up
    return Matrix((side, look, up)).transposed()


OPENINGS = []
CAVITIES = []


def ears(sdf, sx):
    """A short, wide, rounded ear, upright and set wide on the skull, its
    front a deep cup facing forward and out."""
    base, mid, tip = (Vector(_ear(p, sx)) for p in (EAR_BASE, EAR_MID, EAR_TIP))
    front = Vector((sx * 0.35, -1.0, 0.0)).normalized()
    e, k = SP["EAR"], SP["HEAD"]
    w = e * k
    sdf.add(rig.sdf_tube([base, mid, tip], [(0.0100 * w, 0.036 * w), (0.0085 * w, 0.033 * w), (0.0070 * w, 0.025 * w)], side=front), 0.010 * w)
    f = front * 0.0055 * w
    cavity = rig.sdf_tube([base + f + Vector((0, 0, 0.006 * w)), mid + f, tip + f * 0.7],
                          [(0.0058 * w, 0.026 * w), (0.0050 * w, 0.025 * w), (0.0035 * w, 0.017 * w)], side=front)
    sdf.carve(cavity, 0.003 * w)
    CAVITIES.append(cavity[0])


def details(sdf):
    """What reads through a cat's short coat: the spine of each shoulder
    blade, the last ribs, the hollow of the flank, the seam between the
    thigh's muscles, the forehead's furrow."""
    for sx in (1, -1):
        out = (sx, 0, 0)
        sdf.ridge([(0, -0.230, 0.600), (0, -0.262, 0.540), (0, -0.300, 0.480)], out, 0.012, 0.0022, 0.008)
        sdf.groove([(0, 0.060, 0.580), (0, 0.075, 0.510), (0, 0.085, 0.450)], out, 0.024, 0.0010, 0.008)
        sdf.groove([(sx * 0.05, 0.160, 0.570), (sx * 0.05, 0.170, 0.520), (sx * 0.05, 0.168, 0.470)], out, 0.026, 0.0016, 0.010)
        sdf.groove([(sx * 0.05, 0.300, 0.530), (sx * 0.05, 0.305, 0.470), (sx * 0.05, 0.298, 0.405)], (sx, 0.5, 0), 0.014, 0.0020, 0.006)
    sdf.groove([at(0.040, -0.004), at(0.090, -0.012), at(0.125, -0.030)], Vector(HEAD_U), 0.007, 0.0012, 0.003)


def sculpt(voxel):
    """The dense sculpt on the model: one watertight surface, built in the
    layout and warped."""
    OPENINGS.clear()
    CAVITIES.clear()
    sdf = rig.Sdf(*BOUNDS, voxel * 0.5)
    trunk(sdf)
    for sx in (1, -1):
        legs(sdf, sx)
    head(sdf)
    details(sdf)
    obj = sdf.to_mesh(f"{SP['mesh']}Sculpt")
    warp_mesh(obj)
    return obj, sdf


def warp_mesh(obj):
    """Makes a mesh laid out in the layout the model (``warp``)."""
    for v in obj.data.vertices:
        v.co = warp(v.co)
    obj.data.update()


def warped(spots):
    """``spots`` (hair strands laid out against the sculpt) on the model:
    their points warped and their widths scaled."""
    def fn(sdf):
        s = SP["S"]
        return [(tuple(Vector(warp(p)) for p in st[0]), [w * s for w in st[1]]) + tuple(st[2:]) for st in spots(sdf)]
    return fn


# ---------------------------------------------------------------------------
# Regions of the skin, in the layout.


def _fn_min(fns, p):
    import numpy as np

    a = np.array([[[p[0]]]]), np.array([[[p[1]]]]), np.array([[[p[2]]]])
    return float(min(fn(*a).ravel()[0] for fn in fns))


def opening(p):
    if p[2] < 0.40 or p[1] > -0.55 or not OPENINGS:
        return 1.0
    return _fn_min(OPENINGS, p)


def ear_cavity(p):
    if not is_ear(p) or not CAVITIES:
        return 1.0
    return _fn_min(CAVITIES, p)


def nose(p):
    """The bare nose leather: the triangle at the muzzle's tip."""
    if p[1] > -0.60:
        return 0.0
    t, u, s = head_coords(p)
    # A triangle, broad at the top, narrowing down to the philtrum.
    return smooth((t - NOSE_T + 0.016) / 0.003) * smooth((u + 0.056) / 0.003) * smooth((-0.033 - u) / 0.003) * \
        smooth((0.005 + 0.50 * (u + 0.056) - abs(s)) / 0.002)


def pads(p):
    """The bare pads under the paws."""
    return smooth((0.0065 - p[2]) / 0.003)


def bare(p):
    """Bare skin: (the nose, the dark lips and lid rims and the pads)."""
    if p[2] < 0.03:
        return 0.0, pads(p)
    if p[1] > -0.55 or p[2] < 0.40:
        return 0.0, 0.0
    lips = smooth((0.0035 - opening(p)) / 0.003) if p[1] < -0.62 else 0.0
    t, u, s = head_coords(p)
    e = _dist(p, _at(EYE[0], EYE[1], math.copysign(EYE[2], s)))
    k = SP["HEAD"]
    ke = k * SP["EYE"]
    rim = smooth((0.0185 * ke - e) / (0.0015 * ke)) * smooth((e - 0.0150 * ke) / (0.001 * ke))
    return nose(p), max(lips, rim)


def material_of(p, n):
    return "Main"


MATERIALS = ["Main", "Hair", "Eye_Black", "Eye_Iris"]


def fallback():
    return {
        "Main": tuple(c / 255.0 for c in COATS[NAME]["ground"]),
        "Hair": (1.0, 1.0, 1.0),
        "Eye_Black": (0.02, 0.02, 0.02),
        "Eye_Iris": SP["iris"],
    }


def hair_direction(p, n):
    """Which way the fur lies (layout): back from the nose over the head,
    up the ears, back and down the neck, back along the body and down its
    sides, down the legs, back along the belly, along the tail to its tip."""
    p, n = Vector(p), Vector(n)
    tail = tail_place(p)
    if tail is not None:
        d = Vector(tail[2])
    elif p.y < -0.46 and p.z > 0.50:
        if is_ear(p):
            d = Vector((0.0, 0.25, 1.0))
        else:
            # Radiating from the nose: up the bridge and back over the brow
            # and the crown, out and back over the cheeks, down the chin.
            d = (p - at(NOSE_T - 0.010, -0.050)).normalized() + Vector((0.0, 0.35, 0.0))
    elif p.y < -0.28 and p.z > 0.46:
        d = Vector((0.0, 0.55, -0.8))
    elif p.z < 0.40 and (abs(abs(p.x) - FORE_X) < 0.05 or abs(abs(p.x) - HIND_X) < 0.05):
        d = Vector((0.0, 0.1, -1.0))
    elif n.z < -0.5:
        d = Vector((0.0, 1.0, 0.1))
    else:
        d = Vector((0.0, 1.0, -0.35 * max(0.0, -n.z + abs(n.x))))
    d -= n * d.dot(n)
    return d.normalized() if d.length > 1e-6 else Vector((0.0, 1.0, 0.0))


def tone(p, n):
    """A coat-independent shade: the eye's hollow, the ears' cups, soft
    mottling."""
    p, n = Vector(p), Vector(n)
    v = 0.93
    if p.z > 0.50 and p.y < -0.46:
        t, u, s = head_coords(p)
        e = (p - at(EYE[0], EYE[1], math.copysign(EYE[2], s))).length
        v -= 0.10 * smooth((0.026 * SP["HEAD"] - e) / 0.006)
        if is_ear(p):
            c = ear_cavity(p)
            v -= 0.18 * smooth((0.006 - abs(c - 0.003)) / 0.004)
    v += 0.04 * rig.noise(p, (8.0, 8.0, 8.0), seed=2.7) + 0.03 * rig.noise(p, (22.0, 22.0, 22.0), seed=3.9)
    return max(0.3, min(1.0, v))


def dense_zone(p):
    """Where the coat's colour changes on a line (round the eyes, the
    muzzle, the ears, the paws), the game mesh keeps more vertices."""
    if p[2] < 0.05:
        return True
    if p[1] < -0.46 and p[2] > 0.50:
        t, u, s = head_coords(p)
        return t > 0.14 or is_ear(p) or abs(t - EYE[0]) < 0.03 and abs(abs(s) - EYE[2]) < 0.025
    return False


# ---------------------------------------------------------------------------
# The detail baked onto the game mesh: ``fox.py``'s ``paint_sculpt`` with a
# cat's short fur and its coat.


def paint_sculpt(obj):
    """Masks on the dense sculpt (``Mask``: R fur, G unused, B bare skin; A
    the tone), the coat (``Coat``: RGB linear, A the grizzle) and the lie
    of the fur as weights on nine axes (``FlowA``-``FlowC``); and the
    material that turns them into the detail baked onto the game mesh:
    fine short hairs gathered into locks along the lie, the agouti speckle
    where the coat is grizzled, the cobbles of the nose leather and the
    pads. ``set_output(mat, "rough")`` makes it emit the roughness instead
    (dull fur, the moist nose a little glossy)."""
    import numpy as np

    mesh = obj.data
    count = len(mesh.vertices)
    co = np.empty(count * 3, np.float32)
    mesh.vertices.foreach_get("co", co)
    nor = np.empty(count * 3, np.float32)
    mesh.vertices.foreach_get("normal", nor)
    co, nor = co.reshape(-1, 3), nor.reshape(-1, 3)
    m = np.empty((count, 4), np.float32)
    coat = np.empty((count, 4), np.float32)
    f = np.ones((3, count, 4), np.float32)
    c = COATS[NAME]
    for i in range(count):
        p, n = unwarp(tuple(co[i])), tuple(nor[i])
        leather, dark = bare(p)
        skin = max(leather, dark)
        m[i] = (1.0 - skin, 0.0, skin, tone(p, n))
        rgb, g = coat_at(p, n)
        rgb = _mix(rgb, c["pad"], dark)
        rgb = _mix(rgb, c["nose"], leather)
        coat[i, :3] = H.srgb(rgb)[:3]
        coat[i, 3] = g * (1.0 - skin)
        w = H.flow_weights(hair_direction(p, n))
        for k in range(3):
            f[k, i, :3] = w[3 * k : 3 * k + 3]
    mesh.color_attributes.new("Mask", "FLOAT_COLOR", "POINT").data.foreach_set("color", m.ravel())
    mesh.color_attributes.new("Coat", "FLOAT_COLOR", "POINT").data.foreach_set("color", coat.ravel())
    for k, ch in enumerate("ABC"):
        mesh.color_attributes.new(f"Flow{ch}", "FLOAT_COLOR", "POINT").data.foreach_set("color", f[k].ravel())
    mat = bpy.data.materials.new("Sculpt")
    mat.use_nodes = True
    nt = mat.node_tree
    nodes, link = nt.nodes, nt.links.new
    bsdf = nodes["Principled BSDF"]
    coord = nodes.new("ShaderNodeTexCoord")

    def attribute(layer):
        col = nodes.new("ShaderNodeVertexColor")
        col.layer_name = layer
        sep = nodes.new("ShaderNodeSeparateColor")
        link(col.outputs["Color"], sep.inputs["Color"])
        return [sep.outputs["Red"], sep.outputs["Green"], sep.outputs["Blue"]], col.outputs["Alpha"], col.outputs["Color"]

    (fur, _, skin_), tone_out, _ = attribute("Mask")
    _, grizzle, coat_rgb = attribute("Coat")
    weights = []
    for ch in "ABC":
        weights += attribute(f"Flow{ch}")[0]

    def math_(op, a, b=None, value=None):
        node = nodes.new("ShaderNodeMath")
        node.operation = op
        link(a, node.inputs[0])
        if b is not None:
            link(b, node.inputs[1])
        elif value is not None:
            node.inputs[1].default_value = value
        return node.outputs[0]

    def strokes(axis, along, across, detail=3.0, offset=(0.0, 0.0, 0.0)):
        """Noise stretched along ``axis``: hairs, or locks of them."""
        vr = nodes.new("ShaderNodeVectorRotate")
        vr.rotation_type = "EULER_XYZ"
        vr.inputs["Rotation"].default_value = Vector(axis).rotation_difference(Vector((0, 1, 0))).to_euler()
        link(coord.outputs["Object"], vr.inputs["Vector"])
        mp = nodes.new("ShaderNodeMapping")
        mp.inputs["Scale"].default_value = (across, along, across)
        mp.inputs["Location"].default_value = offset
        link(vr.outputs["Vector"], mp.inputs["Vector"])
        tex = nodes.new("ShaderNodeTexNoise")
        tex.inputs["Scale"].default_value = 1.0
        tex.inputs["Detail"].default_value = detail
        tex.inputs["Roughness"].default_value = 0.55
        link(mp.outputs["Vector"], tex.inputs["Vector"])
        return tex.outputs["Fac"]

    def mix(a, b, factor):
        node = nodes.new("ShaderNodeMix")
        node.data_type = "FLOAT"
        link(factor, node.inputs["Factor"])
        link(a, node.inputs["A"])
        link(b, node.inputs["B"])
        return node.outputs["Result"]

    def mix_rgb(a, b, factor, blend="MIX"):
        node = nodes.new("ShaderNodeMix")
        node.data_type = "RGBA"
        node.blend_type = blend
        link(factor, node.inputs["Factor"])
        for sock, v in (("A", a), ("B", b)):
            if isinstance(v, tuple):
                node.inputs[sock].default_value = v
            else:
                link(v, node.inputs[sock])
        return node.outputs["Result"]

    # Fine short hairs (2 cm long, half a millimetre wide) gathered into
    # locks (about 4 mm wide), blended by the lie's weights; and the agouti
    # speckle, a pattern of single guard hairs of its own along the lie.
    hair, speck = None, None
    for k, (axis, w) in enumerate(zip(H.FLOW_AXES, weights)):
        lay = math_("ADD", math_("MULTIPLY", strokes(axis, 45.0, 1500.0), None, 0.45),
                    math_("MULTIPLY", strokes(axis, 14.0, 260.0, detail=2.0), None, 0.55))
        guard = strokes(axis, 30.0, 950.0, detail=1.0, offset=(0.37 * k, 0.11 * k, 0.53))
        term, sterm = math_("MULTIPLY", lay, w), math_("MULTIPLY", guard, w)
        hair = term if hair is None else math_("ADD", hair, term)
        speck = sterm if speck is None else math_("ADD", speck, sterm)
    # Bare skin: the cobbles of the nose leather and the pads.
    cobbles = nodes.new("ShaderNodeTexVoronoi")
    cobbles.inputs["Scale"].default_value = 900.0
    link(coord.outputs["Object"], cobbles.inputs["Vector"])
    skin_h = math_("ADD", math_("MULTIPLY", cobbles.outputs["Distance"], None, 0.8), None, 0.1)
    height = mix(skin_h, hair, fur)
    bump = nodes.new("ShaderNodeBump")
    bump.inputs["Strength"].default_value = 0.9
    bump.inputs["Distance"].default_value = 0.0010
    link(height, bump.inputs["Height"])
    link(bump.outputs["Normal"], bsdf.inputs["Normal"])
    # Colour: the coat, darker between the locks, times the tone; where it
    # is grizzled, the guard hairs' dark tips and pale bands over it.
    shade = nodes.new("ShaderNodeMapRange")
    shade.inputs["From Min"].default_value = 0.3
    shade.inputs["From Max"].default_value = 0.7
    shade.inputs["To Min"].default_value = 0.62
    shade.inputs["To Max"].default_value = 1.06
    link(height, shade.inputs["Value"])
    lum = math_("MULTIPLY", shade.outputs["Result"], tone_out)
    dark = nodes.new("ShaderNodeMapRange")
    dark.inputs["From Min"].default_value, dark.inputs["From Max"].default_value = 0.53, 0.60
    link(speck, dark.inputs["Value"])
    pale = nodes.new("ShaderNodeMapRange")
    pale.inputs["From Min"].default_value, pale.inputs["From Max"].default_value = 0.45, 0.39
    link(speck, pale.inputs["Value"])
    band = H.srgb(_mix(c["ground"], c["white"], 0.45))
    col = mix_rgb(coat_rgb, band, math_("MULTIPLY", math_("MULTIPLY", pale.outputs["Result"], grizzle), fur))
    tip = H.srgb(c["spot"])
    col = mix_rgb(col, tip, math_("MULTIPLY", math_("MULTIPLY", dark.outputs["Result"], grizzle), fur))
    gray = nodes.new("ShaderNodeCombineColor")
    for ch in ("Red", "Green", "Blue"):
        link(lum, gray.inputs[ch])
    col = mix_rgb(col, gray.outputs["Color"], math_("ADD", fur, None, 0.0), blend="MULTIPLY")
    link(col, bsdf.inputs["Base Color"])
    # Roughness: dull fur with a soft sheen along each lock; the nose moist.
    sheen = nodes.new("ShaderNodeMapRange")
    sheen.inputs["From Min"].default_value = 0.3
    sheen.inputs["From Max"].default_value = 0.7
    sheen.inputs["To Min"].default_value = 0.90
    sheen.inputs["To Max"].default_value = 0.56
    link(height, sheen.inputs["Value"])
    nose_wet = nodes.new("ShaderNodeValue")
    nose_wet.outputs[0].default_value = 0.38
    rough = mix(nose_wet.outputs[0], sheen.outputs["Result"], fur)
    emit = nodes.new("ShaderNodeEmission")
    emit.name = "Rough"
    link(rough, emit.inputs["Color"])
    obj.data.materials.clear()
    obj.data.materials.append(mat)


# ---------------------------------------------------------------------------
# The eyes: large, forward, the iris amber to green-gold, the pupil round
# (the puma, the yaguareté) or an upright oval (the gato montés).


def eyes():
    """An eyeball in each socket, looking forward: ``Eye_Iris`` over its
    visible cap with the pupil (``Eye_Black``) in it, the rest
    ``Eye_Black``."""
    out = []
    t, u, s = EYE
    pa, pb = SP["pupil"]
    segments = 32 if pb > 2.0 * pa else 24  # a slit pupil needs the finer fan
    for sx in (1, -1):
        c = at(t, u, sx * s) - eye_look(sx) * 0.0085 * SP["HEAD"] * SP["EYE"]
        frame = eye_frame(sx)
        # The sphere's pole along the look: the pupil and the iris are its
        # polar rings, round with few triangles.
        rotation = Vector((0, 0, 1)).rotation_difference(frame.col[1]).to_euler()
        bpy.ops.mesh.primitive_uv_sphere_add(segments=segments, ring_count=24, radius=EYE_RADIUS * SP["HEAD"] * SP["EYE"], location=c, rotation=rotation)
        eye = bpy.context.active_object
        eye.name = "Eye"
        bpy.ops.object.transform_apply(location=True, rotation=True, scale=True)
        eye.data.materials.append(bpy.data.materials["Eye_Black"])
        eye.data.materials.append(bpy.data.materials["Eye_Iris"])
        side, look, up = (frame.col[i] for i in range(3))
        for poly in eye.data.polygons:
            d = (poly.center - c).normalized()
            a, b = d.dot(side), d.dot(up)
            pupil = d.dot(look) > 0 and (a / pa) ** 2 + (b / pb) ** 2 < 1.0
            # The iris, a dark ring round it (the limbus) and the pupil.
            poly.material_index = 1 if d.dot(look) > 0.40 and not pupil else 0
            poly.use_smooth = True
        warp_mesh(eye)
        out.append(eye)
    return out


# ---------------------------------------------------------------------------
# Hair cards: the fur's tufts, the tail's short fur and the whiskers on a
# coloured atlas.


def _jit(i, salt):
    return H.jitter(i, salt)


def _tuft(sdf, root, d, length, width, column, bone="Body", gap=0.002, lift=0.25, droop=0.3, points=5):
    """A tuft from ``root`` along ``d`` (lifted ``lift`` off the skin at
    first, then lying back down ``droop``), ``points`` points long."""
    n = sdf.sample(root)[1]
    start = (Vector(d).normalized() + n * lift).normalized()
    grav = (Vector(d).normalized() - n * droop + Vector((0, 0, -0.3))).normalized()
    pts, outs = H.fall(sdf, root, start, length, gap, steps=points - 1, gravity=grav, stiff=0.6)
    widths = [width * (1.0 - 0.55 * (k / (points - 1)) ** 1.4) for k in range(points)]
    return (pts, widths, outs, column, bone, 0.0)


def _on_trunk(sdf, y, ang, zc=None):
    """The surface point of the trunk at ``y``, ``ang`` degrees round from
    the top (positive to the left)."""
    zc = 0.52 if zc is None else zc
    a = math.radians(ang)
    d = Vector((math.sin(a), 0.0, math.cos(a)))
    return sdf.surface(Vector((0.0, y, zc)), d, reach=0.3), d


def card_column(p, n):
    """The atlas column whose hair matches the coat at ``p`` facing ``n``."""
    rgb, g = coat_at(p, n)
    c = COATS[NAME]
    lum = sum(rgb) / 3.0
    if lum < (sum(c["spot"]) / 3.0 + sum(c["ground"]) / 3.0) * 0.5:
        return SPOT
    if lum > sum(c["white"]) / 3.0 - 10:
        return WHITE
    if lum > (sum(c["pale"]) / 3.0 + sum(c["ground"]) / 3.0) * 0.5:
        return PALE
    if lum < (sum(c["back"]) / 3.0 + sum(c["ground"]) / 3.0) * 0.5:
        return BACK
    return GROUND if (p[0] * 97.0 + p[1] * 31.0) % 1.0 > 0.5 else GROUND_2


# How many tufts of each kind a species carries: the gato montés's coat is
# long, dense and soft, the big cats' short and sleek.
TUFTS = {
    "gato": dict(cheeks=22, elbows=8, pants=14, back=36, belly=22, tail=200, length=1.4),
    "puma": dict(cheeks=14, elbows=6, pants=10, back=12, belly=12, tail=130, length=1.0),
    "yagua": dict(cheeks=28, elbows=6, pants=10, back=10, belly=22, tail=110, length=0.9),
}


def fur_spots(sdf):
    """The fur's tufts, five points each, lying with the coat and coloured
    as it is under them (``card_column``): the ruff of the jowls flaring
    back, the elbows, the backs of the thighs, a few along the nape and the
    spine, and the fringe of the belly's loose skin (only between the legs:
    a tuft there never hangs over a leg)."""
    out = []
    i = 0
    n_ = TUFTS[NAME]
    ln = n_["length"]

    def add(root, d, length, width, bone="Body", lift=0.3, droop=0.15, column=None, give=0.0):
        n = sdf.sample(root)[1]
        col = card_column(tuple(root), tuple(n)) if column is None else column
        out.append(_tuft(sdf, root, d, length * ln, width, col, bone, lift=lift, droop=droop)[:5] + (give,))

    for sx in (1, -1):
        for k in range(n_["cheeks"]):  # the jowls' ruff
            i += 1
            t = 0.030 + 0.075 * _jit(i, 1)
            u = -0.050 - 0.040 * _jit(i, 2)
            root = sdf.surface(at(t, u, 0.0), Vector((sx, 0.0, -0.3)), reach=0.15)
            add(root, Vector((sx * 0.35, 0.9, -0.35)), 0.018 + 0.010 * _jit(i, 3), 0.0065 + 0.003 * _jit(i, 4), "Head", lift=0.15,
                droop=0.4)
        for k in range(n_["elbows"]):  # the elbows
            i += 1
            root = sdf.surface(Vector((sx * 0.05, -0.268 + 0.03 * _jit(i, 5), 0.375 + 0.03 * _jit(i, 6))), Vector((sx * 0.5, 0.5, -0.5)),
                               reach=0.15)
            add(root, Vector((0, 0.5, -1)), 0.020 + 0.008 * _jit(i, 7), 0.008, lift=0.3)
        for k in range(n_["pants"]):  # the backs of the thighs
            i += 1
            z = 0.40 + 0.14 * _jit(i, 8)
            root = sdf.surface(Vector((sx * 0.06, 0.32, z)), Vector((sx * 0.3, 1.0, -0.1)), reach=0.2)
            add(root, Vector((sx * 0.1, 0.4, -1.0)), 0.018 + 0.010 * _jit(i, 9), 0.008 + 0.003 * _jit(i, 10), f"BackLeg.{'L' if sx > 0 else 'R'}",
                lift=0.25, droop=0.3, give=1.0)
    for k in range(n_["back"]):  # the nape and the spine
        i += 1
        y = -0.40 + 0.74 * _jit(i, 17)
        ang = (_jit(i, 18) - 0.5) * 110.0
        zc = 0.52 + 0.08 * smooth((-0.30 - y) / 0.15)
        root, d0 = _on_trunk(sdf, y, ang, zc)
        add(root, Vector((0, 1.0, -0.3)) + d0 * 0.1, 0.014 + 0.008 * _jit(i, 20), 0.006 + 0.002 * _jit(i, 21), lift=0.10, droop=0.4)
    for k in range(n_["belly"]):  # the belly's fringe, between the legs only
        i += 1
        y = -0.16 + 0.34 * _jit(i, 27)
        ang = 180.0 + (_jit(i, 28) - 0.5) * 70.0
        root, d0 = _on_trunk(sdf, y, ang, 0.50)
        add(root, Vector((0, 0.6, -1.0)), 0.020 + 0.010 * _jit(i, 29), 0.008, lift=0.1, droop=0.3)
    return out


def whisker_spots(sdf):
    """A cat's whiskers: long, stiff and white, in rows on the whisker pads,
    sweeping out, back and a little down; a few over each eye and on the
    cheeks. Four points each."""
    out = []
    k_ = SP["HEAD"]
    for sx in (1, -1):
        for k in range(26):
            row = k % 4
            t = 0.168 + 0.014 * ((k // 4) / 6.0) + 0.003 * _jit(k, 40 + sx)
            u = -0.062 - 0.0060 * row
            root = sdf.surface(at(t, u, 0.0), Vector((sx, 0, -0.1)), reach=0.15)
            d = (Vector((sx * 0.85, 0, 0)) + Vector(HEAD_A) * 0.25 - Vector(HEAD_U) * (0.05 + 0.10 * row + 0.1 * _jit(k, 45))).normalized()
            length = (0.065 + 0.045 * _jit(k, 47 + sx) - 0.008 * row) * k_
            pts = [root + d * length * j / 3 + Vector((0, length * 0.10 * (j / 3) ** 2, -length * 0.10 * (j / 3) ** 2)) for j in range(4)]
            out.append((pts, [0.0026 * k_, 0.0022 * k_, 0.0016 * k_, 0.0008 * k_], [Vector((0, 0, 1))] * 4, WISP_WHITE, "Head", 0.0))
        for k in range(4):  # over the eye
            root = sdf.surface(at(EYE[0] - 0.010 + 0.006 * k, EYE[1] + 0.022, sx * (EYE[2] - 0.004)), Vector(HEAD_U), reach=0.1)
            d = (Vector(HEAD_U) * 0.6 + Vector((sx * 0.6, 0, 0)) - Vector(HEAD_A) * 0.3).normalized()
            length = (0.035 + 0.01 * _jit(k, 49)) * k_
            pts = [root + d * length * j / 3 for j in range(4)]
            out.append((pts, [0.0022 * k_, 0.0018 * k_, 0.0012 * k_, 0.0006 * k_], [Vector((0, 0, 1))] * 4, WISP_WHITE, "Head", 0.0))
        for k in range(3):  # on the cheek
            root = sdf.surface(at(0.095 + 0.008 * k, -0.050, 0.0), Vector((sx, 0, 0)), reach=0.15)
            d = (Vector((sx * 0.8, 0, 0)) - Vector(HEAD_A) * 0.4).normalized()
            length = 0.035 * k_
            pts = [root + d * length * j / 3 for j in range(4)]
            out.append((pts, [0.0022 * k_, 0.0018 * k_, 0.0012 * k_, 0.0006 * k_], [Vector((0, 0, 1))] * 4, WISP_WHITE, "Head", 0.0))
    return out


def tail_spots(sdf):
    """The tail's short fur: locks all along it, radiating round it and
    lying close back toward the tip, a little longer at the tip, coloured
    as the coat under them (the rings, the spots, the black tip). None on
    the root's underside: a lock there hangs behind the thighs. Six points
    each."""
    path = rig._catmull([Vector(p) for p in TAIL], 4)
    total = len(path) - 1
    strands = []
    ln = TUFTS[NAME]["length"]
    for i in range(TUFTS[NAME]["tail"]):
        f = 0.03 + 0.97 * _jit(i, 51) ** 0.9
        k = min(int(f * total), total - 1)
        c = path[k][0].lerp(path[k + 1][0], f * total - k)
        along = (path[k + 1][0] - path[k][0]).normalized()
        ang = 2 * math.pi * _jit(i, 53)
        side = Vector((1.0, 0.0, 0.0))
        up = along.cross(side).normalized()
        if up.z < 0:
            up = -up
        out = (side * math.cos(ang) + up * math.sin(ang)).normalized()
        if not tail_lock_ok(f, out.dot(up)):
            continue
        root = sdf.surface(c, out, reach=0.15)
        length = (0.022 + 0.012 * _jit(i, 55) + 0.022 * smooth((f - 0.8) / 0.15)) * ln
        twist = side * (_jit(i, 61) - 0.5) * 0.6 + up * (_jit(i, 69) - 0.5) * 0.3
        pts, outs = H.fall(sdf, root, (out * (0.12 + 0.2 * _jit(i, 63)) + along + twist).normalized(), length, 0.0015,
                           steps=5, gravity=(along + out * 0.05 + Vector((0, 0, -0.15))).normalized(), stiff=0.4)
        n = sdf.sample(root)[1]
        col = card_column(tuple(root), tuple(n))
        if col in (PALE, WHITE):
            col = GROUND_2  # the underside's fur: the coat's, not white streaks
        width = (0.005 + 0.005 * _jit(i, 59) ** 2) * ln
        widths = [width * (0.7 + 0.6 * (j / 5) ** 0.6) * (1.0 - 0.6 * (j / 5) ** 2.5) for j in range(6)]
        strands.append((pts, widths, outs, col))
    return strands


def tail_lock_ok(f, up):
    """Whether a lock may grow at ``f`` along the tail facing ``up`` (the
    tail's own up): never on the underside near the root, where it would
    hang off the tail behind the thighs."""
    return not (f < 0.18 and up < 0.2)


def fur_atlas(size, seed=7):
    """The cards' atlas: in each of the eight columns a tuft of fine hairs
    hanging from the top, each its own length and its own colour along it
    (``card_colour``, its band shifted at random), a third of them short
    pale underfur beneath the guard hairs; the two wispy columns a single
    stiff hair (the whiskers). Returns colour (sRGB), opacity, normal
    (OpenGL) and roughness arrays, rows bottom up."""
    import numpy as np

    rng = np.random.default_rng(seed)
    cols, h = 8, size
    w = size // cols
    rgb = np.zeros((h, size, 3), np.float32)
    alpha = np.zeros((h, size), np.float32)
    height = np.zeros((h, size), np.float32)
    ys, xs = np.mgrid[0:h, 0:w].astype(np.float32)
    v = ys / (h - 1)  # 0 at the root (the top row), 1 at the tip
    u = xs / (w - 1)
    samples = np.linspace(0.0, 1.0, 64)
    for c in range(cols):
        wisp = c in (WISP_WHITE, WISP_BLACK)
        col_rgb = np.zeros((h, w, 3), np.float32)
        col_a = np.zeros((h, w), np.float32)
        col_h = np.zeros((h, w), np.float32)
        for k in range(3 if wisp else 160):
            under = not wisp and k % 3 == 0
            x0 = rng.uniform(0.42, 0.58) if wisp else rng.uniform(0.05, 0.95)
            length = rng.uniform(0.35, 0.6) if under else (rng.uniform(0.9, 0.99) if wisp else rng.uniform(0.65, 0.99))
            phase, freq = rng.uniform(0, 6.3), rng.uniform(2.0, 6.0)
            path = x0 + (0.5 - x0) * rng.uniform(0.1, 0.45) * v ** 1.5 + rng.uniform(-0.08, 0.08) * v ** 2 + 0.006 * np.sin(freq * v * 6.28 + phase) * v
            width = rng.uniform(2.2, 3.0) / w * (1.0 - 0.7 * np.clip(v / length, 0, 1)) if wisp else \
                rng.uniform(0.7, 1.6) / w * (1.0 - 0.6 * np.clip(v / length, 0, 1))
            dist = np.abs(u - path) / width
            on = (dist < 1.0) & (v < length)
            fade = np.clip((length - v) / 0.05, 0, 1)
            along = np.clip(v / length, 0.0, 1.0)
            if under:
                table = np.array([_mix(card_colour(c, 0.0), card_colour(c, 0.6), 0.5) for x in samples], np.float32)
            else:
                shift = rng.uniform(-0.12, 0.12)
                table = np.array([card_colour(c, x, shift) for x in samples], np.float32)
            idx = np.clip((along * 63).astype(int), 0, 63)
            shade = rng.uniform(0.84, 1.08) * (0.74 + 0.26 * np.clip(along * 3.0, 0, 1)) * (0.8 if under else 1.0)
            hgt = np.where(on, (1.0 - dist * dist) * fade * (0.5 if under else 1.0), 0.0)
            take = hgt > col_h
            col_h = np.where(take, hgt, col_h)
            col_rgb = np.where(take[..., None], table[idx] / 255.0 * shade[..., None], col_rgb)
            col_a = np.maximum(col_a, np.where(on & (fade > 0.3), 1.0, 0.0))
        sl = slice(c * w, (c + 1) * w)
        rgb[:, sl] = col_rgb
        alpha[:, sl] = col_a
        height[:, sl] = col_h
        inside = col_a > 0.5
        if inside.any():
            rgb[:, sl][~inside] = col_rgb[inside].mean(axis=0)
    rgb, alpha, height = rgb[::-1], alpha[::-1], height[::-1]
    gy, gx = np.gradient(height)
    n = np.stack([-gx * 2.0, -gy * 0.5, np.ones_like(gx)], axis=-1)
    n /= np.linalg.norm(n, axis=-1, keepdims=True)
    rough = np.full((h, size, 3), 0.70, np.float32)
    rough[:, WISP_WHITE * w :] = 0.42
    return np.clip(rgb, 0.0, 1.0), np.repeat(alpha[..., None], 3, axis=-1), n * 0.5 + 0.5, rough


def use_fur_atlas(images):
    """Writes ``fur_atlas`` into the hair images ``horse.py``'s build made
    (the same size and columns)."""
    import numpy as np

    size = images["hair"]["color"].size[0]
    arrays = dict(zip(("color", "opacity", "normal", "rough"), fur_atlas(size)))
    for name, a in arrays.items():
        img = images["hair"][name]
        rgba = np.ones((size, size, 4), np.float32)
        rgba[:, :, :3] = a
        img.pixels.foreach_set(rgba.ravel())
        img.update()


# ---------------------------------------------------------------------------
# Skinning.


def weight_hair(cards, arm, kinds):
    """``horse.py``'s ``weight_hair``; and the fur's tufts (all but the
    tail's) take no weight from the tail where they sit mostly on the body
    (a tuft on the tail's own skin keeps following it)."""
    HORSE_WEIGHT_HAIR(cards, arm, kinds)
    if kinds and kinds[0] == "tail":
        # A tail card follows its root's bones whole: per vertex, the tip
        # of a card near the curl of the tail took the next turn's bone
        # and the card tore off it.
        per_card = len(cards.data.vertices) // len(kinds)
        verts = cards.data.vertices
        for c in range(len(kinds)):
            root = {cards.vertex_groups[g.group].name: g.weight for g in verts[c * per_card].groups}
            for j in range(1, per_card):
                v = verts[c * per_card + j]
                for g in list(v.groups):
                    cards.vertex_groups[g.group].remove([v.index])
                for name, w in root.items():
                    cards.vertex_groups[name].add([v.index], w, "REPLACE")
        return
    for v in cards.data.vertices:
        tail = [g for g in v.groups if cards.vertex_groups[g.group].name.startswith("Tail")]
        if not tail or sum(g.weight for g in tail) > 0.5:
            continue
        for g in tail:
            cards.vertex_groups[g.group].remove([v.index])
        rest = [g for g in v.groups]
        if rest:
            total = sum(g.weight for g in rest)
            for g in rest:
                g.weight = g.weight / total if total > 1e-6 else 1.0 / len(rest)
        else:
            cards.vertex_groups["Back"].add([v.index], 1.0, "REPLACE")


def _groups(body):
    groups = {g.name: g for g in body.vertex_groups}

    def group(name):
        if name not in groups:
            groups[name] = body.vertex_groups.new(name=name)
        return groups[name]

    return group


def _normalize(body, limit=True):
    bpy.ops.object.select_all(action="DESELECT")
    body.select_set(True)
    bpy.context.view_layer.objects.active = body
    if limit:
        bpy.ops.object.vertex_group_limit_total(group_select_mode="ALL", limit=4)
    bpy.ops.object.vertex_group_normalize_all(lock_active=False)


def tail_only(body, arm):
    """The tail is skin of its own: each vertex of it takes its weight from
    the tail chain only (the nearest point of it), blended into the
    croup's weights over its root."""
    chain = [(f"Tail{k + 1}", arm.data.bones[f"Tail{k + 1}"].head_local.copy()) for k in range(9)]
    chain.append((None, arm.data.bones["Tail9"].tail_local.copy()))
    group = _groups(body)
    for v in body.data.vertices:
        p, q = v.co, unwarp(v.co)
        place = tail_place(q)
        if place is None or q[1] < 0.42:
            continue
        own = smooth((q[1] - 0.425) / 0.045)
        best = None
        for i in range(len(chain) - 1):
            a, b = chain[i][1], chain[i + 1][1]
            t = max(0.0, min(1.0, (p - a).dot(b - a) / (b - a).length_squared))
            d = (p - (a + (b - a) * t)).length
            if best is None or d < best[0]:
                best = (d, i, t)
        _, i, t = best
        want = {chain[i][0]: 1.0 - t}
        if chain[i + 1][0]:
            want[chain[i + 1][0]] = want.get(chain[i + 1][0], 0.0) + t
        else:
            want[chain[i][0]] = 1.0
        old = {body.vertex_groups[g.group].name: g.weight * (1.0 - own) for g in v.groups}
        for g in list(v.groups):
            body.vertex_groups[g.group].remove([v.index])
        for k in set(old) | set(want):
            w = old.get(k, 0.0) + own * want.get(k, 0.0)
            if w > 1e-4:
                group(k).add([v.index], w, "REPLACE")
    _normalize(body)


def ears_only(body, arm):
    """Each ear's skin follows its own two bones (from its base to its
    top), blended into the head's over the base."""
    group = _groups(body)
    e = SP["EAR"]
    base_u, mid_u, tip_u = EAR_BASE[1], EAR_BASE[1] + (EAR_MID[1] - EAR_BASE[1]) * e, EAR_BASE[1] + (EAR_TIP[1] - EAR_BASE[1]) * e
    mid = (mid_u - base_u) / (tip_u - base_u)
    for v in body.data.vertices:
        p = unwarp(v.co)
        if not is_ear(p):
            continue
        t, u, s = head_coords(p)
        side = "L" if s > 0 else "R"
        f = max(0.0, min(1.0, (u - base_u) / (tip_u - base_u)))
        own = smooth((u - 0.004) / 0.014)
        tip = smooth((f - mid + 0.15) / 0.3)
        want = {f"Ear1.{side}": own * (1.0 - tip), f"Ear2.{side}": own * tip, "Head": 1.0 - own}
        for g in list(v.groups):
            body.vertex_groups[g.group].remove([v.index])
        for k, w in want.items():
            if w > 1e-4:
                group(k).add([v.index], w, "REPLACE")
    _normalize(body, limit=False)


def elbows(body):
    """``horse.py``'s ``elbows`` on a cat: the trunk behind and above each
    elbow follows the chest, not the swinging foreleg."""
    groups = {g.name: g for g in body.vertex_groups}
    torso = groups.get("Torso2") or body.vertex_groups.new(name="Torso2")
    for v in body.data.vertices:
        p = Vector(unwarp(v.co))
        trunk_ = max(smooth((p.y + 0.25) / 0.07), smooth((0.05 - abs(p.x)) / 0.02)) * smooth((p.z - 0.40) / 0.06)
        if trunk_ <= 0.0:
            continue
        for side in ("L", "R"):
            for name in (f"FrontUpperLeg.{side}", f"FrontShoulder.{side}"):
                g = groups.get(name)
                if g is None:
                    continue
                try:
                    w = g.weight(v.index)
                except RuntimeError:
                    continue
                moved = w * 0.85 * trunk_
                g.add([v.index], w - moved, "REPLACE")
                torso.add([v.index], moved, "ADD")
    _normalize(body, limit=False)
    for v in body.data.vertices:
        v.select = any((Vector(unwarp(v.co)) - Vector((sx * 0.075, -0.27, 0.38))).length < 0.11 for sx in (1, -1))
    bpy.ops.object.mode_set(mode="EDIT")
    bpy.ops.object.vertex_group_smooth(group_select_mode="ALL", factor=0.5, repeat=6)
    bpy.ops.object.mode_set(mode="OBJECT")
    _normalize(body)


NOT_SKIN = {"Body", "PoleTarget.L", "PoleTarget.R", "PoleTargetBack.L", "PoleTargetBack.R",
            "FrontShoulder.L", "FrontShoulder.R", "BackShoulder.L", "BackShoulder.R", "Torso"}
FOLDS = [((0.09, 0.27, 0.58), 0.10, 8), ((0.095, 0.21, 0.38), 0.09, 12)]


def skin(body, arm):
    """Heat weights from the bones that move the skin (on the cat scaled up
    to a horse's size, the heat solver's comfortable size); then the
    elbows, the tail apart, the ears and the folds round the hip and the
    stifle smoothed."""
    for b in arm.data.bones:
        b.use_deform = b.name not in NOT_SKIN
    k = 1.5 / withers_height()
    for o in (body, arm):
        o.scale = (k, k, k)
    bpy.context.view_layer.update()
    rig.bind(body, arm)
    for o in (body, arm):
        o.scale = (1.0, 1.0, 1.0)
    body.matrix_parent_inverse = Matrix.Identity(4)
    bpy.context.view_layer.update()
    for b in arm.data.bones:
        b.use_deform = True
    elbows(body)
    tail_only(body, arm)
    ears_only(body, arm)
    H.use(FOLDS=[(warp(c), r * SP["S"], n) for c, r, n in FOLDS])
    H.smooth_folds(body)


# ---------------------------------------------------------------------------
# Clips: ``horse.py``'s machinery with a cat's carriage: the head carried
# low, level with the back or below it; the shoulder blades rolling over
# the back at the walk; the tail hanging in its curve at rest, the tip
# alive, carried out behind and up at the gallop; the spine curling and
# stretching with every bound.

NECK = ("Neck1", "Neck2", "Neck3", "Head")
PIVOT = (0.0, 0.0, 0.50)
HOOF = {"F": dict(toe=-0.400, heel=-0.305), "H": dict(toe=0.272, heel=0.354)}


def body_matrix(lift=0.0, nose_down=0.0, roll=0.0, yaw=0.0, forward=0.0, pivot=None):
    """``horse.py``'s ``body_matrix`` about the cat's own pivot (``lift`` and
    ``forward`` in metres of the model)."""
    return HORSE_BODY_MATRIX(lift, nose_down, roll, yaw, forward, Vector(warp(PIVOT)) if pivot is None else pivot)


def pose_face(pose, ears=((0, 0), (0, 0)), jaw=0.0, grind=0.0, lip=0.0, nostrils=0.0):
    """Ears turned back (degrees) and swivelled out, per side (L, R), their
    tops following a little; the jaw open."""
    lat, up, along = H.head_axes(pose)
    for (back, out), side, sx in zip(ears, ("L", "R"), (1, -1)):
        pose.turn(f"Ear1.{side}", H.rot(up, sx * out) @ H.rot(lat, -back))
        pose.turn(f"Ear2.{side}", H.rot(lat, -0.2 * back))
    pose.turn("Jaw", H.rot(up, grind) @ H.rot(lat, -jaw))


def pose_hair(pose, tail=(0.0, 0.0), tail_lift=0.0, lag=0.0, t=0.0, cycles=1.0, tail_curve=0.0, tail_whip=0.0, tip=(0.0, 0.0), **_):
    """The tail swings ``tail`` (amplitude, phase) down its chain with
    ``lag`` cycles per bone; its root lifted ``tail_lift`` degrees (back and
    up, over its first three bones), each bone after them bending
    ``tail_curve`` back down; a wave of ``tail_whip`` runs down it; ``tip``
    (amplitude, cycles) flicks its last three bones sideways, as a cat's
    tip twitches."""
    amp, phase = tail
    for k, name in enumerate(TAIL_CHAIN):
        m = pose.posed(name).to_3x3() @ pose.rest(name).to_3x3().inverted()
        side = amp * H.wave(t, cycles, phase - lag * k) / len(TAIL_CHAIN) * (1.0 + 0.3 * k)
        if k >= 6 and tip[0]:
            side += tip[0] * H.wave(t, tip[1], 0.1 * k) * (k - 5) / 3.0
        lift = tail_lift * (0.45, 0.35, 0.2)[k] if k < 3 else -tail_curve
        lift += tail_whip * H.wave(t, cycles, 0.25 - lag * k) * (0.4 + 0.12 * k)
        pose.turn(name, H.rot(m @ H.Z, side) @ H.rot(m @ H.X, lift))


def standing(arm, body, neck=(0, 0, 0, 0), yaw=0.0, t=0.0, ears=((0, 0), (0, 0)), jaw=0.0, offsets=None, rests=None, hair=None,
             flex=0.0):
    pose = H.Pose(arm)
    pose.carry("Body", body)
    if flex:
        pose.turn("Torso2", H.rot(body.to_3x3() @ H.X, flex))
    H.pose_neck(pose, body, neck, yaw)
    pose_face(pose, ears, jaw)
    pose_hair(pose, **dict(dict(tail=(3.0, 0.0), lag=0.06, tip=(10.0, 3.0)), **(hair or {})), t=t)
    stand_legs(pose, body, offsets, rests)
    return pose


def stand_legs(pose, body, offsets=None, rests=None):
    """``horse.py``'s ``stand_legs`` with the thoracic sling on the
    forelegs: as the chest drops the trunk sinks between the shoulder
    blades and the standing forelegs stay nearly straight."""
    offsets = offsets or {}
    rests = rests or {}
    for leg in H.LEGS:
        dy, toe = offsets.get(leg, 0.0), rests.get(leg, 0.0)
        f = H.hoof_place(leg, dy, toe)
        fore = leg[1] == "F"
        H.pose_leg(pose, body, leg, f, toe, slide=-0.7 * dy if fore else 0.0, sling=1.0 if fore else 0.0)


def idle(arm, t):
    # Alert: breathing, the head turning to a sound and back, the ears
    # swivelling one at a time, the tip of the tail twitching.
    s = SP["S"]
    body = body_matrix(lift=0.003 * s * H.wave(t, 3), nose_down=0.3 * H.wave(t, 1, 0.25))
    look = H.ease((t - 0.15) / 0.1) * (1 - H.ease((t - 0.55) / 0.12))
    neck = (2.0 + 1.0 * H.wave(t, 1, 0.1), 1.0, 0.0, 2.0 * look + 1.0 * H.wave(t, 2, 0.3))
    ears = ((-5 + 25 * H.flick(t, 0.3, 0.07), 20 * H.flick(t, 0.3, 0.07)), (-5 + 10 * look, 30 * H.flick(t, 0.75, 0.08)))
    return standing(arm, body, neck, yaw=28.0 * look, t=t, ears=ears, hair=dict(tail=(4.0, 0.0), tip=(14.0, 4.0)))


def idle_2(arm, t):
    # Looking back over the shoulder, then a slow blink and a yawn; a hind
    # paw shifted.
    s = SP["S"]
    body = body_matrix(lift=0.003 * s * H.wave(t, 3), roll=0.8, nose_down=0.3)
    back = H.ease(t / 0.2) * (1 - H.ease((t - 0.45) / 0.15))
    yawn = H.ease((t - 0.58) / 0.08) * (1 - H.ease((t - 0.78) / 0.08))
    neck = (2.0, 2.0, 0.0, 10.0 * yawn)
    ears = ((10 * back + 20 * yawn, 25 * back), (-5 + 20 * H.flick(t, 0.6) + 20 * yawn, 10))
    return standing(arm, body, neck, yaw=-60.0 * back, t=t, ears=ears, jaw=34.0 * yawn, offsets={"RH": -0.03 * s},
                    rests={"RH": -12.0}, hair=dict(tail=(6.0, 0.3), tip=(18.0, 2.0)))


def idle_headlow(arm, t):
    # Nose to the ground, following a scent from side to side, the ears
    # pricked forward.
    s = SP["S"]
    body = body_matrix(lift=-0.008 * s, nose_down=4.0)
    sweep = 12.0 * H.wave(t, 2) + 4.0 * H.wave(t, 5, 0.2)
    neck = tuple(w * GRAZE_BEND * 0.9 for w in GRAZE_SHAPE)
    neck = (neck[0], neck[1], neck[2], neck[3] + 3.0 * H.wave(t, 9))
    ears = ((-10 + 6 * H.flick(t, 0.5), 5), (-10, 5 + 20 * H.flick(t, 0.2)))
    return standing(arm, body, neck, yaw=sweep, t=t, ears=ears, offsets={"LF": -0.03 * s}, hair=dict(tail=(4.0, 0.2), tip=(10.0, 3.0)))


GRAZE_SHAPE = (-1.0, 0.0, 0.0, 0.25)
GRAZE_PITCH = 12.0
GRAZE_BEND = 45.0


def graze_lift():
    return -0.05 * SP["S"]


def eating(arm, t):
    # 6 s: down to a kill in the grass, tearing at it with jerks of the
    # head, chewing with the head turned (a cat shears with its side
    # teeth), then down again.
    down = H.ease((t - 0.04) / 0.10) * (1 - H.ease((t - 0.84) / 0.12))
    tear = max(0.0, H.wave(t, 8)) ** 3 * down * (1.0 - H.ease((t - 0.45) / 0.05) * (1 - H.ease((t - 0.62) / 0.05)))
    chewing = H.ease((t - 0.45) / 0.05) * (1 - H.ease((t - 0.62) / 0.05))
    body = body_matrix(lift=graze_lift() * down, nose_down=GRAZE_PITCH * down)
    bend = GRAZE_BEND * down * (1.0 - 0.30 * chewing)
    neck = tuple(w * bend for w in GRAZE_SHAPE)
    neck = (neck[0], neck[1], neck[2], neck[3] + 10.0 * tear)
    jaw = 18.0 * tear + chewing * 10.0 * max(0.0, H.wave(t, 14))
    ears = ((-5 + 15 * H.flick(t, 0.3), 10), (-5, 10 + 25 * H.flick(t, 0.7)))
    return standing(arm, body, neck, yaw=10.0 * tear * H.wave(t, 4) + 14.0 * chewing, t=t, ears=ears, jaw=jaw,
                    offsets={"LF": -0.03 * SP["S"] * down}, hair=dict(tail=(4.0, 0.2), tip=(12.0, 3.0)))


def walk(arm, t):
    # A lateral four-beat walk, the head low and level with the back, the
    # shoulder blades rolling, the tail hanging in its curve and swaying.
    s = SP["S"]
    low = s * SP["LEG"] ** 3  # a short-legged cat has less to give
    body = body_matrix(lift=-0.024 * low + 0.004 * s * H.wave(t, 2, 0.1), nose_down=0.8 * H.wave(t, 2, 0.35), roll=1.8 * H.wave(t, 1, 0.2))
    nod = 2.0 * H.wave(t, 2, 0.05)
    neck = (-9.0 + nod, -3.0 + 0.5 * nod, 0.0, 6.0 - 0.6 * nod)
    hair = dict(tail=(10.0, 0.1), lag=0.07, cycles=1.0, tail_lift=6.0, tip=(8.0, 2.0))
    ears = ((-3 + 15 * H.flick(t, 0.4, 0.08), 8), (-3, 8 + 15 * H.flick(t, 0.85, 0.08)))
    return H.locomotion(arm, t, WALK, body, neck, slide=0.11 * s, hair=hair, ears=ears)


def gallop(arm, t):
    # A rotary gallop: the hinds land close together under the belly with
    # the back curled, drive it straight, the fores reach far out and land,
    # the body sails stretched out, then gathers. The head stretched
    # forward and steady, the ears laid back, the tail streaming out behind
    # and up, balancing.
    s = SP["S"]
    pitch = H.wave(t, 1, 0.0)
    low = s * SP["LEG"] ** 3
    body = body_matrix(lift=-0.040 * low + 0.030 * s * H.wave(t, 2, 0.05), nose_down=5.0 * pitch)
    neck = (-12.0 + 3.0 * H.wave(t, 1, 0.2), -4.0, 0.0, 10.0 - 2.0 * H.wave(t, 1, 0.2))
    hair = dict(tail=(5.0, 0.0), tail_lift=40.0, lag=0.08, tail_curve=-2.0, tail_whip=10.0)
    flex = 16.0 * H.wave(t, 1, 0.25)
    return H.locomotion(arm, t, GALLOP, body, neck, slide=0.13 * s, flex=flex, hair=hair, ears=((30, 8), (30, 8)))


POUNCE_SECONDS = 64 / 48


def gallop_jump(arm, t):
    # The pounce: a crouch, the hindquarters treading, a spring forward and
    # up, the forepaws reaching out and down together onto the prey, the
    # body landing low over it, then up again into the stance.
    s = SP["S"]
    crouch = H.ease(t / 0.2) * (1 - H.ease((t - 0.32) / 0.05))
    tread = crouch * max(0.0, H.wave(t, 10)) * (1 - H.ease((t - 0.25) / 0.05))
    air = H.ease((t - 0.32) / 0.06) * (1 - H.ease((t - 0.58) / 0.08))
    arc = math.sin(math.pi * max(0.0, min(1.0, (t - 0.32) / 0.30)))
    land = H.ease((t - 0.55) / 0.08) * (1 - H.ease((t - 0.85) / 0.12))
    body = body_matrix(lift=-0.10 * s * crouch + 0.20 * s * arc - 0.06 * s * land, nose_down=4.0 * crouch - 10.0 * air * (1 - arc) + 8.0 * land,
                       roll=2.0 * tread)
    pose = H.Pose(arm)
    pose.carry("Body", body)
    pose.turn("Torso2", H.rot(body.to_3x3() @ H.X, -10.0 * air + 6.0 * crouch))
    neck = (-4.0 + 4.0 * crouch - 6.0 * land, -2.0, 0.0, 8.0 * crouch - 6.0 * land)
    H.pose_neck(pose, body, neck)
    pose_face(pose, ((-12, 0), (-12, 0)), jaw=16.0 * land)
    pose_hair(pose, tail=(4.0, 0.0), tail_lift=30.0 * air - 5.0 * crouch, lag=0.06, t=t, tail_curve=-2.0 * air, tip=(20.0 * crouch, 6.0))
    for leg in H.LEGS:
        rest = H.hoof_place(leg, 0.0, 0.0)
        if leg[1] == "F":
            reach = body @ (H.fetlock_rest(leg) + Vector((0.0, -0.14 * s, 0.02 * s)))
            f = rest.lerp(reach, air)
            f.z = max(f.z, rest.z)
            toe = -40.0 * air
        else:
            tuck = body @ (H.fetlock_rest(leg) + Vector((0.0, 0.10 * s, 0.06 * s)))
            f = rest.lerp(tuck, air)
            f.y += 0.006 * s * tread
            f.z = max(f.z, rest.z)
            toe = -50.0 * air
        H.pose_leg(pose, body, leg, f, toe, hip=0.6, sling=0.0 if air > 0.5 else 1.0 - air)
    return pose


def clips(arm):
    global GRAZE_BEND
    GRAZE_BEND = H.graze_bend(arm, height=0.010 * SP["S"])
    H.GRAZE_BEND = GRAZE_BEND
    print(f"felid: {NAME}: nose-down bend {GRAZE_BEND:.1f} degrees")
    made = [
        rig.bake_clip(arm, "Idle", 160 / 48, idle, H.MOVED),
        rig.bake_clip(arm, "Idle_2", 160 / 48, idle_2, H.MOVED),
        rig.bake_clip(arm, "Idle_Headlow", 160 / 48, idle_headlow, H.MOVED),
        rig.bake_clip(arm, "Eating", 288 / 48, eating, H.MOVED),
    ]
    H.SHORT.clear()
    made.append(rig.bake_clip(arm, "Walk", WALK["seconds"], walk, H.MOVED))
    print(f"felid: {NAME}: in the walk the standing paws fall short of their marks by at most "
          f"{ {k: round(v, 4) for k, v in sorted(H.SHORT.items())} } m")
    H.SHORT.clear()
    made.append(rig.bake_clip(arm, "Gallop", GALLOP["seconds"], gallop, H.MOVED))
    print(f"felid: {NAME}: in the gallop the standing paws fall short of their marks by at most "
          f"{ {k: round(v, 4) for k, v in sorted(H.SHORT.items())} } m")
    made.append(rig.bake_clip(arm, "Gallop_Jump", POUNCE_SECONDS, gallop_jump, H.MOVED))
    for a, seconds in ((made[4], WALK["seconds"]), (made[5], GALLOP["seconds"])):
        speed, lows = H.measure(arm, a, seconds)
        print(f"felid: {NAME}: {a.name} {seconds:.3f} s: paws slide back at {speed:.2f} m/s; lowest IK bones {[round(z, 4) for z in lows]}")
    print(f"felid: {NAME}: planned walk {ground_speed(WALK):.2f} m/s, gallop {ground_speed(GALLOP):.2f} m/s")
    return made


# ---------------------------------------------------------------------------
# Wiring ``horse.py``'s steps to the species.


def wire():
    """Points ``horse.py``'s steps at the configured species."""
    k = SP["HEAD"]
    H.use(
        QUATERNIUS=[], ENDS=[(n, b, warp(p)) for n, b, p in ENDS], EXTRA=EXTRA, MUSCLES=[], MUSCLE_NAMES=[], muscle_shape=lambda pose, arm: {},
        HEAD_O=Vector(HEAD_O), HEAD_A=Vector(HEAD_A), HEAD_U=Vector(HEAD_U), EYE=EYE, EYE_RADIUS=EYE_RADIUS * k * SP["S"],
        bone_table=bone_table, sculpt=sculpt, paint_sculpt=paint_sculpt, material_of=material_of, MATERIALS=MATERIALS,
        FALLBACK=fallback(), dense_zone=lambda p: dense_zone(unwarp(p)), BODY_TRIS=SP["BODY_TRIS"], eyes=eyes, skin=skin,
        NOT_SKIN=NOT_SKIN, TAIL_CHAIN=TAIL_CHAIN, FOLDS=FOLDS,
        mane_spots=warped(fur_spots), forelock_spots=lambda sdf: [], lash_spots=warped(whisker_spots),
        ear_hair_spots=lambda sdf: [], tail_spots=warped(tail_spots),
        COATS={NAME: {}}, color_at=lambda coat, p, n: (255, 255, 255), coat_part=lambda coat, part: (255, 255, 255),
        NECK=NECK, MUZZLE=Vector(warp(_at(0.168, -0.100))), PIVOT=Vector(warp(PIVOT)),
        HOOF={leg: {kk: SP["S"] * _wy(y) for kk, y in h.items()} for leg, h in HOOF.items()},
        END_REST={name: Vector(warp(p)) for name, _, p in ENDS}, GRAZE_SHAPE=GRAZE_SHAPE, GRAZE_LIFT=graze_lift(),
        GRAZE_PITCH=GRAZE_PITCH, GRAZE_BEND=GRAZE_BEND, pose_hair=pose_hair, pose_face=pose_face, body_matrix=body_matrix,
        weight_hair=weight_hair,
    )


# ---------------------------------------------------------------------------
# Projecting the face photograph (``FACE_PHOTOS``) onto the colour map.


def _srgb_to_linear(c):
    import numpy as np

    return np.where(c <= 0.04045, c / 12.92, ((c + 0.055) / 1.055) ** 2.4)


def _linear_to_srgb(c):
    import numpy as np

    c = np.clip(c, 0.0, 1.0)
    return np.where(c <= 0.0031308, c * 12.92, 1.055 * c ** (1 / 2.4) - 0.055)


def _texels(body, size):
    """Every texel of ``Main`` under the head (row, column, model point,
    normal), and the mask of every texel any ``Main`` triangle covers."""
    import numpy as np

    mesh = body.data
    uv = mesh.uv_layers.active.data
    co = [v.co.copy() for v in mesh.vertices]
    nor = [v.normal.copy() for v in mesh.vertices]
    covered = np.zeros((size, size), bool)
    rows, cols, pts, nrs = [], [], [], []
    for poly in mesh.polygons:
        if poly.material_index != 0:
            continue
        loops = list(poly.loop_indices)
        head = unwarp(tuple(poly.center))
        is_head = on_head(head)
        for k in range(1, len(loops) - 1):
            tri = (loops[0], loops[k], loops[k + 1])
            t2 = np.array([uv[i].uv[:] for i in tri]) * size - 0.5
            lo = np.floor(t2.min(axis=0)).astype(int)
            hi = np.ceil(t2.max(axis=0)).astype(int)
            xs, ys = np.meshgrid(np.arange(max(lo[0], 0), min(hi[0], size - 1) + 1), np.arange(max(lo[1], 0), min(hi[1], size - 1) + 1))
            if xs.size == 0:
                continue
            a, b, c = t2
            m = np.array([[b[0] - a[0], c[0] - a[0]], [b[1] - a[1], c[1] - a[1]]])
            if abs(np.linalg.det(m)) < 1e-12:
                continue
            inv = np.linalg.inv(m)
            rel = np.stack([xs.ravel() - a[0], ys.ravel() - a[1]])
            l1, l2 = inv @ rel
            l0 = 1.0 - l1 - l2
            inside = (l0 >= -1e-4) & (l1 >= -1e-4) & (l2 >= -1e-4)
            if not inside.any():
                continue
            xi, yi = xs.ravel()[inside], ys.ravel()[inside]
            covered[yi, xi] = True
            if not is_head:
                continue
            vs = [mesh.loops[i].vertex_index for i in tri]
            P = np.array([co[v] for v in vs])
            N = np.array([nor[v] for v in vs])
            w = np.stack([l0[inside], l1[inside], l2[inside]], axis=1)
            rows.append(yi)
            cols.append(xi)
            pts.append(w @ P)
            nrs.append(w @ N)
    return (np.concatenate(rows), np.concatenate(cols), np.concatenate(pts), np.concatenate(nrs)), covered


def project_face(body, image, folder):
    """Projects the species' face photograph (``FACE_PHOTOS``, from
    ``folder``) onto ``image``, the body's colour map, in place."""
    import numpy as np

    photo = FACE_PHOTOS[NAME]
    img = bpy.data.images.load(os.path.join(folder, photo["file"]))
    w, h = img.size
    px = np.empty(w * h * 4, np.float32)
    img.pixels.foreach_get(px)
    pic = _srgb_to_linear(px.reshape(h, w, 4)[::-1, :, :3].astype(np.float64))  # rows top down, as the landmarks
    # Its shading divided out: the luminance blurred over about a tenth of
    # the face, so the markings stay and the light goes.
    lum = pic @ np.array([0.2126, 0.7152, 0.0722])
    (cx, cy), (rx, ry) = photo["face"]
    shade = box_blur(lum, 0.10 * rx)
    inside = np.zeros_like(lum, bool)
    yy, xx = np.mgrid[0:h, 0:w]
    inside = ((xx - cx) / rx) ** 2 + ((yy - cy) / ry) ** 2 < 0.8
    # (Clamped: the deepest shadows are not lifted into blotches.)
    albedo = pic * np.clip((np.median(shade[inside]) / np.maximum(shade, 1e-4)) ** 0.8, 0.5, 2.0)[..., None]
    if photo.get("flatten") is not None:
        # The middle scales of its luminance (between a few hairs and the
        # face's breadth: the smudges of its light) brought toward their
        # median; the fine markings (the tear line, the whisker pads'
        # edges) kept.
        lum2 = albedo @ np.array([0.2126, 0.7152, 0.0722])
        lo = box_blur(lum2, 0.05 * rx)
        mid = np.median(lo[inside])
        flat = mid * (np.maximum(lo, 1e-4) / mid) ** photo["flatten"]
        albedo = albedo * (flat / np.maximum(lo, 1e-4))[..., None]
    if photo.get("eyes"):
        # A photo's eyes larger than the model's (its iris ``r`` pixels
        # across, the model's eye ``to``): round each, the photo out to
        # twice the iris is drawn in, so its lids and the pale ring round
        # them close on the modelled eye and no iris shows round it.
        r, to = photo["eyes"]
        out = 2.0 * r
        src = albedo.copy()
        for side in ("eye_L", "eye_R"):
            ex, ey = photo["points"][side]
            d = np.hypot(xx - ex, yy - ey)
            near = d < out
            s = np.where(d[near] > to, r + (d[near] - to) * (out - r) / (out - to), r * d[near] / max(to, 1e-6))
            k = s / np.maximum(d[near], 1e-6)
            sx = np.clip(np.round(ex + (xx[near] - ex) * k), 0, w - 1).astype(int)
            sy = np.clip(np.round(ey + (yy[near] - ey) * k), 0, h - 1).astype(int)
            albedo[near] = src[sy, sx]
    # The camera from the landmarks.
    marks = face_landmarks()
    names = sorted(photo["points"])
    p3 = [warp(marks[n]) for n in names]
    p2 = [photo["points"][n] for n in names]
    A, mapping = fit_mapping(p3, p2, 0.035 * SP["HEAD"] * SP["S"], [0.6 if n.startswith("ear") else 1.0 for n in names])
    for n, a, b in zip(names, p3, p2):
        affine = A @ np.array(list(a) + [1.0])
        print(f"felid: {NAME}: landmark {n}: the affine fit misses it by {np.hypot(*(affine - b)):.1f} px, the mapping by {np.hypot(*(mapping(a)[0] - b)):.2f} px")
    view = view_direction(A)
    size = image.size[0]
    (rows, cols, pts, nrs), covered = _texels(body, size)
    base = np.empty(size * size * 4, np.float32)
    image.pixels.foreach_get(base)
    base = base.reshape(size, size, 4)
    lin = _srgb_to_linear(base[..., :3].astype(np.float64))
    uvs = mapping(pts)
    weight = np.zeros(len(pts))
    for i in range(len(pts)):
        q = unwarp(tuple(pts[i]))
        n = nrs[i] / (np.linalg.norm(nrs[i]) or 1.0)
        on = 4 <= uvs[i, 0] < w - 4 and 4 <= uvs[i, 1] < h - 4
        weight[i] = face_weight(q, n, view) * ellipse_weight(uvs[i], (cx, cy), (rx, ry)) if on else 0.0
    # The photo filtered to a texel's footprint on it, so its grain does
    # not alias into blotches on a small head.
    strong_n = max(1, int((weight > 0.5).sum()))
    footprint = 2.0 * rx / (2.0 * math.sqrt(strong_n / math.pi))
    if footprint > 2.0:
        albedo = np.stack([box_blur(albedo[..., c], 0.5 * footprint, passes=1) for c in range(3)], axis=-1)

    def sample_at(uv):
        xi = np.clip(uv[:, 0], 0, w - 1).astype(int)
        yi = np.clip(uv[:, 1], 0, h - 1).astype(int)
        return albedo[yi, xi]

    sample = sample_at(uvs)
    if photo.get("mirror"):
        # The animal's right half from its left one, mirrored, blended over
        # the middle of the face.
        mirrored = pts.copy()
        mirrored[:, 0] = np.abs(mirrored[:, 0])
        k = np.clip(-pts[:, 0] / (0.012 * SP["S"]), 0.0, 1.0)[:, None]
        k = k * k * (3 - 2 * k)
        sample = sample * (1.0 - k) + sample_at(mapping(mirrored)) * k
    coat = lin[rows, cols]
    # The photo's colour matched to the coat's under it (where it counts).
    strong = weight > 0.5
    gain = coat[strong].mean(axis=0) / np.maximum(sample[strong].mean(axis=0), 1e-4)
    sample = sample * gain
    if photo.get("saturation"):
        # Its colour as saturated as the coat's under it (the mean distance
        # of each texel's colour from its own grey).
        def chroma(c):
            return np.abs(c - c.mean(axis=1, keepdims=True)).mean()

        grey = sample.mean(axis=1, keepdims=True)
        sample = np.clip(grey + (sample - grey) * chroma(coat[strong]) / max(chroma(sample[strong]), 1e-6), 0.0, None)
    # An overexposed photo's markings brought back to their contrast.
    mean = sample[strong].mean(axis=0)
    sample = np.clip(mean + (sample - mean) * photo.get("contrast", 1.0), 0.0, 1.0)
    out = lin.copy()
    out[rows, cols] = coat * (1.0 - weight[:, None]) + sample * weight[:, None]
    # The gutters round the face's islands take their neighbours' colour,
    # so the filtered map shows no seam.
    done = np.zeros((size, size), bool)
    done[rows[weight > 0], cols[weight > 0]] = True
    for _ in range(6):
        grow = np.zeros_like(done)
        acc = np.zeros_like(out)
        cnt = np.zeros((size, size))
        for dy, dx in ((1, 0), (-1, 0), (0, 1), (0, -1)):
            src = np.roll(np.roll(done, dy, 0), dx, 1)
            val = np.roll(np.roll(out, dy, 0), dx, 1)
            take = src & ~done & ~covered
            acc[take] += val[take]
            cnt[take] += 1
            grow |= take
        out[grow] = acc[grow] / cnt[grow][:, None]
        done |= grow
    base[..., :3] = _linear_to_srgb(out)
    image.pixels.foreach_set(base.astype(np.float32).ravel())
    image.update()
    print(f"felid: {NAME}: face photo on {int((weight > 0.5).sum())} texels ({int((weight > 0).sum())} blended), colour gain {np.round(gain, 2)}")
    bpy.data.images.remove(img)


# ---------------------------------------------------------------------------
# Previews and the command line.


def parse(argv):
    import argparse

    parser = argparse.ArgumentParser(prog="felid.py")
    parser.add_argument("output")
    parser.add_argument("--species", choices=sorted(SPECIES), default="puma")
    parser.add_argument("--preview", default=None, metavar="DIR")
    parser.add_argument("--textures", default=None, metavar="DIR")
    parser.add_argument("--draft", action="store_true")
    parser.add_argument("--no-clips", action="store_true")
    parser.add_argument("--shots", default=None, metavar="REGEX")
    parser.add_argument("--faces", default=None, metavar="DIR", help="project FACE_PHOTOS from DIR onto the face")
    return parser.parse_args(argv)


def _cam(eye, target, lens):
    """A camera laid out in the layout, made to the species' size."""
    return warp(eye), warp(target), lens


SIDE = ((3.4, 0.0, 0.55), (0.0, 0.05, 0.40), 50)


def shots_for(actions):
    c = NAME
    head = _at(0.12, -0.012)
    hy, hz = head[1], head[2]
    shots = [
        ("side.png", c, None, 0, *_cam(*SIDE)),
        ("side_right.png", c, None, 0, *_cam((-2.9, -0.10, 0.55), (0.0, -0.04, 0.42), 50)),
        ("front34.png", c, None, 0, *_cam((1.75, -2.10, 0.80), (0.0, -0.20, 0.45), 45)),
        ("front.png", c, None, 0, *_cam((0.03, -2.70, 0.62), (0.0, -0.30, 0.48), 50)),
        ("rear34.png", c, None, 0, *_cam((1.60, 1.95, 0.85), (0.0, 0.20, 0.42), 45)),
        ("close_head.png", c, None, 0, *_cam((0.55, hy - 0.85, hz + 0.08), (0.0, hy - 0.04, hz - 0.02), 60)),
        ("close_head_side.png", c, None, 0, *_cam((0.95, hy - 0.08, hz + 0.02), (0.0, hy - 0.05, hz - 0.02), 60)),
        ("close_head_front.png", c, None, 0, *_cam((0.04, hy - 1.05, hz + 0.02), (0.0, hy - 0.05, hz - 0.02), 70)),
        # Framed as the head photographs, to set beside them.
        ("head_match_front.png", c, None, 0, *_cam((0.10, hy - 0.80, hz + 0.12), (0.0, hy - 0.06, hz - 0.03), 85)),
        ("head_match_34l.png", c, None, 0, *_cam((0.50, hy - 0.62, hz + 0.06), (0.0, hy - 0.06, hz - 0.03), 85)),
        ("head_match_34r.png", c, None, 0, *_cam((-0.50, hy - 0.62, hz + 0.06), (0.0, hy - 0.06, hz - 0.03), 85)),
        # Where the photo meets the coat, behind the jaw and over the crown;
        # and the head at a middle distance.
        ("head_seam.png", c, None, 0, *_cam((0.62, hy + 0.05, hz + 0.18), (0.04, hy + 0.06, hz - 0.02), 70)),
        ("mid_distance.png", c, None, 0, *_cam((1.6, -3.0, 0.9), (0.0, -0.35, 0.45), 60)),
        ("close_coat.png", c, None, 0, *_cam((0.85, -0.05, 0.70), (0.08, 0.0, 0.52), 60)),
        ("close_tail.png", c, None, 0, *_cam((1.20, 1.10, 0.55), (0.0, 0.62, 0.32), 50)),
        ("close_paws.png", c, None, 0, *_cam((0.55, -1.10, 0.18), (0.05, -0.34, 0.06), 60)),
        ("close_paw_side.png", c, None, 0, *_cam((0.75, -0.36, 0.08), (0.07, -0.35, 0.04), 70)),
        ("close_hind_paw_side.png", c, None, 0, *_cam((0.75, 0.31, 0.08), (0.08, 0.31, 0.04), 70)),
    ]
    by_name = {a.name: a for a in actions}
    frames = {
        "Walk": tuple(range(0, round(WALK["seconds"] * 48) + 1, max(1, round(WALK["seconds"] * 48 / 7)))),
        "Gallop": tuple(range(0, round(GALLOP["seconds"] * 48) + 1, max(1, round(GALLOP["seconds"] * 48 / 8)))),
        "Gallop_Jump": (0, 10, 16, 20, 26, 30, 36, 44),
        "Eating": (0, 60, 140, 240),
        "Idle": (0, 50, 100),
        "Idle_2": (40, 110, 130),
        "Idle_Headlow": (40, 100),
    }
    for name, fs in frames.items():
        if name in by_name:
            for f in fs:
                shots.append((f"{name.lower()}_{f:03}.png", c, by_name[name], f, *_cam(*SIDE)))
    return shots


LIGHT_TRIS = 1000  # the light mesh's budget (the full one: about 11,500)


def light_mesh(body, arm):
    """The light mesh the game draws far off (``Puma_LOD`` and the others),
    as the cattle's (``tools/cattle.py``): the skin alone (no hair cards, no
    eyes) decimated to ``LIGHT_TRIS`` triangles, with its UVs, so the same
    maps dress it, and its weights from the full one."""
    import bmesh as bm_

    skin_only = body.copy()
    skin_only.data = body.data.copy()
    skin_only.modifiers.clear()
    bpy.context.collection.objects.link(skin_only)
    keep = {MATERIALS.index("Main")}
    bm = bm_.new()
    bm.from_mesh(skin_only.data)
    bm_.ops.delete(bm, geom=[f for f in bm.faces if f.material_index not in keep], context="FACES")
    bm.to_mesh(skin_only.data)
    bm.free()
    light = rig.decimated_copy(skin_only, f"{SP['mesh']}_LOD", LIGHT_TRIS)
    bpy.data.objects.remove(skin_only)
    bpy.ops.object.select_all(action="DESELECT")
    light.select_set(True)
    bpy.context.view_layer.objects.active = light
    bpy.ops.object.vertex_group_limit_total(group_select_mode="ALL", limit=4)
    bpy.ops.object.vertex_group_normalize_all(lock_active=False)
    light.parent = arm
    light.modifiers.new("Armature", "ARMATURE").object = arm
    print(f"felid: {NAME}: light mesh {rig.triangles(light)} triangles")
    return light


def main():
    argv = sys.argv[sys.argv.index("--") + 1 :] if "--" in sys.argv else []
    args = parse(argv)
    configure(args.species)
    wire()
    mesh, arm, images = H.build(args.draft, None)
    mesh.name = mesh.data.name = SP["mesh"]
    use_fur_atlas(images)
    if args.faces:
        project_face(mesh, images["colour"], args.faces)
    if args.textures:
        body_set, hair_set = f"{NAME}_body", f"{NAME}_hair"
        for folder, maps in (
            (body_set, (("color", images["colour"]), ("normal_gl", images["normal"]), ("roughness", images["rough"]),
                        ("ao", bpy.data.images["horse_body_ao"]))),
            (hair_set, (("color", images["hair"]["color"]), ("normal_gl", images["hair"]["normal"]),
                        ("roughness", images["hair"]["rough"]), ("opacity", images["hair"]["opacity"]))),
        ):
            for name, img in maps:
                rig.save_jpeg(img, os.path.join(args.textures, folder, f"{name}.jpg"))
    light = light_mesh(mesh, arm)
    light.hide_render = True
    actions = [] if args.no_clips else clips(arm)
    if args.preview:
        shots = shots_for(actions)
        if args.shots:
            shots = [s for s in shots if re.search(args.shots, s[0])]
        H.render_shots(args.preview, arm, mesh, images, [s for s in shots if s[2] is None])
        H.render_shots(args.preview, arm, mesh, images, [s for s in shots if s[2] is not None])
        # The light mesh alone, as the game draws a cat far off.
        light.hide_render, mesh.hide_render = False, True
        far = [("light_side.png", NAME, None, 0, *_cam(*SIDE)), ("light_front34.png", NAME, None, 0, *_cam((1.75, -2.10, 0.80), (0.0, -0.20, 0.45), 45))]
        H.render_shots(args.preview, arm, light, images, [s for s in far if not args.shots or re.search(args.shots, s[0])])
        mesh.hide_render = False
    light.hide_render = False
    print(f"felid: {NAME}: withers {withers_height():.3f} m, hips {hips_height():.3f} m")
    for obj in (mesh, light):
        for a in list(obj.data.color_attributes):
            obj.data.color_attributes.remove(a)
    keep = set(MATERIALS)
    for mat in list(bpy.data.materials):
        if mat.name not in keep:
            bpy.data.materials.remove(mat)
    rig.export(args.output, actions)


if bpy is not None and __name__ == "__main__":
    main()
