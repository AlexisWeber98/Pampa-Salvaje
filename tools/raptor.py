"""Model, rig, paint and animate the pampa's two caracaras in Blender, by
script: the chimango (Milvago chimango) and the carancho (Caracara
plancus), from one falcon's layout and rig.

Run it headless, once per species:

    blender -b --factory-startup --python tools/raptor.py -- OUT.glb \
        --species chimango|carancho [--textures DIR] [--preview DIR] \
        [--shots REGEX] [--scans DIR] [--faces DIR] [--draft] [--no-clips]

Writes OUT.glb: one skinned mesh (``Chimango`` or ``Carancho``) with the
materials ``Main`` (the body, the head, the bill, the legs and the wings'
coverts: one sculpt baked into its own colour, normal, roughness and
occlusion maps), ``Feathers`` (the flight feathers, the tail, the coverts, the scapulars and
the carancho's crest: alpha-tested cards, two-sided as two layers, upper and
under side each with its own pattern, on a painted atlas) and ``Eye``;
its armature ``RaptorArmature`` (``RAPTOR_BONES``: the body, two neck bones
and the head, the tail with two fanning bones, four bones a wing and four
a leg); and the clips ``Glide`` (soaring, the wings held in a shallow
dihedral, small balancing corrections, the tail twisting and the head
looking down), ``Bank_L`` and ``Bank_R`` (the same in a banked turn, the
head kept level), ``Flap`` (three wingbeats at the frequency Pennycuick's
allometry gives for the model's mass, span and wing area, the hand folding
and the primaries opening on the upstroke). Life size in metres, Z up, the bird flying toward -Y
(glTF +Z) with its body centre at the origin.

Real pixels (v4): ``--scans DIR`` cuts the Feather Atlas scans of a
Crested Caracara (``SCANS``: public domain) into the carancho's flight
feathers, tail, coverts and scapulars (``cut_scan``, ``feather_cell``),
and (v5) lends the chimango's painted cells the grain of its secondaries
(``scan_grain``, ``regrain``: luminance detail only);
``--faces DIR`` projects a side photograph of each species
(``FACE_PHOTOS``: CC0 / CC BY) over its head's colour map
(``project_face``; the chimango's only as luminance detail over the
painted colours) and takes its iris colour. Without them the atlas and
the head stay painted. The greater, median and lesser coverts and the
scapulars are cards laid on the wing's and the back's surfaces
(``lying``).

The plumage (reference photographs: Wikimedia Commons, both species; see
tools/README.md and CREDITS.md) is painted per texel (``plumage``) on the
baked sculpt: the feathers' scalloped tips, fringes, shaft streaks and bars
are a pattern laid along the feather tracts (``feather_cells``), so it holds
at any map size. The flight feathers are painted one by one into the
atlas (``paint_feather``): the vane's outline with the outer primaries'
emarginations (the caracaras' fingers), the rachis, the barbs, the bars, the
carancho's barred white window and the tail's dark bands.

Chimango (v5): buff head fading into the neck, a small almond eye set
into the skull (a shallow dome under the lids) with a dark brown stripe
behind it and a narrow pale ring round it, a pinkish-orange cere, a pale
bluish-grey horn bill with a darker hook, a buff-brown body of irregular
feathers (``ragged``) with brown shaft streaks below, brown upperparts with buff fringes, pale buff primaries'
bases and barred pale tail with a broad dark band before its pale tip,
pale yellowish legs. Carancho: black cap and shaggy crest, bare orange
face with a yellower cere, a deep bluish bill, cream cheeks, throat and neck, the breast and the
mantle barred black and white, black belly and thighs, dark wings with a
barred white window in the outer primaries, the tail white with fine dark
bars and a broad black end band, yellow legs.
"""

import math
import os
import re
import sys

try:
    import bpy
except ImportError:  # outside Blender: the layout, the plumage and the clips' timing (tests)
    bpy = None

HERE = os.path.dirname(os.path.abspath(__file__))
if bpy is not None:
    sys.path.insert(0, HERE)
    import animal_rig as rig
    from mathutils import Matrix, Quaternion, Vector

FPS = 60  # a wingbeat lasts about a quarter of a second: 30 frames a second is too coarse for it

# ---------------------------------------------------------------------------
# The species. Lengths in metres; the literature's values (Ferguson-Lees &
# Christie, "Raptors of the World"; Narosky & Yzurieta, "Aves de Argentina y
# Uruguay"): chimango total length 37-40 cm, wingspan 85-95 cm, 280-300 g;
# carancho 50-64 cm, 120-132 cm, 1.1-1.6 kg.

SPECIES = {
    "chimango": dict(
        mesh="Chimango",
        length=(0.37, 0.40),
        span=(0.85, 0.95),
        mass=0.29,
        torso=(0.036, 0.084, 0.037),  # half width, half length, half height
        neck=0.040,
        head=(0.023, 0.029, 0.021),  # half width, half length, half height
        crown=0.25,  # flattening of the crown
        # Bill and eye (v4) measured on a side photograph (iNaturalist
        # 291627011): the eye's centre 0.71 of the way from it to the
        # bill's front back from the horn's start, the iris 0.29 of it
        # across, 0.13 of it over the culmen's base.
        bill=(0.011, 0.012, 0.0085),  # length past the cere, depth and width at the base
        cere=0.006,
        # v5 (coordinator review: the eye read too big, round and bulging):
        # the visible eye smaller, an almond between the lids, its cornea a
        # shallow dome set into the skull (``eye_lens``: the dome's depth
        # over its radius; ``eye_open``: the lids' opening, along and up,
        # over it), as in the Valdivia and Rio Grande do Sul photographs.
        eye=0.0046,  # the visible eye's radius
        eye_at=(0.86, -0.09, 0.40),  # the eye's centre across, along and up the head (of its half sizes)
        eye_lens=0.45,
        eye_open=(1.00, 0.70),
        face=0.0,  # how far the bare face skin reaches back from the cere (0: a ring round the eye)
        ring=1.25,  # the bare skin round the eye, out to this many eye radii
        lores=False,  # feathered lores (bristles), no bare strip from the eye to the cere
        soft_head=True,  # the head's paint and the face photo fade into the neck's (no seam)
        ragged=0.55,  # the body feathers' irregularity: their edges, sizes and lie (0: the regular tract)
        aspect=0.72,  # the body feathers' width over their length (narrower, more pointed than the default)
        relief=0.6,  # the feathers' relief over the default's
        bar_wave=0.4,  # the flight feathers' bars' waviness over the default's (the photographs' are nearly straight)
        calamus=False,  # no bare quill at the cards' roots (it stuck out of the wing's leading edge)
        bar_shift=0.25,  # how far each feather's bars are shifted from its neighbours' (1: anywhere; staggered bars read as a checker)
        tail=0.172,
        tail_w=0.030,
        tail_round=0.10,  # the outer pair this much shorter than the central
        wing=(0.058, 0.070, 0.062),  # humerus, forearm, hand
        prim=0.296,
        prim_w=0.042,
        sec=0.140,
        sec_w=0.032,
        fingers=5,
        leg=(0.042, 0.052, 0.026),  # tibia, tarsus, middle toe
        leg_r=0.0045,  # the tarsus' radius
        dihedral=6.0,  # degrees: the chimango soars with its wings a little raised
        crest=0,
    ),
    "carancho": dict(
        mesh="Carancho",
        length=(0.50, 0.64),
        span=(1.20, 1.32),
        mass=1.30,
        torso=(0.052, 0.122, 0.054),
        neck=0.076,
        head=(0.030, 0.040, 0.029),
        crown=0.35,
        # As the chimango's, on a side photograph (Commons, "Caracará
        # Southern Caracara", Bart van Dorp): 0.67, the iris 0.23, level.
        bill=(0.021, 0.020, 0.0130),
        cere=0.010,
        eye=0.0071,
        eye_at=(0.74, -0.12, 0.22),
        face=0.020,
        tail=0.262,
        tail_w=0.042,
        tail_round=0.04,
        wing=(0.088, 0.100, 0.085),
        prim=0.405,
        prim_w=0.060,
        sec=0.205,
        sec_w=0.046,
        fingers=6,
        leg=(0.070, 0.090, 0.045),
        leg_r=0.0065,
        dihedral=1.5,
        crest=110,
    ),
}

NAME = "chimango"
SP = SPECIES[NAME]


def bird_scale():
    """The species' size over the chimango's (by the forearm)."""
    return SP["wing"][1] / SPECIES["chimango"]["wing"][1]


def configure(name):
    global NAME, SP
    NAME = name
    SP = SPECIES[name]


# ---------------------------------------------------------------------------
# Small vector helpers (plain tuples: the layout runs without Blender).


def add(a, b):
    return tuple(x + y for x, y in zip(a, b))


def sub(a, b):
    return tuple(x - y for x, y in zip(a, b))


def mul(a, k):
    return tuple(x * k for x in a)


def dot(a, b):
    return sum(x * y for x, y in zip(a, b))


def cross(a, b):
    return (a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0])


def norm(a):
    n = math.sqrt(dot(a, a))
    return tuple(x / n for x in a)


def length(a):
    return math.sqrt(dot(a, a))


def lerp(a, b, t):
    return tuple(x + (y - x) * t for x, y in zip(a, b))


def smooth(t):
    t = min(max(t, 0.0), 1.0)
    return t * t * (3 - 2 * t)


def heading(deg, z=0.0):
    """A unit direction in the wing's plane: ``deg`` from straight out
    (+X) toward the back (+Y)."""
    a = math.radians(deg)
    return norm((math.cos(a), math.sin(a), z))


# ---------------------------------------------------------------------------
# The layout: the left half (+X: the bird's left, as it flies toward -Y);
# the right is its mirror.


def torso_centre():
    return (0.0, 0.0, 0.0)


def neck_base():
    hw, hl, hh = SP["torso"]
    return (0.0, -0.80 * hl, 0.30 * hh)


def head_centre():
    hw, hl, hh = SP["torso"]
    return add(neck_base(), (0.0, -0.80 * SP["neck"], 0.55 * SP["neck"] + 0.10 * hh))


def bill_base():
    """The front of the cere, where the horn of the bill starts."""
    c = head_centre()
    w, l, h = SP["head"]
    return add(c, (0.0, -0.92 * l - SP["cere"], -0.12 * h))


def bill_tip():
    b = bill_base()
    L, D, _ = SP["bill"]
    return add(b, (0.0, -0.94 * L, -0.50 * D))


def eye_centre(side=1.0):
    c = head_centre()
    w, l, h = SP["head"]
    ex, ey, ez = SP["eye_at"]
    return add(c, (side * ex * w, ey * l, ez * h))


def tail_root():
    hw, hl, hh = SP["torso"]
    return (0.0, 0.92 * hl, 0.10 * hh)


def shoulder(side=1.0):
    hw, hl, hh = SP["torso"]
    return (side * 0.62 * hw, -0.38 * hl, 0.50 * hh)


def wing_joints(side=1.0):
    """Shoulder, elbow, wrist and the hand's end in the gliding rest pose:
    the arm a little back, the forearm forward to the wrist (the leading
    edge's bend), the hand swept a little back."""
    h, f, m = SP["wing"]
    s = shoulder(side)
    e = add(s, mul(heading(14.0), h))
    w = add(e, mul(heading(-24.0), f))
    t = add(w, mul(heading(12.0), m))
    mirror = lambda p: (side * abs(p[0]), p[1], p[2])
    return [mirror(p) for p in (s, e, w, t)]


def leading_edge(side=1.0):
    """The wing's leading edge in its plane: from the shoulder, a little
    ahead of the bones (the patagium), to the hand's end."""
    s, e, w, t = wing_joints(side)
    hw, hl, hh = SP["torso"]
    ahead = 0.10 * SP["sec"]
    return [s, add(e, (0, -0.70 * ahead, 0)), add(w, (0, -0.35 * ahead, 0)), t]


def chord_at(r):
    """The coverts' depth behind the leading edge at ``r`` (0 shoulder, 1
    the hand's end): to the greater coverts' tips."""
    root, wrist, tip = 0.58 * SP["sec"], 0.46 * SP["sec"], 0.20 * SP["prim"]
    rw = wrist_fraction()
    if r <= rw:
        return root + (wrist - root) * smooth(r / rw)
    return wrist + (tip - wrist) * ((r - rw) / (1 - rw))


def wrist_fraction():
    h, f, m = SP["wing"]
    return (h + f) / (h + f + m)


PRIMARY_LENGTHS = (0.60, 0.64, 0.69, 0.75, 0.82, 0.90, 0.97, 1.00, 0.95, 0.74)
PRIMARY_ANGLES = (86.0, 82.5, 78.5, 74.0, 69.0, 63.5, 57.5, 51.0, 44.0, 37.0)


def remiges(side=1.0, spread=1.0):
    """The flight feathers of one wing: dicts with the feather's ``kind``
    (``P`` primary, ``S`` secondary or tertial), ``index`` (P1-P10 from the
    wrist out; S1-S12 from the wrist in, S13-S15 the tertials), ``root``,
    ``dir`` (along the feather), ``normal`` (up), ``length``, ``width``,
    ``outer`` (the outer vane's share of the width), the stacking ``lift``
    (each feather a little over the next toward the tip, as in a real wing
    seen from above) and its ``bones`` (bone weights). ``spread`` opens
    (above 1) or closes the fingers."""
    s, e, w, t = wing_joints(1.0)
    out = []
    n_feathers = 25
    order = 0
    # Primaries: P1-P6 rooted along the hand, P7-P10 on the digits at its end.
    for i in range(1, 11):
        k = i - 1
        f = min(k / 6.0, 1.0)
        root = lerp(w, t, 0.10 + 0.90 * f)
        if i > 6:
            root = add(t, mul(heading(PRIMARY_ANGLES[k] - 25.0), 0.010 * (i - 6) * SP["prim"] / 0.2))
        ang = 90.0 - (90.0 - PRIMARY_ANGLES[k]) * (1.0 if i < 6 else spread)
        finger = i > 10 - SP["fingers"]
        out.append(dict(
            kind="P", index=i, root=root, dir=heading(ang), length=PRIMARY_LENGTHS[k] * SP["prim"],
            width=SP["prim_w"] * (0.80 if i == 10 else 1.0), outer=0.30 if i > 4 else 0.38, finger=finger,
            bones={"Hand": 1.0 - min(max((i - 5) / 4.0, 0.0), 1.0), "Tip": min(max((i - 5) / 4.0, 0.0), 1.0)},
            order=12 + 3 + i,
        ))
    # Secondaries S1 (at the wrist) to S12 (at the elbow), then three tertials.
    for i in range(1, 16):
        if i <= 12:
            f = (i - 1) / 11.0
            root = lerp(w, e, 0.04 + 0.92 * f)
            ang = 80.0 + 14.0 * f
            ln = SP["sec"] * (1.0 - 0.06 * f)
            bones = {"Forearm": 1.0}
            if i == 1:
                bones = {"Forearm": 0.7, "Hand": 0.3}
            elif i >= 11:
                bones = {"Forearm": 0.6, "Shoulder": 0.4}
        else:
            f = (i - 12) / 3.0
            root = lerp(e, s, 0.15 + 0.55 * f)
            ang = 100.0 + 8.0 * f
            ln = SP["sec"] * (0.96 - 0.16 * f)
            bones = {"Shoulder": 1.0 - 0.2 * f, "Body": 0.2 * f}
        out.append(dict(
            kind="S", index=i, root=root, dir=heading(ang), length=ln, width=SP["sec_w"] * (0.9 if i > 12 else 1.0),
            outer=0.42, finger=False, bones=bones, order=15 - i,
        ))
    for fd in out:
        fd["root"] = add(fd["root"], (0, 0, -0.0002 * fd["order"] * bird_scale()))
        fd["normal"] = (0.0, 0.0, 1.0)
        fd["side"] = 1.0
    return [mirror_feather(fd, side) for fd in out]


def mirror_feather(fd, side):
    m = lambda p: (side * p[0], p[1], p[2])
    fd = dict(fd)
    fd["root"], fd["dir"], fd["normal"] = m(fd["root"]), m(fd["dir"]), m(fd["normal"])
    fd["side"] = side
    fd["bones"] = {mirror_bone(b, side): w for b, w in fd["bones"].items()}
    return fd


def mirror_bone(bone, side):
    if bone in SIDED:
        return f"{bone}.{'L' if side > 0 else 'R'}"
    return bone


SIDED = ("Shoulder", "Forearm", "Hand", "Tip", "Thigh", "Shin", "Tarsus", "Toes")


def rectrices(fan=18.0):
    """The twelve tail feathers, both halves: the central pair on top and
    each outer pair a little lower, fanned ``fan`` degrees at the outermost."""
    r = tail_root()
    hw, hl, hh = SP["torso"]
    out = []
    for side in (1.0, -1.0):
        for i in range(1, 7):
            f = (i - 0.5) / 6.0
            ang = math.radians(fan * f)
            d = norm((side * math.sin(ang), math.cos(ang), -0.04))
            root = add(r, (side * 0.32 * hw * f, 0.0, -0.0004 * (i - 1) * bird_scale()))
            g = min((i - 1) / 4.0, 1.0)
            out.append(dict(
                kind="R", index=i, root=root, dir=d, normal=(0.0, 0.0, 1.0), side=side,
                length=SP["tail"] * (1.0 - SP["tail_round"] * f * f), width=SP["tail_w"],
                outer=0.5 - 0.17 * g, finger=False,
                bones={"Tail": 1.0 - g, mirror_tail(side): g},
            ))
    return out


def mirror_tail(side):
    return "Tail.L" if side > 0 else "Tail.R"


def crest_feathers():
    """The carancho's shaggy black cap: a mop of narrow, ragged feathers
    over the crown, short on the forehead and long at the nape, where
    they overhang the neck in loose tufts."""
    import random

    rnd = random.Random(11)
    out = []
    c = head_centre()
    w, l, h = SP["head"]
    for i in range(SP["crest"]):
        fy = rnd.uniform(-0.45, 1.05)  # forehead to nape
        fx = rnd.uniform(-1.0, 1.0)
        back = max(fy, 0.0)
        root = add(c, (0.85 * fx * w * (1 - 0.25 * abs(fy)), fy * 0.72 * l, (1.00 - 0.22 * fx * fx - 0.25 * back * back) * h))
        d = norm((0.7 * fx + rnd.uniform(-0.35, 0.35), 1.0, 0.30 - 0.55 * back + rnd.uniform(-0.15, 0.20)))
        nrm = norm(cross(d, (1.0, 0.0, 0.0)))
        if nrm[2] < 0:
            nrm = mul(nrm, -1)
        ln = l * (0.30 + 0.70 * back ** 1.2) * rnd.uniform(0.7, 1.3)
        out.append(dict(kind="C", index=1 + i % 3, root=root, dir=d, normal=nrm, side=1.0,
                        length=ln, width=0.42 * w * rnd.uniform(0.8, 1.3), outer=0.5, finger=False,
                        bones={"Head": 1.0}))
    return out


def feather_tip(fd):
    return add(fd["root"], mul(fd["dir"], fd["length"]))


def half_span():
    return max(abs(feather_tip(fd)[0]) for fd in remiges())


def total_length():
    """Bill tip to the central tail feathers' tip."""
    tail_tip = max(feather_tip(fd)[1] for fd in rectrices())
    return tail_tip - bill_tip()[1]


def wing_area():
    """Both wings and the body between them, seen from above (Pennycuick's
    ``S``): the outline through the leading edge, the feathers' tips and
    back along the body."""
    edge = leading_edge()
    tips = [feather_tip(fd) for fd in remiges()]
    # Trailing outline from the wing tip back in: the furthest-back tip at
    # each span band.
    bands = {}
    for p in tips:
        k = round(p[0] / 0.01)
        if k not in bands or p[1] > bands[k][1]:
            bands[k] = p
    trail = [bands[k] for k in sorted(bands, reverse=True)]
    poly = [(p[0], p[1]) for p in edge] + [(p[0], p[1]) for p in trail]
    poly.append((0.0, trail[-1][1]))
    poly.insert(0, (0.0, edge[0][1]))
    a = 0.0
    for (x0, y0), (x1, y1) in zip(poly, poly[1:] + poly[:1]):
        a += x0 * y1 - x1 * y0
    return 2 * abs(a) / 2


def flap_frequency():
    """Wingbeats a second by Pennycuick's allometry (J. Exp. Biol. 199,
    1996): f = m^(3/8) g^(1/2) b^(-23/24) S^(-1/3) rho^(-3/8)."""
    m, g, rho = SP["mass"], 9.81, 1.2
    b, s = 2 * half_span(), wing_area()
    return m ** 0.375 * g ** 0.5 * b ** (-23 / 24) * s ** (-1 / 3) * rho ** (-0.375)


FLAP_BEATS = 3


def flap_seconds():
    """The Flap clip: ``FLAP_BEATS`` wingbeats, a whole number of frames."""
    return round(FLAP_BEATS * FPS / flap_frequency()) / FPS


DOWNSTROKE = 0.56  # the downstroke's share of a wingbeat


def flap_angles(t):
    """The wing's joint angles (degrees) at phase ``t`` of one wingbeat:
    ``elev`` (the arm's elevation over the horizontal), ``elbow`` and
    ``wrist`` (flexion), ``twist`` (the hand's pronation, nose down),
    ``spread`` (the fingers opening). The downstroke (``t`` < ``DOWNSTROKE``)
    sweeps the extended wing down; the upstroke flexes the elbow and the
    wrist and lifts the folded hand back up, the primaries parted."""
    t = t % 1.0
    up, down = 52.0, -34.0
    if t < DOWNSTROKE:
        u = t / DOWNSTROKE
        elev = up + (down - up) * (0.5 - 0.5 * math.cos(math.pi * u))
        flex = 0.0
        twist = 10.0 * math.sin(math.pi * u)
        spread = 1.0
    else:
        u = (t - DOWNSTROKE) / (1 - DOWNSTROKE)
        elev = down + (up - down) * (0.5 - 0.5 * math.cos(math.pi * u))
        flex = math.sin(math.pi * u)
        twist = -14.0 * math.sin(math.pi * u)
        spread = 1.0 + 0.5 * math.sin(math.pi * u)
    return dict(elev=elev, elbow=30.0 * flex, wrist=48.0 * flex, twist=twist, spread=spread,
                hand=-12.0 * flex)


# ---------------------------------------------------------------------------
# The skeleton.

RAPTOR_BONES = ["Body", "Neck1", "Neck2", "Head", "Tail", "Tail.L", "Tail.R"] + [
    f"{b}.{s}" for s in "LR" for b in SIDED
]


def leg_joints(side=1.0):
    """Hip, knee, heel (intertarsal), the foot and the middle toe's tip,
    tucked for flight: the feathered tibia back and down under the belly,
    the bare tarsus back under the vent, the toes curled under the tail's
    coverts."""
    hw, hl, hh = SP["torso"]
    ti, ta, to = SP["leg"]
    hip = (side * 0.42 * hw, 0.15 * hl, -0.30 * hh)
    knee = add(hip, (0.0, -0.10 * ti, -0.45 * ti))
    heel = add(knee, mul(norm((0.0, 0.70, -0.40)), ti))
    heel = (side * 0.34 * hw, heel[1], max(heel[2], -0.86 * hh))
    foot = add(heel, mul(norm((0.0, 1.0, 0.34)), ta))
    toe = add(foot, mul(norm((0.0, 0.70, -0.70)), to))
    return [hip, knee, heel, foot, toe]


def bone_table():
    """``(name, head, tail, parent)`` in rest (the gliding pose)."""
    c = torso_centre()
    hw, hl, hh = SP["torso"]
    nb, hc = neck_base(), head_centre()
    nm = lerp(nb, hc, 0.5)
    tr = tail_root()
    bones = [
        ("Body", c, add(c, (0, -0.5 * hl, 0)), None),
        ("Neck1", nb, nm, "Body"),
        ("Neck2", nm, hc, "Neck1"),
        ("Head", hc, bill_tip(), "Neck2"),
        ("Tail", tr, add(tr, (0, 0.5 * SP["tail"], 0)), "Body"),
        ("Tail.L", add(tr, (0.10 * hw, 0, 0)), add(tr, (0.10 * hw + 0.12 * SP["tail"], 0.5 * SP["tail"], 0)), "Tail"),
        ("Tail.R", add(tr, (-0.10 * hw, 0, 0)), add(tr, (-0.10 * hw - 0.12 * SP["tail"], 0.5 * SP["tail"], 0)), "Tail"),
    ]
    for side, sx in ((1.0, "L"), (-1.0, "R")):
        s, e, w, t = wing_joints(side)
        tip_end = add(t, (side * 0.12 * SP["prim"], 0.03 * SP["prim"], 0))
        bones += [
            (f"Shoulder.{sx}", s, e, "Body"),
            (f"Forearm.{sx}", e, w, f"Shoulder.{sx}"),
            (f"Hand.{sx}", w, t, f"Forearm.{sx}"),
            (f"Tip.{sx}", t, tip_end, f"Hand.{sx}"),
        ]
        hip, knee, heel, foot, toe = leg_joints(side)
        bones += [
            (f"Thigh.{sx}", hip, knee, "Body"),
            (f"Shin.{sx}", knee, heel, f"Thigh.{sx}"),
            (f"Tarsus.{sx}", heel, foot, f"Shin.{sx}"),
            (f"Toes.{sx}", foot, toe, f"Tarsus.{sx}"),
        ]
    return bones


# ---------------------------------------------------------------------------
# The plumage, per texel (numpy). ``P`` (n, 3) points on the sculpt in rest,
# ``N`` their normals; colours are sRGB.


def _np():
    import numpy as np

    return np


def hash3(ix, iy, iz, salt):
    """A repeatable pseudo-random value in [0, 1) per integer cell."""
    np = _np()
    h = (ix.astype(np.int64) * 73856093) ^ (iy.astype(np.int64) * 19349663) ^ (iz.astype(np.int64) * 83492791) ^ (salt * 2654435761)
    h = (h ^ (h >> 13)) * 1274126177
    h = h ^ (h >> 16)
    return (h & 0xFFFFFF).astype(np.float64) / float(0x1000000)


def vnoise(P, freq, salt=0):
    """Smooth value noise in [0, 1] at the points ``P`` (n, 3), ``freq``
    cells a metre."""
    np = _np()
    q = np.asarray(P, np.float64) * freq
    i = np.floor(q)
    f = q - i
    f = f * f * (3 - 2 * f)
    ix, iy, iz = i[:, 0], i[:, 1], i[:, 2]
    out = 0.0
    for dx in (0, 1):
        for dy in (0, 1):
            for dz in (0, 1):
                w = (f[:, 0] if dx else 1 - f[:, 0]) * (f[:, 1] if dy else 1 - f[:, 1]) * (f[:, 2] if dz else 1 - f[:, 2])
                out = out + w * hash3(ix + dx, iy + dy, iz + dz, salt)
    return out


def feather_cells(u, v, size, salt=0, aspect=0.85, rag=None):
    """The overlapping feathers of a tract: ``u`` across and ``v`` along the
    feathers' lie (toward their tips, metres), feathers ``size`` long and
    ``aspect`` as wide, in staggered rows with rounded tips (``rag``, per
    point, in feather lengths: their edges' wear and split barbs). Returns, per
    point: ``d`` how far it is behind its visible feather's tip edge (0 at
    the edge, about 1 a feather's length in), ``a`` its place across that
    feather (-0.5..0.5, 0 on the shaft) and ``h`` the feather's own random
    value."""
    np = _np()
    sv, su = size, size * aspect
    row = np.floor(v / sv)
    fv = v / sv - row
    shift = 0.5 * np.mod(row, 2)
    col = np.floor(u / su + shift)
    a = u / su + shift - col - 0.5
    hv = hash3(row, col, np.zeros_like(row), salt)
    # The tip edge: a rounded arc, a little irregular per feather.
    edge = 0.92 - (0.55 + 0.25 * hv) * (2 * a) ** 2
    if rag is not None:
        edge = edge + rag
    d = edge - fv
    # Past its own tip, the point is on one of the two staggered feathers of
    # the next row back (under this one), deeper in than their tips.
    past = d < 0
    row2 = row + 1
    shift2 = 0.5 * np.mod(row2, 2)
    col2 = np.floor(u / su + shift2)
    a2 = u / su + shift2 - col2 - 0.5
    h2 = hash3(row2, col2, np.zeros_like(row2), salt)
    edge2 = 1.92 - (0.55 + 0.25 * h2) * (2 * a2) ** 2
    if rag is not None:
        edge2 = edge2 + rag
    d = np.where(past, edge2 - fv, d)
    a = np.where(past, a2, a)
    hv = np.where(past, h2, hv)
    return np.clip(d, 0.0, 2.0), a, hv


def bars(d, count, width, wobble=0.0, a=None):
    """1 on the dark bars across a feather: ``count`` bars a feather's
    length, each ``width`` of its period; ``wobble`` bends them into
    chevrons along ``a``."""
    np = _np()
    x = d * count
    if a is not None and wobble:
        x = x + wobble * np.abs(a)
    f = x - np.floor(x)
    return np.clip(1.0 - np.abs(f - 0.5) / (width * 0.5), 0.0, 1.0) ** 0.6 * (np.abs(f - 0.5) < width * 0.5)


def wing_coords(P, side):
    """Along the leading edge (metres from the shoulder) and behind it, for
    points over a wing; and the span fraction (0 shoulder, 1 hand's end)."""
    np = _np()
    edge = leading_edge(1.0)
    x = np.abs(P[:, 0])
    y = P[:, 1]
    le_y = np.interp(x, [p[0] for p in edge], [p[1] for p in edge])
    lens = [0.0]
    for p, q in zip(edge, edge[1:]):
        lens.append(lens[-1] + math.hypot(q[0] - p[0], q[1] - p[1]))
    along = np.interp(x, [p[0] for p in edge], lens)
    return along, y - le_y, np.clip(along / lens[-1], 0.0, 1.2)


def mix(a, b, f):
    np = _np()
    f = np.asarray(f, np.float64)
    if f.ndim == 1:
        f = f[:, None]
    return np.asarray(a, np.float64) * (1 - f) + np.asarray(b, np.float64) * f


def rgb(c, n):
    np = _np()
    return np.tile(np.asarray(c, np.float64), (n, 1))


def regions(P, N):
    """Named masks and fields over the sculpt's points."""
    np = _np()
    hw, hl, hh = SP["torso"]
    x, y, z = P[:, 0], P[:, 1], P[:, 2]
    ax = np.abs(x)
    hc, (w, l, h) = head_centre(), SP["head"]
    eh = np.sqrt((x / w) ** 2 + ((y - hc[1]) / l) ** 2 + ((z - hc[2]) / h) ** 2)
    nb = neck_base()
    bb = bill_base()
    sz = shoulder()[2]
    R = {}
    R["bill"] = y < bb[1] + 0.0005
    R["cere"] = (~R["bill"]) & (y < bb[1] + SP["cere"]) & (eh < 1.4)
    e = eye_centre()
    de = np.sqrt((ax - e[0]) ** 2 + (y - e[1]) ** 2 + (z - e[2]) ** 2)
    r = SP["eye"]
    R["eye_dist"] = de / r
    # Ragged edges: small feathers reach into the bare skin.
    rag = (vnoise(P, 2200.0 / bird_scale(), 7) - 0.5) * 0.8 + (vnoise(P, 6000.0 / bird_scale(), 8) - 0.5) * 0.5
    if SP["face"]:
        back, front = e[1] + 1.25 * r, bb[1]
        cy, ry = 0.5 * (back + front), 0.5 * (back - front)
        cz, rz = e[2] - 0.45 * r, 1.75 * r
        q = ((y - cy) / ry) ** 2 + ((z - cz) / rz) ** 2
        face = (q < 1.0 + 0.35 * rag) & (eh < 1.3) & (ax > 0.2 * w)
        face |= (y < bb[1] + SP["cere"] + 0.3 * SP["face"]) & (eh < 1.3) & (z > e[2] - (2.4 + 0.5 * rag) * r)
    else:
        face = de < (SP.get("ring", 1.45) + 0.25 * rag) * r
        if SP.get("lores", True):
            face |= (y < e[1] - 0.6 * r) & (z > e[2] - (1.3 + 0.3 * rag) * r) & (z < e[2] + (0.75 + 0.3 * rag) * r) & (eh < 1.3) & (ax > 0.3 * w)
    R["face"] = face & ~R["bill"]
    R["head"] = (eh < 1.45) & (y < nb[1] + 0.2 * SP["neck"])
    # The same, fading out over the head's back and the neck (``soft_head``),
    # its edge broken by the feathers: 1 on the head, 0 down the neck.
    if SP.get("soft_head"):
        ragged = eh + 0.30 * (vnoise(P, 900.0 / bird_scale(), 9) - 0.5)
        fade = np.clip((2.4 - ragged) / 1.4, 0, 1) * np.clip((nb[1] + 0.55 * SP["neck"] - y) / (0.7 * SP["neck"]), 0, 1)
        R["head_w"] = fade * fade * (3 - 2 * fade)
    # Legs: near the bare tarsus or a toe.
    dl = np.full(len(P), 1e9)
    seg_t = np.zeros(len(P))
    for sgn in (1.0, -1.0):
        hip, knee, heel, foot, toe = leg_joints(sgn)
        for k, (a_, b_) in enumerate(((heel, foot), (foot, toe))):
            a_, b_ = np.asarray(a_), np.asarray(b_)
            ab = b_ - a_
            t = np.clip(((P - a_) @ ab) / ab.dot(ab), 0, 1)
            d = np.linalg.norm(P - (a_ + t[:, None] * ab), axis=1)
            closer = d < dl
            seg_t = np.where(closer, k + t, seg_t)
            dl = np.minimum(dl, d)
    R["leg"] = dl < 2.1 * SP["leg_r"]
    R["claw"] = R["leg"] & (seg_t > 1.72)
    # Wings: out past the body and near the wing's plane.
    R["wing"] = (ax > 1.02 * hw) & (np.abs(z - sz) < 0.6 * hh)
    R["up"] = N[:, 2] > 0
    ang = np.arctan2(ax, z - 0.1 * hh)  # 0 on the back, pi under the belly
    R["ang"] = ang
    R["tailcov"] = y > tail_root()[1] - 0.15 * hl
    return R


def plumage(P, N):
    """Colour, height (feather relief, 0..1) and roughness at the points
    ``P`` (n, 3) with normals ``N``."""
    np = _np()
    P = np.asarray(P, np.float64)
    N = np.asarray(N, np.float64)
    n = len(P)
    R = regions(P, N)
    hw, hl, hh = SP["torso"]
    x, y, z = P[:, 0], P[:, 1], P[:, 2]
    ax = np.abs(x)
    scale = bird_scale()
    # Feather tracts: around the body's axis on the body; along and behind
    # the leading edge on the wings.
    hc = head_centre()
    rref = 0.8 * hw
    u_body = np.arctan2(ax, z - 0.1 * hh) * rref
    u_head = np.arctan2(ax, z - hc[2]) * SP["head"][0]
    along, behind, span = wing_coords(P, 1.0)
    jitter = vnoise(P, 90.0 / scale, 1) - 0.5
    fine = vnoise(P, 900.0 / scale, 2) - 0.5
    ventral = R["ang"] > 1.9
    dorsal = R["ang"] < 1.2
    tract = (P, N, R, along, behind, span, u_head, u_body, jitter, fine, ventral, dorsal)
    if "head_w" in R:
        # The head's small feathers and the body's larger ones painted both,
        # blended over the head's fade (``soft_head``; v5: switching tracts
        # at one line left a step in the plumage's tone there).
        col, height, rough, bare = _tract(*tract, head=R["head_w"] > 0.0)
        col_b, height_b, rough_b, bare_b = _tract(*tract, head=np.zeros(n, bool))
        t = np.clip((R["head_w"] - 0.2) / 0.6, 0, 1)
        t = t * t * (3 - 2 * t)
        col, height, rough = mix(col_b, col, t), height_b + (height - height_b) * t, rough_b + (rough - rough_b) * t
        bare = np.where(t > 0.5, bare, bare_b)
    else:
        col, height, rough, bare = _tract(*tract, head=R["head"])
    leg = R["leg"] & ~R["claw"]
    if leg.any():
        # Scutes: rings round the tarsus, in a small check on the toes.
        sc = np.mod(y / (0.0032 * scale), 1.0)
        height = np.where(leg, 0.4 + 0.5 * (np.abs(sc - 0.5) * 2) ** 3, height)
        col = np.where(leg[:, None], col * (0.88 + 0.12 * (np.abs(sc - 0.5) * 2)[:, None]), col)
    col = col * (1.0 + 0.10 * jitter[:, None])
    return np.clip(col, 0, 1), np.clip(height, 0, 1), rough


def _tract(P, N, R, along, behind, span, u_head, u_body, jitter, fine, ventral, dorsal, head):
    """``plumage``'s feathers laid along their tracts, ``head`` the points
    on the head's: their colour (with the barbs' grain), relief, roughness
    and which are bare."""
    np = _np()
    hw, hl, hh = SP["torso"]
    scale = bird_scale()
    wing = R["wing"] & ~head
    # Along the feathers: away from a point ahead of the throat on the body
    # (back along the mantle, down the breast), back from the bill on the
    # head.
    v_body = np.linalg.norm(P - np.array((0.0, -1.6 * hl, 0.25 * hh)), axis=1)
    v_head = np.linalg.norm(P - np.array(bill_base()), axis=1)
    u = np.where(head, u_head, np.where(wing, along, u_body))
    v = np.where(wing, behind, np.where(head, v_head, v_body))
    chord = np.asarray([chord_at(s) for s in np.clip(span, 0, 1)])
    cf = np.clip(behind / np.maximum(chord, 1e-6), 0, 1.2)
    size = np.where(head, 0.0040, 0.0085) * scale
    size = np.where(wing, np.where(cf < 0.35, 0.007, np.where(cf < 0.62, 0.013, 0.42 * chord / scale)) * scale, size)
    # Greater coverts: one row, its tips at the coverts' trailing edge.
    gc = wing & (cf >= 0.62)
    v = np.where(gc, behind - 0.58 * chord + 0.08 * size, v)
    rag = None
    if SP.get("ragged"):
        # Irregular feathers (v5: the regular tract read as a cartoon
        # scallop): the tract's lattice bent by a smooth warp a few feathers
        # wide (the feathers' lie, size and overlap vary), their tips' edges
        # worn and split.
        k = SP["ragged"]
        warp = 1.0 / (0.024 * scale)
        u = u + 1.6 * k * size * (vnoise(P, warp, 31) - 0.5)
        v = v + 1.6 * k * size * (vnoise(P, warp, 32) - 0.5) + 0.8 * k * size * (vnoise(P, 2.7 * warp, 35) - 0.5)
        rag = k * (0.34 * (vnoise(P, 1.0 / (0.0017 * scale), 33) - 0.5) + 0.26 * (vnoise(P, 1.0 / (0.0006 * scale), 34) - 0.5))
    d, a, hv = feather_cells(u, v, size, salt=3, aspect=SP.get("aspect", 0.85), rag=rag)
    out = SPECIES_PAINT[NAME](P, N, R, d, a, hv, jitter, ventral, dorsal, wing, cf)
    col, dark_amt, rough, bare = out
    # Relief: each feather's tip edge stands over the one behind; bare
    # parts carry their own (scutes, skin).
    height = np.clip(1.0 - 0.25 * d, 0, 1) - 0.12 * (1 - np.clip(d / 0.10, 0, 1))
    height = height + 0.06 * fine
    # The chimango's softer, more ragged plumage: its tips' steps lower.
    if "relief" in SP:
        height = 0.5 + SP["relief"] * (height - 0.5)
    height = np.where(bare, 0.5 + 0.08 * fine, height)
    # The small lesser coverts along the leading edge: a faint relief only
    # (their rows read as a ring tile otherwise).
    height = np.where(wing & (cf < 0.40), 0.5 + 0.25 * (height - 0.5), height)
    # Barbs: a fine grain along the feathers.
    grain = 0.5 + 0.5 * np.sin((u / (0.0009 * scale)) + 3 * fine)
    col = np.where(bare[:, None], col, col * (0.97 + 0.04 * grain[:, None]))
    return col, height, rough, bare


def _paint_chimango(P, N, R, d, a, hv, jitter, ventral, dorsal, wing, cf):
    np = _np()
    n = len(P)
    y, z = P[:, 1], P[:, 2]
    hw, hl, hh = SP["torso"]
    s = bird_scale()
    rough = np.full(n, 0.82)
    bare = np.zeros(n, bool)
    # Each feather its own: a second and a third random value from its
    # first, for its tone, its streak's width and strength (v5: the same
    # scallop repeated read as a cartoon).
    h2 = np.mod(hv * 7.31 + 0.13, 1.0)
    h3 = np.mod(hv * 13.7 + 0.41, 1.0)
    mottle = vnoise(P, 1.0 / (0.030 * s), 41) - 0.5
    broken = vnoise(P, 1.0 / (0.0012 * s), 42)
    # The body: buff-brown, the mantle browner, each feather's tone a
    # little its own; a darker, irregular centre and a pale fringe, worn
    # and broken in places, wider on some feathers than on others.
    buff, brown, dark = (0.75, 0.57, 0.38), (0.46, 0.32, 0.20), (0.29, 0.19, 0.11)
    pale = (0.86, 0.76, 0.58)
    col = mix(rgb(buff, n), rgb(brown, n), np.clip(0.30 + 0.35 * dorsal + 0.15 * hv + 0.15 * mottle, 0, 1))
    col = col * (0.88 + 0.22 * h2 + 0.06 * mottle)[:, None]
    wid = 0.15 + 0.12 * h3 + 0.10 * (broken - 0.5)
    centre = np.clip(1 - np.abs(a) / wid, 0, 1) * np.clip(d / 0.25, 0, 1)
    # The breast's and the neck's brown shaft streaks (the references'
    # Valdivia and Rio Grande do Sul birds): teardrops widening toward the
    # feather's tip, some feathers plain, others boldly streaked.
    swid = 0.04 + 0.08 * h2
    drop = np.clip(1 - np.abs(a) / (swid * (0.55 + 0.6 * np.clip(1 - d / 0.8, 0, 1))), 0, 1) ** 0.7
    drop = drop * np.clip((d - 0.05) / 0.08, 0, 1) * np.clip((0.95 - d) / 0.35, 0, 1)
    strong = np.clip(1.6 * h3 - 0.35, 0, 1)
    streak = np.where(ventral, 0.85 * drop * strong, 0.35 * centre)
    col = mix(col, rgb((0.40, 0.25, 0.14), n), streak)
    fw = 0.07 + 0.12 * h2
    fringe = np.clip(1 - d / fw, 0, 1) * np.clip(0.25 + 1.4 * (broken - 0.3), 0, 1) * np.where(dorsal, 0.45, 0.20)
    col = mix(col, rgb(pale, n), fringe)
    # Belly, vent and thighs paler; the rump (uppertail coverts) pale buff
    # barred.
    belly = ventral & (y > 0.05 * hl)
    col = mix(col, rgb((0.84, 0.75, 0.59), n), belly * np.clip((y - 0.05 * hl) / (0.6 * hl), 0, 1) * 0.8)
    rump = R["tailcov"] & R["up"] & ~wing
    col = np.where(rump[:, None], mix(rgb((0.82, 0.72, 0.56), n), rgb(brown, n), bars(d, 2.0, 0.28)), col)
    under = R["tailcov"] & ~R["up"] & ~wing
    col = np.where(under[:, None], mix(rgb((0.88, 0.81, 0.67), n), rgb(buff, n), bars(d, 2.0, 0.2)), col)
    # The head: buff, finely streaked crown; a dark brown stripe back from
    # the eye; paler cheeks and throat. It fades into the neck's colour
    # over the head's back (``head_w``: no seam).
    e = eye_centre()
    # The crown and the nape finely streaked, the cheeks and the throat
    # nearly plain.
    cheek = np.clip((e[2] + 0.5 * SP["eye"] - z) / (3.0 * SP["eye"]), 0, 1)
    hcol = mix(rgb((0.81, 0.67, 0.47), n), rgb((0.54, 0.40, 0.26), n), np.clip(centre * 0.9 * (0.6 + 0.6 * h3) * (1 - 0.7 * cheek), 0, 1))
    hcol = hcol * (0.94 + 0.10 * h2 + 0.06 * mottle)[:, None]
    # The stripe back from the eye's rear corner: tapering, a little
    # down-curved, its edges broken by the feathers.
    r = SP["eye"]
    along = (y - e[1] - 0.8 * r) / (5.0 * r)
    mid = e[2] - 0.05 * r - 0.8 * r * np.clip(along, 0, 1)
    half = r * (0.85 - 0.40 * np.clip(along, 0, 1)) + 0.15 * r * (np.clip(d, 0, 1) - 0.5)
    stripe = np.clip((half - np.abs(z - mid)) / (0.45 * r), 0, 1) * np.clip(along * 8 + 1, 0, 1) * np.clip((1 - along) * 2.5, 0, 1)
    stripe = stripe * (np.abs(P[:, 0]) > 0.40 * SP["head"][0])
    hcol = mix(hcol, rgb((0.28, 0.19, 0.12), n), 0.90 * stripe)
    # Paler throat and cheeks, fading back (v5: a hard cut behind the eye
    # read as a straight border down the cheek).
    throat = np.clip((e[2] - 2.2 * r - z) / (1.5 * r), 0, 1) * np.clip((e[1] + 5.0 * r - y) / (4.0 * r), 0, 1)
    hcol = mix(hcol, rgb((0.86, 0.79, 0.65), n), 0.55 * throat)
    hw_ = R["head_w"] * ~wing
    col = mix(col, hcol, hw_)
    # The wings: brown coverts with buff fringes and dark centres above,
    # cinnamon-buff barred below.
    wup = mix(rgb((0.44, 0.30, 0.19), n), rgb((0.27, 0.18, 0.11), n), centre * 0.7)
    wup = mix(wup, rgb((0.80, 0.68, 0.50), n), np.clip(1 - d / fw, 0, 1) * 0.6)
    # Below, cinnamon-buff coverts, each its own tone, with fine brown
    # shaft streaks (v5: dark centres in straight rows read as bricks).
    wdn = rgb((0.77, 0.65, 0.49), n) * (0.92 + 0.14 * h2 + 0.08 * mottle)[:, None]
    wdn = mix(wdn, rgb((0.55, 0.40, 0.26), n), 0.6 * drop * strong)
    col = np.where((wing & R["up"])[:, None], wup, col)
    col = np.where((wing & ~R["up"])[:, None], wdn, col)
    # Bare parts (v5): a narrow ring round the eye, pale yellow, warming
    # to pinkish orange at the cere; the bill pale bluish-grey horn, its
    # hook darker.
    ring = R["face"]
    # The lids' dark rims round the almond opening (``eye_open``), then
    # the bare ring.
    oy, oz = SP.get("eye_open", (1.0, 1.0))
    el = np.sqrt(((y - e[1]) / oy) ** 2 + ((z - e[2]) / oz) ** 2) / r
    rcol = mix(rgb((0.24, 0.17, 0.12), n), rgb((0.86, 0.72, 0.56), n), np.clip((el - 1.12) / 0.10, 0, 1))
    # The ring's outer edge fades into the feathers (v5 review: a noisy
    # threshold left pale specks round it).
    rw = np.clip((SP.get("ring", 1.45) + 0.05 - el) / 0.18, 0, 1)
    col = mix(col, rcol * (0.95 + 0.10 * jitter[:, None]), rw * rw * (3 - 2 * rw))
    ring = rw > 0.5
    # The cere (v5 review: a hard-edged slab read as a pink box): a fleshy
    # saddle over the upper mandible's base, reaching further back on the
    # sides round the nostril, its back edge broken by the lores' bristles
    # and fading into them; under the gape the chin is feathered.
    bb, (L, D, Wb), cl = bill_base(), SP["bill"], SP["cere"]
    ax = np.abs(P[:, 0])
    back = bb[1] + cl * (0.80 + 0.35 * np.clip(ax / (0.55 * Wb), 0, 1)) + 0.30 * cl * (broken - 0.5)
    cw = np.clip((back - y) / (0.40 * cl), 0, 1) * np.clip((z - (bb[2] - 0.38 * D)) / (0.16 * D), 0, 1)
    cw = cw * cw * (3 - 2 * cw) * (R["eye_dist"] > 1.6)
    toward = np.clip((bb[1] + cl - y) / cl, 0, 1)
    ccol = mix(rgb((0.93, 0.76, 0.56), n), rgb((0.91, 0.62, 0.48), n), toward)
    nostril = np.asarray(add(bb, (0.42 * Wb, 0.50 * cl, 0.12 * D)))
    dn = np.sqrt((ax - nostril[0]) ** 2 + (y - nostril[1]) ** 2 + (z - nostril[2]) ** 2) / (0.09 * D)
    ccol = ccol * (1 - 0.30 * np.clip(1.5 - dn, 0, 1) ** 2)[:, None]
    col = mix(col, ccol * (0.96 + 0.08 * jitter[:, None]), cw)
    # The horn: pale bluish grey, its hook darker; its edge on the cere
    # convex forward at the nostril's level, as in the photographs,
    # blended over half a millimetre.
    edge = bb[1] + 0.0005 - 0.14 * D * np.clip(1 - ((z - bb[2] - 0.10 * D) / (0.55 * D)) ** 2, 0, 1)
    hb = np.clip((edge - y) / 0.0006 + 0.5, 0, 1) * (R["bill"] | (y < bb[1] + 0.0012))
    tipf = np.clip((bb[1] - y) / L, 0, 1)
    low = np.clip((bb[2] - z) / D, 0, 1)
    horn = mix(rgb((0.72, 0.73, 0.73), n), rgb((0.80, 0.79, 0.76), n), 0.5 * low)
    horn = mix(horn, rgb((0.36, 0.37, 0.39), n), np.clip((tipf - 0.45) / 0.5, 0, 1) ** 1.3)
    col = mix(col, horn, hb * (cw > 0.01) + R["bill"] * (cw <= 0.01))
    bill = R["bill"]
    cere = cw > 0.5
    col = np.where(R["leg"][:, None], rgb((0.82, 0.77, 0.55), n), col)
    col = np.where(R["claw"][:, None], rgb((0.12, 0.11, 0.10), n), col)
    bare = ring | cere | bill | R["leg"]
    rough = np.where(bill | R["claw"], 0.42, np.where(bare, 0.58, rough))
    return col, None, rough, bare


def neck_axis():
    """Along the neck, from its base toward the head (unit)."""
    return norm(sub(head_centre(), neck_base()))


def bar_phase(P):
    """The carancho's barring as a phase (one bar per unit): planes across
    the body (no pole, no seam), on the breast at right angles to its
    down-and-back run, on the mantle to its run back and up; on the neck
    square to the neck's axis all round it, as the straight rings of the
    photographs (v3 review: blending the breast's and the mantle's planes
    by height there bent the bars into contour lines on the neck's side).
    Slightly wavy across."""
    np = _np()
    hw, hl, hh = SP["torso"]
    s = bird_scale()
    x, y, z = P[:, 0], P[:, 1], P[:, 2]
    nb = np.asarray(neck_base())
    upw = np.clip((z - 0.1 * hh) / (0.5 * hh), 0, 1)
    body = (1 - upw) * (0.35 * (y - nb[1]) - 0.94 * (z - nb[2])) + upw * (0.71 * (y - nb[1]) + 0.71 * (z - nb[2]))
    neck = -((P - nb) @ np.asarray(neck_axis()))
    # From the breast's planes at the neck's base to the neck's rings a
    # neck's length on (by the position along the neck alone, so each bar
    # stays straight across).
    along = -neck / SP["neck"]
    wn = np.clip((along + 0.15) / 0.55, 0, 1)
    wn = wn * wn * (3 - 2 * wn)
    v = (1 - wn) * body + wn * neck
    wave = 0.22 * np.sin(np.abs(x) / (0.007 * s)) + 0.10 * np.sin(np.abs(x) / (0.0023 * s) + y / (0.01 * s))
    wave = wave * (1 - 0.6 * wn)
    return v / (0.0024 * s) + wave + 0.45 * (vnoise(P, 260.0 / s, 21) - 0.5) + 0.25 * (vnoise(P, 900.0 / s, 22) - 0.5)


def _paint_carancho(P, N, R, d, a, hv, jitter, ventral, dorsal, wing, cf):
    np = _np()
    n = len(P)
    x, y, z = P[:, 0], P[:, 1], P[:, 2]
    hw, hl, hh = SP["torso"]
    rough = np.full(n, 0.82)
    black, cream, brown = (0.08, 0.065, 0.055), (0.92, 0.82, 0.62), (0.15, 0.11, 0.085)
    e = eye_centre()
    nb = neck_base()
    s = bird_scale()
    # Dark blackish brown all over the body, the back and the coverts.
    col = mix(rgb(brown, n), rgb(black, n), 0.4 + 0.3 * hv)
    # Fine transverse barring: across the body, slightly wavy, cream
    # bars on the dark (pale on the breast, fainter down the mantle).
    # Bars across the body: planes (no pole, no seam), on the breast at
    # right angles to its down-and-back run, on the mantle to its run back
    # and up; slightly wavy across.
    ph = bar_phase(P)
    stripe = np.clip(0.5 - (np.abs(ph - np.floor(ph) - 0.5) - 0.15) / 0.07, 0, 1)  # 1 on the dark bar
    # The barred zone: from the lower neck down the breast to the belly's
    # start in front, and over the nape and the upper back above.
    fy = (y - (nb[1] - 0.25 * SP["neck"])) / (0.80 * hl)
    breast = ~dorsal & (fy < 1.0)
    mantle = dorsal & (y < -0.35 * hl)
    pale_share = np.clip(1.0 - fy, 0, 1) ** 0.7  # the pale ground darkens downward
    ground = mix(rgb(cream, n), rgb((0.55, 0.47, 0.38), n), 1 - pale_share)
    barred = mix(ground, rgb(black, n), stripe * (0.85 + 0.15 * (1 - pale_share)))
    col = np.where(breast[:, None], mix(col, barred, np.clip((1.0 - fy) * 3.0, 0, 1)), col)
    mb = mix(rgb((0.80, 0.74, 0.62), n), rgb(black, n), stripe)
    col = np.where(mantle[:, None], mix(col, mb, np.clip((-0.35 * hl - y) / (0.25 * hl), 0, 1)), col)
    # Rump and both tail coverts: white, the upper ones barred.
    rump = R["tailcov"] & R["up"] & ~wing
    col = np.where(rump[:, None], mix(rgb(cream, n), rgb(black, n), stripe * 0.9), col)
    under = R["tailcov"] & ~R["up"] & ~wing
    col = np.where(under[:, None], rgb((0.93, 0.89, 0.80), n), col)
    # The head and the neck: black cap; cream cheeks, throat and neck,
    # the neck's lower part finely barred into the breast.
    head = R["head"] | (y < nb[1] - 0.25 * SP["neck"])
    rag = 0.3 * SP["eye"] * (hv - 0.5)
    cap = z > e[2] + 0.55 * SP["eye"] + rag - 0.30 * np.clip((y - e[1]) / SP["head"][1], 0, 2.5) * SP["head"][2]
    neckbar = np.clip((y - (nb[1] - 0.55 * SP["neck"])) / (0.35 * SP["neck"]), 0, 1)
    warm = (0.92, 0.82, 0.62)
    hcol = rgb(warm, n) * (0.97 + 0.06 * jitter[:, None])
    # The bars thin out up the neck into the plain throat.
    thin = np.clip(0.5 - (np.abs(ph - np.floor(ph) - 0.5) - 0.08 - 0.14 * neckbar) / 0.08, 0, 1)
    hcol = mix(hcol, rgb(black, n), thin * neckbar * (~R["head"]))
    hcol = np.where(cap[:, None], mix(rgb(black, n), rgb((0.20, 0.15, 0.11), n), 0.4 * hv), hcol)
    col = np.where(head[:, None], hcol, col)
    # Wings: dark above, the coverts faintly barred; black-brown below
    # with fine pale bars.
    wup = mix(rgb(brown, n), rgb((0.24, 0.19, 0.15), n), 0.35 * hv + 0.25 * (jitter + 0.5))
    wdn = mix(rgb((0.12, 0.095, 0.075), n), rgb((0.40, 0.35, 0.30), n), np.clip(0.30 * hv + 0.45 * stripe * (cf > 0.25), 0, 1))
    col = np.where((wing & R["up"])[:, None], wup, col)
    col = np.where((wing & ~R["up"])[:, None], wdn, col)
    # Bare parts.
    skin = R["face"] | R["cere"]
    # Orange, the cere yellower (the projected photo's colours, v4).
    fcol = mix(rgb((0.92, 0.48, 0.16), n), rgb((0.96, 0.70, 0.22), n), np.clip((bill_base()[1] + 1.5 * SP["cere"] - y) / (1.5 * SP["cere"]), 0, 1)[:, None])
    fcol = fcol * (0.92 + 0.16 * jitter[:, None])
    col = np.where(skin[:, None], fcol, col)
    bill = R["bill"]
    tipf = np.clip((bill_base()[1] - y) / SP["bill"][0], 0, 1)
    col = np.where(bill[:, None], mix(rgb((0.66, 0.72, 0.78), n), rgb((0.88, 0.86, 0.78), n), tipf ** 1.5), col)
    col = np.where(R["leg"][:, None], rgb((0.96, 0.76, 0.24), n), col)
    col = np.where(R["claw"][:, None], rgb((0.09, 0.08, 0.08), n), col)
    rim = (R["eye_dist"] > 0.95) & (R["eye_dist"] < 1.22)
    col = np.where(rim[:, None], rgb((0.16, 0.10, 0.07), n), col)
    bare = skin | bill | R["leg"] | rim
    rough = np.where(bill | R["claw"], 0.40, np.where(bare, 0.55, rough))
    return col, None, rough, bare


def centre_streak(a, d):
    np = _np()
    return np.clip(1 - np.abs(a) / 0.18, 0, 1) * np.clip(d / 0.3, 0, 1)


SPECIES_PAINT = {"chimango": _paint_chimango, "carancho": _paint_carancho}


# ---------------------------------------------------------------------------
# The face from a photograph (v4), as ``felid.py`` does for the cats: a side
# photograph of each species is projected across the head onto the body's
# colour map, both sides (the bird's left side mirrored from its right): the
# bare facial skin with its papillae, the cere, the bill's horn, the eye's
# surround, the cap's and cheeks' small feathers. Its landmarks (pixels)
# match the head's points (``face_landmarks``); an affine fit in the side
# plane with a smooth correction carries each landmark exactly onto its
# pixel (``fit_side``). The photo's own light is divided out (its luminance
# blurred wide), its grain filtered to a texel's footprint, and it is
# blended over the painted head where the skin faces the side (none where
# the photo sees it edge-on: the crown, the throat, the culmen) inside the
# photo's head ellipse. Its iris colours the eye (``iris_colour``).
#
# Every photograph projected is CC0, public domain or CC BY
# (``PIXEL_LICENCES``), checked on its page; each is credited in CREDITS.md.

FACE_PHOTOS = {
    "chimango": dict(
        file="inat_291627011.jpg", page="https://www.inaturalist.org/photos/291627011",
        author="Hugo Hulsberg (iNaturalist observation 168283799)", licence="CC0",
        points=dict(eye=(1279, 300), culmen=(1556, 350), nostril=(1521, 354), front=(1670, 471), hook=(1658, 554)),
        # The face and the eye's surround only. Its plumage is a dark brown
        # southern bird's, not the pampa chimango's buff: on the feathers
        # only its detail is kept, over the painted colour (``match``).
        # v5: on the bare parts too (its bill and cere read greenish), so
        # the head keeps the painted colours everywhere; its edge fades out
        # over ``rim`` of the ellipse and with the head's own fade
        # (``head_w``), so no border shows on the cheek and the neck.
        head=((1450, 385), (270, 160)), iris=57, match="detail", rim=0.55,
    ),
    "carancho": dict(
        file="Caracara_cheriway_Homosassa_Springs_Wildlife_State_Park_head_8a_jpg.jpg",
        page="https://commons.wikimedia.org/wiki/File:Caracara_cheriway_-Homosassa_Springs_Wildlife_State_Park_-head-8a.jpg",
        author="Riverbanks Outdoor Store (B A Bowen Photography)", licence="CC BY 2.0",
        points=dict(eye=(1350, 1240), culmen=(2080, 1020), nostril=(1980, 1080), front=(2470, 1400), hook=(2460, 1530)),
        # A northern caracara's deep orange-red face: turned toward the
        # carancho's orange, its cere toward yellow (``warm``: degrees of hue
        # over the face, more within ``cere`` pixels of the nostril).
        head=((1700, 1250), (1050, 560)), iris=95, warm=(10.0, 26.0), cere=230, saturation=0.88,
    ),
}


def face_landmarks():
    """The head's points matching ``FACE_PHOTOS``' ``points`` (the bird's
    right side, +x): the eye's centre, the culmen's base (where the horn
    starts over the cere), the nostril, the bill's foremost point and the
    hook's tip."""
    L, D, Wb = SP["bill"]
    bb = bill_base()
    return dict(
        eye=eye_centre(1.0),
        culmen=add(bb, (0.0, 0.0, 0.50 * D)),
        nostril=add(bb, (0.42 * Wb, 0.50 * SP["cere"], 0.12 * D)),
        front=add(bb, (0.0, -0.98 * L - 0.12 * D, -0.52 * D)),
        hook=add(bb, (0.0, -0.86 * L, -0.98 * D)),
    )


def fit_side(points3d, points2d, sigma):
    """The side camera from the head's points to the photo's pixels: the
    affine map of (y, z) that fits them best and a smooth correction
    (Gaussian bumps ``sigma`` wide round each point) that carries each one
    exactly onto its pixel. Returns ``f(points) -> pixels``."""
    np = _np()
    Q = np.asarray(points3d, float)[:, 1:]
    X = np.concatenate([Q, np.ones((len(Q), 1))], axis=1)
    Y = np.asarray(points2d, float)
    A, *_ = np.linalg.lstsq(X, Y, rcond=None)
    G = np.exp(-((Q[:, None, :] - Q[None, :, :]) ** 2).sum(-1) / (2 * sigma * sigma))
    C = np.linalg.solve(G + 1e-9 * np.eye(len(Q)), Y - X @ A)

    def f(P):
        q = np.atleast_2d(np.asarray(P, float))[:, 1:]
        K = np.exp(-((q[:, None, :] - Q[None, :, :]) ** 2).sum(-1) / (2 * sigma * sigma))
        return np.concatenate([q, np.ones((len(q), 1))], axis=1) @ A + K @ C

    f.scale = math.sqrt(abs(np.linalg.det(A[:2, :2])))  # pixels a metre
    return f


def _srgb_to_linear(c):
    np = _np()
    return np.where(c <= 0.04045, c / 12.92, ((c + 0.055) / 1.055) ** 2.4)


def _linear_to_srgb(c):
    np = _np()
    c = np.clip(c, 0.0, 1.0)
    return np.where(c <= 0.0031308, c * 12.92, 1.055 * c ** (1 / 2.4) - 0.055)


def face_mapping():
    """The species' photo mapping (``fit_side`` on ``face_landmarks``)."""
    photo = FACE_PHOTOS[NAME]
    marks = face_landmarks()
    names = sorted(photo["points"])
    return fit_side([marks[n] for n in names], [photo["points"][n] for n in names], 0.010 * bird_scale())


def photo_weight(P, N, f):
    """How much of the photo the head takes at the points ``P`` facing
    ``N`` (either side): on the head and the bill, where the skin faces
    the side (none past about 65 degrees off it), inside the photo's head
    ellipse (feathered at its rim)."""
    np = _np()
    photo = FACE_PHOTOS[NAME]
    R = regions(P, N)
    on = R["head"] | R["bill"] | R["cere"]
    side = np.abs(N[:, 0])
    facing = np.clip((side - 0.42) / 0.30, 0, 1)
    facing = facing * facing * (3 - 2 * facing)
    uv = f(np.c_[np.abs(P[:, 0]), P[:, 1], P[:, 2]])
    (cx, cy), (rx, ry) = photo["head"]
    r = np.hypot((uv[:, 0] - cx) / rx, (uv[:, 1] - cy) / ry)
    if "rim" in photo:
        # A wide, smooth rim (v5: the hard cut read as a pale patch's
        # straight border).
        ring = np.clip((1.0 - r) / photo["rim"], 0, 1)
        return on * facing * ring * ring * (3 - 2 * ring), uv
    ring = np.clip((1.0 - r) / 0.18, 0, 1)
    return on * facing * ring, uv


def project_face(P, N, col, height, pic, footprint):
    """The photo ``pic`` (rows top down, sRGB 0..1) projected over the
    painted colour ``col`` and relief ``height`` at the points ``P`` with
    normals ``N``; ``footprint``: a texel's size in metres. Returns the new
    colour and height and the photo's weight at each point."""
    np = _np()
    photo = FACE_PHOTOS[NAME]
    f = face_mapping()
    w, uv = photo_weight(P, N, f)
    if not (w > 0).any():
        return col, height, w
    h_, w_ = pic.shape[:2]
    lin = _srgb_to_linear(pic.astype(np.float64))
    lum = lin @ np.array([0.2126, 0.7152, 0.0722])
    (cx, cy), (rx, ry) = photo["head"]
    # Its light divided out (clamped: the deepest shadows are not lifted
    # into blotches), then its grain filtered to a texel's footprint.
    if photo.get("warm"):
        lin = _srgb_to_linear(warm_photo(pic.astype(np.float64), photo))
        lum = lin @ np.array([0.2126, 0.7152, 0.0722])
    shade = box_blur(lum, 0.08 * rx)
    yy, xx = np.mgrid[0:h_, 0:w_]
    inside = ((xx - cx) / rx) ** 2 + ((yy - cy) / ry) ** 2 < 0.8
    albedo = lin * np.clip((np.median(shade[inside]) / np.maximum(shade, 1e-4)) ** 0.6, 0.6, 1.7)[..., None]
    px = footprint * f.scale
    if px > 2.0:
        albedo = np.stack([box_blur(albedo[..., c], 0.5 * px, passes=1) for c in range(3)], axis=-1)
    sample = bilinear(albedo, uv[:, 1], uv[:, 0])
    paint = _srgb_to_linear(np.asarray(col, np.float64))
    strong = w > 0.5
    if photo.get("match") == "detail":
        # Only the photo's detail (its feathers' grain, the eye's lids, the
        # cere's and the bill's texture) over the painted colour: its
        # plumage is another population's and its bare parts read greenish.
        # Its luminance over its own local mean, centred on 1 where the
        # photo is used (v5: a gain off 1 and the old per-channel colours
        # left a paler patch with an edge), so the painted tone holds.
        Y = np.array([0.2126, 0.7152, 0.0722])
        low = bilinear(box_blur(albedo @ Y, 0.10 * rx)[..., None], uv[:, 1], uv[:, 0])[:, 0]
        ratio = np.log(np.maximum(sample @ Y, 1e-4) / np.maximum(low, 1e-4))
        # Lightening kept small: the photo's pale bristles and highlights
        # read as specks (v5 review); its darker lines (the lids, the
        # feathers' shadows) carry the detail.
        ratio = np.clip(0.5 * (ratio - (np.median(ratio[strong]) if strong.any() else 0.0)), -0.25, 0.05)
        sample = paint * np.exp(ratio)[:, None]
    elif strong.any():
        gain = np.clip(paint[strong].mean() / max(sample[strong].mean(), 1e-4), 0.75, 1.35)
        sample = sample * gain
    out = paint * (1 - w[:, None]) + sample * w[:, None]
    # The skin's papillae and the feathers' edges as relief.
    detail = bilinear(lum - box_blur(lum, max(2.0, 1.5 * px)), uv[:, 1], uv[:, 0]) / max(np.median(lum[inside]), 1e-3)
    if photo.get("match") == "detail":
        # Faint, and over the painted feathers' own relief (v5 review: the
        # photo's bristles stood up as pale specks under the light).
        height = height * (1 - 0.4 * w) + np.clip(0.5 + 0.3 * detail, 0, 1) * 0.4 * w
    else:
        height = height * (1 - w) + np.clip(0.5 + 0.8 * detail, 0, 1) * w
    return _linear_to_srgb(out), height, w


def warm_photo(pic, photo):
    """The photo (sRGB) with its reds and oranges turned ``photo["warm"]``
    degrees of hue toward yellow (the first over the whole face, the second
    within ``photo["cere"]`` pixels of the nostril, fading out), their
    saturation scaled by ``photo["saturation"]``."""
    np = _np()
    mx, mn = pic.max(axis=-1), pic.min(axis=-1)
    c = mx - mn
    r, g, b = pic[..., 0], pic[..., 1], pic[..., 2]
    hue = np.where(c < 1e-6, 0.0, np.where(mx == r, ((g - b) / np.maximum(c, 1e-6)) % 6, np.where(mx == g, (b - r) / np.maximum(c, 1e-6) + 2, (r - g) / np.maximum(c, 1e-6) + 4))) * 60.0
    sat = np.where(mx > 0, c / np.maximum(mx, 1e-6), 0.0)
    redness = np.clip(1 - np.abs(((hue + 180) % 360) - 180 - 15) / 40.0, 0, 1) * np.clip((sat - 0.2) / 0.2, 0, 1)
    face, cere = photo["warm"]
    yy, xx = np.mgrid[0:pic.shape[0], 0:pic.shape[1]]
    nx, ny = photo["points"]["nostril"]
    near = np.clip(1 - np.hypot(xx - nx, yy - ny) / photo["cere"], 0, 1)
    hue = (hue + redness * (face + (cere - face) * near * near * (3 - 2 * near))) % 360
    sat = sat * (1 - redness * (1 - photo.get("saturation", 1.0)))
    h6 = hue / 60.0
    x = mx * sat * (1 - np.abs(h6 % 2 - 1))
    cc = mx * sat
    z = np.zeros_like(cc)
    k = np.floor(h6).astype(int) % 6
    rgb = np.select([k[..., None] == i for i in range(6)],
                    [np.stack(v, -1) for v in ((cc, x, z), (x, cc, z), (z, cc, x), (z, x, cc), (x, z, cc), (cc, z, x))])
    return rgb + (mx - cc)[..., None]


def iris_colour(pic):
    """The photo's iris colour (linear): the median of a ring round its
    eye landmark, between the pupil and the iris' rim, its highlights
    left out."""
    np = _np()
    photo = FACE_PHOTOS[NAME]
    ex, ey = photo["points"]["eye"]
    r = photo["iris"]
    yy, xx = np.mgrid[0:pic.shape[0], 0:pic.shape[1]]
    d = np.hypot(xx - ex, yy - ey)
    ring = (d > 0.50 * r) & (d < 0.85 * r)
    px = _srgb_to_linear(pic[ring].astype(np.float64))
    lum = px @ np.array([0.2126, 0.7152, 0.0722])
    keep = lum < np.percentile(lum, 70)
    return tuple(float(v) for v in np.median(px[keep], axis=0))


# ---------------------------------------------------------------------------
# The feather atlas: each flight feather, tail feather and crest feather
# painted on its own, upper and under side.

ATLAS = (2048, 2048)
CELL = (64, 512)


def atlas_cells():
    """The atlas' cells: ``(kind, index, under)`` -> cell number."""
    keys = [("P", i) for i in range(1, 11)] + [("S", i) for i in range(1, 16)] + [("R", i) for i in range(1, 7)]
    keys += [("C", i) for i in range(1, 4)] + [("G", i) for i in range(1, 5)] + [("U", i) for i in range(1, 5)]
    keys += [("M", i) for i in range(1, 5)] + [("L", i) for i in range(1, 5)] + [("K", i) for i in range(1, 5)]
    out = {}
    for k in keys:
        for under in (False, True):
            out[(k[0], k[1], under)] = len(out)
    return out


def cell_rect(cell):
    cols = ATLAS[0] // CELL[0]
    c, r = cell % cols, cell // cols
    return (c * CELL[0] / ATLAS[0], r * CELL[1] / ATLAS[1], (c + 1) * CELL[0] / ATLAS[0], (r + 1) * CELL[1] / ATLAS[1])


def vane(t, o, L, W, fo, kind, index, finger, rag):
    """1 inside the feather's outline at ``t`` (0 root, 1 tip) and ``o``
    (metres from the rachis, positive into the inner vane)."""
    np = _np()
    shape = np.clip((t - 0.05) / 0.14, 0, 1) ** 0.5
    tipr = min(0.5, (2.2 if kind == "P" else 0.6) * W / L)
    k = np.clip((t - (1 - tipr)) / tipr, 0, 1)
    shape = shape * np.sqrt(1 - k * k)
    ho = fo * W * shape
    hi = (1 - fo) * W * shape
    if finger:
        te = 0.74 + 0.015 * (10 - index)
        ho = ho * (1 - 0.45 * np.clip((t - te) / 0.06, 0, 1))
        if index >= 11 - SP["fingers"]:
            tn = 0.72 + 0.015 * (10 - index)
            hi = hi * (1 - 0.35 * np.clip((t - tn) / 0.08, 0, 1))
    ho = ho * (1 - rag)
    hi = hi * (1 - rag)
    return ((o < 0) & (-o < ho)) | ((o >= 0) & (o < hi)), ho, hi


def paint_feather(kind, index, under, L, W, fo, finger, seed=0):
    """A feather's colour (sRGB), opacity and height, ``CELL`` pixels, rows
    from the root (0) to the tip."""
    np = _np()
    wpx, hpx = CELL
    t = (np.arange(hpx) + 0.5)[:, None] / hpx * np.ones((1, wpx))
    s = (np.arange(wpx) + 0.5)[None, :] / wpx * np.ones((hpx, 1))
    o = (s - fo) * W
    tm = t * L
    rng = np.random.default_rng(seed)
    phase = rng.uniform(0, 6.28, 4)
    rag = 0.05 * (0.5 + 0.5 * np.sin(tm / (0.004 + 0.002 * (o > 0)) + phase[0])) * np.clip(t - 0.3, 0, 1)
    if kind == "C":
        rag = 0.0 * t  # the crest's ragged edge is its open barbs (below)
    inside, ho, hi = vane(t, o, L, W, fo, kind, index, finger, rag)
    rw = 0.022 * W * (1 - 0.6 * t) + 0.00035
    rachis = (np.abs(o) < rw) & (t < 0.995)
    if not SP.get("calamus", True):
        # No bare quill before the vane: the cards' roots reach past the
        # wing's leading edge, where it read as needles (v5 review).
        rachis = rachis & (t > 0.12)
    alpha = (inside | rachis).astype(np.float32)
    # Across the vane: 0 at the rachis, 1 at its edge.
    e = np.where(o < 0, -o / np.maximum(ho, 1e-6), o / np.maximum(hi, 1e-6))
    base, dark, barred, bar_n, bar_w, tip = FEATHER_PAINT[NAME](kind, index, under, t, e, o, tm, L)
    pitch = 0.0011 * bird_scale()
    barb = 0.5 + 0.5 * np.cos(2 * np.pi * (tm - 0.9 * np.abs(o)) / pitch + phase[1])
    if kind == "C":
        # The crest's small feathers are loose: past the vane's middle only
        # their barbs, fine strands raking toward the tip, thinning out to
        # the edge and the tip (v3 review: the cut-out outline read as broken
        # cards, stepped, at the cap's front).
        strand = 0.5 + 0.5 * np.cos(2 * np.pi * (tm - 1.7 * np.abs(o)) / (0.55 * pitch) + 3 * phase[2] + 2.0 * np.sin(np.abs(o) / (0.4 * pitch)))
        thin = 0.15 + 0.85 * np.clip((e - 0.45) / 0.55, 0, 1) + 0.6 * np.clip((t - 0.80) / 0.20, 0, 1)
        alpha = (alpha > 0) & ((e < 0.45) & (t < 0.80) | (strand > thin))
        alpha = alpha.astype(np.float32)
    # Bars nearly square to the shaft, slanting a little toward the tip at
    # the vane's edge, wavy, the outer vane's offset from the inner's.
    wave = SP.get("bar_wave", 1.0)
    wig = wave * (0.08 * np.sin(np.abs(o) / (0.003 * pitch / 0.0011) + phase[3]) + 0.06 * np.sin(tm / (0.007 * pitch / 0.0011) + phase[2])) + SP.get("bar_shift", 1.0) * phase[0] / 6.28
    x = (tm + 0.15 * np.abs(o)) / (0.21 * bird_scale()) * bar_n * 1.6 + wig
    f = x - np.floor(x)
    edge = np.abs(f - 0.5) - bar_w * 0.5
    bar = np.clip(0.5 - edge / 0.10, 0, 1) * barred * (0.75 + 0.25 * barb)
    col = base * (1 - bar[..., None]) + dark * bar[..., None]
    col = col * (0.90 + 0.12 * barb[..., None])
    # The edge of the vane a little darker (thinner, more shadow), the
    # rachis paler.
    col = col * (1 - 0.12 * np.clip((e - 0.8) / 0.2, 0, 1))[..., None]
    shaft = np.asarray(FEATHER_SHAFT[NAME](kind, under))
    col = np.where(rachis[..., None], shaft[None, None, :] * np.ones_like(col), col)
    height = np.where(rachis, 1.0, 0.55 - 0.25 * np.clip(e, 0, 1) + 0.10 * barb)
    height = height * alpha
    return np.clip(col, 0, 1), alpha, height


def _band(t, a, b, soft=0.015):
    np = _np()
    return np.clip((t - a) / soft + 0.5, 0, 1) * np.clip((b - t) / soft + 0.5, 0, 1)


def _feathers_chimango(kind, index, under, t, e, o, tm, L):
    """The chimango's feathers (``_feathers_chimango_plain``), each a
    little lighter or darker than its neighbours, its tip faded with wear
    (v5: one flat tone a kind read as painted)."""
    np = _np()
    base, dark, barred, bar_n, bar_w, tip = _feathers_chimango_plain(kind, index, under, t, e, o, tm, L)
    h = math.sin(index * 12.9898 + "PSRCGUMLK".index(kind) * 78.233) * 43758.5453
    tone = 0.91 + 0.18 * (h - math.floor(h))
    fade = 1.0 + 0.10 * np.clip((t - 0.82) / 0.18, 0, 1)[..., None]
    return base * tone * fade, dark * tone, barred, bar_n, bar_w, tip


def _feathers_chimango_plain(kind, index, under, t, e, o, tm, L):
    np = _np()
    one = np.ones(t.shape + (1,))
    if kind == "P":
        # Pale buff bases (paler and wider below), barred brown on the
        # inner vane; the distal part dark brown.
        reach = (0.40 if not under else 0.55) - 0.025 * max(0, 4 - index)
        pale = np.clip((reach - t) / 0.08 + 0.5, 0, 1)
        if not under:
            pale = pale * np.where(o > 0, 1.0, 0.35)
        light = np.asarray((0.86, 0.77, 0.60) if not under else (0.90, 0.86, 0.77))
        distal = np.asarray((0.30, 0.20, 0.13) if not under else (0.47, 0.40, 0.33))
        base = light * pale[..., None] + distal * (1 - pale[..., None])
        dark = np.asarray((0.36, 0.27, 0.19) if not under else (0.55, 0.47, 0.40)) * one
        barred = pale * (0.30 if under else 0.55) + (1 - pale) * 0.45
        return base, dark, barred, 7.0, 0.30, None
    if kind == "S":
        if index > 12:
            base = np.asarray((0.45, 0.33, 0.23)) * one
            base = base + (np.asarray((0.80, 0.68, 0.50)) - base) * (np.clip((t - 0.86) / 0.08, 0, 1) + np.clip((e - 0.82) / 0.15, 0, 1) * 0.6)[..., None]
            return base, np.asarray((0.30, 0.22, 0.15)) * one, 0.4 + 0 * t, 5.0, 0.25, None
        top, bot = (0.44, 0.31, 0.20), (0.80, 0.73, 0.61)
        base = np.asarray(top if not under else bot) * one
        base = base + (np.asarray((0.74, 0.63, 0.47)) - base) * np.clip((t - 0.90) / 0.06, 0, 1)[..., None]
        dark = np.asarray((0.30, 0.22, 0.15) if not under else (0.52, 0.44, 0.36)) * one
        return base, dark, (0.6 if not under else 0.5) + 0 * t, 6.0, 0.24, None
    if kind == "R":
        # Pale buff, finely barred brown, a broad dark brown band before
        # the pale tip.
        base = np.asarray((0.80, 0.67, 0.49) if not under else (0.86, 0.78, 0.64)) * one
        if index == 1 and not under:
            base = base * 0.88
        dark = np.asarray((0.56, 0.44, 0.32)) * one
        band = _band(t, 0.76, 0.93)
        tipz = np.clip((t - 0.93) / 0.02, 0, 1)
        base = base * (1 - band[..., None]) + np.asarray((0.25, 0.18, 0.12)) * band[..., None]
        base = base * (1 - tipz[..., None]) + np.asarray((0.88, 0.81, 0.68)) * tipz[..., None]
        barred = (1 - band) * (1 - tipz) * 0.85 * (t > 0.12)
        return base, dark, barred, 13.0, 0.32, None
    if kind in "GMLK":
        # Brown, darker along the shaft, a buff fringe round the tip.
        base = np.asarray((0.44, 0.30, 0.19)) * one
        base = base * (1 - 0.25 * np.clip(1 - e / 0.4, 0, 1))[..., None]
        fr = np.clip((t - 0.80) / 0.08, 0, 1) + np.clip((e - 0.80) / 0.15, 0, 1) * np.clip((t - 0.4) / 0.3, 0, 1)
        base = base + (np.asarray((0.78, 0.66, 0.49)) - base) * np.clip(fr, 0, 1)[..., None] * 0.8
        return base, np.asarray((0.30, 0.22, 0.15)) * one, 0.3 + 0 * t, 3.0, 0.22, None
    if kind == "U":
        base = np.asarray((0.77, 0.65, 0.49)) * one
        return base, np.asarray((0.58, 0.46, 0.33)) * one, 0.45 + 0 * t, 4.0, 0.25, None
    base = np.asarray((0.45, 0.34, 0.24)) * one
    return base, base, 0 * t, 1.0, 0.0, None


def _feathers_carancho(kind, index, under, t, e, o, tm, L):
    np = _np()
    one = np.ones(t.shape + (1,))
    black = np.asarray((0.10, 0.08, 0.07))
    if kind == "P":
        # Blackish brown; the outer primaries' barred white window, about
        # a third of their length out from the coverts.
        base = (np.asarray((0.14, 0.11, 0.09)) if not under else np.asarray((0.20, 0.17, 0.15))) * one
        if index >= 3:
            a0, a1 = (0.27, 0.55) if index >= 6 else (0.31, 0.50)
            win = _band(t, a0, a1, 0.06) * (0.85 if index >= 6 else 0.45)
            white = np.asarray((0.80, 0.77, 0.70) if not under else (0.86, 0.84, 0.79))
            base = base * (1 - win[..., None]) + white * win[..., None]
            barred = win * 0.85 + (1 - win) * 0.25
            dark = np.where(win[..., None] > 0.5, np.asarray((0.22, 0.19, 0.16)) * one, np.asarray((0.24, 0.20, 0.17)) * one)
            return base, dark, barred, 12.0, 0.28, None
        return base, np.asarray((0.24, 0.20, 0.17)) * one, 0.3 + 0 * t, 7.0, 0.3, None
    if kind == "S":
        base = (np.asarray((0.15, 0.11, 0.09)) if not under else np.asarray((0.24, 0.21, 0.19))) * one
        dark = (np.asarray((0.29, 0.24, 0.20)) if not under else np.asarray((0.52, 0.49, 0.45))) * one
        return base, dark, (0.45 if not under else 0.7) + 0 * t, 6.0, 0.22, None
    if kind == "R":
        # White, finely barred black, a broad black end band, the very tip
        # pale.
        base = np.asarray((0.92, 0.90, 0.83)) * one
        band = _band(t, 0.74, 0.975)
        tipz = np.clip((t - 0.975) / 0.01, 0, 1)
        base = base * (1 - band[..., None]) + black * band[..., None]
        base = base * (1 - tipz[..., None]) + np.asarray((0.80, 0.76, 0.68)) * tipz[..., None]
        barred = (1 - band) * (1 - tipz) * 0.95 * (t > 0.10)
        return base, np.asarray((0.16, 0.13, 0.11)) * one, barred, 14.0, 0.30, None
    if kind in "GMLK":
        base = np.asarray((0.17, 0.12, 0.09)) * one
        return base, np.asarray((0.31, 0.25, 0.20)) * one, 0.55 + 0 * t, 4.0, 0.18, None
    if kind == "U":
        base = np.asarray((0.13, 0.10, 0.08)) * one
        return base, np.asarray((0.55, 0.50, 0.44)) * one, 0.7 + 0 * t, 4.0, 0.20, None
    return black * one, black * one, 0 * t, 1.0, 0.0, None


FEATHER_PAINT = {"chimango": _feathers_chimango, "carancho": _feathers_carancho}


def _shaft(kind, under):
    if NAME == "chimango":
        # Pale but close to the vane's tone (v5: bright shafts read as
        # painted lines).
        return (0.74, 0.67, 0.55) if under else (0.42, 0.32, 0.23)
    return (0.62, 0.58, 0.52) if under else (0.30, 0.26, 0.22)


FEATHER_SHAFT = {"chimango": _shaft, "carancho": _shaft}


def coverts(feathers):
    """The greater coverts over each flight feather (``G``): one card each,
    from the coverts' middle back over the trailing edge of the wing's
    sculpted part (breaking its line into feather tips), upper side only."""
    out = []
    span_x = wing_joints()[3][0]
    for fd in feathers:
        if fd["kind"] not in "PS":
            continue
        r = min(abs(fd["root"][0]) / span_x, 1.0)
        ch = chord_at(r)
        # Only over the wing: from below, the sculpt's underwing coverts
        # read as one continuous surface (cards under it read as clumps).
        for kind, lift in (("G", 1.0),):
            g = dict(fd)
            g["kind"] = kind
            g["index"] = 1 + (fd["index"] + (0 if fd["kind"] == "S" else 2)) % 4
            g["root"] = add(add(fd["root"], mul(fd["dir"], 0.30 * ch)), (0, 0, lift * 0.0012 * bird_scale()))
            g["length"] = (0.72 if kind == "G" else 0.62) * ch
            g["width"] = fd["width"] * (1.05 if kind == "G" else 1.7)
            g["outer"] = 0.45
            g["finger"] = False
            g["layers"] = (False,) if kind == "G" else (True,)
            # On the slab's surface, not inside it (v3: the coverts' plate
            # hid the cards' bases from above).
            out.append(lying(g, wing_top, tilt=0.4))
    return out


COVER_LIFT = 0.0006  # m (chimango scale): a covert card's root over the surface it lies on


def lying(fd, top, tilt=0.35):
    """``fd`` laid on a surface (``top(x, y)``, its height): the root just
    over it, the feather following it to its tip and lifting off it a
    little there (``tilt``: the tip's lift over the root's, in feather
    widths), so its edge stands over the feathers behind."""
    lift = COVER_LIFT * bird_scale()
    r, d, L = fd["root"], fd["dir"], fd["length"]
    tip = add(r, mul(d, L))
    z0 = top(r[0], r[1]) + lift
    z1 = top(tip[0], tip[1]) + lift + tilt * fd["width"] * 0.25
    fd = dict(fd)
    fd["root"] = (r[0], r[1], z0)
    flat = math.hypot(tip[0] - r[0], tip[1] - r[1])
    fd["dir"] = norm((tip[0] - r[0], tip[1] - r[1], z1 - z0))
    fd["length"] = math.hypot(flat, z1 - z0)
    side = fd["side"]
    fd["normal"] = norm(cross(cross(fd["dir"], (0.0, 0.0, 1.0)), fd["dir"]))
    if fd["normal"][2] < 0:
        fd["normal"] = mul(fd["normal"], -1)
    fd["side"] = side
    return fd


def covert_rows(feathers):
    """The median (``M``) and lesser (``L``) coverts: two rows of small,
    rounded cards over the greater coverts' bases and the wing's leading
    part, one card over each flight feather out to P6, laid on the wing
    slab's upper surface. With the greater coverts behind them they break
    the coverts' plate into overlapping feather edges seen from above (v3
    review)."""
    out = []
    span_x = wing_joints()[3][0]
    for fd in feathers:
        if fd["kind"] not in "PS" or (fd["kind"] == "P" and fd["index"] > 6):
            continue
        r = min(abs(fd["root"][0]) / span_x, 1.0)
        ch = chord_at(r)
        k = fd["index"] + (0 if fd["kind"] == "S" else 13)
        for kind, at, ln, wd in (("M", 0.16, 0.30, 1.25), ("L", 0.02, 0.22, 1.0)):
            m = dict(fd)
            m.update(kind=kind, index=1 + (k + (2 if kind == "L" else 0)) % 4, finger=False, layers=(False,), outer=0.45)
            m["root"] = add(fd["root"], mul(fd["dir"], at * ch))
            m["length"] = ln * ch
            m["width"] = wd * fd["width"]
            out.append(lying(m, wing_top, tilt=0.30 if kind == "M" else 0.20))
    return out


def scapulars():
    """The scapulars (``K``): the long feathers over the wing's root, from
    the mantle's sides back and a little out over the inner secondaries,
    in two overlapping rows, and a row of the mantle's own down the back's
    middle; laid on the back."""
    hw, hl, hh = SP["torso"]
    out = []
    for side in (1.0, -1.0):
        for row, (fx, fy0) in enumerate(((0.62, -0.55), (0.82, -0.30), (0.28, -0.72))):
            for k in range(3):
                y0 = (fy0 + 0.17 * k) * hl
                x0 = side * fx * hw
                d = norm((side * (0.30 + 0.10 * row if row < 2 else 0.12), 1.0, 0.0))
                fd = dict(kind="K", index=1 + (2 * row + k) % 4, root=(x0, y0, 0.0), dir=d, normal=(0.0, 0.0, 1.0), side=side,
                          length=(0.42 + 0.06 * min(row, 1)) * hl, width=0.30 * hw, outer=0.45, finger=False, layers=(False,),
                          bones={"Body": 0.6, mirror_bone("Shoulder", side): 0.4})
                out.append(lying(fd, lambda x, y: max(back_top(x, y), wing_top(x, y) if abs(x) > 0.9 * hw else -1.0), tilt=0.25))
    return out


def feather_set():
    """Every feather card of the bird, both wings with their greater and
    median coverts, the scapulars, the tail and the crest."""
    wings = remiges(1.0) + remiges(-1.0)
    return wings + coverts(wings) + covert_rows(wings) + scapulars() + rectrices() + crest_feathers()


def paint_atlas(scans=None):
    """The whole atlas: colour, opacity, normal (OpenGL) and roughness,
    rows bottom up. ``scans`` (``SCANS``' sheets, loaded: ``load_scans``)
    give the cells of the feathers they hold their real pixels
    (``scan_cells``); the rest are painted."""
    np = _np()
    W, H = ATLAS
    colour = np.zeros((H, W, 3), np.float32)
    alpha = np.zeros((H, W), np.float32)
    height = np.zeros((H, W), np.float32)
    dims = {}
    for fd in feather_set():
        dims.setdefault((fd["kind"], fd["index"]), fd)
    real = scan_cells(scans, dims) if scans else {}
    grain = scan_grain(scans, dims) if scans else {}
    for (kind, index, under), cell in atlas_cells().items():
        fd = dims.get((kind, index))
        if fd is None:
            continue
        if (kind, index, under) in real:
            c, a, h = real[(kind, index, under)]
        else:
            c, a, h = paint_feather(kind, index, under, fd["length"], fd["width"], fd["outer"], fd["finger"], seed=cell)
            if (kind, index) in grain:
                c, h = regrain(c, a, h, grain[(kind, index)], under)
        u0, v0, u1, v1 = cell_rect(cell)
        x0, y0 = round(u0 * W), round(v0 * H)
        colour[y0:y0 + CELL[1], x0:x0 + CELL[0]] = c
        alpha[y0:y0 + CELL[1], x0:x0 + CELL[0]] = a
        height[y0:y0 + CELL[1], x0:x0 + CELL[0]] = h
    gy, gx = np.gradient(height)
    nrm = np.stack([-gx * 2.5, -gy * 2.5, np.ones_like(gx)], axis=-1)
    nrm /= np.linalg.norm(nrm, axis=-1, keepdims=True)
    rough = np.where(height > 0.95, 0.55, 0.78).astype(np.float32)
    return colour, alpha, nrm * 0.5 + 0.5, np.repeat(rough[..., None], 3, axis=-1)


# ---------------------------------------------------------------------------
# Real feathers (v4). The carancho's flight feathers, tail, coverts and
# scapulars carry the pixels of the Feather Atlas's scans of a Crested
# Caracara (U.S. Fish and Wildlife Service Forensics Laboratory: a federal
# work, public domain; its FAQ: "All images and data on the website are
# freely available to the public; no special permission is required for
# their use"). The specimen is a northern Caracara cheriway, the carancho's
# sister species, its plumage the same: the primaries' barred white bases
# (the window), the dark secondaries, the white tail barred black with its
# broad black end. Each scan sheet (blue background) is cut into its
# feathers (``cut_scan``), each feather resampled into its card's cell
# (``feather_cell``): the rachis on the card's, the outer vane (the scans'
# left) on the card's outer side, so the real outline, notches and wear
# come with it. Coverts and scapulars take the distal part of a secondary
# or an inner primary. The chimango has no free scans (none on the Feather
# Atlas, Commons or iNaturalist under CC0/CC BY): its atlas is painted
# (``paint_feather``) and, from v5, carries the grain of the same Crested
# Caracara's secondaries (``use="grain"``, ``GRAIN_PLAN``, ``scan_grain``):
# their luminance detail only (the barbs, the rachis' shading, the wear and
# the vane's small splits), over the chimango's own buff and brown pattern
# (v4 review: next to the carancho's scans its wings read as painted).

SCANS = {
    "carancho": dict(
        specimen="BRD 2948, Caracara cheriway, adult female, Cameron County, Texas",
        page="https://www.fws.gov/lab/featheratlas/",
        author="U.S. Fish and Wildlife Service Forensics Laboratory, The Feather Atlas",
        licence="Public domain",
        # The scans' browns lean olive under the scanner's light against the
        # photographs' blackish browns: their chroma and level brought down.
        grade=(0.70, 0.80),
        # The primaries' vanes drawn this much wider on their cards (their
        # reach over the card's half width): the emarginated outer ones are
        # narrow over their distal half, and the wing's hand read as a comb
        # (v4 review) where in flight they overlap to the notches.
        fill=dict(P=0.78),
        # Kind: (file, the feathers' indices left to right on the sheet).
        sheets=dict(
            P=("CRCA_primary_adult_bluebk.jpg", (10, 9, 8, 7, 6, 5, 4, 3, 2, 1)),
            S=("CRCA_secondary_adult_bluebk.jpg", (1, 2, 3, 4, 5, 6, 7, 8, 9, 10)),
            R=("CRCA_tail_adult_bluebk.jpg", (6, 5, 4, 3, 2, 1)),
        ),
    ),
    "chimango": dict(
        specimen="BRD 2948, Caracara cheriway, adult female, Cameron County, Texas",
        page="https://www.fws.gov/lab/featheratlas/",
        author="U.S. Fish and Wildlife Service Forensics Laboratory, The Feather Atlas",
        licence="Public domain",
        # The secondaries only: plain dark vanes (the primaries' window and
        # the tail's bars would print their own pattern through).
        use="grain",
        sheets=dict(S=("CRCA_secondary_adult_bluebk.jpg", (1, 2, 3, 4, 5, 6, 7, 8, 9, 10))),
    ),
}
PIXEL_LICENCES = ("Public domain", "CC0", "CC BY 2.0", "CC BY 3.0", "CC BY 4.0")

# Each card's cell from a scanned feather: (kind, index) -> (the sheet's
# kind, the feather's index, the part of it used from ``t0`` (0 the root) to
# the tip, mirrored). Secondaries beyond the sheet's ten, the tertials, take
# its others mirrored; no two neighbouring cards share a feather.
SCAN_PLAN = dict(
    [(("P", i), ("P", i, 0.0, False)) for i in range(1, 11)]
    + [(("S", i), ("S", i, 0.0, False)) for i in range(1, 11)]
    + [(("S", i), ("S", 20 - i, 0.0, True)) for i in range(11, 16)]
    + [(("R", i), ("R", i, 0.0, False)) for i in range(1, 7)]
    + [(("G", j), ("S", 2 * j, 0.35, False)) for j in range(1, 5)]
    + [(("M", j), ("S", 2 * j + 1, 0.50, False)) for j in range(1, 5)]
    + [(("L", j), ("S", 3 * j - 2, 0.62, j % 2 == 0)) for j in range(1, 5)]
    + [(("K", 1), ("S", 1, 0.30, False)), (("K", 2), ("S", 10, 0.30, True)), (("K", 3), ("P", 1, 0.30, False)), (("K", 4), ("P", 2, 0.30, True))]
)

# The chimango's cells' grain (``use="grain"``): (kind, index) -> (the
# secondary, the part of it used from ``t0`` to the tip, mirrored); no two
# neighbouring cards share one.
GRAIN_PLAN = dict(
    [(("P", i), (3 * i % 10 + 1, 0.0, i % 2 == 0)) for i in range(1, 11)]
    + [(("S", i), ((i + 4) % 10 + 1, 0.0, i > 10)) for i in range(1, 16)]
    + [(("R", i), (2 * i - 1, 0.0, i % 2 == 0)) for i in range(1, 7)]
    + [((k, j), ((2 * j + o) % 10 + 1, t0, j % 2 == 1)) for k, o, t0 in (("G", 0, 0.35), ("M", 3, 0.50), ("L", 5, 0.62), ("K", 7, 0.30), ("U", 1, 0.40)) for j in range(1, 5)]
)
GRAIN = 0.75  # how much of the scans' log-luminance detail the painted cells take


def box_blur(a, radius, passes=3):
    """A wide blur (three box passes, about a Gaussian) of the 2D array
    ``a`` (``felid.py``'s)."""
    np = _np()
    r = max(1, int(radius))
    for _ in range(passes):
        for axis in (0, 1):
            pad = [(0, 0), (0, 0)]
            pad[axis] = (r + 1, r)
            c = np.cumsum(np.pad(a, pad, mode="edge"), axis=axis)
            a = (np.take(c, range(2 * r + 1, c.shape[axis]), axis=axis) - np.take(c, range(0, c.shape[axis] - 2 * r - 1), axis=axis)) / (2 * r + 1)
    return a


def bilinear(img, rows, cols):
    """``img`` (h, w[, c]) sampled at fractional ``rows``, ``cols``."""
    np = _np()
    h, w = img.shape[:2]
    r = np.clip(rows, 0, h - 1.001)
    c = np.clip(cols, 0, w - 1.001)
    r0, c0 = np.floor(r).astype(int), np.floor(c).astype(int)
    fr, fc = r - r0, c - c0
    if img.ndim == 3:
        fr, fc = fr[..., None], fc[..., None]
    return (img[r0, c0] * (1 - fr) * (1 - fc) + img[r0 + 1, c0] * fr * (1 - fc)
            + img[r0, c0 + 1] * (1 - fr) * fc + img[r0 + 1, c0 + 1] * fr * fc)


def scan_alpha(pic):
    """The feathers' opacity on a scan's blue background (rows top down,
    sRGB 0..1): the background's blue over red against the feathers'
    browns, blacks and neutral whites."""
    np = _np()
    return np.clip((0.12 - (pic[..., 2] - pic[..., 0])) / 0.07, 0, 1)


def cut_scan(pic, count):
    """The ``count`` feathers of a scan sheet, left to right: each a dict
    with its box, its rachis (``shaft(row)`` -> column), the rows of its
    tip (``tip``) and of its vane's root (``base``), and its vane's
    greatest reach left (``left``) and right (``right``) of the rachis.
    A feather's columns hold a long vertical run of feather (the sheet's
    labels only short ones); the gaps between the runs part the feathers."""
    np = _np()
    a = scan_alpha(pic)
    on = a > 0.5
    h, w = on.shape
    tops, bottoms, tall = np.zeros(w, int), np.zeros(w, int), np.zeros(w, bool)
    for j in range(w):
        idx = np.flatnonzero(on[:, j])
        if len(idx) == 0:
            continue
        cut = np.flatnonzero(np.diff(idx) > 4)
        starts, ends = np.r_[idx[0], idx[cut + 1]], np.r_[idx[cut], idx[-1]]
        k = int(np.argmax(ends - starts))
        tops[j], bottoms[j] = starts[k], ends[k]
        tall[j] = ends[k] - starts[k] > 0.25 * h
    segs, j = [], 0
    while j < w:
        if tall[j]:
            k = j
            while k < w and tall[k]:
                k += 1
            if k - j >= 3:
                segs.append((j, k))
            j = k
        else:
            j += 1
    if len(segs) != count:
        raise ValueError(f"raptor: a scan sheet shows {len(segs)} feathers, not {count}")
    # Each feather's columns grown out from its long runs over the rest of
    # its vane (a short run where it is narrow), to an empty column or to
    # the emptiest one between it and its neighbour.
    rr0 = min(int(tops[c0:c1].min()) for c0, c1 in segs)
    rr1 = max(int(bottoms[c0:c1].max()) for c0, c1 in segs)
    count_col = on[rr0:rr1 + 1].sum(axis=0)
    cuts = [0] + [a + int(np.argmin(count_col[a:b + 1])) for (_, a), (b, _) in zip(segs, segs[1:])] + [w]
    bands = []
    for n, (c0, c1) in enumerate(segs):
        r0, r1 = int(tops[c0:c1].min()), int(bottoms[c0:c1].max())
        while c0 - 1 >= cuts[n] and on[r0:r1 + 1, c0 - 1].any():
            c0 -= 1
        while c1 < cuts[n + 1] and on[r0:r1 + 1, c1].any():
            c1 += 1
        bands.append((c0, c1, r0, r1))
    out = []
    lum = pic @ np.array([0.2126, 0.7152, 0.0722])
    for c0, c1, r0, r1 in bands:
        box = on[r0:r1 + 1, c0:c1]
        rows = np.arange(r0, r1 + 1)
        cols = np.arange(c0, c1)
        has = box.any(axis=1)
        left = np.where(has, np.argmax(box, axis=1), 0) + c0
        right = np.where(has, c1 - 1 - np.argmax(box[:, ::-1], axis=1), 0)
        width = np.where(has, right - left + 1, 0)
        # The quill's foot and the tip, then the rachis: the brightest line
        # down the vane (the shaft is pale on every one of these feathers).
        foot = rows[-max(3, len(rows) // 25):]
        qx = np.mean([0.5 * (left[r - r0] + right[r - r0]) for r in foot])
        tx = 0.5 * (left[0] + right[0])
        line = lambda r, qx=qx, tx=tx, r0=r0, r1=r1: qx + (tx - qx) * (r1 - r) / max(r1 - r0, 1)
        pr, px = [], []
        for r in rows[len(rows) // 12: -len(rows) // 6]:
            i = r - r0
            if width[i] < 6:
                continue
            lo, hi = int(left[i] + 0.12 * width[i]), int(right[i] - 0.12 * width[i]) + 1
            near = np.arange(lo, hi)
            near = near[np.abs(near - line(r)) < max(4.0, 0.25 * width[i])]
            if len(near) == 0:
                continue
            k = near[np.argmax(lum[r, near])]
            if lum[r, k] - np.median(lum[r, lo:hi]) > 0.12:
                pr.append(r)
                px.append(k)
        shaft = line
        if len(pr) >= 20:
            # Through the quill's foot too, so the fit holds down to it.
            pr, px = np.r_[pr, [r1] * 8], np.r_[px, [qx] * 8]
            fit = np.polyfit(pr, px, 2)
            res = np.abs(np.polyval(fit, pr) - px)
            good = res < max(2.0, 2.5 * np.median(res))
            fit = np.polyfit(pr[good], px[good], 2)
            inner = has & (np.polyval(fit, rows) >= left) & (np.polyval(fit, rows) <= right)
            if inner.mean() > 0.97 * has.mean():
                shaft = lambda r, fit=fit: np.polyval(fit, r)
        # Each row's own run of feather through the rachis (not a stray tip of
        # its neighbour's inside the box).
        own_l, own_r = left.astype(float), right.astype(float)
        for i, r in enumerate(rows):
            if not has[i]:
                continue
            k = int(np.clip(round(float(shaft(r))), c0, c1 - 1)) - c0
            line_on = box[i]
            if not line_on[k]:
                near = np.flatnonzero(line_on)
                k = int(near[np.argmin(np.abs(near - k))])
            a_, b_ = k, k
            while a_ > 0 and line_on[a_ - 1]:
                a_ -= 1
            while b_ < len(line_on) - 1 and line_on[b_ + 1]:
                b_ += 1
            own_l[i], own_r[i] = a_ + c0, b_ + c0
        left, right = own_l, own_r
        width = np.where(has, right - left + 1, 0)
        vane = np.flatnonzero(width > 0.30 * width.max())
        start = rows[vane[-1]]
        base = start + 0.04 * (start - r0)
        in_vane = has & (rows <= start)
        # The card's width holds the vane's usual reach, not its widest
        # (the outer primaries' bases, under the coverts): the card is laid
        # out to overlap its neighbours as the procedural vane did, so the
        # scanned vanes must fill it the same way (v4 review: a comb).
        reach_l = np.percentile((shaft(rows) - left)[in_vane], 65)
        reach_r = np.percentile((right - shaft(rows))[in_vane], 65)
        out.append(dict(box=(r0, r1, c0, c1), shaft=shaft, tip=float(r0), base=float(min(base, r1)), rows=rows, edges=(left, right, has),
                        left=max(float(reach_l), 2.0), right=max(float(reach_r), 2.0)))
    return out


def feather_cell(pic, f, fo, t0=0.0, flip=False):
    """The scanned feather ``f`` (``cut_scan``) resampled into a card's
    cell (``CELL`` pixels, rows from the root to the tip): the part from
    ``t0`` of its length to its tip, the rachis at ``fo`` across, the
    outer vane (the scan's left; its right if ``flip``) on the cell's
    outer side. Returns its colour (sRGB), opacity and height."""
    np = _np()
    wpx, hpx = CELL
    r0, r1, c0, c1 = f["box"]
    t = t0 + (1 - t0) * (np.arange(hpx) + 0.5) / hpx
    row = f["base"] + t * (f["tip"] - f["base"])
    s = (np.arange(wpx) + 0.5) / wpx
    q = np.where(s < fo, (s - fo) / fo, (s - fo) / (1 - fo))  # -1 at the outer edge, 1 at the inner
    q = -q if flip else q
    reach = np.where(q < 0, f["left"], f["right"]) * 1.04
    R = np.repeat(row[:, None], wpx, axis=1)
    C = f["shaft"](R) + q[None, :] * reach[None, :]
    left, right, has = f["edges"]
    lo = np.interp(R, f["rows"], np.where(has, left, 1e9))
    hi = np.interp(R, f["rows"], np.where(has, right, -1e9))
    inside = (C >= lo - 1.0) & (C <= hi + 1.0)
    alpha = bilinear(scan_alpha(pic), R, C) * inside
    # The background's blue spilled on the edges' pixels taken out, and the
    # colour carried past the outline (normalised blur), so the mip levels
    # of the alpha-tested card show no blue or dark rim.
    sub_pic = pic[r0:r1 + 1, c0:c1].copy()
    sub_a = scan_alpha(sub_pic)
    sub_pic[..., 2] = np.minimum(sub_pic[..., 2], np.maximum(sub_pic[..., 0], sub_pic[..., 1]) + 0.03)
    wsum = box_blur(sub_a, 3, passes=2)
    bled = np.stack([box_blur(sub_pic[..., k] * sub_a, 3, passes=2) for k in range(3)], -1) / np.maximum(wsum, 1e-4)[..., None]
    sub_pic = sub_pic * sub_a[..., None] + bled * (1 - sub_a[..., None])
    col = bilinear(sub_pic, R - r0, C - c0)
    lum = col @ np.array([0.2126, 0.7152, 0.0722])
    detail = lum - box_blur(lum, 4, passes=2)
    height = np.clip(0.5 + 2.0 * detail, 0, 1) * alpha
    return np.clip(col, 0, 1), alpha, height


def underside(col):
    """A feather's under side from its upper one: greyer and paler (the
    scans show the upper sides)."""
    np = _np()
    lum = (col @ np.array([0.2126, 0.7152, 0.0722]))[..., None]
    return np.clip(0.04 + 0.95 * (lum + 0.6 * (col - lum)), 0, 1)


def scan_cells(sheets, dims):
    """Every cell ``SCAN_PLAN`` fills from the loaded scan ``sheets`` (kind
    -> picture, rows top down): ``(kind, index, under)`` -> colour,
    opacity and height."""
    np = _np()
    spec = SCANS.get(NAME)
    if not spec or spec.get("use") == "grain":
        return {}
    feathers = {}
    for kind, (file, order) in spec["sheets"].items():
        if kind in sheets:
            for index, f in zip(order, cut_scan(sheets[kind], len(order))):
                k = spec.get("fill", {}).get(kind, 1.0)
                f["left"], f["right"] = k * f["left"], k * f["right"]
                feathers[(kind, index)] = (sheets[kind], f)
    out = {}
    for key, (kind, index, t0, flip) in SCAN_PLAN.items():
        fd = dims.get(key)
        if fd is None or (kind, index) not in feathers:
            continue
        pic, f = feathers[(kind, index)]
        c, a, h = feather_cell(pic, f, fd["outer"], t0, flip)
        chroma, gain = spec.get("grade", (1.0, 1.0))
        lum = (c @ np.array([0.2126, 0.7152, 0.0722]))[..., None]
        c = np.clip(gain * (lum + chroma * (c - lum)), 0, 1)
        out[key + (False,)] = (c, a, h)
        out[key + (True,)] = (underside(c), a, h)
    return out


def scan_grain(sheets, dims):
    """The scanned feathers' fine luminance detail for the cells
    ``GRAIN_PLAN`` names (``use="grain"``): ``(kind, index)`` -> its
    log-luminance over its local mean (0 on average), fine (the barbs, the
    vane's small splits) and middling (the wear, the vane's sheen), in the
    card's cell (``CELL`` pixels, rows
    from the root to the tip). Each row of the scan is stretched across the
    painted vane, its rachis on the card's (``fo``) and its own edges just
    inside the cell's, so the scan's outline and rachis print nothing."""
    np = _np()
    spec = SCANS.get(NAME)
    if not spec or spec.get("use") != "grain" or "S" not in sheets:
        return {}
    pic = sheets["S"]
    file, order = spec["sheets"]["S"]
    feathers = dict(zip(order, cut_scan(pic, len(order))))
    lum_pic = np.log(np.maximum(pic @ np.array([0.2126, 0.7152, 0.0722]), 0.02))
    wpx, hpx = CELL
    s = (np.arange(wpx) + 0.5) / wpx
    out = {}
    for key, (index, t0, flip) in GRAIN_PLAN.items():
        fd = dims.get(key)
        if fd is None or index not in feathers:
            continue
        f, fo = feathers[index], fd["outer"]
        t = t0 + (1 - t0) * (np.arange(hpx) + 0.5) / hpx
        row = f["base"] + t * (f["tip"] - f["base"])
        q = np.where(s < fo, (s - fo) / fo, (s - fo) / (1 - fo))
        q = -q if flip else q
        left, right, has = f["edges"]
        shaft = f["shaft"](row)
        lo = np.interp(row, f["rows"][has], left[has])
        hi = np.interp(row, f["rows"][has], right[has])
        reach = np.where(q[None, :] < 0, (shaft - lo)[:, None], (hi - shaft)[:, None])
        C = shaft[:, None] + 0.90 * q[None, :] * np.maximum(reach, 0.0)
        lum = bilinear(lum_pic, np.repeat(row[:, None], wpx, axis=1), C)
        # Over its local mean: the bars and the vane's broad shading go, the
        # barbs and the wear stay; nothing on the rachis (the card's own).
        fine = box_blur(lum, 2, passes=2)
        off = np.clip((np.abs(s - fo) * wpx - 1.5) / 2.0, 0, 1)[None, :]
        out[key] = (np.clip(lum - fine, -0.45, 0.45) * off, np.clip(fine - box_blur(lum, 10, passes=2), -0.25, 0.25) * off)
    return out


def regrain(col, alpha, height, grain, under=False):
    """A painted cell (``paint_feather``) with a scan's ``grain``
    (``scan_grain``) on its colour and its relief: the under side (pale,
    seen through the light) its fine grain only, fainter (v5 review: the
    middling sheen read as smears on the pale underwing)."""
    np = _np()
    fine, mid = grain
    detail = 0.5 * fine if under else fine + 0.5 * mid
    col = np.clip(col * np.exp(GRAIN * detail)[..., None], 0, 1)
    height = np.clip(height + 0.9 * detail, 0, 1) * alpha
    return col, height


# ---------------------------------------------------------------------------
# Blender: the sculpt.


def wing_section(ax, y):
    """The wing slab's section at ``ax`` out from the body and ``y``: a
    capsule from the leading edge (radius ``rle``) back to the coverts' thin
    trailing edge, ``chord`` deep. Returns the distance ``b`` behind the
    leading edge's centre, the fraction ``tt`` of the way to the trailing
    edge, the section's half thickness ``rr`` and its middle's height
    ``mid`` there, the ``chord`` and ``rle``."""
    np = _np()
    edge = leading_edge(1.0)
    lens = [0.0]
    for p, q in zip(edge, edge[1:]):
        lens.append(lens[-1] + math.hypot(q[0] - p[0], q[1] - p[1]))
    total = lens[-1]
    s = bird_scale()
    t_le = (0.0075 * s, 0.0062 * s, 0.0040 * s, 0.0022 * s)
    xs = [abs(p[0]) for p in edge]
    ex = np.clip(ax, xs[0], xs[-1])
    le_y = np.interp(ex, xs, [p[1] for p in edge])
    r = np.interp(ex, xs, lens) / total
    chord = np.interp(r, np.linspace(0, 1, 21), [chord_at(v) for v in np.linspace(0, 1, 21)])
    rle = np.interp(r, [0, 0.35, wrist_fraction(), 1.0], t_le)
    rte = 0.0016 * s
    b = y - le_y - rle
    tt = np.clip(b / np.maximum(chord - rle, 1e-6), 0, 1)
    rr = rle + (rte - rle) * tt
    mid = edge[0][2] + 0.3 * rle * (1 - tt)
    return b, tt, rr, mid, chord, rle


def wing_top(x, y):
    """The height of the wing slab's upper surface over (``x``, ``y``)."""
    b, tt, rr, mid, chord, rle = wing_section(abs(x), y)
    return float(mid + rr)


def back_top(x, y):
    """The height of the back's surface over (``x``, ``y``): the torso's
    two upper ellipsoids of ``sculpt``."""
    hw, hl, hh = SP["torso"]
    best = -1.0
    for (cy, cz), (rx, ry, rz) in (((0.0, 0.0), (hw, hl, hh)), ((-0.05 * hl, 0.22 * hh), (0.96 * hw, 0.80 * hl, 0.72 * hh))):
        q = 1.0 - (x / rx) ** 2 - ((y - cy) / ry) ** 2
        if q > 0:
            best = max(best, cz + rz * math.sqrt(q))
    return best


def wing_slab(side):
    """The wing's inner part as one signed distance: from the leading edge
    (rounded, thickest over the arm) back to the coverts' thin trailing
    edge, the flight feathers' cards root under it."""
    np = _np()
    edge = leading_edge(side)
    sz = edge[0][2]
    xs = [abs(p[0]) for p in edge]

    def fn(x, y, z):
        ax = np.abs(x)
        on = (x * side) > 0
        b, tt, rr, mid, chord, rle = wing_section(ax, y)
        db = b - tt * (chord - rle)
        dz = z - mid
        d2 = np.sqrt(db * db + dz * dz) - rr
        # Past the hand's end: rounded off.
        out = np.maximum(ax - xs[-1], 0)
        d = np.sqrt(np.maximum(d2, 0) ** 2 + out * out) + np.minimum(d2, 0)
        return np.where(on, d, 1.0)

    lo = (min(0, side * xs[-1]) - 0.01, min(p[1] for p in edge) - 0.02, sz - 0.02)
    hi = (max(0, side * xs[-1]) + 0.01, max(p[1] for p in edge) + max(chord_at(v) for v in (0, 0.5, 1)) + 0.02, sz + 0.02)
    return fn, np.asarray(lo), np.asarray(hi)


def sculpt(voxel):
    np = _np()
    hw, hl, hh = SP["torso"]
    s = bird_scale()
    span_x = wing_joints()[3][0] + 0.03
    lo = (-span_x, bill_tip()[1] - 0.012, min(p[2] for p in leg_joints()) - 0.012 * s)
    hi = (span_x, tail_root()[1] + 0.40 * SP["tail"] + 0.02, head_centre()[2] + SP["head"][2] + 0.02)
    sdf = rig.Sdf(lo, hi, voxel)
    E, RC, T = rig.sdf_ellipsoid, rig.sdf_round_cone, rig.sdf_tube
    c = torso_centre()
    sdf.add(E(c, (hw, hl, hh)))
    sdf.add(E((0, -0.45 * hl, -0.18 * hh), (0.95 * hw, 0.55 * hl, 0.95 * hh)), k=0.4 * hw)
    sdf.add(E((0, -0.62 * hl, -0.30 * hh), (0.70 * hw, 0.40 * hl, 0.72 * hh)), k=0.5 * hw)
    sdf.add(E((0, -0.05 * hl, 0.22 * hh), (0.96 * hw, 0.80 * hl, 0.72 * hh)), k=0.4 * hw)
    tr = tail_root()
    sdf.add(RC((0, 0.45 * hl, -0.05 * hh), tr, 0.62 * hw, 0.22 * hw), k=0.5 * hw)
    # Neck and head.
    nb, hc = neck_base(), head_centre()
    w, l, h = SP["head"]
    sdf.add(T([(0, -0.45 * hl, 0.15 * hh), nb, lerp(nb, hc, 0.6)], [(0.70 * hw, 0.70 * hw), (0.62 * hw, 0.62 * hw), (0.95 * w, 0.95 * w)]), k=0.35 * hw)
    sdf.add(E(hc, (w, l, h * (1 - 0.12 * SP["crown"]))), k=0.5 * w)
    if SP["crown"]:
        # The carancho's flat crown and the nape's bulk under the crest.
        sdf.add(E(add(hc, (0, 0.35 * l, 0.25 * h)), (0.9 * w, 0.7 * l, 0.7 * h)), k=0.4 * w)
    # Brow ridges over the eyes, the cheeks, and the eye sockets.
    r = SP["eye"]
    for sx in (1.0, -1.0):
        e = eye_centre(sx)
        sdf.add(E(add(e, (-sx * 0.25 * r, -0.15 * r, 1.05 * r)), (0.85 * r, 1.9 * r, 0.55 * r)), k=0.6 * r)
        sdf.add(E(add(e, (-sx * 0.3 * r, 0.5 * r, -1.8 * r)), (1.2 * r, 2.0 * r, 1.2 * r)), k=0.8 * r)
    if SP.get("eye_lens"):
        e = eye_centre(1.0)
        inner = add(e, mul(eye_look(1.0), -2.0 * r))
        depth = (sdf.surface(inner, eye_look(1.0), reach=6.0 * r) - Vector(e)).dot(Vector(eye_look(1.0)))
        print(f"raptor: {NAME}: the eye's centre {1000 * depth:.2f} mm under the skin")
    for sx in (1.0, -1.0):
        if SP.get("eye_lens"):
            # The lids' almond opening over the cornea's dome (``eyes``): a
            # little narrower than it, so the lids' rims lie over its edge.
            oy, oz = SP["eye_open"]
            sdf.carve(E(eye_centre(sx), (0.9 * r, 0.95 * oy * r, 0.95 * oz * r)), k=0.30 * r)
        else:
            sdf.carve(E(add(eye_centre(sx), (sx * 0.1 * r, 0, 0)), (0.98 * r, 0.98 * r, 0.98 * r)), k=0.25 * r)
    # The cere's stub (the bill is its own finer sculpt, ``bill_sculpt``).
    L, D, Wb = SP["bill"]
    bb = bill_base()
    cere_back = add(bb, (0, SP["cere"] + 0.25 * D, 0.10 * D))
    sdf.add(T([cere_back, add(bb, (0, 0.4 * SP["cere"], 0))], [(0.50 * Wb, 0.56 * D), (0.47 * Wb, 0.50 * D)], side=(1, 0, 0)), k=0.3 * D)
    # Wings: the arm and the hand with their coverts.
    for side in (1.0, -1.0):
        sdf.add(wing_slab(side), k=0.25 * hw)
    # Tail coverts over and under the tail's root.
    sdf.add(E(add(tr, (0, 0.20 * SP["tail"], 0.20 * hh)), (0.30 * hw, 0.26 * SP["tail"], 0.10 * hh)), k=0.4 * hw)
    sdf.add(E(add(tr, (0, 0.14 * SP["tail"], -0.24 * hh)), (0.34 * hw, 0.24 * SP["tail"], 0.12 * hh)), k=0.4 * hw)
    # Legs: the feathered thighs and tibiae, the bare tarsi, the toes and
    # the claws.
    lr = SP["leg_r"]
    for side in (1.0, -1.0):
        hip, knee, heel, foot, toe = leg_joints(side)
        sdf.add(T([hip, knee, lerp(knee, heel, 0.85)], [(0.42 * hw, 0.42 * hw), (0.34 * hw, 0.34 * hw), (1.5 * lr, 1.5 * lr)]), k=0.3 * hw)
        sdf.add(RC(heel, foot, 1.05 * lr, 0.95 * lr), k=0.6 * lr)
        to = SP["leg"][2]
        for spread, ln in ((0.0, 1.0), (0.42, 0.78), (-0.42, 0.72)):
            d1 = norm((side * spread * 0.6, 0.62, -0.25))
            k1 = add(foot, mul(d1, 0.55 * to * ln))
            d2 = norm((side * spread * 0.4, 0.35, -0.90))
            k2 = add(k1, mul(d2, 0.45 * to * ln))
            sdf.add(T([foot, k1, k2], [(0.62 * lr, 0.62 * lr), (0.55 * lr, 0.55 * lr), (0.45 * lr, 0.45 * lr)]), k=0.3 * lr)
            claw = [k2, add(k2, mul(norm((0, -0.2, -1.0)), 0.30 * to)), add(k2, mul(norm((0, -0.9, -0.6)), 0.42 * to))]
            sdf.add(T(claw, [(0.42 * lr, 0.42 * lr), (0.25 * lr, 0.25 * lr), (0.06 * lr, 0.06 * lr)]), k=0.1 * lr)
        hal = add(foot, mul(norm((0, -0.3, -0.95)), 0.40 * to))
        sdf.add(T([foot, hal], [(0.58 * lr, 0.58 * lr), (0.48 * lr, 0.48 * lr)]), k=0.2 * lr)
        sdf.add(T([hal, add(hal, mul(norm((0, -0.7, -0.6)), 0.25 * to)), add(hal, mul(norm((0, -0.95, 0.3)), 0.38 * to))],
                  [(0.42 * lr, 0.42 * lr), (0.25 * lr, 0.25 * lr), (0.06 * lr, 0.06 * lr)]), k=0.1 * lr)
    return sdf.to_mesh(f"{SP['mesh']}Sculpt")


def join_plain(into, other):
    """Joins ``other`` into ``into`` (no weights yet)."""
    bpy.ops.object.select_all(action="DESELECT")
    other.select_set(True)
    into.select_set(True)
    bpy.context.view_layer.objects.active = into
    bpy.ops.object.join()
    return into


def bill_sculpt():
    """The cere and the bill, finer than the body (a hooked tip a millimetre
    thick needs voxels a fraction of that): the upper mandible's culmen
    curving down into the hook over the lower mandible, the gape between
    them and the round nostrils in the cere."""
    L, D, Wb = SP["bill"]
    bb = bill_base()
    T, E = rig.sdf_tube, rig.sdf_ellipsoid
    voxel = D / 30.0
    lo = (-Wb, bb[1] - 1.1 * L, bb[2] - 1.2 * D)
    hi = (Wb, bb[1] + SP["cere"] + 0.6 * D, bb[2] + 0.8 * D)
    sdf = rig.Sdf(lo, hi, voxel)
    cere_back = add(bb, (0, SP["cere"] + 0.20 * D, 0.10 * D))
    sdf.add(T([cere_back, bb], [(0.50 * Wb, 0.55 * D), (0.47 * Wb, 0.50 * D)], side=(1, 0, 0)))
    up = [add(bb, (0, 0.05 * L, 0.02 * D)), add(bb, (0, -0.30 * L, 0.04 * D)), add(bb, (0, -0.62 * L, -0.04 * D)),
          add(bb, (0, -0.86 * L, -0.24 * D)), add(bb, (0, -0.98 * L, -0.52 * D)), add(bb, (0, -0.95 * L, -0.80 * D)),
          add(bb, (0, -0.86 * L, -0.96 * D))]
    sdf.add(T(up, [(0.46 * Wb, 0.48 * D), (0.41 * Wb, 0.43 * D), (0.33 * Wb, 0.34 * D), (0.22 * Wb, 0.22 * D),
                   (0.12 * Wb, 0.12 * D), (0.06 * Wb, 0.06 * D), (0.02 * Wb, 0.02 * D)], side=(1, 0, 0), samples=8), k=0.08 * D)
    lower = [add(bb, (0, 0.10 * L, -0.34 * D)), add(bb, (0, -0.30 * L, -0.42 * D)), add(bb, (0, -0.62 * L, -0.50 * D)), add(bb, (0, -0.74 * L, -0.54 * D))]
    sdf.add(T(lower, [(0.40 * Wb, 0.24 * D), (0.32 * Wb, 0.18 * D), (0.18 * Wb, 0.10 * D), (0.08 * Wb, 0.05 * D)], side=(1, 0, 0)), k=0.08 * D)
    for sx in (1.0, -1.0):
        sdf.carve(E(add(bb, (sx * 0.42 * Wb, 0.50 * SP["cere"], 0.12 * D)), (0.10 * D, 0.11 * D, 0.10 * D)), k=0.03 * D)
    return sdf.to_mesh(f"{SP['mesh']}Bill")


# ---------------------------------------------------------------------------
# Baking the plumage.


def emission_material(name, kind, lo, hi):
    """A material emitting the surface's rest position (``kind`` POS,
    normalised to ``lo``..``hi``) or its normal (NRM, 0..1)."""
    mat = bpy.data.materials.new(name)
    mat.use_nodes = True
    nt = mat.node_tree
    for nd in list(nt.nodes):
        nt.nodes.remove(nd)
    geo = nt.nodes.new("ShaderNodeNewGeometry")
    out = nt.nodes.new("ShaderNodeOutputMaterial")
    em = nt.nodes.new("ShaderNodeEmission")
    vm = nt.nodes.new("ShaderNodeVectorMath")
    vm.operation = "MULTIPLY_ADD"
    if kind == "POS":
        span = [hi[i] - lo[i] for i in range(3)]
        vm.inputs[1].default_value = [1.0 / s for s in span]
        vm.inputs[2].default_value = [-lo[i] / span[i] for i in range(3)]
        nt.links.new(geo.outputs["Position"], vm.inputs[0])
    else:
        vm.inputs[1].default_value = (0.5, 0.5, 0.5)
        vm.inputs[2].default_value = (0.5, 0.5, 0.5)
        nt.links.new(geo.outputs["Normal"], vm.inputs[0])
    nt.links.new(vm.outputs[0], em.inputs["Color"])
    nt.links.new(em.outputs[0], out.inputs["Surface"])
    return mat


def float_image(name, size, data=True):
    img = bpy.data.images.new(name, size, size, alpha=True, float_buffer=True)
    img.colorspace_settings.name = "Non-Color"
    return img


def pixels(img):
    np = _np()
    a = np.empty(img.size[0] * img.size[1] * 4, np.float32)
    img.pixels.foreach_get(a)
    return a.reshape(img.size[1], img.size[0], 4)


def set_pixels(img, rgb):
    np = _np()
    h, w = img.size[1], img.size[0]
    a = np.ones((h, w, 4), np.float32)
    a[..., :3] = rgb
    img.pixels.foreach_set(a.ravel())
    img.update()


def load_rgb(path):
    """A picture file as a numpy array, rows top down, sRGB 0..1."""
    np = _np()
    img = bpy.data.images.load(path)
    w, h = img.size
    px = np.empty(w * h * 4, np.float32)
    img.pixels.foreach_get(px)
    bpy.data.images.remove(img)
    return px.reshape(h, w, 4)[::-1, :, :3].astype(np.float64)


def bake_plumage(body, sculpt, size, face=None):
    """Bakes the sculpt's rest position and normal onto ``body``'s texels,
    paints the plumage there (``plumage``), projects the ``face``
    photograph over the head (``project_face``) and bakes the body's own
    normal map (the sculpt's shape plus the feathers' relief). Returns
    colour, normal, roughness and occlusion images."""
    np = _np()
    lo = [min(v.co[i] for v in sculpt.data.vertices) - 0.01 for i in range(3)]
    hi = [max(v.co[i] for v in sculpt.data.vertices) + 0.01 for i in range(3)]
    keep = list(sculpt.data.materials)
    sculpt.data.materials.clear()
    ext = 0.012 * bird_scale()
    out = {}
    for kind in ("POS", "NRM"):
        sculpt.data.materials.clear()
        sculpt.data.materials.append(emission_material(f"bake_{kind}", kind, lo, hi))
        img = float_image(f"bake_{kind}", size)
        set_pixels(img, np.zeros((size, size, 3), np.float32))
        rig.bake_to(body, img, "EMIT", source=sculpt, extrusion=ext, samples=1)
        out[kind] = pixels(img)[..., :3]
    px = out["POS"].reshape(-1, 3) * (np.asarray(hi) - np.asarray(lo)) + np.asarray(lo)
    nrm = out["NRM"].reshape(-1, 3) * 2 - 1
    covered = np.linalg.norm(nrm, axis=1) > 0.5
    col = np.full((size * size, 3), 0.5)
    height = np.full(size * size, 0.5)
    rough = np.full(size * size, 0.8)
    pts, nrs = px[covered], nrm[covered] / np.linalg.norm(nrm[covered], axis=1)[:, None]
    c, h, r = plumage(pts, nrs)
    if face is not None:
        # A texel's size: the median step between neighbouring texels on
        # the head.
        grid = px.reshape(size, size, 3)
        cov = covered.reshape(size, size)
        step = np.linalg.norm(grid[:, 1:] - grid[:, :-1], axis=-1)
        both = cov[:, 1:] & cov[:, :-1]
        hc = np.asarray(head_centre())
        near = np.linalg.norm(grid[:, 1:] - hc, axis=-1) < 1.5 * SP["head"][1]
        sel = step[both & near]
        footprint = float(np.median(sel[sel < 4 * np.median(sel)])) if sel.size else 0.0
        c, h, wface = project_face(pts, nrs, c, h, face, footprint)
        print(f"raptor: {NAME}: face photo on {int((wface > 0.5).sum())} texels ({int((wface > 0).sum())} blended), a texel {1000 * footprint:.2f} mm")
    col[covered], height[covered], rough[covered] = c, h, r
    colour = rig.image_from_array(f"{NAME}_body_color", col.reshape(size, size, 3).astype(np.float32))
    height_img = rig.image_from_array(f"{NAME}_body_height", np.repeat(height.reshape(size, size, 1), 3, axis=2).astype(np.float32), data=True)
    rough_img = rig.image_from_array(f"{NAME}_body_rough", np.repeat(rough.reshape(size, size, 1), 3, axis=2).astype(np.float32), data=True)
    # The sculpt's shape as a tangent normal map, then the body's own
    # normal with the feathers' relief bumped on it.
    sculpt.data.materials.clear()
    sculpt.data.materials.append(rig.plain_material("SculptPlain", (0.5, 0.5, 0.5)))
    shape = rig.new_image(f"{NAME}_body_shape", size, data=True)
    rig.bake_to(body, shape, "NORMAL", source=sculpt, extrusion=ext)
    mat = body.data.materials[0]
    nt = mat.node_tree
    bsdf = nt.nodes["Principled BSDF"]
    st = nt.nodes.new("ShaderNodeTexImage")
    st.image = shape
    nm = nt.nodes.new("ShaderNodeNormalMap")
    nt.links.new(st.outputs["Color"], nm.inputs["Color"])
    ht = nt.nodes.new("ShaderNodeTexImage")
    ht.image = height_img
    bump = nt.nodes.new("ShaderNodeBump")
    bump.inputs["Strength"].default_value = 0.55
    bump.inputs["Distance"].default_value = 0.0012 * bird_scale()
    nt.links.new(ht.outputs["Color"], bump.inputs["Height"])
    nt.links.new(nm.outputs["Normal"], bump.inputs["Normal"])
    nt.links.new(bump.outputs["Normal"], bsdf.inputs["Normal"])
    normal = rig.new_image(f"{NAME}_body_normal", size, data=True)
    rig.bake_to(body, normal, "NORMAL")
    for nd in (st, nm, ht, bump):
        nt.nodes.remove(nd)
    occlusion = rig.new_image(f"{NAME}_body_ao", size, data=True)
    sculpt.hide_render = True
    rig.bake_to(body, occlusion, "AO", samples=24)
    sculpt.hide_render = False
    ao = pixels(occlusion)[..., :1]
    cp = pixels(colour)
    cp[..., :3] *= 1.0 - 0.30 * (1.0 - ao)
    colour.pixels.foreach_set(cp.ravel())
    colour.update()
    return dict(colour=colour, normal=normal, rough=rough_img, ao=occlusion)


# ---------------------------------------------------------------------------
# The feather cards.

CARD_T = (0.0, 0.10, 0.28, 0.48, 0.68, 0.86, 1.0)
SHORT_T = (0.0, 0.5, 1.0)  # the small coverts and the scapulars: they lie flat


def card_rows(fd):
    return SHORT_T if fd["kind"] in "MLK" else CARD_T


def card_points(fd):
    """The card's vertices (rows root to tip; outer edge, rachis, inner
    edge) in rest, bending up toward the tip under the air's load, with a
    camber across."""
    d = fd["dir"]
    n = fd["normal"]
    lead = norm(cross(n, d))
    # The outer vane faces the wing's leading edge (or the tail's side).
    if fd["kind"] in "PSGUML":
        want = (fd["side"] * 0.4, -1.0, 0.0)
    elif fd["kind"] in "RK":
        want = (fd["side"] * 1.0, 0.0, 0.0)
    else:
        want = (1.0, 0.0, 0.0)
    if dot(lead, want) < 0:
        lead = mul(lead, -1)
    inner = mul(lead, -1)
    L, W, fo = fd["length"], fd["width"], fd["outer"]
    bend = {"P": 0.07, "S": 0.03, "R": 0.02, "C": -0.18, "G": 0.0, "U": 0.0, "M": 0.0, "L": 0.0, "K": 0.0}[fd["kind"]]
    rows = []
    for t in card_rows(fd):
        up = bend * L * t * t
        c = add(add(fd["root"], mul(d, t * L)), mul(n, up))
        cam = 0.02 * W
        rows.append([add(add(c, mul(inner, -fo * W)), mul(n, -cam * 0.4)), add(c, mul(n, cam * 0.6)), add(add(c, mul(inner, (1 - fo) * W)), mul(n, -cam * 0.4))])
    return rows, (fo, 0.0, 1.0)


# The light mesh (``light_mesh``): what the game draws far off, where a
# raptor circling 30 to 55 m up is a few dozen pixels across. Its outline is
# the flight feathers, so they stay (the coverts, the scapulars and the crest
# go: the body's maps already show them), each card straight, root to tip,
# instead of bent through seven rows.
LIGHT_KINDS = "PSR"  # primaries, secondaries, tail
LIGHT_ROWS = (0, 6)  # of CARD_T: the root and the tip (a straight card)
LIGHT_BODY = 450  # the skin's budget, the bill included


def light_feathers():
    """The flight feathers the light mesh keeps."""
    return [fd for fd in feather_set() if fd["kind"] in LIGHT_KINDS]


def light_triangles():
    """The light mesh's cards' triangles: two layers a card, two quads
    across each of its row gaps."""
    return sum(len(fd.get("layers", (False, True))) * 2 * 2 * (len(LIGHT_ROWS) - 1) for fd in light_feathers())


def feather_cards(name, light=False):
    """One mesh of every feather: two layers a card (upper side facing up,
    under side facing down, each its own cell of the atlas), weighted
    rigidly to the feather's bones. ``light``: only the flight feathers
    (``light_feathers``), each in ``LIGHT_ROWS`` of its rows."""
    np = _np()
    cells = atlas_cells()
    verts, faces, uvs, groups = [], [], [], []
    for fd in light_feathers() if light else feather_set():
        rows, _ = card_points(fd)
        ts = card_rows(fd)
        if light:
            rows, ts = [rows[i] for i in LIGHT_ROWS], [ts[i] for i in LIGHT_ROWS]
        gap = 0.00035 * bird_scale()
        for under in fd.get("layers", (False, True)):
            cell = cells[(fd["kind"], fd["index"], under)]
            u0, v0, u1, v1 = cell_rect(cell)
            base = len(verts)
            off = mul(fd["normal"], -gap if under else gap)
            for t, row in zip(ts, rows):
                for k, p in enumerate(row):
                    verts.append(add(p, off))
                    s = (0.0, fd["outer"], 1.0)[k]
                    uvs.append((u0 + s * (u1 - u0), v0 + t * (v1 - v0)))
                    groups.append(fd["bones"])
            for i in range(len(rows) - 1):
                for k in range(2):
                    a, b = base + i * 3 + k, base + i * 3 + k + 1
                    c, d = b + 3, a + 3
                    # Winding: the upper layer's face looks along +normal.
                    quad = (a, b, c, d)
                    pa, pb, pd = verts[a], verts[b], verts[d]
                    fn = cross(sub(pb, pa), sub(pd, pa))
                    looks_up = dot(fn, fd["normal"]) > 0
                    if looks_up == under:
                        quad = (a, d, c, b)
                    faces.append(quad)
    mesh = bpy.data.meshes.new(name)
    mesh.from_pydata(verts, [], faces)
    uv = mesh.uv_layers.new(name="UVMap")
    for poly in mesh.polygons:
        for li in poly.loop_indices:
            uv.data[li].uv = uvs[mesh.loops[li].vertex_index]
        poly.use_smooth = True
    mesh.update()
    obj = bpy.data.objects.new(name, mesh)
    bpy.context.collection.objects.link(obj)
    for i, g in enumerate(groups):
        for bone, w in g.items():
            if w <= 0:
                continue
            vg = obj.vertex_groups.get(bone) or obj.vertex_groups.new(name=bone)
            vg.add([i], w, "REPLACE")
    return obj


# ---------------------------------------------------------------------------
# Skinning the body (by place: the wings by their span, the legs by their
# bones, the rest along the spine).


def body_weights(obj):
    np = _np()
    V = np.array([v.co[:] for v in obj.data.vertices])
    n = len(V)
    hw, hl, hh = SP["torso"]
    x, y, z = V[:, 0], V[:, 1], V[:, 2]
    ax = np.abs(x)
    W = {}

    def put(bone, w):
        W[bone] = W.get(bone, 0) + w

    def sm(a, b, v):
        t = np.clip((v - a) / (b - a), 0, 1)
        return t * t * (3 - 2 * t)

    left = x > 0
    # Wings.
    s, e, w, t = wing_joints()
    sz = s[2]
    wf = sm(0.70 * hw, 1.15 * hw, ax) * sm(sz - 0.7 * hh, sz - 0.3 * hh, z)
    bw = 0.12 * SP["wing"][1]
    f_fore = sm(e[0] - bw, e[0] + bw, ax)
    f_hand = sm(w[0] - bw, w[0] + bw, ax)
    for side, mask in (("L", left), ("R", ~left)):
        m = wf * mask
        put(f"Shoulder.{side}", m * (1 - f_fore))
        put(f"Forearm.{side}", m * f_fore * (1 - f_hand))
        put(f"Hand.{side}", m * f_hand)
    rest = 1 - wf
    # Legs: the bare tarsus and toes rigid, the tibia's feathering shared
    # with the body.
    lr = SP["leg_r"]
    legw = np.zeros(n)
    for side, sgn in (("L", 1.0), ("R", -1.0)):
        hip, knee, heel, foot, toe = [np.asarray(p) for p in leg_joints(sgn)]

        def seg(a, b):
            ab = b - a
            tt = np.clip(((V - a) @ ab) / ab.dot(ab), 0, 1)
            return np.linalg.norm(V - (a + tt[:, None] * ab), axis=1), tt

        d_tar, t_tar = seg(heel, foot)
        d_toe = np.min([np.linalg.norm(V - foot, axis=1)], axis=0)
        bare = ((d_tar < 2.4 * lr) | (d_toe < 1.3 * SP["leg"][2])) & (z < heel[2] + 2 * lr) & ((x * sgn) > 0)
        toes = bare & (np.linalg.norm(V - foot, axis=1) < np.linalg.norm(V - heel, axis=1)) & (y > foot[1] - lr)
        d_tib, _ = seg(knee, heel)
        tib = (d_tib < 0.45 * hw) & ~bare & ((x * sgn) > 0) & (z < -0.55 * hh)
        put(f"Toes.{side}", rest * toes)
        put(f"Tarsus.{side}", rest * (bare & ~toes))
        put(f"Shin.{side}", rest * tib * 0.6)
        legw = legw + bare + tib * 0.6
    rest = rest * (1 - np.clip(legw, 0, 1))
    # The spine: head, neck, body and tail.
    nb, hc = np.asarray(neck_base()), np.asarray(head_centre())
    nm = (nb + hc) / 2
    tr = tail_root()
    f_neck1 = sm(nb[1] + 0.3 * SP["neck"], nb[1] - 0.2 * SP["neck"], y)
    f_neck2 = sm(nm[1] + 0.25 * SP["neck"], nm[1] - 0.25 * SP["neck"], y)
    f_head = sm(hc[1] + 0.55 * SP["head"][1], hc[1] + 0.05 * SP["head"][1], y)
    f_tail = sm(tr[1] - 0.25 * hl, tr[1] + 0.05 * hl, y)
    put("Head", rest * f_head)
    put("Neck2", rest * f_neck2 * (1 - f_head))
    put("Neck1", rest * f_neck1 * (1 - f_neck2))
    put("Tail", rest * f_tail * (1 - f_neck1))
    put("Body", rest * (1 - f_neck1) * (1 - f_tail))
    total = sum(W.values())
    for bone, wts in W.items():
        vg = obj.vertex_groups.new(name=bone)
        wts = wts / np.maximum(total, 1e-9)
        for i in np.nonzero(wts > 0.002)[0]:
            vg.add([int(i)], float(wts[i]), "REPLACE")
    return obj


# ---------------------------------------------------------------------------
# The eyes.


def eye_look(sx):
    """Where an eye looks: out to its side and a little forward (a
    falcon's eyes give it a wide binocular field ahead)."""
    return norm((sx * 0.86, -0.48, 0.10))


def eyes(iris=None):
    """The eyeballs, their pupils toward ``eye_look`` (material
    ``Eye_Black``), the rest the dark brown iris (``Eye_Iris``; ``iris``,
    linear, the face photo's)."""
    out = []
    r = SP["eye"]
    for sx in (1.0, -1.0):
        if SP.get("eye_lens"):
            out.append(eye_dome(sx, iris))
            continue
        bpy.ops.mesh.primitive_uv_sphere_add(segments=20, ring_count=12, radius=r * 0.97, location=add(eye_centre(sx), (-sx * 0.12 * r, 0, 0)))
        o = bpy.context.active_object
        o.rotation_euler = Vector((0, 0, 1)).rotation_difference(Vector(eye_look(sx))).to_euler()
        bpy.ops.object.transform_apply(rotation=True)
        look = Vector(eye_look(sx))
        o.data.materials.append(eye_material("Eye_Iris", iris))
        o.data.materials.append(eye_material("Eye_Black"))
        for p in o.data.polygons:
            p.use_smooth = True
            p.material_index = 1 if p.normal.dot(look) > math.cos(math.radians(24)) else 0
        vg = o.vertex_groups.new(name="Head")
        vg.add(list(range(len(o.data.vertices))), 1.0, "REPLACE")
        out.append(o)
    return out


def eye_dome(sx, iris=None):
    """The visible eye as a shallow dome (``SP["eye_lens"]`` deep over its
    radius) facing ``eye_look``, its centre on the lids' opening carved in
    the sculpt: the iris round a pupil about 0.4 of it across."""
    r = SP["eye"]
    bpy.ops.mesh.primitive_uv_sphere_add(segments=24, ring_count=14, radius=r)
    o = bpy.context.active_object
    o.data.materials.append(eye_material("Eye_Iris", iris))
    o.data.materials.append(eye_material("Eye_Black"))
    for p in o.data.polygons:
        p.use_smooth = True
        p.material_index = 1 if p.normal.z > math.cos(math.radians(20)) else 0
    o.scale = (1.0, 1.0, SP["eye_lens"])
    o.rotation_euler = Vector((0, 0, 1)).rotation_difference(Vector(eye_look(sx))).to_euler()
    o.location = eye_centre(sx)
    bpy.ops.object.transform_apply(location=True, rotation=True, scale=True)
    vg = o.vertex_groups.new(name="Head")
    vg.add(list(range(len(o.data.vertices))), 1.0, "REPLACE")
    return o


def eye_material(name, iris=None):
    """A dark brown iris (``iris``, linear, when given) or a black pupil,
    both under a clear glossy coat (the cornea's sharp highlight)."""
    mat = bpy.data.materials.get(name) or bpy.data.materials.new(name)
    mat.use_nodes = True
    b = mat.node_tree.nodes["Principled BSDF"]
    b.inputs["Base Color"].default_value = (tuple(iris or (0.16, 0.06, 0.018)) + (1,)) if name == "Eye_Iris" else (0.004, 0.004, 0.004, 1)
    b.inputs["Roughness"].default_value = 0.5
    b.inputs["Specular IOR Level"].default_value = 0.2
    b.inputs["Coat Weight"].default_value = 0.3
    b.inputs["Coat Roughness"].default_value = 0.02
    return mat


# ---------------------------------------------------------------------------
# Build.


def build(draft=False, textures=None, scans=None, faces=None):
    """The bird: ``scans`` (a folder holding ``SCANS``' sheets) and
    ``faces`` (one holding ``FACE_PHOTOS``' files) bring the real pixels in
    (v4); without them the atlas and the head stay painted."""
    np = _np()
    rig.FPS = FPS
    rig.reset()
    voxel = (0.0020 if NAME == "chimango" else 0.0028) * (1.8 if draft else 1.0)
    dense = sculpt(voxel)
    bill = bill_sculpt()
    budget = 5600 if NAME == "chimango" else 6000
    body = rig.decimated_copy(dense, SP["mesh"], budget=budget)
    join_plain(body, rig.decimated_copy(bill, "BillGame", budget=700))
    join_plain(dense, bill)
    body.data.materials.clear()
    body.data.materials.append(rig.plain_material("Main", (0.5, 0.42, 0.33)))
    rig.unwrap(body, margin=0.003)
    size = 1024 if draft else 2048
    face = iris = None
    if faces and NAME in FACE_PHOTOS:
        face = load_rgb(os.path.join(faces, FACE_PHOTOS[NAME]["file"]))
        iris = iris_colour(face)
    maps = bake_plumage(body, dense, size, face)
    bpy.data.objects.remove(dense)
    sheets = None
    if scans and NAME in SCANS:
        sheets = {k: load_rgb(os.path.join(scans, file)) for k, (file, _) in SCANS[NAME]["sheets"].items()}
    colour, alpha, normal, rough = paint_atlas(sheets)
    atlas = dict(
        colour=rig.image_from_array(f"{NAME}_feathers_color", colour),
        opacity=rig.image_from_array(f"{NAME}_feathers_opacity", np.repeat(alpha[..., None], 3, axis=2), data=True),
        normal=rig.image_from_array(f"{NAME}_feathers_normal", normal.astype(np.float32), data=True),
        rough=rig.image_from_array(f"{NAME}_feathers_rough", rough, data=True),
    )
    if textures:
        for folder, items in (
            (f"{NAME}_body", (("color", maps["colour"]), ("normal_gl", maps["normal"]), ("roughness", maps["rough"]), ("ao", maps["ao"]))),
            (f"{NAME}_feathers", (("color", atlas["colour"]), ("normal_gl", atlas["normal"]), ("roughness", atlas["rough"]), ("opacity", atlas["opacity"]))),
        ):
            for name, img in items:
                rig.save_jpeg(img, os.path.join(textures, folder, f"{name}.jpg"))
    arm = rig.build_armature("RaptorArmature", bone_table())
    body_weights(body)
    cards = feather_cards(f"{SP['mesh']}Feathers")
    cards.data.materials.append(rig.plain_material("Feathers", (0.4, 0.33, 0.26)))
    eye_objs = eyes(iris)
    for o in [cards] + eye_objs:
        rig.join(body, o)
    body.parent = arm
    mod = body.modifiers.new("Armature", "ARMATURE")
    mod.object = arm
    print(f"raptor: {NAME}: {rig.triangles(body)} triangles, span {2 * half_span():.3f} m, length {total_length():.3f} m")
    return body, arm, maps, atlas


def light_mesh(body, arm):
    """The light mesh the game draws far off (``Chimango_LOD``,
    ``Carancho_LOD``), as the felids' (``tools/felid.py``): the skin alone
    (no eyes) decimated to ``LIGHT_BODY`` triangles, with its UVs and its
    weights from the full one, so the same maps dress it; and the flight
    feathers in fewer rows (``light_feathers``), on the same atlas."""
    import bmesh as bm_

    skin = body.copy()
    skin.data = body.data.copy()
    skin.modifiers.clear()
    bpy.context.collection.objects.link(skin)
    main = [m.name.split(".")[0] for m in skin.data.materials].index("Main")
    bm = bm_.new()
    bm.from_mesh(skin.data)
    bm_.ops.delete(bm, geom=[f for f in bm.faces if f.material_index != main], context="FACES")
    bm.to_mesh(skin.data)
    bm.free()
    light = rig.decimated_copy(skin, f"{SP['mesh']}_LOD", LIGHT_BODY)
    bpy.data.objects.remove(skin)
    for i in reversed(range(len(light.data.materials))):
        if light.data.materials[i].name.split(".")[0] != "Main":
            light.data.materials.pop(index=i)
    cards = feather_cards(f"{SP['mesh']}_LODFeathers", light=True)
    cards.data.materials.append(bpy.data.materials["Feathers"])
    rig.join(light, cards)
    light.parent = arm
    light.modifiers.new("Armature", "ARMATURE").object = arm
    print(f"raptor: {NAME}: light mesh {rig.triangles(light)} triangles")
    return light


def preview_materials(maps, atlas):
    body = rig.textured_material("MainPreview", maps["colour"], maps["normal"], rough=0.8)
    feathers = rig.textured_material("FeathersPreview", atlas["colour"], atlas["normal"], alpha=atlas["opacity"], rough=0.75)
    feathers.use_backface_culling = True
    return {"Main": body, "Feathers": feathers}


def use_materials(mesh, by_name):
    for i, mat in enumerate(mesh.data.materials):
        name = mat.name.split(".")[0].replace("Preview", "")
        if name in by_name:
            mesh.data.materials[i] = by_name[name]


# ---------------------------------------------------------------------------
# Clips. Each pose is built by forward kinematics (``rig.Pose.turn``) about
# axes carried by the posed bones.


def axis_of(pose, bone, rest_axis):
    """``rest_axis`` (armature space at rest) as the posed ``bone`` carries
    it now."""
    m = pose.posed(bone).to_3x3() @ pose.rest(bone).to_3x3().inverted()
    return (m @ Vector(rest_axis)).normalized()


def turn(pose, bone, axis, deg, pivot=None):
    if abs(deg) < 1e-9:
        return
    pose.turn(bone, Matrix.Rotation(math.radians(deg), 3, axis), pivot)


def pose_body(pose, roll=0.0, pitch=0.0, yaw=0.0, lift=0.0):
    """Turns the whole bird about its centre: ``roll`` left wing down,
    ``pitch`` nose up, ``yaw`` toward its left; ``lift`` up."""
    c = Vector(torso_centre())
    turn(pose, "Body", Vector((0, 0, 1)), yaw, c)
    turn(pose, "Body", Vector((1, 0, 0)), -pitch, c)
    turn(pose, "Body", Vector((0, 1, 0)), roll, c)
    if lift:
        pose.move("Body", (0, 0, lift))


def pose_wing(pose, side, elev=0.0, sweep=0.0, elbow=0.0, wrist=0.0, twist=0.0, hand=0.0, spread=1.0):
    """One wing: ``elev`` raises the arm, ``sweep`` swings it forward,
    ``elbow`` and ``wrist`` flex the joints (the hand folding back),
    ``twist`` pronates the hand (leading edge down), ``hand`` raises the
    hand over the arm, ``spread`` opens the outer primaries."""
    sx = "L" if side > 0 else "R"
    sh, fa, hd, tp = (f"{b}.{sx}" for b in ("Shoulder", "Forearm", "Hand", "Tip"))
    turn(pose, sh, axis_of(pose, "Body", (0, 0, 1)), -side * sweep)
    turn(pose, sh, axis_of(pose, "Body", (0, 1, 0)), -side * elev)
    turn(pose, fa, axis_of(pose, fa, (0, 0, 1)), side * elbow)
    turn(pose, hd, axis_of(pose, hd, (0, 0, 1)), side * wrist)
    turn(pose, hd, axis_of(pose, hd, (0, 1, 0)), -side * hand)
    s, e, w, t = wing_joints(side)
    turn(pose, hd, axis_of(pose, hd, norm(sub(t, w))), side * twist)
    turn(pose, tp, axis_of(pose, tp, (0, 0, 1)), -side * 22.0 * (spread - 1.0))


def pose_tail(pose, spread=0.0, twist=0.0, pitch=0.0):
    """The tail: fanned ``spread`` degrees more at each side, twisted
    (``twist``, its left edge down) and raised (``pitch``)."""
    turn(pose, "Tail", axis_of(pose, "Tail", (1, 0, 0)), pitch)
    turn(pose, "Tail", axis_of(pose, "Tail", (0, 1, 0)), twist)
    for side, b in ((1.0, "Tail.L"), (-1.0, "Tail.R")):
        turn(pose, b, axis_of(pose, b, (0, 0, 1)), -side * -spread)


def pose_head(pose, yaw=0.0, pitch=0.0, roll=0.0):
    """Turns the head on the neck: ``yaw`` toward the bird's left,
    ``pitch`` the bill up, ``roll`` the left eye down; the upper neck takes
    a third."""
    for bone, share in (("Neck2", 0.35), ("Head", 0.65)):
        turn(pose, bone, axis_of(pose, "Body", (0, 0, 1)), yaw * share)
        turn(pose, bone, axis_of(pose, "Body", (1, 0, 0)), -pitch * share)
        turn(pose, bone, axis_of(pose, "Body", (0, 1, 0)), roll * share)


def glide(arm, t, bank=0.0):
    """Soaring: the wings in their dihedral with slow balancing, the tail
    twisting, the head scanning the ground below; ``bank`` degrees of a
    banked turn (left wing down if positive), the head kept level."""
    pose = rig.Pose(arm)
    w = 2 * math.pi * t
    dih = SP["dihedral"]
    pose_body(pose, roll=bank + 2.5 * math.sin(w), pitch=1.5 * math.sin(2 * w), yaw=0.0)
    for side in (1.0, -1.0):
        inner = bank * side > 0  # the lower wing in the turn flexes a little
        pose_wing(pose, side, elev=dih + 1.6 * math.sin(w + side * 0.7), sweep=-1.0 + 1.5 * math.sin(w + 1.0),
                  elbow=4.0 if inner and bank else 0.0, wrist=6.0 if inner and bank else 0.0,
                  hand=-0.5 * dih + 1.2 * math.sin(2 * w + side), twist=1.0 * math.sin(w + side), spread=1.05 + 0.08 * math.sin(2 * w))
    pose_tail(pose, spread=7.0 + 5.0 * math.sin(w + 0.5), twist=4.0 * math.sin(w + 1.3) + 0.35 * bank, pitch=1.5 * math.sin(2 * w))
    pose_head(pose, yaw=26.0 * math.sin(w) + 0.5 * bank, pitch=-14.0 + 4.0 * math.sin(2 * w + 0.4), roll=-0.85 * bank)
    return pose


def flap(arm, t):
    """``FLAP_BEATS`` wingbeats; the body rises on each downstroke and
    pitches a little; the head stays steady."""
    pose = rig.Pose(arm)
    beat = (t * FLAP_BEATS) % 1.0
    a = flap_angles(beat)
    s = bird_scale()
    lift = 0.006 * s * math.sin(2 * math.pi * (beat - 0.12))
    pitch = 2.5 * math.sin(2 * math.pi * (beat + 0.1))
    pose_body(pose, pitch=pitch, lift=lift)
    for side in (1.0, -1.0):
        pose_wing(pose, side, elev=a["elev"], sweep=4.0 * math.sin(2 * math.pi * beat), elbow=a["elbow"], wrist=a["wrist"],
                  twist=a["twist"], hand=a["hand"], spread=a["spread"])
    pose_tail(pose, spread=12.0 + 3.0 * math.sin(2 * math.pi * beat), pitch=-3.0 - 2.0 * math.sin(2 * math.pi * beat))
    pose_head(pose, pitch=-6.0 - pitch)
    return pose


GLIDE_SECONDS = 4.0
BANK = 24.0  # degrees: a raptor circling in a thermal


def clips(arm):
    return [
        rig.bake_clip(arm, "Glide", GLIDE_SECONDS, lambda a, t: glide(a, t), "Body"),
        rig.bake_clip(arm, "Bank_L", GLIDE_SECONDS, lambda a, t: glide(a, t, BANK), "Body"),
        rig.bake_clip(arm, "Bank_R", GLIDE_SECONDS, lambda a, t: glide(a, t, -BANK), "Body"),
        rig.bake_clip(arm, "Flap", flap_seconds(), flap, "Body"),
    ]


CLIP_ORDER = ("Glide", "Flap", "Bank_L", "Bank_R")


# ---------------------------------------------------------------------------
# Previews.


def stage(size=(1100, 850)):
    """A clear sky (no ground): the bird seen as from the pampa."""
    scene = bpy.context.scene
    scene.render.engine = "BLENDER_EEVEE"
    scene.render.resolution_x, scene.render.resolution_y = size
    scene.render.image_settings.file_format = "PNG"
    scene.render.image_settings.color_mode = "RGB"
    scene.view_settings.view_transform = "AgX"
    world = bpy.data.worlds.new("Sky")
    world.use_nodes = True
    bg = world.node_tree.nodes["Background"]
    bg.inputs["Color"].default_value = (0.52, 0.62, 0.78, 1)
    bg.inputs["Strength"].default_value = 0.9
    scene.world = world
    sun = bpy.data.objects.new("Sun", bpy.data.lights.new("Sun", "SUN"))
    sun.data.energy = 4.0
    sun.data.angle = math.radians(2)
    sun.rotation_euler = Vector((0.45, 0.35, -0.82)).to_track_quat("-Z", "Y").to_euler()
    scene.collection.objects.link(sun)
    # The ground's bounce under the wings.
    fill = bpy.data.objects.new("Fill", bpy.data.lights.new("Fill", "SUN"))
    fill.data.energy = 0.9
    fill.data.color = (0.85, 0.78, 0.62)
    fill.rotation_euler = Vector((0.0, -0.2, 1.0)).to_track_quat("-Z", "Y").to_euler()
    scene.collection.objects.link(fill)
    cam = bpy.data.objects.new("Cam", bpy.data.cameras.new("Cam"))
    scene.collection.objects.link(cam)
    scene.camera = cam
    return [sun, fill, cam], cam


def shots_for(actions):
    """``(file, action or None, frame, eye, target, lens)``."""
    by = {a.name: a for a in actions}
    b = 2 * half_span()
    d = 2.7 * b
    c = (0.0, 0.03 * b, 0.0)
    hc = head_centre()
    hr = SP["head"][1]
    glide_a = by.get("Glide")
    shots = [
        ("side", glide_a, 0, (d, 0.03 * b, 0.05 * b), c, 100),
        ("top", glide_a, 0, (0.0, 0.031 * b, d), c, 100),
        ("bottom", glide_a, 0, (0.0, 0.031 * b, -d), c, 100),
        ("front34", glide_a, 0, (0.55 * d, -0.75 * d, 0.30 * d), c, 100),
        ("below34", glide_a, 0, (0.45 * d, -0.55 * d, -0.70 * d), c, 100),
        ("rear34", glide_a, 0, (-0.5 * d, 0.75 * d, 0.35 * d), c, 100),
        ("head", None, 0, add(hc, (11 * hr, -9 * hr, 2.5 * hr)), add(hc, (0, -0.5 * hr, 0)), 105),
        ("head_side", None, 0, add(hc, (14 * hr, -2.5 * hr, 0.6 * hr)), add(hc, (0, -0.7 * hr, -0.1 * hr)), 105),
        ("head_front", None, 0, add(hc, (3 * hr, -15 * hr, 1.5 * hr)), add(hc, (0, -0.5 * hr, -0.1 * hr)), 105),
    ]
    if "Bank_L" in by:
        shots.append(("bank", by["Bank_L"], 0, (0.0, -d, -0.25 * d), c, 100))
    if "Flap" in by:
        n = round(flap_seconds() * FPS / FLAP_BEATS)
        for k, f in enumerate((0, n // 4, n // 2, 3 * n // 4)):
            shots.append((f"flap_{k}", by["Flap"], f, (0.40 * d, -0.85 * d, 0.30 * d), c, 100))
        shots.append(("flap_front", by["Flap"], n // 2, (0.0, -d, 0.0), c, 100))
    return shots


def render_shots(out_dir, arm, mesh, maps, atlas, shots):
    os.makedirs(out_dir, exist_ok=True)
    scene = bpy.context.scene
    props, cam = stage()
    exported = list(mesh.data.materials)
    use_materials(mesh, preview_materials(maps, atlas))
    for file, action, frame, eye, target, lens in shots:
        arm.animation_data_create()
        arm.animation_data.action = action
        if action is None:
            for pb in arm.pose.bones:
                pb.rotation_quaternion = Quaternion()
                pb.location = (0, 0, 0)
        scene.frame_set(frame)
        cam.location = eye
        cam.data.lens = lens
        cam.data.clip_start = 0.01
        rig.look_at(cam, target)
        scene.render.filepath = os.path.join(out_dir, f"{NAME}_{file}.png")
        bpy.ops.render.render(write_still=True)
    arm.animation_data.action = None
    for i, mat in enumerate(exported):
        mesh.data.materials[i] = mat
    for o in props:
        bpy.data.objects.remove(o)


def parse(argv):
    import argparse

    parser = argparse.ArgumentParser(prog="raptor.py")
    parser.add_argument("output")
    parser.add_argument("--species", default="chimango", choices=sorted(SPECIES))
    parser.add_argument("--preview", default=None, metavar="DIR")
    parser.add_argument("--textures", default=None, metavar="DIR")
    parser.add_argument("--shots", default=None, metavar="REGEX")
    parser.add_argument("--scans", default=None, metavar="DIR", help="cut SCANS' Feather Atlas sheets from DIR into the feather atlas")
    parser.add_argument("--faces", default=None, metavar="DIR", help="project FACE_PHOTOS from DIR onto the head")
    parser.add_argument("--draft", action="store_true")
    parser.add_argument("--no-clips", action="store_true")
    return parser.parse_args(argv)


def main():
    argv = sys.argv[sys.argv.index("--") + 1 :] if "--" in sys.argv else []
    args = parse(argv)
    configure(args.species)
    mesh, arm, maps, atlas = build(args.draft, args.textures, args.scans, args.faces)
    light = light_mesh(mesh, arm)
    light.hide_render = True
    actions = [] if args.no_clips else clips(arm)
    if args.preview:
        shots = shots_for(actions)
        if args.shots:
            shots = [s for s in shots if re.search(args.shots, s[0])]
        render_shots(args.preview, arm, mesh, maps, atlas, shots)
        # The light mesh alone, as the game draws a raptor far off.
        far = [s for s in shots_for(actions) if s[0] in ("below34", "bank")]
        if not args.shots or any(re.search(args.shots, "light_" + s[0]) for s in far):
            light.hide_render, mesh.hide_render = False, True
            render_shots(args.preview, arm, light, maps, atlas, [("light_" + s[0],) + s[1:] for s in far])
            mesh.hide_render = False
    light.hide_render = False
    for obj in (mesh, light):
        for a in list(obj.data.color_attributes):
            obj.data.color_attributes.remove(a)
    keep = {"Main", "Feathers", "Eye_Iris", "Eye_Black"}
    for mat in list(bpy.data.materials):
        if mat.name not in keep:
            bpy.data.materials.remove(mat)
    rig.export(args.output, actions)


if bpy is not None and __name__ == "__main__":
    main()
