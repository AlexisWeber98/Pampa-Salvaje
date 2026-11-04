"""Model, rig, paint and animate the pampa's two pigeons in Blender, by
script: the eared dove (torcaza, Zenaida auriculata), whose big flocks cross
the sky, and the picazuro pigeon (picazuró, Patagioenas picazuro).

Run it headless, once per species:

    blender -b --factory-startup --python tools/pigeon.py -- OUT.glb \
        --species torcaza|picazuro [--textures DIR] [--preview DIR] \
        [--shots REGEX] [--scans DIR] [--faces DIR] [--draft] [--no-clips]

Writes OUT.glb: one skinned mesh (``Torcaza`` or ``Picazuro``) with the
materials ``Main`` (the body, the head, the bill, the legs and the wing's
coverts: one sculpt baked into its colour, normal, roughness and occlusion
maps), ``Feathers`` (the flight feathers, the tail, the coverts and the
scapulars: alpha-tested two-sided cards on one atlas) and ``Eye_Iris`` /
``Eye_Black``; a light mesh (``Torcaza_LOD``, ``Picazuro_LOD``: the skin
decimated and the flight feathers in fewer rows) on the same maps; the
armature ``RaptorArmature`` (``tools/raptor.py``'s 23 bones, so one rig
drives every bird of the sky); and the clips ``Flap`` (three wingbeats at
the frequency Pennycuick's allometry gives for the model's mass, span and
wing area), ``Glide`` (wings held out in a shallow dihedral, small
balancing corrections) and ``FlapGlide`` (the flocks' flap-and-glide: three
wingbeats, the wings stopping half way down into a glide, then a quick
upstroke out of it). Life size in metres, Z up, flying toward -Y (glTF +Z),
the body centre at the origin.

The layout, the plumage engine, the face projection, the feather cards, the
skinning and the previews are ``tools/raptor.py``'s, imported: a pigeon is
registered there as one more species (``install``), and ``raptor``'s
functions that differ for a pigeon (the wing's primaries, the bill's tip,
the face's landmarks, where the eye looks) dispatch to this module's while
a pigeon is configured. This module adds what a pigeon has of its own: the
plump body, the small round head and the slender bill with its swollen
operculum (``sculpt``, ``bill_sculpt``), the pointed wing (``PRIMARY_LENGTHS``),
the plumage of each species (``_paint_torcaza``, ``_paint_picazuro``), the
feather atlas cut from the Feather Atlas scans (``SCANS``, ``paint_atlas``)
and the clips. Its generic parts (``on_blue``, ``crop``, ``scan_feathers``,
``grain_cell``, ``sculpt``, ``bill_sculpt``, ``flap_angles``, the clips) only
read ``SP`` and serve any small bird laid out the same way.

Real pixels: ``--scans DIR`` cuts the U.S. Fish and Wildlife Service Feather
Atlas scans (public domain) into the atlas: the Mourning Dove's (Zenaida
macroura, the torcaza's congener) primaries, secondaries with the black-
spotted tertials, and tail with its black spots and white tips, for the
torcaza; the Band-tailed Pigeon's (Patagioenas fasciata, the picazuró's
congener) primaries and secondaries, darkened to slate, and its tail's
grain under the picazuró's own blackish tail. The sheets on black are keyed
and laid on the atlas' blue (``on_blue``), so ``raptor``'s cutter takes
them. ``--faces DIR`` projects a side photograph of each species
(``FACE_PHOTOS``: CC BY / public domain) over its head, and takes its iris
colour. Without them the atlas and the head stay painted.

Torcaza (Argentine birds, Z. a. chrysauchenia): grey crown, pinkish buff
face with a black line behind the eye and a black spot on the ear coverts,
a golden-pink sheen on the neck's sides, vinaceous buff breast fading to a
buff belly, olive-brown back and coverts with black spots on the inner
coverts, the tertials and the scapulars, dark grey-brown primaries, grey
underwing, a graduated tail whose outer feathers are grey with a black
band and white tips, red legs (tucked into the belly's feathers in
flight), a slender, slightly decurved slate-grey bill with a paler swollen
operculum and a narrow pale blue-grey ring round the dark eye. The black
spots of the inner coverts are painted on cells of their own (``SPOTS``,
``spot``), so they show from above. Picazuró: vinaceous head, neck and breast, the
crown greyer; the hind neck and the neck's sides scaled black and white;
grey belly and vent; brownish grey mantle and coverts with pale fringes (``_fringed``),
the greater coverts' white outer webs a white band along the wing; slate-
grey rump; blackish slate flight feathers and tail; grey underwing; a red
ring round the orange iris; a grey bill with a whitish operculum; red legs.
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
import raptor as rp  # noqa: E402

if bpy is not None:
    import animal_rig as rig
    from mathutils import Vector

FPS = rp.FPS

# ---------------------------------------------------------------------------
# The species. Lengths in metres. Literature: Baptista, Trail & Horblit in
# del Hoyo et al., "Handbook of the Birds of the World" 4 (Columbidae), and
# Narosky & Yzurieta, "Aves de Argentina y Uruguay": eared dove 22-28 cm
# long (the southern birds the largest), wing 140-155 mm, 100-140 g;
# picazuro pigeon 34-36 cm, wing 205-230 mm, 300-400 g. The spans are not
# published; they follow from the wing's length and the body's width as in
# the Rock Pigeon (wing 210-240 mm, span 64-72 cm) and the Mourning Dove
# (wing 135-160 mm, span 37-45 cm). The proportions (eye, bill, head) are
# measured on the face photographs (``FACE_PHOTOS``).

SPECIES = {
    "torcaza": dict(
        mesh="Torcaza",
        length=(0.22, 0.28),
        span=(0.37, 0.43),
        chord=(0.140, 0.155),  # the folded wing: wrist to the longest primary's tip
        mass=0.125,
        torso=(0.025, 0.052, 0.023),
        neck=0.022,
        head=(0.0122, 0.0158, 0.0135),
        crown=0.0,
        # Bill (Uruguay photograph): from the operculum's front 0.95 of the
        # eye's distance to the bill's base, slender, 4 mm deep there (v1
        # review: 9 mm and 3.6 mm read as a short stick).
        bill=(0.0102, 0.0040, 0.0034),
        cere=0.0042,  # the operculum's length
        eye=0.0031,
        eye_at=(0.80, -0.18, 0.22),
        eye_lens=0.55,
        eye_open=(1.0, 0.95),
        face=0.0,
        ring=1.32,
        lores=False,
        soft_head=True,
        ragged=0.18,
        aspect=0.80,
        relief=0.35,
        calamus=False,
        tail=0.098,
        tail_w=0.021,
        tail_round=0.36,  # graduated: the outer pair about a third shorter
        wing=(0.029, 0.035, 0.031),
        prim=0.104,
        prim_w=0.0175,
        sec=0.072,
        sec_w=0.022,
        fingers=0,
        leg=(0.021, 0.019, 0.019),
        leg_r=0.0021,
        dihedral=3.0,
        crest=0,
        stroke=(64.0, -50.0),  # the arm's elevation at the top and the bottom of a wingbeat (degrees)
    ),
    "picazuro": dict(
        mesh="Picazuro",
        length=(0.34, 0.37),
        span=(0.60, 0.68),
        chord=(0.205, 0.230),
        mass=0.37,
        torso=(0.036, 0.082, 0.034),
        neck=0.036,
        head=(0.0165, 0.0210, 0.0182),
        crown=0.0,
        # Bill (Buenos Aires photograph): the tip 8.6 iris radii in front of
        # the eye, the operculum a third of the bill, the tip drooping.
        bill=(0.0135, 0.0058, 0.0050),
        cere=0.0072,
        eye=0.0044,
        eye_at=(0.78, -0.12, 0.30),
        eye_lens=0.55,
        eye_open=(1.0, 1.0),
        face=0.0,
        ring=1.62,
        lores=False,
        soft_head=True,
        ragged=0.18,
        aspect=0.80,
        relief=0.35,
        calamus=False,
        tail=0.138,
        tail_w=0.031,
        tail_round=0.08,
        wing=(0.045, 0.053, 0.046),
        prim=0.158,
        prim_w=0.026,
        sec=0.104,
        sec_w=0.032,
        fingers=0,
        leg=(0.031, 0.027, 0.027),
        leg_r=0.0031,
        dihedral=2.0,
        crest=0,
        stroke=(60.0, -48.0),
    ),
}

NAME = "torcaza"
SP = SPECIES[NAME]

# The pointed pigeon wing (the Mourning Dove's and Band-tailed Pigeon's
# scans): P9 the longest, P10 and P8 a little shorter, P1 about 0.6 of it;
# no emarginated fingers, the outer primaries swept back into a point.
PRIMARY_LENGTHS = (0.60, 0.63, 0.67, 0.72, 0.79, 0.86, 0.92, 0.975, 1.00, 0.92)
PRIMARY_ANGLES = (84.0, 79.5, 74.5, 69.0, 63.0, 56.5, 50.0, 43.5, 38.0, 34.0)


def install():
    """Registers the pigeons as ``raptor`` species and wraps the ``raptor``
    functions a pigeon changes so they dispatch here while one is
    configured (the raptors are untouched). Idempotent."""
    for name, sp in SPECIES.items():
        rp.SPECIES[name] = sp
        rp.SPECIES_PAINT[name] = PAINT[name]
        rp.FEATHER_PAINT[name] = FEATHER_PAINT[name]
        rp.FEATHER_SHAFT[name] = _shaft
        rp.FACE_PHOTOS[name] = FACE_PHOTOS[name]
    if getattr(rp, "_pigeon_installed", False):
        return
    for name, mine in (("remiges", remiges), ("bill_tip", bill_tip), ("face_landmarks", face_landmarks), ("eye_look", eye_look),
                       ("photo_weight", photo_weight), ("leg_joints", leg_joints), ("coverts", coverts),
                       ("face_mapping", face_mapping), ("covert_rows", covert_rows), ("atlas_cells", atlas_cells),
                       ("regions", regions)):
        orig = getattr(rp, name)
        ORIGINAL[name] = orig

        def dispatch(*a, _mine=mine, _orig=orig, **k):
            return _mine(*a, **k) if rp.NAME in SPECIES else _orig(*a, **k)

        dispatch.__name__, dispatch.__doc__ = name, mine.__doc__
        setattr(rp, name, dispatch)
    rp._pigeon_installed = True


ORIGINAL = {}


def configure(name):
    """Makes ``name`` the bird ``raptor``'s layout, plumage and atlas
    functions work on."""
    global NAME, SP
    install()
    rp.configure(name)
    NAME, SP = name, SPECIES[name]


# ---------------------------------------------------------------------------
# The layout that differs from the raptors' (``raptor``'s functions call
# these through ``install``'s dispatch).


def remiges(side=1.0, spread=1.0):
    """``raptor.remiges`` with the pigeon's primaries (``PRIMARY_LENGTHS``,
    ``PRIMARY_ANGLES``)."""
    keep = rp.PRIMARY_LENGTHS, rp.PRIMARY_ANGLES
    rp.PRIMARY_LENGTHS, rp.PRIMARY_ANGLES = PRIMARY_LENGTHS, PRIMARY_ANGLES
    try:
        return ORIGINAL["remiges"](side, spread)
    finally:
        rp.PRIMARY_LENGTHS, rp.PRIMARY_ANGLES = keep


def bill_tip():
    """The tip of the slender bill, its culmen curving down toward it, the
    upper mandible drooping a little over the lower."""
    L, D, _ = SP["bill"]
    return rp.add(rp.bill_base(), (0.0, -1.0 * L - 0.06 * D, -0.50 * D))


def face_landmarks():
    """The head's points matching ``FACE_PHOTOS``' ``points`` (the bird's
    right side): the eye's centre, the culmen's base (where the forehead's
    feathers meet the operculum), the nostril's slit at the operculum's
    front, the crown straight over the eye, the gape and the bill's
    tip."""
    L, D, Wb = SP["bill"]
    bb = rp.bill_base()
    e = rp.eye_centre(1.0)
    hc, (w, l, h) = rp.head_centre(), SP["head"]
    return dict(
        eye=e,
        culmen=rp.add(bb, (0.0, SP["cere"], 0.42 * D)),
        nostril=rp.add(bb, (0.40 * Wb, 0.02 * SP["cere"], 0.05 * D)),
        crown=(e[0] * 0.3, e[1], hc[2] + h * 0.98),
        gape=rp.add(bb, (0.46 * Wb, 0.55 * SP["cere"], -0.42 * D)),
        tip=bill_tip(),
    )


def face_mapping():
    """The side camera from the head to the photo, anchored on the eye
    (``raptor.face_mapping`` fits an affine map through every landmark: on
    a head turned a little toward the camera it stretched the eye into an
    oval). Here the eye's centre lands on the photo's, the visible eye's
    radius on the photo's ``iris`` radius (the scale), and the turn (and
    which way the bill points) is the one that brings the other landmarks'
    directions from the eye closest to theirs."""
    np = rp._np()
    photo = FACE_PHOTOS[NAME]
    marks = face_landmarks()
    e = np.asarray(marks["eye"])[1:]
    ex, ey = photo["points"]["eye"]
    k = photo["iris"] / SP["eye"]
    names = [n for n in sorted(photo["points"]) if n != "eye"]
    best = None
    for flip in (1.0, -1.0):
        u = np.array([((marks[n][1] - e[0]) * flip, -(marks[n][2] - e[1])) for n in names])
        v = np.array([(photo["points"][n][0] - ex, photo["points"][n][1] - ey) for n in names], float)
        ang = math.atan2(float((u[:, 0] * v[:, 1] - u[:, 1] * v[:, 0]).sum()), float((u * v).sum()))
        rot = np.array([[math.cos(ang), -math.sin(ang)], [math.sin(ang), math.cos(ang)]])
        err = float((((u @ rot.T) * k - v) ** 2).sum())
        if best is None or err < best[0]:
            best = (err, flip, rot)
    _, flip, rot = best

    def f(P):
        q = np.atleast_2d(np.asarray(P, float))[:, 1:] - e
        q = np.stack([q[:, 0] * flip, -q[:, 1]], axis=1)
        return (q @ rot.T) * k + np.array([ex, ey], float)

    f.scale = k
    return f


def photo_weight(P, N, f):
    """``raptor.photo_weight``, kept off the bill and the operculum where
    the photo's bill is foreshortened (``bill=False``)."""
    np = rp._np()
    w, uv = ORIGINAL["photo_weight"](P, N, f)
    if FACE_PHOTOS[NAME].get("bill", True):
        return w, uv
    bb = rp.bill_base()
    off = np.clip((np.asarray(P)[:, 1] - (bb[1] + 1.3 * SP["cere"])) / (0.6 * SP["cere"]), 0, 1)
    return w * off, uv


def leg_joints(side=1.0):
    """Hip, knee, heel, foot and the middle toe's tip, tucked as a pigeon
    flies: the legs drawn up against the belly, the feathered tibia back
    under it, the bare tarsus folded back and up into the belly's
    feathers, the toes clenched, only their tips reaching the vent's
    surface (v1 review: the red feet dangled under the bird)."""
    hw, hl, hh = SP["torso"]
    ti, ta, to = SP["leg"]
    hip = (side * 0.40 * hw, 0.05 * hl, -0.40 * hh)
    knee = rp.add(hip, (0.0, 0.20 * ti, -0.45 * ti))
    heel = rp.add(knee, rp.mul(rp.norm((0.0, 0.95, 0.30)), ti))
    heel = (side * 0.30 * hw, heel[1], heel[2])
    foot = rp.add(heel, rp.mul(rp.norm((0.0, 0.80, 0.60)), ta))
    toe = rp.add(foot, rp.mul(rp.norm((0.0, 0.55, -0.83)), 0.38 * to))
    return [hip, knee, heel, foot, toe]


LEG_MASK = 1.5  # the bare leg's paint: within this many leg radii of the tarsus' and the toes' axes


def regions(P, N):
    """``raptor.regions`` with the bare leg kept to the leg itself: the
    tucked tarsus and toes lie just under the belly's surface, where
    ``raptor``'s wider band (2.1 leg radii) painted the feathers over them
    red."""
    np = rp._np()
    R = ORIGINAL["regions"](P, N)
    P = np.asarray(P, float)
    dl = np.full(len(P), 1e9)
    for sgn in (1.0, -1.0):
        hip, knee, heel, foot, toe = leg_joints(sgn)
        for a_, b_ in ((heel, foot), (foot, toe)):
            a_, b_ = np.asarray(a_), np.asarray(b_)
            ab = b_ - a_
            t = np.clip(((P - a_) @ ab) / ab.dot(ab), 0, 1)
            dl = np.minimum(dl, np.linalg.norm(P - (a_ + t[:, None] * ab), axis=1))
    R["leg"] = R["leg"] & (dl < LEG_MASK * SP["leg_r"])
    R["claw"] = R["claw"] & R["leg"]
    return R


def coverts(feathers):
    """``raptor.coverts``, the greater coverts over the primaries (the
    primary coverts, ``U``) on cells of their own: a pigeon's are plain,
    unlike the inner wing's (the picazuró's white band)."""
    out = ORIGINAL["coverts"](feathers)
    for g, fd in zip(out, [fd for fd in feathers if fd["kind"] in "PS"]):
        if fd["kind"] == "P":
            g["kind"] = "U"
        elif (fd["kind"], fd["index"]) in SPOTS.get(NAME, {}).get("G", {}):
            g["index"] = SPOTS[NAME]["G"][(fd["kind"], fd["index"])]
    return out


# The torcaza's black spots (v1 review: missing from above): on the inner
# greater and median coverts, over the innermost secondaries and the
# tertials; their cards take cells of their own (index 5 and up), each
# with one black subterminal spot (``spot``). Keyed by the flight feather
# the covert lies over.
SPOTS = {
    "torcaza": {
        "G": {("S", 11): 5, ("S", 13): 6, ("S", 15): 7},
        "M": {("S", 12): 5, ("S", 14): 6, ("S", 13): 7},
    },
}


def covert_rows(feathers):
    """``raptor.covert_rows``, the torcaza's spotted median coverts
    (``SPOTS``) on their own cells."""
    out = ORIGINAL["covert_rows"](feathers)
    spots = SPOTS.get(NAME, {}).get("M", {})
    if not spots:
        return out
    # Two cards (``M`` then ``L``) over each secondary and the inner six
    # primaries, in order.
    under = [fd for fd in feathers if fd["kind"] == "S" or (fd["kind"] == "P" and fd["index"] <= 6)]
    for m, fd in zip([c for c in out if c["kind"] == "M"], under):
        if (fd["kind"], fd["index"]) in spots:
            m["index"] = spots[(fd["kind"], fd["index"])]
    return out


def atlas_cells():
    """``raptor.atlas_cells`` and the spotted coverts' own cells."""
    out = dict(ORIGINAL["atlas_cells"]())
    for kind, by in SPOTS.get(NAME, {}).items():
        for index in sorted(set(by.values())):
            for under in (False, True):
                out[(kind, index, under)] = len(out)
    return out


def spot(c, a, kind, index, L, W, fo):
    """A covert cell's colour with its black subterminal spot: a rounded,
    slightly irregular blot across the rachis, toward the tip of the
    feather's visible part, its size and place a little different on each
    cell (the photographs' spots are 5 to 8 mm across)."""
    np = rp._np()
    hpx, wpx = c.shape[:2]
    t = (np.arange(hpx) + 0.5)[:, None] / hpx
    s = (np.arange(wpx) + 0.5)[None, :] / wpx
    k = index - 5
    tc = (0.66 if kind == "G" else 0.68) + 0.05 * ((k * 0.37) % 1.0 - 0.5)
    r = (0.0028 + 0.0005 * k) * rp.bird_scale() / 0.5
    o = (s - fo - 0.06 * (k - 1)) * W
    ang = np.arctan2(o, (t - tc) * L)
    d = np.sqrt(((t - tc) * L) ** 2 + (o / 1.15) ** 2) / (r * (1.0 + 0.10 * np.sin(3 * ang + k) + 0.06 * np.sin(5 * ang + 2 * k)))
    blot = np.clip((1.0 - d) / 0.18, 0, 1)[..., None] * (a[..., None] > 0)
    return c * (1 - blot) + np.asarray((0.07, 0.06, 0.06), np.float32) * blot


def eye_look(sx):
    """A pigeon's eyes look out to the side (a narrow binocular field)."""
    return rp.norm((sx * 0.94, -0.30, 0.12))


def chord():
    """The folded wing's length (the field guides' "wing"): from the wrist
    to the longest primary's tip."""
    s, e, w, t = rp.wing_joints()
    return max(rp.length(rp.sub(rp.feather_tip(fd), w)) for fd in rp.remiges() if fd["kind"] == "P")


# ---------------------------------------------------------------------------
# The plumage, per texel (``raptor.plumage`` lays the feather tracts and
# calls these). Colours are sRGB, sampled from the reference photographs.


def _neck_coord(P):
    """Along the neck (0 at its base, 1 at the head's centre) and around it
    (0 on the nape, pi on the throat)."""
    np = rp._np()
    nb, hc = np.asarray(rp.neck_base()), np.asarray(rp.head_centre())
    ax = hc - nb
    s = (P - nb) @ ax / ax.dot(ax)
    centre = nb + s[:, None] * ax
    rel = P - centre
    up = np.asarray(rp.norm(rp.cross(tuple(ax), (1.0, 0.0, 0.0))))
    around = np.arctan2(np.abs(rel[:, 0]), rel @ up)
    return s, around


def _bare_parts(P, R, col, rough, ring_col, bill_col, operculum_col, leg_col, jitter):
    """The bare parts both pigeons share: the orbital ring (its lids' dark
    rims, fading into the feathers), the bill with its swollen operculum,
    the legs and the dark claws. Returns colour, roughness and the bare
    mask."""
    np = rp._np()
    n = len(P)
    y, z = P[:, 1], P[:, 2]
    e, r = rp.eye_centre(), SP["eye"]
    oy, oz = SP["eye_open"]
    el = np.sqrt(((y - e[1]) / oy) ** 2 + ((z - e[2]) / oz) ** 2) / r
    rcol = rp.mix(rp.rgb((0.10, 0.09, 0.09), n), rp.rgb(ring_col, n), np.clip((el - 1.05) / 0.10, 0, 1))
    rw = np.clip((SP["ring"] + 0.05 - el) / 0.16, 0, 1) * (np.abs(P[:, 0]) > 0.3 * SP["head"][0])
    rw = rw * rw * (3 - 2 * rw)
    col = rp.mix(col, rcol * (0.96 + 0.08 * jitter[:, None]), rw)
    bb, (L, D, Wb), cl = rp.bill_base(), SP["bill"], SP["cere"]
    # The operculum: the soft swelling over the nostrils, from the
    # forehead's feathers to the horn, its back edge a little ragged.
    ax = np.abs(P[:, 0])
    # Its swelling: soft edges, fading into the bill's horn forward and
    # into the forehead's feathers back and up.
    back = bb[1] + cl * (1.0 + 0.15 * np.clip(ax / Wb, 0, 1))
    op = np.clip((back - y) / (0.45 * cl), 0, 1) * np.clip((y - (bb[1] - 0.15 * L)) / (0.10 * L), 0, 1)
    # Round in section (v1 review: a box mask read as a square block on
    # the face from the front).
    sec = np.sqrt((ax / (1.0 * Wb)) ** 2 + ((z - (bb[2] + 0.05 * D)) / (0.85 * D)) ** 2)
    op = op * np.clip((1.0 - sec) / 0.30, 0, 1) * (R["eye_dist"] > 1.8)
    op = op * op * (3 - 2 * op)
    tipf = np.clip((bb[1] - y) / L, 0, 1)
    horn = rp.mix(rp.rgb(bill_col, n), rp.rgb(np.asarray(bill_col) * 0.75, n), np.clip((tipf - 0.6) / 0.4, 0, 1))
    col = np.where(R["bill"][:, None], horn, col)
    col = rp.mix(col, rp.rgb(operculum_col, n) * (0.95 + 0.10 * jitter[:, None]), op)
    op = op > 0.5
    bill = R["bill"] | op
    col = np.where(R["leg"][:, None], rp.rgb(leg_col, n), col)
    col = np.where(R["claw"][:, None], rp.rgb((0.16, 0.13, 0.12), n), col)
    ring = rw > 0.5
    rough = np.where(bill | R["claw"], 0.45, np.where(ring | R["leg"], 0.55, rough))
    return col, rough, ring | bill | R["leg"]


def _paint_torcaza(P, N, R, d, a, hv, jitter, ventral, dorsal, wing, cf):
    np = rp._np()
    n = len(P)
    y, z = P[:, 1], P[:, 2]
    hw, hl, hh = SP["torso"]
    s = rp.bird_scale()
    rough = np.full(n, 0.85)
    h2 = np.mod(hv * 7.31 + 0.13, 1.0)
    mottle = rp.vnoise(P, 1.0 / (0.025 * s), 41) - 0.5
    # Body: olive-brown above, vinaceous buff below fading to a buff belly;
    # the feathers' tips a little paler, each feather its own tone.
    back, breast, belly = (0.42, 0.37, 0.30), (0.64, 0.49, 0.41), (0.74, 0.63, 0.51)
    below = np.clip((R["ang"] - 1.2) / 0.9, 0, 1)
    col = rp.mix(rp.rgb(back, n), rp.rgb(breast, n), below)
    col = rp.mix(col, rp.rgb(belly, n), below * np.clip((y + 0.1 * hl) / (0.8 * hl), 0, 1))
    col = col * (0.95 + 0.08 * h2 + 0.05 * mottle)[:, None]
    col = col * (1.0 + 0.05 * np.clip(1 - d / 0.15, 0, 1))[:, None]
    # Rump and uppertail coverts greyer; undertail coverts buff with grey.
    rump = R["tailcov"] & R["up"] & ~wing
    col = np.where(rump[:, None], rp.rgb((0.45, 0.43, 0.40), n) * (0.95 + 0.08 * h2)[:, None], col)
    under = R["tailcov"] & ~R["up"] & ~wing
    col = np.where(under[:, None], rp.mix(rp.rgb((0.80, 0.72, 0.62), n), rp.rgb((0.62, 0.60, 0.58), n), 0.35 * np.clip(1 - d / 0.4, 0, 1)), col)
    # Neck: a golden-pink sheen on its sides.
    s_n, around = _neck_coord(P)
    sheen = np.clip(1 - np.abs(s_n - 0.35) / 0.35, 0, 1) * np.clip(1 - np.abs(around - 1.5) / 0.8, 0, 1)
    col = rp.mix(col, rp.rgb((0.74, 0.55, 0.44), n), 0.55 * sheen * (0.7 + 0.6 * np.clip(1 - d / 0.5, 0, 1)))
    # Head: grey crown into a pinkish buff face; the black line behind the
    # eye and the black spot on the ear coverts.
    e, r = rp.eye_centre(), SP["eye"]
    hc, (w, l, h) = rp.head_centre(), SP["head"]
    crown = np.clip((z - (e[2] + 0.6 * r)) / (1.2 * r), 0, 1) * np.clip((hc[1] + 0.8 * l - y) / (1.0 * l), 0, 1)
    hcol = rp.mix(rp.rgb((0.66, 0.53, 0.45), n), rp.rgb((0.55, 0.55, 0.58), n), crown)
    hcol = hcol * (0.97 + 0.05 * h2)[:, None]
    side = np.abs(P[:, 0]) > 0.40 * w
    along = (y - e[1] - 1.1 * r) / (2.6 * r)
    line = np.clip(1 - np.abs(z - (e[2] - 0.15 * r)) / (0.32 * r), 0, 1) * np.clip(along * 6 + 1, 0, 1) * np.clip((1 - along) * 4, 0, 1)
    spot = ((y - (e[1] + 2.6 * r)) / (1.1 * r)) ** 2 + ((z - (e[2] - 2.4 * r) - 0.30 * (y - e[1] - 2.6 * r)) / (0.32 * r)) ** 2
    marks = np.maximum(line, 0.8 * np.clip((1 - spot) / 0.4, 0, 1)) * side
    hcol = rp.mix(hcol, rp.rgb((0.12, 0.10, 0.11), n), 0.85 * marks)
    col = rp.mix(col, hcol, R["head_w"] * ~wing)
    # Wings: olive-brown coverts, the outer (primary) coverts and the
    # alula dark grey-brown; round black spots on some inner coverts; grey
    # below.
    along_w, behind, span = rp.wing_coords(P, 1.0)
    wup = rp.rgb((0.46, 0.41, 0.34), n) * (0.94 + 0.10 * h2 + 0.05 * mottle)[:, None]
    wup = rp.mix(wup, rp.rgb((0.36, 0.34, 0.32), n), np.clip((span - 0.62) / 0.12, 0, 1))
    wup = rp.mix(wup, rp.rgb((0.55, 0.50, 0.43), n), 0.35 * np.clip(1 - d / 0.12, 0, 1))
    spotted = (span < 0.45) & (cf > 0.30) & (hv < 0.33)
    blot = np.clip((1 - (((d - 0.35) / 0.26) ** 2 + (a / 0.30) ** 2)) / 0.25, 0, 1) * spotted
    wup = rp.mix(wup, rp.rgb((0.09, 0.08, 0.08), n), blot)
    wdn = rp.rgb((0.60, 0.61, 0.63), n) * (0.95 + 0.08 * h2)[:, None]
    col = np.where((wing & R["up"])[:, None], wup, col)
    col = np.where((wing & ~R["up"])[:, None], wdn, col)
    col, rough, bare = _bare_parts(P, R, col, rough, (0.62, 0.70, 0.72), (0.27, 0.28, 0.30), (0.56, 0.59, 0.62), (0.80, 0.34, 0.36), jitter)
    return col, None, rough, bare


def _paint_picazuro(P, N, R, d, a, hv, jitter, ventral, dorsal, wing, cf):
    np = rp._np()
    n = len(P)
    y, z = P[:, 1], P[:, 2]
    hw, hl, hh = SP["torso"]
    s = rp.bird_scale()
    rough = np.full(n, 0.85)
    h2 = np.mod(hv * 7.31 + 0.13, 1.0)
    mottle = rp.vnoise(P, 1.0 / (0.03 * s), 41) - 0.5
    # Body: brownish vinaceous grey above, vinaceous breast, grey belly.
    mantle, breast, belly = (0.42, 0.36, 0.37), (0.50, 0.38, 0.41), (0.50, 0.49, 0.53)
    below = np.clip((R["ang"] - 1.2) / 0.9, 0, 1)
    col = rp.mix(rp.rgb(mantle, n), rp.rgb(breast, n), below)
    col = rp.mix(col, rp.rgb(belly, n), below * np.clip((y + 0.05 * hl) / (0.7 * hl), 0, 1))
    col = col * (0.95 + 0.08 * h2 + 0.05 * mottle)[:, None]
    # Lower back, rump and uppertail coverts slate grey; vent grey.
    lower = R["up"] & ~wing & (y > 0.25 * hl)
    col = rp.mix(col, rp.rgb((0.40, 0.42, 0.48), n), lower * np.clip((y - 0.25 * hl) / (0.4 * hl), 0, 1))
    under = R["tailcov"] & ~R["up"] & ~wing
    col = np.where(under[:, None], rp.rgb((0.50, 0.51, 0.56), n) * (0.95 + 0.08 * h2)[:, None], col)
    # Head: vinaceous, the crown greyer.
    e, r = rp.eye_centre(), SP["eye"]
    hc, (w, l, h) = rp.head_centre(), SP["head"]
    crown = np.clip((z - (e[2] + 0.5 * r)) / (1.5 * r), 0, 1)
    hcol = rp.mix(rp.rgb((0.55, 0.42, 0.45), n), rp.rgb((0.56, 0.50, 0.55), n), crown)
    hcol = hcol * (0.97 + 0.05 * h2)[:, None]
    col = rp.mix(col, hcol, R["head_w"] * ~wing)
    # The neck's scaled patch: on the hind neck and the neck's sides, small
    # blackish feathers each with a white crescent at its tip (the
    # photographs' fine scales), finer than the body's tract; from just
    # behind the head (over its fade) down to the neck's base.
    s_n, around = _neck_coord(P)
    patch = np.clip((0.78 - s_n) / 0.10, 0, 1) * np.clip((s_n + 0.30) / 0.15, 0, 1) * np.clip((1.85 - around) / 0.25, 0, 1)
    rad = SP["neck"] * 0.8
    dn, an, hn = rp.feather_cells(around * rad, -s_n * SP["neck"] * 1.6, 0.0026 * s, salt=17, aspect=0.9)
    crescent = np.clip((0.16 - dn) / 0.05, 0, 1) * np.clip((0.40 - np.abs(an)) / 0.10, 0, 1) * (0.75 + 0.25 * hn)
    scale = rp.mix(rp.rgb((0.12, 0.11, 0.13), n), rp.rgb((0.88, 0.87, 0.88), n), crescent)
    col = rp.mix(col, scale, patch)
    # Wings: brownish grey coverts with pale fringes; the greater coverts'
    # white outer webs a white band along the inner wing; the outer
    # coverts slate; grey below.
    along_w, behind, span = rp.wing_coords(P, 1.0)
    wup = rp.rgb((0.40, 0.36, 0.35), n) * (0.94 + 0.10 * h2 + 0.05 * mottle)[:, None]
    wup = rp.mix(wup, rp.rgb((0.30, 0.31, 0.35), n), np.clip((span - 0.60) / 0.12, 0, 1))
    wup = rp.mix(wup, rp.rgb((0.52, 0.48, 0.47), n), 0.18 * np.clip(1 - d / 0.12, 0, 1) * (span < 0.62))
    band = np.clip((cf - 0.62) / 0.04, 0, 1) * np.clip((span - 0.08) / 0.05, 0, 1) * np.clip((0.60 - span) / 0.06, 0, 1)
    band = band * np.clip((-a + 0.05) / 0.10, 0, 1)
    wup = rp.mix(wup, rp.rgb((0.90, 0.90, 0.89), n), 0.9 * band)
    wdn = rp.rgb((0.52, 0.54, 0.58), n) * (0.95 + 0.08 * h2)[:, None]
    col = np.where((wing & R["up"])[:, None], wup, col)
    col = np.where((wing & ~R["up"])[:, None], wdn, col)
    col, rough, bare = _bare_parts(P, R, col, rough, (0.66, 0.16, 0.18), (0.50, 0.52, 0.56), (0.74, 0.74, 0.77), (0.80, 0.25, 0.30), jitter)
    return col, None, rough, bare


PAINT = {"torcaza": _paint_torcaza, "picazuro": _paint_picazuro}


# ---------------------------------------------------------------------------
# The feathers painted on their cards (where no scan fills them, or under a
# scan's grain): ``raptor.paint_feather`` calls these. Plain vanes, no bars.


def _feathers_torcaza(kind, index, under, t, e, o, tm, L):
    np = rp._np()
    one = np.ones(t.shape + (1,))
    if kind == "P":
        base = (0.30, 0.28, 0.26) if not under else (0.52, 0.53, 0.55)
        edge = np.clip((e - 0.85) / 0.12, 0, 1) * (o < 0) * (not under)
        col = np.asarray(base) * one + (np.asarray((0.62, 0.58, 0.52)) - np.asarray(base)) * edge[..., None]
        return col, col, 0 * t, 1.0, 0.0, None
    if kind == "S":
        base = (0.40, 0.37, 0.33) if not under else (0.58, 0.59, 0.61)
        col = np.asarray(base) * one
        return col, col, 0 * t, 1.0, 0.0, None
    if kind == "R":
        # Grey-brown; the outer feathers grey with a black band and a white
        # tip, the band reaching further in on the outer ones.
        base = np.asarray((0.45, 0.42, 0.38) if index == 1 else (0.52, 0.53, 0.55)) * one
        if index > 1:
            a0 = 0.70 - 0.03 * index
            band = _band(t, a0, 0.84)
            tipz = np.clip((t - 0.84) / 0.02, 0, 1)
            base = base * (1 - band[..., None]) + np.asarray((0.08, 0.08, 0.09)) * band[..., None]
            base = base * (1 - tipz[..., None]) + np.asarray((0.90, 0.88, 0.84)) * tipz[..., None]
        return base, base, 0 * t, 1.0, 0.0, None
    base = np.asarray((0.53, 0.48, 0.41)) * one
    base = base + (np.asarray((0.62, 0.57, 0.50)) - base) * np.clip((t - 0.85) / 0.10, 0, 1)[..., None]
    return base, base, 0 * t, 1.0, 0.0, None


def _feathers_picazuro(kind, index, under, t, e, o, tm, L):
    np = rp._np()
    one = np.ones(t.shape + (1,))
    if kind in "PS":
        base = np.asarray((0.21, 0.22, 0.25) if not under else (0.42, 0.44, 0.48)) * one
        return base, base, 0 * t, 1.0, 0.0, None
    if kind == "R":
        # Blackish slate, greyer toward the base (above), dark grey below.
        top = np.asarray((0.11, 0.11, 0.13) if not under else (0.22, 0.23, 0.26))
        root = np.asarray((0.26, 0.27, 0.31) if not under else (0.32, 0.33, 0.37))
        f = np.clip((t - 0.25) / 0.35, 0, 1)[..., None]
        base = root * (1 - f) + top * f
        return base, base, 0 * t, 1.0, 0.0, None
    if kind == "U":
        base = np.asarray((0.26, 0.27, 0.31)) * one
        return base, base, 0 * t, 1.0, 0.0, None
    if kind == "G":
        # Brownish grey, the outer web's edge white past the root: the
        # wing band.
        base = _fringed(np.asarray((0.40, 0.36, 0.35)) * one, t, e)
        white = (o < 0) * np.clip((t - 0.45) / 0.08, 0, 1) * np.clip((e - 0.45) / 0.15, 0, 1)
        base = base + (np.asarray((0.92, 0.92, 0.91)) - base) * white[..., None]
        return base, base, 0 * t, 1.0, 0.0, None
    base = np.asarray((0.40, 0.36, 0.35)) * one
    if kind in "MLK":
        base = _fringed(base, t, e)
    else:
        base = base + (np.asarray((0.50, 0.46, 0.45)) - base) * np.clip((t - 0.85) / 0.10, 0, 1)[..., None]
    return base, base, 0 * t, 1.0, 0.0, None


FRINGE = (0.60, 0.56, 0.54)  # the picazuró's coverts' pale fringes (sRGB, from the photographs)


def _fringed(base, t, e):
    """The picazuró's coverts' pale fringe: a narrow pale border round the
    feather's rounded tip and down its sides' outer part, a darker band
    just inside it, so the overlapping coverts read as scales (v1 review:
    plainer than the photographs)."""
    np = rp._np()
    tipped = np.clip((t - 0.40) / 0.15, 0, 1)
    rim = np.maximum(np.clip((e - 0.88) / 0.07, 0, 1) * tipped, np.clip((t - 0.93) / 0.03, 0, 1))
    inner = np.maximum(np.clip((e - 0.66) / 0.15, 0, 1) * tipped, np.clip((t - 0.80) / 0.08, 0, 1)) * (1 - rim)
    base = base * (1 - 0.22 * inner)[..., None]
    return base + (np.asarray(FRINGE) - base) * rim[..., None]


FEATHER_PAINT = {"torcaza": _feathers_torcaza, "picazuro": _feathers_picazuro}


def _band(t, a, b, soft=0.015):
    return rp._band(t, a, b, soft)


def _shaft(kind, under):
    if NAME == "picazuro":
        return (0.20, 0.20, 0.22) if not under else (0.40, 0.41, 0.44)
    return (0.40, 0.38, 0.35) if not under else (0.66, 0.66, 0.66)


# ---------------------------------------------------------------------------
# The face photographs (``raptor.project_face``). Landmarks in the photo's
# pixels, read on a grid over each head; ``head`` the photo's head ellipse,
# ``iris`` the iris' radius in pixels. Both photos are of the species'
# own southern populations (Uruguay, Buenos Aires), so their colours are
# kept (no ``match="detail"``), gain-matched to the painted head.

FACE_PHOTOS = {
    "torcaza": dict(
        file="Torcaza_Zenaida_auriculata_Uruguay_2015.jpg",
        page="https://commons.wikimedia.org/wiki/File:Torcaza_(Zenaida_auriculata),_Uruguay,_2015.jpg",
        author="Enrique González (Wikimedia Commons)", licence="CC BY 4.0",
        # The bill points down and toward the camera (foreshortened): its
        # tip is not a landmark, and the photo stays off the bill.
        points=dict(eye=(1205, 770), culmen=(1100, 847), nostril=(1075, 866), crown=(1205, 566)),
        head=((1205, 770), (250, 200)), iris=40, rim=0.55, bill=False,
    ),
    "picazuro": dict(
        file="Patagioenas_picazuro_Buenos_Aires_Argentina_8_4c.jpg",
        page="https://commons.wikimedia.org/wiki/File:Patagioenas_picazuro_-Buenos_Aires,_Argentina-8-4c.jpg",
        author="dfaulder (Wikimedia Commons, from Flickr)", licence="CC BY 2.0",
        points=dict(eye=(1688, 342), culmen=(1780, 393), nostril=(1843, 430), crown=(1688, 252), gape=(1787, 433), tip=(1903, 507)),
        head=((1690, 372), (185, 135)), iris=25, rim=0.40,
    ),
}

# ---------------------------------------------------------------------------
# Real feathers: the Feather Atlas scans (U.S. Fish and Wildlife Service
# Forensics Laboratory: a federal work, public domain; its FAQ: "All images
# and data on the website are freely available to the public; no special
# permission is required for their use"). The sheets on blue are cut as
# ``raptor``'s; those on black (with a white grid) are keyed first and laid
# on the same blue (``on_blue``). ``crop``: the rows and columns holding the
# feathers (the title and the axes off). ``order``: the feathers' labels
# left to right (``P10``: the tenth primary; ``x1``: an extra feather).

MANTLE = (0.85, 0.62, (1.02, 0.98, 0.88))  # the torcaza's scapulars' grade (``plan``'s optional fifth item)

SCANS = {
    "torcaza": dict(
        specimen="Mourning Dove (Zenaida macroura): adult secondaries and tail, juvenile wing",
        page="https://www.fws.gov/lab/featheratlas/",
        author="U.S. Fish and Wildlife Service Forensics Laboratory, The Feather Atlas",
        licence="Public domain",
        sheets=dict(
            P=dict(file="MODO_wing_imm.jpg", background="black", crop=(50, 385, 55, 800),
                   order=("P10", "P9", "P8", "P7", "P6", "P5", "P4", "P3", "P2", "P1", "x1", "x2")),
            S=dict(file="MODO_secondary_adult.bluebk.jpg", background="blue", crop=(60, 272, 50, 800),
                   order=("S1", "S2", "S3", "S4", "S5", "S6", "S7", "S8", "S9", "T1", "T2", "T3")),
            R=dict(file="MODO_tail_adult.bluebk.jpg", background="blue", crop=(105, 660, 135, 760),
                   order=("R7", "R6", "R5", "R4", "R3", "R2", "R1")),
        ),
        # The juvenile's primaries and the adult's secondaries a little
        # warmer and darker (the eared dove's olive-brown).
        grade=(0.95, 0.92, (1.04, 1.0, 0.94)),
        plan=dict(
            [(("P", i), ("scan", f"P{i}", 0.0, False)) for i in range(1, 11)]
            + [(("S", i), ("scan", f"S{i}", 0.0, False)) for i in range(1, 10)]
            + [(("S", 10), ("scan", "S8", 0.0, True)), (("S", 11), ("scan", "S6", 0.0, True)), (("S", 12), ("scan", "S4", 0.0, True))]
            + [(("S", 12 + j), ("scan", f"T{j}", 0.0, False)) for j in range(1, 4)]
            + [(("R", 1), ("scan", "R1", 0.0, False)), (("R", 2), ("scan", "R2", 0.0, False)), (("R", 3), ("scan", "R3", 0.0, False)),
               (("R", 4), ("scan", "R4", 0.0, False)), (("R", 5), ("scan", "R6", 0.0, False)), (("R", 6), ("scan", "R7", 0.0, False))]
            + [(("G", j), ("scan", f"S{2 * j}", 0.35, j % 2 == 0)) for j in range(1, 5)]
            + [(("M", j), ("scan", f"S{2 * j - 1}", 0.50, False)) for j in range(1, 5)]
            # The spotted inner coverts (``SPOTS``): their plain twins'
            # pixels, the spots painted on them (``spot``).
            + [(("G", 4 + j), ("scan", f"S{2 * j}", 0.35, j % 2 == 0)) for j in range(1, 4)]
            + [(("M", 4 + j), ("scan", f"S{2 * j - 1}", 0.50, False)) for j in range(1, 4)]
            + [(("L", j), ("scan", f"S{j + 4}", 0.62, j % 2 == 1)) for j in range(1, 5)]
            # The primary coverts: dark grey-brown, from the secondaries' tips.
            + [(("U", j), ("scan", f"S{j}", 0.40, j % 2 == 0, (0.8, 0.70, (1.0, 0.98, 0.95)))) for j in range(1, 5)]
            # The scapulars: the tertials' spotted tips, toned to the
            # mantle's olive-brown.
            + [(("K", 1), ("scan", "T1", 0.15, False, MANTLE)), (("K", 2), ("scan", "T2", 0.15, True, MANTLE)),
               (("K", 3), ("scan", "S9", 0.25, False, MANTLE)), (("K", 4), ("scan", "T3", 0.10, False, MANTLE))]
        ),
    ),
    "picazuro": dict(
        specimen="Band-tailed Pigeon (Patagioenas fasciata): adult wing and tail",
        page="https://www.fws.gov/lab/featheratlas/",
        author="U.S. Fish and Wildlife Service Forensics Laboratory, The Feather Atlas",
        licence="Public domain",
        sheets=dict(
            P=dict(file="BTPI_wing_adult.jpg", background="black", crop=(105, 357, 45, 800),
                   order=("P10", "P9", "P8", "P7", "P6", "P5", "P4", "P3", "P2", "P1", "S1", "S2")),
            R=dict(file="BTPI_tail_adult.jpg", background="black", crop=(90, 695, 85, 790),
                   order=("R6", "R5", "R4", "R3", "R2", "R1")),
        ),
        # The Band-tailed Pigeon's brownish grey turned to the picazuró's
        # blackish slate.
        grade=(0.45, 0.62, (0.96, 0.98, 1.06)),
        # Their under sides (``raptor.underside`` of the scan): grey, as the
        # flight photographs' underwing.
        under=(0.35, 0.88, (0.97, 1.0, 1.06)),
        plan=dict(
            [(("P", i), ("scan", f"P{i}", 0.0, False)) for i in range(1, 11)]
            + [(("S", i), ("scan", ("S1", "S2", "P1", "P2", "P3")[(i - 1) % 5], 0.0, (i - 1) // 5 % 2 == 1)) for i in range(1, 16)]
            # The tail: the picazuró's own blackish slate, with the scans'
            # grain (their pattern, a dark band, is the Band-tailed's).
            + [(("R", i), ("grain", f"R{i}", 0.0, False)) for i in range(1, 7)]
            + [((k, j), ("grain", ("S1", "S2", "P1", "P2")[j - 1], t0, j % 2 == 0)) for k, t0 in (("G", 0.35), ("M", 0.5), ("L", 0.62), ("K", 0.3)) for j in range(1, 5)]
        ),
    ),
}

BLUE = (0.36, 0.48, 0.60)  # the Feather Atlas' blue background (sRGB)


def crop(pic, box):
    """``pic``'s rows ``r0..r1`` and columns ``c0..c1``."""
    r0, r1, c0, c1 = box
    return pic[r0:r1, c0:c1]


def grid_lines(pic, background):
    """The sheet's grid: its thin lines' pixels, with the ``background``
    (a mask) three pixels off on both sides, across or along (inside a
    feather the grid is hidden)."""
    np = rp._np()
    pad = np.pad(background, 3, mode="edge")
    h, w = background.shape
    off = lambda dy, dx: pad[3 + dy:3 + dy + h, 3 + dx:3 + dx + w]
    return ~background & ((off(-3, 0) & off(3, 0)) | (off(0, -3) & off(0, 3)))


def erase_grid(pic, background):
    """``pic`` with its grid's lines painted out where they cross the
    feathers' translucent down too: on the rows and columns where the
    grid shows (``grid_lines`` over an eighth of them or more), each pixel brighter
    than both its neighbours three pixels off takes their mean."""
    np = rp._np()
    lines = grid_lines(pic, background)
    out = pic.copy()
    for axis in (0, 1):
        # The columns on the rows' result: a crossing of the grid's lines
        # is bright on both, so it goes on the second pass.
        pic = out.copy()
        lum = pic @ np.array([0.2126, 0.7152, 0.0722])
        share = lines.mean(axis=1 - axis)
        for i in np.flatnonzero(share > 0.12):
            lo, hi = max(i - 3, 0), min(i + 3, lum.shape[axis] - 1)
            take = lambda a, j: a[j] if axis == 0 else a[:, j]
            bright = (take(lum, i) > take(lum, lo) + 0.05) & (take(lum, i) > take(lum, hi) + 0.05)
            fill = 0.5 * (take(pic, lo) + take(pic, hi))
            if axis == 0:
                out[i] = np.where(bright[:, None], fill, out[i])
            else:
                out[:, i] = np.where(bright[:, None], fill, out[:, i])
    return out


def black_alpha(pic):
    """The feathers' opacity on a scan's black background (its grid
    already erased: ``erase_grid``): bright over the black."""
    np = rp._np()
    lum = pic @ np.array([0.2126, 0.7152, 0.0722])
    return np.clip((lum - 0.06) / 0.06, 0, 1)


def on_blue(pic):
    """A scan sheet on black laid on the Feather Atlas' blue, so
    ``raptor.cut_scan`` and ``raptor.feather_cell`` key it as theirs."""
    np = rp._np()
    a = black_alpha(pic)
    # The edges' pixels are the feather's colour mixed with the black:
    # theirs taken from the feather's inside nearby instead.
    inner = (a > 0.98).astype(np.float64)
    wsum = rp.box_blur(inner, 3, passes=2)
    bled = np.stack([rp.box_blur(pic[..., k] * inner, 3, passes=2) for k in range(3)], -1) / np.maximum(wsum, 1e-6)[..., None]
    pic = np.where(((a < 0.98) & (wsum > 1e-3))[..., None], bled, pic)
    return pic * a[..., None] + np.asarray(BLUE) * (1 - a[..., None])


def sheet(pic, spec):
    """A loaded sheet ready to cut: cropped, on blue, its grid's white
    lines painted over with the blue (``raptor.scan_alpha`` would take
    them for feathers)."""
    np = rp._np()
    pic = crop(pic, spec["crop"])
    if spec["background"] == "black":
        pic = erase_grid(pic, (pic @ np.array([0.2126, 0.7152, 0.0722])) < 0.08)
        return on_blue(pic)
    background = (pic[..., 2] - pic[..., 0]) > 0.15
    pic = erase_grid(pic, background)
    grid = grid_lines(pic, background)
    return np.where(grid[..., None], np.asarray(BLUE), pic)


def scan_feathers(sheets, species=None):
    """The scanned feathers of a species' sheets (kind -> loaded picture,
    rows top down): label -> (picture, ``raptor.cut_scan``'s feather)."""
    out = {}
    for kind, spec in SCANS[species or NAME]["sheets"].items():
        if kind not in sheets:
            continue
        pic = sheet(sheets[kind], spec)
        for label, f in zip(spec["order"], rp.cut_scan(pic, len(spec["order"]))):
            out[label] = (pic, fix_shaft(f))
    return out


def fix_shaft(f):
    """A cut feather whose rachis ``raptor.cut_scan`` found on the vane's
    edge (a pale fringe brighter than the shaft: one vane's reach a small
    part of the other's) gets a rachis down its vane instead: the rows'
    middles, a little toward the outer vane (the scan's left), fitted with
    a smooth curve; its reaches measured again from it."""
    np = rp._np()
    if min(f["left"], f["right"]) >= 0.3 * max(f["left"], f["right"]):
        return f
    left, right, has = f["edges"]
    rows = f["rows"]
    sel = has & (rows <= f["base"]) & (rows >= f["tip"] + 0.05 * (f["base"] - f["tip"]))
    mid = 0.5 * (left + right) - 0.08 * (right - left)
    fit = np.polyfit(rows[sel], mid[sel], 2)
    f = dict(f)
    f["shaft"] = lambda r, fit=fit: np.polyval(fit, r)
    f["left"] = max(float(np.percentile((f["shaft"](rows) - left)[sel], 65)), 2.0)
    f["right"] = max(float(np.percentile((right - f["shaft"](rows))[sel], 65)), 2.0)
    return f


def grade(col, g):
    """A scan's colour toned: chroma, gain and a tint."""
    np = rp._np()
    chroma, gain, tint = g
    lum = (col @ np.array([0.2126, 0.7152, 0.0722]))[..., None]
    return np.clip(gain * (lum + chroma * (col - lum)) * np.asarray(tint), 0, 1)


def grain_cell(pic, f, fo, t0=0.0, flip=False):
    """The scanned feather ``f``'s fine and middling log-luminance detail
    in a card's cell (as ``raptor.scan_grain``): each scan row stretched
    across the painted vane, the rachis on the card's, nothing on it."""
    np = rp._np()
    wpx, hpx = rp.CELL
    lum_pic = np.log(np.maximum(pic @ np.array([0.2126, 0.7152, 0.0722]), 0.02))
    s = (np.arange(wpx) + 0.5) / wpx
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
    lum = rp.bilinear(lum_pic, np.repeat(row[:, None], wpx, axis=1), C)
    fine = rp.box_blur(lum, 2, passes=2)
    off = np.clip((np.abs(s - fo) * wpx - 1.5) / 2.0, 0, 1)[None, :]
    return np.clip(lum - fine, -0.45, 0.45) * off, np.clip(fine - rp.box_blur(lum, 10, passes=2), -0.25, 0.25) * off


def atlas_cell_sources(sheets):
    """Each atlas cell's pixels the scans give: ``(kind, index, under)`` ->
    colour, opacity, height for the ``scan`` cells; ``(kind, index)`` ->
    grain for the ``grain`` cells."""
    spec = SCANS[NAME]
    feathers = scan_feathers(sheets)
    dims = {}
    for fd in rp.feather_set():
        dims.setdefault((fd["kind"], fd["index"]), fd)
    real, grain = {}, {}
    for key, (use, label, t0, flip, *own) in spec["plan"].items():
        fd = dims.get(key)
        if fd is None or label not in feathers:
            continue
        pic, f = feathers[label]
        if use == "scan":
            c, a, h = rp.feather_cell(pic, f, fd["outer"], t0, flip)
            under = grade(rp.underside(c), spec["under"]) if "under" in spec else None
            c = grade(c, own[0] if own else spec["grade"])
            real[key + (False,)] = (c, a, h)
            real[key + (True,)] = (rp.underside(c) if under is None else under, a, h)
        else:
            grain[key] = grain_cell(pic, f, fd["outer"], t0, flip)
    return real, grain, dims


def paint_atlas(sheets=None):
    """The whole atlas (``raptor.paint_atlas``' layout): colour, opacity,
    normal (OpenGL) and roughness, rows bottom up; the cells ``SCANS``'
    plan names carry the scans (``sheets``: kind -> loaded picture), the
    rest are painted."""
    np = rp._np()
    W, H = rp.ATLAS
    colour = np.zeros((H, W, 3), np.float32)
    alpha = np.zeros((H, W), np.float32)
    height = np.zeros((H, W), np.float32)
    real, grain, dims = atlas_cell_sources(sheets) if sheets else ({}, {}, None)
    if dims is None:
        dims = {}
        for fd in rp.feather_set():
            dims.setdefault((fd["kind"], fd["index"]), fd)
    for (kind, index, under), cell in rp.atlas_cells().items():
        fd = dims.get((kind, index))
        if fd is None:
            continue
        if (kind, index, under) in real:
            c, a, h = real[(kind, index, under)]
        else:
            c, a, h = rp.paint_feather(kind, index, under, fd["length"], fd["width"], fd["outer"], fd["finger"], seed=cell)
            if (kind, index) in grain:
                c, h = rp.regrain(c, a, h, grain[(kind, index)], under)
        if index in SPOTS.get(NAME, {}).get(kind, {}).values() and not under:
            c = spot(c, a, kind, index, fd["length"], fd["width"], fd["outer"])
        u0, v0, u1, v1 = rp.cell_rect(cell)
        x0, y0 = round(u0 * W), round(v0 * H)
        colour[y0:y0 + rp.CELL[1], x0:x0 + rp.CELL[0]] = c
        alpha[y0:y0 + rp.CELL[1], x0:x0 + rp.CELL[0]] = a
        height[y0:y0 + rp.CELL[1], x0:x0 + rp.CELL[0]] = h
    gy, gx = np.gradient(height)
    nrm = np.stack([-gx * 2.5, -gy * 2.5, np.ones_like(gx)], axis=-1)
    nrm /= np.linalg.norm(nrm, axis=-1, keepdims=True)
    rough = np.where(height > 0.95, 0.55, 0.80).astype(np.float32)
    return colour, alpha, nrm * 0.5 + 0.5, np.repeat(rough[..., None], 3, axis=-1)


# ---------------------------------------------------------------------------
# Blender: the sculpt. A pigeon's body: a deep, plump breast over the keel,
# a short thick neck, a small round head with a steep forehead and no brow
# ridge, the wing's coverts slab (``raptor.wing_slab``), short legs tucked
# under the vent.


def sculpt(voxel):
    np = rp._np()
    hw, hl, hh = SP["torso"]
    s = rp.bird_scale()
    span_x = rp.wing_joints()[3][0] + 0.02 * s
    lo = (-span_x, bill_tip()[1] - 0.01 * s, min(p[2] for p in rp.leg_joints()) - 0.012 * s)
    hi = (span_x, rp.tail_root()[1] + 0.40 * SP["tail"] + 0.02 * s, rp.head_centre()[2] + SP["head"][2] + 0.02 * s)
    sdf = rig.Sdf(lo, hi, voxel)
    E, RC, T = rig.sdf_ellipsoid, rig.sdf_round_cone, rig.sdf_tube
    sdf.add(E((0, 0, 0), (hw, hl, hh)))
    # The breast: full and rounded, deepest ahead of the wings; the body
    # streamlined behind it.
    sdf.add(E((0, -0.42 * hl, -0.12 * hh), (1.0 * hw, 0.62 * hl, 1.0 * hh)), k=0.4 * hw)
    sdf.add(E((0, -0.58 * hl, -0.26 * hh), (0.78 * hw, 0.44 * hl, 0.78 * hh)), k=0.5 * hw)
    sdf.add(E((0, -0.05 * hl, 0.20 * hh), (0.95 * hw, 0.80 * hl, 0.70 * hh)), k=0.4 * hw)
    tr = rp.tail_root()
    sdf.add(RC((0, 0.40 * hl, -0.05 * hh), tr, 0.62 * hw, 0.22 * hw), k=0.5 * hw)
    nb, hc = rp.neck_base(), rp.head_centre()
    w, l, h = SP["head"]
    sdf.add(T([(0, -0.40 * hl, 0.12 * hh), nb, rp.lerp(nb, hc, 0.6)], [(0.80 * hw, 0.80 * hw), (0.68 * hw, 0.70 * hw), (0.92 * w, 0.95 * w)]), k=0.4 * hw)
    sdf.add(E(hc, (w, l, h)), k=0.5 * w)
    # The steep forehead down to the operculum.
    sdf.add(E(rp.add(hc, (0, -0.55 * l, -0.12 * h)), (0.62 * w, 0.55 * l, 0.62 * h)), k=0.5 * w)
    r = SP["eye"]
    for sx in (1.0, -1.0):
        oy, oz = SP["eye_open"]
        sdf.carve(E(rp.eye_centre(sx), (0.9 * r, 0.95 * oy * r, 0.95 * oz * r)), k=0.30 * r)
    L, D, Wb = SP["bill"]
    bb = rp.bill_base()
    # The face sloping into the operculum: a tapered wedge from the
    # forehead to the bill's base, blended wide, so the bill grows out of
    # the face instead of standing off it (v1 review: a stick).
    sdf.add(T([rp.add(bb, (0, SP["cere"] + 1.4 * D, 0.28 * D)), rp.add(bb, (0, SP["cere"], 0.12 * D)), rp.add(bb, (0, 0.1 * SP["cere"], 0.02 * D))],
              [(1.30 * Wb, 1.25 * D), (0.95 * Wb, 0.95 * D), (0.58 * Wb, 0.60 * D)], side=(1, 0, 0)), k=0.8 * D)
    for side in (1.0, -1.0):
        sdf.add(rp.wing_slab(side), k=0.25 * hw)
    sdf.add(E(rp.add(tr, (0, 0.22 * SP["tail"], 0.20 * hh)), (0.32 * hw, 0.28 * SP["tail"], 0.10 * hh)), k=0.4 * hw)
    sdf.add(E(rp.add(tr, (0, 0.18 * SP["tail"], -0.24 * hh)), (0.36 * hw, 0.30 * SP["tail"], 0.12 * hh)), k=0.4 * hw)
    # Legs, tucked: the feathered tibiae, the bare tarsi folded up into
    # the belly, the toes clenched (three curling down and back under the
    # foot, the hallux forward), small claws.
    lr = SP["leg_r"]
    to = SP["leg"][2]
    for side in (1.0, -1.0):
        hip, knee, heel, foot, toe = rp.leg_joints(side)
        sdf.add(T([hip, knee, rp.lerp(knee, heel, 0.85)], [(0.40 * hw, 0.40 * hw), (0.32 * hw, 0.32 * hw), (1.5 * lr, 1.5 * lr)]), k=0.3 * hw)
        sdf.add(RC(heel, foot, 1.05 * lr, 0.95 * lr), k=0.6 * lr)
        for spread, ln in ((0.0, 1.0), (0.45, 0.85), (-0.45, 0.80), (None, 0.55)):
            if spread is None:
                d1 = rp.norm((0.0, -0.6, -0.8))
            else:
                d1 = rp.norm((side * spread * 0.5, 0.50, -0.87))
            k1 = rp.add(foot, rp.mul(d1, 0.32 * to * ln))
            k2 = rp.add(k1, rp.mul(rp.norm((d1[0], -0.8 * d1[1], -0.6)), 0.18 * to * ln))
            sdf.add(T([foot, k1, k2], [(0.55 * lr, 0.55 * lr), (0.48 * lr, 0.48 * lr), (0.30 * lr, 0.30 * lr)]), k=0.3 * lr)
    return sdf.to_mesh(f"{SP['mesh']}Sculpt")


def bill_sculpt():
    """The slender bill, finer than the body: the culmen gently decurved,
    the upper mandible's tip drooping over the lower; the gape between
    them; the soft swollen operculum over the base with the nostrils' slits
    at its front edge, its back sinking into the forehead."""
    L, D, Wb = SP["bill"]
    bb = rp.bill_base()
    cl = SP["cere"]
    T, E = rig.sdf_tube, rig.sdf_ellipsoid
    voxel = D / 26.0
    lo = (-Wb, bb[1] - 1.2 * L, bb[2] - 1.0 * D)
    hi = (Wb, bb[1] + cl + 1.4 * D, bb[2] + 1.0 * D)
    sdf = rig.Sdf(lo, hi, voxel)
    p = lambda y, z: rp.add(bb, (0.0, y, z))
    up = [p(0.10 * L, 0.04 * D), p(-0.30 * L, 0.0), p(-0.62 * L, -0.08 * D), p(-0.86 * L, -0.21 * D), p(-1.00 * L, -0.42 * D)]
    sdf.add(T(up, [(0.46 * Wb, 0.47 * D), (0.31 * Wb, 0.29 * D), (0.24 * Wb, 0.22 * D), (0.20 * Wb, 0.19 * D), (0.08 * Wb, 0.09 * D)], side=(1, 0, 0), samples=10), k=0.06 * D)
    lower = [p(0.10 * L, -0.40 * D), p(-0.35 * L, -0.43 * D), p(-0.68 * L, -0.49 * D), p(-0.88 * L, -0.53 * D)]
    sdf.add(T(lower, [(0.36 * Wb, 0.20 * D), (0.26 * Wb, 0.15 * D), (0.18 * Wb, 0.11 * D), (0.07 * Wb, 0.05 * D)], side=(1, 0, 0)), k=0.06 * D)
    # The operculum: a soft swelling each side over the nostril, and its
    # back over the forehead's base.
    sdf.add(T([p(cl + 0.60 * D, 0.20 * D), p(cl * 0.5, 0.10 * D), p(0.0, 0.05 * D)], [(0.62 * Wb, 0.62 * D), (0.50 * Wb, 0.54 * D), (0.42 * Wb, 0.44 * D)], side=(1, 0, 0)), k=0.15 * D)
    for sx in (1.0, -1.0):
        sdf.add(E(rp.add(bb, (sx * 0.22 * Wb, 0.45 * cl, 0.22 * D)), (0.32 * Wb, 0.55 * cl, 0.32 * D)), k=0.12 * D)
        sdf.carve(E(rp.add(bb, (sx * 0.40 * Wb, 0.02 * cl, 0.05 * D)), (0.09 * D, 0.20 * cl, 0.05 * D)), k=0.03 * D)
    return sdf.to_mesh(f"{SP['mesh']}Bill")


# ---------------------------------------------------------------------------
# Build.

VOXEL = 0.0010  # m: the dense sculpt's voxel at the torcaza's size, scaled with the bird
BUDGET = {"torcaza": 5200, "picazuro": 5600}  # the skin's triangles


def build(draft=False, textures=None, scans=None, faces=None):
    """The bird (``raptor.build``'s steps with the pigeon's sculpt, bill and
    atlas): ``scans`` (a folder holding ``SCANS``' sheets) and ``faces``
    (one holding ``FACE_PHOTOS``' files) bring the real pixels in."""
    np = rp._np()
    rig.FPS = FPS
    rig.reset()
    voxel = VOXEL * SP["wing"][1] / SPECIES["torcaza"]["wing"][1] * (1.8 if draft else 1.0)
    dense = sculpt(voxel)
    bill = bill_sculpt()
    body = rig.decimated_copy(dense, SP["mesh"], budget=BUDGET[NAME])
    rp.join_plain(body, rig.decimated_copy(bill, "BillGame", budget=600))
    rp.join_plain(dense, bill)
    body.data.materials.clear()
    body.data.materials.append(rig.plain_material("Main", (0.5, 0.45, 0.42)))
    rig.unwrap(body, margin=0.003)
    size = 1024 if draft else 2048
    face = iris = None
    if faces:
        face = rp.load_rgb(os.path.join(faces, FACE_PHOTOS[NAME]["file"]))
        iris = rp.iris_colour(face)
    maps = rp.bake_plumage(body, dense, size, face)
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
    cards = rp.feather_cards(f"{SP['mesh']}Feathers")
    cards.data.materials.append(rig.plain_material("Feathers", (0.4, 0.38, 0.36)))
    eye_objs = rp.eyes(iris)
    for o in [cards] + eye_objs:
        rig.join(body, o)
    body.parent = arm
    body.modifiers.new("Armature", "ARMATURE").object = arm
    print(f"pigeon: {NAME}: {rig.triangles(body)} triangles, span {2 * rp.half_span():.3f} m, length {rp.total_length():.3f} m, "
          f"wing {chord():.3f} m, {wingbeat():.2f} wingbeats a second")
    return body, arm, maps, atlas


# ---------------------------------------------------------------------------
# Clips (``raptor``'s pose helpers). A pigeon in cruising flight beats its
# wings through a wide arc, the downstroke a little longer than the
# upstroke (Tobalske & Dial, J. Exp. Biol. 199, 1996: Rock Pigeon,
# downstroke about 0.55 of the beat); on the upstroke the wing flexes at
# the elbow and the wrist, the hand swept back and the primaries parted.

DOWNSTROKE = 0.55
FLAP_BEATS = 3
GLIDE_SECONDS = 2.0
HOLD_SECONDS = 0.6  # ``FlapGlide``: the glide between the bursts
BLEND_SECONDS = 0.07  # into and out of it
EXIT_PHASE = 0.27  # the downstroke's point (of a beat) where the wings stop into the glide
ENTRY_PHASE = 0.775  # the upstroke's point the burst starts from (the elevation passing the glide's)


def flap_angles(t):
    """The wing's joint angles (degrees) at phase ``t`` of one wingbeat,
    from the top of the upstroke: ``elev`` (the arm over the horizontal),
    ``elbow`` and ``wrist`` (flexion), ``twist`` (the hand's pronation),
    ``hand`` (the hand raised over the arm), ``spread`` (the primaries
    parting) and ``sweep`` (the arm forward)."""
    t = t % 1.0
    up, down = SP["stroke"]
    if t < DOWNSTROKE:
        u = t / DOWNSTROKE
        elev = up + (down - up) * (0.5 - 0.5 * math.cos(math.pi * u))
        flex, twist, spread = 0.0, 8.0 * math.sin(math.pi * u), 1.0
        sweep = 6.0 * math.sin(math.pi * u)
    else:
        u = (t - DOWNSTROKE) / (1 - DOWNSTROKE)
        elev = down + (up - down) * (0.5 - 0.5 * math.cos(math.pi * u))
        flex = math.sin(math.pi * u)
        twist, spread = -12.0 * flex, 1.0 + 0.6 * flex
        sweep = -8.0 * flex
    return dict(elev=elev, elbow=40.0 * flex, wrist=70.0 * flex, twist=twist, spread=spread, hand=-18.0 * flex, sweep=sweep)


def glide_angles(t=0.0):
    """The gliding wing at phase ``t`` of ``Glide``: held out in a shallow
    dihedral, small balancing corrections."""
    w = 2 * math.pi * t
    return dict(elev=SP["dihedral"] + 1.2 * math.sin(w), elbow=0.0, wrist=2.0, twist=0.8 * math.sin(w + 1.0),
                spread=1.0, hand=-0.4 * SP["dihedral"] + 0.8 * math.sin(2 * w), sweep=-2.0 + 1.0 * math.sin(w + 0.5))


# Pennycuick's allometry (``raptor.flap_frequency``) overestimates the
# pigeons' wingbeat: for a homing pigeon (Columba livia, about 0.57 kg,
# 0.68 m span, 0.072 m2 of wing: the birds and their 28.1 g loggers in the
# study below) it gives 8.2 Hz, where 200 Hz accelerometers measured
# 5.5 +- 0.46 Hz flying solo and 6.6 +- 0.42 Hz flying in pairs (Taylor,
# Biro, Lambert, Walker, Taylor & Portugal, "Birds invest wingbeats to keep
# a steady head and reap the ultimate benefits of flocking", bioRxiv
# 10.1101/492090, 2018). The sky's pigeons fly in flocks: the allometry is
# scaled to the pairs' measured value (``WINGBEAT_REFERENCE``).
WINGBEAT_REFERENCE = dict(mass=0.57, span=0.68, area=0.072, measured=6.6)


def allometric_wingbeat(mass, span, area):
    """Pennycuick's (J. Exp. Biol. 199, 1996) wingbeat frequency, Hz."""
    return mass ** 0.375 * 9.81 ** 0.5 * span ** (-23 / 24) * area ** (-1 / 3) * 1.2 ** (-0.375)


def wingbeat():
    """The model's wingbeat frequency in flocking flight, Hz: the
    allometry for its mass, span and wing area, calibrated on the homing
    pigeons' measured one."""
    ref = WINGBEAT_REFERENCE
    k = ref["measured"] / allometric_wingbeat(ref["mass"], ref["span"], ref["area"])
    return k * allometric_wingbeat(SP["mass"], 2 * rp.half_span(), rp.wing_area())


def flap_seconds():
    """``Flap``: ``FLAP_BEATS`` wingbeats, a whole number of frames."""
    return round(FLAP_BEATS * FPS / wingbeat()) / FPS


def beat_seconds():
    return flap_seconds() / FLAP_BEATS


def flap_glide_seconds():
    """``FlapGlide``: from the upstroke's ``ENTRY_PHASE`` through
    ``FLAP_BEATS`` beats to the downstroke's ``EXIT_PHASE``, then the hold."""
    beats = FLAP_BEATS - (ENTRY_PHASE - EXIT_PHASE)
    return round((beats * beat_seconds() + HOLD_SECONDS) * FPS) / FPS


def flap_glide_angles(t):
    """``FlapGlide``'s wing at phase ``t`` of the clip, and the glide's
    share (0 flapping, 1 gliding). The burst runs from ``ENTRY_PHASE`` to
    ``EXIT_PHASE`` ``FLAP_BEATS`` beats later; the wings blend into the
    glide there over ``BLEND_SECONDS`` and, at the hold's end, out of it
    into the upstroke the burst starts from."""
    total = flap_glide_seconds()
    sec = t * total
    flapping = total - HOLD_SECONDS
    if sec < flapping + 0.5 * HOLD_SECONDS:
        phase = ENTRY_PHASE + min(sec, flapping) / beat_seconds()
        g = rp.smooth((sec - flapping) / BLEND_SECONDS)
    else:
        phase = ENTRY_PHASE - (total - sec) / beat_seconds()
        g = rp.smooth((total - sec) / BLEND_SECONDS)
    a, b = flap_angles(phase), glide_angles(0.0)
    return {k: a[k] + (b[k] - a[k]) * g for k in a}, g


def pose_wings(pose, a):
    for side in (1.0, -1.0):
        rp.pose_wing(pose, side, elev=a["elev"], sweep=a["sweep"], elbow=a["elbow"], wrist=a["wrist"], twist=a["twist"],
                     hand=a["hand"], spread=a["spread"])


def flap(arm, t):
    """``FLAP_BEATS`` wingbeats; the body rises on each downstroke and
    pitches a little; the head stays steady, the tail closed."""
    pose = rig.Pose(arm)
    beat = (t * FLAP_BEATS) % 1.0
    s = rp.bird_scale()
    lift = 0.004 * s * math.sin(2 * math.pi * (beat - 0.12))
    pitch = 2.0 * math.sin(2 * math.pi * (beat + 0.1))
    rp.pose_body(pose, pitch=pitch, lift=lift)
    pose_wings(pose, flap_angles(beat))
    rp.pose_tail(pose, spread=3.0 + 2.0 * math.sin(2 * math.pi * beat), pitch=-2.0 - 2.0 * math.sin(2 * math.pi * beat))
    rp.pose_head(pose, pitch=-2.0 - pitch)
    return pose


def glide(arm, t):
    pose = rig.Pose(arm)
    w = 2 * math.pi * t
    rp.pose_body(pose, roll=1.5 * math.sin(w), pitch=1.0 * math.sin(2 * w))
    pose_wings(pose, glide_angles(t))
    rp.pose_tail(pose, spread=5.0 + 2.0 * math.sin(w + 0.5), twist=2.0 * math.sin(w + 1.3))
    rp.pose_head(pose, yaw=6.0 * math.sin(w), pitch=-4.0)
    return pose


def flap_glide(arm, t):
    pose = rig.Pose(arm)
    a, g = flap_glide_angles(t)
    beat = (ENTRY_PHASE + t * flap_glide_seconds() / beat_seconds()) % 1.0
    s = rp.bird_scale()
    lift = (1 - g) * 0.004 * s * math.sin(2 * math.pi * (beat - 0.12))
    pitch = (1 - g) * 2.0 * math.sin(2 * math.pi * (beat + 0.1))
    rp.pose_body(pose, pitch=pitch, lift=lift)
    pose_wings(pose, a)
    rp.pose_tail(pose, spread=3.0 + 2.0 * g, pitch=-2.0 * (1 - g))
    rp.pose_head(pose, pitch=-2.0 - pitch)
    return pose


CLIP_ORDER = ("Flap", "Glide", "FlapGlide")


def clips(arm):
    return [
        rig.bake_clip(arm, "Flap", flap_seconds(), flap, "Body"),
        rig.bake_clip(arm, "Glide", GLIDE_SECONDS, glide, "Body"),
        rig.bake_clip(arm, "FlapGlide", flap_glide_seconds(), flap_glide, "Body"),
    ]


# ---------------------------------------------------------------------------
# Previews.


def shots_for(actions):
    """``(file, action or None, frame, eye, target, lens)``: the gliding bird
    from above, below, below at three quarters, the front at three
    quarters and the side; two phases of the wingbeat (the top of the
    upstroke and the bottom of the downstroke); the head close up."""
    by = {a.name: a for a in actions}
    b = 2 * rp.half_span()
    d = 2.7 * b
    c = (0.0, 0.05 * b, 0.0)
    hc = rp.head_centre()
    hr = SP["head"][1]
    g = by.get("Glide")
    shots = [
        ("top", g, 0, (0.0, 0.051 * b, d), c, 100),
        ("bottom", g, 0, (0.0, 0.051 * b, -d), c, 100),
        ("below34", g, 0, (0.45 * d, -0.55 * d, -0.70 * d), c, 100),
        ("front34", g, 0, (0.55 * d, -0.75 * d, 0.30 * d), c, 100),
        ("side", g, 0, (d, 0.05 * b, 0.05 * b), c, 100),
        ("head", None, 0, rp.add(hc, (11 * hr, -9 * hr, 2.5 * hr)), rp.add(hc, (0, -0.6 * hr, 0)), 105),
        ("head_side", None, 0, rp.add(hc, (14 * hr, -2.5 * hr, 0.6 * hr)), rp.add(hc, (0, -0.8 * hr, -0.1 * hr)), 105),
        ("head_front", None, 0, rp.add(hc, (3 * hr, -15 * hr, 1.5 * hr)), rp.add(hc, (0, -0.6 * hr, -0.1 * hr)), 105),
    ]
    if "Flap" in by:
        n = round(beat_seconds() * FPS)
        shots.append(("flap_up", by["Flap"], 0, (0.40 * d, -0.85 * d, 0.30 * d), c, 100))
        shots.append(("flap_down", by["Flap"], round(DOWNSTROKE * n), (0.40 * d, -0.85 * d, 0.30 * d), c, 100))
        shots.append(("flap_mid", by["Flap"], round((DOWNSTROKE + 1) / 2 * n), (0.40 * d, -0.85 * d, 0.30 * d), c, 100))
    return shots


def parse(argv):
    import argparse

    parser = argparse.ArgumentParser(prog="pigeon.py")
    parser.add_argument("output")
    parser.add_argument("--species", default="torcaza", choices=sorted(SPECIES))
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
        rp.render_shots(args.preview, arm, mesh, maps, atlas, shots)
        far = [s for s in shots_for(actions) if s[0] in ("below34", "flap_down")]
        if not args.shots or any(re.search(args.shots, "light_" + s[0]) for s in far):
            light.hide_render, mesh.hide_render = False, True
            rp.render_shots(args.preview, arm, light, maps, atlas, [("light_" + s[0],) + s[1:] for s in far])
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
