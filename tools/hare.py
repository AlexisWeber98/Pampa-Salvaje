"""Model, rig, paint and animate the pampa's hare, the European hare (Lepus
europaeus, the liebre europea, brought to the pampa in the 1880s and
widespread by 1919), in Blender, by script.

Run it headless:

    blender -b --factory-startup --python tools/hare.py -- OUT.glb \
        [--textures DIR] [--photos DIR] [--preview DIR] [--shots REGEX] [--draft] [--no-clips]
        [--silhouettes DIR] [--clay]

``--clay`` also renders each preview in grey clay (``clay_*.png``),
without the coat and the hair cards (``clay``), so the form is judged alone.

``--photos DIR`` projects real hair from the photograph in ``PROJECTIONS``
(found in ``DIR`` through its ``sources.tsv``) onto the body's colour map:
the head's colour and the body's hair detail (``project_photos``).

Writes OUT.glb: one skinned mesh ``Hare`` (and its light mesh for far off,
``Hare_LOD``: the skin alone decimated to ``LIGHT_TRIS``) with the
materials ``Main`` (the whole skin: the coat, the bare nostrils and lips
and the claws, drawn by the body's own colour map), ``Hair`` (the fur's
tufts, the ears' white fringes, the tail's fluff and the whiskers,
alpha-tested cards whose atlas carries their own colours) and the eyes
``Eye_Black`` and ``Eye_Iris``; its armature ``AnimalArmature`` keeps the
Quaternius quadruped's joint names, as the fox's (``tools/fox.py``):
``Body``, ``Back``, ``Torso``-``Torso3``, ``Neck1``-``Neck3``, ``Head``,
the legs, the IK bones and their ``_end`` leaves, two bones per ear, the
jaw and three for the tail; and the cattle's seven clips, played as a
hare's states (``crates/core/src/fauna/wild.rs``, ``HareState``):

- ``Idle``: the alert upright sit, ears up and turning (``Sit``);
- ``Idle_2``: crouched in its form, ears laid along the back (``Sit``);
- ``Idle_Headlow``: sitting up, washing its face with the fore paws (``Sit``);
- ``Eating``: crouched, grazing, the jaw working;
- ``Walk``: the slow hop, the fores placed one after the other and the
  hinds swung up beside them together (``Hop``, ``HOP_SPEED``);
- ``Gallop``: the flight, a gallop whose hind feet land far ahead of the
  fore feet's prints (``Flee``, ``FLIGHT_SPEED``);
- ``Gallop_Jump``: one long escape bound (the game does not play it yet).

The body's maps are baked at ``BAKE_PX`` (2048) and shipped at 1024
(``tools/README.md``); the game draws it as ``Species::Hare``
(``crates/render/src/fauna/livestock.rs``).

The exporter sorts the clips by name: ``tools/glbtrim.py --keep`` puts them
back in this order. Life size, the feet at the origin, facing -Y (glTF +Z).

It is the fox's script (``tools/fox.py``, itself the horse's steps) run
through ``animal_rig.Steps`` with a hare's anatomy, coat and clips put in
its place: the sculpt baked onto a decimated mesh, the hair cards on their
atlas, the skinning and the previews are the fox's; the hind legs have a
solver of their own (``pose_hind``: the metatarsus laid at a given angle,
so the hare can sit on its long hind feet), and the gaits are planned
here (``HOP``, ``FLIGHT``). The sculpt is polygon-modelled (``cage_parts``):
quad cages for the trunk and the neck, the head and each leg (the thigh,
the hamstrings, the fore leg), ``CAGE_N`` vertices a ring laid on the
photographs' outlines, subdivided, pressed by ``MASSES`` (the shoulder
blade, the hollows behind it and in the flank, the cheek), voxelised and
smoothly joined with the small features (the ears, the lids, the claws).

The coat (the reference photographs: Commons, Lepus europaeus; sources in
tools/README.md): the back and the flanks agouti, the guard hairs grey at
the root, an ochre band and a black tip, darker and more speckled along
the back, warmer ochre on the flanks; the nape, the shoulders, the chest
and the sides of the neck rufous ochre; the rump and the thighs greyer;
the belly and the inside of the legs white; the legs and the feet buff;
the head grizzled, a narrow pale cream ring round the large eye (an amber iris
round a large pupil, a dark rim) and a pale streak from the muzzle under
the eye; the muzzle and the chin whitish; the long ears brown grey
behind, darker grey brown in the hollow under sparse pale hairs, a pale
hair fringe along the front edge, black only at the tip;
the tail black on top, white below and at the sides.

Anatomy, in metres (literature: ``SIZE``): head and body 0.60 m along the
spine from the nose to the root of the tail, the ears 0.11 m from the
notch (0.14 m from their base on the skull), the hind foot 0.16 m from the heel to the toes, the
tail 0.11 m; the side view's proportions fitted to the reference
photographs' outlines (``PHOTO_RATIOS``, ``--silhouettes``). See
tools/README.md.
"""

import math
import os
import re
import sys

try:
    import bpy
except ImportError:  # outside Blender: the layout, the coat, the cards' colours and the plans (tests)
    bpy = None

HERE = os.path.dirname(os.path.abspath(__file__))

# ---------------------------------------------------------------------------
# What the literature gives (the model's measures are checked against it by
# tools/test_hare.py).

SIZE = dict(
    head_body=(0.55, 0.65),  # nose to the root of the tail, m (Wikipedia, European hare: "55 to 65 cm")
    tail=(0.075, 0.14),  # m, with its fur
    hind_foot=(0.14, 0.16),  # heel to toes, m ("between 14 and 16 cm")
    ear=(0.094, 0.110),  # from the notch to the tip, m (Wikipedia: "9.4 to 11.0 cm"; ``ear_notch_length``)
    mass=(3.5, 5.0),  # kg
)
SOURCES = [
    ("https://en.wikipedia.org/wiki/European_hare", "head-body 55-65 cm, tail 7.5-14 cm, hind foot 14-16 cm, "
     "ear 9.4-11.0 cm from the notch, 3.5-5 kg, 70 km/h"),
    ("Heglund, Taylor and McMahon 1974, Science 186: 1112-1113", "galloping stride frequency f = 269 M^-0.14 "
     "strides/min (3.7 Hz for 4 kg); with the slow rise of frequency with speed in the gallop, about 4 Hz at 11 m/s"),
]

# The photographs (Wikimedia Commons, licences read on each file page):
# (file, author, licence, use). "reference": proportions, colours and
# poses compared by eye, no pixel of it in any map; "pixels": projected
# into a shipped map (only CC0, public domain or CC BY, ``FREE``).
COMMONS = "https://commons.wikimedia.org/wiki/File:"
FREE = ("CC0", "Public domain", "CC BY 2.0", "CC BY 3.0", "CC BY 4.0")
PHOTOS = [
    ("Zając szarak na polu w Grocholicach w gminie Sadowie w województwie świętokrzyskim, 20260419 0913 5150.jpg",
     "Jakub Hałun", "CC BY 4.0", "pixels"),  # standing, from the side: proportions; its hair projected (``PROJECTIONS``)
    ("Zając szarak na polu w Grocholicach w gminie Sadowie w województwie świętokrzyskim, 20260419 0913 5163.jpg",
     "Jakub Hałun", "CC BY 4.0", "reference"),  # the alert sit, from the side; the tail
    ("Zając szarak na polu w Trębanowie w województwie świętokrzyskim, 20260627 0845 5415.jpg",
     "Jakub Hałun", "CC BY 4.0", "reference"),  # crouched, the ears laid back
    ("Brown Hare portrait.jpg", "cazalegg (Caroline Legg)", "CC BY 4.0", "reference"),  # the head, the eye, the ears
    ("European Brown Hare Portrait.jpg", "Caroline Legg", "CC BY 2.0", "reference"),  # the eye close
    ("European Brown Hare 1.jpg", "Caroline Legg", "CC BY 4.0", "reference"),  # the alert sit from the front
    ("Biegnące zające szaraki 20140427 2184.jpg", "Jakub Hałun", "CC BY-SA 3.0", "reference"),  # the gallop, three phases
    ("Iepure de câmp alergând.jpg", "NGC 54", "CC0", "reference"),  # the gallop from behind
    ("Lepus europaeus (running).jpg", "Hans Hillewaert", "CC BY-SA 4.0", "reference"),  # the gallop, the tail held down
    ("European hare running.jpg", "Tiia Monto", "CC BY-SA 4.0", "reference"),  # the slow hop and the gallop
]


# ---------------------------------------------------------------------------
# Small maths, plain tuples so the coat works without Blender.


def _clip(t, lo, hi):
    """``t`` clamped to [lo, hi]: a float, or a numpy array (the sculpt's
    lofts evaluate the warp over the grid)."""
    if hasattr(t, "shape"):
        import numpy as np

        return np.clip(t, lo, hi)
    return max(lo, min(hi, t))


def smooth(t):
    t = _clip(t, 0.0, 1.0)
    return t * t * (3 - 2 * t)


def _mix(a, b, f):
    return tuple(a[i] + (b[i] - a[i]) * f for i in range(3))


def _dist(a, b):
    return math.sqrt(sum((a[i] - b[i]) ** 2 for i in range(3)))


def _norm(v):
    n = math.sqrt(sum(c * c for c in v)) or 1.0
    return tuple(c / n for c in v)


# The build: the anatomy below is laid out long, and the model is that
# layout made compact (``warp``, as the fox's): the trunk between the
# shoulders and the hips shortened (the hindquarters, the tail with them,
# come forward; the hind foot and its metatarsus move whole with the hock,
# so the foot keeps its length); the body (not the head) stretched
# ``BODY_KZ`` upward from the ground, so the trunk is deep and the legs
# long; the head grown ``HEAD_K`` about its joint and carried up and back
# onto the shoulders (the neck shortened). The proportions are fitted to
# the reference photographs' outlines (tools/README.md, ``--silhouettes``).
# The coat and the skin's regions read a model point back with ``unwarp``.
TRUNK_FROM, TRUNK_TO, TRUNK_SQUASH = -0.11, 0.09, 0.62
BODY_KZ = 1.2
HEAD_K, HEAD_C, HEAD_SHIFT = 1.32, (0.0, -0.200, 0.270), (0.0, 0.040, -0.020)


def _head_w(y, z):
    """How much of the head's growth applies at (y, z): all of it on the
    head, none on the trunk, blended over the neck; everything above the
    poll (the ears, however they lean) is head."""
    w = smooth((-0.150 - y) / 0.075) * smooth((z - 0.205) / 0.05)
    ears = smooth((z - 0.345) / 0.02)
    if hasattr(w, "shape"):
        import numpy as np

        return np.maximum(w, ears)
    return max(w, ears)


def _shift_w(y, z):
    """How much of the head's carriage back (``HEAD_SHIFT``'s y) applies:
    all of it from the back of the skull forward, none behind the withers
    or low on the chest, blended over 13 cm so the neck never folds."""
    return smooth((-0.08 - y) / 0.13) * smooth((z - 0.17) / 0.08)


def _squash(y):
    """How far the trunk's shortening moves a point at ``y`` along the body."""
    return (_clip(y, TRUNK_FROM, TRUNK_TO) - TRUNK_FROM) * (TRUNK_SQUASH - 1.0)


def warp(p):
    """A point of the layout (Blender metres) to the model; its coordinates
    may be numpy arrays (``warp_fn``)."""
    x, y, z = p
    # Below the hock the hind leg moves whole with it.
    foot = smooth((0.10 - z) / 0.04) * smooth((y + 0.02) / 0.04)
    y2 = y + _squash(y + (HOCK[1] - y) * foot)
    body = (x, y2, z * BODY_KZ)
    w = _head_w(y, z)
    if not hasattr(w, "shape") and w <= 0.0:
        return (x, y2 + HEAD_SHIFT[1] * _shift_w(y, z), body[2])
    c = HEAD_C
    lift = c[2] * (BODY_KZ - 1.0)  # the neck's root rises with the stretched body
    head = (c[0] + (x - c[0]) * HEAD_K + HEAD_SHIFT[0], c[1] + (y2 - c[1]) * HEAD_K,
            c[2] + (z - c[2]) * HEAD_K + HEAD_SHIFT[2] + lift)
    out = [body[i] + (head[i] - body[i]) * w for i in range(3)]
    out[1] += HEAD_SHIFT[1] * _shift_w(y, z)
    return tuple(out)


def unwarp(p):
    """A point of the model back to the layout (an iteration on the warp's
    local scale along each axis)."""
    q = tuple(p)
    for _ in range(40):
        got = warp(q)
        err = [p[i] - got[i] for i in range(3)]
        if max(abs(e) for e in err) < 1e-7:
            break
        w = _head_w(q[1], q[2])
        k = (1.0 + (HEAD_K - 1.0) * w, min(1.0, TRUNK_SQUASH) + (HEAD_K - min(1.0, TRUNK_SQUASH)) * w,
             BODY_KZ + (HEAD_K - BODY_KZ) * w)
        q = tuple(q[i] + err[i] / k[i] for i in range(3))
    return q


# ---------------------------------------------------------------------------
# The head's frame: from the poll (between the ears) down the face to the
# nose. ``_at(t, u, s)`` is ``t`` metres along it, ``u`` up from it (toward
# the forehead) and ``s`` to the left.

HEAD_O = (0.0, -0.225, 0.330)
HEAD_A = _norm((0.0, -0.990, -0.140))  # 8 degrees below level: the long face carried forward
HEAD_U = (0.0, HEAD_A[2], -HEAD_A[1])  # perpendicular, forward and up
NOSE_T = 0.090  # the tip of the nose, along the head


def _at(t, u=0.0, s=0.0):
    return (s, HEAD_O[1] + HEAD_A[1] * t + HEAD_U[1] * u, HEAD_O[2] + HEAD_A[2] * t + HEAD_U[2] * u)


def head_coords(p):
    """(t, u, s) of ``p`` (Blender metres) in the head's frame."""
    r = (p[0] - HEAD_O[0], p[1] - HEAD_O[1], p[2] - HEAD_O[2])
    return (r[1] * HEAD_A[1] + r[2] * HEAD_A[2], r[1] * HEAD_U[1] + r[2] * HEAD_U[2], r[0])


EYE = (0.043, -0.006, 0.0250)  # head frame (t, u, |s|) of the eyeball's centre: high, mid-way along the head
EYE_RADIUS = 0.0098  # about 2 cm across, the lids' opening as wide (the photographs)
EYE_RING = 0.0128  # the pale ring's outer edge from the eye's centre: a narrow cream band on the lids
# The ears: base, middle and tip in the head frame (t, u, |s|); up and 15
# degrees back standing, 0.14 m from the skull: the literature's notch-to-tip
# length, and longer than the head (the photographs); the head is sized to them.
EAR_BASE, EAR_MID, EAR_TIP = (0.004, 0.012, 0.020), (-0.0228, 0.0647, 0.0336), (-0.0425, 0.1030, 0.0510)


EAR_STAGGER = 12.0  # degrees: at rest the left ear leans this much forward about its base and the right as much back, so from the side both show


def _ear(point, sx):
    t, u, s = point
    a = math.radians(sx * EAR_STAGGER)
    dt, du = t - EAR_BASE[0], u - EAR_BASE[1]
    return _at(EAR_BASE[0] + dt * math.cos(a) + du * math.sin(a), EAR_BASE[1] - dt * math.sin(a) + du * math.cos(a), sx * s)


# The ear's section from its base to its tip (``ears``): (half thickness,
# half width) in metres at the base, two fifths and nine tenths of the way
# to its middle, then halfway to the tip and at the tip. The base is a
# rolled tube (round: as thick as it is wide), opening into a thin blade.
EAR_SECTION = [(0.0080, 0.0085), (0.0055, 0.0140), (0.0034, 0.0172), (0.0031, 0.0158), (0.0014, 0.0068)]


def ear_front(sx):
    """The way an ear's hollow faces at rest: out and forward, so from the
    side both ears show their broad faces."""
    return _norm((sx * 0.70, -0.71, 0.0))


# ---------------------------------------------------------------------------
# The skeleton: the Quaternius quadruped's names (as the fox's), on a hare
# standing. Name, parent, head (Blender metres, facing -Y). The fore leg:
# ``FrontShoulder`` the shoulder blade and the upper arm, ``FrontUpperLeg``
# the forearm (elbow to wrist), ``FrontLowerLeg`` the fore foot (wrist to
# the ball of the toes); the hind: ``BackLeg`` the femur, ``BackUpperLeg``
# the long tibia, ``BackLowerLeg`` the metatarsus (hock to the ball); the
# feet hang from the IK bones. The fore legs hang from ``Torso3`` (the
# chest), so they follow the spine's flexion at the gallop.

FORE_X, HIND_X = 0.034, 0.048
# The tail lies down the round rump (the hare holds it down at rest and in
# flight), its end ``TAIL_END``.
TAIL = [(0.0, 0.234, 0.152), (0.0, 0.252, 0.140), (0.0, 0.264, 0.121), (0.0, 0.269, 0.100)]
TAIL_END = (0.0, 0.268, 0.080)
HIP = (HIND_X, 0.140, 0.195)
STIFLE = (0.052, 0.066, 0.116)
HOCK = (HIND_X, 0.180, 0.058)
BALL_H = (HIND_X, 0.112, 0.013)
TOE_H = (HIND_X, 0.054, 0.0)
HEEL = (HIND_X, 0.199, 0.052)  # the point of the heel (calcaneus), behind the hock
ELBOW = (FORE_X, -0.140, 0.118)
WRIST = (FORE_X, -0.146, 0.034)
BALL_F = (FORE_X, -0.153, 0.010)
TOE_F = (FORE_X, -0.181, 0.0)

BONES = [
    ("Body", None, (0.0, 0.0, 0.08)),
    ("Back", "Body", (0.0, 0.130, 0.215)),
    ("Torso", "Back", (0.0, 0.060, 0.228)),
    ("Torso2", "Torso", (0.0, -0.020, 0.218)),
    ("Torso3", "Torso2", (0.0, -0.095, 0.205)),
    ("Neck1", "Torso3", (0.0, -0.150, 0.215)),
    ("Neck2", "Neck1", (0.0, -0.178, 0.238)),
    ("Neck3", "Neck2", (0.0, -0.198, 0.262)),
    ("Head", "Neck3", (0.0, -0.212, 0.290)),
]
for _s, _sx in (("L", 1.0), ("R", -1.0)):
    BONES += [
        (f"FrontShoulder.{_s}", "Torso3", (_sx * 0.025, -0.120, 0.215)),
        (f"FrontUpperLeg.{_s}", f"FrontShoulder.{_s}", (_sx * ELBOW[0], ELBOW[1], ELBOW[2])),
        (f"FrontLowerLeg.{_s}", f"FrontUpperLeg.{_s}", (_sx * WRIST[0], WRIST[1], WRIST[2])),
    ]
for _s, _sx in (("L", 1.0), ("R", -1.0)):
    BONES += [
        (f"BackShoulder.{_s}", "Back", (_sx * 0.028, 0.140, 0.215)),
        (f"BackLeg.{_s}", f"BackShoulder.{_s}", (_sx * HIP[0], HIP[1], HIP[2])),
        (f"BackUpperLeg.{_s}", f"BackLeg.{_s}", (_sx * STIFLE[0], STIFLE[1], STIFLE[2])),
        (f"BackLowerLeg.{_s}", f"BackUpperLeg.{_s}", (_sx * HOCK[0], HOCK[1], HOCK[2])),
    ]
BONES += [(f"Tail{k + 1}", "Back" if k == 0 else f"Tail{k}", TAIL[k]) for k in range(3)]
for _s, _sx in (("L", 1.0), ("R", -1.0)):
    BONES += [
        (f"PoleTargetBack.{_s}", "Body", (_sx * HIND_X, -0.36, 0.08)),
        (f"PoleTarget.{_s}", "Body", (_sx * FORE_X, -0.34, 0.08)),
        (f"IKBackLeg.{_s}", None, (_sx * BALL_H[0], BALL_H[1], BALL_H[2])),
        (f"FFB.{_s}", f"IKBackLeg.{_s}", (_sx * HIND_X, 0.090, 0.007)),
        (f"IKFrontLeg.{_s}", None, (_sx * BALL_F[0], BALL_F[1], BALL_F[2])),
        (f"FF.{_s}", f"IKFrontLeg.{_s}", (_sx * FORE_X, -0.167, 0.005)),
    ]

ENDS = []
for _s, _sx in (("L", 1.0), ("R", -1.0)):
    ENDS += [
        (f"FrontLowerLeg.{_s}_end", f"FrontLowerLeg.{_s}", (_sx * BALL_F[0], BALL_F[1], BALL_F[2])),
        (f"BackLowerLeg.{_s}_end", f"BackLowerLeg.{_s}", (_sx * BALL_H[0], BALL_H[1], BALL_H[2])),
        (f"PoleTargetBack.{_s}_end", f"PoleTargetBack.{_s}", (_sx * HIND_X, -0.36, 0.12)),
        (f"PoleTarget.{_s}_end", f"PoleTarget.{_s}", (_sx * FORE_X, -0.34, 0.12)),
        (f"FFB.{_s}_end", f"FFB.{_s}", (_sx * TOE_H[0], TOE_H[1], TOE_H[2])),
        (f"FF.{_s}_end", f"FF.{_s}", (_sx * TOE_F[0], TOE_F[1], TOE_F[2])),
    ]
ENDS.append(("Tail3_end", "Tail3", TAIL_END))
TAILS = {"Head": _at(NOSE_T - 0.004, -0.036), "Body": (0.0, 0.0, 0.2)}

EXTRA = []
for _s, _sx in (("L", 1.0), ("R", -1.0)):
    EXTRA += [(f"Ear1.{_s}", "Head", _ear(EAR_BASE, _sx), _ear(EAR_MID, _sx)),
              (f"Ear2.{_s}", f"Ear1.{_s}", _ear(EAR_MID, _sx), _ear(EAR_TIP, _sx))]
EXTRA.append(("Jaw", "Head", _at(0.030, -0.048), _at(NOSE_T - 0.014, -0.046)))

TAIL_CHAIN = ["Tail1", "Tail2", "Tail3"]


def joint_names():
    return [b[0] for b in BONES] + [e[0] for e in EXTRA]


def spine_length():
    """Head and body: from the nose's tip along the spine (the poll, the
    neck's and the trunk's joints) to the root of the tail, metres."""
    heads = {name: h for name, _, h in BONES}
    path = [_at(NOSE_T, -0.030), HEAD_O] + [heads[n] for n in ("Neck2", "Neck1", "Torso3", "Torso2", "Torso", "Back")] + [TAIL[0]]
    path = [warp(p) for p in path]
    return sum(_dist(a, b) for a, b in zip(path, path[1:]))


def ear_length():
    """The ear from its base on the skull to the tip, metres."""
    b, m, t = (warp(_ear(p, 1)) for p in (EAR_BASE, EAR_MID, EAR_TIP))
    return _dist(b, m) + _dist(m, t)


EAR_NOTCH = 0.4  # the notch, where the rolled base opens into the blade (``ears``' ``low``): this far from the base to the middle


def ear_notch_length(sx=1.0):
    """The ear (the left, or the right with ``sx`` -1) from the notch to
    the tip, as the literature measures it."""
    b, m, t = (warp(_ear(p, sx)) for p in (EAR_BASE, EAR_MID, EAR_TIP))
    notch = _mix(b, m, EAR_NOTCH)
    return _dist(notch, m) + _dist(m, t)


def hind_foot_length():
    return _dist(warp(HEEL), warp(TOE_H))


def tail_length():
    pts = [warp(p) for p in TAIL + [TAIL_END]]
    return sum(_dist(a, b) for a, b in zip(pts, pts[1:])) + 0.012


# The side view's proportions, measured on the reference photographs'
# outlines (Jakub Hałun, standing and crouched from the side; traced by
# hand, see tools/README.md): low and high bounds.
PHOTO_RATIOS = dict(
    trunk_depth=(0.44, 0.54),  # the trunk's depth (back to belly) over the length from the nose to the rump: 0.49
    head_length=(0.23, 0.30),  # nose to the ear's base over the same length: 0.26-0.28
    head_depth=(0.50, 0.65),  # the head's depth at the eye over its length (nose to the ear's base): 0.55-0.6
    ear=(1.10, 1.50),  # the ear (from its base) over the head's length: 1.25-1.5 on the photographs; 1.15 with the literature's notch-to-tip length
    haunch_depth=(0.44, 0.56),  # the haunch, from the croup down to the thigh's and the hamstrings' lower edge, at its deepest over the rear quarter: 0.49
)


def _turn(deg, about, p):
    """``p`` (y, z) turned ``deg`` degrees nose down about ``about``."""
    c, s = math.cos(math.radians(deg)), math.sin(math.radians(deg))
    y, z = p[0] - about[0], p[1] - about[1]
    return (about[0] + y * c - z * s, about[1] + y * s + z * c)


def alert_front():
    """The alert sit (``idle`` at rest) seen square from the side, in pure
    Python: the body pitched and lifted (``SIT``), the spine rounded and the
    pelvis tucked (``SIT_ALERT``), the neck bent, the shoulders slid
    (``SIT_FORE_SLIDE``; the upper arms' cage with them, the sling left
    out). Returns how far the chest's front (the trunk's and the upper arms'
    cages below the eye) and the eye and the nose stand ahead of the
    vertical through the ears' bases, in eye heights above the ground (the
    photograph, Hałun 5163, about 30 degrees behind square: 0.08, 0.14 and
    0.24, so about 0.09, 0.16 and 0.28 square)."""
    heads = {name: tuple(warp(h)[1:]) for name, _, h in BONES}
    a = SIT_ALERT
    pivot = tuple(SIT["pivot"][1:])

    def front(p):
        p = _turn(a["flex"] * 0.25, heads["Torso3"], p)
        p = _turn(a["flex"] * 0.40, heads["Torso2"], p)
        p = _turn(a["flex"] * 0.35 + a["tuck"], heads["Torso"], p)
        p = _turn(-a["tuck"], heads["Back"], p)
        p = _turn(SIT["nose_down"], pivot, p)
        return (p[0], p[1] + SIT["lift"])

    pitch = SIT["nose_down"] + a["flex"]
    chain = [heads[n] for n in ("Neck1", "Neck2", "Neck3", "Head")]
    at, bend = front(chain[0]), 0.0
    for k in range(3):
        bend += a["neck"][k]
        d = _turn(pitch - bend, (0.0, 0.0), _sub((0.0,) + chain[k + 1], (0.0,) + chain[k])[1:])
        at = (at[0] + d[0], at[1] + d[1])
    bend += a["neck"][3]

    def head(p):
        d = _turn(pitch - bend, (0.0, 0.0), (p[1] - chain[3][0], p[2] - chain[3][1]))
        return (at[0] + d[0], at[1] + d[1])

    ear, eye, nose = (head(warp(p)) for p in (_ear(EAR_BASE, 1), _at(*EYE), _at(NOSE_T, -0.030)))
    slid = [(p[1] - SIT_FORE_SLIDE, p[2]) for ring in leg_cage(FORE_RINGS)[1:4] for p in ring]
    # the chest's skin between the fore legs is weighted about half to the
    # upper arms (``skin``): it follows half their slide
    skin = [(p[1] - 0.5 * SIT_FORE_SLIDE, p[2]) for ring in trunk_cage()[2:9] for p in ring]
    chest = [front(p) for p in slid + skin]
    chest = min(p[0] for p in chest if p[1] < eye[1] - 0.06)
    return {name: (ear[0] - y) / eye[1] for name, y in (("chest", chest), ("eye", eye[0]), ("nose", nose[0]))}


def proportions():
    """The model's side view at rest, as ``PHOTO_RATIOS`` measures it."""
    nose = warp(_at(NOSE_T, -0.030))
    base = warp(_ear(EAR_BASE, 1))
    length = SIDE_RUMP[1] - nose[1]
    head = math.dist(nose[1:], base[1:])
    eye = min(HEAD_RINGS, key=lambda r: abs(r[0] - EYE[0]))
    haunch = [p for r in leg_cage(SEAT_RINGS) + leg_cage(HIND_RINGS[:5]) + trunk_cage() for p in r]
    deepest = max(max(p[2] for p in haunch if abs(p[1] - y) < 0.012) - min(p[2] for p in haunch if abs(p[1] - y) < 0.012)
                  for y in (0.07, 0.09, 0.11, 0.13, 0.15))
    return dict(trunk_depth=max(top[1] - bot[1] for top, bot, *_ in TRUNK_RINGS) / length, head_length=head / length,
                head_depth=(eye[1] - eye[2]) * HEAD_K / head, ear=ear_length() / head, haunch_depth=deepest / length)


# ---------------------------------------------------------------------------
# The coat, painted on the sculpt (``coat_at``) and baked into the colour
# map. sRGB 0..255. ``grizzle`` is how much of the agouti speckle shows
# (guard hairs with a pale band and a black tip).

# The coat's mean albedo over the trunk (linear luminance): the standing
# Hałun photograph's trunk measures 0.117 with its white fur at 0.75 (so
# about albedo), under a soft light that shades a round body to about two
# thirds on average: de-lit, 0.18. The game's sun then shows a sandy grey
# brown hare, as light as the fox (``GAME_LIFT`` 1.0).
COAT_ALBEDO = 0.18
COAT = dict(
    back=(120, 100, 77),  # along the back, under the speckle
    flank=(176, 149, 116),  # the flanks: greyer agouti, a little warmer than the back
    rufous=(222, 176, 126),  # the nape, the shoulders, the chest, the sides of the neck: tawny, not orange (the photographs)
    grey=(166, 153, 136),  # the rump and the thighs
    buff=(176, 140, 100),  # the outside of the legs
    foot=(178, 148, 108),  # the tops of the feet
    sole=(150, 118, 82),  # the furred soles
    white=(236, 232, 222),  # the belly, the inside of the legs, the chin, under the tail
    cream=(222, 206, 172),  # the eye's ring, the streak under the eye, the muzzle
    cheek=(128, 114, 98),  # the grizzled cheeks
    black=(30, 26, 24),  # the ears' tips, the tail's top
    nose=(72, 56, 50),  # the bare skin round the nostrils, the lips, the lid rims
    claw=(70, 60, 52),
)


def tail_place(q):
    """How far along the tail ``q`` is (0 at its root, 1 at its tip) and
    how far from its axis, with the axis' direction there; None off it."""
    if q[1] < TAIL[0][1] - 0.012 or abs(q[0]) > 0.04 or q[2] < 0.05:
        return None
    pts = TAIL + [TAIL_END]
    best = None
    run, total = 0.0, sum(_dist(a, b) for a, b in zip(pts, pts[1:]))
    for a, b in zip(pts, pts[1:]):
        ab = tuple(b[i] - a[i] for i in range(3))
        ll = sum(c * c for c in ab)
        f = max(0.0, min(1.0, sum((q[i] - a[i]) * ab[i] for i in range(3)) / ll))
        c = tuple(a[i] + ab[i] * f for i in range(3))
        d = _dist(q, c)
        if best is None or d < best[0]:
            best = (d, (run + math.sqrt(ll) * f) / total, ab)
        run += math.sqrt(ll)
    d, f, ab = best
    if d > 0.026 or (f <= 0.0 and q[1] < TAIL[0][1] - 0.002):
        return None
    return f, d, ab


def is_ear(q):
    """On an ear: above the skull, behind the eyes."""
    t, u, s = head_coords(q)
    return u > 0.016 and t < 0.03 and abs(s) > 0.004 and q[2] > 0.30


def ear_place(q):
    """(f, front): how far up the ear ``q`` is (0 at its base, 1 at the
    tip) and the unit vector its hollow faces."""
    t, u, s = head_coords(q)
    sx = 1.0 if s >= 0 else -1.0
    a = math.radians(sx * EAR_STAGGER)  # undo the ear's lean (``_ear``)
    u = EAR_BASE[1] + (t - EAR_BASE[0]) * math.sin(a) + (u - EAR_BASE[1]) * math.cos(a)
    f = max(0.0, min(1.0, (u - EAR_BASE[1]) / (EAR_TIP[1] - EAR_BASE[1])))
    return f, ear_front(sx)


def _wobble(q, k):
    """A cheap smooth noise in about [-1, 1] at ``q`` (``k`` per metre): the
    ragged edge of a white patch."""
    x, y, z = (c * k for c in q)
    return (math.sin(1.7 * x + 2.3 * y + 0.4) * math.sin(1.3 * y - 2.1 * z + 1.1) + 0.6 * math.sin(2.9 * z + 1.9 * x - 0.7)) / 1.6


def coat_at(q, m):
    """The coat at the model point ``q`` facing ``m`` (Blender metres, the
    rest pose): its colour (sRGB 0..255) and how much the agouti grizzle
    shows."""
    c = COAT
    tail = tail_place(q)
    if tail is not None:
        f, _, ab = tail
        ln = math.sqrt(sum(v * v for v in ab))
        a = tuple(v / ln for v in ab)
        up = (0.0, -a[2], a[1]) if a[1] > 0 else (0.0, a[2], -a[1])
        top = sum(m[i] * up[i] for i in range(3)) + 0.06 * _wobble(q, 400.0)
        black = smooth((top - 0.05) / 0.30) * smooth((f - 0.03) / 0.10)
        return _mix(c["white"], c["black"], black), 0.0
    if is_ear(q):
        return _ear_coat(q, m)
    rgb, g = _body_coat(q, m)
    head = smooth((-0.195 - q[1]) / 0.045) * smooth((q[2] - 0.235) / 0.025)
    if head > 0.0:
        h_rgb, h_g = _head_coat(q, m)
        rgb, g = _mix(rgb, h_rgb, head), g + (h_g - g) * head
    return flecked(rgb, g, q, 1.0 + 0.8 * head)


FLECK_DARK, FLECK_PALE = (52, 42, 32), (206, 170, 116)


def _hash(i, j, k, seed):
    h = (i * 73856093) ^ (j * 19349663) ^ (k * 83492791) ^ (seed * 2654435761)
    h = (h ^ (h >> 13)) * 1274126177 & 0xFFFFFFFF
    return ((h ^ (h >> 16)) & 0xFFFF) / 32767.5 - 1.0


def _vnoise(p, seed=0):
    """Value noise in [-1, 1] at ``p`` (lattice units): random values at
    the integer points, smoothly blended between them."""
    x, y, z = p
    i, j, k = math.floor(x), math.floor(y), math.floor(z)
    fx, fy, fz = smooth(x - i), smooth(y - j), smooth(z - k)
    out = 0.0
    for di, wx in ((0, 1 - fx), (1, fx)):
        for dj, wy in ((0, 1 - fy), (1, fy)):
            for dk, wz in ((0, 1 - fz), (1, fz)):
                out += wx * wy * wz * _hash(i + di, j + dj, k + dk, seed)
    return out


def fleck(q, fine=1.0):
    """The coat's flecks, about [-1, 1]: where clumps of black-tipped guard
    hairs lie together (positive) and where their ochre bands show
    (negative), a few millimetres across, longer along the lie of the hair
    (back along the body) than across it, irregular."""
    x, y, z = (c * fine for c in q)
    a = _vnoise((x * 760.0, y * 150.0, z * 760.0), 1) + 0.55 * _vnoise((x * 1700.0, y * 330.0, z * 1700.0), 2) \
        + 0.3 * _vnoise((x * 3400.0, y * 700.0, z * 3400.0), 3)
    return 1.6 * a / 1.85


def flecked(rgb, g, q, fine=1.0):
    """The coat with its flecks, as strong as its grizzle: the dark clumps
    and the pale bands of the agouti guard hairs."""
    if g <= 0.0:
        return rgb, g
    f = fleck(q, fine)
    rgb = _mix(rgb, FLECK_DARK, 0.86 * g * smooth((f - 0.02) / 0.30))
    rgb = _mix(rgb, FLECK_PALE, 0.30 * g * smooth((-f - 0.15) / 0.30))
    return rgb, 0.7 * g


def _body_coat(q, m):
    x, y, z = q
    c = COAT
    up = m[2]
    # The flanks, darker toward the back's top; the back's black-tipped
    # guard hairs speckled densest along the spine.
    rgb = _mix(c["flank"], c["back"], smooth((up - 0.15) / 0.55))
    g = 0.85 + 0.15 * smooth((up - 0.2) / 0.5)
    # The thigh's front edge: the fur parts over it, a darker line that
    # draws the thigh against the flank (the photographs).
    tx, ty, tz = abs(x) - 0.050, y - 0.122, z - 0.168
    a = math.radians(28.0)
    ry, rz = ty * math.cos(a) + tz * math.sin(a), -ty * math.sin(a) + tz * math.cos(a)
    r = math.sqrt((tx / 0.055) ** 2 + (ry / 0.091) ** 2 + (rz / 0.089) ** 2)
    seam = smooth((0.10 - abs(r - 1.0)) / 0.08) * smooth((-ty + 0.01) / 0.03) * smooth((z - 0.10) / 0.04)
    rgb = _mix(rgb, c["back"], 0.55 * seam)
    # The rump and the thighs greyer.
    rgb = _mix(rgb, c["grey"], 0.75 * smooth((y - 0.07) / 0.05) * smooth((z - 0.06) / 0.04))
    # The nape, the shoulders, the chest and the sides of the neck rufous,
    # little grizzled; the chest's front the warmest.
    fore = smooth((-0.06 - y - 0.35 * (z - 0.20) + 0.015 * _wobble(q, 200.0)) / 0.10)  # slanting back down the shoulder
    rgb = _mix(rgb, c["rufous"], 0.85 * fore * smooth((0.75 - up) / 0.6))
    rgb = _mix(rgb, _mix(c["rufous"], c["back"], 0.35), 0.6 * fore * smooth((up - 0.6) / 0.3))
    g *= 1.0 - 0.45 * fore
    # The legs: buff outside, white inside, the feet paler, the soles
    # browner.
    sx = 1.0 if x >= 0 else -1.0
    inner = smooth((-m[0] * sx - 0.15) / 0.4)
    if y < -0.09 and z < 0.13:  # fore
        leg = smooth((0.13 - z) / 0.03)
        front = smooth((-m[1] - 0.3) / 0.5)
        rgb = _mix(rgb, _mix(_mix(c["buff"], c["foot"], 0.35 * front), c["white"], 0.8 * inner), leg)
        g *= 1.0 - 0.6 * leg
    if y > 0.03 and z < 0.13:  # hind, below the stifle
        leg = smooth((0.12 - z) / 0.04) * smooth((y - 0.05) / 0.03)
        rgb = _mix(rgb, _mix(c["buff"], c["white"], 0.85 * inner), leg)
        g *= 1.0 - 0.6 * leg
    if z < 0.03:  # the feet
        foot = smooth((0.03 - z) / 0.015)
        rgb = _mix(rgb, c["foot"], 0.8 * foot)
        rgb = _mix(rgb, c["sole"], smooth((-up - 0.5) / 0.3) * smooth((0.006 - z) / 0.004))
    # The belly white, its edge ragged; the inside of the thighs white.
    under = smooth((-up - 0.45 + 0.15 * _wobble(q, 160.0)) / 0.3) * smooth((0.15 - z) / 0.03) * smooth((y + 0.13) / 0.04)
    thigh_in = smooth((-m[0] * sx - 0.3) / 0.3) * smooth((y - 0.03) / 0.03) * smooth((0.17 - z) / 0.04)
    white = max(under, thigh_in)
    rgb = _mix(rgb, c["white"], white)
    g *= 1.0 - white
    return rgb, g


def _head_coat(q, m):
    c = COAT
    t, u, s = head_coords(q)
    rgb, g = _mix(c["back"], c["flank"], 0.5), 0.5  # the crown and the forehead, finely grizzled
    # The cheeks grizzled grey brown.
    rgb = _mix(rgb, c["cheek"], smooth((-0.018 - u) / 0.01) * smooth((0.085 - t) / 0.02))
    # The nape rufous, behind the ears.
    rgb = _mix(rgb, c["rufous"], smooth((-0.005 - t) / 0.02))
    # The muzzle: its top ochre grey, its sides and the upper lips whitish;
    # the chin and under the jaw white.
    muzzle = smooth((t - NOSE_T + 0.040) / 0.012)
    rgb = _mix(rgb, c["cream"], 0.55 * muzzle * smooth((-0.030 - u) / 0.006))
    chin = smooth((t - 0.040) / 0.02) * smooth((-0.044 - u) / 0.004)
    rgb = _mix(rgb, c["white"], chin)
    # The narrow cream ring round the eye, on the lids, and the pale
    # streak from the muzzle under the eye back toward the ear.
    e = _dist(q, _at(EYE[0], EYE[1], math.copysign(EYE[2], s)))
    ring = smooth((EYE_RING + 0.0004 * _wobble(q, 600.0) - e) / 0.0010) * smooth((e - EYE_RADIUS + 0.002) / 0.002)
    streak_u = EYE[1] - 0.017 + 0.18 * (EYE[0] - t)
    streak = smooth((0.005 - abs(u - streak_u)) / 0.004) * smooth((t + 0.005) / 0.02) * smooth((NOSE_T - 0.025 - t) / 0.02)
    rgb = _mix(rgb, c["cream"], max(0.8 * ring, 0.6 * streak))
    pale = max(muzzle * smooth((-0.026 - u) / 0.006), chin, ring, 0.75 * streak)
    # The neck behind the head: into the body's colours.
    return rgb, g * (1.0 - pale)


EAR_BACK = ((146, 124, 98), (138, 126, 112))  # the back of the ear, agouti brown grey: warmer at the base, greyer up
EAR_HOLLOW = (84, 73, 64)  # the hollow: grey-brown skin under sparse hair
EAR_FRINGE = (222, 212, 192)  # the pale hair fringing the front edge, on both sides
EAR_HOLLOW_HAIR = (176, 164, 150)  # its sparse pale hairs


def ear_across(q):
    """Where ``q`` lies across its ear: -1 on the inner (front) edge, +1
    on the outer edge, 0 on the ear's middle line."""
    t, u, s = head_coords(q)
    sx = 1.0 if s >= 0 else -1.0
    pts = [_ear(p, sx) for p in (EAR_BASE, EAR_MID, EAR_TIP)]
    best = None
    for k, (a, b) in enumerate(zip(pts, pts[1:])):
        ab = tuple(b[i] - a[i] for i in range(3))
        ll = sum(c * c for c in ab)
        h = max(0.0, min(1.0, sum((q[i] - a[i]) * ab[i] for i in range(3)) / ll))
        c = tuple(a[i] + ab[i] * h for i in range(3))
        d = _dist(q, c)
        if best is None or d < best[0]:
            best = (d, c, ab, 0.5 * (k + h))
    _, c, ab, f = best
    front = ear_front(sx)
    side = _norm((front[1] * ab[2] - front[2] * ab[1], front[2] * ab[0] - front[0] * ab[2], front[0] * ab[1] - front[1] * ab[0]))
    if side[0] * sx < 0:
        side = tuple(-v for v in side)
    # The blade's half width along the ear (``EAR_SECTION`` at 0, 0.2, 0.45, 0.75 and 1 of its length).
    stops = (0.0, 0.2, 0.45, 0.75, 1.0)
    for i in range(4):
        if f <= stops[i + 1]:
            w = EAR_SECTION[i][1] + (EAR_SECTION[i + 1][1] - EAR_SECTION[i][1]) * (f - stops[i]) / (stops[i + 1] - stops[i])
            break
    return max(-1.0, min(1.0, sum((q[i] - c[i]) * side[i] for i in range(3)) / max(w, 1e-4)))


def _ear_coat(q, m):
    c = COAT
    f, front = ear_place(q)
    inside = smooth((sum(m[i] * front[i] for i in range(3)) - 0.1) / 0.4)
    back = _mix(*EAR_BACK, smooth((f - 0.2) / 0.5))
    # The hollow dark, its sparse pale hairs in streaks up the ear (thicker
    # toward the inner edge, where they make the pale fringe).
    across = ear_across(q)
    streak = smooth((fleck((q[0] * 2.0, q[1] * 0.4, q[2] * 0.4), 1.0) - 0.15) / 0.3)
    hollow = _mix(EAR_HOLLOW, EAR_HOLLOW_HAIR, 0.45 * streak * (1.0 - 0.5 * f))
    rgb = _mix(back, hollow, inside)
    # The front edge fringed with pale hair, a band about a fifth of the
    # ear's width, on both sides.
    fringe = smooth((-across - 0.58 + 0.06 * _wobble(q, 700.0)) / 0.14) * smooth((f - 0.12) / 0.1)
    rgb = _mix(rgb, EAR_FRINGE, 0.85 * fringe)
    g = 0.6 * (1.0 - inside) * (1.0 - fringe)
    # Black only at the tip, on both sides.
    dark = smooth((f - 0.84 + 0.02 * _wobble(q, 500.0)) / 0.05)
    rgb = _mix(rgb, c["black"], dark)
    return flecked(rgb, g * (1.0 - dark), q, 1.6)


# The hair cards' atlas: eight columns (``horse.py``'s ``HAIR_COLUMNS``), the
# fox's order, a hare's colours.
GRIZZLED, GRIZZLED_2, CREAM, RUFOUS, DORSAL, BLACK, WISP_WHITE, WISP_BLACK = range(8)


CARD_SHADE = 0.66  # the cards are unshaded: as dark as the baked coat (its fur's shading and occlusion) under them


def card_colour(column, f, shift=0.0):
    """The colour (sRGB 0..255) of one hair of the atlas ``column`` at ``f``
    along it (0 at the root): an agouti guard hair is slate grey at the
    root, then an ochre band, then a black tip; ``shift`` moves the band.
    Shaded by ``CARD_SHADE``."""
    return tuple(CARD_SHADE * c for c in _card_colour(column, f, shift))


def _card_colour(column, f, shift):
    if column == GRIZZLED:
        band = 0.50 + shift
        rgb = _mix((104, 98, 92), (204, 170, 112), smooth((f - band) / 0.05))
        return _mix(rgb, (34, 28, 24), 0.9 * smooth((f - band - 0.22) / 0.05))
    if column == GRIZZLED_2:
        rgb = _mix((112, 102, 92), (182, 140, 90), smooth((f - 0.42 - shift) / 0.15))
        return _mix(rgb, (70, 56, 44), 0.6 * smooth((f - 0.82 - shift) / 0.08))
    if column == DORSAL:
        band = 0.46 + shift
        rgb = _mix((84, 78, 72), (176, 146, 102), smooth((f - band) / 0.04))
        return _mix(rgb, (26, 22, 20), 0.92 * smooth((f - band - 0.14) / 0.05))
    if column == CREAM:
        return _mix((214, 208, 198), (240, 236, 228), smooth(f / 0.4))
    if column == RUFOUS:
        return _mix((170, 124, 78), (206, 160, 106), smooth(f / 0.5))
    if column == BLACK:
        return (30, 26, 24)
    if column == WISP_WHITE:
        return (234, 230, 222)
    return (26, 22, 20)


# ---------------------------------------------------------------------------
# Real hair projected from a photograph (``--photos``; the felids' method,
# tools/felid.py): the standing hare from the side (Jakub Hałun, CC BY 4.0,
# ``PHOTOS``), seen from the hare's left and mirrored onto its right. Each
# region maps the model's side view (y, z) onto the photo's pixels by an
# affine fit through its landmarks plus a Gaussian correction that puts
# every landmark exactly on its pixel (``fit_mapping``). The head takes
# the photo's colour, de-lit (over its wide blurred luminance) and matched
# to the baked coat's mean, channel by channel; the body only its hair
# detail (the log-luminance over its local mean), laid over the painted
# coat. Both fade out over the region's outline in the photo (feathered)
# and where the skin turns from the camera (``photo_weight``: nothing past
# 70 degrees, so nothing smears round the sides).

PHOTO_PX = 1920  # the width of the photograph the pixel coordinates are in (Commons' 1920 px thumbnail)
PROJECTIONS = [
    dict(file=PHOTOS[0][0], region="head", mode="colour", strength=0.85, feather=40.0,
         landmarks=[  # model (layout, side view y, z) <-> photo (u, v) px
             (_at(EYE[0], EYE[1])[1:], (583, 732)),
             (_at(NOSE_T, -0.030)[1:], (365, 870)),
             (_ear(EAR_BASE, 1)[1:], (670, 654)),
             (_at(NOSE_T - 0.016, -0.046)[1:], (441, 934)),
             (_at(0.020, -0.049)[1:], (594, 985)),
             (_at(0.040, 0.010)[1:], (530, 666)),
         ],
         outline=[(371, 870), (403, 743), (505, 654), (632, 641), (734, 692), (759, 781), (734, 870), (670, 946),
                  (543, 984), (428, 946), (377, 908)]),
]


def photo_weight(facing):
    """How much a photo taken square to the side may give a point of skin
    whose normal is ``facing`` (its cosine with the view): nothing past 70
    degrees, all of it from 55 degrees in."""
    return smooth((facing - math.cos(math.radians(70.0))) / (math.cos(math.radians(55.0)) - math.cos(math.radians(70.0))))


def fit_mapping(src, dst, sigma=0.03):
    """A smooth map from the model's side view (metres) to the photo's
    pixels: the affine fit through ``src`` -> ``dst`` plus a Gaussian
    correction (``sigma`` metres) that puts every landmark exactly on its
    pixel. Returns a function of an (n, 2) array."""
    import numpy as np

    a = np.asarray(src, np.float64)
    b = np.asarray(dst, np.float64)
    m = np.hstack([a, np.ones((len(a), 1))])
    affine, *_ = np.linalg.lstsq(m, b, rcond=None)
    rest = b - m @ affine
    k = np.exp(-((a[:, None, :] - a[None, :, :]) ** 2).sum(-1) / (2 * sigma * sigma))
    coef = np.linalg.solve(k + 1e-9 * np.eye(len(a)), rest)

    def fn(p):
        p = np.asarray(p, np.float64)
        out = np.hstack([p, np.ones((len(p), 1))]) @ affine
        g = np.exp(-((p[:, None, :] - a[None, :, :]) ** 2).sum(-1) / (2 * sigma * sigma))
        return out + g @ coef

    return fn


def outline_distance(uv, poly):
    """Signed distance in pixels from each point of ``uv`` (n, 2) to the
    polygon ``poly``: positive inside."""
    import numpy as np

    uv = np.asarray(uv, np.float64)
    pts = np.asarray(poly, np.float64)
    inside = np.zeros(len(uv), bool)
    best = np.full(len(uv), np.inf)
    for i in range(len(pts)):
        a, b = pts[i], pts[(i + 1) % len(pts)]
        cond = (a[1] > uv[:, 1]) != (b[1] > uv[:, 1])
        x = a[0] + (uv[:, 1] - a[1]) * (b[0] - a[0]) / ((b[1] - a[1]) or 1e-12)
        inside ^= cond & (uv[:, 0] < x)
        ab = b - a
        t = np.clip(((uv - a) @ ab) / (ab @ ab), 0.0, 1.0)
        best = np.minimum(best, np.linalg.norm(uv - (a + t[:, None] * ab), axis=1))
    return np.where(inside, best, -best)


# ---------------------------------------------------------------------------
# The gaits, planned. ``seconds`` one stride; per leg the phase it lands
# (0..1 of the stride); per pair (fore, hind) the duty factor (the part of
# the stride the foot stands), where it lands (``land``, metres along the
# body, + back) and the swing's lift. A standing foot stays where it
# landed while the body goes over it, so every standing foot slides back
# at the same speed: ``sweep = speed x duty x seconds``.
#
# The slow hop (``HareState::Hop``, 1.2 m/s): the fores land one after the
# other and the hinds, swung up together, land beside them (the hare's
# half-bound), 0.5 m a stride (as the game's ``Hare::lift``), 2.4 a second.
#
# The flight (``HareState::Flee``, 11 m/s): the hare's gallop. The fores
# land one after the other, the body vaults over them with the back
# rounded and the hinds swing past them and land far ahead of their
# prints; the hinds drive with the back extending into a long flight.
# About 4 strides a second at this size (Heglund, Taylor and McMahon 1974:
# 3.7 Hz at the trot-gallop change for 4 kg, rising slowly with speed),
# so 2.75 m a stride; the feet stand a few hundredths of a second.

HOP_SPEED = 1.2
FLIGHT_SPEED = 11.0
HOP = dict(seconds=20 / 48, phase=dict(LF=0.0, RF=0.12, LH=0.60, RH=0.63), duty=(0.52, 0.42),
           land=(-0.255, 0.035), lift=(0.030, 0.050))
FLIGHT = dict(seconds=12 / 48, phase=dict(LF=0.0, RF=0.08, LH=0.36, RH=0.40), duty=(0.10, 0.12),
              land=(-0.290, 0.000), lift=(0.070, 0.065))


def speed_of(plan):
    """The ground speed the plan's standing feet give, m/s (every foot's
    sweep over its stance)."""
    return plan["sweep"] / plan["seconds"] if "sweep" in plan else None


def sweep(plan, leg, speed):
    """How far a standing foot of ``leg`` slides back, metres."""
    duty = plan["duty"][0 if leg[1] == "F" else 1]
    return speed * duty * plan["seconds"]


def prints(plan, speed):
    """Where each foot lands on the ground over one stride (metres along
    the track, the hare running toward -y; the body's origin at 0 when the
    stride starts)."""
    out = {}
    for leg, on in plan["phase"].items():
        land = plan["land"][0 if leg[1] == "F" else 1]
        out[leg] = land - speed * plan["seconds"] * on
    return out


# ---------------------------------------------------------------------------
# Everything below runs in Blender.

if bpy is not None:
    from mathutils import Matrix, Quaternion, Vector

    sys.path.insert(0, HERE)
    import animal_rig as rig  # noqa: E402
    import bmesh  # noqa: E402

    F = rig.Steps(os.path.join(HERE, "fox.py"), prefix="hare:")
    H = F.H
    _jit = F._jit


def at(t, u=0.0, s=0.0):
    return Vector(_at(t, u, s))


def head_matrix():
    return Matrix((Vector((1, 0, 0)), Vector(HEAD_A), Vector(HEAD_U))).transposed()


def rot_x(deg):
    return Matrix.Rotation(math.radians(deg), 3, "X")


BOUNDS = ((-0.10, -0.36, -0.004), (0.10, 0.33, 0.49))
OPENINGS = []
CAVITIES = []
CLAWS = []


# The side photograph's mapping to the model (Jakub Hałun 5150,
# ``PHOTOS[0]``, 1920 px coordinates, as ``PROJECTIONS``): ``SIDE_PX``
# metres a pixel, the rump's end on ``SIDE_RUMP`` and the fore feet's
# line on the ground; the trunk's rings (``TRUNK_RINGS``) were laid on
# its outline.
SIDE_PX = (0.00042, 0.00038)  # along the body (the photo sees it a little turned toward the camera, so its length is foreshortened) and up
SIDE_RUMP = (1580, 0.215)  # the rump's end: pixel u, model y
SIDE_GROUND = 1720  # pixel v of the fore feet's soles

def side_to_model(u, v):
    """A pixel of the side photograph (1920 px) to the model's (y, z)."""
    return (SIDE_RUMP[1] + (u - SIDE_RUMP[0]) * SIDE_PX[0], (SIDE_GROUND - v) * SIDE_PX[1])


def side_layout(u, v):
    """A pixel of the side photograph to the layout's (y, z) (``unwarp``)."""
    return tuple(unwarp((0.0,) + side_to_model(u, v))[1:])


# The body's hair from the same photograph, on the same mapping as the
# trunk's outline.
PROJECTIONS.append(dict(file=PHOTOS[0][0], region="body", mode="detail", strength=1.4, feather=60.0,
                        landmarks=[(side_layout(u, v), (u, v)) for u, v in
                                   ((560, 1150), (823, 832), (1332, 791), (1573, 1048), (1077, 1328), (670, 1277))],
                        outline=[(524, 832), (785, 819), (1014, 813), (1332, 783), (1523, 870), (1580, 1023), (1567, 1214),
                                 (1497, 1277), (1268, 1328), (1077, 1341), (823, 1303), (632, 1239), (536, 1112)]))



def foot(sdf, x, ball_y, toe_y, fore):
    """The claws' tips just showing out of the furred toes (the foot's mass
    is the leg's cage: ``HIND_RINGS``, ``FORE_RINGS``), the grooves between
    the toes at the front and the sole cut flat on the ground."""
    E, R = rig.sdf_ellipsoid, rig.sdf_round_cone
    back = 1.0 if toe_y < ball_y else -1.0
    k = 0.85 if fore else 1.0
    mid = (ball_y + toe_y) / 2
    for dx in (-0.0075, -0.0025, 0.0025, 0.0075):
        tip = (x + dx * k, toe_y - back * 0.002, 0.0028)
        sdf.add(R((x + dx * k * 0.9, toe_y + back * 0.006, 0.0045), tip, 0.0018 * k, 0.0007), 0.001)
        CLAWS.append(tip)
    for dx in (-0.005, 0.0, 0.005):
        sdf.carve(E((x + dx * k, toe_y + back * 0.008, 0.009 * k), (0.0008, 0.010, 0.006)), 0.0015)
    lo, hi = (x - 0.03, mid - 0.05, -0.05), (x + 0.03, mid + 0.05, 0.0)
    sdf.carve((lambda X, Y, Z: Z + 0.0 * X + 0.0 * Y, lo, hi), 0.002)


# ---------------------------------------------------------------------------
# The body (v7): polygon modelling. Each part is a low quad cage, rings of
# ``CAGE_N`` vertices laid on the outlines traced from the side, the front
# and above (the Hałun photographs, ``PHOTOS``), subdivided (Catmull-Clark)
# and shaped by ``MASSES`` (the shoulder blade, the hollows behind the
# shoulder and in the flank, the cheek); the parts are voxelised and
# smoothly joined (``CAGE_BLEND``) with the small features (the ears, the
# lids, the nostrils, the claws, the tail) on the sculpt's grid. A ring's
# section is a superellipse: half width ``w`` across, ``top`` and ``bot``
# from its centre along its up, ``c`` (-1 at the bottom .. 1 at the top)
# the height of the widest point, ``n`` 2 an ellipse, more a squarer one.
# The cages are in the model (after ``warp``), but for the head's, laid in
# the head's frame and warped with it.
CAGE_N = 16

# The trunk and the neck, from the throat under the head to the round
# rump: each ring a chord across the side outline, from a point on the top
# line (the nape, the withers, the back, the croup) to one on the under
# line (the throat, the chest's front, the deep chest, the belly rising to
# the flank, the haunch's edge); the neck's and the rump's chords fan.
# (top (y, z), bottom (y, z), half width, c, n).
TRUNK_RINGS = [
    ((-0.176, 0.384), (-0.222, 0.318), 0.020, 0.0, 2.0),  # inside the back of the head
    ((-0.162, 0.374), (-0.222, 0.290), 0.026, 0.0, 2.0),  # the neck: the nape short, the throat long, set back under the jaw
    ((-0.147, 0.367), (-0.240, 0.252), 0.030, 0.0, 2.0),
    ((-0.132, 0.362), (-0.247, 0.218), 0.033, -0.1, 2.0),  # the chest's front
    ((-0.117, 0.359), (-0.232, 0.168), 0.034, -0.1, 2.0),
    ((-0.100, 0.357), (-0.200, 0.132), 0.037, 0.0, 2.05),  # the withers; the chest's keel
    ((-0.080, 0.354), (-0.150, 0.117), 0.039, 0.05, 2.1),
    ((-0.058, 0.352), (-0.098, 0.114), 0.040, 0.1, 2.1),
    ((-0.032, 0.351), (-0.040, 0.118), 0.039, 0.15, 2.1),  # the deep chest behind the elbows
    ((-0.006, 0.352), (-0.008, 0.136), 0.036, 0.25, 2.1),  # the last ribs, then the belly tucked up hard
    ((0.020, 0.354), (0.022, 0.172), 0.030, 0.35, 2.0),
    ((0.045, 0.356), (0.046, 0.198), 0.028, 0.35, 2.0),  # the waist: narrow and high, under the loins
    ((0.070, 0.357), (0.071, 0.206), 0.029, 0.3, 2.0),  # the flank: the thigh's mass hangs outside it
    ((0.095, 0.357), (0.096, 0.204), 0.032, 0.2, 2.05),
    ((0.120, 0.354), (0.122, 0.194), 0.035, 0.1, 2.1),  # the haunch, the croup high and long
    ((0.145, 0.345), (0.150, 0.186), 0.036, 0.05, 2.05),
    ((0.168, 0.327), (0.175, 0.180), 0.034, 0.0, 2.0),  # the round rump
    ((0.188, 0.295), (0.195, 0.177), 0.033, 0.0, 2.0),
    ((0.203, 0.264), (0.207, 0.180), 0.027, 0.0, 2.0),
    ((0.212, 0.236), (0.213, 0.192), 0.016, 0.0, 2.0),
]

# The head, in its frame (``_at``): the side view traced on the crouched
# hare's head (Jakub Hałun 5415, ``PHOTOS[2]``, seen square from the side)
# and scaled by its eye-to-nose distance onto ``EYE`` and ``NOSE_T``: long,
# tapering to the nose, the eye about half way, the forehead falling in
# one long convex curve to a blunt nose, the jaw's line nearly straight.
# Widths (the hares seen from the front): about 5 cm across the cheeks
# under the eyes, narrowing to the muzzle, widest low. (t, top u, bottom
# u, half width, c, n).
HEAD_RINGS = [(-0.020, -0.002, -0.038, 0.016, -0.05, 2.0), (-0.012, 0.012, -0.049, 0.023, -0.15, 1.95),
              (-0.002, 0.0185, -0.0545, 0.0285, -0.3, 1.9), (0.010, 0.0215, -0.0555, 0.0320, -0.4, 1.85),
              (0.022, 0.0220, -0.0540, 0.0345, -0.45, 1.85), (0.034, 0.0195, -0.0515, 0.0345, -0.45, 1.85),
              (0.045, 0.0155, -0.0466, 0.0325, -0.45, 1.85), (0.055, 0.0130, -0.0462, 0.0285, -0.4, 1.9),
              (0.065, 0.0078, -0.0458, 0.0238, -0.3, 1.95), (0.073, 0.0012, -0.0438, 0.0198, -0.25, 2.0),
              (0.0795, -0.0060, -0.0418, 0.0165, -0.2, 2.0), (0.0853, -0.0148, -0.0384, 0.0125, -0.15, 2.0),
              (0.0889, -0.0224, -0.0340, 0.0080, 0.0, 2.0), (0.0905, -0.0278, -0.0296, 0.0032, 0.0, 2.0)]

# The legs (the left; the right mirrored), in the model along the bones at
# rest: (centre, half width, front, back); the ring stands across the leg,
# its "front" the way the leg faces (forward, or up on the foot).
HIND_RINGS = [
    ((0.026, 0.122, 0.292), 0.018, 0.030, 0.040),  # inside the croup
    ((0.034, 0.098, 0.258), 0.030, 0.052, 0.074),  # the great thigh, a broad plate on the haunch from the hip ...
    ((0.040, 0.066, 0.212), 0.033, 0.050, 0.078),
    ((0.044, 0.036, 0.170), 0.029, 0.038, 0.062),
    ((0.048, 0.012, 0.142), 0.021, 0.024, 0.060),  # ... down and forward to the stifle, its back filling the hollow over the calf
    # The shank wrapped in the haunch: the calf (and the hamstrings' ends)
    # reach up and back to the seat, so the haunch is one round mass whose
    # lower edge is the shin and whose back runs down to the heel.
    ((0.050, 0.030, 0.120), 0.021, 0.016, 0.085),
    ((0.050, 0.058, 0.100), 0.019, 0.013, 0.070),
    ((0.049, 0.084, 0.082), 0.016, 0.010, 0.046),
    ((0.048, 0.104, 0.066), 0.0135, 0.012, 0.022),  # the hock, thick, the heel standing out behind
    ((0.048, 0.082, 0.044), 0.0105, 0.009, 0.011),  # the long metatarsus
    ((0.048, 0.056, 0.025), 0.0105, 0.008, 0.010),
    ((0.048, 0.034, 0.013), 0.0115, 0.0095, 0.0100),  # the ball
    ((0.048, 0.010, 0.0100), 0.0115, 0.0090, 0.0090),  # the long hind foot, its toes close together
    ((0.048, -0.011, 0.0085), 0.0105, 0.0075, 0.0075),
    ((0.048, -0.023, 0.0065), 0.0060, 0.0040, 0.0040),
]
# The hamstrings: from under the tail (the seat bones) down and forward
# into the back of the calf, the haunch's round back edge. They must end
# inside the shank: hanging behind it (or reaching for the heel) they read
# as a second, tail-like limb, worst in the sits.
SEAT_RINGS = [
    ((0.030, 0.196, 0.236), 0.026, 0.024, 0.020),  # as broad as the thigh plate: one mass with it, no groove
    ((0.038, 0.188, 0.192), 0.032, 0.030, 0.024),
    ((0.043, 0.172, 0.150), 0.029, 0.045, 0.022),
    ((0.046, 0.150, 0.112), 0.024, 0.055, 0.016),
    ((0.048, 0.128, 0.084), 0.017, 0.034, 0.011),  # inside the calf's back, over the heel
]
FORE_RINGS = [
    ((0.022, -0.112, 0.268), 0.015, 0.018, 0.022),  # inside the shoulder
    ((0.028, -0.150, 0.218), 0.024, 0.024, 0.034),  # the point of the shoulder and the upper arm
    ((0.032, -0.146, 0.178), 0.022, 0.019, 0.031),  # the triceps behind
    ((0.035, -0.139, 0.142), 0.0185, 0.0170, 0.0250),  # the elbow, its point standing out behind under the chest
    ((0.034, -0.143, 0.110), 0.0180, 0.0185, 0.0175),  # the forearm, a mass of muscle and fur
    ((0.034, -0.145, 0.074), 0.0130, 0.0125, 0.0115),
    ((0.034, -0.146, 0.041), 0.0105, 0.0095, 0.0105),  # the wrist
    ((0.034, -0.149, 0.024), 0.0095, 0.0085, 0.0090),
    ((0.034, -0.156, 0.011), 0.0100, 0.0085, 0.0085),  # the narrow fore foot
    ((0.034, -0.170, 0.0080), 0.0095, 0.0075, 0.0065),
    ((0.034, -0.182, 0.0055), 0.0050, 0.0035, 0.0035),
]

# Masses pressed into the subdivided cages (model metres): (parts, centre,
# radii, displacement along the normal; mirrored for the right side).
MASSES = [
    ("trunk", (0.036, -0.100, 0.290), (0.018, 0.030, 0.045), 0.0060),  # the shoulder blade
    ("trunk", (0.040, -0.060, 0.245), (0.016, 0.016, 0.055), -0.0050),  # the hollow behind the shoulder
    ("trunk", (0.038, 0.058, 0.262), (0.018, 0.024, 0.040), -0.0040),  # the hollow of the flank before the thigh
    ("trunk", (0.040, -0.020, 0.205), (0.020, 0.040, 0.050), 0.0020),  # the ribs
    ("hind", (0.058, 0.135, 0.205), (0.020, 0.030, 0.045), 0.0030),  # the hamstrings
    ("hind", (0.072, 0.062, 0.215), (0.022, 0.050, 0.045), 0.0040),  # the thigh's outer muscle
    ("hind", (0.056, 0.045, 0.118), (0.012, 0.018, 0.020), 0.0020),  # the calf
    ("fore", (0.050, -0.152, 0.205), (0.016, 0.020, 0.030), 0.0020),  # the deltoid on the point of the shoulder
    ("fore", (0.044, -0.150, 0.100), (0.012, 0.010, 0.030), 0.0012),  # the forearm's extensors
]
HEAD_MASSES = [  # in the head's frame (t, u, s): warped with the head
    ((0.030, -0.042, 0.029), (0.019, 0.017, 0.014), 0.0070),  # the cheek (the masseter) under and behind the eye
    ((0.052, -0.020, 0.025), (0.016, 0.004, 0.006), 0.0012),  # the cheekbone's ridge from under the eye forward
    ((0.000, -0.044, 0.016), (0.010, 0.008, 0.010), 0.0020),  # the jaw's angle
]
CAGE_BLEND = dict(head=0.006, fore=0.018, hind=0.020)  # the smooth joins to the trunk: a crisp jaw line over the throat


def _ring(mid, side, up, hh, w, c, n, count=CAGE_N):
    """A section's ``count`` points round from the top (pure Python)."""
    pts = []
    for k in range(count):
        th = 2.0 * math.pi * k / count
        a, b = math.cos(th), math.sin(th)
        v = math.copysign(abs(a) ** (2.0 / n), a)
        h = math.copysign(abs(b) ** (2.0 / n), b)
        vv = hh * (c + v * ((1.0 - c) if v > 0 else (1.0 + c)))
        pts.append(tuple(mid[i] + side[i] * w * h + up[i] * vv for i in range(3)))
    return pts


def _sub(a, b):
    return tuple(a[i] - b[i] for i in range(3))


def _dot(a, b):
    return sum(a[i] * b[i] for i in range(3))


def trunk_cage():
    """The trunk's rings in the model."""
    out = []
    for top, bot, w, c, n in TRUNK_RINGS:
        t3, b3 = (0.0,) + tuple(top), (0.0,) + tuple(bot)
        mid = tuple((t3[i] + b3[i]) / 2 for i in range(3))
        out.append(_ring(mid, (1.0, 0.0, 0.0), _norm(_sub(t3, b3)), math.dist(t3, b3) / 2, w, c, n))
    return out


def head_cage():
    """The head's rings in the layout's head frame (warp them)."""
    return [_ring(_at(t, (top + bot) / 2), (1.0, 0.0, 0.0), HEAD_U, (top - bot) / 2, w, c, n)
            for t, top, bot, w, c, n in HEAD_RINGS]


def leg_cage(rings, sx=1.0):
    """A leg's rings: each across the leg's direction there, its up the way
    the leg faces (forward, projected)."""
    pts = [r[0] for r in rings]
    out = []
    for i, (p, w, front, back) in enumerate(rings):
        a, b = pts[max(i - 1, 0)], pts[min(i + 1, len(pts) - 1)]
        tng = _norm(_sub(b, a))
        hint = (0.0, -1.0, 0.0)
        up = _norm(_sub(hint, tuple(tng[k] * _dot(hint, tng) for k in range(3))))
        side = (1.0, 0.0, 0.0)
        side = _norm(_sub(side, tuple(tng[k] * _dot(side, tng) + up[k] * _dot(side, up) for k in range(3))))
        mid = tuple(p[k] + up[k] * (front - back) / 2 for k in range(3))
        ring = _ring(mid, side, up, (front + back) / 2, w, 0.0, 2.0)
        out.append([(sx * q[0], q[1], q[2]) for q in ring])
    return out


def tube(rings):
    """A closed quad tube through ``rings`` (each ``CAGE_N`` points),
    capped at both ends: vertices and faces."""
    m = len(rings[0])
    verts = [p for r in rings for p in r]
    faces = []
    for i in range(len(rings) - 1):
        for k in range(m):
            k1 = (k + 1) % m
            faces.append((i * m + k, i * m + k1, (i + 1) * m + k1, (i + 1) * m + k))
    faces.append(tuple(reversed(range(m))))
    last = (len(rings) - 1) * m
    faces.append(tuple(last + k for k in range(m)))
    return verts, faces


def cage_parts():
    """The cages in the model: (name, kind, vertices, faces)."""
    out = [("trunk", "trunk") + tube(trunk_cage()),
           ("head", "head") + tube([[warp(p) for p in r] for r in head_cage()])]
    for sx, side in ((1.0, "L"), (-1.0, "R")):
        out.append((f"fore.{side}", "fore") + tube(leg_cage(FORE_RINGS, sx)))
        out.append((f"hind.{side}", "hind") + tube(leg_cage(HIND_RINGS, sx)))
        out.append((f"seat.{side}", "hind") + tube(leg_cage(SEAT_RINGS, sx)))
    return out


def _masses(kind):
    out = [(c, r, a) for k, c, r, a in MASSES if k == kind]
    if kind == "head":
        out = [(warp(_at(t, u, s)), tuple(x * HEAD_K for x in r), a) for (t, u, s), r, a in HEAD_MASSES]
    return out + [((-c[0], c[1], c[2]), r, a) for c, r, a in out if c[0] != 0.0]


def shaped(name, kind, verts, faces, levels):
    """The cage subdivided ``levels`` times and pressed by its masses: a
    numpy array of its points and its faces."""
    import numpy as np

    mesh = bpy.data.meshes.new(f"Cage_{name}")
    mesh.from_pydata(verts, [], faces)
    bm = bmesh.new()
    bm.from_mesh(mesh)
    bmesh.ops.recalc_face_normals(bm, faces=bm.faces)
    bm.to_mesh(mesh)
    bm.free()
    obj = bpy.data.objects.new(mesh.name, mesh)
    bpy.context.collection.objects.link(obj)
    mod = obj.modifiers.new("Subdivision", "SUBSURF")
    mod.levels = mod.render_levels = levels
    dg = bpy.context.evaluated_depsgraph_get()
    dense = bpy.data.meshes.new_from_object(obj.evaluated_get(dg))
    bpy.data.objects.remove(obj)
    bpy.data.meshes.remove(mesh)
    co = np.zeros(len(dense.vertices) * 3)
    dense.vertices.foreach_get("co", co)
    nor = np.zeros(len(dense.vertices) * 3)
    dense.vertex_normals.foreach_get("vector", nor)
    co, nor = co.reshape(-1, 3), nor.reshape(-1, 3)
    push = np.zeros(len(co))
    for c, r, a in _masses(kind):
        q = (co - np.asarray(c)) / np.asarray(r)
        d2 = np.sum(q * q, axis=1)
        push += a * np.where(d2 < 1.0, (1.0 - d2) ** 2, 0.0)
    co = co + nor * push[:, None]
    quads = [tuple(p.vertices) for p in dense.polygons]
    bpy.data.meshes.remove(dense)
    return co, quads


def cage_field(co, quads, voxel, band=24):
    """A part's distance field: a level set of its surface (OpenVDB) on a
    dense grid in the model, read back at the layout's points through
    ``warp``: ``(fn, lo, hi)`` for ``Sdf.add``."""
    import numpy as np
    import openvdb

    tris = np.zeros((0, 3), np.int32)
    q = np.asarray([f for f in quads if len(f) == 4], np.int32)
    grid = openvdb.FloatGrid.createLevelSetFromPolygons(co.astype(np.float32), triangles=tris, quads=q,
                                                       transform=openvdb.createLinearTransform(voxelSize=voxel),
                                                       halfWidth=float(band))
    bg = float(grid.background)
    lo = np.floor(co.min(axis=0) / voxel).astype(int) - band - 2
    hi = np.ceil(co.max(axis=0) / voxel).astype(int) + band + 2
    arr = np.full(tuple(hi - lo + 1), bg, np.float32)
    grid.copyToArray(arr, ijk=tuple(int(v) for v in lo))
    shape = np.asarray(arr.shape)

    def one(x, y, z):
        m = warp((x, y, z))
        f = [m[i] / voxel - lo[i] for i in range(3)]
        i0 = [np.floor(f[i]).astype(int) for i in range(3)]
        t = [f[i] - i0[i] for i in range(3)]
        inside = np.ones(np.shape(f[0]), bool)
        for i in range(3):
            inside &= (i0[i] >= 0) & (i0[i] < shape[i] - 1)
            i0[i] = np.clip(i0[i], 0, shape[i] - 2)
        out = np.zeros(np.shape(f[0]))
        for dx in (0, 1):
            for dy in (0, 1):
                for dz in (0, 1):
                    wgt = (t[0] if dx else 1 - t[0]) * (t[1] if dy else 1 - t[1]) * (t[2] if dz else 1 - t[2])
                    out += wgt * arr[i0[0] + dx, i0[1] + dy, i0[2] + dz]
        return np.where(inside, out, bg)

    def fn(x, y, z):
        x, y, z = np.broadcast_arrays(x, y, z)
        return np.stack([one(x[i], y[i], z[i]) for i in range(x.shape[0])])

    # The bounds in the layout: the surface's points read back (``unwarp``).
    step = max(1, len(co) // 4000)
    back = np.asarray([unwarp(tuple(p)) for p in co[::step]])
    m = 0.012
    return fn, tuple(back.min(axis=0) - m), tuple(back.max(axis=0) + m)


def body(sdf, voxel, levels):
    """The cages, shaped and joined on the sculpt's grid."""
    for name, kind, verts, faces in cage_parts():
        co, quads = shaped(name, kind, verts, faces, levels)
        sdf.add(cage_field(co, quads, voxel), CAGE_BLEND.get(kind, 0.0))


def tail(sdf):
    """The tail: a short tuft pressed on the rump, its fluff part of the
    volume, flattened; it ends half way down its last bone (the tail's cards
    give its length), so in the sits it never trails on the ground."""
    end = tuple((a + b) / 2 for a, b in zip(TAIL[3], TAIL_END))
    sdf.add(rig.sdf_tube(TAIL + [end], [(0.014, 0.016), (0.015, 0.017), (0.012, 0.013), (0.008, 0.009), (0.004, 0.004)],
                         side=(0.0, 1.0, 0.0)), 0.010)


def face(sdf):
    """What the cage leaves to the grid on the head: the lids round the
    eyes, the nose pad, the split lip, the mouth, the nostrils and the ears."""
    E = rig.sdf_ellipsoid
    Hx = head_matrix()
    n = NOSE_T
    for sx in (1, -1):
        sdf.add(E(at(0.040, 0.004, sx * 0.017), (0.010, 0.016, 0.008), Hx), 0.008)  # the brow over the eye
        sdf.add(E(at(n - 0.012, -0.034, sx * 0.0058), (0.0058, 0.0072, 0.0055), Hx), 0.005)  # the upper lip's whisker pads
    sdf.add(E(at(n - 0.005, -0.026), (0.0048, 0.0040, 0.0046), Hx), 0.005)  # the nose pad
    mouth = E(at(n - 0.010, -0.0385), (0.0030, 0.0025, 0.0004), Hx)
    sdf.carve(mouth, 0.0006)
    OPENINGS.append(mouth[0])
    for sx in (1, -1):
        nostril = E(at(n - 0.0010, -0.0270, sx * 0.0034), (0.0011, 0.0016, 0.0032), Hx @ rot_x(-25))
        sdf.carve(nostril, 0.0008)
        OPENINGS.append(nostril[0])
        t, u, s = EYE
        sdf.add(E(at(t, u, sx * (s - 0.0005)), (0.0098, 0.0122, 0.0110), Hx), 0.003)  # lids, the eye standing out of the head
        sdf.carve(E(at(t + 0.0005, u, sx * (s + 0.0065)), (0.0072, 0.0100, 0.0088), Hx), 0.0012)  # the opening
        ears(sdf, sx)


def ears(sdf, sx):
    """A long ear: a rolled tube at its base opening into a long thin
    spoon, widest a third of the way up, the tip rounded; its hollow
    faces forward and out."""
    base, mid, tip = (Vector(_ear(p, sx)) for p in (EAR_BASE, EAR_MID, EAR_TIP))
    low, mid9 = base.lerp(mid, EAR_NOTCH), base.lerp(mid, 0.9)
    high = mid.lerp(tip, 0.5)
    front = Vector(ear_front(sx))
    sec = EAR_SECTION
    # The rolled base: a round tube, tapering into the blade.
    sdf.add(rig.sdf_round_cone(base + Vector((0, 0, -0.006)), low, sec[0][1], sec[1][1] * 0.55), 0.007)
    # The blade: thin, its thickness along ``front``.
    blade = [low, mid9, high, tip]
    sdf.add(rig.sdf_tube(blade, [sec[1], sec[2], sec[3], sec[4]], side=front), 0.004)
    # The hollow: the rolled base's opening, then the blade's cup.
    f = front * (sec[2][0] * 0.85)
    # It opens gently above the rolled base (a cut into the round tube left
    # thin shards standing at the ear's root).
    cavity = rig.sdf_tube([low.lerp(mid9, 0.10) + front * 0.0045, low.lerp(mid9, 0.35) + front * 0.0035, mid9 + f, high + f, tip + f * 0.5],
                          [(0.0016, 0.0030), (0.0026, 0.0075), (0.0029, 0.0118), (0.0026, 0.0105), (0.0010, 0.0035)], side=front)
    sdf.carve(cavity, 0.0025)
    CAVITIES.append(cavity[0])



def sculpt(voxel):
    """The dense sculpt: one watertight surface."""
    OPENINGS.clear()
    CAVITIES.clear()
    CLAWS.clear()
    sdf = rig.Sdf(*BOUNDS, voxel * 0.24)
    body(sdf, voxel * 0.24, 2 if voxel > 0.006 else 3)
    tail(sdf)
    for sx in (1, -1):
        foot(sdf, FORE_X * sx, BALL_F[1], TOE_F[1], True)
        foot(sdf, HIND_X * sx, BALL_H[1], TOE_H[1], False)
    face(sdf)
    obj = sdf.to_mesh("HareSculpt")
    F.warp_mesh(obj)
    return obj, sdf


# ---------------------------------------------------------------------------
# Regions of the skin, from the rest position.


def opening(p):
    if p[2] < 0.22 or p[1] > -0.28 or not OPENINGS:
        return 1.0
    return F._fn_min(OPENINGS, p)


def ear_cavity(p):
    if not is_ear(p) or not CAVITIES:
        return 1.0
    return F._fn_min(CAVITIES, p)


def is_claw(p):
    return p[2] < 0.012 and any(_dist(p, c) < 0.0030 for c in CLAWS)


def bare(p):
    """Bare skin: round the nostrils, the split lip and the lid rims."""
    if p[1] > -0.25 or p[2] < 0.22:
        return 0.0
    lips = 0.6 * smooth((0.0012 - opening(p)) / 0.0010)
    t, u, s = head_coords(p)
    e = _dist(p, _at(EYE[0], EYE[1], math.copysign(EYE[2], s)))
    rim = smooth((0.0118 - e) / 0.0010) * smooth((e - EYE_RADIUS + 0.0004) / 0.0008)
    return max(lips, rim)


def material_of(p, n):
    return "Main"


MATERIALS = ["Main", "Hair", "Eye_Black", "Eye_Iris"]
FALLBACK = {
    "Main": (0.42, 0.30, 0.18),
    "Hair": (1.0, 1.0, 1.0),
    "Eye_Black": (0.025, 0.020, 0.016),
    "Eye_Iris": (0.40, 0.27, 0.11),  # golden brown, darker than an amber disc
}


def hair_direction(p, n):
    """Which way the fur lies: back from the nose over the head, up the
    ears, back and down the neck, back along the body and down its sides,
    down the legs and forward along the feet, back along the belly, along
    the tail."""
    p, n = Vector(p), Vector(n)
    tail = tail_place(p)
    if tail is not None:
        d = Vector(tail[2])
    elif p.y < -0.20 and p.z > 0.24:
        d = Vector((0.0, 0.1, 1.0)) if is_ear(p) else -Vector(HEAD_A) + Vector((math.copysign(0.25, p.x), 0, 0))
    elif p.y < -0.15 and p.z > 0.15:
        d = Vector((0.0, 0.5, -0.8))
    elif p.z < 0.025:
        d = Vector((0.0, -1.0, 0.0))
    elif p.z < 0.12 and (abs(abs(p.x) - FORE_X) < 0.02 and p.y < -0.1 or abs(abs(p.x) - HIND_X) < 0.025 and p.y > 0.05):
        d = Vector((0.0, 0.15, -1.0))
    elif n.z < -0.5:
        d = Vector((0.0, 1.0, 0.1))
    else:
        d = Vector((0.0, 1.0, -0.4 * max(0.0, -n.z + abs(n.x))))
    d -= n * d.dot(n)
    return d.normalized() if d.length > 1e-6 else Vector((0.0, 1.0, 0.0))


def tone(p, n):
    """A coat-independent shade: round the eye, the ears' hollows, and a
    soft mottling."""
    p, n = Vector(p), Vector(n)
    v = 0.92
    if p.z > 0.24 and p.y < -0.19:
        t, u, s = head_coords(p)
        e = (p - at(EYE[0], EYE[1], math.copysign(EYE[2], s))).length
        v -= 0.10 * smooth((0.012 - e) / 0.002)
        if is_ear(p):
            c = ear_cavity(p)
            v -= 0.15 * smooth((0.003 - abs(c - 0.0015)) / 0.002)
    v += 0.05 * rig.noise(p, (14.0, 14.0, 14.0), seed=2.7) + 0.03 * rig.noise(p, (34.0, 34.0, 34.0), seed=3.9)
    return max(0.3, min(1.0, v))


def dense_zone(p):
    """Where the coat's colour changes on a line and the thin parts (the
    ears, the toes): the game mesh keeps more vertices there."""
    if p[2] < 0.02:
        return True
    if p[1] < -0.19 and p[2] > 0.24:
        t, u, s = head_coords(p)
        return t > NOSE_T - 0.035 or is_ear(p) or abs(t - EYE[0]) < 0.02 and abs(abs(s) - EYE[2]) < 0.015
    return tail_place(p) is not None


# ---------------------------------------------------------------------------
# The eyes: large, amber, the pupil round, set far out at the sides.


# The iris on the eyeball, as the cosine from the line of sight: inside
# ``IRIS[1]`` the large pupil (about half the iris across), outside
# ``IRIS[0]`` the dark rim (the limbus) the lids' opening shows round it.
IRIS = (0.66, 0.93)


# The eye's paint (v7e), on its vertices (``EYE_COLOUR``, which the game
# multiplies into the eye's white-based materials): the polar angle from
# the line of sight in degrees, ``EYE_RINGS`` the sphere's latitudes, dense
# over the visible cap so the iris has rings to paint; inside it a darker
# collar round the pupil, a lighter amber middle with radial streaks, the
# outer edge darker into the limbus, and a catchlight (the sky in the
# cornea) up and forward of the pupil.
EYE_RINGS = (0.0, 4.0, 8.0, 12.0, 16.0, 20.0, 23.0, 26.5, 30.0, 34.0, 38.0, 42.0, 46.0, 50.0, 58.0, 75.0, 105.0, 145.0, 180.0)
IRIS_SRGB = dict(collar=(132, 74, 26), middle=(206, 138, 50), outer=(128, 76, 28), limbus=(44, 30, 18), back=(26, 20, 14))
CATCHLIGHT = (12.0, 40.0, 5.5)  # (polar angle, azimuth from the head's up toward the nose, radius) in degrees
EYE_FRAMES = []  # (centre, line of sight, head up) of each eye in the model, for ``paint_eyes``


def eye_colour(theta, phi):
    """The eye's sRGB at polar angle ``theta`` from the line of sight and
    azimuth ``phi`` (degrees, from the head's up toward the nose)."""
    pupil = math.degrees(math.acos(IRIS[1]))
    rim = math.degrees(math.acos(IRIS[0]))
    d = math.hypot(theta - CATCHLIGHT[0], ((phi - CATCHLIGHT[1] + 180.0) % 360.0 - 180.0) * math.sin(math.radians(max(theta, 1.0))))
    if d < CATCHLIGHT[2] and theta < pupil + 4.0:
        return (246, 246, 240)
    if theta < pupil:
        return (8, 7, 6)
    if theta > rim:
        k = _clip((theta - rim) / 10.0, 0.0, 1.0)
        return tuple(IRIS_SRGB["limbus"][i] + (IRIS_SRGB["back"][i] - IRIS_SRGB["limbus"][i]) * k for i in range(3))
    f = (theta - pupil) / (rim - pupil)  # 0 at the pupil, 1 at the rim
    if f < 0.25:
        rgb = _mix(IRIS_SRGB["collar"], IRIS_SRGB["middle"], f / 0.25)
    elif f < 0.6:
        rgb = IRIS_SRGB["middle"]
    else:
        rgb = _mix(IRIS_SRGB["middle"], IRIS_SRGB["outer"], (f - 0.6) / 0.4)
    streak = 0.92 + 0.18 * _hash(int(round(phi / 11.25)) % 32, 7, 3, 41)
    return tuple(_clip(c * streak, 0.0, 255.0) for c in rgb)


def eyes():
    out = []
    EYE_FRAMES.clear()
    t, u, s = EYE
    for sx in (1, -1):
        c = at(t, u, sx * (s - 0.0030))
        look = (Vector((sx * 0.86, 0, 0)) + Vector(HEAD_A) * 0.48 + Vector(HEAD_U) * 0.08).normalized()
        # The sphere's pole on the line of sight, so its rings are the
        # pupil's and the iris' round edges.
        bpy.ops.mesh.primitive_uv_sphere_add(segments=32, ring_count=len(EYE_RINGS) - 1, radius=EYE_RADIUS, location=c,
                                             rotation=Vector((0, 0, 1)).rotation_difference(look).to_euler())
        eye = bpy.context.active_object
        eye.name = "Eye"
        bpy.ops.object.transform_apply(location=True, rotation=True, scale=True)
        # The latitudes moved onto ``EYE_RINGS``: dense over the iris.
        step = 180.0 / (len(EYE_RINGS) - 1)
        for v in eye.data.vertices:
            d = (v.co - c).normalized()
            th = math.degrees(math.acos(max(-1.0, min(1.0, d.dot(look)))))
            k = min(len(EYE_RINGS) - 1, int(round(th / step)))
            axis = look.cross(d)
            if axis.length < 1e-6:
                continue
            q = Quaternion(axis.normalized(), math.radians(EYE_RINGS[k]))
            v.co = c + (q @ look) * EYE_RADIUS
        eye.data.materials.append(bpy.data.materials["Eye_Black"])
        eye.data.materials.append(bpy.data.materials["Eye_Iris"])
        for poly in eye.data.polygons:
            d = (poly.center - c).normalized()
            k = d.dot(look)
            poly.material_index = 1 if IRIS[0] < k < IRIS[1] else 0  # the iris round a large round pupil, a dark rim round it
            poly.use_smooth = True
        F.warp_mesh(eye)
        EYE_FRAMES.append((Vector(warp(tuple(c))), look, Vector(HEAD_U)))
        out.append(eye)
    return out


def paint_eyes(mesh):
    """The eyes' colours (``eye_colour``) as the mesh's ``EYE_COLOUR``
    attribute, white elsewhere (the game multiplies it into every
    material); the eye materials' base colour white, so the paint shows."""
    import numpy as np

    eye_mats = {i for i, m in enumerate(mesh.data.materials) if m and m.name.startswith("Eye")}
    on_eye = set()
    for poly in mesh.data.polygons:
        if poly.material_index in eye_mats:
            on_eye.update(poly.vertices)
    col = np.ones((len(mesh.data.vertices), 4))
    for i in on_eye:
        p = mesh.data.vertices[i].co
        c, look, up = min(EYE_FRAMES, key=lambda f: (p - f[0]).length)
        d = (p - c).normalized()
        th = math.degrees(math.acos(max(-1.0, min(1.0, d.dot(look)))))
        flat = d - look * d.dot(look)
        nose = Vector(HEAD_A) - look * Vector(HEAD_A).dot(look)
        upp = up - look * up.dot(look)
        phi = math.degrees(math.atan2(flat.dot(nose.normalized()), flat.dot(upp.normalized()))) if flat.length > 1e-6 else 0.0
        col[i, :3] = [(x / 255.0) ** 2.2 for x in eye_colour(th, phi)]  # linear
    attr = mesh.data.color_attributes.new("EYE_COLOUR", "FLOAT_COLOR", "POINT")
    attr.data.foreach_set("color", col.ravel())
    mesh.data.color_attributes.active_color = attr
    mesh.data.color_attributes.render_color_index = mesh.data.color_attributes.active_color_index
    for name in ("Eye_Iris", "Eye_Black"):
        mat = bpy.data.materials[name]
        nt = mat.node_tree
        bsdf = nt.nodes["Principled BSDF"]
        bsdf.inputs["Roughness"].default_value = 0.03
        bsdf.inputs["Base Color"].default_value = (1.0, 1.0, 1.0, 1.0)
        node = nt.nodes.get("EyeColour") or nt.nodes.new("ShaderNodeVertexColor")
        node.name = "EyeColour"
        node.layer_name = "EYE_COLOUR"
        nt.links.new(node.outputs["Color"], bsdf.inputs["Base Color"])


# ---------------------------------------------------------------------------
# Hair cards.


def fur_spots(sdf):
    """The fur's tufts, five points each, coloured as the coat under them
    (``card_column``): the long black-tipped guard hairs bristling over
    the back and the flanks (the hare's ragged outline), the cheeks' fluff
    flaring back, the white fringe of the belly, the chest's ruff, the
    fringe behind the thighs and over the hind feet's toes."""
    out = []
    i = 0

    def add(root, d, length, width, bone="Body", lift=0.3, droop=0.15, column=None, give=0.0):
        n = sdf.sample(root)[1]
        col = F.card_column(tuple(root), tuple(n)) if column is None else column
        out.append(F._tuft(sdf, root, d, length, width, col, bone, lift=lift, droop=droop)[:5] + (give,))

    for sx in (1, -1):
        side = "L" if sx > 0 else "R"
        for k in range(8):  # the cheeks
            i += 1
            t = 0.010 + 0.060 * _jit(i, 1)
            u = -0.030 - 0.024 * _jit(i, 2)
            root = sdf.surface(at(t, u, 0.0), Vector((sx, 0.0, -0.3)), reach=0.06)
            add(root, Vector((sx * 0.3, 0.9, -0.3)), 0.007 + 0.004 * _jit(i, 3), 0.0030 + 0.0012 * _jit(i, 4), "Head", lift=0.05, droop=0.6, column=GRIZZLED_2)
        for k in range(14):  # behind the thighs
            i += 1
            z = 0.10 + 0.08 * _jit(i, 8)
            root = sdf.surface(Vector((sx * 0.036, 0.17, z)), Vector((sx * 0.35, 1.0, -0.1)), reach=0.1)
            add(root, Vector((sx * 0.1, 0.5, -1.0)), 0.012 + 0.008 * _jit(i, 9), 0.006 + 0.002 * _jit(i, 10), f"BackLeg.{side}",
                lift=0.3, droop=0.25, give=1.0)
        for k in range(7):  # the fringe behind the forearm
            i += 1
            z = 0.045 + 0.06 * _jit(i, 14)
            root = sdf.surface(Vector((sx * FORE_X, -0.143, z)), Vector((sx * 0.3, 1.0, 0.0)), reach=0.04)
            add(root, Vector((0, 0.6, -1.0)), 0.008 + 0.005 * _jit(i, 15), 0.004, f"FrontUpperLeg.{side}", lift=0.25, droop=0.3, give=1.0)
        for k in range(7):  # the shank's fur, behind and outside
            i += 1
            f = _jit(i, 16)
            p0 = Vector((sx * HIND_X, 0.090 + 0.07 * f, 0.112 - 0.045 * f))
            root = sdf.surface(p0, Vector((sx * 0.7, 0.7, 0.1)), reach=0.05)
            add(root, Vector((0, 0.8, -0.6)), 0.009 + 0.005 * _jit(i, 17), 0.0045, f"BackUpperLeg.{side}", lift=0.2, droop=0.3, give=1.0)
        for k in range(6):  # over the hind toes
            i += 1
            root = sdf.surface(Vector((sx * HIND_X + sx * 0.008 * (_jit(i, 11) - 0.5), 0.085 + 0.02 * _jit(i, 12), 0.015)), Vector((0, 0, 1)), reach=0.03)
            add(root, Vector((0, -1.0, -0.3)), 0.010 + 0.004 * _jit(i, 13), 0.006, f"IKBackLeg.{side}", lift=0.2, droop=0.5)
    for k in range(56):  # the guard hairs over the back, the croup and the flanks
        i += 1
        y = -0.15 + 0.35 * _jit(i, 17)
        ang = (_jit(i, 18) - 0.5) * (150.0 if y < 0.15 else 120.0)
        zc = 0.19
        root, d0 = F._on_trunk(sdf, y, ang, zc)
        add(root, Vector((0, 1.0, -0.25)) + d0 * 0.10, 0.009 + 0.006 * _jit(i, 20), 0.0030 + 0.0012 * _jit(i, 21), lift=0.10, droop=0.3)
    for k in range(22):  # the belly's white fringe
        i += 1
        y = -0.10 + 0.21 * _jit(i, 27)
        ang = 180.0 + (_jit(i, 28) - 0.5) * 100.0
        root, d0 = F._on_trunk(sdf, y, ang, 0.19)
        add(root, Vector((0, 1.0, -0.35)), 0.009 + 0.006 * _jit(i, 29), 0.005, lift=0.08, droop=0.45, column=CREAM)
    for k in range(14):  # the chest's ruff
        i += 1
        root = sdf.surface(Vector((0.03 * (_jit(i, 31) - 0.5), -0.14, 0.14 + 0.07 * _jit(i, 32))), Vector((0, -1, -0.2)), reach=0.1)
        add(root, Vector((0, 0.3, -1.0)), 0.009 + 0.005 * _jit(i, 33), 0.005, "Torso3", lift=0.2, droop=0.4)
    return out


def ear_tufts(sdf):
    """The long white hairs of each ear's front rim, and a few inside its
    base, four points each."""
    out = []
    for sx, side in ((1, "L"), (-1, "R")):
        base, mid, tip = (Vector(_ear(p, sx)) for p in (EAR_BASE, EAR_MID, EAR_TIP))
        front = Vector(ear_front(sx))
        for k in range(14):
            f = 0.12 + 0.70 * k / 13 + 0.03 * (_jit(k, 33 + sx) - 0.5)
            on = base.lerp(mid, f / 0.5) if f < 0.5 else mid.lerp(tip, (f - 0.5) / 0.5)
            up = (tip - base).normalized()
            width_here = 0.0115 * math.sin(math.pi * min(1.0, 0.25 + f * 0.85)) + 0.002
            inner = (front.cross(up)).normalized() * (-sx)  # toward the ear's front (medial) edge
            root = on + inner * width_here * 0.92 + front * 0.001
            d = (up * 0.9 + inner * 0.25 + front * 0.2).normalized()
            length = 0.007 + 0.006 * _jit(k, 31 + sx)
            bone = f"Ear1.{side}" if f < 0.5 else f"Ear2.{side}"
            pts = [root + d * length * j / 3 for j in range(4)]
            out.append((pts, [0.0035, 0.003, 0.0022, 0.0010], [front] * 4, WISP_WHITE, bone, 1.0))
    return out


def whisker_spots(sdf):
    """The long whiskers on the muzzle, black and white, and a few over the
    eyes, four points each."""
    out = []
    for sx in (1, -1):
        for k in range(14):
            t = NOSE_T - 0.022 + 0.012 * _jit(k, 41 + sx)
            u = -0.034 - 0.008 * _jit(k, 43 + sx)
            root = sdf.surface(at(t, u, 0.0), Vector((sx, 0, -0.1)), reach=0.05)
            d = (Vector((sx * 0.8, 0, 0)) - Vector(HEAD_A) * 0.35 - Vector(HEAD_U) * (0.25 * _jit(k, 45) - 0.05)).normalized()
            length = 0.045 + 0.035 * _jit(k, 47 + sx)
            pts = [root + d * length * j / 3 + Vector((0, 0, -0.004 * j * j / 9)) for j in range(4)]
            out.append((pts, [0.0013, 0.0011, 0.0008, 0.0004], [Vector((0, 0, 1))] * 4, WISP_BLACK if k % 4 else WISP_WHITE, "Head", 0.0))
        for k in range(3):
            root = sdf.surface(at(EYE[0] - 0.004 + 0.006 * k, EYE[1] + 0.012, sx * EYE[2]), Vector((sx, 0, 0.5)), reach=0.03)
            d = (Vector((sx * 0.5, 0, 0)) + Vector(HEAD_U) * 0.8 - Vector(HEAD_A) * 0.2).normalized()
            pts = [root + d * 0.030 * j / 3 for j in range(4)]
            out.append((pts, [0.0025, 0.002, 0.0015, 0.0008], [Vector((0, 0, 1))] * 4, WISP_BLACK, "Head", 0.0))
    return out


def tail_spots(sdf):
    """The tail's fluff: short locks all round it, black on top, white
    below and at the sides, seven points each."""
    pts_axis = [Vector(p) for p in TAIL + [TAIL_END]]
    path = rig._catmull(pts_axis, 4)
    total = len(path) - 1
    strands = []
    for i in range(64):
        f = 0.05 + 0.92 * _jit(i, 51)
        k = min(int(f * total), total - 1)
        c = path[k][0].lerp(path[k + 1][0], f * total - k)
        along = (path[k + 1][0] - path[k][0]).normalized()
        ang = 2 * math.pi * _jit(i, 53)
        side = Vector((1.0, 0.0, 0.0))
        up = along.cross(side).normalized()
        if up.z < 0:
            up = -up
        out = (side * math.cos(ang) + up * math.sin(ang)).normalized()
        root = sdf.surface(c, out, reach=0.05)
        length = 0.006 + 0.005 * _jit(i, 55) * math.sin(math.pi * min(1.0, f + 0.15))
        pts, outs = H.fall(sdf, root, (out * 0.1 + along).normalized(), length, 0.0008 + 0.0012 * _jit(i, 65),
                           steps=6, gravity=(along + out * 0.2).normalized(), stiff=0.5)
        top = out.dot(up)
        col = BLACK if top > 0.55 else CREAM
        width = 0.0035 + 0.0025 * _jit(i, 59)
        widths = [width * (0.7 + 0.6 * (j / 6) ** 0.6) * (1.0 - 0.6 * (j / 6) ** 2.5) for j in range(7)]
        strands.append((pts, widths, outs, col))
    return strands


# ---------------------------------------------------------------------------
# Skinning.


def tail_only(body, arm):
    """The tail's skin follows the tail chain only (its nearest point),
    blended into the croup's weights over its root."""
    chain = [(name, arm.data.bones[name].head_local.copy()) for name in TAIL_CHAIN]
    chain.append((None, arm.data.bones[TAIL_CHAIN[-1]].tail_local.copy()))
    groups = {g.name: g for g in body.vertex_groups}

    def group(name):
        if name not in groups:
            groups[name] = body.vertex_groups.new(name=name)
        return groups[name]

    for v in body.data.vertices:
        p = Vector(unwarp(v.co))
        place = tail_place(tuple(p))
        if place is None:
            continue
        own = smooth((p.y - TAIL[0][1] + 0.006) / 0.02)
        best = None
        for i in range(len(chain) - 1):
            a, b = chain[i][1], chain[i + 1][1]
            t = max(0.0, min(1.0, (v.co - a).dot(b - a) / (b - a).length_squared))
            d = (v.co - (a + (b - a) * t)).length
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


def elbows(body):
    """The trunk behind and above each elbow follows the chest, not the
    swinging fore leg."""
    groups = {g.name: g for g in body.vertex_groups}
    chest = groups.get("Torso3") or body.vertex_groups.new(name="Torso3")
    for v in body.data.vertices:
        p = Vector(unwarp(v.co))
        trunk_ = max(smooth((p.y + 0.128) / 0.03), smooth((0.022 - abs(p.x)) / 0.010)) * smooth((p.z - 0.125) / 0.03)
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
                chest.add([v.index], moved, "ADD")
    bpy.ops.object.select_all(action="DESELECT")
    body.select_set(True)
    bpy.context.view_layer.objects.active = body
    bpy.ops.object.vertex_group_normalize_all(lock_active=False)
    for v in body.data.vertices:
        v.select = any((Vector(unwarp(v.co)) - Vector((sx * 0.038, -0.134, 0.12))).length < 0.045 for sx in (1, -1))
    bpy.ops.object.mode_set(mode="EDIT")
    bpy.ops.object.vertex_group_smooth(group_select_mode="ALL", factor=0.5, repeat=6)
    bpy.ops.object.mode_set(mode="OBJECT")
    bpy.ops.object.vertex_group_limit_total(group_select_mode="ALL", limit=4)
    bpy.ops.object.vertex_group_normalize_all(lock_active=False)


def head_rigid(body):
    """The skin of the face round the eyes moves with the head alone (the
    eyes are rigid on ``Head``): forward of the poll and above the jaw the
    neck's and the jaw's heat weights are blended out, so the lids stay on
    the eyes when the neck bends."""
    groups = {g.name: g for g in body.vertex_groups}
    head = groups.get("Head") or body.vertex_groups.new(name="Head")
    for v in body.data.vertices:
        p = unwarp(v.co)
        if p[2] < 0.22 or p[1] > -0.17 or is_ear(p):
            continue
        t, u, s = head_coords(p)
        w = smooth((t - 0.000) / 0.025) * smooth((u + 0.040) / 0.010)
        if w <= 0.0:
            continue
        for g in list(v.groups):
            name = body.vertex_groups[g.group].name
            if name != "Head":
                body.vertex_groups[g.group].add([v.index], g.weight * (1.0 - w), "REPLACE")
        try:
            old = head.weight(v.index)
        except RuntimeError:
            old = 0.0
        head.add([v.index], old + (1.0 - old) * w, "REPLACE")
    bpy.ops.object.select_all(action="DESELECT")
    body.select_set(True)
    bpy.context.view_layer.objects.active = body
    bpy.ops.object.vertex_group_normalize_all(lock_active=False)


def skin(body, arm):
    """The fox's skinning (``fox.py``'s ``skin``), then the face rigid on the
    head (``head_rigid``) and the calves on the shanks (``calves``)."""
    F.skin(body, arm)
    head_rigid(body)
    calves(body)


def calves(body):
    """The haunch's lower back (the calf and the hamstrings' ends, down to
    the heel) follows the shank (``BackUpperLeg``): heat weights shared it
    with the pelvis, so when the leg reached back it stretched from the rump
    to the heel like a kangaroo's tail."""
    groups = {g.name: g for g in body.vertex_groups}
    picked = []
    for v in body.data.vertices:
        x, y, z = v.co
        w = (smooth((0.150 - z) / 0.045) * smooth((z - 0.060) / 0.012) * smooth((y - 0.030) / 0.030)
             * smooth((abs(x) - 0.018) / 0.010) * smooth((0.205 - y) / 0.02))
        if w <= 0.0:
            continue
        side = "L" if x > 0 else "R"
        name = f"BackUpperLeg.{side}"
        shank = groups.get(name) or body.vertex_groups.new(name=name)
        groups[name] = shank
        own = {name, f"BackLowerLeg.{side}"}
        moved = 0.0
        for g in list(v.groups):
            other = body.vertex_groups[g.group]
            if other.name not in own:
                moved += g.weight * w
                other.add([v.index], g.weight * (1.0 - w), "REPLACE")
        shank.add([v.index], moved, "ADD")
        picked.append(v.index)
    bpy.ops.object.select_all(action="DESELECT")
    body.select_set(True)
    bpy.context.view_layer.objects.active = body
    bpy.ops.object.vertex_group_normalize_all(lock_active=False)
    for v in body.data.vertices:
        v.select = False
    for i in picked:
        body.data.vertices[i].select = True
    bpy.ops.object.mode_set(mode="EDIT")
    bpy.ops.object.vertex_group_smooth(group_select_mode="ALL", factor=0.5, repeat=4)
    bpy.ops.object.mode_set(mode="OBJECT")
    bpy.ops.object.vertex_group_limit_total(group_select_mode="ALL", limit=4)
    bpy.ops.object.vertex_group_normalize_all(lock_active=False)


def weight_hair(cards, arm, kinds):
    """``horse.py``'s ``weight_hair`` on the hare's three tail bones (the
    horse's ends its chain at ``Tail9``): the fluff's cards follow the tail
    chain (``kinds`` "tail"); the others hang from their bone, more toward
    the tip (``kinds`` (bone, give)), and take no weight from the tail."""
    groups = {g.name: g for g in cards.vertex_groups}

    def group(name):
        if name not in groups:
            groups[name] = cards.vertex_groups.new(name=name)
        return groups[name]

    chain = [(name, arm.data.bones[name].head_local.copy()) for name in TAIL_CHAIN]
    chain.append((None, arm.data.bones[TAIL_CHAIN[-1]].tail_local.copy()))
    uv = cards.data.uv_layers[0]
    vfrac = {loop.vertex_index: 1.0 - uv.data[loop.index].uv[1] for loop in cards.data.loops}
    per_card = len(cards.data.vertices) // max(1, len(kinds))
    for v in cards.data.vertices:
        kind = kinds[v.index // per_card] if per_card else "tail"
        if kind == "tail":
            best = None
            for i in range(len(chain) - 1):
                a, b = chain[i][1], chain[i + 1][1]
                t = max(0.0, min(1.0, (v.co - a).dot(b - a) / (b - a).length_squared))
                d = (v.co - (a + (b - a) * t)).length
                if best is None or d < best[0]:
                    best = (d, i, t)
            _, i, t = best
            for g in list(v.groups):
                cards.vertex_groups[g.group].remove([v.index])
            group(chain[i][0]).add([v.index], 1.0 - t, "REPLACE")
            group(chain[i + 1][0] or chain[i][0]).add([v.index], t if chain[i + 1][0] else 1.0, "REPLACE")
            continue
        for g in list(v.groups):
            if cards.vertex_groups[g.group].name.startswith("Tail"):
                cards.vertex_groups[g.group].remove([v.index])
        rest = list(v.groups)
        total = sum(g.weight for g in rest)
        for g in rest:
            g.weight = g.weight / total if total > 1e-6 else 1.0 / len(rest)
        if not rest:
            group("Back").add([v.index], 1.0, "REPLACE")
        bone, give = kind
        w = 0.9 * vfrac.get(v.index, 0.0) ** 0.8 * give
        for g in v.groups:
            g.weight *= 1.0 - w
        group(bone).add([v.index], w, "ADD")


NOT_SKIN = {"Body", "PoleTarget.L", "PoleTarget.R", "PoleTargetBack.L", "PoleTargetBack.R",
            "FrontShoulder.L", "FrontShoulder.R", "BackShoulder.L", "BackShoulder.R", "Torso"}
FOLDS = [((0.078, 0.13, 0.20), 0.065, 14), ((0.064, 0.09, 0.112), 0.045, 12)]  # on the skin: the point of the hip, behind the stifle

LIGHT_TRIS = 800  # the light mesh's budget


def surface_maps(body, size):
    """The model's rest position, normal and coverage under each texel of
    the body's colour map (the skin, ``Main``, alone), as float arrays
    (rows bottom up)."""
    import bmesh as bm_
    import numpy as np

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
    out = {}
    for what in ("Position", "Normal", "Cover"):
        mat = bpy.data.materials.new(f"Bake{what}")
        mat.use_nodes = True
        nt = mat.node_tree
        for n in list(nt.nodes):
            nt.nodes.remove(n)
        emit = nt.nodes.new("ShaderNodeEmission")
        res = nt.nodes.new("ShaderNodeOutputMaterial")
        nt.links.new(emit.outputs[0], res.inputs["Surface"])
        if what == "Cover":
            emit.inputs["Color"].default_value = (1, 1, 1, 1)
        else:
            geo = nt.nodes.new("ShaderNodeNewGeometry")
            nt.links.new(geo.outputs[what], emit.inputs["Color"])
        skin_only.data.materials.clear()
        skin_only.data.materials.append(mat)
        img = bpy.data.images.new(f"hare_{what}", size, size, float_buffer=True)
        img.colorspace_settings.name = "Non-Color"
        img.pixels.foreach_set(np.zeros(size * size * 4, np.float32))
        rig.bake_to(skin_only, img, "EMIT", samples=1)
        px = np.empty(size * size * 4, np.float32)
        img.pixels.foreach_get(px)
        out[what] = px.reshape(size, size, 4)[..., :3].copy()
    bpy.data.objects.remove(skin_only)
    return out["Position"], out["Normal"], out["Cover"][..., 0] > 0.5


def _blur(a, r):
    """A box blur of radius ``r`` pixels, twice (about a Gaussian), over the
    first two axes."""
    import numpy as np

    for _ in range(2):
        for axis in (0, 1):
            pad = [(0, 0)] * a.ndim
            pad[axis] = (r + 1, r)
            c = np.cumsum(np.pad(a, pad, mode="edge"), axis=axis)
            hi = np.take(c, np.arange(2 * r + 1, c.shape[axis]), axis=axis)
            lo = np.take(c, np.arange(0, c.shape[axis] - 2 * r - 1), axis=axis)
            a = (hi - lo) / (2 * r + 1)
    return a


def _sample(img, uv):
    """Bilinear samples of ``img`` (rows top down) at pixel points ``uv``."""
    import numpy as np

    h, w = img.shape[:2]
    u = np.clip(uv[:, 0], 0, w - 1.001)
    v = np.clip(uv[:, 1], 0, h - 1.001)
    u0, v0 = np.floor(u).astype(int), np.floor(v).astype(int)
    fu, fv = (u - u0)[:, None], (v - v0)[:, None]
    return (img[v0, u0] * (1 - fu) * (1 - fv) + img[v0, u0 + 1] * fu * (1 - fv)
            + img[v0 + 1, u0] * (1 - fu) * fv + img[v0 + 1, u0 + 1] * fu * fv)


def load_photo(folder, title):
    """The photograph ``title`` (Commons file name) from ``folder``, found
    through its ``sources.tsv`` (local file, File:title, ...), as a linear
    RGB float array, rows top down, scaled to ``PHOTO_PX`` wide."""
    import numpy as np

    for line in open(os.path.join(folder, "sources.tsv"), encoding="utf-8"):
        cols = line.rstrip("\n").split("\t")
        if len(cols) > 1 and cols[1] == f"File:{title}":
            img = bpy.data.images.load(os.path.join(folder, cols[0]))
            if img.size[0] != PHOTO_PX:
                img.scale(PHOTO_PX, round(img.size[1] * PHOTO_PX / img.size[0]))
            w, h = img.size
            px = np.empty(w * h * 4, np.float32)
            img.pixels.foreach_get(px)
            return px.reshape(h, w, 4)[::-1, :, :3].copy()
    raise SystemExit(f"hare: {title} not in {folder}/sources.tsv")


def project_photos(colour, folder):
    """Lays ``PROJECTIONS`` over the body's colour map ``colour`` (a Blender
    image, linear). Returns the number of texels each region touched."""
    import numpy as np

    body = bpy.data.objects["Hare"]
    size = colour.size[0]
    pos, nor, cover = surface_maps(body, size)
    px = np.empty(size * size * 4, np.float32)
    colour.pixels.foreach_get(px)
    px = px.reshape(size, size, 4)
    base = px[..., :3].reshape(-1, 3).copy()
    out = base.copy()
    P, N = pos.reshape(-1, 3), nor.reshape(-1, 3)
    N = N / np.maximum(np.linalg.norm(N, axis=1, keepdims=True), 1e-6)
    ok = cover.ravel()
    side = np.where(P[:, 0] >= 0, 1.0, -1.0)
    facing = N[:, 0] * side
    # The head's own frame (undoing its growth) to keep the ears out.
    c, k, sh = np.array(HEAD_C), HEAD_K, np.array(HEAD_SHIFT) + np.array((0.0, 0.0, HEAD_C[2] * (BODY_KZ - 1.0)))
    q = c + (P - c - sh) / k
    r = q - np.array(HEAD_O)
    t_h = r[:, 1] * HEAD_A[1] + r[:, 2] * HEAD_A[2]
    u_h = r[:, 1] * HEAD_U[1] + r[:, 2] * HEAD_U[2]
    ear = (u_h > 0.014) & (t_h < 0.03)
    # Round the modelled eye the painted ring stays: the photo's own eye
    # never lands on the lids.
    s_h = np.abs(r[:, 0])
    eye_d = np.sqrt((t_h - EYE[0]) ** 2 + (u_h - EYE[1]) ** 2 + (s_h - EYE[2]) ** 2)
    keep_eye = np.clip((eye_d - 1.5 * EYE_RADIUS) / (0.8 * EYE_RADIUS), 0.0, 1.0)
    head = np.clip((t_h + 0.030) / 0.035, 0, 1) * (u_h > -0.09)
    touched = {}
    photos = {}
    for spec in PROJECTIONS:
        if spec["file"] not in photos:
            photos[spec["file"]] = load_photo(folder, spec["file"])
        img = photos[spec["file"]]
        lum = img @ np.array([0.2126, 0.7152, 0.0722], np.float32)
        model = [tuple(warp((0.0, y, z))[1:]) for (y, z), _ in spec["landmarks"]]
        fn = fit_mapping(model, [uv for _, uv in spec["landmarks"]])
        gate = head if spec["region"] == "head" else (1.0 - head) * (P[:, 2] > 0.10)
        live = ok & (gate > 0) & ~ear & (facing > 0.3)
        idx = np.nonzero(live)[0]
        uv = fn(P[idx][:, 1:])
        w = gate[idx] * keep_eye[idx] * np.array([photo_weight(f) for f in facing[idx]]) * spec["strength"]
        d = outline_distance(uv, spec["outline"])
        w *= np.clip(d / spec["feather"], 0.0, 1.0) ** 2 * (3 - 2 * np.clip(d / spec["feather"], 0.0, 1.0))
        if spec["mode"] == "colour":
            wide = _blur(lum, 60)[..., None]
            delit = img / np.maximum(wide, 1e-3) * float(np.median(wide))
            got = _sample(delit, uv)
            sel = w > 0.3
            gain = base[idx][sel].mean(0) / np.maximum(got[sel].mean(0), 1e-4) if sel.any() else np.ones(3)
            out[idx] = out[idx] * (1 - w[:, None]) + got * gain * w[:, None]
        else:
            local = np.log(np.maximum(lum, 1e-4)) - np.log(np.maximum(_blur(lum, 10), 1e-4))
            detail = np.clip(_sample(local[..., None], uv)[:, 0], -0.9, 0.7)
            out[idx] = out[idx] * np.exp(detail * w)[:, None]
        touched[spec["region"]] = int((w > 0.5).sum())
    px[..., :3] = np.clip(out, 0.0, 1.0).reshape(size, size, 3)
    colour.pixels.foreach_set(px.ravel())
    colour.update()
    print(f"hare: photo texels (weight > 0.5) {touched}")
    return touched


def light_mesh(body, arm):
    """The light mesh the game draws far off (``Hare_LOD``): the skin alone
    (no hair cards, no eyes) decimated to ``LIGHT_TRIS`` triangles, with
    its UVs (the same maps dress it) and its weights from the full one."""
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
    light = rig.decimated_copy(skin_only, "Hare_LOD", LIGHT_TRIS)
    bpy.data.objects.remove(skin_only)
    bpy.ops.object.select_all(action="DESELECT")
    light.select_set(True)
    bpy.context.view_layer.objects.active = light
    bpy.ops.object.vertex_group_limit_total(group_select_mode="ALL", limit=4)
    bpy.ops.object.vertex_group_normalize_all(lock_active=False)
    light.parent = arm
    light.modifiers.new("Armature", "ARMATURE").object = arm
    print(f"hare: light mesh {rig.triangles(light)} triangles")
    return light


# ---------------------------------------------------------------------------
# Posing: the body, the spine's flexion, the neck and the ears, the tail,
# and each foot where the clip wants it. The fore legs reach with the
# horse's two-bone solver (``pose_leg``); the hind with ``pose_hind``.

NECK = ("Neck1", "Neck2", "Neck3", "Head")
PIVOT = (0.0, 0.0, 0.19)
HOOF = {"F": dict(toe=warp(TOE_F)[1], heel=warp(BALL_F)[1]), "H": dict(toe=warp(TOE_H)[1], heel=warp(BALL_H)[1])}
LEGS = ("LF", "RF", "LH", "RH")
SHORT = {}


def body_matrix(lift=0.0, nose_down=0.0, roll=0.0, yaw=0.0, forward=0.0, pivot=None):
    return H.body_matrix(lift, nose_down, roll, yaw, forward, Vector(warp(PIVOT)) if pivot is None else Vector(pivot))


def rest_ball(leg):
    x = (FORE_X if leg[1] == "F" else HIND_X) * (1 if leg[0] == "L" else -1)
    b = BALL_F if leg[1] == "F" else BALL_H
    return Vector(warp((x, b[1], b[2])))


def pose_hind(pose, body, leg, ball, meta, toe=0.0, slide=0.0):
    """The hind leg of ``leg`` with its ball at ``ball`` (armature space),
    the metatarsus laid ``meta`` degrees back from upright (0: the hock
    straight over the ball; about 56 standing; 90: flat on the ground, the
    hare sitting), the toes rolled ``toe`` degrees up (negative: down);
    the femur and the tibia reach the hock between them, the stifle
    forward. ``slide`` moves the hip forward along the body."""
    s = leg[0]
    femur, tibia, meta_b = f"BackLeg.{s}", f"BackUpperLeg.{s}", f"BackLowerLeg.{s}"
    end, ik = f"BackLowerLeg.{s}_end", f"IKBackLeg.{s}"
    rb = body.to_3x3()
    fwd = rb @ Vector((0, -1, 0))
    if slide:
        pose.move(f"BackShoulder.{s}", fwd * slide)
    hip = pose.posed(femur).translation.copy()
    l1 = (pose.rest_head(tibia) - pose.rest_head(femur)).length
    l2 = (pose.rest_head(meta_b) - pose.rest_head(tibia)).length
    l3 = (H.END_REST[end] - pose.rest_head(meta_b)).length
    ball = Vector(ball)
    a = math.radians(meta)
    hock = ball + Vector((0.0, math.sin(a), math.cos(a))) * l3
    knee, reach = H.two_bone(hip, hock, l1, l2, fwd)
    pose.aim(femur, H.child_point(pose, femur, tibia), knee)
    pose.aim(tibia, H.child_point(pose, tibia, meta_b), reach)
    pose.aim(meta_b, H.end_point(pose, meta_b, end), ball)
    actual = H.end_point(pose, meta_b, end)
    SHORT[leg] = max(SHORT.get(leg, 0.0), (actual - ball).length)
    m = H.rot(H.X, -toe).to_4x4() @ pose.rest(ik).to_3x3().to_4x4()
    m.translation = actual
    pose.world[ik] = m
    return actual


def front_matrix(pose):
    """The chest's motion (the body's and the spine's flexion together)."""
    return pose.posed("Torso3") @ pose.rest("Torso3").inverted()


def hare_pose(arm, body, feet, flex=0.0, neck=(0, 0, 0, 0), yaw=0.0, ears=((0, 0), (0, 0)), jaw=0.0, tail_lift=0.0,
              tail=(0.0, 0.0), t=0.0, twist=0.0, tuck=0.0):
    """One frame: ``body`` (``body_matrix``) carries the hips; the spine
    rounds ``flex`` degrees (arched positive) over its three joints and
    turns ``twist`` degrees sideways; the pelvis tucks ``tuck`` degrees (the
    rump down and under, the thighs with it; the trunk ahead of it keeps
    its line); the neck bends ``neck`` (nose up
    positive, from rest, in the chest's frame) and turns ``yaw``; the ears
    turn back and out (``fox.py``'s ``pose_face``); ``feet[leg]`` is a dict
    with ``at`` (the ball, armature space) and ``toe`` (and ``meta`` for
    the hinds, ``slide`` and ``sling`` for the fores)."""
    pose = H.Pose(arm)
    pose.carry("Body", body)
    rb = body.to_3x3()
    if tuck:
        pose.turn("Back", H.rot(rb @ H.X, -tuck))
        pose.turn("Torso", H.rot(rb @ H.X, tuck))
    for bone, share in (("Torso", 0.35), ("Torso2", 0.40), ("Torso3", 0.25)):
        if flex:
            pose.turn(bone, H.rot(rb @ H.X, flex * share))
        if twist:
            pose.turn(bone, H.rot(rb @ H.Z, twist * share))
    front = front_matrix(pose)
    H.pose_neck(pose, front, neck, yaw)
    F.pose_face(pose, ears, jaw)
    F.pose_hair(pose, tail=tail, tail_lift=tail_lift, t=t, lag=0.1)
    for leg in LEGS:
        f = feet[leg]
        if leg[1] == "F":
            H.pose_leg(pose, front, leg, Vector(f["at"]), f.get("toe", 0.0), slide=f.get("slide", 0.0), sling=f.get("sling", 0.0))
        else:
            pose_hind(pose, body, leg, f["at"], f.get("meta", 56.0), f.get("toe", 0.0), slide=f.get("slide", 0.0))
    return pose


def placed(leg, dy=0.0, dz=0.0, dx=0.0, **kw):
    """A foot ``dy`` back (``dz`` up) from where it stands at rest."""
    b = rest_ball(leg)
    return dict(at=b + Vector((dx * (1 if leg[0] == "L" else -1), dy, dz)), **kw)


# The sits: the hips low, the hind feet flat under them (``meta`` about
# 85-90 degrees), the fores straight (the alert sit) or forward (the form).
SIT = dict(lift=-0.068, nose_down=-37.0, pivot=(0.0, 0.10, 0.14))
# The alert sit's spine and neck (``idle``): the chest rounded forward
# over the fores and the neck carried forward, the poll bent back so the
# head stays level; the head then sits over the chest, whose front lies
# near the vertical through the ears' bases (Hałun 5163, ``alert_front``).
SIT_ALERT = dict(flex=15.0, tuck=2.0, neck=(-18.0, -16.0, -12.0, 40.0))
SIT_FORE_SLIDE = -0.080  # the alert sit's shoulders drawn back down the chest, the elbows under it (``sit_feet``)
CROUCH = dict(lift=-0.088, nose_down=-4.0, pivot=(0.0, 0.14, 0.12))


def sit_feet(hind_dy=-0.020, fore_dy=0.066, meta=86.0, fore_dz=0.0, fore_toe=0.0, fore_slide=0.0):
    """``fore_slide`` (negative) draws the shoulder blades back down the
    chest, so the elbows tuck under it and the fores rise from beneath."""
    feet = {}
    for leg in LEGS:
        if leg[1] == "H":
            feet[leg] = placed(leg, hind_dy, 0.0, meta=meta, toe=0.0)
        else:
            feet[leg] = placed(leg, fore_dy, fore_dz, toe=fore_toe, sling=1.0, slide=fore_slide)
    return feet


def fores_under(arm, body, feet, ahead=0.0, **kw):
    """The fore feet set straight under the elbows (``ahead`` metres in
    front of them) as ``hare_pose`` with ``feet`` leaves them, so the fore
    legs stand upright under the shoulders."""
    first = hare_pose(arm, body, feet, **kw)
    for leg in ("LF", "RF"):
        elbow = first.posed(f"FrontUpperLeg.{leg[0]}").translation
        r = rest_ball(leg)
        feet[leg] = dict(feet[leg], at=Vector((r.x, elbow.y - ahead, feet[leg]["at"].z)))
    return feet


def idle(arm, t):
    # The alert sit: up on the fores, the head high and level, the ears up
    # and turning to sounds one at a time, the nose twitching.
    body = body_matrix(lift=SIT["lift"] + 0.0012 * H.wave(t, 4), nose_down=SIT["nose_down"], pivot=SIT["pivot"])
    look = H.ease((t - 0.20) / 0.10) * (1 - H.ease((t - 0.62) / 0.12))
    n = SIT_ALERT["neck"]
    neck = (n[0], n[1], n[2], n[3] + 1.0 * H.wave(t, 2, 0.1) + 1.5 * H.wave(t, 18) * H.ease((t - 0.7) / 0.05))
    # Never both alike: the left a little back, the right forward and out,
    # so from the side both read.
    ears = ((14 + 22 * H.flick(t, 0.35, 0.08), 8 + 18 * H.flick(t, 0.35, 0.08)), (-14 + 10 * look, 22 + 25 * H.flick(t, 0.8, 0.07)))
    kw = dict(flex=SIT_ALERT["flex"], tuck=SIT_ALERT["tuck"], neck=neck, yaw=28.0 * look, ears=ears, jaw=1.0 * max(0.0, H.wave(t, 12)), t=t)
    return hare_pose(arm, body, fores_under(arm, body, sit_feet(hind_dy=-0.035, fore_slide=SIT_FORE_SLIDE), 0.0, **kw), **kw)


def idle_2(arm, t):
    # Pressed into its form (the photograph): the body long and low, the back
    # rounded, the hind feet drawn under the belly, the fores under the chest,
    # the head low and forward, the ears laid back along the back; only
    # breathing, an ear lifting a little now and then.
    body = body_matrix(lift=CROUCH["lift"] + 0.0010 * H.wave(t, 3), nose_down=CROUCH["nose_down"], pivot=CROUCH["pivot"])
    feet = sit_feet(hind_dy=-0.050, fore_dy=-0.040, meta=88.0, fore_toe=0.0)  # the fore feet flat on the ground, forward
    for leg in ("LF", "RF"):
        feet[leg]["sling"] = 0.0
    lift = 18.0 * H.ease((t - 0.55) / 0.08) * (1 - H.ease((t - 0.75) / 0.1))
    ears = ((88.0 - lift, -4.0), (88.0, -4.0))
    return hare_pose(arm, body, feet, flex=12.0, tuck=6.0, neck=(-8.0, -6.0, 2.0, 8.0), ears=ears, t=t)


def idle_headlow(arm, t):
    # Sitting up, washing: the fore paws lifted to the face and drawn down
    # over the muzzle together, three strokes; the head nodding into them.
    body = body_matrix(lift=SIT["lift"] + 0.008, nose_down=SIT["nose_down"] - 10.0, pivot=SIT["pivot"])
    wash = H.ease((t - 0.08) / 0.10) * (1 - H.ease((t - 0.82) / 0.10))
    stroke = 0.5 + 0.5 * H.wave(t, 3, 0.25)
    feet = sit_feet()
    neck = (-6.0, -10.0 * wash, -12.0 * wash, -10.0 - 26.0 * wash + 10.0 * stroke * wash)
    ears = ((20.0 * wash, 10.0), (20.0 * wash, 10.0))
    kw = dict(flex=2.0 + 10.0 * wash, neck=neck, ears=ears, jaw=2.0 * wash * max(0.0, H.wave(t, 9)), t=t)
    # Where the muzzle is with the head bowed, then the paws drawn to it.
    first = hare_pose(arm, body, feet, **kw)
    muzzle = first.posed("Head") @ first.rest("Head").inverted() @ H.MUZZLE
    for leg in ("LF", "RF"):
        sx = 1 if leg[0] == "L" else -1
        face = muzzle + Vector((sx * 0.013, 0.022 + 0.010 * stroke, -0.012 + 0.030 * (1 - stroke)))
        feet[leg] = dict(at=feet[leg]["at"].lerp(face, wash), toe=-50.0 * wash, sling=1.0 - wash)
    return hare_pose(arm, body, feet, **kw)


def eating(arm, t):
    # Crouched, grazing: the head down to the grass, nibbling (the jaw
    # working fast), lifting to chew and look, then down again; the ears up.
    down = H.ease((t - 0.04) / 0.10) * (1 - H.ease((t - 0.84) / 0.12))
    up = H.ease((t - 0.45) / 0.05) * (1 - H.ease((t - 0.60) / 0.05))
    body = body_matrix(lift=CROUCH["lift"] + 0.020, nose_down=CROUCH["nose_down"] + 6.0 * down, pivot=CROUCH["pivot"])
    feet = sit_feet(hind_dy=-0.050, fore_dy=0.010, meta=84.0)
    bend = down * (1.0 - 0.6 * up)
    neck = (-18.0 * bend, -14.0 * bend, -10.0 * bend, -10.0 * bend + 2.0 * H.wave(t, 6))
    jaw = 7.0 * max(0.0, H.wave(t, 30)) * (1.0 - 0.3 * up)
    ears = ((10 + 15 * H.flick(t, 0.3), 12), (10, 12 + 25 * H.flick(t, 0.7)))
    return hare_pose(arm, body, feet, flex=10.0, neck=neck, yaw=8.0 * H.wave(t, 2) * down, ears=ears, jaw=jaw, t=t)


def gait_feet(plan, q, speed, meta_plan, toe_plan):
    """Each foot at phase ``q`` of ``plan``: standing, where it landed,
    sliding back with the ground; swinging, from where it lifted to where
    it lands next, lifted and folded. ``meta_plan(stand, s)`` gives a hind
    metatarsus' angle (``stand`` True and ``s`` 0..1 through the stance,
    or through the swing); ``toe_plan`` the same for the toes' roll."""
    feet = {}
    for leg in LEGS:
        k = 0 if leg[1] == "F" else 1
        duty, land, lift = plan["duty"][k], plan["land"][k], plan["lift"][k]
        sw = sweep(plan, leg, speed)
        r = rest_ball(leg)
        s = (q - plan["phase"][leg]) % 1.0
        if s < duty:
            f = s / duty
            y, z, stand = land + sw * f, 0.0, True
        else:
            f = (s - duty) / (1.0 - duty)
            y = land + sw - sw * H.ease(f ** 0.9)
            z = lift * math.sin(math.pi * f) ** 0.8
            stand = False
        foot = dict(at=Vector((r.x, y, r.z + z)), toe=toe_plan(leg, stand, f))
        if leg[1] == "H":
            foot["meta"] = meta_plan(stand, f)
        feet[leg] = foot
    return feet


def walk(arm, t):
    # The slow hop: the fores land one after the other, the hinds come up
    # together beside them with the back arching, then the fores reach out
    # again as the back lengthens. Ears up, the head steady.
    plan = HOP
    q = t

    def meta(stand, f):
        return 70.0 - 40.0 * H.ease(f) if stand else 30.0 + 50.0 * math.sin(math.pi * min(1.0, f * 1.2))

    def toe(leg, stand, f):
        if leg[1] == "F":
            return -25.0 * H.ease((f - 0.6) / 0.4) if stand else -30.0 * math.sin(math.pi * f)
        return -30.0 * H.ease((f - 0.6) / 0.4) if stand else -40.0 * math.sin(math.pi * f)

    feet = gait_feet(plan, q, HOP_SPEED, meta, toe)
    arch = 10.0 + 10.0 * math.cos(2 * math.pi * (q - 0.55))
    body = body_matrix(lift=-0.012 + 0.012 * math.cos(2 * math.pi * (q - 0.85)),
                       nose_down=4.0 * math.cos(2 * math.pi * (q - 0.35)) - 0.5 * (arch - 10.0))
    neck = (-2.0, -2.0, 0.0, 2.0 - 3.0 * math.cos(2 * math.pi * (q - 0.35)))
    ears = ((12.0, 10.0), (12.0, 10.0))
    return hare_pose(arm, body, feet, flex=arch, neck=neck, ears=ears, tail_lift=6.0, t=t, tuck=0.6 * (arch - 10.0))


def gallop(arm, t):
    # The flight: the fores land one after the other with the back rounded
    # and the hinds swinging forward past them; the hinds land far ahead of
    # the fores' prints and drive, the back extending into a long flight.
    # The head stretched forward, the ears swept back, the tail down.
    plan = FLIGHT
    q = t

    def meta(stand, f):
        # Standing: from the landing (the hock well behind the ball) to the
        # push-off (heel up); swinging: folded up under the thigh, then
        # reaching forward to land.
        return 58.0 - 43.0 * H.ease(f) if stand else 15.0 + 43.0 * f + 65.0 * math.sin(math.pi * f) ** 1.2

    def toe(leg, stand, f):
        if leg[1] == "F":
            return 10.0 * (1 - H.ease(f / 0.2)) - 50.0 * H.ease((f - 0.5) / 0.5) if stand else -60.0 * math.sin(math.pi * f) ** 0.7
        return -45.0 * H.ease((f - 0.4) / 0.6) if stand else -55.0 * math.sin(math.pi * f) ** 0.7

    feet = gait_feet(plan, q, FLIGHT_SPEED, meta, toe)
    flex = 6.0 + 15.0 * math.cos(2 * math.pi * (q - 0.26))
    body = body_matrix(lift=-0.018 + 0.016 * math.cos(2 * math.pi * (q - 0.78)),
                       nose_down=5.0 * math.cos(2 * math.pi * (q - 0.14)) - 0.75 * (flex - 6.0),
                       forward=0.020 * math.cos(2 * math.pi * (q - 0.26)))
    lift_head = 0.35 * (flex - 6.0)
    neck = (-12.0, -8.0 + lift_head, -4.0, 10.0 + lift_head - 3.0 * math.cos(2 * math.pi * (q - 0.2)))
    # Swept back but raised off the neck, so the flight still reads as a
    # hare's at a distance (v6c laid them flat at 62 degrees).
    ears = ((34.0, 6.0), (34.0, 6.0))
    return hare_pose(arm, body, feet, flex=flex, neck=neck, ears=ears, tail_lift=-6.0, t=t, tuck=0.9 * (flex - 6.0))


LEAP_SECONDS = 40 / 48


def gallop_jump(arm, t):
    # One long escape bound from the flight: gathered on the hinds, a high
    # flight with the body stretched and the legs trailing, the fores
    # reaching for the landing, then gathered again.
    up = H.ease(t / 0.25) * (1 - H.ease((t - 0.70) / 0.25))
    stretch = math.sin(math.pi * max(0.0, min(1.0, (t - 0.15) / 0.65)))
    feet = {}
    for leg in LEGS:
        r = rest_ball(leg)
        if leg[1] == "F":
            y = r.y - 0.12 * stretch - 0.06 * H.ease((t - 0.6) / 0.2)
            feet[leg] = dict(at=Vector((r.x, y, r.z + 0.20 * up)), toe=-50.0 * stretch)
        else:
            y = r.y + 0.22 * stretch - 0.10 * (1 - up)
            feet[leg] = dict(at=Vector((r.x, y, r.z + 0.20 * up + 0.08 * stretch)), toe=-60.0 * stretch, meta=20.0 + 50.0 * (1 - stretch))
    body = body_matrix(lift=0.20 * up, nose_down=-12.0 * H.wave(t, 1, 0.0) * up)
    return hare_pose(arm, body, feet, flex=-14.0 * stretch + 16.0 * (1 - up), neck=(-14.0, -6.0, 0.0, 8.0), ears=((70.0, 4.0), (70.0, 4.0)),
                     tail_lift=-4.0, t=t)


def stance_speed(arm, action, seconds, grip=0.001):
    """The speed the feet slide back at while they stand, m/s: between
    keys where an IK foot stays within ``grip`` (1 mm) of its lowest point.
    ``horse.py``'s ``measure`` takes 1.5 cm, which at the hare's size also
    counts the first and last moments of the swing (the foot moving
    forward, near the ground) and so reads low."""
    scene = bpy.context.scene
    arm.animation_data_create()
    arm.animation_data.action = action
    frames = round(seconds * rig.FPS)
    track = {leg: [] for leg in LEGS}
    for f in range(frames + 1):
        scene.frame_set(f)
        for leg in LEGS:
            pb = arm.pose.bones[f"IK{'Front' if leg[1] == 'F' else 'Back'}Leg.{leg[0]}"]
            track[leg].append((arm.matrix_world @ pb.head).copy())
    arm.animation_data.action = None
    speeds = []
    for pts in track.values():
        low = min(p.z for p in pts)
        speeds += [(b.y - a.y) * rig.FPS for a, b in zip(pts, pts[1:]) if a.z < low + grip and b.z < low + grip]
    return sum(speeds) / len(speeds) if speeds else 0.0


CLIP_FPS = 96  # keys a second: the gallop's feet stand about a tenth of its 0.25 s, two or three keys


def clips(arm):
    rig.FPS = CLIP_FPS
    bpy.context.scene.render.fps = CLIP_FPS
    made = [
        rig.bake_clip(arm, "Idle", 192 / 48, idle, H.MOVED),
        rig.bake_clip(arm, "Idle_2", 192 / 48, idle_2, H.MOVED),
        rig.bake_clip(arm, "Idle_Headlow", 192 / 48, idle_headlow, H.MOVED),
        rig.bake_clip(arm, "Eating", 288 / 48, eating, H.MOVED),
    ]
    SHORT.clear()
    made.append(rig.bake_clip(arm, "Walk", HOP["seconds"], walk, H.MOVED))
    print(f"hare: in the hop the hind balls fall short of their marks by at most { {k: round(v, 4) for k, v in sorted(SHORT.items())} } m")
    SHORT.clear()
    made.append(rig.bake_clip(arm, "Gallop", FLIGHT["seconds"], gallop, H.MOVED))
    print(f"hare: in the flight the hind balls fall short of their marks by at most { {k: round(v, 4) for k, v in sorted(SHORT.items())} } m")
    made.append(rig.bake_clip(arm, "Gallop_Jump", LEAP_SECONDS, gallop_jump, H.MOVED))
    for a, seconds in ((made[4], HOP["seconds"]), (made[5], FLIGHT["seconds"])):
        speed, lows = H.measure(arm, a, seconds)
        print(f"hare: {a.name} {seconds:.3f} s: standing feet slide back at {stance_speed(arm, a, seconds):.2f} m/s "
              f"(horse.py's measure: {speed:.2f}); lowest IK bones {[round(z, 4) for z in lows]}")
    return made


# ---------------------------------------------------------------------------
# Wiring the fox's steps (and through them the horse's) to the hare.

# The shipped colour maps (the body's and the hair atlas') are lifted this
# much in linear light. v6c's coat, painted on the photograph's raw (lit)
# colours, read almost black in the game even lifted 1.6 (L142); the coat
# is now painted at the photograph's de-lit albedo (``COAT_ALBEDO``), so
# the maps ship as baked.
GAME_LIFT = 1.0


def lift_colour(path, k):
    """Multiplies the JPEG at ``path`` by ``k`` in linear light (clipped)
    and saves it back in place."""
    import numpy as np

    img = bpy.data.images.load(path)
    w, h = img.size
    px = np.empty(w * h * 4, np.float32)
    img.pixels.foreach_get(px)
    px = px.reshape(-1, 4)
    c = px[:, :3]
    lin = np.where(c <= 0.04045, c / 12.92, ((c + 0.055) / 1.055) ** 2.4) * k
    lin = np.clip(lin, 0.0, 1.0)
    px[:, :3] = np.where(lin <= 0.0031308, lin * 12.92, 1.055 * lin ** (1 / 2.4) - 0.055)
    img.pixels.foreach_set(px.ravel())
    img.update()
    # ``Image.save`` writes the buffer as it is (``save_render`` would pass
    # it through the scene's view transform a second time).
    img.save(filepath=path, quality=92)
    bpy.data.images.remove(img)


BAKE_PX = 2048  # the body maps' bake; the game ships them Lanczos-downscaled to 1024, like the raptors' (tools/README.md)


def _bake_at(steps, px):
    """``horse.py``'s ``build`` in ``steps`` (its namespace), recompiled from
    its source with the body's maps at ``px`` instead of its fixed 1024. The
    photo projection blurs in photo pixels, so it does not depend on the
    size; the hair atlas stays at 1024."""
    import inspect

    source = inspect.getsource(steps.build)
    fixed = "    size = 1024\n"
    if source.count(fixed) != 1:
        raise SystemExit("hare: horse.py's build no longer sets its maps' size in one place")
    code = compile(source.replace(fixed, f"    size = {int(px)}\n"), inspect.getsourcefile(steps.build), "exec")
    exec(code, steps._ns)


if bpy is not None:
    _HARE = dict(globals())
    _SKIP = {"bpy", "rig", "F", "H", "math", "os", "re", "sys", "Matrix", "Quaternion", "Vector", "HERE", "_jit", "body_matrix",
             "clips", "main", "parse", "shots_for", "SIDE", "NECK", "PIVOT", "HOOF", "gallop", "walk", "idle", "idle_2",
             "idle_headlow", "eating", "gallop_jump", "SHORT", "skin", "LEGS", "GRAZE_SHAPE", "light_mesh", "at", "head_matrix", "rot_x"}
    F.use(**{k: v for k, v in _HARE.items() if k in F._ns and k not in _SKIP and not k.startswith("__")})
    F.use(PIVOT=PIVOT, at=at, head_matrix=head_matrix)
    H.use(
        QUATERNIUS=[], ENDS=[(n, b, warp(p)) for n, b, p in ENDS], EXTRA=EXTRA, MUSCLES=[], MUSCLE_NAMES=[], muscle_shape=lambda pose, arm: {},
        HEAD_O=Vector(HEAD_O), HEAD_A=Vector(HEAD_A), HEAD_U=Vector(HEAD_U), EYE=EYE, EYE_RADIUS=EYE_RADIUS,
        bone_table=F.bone_table, sculpt=sculpt, paint_sculpt=F.paint_sculpt, material_of=material_of, MATERIALS=MATERIALS,
        FALLBACK=FALLBACK, dense_zone=lambda p: dense_zone(unwarp(p)), BODY_TRIS=4600, eyes=eyes, skin=skin, NOT_SKIN=NOT_SKIN,
        TAIL_CHAIN=TAIL_CHAIN, FOLDS=FOLDS,
        mane_spots=F.warped(fur_spots), forelock_spots=lambda sdf: [], lash_spots=F.warped(whisker_spots),
        ear_hair_spots=F.warped(ear_tufts), tail_spots=F.warped(tail_spots),
        COATS={"liebre": {}}, color_at=lambda coat, p, n: (255, 255, 255), coat_part=lambda coat, part: (255, 255, 255),
        NECK=NECK, MUZZLE=Vector(warp(_at(NOSE_T - 0.004, -0.040))), PIVOT=Vector(warp(PIVOT)),
        HOOF=HOOF, END_REST={name: Vector(warp(p)) for name, _, p in ENDS}, body_matrix=F.body_matrix,
        pose_hair=F.pose_hair, pose_face=F.pose_face, weight_hair=weight_hair,
    )
    _bake_at(H, BAKE_PX)


# ---------------------------------------------------------------------------
# Previews and the command line.


def parse(argv):
    import argparse

    parser = argparse.ArgumentParser(prog="hare.py")
    parser.add_argument("output")
    parser.add_argument("--preview", default=None, metavar="DIR")
    parser.add_argument("--textures", default=None, metavar="DIR")
    parser.add_argument("--draft", action="store_true")
    parser.add_argument("--no-clips", action="store_true")
    parser.add_argument("--shots", default=None, metavar="REGEX")
    parser.add_argument("--photos", default=None, metavar="DIR")
    parser.add_argument("--silhouettes", default=None, metavar="DIR")
    parser.add_argument("--clay", action="store_true")
    return parser.parse_args(argv)


SIDE = ((1.52, -0.30, 0.23), (0.0, -0.02, 0.19), 50)  # a little ahead of square, so both ears read
GALLOP_SIDE = ((1.75, -0.05, 0.20), (0.0, -0.05, 0.16), 45)


def shots_for(actions):
    shots = [
        ("side.png", "liebre", None, 0, *SIDE),
        ("side_right.png", "liebre", None, 0, (-1.55, -0.02, 0.22), (0.0, -0.02, 0.19), 50),
        ("front34.png", "liebre", None, 0, (0.85, -1.05, 0.38), (0.0, -0.08, 0.20), 45),
        ("front.png", "liebre", None, 0, (0.0, -1.35, 0.28), (0.0, -0.10, 0.22), 50),
        ("rear34.png", "liebre", None, 0, (0.80, 0.95, 0.40), (0.0, 0.08, 0.18), 45),
        ("close_head.png", "liebre", None, 0, (0.34, -0.66, 0.47), (0.0, -0.25, 0.37), 60),
        ("close_head_side.png", "liebre", None, 0, (0.62, -0.26, 0.40), (0.0, -0.26, 0.38), 60),
        ("close_head_front.png", "liebre", None, 0, (0.03, -0.84, 0.42), (0.0, -0.25, 0.38), 70),
        ("close_eye.png", "liebre", None, 0, (0.24, -0.40, 0.40), (0.02, -0.236, 0.39), 85),
        ("close_coat.png", "liebre", None, 0, (0.40, -0.02, 0.32), (0.04, 0.02, 0.20), 60),
        ("close_tail.png", "liebre", None, 0, (0.40, 0.62, 0.28), (0.0, 0.25, 0.14), 55),
        ("close_feet.png", "liebre", None, 0, (0.40, -0.40, 0.10), (0.03, 0.0, 0.04), 50),
        ("mid_distance.png", "liebre", None, 0, (5.0, -3.5, 1.4), (0.0, 0.0, 0.18), 50),
    ]
    by_name = {a.name: a for a in actions}
    frames = {
        "Idle": ((0, 80, 140), SIDE),
        "Idle_2": ((0, 120), SIDE),
        "Idle_Headlow": ((0, 40, 70, 100), SIDE),
        "Eating": ((0, 60, 150, 240), SIDE),
        "Walk": (tuple(range(0, 20, 2)), SIDE),
        "Gallop": (tuple(range(0, 12)), GALLOP_SIDE),
        "Gallop_Jump": (tuple(range(0, 40, 4)), GALLOP_SIDE),
    }
    for name, (fs, cam) in frames.items():
        if name in by_name:
            for f in fs:
                # Named by the frame at 48 a second (the v1-v4 sheets'), keyed at ``CLIP_FPS``.
                shots.append((f"{name.lower()}_{f:03}.png", "liebre", by_name[name], f * CLIP_FPS // 48, *cam))
    for name, f, file in (("Idle", 0, "idle_front34.png"), ("Idle_2", 0, "idle_2_front34.png"), ("Gallop", 3, "gallop_front34.png")):
        if name in by_name:
            shots.append((file, "liebre", by_name[name], f * CLIP_FPS // 48, (0.85, -1.05, 0.38), (0.0, -0.08, 0.18), 45))
    if "Idle" in by_name:  # the alert sit from behind the side, the rump and the thigh against the light
        shots.append(("idle_rear34.png", "liebre", by_name["Idle"], 0, (0.95, 0.85, 0.42), (0.0, 0.02, 0.20), 45))
    return shots


def clay(images):
    """``--clay``: grey clay for the previews from now on, no coat and no
    hair cards."""
    grey = bpy.data.materials.new("Clay")
    grey.use_nodes = True
    bsdf = grey.node_tree.nodes["Principled BSDF"]
    bsdf.inputs["Base Color"].default_value = (0.42, 0.42, 0.42, 1.0)
    bsdf.inputs["Roughness"].default_value = 0.65
    none = bpy.data.materials.new("ClayHidden")
    none.use_nodes = True
    none.node_tree.nodes["Principled BSDF"].inputs["Alpha"].default_value = 0.0
    H.paint_coat = lambda mesh, coat, layer="Coat": None
    H.coat_materials = lambda coat, imgs: {n: (none if n == "Hair" else grey) for n in MATERIALS}


# The orthographic side views compared with the reference photographs'
# outlines (``--silhouettes``): name, the pose (None: the rest pose,
# standing), and the camera's turn from square to the hare's left side
# toward its tail, in degrees (the alert-sit photo is taken from behind
# the side).
SILHOUETTES = [("stand", None, 0.0), ("sit", "idle", 30.0), ("form", "idle_2", 0.0)]


def silhouettes(out_dir, arm, mesh, images):
    """Renders ``SILHOUETTES`` orthographic, the background transparent
    (the alpha is the outline), 1000 px for 0.8 m, from the left."""
    os.makedirs(out_dir, exist_ok=True)
    scene = bpy.context.scene
    props, cam = H.stage()
    props[0].hide_render = True  # no ground
    scene.render.film_transparent = True
    scene.render.image_settings.color_mode = "RGBA"
    scene.render.resolution_x = scene.render.resolution_y = 1000
    cam.data.type = "ORTHO"
    cam.data.ortho_scale = 0.8
    exported = list(mesh.data.materials)
    H.paint_coat(mesh, "liebre")
    H.use_materials(mesh, H.coat_materials("liebre", images))
    poses = {"idle": idle, "idle_2": idle_2}
    for name, pose, turn in SILHOUETTES:
        arm.animation_data_create()
        if pose is None:
            arm.animation_data.action = None
            for pb in arm.pose.bones:
                pb.rotation_quaternion = Quaternion()
                pb.location = (0, 0, 0)
        else:
            arm.animation_data.action = rig.bake_clip(arm, f"Sil_{name}", 2 / rig.FPS, poses[pose], H.MOVED)
            arm.animation_data.action = bpy.data.actions[f"Sil_{name}"]
        scene.frame_set(0)
        a = math.radians(turn)
        target = Vector((0.0, -0.02, 0.22))
        cam.location = target + Vector((math.cos(a), math.sin(a), 0.0)) * 2.0
        rig.look_at(cam, target)
        scene.render.filepath = os.path.join(out_dir, f"sil_{name}.png")
        bpy.ops.render.render(write_still=True)
    arm.animation_data.action = None
    for a in [a for a in bpy.data.actions if a.name.startswith("Sil_")]:
        bpy.data.actions.remove(a)
    for i, mat in enumerate(exported):
        mesh.data.materials[i] = mat
    for o in props:
        bpy.data.objects.remove(o)
    scene.render.film_transparent = False
    scene.render.image_settings.color_mode = "RGB"


def main():
    argv = sys.argv[sys.argv.index("--") + 1 :] if "--" in sys.argv else []
    args = parse(argv)
    mesh, arm, images = H.build(args.draft, None)
    mesh.name = mesh.data.name = "Hare"
    paint_eyes(mesh)
    F.use_fur_atlas(images)
    if args.photos:
        project_photos(images["colour"], args.photos)
    if args.textures:
        for folder, maps in (
            ("hare_body", (("color", images["colour"]), ("normal_gl", images["normal"]), ("roughness", images["rough"]),
                           ("ao", bpy.data.images["horse_body_ao"]))),
            ("hare_hair", (("color", images["hair"]["color"]), ("normal_gl", images["hair"]["normal"]),
                           ("roughness", images["hair"]["rough"]), ("opacity", images["hair"]["opacity"]))),
        ):
            for name, img in maps:
                rig.save_jpeg(img, os.path.join(args.textures, folder, f"{name}.jpg"))
            if GAME_LIFT != 1.0:
                lift_colour(os.path.join(args.textures, folder, "color.jpg"), GAME_LIFT)
    if args.silhouettes:
        rig.FPS = CLIP_FPS
        silhouettes(args.silhouettes, arm, mesh, images)
    actions = [] if args.no_clips else clips(arm)
    if args.preview:
        shots = shots_for(actions)
        if args.shots:
            shots = [s for s in shots if re.search(args.shots, s[0])]
        H.render_shots(args.preview, arm, mesh, images, [s for s in shots if s[2] is None])
        H.render_shots(args.preview, arm, mesh, images, [s for s in shots if s[2] is not None])
        if args.clay:
            coat = H.paint_coat, H.coat_materials
            clay(images)
            shots = [("clay_" + s[0],) + s[1:] for s in shots]
            H.render_shots(args.preview, arm, mesh, images, [s for s in shots if s[2] is None])
            H.render_shots(args.preview, arm, mesh, images, [s for s in shots if s[2] is not None])
            H.paint_coat, H.coat_materials = coat
    light = light_mesh(mesh, arm)
    for o in (mesh, light):
        for a in list(o.data.color_attributes):
            o.data.color_attributes.remove(a)
    paint_eyes(mesh)  # the eyes' paint, the only vertex colours exported
    keep = set(MATERIALS)
    for mat in list(bpy.data.materials):
        if mat.name not in keep:
            bpy.data.materials.remove(mat)
    print(f"hare: {rig.triangles(mesh)} triangles, {len(arm.data.bones)} joints")
    rig.export(args.output, actions)


if bpy is not None and __name__ == "__main__":
    main()
