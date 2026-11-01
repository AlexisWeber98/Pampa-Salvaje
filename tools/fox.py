"""Model, rig, paint and animate the pampa's fox, the zorro gris pampeano
(Lycalopex gymnocercus), in Blender, by script.

Run it headless:

    blender -b --factory-startup --python tools/fox.py -- OUT.glb \
        [--textures DIR] [--preview DIR] [--shots REGEX] [--draft] [--no-clips]

Writes OUT.glb: one skinned mesh ``Fox`` with the materials ``Main`` (the
whole skin: the coat, the bare nose, lips and pads and the claws, all
drawn by the body's own colour map), ``Hair`` (the fur tufts and the
brush of the tail, alpha-tested cards whose atlas carries their own
colours) and the eyes ``Eye_Black`` and ``Eye_Iris``; its armature
``AnimalArmature`` keeps the Quaternius quadruped's joint names (``Body``,
``Back``, ``Torso``-``Torso3``, ``Neck1``-``Neck3``, ``Head``, the legs,
the tail, the IK bones and their ``_end`` leaves: the names of the
Quaternius fox the game draws today) with two bones per ear, the jaw and
nine for the tail; and the seven clips ``Idle``, ``Idle_2``,
``Idle_Headlow``, ``Eating``, ``Walk``, ``Gallop`` and ``Gallop_Jump`` (a
mouse pounce: the game plays it for no wild animal) as the cattle's (the
exporter sorts them by name: ``tools/glbtrim.py --keep`` puts them back in
this order). Life size: 0.38 m over the withers with the pads at the
origin, facing -Y (glTF +Z).

It is the horse (``tools/horse.py``) with a fox's anatomy, as the cattle
(``tools/cattle.py``) are: the same steps (the sculpt baked onto a
decimated mesh, the hair cards, the skinning, the analytic leg IK, the
gaits, the previews) run through ``animal_rig.Steps`` with the names below
replaced. A fox has one coat, so the coat is painted on the sculpt itself
(``coat_at``, with the agouti grizzle of its guard hairs) and baked into
the body's colour map: the game draws ``Main`` and ``Hair`` with their
maps as they are (no paint, no vertex colours).

The coat (the reference photographs: Commons, Lycalopex gymnocercus): the
back and the flanks grizzled grey (guard hairs grey at the root, a pale
band, a black tip) with a darker line down the spine; the head rufous
buff with a grey muzzle, the sides of the muzzle and the chin dark, the
cheeks, the throat and the chest white; large ears, rufous behind and
white inside; the belly and the inside of the legs cream; the outside of
the legs tawny rufous, a dark patch behind each hock; a bushy grey tail,
darker on top with a dark spot over its gland, the tip black.

Anatomy, in metres: 0.38 m over the withers, 0.65 m from the nose to the
root of the tail, the tail 0.37 m; slender legs on small digitigrade paws
(four toes with their pads and claws, the main pad behind), a deep narrow
chest and a tucked belly, a long slender muzzle, the ears 7 cm tall.
See tools/README.md.
"""

import math
import os
import re
import sys

try:
    import bpy
except ImportError:  # outside Blender: only the coat, the cards' colours and the plans (tests)
    bpy = None

HERE = os.path.dirname(os.path.abspath(__file__))

# ---------------------------------------------------------------------------
# The head's frame: from the top of the poll down the face to the nose.
# ``_at(t, u, s)`` is ``t`` metres along it, ``u`` up from it (toward the
# forehead) and ``s`` to the left. Plain tuples, so the coat works without
# Blender.

HEAD_O = (0.0, -0.300, 0.452)
_A = (0.0, -0.96, -0.28)
_n = math.sqrt(sum(c * c for c in _A))
HEAD_A = tuple(c / _n for c in _A)
HEAD_U = (0.0, HEAD_A[2], -HEAD_A[1])  # perpendicular, forward and up


def _at(t, u=0.0, s=0.0):
    return (s, HEAD_O[1] + HEAD_A[1] * t + HEAD_U[1] * u, HEAD_O[2] + HEAD_A[2] * t + HEAD_U[2] * u)


def head_coords(p):
    """(t, u, s) of ``p`` (Blender metres) in the head's frame."""
    r = (p[0] - HEAD_O[0], p[1] - HEAD_O[1], p[2] - HEAD_O[2])
    return (r[1] * HEAD_A[1] + r[2] * HEAD_A[2], r[1] * HEAD_U[1] + r[2] * HEAD_U[2], r[0])


def smooth(t):
    t = max(0.0, min(1.0, t))
    return t * t * (3 - 2 * t)


# The build: the anatomy below is laid out long, and the model is that
# layout made compact (``warp``): the trunk between the shoulders and the
# hips shortened (the hindquarters, the tail with them, come forward) and,
# above the shoulders, the neck shortened (the head comes back). The coat
# and the skin's regions read a model point back with ``unwarp``.
TRUNK_FROM, TRUNK_TO, TRUNK_SQUASH = -0.15, 0.14, 0.75
NECK_FROM, NECK_TO, NECK_BACK = -0.24, -0.18, 0.016


def _neck(z):
    """How much of the neck's shortening applies at height ``z``: none
    down the forelegs, all of it over the shoulders."""
    return smooth((z - 0.27) / 0.04)


def _warp_y(y, z):
    if y > TRUNK_TO:
        return y - (TRUNK_TO - TRUNK_FROM) * (1.0 - TRUNK_SQUASH)
    if y > TRUNK_FROM:
        return TRUNK_FROM + (y - TRUNK_FROM) * TRUNK_SQUASH
    k = _neck(z) * NECK_BACK
    if y > NECK_FROM:
        return y + k * (NECK_TO - y) / (NECK_TO - NECK_FROM) if y < NECK_TO else y
    return y + k


def warp(p):
    """A point of the layout (Blender metres) to the model."""
    return (p[0], _warp_y(p[1], p[2]), p[2])


def unwarp(p):
    """A point of the model back to the layout (the warp keeps ``x`` and
    ``z`` and is monotonic in ``y``)."""
    lo, hi = p[1] - 0.1, p[1] + 0.1
    for _ in range(40):
        mid = 0.5 * (lo + hi)
        if _warp_y(mid, p[2]) < p[1]:
            lo = mid
        else:
            hi = mid
    return (p[0], 0.5 * (lo + hi), p[2])


def _mix(a, b, f):
    return tuple(a[i] + (b[i] - a[i]) * f for i in range(3))


def _dist(a, b):
    return math.sqrt(sum((a[i] - b[i]) ** 2 for i in range(3)))


# ---------------------------------------------------------------------------
# The skeleton: the Quaternius quadruped's names and parents (the clips of
# ``horse.py`` drive them by name), set on a fox. Name, parent, head
# (Blender metres, facing -Y); a bone points at its first child (or at
# ``TAILS``, or its ``_end``). The fore leg: ``FrontUpperLeg`` is the
# forearm (elbow to wrist), ``FrontLowerLeg`` the metacarpus (wrist to the
# knuckle over the pad); the hind: ``BackLeg`` the femur, ``BackUpperLeg``
# the tibia, ``BackLowerLeg`` the metatarsus; the paws hang from the IK
# bones, as the horse's hooves.

FORE_X, HIND_X = 0.045, 0.05
def _tail_points(root=(0.0, 0.215, 0.345), seg=0.041, angles=(-14.0, -52.0)):
    """The tail's axis: nine segments carried back and down from the root,
    curving from ``angles[0]`` to ``angles[1]`` degrees below level."""
    pts = [root]
    for k in range(9):
        a = math.radians(angles[0] + (angles[1] - angles[0]) * k / 8)
        y, z = pts[-1][1], pts[-1][2]
        pts.append((0.0, y + seg * math.cos(a), z + seg * math.sin(a)))
    return pts


TAIL = _tail_points()

# The trunk's profile (layout metres): a tube through these points with an
# elliptical section (half width, half height) at each, and the point of the
# chest. A fox's chest is narrow and deep but flat in front: from the side
# the throat runs nearly straight down into the forelegs, the brisket barely
# ahead of the elbows and no lower than them, the belly tucked up behind the
# ribs (``chest_front``, ``chest_bottom``).
TRUNK = [((0, -0.200, 0.294), (0.046, 0.054)), ((0, -0.150, 0.282), (0.060, 0.080)), ((0, -0.090, 0.285), (0.065, 0.088)),
         ((0, 0.000, 0.290), (0.059, 0.080)), ((0, 0.090, 0.303), (0.052, 0.066)), ((0, 0.165, 0.310), (0.053, 0.060)),
         ((0, 0.205, 0.315), (0.045, 0.049))]
BRISKET = ((0.0, -0.198, 0.250), (0.032, 0.022, 0.032))
SHOULDER_POINT = (-0.208, 0.262)  # (y, z): the front of the point of the shoulder
ELBOW_Z = 0.212


def chest_front(z):
    """How far forward (the least ``y``) the trunk's profile and the point
    of the chest reach at height ``z``; None if neither reaches it."""
    best = None
    for (_, cy, cz), (_, b) in TRUNK + [((0.0, BRISKET[0][1], BRISKET[0][2]), (0.0, BRISKET[1][2]))]:
        ry = b if (cy, cz) != (BRISKET[0][1], BRISKET[0][2]) else BRISKET[1][1]
        d = (z - cz) / b
        if abs(d) < 1.0:
            y = cy - ry * math.sqrt(1.0 - d * d)
            best = y if best is None else min(best, y)
    return best


def chest_bottom(ahead_of=-0.10):
    """The lowest point of the trunk's profile ahead of ``ahead_of``."""
    return min(min(cz - b for (_, cy, cz), (_, b) in TRUNK if cy < ahead_of), BRISKET[0][2] - BRISKET[1][2])


BONES = [
    ("Body", None, (0.0, 0.0, 0.10)),
    ("Back", "Body", (0.0, 0.16, 0.330)),
    ("Torso", "Back", (0.0, 0.08, 0.322)),
    ("Torso2", "Torso", (0.0, -0.02, 0.315)),
    ("Torso3", "Torso2", (0.0, -0.10, 0.315)),
    ("Neck1", "Torso3", (0.0, -0.19, 0.335)),
    ("Neck2", "Neck1", (0.0, -0.230, 0.362)),
    ("Neck3", "Neck2", (0.0, -0.262, 0.390)),
    ("Head", "Neck3", (0.0, -0.291, 0.416)),
    ("FrontShoulder.L", "Torso2", (0.03, -0.165, 0.300)),
    ("FrontUpperLeg.L", "FrontShoulder.L", (FORE_X, -0.160, 0.212)),
    ("FrontLowerLeg.L", "FrontUpperLeg.L", (FORE_X, -0.176, 0.076)),
    ("FrontShoulder.R", "Torso2", (-0.03, -0.165, 0.300)),
    ("FrontUpperLeg.R", "FrontShoulder.R", (-FORE_X, -0.160, 0.212)),
    ("FrontLowerLeg.R", "FrontUpperLeg.R", (-FORE_X, -0.176, 0.076)),
    ("BackShoulder.L", "Back", (0.03, 0.150, 0.310)),
    ("BackLeg.L", "BackShoulder.L", (HIND_X, 0.150, 0.300)),
    ("BackUpperLeg.L", "BackLeg.L", (0.052, 0.098, 0.200)),
    ("BackLowerLeg.L", "BackUpperLeg.L", (HIND_X, 0.178, 0.086)),
    ("BackShoulder.R", "Back", (-0.03, 0.150, 0.310)),
    ("BackLeg.R", "BackShoulder.R", (-HIND_X, 0.150, 0.300)),
    ("BackUpperLeg.R", "BackLeg.R", (-0.052, 0.098, 0.200)),
    ("BackLowerLeg.R", "BackUpperLeg.R", (-HIND_X, 0.178, 0.086)),
] + [(f"Tail{k + 1}", "Back" if k == 0 else f"Tail{k}", TAIL[k]) for k in range(9)] + [
    ("PoleTargetBack.L", "Body", (HIND_X, -0.42, 0.10)),
    ("PoleTarget.L", "Body", (FORE_X, -0.38, 0.10)),
    ("PoleTargetBack.R", "Body", (-HIND_X, -0.42, 0.10)),
    ("PoleTarget.R", "Body", (-FORE_X, -0.38, 0.10)),
    ("IKBackLeg.L", None, (HIND_X, 0.166, 0.022)),
    ("FFB.L", "IKBackLeg.L", (HIND_X, 0.150, 0.010)),
    ("IKFrontLeg.L", None, (FORE_X, -0.190, 0.022)),
    ("FF.L", "IKFrontLeg.L", (FORE_X, -0.206, 0.010)),
    ("IKBackLeg.R", None, (-HIND_X, 0.166, 0.022)),
    ("FFB.R", "IKBackLeg.R", (-HIND_X, 0.150, 0.010)),
    ("IKFrontLeg.R", None, (-FORE_X, -0.190, 0.022)),
    ("FF.R", "IKFrontLeg.R", (-FORE_X, -0.206, 0.010)),
]

# Where the leaves point (and the ``_end`` nodes, as in the original).
ENDS = [
    ("FrontLowerLeg.L_end", "FrontLowerLeg.L", (FORE_X, -0.190, 0.022)),
    ("FrontLowerLeg.R_end", "FrontLowerLeg.R", (-FORE_X, -0.190, 0.022)),
    ("BackLowerLeg.L_end", "BackLowerLeg.L", (HIND_X, 0.166, 0.022)),
    ("BackLowerLeg.R_end", "BackLowerLeg.R", (-HIND_X, 0.166, 0.022)),
    ("Tail9_end", "Tail9", TAIL[9]),
    ("PoleTargetBack.L_end", "PoleTargetBack.L", (HIND_X, -0.42, 0.15)),
    ("PoleTarget.L_end", "PoleTarget.L", (FORE_X, -0.38, 0.15)),
    ("PoleTargetBack.R_end", "PoleTargetBack.R", (-HIND_X, -0.42, 0.15)),
    ("PoleTarget.R_end", "PoleTarget.R", (-FORE_X, -0.38, 0.15)),
    ("FFB.L_end", "FFB.L", (HIND_X, 0.132, 0.0)),
    ("FF.L_end", "FF.L", (FORE_X, -0.226, 0.0)),
    ("FFB.R_end", "FFB.R", (-HIND_X, 0.132, 0.0)),
    ("FF.R_end", "FF.R", (-FORE_X, -0.226, 0.0)),
]
NOSE_T = 0.142  # the tip of the nose, along the head
TAILS = {"Head": _at(NOSE_T - 0.004, -0.045), "Body": (0.0, 0.0, 0.2)}

# The bones the Quaternius rig lacks (or has otherwise): two per ear and the
# jaw. Name, parent, head, tail.
EAR_BASE, EAR_MID, EAR_TIP = (0.014, -0.006, 0.030), (0.002, 0.031, 0.044), (-0.012, 0.066, 0.058)  # head frame (t, u, |s|)


def _ear(point, sx):
    t, u, s = point
    return _at(t, u, sx * s)


EXTRA = []
for _side, _sx in (("L", 1.0), ("R", -1.0)):
    EXTRA += [(f"Ear1.{_side}", "Head", _ear(EAR_BASE, _sx), _ear(EAR_MID, _sx)),
              (f"Ear2.{_side}", f"Ear1.{_side}", _ear(EAR_MID, _sx), _ear(EAR_TIP, _sx))]
EXTRA.append(("Jaw", "Head", _at(0.035, -0.060), _at(NOSE_T - 0.012, -0.056)))

# The names the game's livestock path and ``horse.py``'s clips need.
QUATERNIUS_NAMES = ["Body", "Back", "Torso", "Torso2", "Torso3", "Neck1", "Neck2", "Neck3", "Head"] + [
    f"{b}.{s}" for s in ("L", "R") for b in ("FrontShoulder", "FrontUpperLeg", "FrontLowerLeg", "BackShoulder", "BackLeg",
                                              "BackUpperLeg", "BackLowerLeg", "PoleTargetBack", "PoleTarget", "IKBackLeg",
                                              "FFB", "IKFrontLeg", "FF", "Ear1")] + [f"Tail{k}" for k in range(1, 9)]


TAIL_CHAIN = [f"Tail{k}" for k in range(1, 10)]


def joint_names():
    return [b[0] for b in BONES] + [e[0] for e in EXTRA]


EYE = (0.066, -0.019, 0.0215)  # head frame (t, u, |s|) of the eyeball centre
EYE_RADIUS = 0.0078

# ---------------------------------------------------------------------------
# The coat, painted on the sculpt (``coat_at``) and baked into the colour
# map. sRGB 0..255. ``grizzle`` is how much of the agouti speckle shows
# (guard hairs with a pale band and a black tip): full on the back, none
# on the white and the rufous parts.

COAT = dict(
    grey=(126, 116, 100),  # the grizzled back and flanks, under the speckle
    dorsal=(88, 80, 70),  # the line down the spine and the tail's top
    buff=(176, 146, 104),  # the sides of the neck, the lower flanks, the tail's underside
    rufous=(178, 120, 72),  # the backs of the ears, the outside of the legs, the crown
    cream=(228, 219, 200),  # the belly, the inside of the legs, inside the ears
    white=(238, 234, 226),  # the cheeks, the throat, the chest
    dark=(46, 38, 33),  # the chin, the sides of the muzzle, the tail's tip, behind the hocks
    nose=(26, 23, 23),  # the bare nose, the lips, the lid rims, the pads
    claw=(58, 50, 44),
)


def tail_place(q):
    """How far along the tail ``q`` is (0 at its root, 1 at its tip) and
    how far from the tail's axis, or None off the tail."""
    if q[1] < 0.20 or abs(q[0]) > 0.09:
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
    if d > 0.085 or (f <= 0.0 and q[1] < 0.225):
        return None
    return f, d, ab


def is_ear(q):
    """On an ear: above the skull, out to the side, behind the eyes."""
    t, u, s = head_coords(q)
    return u > 0.004 and abs(s) > 0.012 and -0.045 < t < 0.05 and q[1] < -0.26


def coat_at(q, m):
    """The coat at the model point ``q`` facing ``m`` (Blender metres):
    its colour (sRGB 0..255) and how much the agouti grizzle shows."""
    x, y, z = q
    c = COAT
    tail = tail_place(q)
    if tail is not None:
        f, _, ab = tail
        # The tail's own up: across it, toward the sky.
        ln = math.sqrt(sum(v * v for v in ab))
        a = tuple(v / ln for v in ab)
        up = (0.0, -a[2], a[1]) if a[1] > 0 else (0.0, a[2], -a[1])
        top = sum(m[i] * up[i] for i in range(3))
        rgb = _mix(c["grey"], c["dorsal"], smooth((top - 0.1) / 0.6))
        rgb = _mix(rgb, c["buff"], smooth((-top - 0.2) / 0.5))
        # The gland's dark spot on top, a hand's width from the root.
        rgb = _mix(rgb, c["dark"], 0.8 * smooth((top - 0.3) / 0.4) * smooth((0.06 - abs(f - 0.17)) / 0.04))
        tip = smooth((f - 0.74) / 0.08)
        return _mix(rgb, c["dark"], tip), 0.9 * (1.0 - tip)
    rgb, g = _body_coat(q, m)
    head = smooth((-0.245 - y) / 0.03) * smooth((z - 0.335) / 0.03)
    if head > 0.0:
        h_rgb, h_g = _head_coat(q, m)
        rgb, g = _mix(rgb, h_rgb, head), g + (h_g - g) * head
    return rgb, g


def _body_coat(q, m):
    x, y, z = q
    c = COAT
    rgb, g = c["grey"], 0.9
    # The darker line down the spine.
    rgb = _mix(rgb, c["dorsal"], 0.7 * smooth((0.03 - abs(x)) / 0.02) * smooth((m[2] - 0.6) / 0.3) * smooth((z - 0.30) / 0.03))
    # The sides of the neck and the lower flanks warmer.
    rgb = _mix(rgb, c["buff"], 0.55 * smooth((-0.16 - y) / 0.06) * smooth((0.85 - m[2]) / 0.4))
    rgb = _mix(rgb, c["buff"], 0.45 * smooth((0.27 - z) / 0.03) * smooth((0.35 - abs(m[2])) / 0.4))
    # The legs: tawny rufous outside, cream inside; the hinds grey on the
    # thigh, rufous below the stifle; a dark patch behind each hock.
    sx = 1.0 if x >= 0 else -1.0
    inner = smooth((-m[0] * sx - 0.15) / 0.4)
    if y < -0.08 and z < 0.24:  # fore
        leg = smooth((0.235 - z) / 0.04)
        rgb = _mix(rgb, _mix(c["rufous"], c["cream"], max(inner, 0.6 * smooth((m[1] - 0.3) / 0.4))), leg)
        g *= 1.0 - leg
    if y > 0.04 and z < 0.30:  # hind
        leg = smooth((0.21 - z) / 0.05)
        rgb = _mix(rgb, c["rufous"], 0.6 * smooth((0.25 - z) / 0.06) * (1.0 - inner))
        rgb = _mix(rgb, _mix(c["rufous"], c["cream"], inner), leg)
        g *= 1.0 - 0.8 * leg
        spot = smooth((m[1] - 0.35) / 0.3) * smooth((0.025 - abs(z - 0.115)) / 0.02) * smooth((0.6 - inner) / 0.3)
        rgb = _mix(rgb, c["dark"], 0.85 * spot)
    if z < 0.05:  # the paws, paler
        rgb = _mix(rgb, c["buff"], 0.6 * smooth((0.05 - z) / 0.02))
    # The belly cream; the throat, the chest and the front of the neck
    # white (the bib), up to the jaw.
    under = smooth((-m[2] - 0.35) / 0.4) * smooth((0.28 - z) / 0.04)
    front = -0.75 * m[2] - 0.65 * m[1] + 0.22 * _wobble(q, 140.0) + 0.10 * _wobble(q, 420.0)
    chest = smooth((-0.17 - y) / 0.05) * smooth((front - 0.10) / 0.55) * smooth((0.30 + 0.9 * (-0.17 - y) - z + 0.012 * _wobble(q, 260.0)) / 0.05)
    # The white is the throat's and the upper chest's: lower down, between
    # the forelegs, the chest turns buff and a little grizzled.
    low = smooth((0.305 - z + 0.012 * _wobble(q, 330.0)) / 0.07) * smooth((-0.15 - y) / 0.04)
    rgb = _mix(rgb, c["cream"], max(under, 0.0))
    rgb = _mix(rgb, _mix(c["white"], _mix(c["cream"], c["buff"], 0.45), 0.8 * low), chest)
    g *= 1.0 - max(under, chest * (1.0 - 0.6 * low))
    return rgb, g


def _wobble(q, k):
    """A cheap smooth noise in about [-1, 1] at ``q`` (``k`` per metre):
    the ragged, grizzled edge of a white patch."""
    x, y, z = (c * k for c in q)
    return (math.sin(1.7 * x + 2.3 * y + 0.4) * math.sin(1.3 * y - 2.1 * z + 1.1) + 0.6 * math.sin(2.9 * z + 1.9 * x - 0.7)) / 1.6


def _head_coat(q, m):
    c = COAT
    t, u, s = head_coords(q)
    if is_ear(q):
        # Rufous behind, white inside (facing forward), darker at the tip.
        inside = smooth((-m[1] - 0.15) / 0.4)
        rgb = _mix(c["rufous"], (204, 190, 166), inside)
        return _mix(rgb, (110, 72, 46), 0.6 * smooth((u - 0.055) / 0.02) * (1.0 - inside)), 0.0
    rgb, g = (170, 128, 84), 0.35  # the crown and the forehead, rufous buff
    # The muzzle's top grey, its sides and the lips dark, the chin dark.
    muzzle = smooth((t - 0.085) / 0.02)
    rgb = _mix(rgb, (150, 136, 118), muzzle * smooth((u + 0.036) / 0.008))
    rgb = _mix(rgb, c["dark"], 0.85 * muzzle * smooth((-0.038 - u) / 0.006) * smooth((abs(s) - 0.006) / 0.006))
    chin = smooth((t - 0.06) / 0.02) * smooth((-0.055 - u) / 0.006)
    rgb = _mix(rgb, c["dark"], 0.9 * chin)
    # The cheeks and the throat white, under the eyes and behind the jaw.
    cheek = smooth((-0.032 - u) / 0.012) * smooth((0.085 - t) / 0.02) * (1.0 - chin)
    rgb = _mix(rgb, c["white"], cheek)
    # A pale brow over each eye, the dark line from its front corner.
    e = _dist(q, _at(EYE[0], EYE[1], math.copysign(EYE[2], s)))
    rgb = _mix(rgb, c["cream"], 0.5 * smooth((0.016 - e) / 0.006) * smooth((u - EYE[1]) / 0.006))
    line = smooth((0.004 - abs(u - (EYE[1] - 0.004 - 0.25 * (t - EYE[0])))) / 0.003) * smooth((t - EYE[0] - 0.006) / 0.004) * smooth((0.11 - t) / 0.02)
    rgb = _mix(rgb, c["dark"], 0.6 * line)
    # The neck behind the head: back to the grey of the body.
    rgb = _mix(rgb, c["grey"], smooth((-0.02 - t) / 0.03) * smooth((m[2] - 0.3) / 0.4))
    return rgb, g * (1.0 - cheek) * (1.0 - chin)


# The hair cards' atlas: eight columns (``horse.py``'s ``HAIR_COLUMNS``: six
# of full locks, two of wispy ones), each with its own colours from the
# root (``f`` 0) to the tip (1).
GRIZZLED, GRIZZLED_2, CREAM, RUFOUS, DORSAL, BLACK, WISP_WHITE, WISP_BLACK = range(8)


def card_colour(column, f, shift=0.0):
    """The colour (sRGB 0..255) of one hair of the cards' atlas ``column``
    at ``f`` along it (0 at the root): an agouti guard hair is grey at the
    root, then a pale band, then a black tip; ``shift`` moves its band
    (each hair has its own, so the bands never line up across a tuft)."""
    if column == GRIZZLED:
        band = 0.48 + shift
        rgb = _mix((118, 108, 94), (190, 172, 140), smooth((f - band) / 0.05))
        return _mix(rgb, (52, 44, 38), 0.85 * smooth((f - band - 0.24) / 0.06))
    if column == GRIZZLED_2:  # more underfur showing: grey to buff, a faint dark tip
        rgb = _mix((120, 110, 96), (168, 150, 120), smooth((f - 0.45 - shift) / 0.2))
        return _mix(rgb, (78, 68, 58), 0.6 * smooth((f - 0.8 - shift) / 0.08))
    if column == DORSAL:
        band = 0.44 + shift
        rgb = _mix((90, 82, 72), (148, 132, 106), smooth((f - band) / 0.05))
        return _mix(rgb, (44, 38, 34), 0.85 * smooth((f - band - 0.18) / 0.06))
    if column == CREAM:
        return _mix((190, 180, 162), (222, 214, 198), smooth(f / 0.4))
    if column == RUFOUS:
        return _mix((168, 136, 98), (196, 166, 124), smooth(f / 0.5))
    if column == BLACK:
        return (32, 28, 26)
    if column == WISP_WHITE:
        return (212, 200, 180)
    return (24, 22, 20)


# ---------------------------------------------------------------------------
# The gaits (``horse.py``'s plans): seconds, duty factor, stance sweep (m),
# the phase each leg lands, the swing lift (fore, hind) and the paw's fold.
# A fox walks about 0.7 m/s in a lateral four-beat walk; it gallops (a
# rotary gallop, hinds then fores, with two flights) at about 5 m/s, three
# strides a second; the game plays the clips faster or slower to its
# speed.

WALK = dict(seconds=28 / 48, duty=0.62, sweep=0.22, phase=dict(LH=0.0, LF=0.25, RH=0.5, RF=0.75),
            lift=(0.045, 0.040), fold=(75.0, 60.0), centre=dict(F=0.025, H=0.0), strike=6.0, breakover=30.0, sling=0.6)
GALLOP = dict(seconds=15 / 48, duty=(0.22, 0.24), sweep=(0.30, 0.33), phase=dict(RH=0.0, LH=0.08, LF=0.42, RF=0.52),
              lift=(0.10, 0.075), fold=(95.0, 80.0), centre=dict(F=-0.03, H=-0.02), strike=8.0, breakover=40.0,
              reach=0.03, hip=0.6, hip_swing=0.75, hind_lead=0.8, hind_peak=0.6, fore_peak=1.2, sling=1.0)


def ground_speed(gait):
    """The speed the standing paws slide back at, m/s (fore legs)."""
    duty = gait["duty"][0] if isinstance(gait["duty"], tuple) else gait["duty"]
    sweep = gait["sweep"][0] if isinstance(gait["sweep"], tuple) else gait["sweep"]
    return sweep / (duty * gait["seconds"])


# ---------------------------------------------------------------------------
# Everything below runs in Blender.

if bpy is not None:
    from mathutils import Matrix, Quaternion, Vector

    sys.path.insert(0, HERE)
    import animal_rig as rig  # noqa: E402

    H = rig.Steps(os.path.join(HERE, "horse.py"), prefix="fox:")


def at(t, u=0.0, s=0.0):
    return Vector(_at(t, u, s))


def head_matrix():
    return Matrix((Vector((1, 0, 0)), Vector(HEAD_A), Vector(HEAD_U))).transposed()


def bone_table():
    """Every bone: (name, parent, head, rotation, length): pointing at its
    first child (or its ``TAILS`` point, or its ``_end``), the extra bones
    from head to tail."""
    heads = {name: Vector(h) for name, _, h in BONES}
    ends = {bone: Vector(p) for _, bone, p in ENDS}
    first = {}
    for name, parent, _ in BONES:
        if parent and parent not in first:
            first[parent] = name
    out = []
    for name, parent, head in BONES:
        h = heads[name]
        if name in TAILS:
            tail = Vector(TAILS[name])
        elif name in first and (heads[first[name]] - h).length > 0.008:
            tail = heads[first[name]]
        elif name in ends:
            tail = ends[name]
        else:
            tail = h + Vector((0.0, 0.0, 0.02))
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
# The sculpt: the furred silhouette (the coat's volume is part of it; the
# cards only break its outline).

BOUNDS = ((-0.11, -0.53, -0.004), (0.11, 0.53, 0.56))


def rot_x(deg):
    return Matrix.Rotation(math.radians(deg), 3, "X")


def paw(sdf, x, heel_y, toe_y, fore):
    """A digitigrade paw: the main pad behind, four toes in an arc in
    front, each with its pad and a short dark claw, a flat sole; the fore
    paw a little larger, and its dewclaw high on the inside."""
    E, R = rig.sdf_ellipsoid, rig.sdf_round_cone
    back = 1.0 if toe_y < heel_y else -1.0  # +1: the toes are toward -Y
    k = 1.0 if fore else 0.82
    mid = heel_y - back * 0.016
    sdf.add(E((x, mid, 0.013), (0.0125 * k, (0.019 if fore else 0.0165) * k, 0.012 if fore else 0.0135)), 0.006)  # the paw's body
    sdf.add(E((x, heel_y - back * 0.003, 0.009), (0.010 * k, 0.009, 0.009)), 0.004)  # the main pad
    for i, (dx, dy) in enumerate(((-0.0135, 0.031), (-0.0048, 0.037), (0.0048, 0.037), (0.0135, 0.031))):
        c = (x + dx * k, heel_y - back * dy * k, 0.0075)
        sdf.add(E(c, (0.0058 * k, 0.0078 * k, 0.0072)), 0.004)
        if not fore:  # the knuckles, arched over each toe
            sdf.add(E((c[0], c[1] + back * 0.006, 0.0125), (0.0050 * k, 0.0060, 0.0050)), 0.004)
        tip = (c[0] + 0.15 * dx, c[1] - back * 0.0085 * k, 0.0035)
        sdf.add(R((c[0] + 0.1 * dx, c[1] - back * 0.004, 0.0085), tip, 0.0024, 0.0008), 0.0015)  # the claw
        CLAWS.append(tip)
    if fore:
        sdf.add(E((x, mid - back * 0.004, 0.016), (0.0125 * k, 0.018 * k, 0.008)), 0.006)  # the fur over the toes
    if fore:
        sx = 1.0 if x > 0 else -1.0
        dew = (x - sx * 0.0085, heel_y + back * 0.006, 0.040)
        sdf.add(E(dew, (0.0045, 0.0055, 0.006)), 0.003)
        sdf.add(R(dew, (dew[0] - sx * 0.002, dew[1] - back * 0.006, dew[2] - 0.004), 0.002, 0.0008), 0.0015)
        CLAWS.append((dew[0] - sx * 0.002, dew[1] - back * 0.006, dew[2] - 0.004))
    # The grooves between the toes, over the top and the front, so each toe
    # reads from the side as from above.
    for dx in (-0.0092, 0.0, 0.0092):
        sdf.carve(E((x + dx * k, heel_y - back * 0.036 * k, 0.012), (0.0011, 0.010 * k, 0.0075)), 0.0015)
    lo, hi = (x - 0.03, mid - 0.04, -0.05), (x + 0.03, mid + 0.04, 0.0)
    sdf.carve((lambda X, Y, Z: Z + 0.0 * X + 0.0 * Y, lo, hi), 0.002)


CLAWS = []


def legs(sdf, sx):
    """One side's legs, slender under their short fur: the shoulder and
    the upper arm, the elbow, the forearm, the wrist, the metacarpus and
    the paw; the thigh with the long fur of its back (the "pants"), the
    stifle, the lower thigh, the hock with its tendon, the metatarsus and
    the paw."""
    E, R, T = rig.sdf_ellipsoid, rig.sdf_round_cone, rig.sdf_tube
    x = FORE_X * sx
    sdf.add(E((sx * 0.040, -0.150, 0.300), (0.024, 0.040, 0.062), rot_x(-25)), 0.03)  # shoulder blade
    sdf.add(E((sx * 0.046, -0.188, 0.262), (0.020, 0.024, 0.032)), 0.02)  # point of the shoulder
    sdf.add(E((sx * 0.047, -0.172, 0.236), (0.022, 0.030, 0.046), rot_x(25)), 0.02)  # upper arm
    sdf.add(E((sx * 0.046, -0.147, 0.214), (0.016, 0.018, 0.018)), 0.016)  # elbow
    sdf.add(T([(x, -0.163, 0.205), (x, -0.168, 0.150), (x, -0.174, 0.100), (x, -0.177, 0.080)],
              [(0.0220, 0.0235), (0.0170, 0.0180), (0.0128, 0.0130), (0.0112, 0.0116)]), 0.012)  # forearm
    sdf.add(E((x, -0.176, 0.076), (0.0105, 0.0115, 0.013)), 0.006)  # wrist
    sdf.add(R((x, -0.178, 0.068), (x, -0.188, 0.028), 0.0095, 0.0108), 0.006)  # metacarpus
    paw(sdf, x, -0.183, -0.230, True)
    x = HIND_X * sx
    sdf.add(E((sx * 0.042, 0.146, 0.272), (0.034, 0.064, 0.076), rot_x(12)), 0.05)  # thigh
    sdf.add(E((sx * 0.036, 0.176, 0.240), (0.026, 0.032, 0.058), rot_x(-10)), 0.045)  # the pants
    sdf.add(E((sx * 0.050, 0.100, 0.212), (0.012, 0.016, 0.016)), 0.02)  # stifle
    sdf.add(T([(sx * 0.050, 0.112, 0.214), (sx * 0.052, 0.142, 0.150), (x, 0.172, 0.096)],
              [(0.0250, 0.0300), (0.0190, 0.0220), (0.0106, 0.0118)]), 0.018)  # lower thigh
    sdf.add(R((x, 0.150, 0.170), (x, 0.190, 0.090), 0.0065, 0.0060), 0.008)  # the hock's tendon
    sdf.add(E((x, 0.181, 0.085), (0.0095, 0.0125, 0.011)), 0.006)  # hock
    sdf.add(R((x, 0.178, 0.078), (x, 0.168, 0.028), 0.0085, 0.0090), 0.005)  # metatarsus
    paw(sdf, x, 0.172, 0.128, False)


def trunk(sdf):
    E, R, T = rig.sdf_ellipsoid, rig.sdf_round_cone, rig.sdf_tube
    sdf.add(T([p for p, _ in TRUNK], [r for _, r in TRUNK]))
    sdf.add(E((0, -0.120, 0.356), (0.032, 0.070, 0.026)), 0.04)  # withers
    sdf.add(E(*BRISKET), 0.025)  # the point of the chest
    sdf.add(E((0, 0.165, 0.334), (0.042, 0.060, 0.034)), 0.03)  # croup
    # The neck, deep under its ruff, rising to the head.
    sdf.add(T([(0, -0.165, 0.315), (0, -0.225, 0.350), (0, -0.262, 0.380), (0, -0.290, 0.410)],
              [(0.050, 0.066), (0.047, 0.058), (0.041, 0.050), (0.034, 0.041)]), 0.035)
    sdf.add(E((0, -0.245, 0.332), (0.034, 0.030, 0.040), rot_x(-30)), 0.03)  # the throat's ruff
    # The tail: bushy, its fur part of the volume.
    radii = [0.024, 0.038, 0.050, 0.056, 0.059, 0.058, 0.054, 0.047, 0.037, 0.021]
    sdf.add(T(TAIL, [(0.92 * r, r) for r in radii]), 0.02)


def head(sdf):
    E, T = rig.sdf_ellipsoid, rig.sdf_tube
    Hx = head_matrix()
    n = NOSE_T
    sdf.add(E(at(0.036, -0.026), (0.034, 0.045, 0.030), Hx), 0.02)  # cranium
    sdf.add(E(at(0.060, -0.019), (0.026, 0.026, 0.020), Hx), 0.012)  # brow
    for sx in (1, -1):
        sdf.add(E(at(0.058, -0.037, sx * 0.025), (0.012, 0.022, 0.012), Hx), 0.01)  # cheekbone
        sdf.add(E(at(0.040, -0.050, sx * 0.023), (0.024, 0.036, 0.026), Hx), 0.014)  # the furred cheeks
        sdf.add(E(at(0.010, -0.040, sx * 0.029), (0.017, 0.026, 0.027), Hx), 0.014)  # behind the jaw
    sdf.add(T([at(0.066, -0.030), at(0.104, -0.035), at(n - 0.004, -0.039)], [(0.021, 0.023), (0.0150, 0.0170), (0.0098, 0.0118)]), 0.012)  # muzzle
    sdf.add(E(at(n - 0.003, -0.0355), (0.0078, 0.0062, 0.0064), Hx), 0.004)  # nose
    sdf.add(T([at(0.030, -0.060), at(0.090, -0.057), at(n - 0.012, -0.052)], [(0.017, 0.015), (0.0125, 0.0110), (0.0080, 0.0070)]), 0.01)  # lower jaw
    mouth = E(at(n - 0.032, -0.0492), (0.0165, 0.026, 0.0009), Hx)
    sdf.carve(mouth, 0.0008)
    OPENINGS.append(mouth[0])
    for sx in (1, -1):
        nostril = E(at(n + 0.0055, -0.034, sx * 0.0042), (0.0022, 0.0028, 0.0026), Hx)
        sdf.carve(nostril, 0.001)
        OPENINGS.append(nostril[0])
        t, u, s = EYE
        sdf.add(E(at(t, u, sx * (s - 0.0005)), (0.0072, 0.0105, 0.0085), Hx), 0.003)  # lids
        sdf.carve(E(at(t + 0.001, u, sx * (s + 0.0045)), (0.0068, 0.0092, 0.0045), Hx), 0.0015)  # the opening
        ears(sdf, sx)


OPENINGS = []
CAVITIES = []


def ears(sdf, sx):
    """A large pointed ear, upright and a little out and back, thin, its
    front a deep cup."""
    base, mid, tip = (Vector(_ear(p, sx)) for p in (EAR_BASE, EAR_MID, EAR_TIP))
    front = (Vector((sx * 0.30, 0.0, 0.0)) - Vector(HEAD_A) * 0.95).normalized()  # the cup faces forward and out
    sdf.add(rig.sdf_tube([base, mid, tip], [(0.0085, 0.0400), (0.0060, 0.0260), (0.0010, 0.0030)], side=front), 0.008)
    f = front * 0.0042
    cavity = rig.sdf_tube([base + f + Vector((0, 0, 0.004)), mid + f, tip + f * 0.6],
                          [(0.0046, 0.0320), (0.0035, 0.0205), (0.0005, 0.0017)], side=front)
    sdf.carve(cavity, 0.002)
    CAVITIES.append(cavity[0])


def details(sdf):
    """What reads through the fur: the shoulder blade's spine, the last
    rib, the hollow of the flank, the seam between the thigh's muscles and
    the furrow down the middle of the forehead."""
    for sx in (1, -1):
        out = (sx, 0, 0)
        sdf.ridge([(0, -0.130, 0.335), (0, -0.150, 0.300), (0, -0.168, 0.268)], out, 0.007, 0.0012, 0.004)
        sdf.groove([(0, 0.040, 0.330), (0, 0.050, 0.290), (0, 0.055, 0.255)], out, 0.012, 0.0008, 0.003)
        sdf.groove([(sx * 0.03, 0.105, 0.320), (sx * 0.03, 0.110, 0.290), (sx * 0.03, 0.108, 0.262)], out, 0.014, 0.0018, 0.004)
        sdf.groove([(sx * 0.03, 0.170, 0.300), (sx * 0.03, 0.172, 0.262), (sx * 0.03, 0.168, 0.225)], (sx, 0.5, 0), 0.008, 0.0012, 0.003)
    sdf.groove([at(0.020, -0.01), at(0.055, -0.015), at(0.085, -0.025)], Vector(HEAD_U), 0.005, 0.0008, 0.002)


def sculpt(voxel):
    """The dense sculpt: one watertight surface."""
    OPENINGS.clear()
    CAVITIES.clear()
    CLAWS.clear()
    sdf = rig.Sdf(*BOUNDS, voxel * 0.27)
    trunk(sdf)
    for sx in (1, -1):
        legs(sdf, sx)
    head(sdf)
    details(sdf)
    obj = sdf.to_mesh("FoxSculpt")
    warp_mesh(obj)
    return obj, sdf


def warp_mesh(obj):
    """Makes a mesh laid out on the long frame compact (``warp``)."""
    for v in obj.data.vertices:
        v.co = warp(v.co)
    obj.data.update()


def warped(spots):
    """``spots`` (hair strands laid out against the long sculpt) with their
    points on the model."""
    def fn(sdf):
        return [(tuple(Vector(warp(p)) for p in s[0]),) + tuple(s[1:]) for s in spots(sdf)]
    return fn


# ---------------------------------------------------------------------------
# Regions of the skin, from the rest position.


def _fn_min(fns, p):
    import numpy as np

    a = np.array([[[p[0]]]]), np.array([[[p[1]]]]), np.array([[[p[2]]]])
    return float(min(fn(*a).ravel()[0] for fn in fns))


def opening(p):
    if p[2] < 0.36 or p[1] > -0.38 or not OPENINGS:
        return 1.0
    return _fn_min(OPENINGS, p)


def ear_cavity(p):
    if not is_ear(p) or not CAVITIES:
        return 1.0
    return _fn_min(CAVITIES, p)


def is_claw(p):
    return p[2] < 0.05 and any(_dist(p, c) < 0.0042 for c in CLAWS)


def nose(p):
    """The bare, moist nose leather."""
    t, u, s = head_coords(p)
    return smooth((t - NOSE_T + 0.008) / 0.004) * smooth((u + 0.046) / 0.003)


def pads(p):
    """The bare pads under the paws."""
    return smooth((0.0045 - p[2]) / 0.002)


def bare(p):
    """Bare skin: the nose, round the nostrils, the lips, the lid rims and
    the pads."""
    if p[2] < 0.02:
        return pads(p)
    if p[1] > -0.33 or p[2] < 0.36:
        return 0.0
    lips = smooth((0.0028 - opening(p)) / 0.002) if p[1] < -0.36 else 0.0
    t, u, s = head_coords(p)
    e = _dist(p, _at(EYE[0], EYE[1], math.copysign(EYE[2], s)))
    rim = smooth((0.0105 - e) / 0.0015) * smooth((e - 0.0072) / 0.001)
    return max(nose(p), lips, rim)


def material_of(p, n):
    return "Main"


MATERIALS = ["Main", "Hair", "Eye_Black", "Eye_Iris"]
FALLBACK = {
    "Main": (0.57, 0.54, 0.48),
    "Hair": (1.0, 1.0, 1.0),
    "Eye_Black": (0.02, 0.02, 0.02),
    "Eye_Iris": (0.70, 0.44, 0.10),
}


def hair_direction(p, n):
    """Which way the fur lies: back from the nose over the head, up the
    ears, back and down the neck, back along the body and down its sides,
    down the legs, back along the belly, along the tail to its tip."""
    p, n = Vector(p), Vector(n)
    tail = tail_place(p)
    if tail is not None:
        d = Vector(tail[2])
    elif p.y < -0.26 and p.z > 0.36:
        d = Vector((0.0, 0.25, 1.0)) if is_ear(p) else -Vector(HEAD_A) + Vector((math.copysign(0.25, p.x), 0, 0))
    elif p.y < -0.15 and p.z > 0.25:
        d = Vector((0.0, 0.55, -0.8))
    elif p.z < 0.23 and (abs(abs(p.x) - FORE_X) < 0.03 or abs(abs(p.x) - HIND_X) < 0.03):
        d = Vector((0.0, 0.1, -1.0))
    elif n.z < -0.5:
        d = Vector((0.0, 1.0, 0.1))
    else:
        d = Vector((0.0, 1.0, -0.35 * max(0.0, -n.z + abs(n.x))))
    d -= n * d.dot(n)
    return d.normalized() if d.length > 1e-6 else Vector((0.0, 1.0, 0.0))


def tone(p, n):
    """A coat-independent shade: the creases, the eye ring, the ears'
    cups, and soft mottling."""
    p, n = Vector(p), Vector(n)
    v = 0.92
    if p.z > 0.36 and p.y < -0.26:
        t, u, s = head_coords(p)
        e = (p - at(EYE[0], EYE[1], math.copysign(EYE[2], s))).length
        v -= 0.10 * smooth((0.014 - e) / 0.004)
        if is_ear(p):
            c = ear_cavity(p)
            v -= 0.18 * smooth((0.004 - abs(c - 0.002)) / 0.003)
    v += 0.05 * rig.noise(p, (12.0, 12.0, 12.0), seed=2.7) + 0.03 * rig.noise(p, (30.0, 30.0, 30.0), seed=3.9)
    return max(0.3, min(1.0, v))


def dense_zone(p):
    """Where the coat's colour changes on a line (round the eyes, the
    muzzle, the ears, the paws) and the thin parts (the ears, the toes):
    the game mesh keeps more vertices there."""
    if p[2] < 0.03:
        return True
    if p[1] < -0.26 and p[2] > 0.36:
        t, u, s = head_coords(p)
        return t > 0.10 or is_ear(p) or abs(t - EYE[0]) < 0.02 and abs(abs(s) - EYE[2]) < 0.015
    return False


# ---------------------------------------------------------------------------
# The detail baked onto the game mesh: ``horse.py``'s ``paint_sculpt`` with
# a fox's fur and its coat's colours.


def paint_sculpt(obj):
    """Masks on the dense sculpt (``Mask``: R fur, G claw, B bare skin; A
    the tone), the coat (``Coat``: RGB linear, A the grizzle) and the lie
    of the fur as weights on nine axes (``FlowA``-``FlowC``); and the
    material that turns them into the detail baked onto the game mesh:
    fine hairs gathered into locks along the lie, the agouti speckle of
    the guard hairs (a pale band, a black tip) where the coat is grizzled,
    the cobbles of the nose leather and the pads, the claws' horn. Its
    colour is the coat's; ``set_output(mat, "rough")`` makes it emit the
    roughness instead (dull fur, the wet nose glossy)."""
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
    for i in range(count):
        p, n = unwarp(tuple(co[i])), tuple(nor[i])
        claw = 1.0 if is_claw(p) else 0.0
        skin = bare(p) * (1.0 - claw)
        m[i] = (1.0 - max(claw, skin), claw, skin, tone(p, n))
        rgb, g = coat_at(p, n)
        if claw:
            rgb = COAT["claw"]
        elif skin > 0.0:
            rgb = _mix(rgb, COAT["nose"], skin)
        coat[i, :3] = H.srgb(rgb)[:3]
        coat[i, 3] = g * (1.0 - skin)
        w = H.flow_weights(hair_direction(p, n))
        for k in range(3):
            f[k, i, :3] = w[3 * k : 3 * k + 3]
    mesh.color_attributes.new("Mask", "FLOAT_COLOR", "POINT").data.foreach_set("color", m.ravel())
    mesh.color_attributes.new("Coat", "FLOAT_COLOR", "POINT").data.foreach_set("color", coat.ravel())
    for k, c in enumerate("ABC"):
        mesh.color_attributes.new(f"Flow{c}", "FLOAT_COLOR", "POINT").data.foreach_set("color", f[k].ravel())
    mat = bpy.data.materials.new("Sculpt")
    mat.use_nodes = True
    nt = mat.node_tree
    nodes, link = nt.nodes, nt.links.new
    bsdf = nodes["Principled BSDF"]
    coord = nodes.new("ShaderNodeTexCoord")

    def attribute(layer):
        c = nodes.new("ShaderNodeVertexColor")
        c.layer_name = layer
        sep = nodes.new("ShaderNodeSeparateColor")
        link(c.outputs["Color"], sep.inputs["Color"])
        return [sep.outputs["Red"], sep.outputs["Green"], sep.outputs["Blue"]], c.outputs["Alpha"], c.outputs["Color"]

    (fur, horn, skin_), tone_out, _ = attribute("Mask")
    _, grizzle, coat_rgb = attribute("Coat")
    weights = []
    for c in "ABC":
        weights += attribute(f"Flow{c}")[0]

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

    # Fine hairs (3 cm long, half a millimetre wide) gathered into locks
    # (about 4 mm wide), blended by the lie's weights; and the agouti
    # speckle, a pattern of single guard hairs of its own along the lie.
    hair, speck = None, None
    for k, (axis, w) in enumerate(zip(H.FLOW_AXES, weights)):
        lay = math_("ADD", math_("MULTIPLY", strokes(axis, 30.0, 1400.0), None, 0.45),
                    math_("MULTIPLY", strokes(axis, 9.0, 240.0, detail=2.0), None, 0.55))
        guard = strokes(axis, 22.0, 900.0, detail=1.0, offset=(0.37 * k, 0.11 * k, 0.53))
        term, sterm = math_("MULTIPLY", lay, w), math_("MULTIPLY", guard, w)
        hair = term if hair is None else math_("ADD", hair, term)
        speck = sterm if speck is None else math_("ADD", speck, sterm)
    # The claws: fine lengthwise tubules.
    horn_h = math_("ADD", math_("MULTIPLY", strokes((0, 1, 0), 60.0, 3000.0, detail=2.0), None, 0.3), None, 0.35)
    # Bare skin: the cobbles of the nose leather and the pads.
    cobbles = nodes.new("ShaderNodeTexVoronoi")
    cobbles.inputs["Scale"].default_value = 1300.0
    link(coord.outputs["Object"], cobbles.inputs["Vector"])
    skin_h = math_("ADD", math_("MULTIPLY", cobbles.outputs["Distance"], None, 0.8), None, 0.1)
    height = mix(mix(skin_h, horn_h, horn), hair, fur)
    bump = nodes.new("ShaderNodeBump")
    bump.inputs["Strength"].default_value = 1.0
    bump.inputs["Distance"].default_value = 0.0012
    link(height, bump.inputs["Height"])
    link(bump.outputs["Normal"], bsdf.inputs["Normal"])
    # Colour: the coat, darker between the locks, times the tone; where it
    # is grizzled, the guard hairs' black tips and pale bands over it.
    shade = nodes.new("ShaderNodeMapRange")
    shade.inputs["From Min"].default_value = 0.3
    shade.inputs["From Max"].default_value = 0.7
    shade.inputs["To Min"].default_value = 0.55
    shade.inputs["To Max"].default_value = 1.08
    link(height, shade.inputs["Value"])
    lum = math_("MULTIPLY", shade.outputs["Result"], tone_out)
    dark = nodes.new("ShaderNodeMapRange")
    dark.inputs["From Min"].default_value, dark.inputs["From Max"].default_value = 0.53, 0.60
    link(speck, dark.inputs["Value"])
    pale = nodes.new("ShaderNodeMapRange")
    pale.inputs["From Min"].default_value, pale.inputs["From Max"].default_value = 0.45, 0.39
    link(speck, pale.inputs["Value"])
    col = mix_rgb(coat_rgb, (0.46, 0.39, 0.28, 1.0), math_("MULTIPLY", math_("MULTIPLY", pale.outputs["Result"], grizzle), fur))
    col = mix_rgb(col, (0.018, 0.015, 0.013, 1.0), math_("MULTIPLY", math_("MULTIPLY", dark.outputs["Result"], grizzle), fur))
    gray = nodes.new("ShaderNodeCombineColor")
    for ch in ("Red", "Green", "Blue"):
        link(lum, gray.inputs[ch])
    col = mix_rgb(col, gray.outputs["Color"], math_("ADD", fur, None, 0.0), blend="MULTIPLY")
    link(col, bsdf.inputs["Base Color"])
    # Roughness: dull fur with a soft sheen along each lock; the nose wet
    # and glossy, the pads drier; the claws' horn.
    sheen = nodes.new("ShaderNodeMapRange")
    sheen.inputs["From Min"].default_value = 0.3
    sheen.inputs["From Max"].default_value = 0.7
    sheen.inputs["To Min"].default_value = 0.92
    sheen.inputs["To Max"].default_value = 0.58
    link(height, sheen.inputs["Value"])
    nose_wet = nodes.new("ShaderNodeValue")
    nose_wet.outputs[0].default_value = 0.32
    claw_r = nodes.new("ShaderNodeValue")
    claw_r.outputs[0].default_value = 0.45
    rough = mix(mix(nose_wet.outputs[0], claw_r.outputs[0], horn), sheen.outputs["Result"], fur)
    emit = nodes.new("ShaderNodeEmission")
    emit.name = "Rough"
    link(rough, emit.inputs["Color"])
    obj.data.materials.clear()
    obj.data.materials.append(mat)


# ---------------------------------------------------------------------------
# The eyes: amber, the pupil an upright oval.


def eyes():
    """An eyeball in each socket, looking forward and out: ``Eye_Iris``
    over its visible cap with the upright oval of ``Eye_Black`` (the
    pupil) in it, the rest ``Eye_Black``."""
    out = []
    t, u, s = EYE
    for sx in (1, -1):
        c = at(t, u, sx * (s - 0.0025))
        bpy.ops.mesh.primitive_uv_sphere_add(segments=20, ring_count=14, radius=EYE_RADIUS, location=c)
        eye = bpy.context.active_object
        eye.name = "Eye"
        bpy.ops.object.transform_apply(location=True, rotation=True, scale=True)
        eye.data.materials.append(bpy.data.materials["Eye_Black"])
        eye.data.materials.append(bpy.data.materials["Eye_Iris"])
        look = (Vector((sx * 0.62, 0, 0)) + Vector(HEAD_A) * 0.78 + Vector(HEAD_U) * 0.05).normalized()
        up = Vector(HEAD_U)
        up = (up - look * up.dot(look)).normalized()
        side = look.cross(up)
        for poly in eye.data.polygons:
            d = (poly.center - c).normalized()
            a, b = d.dot(side), d.dot(up)
            pupil = d.dot(look) > 0 and (a / 0.13) ** 2 + (b / 0.36) ** 2 < 1.0
            poly.material_index = 1 if d.dot(look) > 0.42 and not pupil else 0
            poly.use_smooth = True
        warp_mesh(eye)
        out.append(eye)
    return out


# ---------------------------------------------------------------------------
# Hair cards: the fur's tufts and the tail's brush on a coloured atlas.


def _jit(i, salt):
    return H.jitter(i, salt)


def _tuft(sdf, root, d, length, width, column, bone="Body", gap=0.0015, lift=0.25, droop=0.3, points=5):
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
    zc = 0.29 if zc is None else zc
    a = math.radians(ang)
    d = Vector((math.sin(a), 0.0, math.cos(a)))
    return sdf.surface(Vector((0.0, y, zc)), d, reach=0.15), d


def card_column(p, n):
    """The atlas column whose hair matches the coat at ``p`` facing ``n``."""
    rgb, g = coat_at(p, n)
    lum = sum(rgb) / 3.0
    if g > 0.45:
        return DORSAL if lum < 105 else (GRIZZLED if (p[0] * 97.0 + p[1] * 31.0) % 1.0 > 0.5 else GRIZZLED_2)
    if lum < 75:
        return BLACK
    if rgb[0] - rgb[2] > 55 and lum < 175:
        return RUFOUS
    return CREAM


def fur_spots(sdf):
    """The fur's tufts, five points each, lying with the coat and coloured
    as it is under them (``card_column``): the ruff of the cheeks flaring
    back, the long guard hairs over the nape,
    the shoulders and down the spine, the
    pants behind the thighs, the fringe of the belly and the elbows."""
    out = []
    i = 0

    def add(root, d, length, width, bone="Body", lift=0.3, droop=0.15, column=None, give=0.0):
        n = sdf.sample(root)[1]
        col = card_column(tuple(root), tuple(n)) if column is None else column
        out.append(_tuft(sdf, root, d, length, width, col, bone, lift=lift, droop=droop)[:5] + (give,))

    for sx in (1, -1):
        for k in range(18):  # the cheeks
            i += 1
            t = 0.005 + 0.06 * _jit(i, 1)
            u = -0.040 - 0.028 * _jit(i, 2)
            root = sdf.surface(at(t, u, 0.0), Vector((sx, 0.0, -0.4)), reach=0.08)
            add(root, Vector((sx * 0.3, 0.9, -0.35)), 0.009 + 0.006 * _jit(i, 3), 0.0035 + 0.0015 * _jit(i, 4), "Head", lift=0.12, droop=0.4,
                column=GRIZZLED_2 if _jit(i, 5) > 0.6 else None)
        for k in range(10):  # the elbows
            i += 1
            root = sdf.surface(Vector((sx * 0.03, -0.150 + 0.02 * _jit(i, 5), 0.215 + 0.02 * _jit(i, 6))), Vector((sx * 0.5, 0.4, -0.6)), reach=0.08)
            add(root, Vector((0, 0.5, -1)), 0.016 + 0.008 * _jit(i, 7), 0.006, lift=0.3)
        for k in range(22):  # the pants
            i += 1
            z = 0.19 + 0.09 * _jit(i, 8)
            root = sdf.surface(Vector((sx * 0.036, 0.16, z)), Vector((sx * 0.35, 1.0, -0.1)), reach=0.1)
            add(root, Vector((sx * 0.1, 0.4, -1.0)), 0.016 + 0.010 * _jit(i, 9), 0.007 + 0.002 * _jit(i, 10), f"BackLeg.{'L' if sx > 0 else 'R'}",
                lift=0.3, droop=0.25, give=1.0)
    for k in range(34):  # the nape, the shoulders and down the spine
        i += 1
        y = -0.28 + 0.44 * _jit(i, 17) ** 1.3
        ang = (_jit(i, 18) - 0.5) * (110.0 if y < -0.1 else 50.0)
        zc = 0.30 + 0.12 * smooth((-0.15 - y) / 0.13)
        root, d0 = _on_trunk(sdf, y, ang, zc)
        add(root, Vector((0, 1.0, -0.3)) + d0 * 0.1, 0.014 + 0.010 * _jit(i, 20), 0.005 + 0.002 * _jit(i, 21), lift=0.12, droop=0.35,
            column=GRIZZLED_2 if card_column(tuple(root), tuple(sdf.sample(root)[1])) == GRIZZLED else None)
    for k in range(14):  # the belly's fringe
        i += 1
        y = -0.12 + 0.20 * _jit(i, 27)
        ang = 180.0 + (_jit(i, 28) - 0.5) * 110.0
        root, d0 = _on_trunk(sdf, y, ang, 0.28)
        add(root, Vector((0, 1.0, -0.4)), 0.013 + 0.006 * _jit(i, 29), 0.006, lift=0.1, droop=0.3)
    return out


def ear_tufts(sdf):
    """The long white hair inside each ear's rim, four points each."""
    out = []
    for sx, side in ((1, "L"), (-1, "R")):
        base, tip = Vector(_ear(EAR_BASE, sx)), Vector(_ear(EAR_TIP, sx))
        front = (Vector((sx * 0.30, 0.0, 0.0)) - Vector(HEAD_A) * 0.95).normalized()
        up = (tip - base).normalized()
        for k in range(14):
            f = 0.08 + 0.6 * k / 13
            root = base.lerp(tip, f) + front * 0.002 + Vector((sx * (0.008 if k % 2 else -0.008) * (1 - f), 0, 0))
            d = (up * 0.8 + front * 0.45 + Vector((sx * (0.12 if k % 2 else -0.12), 0, 0))).normalized()
            length = 0.011 + 0.006 * _jit(k, 31 + sx)
            pts = [root + d * length * j / 3 for j in range(4)]
            out.append((pts, [0.005, 0.0045, 0.0035, 0.0015], [front] * 4, WISP_WHITE, f"Ear1.{side}", 1.0))
    return out


def whisker_spots(sdf):
    """The stiff black whiskers on the muzzle and over the eyes, four
    points each."""
    out = []
    for sx in (1, -1):
        for k in range(12):
            t = NOSE_T - 0.040 + 0.028 * _jit(k, 41 + sx)
            u = -0.040 - 0.008 * _jit(k, 43 + sx)
            root = sdf.surface(at(t, u, 0.0), Vector((sx, 0, -0.1)), reach=0.05)
            d = (Vector((sx * 0.8, 0, 0)) - Vector(HEAD_A) * 0.45 - Vector(HEAD_U) * (0.1 + 0.25 * _jit(k, 45))).normalized()
            length = 0.035 + 0.025 * _jit(k, 47 + sx)
            pts = [root + d * length * j / 3 + Vector((0, 0, -0.003 * j * j / 9)) for j in range(4)]
            out.append((pts, [0.0035, 0.003, 0.0022, 0.0012], [Vector((0, 0, 1))] * 4, WISP_BLACK, "Head", 0.0))
    return out


def tail_spots(sdf):
    """The brush: locks all along the tail, radiating round it and lying
    back toward the tip, longest at its middle; grey, darker on top, the
    last fifth black. Seven points each."""
    path = rig._catmull([Vector(p) for p in TAIL], 4)
    total = len(path) - 1
    strands = []
    for i in range(420):
        f = 0.04 + 0.96 * _jit(i, 51) ** 0.85
        k = min(int(f * total), total - 1)
        c = path[k][0].lerp(path[k + 1][0], f * total - k)
        along = (path[k + 1][0] - path[k][0]).normalized()
        ang = 2 * math.pi * _jit(i, 53)
        side = Vector((1.0, 0.0, 0.0))
        up = along.cross(side).normalized()
        if up.z < 0:
            up = -up
        out = (side * math.cos(ang) + up * math.sin(ang)).normalized()
        if f < 0.16 and out.dot(up) < 0.2:
            continue  # the root's underside: a lock there hangs behind the thigh, not on the tail
        root = sdf.surface(c, out, reach=0.12)
        spread = 0.95 * (1.0 - 0.75 * smooth((f - 0.7) / 0.2))  # the tip's locks even, so it ends in a clean point
        length = (0.040 + 0.042 * math.sin(math.pi * min(1.0, f * 1.1)) ** 0.7) * (1.4 - spread + spread * _jit(i, 55) ** 1.5)
        twist = side * (_jit(i, 61) - 0.5) * 1.0 + up * (_jit(i, 69) - 0.5) * 0.5
        pts, outs = H.fall(sdf, root, (out * (0.4 + 0.6 * _jit(i, 63)) + along + twist).normalized(), length, 0.0015 + 0.004 * _jit(i, 65),
                           steps=6, gravity=(along + out * 0.1 + twist * 0.5 + Vector((0, 0, -0.2))).normalized(), stiff=0.3 + 0.3 * _jit(i, 67))
        top = out.dot(up)
        if f > 0.78:
            col = BLACK
        elif top > 0.6:
            col = DORSAL
        else:
            col = GRIZZLED if _jit(i, 57) > 0.7 else GRIZZLED_2
        width = 0.005 + 0.011 * _jit(i, 59) ** 2
        widths = [width * (0.6 + 0.8 * (j / 6) ** 0.6) * (1.0 - 0.6 * (j / 6) ** 2.5) for j in range(7)]
        strands.append((pts, widths, outs, col))
    return strands


def fur_atlas(size, seed=7):
    """The cards' atlas: in each of the eight columns a tuft of fine hairs
    hanging from the top, each its own length and its own colour along it
    (``card_colour``, its agouti band shifted at random), a third of them
    short pale underfur beneath the guard hairs; the two wispy columns a
    few loose hairs. Returns colour (sRGB), opacity, normal (OpenGL) and
    roughness arrays, rows bottom up."""
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
        for k in range(26 if wisp else 150):
            under = not wisp and k % 3 == 0
            x0 = rng.uniform(0.35, 0.65) if wisp else rng.uniform(0.05, 0.95)
            length = rng.uniform(0.35, 0.6) if under else rng.uniform(0.65, 0.99)
            phase, freq = rng.uniform(0, 6.3), rng.uniform(2.0, 6.0)
            path = x0 + (0.5 - x0) * rng.uniform(0.1, 0.45) * v ** 1.5 + rng.uniform(-0.1, 0.1) * v ** 2 + 0.008 * np.sin(freq * v * 6.28 + phase) * v
            width = rng.uniform(0.7, 1.3 if wisp else 1.7) / w * (1.0 - 0.6 * np.clip(v / length, 0, 1))
            dist = np.abs(u - path) / width
            on = (dist < 1.0) & (v < length)
            fade = np.clip((length - v) / 0.05, 0, 1)
            along = np.clip(v / length, 0.0, 1.0)
            if under:
                table = np.array([_mix(card_colour(CREAM, x), card_colour(c, 0.0), 0.5) for x in samples], np.float32)
            else:
                shift = rng.uniform(-0.14, 0.14)
                table = np.array([card_colour(c, x, shift) for x in samples], np.float32)
            idx = np.clip((along * 63).astype(int), 0, 63)
            shade = rng.uniform(0.82, 1.08) * (0.72 + 0.28 * np.clip(along * 3.0, 0, 1)) * (0.8 if under else 1.0)
            hgt = np.where(on, (1.0 - dist * dist) * fade * (0.5 if under else 1.0), 0.0)
            take = hgt > col_h
            col_h = np.where(take, hgt, col_h)
            col_rgb = np.where(take[..., None], table[idx] / 255.0 * shade[..., None] if np.ndim(shade) else table[idx] / 255.0 * shade, col_rgb)
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
    rough = np.full((h, size, 3), 0.68, np.float32)
    rough[:, WISP_BLACK * w :] = 0.45
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
    brush) take no weight from the tail where they sit mostly on the
    body: a tuft of the pants or the croup near its root follows the body,
    not the lifted tail (a tuft on the tail's own skin keeps following it)."""
    HORSE_WEIGHT_HAIR(cards, arm, kinds)
    if kinds and kinds[0] == "tail":
        return
    for v in cards.data.vertices:
        tail = [g for g in v.groups if cards.vertex_groups[g.group].name.startswith("Tail")]
        if not tail or sum(g.weight for g in tail) > 0.5:
            continue  # none, or a tuft on the tail itself: it follows the tail
        for g in tail:
            cards.vertex_groups[g.group].remove([v.index])
        rest = [g for g in v.groups]
        if rest:
            total = sum(g.weight for g in rest)
            for g in rest:
                g.weight = g.weight / total if total > 1e-6 else 1.0 / len(rest)
        else:
            cards.vertex_groups["Back"].add([v.index], 1.0, "REPLACE")


def tail_only(body, arm):
    """The tail is skin of its own: each vertex of it takes its weight from
    the tail chain only (the nearest point of it), so the hind legs never
    drag it and it never drags them; blended into the croup's weights over
    its root."""
    chain = [(f"Tail{k + 1}", arm.data.bones[f"Tail{k + 1}"].head_local.copy()) for k in range(9)]
    chain.append((None, arm.data.bones["Tail9"].tail_local.copy()))
    groups = {g.name: g for g in body.vertex_groups}

    def group(name):
        if name not in groups:
            groups[name] = body.vertex_groups.new(name=name)
        return groups[name]

    for v in body.data.vertices:
        p, q = v.co, unwarp(v.co)
        place = tail_place(q)
        if place is None or q[1] < 0.215:
            continue
        own = smooth((q[1] - 0.218) / 0.03)
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
    bpy.ops.object.select_all(action="DESELECT")
    body.select_set(True)
    bpy.context.view_layer.objects.active = body
    bpy.ops.object.vertex_group_limit_total(group_select_mode="ALL", limit=4)
    bpy.ops.object.vertex_group_normalize_all(lock_active=False)


def ears_only(body, arm):
    """Each ear's skin follows its own two bones (from its base to its
    tip), blended into the head's over the base: heat weights split a thin
    ear between the head and the ear, so a laid-back ear tore into a fan."""
    groups = {g.name: g for g in body.vertex_groups}

    def group(name):
        if name not in groups:
            groups[name] = body.vertex_groups.new(name=name)
        return groups[name]

    mid = (EAR_MID[1] - EAR_BASE[1]) / (EAR_TIP[1] - EAR_BASE[1])
    for v in body.data.vertices:
        p = unwarp(v.co)
        if not is_ear(p):
            continue
        t, u, s = head_coords(p)
        side = "L" if s > 0 else "R"
        f = max(0.0, min(1.0, (u - EAR_BASE[1]) / (EAR_TIP[1] - EAR_BASE[1])))
        own = smooth((u - 0.004) / 0.012)
        tip = smooth((f - mid + 0.15) / 0.3)
        want = {f"Ear1.{side}": own * (1.0 - tip), f"Ear2.{side}": own * tip, "Head": 1.0 - own}
        for g in list(v.groups):
            body.vertex_groups[g.group].remove([v.index])
        for k, w in want.items():
            if w > 1e-4:
                group(k).add([v.index], w, "REPLACE")
    bpy.ops.object.select_all(action="DESELECT")
    body.select_set(True)
    bpy.context.view_layer.objects.active = body
    bpy.ops.object.vertex_group_normalize_all(lock_active=False)


def elbows(body):
    """``horse.py``'s ``elbows`` at a fox's size: the trunk behind and
    above each elbow follows the chest, not the swinging foreleg."""
    groups = {g.name: g for g in body.vertex_groups}
    torso = groups.get("Torso2") or body.vertex_groups.new(name="Torso2")
    for v in body.data.vertices:
        p = Vector(unwarp(v.co))
        trunk_ = max(smooth((p.y + 0.15) / 0.04), smooth((0.028 - abs(p.x)) / 0.012)) * smooth((p.z - 0.225) / 0.035)
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
    bpy.ops.object.select_all(action="DESELECT")
    body.select_set(True)
    bpy.context.view_layer.objects.active = body
    bpy.ops.object.vertex_group_normalize_all(lock_active=False)
    for v in body.data.vertices:
        v.select = any((Vector(unwarp(v.co)) - Vector((sx * 0.045, -0.15, 0.22))).length < 0.06 for sx in (1, -1))
    bpy.ops.object.mode_set(mode="EDIT")
    bpy.ops.object.vertex_group_smooth(group_select_mode="ALL", factor=0.5, repeat=6)
    bpy.ops.object.mode_set(mode="OBJECT")
    bpy.ops.object.vertex_group_limit_total(group_select_mode="ALL", limit=4)
    bpy.ops.object.vertex_group_normalize_all(lock_active=False)


NOT_SKIN = {"Body", "PoleTarget.L", "PoleTarget.R", "PoleTargetBack.L", "PoleTargetBack.R",
            "FrontShoulder.L", "FrontShoulder.R", "BackShoulder.L", "BackShoulder.R", "Torso"}
FOLDS = [((0.05, 0.15, 0.31), 0.05, 8), ((0.055, 0.12, 0.20), 0.05, 12)]


def skin(body, arm):
    """Heat weights from the bones that move the skin (on the fox scaled
    up four times, the heat solver's comfortable size); then the elbows,
    the tail apart and the folds round the hip and the stifle smoothed."""
    for b in arm.data.bones:
        b.use_deform = b.name not in NOT_SKIN
    for o in (body, arm):
        o.scale = (4.0, 4.0, 4.0)
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
    H.use(FOLDS=[(warp(c), r, n) for c, r, n in FOLDS])
    H.smooth_folds(body)


# ---------------------------------------------------------------------------
# Clips: ``horse.py``'s machinery with a fox's carriage: the head carried
# low and forward, the ears always busy, the tail hanging in a curve at
# rest and streaming level behind at the gallop, the back flexing with
# each bound.

NECK = ("Neck1", "Neck2", "Neck3", "Head")
PIVOT = (0.0, 0.0, 0.29)
HOOF = {"F": dict(toe=-0.230, heel=-0.181), "H": dict(toe=0.128, heel=0.175)}


def body_matrix(lift=0.0, nose_down=0.0, roll=0.0, yaw=0.0, forward=0.0, pivot=None):
    """``horse.py``'s ``body_matrix`` about the fox's own pivot."""
    return HORSE_BODY_MATRIX(lift, nose_down, roll, yaw, forward, Vector(warp(PIVOT)) if pivot is None else pivot)


def pose_face(pose, ears=((0, 0), (0, 0)), jaw=0.0, grind=0.0, lip=0.0, nostrils=0.0):
    """Ears turned back (degrees) and swivelled out, per side (L, R), their
    tips following a little; the jaw open."""
    lat, up, along = H.head_axes(pose)
    for (back, out), side, sx in zip(ears, ("L", "R"), (1, -1)):
        pose.turn(f"Ear1.{side}", H.rot(up, sx * out) @ H.rot(lat, -back))
        pose.turn(f"Ear2.{side}", H.rot(lat, -0.25 * back))
    pose.turn("Jaw", H.rot(up, grind) @ H.rot(lat, -jaw))


def pose_hair(pose, tail=(0.0, 0.0), tail_lift=0.0, lag=0.0, t=0.0, cycles=1.0, tail_curve=0.0, tail_whip=0.0, **_):
    """The tail swings ``tail`` (amplitude, phase) down its chain with
    ``lag`` cycles per bone; its root lifted ``tail_lift`` degrees (back and
    up, over its first three bones), each bone after them bending
    ``tail_curve`` back down; a wave of ``tail_whip`` runs down it."""
    amp, phase = tail
    for k, name in enumerate(TAIL_CHAIN):
        m = pose.posed(name).to_3x3() @ pose.rest(name).to_3x3().inverted()
        side = amp * H.wave(t, cycles, phase - lag * k) / len(TAIL_CHAIN) * (1.0 + 0.3 * k)
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
    pose_hair(pose, **dict(dict(tail=(4.0, 0.0), lag=0.06), **(hair or {})), t=t)
    stand_legs(pose, body, offsets, rests)
    return pose


def stand_legs(pose, body, offsets=None, rests=None):
    """``horse.py``'s ``stand_legs`` with the thoracic sling on the
    forelegs: as the chest drops (the head down to the ground) the trunk
    sinks between the shoulder blades and the standing forelegs stay nearly
    straight, rather than folding forward at the wrist."""
    offsets = offsets or {}
    rests = rests or {}
    for leg in H.LEGS:
        dy, toe = offsets.get(leg, 0.0), rests.get(leg, 0.0)
        f = H.hoof_place(leg, dy, toe)
        fore = leg[1] == "F"
        H.pose_leg(pose, body, leg, f, toe, slide=-0.7 * dy if fore else 0.0, sling=1.0 if fore else 0.0)


def idle(arm, t):
    # Alert: breathing, the head turning to a sound and back, the ears
    # swivelling one at a time, the tail swaying slowly.
    body = body_matrix(lift=0.0015 * H.wave(t, 3), nose_down=0.3 * H.wave(t, 1, 0.25))
    look = H.ease((t - 0.15) / 0.1) * (1 - H.ease((t - 0.55) / 0.12))
    neck = (2.0 + 1.0 * H.wave(t, 1, 0.1), 1.0, 0.0, 3.0 * look + 1.5 * H.wave(t, 2, 0.3))
    ears = ((-5 + 30 * H.flick(t, 0.3, 0.07), 20 * H.flick(t, 0.3, 0.07)), (-5 + 10 * look, 35 * H.flick(t, 0.75, 0.08)))
    return standing(arm, body, neck, yaw=24.0 * look, t=t, ears=ears, hair=dict(tail=(6.0, 0.0)))


def idle_2(arm, t):
    # Looking back over the shoulder, then up to sniff the air; a hind leg
    # rested on its toes.
    body = body_matrix(lift=0.0015 * H.wave(t, 3), roll=0.8, nose_down=0.3)
    back = H.ease(t / 0.2) * (1 - H.ease((t - 0.45) / 0.15))
    sniff = H.ease((t - 0.55) / 0.12) * (1 - H.ease((t - 0.85) / 0.12))
    neck = (4.0 * sniff, 2.0, 0.0, 14.0 * sniff + 2.0 * sniff * H.wave(t, 12))
    ears = ((10 * back, 30 * back), (-5 + 25 * H.flick(t, 0.6), 10))
    return standing(arm, body, neck, yaw=-55.0 * back, t=t, ears=ears, offsets={"RH": -0.025}, rests={"RH": -25.0},
                    hair=dict(tail=(8.0, 0.3)))


def idle_headlow(arm, t):
    # Nose to the ground, following a scent from side to side, the ears
    # pricked forward.
    body = body_matrix(lift=-0.004, nose_down=4.0)
    sweep = 14.0 * H.wave(t, 2) + 4.0 * H.wave(t, 5, 0.2)
    neck = tuple(w * GRAZE_BEND * 0.9 for w in GRAZE_SHAPE)
    neck = (neck[0], neck[1], neck[2], neck[3] + 3.0 * H.wave(t, 9))
    ears = ((-12 + 6 * H.flick(t, 0.5), 5), (-12, 5 + 20 * H.flick(t, 0.2)))
    return standing(arm, body, neck, yaw=sweep, t=t, ears=ears, offsets={"LF": -0.02}, hair=dict(tail=(5.0, 0.2)))


GRAZE_SHAPE = (-1.0, 0.0, 0.0, 0.3)
GRAZE_LIFT, GRAZE_PITCH = -0.04, 15.0
GRAZE_BEND = 45.0


def eating(arm, t):
    # 6 s: down to something in the grass, tearing at it with jerks of the
    # head, chewing with the head up a little, then down again.
    down = H.ease((t - 0.04) / 0.10) * (1 - H.ease((t - 0.84) / 0.12))
    tear = max(0.0, H.wave(t, 8)) ** 3 * down * (1.0 - H.ease((t - 0.45) / 0.05) * (1 - H.ease((t - 0.62) / 0.05)))
    chewing = H.ease((t - 0.45) / 0.05) * (1 - H.ease((t - 0.62) / 0.05))
    body = body_matrix(lift=GRAZE_LIFT * down, nose_down=GRAZE_PITCH * down)
    bend = GRAZE_BEND * down * (1.0 - 0.35 * chewing)
    neck = tuple(w * bend for w in GRAZE_SHAPE)
    neck = (neck[0], neck[1], neck[2], neck[3] + 10.0 * tear)
    jaw = 14.0 * tear + chewing * 8.0 * max(0.0, H.wave(t, 16))
    ears = ((-5 + 15 * H.flick(t, 0.3), 10), (-5, 10 + 25 * H.flick(t, 0.7)))
    return standing(arm, body, neck, yaw=12.0 * tear * H.wave(t, 4), t=t, ears=ears, jaw=jaw, offsets={"LF": -0.03 * down},
                    hair=dict(tail=(4.0, 0.2)))


def walk(arm, t):
    # A lateral four-beat walk, the head low and level, the tail hanging
    # and swaying with the hips.
    body = body_matrix(lift=-0.004 + 0.002 * H.wave(t, 2, 0.1), nose_down=0.6 * H.wave(t, 2, 0.35), roll=1.5 * H.wave(t, 1, 0.2))
    nod = 2.0 * H.wave(t, 2, 0.05)
    neck = (-6.0 + nod, -2.0 + 0.5 * nod, 0.0, 3.0 - 0.6 * nod)
    hair = dict(tail=(10.0, 0.1), lag=0.06, cycles=1.0, tail_lift=5.0)
    ears = ((-3 + 15 * H.flick(t, 0.4, 0.08), 8), (-3, 8 + 15 * H.flick(t, 0.85, 0.08)))
    return H.locomotion(arm, t, WALK, body, neck, slide=0.028, hair=hair, ears=ears)


def gallop(arm, t):
    # A rotary gallop: the hinds land under the belly with the back
    # rounded, drive it straight, the fores reach far out and land, the
    # body sails extended, then gathers. The head stretched forward and
    # steady, the ears laid back, the tail streaming level.
    pitch = H.wave(t, 1, 0.0)
    body = body_matrix(lift=-0.012 + 0.016 * H.wave(t, 2, 0.05), nose_down=4.0 * pitch)
    neck = (-10.0 + 3.0 * H.wave(t, 1, 0.2), -4.0, 0.0, 8.0 - 2.0 * H.wave(t, 1, 0.2))
    hair = dict(tail=(6.0, 0.0), tail_lift=34.0, lag=0.08, tail_curve=3.0, tail_whip=8.0)
    flex = 9.0 * H.wave(t, 1, 0.25)
    return H.locomotion(arm, t, GALLOP, body, neck, slide=0.035, flex=flex, hair=hair, ears=((28, 8), (28, 8)))


POUNCE_SECONDS = 64 / 48


def gallop_jump(arm, t):
    # The mouse pounce: a crouch with the ears pricked, a spring up and
    # forward, the body arching nose down at the top, the forepaws reaching
    # down together onto the prey, then up again into the stance.
    crouch = H.ease(t / 0.2) * (1 - H.ease((t - 0.3) / 0.05))
    air = H.ease((t - 0.3) / 0.08) * (1 - H.ease((t - 0.62) / 0.08))
    arc = math.sin(math.pi * max(0.0, min(1.0, (t - 0.30) / 0.38)))
    dive = H.ease((t - 0.42) / 0.18) * (1 - H.ease((t - 0.75) / 0.15))
    body = body_matrix(lift=-0.03 * crouch + 0.22 * arc, nose_down=-18.0 * air * (1 - dive) + 40.0 * dive * air + 6.0 * dive * (1 - air),
                       forward=0.0)
    pose = H.Pose(arm)
    pose.carry("Body", body)
    pose.turn("Torso2", H.rot(body.to_3x3() @ H.X, -6.0 * air + 8.0 * crouch))
    neck = (-6.0 + 10.0 * crouch - 12.0 * dive, -2.0, 0.0, 6.0 - 10.0 * dive)
    H.pose_neck(pose, body, neck)
    pose_face(pose, ((-15, 0), (-15, 0)), jaw=10.0 * dive * (1 - air))
    pose_hair(pose, tail=(4.0, 0.0), tail_lift=45.0 * air + 10.0 * crouch, lag=0.06, t=t, tail_curve=-3.0 * air)
    for leg in H.LEGS:
        rest = H.hoof_place(leg, 0.0, 0.0)
        if leg[1] == "F":
            reach = body @ (H.fetlock_rest(leg) + Vector((0.0, -0.06, 0.03))) * 1.0
            f = rest.lerp(reach, air)
            f.z = max(f.z, rest.z)
            toe = -50.0 * air * (1 - dive) + 20.0 * dive * air
        else:
            tuck = body @ (H.fetlock_rest(leg) + Vector((0.0, 0.07, 0.05)))
            f = rest.lerp(tuck, air)
            f.z = max(f.z, rest.z)
            toe = -60.0 * air
        H.pose_leg(pose, body, leg, f, toe, hip=0.6)
    return pose


def clips(arm):
    global GRAZE_BEND
    GRAZE_BEND = H.graze_bend(arm, height=0.012)
    H.GRAZE_BEND = GRAZE_BEND
    print(f"fox: nose-down bend {GRAZE_BEND:.1f} degrees")
    made = [
        rig.bake_clip(arm, "Idle", 160 / 48, idle, H.MOVED),
        rig.bake_clip(arm, "Idle_2", 160 / 48, idle_2, H.MOVED),
        rig.bake_clip(arm, "Idle_Headlow", 160 / 48, idle_headlow, H.MOVED),
        rig.bake_clip(arm, "Eating", 288 / 48, eating, H.MOVED),
    ]
    H.SHORT.clear()
    made.append(rig.bake_clip(arm, "Walk", WALK["seconds"], walk, H.MOVED))
    print(f"fox: in the walk the standing paws fall short of their marks by at most "
          f"{ {k: round(v, 4) for k, v in sorted(H.SHORT.items())} } m")
    H.SHORT.clear()
    made.append(rig.bake_clip(arm, "Gallop", GALLOP["seconds"], gallop, H.MOVED))
    print(f"fox: in the gallop the standing paws fall short of their marks by at most "
          f"{ {k: round(v, 4) for k, v in sorted(H.SHORT.items())} } m")
    made.append(rig.bake_clip(arm, "Gallop_Jump", POUNCE_SECONDS, gallop_jump, H.MOVED))
    for a, seconds in ((made[4], WALK["seconds"]), (made[5], GALLOP["seconds"])):
        speed, lows = H.measure(arm, a, seconds)
        print(f"fox: {a.name} {seconds:.3f} s: paws slide back at {speed:.2f} m/s; lowest IK bones {[round(z, 4) for z in lows]}")
    print(f"fox: planned walk {ground_speed(WALK):.2f} m/s, gallop {ground_speed(GALLOP):.2f} m/s")
    return made


# ---------------------------------------------------------------------------
# Wiring ``horse.py``'s steps to the fox.

if bpy is not None:
    HORSE_BODY_MATRIX = H.body_matrix
    HORSE_WEIGHT_HAIR = H.weight_hair
    H.use(
        QUATERNIUS=[], ENDS=[(n, b, warp(p)) for n, b, p in ENDS], EXTRA=EXTRA, MUSCLES=[], MUSCLE_NAMES=[], muscle_shape=lambda pose, arm: {},
        HEAD_O=Vector(HEAD_O), HEAD_A=Vector(HEAD_A), HEAD_U=Vector(HEAD_U), EYE=EYE, EYE_RADIUS=EYE_RADIUS,
        bone_table=bone_table, sculpt=sculpt, paint_sculpt=paint_sculpt, material_of=material_of, MATERIALS=MATERIALS,
        FALLBACK=FALLBACK, dense_zone=lambda p: dense_zone(unwarp(p)), BODY_TRIS=5200, eyes=eyes, skin=skin, NOT_SKIN=NOT_SKIN,
        TAIL_CHAIN=TAIL_CHAIN, FOLDS=FOLDS,
        mane_spots=warped(fur_spots), forelock_spots=lambda sdf: [], lash_spots=warped(whisker_spots),
        ear_hair_spots=warped(ear_tufts), tail_spots=warped(tail_spots),
        COATS={"gris": {}}, color_at=lambda coat, p, n: (255, 255, 255), coat_part=lambda coat, part: (255, 255, 255),
        NECK=NECK, MUZZLE=Vector(warp(at(NOSE_T - 0.004, -0.050))), PIVOT=Vector(warp(PIVOT)),
        HOOF={leg: {k: _warp_y(y, 0.0) for k, y in h.items()} for leg, h in HOOF.items()},
        END_REST={name: Vector(warp(p)) for name, _, p in ENDS}, GRAZE_SHAPE=GRAZE_SHAPE, GRAZE_LIFT=GRAZE_LIFT,
        GRAZE_PITCH=GRAZE_PITCH, GRAZE_BEND=GRAZE_BEND, pose_hair=pose_hair, pose_face=pose_face, body_matrix=body_matrix,
        weight_hair=weight_hair,
    )


# ---------------------------------------------------------------------------
# Previews and the command line.


def parse(argv):
    import argparse

    parser = argparse.ArgumentParser(prog="fox.py")
    parser.add_argument("output")
    parser.add_argument("--preview", default=None, metavar="DIR")
    parser.add_argument("--textures", default=None, metavar="DIR")
    parser.add_argument("--draft", action="store_true")
    parser.add_argument("--no-clips", action="store_true")
    parser.add_argument("--shots", default=None, metavar="REGEX")
    return parser.parse_args(argv)


SIDE = ((1.75, 0.0, 0.30), (0.0, 0.02, 0.25), 50)


def shots_for(actions):
    shots = [
        ("side.png", "gris", None, 0, *SIDE),
        ("side_right.png", "gris", None, 0, (-1.55, -0.06, 0.30), (0.0, -0.04, 0.25), 50),
        ("front34.png", "gris", None, 0, (0.95, -1.15, 0.45), (0.0, -0.12, 0.26), 45),
        ("front.png", "gris", None, 0, (0.02, -1.45, 0.34), (0.0, -0.15, 0.28), 50),
        ("rear34.png", "gris", None, 0, (0.85, 1.05, 0.45), (0.0, 0.10, 0.24), 45),
        ("close_head.png", "gris", None, 0, (0.33, -0.78, 0.50), (0.0, -0.37, 0.44), 60),
        ("close_head_side.png", "gris", None, 0, (0.58, -0.38, 0.46), (0.0, -0.38, 0.44), 60),
        ("close_head_front.png", "gris", None, 0, (0.04, -0.88, 0.47), (0.0, -0.38, 0.44), 70),
        ("close_eye.png", "gris", None, 0, (0.22, -0.60, 0.48), (0.02, -0.37, 0.455), 85),
        ("close_coat.png", "gris", None, 0, (0.45, -0.05, 0.42), (0.04, 0.0, 0.30), 60),
        ("close_tail.png", "gris", None, 0, (0.70, 0.62, 0.36), (0.0, 0.34, 0.22), 50),
        ("close_paws.png", "gris", None, 0, (0.30, -0.62, 0.10), (0.03, -0.20, 0.04), 60),
        ("close_hind.png", "gris", None, 0, (0.45, 0.62, 0.17), (0.03, 0.17, 0.12), 55),
        ("close_chest_front.png", "gris", None, 0, (0.06, -0.85, 0.32), (0.0, -0.20, 0.28), 60),
        ("close_hind_paw_side.png", "gris", None, 0, (0.40, 0.11, 0.05), (0.05, 0.11, 0.025), 70),
    ]
    by_name = {a.name: a for a in actions}
    frames = {
        "Walk": (tuple(range(0, 35, 5)), ),
        "Gallop": (tuple(range(0, 16, 2)), ),
        "Gallop_Jump": ((0, 12, 18, 24, 30, 36, 44), ),
        "Eating": ((0, 60, 140, 240), ),
        "Idle": ((0, 50, 100), ),
        "Idle_2": ((40, 110), ),
        "Idle_Headlow": ((40, 100), ),
    }
    for name, (fs,) in frames.items():
        if name in by_name:
            for f in fs:
                shots.append((f"{name.lower()}_{f:03}.png", "gris", by_name[name], f, *SIDE))
    return shots


def main():
    argv = sys.argv[sys.argv.index("--") + 1 :] if "--" in sys.argv else []
    args = parse(argv)
    mesh, arm, images = H.build(args.draft, None)
    mesh.name = mesh.data.name = "Fox"
    use_fur_atlas(images)
    if args.textures:
        for folder, maps in (
            ("fox_body", (("color", images["colour"]), ("normal_gl", images["normal"]), ("roughness", images["rough"]),
                          ("ao", bpy.data.images["horse_body_ao"]))),
            ("fox_hair", (("color", images["hair"]["color"]), ("normal_gl", images["hair"]["normal"]),
                          ("roughness", images["hair"]["rough"]), ("opacity", images["hair"]["opacity"]))),
        ):
            for name, img in maps:
                rig.save_jpeg(img, os.path.join(args.textures, folder, f"{name}.jpg"))
    actions = [] if args.no_clips else clips(arm)
    if args.preview:
        shots = shots_for(actions)
        if args.shots:
            shots = [s for s in shots if re.search(args.shots, s[0])]
        H.render_shots(args.preview, arm, mesh, images, [s for s in shots if s[2] is None])
        H.render_shots(args.preview, arm, mesh, images, [s for s in shots if s[2] is not None])
    for a in list(mesh.data.color_attributes):
        mesh.data.color_attributes.remove(a)
    keep = set(MATERIALS)
    for mat in list(bpy.data.materials):
        if mat.name not in keep:
            bpy.data.materials.remove(mat)
    rig.export(args.output, actions)


if bpy is not None and __name__ == "__main__":
    main()
