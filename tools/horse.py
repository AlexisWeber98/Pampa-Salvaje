"""Model, rig, paint and animate the criollo horse in Blender, by script.

Run it headless:

    blender -b --factory-startup --python tools/horse.py -- OUT.glb \
        [--textures DIR] [--preview DIR] [--reference GLB] [--draft] [--no-clips]

Writes OUT.glb (11,944 triangles: a 7,200-triangle body, 4,296 of hair
cards, 448 of eyes): one skinned mesh ``Horse`` with the Quaternius horse's
materials (``Main``, ``Hair``, ``Main_Dark``, ``Muzzle``, ``Hooves``,
``Main_Light``, ``Eye_Black``, ``Eye_White``); its armature
``AnimalArmature`` copies the Quaternius rig (the same 50 joint names,
parents and rest axes, and the same ``_end`` leaf nodes) and adds bones for
the mane, the forelock, a longer tail and the face, and six muscle helpers
(``MUSCLES``: the quarters, the gaskins and the shoulders) that every clip
thickens and shifts with the flexion of their joints (``muscle_shape``);
and the clips ``Idle``,
``Idle_2``, ``Idle_Headlow``, ``Eating``, ``Walk``, ``Gallop`` and
``Gallop_Jump`` with the original's lengths (the exporter sorts them by
name: ``tools/glbtrim.py --keep`` puts them back in this order). Life size:
the withers at 1.5 m, the hooves at the origin, facing -Y (glTF +Z).

``Main`` is the whole coat: the game paints it with vertex colours per coat
(``Coat::color_at``), so the GLB has none; its maps (``--textures DIR``:
``DIR/horse_body``, colour, normal_gl, roughness, ao) carry only the hair:
a neutral luminance to multiply with the coat, the baked relief of the
sculpt and its short hair, and the occlusion. ``Hair`` is the mane, the
forelock and the tail: alpha-tested cards on a hair atlas
(``DIR/horse_hair``, colour, normal_gl, roughness, opacity), its colour
also neutral (the game tints it with ``Coat::mane``).
``tools/textures_to_ktx2.sh`` turns both sets into KTX2.

``--preview DIR`` renders PNGs (side, three-quarter front and rear,
close-ups at 2 to 3 m, frames of every clip), painted with four coats
reproduced from ``Coat::color_at`` (bayo, tordillo, overo, zaino);
``--reference GLB`` renders the Quaternius horse with the same coats and
cameras next to them (``ref_*.png``) and compares the tack's rays on both.
``--draft`` uses coarser voxels, to iterate on the shape. Helpers live in
``tools/animal_rig.py``. See tools/README.md.

Anatomy, in metres: a criollo, compact and deep-chested, short-backed,
with strong quarters, a crested neck and a straight head; short strong
legs with flat cannons, defined joints, chestnuts and ergots, and round
hooves with a flat sole. The skeleton is the Quaternius horse's at
1.5 / 3.6 scale, and the body is shaped around it so the game's tack rays
(``crates/render/src/character/tack.rs``) land on it as on the original.
"""

import math
import os
import sys

import bpy
from mathutils import Matrix, Quaternion, Vector

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import animal_rig as rig  # noqa: E402

# The Quaternius horse is 3.6 units at the withers; the game draws it at
# 1.5 m.
SCALE = 1.5 / 3.6

# The Quaternius rig at that scale: name, parent, head (Blender, metres,
# facing -Y; its bind pose, which is symmetric) and rest orientation in
# armature space (w, x, y, z; its node transforms, which the game animates
# from), from assets/models/fauna/quaternius/horse.glb. The game looks bones up by these names and turns them in their
# own axes (``gait.rs``, ``tack.rs``, ``rider.rs``: the ``Torso`` +Y to the
# front, +X to the horse's right).
QUATERNIUS = [
    ("Body", None, (0.0000, -0.0007, 0.3800), (0.70721, 0.70700, 0.00000, 0.00000)),
    ("Back", "Body", (-0.0000, 0.5974, 1.3084), (0.18940, -0.98190, -0.00000, 0.00000)),
    ("Torso", "Back", (0.0000, 0.2991, 1.1888), (0.00000, -0.00000, 0.06502, -0.99788)),
    ("Torso2", "Torso", (0.0000, -0.0385, 1.1445), (0.00000, -0.00002, 0.00471, -0.99999)),
    ("Torso3", "Torso2", (-0.0000, -0.4261, 1.1408), (0.00000, 0.00000, 0.16095, 0.98696)),
    ("Neck1", "Torso3", (-0.0000, -0.6196, 1.2056), (0.00528, 0.00227, 0.42010, 0.90746)),
    ("Neck2", "Neck1", (-0.0000, -0.7320, 1.3378), (0.00888, 0.00136, 0.43406, 0.90084)),
    ("Neck3", "Neck2", (-0.0000, -0.8894, 1.5347), (0.00888, 0.00137, 0.43578, 0.90001)),
    ("Head", "Neck3", (-0.0000, -1.0527, 1.7405), (0.01364, -0.01339, -0.29688, 0.95472)),
    ("Ear1.L", "Head", (0.0739, -1.0568, 1.7878), (0.00653, -0.02741, -0.66698, -0.74454)),
    ("Ear2.L", "Ear1.L", (0.0781, -1.0650, 1.8496), (0.51359, 0.45376, -0.40498, -0.60524)),
    ("Ear3.L", "Ear2.L", (0.0901, -1.0719, 1.8910), (0.43682, 0.52168, -0.40158, -0.61300)),
    ("Ear4.L", "Ear3.L", (0.0973, -1.0878, 1.9383), (0.44690, 0.50568, -0.39080, -0.62597)),
    ("Ear1.R", "Head", (-0.0739, -1.0568, 1.7878), (0.04349, -0.01773, 0.66731, 0.74330)),
    ("Ear2.R", "Ear1.R", (-0.0781, -1.0650, 1.8496), (0.54087, 0.45663, 0.38028, 0.59527)),
    ("Ear3.R", "Ear2.R", (-0.0901, -1.0719, 1.8910), (0.46435, 0.52424, 0.37592, 0.60683)),
    ("Ear4.R", "Ear3.R", (-0.0973, -1.0878, 1.9383), (0.47469, 0.50766, 0.36552, 0.61916)),
    ("FrontShoulder.L", "Torso2", (0.0923, -0.5660, 0.9739), (0.53171, 0.53156, 0.46616, -0.46629)),
    ("FrontUpperLeg.L", "FrontShoulder.L", (0.1748, -0.5660, 0.9848), (0.69184, -0.72205, -0.00000, 0.00000)),
    ("FrontLowerLeg.L", "FrontUpperLeg.L", (0.1748, -0.5863, 0.4910), (0.00000, -0.00000, 0.74887, -0.66272)),
    ("FrontShoulder.R", "Torso2", (-0.0923, -0.5660, 0.9739), (0.53171, 0.53156, -0.46616, 0.46629)),
    ("FrontUpperLeg.R", "FrontShoulder.R", (-0.1748, -0.5660, 0.9848), (0.69184, -0.72205, -0.00000, 0.00000)),
    ("FrontLowerLeg.R", "FrontUpperLeg.R", (-0.1748, -0.5863, 0.4910), (0.00000, -0.00000, 0.74887, -0.66271)),
    ("BackShoulder.L", "Back", (0.0370, 0.5178, 1.2272), (0.52338, 0.51688, 0.48156, -0.47645)),
    ("BackLeg.L", "BackShoulder.L", (0.1746, 0.5178, 1.2381), (0.56822, -0.82286, -0.00564, 0.00208)),
    ("BackUpperLeg.L", "BackLeg.L", (0.1746, 0.3971, 0.9175), (0.92558, -0.37849, -0.00347, 0.00496)),
    ("BackLowerLeg.L", "BackUpperLeg.L", (0.1746, 0.7697, 0.5522), (0.00510, -0.00322, 0.69721, -0.71684)),
    ("BackShoulder.R", "Back", (-0.0370, 0.5178, 1.2272), (0.52338, 0.51689, -0.48156, 0.47645)),
    ("BackLeg.R", "BackShoulder.R", (-0.1746, 0.5178, 1.2381), (0.56822, -0.82286, 0.00564, -0.00208)),
    ("BackUpperLeg.R", "BackLeg.R", (-0.1746, 0.3971, 0.9175), (0.92559, -0.37849, 0.00347, -0.00496)),
    ("BackLowerLeg.R", "BackUpperLeg.R", (-0.1746, 0.7697, 0.5522), (0.00510, -0.00322, -0.69722, 0.71683)),
    ("Tail1", "Back", (-0.0000, 0.5974, 1.3084), (0.96819, 0.25023, -0.00000, -0.00000)),
    ("Tail2", "Tail1", (-0.0000, 0.7104, 1.3711), (0.94996, -0.31238, -0.00000, 0.00000)),
    ("Tail3", "Tail2", (0.0000, 0.8022, 1.3034), (0.81238, -0.58313, -0.00000, -0.00000)),
    ("Tail4", "Tail3", (0.0000, 0.8592, 1.1349), (0.68062, -0.73203, -0.02040, -0.02193)),
    ("Tail5", "Tail4", (0.0000, 0.8498, 1.0053), (0.69474, -0.71864, -0.02082, -0.02153)),
    ("Tail6", "Tail5", (0.0000, 0.8450, 0.8613), (0.74230, -0.66940, -0.02225, -0.02005)),
    ("Tail7", "Tail6", (0.0000, 0.8624, 0.6938), (0.86759, -0.49638, -0.02600, -0.01487)),
    ("PoleTargetBack.L", "Body", (0.1746, -1.5786, 0.3754), (0.70721, 0.70700, 0.00000, 0.00000)),
    ("PoleTarget.L", "Body", (0.1748, -1.3469, 0.3770), (0.70721, 0.70700, 0.00000, 0.00000)),
    ("PoleTargetBack.R", "Body", (-0.1746, -1.5786, 0.3754), (0.70721, 0.70700, -0.00000, -0.00000)),
    ("PoleTarget.R", "Body", (-0.1748, -1.3469, 0.3770), (0.70721, 0.70700, 0.00000, 0.00000)),
    ("IKBackLeg.L", None, (0.1746, 0.7597, 0.1640), (0.56251, -0.82679, -0.00000, 0.00000)),
    ("FFB.L", "IKBackLeg.L", (0.1746, 0.7185, 0.0597), (0.00000, -0.00000, 0.57893, -0.81538)),
    ("IKFrontLeg.L", None, (0.1748, -0.5470, 0.1640), (0.56251, -0.82679, -0.00000, 0.00000)),
    ("FF.L", "IKFrontLeg.L", (0.1748, -0.5882, 0.0597), (0.00000, -0.00000, 0.57893, -0.81538)),
    ("IKBackLeg.R", None, (-0.1746, 0.7597, 0.1640), (0.56251, -0.82679, 0.00000, -0.00000)),
    ("FFB.R", "IKBackLeg.R", (-0.1746, 0.7185, 0.0597), (0.00000, -0.00000, -0.57893, 0.81538)),
    ("IKFrontLeg.R", None, (-0.1748, -0.5470, 0.1640), (0.56251, -0.82679, 0.00000, -0.00000)),
    ("FF.R", "IKFrontLeg.R", (-0.1748, -0.5882, 0.0597), (0.00000, -0.00000, -0.57893, 0.81538)),
]

# The leaf nodes of the original (not joints): name, bone, position.
ENDS = [
    ("Ear4.L_end", "Ear4.L", (0.1093, -1.1074, 1.9970)),
    ("Ear4.R_end", "Ear4.R", (-0.1093, -1.1074, 1.9970)),
    ("FrontLowerLeg.L_end", "FrontLowerLeg.L", (0.1748, -0.5470, 0.1640)),
    ("FrontLowerLeg.R_end", "FrontLowerLeg.R", (-0.1748, -0.5470, 0.1640)),
    ("BackLowerLeg.L_end", "BackLowerLeg.L", (0.1746, 0.7597, 0.1640)),
    ("BackLowerLeg.R_end", "BackLowerLeg.R", (-0.1746, 0.7597, 0.1640)),
    ("Tail7_end", "Tail7", (0.0000, 0.9040, 0.6230)),
    ("PoleTargetBack.L_end", "PoleTargetBack.L", (0.1746, -1.5786, 0.5862)),
    ("PoleTarget.L_end", "PoleTarget.L", (0.1748, -1.3469, 0.5778)),
    ("PoleTargetBack.R_end", "PoleTargetBack.R", (-0.1746, -1.5786, 0.5862)),
    ("PoleTarget.R_end", "PoleTarget.R", (-0.1748, -1.3469, 0.5778)),
    ("FFB.L_end", "FFB.L", (0.1746, 0.6979, 0.0008)),
    ("FF.L_end", "FF.L", (0.1748, -0.6088, 0.0008)),
    ("FFB.R_end", "FFB.R", (-0.1746, 0.6979, 0.0008)),
    ("FF.R_end", "FF.R", (-0.1748, -0.6088, 0.0008)),
]

# The head's frame, the one the tack's head axis uses (``Axis::head``, body
# frame +X forward, +Y up, +Z right; here Blender's -Y, +Z, -X): from behind
# the poll down the face to the muzzle. ``at(t, u, s)`` is ``t`` metres
# along it, ``u`` up from it (toward the forehead) and ``s`` to the left.
HEAD_O = Vector((0.0, -1.10, 1.78))
HEAD_A = Vector((0.0, -0.755, -0.656)).normalized()
HEAD_U = Vector((0.0, -0.656, 0.755)).normalized()
HEAD_AXES = Matrix((Vector((1, 0, 0)), HEAD_A, HEAD_U)).transposed()


def at(t, u=0.0, s=0.0):
    return HEAD_O + HEAD_A * t + HEAD_U * u + Vector((s, 0.0, 0.0))


def body_frame(p):
    """Blender metres to the game's body frame (+X forward, +Y up, +Z
    right)."""
    return Vector((-p[1], p[2], -p[0]))


def from_body(p):
    return Vector((-p[2], -p[0], p[1]))


EYE = (0.125, 0.02, 0.108)  # head frame (t, u, |s|) of the eyeball centre
EYE_RADIUS = 0.03

# The bones the Quaternius rig lacks: the mane, the forelock, a longer tail
# and the face. Name, parent, head, tail.
MANE = [
    ("Mane1", "Neck1", (0.0, -0.56, 1.53), (-0.10, -0.53, 1.36)),
    ("Mane2", "Neck2", (0.0, -0.73, 1.60), (-0.11, -0.69, 1.42)),
    ("Mane3", "Neck3", (0.0, -0.88, 1.71), (-0.11, -0.84, 1.53)),
    ("Mane4", "Neck3", (0.0, -0.98, 1.80), (-0.10, -0.95, 1.64)),
]
EXTRA = MANE + [
    ("Forelock", "Head", tuple(at(-0.02, 0.10)), tuple(at(0.14, 0.12))),
    ("Tail8", "Tail7", (0.0, 0.904, 0.623), (0.0, 0.92, 0.52)),
    ("Tail9", "Tail8", (0.0, 0.92, 0.52), (0.0, 0.935, 0.41)),
    ("Jaw", "Head", tuple(at(0.04, -0.11)), tuple(at(0.40, -0.14))),
    ("LipLower", "Jaw", tuple(at(0.40, -0.14)), tuple(at(0.48, -0.14))),
    ("LipUpper", "Head", tuple(at(0.42, -0.07)), tuple(at(0.52, -0.08))),
    ("Nostril.L", "Head", tuple(at(0.47, -0.02, 0.03)), tuple(at(0.48, -0.02, 0.07))),
    ("Nostril.R", "Head", tuple(at(0.47, -0.02, -0.03)), tuple(at(0.48, -0.02, -0.07))),
]

# Muscle helpers: leaf bones that change the shape of a muscle mass with the
# flexion of its joint (``muscle_shape``), so the skin does not only follow
# the bones. Each takes over part of its parent's weight round the muscle
# (``weight_muscles``), so at rest and unscaled it moves exactly as before.
# Name, parent, head (the mass's core, which the scale is about), tail
# (along the muscle), and the region it takes: centre and radii, the least
# distance from the midline (``side``) and the bones it takes the weight of
# (``steal``, the parent by default).
MUSCLES = []
for _s, _x in (("L", 1.0), ("R", -1.0)):
    MUSCLES += [
        # The quarter: the hamstrings and the biceps of the thigh behind the
        # femur, from the point of the buttock down to the stifle.
        dict(name=f"Quarter.{_s}", parent=f"BackLeg.{_s}", head=(_x * 0.10, 0.64, 1.02), tail=(_x * 0.12, 0.70, 0.80),
             centre=(_x * 0.17, 0.66, 1.04), radii=(0.14, 0.24, 0.22), side=0.08,
             steal=[f"BackLeg.{_s}", f"BackUpperLeg.{_s}", "Tail2", "Tail3", "Tail4"]),
        # The gaskin: the second thigh behind the tibia.
        dict(name=f"Gaskin.{_s}", parent=f"BackUpperLeg.{_s}", head=(_x * 0.175, 0.57, 0.75), tail=(_x * 0.175, 0.70, 0.62),
             centre=(_x * 0.175, 0.62, 0.74), radii=(0.12, 0.17, 0.17)),
        # The shoulder: the muscles over the shoulder blade and the triceps
        # above the elbow. Their skin rides the shoulder blade (the stub
        # that slides as the foreleg reaches), taken from the upper foreleg
        # and the chest.
        dict(name=f"Scapula.{_s}", parent=f"FrontShoulder.{_s}", head=(_x * 0.10, -0.49, 1.10), tail=(_x * 0.10, -0.42, 0.90),
             centre=(_x * 0.17, -0.50, 1.10), radii=(0.15, 0.20, 0.25),
             steal=[f"FrontUpperLeg.{_s}", "Torso2"]),
    ]
MUSCLE_NAMES = [m["name"] for m in MUSCLES]
EXTRA += [(m["name"], m["parent"], m["head"], m["tail"]) for m in MUSCLES]


def bone_table():
    """Every bone: (name, parent, head, rotation, length). The copied ones
    are as long as the way to their first child (5 cm for leaves); the
    added ones point from head to tail."""
    heads = {name: Vector(h) for name, _, h, _ in QUATERNIUS}
    children = {}
    for name, parent, _, _ in QUATERNIUS:
        children.setdefault(parent, []).append(name)
    out = []
    for name, parent, head, q in QUATERNIUS:
        y = Quaternion(q).to_matrix() @ Vector((0, 1, 0))
        length = 0.05
        for c in children.get(name, []):
            d = (heads[c] - heads[name]).dot(y)
            if d > 0.01:
                length = d
                break
        out.append((name, parent, head, q, length))
    for name, parent, head, tail in EXTRA:
        d = Vector(tail) - Vector(head)
        q = d.to_track_quat("Y", "Z")
        out.append((name, parent, head, tuple(q), d.length))
    return out


# ---------------------------------------------------------------------------
# The sculpt: smooth unions of masses and muscles on a signed distance grid.

BOUNDS = ((-0.37, -1.64, -0.01), (0.37, 1.02, 2.06))


def rot_x(deg):
    return Matrix.Rotation(math.radians(deg), 3, "X")


def legs(sdf, sx):
    """One side's legs, from the shoulder and the haunch down to the
    hooves."""
    E, R, T = rig.sdf_ellipsoid, rig.sdf_round_cone, rig.sdf_tube
    x = 0.175 * sx
    # Fore: the shoulder blade slanting forward, its point, the triceps
    # mass over the elbow, the forearm, the flat knee, the cannon with its
    # tendon behind, the fetlock with its ergot, the pastern and the hoof.
    sdf.add(E((sx * 0.16, -0.53, 1.23), (0.065, 0.12, 0.26), rot_x(-41)), 0.09)
    sdf.add(E((sx * 0.145, -0.735, 1.08), (0.05, 0.05, 0.06)), 0.06)
    sdf.add(E((sx * 0.175, -0.47, 0.99), (0.085, 0.13, 0.15), rot_x(-15)), 0.08)
    sdf.add(E((sx * 0.165, -0.44, 0.87), (0.05, 0.055, 0.06)), 0.06)
    sdf.add(R((x, -0.545, 0.92), (x, -0.585, 0.53), 0.085, 0.046), 0.05)
    sdf.add(E((x, -0.592, 0.79), (0.068, 0.072, 0.12), rot_x(-4)), 0.04)
    sdf.add(E((x, -0.618, 0.74), (0.05, 0.042, 0.11), rot_x(-6)), 0.03)  # extensor of the knee
    sdf.add(E((x, -0.552, 0.76), (0.046, 0.04, 0.10)), 0.03)  # flexors
    sdf.add(E((sx * 0.17, -0.40, 1.02), (0.07, 0.10, 0.12), rot_x(-25)), 0.05)  # long head of the triceps
    sdf.add(E((sx * 0.13, -0.575, 0.63), (0.014, 0.02, 0.026)), 0.006)  # chestnut
    sdf.add(E((x, -0.592, 0.49), (0.047, 0.05, 0.06)), 0.03)
    sdf.add(E((x, -0.548, 0.495), (0.03, 0.03, 0.04)), 0.02)
    sdf.add(T([(x, -0.590, 0.46), (x, -0.580, 0.33), (x, -0.565, 0.21)], [(0.035, 0.044), (0.031, 0.040), (0.034, 0.043)]), 0.02)
    sdf.add(R((x, -0.553, 0.44), (x, -0.530, 0.22), 0.017, 0.02), 0.02)
    sdf.add(E((x, -0.552, 0.165), (0.045, 0.058, 0.05)), 0.03)
    sdf.add(E((x, -0.508, 0.14), (0.016, 0.018, 0.016)), 0.012)  # ergot
    sdf.add(R((x, -0.555, 0.15), (x, -0.588, 0.085), 0.035, 0.039), 0.02)
    hoof(sdf, (x, -0.612, 0.0), (x, -0.590, 0.09), 0.064, 0.045)
    # Hind: the haunch and the long muscles of the quarter down the back
    # of the thigh, the round stifle, the gaskin, the hock with its point,
    # the cannon, the fetlock, the pastern and the hoof.
    sdf.add(E((sx * 0.15, 0.60, 1.15), (0.125, 0.24, 0.30), rot_x(20)), 0.08)
    sdf.add(E((sx * 0.185, 0.50, 1.00), (0.085, 0.2, 0.22), rot_x(25)), 0.06)
    sdf.add(R((sx * 0.11, 0.79, 1.15), (sx * 0.15, 0.75, 0.86), 0.095, 0.07), 0.06)
    sdf.add(E((sx * 0.195, 0.38, 0.94), (0.065, 0.08, 0.10)), 0.08)
    sdf.add(R((sx * 0.18, 0.60, 0.84), (x, 0.775, 0.58), 0.085, 0.048), 0.05)
    sdf.add(E((x, 0.70, 0.74), (0.055, 0.06, 0.10), rot_x(30)), 0.04)
    sdf.add(E((x, 0.79, 0.55), (0.042, 0.058, 0.055)), 0.03)
    sdf.add(R((x, 0.76, 0.70), (x, 0.83, 0.585), 0.03, 0.024), 0.03)  # tendon to the point of hock
    sdf.add(T([(x, 0.775, 0.52), (x, 0.770, 0.35), (x, 0.765, 0.21)], [(0.036, 0.048), (0.032, 0.043), (0.035, 0.045)]), 0.02)
    sdf.add(R((x, 0.805, 0.50), (x, 0.790, 0.22), 0.018, 0.02), 0.02)
    sdf.add(E((x, 0.775, 0.165), (0.044, 0.058, 0.05)), 0.03)
    sdf.add(E((x, 0.822, 0.14), (0.016, 0.018, 0.016)), 0.012)
    sdf.add(R((x, 0.772, 0.15), (x, 0.728, 0.085), 0.034, 0.037), 0.02)
    hoof(sdf, (x, 0.705, 0.0), (x, 0.728, 0.088), 0.058, 0.042)


def hoof(sdf, ground, coronet, r_ground, r_top):
    """A hoof: a wall flaring from the coronet to the ground, the toe ahead,
    a flat sole, and the soft band of the coronet."""
    g = Vector(ground)
    sdf.add(rig.sdf_capped_cone(g - Vector((0, 0, 0.01)), coronet, r_ground, r_top, 0.008), 0.01)
    sdf.add(rig.sdf_ellipsoid(coronet, (r_top + 0.004, r_top + 0.008, 0.014)), 0.012)
    lo, hi = g - Vector((0.1, 0.1, 0.1)), g + Vector((0.1, 0.1, 0.0))
    sdf.carve((lambda x, y, z: z + 0.0 * x + 0.0 * y, lo, hi), 0.004)


def head(sdf):
    E, T = rig.sdf_ellipsoid, rig.sdf_tube
    H = HEAD_AXES
    sdf.add(E(at(0.05, 0.0), (0.115, 0.12, 0.10), H), 0.06)  # cranium
    sdf.add(T([at(0.08, 0.005), at(0.25, -0.01), at(0.40, -0.035)], [(0.085, 0.072), (0.07, 0.068), (0.06, 0.06)]), 0.06)
    sdf.add(T([at(0.12, -0.07), at(0.28, -0.065), at(0.42, -0.06)], [(0.075, 0.06), (0.06, 0.045), (0.055, 0.037)]), 0.05)  # lower face
    for sx in (1, -1):
        sdf.add(E(at(0.15, -0.10, sx * 0.07), (0.055, 0.13, 0.13), H), 0.05)  # cheek
        sdf.add(T([at(0.10, -0.15, sx * 0.05), at(0.28, -0.115, sx * 0.032), at(0.42, -0.105, sx * 0.018)], [(0.024, 0.026), (0.02, 0.022), (0.02, 0.022)], side=(1, 0, 0)), 0.03)
        sdf.add(E(at(0.11, 0.05, sx * 0.095), (0.028, 0.05, 0.022), H), 0.025)  # brow
    sdf.add(E(at(0.45, -0.06), (0.066, 0.082, 0.068), H), 0.05)  # muzzle
    sdf.add(E(at(0.49, -0.088), (0.056, 0.05, 0.03), H), 0.03)  # upper lip
    sdf.add(E(at(0.45, -0.118), (0.042, 0.052, 0.024), H), 0.025)  # lower lip and chin
    sdf.add(E(at(0.17, -0.13), (0.05, 0.09, 0.065), H), 0.05)  # between the jaws
    mouth = E(at(0.468, -0.106, 0.0), (0.055, 0.065, 0.004), H)
    sdf.carve(mouth, 0.005)
    OPENINGS.append(mouth[0])
    for sx in (1, -1):
        sdf.add(E(at(0.468, -0.02, sx * 0.052), (0.016, 0.034, 0.03), H), 0.015)  # alar fold
        nostril = E(at(0.482, -0.035, sx * 0.05), (0.013, 0.02, 0.024), H)
        sdf.carve(nostril, 0.008)
        OPENINGS.append(nostril[0])
        # The eye in its socket, under lids.
        t, u, s = EYE
        sdf.add(E(at(t, u, sx * (s - 0.002)), (0.026, 0.04, 0.034), H), 0.012)  # lids
        sdf.carve(E(at(t, u, sx * (s + 0.016)), (0.031, 0.035, 0.0185), H), 0.004)  # opening
        # The rim of the lids, a soft bead round the opening.
        ring = [at(t + 0.037 * math.cos(a), u + 0.021 * math.sin(a), sx * (s + 0.004)) for a in (math.radians(d) for d in range(0, 361, 30))]
        sdf.add(rig.sdf_tube(ring, [(0.0035, 0.0035)] * len(ring), side=(1, 0, 0)), 0.003)
        ears(sdf, sx)


CAVITIES = []  # the ears' hollows, as distance functions


def ear_cavity(p):
    """How far ``p`` is from the inside of an ear's cup (negative inside
    the hollow)."""
    import numpy as np

    if p[2] < 1.8 or p[1] > -0.98 or not CAVITIES:
        return 1.0
    a = np.array([[[p[0]]]]), np.array([[[p[1]]]]), np.array([[[p[2]]]])
    return float(min(fn(*a).ravel()[0] for fn in CAVITIES))


def ears(sdf, sx):
    T = rig.sdf_tube
    base = Vector((sx * 0.072, -1.058, 1.80))
    mid = Vector((sx * 0.088, -1.077, 1.885))
    tip = Vector((sx * 0.104, -1.10, 1.972))
    sdf.add(T([base, mid, tip], [(0.036, 0.03), (0.042, 0.03), (0.006, 0.005)], side=(1, 0, 0)), 0.03)
    # The cup: a hollow opening forward and a little out, its rim thin.
    f = Vector((sx * 0.006, -0.010, 0.0))
    cavity = T([base + f + Vector((0, 0, 0.035)), mid + f, tip + f * 0.5 - Vector((0, 0, 0.016))], [(0.026, 0.02), (0.033, 0.025), (0.004, 0.004)], side=(1, 0, 0))
    sdf.carve(cavity, 0.005)
    CAVITIES.append(cavity[0])


def trunk(sdf):
    E, R, T = rig.sdf_ellipsoid, rig.sdf_round_cone, rig.sdf_tube
    sdf.add(T(
        [(0, -0.66, 1.13), (0, -0.45, 1.11), (0, -0.15, 1.105), (0, 0.15, 1.12), (0, 0.40, 1.19), (0, 0.55, 1.24)],
        [(0.16, 0.21), (0.215, 0.31), (0.232, 0.315), (0.228, 0.30), (0.205, 0.25), (0.18, 0.20)],
    ))
    sdf.add(E((0, 0.0, 0.95), (0.21, 0.40, 0.16)), 0.10)  # belly
    sdf.add(E((0, 0.50, 1.27), (0.235, 0.34, 0.22)), 0.10)  # croup
    sdf.add(E((0, -0.44, 1.33), (0.07, 0.20, 0.10)), 0.08)  # withers
    sdf.add(E((0, 0.0, 1.30), (0.18, 0.45, 0.12)), 0.12)  # long back muscles
    for sx in (1, -1):
        sdf.add(E((sx * 0.07, -0.68, 1.0), (0.08, 0.07, 0.11)), 0.06)  # pectoral
        sdf.add(E((sx * 0.20, 0.42, 1.35), (0.06, 0.07, 0.05)), 0.10)  # point of hip
    sdf.carve(E((0, -0.81, 0.97), (0.012, 0.04, 0.13)), 0.02)
    sdf.add(T([(0, 0.70, 1.40), (0, 0.80, 1.31), (0, 0.86, 1.14)], [(0.05, 0.05), (0.045, 0.045), (0.03, 0.03)]), 0.04)  # dock
    # The neck, its crest and the muscles along its lower edge, with the
    # jugular groove above them.
    sdf.add(T(
        [(0, -0.52, 1.25), (0, -0.70, 1.36), (0, -0.86, 1.53), (0, -0.98, 1.69), (0, -1.04, 1.77)],
        [(0.17, 0.25), (0.145, 0.20), (0.115, 0.155), (0.095, 0.125), (0.09, 0.11)],
    ), 0.10)
    sdf.add(T(
        [(0, -0.42, 1.47), (0, -0.66, 1.55), (0, -0.86, 1.68), (0, -0.98, 1.79), (0, -1.03, 1.84)],
        [(0.07, 0.05), (0.075, 0.06), (0.065, 0.05), (0.05, 0.04), (0.04, 0.035)],
    ), 0.07)
    for sx in (1, -1):
        sdf.add(T([(sx * 0.12, -0.74, 1.17), (sx * 0.10, -0.86, 1.35), (sx * 0.07, -0.98, 1.56)], [(0.04, 0.045), (0.038, 0.042), (0.032, 0.036)]), 0.05)
        sdf.groove([(0, -0.80, 1.18), (0, -0.92, 1.40), (0, -1.0, 1.57)], (sx, 0, 0), 0.03, 0.005, 0.008)


def details(sdf):
    """Surface anatomy pressed into or raised from the masses: the spine
    of the shoulder blade, faint ribs, the line between the muscles of the
    quarter, the stifle fold, the spur vein along the belly, the facial
    crest and a vein down the face."""
    for sx in (1, -1):
        out = (sx, 0, 0)
        sdf.ridge([(0, -0.45, 1.36), (0, -0.52, 1.24), (0, -0.60, 1.12)], out, 0.02, 0.003, 0.012)
        for k, (y0, depth) in enumerate(((-0.12, 0.0009), (-0.02, 0.0013), (0.08, 0.0011), (0.17, 0.0007))):
            w = 0.015 * (jitter(k, 71 + sx) - 0.5)
            sdf.groove([(0, y0, 1.17), (0, y0 + 0.035 + w, 1.07), (0, y0 + 0.06, 0.99)], out, 0.06, depth, 0.003)
        sdf.groove([(0, 0.76, 1.24), (0, 0.76, 1.05), (0, 0.72, 0.86)], (sx, 0.5, 0), 0.03, 0.004, 0.008)
        sdf.groove([(0, 0.31, 0.95), (0, 0.27, 1.02), (0, 0.22, 1.10)], out, 0.025, 0.004, 0.008)
        sdf.ridge([(0, -0.35, 0.92), (0, -0.10, 0.88), (0, 0.15, 0.89), (0, 0.32, 0.95)], (sx, 0, -0.6), 0.006, 0.0025, 0.004)
        # The seam behind the shoulder blade and the triceps, the croup's
        # gluteal mass, the splenius under the crest.
        sdf.groove([(0, -0.38, 1.22), (0, -0.43, 1.06), (0, -0.47, 0.93)], out, 0.03, 0.004, 0.008)
        sdf.add(rig.sdf_ellipsoid((sx * 0.12, 0.38, 1.40), (0.10, 0.18, 0.07)), 0.06)
        sdf.add(rig.sdf_ellipsoid((sx * 0.07, -0.76, 1.53), (0.05, 0.18, 0.07), rot_x(-35)), 0.05)
        # Lower legs: the groove between the cannon bone and the flexor
        # tendons, a vein inside the forearm (cephalic) and inside the hind
        # leg (saphenous).
        x = 0.175 * sx
        for y0, y1 in ((-0.568, -0.548), (0.792, 0.782)):
            sdf.groove([(x, y0, 0.44), (x, (y0 + y1) / 2, 0.33), (x, y1, 0.23)], (sx, 0, 0), 0.008, 0.0018, 0.003)
            sdf.groove([(x, y0, 0.44), (x, (y0 + y1) / 2, 0.33), (x, y1, 0.23)], (-sx, 0, 0), 0.008, 0.0018, 0.003)
        sdf.ridge([(x, -0.575, 0.88), (x, -0.585, 0.68), (x, -0.578, 0.50), (x, -0.57, 0.40)], (-sx, -0.25, 0), 0.0045, 0.002, 0.003)
        sdf.ridge([(sx * 0.17, 0.60, 0.86), (x, 0.70, 0.66), (x, 0.775, 0.50), (x, 0.772, 0.36)], (-sx, 0, 0), 0.0045, 0.002, 0.003)
        sdf.ridge([at(0.17, -0.03, sx * 0.03), at(0.24, -0.05, sx * 0.03), at(0.31, -0.065, sx * 0.025)], (sx, 0, 0), 0.008, 0.003, 0.006)
        sdf.ridge([at(0.20, -0.10, sx * 0.03), at(0.30, -0.09, sx * 0.02), at(0.40, -0.06, sx * 0.02)], (sx, 0, 0), 0.004, 0.002, 0.003)


def sculpt(voxel):
    """The dense watertight sculpt."""
    sdf = rig.Sdf(*BOUNDS, voxel)
    trunk(sdf)
    for sx in (1, -1):
        legs(sdf, sx)
    head(sdf)
    details(sdf)
    obj = sdf.to_mesh("HorseSculpt")
    return obj, sdf


# ---------------------------------------------------------------------------
# Regions of the coat, from the rest position.


def head_coords(p):
    """(t, u, s) of ``p`` in the head's frame."""
    r = Vector(p) - HEAD_O
    return r.dot(HEAD_A), r.dot(HEAD_U), r.x


def near_leg(p):
    """How far ``p`` is (sideways) from the nearest leg column."""
    return abs(abs(p[0]) - 0.175)


def is_hoof(p):
    return p[2] < 0.083 and near_leg(p) < 0.09


OPENINGS = []  # the nostrils and the mouth, as distance functions


def opening(p):
    """How far ``p`` is from the inside of a nostril or the mouth."""
    import numpy as np

    if p[2] < 1.2 or p[1] > -1.2 or not OPENINGS:
        return 1.0
    a = np.array([[[p[0]]]]), np.array([[[p[1]]]]), np.array([[[p[2]]]])
    return float(min(fn(*a).ravel()[0] for fn in OPENINGS))


def is_muzzle(p):
    """The bare insides of the nostrils and the lips (``Muzzle``); the
    dark skin round them is ``Main``, darkened in its maps."""
    return opening(p) < 0.003


def muzzle_skin(p):
    """How much of the muzzle's dark, nearly bare skin is at ``p``: one
    round the nostrils and the lips, fading up the face."""
    t, u, _ = head_coords(p)
    if p[2] < 1.2:
        return 0.0
    near = smooth((0.03 - opening(p)) / 0.03)
    return max(near, smooth((t - 0.33) / 0.1) * smooth((0.05 - u) / 0.07))


def in_ear(p, n):
    """The inside of an ear's cup."""
    return ear_cavity(p) < 0.004


def is_horn(p):
    """A chestnut (inside the forearm) or an ergot (behind the fetlock)."""
    x, y, z = abs(p[0]), p[1], p[2]
    if abs(z - 0.63) < 0.03 and abs(y + 0.575) < 0.025 and x < 0.15:
        return True
    return abs(z - 0.14) < 0.022 and near_leg(p) < 0.025 and (abs(y + 0.508) < 0.022 or abs(y - 0.822) < 0.022)


def is_face(p):
    """The front of the face, from the forehead to the nose: ``Main_Light``
    (the game paints it ``Coat::head``, paler on a tordillo)."""
    t, u, _ = head_coords(p)
    return p[2] > 1.3 and 0.10 < t < 0.40 and u > 0.035


def material_of(p, n):
    if is_hoof(p):
        return "Hooves"
    if is_muzzle(p):
        return "Muzzle"
    if is_horn(p):
        return "Main_Dark"
    if is_face(p):
        return "Main_Light"
    return "Main"


MATERIALS = ["Main", "Hair", "Main_Dark", "Muzzle", "Hooves", "Main_Light", "Eye_Black", "Eye_White"]
# The exported fallback colours (sRGB, exported linear): the game repaints Main, Hair,
# Main_Dark, Main_Light and Hooves per coat and keeps the others.
FALLBACK = {
    "Main": (0.78, 0.64, 0.41),
    "Hair": (0.13, 0.11, 0.09),
    "Main_Dark": (0.16, 0.13, 0.10),
    "Muzzle": (0.19, 0.165, 0.15),
    "Hooves": (0.15, 0.13, 0.11),
    "Main_Light": (0.78, 0.64, 0.41),
    "Eye_Black": (0.13, 0.075, 0.04),
    "Eye_White": (0.78, 0.74, 0.68),
}


FOREHEAD_WHORL = at(0.11, 0.07)
FLANK_WHORL = Vector((0.20, 0.30, 1.02))  # mirrored for the right flank
# The axes the short hair can lie along, for the bake: eight in the
# side plane (every 22.5 degrees) and one across.
FLOW_AXES = [Vector((0.0, math.cos(math.radians(22.5 * k)), math.sin(math.radians(22.5 * k)))) for k in range(8)] + [Vector((1.0, 0.0, 0.0))]


def hair_direction(p, n):
    """Which way the short coat lies at ``p`` (facing ``n``): back along
    the body and a little down, down the neck from the crest, the
    shoulders, the chest, the buttocks and the legs, forward along the
    belly, down the face from the whorl on the forehead, round the whorls
    on the flanks."""
    p, n = Vector(p), Vector(n)
    t, u, s = head_coords(p)
    if p.z > 1.3 and t > -0.06 and p.y < -0.95:
        r = p - FOREHEAD_WHORL
        r -= n * r.dot(n)
        d = HEAD_A.copy() if r.length > 0.07 else HEAD_A.lerp(r.normalized(), 1.0 - r.length / 0.07)
    elif p.y < -0.45 and p.z > 1.05:
        d = Vector((0.0, 0.45, -1.0))  # the neck
    elif p.z < 0.95 and near_leg(p) < 0.12:
        d = Vector((0.0, 0.05, -1.0))  # the legs
    elif p.y < -0.6:
        d = Vector((0.0, 0.15, -1.0))  # the chest and the point of the shoulder
    elif p.z < 0.9:
        d = Vector((0.0, -0.8, -0.3))  # the belly, forward to the girth
    elif p.y > 0.6:
        d = Vector((0.0, 0.35, -1.0))  # the buttocks
    else:
        d = Vector((0.0, 1.0, -0.35))
        c = Vector((math.copysign(FLANK_WHORL.x, p.x), FLANK_WHORL.y, FLANK_WHORL.z))
        r = p - c
        if r.length < 0.16:
            swirl = Vector((0.0, -r.z, r.y)) * math.copysign(1.0, p.x) + r * 0.4
            if swirl.length > 1e-6:
                d = d.normalized().lerp(swirl.normalized(), 1.0 - r.length / 0.16)
    d -= n * d.dot(n)
    return d.normalized() if d.length > 1e-6 else Vector((0.0, 1.0, 0.0))


def flow_weights(d):
    """``d`` as weights on ``FLOW_AXES`` (hair is the same either way
    along its axis)."""
    w = [0.0] * 9
    w[8] = d.x * d.x
    ang = math.degrees(math.atan2(d.z, d.y)) % 180.0
    k = ang / 22.5
    i = int(k) % 8
    f = k - int(k)
    rest = 1.0 - w[8]
    w[i] += rest * (1 - f)
    w[(i + 1) % 8] += rest * f
    return w


def tone(p, n):
    """A coat-independent tone (multiplied with the coat): a darker
    topline, lighter flanks, belly and inner legs, a dark eye ring and a
    darker muzzle, and broad soft mottling. At most one."""
    p, n = Vector(p), Vector(n)
    t, u, s = head_coords(p)
    v = 0.86
    top = smooth((n.z - 0.45) / 0.4) * smooth((p.z - 1.15) / 0.2)
    v -= 0.12 * top
    if p.z > 0.75 and near_leg(p) > 0.06 or p.z > 1.0:
        v += 0.10 * smooth((-n.z - 0.05) / 0.5) + 0.05 * smooth((1.18 - p.z) / 0.25) * smooth((0.85 - abs(n.z)) / 0.3)
    if p.z < 1.0 and abs(p.x) < 0.17:
        v += 0.05  # the inside of the legs
    for sx in (1, -1):
        e = (p - at(EYE[0], EYE[1], sx * EYE[2])).length
        v -= 0.16 * smooth((0.05 - e) / 0.025)
    if p.z > 1.2:
        v *= 1.0 - 0.8 * muzzle_skin(p)
        for sx in (1, -1):
            # The dark skin of the lid rims.
            e = (p - at(EYE[0], EYE[1], sx * EYE[2])).length
            v *= 1.0 - 0.45 * smooth((0.044 - e) / 0.01) * smooth((e - 0.026) / 0.006)
    if p.z > 1.82 and p.y < -0.98:
        c = ear_cavity(p)
        v -= 0.2 * smooth((0.016 - abs(c - 0.01)) / 0.008) + 0.15 * smooth((p.z - 1.93) / 0.04)
    v += 0.07 * rig.noise(p, (3.0, 3.0, 3.0), seed=2.7) + 0.04 * rig.noise(p, (7.0, 7.0, 7.0), seed=3.9)
    v += 0.03 * rig.noise(p, (11.0, 11.0, 11.0), seed=5.1)
    return max(0.3, min(1.0, v))


def smooth(t):
    t = max(0.0, min(1.0, t))
    return t * t * (3 - 2 * t)


def paint_sculpt(obj):
    """Masks on the dense sculpt (``Mask``: R short coat, G hoof horn, B
    bare skin; A the coat's tone, ``tone``) and the lie of the hair as
    weights on nine axes (``FlowA``, ``FlowB``, ``FlowC``); and the
    material that turns them into the detail baked onto the game mesh:
    fine strokes of short hair along the lie, whorls included; growth rings
    and tubules on the hooves; pores and wrinkles on the bare skin. Its
    colour is a neutral luminance with the tone; ``set_output(mat,
    "rough")`` makes it emit the roughness instead (the coat's sheen,
    varying stroke by stroke)."""
    import numpy as np

    mesh = obj.data
    count = len(mesh.vertices)
    co = np.empty(count * 3, np.float32)
    mesh.vertices.foreach_get("co", co)
    nor = np.empty(count * 3, np.float32)
    mesh.vertices.foreach_get("normal", nor)
    co, nor = co.reshape(-1, 3), nor.reshape(-1, 3)
    m = np.empty((count, 4), np.float32)
    f = np.ones((3, count, 4), np.float32)
    for i in range(count):
        p, n = Vector(co[i]), Vector(nor[i])
        hoof = 1.0 if is_hoof(p) else 0.0
        skin = muzzle_skin(p)
        m[i] = (1.0 - max(hoof, skin), hoof, skin, tone(p, n))
        w = flow_weights(hair_direction(p, n))
        for k in range(3):
            f[k, i, :3] = w[3 * k : 3 * k + 3]
    # Linear float colours, so the weights are not rounded or gamma'd.
    mask = mesh.color_attributes.new("Mask", "FLOAT_COLOR", "POINT")
    mask.data.foreach_set("color", m.ravel())
    for k, c in enumerate("ABC"):
        mesh.color_attributes.new(f"Flow{c}", "FLOAT_COLOR", "POINT").data.foreach_set("color", f[k].ravel())
    mat = bpy.data.materials.new("Sculpt")
    mat.use_nodes = True
    nt = mat.node_tree
    nodes, link = nt.nodes, nt.links.new
    bsdf = nodes["Principled BSDF"]
    coord = nodes.new("ShaderNodeTexCoord")

    def channels(layer):
        c = nodes.new("ShaderNodeVertexColor")
        c.layer_name = layer
        sep = nodes.new("ShaderNodeSeparateColor")
        link(c.outputs["Color"], sep.inputs["Color"])
        return [sep.outputs["Red"], sep.outputs["Green"], sep.outputs["Blue"]], c.outputs["Alpha"]

    (coat, hoof, skin), tone_out = channels("Mask")
    weights = []
    for c in "ABC":
        weights += channels(f"Flow{c}")[0]

    def math_(op, a, b=None, value=None):
        node = nodes.new("ShaderNodeMath")
        node.operation = op
        link(a, node.inputs[0])
        if b is not None:
            link(b, node.inputs[1])
        elif value is not None:
            node.inputs[1].default_value = value
        return node.outputs[0]

    def strokes(axis, along=40.0, across=650.0, detail=3.0):
        """Fine noise stretched along ``axis``: short hairs."""
        vr = nodes.new("ShaderNodeVectorRotate")
        vr.rotation_type = "EULER_XYZ"
        vr.inputs["Rotation"].default_value = Vector(axis).rotation_difference(Vector((0, 1, 0))).to_euler()
        link(coord.outputs["Object"], vr.inputs["Vector"])
        mp = nodes.new("ShaderNodeMapping")
        mp.inputs["Scale"].default_value = (across, along, across)
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

    # Hairs some 2.5 cm long and well under a millimetre wide, blended by
    # the lie's weights.
    # Each axis: fine hairs over a coarser lay (locks a centimetre wide)
    # that still reads at 3 m.
    hair = None
    for axis, w in zip(FLOW_AXES, weights):
        lay = math_("ADD", math_("MULTIPLY", strokes(axis), None, 0.5), math_("MULTIPLY", strokes(axis, along=12.0, across=110.0, detail=2.0), None, 0.5))
        term = math_("MULTIPLY", lay, w)
        hair = term if hair is None else math_("ADD", hair, term)
    # Hoof horn: fine vertical tubules and growth rings round the wall.
    tubules = strokes((0, 0, 1), along=30.0, across=1500.0, detail=2.0)
    rings = nodes.new("ShaderNodeTexWave")
    rings.bands_direction = "Z"
    rings.wave_profile = "SIN"
    rings.inputs["Scale"].default_value = 90.0
    rings.inputs["Distortion"].default_value = 0.4
    rings.inputs["Detail"].default_value = 1.0
    link(coord.outputs["Object"], rings.inputs["Vector"])
    horn = math_("ADD", math_("MULTIPLY", tubules, None, 0.35), math_("MULTIPLY", rings.outputs["Fac"], None, 0.15))
    horn = math_("ADD", horn, None, 0.25)
    # Bare skin: pores and fine wrinkles.
    pores = nodes.new("ShaderNodeTexVoronoi")
    pores.inputs["Scale"].default_value = 650.0
    link(coord.outputs["Object"], pores.inputs["Vector"])
    wrinkles = strokes((1, 0, 0), along=60.0, across=260.0)
    skin_h = math_("ADD", math_("MULTIPLY", pores.outputs["Distance"], None, 0.6), math_("MULTIPLY", wrinkles, None, 0.4))
    height = mix(mix(skin_h, horn, hoof), hair, coat)
    bump = nodes.new("ShaderNodeBump")
    bump.inputs["Strength"].default_value = 0.9
    bump.inputs["Distance"].default_value = 0.002
    link(height, bump.inputs["Height"])
    link(bump.outputs["Normal"], bsdf.inputs["Normal"])
    # Luminance: the strokes (darker between hairs) times the tone.
    shade = nodes.new("ShaderNodeMapRange")
    shade.inputs["From Min"].default_value = 0.3
    shade.inputs["From Max"].default_value = 0.7
    shade.inputs["To Min"].default_value = 0.58
    shade.inputs["To Max"].default_value = 1.0
    link(height, shade.inputs["Value"])
    lum = math_("MULTIPLY", shade.outputs["Result"], tone_out)
    link(lum, bsdf.inputs["Base Color"])
    # Roughness: a satin coat, glossier along each hair; horn and skin a
    # little rougher.
    sheen = nodes.new("ShaderNodeMapRange")
    sheen.inputs["From Min"].default_value = 0.3
    sheen.inputs["From Max"].default_value = 0.7
    sheen.inputs["To Min"].default_value = 0.9
    sheen.inputs["To Max"].default_value = 0.45
    link(height, sheen.inputs["Value"])
    rough = mix(mix(nodes.new("ShaderNodeValue").outputs[0], nodes.new("ShaderNodeValue").outputs[0], hoof), sheen.outputs["Result"], coat)
    vals = [n for n in nodes if n.type == "VALUE"]
    vals[0].outputs[0].default_value = 0.6
    vals[1].outputs[0].default_value = 0.55
    emit = nodes.new("ShaderNodeEmission")
    emit.name = "Rough"
    link(rough, emit.inputs["Color"])
    obj.data.materials.clear()
    obj.data.materials.append(mat)


def set_output(mat, which):
    """Connects the sculpt's output to its surface or to its roughness."""
    nt = mat.node_tree
    out = nt.nodes["Material Output"]
    src = nt.nodes["Rough"].outputs[0] if which == "rough" else nt.nodes["Principled BSDF"].outputs[0]
    nt.links.new(src, out.inputs["Surface"])


# ---------------------------------------------------------------------------
# The game mesh, its eyes and its materials.


def eyes():
    """An eyeball in each socket: ``Eye_Black`` with a sliver of
    ``Eye_White`` at the back corner."""
    out = []
    t, u, s = EYE
    for sx in (1, -1):
        bpy.ops.mesh.primitive_uv_sphere_add(segments=14, ring_count=9, radius=EYE_RADIUS, location=at(t, u, sx * (s - 0.012)))
        eye = bpy.context.active_object
        eye.name = "Eye"
        bpy.ops.object.transform_apply(location=True, rotation=True, scale=True)
        eye.data.materials.append(bpy.data.materials["Eye_Black"])
        eye.data.materials.append(bpy.data.materials["Eye_White"])
        c = at(t, u, sx * (s - 0.012))
        for poly in eye.data.polygons:
            d = (poly.center - c).normalized()
            poly.material_index = 1 if (d.dot(-HEAD_A) > 0.72 and abs(d.x) > 0.45) else 0
            poly.use_smooth = True
        out.append(eye)
    return out


def assign_materials(body):
    index = {name: i for i, name in enumerate(MATERIALS)}
    for poly in body.data.polygons:
        poly.material_index = index[material_of(poly.center, poly.normal)]


def plain_materials():
    for name in MATERIALS:
        mat = rig.plain_material(name, srgb(FALLBACK[name])[:3])
        if name.startswith("Eye"):
            mat.node_tree.nodes["Principled BSDF"].inputs["Roughness"].default_value = 0.04
        mat.name = name


# ---------------------------------------------------------------------------
# Hair: the mane, the forelock and the tail, as cards on a hair atlas.

HAIR_COLUMNS = 8
THICK = (0, 1, 2, 3, 4, 5)  # full locks: the mane and the tail
WISPY = (6, 7)  # thin, see-through locks: the forelock, lashes, ears


def jitter(i, salt):
    """A deterministic pseudo-random number in [0, 1)."""
    return (math.sin(i * 12.9898 + salt * 78.233) * 43758.5453) % 1.0


def fall(sdf, root, start, length, gap, steps=6, gravity=(0.0, 0.0, -1.0), stiff=0.5):
    """A lock of hair from ``root``: it leaves along ``start``, bends under
    ``gravity`` (``stiff`` how long it keeps its first direction) and stays
    ``gap`` off the body. Returns its points and facing normals."""
    pts, outs = [Vector(root)], []
    d = Vector(start).normalized()
    g = Vector(gravity).normalized()
    step = length / steps
    for i in range(steps):
        f = (i + 1) / steps
        w = smooth(f / max(stiff, 1e-3))
        d = (Vector(start).normalized() * (1 - w) + g * w).normalized()
        p = sdf.outside(pts[-1] + d * step, gap + 0.004 * f)
        pts.append(p)
    for p in pts:
        _, n = sdf.sample(p)
        outs.append(n)
    return pts, outs


def mane_spots(sdf):
    """The mane: a criollo's full mane in three overlapping layers of locks
    along the crest, falling over the right side of the neck (-X) with a
    few over to the left, longest along the middle and still full at the
    withers; the top layer rises off the crest before it falls, so the mane
    shows from the left too."""
    crest = rig._catmull([Vector(p) for p in ((0, -1.035, 1.84), (0, -0.98, 1.80), (0, -0.86, 1.70), (0, -0.66, 1.575), (0, -0.46, 1.505), (0, -0.40, 1.49))], 8)
    strands = []
    count = 76
    for layer in range(3):
        for i in range(count):
            f = (i + 0.5 * (layer % 2) + 0.4 * jitter(i, layer + 9)) / count
            f = min(f, 0.999)
            k = min(int(f * (len(crest) - 1)), len(crest) - 2)
            p = crest[k][0].lerp(crest[k + 1][0], f * (len(crest) - 1) - k)
            side = 1.0 if (layer < 2 and i % 7 == 3) else -1.0
            root = sdf.surface(p - Vector((0, 0, 0.12)), (side * 0.2, 0.1, 1.0))
            along = 0.6 + 0.4 * smooth(f / 0.25)
            length = (0.26 + 0.14 * jitter(i, layer + 1)) * (1.0 - 0.12 * layer) * along * (0.6 if side > 0 else 1.0)
            if layer == 2:
                start = Vector((side * 0.35, 0.15, 0.95))
                stiff = 0.35
            else:
                start = Vector((side * (0.9 - 0.2 * layer), 0.2, 0.3 + 0.2 * layer))
                stiff = 0.22 + 0.1 * layer
            pts, outs = fall(sdf, root, start, length, 0.006 + 0.01 * layer, steps=5, gravity=(side * 0.15, 0.22, -1.0), stiff=stiff)
            width = 0.07 + 0.03 * jitter(i, layer + 5)
            bone = min(MANE, key=lambda b: abs(b[2][1] - root.y))[0]
            strands.append((pts, [width * (1.0 - 0.4 * j / 5) for j in range(6)], outs, (i + layer) % len(THICK), bone))
    return strands


def forelock_spots(sdf):
    """The forelock: fine, separate locks fanning down between the eyes."""
    strands = []
    for i in range(26):
        s = (jitter(i, 41) - 0.5) * 0.09
        root = sdf.surface(at(-0.04 + 0.05 * jitter(i, 43), 0.0, s), HEAD_U)
        length = 0.13 + 0.09 * jitter(i, 47)
        start = HEAD_A * 0.75 + HEAD_U * 0.3 + Vector((s * 1.5, 0, 0))
        pts, outs = fall(sdf, root, start, length, 0.006 + 0.004 * (i % 3), steps=5, gravity=HEAD_A * 0.6 + Vector((s * 1.2, 0, -0.7)), stiff=0.25)
        width = 0.028 + 0.018 * jitter(i, 49)
        strands.append((pts, [width * (1.0 - 0.3 * j / 5) for j in range(6)], outs, WISPY[i % len(WISPY)], "Forelock"))
    return strands


def lash_spots(sdf):
    """Lashes along each upper lid, out and a little down."""
    strands = []
    t0, u0, s0 = EYE
    for sx in (1, -1):
        for i in range(11):
            f = i / 10
            root = at(t0 - 0.026 + 0.055 * f, u0 + 0.022 + 0.006 * math.sin(math.pi * f), sx * (s0 + 0.006))
            d = Vector((sx * 0.8, 0.0, 0.0)) + HEAD_A * 0.35 - HEAD_U * 0.25
            length = 0.022 + 0.012 * math.sin(math.pi * f)
            pts = [root, root + d.normalized() * length * 0.5 + HEAD_U * 0.002, root + d.normalized() * length - HEAD_U * 0.003]
            n = Vector((sx, 0.0, 0.0))
            strands.append((pts, [0.012, 0.01, 0.006], [n, n, n], WISPY[i % len(WISPY)], "Head"))
    return strands


def ear_hair_spots(sdf):
    """A few fine strands in each ear's cup, standing up and out of it."""
    strands = []
    for sx in (1, -1):
        base = Vector((sx * 0.078, -1.068, 1.835))
        for i in range(9):
            f = i / 8
            root = base + Vector((sx * 0.004 * (jitter(i, 81) - 0.5), 0.004, 0.08 * f))
            d = Vector((sx * 0.25 * (jitter(i, 83) - 0.3), -0.55, 0.8))
            pts, outs = fall(sdf, root, d, 0.035 + 0.02 * jitter(i, 85), 0.001, steps=3, gravity=(sx * 0.3, -0.6, 0.5), stiff=0.6)
            outs = [Vector((sx * 0.3, -1.0, 0.0)).normalized()] * len(pts)
            strands.append((pts, [0.016, 0.014, 0.01, 0.006], outs, WISPY[i % len(WISPY)], f"Ear3.{'L' if sx > 0 else 'R'}"))
    return strands


def tail_spots(sdf):
    """The tail: locks from all round the dock, falling to about the
    hocks (a criollo's tail, its dock ending above them), fuller at the
    top."""
    dock = rig._catmull([Vector(p) for p in ((0, 0.72, 1.41), (0, 0.80, 1.33), (0, 0.865, 1.14))], 8)
    strands = []
    count = 130
    for i in range(count):
        f = jitter(i, 51) ** 0.8
        k = min(int(f * (len(dock) - 1)), len(dock) - 2)
        c = dock[k][0].lerp(dock[k + 1][0], f * (len(dock) - 1) - k)
        ang = 2 * math.pi * jitter(i, 53)
        out = Vector((math.cos(ang), 0.35 * math.sin(ang) + 0.6, 0.6 * math.sin(ang) + 0.3)).normalized()
        root = sdf.surface(c, out, reach=0.2)
        bottom = 0.44 + 0.14 * jitter(i, 57) ** 1.5
        length = max(0.3, root.z - bottom)
        start = out * 0.4 + Vector((0, 0.5, -0.4))
        spread = Vector(((jitter(i, 59) - 0.5) * 0.25, 0.08 + 0.1 * jitter(i, 61), -1.0))
        pts, outs = fall(sdf, root, start, length, 0.01 + 0.02 * jitter(i, 63), steps=6, gravity=spread, stiff=0.18)
        # Face each card out from the tail's axis.
        axis_y = 0.92
        outs = [Vector((p.x, p.y - axis_y, 0.0)).normalized() if abs(p.x) + abs(p.y - axis_y) > 1e-3 else Vector((0, 1, 0)) for p in pts]
        width = 0.07 + 0.035 * jitter(i, 67)
        strands.append((pts, [width * (0.8 + 0.5 * j / 6) * (1.0 - 0.3 * (j / 6) ** 2) for j in range(7)], outs, THICK[i % len(THICK)]))
    return strands


# ---------------------------------------------------------------------------
# Rig and skin.

NOT_SKIN = {"Body", "PoleTarget.L", "PoleTarget.R", "PoleTargetBack.L", "PoleTargetBack.R",
            "Mane1", "Mane2", "Mane3", "Mane4", "Forelock", "Tail5", "Tail6", "Tail7", "Tail8", "Tail9",
            "FrontShoulder.L", "FrontShoulder.R", "BackShoulder.L", "BackShoulder.R", "Torso"} | set(MUSCLE_NAMES)
TAIL_CHAIN = ["Tail2", "Tail3", "Tail4", "Tail5", "Tail6", "Tail7", "Tail8", "Tail9"]


def build_rig():
    arm = rig.build_oriented_armature("AnimalArmature", bone_table())
    rig.bone_ends(arm, ENDS)
    return arm


def skin(body, arm):
    """Heat weights from the bones that move the skin (not the IK poles,
    the hair bones, the root or the stubs between the trunk and the legs)."""
    for b in arm.data.bones:
        b.use_deform = b.name not in NOT_SKIN
    rig.bind(body, arm)
    for b in arm.data.bones:
        b.use_deform = True
    elbows(body)
    hinds(body, arm)
    weight_muscles(body)


def weight_muscles(body):
    """Each muscle helper (``MUSCLES``) takes over, on the skin inside its
    region (an ellipsoid, fading over its outer third and toward the
    midline, on its own side only), the weight of its parent bone, all of it at the core: unscaled
    it moves exactly as the parent did, so the rest and the plain clips
    deform as before. The quarter also takes the weight the heat bind gave
    the tail's first bones on the buttock and the tibia's above the stifle
    (``steal``): the mass follows the femur, not the lifted tail."""
    groups = {g.name: g for g in body.vertex_groups}
    for m in MUSCLES:
        sources = [groups[n] for n in m.get("steal", [m["parent"]]) if n in groups]
        helper = groups.get(m["name"]) or body.vertex_groups.new(name=m["name"])
        groups[m["name"]] = helper
        c, r = Vector(m["centre"]), m["radii"]
        for v in body.data.vertices:
            p = v.co
            # Fading in over 6 cm from the midline cut, so no seam shows
            # where the helper's skin meets the tail's.
            lateral = smooth((p.x * math.copysign(1.0, c.x) - m.get("side", 0.04)) / 0.06)
            d = math.sqrt(sum(((p[i] - c[i]) / r[i]) ** 2 for i in range(3)))
            inside = smooth((1.0 - d) / 0.35) * lateral
            if inside <= 0.0:
                continue
            total = 0.0
            for g in sources:
                try:
                    w = g.weight(v.index)
                except RuntimeError:
                    continue
                moved = w * inside
                # Wholly moved at the core, so a vertex keeps its four bones.
                if w - moved < 0.02:
                    g.remove([v.index])
                    moved = w
                else:
                    g.add([v.index], w - moved, "REPLACE")
                total += moved
            if total > 1e-4:
                helper.add([v.index], total, "REPLACE")
    bpy.ops.object.select_all(action="DESELECT")
    body.select_set(True)
    bpy.context.view_layer.objects.active = body
    bpy.ops.object.vertex_group_limit_total(group_select_mode="ALL", limit=4)
    bpy.ops.object.vertex_group_normalize_all(lock_active=False)


def elbows(body):
    """The trunk behind and above the elbow follows the chest, not the
    swinging foreleg (heat weights let the upper arm drag the girth into a
    crease at full stride): the foreleg's weight there goes to ``Torso2``."""
    groups = {g.name: g for g in body.vertex_groups}
    torso = groups.get("Torso2") or body.vertex_groups.new(name="Torso2")
    for v in body.data.vertices:
        p = v.co
        trunk = max(smooth((p.y + 0.50) / 0.15), smooth((0.11 - abs(p.x)) / 0.05)) * smooth((p.z - 0.78) / 0.14)
        if trunk <= 0.0:
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
                moved = w * 0.85 * trunk
                g.add([v.index], w - moved, "REPLACE")
                torso.add([v.index], moved, "ADD")
    bpy.ops.object.select_all(action="DESELECT")
    body.select_set(True)
    bpy.context.view_layer.objects.active = body
    bpy.ops.object.vertex_group_normalize_all(lock_active=False)
    # Then smooth the weights round the elbows, so the skin folds over a
    # hand's width rather than in a crease.
    for v in body.data.vertices:
        v.select = any((v.co - Vector((sx * 0.17, -0.44, 0.90))).length < 0.24 for sx in (1, -1))
    bpy.ops.object.mode_set(mode="EDIT")
    bpy.ops.object.vertex_group_smooth(group_select_mode="ALL", factor=0.5, repeat=6)
    bpy.ops.object.mode_set(mode="OBJECT")
    bpy.ops.object.vertex_group_limit_total(group_select_mode="ALL", limit=4)
    bpy.ops.object.vertex_group_normalize_all(lock_active=False)


def hinds(body, arm):
    """The hind legs keep their volume at the gallop. Heat weights leave the
    gaskin's muscle and the outer thigh with the trunk, so when the leg
    swings back the gaskin thins to a stalk under a crease. Each hind leg's
    skin within a limb's reach of its bones is bound to them instead: the
    gaskin (round the tibia, ``BackUpperLeg``) wholly to it, blending into
    the femur (``BackLeg``) at the stifle and the cannon (``BackLowerLeg``)
    at the hock, and the outer thigh to the femur, blending into the
    hindquarters (``Back``) toward the hip and the croup. Past the reach the
    heat weights take over again, smoothly."""
    groups = {g.name: g for g in body.vertex_groups}

    def group(name):
        if name not in groups:
            groups[name] = body.vertex_groups.new(name=name)
        return groups[name]

    def segment(p, a, b):
        ab = b - a
        t = max(0.0, min(1.0, (p - a).dot(ab) / ab.length_squared))
        return (p - (a + ab * t)).length, t

    for side, sx in (("L", 1.0), ("R", -1.0)):
        hip = arm.data.bones[f"BackLeg.{side}"].head_local.copy()
        stifle = arm.data.bones[f"BackUpperLeg.{side}"].head_local.copy()
        hock = arm.data.bones[f"BackLowerLeg.{side}"].head_local.copy()
        femur, tibia, cannon = (group(f"{n}.{side}") for n in ("BackLeg", "BackUpperLeg", "BackLowerLeg"))
        back = group("Back")
        for v in body.data.vertices:
            p = v.co
            if p.x * sx < 0.03:
                continue
            want = {}
            d_t, t_t = segment(p, stifle, hock)
            d_f, t_f = segment(p, hip, stifle)
            # The gaskin: round the tibia, below the stifle's crease.
            reach_t = smooth((0.16 - d_t) / 0.05) * smooth((t_t - 0.02) / 0.1)
            # The outer thigh: round the femur, on the leg's own side.
            reach_f = smooth((0.20 - d_f) / 0.06) * smooth((p.x * sx - 0.10) / 0.06) * (1.0 - reach_t)
            if reach_t + reach_f <= 1e-4:
                continue
            if reach_t > 0.0:
                into_femur = 1.0 - smooth(t_t / 0.18)
                into_cannon = smooth((t_t - 0.88) / 0.12)
                w = {tibia.name: 1.0 - into_femur - into_cannon, femur.name: into_femur, cannon.name: into_cannon}
                for k, x in w.items():
                    want[k] = want.get(k, 0.0) + reach_t * x
            if reach_f > 0.0:
                into_back = 1.0 - smooth((t_f - 0.15) / 0.35)
                w = {femur.name: 1.0 - into_back, back.name: into_back}
                for k, x in w.items():
                    want[k] = want.get(k, 0.0) + reach_f * x
            keep = 1.0 - (reach_t + reach_f)
            old = {}
            for g in v.groups:
                old[body.vertex_groups[g.group].name] = g.weight * keep
            for g in list(v.groups):
                body.vertex_groups[g.group].remove([v.index])
            for k in set(old) | set(want):
                w = old.get(k, 0.0) + want.get(k, 0.0)
                if w > 1e-4:
                    group(k).add([v.index], w, "REPLACE")
    bpy.ops.object.select_all(action="DESELECT")
    body.select_set(True)
    bpy.context.view_layer.objects.active = body
    bpy.ops.object.vertex_group_limit_total(group_select_mode="ALL", limit=4)
    bpy.ops.object.vertex_group_normalize_all(lock_active=False)


def weight_hair(cards, arm, kinds):
    """Mane and forelock cards hang from their bone more toward the tip;
    tail cards follow the tail chain by height. ``kinds`` per card: the
    bone name, or "tail"."""
    import bmesh as bm_

    groups = {g.name: g for g in cards.vertex_groups}

    def group(name):
        if name not in groups:
            groups[name] = cards.vertex_groups.new(name=name)
        return groups[name]

    chain = []
    for name in TAIL_CHAIN:
        b = arm.data.bones[name]
        chain.append((name, b.head_local.copy()))
    chain.append((None, arm.data.bones["Tail9"].tail_local.copy()))
    uv = cards.data.uv_layers[0]
    vfrac = {}
    for loop in cards.data.loops:
        vfrac[loop.vertex_index] = 1.0 - uv.data[loop.index].uv[1]
    per_card = len(cards.data.vertices) // max(1, len(kinds))
    for v in cards.data.vertices:
        kind = kinds[v.index // per_card] if per_card else "tail"
        f = vfrac.get(v.index, 0.0)
        if kind == "tail":
            # Nearest point on the chain.
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
            if chain[i + 1][0]:
                group(chain[i + 1][0]).add([v.index], t, "REPLACE")
            else:
                group(chain[i][0]).add([v.index], 1.0, "REPLACE")
        else:
            w = 0.9 * f ** 0.8
            for g in v.groups:
                g.weight *= 1.0 - w
            group(kind).add([v.index], w, "ADD")


# ---------------------------------------------------------------------------
# The coats, as the game paints them (``Coat`` in
# crates/render/src/character/horse.rs), for the previews.

COATS = {
    "zaino": dict(body=(78, 50, 34), mane=(34, 28, 24), points=(44, 32, 26), head=None, hoof=(38, 32, 28)),
    "tordillo": dict(body=(118, 118, 114), mane=(206, 202, 194), points=(92, 92, 90), head=(150, 150, 146), hoof=(78, 68, 58)),
    "overo": dict(body=(142, 84, 48), mane=(118, 72, 44), points=(142, 84, 48), head=None, hoof=(78, 68, 58)),
    "alazan": dict(body=(154, 80, 42), mane=(176, 104, 58), points=(132, 70, 38), head=None, hoof=(38, 32, 28)),
    "bayo": dict(body=(198, 162, 104), mane=(34, 28, 24), points=(40, 32, 26), head=None, hoof=(38, 32, 28)),
}
OVERO_BLOBS = [
    ((0.0, 0.8, 0.0), (0.5, 0.2, 0.4)),
    ((-0.15, 1.08, -0.26), (0.36, 0.26, 0.16)),
    ((0.12, 1.02, 0.26), (0.38, 0.32, 0.18)),
    ((-0.56, 1.18, 0.24), (0.22, 0.22, 0.16)),
    ((0.78, 1.5, -0.12), (0.2, 0.24, 0.12)),
    ((0.52, 1.1, 0.22), (0.2, 0.26, 0.14)),
    ((0.9, 1.62, 0.1), (0.18, 0.2, 0.1)),
    ((0.45, 1.0, -0.22), (0.2, 0.26, 0.14)),
]


def color_at(coat, p, n):
    """``Coat::color_at``: ``p`` and ``n`` in the body frame; sRGB 0..255."""
    c = COATS[coat]
    white = (232, 228, 218)
    x, y, z = p
    if y < 0.5:
        stocking = (x > 0 and z < 0) or (x < 0 and z > 0)
        if coat == "overo" and stocking:
            return white
        return c["points"]
    if coat == "overo" and any(sum(((p[i] - ce[i]) / r[i]) ** 2 for i in range(3)) < 1.0 for ce, r in OVERO_BLOBS):
        return white
    if coat == "tordillo":
        a = math.sin(x * 23.0 + math.sin(z * 17.0))
        b = math.sin(y * 21.0 + math.cos(x * 13.0))
        cc = math.sin(z * 19.0 + y * 7.0)
        if a * b * cc > 0.12:
            return (178, 176, 170)
    if coat == "bayo" and n[1] > 0.6 and abs(z) < 0.05 and -0.8 <= x < 0.45:
        return (104, 76, 46)
    return c["body"]


def coat_part(coat, part):
    c = COATS[coat]
    return {"Hair": c["mane"], "Main_Dark": c["points"], "Main_Light": c["head"] or c["body"], "Hooves": c["hoof"]}.get(part)


def paint_coat(mesh, coat, layer="Coat"):
    """Vertex colours of ``coat`` on ``mesh`` (rest positions, Blender
    metres; ``scale`` maps them to life size)."""
    attr = mesh.data.color_attributes.get(layer) or mesh.data.color_attributes.new(layer, "BYTE_COLOR", "POINT")
    m = mesh.matrix_world
    nm = m.to_3x3().inverted().transposed()
    for v in mesh.data.vertices:
        p = body_frame(m @ v.co)
        n = body_frame((nm @ v.normal).normalized())
        rgb = color_at(coat, p, n)
        attr.data[v.index].color_srgb = (rgb[0] / 255, rgb[1] / 255, rgb[2] / 255, 1.0)
    mesh.data.color_attributes.active_color = attr


# ---------------------------------------------------------------------------
# Building it.

BODY_TRIS = 7200


def srgb(c):
    """sRGB 0..255 (or 0..1) to linear 0..1."""
    out = []
    for x in c[:3]:
        x = x / 255.0 if max(c[:3]) > 1.0 else x
        out.append(x / 12.92 if x <= 0.04045 else ((x + 0.055) / 1.055) ** 2.4)
    return (*out, 1.0)


def dense_zone(p):
    """Where the coat's colour changes on a line (the points, the bayo's
    stripe along the back, the muzzle, round the eyes and the ears): the
    game mesh keeps more vertices there, so the vertex-coloured edges stay
    clean."""
    t, u, _ = head_coords(p)
    if 0.43 < p[2] < 0.57 and near_leg(p) < 0.12:
        return True
    if abs(p[0]) < 0.075 and p[2] > 1.3 and -0.45 < p[1] < 0.8:
        return True
    if p[2] > 1.2 and (t > 0.38 or p[2] > 1.85 and p[1] < -0.98):
        return True
    return any((Vector(p) - at(EYE[0], EYE[1], sx * EYE[2])).length < 0.06 for sx in (1, -1))


def game_mesh(sculpt_obj):
    """The sculpt decimated to ``BODY_TRIS``: first evenly to twice
    that, then only away from the ``dense_zone``."""
    mid = rig.decimated_copy(sculpt_obj, "HorseMid", budget=2 * BODY_TRIS)
    coarse = mid.vertex_groups.new(name="Coarse")
    coarse.add([v.index for v in mid.data.vertices if not dense_zone(v.co)], 1.0, "REPLACE")
    print(f"horse: {sum(1 for p in mid.data.polygons if dense_zone(p.center))} of {len(mid.data.polygons)} triangles in the dense zone")
    body = rig.decimated_copy(mid, "Horse", budget=BODY_TRIS, keep="Coarse")
    bpy.data.objects.remove(mid)
    body.vertex_groups.clear()
    return body


class Clock:
    """Prints how long each step took."""

    def __init__(self):
        import time

        self.time = time.time
        self.last = time.time()

    def __call__(self, step):
        now = self.time()
        print(f"horse: {step} took {now - self.last:.0f} s")
        self.last = now


def build(draft, textures=None):
    """The skinned horse: the body (unwrapped, its maps baked from a dense
    sculpt), the eyes and the hair cards in one mesh ``Horse``. With
    ``textures``, writes the JPEG sources of the two KTX2 sets there."""
    import numpy as np

    rig.FPS = 48  # the original's clip lengths in whole frames
    rig.reset()
    clock = Clock()
    sculpt_obj, sdf = sculpt(0.008 if draft else 0.0045)
    clock("sculpt")
    paint_sculpt(sculpt_obj)
    clock("paint")
    plain_materials()
    body = game_mesh(sculpt_obj)
    for a in list(body.data.color_attributes):
        body.data.color_attributes.remove(a)
    body.data.materials.clear()
    for name in MATERIALS:
        body.data.materials.append(bpy.data.materials[name])
    assign_materials(body)
    rig.unwrap(body, margin=0.003)
    size = 1024
    samples = 8 if draft else 16
    colour = rig.new_image("horse_body_color", size)
    normal = rig.new_image("horse_body_normal", size, data=True)
    occlusion = rig.new_image("horse_body_ao", size, data=True)
    rough = rig.new_image("horse_body_rough", size, data=True)
    rig.bake_to(body, normal, "NORMAL", source=sculpt_obj, extrusion=0.012, samples=samples)
    rig.bake_to(body, colour, "DIFFUSE", source=sculpt_obj, extrusion=0.012, samples=samples, pass_filter={"COLOR"})
    set_output(sculpt_obj.data.materials[0], "rough")
    rig.bake_to(body, rough, "EMIT", source=sculpt_obj, extrusion=0.012, samples=samples)
    set_output(sculpt_obj.data.materials[0], "surface")
    sculpt_obj.hide_render = True
    rig.bake_to(body, occlusion, "AO", samples=16 if draft else 48)
    clock("bake")
    px = np.empty(size * size * 4, np.float32)
    colour.pixels.foreach_get(px)
    ao = np.empty(size * size * 4, np.float32)
    occlusion.pixels.foreach_get(ao)
    px = px.reshape(-1, 4)
    # The creases and grooves darker: the muscles read in any light (the
    # game lights its occlusion map only with the ambient light).
    px[:, :3] *= 1.0 - 0.6 * (1.0 - ao.reshape(-1, 4)[:, :1])
    colour.pixels.foreach_set(px.ravel())
    colour.update()

    import numpy as np

    size_h = 512 if draft else 1024
    thick = rig.hair_atlas(HAIR_COLUMNS, size_h, seed=3, strands=120, clump=0.7, clumps=5)
    wispy = rig.hair_atlas(HAIR_COLUMNS, size_h, seed=4, strands=22, clump=0.85, clumps=3)
    w = size_h // HAIR_COLUMNS
    h_lum, h_alpha, h_normal, h_rough = (np.concatenate([a[:, : len(THICK) * w], b[:, len(THICK) * w :]], axis=1) for a, b in zip(thick, wispy))
    hair_imgs = dict(
        color=rig.image_from_array("horse_hair_color", h_lum),
        opacity=rig.image_from_array("horse_hair_opacity", h_alpha, data=True),
        normal=rig.image_from_array("horse_hair_normal", h_normal, data=True),
        rough=rig.image_from_array("horse_hair_rough", h_rough, data=True),
    )
    if textures:
        for folder, maps in (
            ("horse_body", (("color", colour), ("normal_gl", normal), ("roughness", rough), ("ao", occlusion))),
            ("horse_hair", (("color", hair_imgs["color"]), ("normal_gl", hair_imgs["normal"]), ("roughness", hair_imgs["rough"]), ("opacity", hair_imgs["opacity"]))),
        ):
            for name, img in maps:
                rig.save_jpeg(img, os.path.join(textures, folder, f"{name}.jpg"))

    arm = build_rig()
    skin(body, arm)
    for eye in eyes():
        eye.vertex_groups.new(name="Head").add([v.index for v in eye.data.vertices], 1.0, "REPLACE")
        eye.parent = arm
        rig.join(body, eye)
    groups = [mane_spots(sdf) + forelock_spots(sdf), lash_spots(sdf), ear_hair_spots(sdf)]
    tail = tail_spots(sdf)
    for strands, kinds in [(g, [s[4] for s in g]) for g in groups] + [(tail, ["tail"] * len(tail))]:
        cards = rig.strand_cards("HairCards", [s[:4] for s in strands], HAIR_COLUMNS)
        cards.data.materials.append(bpy.data.materials["Hair"])
        rig.transfer_weights(body, cards, arm)
        weight_hair(cards, arm, kinds)
        rig.join(body, cards)
    bpy.data.objects.remove(sculpt_obj)
    clock("rig and hair")
    print(f"horse: {rig.triangles(body)} triangles in all, {len(arm.data.bones)} bones")
    images = dict(colour=colour, normal=normal, rough=rough, hair=hair_imgs)
    return body, arm, images


# ---------------------------------------------------------------------------
# Previews.


def _tex(nt, image, data=False):
    node = nt.nodes.new("ShaderNodeTexImage")
    node.image = image
    return node


def preview_material(name, images, rgb=None, vertex=False):
    """A preview of ``name`` as the game would draw it: ``rgb`` (sRGB) or
    the ``Coat`` vertex colours times the neutral luminance map, with the
    normal and roughness maps."""
    mat = bpy.data.materials.new(f"{name}Preview")
    mat.use_nodes = True
    nt = mat.node_tree
    link = nt.links.new
    bsdf = nt.nodes["Principled BSDF"]
    hair = name == "Hair"
    maps = images["hair"] if hair else dict(color=images["colour"], normal=images["normal"], rough=images["rough"])
    lum = _tex(nt, maps["color"])
    mul = nt.nodes.new("ShaderNodeMix")
    mul.data_type = "RGBA"
    mul.blend_type = "MULTIPLY"
    mul.inputs["Factor"].default_value = 1.0
    link(lum.outputs["Color"], mul.inputs["B"])
    if vertex:
        col = nt.nodes.new("ShaderNodeVertexColor")
        col.layer_name = "Coat"
        link(col.outputs["Color"], mul.inputs["A"])
    else:
        mul.inputs["A"].default_value = srgb(rgb)
    link(mul.outputs["Result"], bsdf.inputs["Base Color"])
    ntex = _tex(nt, maps["normal"])
    nmap = nt.nodes.new("ShaderNodeNormalMap")
    link(ntex.outputs["Color"], nmap.inputs["Color"])
    link(nmap.outputs["Normal"], bsdf.inputs["Normal"])
    rtex = _tex(nt, maps["rough"])
    link(rtex.outputs["Color"], bsdf.inputs["Roughness"])
    if hair:
        atex = _tex(nt, maps["opacity"])
        clip = nt.nodes.new("ShaderNodeMath")
        clip.operation = "GREATER_THAN"
        clip.inputs[1].default_value = 0.5
        link(atex.outputs["Color"], clip.inputs[0])
        link(clip.outputs[0], bsdf.inputs["Alpha"])
        mat.use_backface_culling = False
    bsdf.inputs["Specular IOR Level"].default_value = 0.35
    return mat


def coat_materials(coat, images):
    out = {}
    for name in MATERIALS:
        if name == "Main":
            out[name] = preview_material(name, images, vertex=True)
        elif name.startswith("Eye"):
            out[name] = bpy.data.materials[name]
        else:
            rgb = coat_part(coat, name) or FALLBACK[name]
            out[name] = preview_material(name, images, rgb)
    return out


def use_materials(mesh, by_name):
    for i, mat in enumerate(mesh.data.materials):
        base = mat.name.split(".")[0]
        for suffix in ("Preview",):
            base = base.replace(suffix, "")
        mesh.data.materials[i] = by_name[base]


def load_reference(path):
    """The Quaternius horse at the game's scale, its armature at rest."""
    before = set(bpy.data.objects)
    bpy.ops.import_scene.gltf(filepath=path)
    new = [o for o in bpy.data.objects if o not in before]
    for o in new:
        if o.type == "MESH" and o.parent is None:
            bpy.data.objects.remove(o)  # the importer's bone shape
    new = [o for o in bpy.data.objects if o not in before]
    arm = next(o for o in new if o.type == "ARMATURE")
    arm.scale = (100 * SCALE,) * 3
    arm.animation_data_clear()
    for pb in arm.pose.bones:
        pb.rotation_quaternion = Quaternion()
        pb.location = (0, 0, 0)
    mesh = next(o for o in new if o.type == "MESH")
    bpy.context.view_layer.update()
    return new, mesh


def reference_materials(mesh, coat):
    """The game's paint on the Quaternius horse: ``Main`` by vertex, the
    other coat parts flat, the rest as exported."""
    for i, mat in enumerate(mesh.data.materials):
        base = mat.name.split(".")[0]
        if base not in ("Main", "Hair", "Main_Dark", "Main_Light", "Hooves"):
            continue
        m = bpy.data.materials.new(f"Ref{base}")
        m.use_nodes = True
        bsdf = m.node_tree.nodes["Principled BSDF"]
        bsdf.inputs["Roughness"].default_value = 0.6
        if base == "Main":
            col = m.node_tree.nodes.new("ShaderNodeVertexColor")
            col.layer_name = "Coat"
            m.node_tree.links.new(col.outputs["Color"], bsdf.inputs["Base Color"])
        else:
            bsdf.inputs["Base Color"].default_value = srgb(coat_part(coat, base))
        mesh.data.materials[i] = m


def stage(size=(1000, 800)):
    scene = bpy.context.scene
    scene.render.engine = "BLENDER_EEVEE"
    scene.render.resolution_x, scene.render.resolution_y = size
    scene.render.image_settings.file_format = "PNG"
    scene.render.image_settings.color_mode = "RGB"
    scene.view_settings.view_transform = "AgX"
    world = bpy.data.worlds.new("Sky")
    world.use_nodes = True
    bg = world.node_tree.nodes["Background"]
    bg.inputs["Color"].default_value = (0.55, 0.58, 0.62, 1)
    bg.inputs["Strength"].default_value = 0.5
    scene.world = world
    bpy.ops.mesh.primitive_plane_add(size=30)
    ground = bpy.context.active_object
    gm = bpy.data.materials.new("Ground")
    gm.use_nodes = True
    gm.node_tree.nodes["Principled BSDF"].inputs["Base Color"].default_value = (0.32, 0.30, 0.26, 1)
    ground.data.materials.append(gm)
    # A low afternoon sun from the camera's side, front left of the horse.
    sun = bpy.data.objects.new("Sun", bpy.data.lights.new("Sun", "SUN"))
    sun.data.energy = 4.5
    sun.data.angle = math.radians(3)
    sun.rotation_euler = Vector((-0.55, 0.45, -0.70)).to_track_quat("-Z", "Y").to_euler()
    scene.collection.objects.link(sun)
    cam = bpy.data.objects.new("Cam", bpy.data.cameras.new("Cam"))
    scene.collection.objects.link(cam)
    scene.camera = cam
    return [ground, sun, cam], cam


def render_shots(out_dir, arm, mesh, images, shots, reference=None):
    """``shots``: (file, coat, action, frame, eye, target, lens). With
    ``reference`` (its objects and mesh), the still shots are also
    rendered of the Quaternius horse, ``ref_<file>``."""
    os.makedirs(out_dir, exist_ok=True)
    scene = bpy.context.scene
    props, cam = stage()
    by_coat = {}
    for shot in shots:
        by_coat.setdefault(shot[1], []).append(shot)
    exported = list(mesh.data.materials)
    for coat, group in by_coat.items():
        paint_coat(mesh, coat)
        mats = coat_materials(coat, images)
        use_materials(mesh, mats)
        if reference:
            paint_coat(reference[1], coat)
            reference_materials(reference[1], coat)
        for file, _, action, frame, eye, target, lens in group:
            arm.animation_data_create()
            arm.animation_data.action = action
            if action is None:
                for pb in arm.pose.bones:
                    pb.rotation_quaternion = Quaternion()
                    pb.location = (0, 0, 0)
            scene.frame_set(frame)
            cam.location = eye
            cam.data.lens = lens
            rig.look_at(cam, target)
            jobs = [(file, False)]
            if reference and action is None:
                jobs.append((f"ref_{file}", True))
            for name, ref in jobs:
                mesh.hide_render = ref
                for o in reference[0] if reference else []:
                    o.hide_render = not ref
                scene.render.filepath = os.path.join(out_dir, name)
                bpy.ops.render.render(write_still=True)
            mesh.hide_render = False
    arm.animation_data.action = None
    for i, mat in enumerate(exported):
        mesh.data.materials[i] = mat
    for o in props:
        bpy.data.objects.remove(o)


# ---------------------------------------------------------------------------
# The tack's rays (crates/render/src/character/tack.rs), body frame.


def tack_rays():
    rays = []
    for s, side in ((-1.0, "L"), (1.0, "R")):
        rays.append((f"bit.{side}", (1.33, 1.48, 0.016), (0.0, 0.0, s)))
        rays.append((f"jowl.{side}", (1.12, 1.62, 0.015), Vector((0.0, -0.2, s)).normalized()))
        rays.append((f"neck.{side}", (0.7, 1.45, 0.0), Vector((0.0, 1.0, 0.55 * s)).normalized()))
    for t in (-0.4, -0.2, -0.02, 0.2, 0.4):
        for th in (0.0, 0.5, -0.5, 1.0, -1.0, math.pi / 2, -math.pi / 2):
            rays.append((f"barrel {t:+.2f} {th:+.2f}", (t, 1.14, 0.0), (0.0, math.cos(th), math.sin(th))))
    along = Vector((0.755, -0.656, 0.0)).normalized()
    up = Vector((-along.y, along.x, 0.0))
    for t in (0.0, 0.1, 0.2, 0.3, 0.4):
        for th in (0.0, math.pi / 2, -math.pi / 2, math.pi):
            o = Vector((1.1, 1.78, 0.01)) + along * t
            d = up * math.cos(th) + Vector((0, 0, 1)) * math.sin(th)
            rays.append((f"head {t:.1f} {th:+.2f}", tuple(o), tuple(d)))
    return rays


def cast_all(mesh, skip=("Hair", "Eye_Black", "Eye_White")):
    """The distance along each tack ray to ``mesh``'s skin (at rest), or
    None."""
    from mathutils.bvhtree import BVHTree

    m = mesh.matrix_world
    verts = [m @ v.co for v in mesh.data.vertices]
    polys = [tuple(p.vertices) for p in mesh.data.polygons if mesh.data.materials[p.material_index].name.split(".")[0] not in skip]
    tree = BVHTree.FromPolygons(verts, polys)
    out = {}
    for name, o, d in tack_rays():
        hit = tree.ray_cast(from_body(o), from_body(d))
        out[name] = hit[3] if hit[0] is not None else None
    return out


def tack_report(mesh, reference):
    mine = cast_all(mesh)
    theirs = cast_all(reference) if reference else {}
    worst = 0.0
    lines = []
    for name, d in mine.items():
        r = theirs.get(name)
        delta = (d - r) if (d is not None and r is not None) else None
        if delta is not None:
            worst = max(worst, abs(delta))
        lines.append(f"  {name:22s} new {d if d is None else round(d, 3)!s:>6} ref {r if r is None else round(r, 3)!s:>6} delta {delta if delta is None else round(delta, 3)!s:>6}")
    print("horse: tack rays (m)\n" + "\n".join(lines) + f"\nhorse: worst tack ray offset {worst:.3f} m")


# ---------------------------------------------------------------------------
# Command line.

CLIP_ORDER = ["Idle", "Idle_2", "Idle_Headlow", "Eating", "Walk", "Gallop", "Gallop_Jump"]


def parse(argv):
    import argparse

    parser = argparse.ArgumentParser(prog="horse.py")
    parser.add_argument("output")
    parser.add_argument("--preview", default=None, metavar="DIR")
    parser.add_argument("--textures", default=None, metavar="DIR")
    parser.add_argument("--reference", default=None, metavar="GLB")
    parser.add_argument("--draft", action="store_true")
    parser.add_argument("--no-clips", action="store_true")
    return parser.parse_args(argv)


def shots_for(actions):
    side, target = (4.4, -0.2, 1.05), (0.0, -0.2, 0.95)
    front34 = ((2.9, -3.3, 1.55), (0.0, -0.35, 0.95))
    shots = []
    for coat in ("bayo", "tordillo", "overo", "zaino"):
        shots.append((f"{coat}_side.png", coat, None, 0, side, target, 50))
        shots.append((f"{coat}_front34.png", coat, None, 0, *front34, 45))
    shots += [
        ("bayo_rear34.png", "bayo", None, 0, (2.5, 3.3, 1.65), (0.0, 0.3, 0.95), 45),
        # Close-ups at 2 to 3 m.
        ("close_head.png", "bayo", None, 0, (1.05, -2.55, 1.85), (0.0, -1.2, 1.6), 60),
        ("close_head_zaino.png", "zaino", None, 0, (1.4, -2.3, 1.7), (0.0, -1.2, 1.6), 60),
        ("close_legs.png", "bayo", None, 0, (2.0, -1.0, 0.55), (0.1, -0.35, 0.42), 45),
        ("close_quarters.png", "tordillo", None, 0, (1.9, 2.1, 1.25), (0.0, 0.6, 0.95), 45),
        ("close_mane.png", "overo", None, 0, (-1.8, -1.6, 1.9), (0.0, -0.75, 1.55), 50),
        ("close_mane_side.png", "bayo", None, 0, (2.4, -0.8, 1.7), (0.0, -0.75, 1.5), 50),
        ("close_mane_right.png", "alazan", None, 0, (-2.4, -0.8, 1.7), (0.0, -0.75, 1.5), 50),
        ("close_head_side.png", "tordillo", None, 0, (2.2, -1.25, 1.7), (0.0, -1.25, 1.6), 60),
        ("close_eye.png", "zaino", None, 0, (0.9, -1.75, 1.75), (0.1, -1.2, 1.7), 70),
    ]
    by_name = {a.name: a for a in actions}
    frames = {
        "Walk": ((0, 14, 28, 42), "tordillo"),
        "Gallop": (tuple(range(0, 28, 2)), "zaino"),
        "Gallop_Jump": ((10, 24, 40, 60), "overo"),
        "Eating": ((0, 140), "bayo"),
        "Idle": ((0, 60, 120), "alazan"),
        "Idle_2": ((80,), "bayo"),
        "Idle_Headlow": ((80,), "zaino"),
    }
    if "Gallop" in by_name:
        for f in (7, 14, 21):
            shots.append((f"gallop_elbow_{f:03}.png", "zaino", by_name["Gallop"], f, (1.7, -1.7, 1.0), (0.1, -0.4, 0.9), 45))
        # The muscles at work: the quarters and the shoulder through the
        # stride, close.
        for f in range(0, 28, 4):
            shots.append((f"gallop_quarters_{f:03}.png", "alazan", by_name["Gallop"], f, (2.3, 1.2, 1.05), (0.0, 0.45, 0.85), 45))
            shots.append((f"gallop_shoulder_{f:03}.png", "alazan", by_name["Gallop"], f, (2.3, -1.6, 1.1), (0.0, -0.5, 0.9), 45))
    for name, (fs, coat) in frames.items():
        if name in by_name:
            for f in fs:
                shots.append((f"{name.lower()}_{f:03}.png", coat, by_name[name], f, side, target, 50))
    return shots


def main():
    argv = sys.argv[sys.argv.index("--") + 1 :] if "--" in sys.argv else []
    args = parse(argv)
    mesh, arm, images = build(args.draft, args.textures)
    actions = [] if args.no_clips else clips(arm)
    reference = load_reference(args.reference) if args.reference else None
    tack_report(mesh, reference[1] if reference else None)
    if args.preview:
        render_shots(args.preview, arm, mesh, images, shots_for(actions), reference)
    if reference:
        for o in reference[0]:
            bpy.data.objects.remove(o)
    for a in list(mesh.data.color_attributes):
        mesh.data.color_attributes.remove(a)
    keep = set(MATERIALS)
    for mat in list(bpy.data.materials):
        if mat.name not in keep:
            bpy.data.materials.remove(mat)
    rig.export(args.output, actions)



# ---------------------------------------------------------------------------
# Clips. Each is a function of the phase t in [0, 1): ``rig.Pose`` turns
# the bones by forward kinematics (``turn``, ``aim``), the legs reach their
# fetlocks by analytic IK, and the hooves (hanging from the IK bones, as in
# the original) roll over the heel and the toe while they stand.

X = Vector((1.0, 0.0, 0.0))
Z = Vector((0.0, 0.0, 1.0))
PIVOT = Vector((0.0, -0.05, 1.10))  # the body pitches and rolls about here
# Bones whose location is keyed: the roots, and the stubs that slide the
# shoulder blades and the pelvis.
MOVED = {"Body", "IKFrontLeg.L", "IKFrontLeg.R", "IKBackLeg.L", "IKBackLeg.R",
         "FrontShoulder.L", "FrontShoulder.R", "BackShoulder.L", "BackShoulder.R"}
NECK = ("Neck1", "Neck2", "Neck3", "Head")
MUZZLE = at(0.50, -0.07)
LEGS = ("LF", "RF", "LH", "RH")


def _clamp01(x):
    return max(0.0, min(1.0, x))


def _swing(d, fwd, up):
    """How far ``d`` points forward of straight down, in degrees, in the
    plane of ``fwd`` and ``up``."""
    return math.degrees(math.atan2(d.dot(fwd), -d.dot(up)))


def _frame(pose, bone):
    r = pose.posed(bone).to_3x3() @ pose.rest(bone).to_3x3().inverted()
    return r @ Vector((0, -1, 0)), r @ Vector((0, 0, 1))


def flexions(pose, side):
    """The joints that drive the muscles of one side: the femur's swing
    forward of its rest (degrees, in the hindquarters' frame), the hock's
    angle and the foreleg's swing forward, elbow to fetlock (in the
    chest's frame: the forearm itself stays near upright through the
    stance, while the whole leg sweeps back under the pushing triceps)."""
    fwd, up = _frame(pose, "Back")
    hip = pose.posed(f"BackLeg.{side}").translation
    stifle = pose.posed(f"BackUpperLeg.{side}").translation
    hock = pose.posed(f"BackLowerLeg.{side}").translation
    fet = end_point(pose, f"BackLowerLeg.{side}", f"BackLowerLeg.{side}_end")
    f_fwd, f_up = _frame(pose, "Torso2")
    elbow = pose.posed(f"FrontUpperLeg.{side}").translation
    hoof = end_point(pose, f"FrontLowerLeg.{side}", f"FrontLowerLeg.{side}_end")
    return dict(femur=_swing(stifle - hip, fwd, up), hock=math.degrees((stifle - hock).angle(fet - hock)),
                foreleg=_swing(hoof - elbow, f_fwd, f_up))


REST_FLEXIONS = {}


def muscle_shape(pose, arm):
    """Each muscle helper's scale (across, along, across its bone) and its
    shift (armature space) in ``pose``, from the flexion of its joint
    against the rest pose (``REST_FLEXIONS``), as in the user's gallop
    references (L43): the quarter rounds and bulges as the hind leg comes
    forward under the belly (a little as it drives back too), the gaskin
    bunches as the hock opens to push, and the shoulder's mass rides
    forward and up with the shoulder blade as the foreleg reaches and the
    triceps bunches as the leg stands and pushes back. A contracting muscle
    shortens along its bone as much as it thickens across."""
    if arm.name not in REST_FLEXIONS:
        rest = rig.Pose(arm)
        REST_FLEXIONS[arm.name] = {s: flexions(rest, s) for s in ("L", "R")}
    out = {}
    for side in ("L", "R"):
        now, rest = flexions(pose, side), REST_FLEXIONS[arm.name][side]
        femur = now["femur"] - rest["femur"]
        quarter = 0.18 * ease(_clamp01(femur / 25.0)) + 0.07 * ease(_clamp01(-femur / 25.0))
        gaskin = 0.20 * ease(_clamp01((now["hock"] - rest["hock"]) / 18.0))
        fore = now["foreleg"] - rest["foreleg"]
        reach, push = ease(_clamp01(fore / 40.0)), ease(_clamp01(-fore / 18.0))
        triceps = 0.18 * push + 0.06 * reach
        fwd, up = _frame(pose, "Torso2")
        shift = fwd * (0.045 * reach - 0.02 * push) + up * 0.02 * reach
        for name, bulge, move in ((f"Quarter.{side}", quarter, None), (f"Gaskin.{side}", gaskin, None),
                                  (f"Scapula.{side}", triceps, shift)):
            out[name] = (Vector((1.0 + bulge, 1.0 - 0.5 * bulge, 1.0 + bulge)), move or Vector())
    return out


class Pose(rig.Pose):
    """``rig.Pose`` that also shapes the muscle helpers (``muscle_shape``)
    from the pose's joints when it is keyed: their shift and scale, so
    every clip moves the muscles with its own flexion."""

    def apply(self, frame, root):
        shapes = muscle_shape(self, self.arm)
        for name, (scale, move) in shapes.items():
            parent = self.arm.data.bones[name].parent.name
            carried = self.posed(parent) @ self.rest(parent).inverted() @ self.rest(name)
            self.world[name] = Matrix.Translation(move) @ carried @ Matrix.Diagonal(scale.to_4d())
        roots = {root} if isinstance(root, str) else set(root)
        super().apply(frame, roots | set(shapes))
        for name, (scale, _) in shapes.items():
            pb = self.arm.pose.bones[name]
            pb.scale = scale
            pb.keyframe_insert("scale", frame=frame)


def wave(t, cycles=1.0, phase=0.0):
    return math.sin(2 * math.pi * (t * cycles + phase))


def ease(q):
    q = max(0.0, min(1.0, q))
    return q * q * (3 - 2 * q)


def rot(axis, deg):
    return Matrix.Rotation(math.radians(deg), 3, Vector(axis))


def body_matrix(lift=0.0, nose_down=0.0, roll=0.0, yaw=0.0, forward=0.0, pivot=PIVOT):
    """The body's motion: up ``lift`` m, pitched nose down by ``nose_down``
    degrees, rolled (right side down positive) and turned about ``pivot``,
    and ``forward`` m ahead."""
    r = (rot(Z, yaw) @ rot((0, 1, 0), roll) @ rot(X, nose_down)).to_4x4()
    return Matrix.Translation(Vector((0, -forward, lift)) + pivot) @ r @ Matrix.Translation(-pivot)


def leg_bones(leg):
    side = leg[0]
    if leg[1] == "F":
        return dict(stub=f"FrontShoulder.{side}", chain=[f"FrontUpperLeg.{side}", f"FrontLowerLeg.{side}"],
                    ik=f"IKFrontLeg.{side}", end=f"FrontLowerLeg.{side}_end", bend=-1.0)
    return dict(stub=f"BackShoulder.{side}", chain=[f"BackLeg.{side}", f"BackUpperLeg.{side}", f"BackLowerLeg.{side}"],
                ik=f"IKBackLeg.{side}", end=f"BackLowerLeg.{side}_end", bend=1.0)


END_REST = {name: Vector(p) for name, _, p in ENDS}


def fetlock_rest(leg):
    return END_REST[leg_bones(leg)["end"]].copy()


# The hoof's heel and toe on the ground, at rest (y), for the rolls.
HOOF = {"F": dict(toe=-0.672, heel=-0.552), "H": dict(toe=0.652, heel=0.758)}


def hoof_place(leg, y_shift, toe_up, z_shift=0.0):
    """The fetlock of a standing hoof slid ``y_shift`` along the ground
    from rest and rolled ``toe_up`` degrees about its heel (toe up) or its
    toe (heel up, negative)."""
    f0 = fetlock_rest(leg)
    h = HOOF[leg[1]]
    pivot = Vector((f0.x, (h["heel"] if toe_up > 0 else h["toe"]), 0.0))
    r = rot(X, -toe_up)
    f = pivot + r @ (f0 - pivot)
    return f + Vector((0.0, y_shift, z_shift))


def two_bone(a, target, l1, l2, bend):
    """The middle joint of a two-bone limb from ``a`` reaching ``target``,
    bending toward ``bend``; and the reachable end."""
    d = target - a
    dist = max(abs(l1 - l2) + 1e-4, min(d.length, l1 + l2 - 1e-4))
    u = d.normalized()
    end = a + u * dist
    x = (l1 * l1 + dist * dist - l2 * l2) / (2 * dist)
    h = math.sqrt(max(0.0, l1 * l1 - x * x))
    perp = (bend - u * bend.dot(u)).normalized()
    return a + u * x + perp * h, end


def child_point(pose, bone, child):
    """Where ``child``'s head is in the current pose."""
    return pose.posed(child).translation.copy()


def end_point(pose, bone, end):
    """Where the leaf node ``end`` (rest position) is, carried by
    ``bone``."""
    m = pose.posed(bone) @ pose.rest(bone).inverted()
    return m @ END_REST[end]


SHORT = {}  # per leg, the most a standing hoof fell short of its mark


# How straight a standing foreleg is held by the thoracic sling (``sling``
# in ``pose_leg``): the elbow this part of the leg's full length from the
# fetlock (a knee of about 160 degrees), and the most the chest may sink
# or rise between the shoulder blades for it, in metres.
SLING_REACH, SLING_MOST = 0.985, 0.09


def pose_leg(pose, body, leg, fetlock, toe_up, slide=0.0, hip=0.5, sling=0.0):
    """Reaches ``fetlock`` (world) with ``leg``; the hoof hangs from the IK
    bone there, its toe ``toe_up`` degrees up. ``slide`` moves the shoulder
    blade (fore) or the pelvis side (hind) forward by that many metres, in
    the body's frame; ``hip`` is how much of the reach the hind thigh
    takes. ``sling`` (0 to 1, fores only) moves the shoulder blade up or
    down the chest, as the thoracic sling lets the trunk sink between the
    shoulders under load, so a standing foreleg stays nearly straight
    (``SLING_REACH``) whatever the body's own rise and pitch: a weight-
    bearing knee never buckles."""
    b = leg_bones(leg)
    rb = body.to_3x3()
    lateral = rb @ X
    fwd = rb @ Vector((0, -1, 0))
    if slide:
        pose.move(b["stub"], fwd * slide)
    if sling and leg[1] == "F":
        up = rb @ Z
        elbow = pose.posed(b["chain"][0]).translation
        knee = child_point(pose, b["chain"][0], b["chain"][1])
        full = (knee - elbow).length + (end_point(pose, b["chain"][1], b["end"]) - knee).length
        # The rise h along ``up`` that puts the elbow SLING_REACH x full from
        # the fetlock: |d + h up| = r, d = elbow - fetlock.
        d, r = elbow - Vector(fetlock), SLING_REACH * full
        bq = d.dot(up)
        disc = bq * bq - (d.length_squared - r * r)
        if disc >= 0.0:
            h = -bq + math.sqrt(disc)
            pose.move(b["stub"], up * sling * max(-SLING_MOST, min(SLING_MOST, h)))
    chain = b["chain"]
    fetlock = Vector(fetlock)
    if len(chain) == 3:
        # The hind thigh swings part of the way toward the target.
        h0 = pose.posed(chain[0]).translation
        s0 = child_point(pose, chain[0], chain[1])
        rest_dir = (s0 - h0).normalized()
        f_rest = body @ fetlock_rest(leg)
        swing = (f_rest - h0).normalized().rotation_difference((fetlock - h0).normalized())
        partial = Quaternion().slerp(swing, hip)
        pose.aim(chain[0], s0, h0 + partial.to_matrix() @ rest_dir)
        chain = chain[1:]
    a = pose.posed(chain[0]).translation.copy()
    k0 = child_point(pose, chain[0], chain[1])
    l1 = (k0 - a).length
    l2 = (end_point(pose, chain[1], b["end"]) - k0).length
    bend = fwd * (-b["bend"])
    knee, reach = two_bone(a, fetlock, l1, l2, bend + lateral * 0.0)
    pose.aim(chain[0], k0, knee)
    pose.aim(chain[1], end_point(pose, chain[1], b["end"]), reach)
    actual = end_point(pose, chain[1], b["end"])
    r = rot(X, -toe_up)
    rest = pose.rest(b["ik"])
    m = r.to_4x4() @ rest.to_3x3().to_4x4()
    m.translation = actual
    pose.world[b["ik"]] = m
    return actual


def pose_neck(pose, body, deltas=(0, 0, 0, 0), yaw=0.0):
    """The neck and the head bent ``deltas`` degrees (nose up positive)
    from rest at each joint, in the body's frame, and turned ``yaw``
    degrees (to the horse's left positive), more toward the head."""
    rb = body.to_3x3()
    points = [pose.rest_head(n) for n in NECK[1:]] + [MUZZLE]
    total = 0.0
    for k, (bone, nxt) in enumerate(zip(NECK, points)):
        total += deltas[k]
        head = pose.posed(bone).translation.copy()
        here = pose.posed(bone) @ pose.rest(bone).inverted() @ nxt
        rest_vec = nxt - pose.rest_head(bone)
        y = yaw * (k + 1) / len(NECK)
        want = rb @ (rot(Z, y) @ rot(X, -total) @ rest_vec)
        pose.aim(bone, here, head + want)


def head_axes(pose):
    m = pose.posed("Head").to_3x3() @ pose.rest("Head").to_3x3().inverted()
    return m @ X, m @ HEAD_U, m @ HEAD_A


def pose_face(pose, ears=((0, 0), (0, 0)), jaw=0.0, grind=0.0, lip=0.0, nostrils=0.0):
    """Ears turned back (degrees) and swivelled out, per side (L, R); the
    jaw open and ground sideways; the upper lip reaching; the nostrils
    flared."""
    lat, up, along = head_axes(pose)
    for (back, out), side, sx in zip(ears, ("L", "R"), (1, -1)):
        pose.turn(f"Ear1.{side}", rot(up, sx * out) @ rot(lat, back))
    pose.turn("Jaw", rot(up, grind) @ rot(lat, -jaw))
    pose.turn("LipUpper", rot(lat, -lip))
    for side, sx in (("L", 1), ("R", -1)):
        pose.turn(f"Nostril.{side}", rot(along, sx * nostrils))


def pose_hair(pose, mane=0.0, mane_lift=0.0, forelock=0.0, tail=(0.0, 0.0), tail_lift=0.0, lag=0.0, t=0.0, cycles=1.0,
              tail_curve=0.0, tail_whip=0.0):
    """Secondary motion: the mane swings ``mane`` degrees out from the
    neck (``mane_lift`` up, in the wind of a gallop), the forelock
    ``forelock`` degrees, the tail ``tail`` (sideways amplitude, phase)
    down the chain with ``lag`` cycles per bone. The dock is lifted
    ``tail_lift`` degrees (back and up, over its first two bones); each
    bone after the dock bends ``tail_curve`` degrees back down, so the hair
    flows in a curve; and a wave of ``tail_whip`` degrees runs down the
    chain with the same lag, growing toward the tip (the follow-through of
    each stride)."""
    for k, name in enumerate(("Mane1", "Mane2", "Mane3", "Mane4")):
        parent = pose.posed(name).to_3x3() @ pose.rest(name).to_3x3().inverted()
        along = parent @ Vector((0, -1, 0.6)).normalized()
        swing = mane * wave(t, cycles, -0.08 * k) + mane_lift
        pose.turn(name, rot(along, swing))
    lat, up, along = head_axes(pose)
    pose.turn("Forelock", rot(lat, forelock * wave(t, cycles, -0.1)))
    amp, phase = tail
    for k, name in enumerate(TAIL_CHAIN):
        m = pose.posed(name).to_3x3() @ pose.rest(name).to_3x3().inverted()
        side = amp * wave(t, cycles, phase - lag * k) / len(TAIL_CHAIN) * (1.0 + 0.3 * k)
        lift = tail_lift * (0.6, 0.4)[k] if k < 2 else -tail_curve
        lift += tail_whip * wave(t, cycles, 0.25 - lag * k) * (0.4 + 0.15 * k)
        # A positive lift turns the hanging tail back and up, away from
        # the hind legs (the other way it tucks under the belly).
        pose.turn(name, rot(m @ Z, side) @ rot(m @ X, lift))


def stand_legs(pose, body, offsets=None, rests=None):
    """Every hoof standing at rest (slid by ``offsets[leg]`` m, rolled
    ``rests[leg]`` degrees: a resting hind leg stands on its toe)."""
    offsets = offsets or {}
    rests = rests or {}
    for leg in LEGS:
        dy, toe = offsets.get(leg, 0.0), rests.get(leg, 0.0)
        f = hoof_place(leg, dy, toe)
        # A hoof set forward takes its shoulder blade with it, the knee
        # straight.
        pose_leg(pose, body, leg, f, toe, slide=-0.7 * dy if leg[1] == "F" else 0.0)


def standing(arm, body, neck=(0, 0, 0, 0), yaw=0.0, t=0.0, ears=((0, 0), (0, 0)), jaw=0.0, grind=0.0,
             lip=0.0, offsets=None, rests=None, tail=(6.0, 0.0), tail_lift=0.0, flex=0.0, nostrils=0.0):
    pose = Pose(arm)
    pose.carry("Body", body)
    if flex:
        pose.turn("Torso2", rot(body.to_3x3() @ X, flex))
    pose_neck(pose, body, neck, yaw)
    pose_face(pose, ears, jaw, grind, lip, nostrils)
    pose_hair(pose, mane=2.0, forelock=2.0, tail=tail, tail_lift=tail_lift, lag=0.06, t=t)
    stand_legs(pose, body, offsets, rests)
    return pose


def flick(t, at_, width=0.06):
    """A quick there-and-back in [0, 1] centred on phase ``at_``."""
    d = abs(((t - at_ + 0.5) % 1.0) - 0.5) / width
    return max(0.0, 1.0 - d) ** 2 * (3 - 2 * max(0.0, 1.0 - d)) if d < 1 else 0.0


def idle(arm, t):
    # Breathing, a slow look about, an ear flicking back and a swish of the
    # tail.
    body = body_matrix(lift=0.004 * wave(t), nose_down=0.4 * wave(t, 1, 0.25))
    neck = (1.5 * wave(t, 1, 0.1), 1.0 * wave(t, 1, 0.2), 0.0, 2.0 * wave(t, 1, 0.3))
    ears = ((35 * flick(t, 0.3), 25 * flick(t, 0.3)), (20 * flick(t, 0.7, 0.08), 30 * flick(t, 0.72, 0.08)))
    return standing(arm, body, neck, yaw=8.0 * wave(t, 1, 0.15), t=t, ears=ears,
                    tail=(14.0, 0.0), nostrils=4.0 * max(0.0, wave(t)))


def idle_2(arm, t):
    # Resting the right hind: the hip drops on that side, the hoof stands on
    # its toe a little forward; the head lower, the ears lopped sideways.
    body = body_matrix(lift=0.003 * wave(t), roll=0.8, nose_down=0.4)
    neck = (-6.0, -5.0, 0.0, -4.0 + 1.5 * wave(t, 1, 0.2))
    lop = 25.0
    ears = ((10 + 30 * flick(t, 0.55), lop), (10, lop + 25 * flick(t, 0.2)))
    return standing(arm, body, neck, yaw=-5.0, t=t, ears=ears, offsets={"RH": -0.10}, rests={"RH": -38.0},
                    tail=(10.0, 0.3), nostrils=3.0 * max(0.0, wave(t)))


def idle_headlow(arm, t):
    # Dozing: the neck lowered from its base so the poll hangs about at the
    # withers, the poll opening so the face stays in front of the vertical
    # (about 15 degrees), as a resting horse holds it; the left hind rests
    # on its toe, the hip dropped on that side; all four hooves on the
    # ground.
    body = body_matrix(roll=-0.8, nose_down=0.3 + 0.3 * wave(t))
    neck = (-30.0 + 1.5 * wave(t, 1, 0.1), -8.0, 8.0, 21.0 + 2.0 * wave(t, 1, 0.3))
    ears = ((25, 30 + 15 * flick(t, 0.5)), (25 + 30 * flick(t, 0.35), 30))
    return standing(arm, body, neck, t=t, ears=ears, offsets={"LH": -0.08}, rests={"LH": -32.0},
                    tail=(8.0, 0.0))


def eating(arm, t):
    # 6 s: down to the grass (0.3 s to 1 s), grazing with the lip and
    # tearing, chewing, then back up (4.3 s to 5.2 s).
    down = ease((t - 0.05) / 0.13) * (1 - ease((t - 0.72) / 0.15))
    bite = max(0.0, wave(t, 9)) * down
    body = body_matrix(lift=GRAZE_LIFT * down, nose_down=GRAZE_PITCH * down)
    bend = GRAZE_BEND * down
    neck = tuple(w * bend for w in GRAZE_SHAPE)
    neck = (neck[0], neck[1], neck[2], neck[3] + 6.0 * bite)
    chew = max(0.0, wave(t, 14)) * (0.6 + 0.4 * down)
    ears = ((15 + 25 * flick(t, 0.45), 20), (15, 20 + 30 * flick(t, 0.62)))
    pose = standing(arm, body, neck, yaw=6.0 * down * wave(t, 2), t=t, ears=ears, jaw=4.0 * chew,
                    grind=3.0 * wave(t, 14, 0.25) * chew, lip=8.0 * bite, offsets={"LF": -0.12 * down}, tail=(10.0, 0.2))
    return pose


# How the neck bends to graze: its joints share the bend in these parts
# (nose up positive), and ``GRAZE_BEND`` is found so the lips reach the
# grass (``graze_bend``). The poll opens almost as much as the neck base
# bends, so the face reaches the grass about vertical, never tucked behind.
GRAZE_SHAPE = (-1.0, -0.1, 0.0, 0.9)
GRAZE_LIFT, GRAZE_PITCH = -0.005, 3.0
GRAZE_BEND = 60.0


def graze_bend(arm, height=0.04):
    """The bend that puts the lips nearest ``height`` m over the ground
    (searched in half-degree steps)."""
    body = body_matrix(lift=GRAZE_LIFT, nose_down=GRAZE_PITCH)
    best = None
    for k in range(40, 280):
        bend = k * 0.5
        pose = rig.Pose(arm)
        pose.carry("Body", body)
        pose_neck(pose, body, tuple(w * bend for w in GRAZE_SHAPE))
        lips = pose.posed("Head") @ pose.rest("Head").inverted() @ MUZZLE
        if best is None or abs(lips.z - height) < best[0]:
            best = (abs(lips.z - height), bend, lips)
    print(f"horse: grazing lips at {tuple(round(c, 3) for c in best[2])}")
    return best[1]


# Gaits: seconds, duty factor, stance sweep (m), the phase each leg lands,
# the swing lift of the fetlock (fore, hind) and the hoof's fold.
# The walk's head nod, degrees at the base of the neck (see ``walk``).
WALK_NOD = 3.0
WALK = dict(seconds=56 / 48, duty=0.60, sweep=0.875, phase=dict(LH=0.0, LF=0.25, RH=0.5, RF=0.75),
            lift=(0.14, 0.10), fold=(85.0, 60.0), centre=dict(F=-0.03, H=-0.06), strike=8.0, breakover=40.0)
# The gallop's duty factor and stance sweep are (fore, hind): the hinds land
# far under the belly and drive back to full extension (the user's
# references, L43), so they sweep further and stand longer at the same
# ground speed. ``reach``: how far (m) the fore hooves swing out beyond
# where they land before they come back to it (the lead fore reaching
# nearly straight); ``hip``: how much of each hind leg's reach its thigh
# takes in the stance (``hip_swing``: at the middle of the swing);
# ``hind_lead``: by what part of the swing a hind hoof is forward
# over its landing; ``hind_peak``: the hind fetlock is highest early in
# the swing (the exponent of its arc), folded up behind after the push.
GALLOP = dict(seconds=28 / 48, duty=(0.24, 0.24 * 1.36 / 1.02), sweep=(1.02, 1.36), phase=dict(RH=0.0, LH=0.12, RF=0.40, LF=0.52),
              lift=(0.46, 0.18), fold=(60.0, 55.0), centre=dict(F=-0.12, H=-0.19), strike=10.0, breakover=50.0,
              reach=0.10, hip=0.62, hip_swing=0.75, hind_lead=0.75, hind_peak=0.5, fore_peak=1.6, sling=1.0)


def per_leg(gait, key, leg):
    """``gait[key]`` for ``leg``: one value, or (fore, hind)."""
    v = gait[key]
    return v[0 if leg[1] == "F" else 1] if isinstance(v, tuple) else v


def ground_speed(gait):
    return per_leg(gait, "sweep", "LF") / (per_leg(gait, "duty", "LF") * gait["seconds"])


def stride(gait, leg, t):
    """The fetlock and the hoof roll of ``leg`` at phase ``t``."""
    q = (t - gait["phase"][leg]) % 1.0
    duty, half = per_leg(gait, "duty", leg), per_leg(gait, "sweep", leg) / 2
    c = gait["centre"][leg[1]]
    strike, breakover = gait["strike"], gait["breakover"]
    if q < duty:
        s = q / duty
        toe = strike * (1 - ease(s / 0.12)) - breakover * ease((s - 0.72) / 0.28)
        return hoof_place(leg, c - half + 2 * half * s, toe), toe
    s = (q - duty) / (1 - duty)
    a = hoof_place(leg, c + half, -breakover)
    b = hoof_place(leg, c - half, strike)
    fore = leg[1] == "F"
    lift = gait["lift"][0 if fore else 1]
    fold = gait["fold"][0 if fore else 1]
    # A galloping hind comes forward early, its hoof ahead of the hock and
    # under the belly by three quarters of the swing (``hind_lead``).
    y = a.y + (b.y - a.y) * ease(s if fore else min(1.0, s / gait.get("hind_lead", 1.0)))
    if fore:
        # Out beyond the landing and back onto it (the swing's retraction).
        y -= gait.get("reach", 0.0) * ease((s - 0.45) / 0.35) * (1 - ease((s - 0.82) / 0.18))
    z = a.z + (b.z - a.z) * s + lift * math.sin(math.pi * s ** (gait.get("fore_peak", 0.8) if fore else gait.get("hind_peak", 1.0))) ** 0.9
    toe = -breakover + (strike + breakover) * ease(s) - fold * math.sin(math.pi * min(1.0, s * 1.15)) ** 1.3
    return Vector((a.x, y, z)), toe


def locomotion(arm, t, gait, body, neck, slide, flex=0.0, hair=None, ears=((10, 0), (10, 0)), yaw=0.0):
    pose = Pose(arm)
    pose.carry("Body", body)
    if flex:
        pose.turn("Torso2", rot(body.to_3x3() @ X, flex))
    pose_neck(pose, body, neck, yaw)
    pose_face(pose, ears)
    pose_hair(pose, **(hair or {}), t=t)
    for leg in LEGS:
        f, toe = stride(gait, leg, t)
        q = (t - gait["phase"][leg]) % 1.0
        # The shoulder blade and the pelvis follow the leg: forward as it
        # reaches, back as it pushes.
        duty = per_leg(gait, "duty", leg)
        s = (q / duty) if q < duty else 1 - (q - duty) / (1 - duty)
        # In the swing the thigh takes more of the reach, so the stifle
        # comes forward and the hock flexes no further than a right angle.
        hip = gait.get("hip", 0.55)
        if q >= duty:
            hip += (gait.get("hip_swing", hip) - hip) * math.sin(math.pi * (q - duty) / (1 - duty))
        # Held straight by the sling while it stands, eased in and out over
        # the first and last tenth of the stance.
        sling = gait.get("sling", 0.0) * (ease(q / duty / 0.1) * ease((1 - q / duty) / 0.1) if q < duty else 0.0)
        actual = pose_leg(pose, body, leg, f, toe, slide=slide * (1 - 2 * ease(s)) * (1.0 if leg[1] == "F" else 0.5), hip=hip,
                          sling=sling)
        if q < duty:
            # How far a standing hoof falls short of its mark (beyond reach).
            SHORT[leg] = max(SHORT.get(leg, 0.0), (actual - f).length)
    return pose


def walk(arm, t):
    # Four beats; the body dips a little at each step and rolls over the
    # standing legs; the head nods twice a cycle; ears forward, the tail
    # swaying with the hips.
    body = body_matrix(lift=-0.02 + 0.008 * wave(t, 2, 0.1), nose_down=0.8 * wave(t, 2, 0.35), roll=1.2 * wave(t, 1, 0.2))
    # About 9 cm up and down at the poll, twice a stride (Loscher et al.
    # 2016: 9.1 +- 3.4 cm for the head and neck of walking horses).
    nod = WALK_NOD * wave(t, 2, 0.05)
    neck = (-4.0 + nod, 0.5 * nod, 0.0, -0.6 * nod)
    hair = dict(mane=4.0, forelock=4.0, tail=(10.0, 0.1), lag=0.05, cycles=2.0)
    ears = ((5 + 25 * flick(t, 0.4, 0.08), 10), (5, 10 + 25 * flick(t, 0.85, 0.08)))
    return locomotion(arm, t, WALK, body, neck, slide=0.06, hair=hair, ears=ears)


def gallop(arm, t):
    # Transverse gallop on the left lead (the user's references, L42-L43):
    # the hinds land far under the belly with the croup low (the body most
    # nose up as the right hind lands), drive back to full extension, then
    # the fores land, the lead fore reaching nearly straight, and the body
    # sails with the legs gathered under it. The neck rises over the hinds
    # and falls over the fores; the mane and the tail stream behind. The
    # body rocks about 11 degrees from end to end over a stride (a real
    # gallop rocks 10 to 15), which the rider's loose arm follows
    # (``rider.rs`` ``FREE_SWING``). The back rounds in the game, in phase
    # with the hinds' stance (``gait.rs`` ``GATHER``); the clip bends it 2
    # degrees the other way there, so the rigid bastos stay on the back
    # (bending it with the game, they rocked off 3.3 cm at 2 degrees; with
    # none, 2.3 cm; with this, 1.7 cm).
    # Nose up 5.5 degrees as the hinds land (the croup low), but only 2.5
    # nose down over the fores: more drops the chest onto the standing
    # forelegs.
    pitch = wave(t, 1, -0.25)
    body = body_matrix(lift=-0.05 + 0.05 * wave(t, 1, 0.1), nose_down=(5.5 if pitch < 0 else 2.5) * pitch)
    pump = 7.0 * wave(t, 1, 0.08)
    neck = (6.0 + pump, -2.0 + 0.4 * pump, -2.0, -6.0 - 0.6 * pump)
    # The tail carried up at the dock and streaming back in a curve, a wave
    # running down it each stride (Muybridge, The Horse in Motion).
    hair = dict(mane=10.0, mane_lift=-12.0, forelock=10.0, tail=(12.0, 0.0), tail_lift=50.0, lag=0.09, cycles=1.0,
                tail_curve=8.0, tail_whip=9.5)
    flex = -2.0 * wave(t, 1, 0.15)
    return locomotion(arm, t, GALLOP, body, neck, slide=0.09, flex=flex, hair=hair, ears=((22, 5), (22, 5)))


# The jump keys, in the original's 24 fps frames over 35: the approach
# (the gallop's start), the gather (4-8), the take-off (8-10), the flight
# (10-28, the top near 18), the landing (28-30) and back into the gallop.
JUMP_SECONDS = 70 / 48
# When each hind hoof plants for the take-off and leaves the ground, in the
# same frames of 35: the right hind a little ahead of the left, both after
# the fores have started to lift (8.5).
JUMP_PLANT = {"RH": (7.0, 10.6), "LH": (7.5, 11.0)}
# Where (m from rest, ahead negative) the hinds plant for the take-off: less
# far under the belly than the gallop's landing, so they leave the ground
# stretched back to about their full length.
JUMP_HIND_FROM = -0.61
# When each fore unfolds for the landing, in the same frames: only in the
# last tenth of the game's flight (frames 25-27.6, flight 0.9 to 1.0; the
# game's arc falls 1.55 m), so the hoof reaches the ground as the body
# comes down to it rather than hanging below a body still in the air; the
# trailing fore (RF) a beat ahead of the leading one (LF), which touches
# down just after the flight ends.
JUMP_REACH = {"RF": (25.0, 27.6), "LF": (25.6, 28.4)}


def jump_key(t, keys):
    """Smoothly interpolated value at ``t`` from ``keys`` [(frame of 35,
    value), ...]."""
    f = t * 35
    for (f0, v0), (f1, v1) in zip(keys, keys[1:]):
        if f0 <= f <= f1:
            s = ease((f - f0) / (f1 - f0))
            if isinstance(v0, tuple):
                return tuple(a + (b - a) * s for a, b in zip(v0, v1))
            return v0 + (v1 - v0) * s
    return keys[-1][1]


def gallop_jump(arm, t):
    lift = jump_key(t, [(0, 0.0), (4, -0.02), (7, -0.17), (9, -0.08), (13, 0.30), (18, 0.50), (23, 0.36), (28, 0.02), (30, -0.08), (33, -0.01), (35, 0.0)])
    nose = jump_key(t, [(0, 0.0), (4, -4.0), (8, -14.0), (11, -18.0), (17, 0.0), (24, 14.0), (28, 10.0), (31, -3.0), (35, 0.0)])
    gallop_t = (t * JUMP_SECONDS / GALLOP["seconds"]) % 1.0
    base = body_matrix(lift=-0.05 + 0.05 * wave(0.0, 1, 0.1), nose_down=3.5 * wave(0.0, 1, -0.05))
    body = Matrix.Translation(Vector((0, 0, lift))) @ body_matrix(nose_down=nose, pivot=Vector((0, 0.1, 1.1))) @ base
    neck = jump_key(t, [(0, (6.0, -2.0, -2.0, -6.0)), (6, (12.0, 0.0, -2.0, -4.0)), (10, (-6.0, -4.0, 0.0, 4.0)),
                        (18, (-12.0, -6.0, 0.0, 8.0)), (26, (10.0, 2.0, 0.0, -6.0)), (30, (2.0, -2.0, -2.0, -4.0)), (35, (6.0, -2.0, -2.0, -6.0))])
    flex = jump_key(t, [(0, 0.0), (6, 8.0), (9, -6.0), (14, 4.0), (24, -4.0), (30, 6.0), (35, 0.0)])
    pose = Pose(arm)
    pose.carry("Body", body)
    pose.turn("Torso2", rot(body.to_3x3() @ X, flex))
    pose_neck(pose, body, neck)
    pose_face(pose, ((10, 0), (10, 0)))
    pose_hair(pose, mane=8.0, mane_lift=jump_key(t, [(0, -10.0), (12, 10.0), (20, -18.0), (35, -10.0)]), forelock=8.0,
              tail=(10.0, 0.0), tail_lift=jump_key(t, [(0, 50.0), (12, 50.0), (22, 60.0), (35, 50.0)]), lag=0.06, t=t, cycles=2.0,
              tail_curve=4.0, tail_whip=6.0)
    # Legs: before the take-off and after the landing they follow the
    # gallop's footfalls at its own pace; in the air the fores fold tight
    # under the chest and the hinds trail, then tuck. The fores leave the
    # ground first; the hinds come down under the belly while the fores
    # lift (JUMP_PLANT), push the horse up and forward while their hooves
    # slide back at the gallop's ground speed, and leave last, stretched
    # back (the game's flight starts there: ``horse.rs`` ``JUMP_FLIGHT``).
    frame = t * 35
    air = ease((frame - 8.5) / 2.0) * (1 - ease((frame - 28.5) / 2.5))
    for leg in LEGS:
        fore = leg[1] == "F"
        gt = (gallop_t if t * 35 < 18 else (gallop_t + 0.0)) % 1.0
        f_ground, toe_ground = stride(GALLOP, leg, gt)
        if fore:
            # Folded tight over the top and most of the descent, then
            # unfolding late, forward and down, the trailing fore (RF on the
            # left lead) a frame ahead of the leading one, so it lands first
            # (Muybridge, Animal Locomotion pl. 639).
            on, off = JUMP_REACH[leg]
            reach = jump_key(t, [(0, 0.0), (on, 0.0), (off, 1.0), (35, 1.0)])
            tuck = body @ (fetlock_rest(leg) + Vector((0, 0.10, 0.48)).lerp(Vector((0, -0.22, 0.10)), reach))
            toe_air = -120.0 * (1.0 - 0.85 * reach)
            f = f_ground.lerp(tuck, air)
            toe = toe_ground + (toe_air - toe_ground) * air
        else:
            # Stretched back as they leave, then flexed and trailing out
            # behind the body over the top of the arc (retroflexed: hocks
            # bent, hooves behind the stifles, never under the belly), and
            # hanging back, opening, through the descent while the fores
            # land; they swing forward under the body only after that, in
            # the gallop that takes over (``horse.rs`` ``GALLOP_LANDING``).
            trail = jump_key(t, [(0, 1.0), (12, 1.0), (16, 0.0), (35, 0.0)])
            land = jump_key(t, [(0, 0.0), (20, 0.0), (26, 1.0), (35, 1.0)])
            fold = Vector((0, 0.18 + 0.12 * trail, 0.30 - 0.30 * trail))
            tuck = body @ (fetlock_rest(leg) + fold.lerp(Vector((0, 0.30, 0.12)), land))
            toe_air = (-70.0 - 30.0 * trail) * (1.0 - 0.5 * land)
            on, off = JUMP_PLANT[leg]
            # Planted from ``on`` to ``off``, sliding back from under the
            # belly; the toe breaks over as the hoof leaves.
            pushed = min(max(frame - on, 0.0), off - on) * JUMP_SECONDS / 35
            y = JUMP_HIND_FROM + ground_speed(GALLOP) * pushed
            toe_push = -GALLOP["breakover"] * ease((frame - (off - 1.5)) / 1.5)
            planted = hoof_place(leg, y, toe_push)
            hold = ease((frame - (on - 2.5)) / 2.5) * (1 - ease(frame - 25.5))
            hind_air = ease((frame - off) / 2.0) * (1 - ease((frame - 28.5) / 2.5))
            f = f_ground.lerp(planted, hold).lerp(tuck, hind_air)
            toe = toe_ground + (toe_push - toe_ground) * hold
            toe = toe + (toe_air - toe) * hind_air
        pose_leg(pose, body, leg, f, toe, hip=0.55)
    return pose


def measure(arm, action, seconds):
    """The ground speed the hooves show (the game's way: the mean backward
    slide of each IK hoof while within 1.5 cm of its lowest point), and the
    lowest and highest hoof heights."""
    scene = bpy.context.scene
    arm.animation_data_create()
    arm.animation_data.action = action
    frames = round(seconds * rig.FPS)
    track = {leg: [] for leg in LEGS}
    for f in range(frames + 1):
        scene.frame_set(f)
        for leg in LEGS:
            pb = arm.pose.bones[leg_bones(leg)["ik"]]
            track[leg].append((arm.matrix_world @ pb.head).copy())
    arm.animation_data.action = None
    speeds = []
    for leg, pts in track.items():
        low = min(p.z for p in pts)
        for a, b in zip(pts, pts[1:]):
            if a.z < low + 0.015 and b.z < low + 0.015:
                speeds.append((b.y - a.y) * rig.FPS)
    lows = [min(p.z for p in pts) for pts in track.values()]
    return (sum(speeds) / len(speeds) if speeds else 0.0), lows


def clips(arm):
    global GRAZE_BEND
    GRAZE_BEND = graze_bend(arm)
    print(f"horse: grazing bend {GRAZE_BEND:.1f} degrees")
    made = [
        rig.bake_clip(arm, "Idle", 160 / 48, idle, MOVED),
        rig.bake_clip(arm, "Idle_2", 160 / 48, idle_2, MOVED),
        rig.bake_clip(arm, "Idle_Headlow", 160 / 48, idle_headlow, MOVED),
        rig.bake_clip(arm, "Eating", 288 / 48, eating, MOVED),
        rig.bake_clip(arm, "Walk", WALK["seconds"], walk, MOVED),
    ]
    SHORT.clear()
    made += [
        rig.bake_clip(arm, "Gallop", GALLOP["seconds"], gallop, MOVED),
        rig.bake_clip(arm, "Gallop_Jump", JUMP_SECONDS, gallop_jump, MOVED),
    ]
    print(f"horse: in the gallop the standing hooves fall short of their marks by at most "
          f"{ {k: round(v, 3) for k, v in sorted(SHORT.items())} } m")
    for a, seconds in ((made[4], WALK["seconds"]), (made[5], GALLOP["seconds"])):
        speed, lows = measure(arm, a, seconds)
        print(f"horse: {a.name} {seconds:.3f} s: hooves slide back at {speed:.2f} m/s; lowest IK hooves {[round(z, 3) for z in lows]}")
    print(f"horse: planned walk {ground_speed(WALK):.2f} m/s, gallop {ground_speed(GALLOP):.2f} m/s")
    return made


main()
