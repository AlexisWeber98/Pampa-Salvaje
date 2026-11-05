"""Model, rig, paint and animate the pampa's two sparrows in Blender, by
script: the house sparrow (gorrión, Passer domesticus, male), of the towns'
streets and roofs, and the rufous-collared sparrow (chingolo, Zonotrichia
capensis), of the field edges and the fences.

Run it headless, once per species:

    blender -b --factory-startup --python tools/sparrow.py -- OUT.glb \
        --species gorrion|chingolo [--textures DIR] [--preview DIR] \
        [--shots REGEX] [--scans DIR] [--faces DIR] [--draft] [--no-clips]

Writes OUT.glb: one skinned mesh (``Gorrion`` or ``Chingolo``) with the
materials ``Main`` (the body, the head, the bill, the legs and the wing's
coverts slab: one sculpt baked into its colour, normal, roughness and
occlusion maps), ``Feathers`` (alpha-tested two-sided cards on one atlas:
the spread wing's flight feathers and coverts, the folded wing's, the
scapulars, the tail and the chingolo's crest) and ``Eye_Iris`` /
``Eye_Black``; a light mesh (``Gorrion_LOD``, ``Chingolo_LOD``) on the same
maps; the armature ``RaptorArmature`` (``tools/raptor.py``'s 23 bones and
``Fold.L`` / ``Fold.R``, the folded wings); and the clips (60 fps):

- ``Flap``: three wingbeats, the legs tucked under the belly;
- ``FlapBound``: the passerines' bounding flight, a burst of wingbeats and
  a short ballistic bound with the wings folded against the body;
- ``Hop``: one two-footed hop on the ground (crouch, push, flight, landing),
  in place: the game moves the bird ``HOP_LENGTH`` forward over its
  airborne part (``hop_airborne``);
- ``Peck``: the bird bends down and pecks the ground twice;
- ``Idle``: standing (on the ground, a roof, a wire), small head turns and
  tail flicks.

Life size in metres, Z up, the bird facing -Y (glTF +Z). In the flight clips
the body's centre is at the origin; in the ground clips the toes stand on
z = 0 (the body's centre ``stand_height()`` over it).

One mesh carries two wings a side: the spread wing of flight (the cards and
the sculpt's slab on the ``Shoulder`` chain) and the folded wing of a bird
at rest (cards laid along the flank on ``Fold.*``, ``fold_wing``). Each clip
shows one and collapses the other by scaling its bone to ``HIDDEN`` (a few
frames of both in ``FlapBound``'s fold and unfold): a wing folded by the
rig's bones would need a bone a feather, since the flight feathers turn on
their follicles as a wing closes. The legs are modelled standing (visible,
as a perching bird's); the flight clips tuck them under the belly.

The layout, the plumage engine, the feather cards, the skinning, the
light mesh and the previews are ``tools/raptor.py``'s, and the scan cutter
and the atlas painter ``tools/pigeon.py``'s, imported: a sparrow is
registered in both as one more species (``install``), and ``raptor``'s
functions that differ for a sparrow dispatch to this module's while one is
configured (the raptors and the pigeons are untouched). This module adds a
passerine's own: the round wing of nine primaries, six secondaries and
three tertials (``remiges``), the short conical seed-eater's bill
(``bill_sculpt``), the plump body and round head (``sculpt``), the standing
legs with their toes (``leg_joints``, ``leg_sculpt``), the folded wing, the
plumage of each species and the clips.

Real pixels: ``--scans DIR`` cuts the U.S. Fish and Wildlife Service
Feather Atlas scans (public domain) into the feather atlas: an adult male
House Sparrow's tail (the gorrión's own tail), a White-crowned Sparrow's
(Zonotrichia leucophrys, the chingolo's congener) tail for the chingolo,
and the White-crowned Sparrow's primaries and secondaries as the luminance
grain under both species' painted wings (no House Sparrow wing sheet is
archived at full size). ``--faces DIR`` projects a side photograph of each
species' face (``FACE_PHOTOS``, Wikimedia Commons, CC BY, licences re-read
through the Commons API: "House sparrow 2022 04 22" by Alexis Lours, CC BY
4.0; "Zonotrichia capensis -Buenos Aires, Argentina-8" by dfaulder, CC BY
2.0) over its head, anchored on the eye as the pigeons' (``face_mapping``),
kept off the bill and the photo's background (``face_background``), and
takes its iris colour. The other reference photographs (Wikimedia Commons,
CC0 / CC BY / CC BY-SA: CREDITS.md "Sparrow reference photographs") were
only looked at beside the renders.

Gorrión (adult male, as in the towns all year): grey crown and rump, a
chestnut band from behind the eye to the nape and the neck's sides, a small
white spot over the eye's rear, black lores meeting round the bill's base,
a white cheek, a ragged black bib on the chin and the throat, soft
grey-buff underparts paler on the belly, a rufous-brown mantle of
black-centred feathers edged rufous and buff, chestnut lesser coverts, black
median coverts with white tips (the white wing bar), greater coverts and
tertials black-centred with rufous-buff fringes, dark brown flight
feathers edged buff, a plain brown notched tail, a blackish conical bill,
pinkish-brown legs. Chingolo: grey head with black lateral crown stripes
and a raised crest, a grey central crown stripe and supercilium, a black
line behind the eye and a black malar stripe, white throat, a rufous
muted, textured rufous collar on the hind neck and the neck's sides, a black patch on the
breast's sides, whitish underparts with buff flanks, a brown mantle
streaked black, two thin whitish wing bars, rufous-fringed tertials, a
plain brown tail, a grey bill paler below, pinkish-brown legs.
"""

import math
import os
import re
import sys

try:
    import bpy
except ImportError:  # outside Blender: the layout, the plumage, the scans and the clips' timing (tests)
    bpy = None

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
import pigeon as pg  # noqa: E402  (installs the pigeons on raptor first)
import raptor as rp  # noqa: E402

if bpy is not None:
    import animal_rig as rig
    from mathutils import Matrix, Vector

FPS = rp.FPS

# ---------------------------------------------------------------------------
# The species. Lengths in metres. Literature (field guides and handbooks):
# house sparrow 14-16 cm long, span 21-25.5 cm, 24-38 g (the RSPB's and
# the BTO's species pages: 14 cm, 21-25.5 cm, about 30 g), wing (the folded
# wing's chord) of adult males 72-82 mm (Summers-Smith, "The Sparrows",
# 1988; Svensson, "Identification Guide to European Passerines", 1992: 72-82
# mm in males), tarsus about 19 mm; rufous-collared sparrow 13-15 cm long
# (Narosky & Yzurieta, "Aves de Argentina y Uruguay": 13.5 cm; Jaramillo in
# Byers, Curson & Olsson, "Sparrows and Buntings", 1995: 15 cm), wing 62-72
# mm, about 20-25 g, tarsus about 21 mm. The chingolo's span is not
# published; it follows from its wing as the house sparrow's does from its
# own. Proportions (eye, bill, head, legs) from the reference photographs.

SPECIES = {
    "gorrion": dict(
        mesh="Gorrion",
        length=(0.14, 0.16),
        span=(0.21, 0.255),
        chord=(0.072, 0.082),
        mass=0.030,
        torso=(0.0155, 0.032, 0.0150),
        neck=0.009,
        head=(0.0092, 0.0108, 0.0098),
        crown=0.0,
        # The heavy conical bill: length past the feathers, depth and width
        # at its base (the photographs: about 0.55 of the head's length,
        # nearly as deep as long).
        bill=(0.0102, 0.0074, 0.0062),
        cere=0.0010,  # the nostrils' feathered strip at the bill's base
        eye=0.0017,
        eye_at=(0.74, -0.30, 0.24),
        eye_lens=0.55,
        eye_open=(1.0, 0.98),
        face=0.0,
        ring=1.18,
        lores=False,
        soft_head=True,
        ragged=0.30,
        aspect=0.80,
        relief=0.45,
        calamus=False,
        tail=0.058,
        tail_w=0.0088,
        tail_round=-0.06,  # notched: the outer pair a little longer than the central
        wing=(0.0165, 0.0200, 0.0135),
        prim=0.0625,
        prim_w=0.0092,
        sec=0.050,
        sec_w=0.0105,
        fingers=0,
        leg=(0.024, 0.019, 0.0145),  # tibia, tarsus, middle toe
        leg_r=0.00085,
        dihedral=4.0,
        crest=0,
        stroke=(72.0, -62.0),
    ),
    "chingolo": dict(
        mesh="Chingolo",
        length=(0.13, 0.15),
        span=(0.19, 0.23),
        chord=(0.062, 0.072),
        mass=0.022,
        torso=(0.0142, 0.029, 0.0138),
        neck=0.009,
        head=(0.0085, 0.0100, 0.0092),
        crown=0.0,
        bill=(0.0090, 0.0062, 0.0054),
        cere=0.0009,
        eye=0.0018,
        eye_at=(0.74, -0.28, 0.24),
        eye_lens=0.55,
        eye_open=(1.0, 0.98),
        face=0.0,
        ring=1.18,
        lores=False,
        soft_head=True,
        ragged=0.30,
        aspect=0.80,
        relief=0.45,
        calamus=False,
        tail=0.056,
        tail_w=0.0080,
        tail_round=-0.03,
        wing=(0.0150, 0.0180, 0.0120),
        prim=0.0565,
        prim_w=0.0085,
        sec=0.045,
        sec_w=0.0095,
        fingers=0,
        leg=(0.025, 0.021, 0.0150),
        leg_r=0.00085,
        dihedral=4.0,
        crest=34,  # the crest's cards (``crest``): its loose tips over the sculpted crest
        stroke=(72.0, -62.0),
    ),
}

NAME = "gorrion"
SP = SPECIES[NAME]

# The round passerine wing (the White-crowned Sparrow's scan): nine
# primaries, P6 to P8 the longest, P9 about as long as P5, P1 three
# quarters of them; the vestigial tenth is left out. The tip is blunt, the
# outer primaries less swept than a pigeon's.
PRIMARY_LENGTHS = (0.75, 0.78, 0.82, 0.87, 0.92, 0.97, 1.00, 0.99, 0.90, 0.30)
PRIMARY_ANGLES = (86.0, 81.0, 76.0, 71.0, 65.5, 60.0, 54.5, 49.0, 44.0, 40.0)
PRIMARIES = 9
# ``raptor.remiges`` lays twelve secondaries and three tertials: a sparrow
# has six and three. These of raptor's are kept, renumbered S1-S9 (S7-S9
# the tertials).
KEEP_SECONDARIES = (1, 3, 5, 7, 9, 11, 13, 14, 15)
TERTIALS = (7, 8, 9)

HIDDEN = 1e-3  # a hidden wing's bone scale


def install():
    """Registers the sparrows as ``raptor`` (and ``pigeon``) species and
    wraps the ``raptor`` functions a sparrow changes so they dispatch here
    while one is configured. Idempotent."""
    for name, sp in SPECIES.items():
        rp.SPECIES[name] = sp
        rp.SPECIES_PAINT[name] = PAINT[name]
        rp.FEATHER_PAINT[name] = FEATHER_PAINT[name]
        rp.FEATHER_SHAFT[name] = _shaft
        pg.SCANS[name] = SCANS[name]
        rp.FACE_PHOTOS[name] = pg.FACE_PHOTOS[name] = FACE_PHOTOS[name]
    if getattr(rp, "_sparrow_installed", False):
        return
    for name, mine in (("remiges", remiges), ("bill_tip", bill_tip), ("leg_joints", leg_joints), ("eye_look", eye_look),
                       ("feather_set", feather_set), ("bone_table", bone_table), ("card_points", card_points),
                       ("coverts", pg.coverts), ("face_landmarks", face_landmarks), ("face_mapping", face_mapping),
                       ("photo_weight", photo_weight), ("plumage", plumage)):
        orig = getattr(rp, name)
        ORIGINAL[name] = orig

        def dispatch(*a, _mine=mine, _orig=orig, **k):
            return _mine(*a, **k) if rp.NAME in SPECIES else _orig(*a, **k)

        dispatch.__name__, dispatch.__doc__ = name, mine.__doc__
        setattr(rp, name, dispatch)
    rp._sparrow_installed = True


ORIGINAL = {}


def configure(name):
    """Makes ``name`` the bird ``raptor``'s (and ``pigeon``'s atlas
    functions') layout, plumage and atlas work on."""
    global NAME, SP
    install()
    rp.configure(name)
    NAME, SP = name, SPECIES[name]
    pg.NAME, pg.SP = name, SP


# ---------------------------------------------------------------------------
# The layout that differs from the raptors' (through ``install``'s
# dispatch).


def side_name(side):
    return "L" if side > 0 else "R"


def remiges(side=1.0, spread=1.0):
    """``raptor.remiges`` with a passerine's nine primaries
    (``PRIMARY_LENGTHS``, ``PRIMARY_ANGLES``), six secondaries and three
    tertials (``KEEP_SECONDARIES``, renumbered); the tertials hang from the
    arm alone (with the wing's bones scaled away they vanish whole)."""
    keep = rp.PRIMARY_LENGTHS, rp.PRIMARY_ANGLES
    rp.PRIMARY_LENGTHS, rp.PRIMARY_ANGLES = PRIMARY_LENGTHS, PRIMARY_ANGLES
    try:
        raw = ORIGINAL["remiges"](side, spread)
    finally:
        rp.PRIMARY_LENGTHS, rp.PRIMARY_ANGLES = keep
    out = []
    for fd in raw:
        if fd["kind"] == "P" and fd["index"] <= PRIMARIES:
            out.append(fd)
        elif fd["kind"] == "S" and fd["index"] in KEEP_SECONDARIES:
            fd = dict(fd)
            fd["index"] = KEEP_SECONDARIES.index(fd["index"]) + 1
            if fd["index"] in TERTIALS:
                fd["bones"] = {f"Shoulder.{side_name(side)}": 1.0}
            out.append(fd)
    return out


def bill_tip():
    """The conical bill's point: straight ahead of its base, a little under
    the culmen's line."""
    L, D, _ = SP["bill"]
    return rp.add(rp.bill_base(), (0.0, -L, -0.10 * D))


def leg_joints(side=1.0):
    """Hip, knee, heel, foot and the middle toe's tip, standing (the rest
    pose: a perching bird's legs show): the femur forward and down inside
    the body, the feathered tibia back and down to the heel just under the
    belly, the bare tarsus forward and down to the foot, the middle toe
    forward along the ground."""
    hw, hl, hh = SP["torso"]
    ti, ta, to = SP["leg"]
    hip = (side * 0.50 * hw, -0.05 * hl, -0.20 * hh)
    knee = rp.add(hip, rp.mul(rp.norm((0.0, -0.60, -0.80)), 0.55 * ti))
    heel = rp.add(knee, rp.mul(rp.norm((0.0, 0.97, -0.24)), ti))
    heel = (side * 0.42 * hw, heel[1], heel[2])
    foot = rp.add(heel, rp.mul(rp.norm((0.0, -0.55, -0.83)), ta))
    foot = (side * 0.38 * hw, foot[1], foot[2])
    toe = rp.add(foot, (0.0, -to, 0.0))
    return [hip, knee, heel, foot, toe]


def face_landmarks():
    """The head's points matching ``FACE_PHOTOS``' ``points`` (the bird's
    right side): the eye's centre, the culmen's base (where the forehead's
    feathers meet the bill), the nostril, the crown straight over the eye
    and the bill's tip."""
    L, D, Wb = SP["bill"]
    bb = rp.bill_base()
    e = rp.eye_centre(1.0)
    hc, (w, l, h) = rp.head_centre(), SP["head"]
    return dict(
        eye=e,
        culmen=rp.add(bb, (0.0, 0.0, 0.38 * D)),
        nostril=rp.add(bb, (0.36 * Wb, -0.05 * L, 0.22 * D)),
        crown=(0.3 * e[0], e[1], hc[2] + 0.98 * h),
        tip=bill_tip(),
    )


def face_mapping():
    """``pigeon.face_mapping`` (the photo anchored on the eye: its centre on
    the photo's, the eye's radius on the photo's iris, the turn from the
    other landmarks) on the sparrow's landmarks."""
    keep = pg.face_landmarks
    pg.face_landmarks = face_landmarks
    try:
        return pg.face_mapping()
    finally:
        pg.face_landmarks = keep


FACE_BACKGROUND = {}  # the face photos' background masks (``face_background``), set by ``build``


def face_background(pic, key):
    """1 where the face photo ``pic`` shows its background (``key``: the
    blue sky or the green foliage behind the bird), softened over a few
    pixels; 0 on the bird."""
    np = rp._np()
    r, g, b = pic[..., 0], pic[..., 1], pic[..., 2]
    if key == "blue":
        bg = np.clip((b - r - 0.12) / 0.10, 0, 1)
    else:
        bg = np.clip((np.minimum(g - r, g - b) - 0.03) / 0.05, 0, 1)
    return np.clip(1.6 * rp.box_blur(bg, 5.0), 0, 1)


def photo_weight(P, N, f):
    """``pigeon.photo_weight`` (kept off the bill), and off the photo's
    background (``FACE_BACKGROUND``; v2: where the model's face reached
    past the photo's head, the foliage's green showed on the lores)."""
    np = rp._np()
    w, uv = pg.photo_weight(P, N, f)
    bg = FACE_BACKGROUND.get(NAME)
    if bg is not None:
        w = w * (1 - np.clip(rp.bilinear(bg[..., None], uv[:, 1], uv[:, 0])[:, 0] * 1.5, 0, 1))
    return w, uv


def eye_look(sx):
    """A sparrow's eyes look out to the side, a little forward."""
    return rp.norm((sx * 0.90, -0.40, 0.10))


def chord():
    """The folded wing's length (the field guides' "wing"): from the wrist
    to the longest primary's tip."""
    s, e, w, t = rp.wing_joints()
    return max(rp.length(rp.sub(rp.feather_tip(fd), w)) for fd in rp.remiges() if fd["kind"] == "P")


# The torso's ellipsoids (``sculpt``, ``flank``): centre (y, z) over the
# torso's half length and height, radii over its half sizes.
TORSO = (
    ((0.0, 0.0), (1.0, 1.0, 1.0)),
    ((-0.40, -0.10), (1.0, 0.62, 1.02)),  # the round breast
    ((-0.05, 0.18), (0.95, 0.80, 0.70)),  # the back
    ((0.18, -0.20), (0.92, 0.62, 0.86)),  # the belly
)


def flank(y, z):
    """The torso's half width at (``y``, ``z``): the widest of
    ``TORSO``'s ellipsoids there (0 off them)."""
    hw, hl, hh = SP["torso"]
    best = 0.0
    for (cy, cz), (rx, ry, rz) in TORSO:
        q = 1.0 - ((y - cy * hl) / (ry * hl)) ** 2 - ((z - cz * hh) / (rz * hh)) ** 2
        if q > 0:
            best = max(best, rx * hw * math.sqrt(q))
    return best


def flank_normal(y, z):
    """The flank's outward normal at (``y``, ``z``) (side +1), its slopes
    kept moderate."""
    e = 0.0004
    fy = max(-0.5, min(0.5, (flank(y + e, z) - flank(y - e, z)) / (2 * e)))
    fz = max(-1.2, min(0.3, (flank(y, z + e) - flank(y, z - e)) / (2 * e)))
    return rp.norm((1.0, -fy, -fz))


# ---------------------------------------------------------------------------
# The folded wing: cards laid along the flank on ``Fold.*``, as a perched
# sparrow's wing: the tertials outermost over the secondaries, these over
# the primaries, whose tips project past the tertials over the rump; the
# greater, median and lesser coverts in rows over the secondaries' bases
# toward the bend of the wing; the primary coverts under them. Each kind
# reuses its flight twins' atlas cells.

FOLD_BACK = (-0.10, 1.0, -0.04)  # the folded wing's run from the bend toward the tips (rest, side +1)
FOLD_OUT = (1.0, 0.0, 0.30)  # the folded wing's outer face, leaning in up toward the back


def fold_wrist(side=1.0):
    """The bend of the folded wing, on the breast's upper side."""
    hw, hl, hh = SP["torso"]
    y, z = -0.62 * hl, 0.22 * hh
    return (side * (flank(y, z) + 0.0006 * rp.bird_scale() / 0.27), y, z)


def fold_wing(side=1.0):
    """The folded wing's cards (one side): dicts as ``raptor.remiges``'
    (``fold`` set), every one on ``Fold.<side>``."""
    hw, hl, hh = SP["torso"]
    s = rp.bird_scale() / 0.27  # 1 at the gorrión's size
    w = fold_wrist(1.0)
    back = rp.norm(FOLD_BACK)
    out_n = rp.norm(FOLD_OUT)
    down = rp.norm(rp.cross(back, out_n))  # the wing's lower edge's way in its plane
    if down[2] > 0:
        down = rp.mul(down, -1)
    lift = 0.00024 * s
    bone = {f"Fold.{side_name(side)}": 1.0}
    hum, fore, hand = SP["wing"]
    feathers = []

    def card(kind, index, along, height, layer, length, width, outer=0.42, layers=(False, True), tilt=0.0, k=0):
        """A card rooted ``along`` metres back from the bend and ``height``
        metres up the wing's face from it, ``layer`` lifts off the flank,
        its tip turned ``tilt`` (radians) down the wing's face; ``k`` (the
        card's number) gives it a small, repeatable irregularity of tilt,
        length and lift (v1 review: identical cards read as a smooth slab
        with stepped tips)."""
        if k:
            j = [_hash(k, kind, salt) for salt in (1, 2, 3)]
            tilt += 0.06 * (j[0] - 0.5)
            length *= 1.0 + 0.07 * (j[1] - 0.5)
            layer += 0.35 * j[2]
        root = rp.add(rp.add(w, rp.mul(back, along)), rp.mul(down, -height))
        # On the flank (the torso's surface there), lifted by its layer.
        fx = max(flank(root[1], root[2]), 0.55 * hw)
        root = (fx + 0.0006 * s + layer * lift, root[1], root[2])
        d = rp.norm(rp.add(rp.mul(back, math.cos(tilt)), rp.mul(down, math.sin(tilt))))
        n = out_n
        if kind in "GMLU":
            # The coverts lie tangent to the flank under their roots: on the
            # body's convex side a covert's tip then stands over the
            # feathers rooted lower down (v1 review: laid in the wing's one
            # plane, the median coverts rooted high on the back's curve
            # tipped in under the greater coverts and their white tips hid).
            n = flank_normal(root[1], root[2])
            d0 = rp.norm(rp.sub(back, rp.mul(n, rp.dot(back, n))))
            dn = rp.norm(rp.cross(d0, n))
            if dn[2] > 0:
                dn = rp.mul(dn, -1)
            d = rp.norm(rp.add(rp.mul(d0, math.cos(tilt)), rp.mul(dn, math.sin(tilt))))
        fd = dict(kind=kind, index=index, root=root, dir=d, normal=n, side=1.0, length=length, width=width,
                  outer=outer, finger=False, layers=layers, bones=bone, fold=True, order=0)
        feathers.append(fd)

    prim = {fd["index"]: fd for fd in remiges() if fd["kind"] == "P"}
    sec = {fd["index"]: fd for fd in remiges() if fd["kind"] == "S"}
    mm = 0.001 * s
    # Primaries: rooted along the hand, folded back from the bend, lowest
    # in the wing and innermost in the stack (P9 against the flank), their
    # tips fanning a little down; they project past the tertials.
    for i in range(1, PRIMARIES + 1):
        f = (i - 1) / (PRIMARIES - 1)
        card("P", i, 0.05 * hand + 0.85 * hand * f, -1.4 * mm - 1.0 * mm * f, 1 + 2.0 * (PRIMARIES - i) / PRIMARIES,
             prim[i]["length"], prim[i]["width"], outer=0.30, tilt=0.015 + 0.030 * f, k=i)
    # Secondaries: rooted along the forearm, from the bend back to the
    # elbow (folded, the forearm runs forward from the elbow to the bend),
    # over the primaries; S1 nearest the bend and lowest.
    for i in range(1, 7):
        f = (i - 1) / 5.0
        card("S", i, 0.04 * fore + 0.80 * fore * f, -0.6 * mm + 1.8 * mm * f, 4 + 0.4 * i,
             0.90 * sec[i]["length"], sec[i]["width"], tilt=0.02 - 0.010 * f, k=10 + i)
    # Tertials: at the elbow, outermost over the secondaries' inner part;
    # the secondaries' and the primaries' tips show past them.
    for k, i in enumerate(TERTIALS):
        card("S", i, 0.80 * fore + 0.14 * fore * k, 4.2 * mm - 1.7 * mm * k, 8.7 - 0.6 * k,
             sec[i]["length"] * (0.86 - 0.06 * k), sec[i]["width"] * 1.0, tilt=0.03 + 0.03 * k, k=20 + k)
    # Primary coverts (``U``) over the primaries' bases at the bend, under
    # the greater coverts.
    for j in range(1, 5):
        card("U", j, 0.5 * mm + 1.6 * mm * j, -0.6 * mm - 0.3 * mm * j, 6.0, 0.24 * SP["sec"], 0.70 * SP["prim_w"], layers=(False,), tilt=0.12, k=30 + j)
    # Greater coverts over the secondaries' bases, one over each, their
    # roots rising toward the elbow (the folded wing's upper edge curves
    # up over the back) so their tips run in a line slanting down and back
    # across the wing; each shows its black centre and broad fringe.
    for i in range(1, 9):
        f = (i - 1) / 7.0
        card("G", 1 + (i - 1) % 4, 3.0 * mm + 0.78 * fore * f, 0.8 * mm + 3.4 * mm * f, 10.4 - 0.15 * i,
             0.34 * SP["sec"] * (1.0 - 0.10 * f), 0.72 * SP["sec_w"], outer=0.45, layers=(False,), tilt=0.30 - 0.08 * f, k=40 + i)
    # Median coverts: a row over the greater coverts' bases, slanting with
    # them, their white tips the wing bar (over the greater coverts, under
    # the lesser coverts' tips).
    for i in range(1, 9):
        f = (i - 1) / 7.0
        card("M", 1 + (i + 1) % 4, 1.0 * mm + 0.62 * fore * f, 3.8 * mm + 3.0 * mm * f, 11.9 - 0.1 * i,
             0.17 * SP["sec"], 0.62 * SP["sec_w"], outer=0.45, layers=(False,), tilt=0.34, k=50 + i)
    # Lesser coverts at the bend of the wing (the gorrión's chestnut
    # shoulder), small, in two rows, their tips short of the median
    # coverts' white.
    for j in range(1, 11):
        row, k = divmod(j - 1, 5)
        card("L", 1 + j % 4, -2.0 * mm + 1.4 * mm * k + 0.8 * mm * row, 5.8 * mm + 0.3 * mm * k - 1.0 * mm * row, 12.6 - 0.1 * j + 0.5 * (1 - row),
             0.09 * SP["sec"], 0.40 * SP["sec_w"], outer=0.45, layers=(False,), tilt=0.35, k=60 + j)
    return [rp.mirror_feather(fd, side) if side < 0 else fd for fd in feathers]


def _hash(k, kind, salt):
    """A repeatable value in [0, 1) for card ``k`` of ``kind``."""
    h = math.sin(k * 12.9898 + ord(kind) * 78.233 + salt * 37.719) * 43758.5453
    return h - math.floor(h)


def back_top(x, y):
    """The height of the back over (``x``, ``y``): the highest of
    ``TORSO``'s ellipsoids there (-1 off them)."""
    hw, hl, hh = SP["torso"]
    best = -1.0
    for (cy, cz), (rx, ry, rz) in TORSO:
        q = 1.0 - (x / (rx * hw)) ** 2 - ((y - cy * hl) / (ry * hl)) ** 2
        if q > 0:
            best = max(best, cz * hh + rz * hh * math.sqrt(q))
    return best


def scapulars():
    """The scapulars (``K``): two rows a side down the mantle's edges,
    lying on the back and back over the folded wing's upper edge, on the
    body alone (the flying wing's bones are scaled away on the ground)."""
    hw, hl, hh = SP["torso"]
    out = []
    for side in (1.0, -1.0):
        for row, fx in enumerate((0.50, 0.74)):
            for k in range(4):
                y0 = (-0.62 + 0.20 * k + 0.08 * row) * hl
                d = rp.norm((side * (0.10 + 0.06 * row), 1.0, 0.0))
                fd = dict(kind="K", index=1 + (2 * row + k) % 4, root=(side * fx * hw, y0, 0.0), dir=d, normal=(0.0, 0.0, 1.0), side=side,
                          length=0.36 * hl, width=0.30 * hw, outer=0.45, finger=False, layers=(False,), bones={"Body": 1.0})
                out.append(rp.lying(fd, back_top, tilt=0.20))
    return out


def crest():
    """The chingolo's crest: the crown's feathers raised, short broad cards
    over the crown and the hind crown in three rows, overlapping, the rear
    ones longest and most raised, pointing up and back to a peak
    (``SP["crest"]`` of them; v1 review: the crest barely showed)."""
    n = SP["crest"]
    if not n:
        return []
    hc, (w, l, h) = rp.head_centre(), SP["head"]
    out = []
    for i in range(n):
        f = (i + 0.5) / n
        fx = (2 * ((i * 0.618) % 1.0) - 1.0) * 0.55
        fy = -0.60 + 1.10 * f
        back = max(fy + 0.15, 0.0)
        root = rp.add(hc, (fx * w, fy * l, h * (0.98 + 0.12 * back - 0.30 * fy * fy - 0.30 * fx * fx)))
        d = rp.norm((0.20 * fx, 1.0, 0.45 + 0.95 * back))
        nrm = rp.norm(rp.cross(d, (1.0, 0.0, 0.0)))
        if nrm[2] < 0:
            nrm = rp.mul(nrm, -1)
        # Each feather rolled out toward its side of the crown (a raised
        # crest fans; lying flat across the head its cards showed only
        # their edges from the side, as hairs).
        a = math.radians((40.0 + 25.0 * ((i * 0.53) % 1.0)) * (1.0 if fx >= 0 else -1.0))
        nrm = rp.norm(rp.add(rp.mul(nrm, math.cos(a)), rp.mul(rp.cross(d, nrm), math.sin(a))))
        # The crown's pattern on the crest: grey in the middle, black on
        # the lateral stripes, black edged grey between.
        index = 1 if abs(fx) < 0.16 else (2 if abs(fx) > 0.30 else 3)
        out.append(dict(kind="C", index=index, root=root, dir=d, normal=nrm, side=1.0,
                        length=l * (0.40 + 0.55 * back) * (0.9 + 0.2 * ((i * 0.37) % 1.0)),
                        width=1.00 * w, outer=0.5, finger=False, bones={"Head": 1.0}, layers=(False,)))
    return out


def feather_set():
    """Every card: the spread wings with their coverts, the folded wings,
    the scapulars, the tail and the chingolo's crest."""
    wings = rp.remiges(1.0) + rp.remiges(-1.0)
    flying = wings + rp.coverts(wings) + rp.covert_rows(wings)
    return flying + fold_wing(1.0) + fold_wing(-1.0) + scapulars() + rp.rectrices() + crest()


FOLD_BEND = -0.10  # the folded cards bend in toward the body (raptor's cards bend up under the air's load)


def card_points(fd):
    """``raptor.card_points``; a folded wing's card curls in toward the
    flank (``FOLD_BEND``) instead of up."""
    rows, x = ORIGINAL["card_points"](fd)
    if not fd.get("fold"):
        return rows, x
    # The coverts curl less (v1 review: the median coverts' white tips
    # curled under the greater coverts).
    bend = {"P": 0.07, "S": 0.03, "G": 0.0, "M": -0.18, "L": -0.12, "U": 0.0}.get(fd["kind"], 0.0)
    L, n = fd["length"], fd["normal"]
    out = []
    for t, row in zip(rp.card_rows(fd), rows):
        k = (FOLD_BEND - bend) * L * t * t
        out.append([rp.add(p, rp.mul(n, k)) for p in row])
    return out, x


def bone_table():
    """``raptor.bone_table`` and the folded wings' bones (on the body,
    along the folded wing from its bend)."""
    out = list(ORIGINAL["bone_table"]())
    for side in (1.0, -1.0):
        w = fold_wrist(side)
        back = rp.norm(FOLD_BACK if side > 0 else (-FOLD_BACK[0],) + FOLD_BACK[1:])
        out.append((f"Fold.{side_name(side)}", w, rp.add(w, rp.mul(back, 0.5 * SP["sec"])), "Body"))
    return out


# ---------------------------------------------------------------------------
# The plumage, per texel (``raptor.plumage`` lays the feather tracts and
# calls these). Colours are sRGB, sampled from the reference photographs.


def _local(P):
    """Head-local helpers: the eye, its radius, the head's centre and half
    sizes, the bill's base and size."""
    return rp.eye_centre(), SP["eye"], rp.head_centre(), SP["head"], rp.bill_base(), SP["bill"]


def _blob(y, z, cy, cz, ry, rz, soft=0.25):
    np = rp._np()
    q = np.sqrt(((y - cy) / ry) ** 2 + ((z - cz) / rz) ** 2)
    return np.clip((1.0 - q) / soft, 0, 1)


def _flow(P):
    """Around the body and along its feathers' lie, metres: the body's
    feather tract as ``raptor``'s ``_tract`` lays it."""
    np = rp._np()
    hw, hl, hh = SP["torso"]
    u = np.arctan2(np.abs(P[:, 0]), P[:, 2] - 0.1 * hh) * 0.8 * hw
    v = np.linalg.norm(P - np.array((0.0, -1.6 * hl, 0.25 * hh)), axis=1)
    return u, v


def strands(P):
    """The body feathers' loose barbs (0..1): fine strands along the
    feathers' lie, a quarter of a millimetre apart, wavy and gathered in
    clumps (the photographs' downy breast and flanks)."""
    np = rp._np()
    s = rp.bird_scale() / 0.27
    u, v = _flow(P)
    u = u + 0.0010 * s * (rp.vnoise(P, 1.0 / (0.0024 * s), 81) - 0.5) + 0.0004 * s * (rp.vnoise(P, 1.0 / (0.0009 * s), 82) - 0.5)
    q = np.c_[u / (0.00026 * s), v / (0.0026 * s), np.zeros(len(P))]
    fine = 0.65 * rp.vnoise(q, 1.0, 83) + 0.35 * rp.vnoise(q * np.array((2.3, 1.6, 1.0)), 1.0, 84)
    return np.clip(0.5 + 1.6 * (fine - 0.5), 0, 1)


def _downy(col, d, hv, st, k):
    """Soft body feathers over ``col`` (``k``: how much, per point): each
    feather a little lighter or darker than the next, its barbs' strands,
    the partings between the feathers' frayed tips shadowed, broken by the
    strands (v1 review: the underparts read as flat plastic)."""
    np = rp._np()
    k = np.broadcast_to(np.asarray(k, float), (len(col),))[:, None]
    h2 = np.mod(hv * 7.31 + 0.13, 1.0)
    out = col * (0.93 + 0.12 * h2)[:, None]
    out = out * (1.0 + 0.22 * (st - 0.5))[:, None]
    frayed = d + 0.18 * (st - 0.5)
    parting = np.clip((frayed - 0.84) / 0.14, 0, 1) * np.clip(1.25 - 1.5 * st, 0, 1)
    out = out * (1.0 - 0.12 * parting)[:, None]
    tip = np.clip(1 - frayed / 0.10, 0, 1)
    out = out * (1.0 + 0.07 * tip)[:, None]
    return col * (1 - k) + out * k


def _mantle(col, d, a, hv, black, outer, inner, width=0.30):
    """The mantle's black-centred feathers (``mantle_cells``): a broad black
    centre down the whole visible feather, a little narrower at its tip;
    the outer edge ``outer`` (rufous), the inner ``inner`` (buff), the tip
    fringed with it, a thin shadow under the tip's edge (v1 review: painted
    dashes on orange)."""
    np = rp._np()
    n = len(col)
    h3 = np.mod(hv * 13.7 + 0.41, 1.0)
    h4 = np.mod(hv * 5.17 + 0.29, 1.0)
    wid = width * (0.75 + 0.5 * h3) * (0.70 + 0.30 * np.clip(d / 0.45, 0, 1))
    centre = np.clip((wid - np.abs(a)) / 0.05 + 0.5, 0, 1) * np.clip((d - 0.05) / 0.05, 0, 1) * (0.55 + 0.45 * np.clip(2.5 * h4, 0, 1))
    edge = rp.mix(rp.rgb(outer, n), rp.rgb(inner, n), np.clip(0.5 + a / 0.25, 0, 1))
    edge = rp.mix(edge, rp.rgb(inner, n), np.clip(1 - d / 0.08, 0, 1))
    out = rp.mix(edge * (0.92 + 0.14 * h3)[:, None], rp.rgb(black, n), centre)
    out = out * (1.0 - 0.30 * np.clip(1 - d / 0.025, 0, 1))[:, None]
    return rp.mix(col, out, 0.92)


def plumage(P, N):
    """``raptor.plumage`` with the body's soft feathers' relief: on the
    underparts, the flanks and the rump the steps of the feathers' tips
    lowered and the barbs' strands (``strands``) raised over them (v1
    review: the scaled relief read as plastic); on the mantle a little of
    the strands too."""
    np = rp._np()
    col, height, rough = ORIGINAL["plumage"](P, N)
    R = rp.regions(P, N)
    body = (1 - R.get("head_w", R["head"].astype(float))) * ~R["wing"] * ~R["leg"] * ~R["bill"]
    soft = np.clip((R["ang"] - 1.2) / 0.5, 0, 1)
    st = strands(P)
    # About the relief's own level there, so no step shows where the soft
    # relief fades into the head's (v2: a bright crescent on the breast).
    k = soft * body
    level = float(np.median(height[k > 0.5])) if (k > 0.5).any() else 0.5
    downy = level + 0.40 * (height - level) + 0.24 * (st - 0.5)
    height = height + (downy - height) * k
    height = height + 0.10 * (st - 0.5) * (1 - soft) * body
    return col, np.clip(height, 0, 1), rough


def mantle_cells(P):
    """The mantle's feathers, larger and longer than ``raptor``'s tract
    lays them (a sparrow's back is boldly streaked, feather by feather):
    ``raptor.feather_cells`` across and along the back, their lie warped a
    little."""
    np = rp._np()
    s = rp.bird_scale() / 0.27
    hw, hl, hh = SP["torso"]
    warp = rp.vnoise(P, 1.0 / (0.010 * s), 61) - 0.5
    u = P[:, 0] + 0.0012 * s * warp
    v = P[:, 1] + 0.0016 * s * (rp.vnoise(P, 1.0 / (0.008 * s), 62) - 0.5)
    rag = 0.18 * (rp.vnoise(P, 1.0 / (0.0006 * s), 63) - 0.5)
    return rp.feather_cells(u, v, 0.0056 * s, salt=59, aspect=0.48, rag=rag)


def _eye_and_bill(P, R, col, rough, jitter, upper, lower, leg_col):
    """The bare parts both sparrows share: the lids' dark rims round the
    eye, the bill (upper and lower mandible, the lower paler at its base),
    the legs and the dark claws. Returns colour, roughness and the bare
    mask."""
    np = rp._np()
    n = len(P)
    y, z = P[:, 1], P[:, 2]
    e, r, hc, (w, l, h), bb, (L, D, Wb) = _local(P)
    oy, oz = SP["eye_open"]
    el = np.sqrt(((y - e[1]) / oy) ** 2 + ((z - e[2]) / oz) ** 2) / r
    rw = np.clip((SP["ring"] + 0.04 - el) / 0.10, 0, 1) * (np.abs(P[:, 0]) > 0.3 * w)
    col = rp.mix(col, rp.rgb((0.10, 0.09, 0.09), n), rw * rw * (3 - 2 * rw))
    # The bill: the gape's line from the base's middle to the tip; the
    # lower mandible under it.
    tipf = np.clip((bb[1] - y) / L, 0, 1)
    gape = bb[2] - 0.12 * D - (0.10 * D - 0.08 * D) * tipf
    low = np.clip((gape - z) / (0.06 * D), 0, 1)
    horn = rp.mix(rp.rgb(upper, n), rp.rgb(lower, n), low)
    horn = rp.mix(horn, rp.rgb(np.asarray(upper) * 0.8, n), np.clip((tipf - 0.75) / 0.25, 0, 1) * 0.6)
    # The culmen's ridge catching the light, the mandibles' sides darker:
    # the cone reads from the front (v1 review: a flat dark disc).
    ridge = np.clip(1 - np.abs(P[:, 0]) / (0.22 * Wb), 0, 1) * np.clip((z - gape) / (0.15 * D), 0, 1)
    horn = horn * (0.82 + 0.18 * np.clip(1 - np.abs(P[:, 0]) / (0.5 * Wb), 0, 1))[:, None]
    horn = rp.mix(horn, horn * 1.9 + 0.05, 0.6 * ridge)
    # The gape's line itself, a thin dark crease.
    crease = np.clip(1 - np.abs(z - gape) / (0.03 * D), 0, 1) * (tipf > 0.05)
    horn = horn * (1 - 0.45 * crease)[:, None]
    # The bill itself (``raptor.regions``' "bill" also takes the face's
    # feathers just ahead of its base): inside its section at the base.
    sec_ = (P[:, 0] / (0.47 * Wb)) ** 2 + ((z - (bb[2] - 0.09 * D)) / (0.47 * D)) ** 2
    bill = R["bill"] & (((sec_ < 1.1) & (y < bb[1] - 0.0002)) | (y < bb[1] - 0.12 * L))
    col = np.where(bill[:, None], horn * (0.97 + 0.06 * jitter[:, None]), col)
    col = np.where(R["leg"][:, None], rp.rgb(leg_col, n), col)
    col = np.where(R["claw"][:, None], rp.rgb((0.18, 0.15, 0.13), n), col)
    ring = rw > 0.5
    # The bill glossy: its highlights show its cone (v1 review: a dark disc
    # from the front).
    rough = np.where(bill, 0.30, np.where(R["claw"], 0.42, np.where(ring | R["leg"], 0.6, rough)))
    return col, rough, ring | bill | R["leg"]


def _bib(P, d, hv, bb, D, r):
    """The gorrión's black bib: the chin and the throat down to the top of
    the breast, a rounded patch, its lower edge ragged feather by feather
    and pale-tipped (v1 review: it ran down the breast as a tie). Returns
    the black's weight and the pale tips' weight."""
    np = rp._np()
    s_n, around = pg._neck_coord(P)
    rag = 0.22 * (hv - 0.5)
    end = np.clip((s_n - (-0.18 + rag)) / 0.05, 0, 1)
    narrow = 2.25 + 0.45 * np.clip((0.25 - s_n) / 0.45, 0, 1)
    front = np.clip((around - narrow) / 0.18, 0, 1)
    # Under the bill: the chin, from the gape back.
    chin = np.clip((gape_z(bb, D) - P[:, 2]) / (0.3 * r), 0, 1) * np.clip((P[:, 1] - (bb[1] - 0.2 * r)) / (0.4 * r), 0, 1) \
        * np.clip(1 - np.abs(P[:, 0]) / (0.9 * SP["head"][0]), 0, 1) * (s_n > 0.5)
    # On the head only under the gape's line (``_neck_coord``'s "front" also
    # takes the forehead ahead of the neck's axis).
    under = np.clip((gape_z(bb, D) + 0.2 * r - P[:, 2]) / (0.3 * r), 0, 1)
    front = front * np.where(s_n > 0.55, under, 1.0)
    black = np.maximum(front * end, chin)
    tips = np.clip(1 - d / 0.14, 0, 1) * np.clip((0.12 - s_n) / 0.18, 0, 1) * black
    return black, tips


def _paint_gorrion(P, N, R, d, a, hv, jitter, ventral, dorsal, wing, cf):
    np = rp._np()
    n = len(P)
    x, y, z = P[:, 0], P[:, 1], P[:, 2]
    ax = np.abs(x)
    hw, hl, hh = SP["torso"]
    s = rp.bird_scale()
    rough = np.full(n, 0.88)
    h2 = np.mod(hv * 7.31 + 0.13, 1.0)
    mottle = rp.vnoise(P, 1.0 / (0.020 * s), 41) - 0.5
    st = strands(P)
    e, r, hc, (w, l, h), bb, (L, D, Wb) = _local(P)
    # Underparts grey-buff and downy (the photographs: no clean white), the
    # belly paler, the flanks browner.
    below = np.clip((R["ang"] - 1.3) / 0.8, 0, 1)
    col = rp.mix(rp.rgb((0.50, 0.37, 0.25), n), rp.rgb((0.63, 0.60, 0.53), n), below)
    col = rp.mix(col, rp.rgb((0.78, 0.74, 0.65), n), below * np.clip((y + 0.05 * hl) / (0.6 * hl), 0, 1) * np.clip((R["ang"] - 2.2) / 0.6, 0, 1))
    flank_b = np.clip(1 - np.abs(R["ang"] - 1.75) / 0.45, 0, 1) * np.clip((y + 0.3 * hl) / (0.4 * hl), 0, 1)
    col = rp.mix(col, rp.rgb((0.62, 0.56, 0.47), n), 0.7 * flank_b)
    col = col * (0.96 + 0.07 * mottle)[:, None]
    col = _downy(col, d, hv, st, np.clip(below * 1.4, 0, 1))
    # The mantle: rufous-brown, each feather's broad black centre edged
    # rufous outside and buff inside.
    back = np.clip((1.3 - R["ang"]) / 0.4, 0, 1) * np.clip((0.55 * hl - y) / (0.3 * hl), 0, 1)
    md, ma, mh = mantle_cells(P)
    streaky = _mantle(col, md, ma, mh, (0.07, 0.06, 0.05), (0.56, 0.31, 0.16), (0.76, 0.63, 0.45), width=0.30)
    # The pale buff "braces" down the mantle's sides.
    brace = np.clip(1 - np.abs(ax - 0.55 * hw) / (0.12 * hw), 0, 1) * np.clip((y + 0.55 * hl) / (0.2 * hl), 0, 1) * np.clip((0.25 * hl - y) / (0.2 * hl), 0, 1)
    streaky = rp.mix(streaky, rp.rgb((0.80, 0.68, 0.50), n), 0.55 * brace * np.clip(1 - np.abs(ma) / 0.45, 0, 1))
    col = rp.mix(col, streaky, back)
    # Rump and uppertail coverts grey-brown, soft; undertail coverts pale.
    rump = np.clip((y - 0.40 * hl) / (0.25 * hl), 0, 1) * (R["ang"] < 1.4) * ~wing
    col = rp.mix(col, _downy(rp.rgb((0.48, 0.45, 0.40), n), d, hv, st, 1.0), rump)
    under = R["tailcov"] & ~R["up"] & ~wing
    col = np.where(under[:, None], _downy(rp.rgb((0.80, 0.77, 0.70), n), d, hv, st, 1.0), col)
    # The neck's sides and hind neck: chestnut from the head's band down
    # to the mantle; the nape's middle grey-brown.
    s_n, around = pg._neck_coord(P)
    nape = np.clip((1.0 - s_n) / 0.3, 0, 1) * np.clip((s_n + 0.4) / 0.3, 0, 1)
    chest_neck = nape * np.clip(1 - np.abs(around - 1.15) / 0.55, 0, 1)
    col = rp.mix(col, _downy(rp.rgb((0.50, 0.25, 0.12), n), d, hv, st, 0.6), 0.9 * chest_neck)
    # The head: grey crown; a broad chestnut band from behind the eye round
    # to the nape, curving down behind the cheek; black lores; a small white
    # spot over the eye's rear; a clear white cheek. The forehead over the
    # bill stays grey (v1 review: the black round the bill's base read as a
    # dark disc from the front).
    hcol = rp.rgb((0.52, 0.52, 0.53), n) * (0.95 + 0.07 * h2 + 0.04 * mottle)[:, None]
    side = np.clip((ax - 0.25 * w) / (0.15 * w), 0, 1)
    up = z - e[2]
    bk = y - e[1]
    band = side * np.clip((bk - 0.4 * r) / (0.6 * r), 0, 1) * np.clip((1.7 * r + 0.10 * bk - up) / (0.4 * r), 0, 1) \
        * np.clip((up - (0.1 * r - 0.45 * bk)) / (0.4 * r), 0, 1)
    hcol = rp.mix(hcol, rp.rgb((0.50, 0.23, 0.11), n) * (0.94 + 0.10 * h2)[:, None], band)
    cheek = side * np.clip((-0.45 * r - up) / (0.4 * r), 0, 1) * np.clip((bk + 1.5 * r) / (0.5 * r), 0, 1) * (1 - band)
    hcol = rp.mix(hcol, rp.rgb((0.90, 0.90, 0.87), n), cheek)
    t_l = np.clip((y - bb[1]) / (e[1] - bb[1]), 0, 1)
    lz = (bb[2] + 0.05 * D) + (e[2] - (bb[2] + 0.05 * D)) * t_l
    lores = np.clip(1 - np.abs(z - lz) / (0.55 * r), 0, 1) * np.clip((e[1] - 0.3 * r - y) / (0.3 * r), 0, 1) * np.clip((ax - 0.42 * Wb) / (0.12 * Wb), 0, 1)
    # The lores meet in a thin black line round the bill's base, so the
    # bill sits in the black of the lores and the chin.
    frame = np.clip((bb[1] + 0.35 * r - y) / (0.25 * r), 0, 1) * np.clip((bb[2] + 0.55 * D - z) / (0.12 * D), 0, 1)
    hcol = rp.mix(hcol, rp.rgb((0.07, 0.06, 0.06), n), 0.95 * np.clip(np.maximum(lores * 1.6, frame), 0, 1))
    spot = side * _blob(y, z, e[1] + 1.2 * r, e[2] + 1.0 * r, 0.75 * r, 0.40 * r, 0.4)
    hcol = rp.mix(hcol, rp.rgb((0.88, 0.88, 0.86), n), 0.9 * spot)
    col = rp.mix(col, hcol, R["head_w"] * ~wing)
    # The white of the cheek runs down behind it onto the neck's side.
    neck_white = np.clip((s_n - 0.15) / 0.2, 0, 1) * np.clip((1.05 - s_n) / 0.2, 0, 1) * np.clip(1 - np.abs(around - 2.0) / 0.35, 0, 1)
    col = rp.mix(col, _downy(rp.rgb((0.84, 0.84, 0.80), n), d, hv, st, 0.6), 0.85 * neck_white * ~wing * (R["head_w"] < 0.95))
    black, tips = _bib(P, d, hv, bb, D, r)
    bibc = rp.mix(rp.rgb((0.06, 0.05, 0.05), n) * (0.9 + 0.3 * st)[:, None], rp.rgb((0.62, 0.61, 0.58), n), 0.65 * tips)
    col = rp.mix(col, bibc, black * ~wing)
    # The wings' slab: chestnut lesser coverts, the rest brown; below,
    # grey-buff.
    along_w, behind, span = rp.wing_coords(P, 1.0)
    wup = rp.rgb((0.40, 0.28, 0.18), n) * (0.94 + 0.10 * h2 + 0.05 * mottle)[:, None]
    wup = rp.mix(wup, rp.rgb((0.50, 0.27, 0.14), n), np.clip((0.30 - cf) / 0.1, 0, 1) * (span < 0.6))
    wdn = rp.rgb((0.66, 0.63, 0.57), n) * (0.95 + 0.08 * h2)[:, None]
    col = np.where((wing & R["up"])[:, None], wup, col)
    col = np.where((wing & ~R["up"])[:, None], wdn, col)
    col, rough, bare = _eye_and_bill(P, R, col, rough, jitter, (0.20, 0.20, 0.23), (0.34, 0.33, 0.35), (0.62, 0.48, 0.42))
    return col, None, rough, bare


def gape_z(bb, D):
    return bb[2] - 0.12 * D


def _paint_chingolo(P, N, R, d, a, hv, jitter, ventral, dorsal, wing, cf):
    np = rp._np()
    n = len(P)
    x, y, z = P[:, 0], P[:, 1], P[:, 2]
    ax = np.abs(x)
    hw, hl, hh = SP["torso"]
    s = rp.bird_scale()
    rough = np.full(n, 0.88)
    h2 = np.mod(hv * 7.31 + 0.13, 1.0)
    mottle = rp.vnoise(P, 1.0 / (0.020 * s), 41) - 0.5
    st = strands(P)
    e, r, hc, (w, l, h), bb, (L, D, Wb) = _local(P)
    # Underparts grey-buff, paler on the belly, buff-brown on the flanks
    # and the vent; soft (the photographs).
    below = np.clip((R["ang"] - 1.3) / 0.8, 0, 1)
    col = rp.mix(rp.rgb((0.48, 0.39, 0.29), n), rp.rgb((0.64, 0.61, 0.56), n), below)
    col = rp.mix(col, rp.rgb((0.77, 0.74, 0.68), n), below * np.clip((y + 0.05 * hl) / (0.6 * hl), 0, 1) * np.clip((R["ang"] - 2.2) / 0.6, 0, 1))
    flank_b = np.clip(1 - np.abs(R["ang"] - 1.8) / 0.5, 0, 1) * np.clip((y + 0.4 * hl) / (0.5 * hl), 0, 1)
    col = rp.mix(col, rp.rgb((0.64, 0.55, 0.43), n), 0.85 * flank_b)
    col = rp.mix(col, rp.rgb((0.72, 0.64, 0.53), n), below * np.clip((y - 0.45 * hl) / (0.3 * hl), 0, 1) * 0.6)
    col = col * (0.96 + 0.07 * mottle)[:, None]
    col = _downy(col, d, hv, st, np.clip(below * 1.4, 0, 1))
    # The mantle: brown, each feather black-centred, edged buff and
    # rufous-brown (the photographs' strong streaks).
    back = np.clip((1.3 - R["ang"]) / 0.4, 0, 1) * np.clip((0.50 * hl - y) / (0.3 * hl), 0, 1)
    md, ma, mh = mantle_cells(P)
    streaky = _mantle(col, md, ma, mh, (0.09, 0.07, 0.05), (0.52, 0.38, 0.25), (0.74, 0.65, 0.50), width=0.24)
    col = rp.mix(col, streaky, back)
    rump = np.clip((y - 0.35 * hl) / (0.25 * hl), 0, 1) * (R["ang"] < 1.4) * ~wing
    col = rp.mix(col, _downy(rp.rgb((0.52, 0.44, 0.34), n), d, hv, st, 1.0), rump)
    under = R["tailcov"] & ~R["up"] & ~wing
    col = np.where(under[:, None], _downy(rp.rgb((0.78, 0.72, 0.62), n), d, hv, st, 1.0), col)
    # The head: grey; black lateral crown stripes, a grey central stripe;
    # a black line behind the eye; a black malar stripe under the grey
    # cheek; the throat white.
    hcol = rp.rgb((0.50, 0.50, 0.51), n) * (0.95 + 0.07 * h2 + 0.04 * mottle)[:, None]
    up = z - e[2]
    crown_band = np.clip((up - 0.9 * r) / (0.35 * r), 0, 1) * np.clip((0.26 * w - np.abs(ax - 0.40 * w)) / (0.06 * w) + 0.5 * (d - 0.5), 0, 1) \
        * np.clip((y - (e[1] - 2.8 * r)) / (0.8 * r), 0, 1)
    hcol = rp.mix(hcol, rp.rgb((0.08, 0.07, 0.07), n), 0.95 * np.clip(crown_band * 1.3, 0, 1))
    side = np.clip((ax - 0.30 * w) / (0.15 * w), 0, 1)
    eyeline = side * np.clip((0.55 * r - np.abs(up - 0.05 * r + 0.10 * (y - e[1]))) / (0.15 * r), 0, 1) \
        * np.clip((y - e[1] - 0.8 * r) / (0.4 * r), 0, 1) * np.clip((e[1] + 5.0 * r - y) / (1.0 * r), 0, 1)
    hcol = rp.mix(hcol, rp.rgb((0.09, 0.08, 0.08), n), 0.92 * eyeline)
    # The lores and the face ahead of the eye dark grey, as the face
    # photo's (it only reaches the face's side).
    lore = np.clip((ax - 0.15 * w) / (0.10 * w), 0, 1) * np.clip(1 - np.abs(up + 0.1 * r) / (0.9 * r), 0, 1) * np.clip((e[1] - 0.6 * r - y) / (0.4 * r), 0, 1)
    hcol = rp.mix(hcol, rp.rgb((0.26, 0.25, 0.25), n) * (0.9 + 0.2 * h2)[:, None], 0.85 * lore)
    malar_z = gape_z(bb, D) - 0.6 * r - 0.30 * np.clip(y - bb[1], 0, None)
    malar = side * np.clip((0.55 * r - np.abs(z - malar_z)) / (0.15 * r), 0, 1) * np.clip((y - bb[1] - 0.3 * r) / (0.5 * r), 0, 1) \
        * np.clip((e[1] + 4.0 * r - y) / (1.0 * r), 0, 1)
    hcol = rp.mix(hcol, rp.rgb((0.08, 0.07, 0.07), n), 0.92 * malar)
    # The face ahead of the eye, over the malar stripe, darker grey (the
    # face photo's tone; it only reaches the face's side).
    front = np.clip((e[1] - 0.5 * r - y) / (1.0 * r), 0, 1) * np.clip((z - malar_z - 0.3 * r) / (0.4 * r), 0, 1) * np.clip((e[2] + 1.0 * r - z) / (0.4 * r), 0, 1)
    hcol = rp.mix(hcol, rp.rgb((0.34, 0.34, 0.35), n) * (0.92 + 0.16 * h2)[:, None], 0.6 * front * (1 - malar))
    throat = np.clip((malar_z - 0.35 * r - z) / (0.3 * r), 0, 1) * np.clip(1 - ax / (0.55 * w + 0.002 * (y - bb[1]) / l), 0, 1)
    col = rp.mix(col, hcol, R["head_w"] * ~wing)
    white = np.clip((R["ang"] - 2.3) / 0.35, 0, 1) * np.clip((rp.neck_base()[1] + 0.1 * hl - y) / (0.35 * hl), 0, 1)
    col = rp.mix(col, _downy(rp.rgb((0.90, 0.89, 0.86), n), d, hv, st, 0.5), np.maximum(throat * R["head_w"], white) * ~wing)
    # The rufous collar (over the head's fade: it starts just under the
    # head): the hind neck and the neck's sides, not the throat; a muted,
    # textured cinnamon-rufous, feather by feather (v1 review: flat
    # saturated orange). The black patch on each side of the breast at its
    # lower front corner.
    s_n, around = pg._neck_coord(P)
    neck = np.clip((0.72 - s_n + 0.10 * (hv - 0.5)) / 0.12, 0, 1) * np.clip((s_n + 0.35) / 0.2, 0, 1)
    collar = neck * np.clip((2.05 - around + 0.15 * (hv - 0.5)) / 0.25, 0, 1) * ~wing
    rufous = rp.rgb((0.60, 0.38, 0.22), n) * (0.93 + 0.08 * h2)[:, None]
    rufous = rp.mix(rufous, rp.rgb((0.66, 0.46, 0.30), n), 0.35 * np.clip(1 - d / 0.15, 0, 1))
    col = rp.mix(col, _downy(rufous, d, hv, st, 0.7), 0.95 * collar)
    patch = np.clip(1 - np.abs(around - 2.30) / 0.42, 0, 1) * np.clip(1 - np.abs(s_n + 0.04) / 0.34, 0, 1)
    col = rp.mix(col, rp.rgb((0.07, 0.06, 0.06), n) * (0.9 + 0.3 * st)[:, None], np.clip(patch * 2.4 - 0.4 + 0.4 * (d - 0.5), 0, 1))
    # The wings' slab: brown, below buff-grey.
    wup = rp.rgb((0.45, 0.33, 0.22), n) * (0.94 + 0.10 * h2 + 0.05 * mottle)[:, None]
    wdn = rp.rgb((0.70, 0.66, 0.60), n) * (0.95 + 0.08 * h2)[:, None]
    col = np.where((wing & R["up"])[:, None], wup, col)
    col = np.where((wing & ~R["up"])[:, None], wdn, col)
    col, rough, bare = _eye_and_bill(P, R, col, rough, jitter, (0.24, 0.24, 0.26), (0.52, 0.52, 0.54), (0.70, 0.56, 0.48))
    return col, None, rough, bare


PAINT = {"gorrion": _paint_gorrion, "chingolo": _paint_chingolo}


# ---------------------------------------------------------------------------
# The feathers painted on their cards (under the scans' grain):
# ``raptor.paint_feather`` calls these. ``t`` along the feather (0 its
# root), ``e`` across its vane (0 the rachis, 1 the edge), ``o`` across
# (negative on the outer vane).


def _edge(t, e, o, outer_only=False, width=0.30, tip=0.10):
    """A feather's pale fringe: its vane's edge (the outer vane's only,
    ``outer_only``) and its tip."""
    np = rp._np()
    side = (o < 0) if outer_only else np.ones_like(e, bool)
    along = np.clip((t - 0.25) / 0.20, 0, 1)
    rim = np.clip((e - (1 - width)) / 0.10, 0, 1) * side * along
    return np.maximum(rim, np.clip((t - (1 - tip)) / 0.03, 0, 1)) if tip > 0 else rim


def _centred(t, e, o, centre, fringe, width=0.30, tip=0.12, outer=False):
    """A dark-centred feather: the ``centre`` colour over the rachis, its
    border following the vane a little inside the edge (``width``: the
    fringe's share of the vane), tapering toward the tip into a point along
    the shaft (``tip``: how far the pale tip reaches in), the ``fringe``
    colour outside it. ``outer``: only the outer vane is fringed (the
    flight feathers' and the tail's pale outer edges)."""
    np = rp._np()
    ts = max(0.35, 1.0 - tip - 0.30)
    taper = np.clip(1 - (t - ts) / max(1.0 - tip - ts, 0.05), 0, 1)
    cw = (1.0 - width) * (0.18 + 0.82 * taper ** 0.8)
    cw = np.where(t > 1.0 - 0.6 * tip, cw * np.clip((1.0 - 0.6 * tip - t) / 0.02 + 1, 0, 1), cw)
    if outer:
        cw = np.where(o < 0, cw, 2.0)
    dark = np.clip((cw - e) / 0.08 + 0.5, 0, 1)
    return np.asarray(fringe) * np.ones(t.shape + (1,)) + (np.asarray(centre) - np.asarray(fringe)) * dark[..., None]


def _feathers(kind, index, under, t, e, o, tm, L, pal):
    np = rp._np()
    one = np.ones(t.shape + (1,))
    if under:
        base = np.asarray(pal["under"]) * one
        if kind == "R":
            base = np.asarray(pal["tail_under"]) * one
        return base, base, 0 * t, 1.0, 0.0, None
    if kind == "P":
        col = _centred(t, e, o, pal["prim"], pal["prim_edge"], width=pal["widths"]["P"], tip=0.0, outer=True)
        return col, col, 0 * t, 1.0, 0.0, None
    if kind == "S":
        if index in TERTIALS:
            # The tertials' dark centre steps in on the outer vane two
            # thirds of the way out (the house sparrow's notched tertials),
            # its fringe paler toward the tip.
            col = _centred(t, e, o, pal["tert"], pal["tert_edge"], width=pal["widths"]["T"], tip=0.10)
            step = (o < 0) & (t > 0.62) & (e > 0.30)
            col = np.where(step[..., None], np.asarray(pal["tert_edge"]) * one, col)
            col = col + (np.asarray(pal["mantle_edge"]) - col) * (0.5 * np.clip((t - 0.80) / 0.20, 0, 1) * (col.mean(axis=-1) > 0.3))[..., None]
        else:
            col = _centred(t, e, o, pal["sec"], pal["sec_edge"], width=pal["widths"]["S"], tip=0.04, outer=True)
        return col, col, 0 * t, 1.0, 0.0, None
    if kind == "R":
        col = _centred(t, e, o, pal["tail"], pal["tail_edge"], width=pal["widths"]["R"], tip=0.0, outer=True)
        return col, col, 0 * t, 1.0, 0.0, None
    if kind == "G":
        col = _centred(t, e, o, pal["gc"], pal["gc_edge"], width=pal["widths"]["G"], tip=0.16)
        return col, col, 0 * t, 1.0, 0.0, None
    if kind == "M":
        col = _centred(t, e, o, pal["mc"], pal["mc_edge"], width=0.10, tip=pal["mc_tip"])
        return col, col, 0 * t, 1.0, 0.0, None
    if kind == "L":
        col = _centred(t, e, o, pal["lc"], pal["lc_edge"], width=0.25, tip=0.15)
        return col, col, 0 * t, 1.0, 0.0, None
    if kind == "U":
        col = _centred(t, e, o, pal["prim"], pal["prim_edge"], width=0.15, tip=0.0)
        return col, col, 0 * t, 1.0, 0.0, None
    if kind == "K":
        # The scapulars: the mantle's black-centred, buff-fringed feathers.
        streak = np.clip(1 - e / 0.45, 0, 1) * np.clip((t - 0.15) / 0.2, 0, 1)
        col = np.asarray(pal["mantle"]) * one
        col = col + (np.asarray(pal["mantle_dark"]) - col) * streak[..., None]
        col = col + (np.asarray(pal["mantle_edge"]) - col) * _edge(t, e, o, False, 0.25, 0.10)[..., None] * 0.8
        return col, col, 0 * t, 1.0, 0.0, None
    # The crest: grey, a black stripe on its sides.
    col = np.asarray(pal["crest"]) * one
    return col, col, 0 * t, 1.0, 0.0, None


PALETTES = {
    "gorrion": dict(
        under=(0.55, 0.53, 0.49), tail_under=(0.48, 0.45, 0.41),
        prim=(0.22, 0.17, 0.13), prim_edge=(0.56, 0.45, 0.33),
        sec=(0.18, 0.13, 0.10), sec_edge=(0.50, 0.34, 0.21),
        tert=(0.08, 0.07, 0.06), tert_edge=(0.55, 0.39, 0.24),
        tail=(0.33, 0.27, 0.21), tail_edge=(0.55, 0.45, 0.34),
        gc=(0.08, 0.07, 0.06), gc_edge=(0.52, 0.35, 0.21),
        mc=(0.08, 0.07, 0.06), mc_edge=(0.16, 0.13, 0.11), mc_tip=0.30,
        lc=(0.50, 0.28, 0.15), lc_edge=(0.56, 0.33, 0.18),
        mantle=(0.56, 0.36, 0.22), mantle_dark=(0.08, 0.07, 0.06), mantle_edge=(0.76, 0.63, 0.45),
        crest=(0.52, 0.52, 0.54),
        mc_white=(0.96, 0.95, 0.92),
        widths=dict(P=0.20, S=0.32, T=0.30, G=0.42, R=0.18),
    ),
    "chingolo": dict(
        under=(0.58, 0.55, 0.51), tail_under=(0.50, 0.47, 0.43),
        prim=(0.27, 0.22, 0.18), prim_edge=(0.58, 0.47, 0.35),
        sec=(0.20, 0.15, 0.11), sec_edge=(0.60, 0.44, 0.28),
        tert=(0.12, 0.09, 0.07), tert_edge=(0.66, 0.50, 0.32),
        tail=(0.36, 0.29, 0.22), tail_edge=(0.52, 0.43, 0.33),
        gc=(0.13, 0.10, 0.08), gc_edge=(0.60, 0.46, 0.31),
        mc=(0.16, 0.12, 0.09), mc_edge=(0.50, 0.38, 0.27), mc_tip=0.13,
        lc=(0.46, 0.35, 0.24), lc_edge=(0.55, 0.42, 0.30),
        mantle=(0.56, 0.42, 0.29), mantle_dark=(0.11, 0.09, 0.07), mantle_edge=(0.68, 0.56, 0.42),
        crest=(0.30, 0.29, 0.29),
        mc_white=(0.86, 0.84, 0.78),
        widths=dict(P=0.22, S=0.45, T=0.50, G=0.62, R=0.18),
    ),
}


def _feathers_species(kind, index, under, t, e, o, tm, L):
    pal = PALETTES[NAME]
    np = rp._np()
    col, dark, barred, n, w, tip = _feathers(kind, index, under, t, e, o, tm, L, pal)
    if kind in "MG" and not under:
        # The wing bars: the median coverts' white tips (the gorrión's bold
        # bar, the chingolo's thin one), the chingolo's greater coverts'
        # whitish tips the second bar.
        if kind == "M" or NAME == "chingolo":
            # A crescent: the white reaches further in along the shaft than
            # at the vane's edges.
            tipw = pal["mc_tip"] if kind == "M" else 0.08
            white = np.clip((t - (1 - tipw) - 0.10 * (1 - np.clip(e, 0, 1) ** 2) * (kind == "M" and NAME == "gorrion") + 0.05) / 0.04, 0, 1)
            col = col + (np.asarray(pal["mc_white"]) - col) * white[..., None]
    if not under and kind != "C":
        # The barbs: fine oblique lines raking toward the tip, the vane's
        # texture (v1 review: smooth cards).
        barb = 0.5 + 0.5 * np.cos(2 * np.pi * (tm - 1.3 * np.abs(o)) / (0.00042 * rp.bird_scale() / 0.27) + 9.0 * (o < 0))
        col = col * (0.90 + 0.16 * barb)[..., None]
    if kind == "C" and not under:
        # The crest (``crest``): C1 grey, C2 black, C3 black edged grey.
        # Each with a dark shaft streak (the photographs' streaked crest).
        grey, black = np.asarray((0.62, 0.62, 0.62)), np.asarray((0.07, 0.06, 0.06))
        edge = np.clip((e - 0.55) / 0.15, 0, 1)[..., None]
        shaft = np.clip(1 - e / 0.22, 0, 1)[..., None] * np.clip((t - 0.2) / 0.2, 0, 1)[..., None]
        col = {1: grey + (black - grey) * 0.75 * shaft, 2: black + (grey - black) * 0.35 * edge, 3: black + (grey - black) * edge}[index] * np.ones_like(col)
    return col, col, barred, n, w, tip


FEATHER_PAINT = {"gorrion": _feathers_species, "chingolo": _feathers_species}


def _shaft(kind, under):
    pal = PALETTES[NAME]
    base = pal["under"] if under else (pal["tert"] if kind in "SGK" else pal["prim"])
    return tuple(min(1.0, 1.25 * c + 0.04) for c in base)


# ---------------------------------------------------------------------------
# Real feathers: the Feather Atlas scans (U.S. Fish and Wildlife Service
# Forensics Laboratory: a federal work, public domain; its FAQ: "All images
# and data on the website are freely available to the public; no special
# permission is required for their use"), cut by ``pigeon``'s sheet cutter
# (``pigeon.paint_atlas``). ``scan``: the feather's pixels, toned
# (``grade``); ``grain``: only its luminance detail under the painted
# colours.

_WCSP_SHEETS = dict(
    P=dict(file="WCSP_primary_adult_bluebk.jpg", background="blue", crop=(100, 528, 88, 798),
           order=("P9", "P8", "P7", "P6", "P5", "P4", "P3", "P2", "P1")),
    S=dict(file="WCSP_secondary_adult.jpg", background="black", crop=(95, 425, 88, 798),
           order=("S1", "S2", "S3", "S4", "S5", "S6", "T1", "T2", "T3")),
)


def _grain_plan():
    plan = [(("P", i), ("grain", f"P{i}", 0.0, False)) for i in range(1, 10)]
    plan += [(("S", i), ("grain", f"S{i}", 0.0, False)) for i in range(1, 7)]
    plan += [(("S", 6 + j), ("grain", f"T{j}", 0.0, False)) for j in range(1, 4)]
    plan += [(("G", j), ("grain", f"S{2 * j - 1}", 0.40, j % 2 == 0)) for j in range(1, 5)]
    plan += [(("M", j), ("grain", f"S{j + 1}", 0.55, j % 2 == 1)) for j in range(1, 5)]
    plan += [(("L", j), ("grain", f"S{7 - j}", 0.65, j % 2 == 0)) for j in range(1, 5)]
    plan += [(("U", j), ("grain", f"P{j + 2}", 0.45, j % 2 == 1)) for j in range(1, 5)]
    plan += [(("K", j), ("grain", f"T{1 + j % 3}", 0.20, j > 2)) for j in range(1, 5)]
    return plan


def close_gaps(pic, reach=2):
    """A cut sheet (``pigeon.sheet``) with the background's thin vertical
    slits inside a feather filled from both sides: the House Sparrow's tail
    sheet's grid line leaves one down a vane's edge, and ``raptor.cut_scan``
    took the vane for two feathers."""
    np = rp._np()
    on = rp.scan_alpha(pic) > 0.5
    out = pic.copy()
    h, w = on.shape
    for k in range(1, reach + 1):
        left = np.zeros_like(on)
        right = np.zeros_like(on)
        left[:, k:] = on[:, :-k]
        right[:, :-k] = on[:, k:]
        slit = ~on & left & right
        lpic = np.zeros_like(pic)
        rpic = np.zeros_like(pic)
        lpic[:, k:] = pic[:, :-k]
        rpic[:, :-k] = pic[:, k:]
        out = np.where(slit[..., None], 0.5 * (lpic + rpic), out)
    return out


def paint_atlas(sheets=None):
    """``pigeon.paint_atlas``, the thin slits closed (``close_gaps``) on the
    sheets that need it (``slits``)."""
    keep = pg.sheet
    pg.sheet = lambda pic, spec: close_gaps(keep(pic, spec)) if spec.get("slits") else keep(pic, spec)
    try:
        return pg.paint_atlas(sheets)
    finally:
        pg.sheet = keep


SCANS = {
    "gorrion": dict(
        specimen="House Sparrow (Passer domesticus): adult male tail (BRD 3577); White-crowned Sparrow (Zonotrichia leucophrys): "
                 "adult primaries and secondaries as the wing's grain",
        page="https://www.fws.gov/lab/featheratlas/",
        author="U.S. Fish and Wildlife Service Forensics Laboratory, The Feather Atlas",
        licence="Public domain",
        sheets=dict(_WCSP_SHEETS, R=dict(file="HOSP_tail_male.jpg", background="black", crop=(170, 686, 122, 798), slits=True,
                                         order=("R6", "R5", "R4", "R3", "R2", "R1"))),
        # The House Sparrow's own tail, a little warmer.
        grade=(0.95, 0.95, (1.04, 1.0, 0.94)),
        plan=dict(_grain_plan() + [(("R", i), ("scan", f"R{i}", 0.0, False)) for i in range(1, 7)]),
    ),
    "chingolo": dict(
        specimen="White-crowned Sparrow (Zonotrichia leucophrys, the chingolo's congener): adult primaries, secondaries and tail",
        page="https://www.fws.gov/lab/featheratlas/",
        author="U.S. Fish and Wildlife Service Forensics Laboratory, The Feather Atlas",
        licence="Public domain",
        sheets=dict(_WCSP_SHEETS, R=dict(file="WCSP_tail_adult.jpg", background="black", crop=(160, 766, 133, 798),
                                         order=("R6", "R5", "R4", "R3", "R2", "R1"))),
        # The White-crowned Sparrow's grey-brown tail warmed to the
        # chingolo's brown.
        grade=(1.0, 0.92, (1.10, 1.0, 0.86)),
        plan=dict(_grain_plan() + [(("R", i), ("scan", f"R{i}", 0.0, False)) for i in range(1, 7)]),
    ),
}

# The reference photographs (Wikimedia Commons, licences re-read through the
# Commons API): looked at beside the renders for the proportions, the
# plumage and the poses; none of their pixels is used. CREDITS.md lists
# each ("Sparrow reference photographs").
REFERENCES = (
    dict(species="gorrion", page="https://commons.wikimedia.org/wiki/File:House_sparrow_2022_04_22.jpg", author="Alexis Lours", licence="CC BY 4.0"),
    dict(species="gorrion", page="https://commons.wikimedia.org/wiki/File:House_sparrow_(Passer_domesticus),_Amrum.jpg", author="Hobbyfotowiki", licence="CC0"),
    dict(species="gorrion", page="https://commons.wikimedia.org/wiki/File:Male_House_Sparrow_-_Passer_domesticus_-_28h._March_2006_Costanera,_Buenos_Aires,_Argentina_-_Flickr_-_Lip_Kee.jpg",
         author="Lip Kee from Singapore", licence="CC BY-SA 2.0"),
    dict(species="gorrion", page="https://commons.wikimedia.org/wiki/File:Sparrow_in_flight.jpg", author="davidgsteadman", licence="CC BY 2.0"),
    dict(species="gorrion", page="https://commons.wikimedia.org/wiki/File:Pardal-de-telhado,_macho,_House_Sparrow,_male_(49147686886).jpg",
         author="Luiz Lapa from Oeiras, Portugal", licence="CC BY 2.0"),
    dict(species="gorrion", page="https://commons.wikimedia.org/wiki/File:House_Sparrows_bathing_1.jpg", author="Andrey from Russia", licence="CC BY 2.0"),
    dict(species="gorrion", page="https://commons.wikimedia.org/wiki/File:House_sparrow_hop.JPG", author="HTO", licence="Public domain"),
    dict(species="gorrion", page="https://commons.wikimedia.org/wiki/File:House_sparrow_hopping.jpg", author="Karthik Easvur", licence="CC BY-SA 3.0"),
    dict(species="gorrion", page="https://commons.wikimedia.org/wiki/File:HouseSparrow_DSC0487.jpg", author="T. R. Shankar Raman", licence="CC BY-SA 4.0"),
    dict(species="chingolo", page="https://commons.wikimedia.org/wiki/File:Zonotrichia_capensis_-Buenos_Aires,_Argentina-8.jpg", author="dfaulder", licence="CC BY 2.0"),
    dict(species="chingolo", page="https://commons.wikimedia.org/wiki/File:Rufous-collared_sparrow,_(Zonotrichia_capensis),_Peru.jpg", author="Hobbyfotowiki", licence="CC0"),
    dict(species="chingolo", page="https://commons.wikimedia.org/wiki/File:Rufous-collared_Sparrow_Zonotrichia_capensis_-_Flickr_-_gailhampshire.jpg",
         author="gailhampshire from Cradley, Malvern, U.K", licence="CC BY 2.0"),
    dict(species="chingolo", page="https://commons.wikimedia.org/wiki/File:Chincol_(Zonotrichia_capensis).jpg", author="Ailinne", licence="CC BY 4.0"),
    dict(species="chingolo", page="https://commons.wikimedia.org/wiki/File:Chingolo_com%C3%BAn_Zonotrichia_capensis,_Arroyo_Balta,_Mercedes,_Argentina.jpg",
         author="AmbienteSustentable", licence="CC BY-SA 4.0"),
    dict(species="chingolo", page="https://commons.wikimedia.org/wiki/File:Chingolo_(Zonotrichia_capensis),_Uruguay,_2021.jpg", author="Enrique González", licence="CC BY-SA 4.0"),
)
# The face photographs (``raptor.project_face``, through ``face_mapping``):
# side views of an adult male house sparrow and of a chingolo of Buenos
# Aires (the pampa's own population), both licences re-read through the
# Commons API (CC BY: pixels allowed, ``raptor.PIXEL_LICENCES``), their
# originals. Landmarks in the photo's pixels, read on a grid over each head;
# ``head`` the photo's head ellipse, ``iris`` the iris' radius in pixels.
# The chingolo's raised crest is no landmark (the model's crown is lower).

FACE_PHOTOS = {
    "gorrion": dict(
        file="House_sparrow_2022_04_22.jpg",
        page="https://commons.wikimedia.org/wiki/File:House_sparrow_2022_04_22.jpg",
        author="Alexis Lours (Wikimedia Commons)", licence="CC BY 4.0",
        points=dict(eye=(2888, 1135), culmen=(3085, 1165), crown=(2888, 990), tip=(3210, 1312)),
        head=((2880, 1170), (370, 300)), iris=33, rim=0.45, bill=False, key="blue",
    ),
    "chingolo": dict(
        file="Zonotrichia_capensis_-Buenos_Aires_Argentina-8.jpg",
        page="https://commons.wikimedia.org/wiki/File:Zonotrichia_capensis_-Buenos_Aires,_Argentina-8.jpg",
        author="dfaulder (Wikimedia Commons, from Flickr)", licence="CC BY 2.0",
        # Its forehead's feathers are raised (the crest up): the culmen's
        # base sits higher over the eye than on the model, so only the
        # bill's tip turns the photo.
        points=dict(eye=(2502, 772), tip=(2765, 828)),
        head=((2470, 800), (300, 250)), iris=32, rim=0.50, bill=False, key="green",
    ),
}

REFERENCE_LICENCES = ("Public domain", "CC0", "CC BY 2.0", "CC BY 3.0", "CC BY 4.0", "CC BY-SA 2.0", "CC BY-SA 3.0", "CC BY-SA 4.0")

# ---------------------------------------------------------------------------
# Blender: the sculpt. A sparrow's body: a round, plump breast, a short
# thick neck, a big round head with a rounded forehead sloping into the
# bill's base, the wing's coverts slab, the feathered tibiae ("trousers")
# down to the heels; the bare tarsi and toes are their own finer sculpt
# (``leg_sculpt``).


def sculpt(voxel):
    hw, hl, hh = SP["torso"]
    s = rp.bird_scale()
    span_x = rp.wing_joints()[3][0] + 0.02 * s
    lo = (-span_x, bill_tip()[1] - 0.01 * s, min(p[2] for p in rp.leg_joints()) - 0.01 * s)
    hi = (span_x, rp.tail_root()[1] + 0.40 * SP["tail"] + 0.02 * s, rp.head_centre()[2] + SP["head"][2] + 0.02 * s)
    sdf = rig.Sdf(lo, hi, voxel)
    E, RC, T = rig.sdf_ellipsoid, rig.sdf_round_cone, rig.sdf_tube
    for k, ((cy, cz), (rx, ry, rz)) in enumerate(TORSO):
        sdf.add(E((0, cy * hl, cz * hh), (rx * hw, ry * hl, rz * hh)), k=0.0 if k == 0 else 0.45 * hw)
    tr = rp.tail_root()
    sdf.add(RC((0, 0.40 * hl, -0.02 * hh), tr, 0.62 * hw, 0.24 * hw), k=0.5 * hw)
    nb, hc = rp.neck_base(), rp.head_centre()
    w, l, h = SP["head"]
    # Blended softly into the breast (v2: a tighter blend left a crease
    # that caught the light as a pale arc across the breast).
    sdf.add(T([(0, -0.40 * hl, 0.10 * hh), nb, rp.lerp(nb, hc, 0.6)], [(0.82 * hw, 0.84 * hw), (0.74 * hw, 0.76 * hw), (0.95 * w, 0.95 * w)]), k=0.75 * hw)
    sdf.add(E(hc, (w, l, h)), k=0.5 * w)
    # The forehead rounding down into the bill's base; the hind crown's
    # bulk (the chingolo's crest rises from it).
    sdf.add(E(rp.add(hc, (0, -0.50 * l, -0.05 * h)), (0.72 * w, 0.58 * l, 0.72 * h)), k=0.5 * w)
    sdf.add(E(rp.add(hc, (0, 0.35 * l, 0.15 * h)), (0.85 * w, 0.65 * l, 0.80 * h)), k=0.5 * w)
    if SP["crest"]:
        # The chingolo's crest: the hind crown's feathers raised in a short
        # peak, pointing back.
        sdf.add(E(rp.add(hc, (0, 0.22 * l, 0.78 * h)), (0.50 * w, 0.62 * l, 0.36 * h), rotation=None), k=0.45 * w)
        sdf.add(E(rp.add(hc, (0, 0.55 * l, 0.92 * h)), (0.30 * w, 0.42 * l, 0.22 * h)), k=0.35 * w)
    # The cheeks and the chin under the bill: full, so the face is round.
    for sx in (1.0, -1.0):
        sdf.add(E(rp.add(hc, (sx * 0.35 * w, -0.20 * l, -0.45 * h)), (0.55 * w, 0.60 * l, 0.50 * h)), k=0.4 * w)
    r = SP["eye"]
    for sx in (1.0, -1.0):
        oy, oz = SP["eye_open"]
        sdf.carve(E(rp.eye_centre(sx), (0.9 * r, 0.95 * oy * r, 0.95 * oz * r)), k=0.30 * r)
    L, D, Wb = SP["bill"]
    bb = rp.bill_base()
    # The face's feathers meeting the bill's base all round it.
    sdf.add(T([rp.add(bb, (0, SP["cere"] + 0.7 * D, 0.0)), rp.add(bb, (0, SP["cere"], -0.08 * D)), rp.add(bb, (0, -0.08 * L, -0.10 * D))],
              [(0.66 * Wb, 0.62 * D), (0.52 * Wb, 0.52 * D), (0.43 * Wb, 0.45 * D)], side=(1, 0, 0)), k=0.5 * D)
    for side in (1.0, -1.0):
        sdf.add(rp.wing_slab(side), k=0.25 * hw)
    sdf.add(E(rp.add(tr, (0, 0.22 * SP["tail"], 0.18 * hh)), (0.34 * hw, 0.26 * SP["tail"], 0.10 * hh)), k=0.4 * hw)
    sdf.add(E(rp.add(tr, (0, 0.16 * SP["tail"], -0.22 * hh)), (0.36 * hw, 0.26 * SP["tail"], 0.12 * hh)), k=0.4 * hw)
    # The tibiae's feathers: from the hip down and back to just over the
    # heel, tapering.
    lr = SP["leg_r"]
    for side in (1.0, -1.0):
        hip, knee, heel, foot, toe = rp.leg_joints(side)
        sdf.add(T([hip, knee, rp.lerp(knee, heel, 0.55), rp.lerp(knee, heel, 0.90)],
                  [(0.42 * hw, 0.42 * hw), (0.36 * hw, 0.36 * hw), (0.20 * hw, 0.20 * hw), (2.2 * lr, 2.2 * lr)]), k=0.35 * hw)
    return sdf.to_mesh(f"{SP['mesh']}Sculpt")


def toe_paths(side=1.0):
    """The four toes of one foot (``leg_joints``' foot): the middle, inner
    and outer toes forward and splayed along the ground, the hallux back;
    each a polyline from the foot to the claw's tip, and its radii."""
    hip, knee, heel, foot, toe = rp.leg_joints(side)
    to, lr = SP["leg"][2], SP["leg_r"]
    out = []
    for spread, ln in ((0.0, 1.0), (0.42, 0.78), (-0.40, 0.74), (None, 0.62)):
        if spread is None:
            d = rp.norm((0.0, 1.0, -0.05))
        else:
            d = rp.norm((side * math.sin(spread), -math.cos(spread), -0.02))
        a = rp.add(foot, rp.mul(d, 0.40 * to * ln))
        b = rp.add(foot, rp.mul(d, 0.78 * to * ln))
        # The claw: curving down to its point.
        c = rp.add(b, rp.add(rp.mul(d, 0.16 * to * ln), (0, 0, -0.04 * to)))
        tip = rp.add(c, rp.add(rp.mul(d, 0.07 * to * ln), (0, 0, -0.08 * to)))
        out.append(([foot, a, b, c, tip], [(0.62 * lr, 0.58 * lr), (0.55 * lr, 0.50 * lr), (0.48 * lr, 0.44 * lr),
                                           (0.30 * lr, 0.30 * lr), (0.06 * lr, 0.06 * lr)]))
    return out


def leg_sculpt():
    """The bare tarsi and toes, finer than the body: the tarsus a slightly
    flattened tube thickening at the heel, the toes with their pads and
    curved claws."""
    lr = SP["leg_r"]
    T, RC = rig.sdf_tube, rig.sdf_round_cone
    joints = [rp.leg_joints(s) for s in (1.0, -1.0)]
    pts = [p for j in joints for p in j[2:]] + [p for s in (1.0, -1.0) for path, _ in toe_paths(s) for p in path]
    lo = tuple(min(p[i] for p in pts) - 4 * lr for i in range(3))
    hi = tuple(max(p[i] for p in pts) + 4 * lr for i in range(3))
    sdf = rig.Sdf(lo, hi, lr / 4.5)
    for side, (hip, knee, heel, foot, toe) in zip((1.0, -1.0), joints):
        sdf.add(RC(rp.lerp(knee, heel, 0.85), heel, 1.6 * lr, 1.25 * lr), k=0.4 * lr)
        sdf.add(T([heel, rp.lerp(heel, foot, 0.5), foot], [(1.15 * lr, 1.05 * lr), (0.92 * lr, 0.98 * lr), (0.90 * lr, 0.80 * lr)], side=(1, 0, 0)), k=0.4 * lr)
        for path, radii in toe_paths(side):
            sdf.add(T(path, radii, samples=8), k=0.3 * lr)
    return sdf.to_mesh(f"{SP['mesh']}Legs")


def bill_sculpt():
    """The conical seed-eater's bill: deep and wide at the base, tapering
    straight to a sharp point, the culmen gently curved, the lower
    mandible's base as deep as the upper's, the gape's line bending down
    at the base; small nostrils under the forehead's feathers."""
    L, D, Wb = SP["bill"]
    bb = rp.bill_base()
    T, E = rig.sdf_tube, rig.sdf_ellipsoid
    voxel = D / 30.0
    lo = (-Wb, bb[1] - 1.15 * L, bb[2] - 0.9 * D)
    hi = (Wb, bb[1] + 0.8 * D, bb[2] + 0.9 * D)
    sdf = rig.Sdf(lo, hi, voxel)
    p = lambda y, z: rp.add(bb, (0.0, y, z))
    g = -0.12 * D  # the gape's line at the base
    up = [p(0.10 * D, 0.12 * D), p(-0.05 * L, 0.10 * D), p(-0.35 * L, 0.04 * D), p(-0.65 * L, -0.02 * D), p(-0.90 * L, -0.07 * D), p(-1.00 * L, -0.10 * D)]
    # The upper mandible a rounded triangle in section, its culmen a ridge
    # (v1 review: a round section read as a dark disc from the front): a
    # wide, shallow lower part and a narrow ridge over it.
    sdf.add(T([rp.add(q, (0, 0, -0.06 * D)) for q in up], [(0.42 * Wb, 0.19 * D), (0.40 * Wb, 0.18 * D), (0.31 * Wb, 0.13 * D), (0.19 * Wb, 0.085 * D),
                                                       (0.08 * Wb, 0.04 * D), (0.02 * Wb, 0.012 * D)], side=(1, 0, 0), samples=10), k=0.04 * D)
    sdf.add(T([rp.add(q, (0, 0, 0.06 * D)) for q in up], [(0.20 * Wb, 0.20 * D), (0.19 * Wb, 0.19 * D), (0.15 * Wb, 0.14 * D), (0.10 * Wb, 0.09 * D),
                                                      (0.05 * Wb, 0.04 * D), (0.015 * Wb, 0.012 * D)], side=(1, 0, 0), samples=10), k=0.06 * D)
    low = [p(0.10 * D, g - 0.20 * D), p(-0.05 * L, g - 0.18 * D), p(-0.35 * L, g - 0.11 * D), p(-0.65 * L, g - 0.05 * D), p(-0.92 * L, -0.08 * D)]
    sdf.add(T(low, [(0.40 * Wb, 0.21 * D), (0.38 * Wb, 0.20 * D), (0.29 * Wb, 0.14 * D), (0.17 * Wb, 0.085 * D), (0.04 * Wb, 0.03 * D)],
              side=(1, 0, 0), samples=10), k=0.04 * D)
    for sx in (1.0, -1.0):
        sdf.carve(E(rp.add(bb, (sx * 0.36 * Wb, -0.05 * L, 0.22 * D)), (0.06 * D, 0.10 * L, 0.05 * D)), k=0.02 * D)
    return sdf.to_mesh(f"{SP['mesh']}Bill")


# ---------------------------------------------------------------------------
# Build.

BAKE_REACH = 0.0009  # m: the bake's cage over the skin (``build``)
VOXEL = 0.00042  # m: the dense sculpt's voxel at the gorrión's size, scaled with the bird
BUDGET = {"gorrion": 5600, "chingolo": 5600}  # the skin's triangles (the legs' and the bill's apart)


def fix_toe_weights(obj):
    """The bare toes rigid on ``Toes.*`` (``raptor.body_weights`` takes the
    toes behind the foot only, as a raptor's tucked foot's): every vertex
    near a toe (``toe_paths``) under the foot's joint; the tarsus stays on
    ``Tarsus.*``."""
    np = rp._np()
    V = np.array([v.co[:] for v in obj.data.vertices])
    lr = SP["leg_r"]
    for side in (1.0, -1.0):
        sx = side_name(side)
        foot = rp.leg_joints(side)[3]
        dist = np.full(len(V), 1e9)
        for path, _ in toe_paths(side):
            for a_, b_ in zip(path, path[1:]):
                a_, b_ = np.asarray(a_), np.asarray(b_)
                ab = b_ - a_
                t = np.clip(((V - a_) @ ab) / max(ab.dot(ab), 1e-12), 0, 1)
                dist = np.minimum(dist, np.linalg.norm(V - (a_ + t[:, None] * ab), axis=1))
        near = (dist < 2.2 * lr) & (V[:, 2] < foot[2] + 0.6 * lr) & (V[:, 0] * side > 0)
        idx = [int(i) for i in np.nonzero(near)[0]]
        for vg in obj.vertex_groups:
            vg.remove(idx)
        vg = obj.vertex_groups.get(f"Toes.{sx}") or obj.vertex_groups.new(name=f"Toes.{sx}")
        vg.add(idx, 1.0, "REPLACE")


def build(draft=False, textures=None, scans=None, faces=None):
    """The bird (``raptor.build``'s steps with the sparrow's sculpt, legs,
    bill and atlas): ``scans`` (a folder holding ``SCANS``' sheets) and
    ``faces`` (one holding ``FACE_PHOTOS``' files) bring the real pixels
    in."""
    np = rp._np()
    rig.FPS = FPS
    rig.reset()
    voxel = VOXEL * SP["wing"][1] / SPECIES["gorrion"]["wing"][1] * (1.8 if draft else 1.0)
    dense = sculpt(voxel)
    bill = bill_sculpt()
    legs = leg_sculpt()
    body = rig.decimated_copy(dense, SP["mesh"], budget=BUDGET[NAME])
    rp.join_plain(body, rig.decimated_copy(bill, "BillGame", budget=500))
    rp.join_plain(body, rig.decimated_copy(legs, "LegsGame", budget=1400))
    rp.join_plain(dense, bill)
    rp.join_plain(dense, legs)
    body.data.materials.clear()
    body.data.materials.append(rig.plain_material("Main", (0.5, 0.45, 0.42)))
    rig.unwrap(body, margin=0.003)
    size = 1024 if draft else 2048
    # ``raptor.bake_plumage``'s cage reaches 12 mm at a chimango's size
    # (3 mm here): over the bill's base it caught the bill's black on the
    # face's feathers. The skin is within half a millimetre of the sculpt.
    keep = rig.bake_to
    rig.bake_to = lambda target, image, kind, source=None, extrusion=0.02, **k: keep(target, image, kind, source=source, extrusion=min(extrusion, BAKE_REACH), **k)
    face = iris = None
    if faces:
        face = rp.load_rgb(os.path.join(faces, FACE_PHOTOS[NAME]["file"]))
        iris = rp.iris_colour(face)
        FACE_BACKGROUND[NAME] = face_background(face, FACE_PHOTOS[NAME]["key"])
    try:
        maps = rp.bake_plumage(body, dense, size, face)
    finally:
        rig.bake_to = keep
    bpy.data.objects.remove(dense)
    sheets = None
    if scans:
        sheets = {k: rp.load_rgb(os.path.join(scans, spec["file"])) for k, spec in SCANS[NAME]["sheets"].items()}
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
    arm = rig.build_armature("RaptorArmature", rp.bone_table())
    rp.body_weights(body)
    fix_toe_weights(body)
    cards = rp.feather_cards(f"{SP['mesh']}Feathers")
    cards.data.materials.append(rig.plain_material("Feathers", (0.4, 0.36, 0.32)))
    eye_objs = rp.eyes(iris)
    for o in [cards] + eye_objs:
        rig.join(body, o)
    body.parent = arm
    body.modifiers.new("Armature", "ARMATURE").object = arm
    print(f"sparrow: {NAME}: {rig.triangles(body)} triangles, span {2 * rp.half_span():.3f} m, length {rp.total_length():.3f} m, "
          f"wing {chord():.3f} m, {wingbeat():.1f} wingbeats a second")
    return body, arm, maps, atlas


# ---------------------------------------------------------------------------
# Clips. Pure-python timing and joint angles first (tested without
# Blender), then the poses (``rig.Pose``, scale keys for the hidden wings).

# Wingbeat: Pennycuick's allometry (``pigeon.allometric_wingbeat``, J. Exp.
# Biol. 199, 1996), fitted across many species including passerines; no
# measured wingbeat of either species was found, so it is used as it
# stands (open: the pigeons' measured value showed it high for them).
FLAP_BEATS = 3
DOWNSTROKE = 0.55
BOUND_BEATS = 4  # ``FlapBound``: wingbeats a burst
BOUND_SECONDS = 0.12  # the bound with the wings folded
SWAP_SECONDS = 2.0 / 60  # the flying and the folded wing swapped over two frames
SWAP_PHASE = 0.80  # the upstroke's point (of a beat) where the wing folds into the bound and leaves it


def wingbeat():
    """The model's wingbeat frequency, Hz (Pennycuick's allometry)."""
    return pg.allometric_wingbeat(SP["mass"], 2 * rp.half_span(), rp.wing_area())


def frames(seconds):
    return round(seconds * FPS) / FPS


def flap_seconds():
    return round(FLAP_BEATS * FPS / wingbeat()) / FPS


def beat_seconds():
    return flap_seconds() / FLAP_BEATS


def flap_angles(t):
    """``pigeon.flap_angles`` for a passerine: a wide stroke, the upstroke's
    wing strongly flexed close to the body (the hand swept back, the
    primaries parted)."""
    t = t % 1.0
    up, down = SP["stroke"]
    if t < DOWNSTROKE:
        u = t / DOWNSTROKE
        elev = up + (down - up) * (0.5 - 0.5 * math.cos(math.pi * u))
        flex, twist, spread, sweep = 0.0, 8.0 * math.sin(math.pi * u), 1.0, 8.0 * math.sin(math.pi * u)
    else:
        u = (t - DOWNSTROKE) / (1 - DOWNSTROKE)
        elev = down + (up - down) * (0.5 - 0.5 * math.cos(math.pi * u))
        flex = math.sin(math.pi * u)
        twist, spread, sweep = -14.0 * flex, 1.0 + 0.5 * flex, -12.0 * flex
    return dict(elev=elev, elbow=55.0 * flex, wrist=85.0 * flex, twist=twist, spread=spread, hand=-20.0 * flex, sweep=sweep)


def flap_bound_seconds():
    return frames(BOUND_BEATS * beat_seconds() + BOUND_SECONDS)


def flap_bound_state(t):
    """``FlapBound`` at phase ``t``: the wingbeat's phase, the folded wing's
    share (0 the flying wing shows, 1 the folded) and the time into the
    bound (None while flapping). The burst runs from ``SWAP_PHASE`` of a
    beat through ``BOUND_BEATS`` beats back to it; the wings swap over
    ``SWAP_SECONDS`` at both ends of the bound."""
    total = flap_bound_seconds()
    sec = t * total
    flapping = total - BOUND_SECONDS
    if sec < flapping:
        phase = SWAP_PHASE + sec / beat_seconds()
        g = max(rp.smooth(1.0 - sec / SWAP_SECONDS), rp.smooth((sec - flapping + SWAP_SECONDS) / SWAP_SECONDS))
        return phase % 1.0, g, None
    return SWAP_PHASE, 1.0, sec - flapping


# The ground: standing, the body raised ``STAND_PITCH`` degrees nose up, the
# legs a little bent.
STAND_PITCH = 18.0
STAND_BEND = 0.86  # the hip-to-foot reach standing over the rest pose's
HOP_SECONDS = 0.32
HOP_HEIGHT = 0.70  # of the tarsus: the hop's apex over standing
HOP_LENGTH = 2.6  # body lengths (torso half lengths) the game moves the bird a hop
PECK_SECONDS = 1.4
IDLE_SECONDS = 3.0


def stand_height():
    """The body's centre over the ground standing (``Idle``): the body
    pitched ``STAND_PITCH`` nose up about its centre, each foot under it at
    its rest place along the body, the hip-to-foot reach ``STAND_BEND`` of
    the rest pose's."""
    hip, knee, heel, foot, toe = rp.leg_joints()
    a = math.radians(STAND_PITCH)
    hy = hip[1] * math.cos(a) + hip[2] * math.sin(a)
    hz = hip[2] * math.cos(a) - hip[1] * math.sin(a)
    reach = rp.length(rp.sub(foot, hip)) * STAND_BEND
    dy = foot[1] - hy
    return math.sqrt(max(reach * reach - dy * dy, 0.0)) - hz + foot_lift()


def foot_lift():
    """The foot's joint over the ground (the toes' thickness under it)."""
    return 0.9 * SP["leg_r"]


def body_point(p, pitch, lift=0.0, pivot=(0.0, 0.0, 0.0)):
    """The rest point ``p`` with the body turned ``pitch`` degrees nose up
    about ``pivot`` (``raptor.pose_body``'s turn) and lifted ``lift``."""
    a = math.radians(pitch)
    q = rp.sub(p, pivot)
    return rp.add(pivot, (q[0], q[1] * math.cos(a) + q[2] * math.sin(a), q[2] * math.cos(a) - q[1] * math.sin(a) + lift))


def ground_point(p, pitch=0.0, lift=0.0):
    """The rest point ``p`` on the ground (``standing``): the body pitched
    ``STAND_PITCH`` about its centre, its centre ``stand_height()`` +
    ``lift`` up, then tipped ``pitch`` more about the hips (a pecking bird
    pivots on its legs)."""
    h = stand_height() + lift
    hip = body_point(rp.leg_joints()[0], STAND_PITCH, h)
    return body_point(body_point(p, STAND_PITCH, h), pitch, 0.0, hip)


def heel_ik(knee, foot, back, shin, tarsus):
    """The heel of a leg whose shin (``shin`` long) hangs from ``knee`` and
    whose tarsus (``tarsus`` long) reaches for ``foot``, bent toward
    ``back`` (a bird's heel points back); and the foot it reaches (``foot``
    within the leg's reach)."""
    v = rp.sub(foot, knee)
    d = max(abs(shin - tarsus) + 1e-5, min(rp.length(v), shin + tarsus - 1e-5))
    vn = rp.norm(v)
    a = math.acos(max(-1.0, min(1.0, (shin * shin + d * d - tarsus * tarsus) / (2 * shin * d))))
    perp = rp.sub(back, rp.mul(vn, sum(b * c for b, c in zip(back, vn))))
    perp = rp.norm(perp) if rp.length(perp) > 1e-9 else (0.0, 0.0, 1.0)
    heel = rp.add(knee, rp.add(rp.mul(vn, math.cos(a) * shin), rp.mul(perp, math.sin(a) * shin)))
    return heel, rp.add(knee, rp.mul(vn, d))


def ground_leg(side, pitch=0.0, lift=0.0, feet=0.0):
    """One leg on the ground (``standing``), armature space: the hip
    carried by the body (``ground_point``), the femur held as standing
    whatever the body's tip (v1 review: carried with a pecking body it swung
    the knee forward and the bare shin showed under the belly in a Z), the
    foot at its rest place ``feet`` over the ground, the heel by ``heel_ik``.
    Returns hip, knee, heel and foot."""
    hip, knee, heel, foot, toe = rp.leg_joints(side)
    H = ground_point(hip, pitch, lift)
    K = rp.add(H, rp.sub(body_point(knee, STAND_PITCH), body_point(hip, STAND_PITCH)))
    back = body_point((0.0, 1.0, 0.0), STAND_PITCH)
    He, F = heel_ik(K, (foot[0], foot[1], feet + foot_lift()), back, rp.length(rp.sub(heel, knee)), rp.length(rp.sub(foot, heel)))
    return H, K, He, F


def hop_heights(t):
    """``Hop`` at phase ``t``: the body's lift over standing and the feet's
    height over the ground (metres): a crouch, the push, the flight's arc,
    the landing's give, back to standing."""
    ta = SP["leg"][1]
    crouch, apex = 0.28 * ta, HOP_HEIGHT * ta
    k = [0.0, 0.26, 0.40, 0.70, 0.84, 1.0]  # crouch, push, the arc's end (the feet down), the give, standing
    if t < k[1]:
        return -crouch * rp.smooth(t / k[1]), 0.0
    if t < k[2]:
        u = (t - k[1]) / (k[2] - k[1])
        body = -crouch + (crouch + 0.55 * apex) * math.sin(0.5 * math.pi * u)
        return body, max(0.0, body - 0.35 * apex)
    if t < k[3]:
        u = (t - k[2]) / (k[3] - k[2])
        body = 0.55 * apex + 0.45 * apex * math.sin(math.pi * u) * 1.0 - 0.55 * apex * u * u
        return body, max(0.0, body - 0.25 * apex * (1 - u))
    if t < k[4]:
        u = (t - k[3]) / (k[4] - k[3])
        return -0.22 * crouch * math.sin(math.pi * u * 0.5) * 2 * (1 - 0.5 * u), 0.0
    u = (t - k[4]) / (k[5] - k[4])
    return -0.22 * crouch * (1 - rp.smooth(u)), 0.0


def hop_airborne():
    """The part of ``Hop`` (phases) with the feet off the ground."""
    n = 600
    up = [k / n for k in range(n) if hop_heights(k / n)[1] > 1e-9]
    return (min(up), max(up)) if up else (0.0, 0.0)


def peck_angles(t):
    """``Peck`` at phase ``t``: the body's pitch (degrees over standing),
    the neck's and the head's bend down, and the head's small jab."""
    down = rp.smooth(t / 0.22) * (1 - rp.smooth((t - 0.72) / 0.22))
    jab = 0.0
    for c in (0.36, 0.55):
        jab += math.exp(-((t - c) / 0.035) ** 2)
    look = 8.0 * math.sin(2 * math.pi * t) * (1 - down)
    return dict(pitch=-42.0 * down - 4.0 * jab, neck=-22.0 * down - 6.0 * jab, head=-12.0 * down - 10.0 * jab, yaw=look)


def idle_angles(t):
    """``Idle`` at phase ``t``: the head's turns (held, quick between), a
    tail flick, the breathing."""
    keys = [(0.0, 0.0), (0.12, 0.0), (0.16, 34.0), (0.40, 34.0), (0.44, -12.0), (0.62, -12.0), (0.66, -38.0), (0.86, -38.0), (0.92, 0.0), (1.0, 0.0)]
    for (t0, a0), (t1, a1) in zip(keys, keys[1:]):
        if t0 <= t <= t1:
            yaw = a0 + (a1 - a0) * rp.smooth((t - t0) / (t1 - t0))
            break
    tilt = 10.0 * math.sin(2 * math.pi * t * 2 + 0.4) * math.sin(math.pi * t)
    flick = sum(math.exp(-((t - c) / 0.025) ** 2) for c in (0.30, 0.78))
    return dict(yaw=yaw, roll=tilt, tail=14.0 * flick, breath=math.sin(2 * math.pi * t * 6))


CLIP_ORDER = ("Flap", "FlapBound", "Hop", "Peck", "Idle")


if bpy is not None:

    class Pose(rig.Pose):
        """``rig.Pose`` that also keys the wings' bones' scale (``hide``)."""

        SCALED = ("Shoulder.L", "Shoulder.R", "Fold.L", "Fold.R")

        def hide(self, bone, k):
            """Scales ``bone`` (and what hangs from it) by ``k`` about its
            head."""
            m = self.posed(bone)
            at = m.translation.copy()
            S = Matrix.Translation(at) @ Matrix.Diagonal((k, k, k, 1.0)) @ Matrix.Translation(-at)
            names = [bone] + [b.name for b in self.arm.data.bones[bone].children_recursive]
            for b in names:
                if b in self.world or b == bone:
                    self.world[b] = S @ self.posed(b)

        def apply(self, frame, root):
            super().apply(frame, root)
            bones = self.arm.data.bones
            for name in self.SCALED:
                b = bones[name]
                parent = b.parent
                world = self.posed(name)
                pw = self.posed(parent.name)
                rest_rel = parent.matrix_local.inverted() @ b.matrix_local
                basis = rest_rel.inverted() @ pw.inverted() @ world
                pb = self.arm.pose.bones[name]
                pb.scale = basis.to_scale()
                pb.keyframe_insert("scale", frame=frame)


def body_frame(pose):
    """The posed body's transform of rest points."""
    return pose.posed("Body") @ pose.rest("Body").inverted()


def plant_leg(pose, side, joints, toes):
    """Places the leg on ``side`` on its ``joints`` (hip, knee, heel, foot;
    armature space), the toes along ``toes``."""
    sx = side_name(side)
    H, K, He, F = (Vector(j) for j in joints)
    pose.place(f"Thigh.{sx}", H, K - H)
    pose.place(f"Shin.{sx}", K, He - K)
    pose.place(f"Tarsus.{sx}", He, F - He)
    pose.place(f"Toes.{sx}", pose.tail(f"Tarsus.{sx}"), Vector(toes))


TUCK = (0.30, 0.42, -0.78)  # the tucked foot in flight, over the torso's half sizes (side +1)


def tuck_legs(pose):
    """The legs in flight: the feet drawn up under the belly's feathers
    behind the hip (``TUCK``), the toes curled back; the femur carried by
    the body."""
    hw, hl, hh = SP["torso"]
    M = body_frame(pose)
    back = tuple(M.to_3x3() @ Vector((0, 1, 0)))
    for side in (1.0, -1.0):
        hip, knee, heel, foot, toe = rp.leg_joints(side)
        H, K = tuple(M @ Vector(hip)), tuple(M @ Vector(knee))
        want = tuple(M @ Vector((side * TUCK[0] * hw, TUCK[1] * hl, TUCK[2] * hh)))
        He, F = heel_ik(K, want, back, rp.length(rp.sub(heel, knee)), rp.length(rp.sub(foot, heel)))
        plant_leg(pose, side, (H, K, He, F), M.to_3x3() @ Vector((0, 0.6, -0.8)))


def pose_wings(pose, a):
    for side in (1.0, -1.0):
        rp.pose_wing(pose, side, elev=a["elev"], sweep=a["sweep"], elbow=a["elbow"], wrist=a["wrist"], twist=a["twist"],
                     hand=a["hand"], spread=a["spread"])


def show_wings(pose, folded):
    """``folded`` (0..1): the flying wing's bones at full size at 0, the
    folded wing's at 1; between, both partly (``FlapBound``'s swap)."""
    for side in "LR":
        pose.hide(f"Shoulder.{side}", max(HIDDEN, 1.0 - folded))
        pose.hide(f"Fold.{side}", max(HIDDEN, folded))


def flying(pose, a, beat, folded, bounce=0.0):
    s = rp.bird_scale()
    lift = 0.004 * s * math.sin(2 * math.pi * (beat - 0.12)) * (1 - folded) + bounce
    pitch = 3.0 * math.sin(2 * math.pi * (beat + 0.1)) * (1 - folded)
    rp.pose_body(pose, pitch=pitch, lift=lift)
    pose_wings(pose, a)
    rp.pose_tail(pose, spread=6.0 * (1 - folded) + 2.0 * math.sin(2 * math.pi * beat), pitch=-2.0 - 2.0 * math.sin(2 * math.pi * beat))
    rp.pose_head(pose, pitch=-2.0 - pitch)
    tuck_legs(pose)
    show_wings(pose, folded)
    return pose


def flap(arm, t):
    beat = (t * FLAP_BEATS) % 1.0
    return flying(Pose(arm), flap_angles(beat), beat, 0.0)


def flap_bound(arm, t):
    phase, g, bound = flap_bound_state(t)
    a = flap_angles(phase)
    if bound is not None:
        # The bound: the folded wing; the body coasts on a shallow arc.
        u = bound / BOUND_SECONDS
        bounce = -0.006 * rp.bird_scale() * math.sin(math.pi * u)
        return flying(Pose(arm), a, phase, 1.0, bounce)
    return flying(Pose(arm), a, phase, g)


def standing(pose, pitch=0.0, lift=0.0, feet=0.0):
    """The ground pose: the body pitched ``STAND_PITCH`` nose up, its centre
    ``stand_height()`` + ``lift`` over the ground (z = 0), tipped ``pitch``
    more about the hips (``ground_point``), the feet ``feet`` over the
    ground (``ground_leg``), the folded wing showing."""
    rp.pose_body(pose, pitch=STAND_PITCH, lift=stand_height() + lift)
    if pitch:
        rp.turn(pose, "Body", Vector((1, 0, 0)), -pitch, ground_point(rp.leg_joints()[0], 0.0, lift))
    for side in (1.0, -1.0):
        plant_leg(pose, side, ground_leg(side, pitch, lift, feet), (0.0, -1.0, 0.0))
    show_wings(pose, 1.0)
    return pose


def fold_tail(pose, raise_=0.0, spread=0.0):
    """The tail on the ground: closed, its outer feathers drooping either
    side of the central pair (a shallow inverted V in section), raised
    ``raise_`` degrees."""
    rp.pose_tail(pose, spread=13.0 - spread, pitch=raise_)
    for side, b in ((1.0, "Tail.L"), (-1.0, "Tail.R")):
        rp.turn(pose, b, rp.axis_of(pose, b, (0, 1, 0)), side * 14.0)


def hop(arm, t):
    body, feet = hop_heights(t)
    pose = Pose(arm)
    push = math.sin(math.pi * min(max((t - 0.20) / 0.30, 0.0), 1.0))
    standing(pose, pitch=-6.0 * push, lift=body, feet=feet)
    fold_tail(pose, raise_=6.0 * math.sin(math.pi * min(max((t - 0.68) / 0.25, 0.0), 1.0)))
    rp.pose_head(pose, pitch=6.0 * push)
    return pose


def pose_neck(pose, neck=0.0, head=0.0, yaw=0.0, roll=0.0):
    """Bends the neck (``Neck1``, ``Neck2``) and the head down (negative)
    or up; ``yaw`` and ``roll`` on the head as ``raptor.pose_head``."""
    x = rp.axis_of(pose, "Body", (1, 0, 0))
    rp.turn(pose, "Neck1", x, -0.5 * neck)
    rp.turn(pose, "Neck2", x, -0.5 * neck)
    rp.turn(pose, "Head", x, -head)
    rp.pose_head(pose, yaw=yaw, roll=roll)


def peck(arm, t):
    a = peck_angles(t)
    pose = Pose(arm)
    standing(pose, pitch=a["pitch"])
    pose_neck(pose, a["neck"], a["head"], a["yaw"])
    fold_tail(pose, raise_=-0.3 * a["pitch"])
    return pose


def idle(arm, t):
    a = idle_angles(t)
    pose = Pose(arm)
    standing(pose, lift=0.0002 * a["breath"] * rp.bird_scale() / 0.27)
    rp.pose_head(pose, yaw=a["yaw"], roll=a["roll"])
    fold_tail(pose, raise_=a["tail"], spread=3.0 * a["tail"] / 14.0)
    return pose


def clips(arm):
    return [
        rig.bake_clip(arm, "Flap", flap_seconds(), flap, "Body"),
        rig.bake_clip(arm, "FlapBound", flap_bound_seconds(), flap_bound, "Body"),
        rig.bake_clip(arm, "Hop", HOP_SECONDS, hop, "Body"),
        rig.bake_clip(arm, "Peck", PECK_SECONDS, peck, "Body"),
        rig.bake_clip(arm, "Idle", IDLE_SECONDS, idle, "Body"),
    ]


# ---------------------------------------------------------------------------
# Previews.


def shots_for(actions):
    """``(file, action, frame, eye, target, lens, ground)``: the flying bird
    from above, below, the front at three quarters and the side; the
    upstroke and the downstroke; the bird on the ground standing, mid hop
    and pecking, from the side and at three quarters; the head close up
    (standing)."""
    by = {a.name: a for a in actions}
    b = 2 * rp.half_span()
    d = 2.7 * b
    c = (0.0, 0.05 * b, 0.0)
    hc = rp.head_centre()
    hr = SP["head"][1]
    shots = []
    if "Flap" in by:
        n = round(beat_seconds() * FPS)
        f = by["Flap"]
        glide = round(0.25 * DOWNSTROKE * n)  # the wing spread out, on its way down
        shots += [
            ("top", f, glide, (0.0, 0.051 * b, d), c, 100, False),
            ("bottom", f, glide, (0.0, 0.051 * b, -d), c, 100, False),
            ("below34", f, glide, (0.45 * d, -0.55 * d, -0.70 * d), c, 100, False),
            ("front34", f, glide, (0.55 * d, -0.75 * d, 0.30 * d), c, 100, False),
            ("side", f, glide, (d, 0.05 * b, 0.05 * b), c, 100, False),
            ("flap_up", f, 0, (0.40 * d, -0.85 * d, 0.30 * d), c, 100, False),
            ("flap_down", f, round(DOWNSTROKE * n), (0.40 * d, -0.85 * d, 0.30 * d), c, 100, False),
            ("flap_mid", f, round((DOWNSTROKE + 1) / 2 * n), (0.40 * d, -0.85 * d, 0.30 * d), c, 100, False),
        ]
    if "FlapBound" in by:
        fb = by["FlapBound"]
        n = round(flap_bound_seconds() * FPS)
        shots.append(("bound", fb, n - round(0.5 * BOUND_SECONDS * FPS), (0.75 * d, -0.45 * d, 0.25 * d), c, 100, False))
    g = 2.0 * b
    h0 = stand_height()
    gc = (0.0, 0.0, 0.85 * h0)
    if "Idle" in by:
        shots += [
            ("stand_side", by["Idle"], 0, (g, 0.0, 0.8 * h0), gc, 100, True),
            ("stand_34", by["Idle"], 0, (0.65 * g, -0.75 * g, 0.9 * h0 + 0.15 * g), gc, 100, True),
            ("stand_rear34", by["Idle"], 0, (-0.6 * g, 0.75 * g, 0.9 * h0 + 0.25 * g), gc, 100, True),
            ("stand_top", by["Idle"], 0, (0.15 * g, 0.2 * g, 1.1 * g), gc, 100, True),
            ("wing_close", by["Idle"], 0, (0.45 * g, 0.05 * g, 1.05 * h0), (0.0, 0.25 * b * 0.0 + 0.004, 1.05 * h0), 100, True),
        ]
        a = math.radians(STAND_PITCH)
        hs = (0.0, hc[1] * math.cos(a) + hc[2] * math.sin(a), h0 + hc[2] * math.cos(a) - hc[1] * math.sin(a))
        shots += [
            ("head", by["Idle"], 0, rp.add(hs, (16 * hr, -13 * hr, 4 * hr)), rp.add(hs, (0, -0.6 * hr, 0)), 105, True),
            ("head_side", by["Idle"], 0, rp.add(hs, (21 * hr, -3.5 * hr, 1 * hr)), rp.add(hs, (0, -0.8 * hr, -0.1 * hr)), 105, True),
            ("head_front", by["Idle"], 0, rp.add(hs, (4 * hr, -22 * hr, 2 * hr)), rp.add(hs, (0, -0.6 * hr, -0.1 * hr)), 105, True),
        ]
    if "Hop" in by:
        n = round(HOP_SECONDS * FPS)
        lo, hi = hop_airborne()
        shots.append(("hop_air", by["Hop"], round(0.5 * (lo + hi) * n), (g, -0.1 * g, 0.9 * h0), (0.0, 0.0, 1.2 * h0), 100, True))
        shots.append(("hop_crouch", by["Hop"], round(0.24 * n), (g, -0.1 * g, 0.9 * h0), gc, 100, True))
    if "Peck" in by:
        n = round(PECK_SECONDS * FPS)
        shots.append(("peck", by["Peck"], round(0.36 * n), (0.8 * g, -0.45 * g, 0.9 * h0), (0.0, -0.15 * b, 0.6 * h0), 100, True))
    return shots


def render_shots(out_dir, arm, mesh, maps, atlas, shots):
    """``raptor.render_shots``; the ground shots over a ground plane of
    packed earth."""
    sky = [s[:6] for s in shots if not s[6]]
    ground = [s[:6] for s in shots if s[6]]
    if sky:
        rp.render_shots(out_dir, arm, mesh, maps, atlas, sky)
    if ground:
        bpy.ops.mesh.primitive_plane_add(size=2.0, location=(0, 0, 0))
        plane = bpy.context.active_object
        mat = bpy.data.materials.new("Ground")
        mat.use_nodes = True
        nt = mat.node_tree
        bsdf = nt.nodes["Principled BSDF"]
        noise = nt.nodes.new("ShaderNodeTexNoise")
        noise.inputs["Scale"].default_value = 900.0
        ramp = nt.nodes.new("ShaderNodeValToRGB")
        ramp.color_ramp.elements[0].color = (0.20, 0.16, 0.12, 1)
        ramp.color_ramp.elements[1].color = (0.42, 0.36, 0.28, 1)
        nt.links.new(noise.outputs["Fac"], ramp.inputs["Fac"])
        nt.links.new(ramp.outputs["Color"], bsdf.inputs["Base Color"])
        bsdf.inputs["Roughness"].default_value = 0.95
        plane.data.materials.append(mat)
        rp.render_shots(out_dir, arm, mesh, maps, atlas, ground)
        bpy.data.objects.remove(plane)


def parse(argv):
    import argparse

    parser = argparse.ArgumentParser(prog="sparrow.py")
    parser.add_argument("output")
    parser.add_argument("--species", default="gorrion", choices=sorted(SPECIES))
    parser.add_argument("--preview", default=None, metavar="DIR")
    parser.add_argument("--textures", default=None, metavar="DIR")
    parser.add_argument("--shots", default=None, metavar="REGEX")
    parser.add_argument("--scans", default=None, metavar="DIR", help="cut SCANS' Feather Atlas sheets from DIR into the feather atlas")
    parser.add_argument("--faces", default=None, metavar="DIR", help="project FACE_PHOTOS from DIR onto the head")
    parser.add_argument("--draft", action="store_true")
    parser.add_argument("--no-clips", action="store_true")
    return parser.parse_args(argv)


def main():
    argv = sys.argv[sys.argv.index("--") + 1:] if "--" in sys.argv else []
    args = parse(argv)
    configure(args.species)
    mesh, arm, maps, atlas = build(args.draft, args.textures, args.scans, args.faces)
    light = rp.light_mesh(mesh, arm)
    light.hide_render = True
    actions = [] if args.no_clips else clips(arm)
    if args.preview:
        shots = shots_for(actions)
        if args.shots:
            shots = [s for s in shots if re.search(args.shots, s[0])]
        render_shots(args.preview, arm, mesh, maps, atlas, shots)
        far = [s for s in shots_for(actions) if s[0] in ("below34", "stand_34")]
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


install()

if bpy is not None and __name__ == "__main__":
    main()
