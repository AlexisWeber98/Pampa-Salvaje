"""Model, rig, paint and animate the pampa's cattle (cow and bull) in Blender,
by script.

Run it headless, once per animal:

    blender -b --factory-startup --python tools/cattle.py -- OUT.glb \
        [--bull] [--textures DIR] [--preview DIR] [--shots REGEX] [--draft] [--no-clips]

Writes OUT.glb: one skinned mesh ``Cow`` (the cow, or with ``--bull`` the
bull), and its light mesh ``Cow_LOD`` (``light_mesh``: the skin alone,
decimated, that the game draws for a herd far off) with the materials of the Quaternius cattle the game draws today
(``Main``, ``Main_Light``, ``Muzzle``, ``Hooves``, ``Horns``, ``Eye_Black``,
``Eye_White``) plus ``Hair`` (the tail's switch, the curls of the poll, the
fringe of the ears, the lashes and the bull's sheath tuft, alpha-tested
cards); its armature ``AnimalArmature`` keeps the Quaternius quadruped's
joint names (``Body``, ``Back``, ``Torso``-``Torso3``, ``Neck1``-``Neck3``,
``Head``, the legs, ``Tail1``-``Tail7``, the IK bones and their ``_end``
leaves) and adds the ears, the jaw, the nostrils, a longer tail, the
dewlap, the udder (the bull's scrotum) and six muscle helpers; and the seven
clips ``Idle``, ``Idle_2``, ``Idle_Headlow``, ``Eating``, ``Walk``,
``Gallop`` and ``Gallop_Jump`` (a buck: the game plays it for no cattle
today) with the Quaternius lengths (the exporter sorts them by name:
``tools/glbtrim.py --keep`` puts them back in this order). Life size: a cow
1.40 m over the loin with the claws at the origin, facing -Y (glTF +Z); the
bull is built on the same skeleton, more massive, and the game draws it
larger (``Species::withers``).

It is the horse (``tools/horse.py``) with a bovine anatomy: the same steps
(the sculpt baked onto a decimated mesh, the hair cards, the skinning, the
analytic leg IK, the gaits, the previews) run through
``animal_rig.Steps`` with the names below replaced, so a fix to one of
those steps reaches both animals.

``Main`` is the whole coat. Its maps (``--textures DIR``:
``DIR/cow_body`` or ``DIR/bull_body``, and ``DIR/cattle_hair``) carry a
neutral luminance, the baked relief of the sculpt and its short hair, the
horn's rings and the nose pad's print, and the occlusion; and each coat
is baked into a colour map of its own in the body's set
(``hereford.jpg``, ``angus.jpg``, ``roan.jpg``, ``overo.jpg``:
``bake_coats``, the coat of ``color_at`` at each texel times the
luminance), which the game draws ``Main`` with, so the GLB has no vertex
colours and the game needs no port of ``color_at``; the one-colour
parts (``Main_Light``: the udder's skin; ``Muzzle``: the
bare nose pad and the mouth; ``Hooves``; ``Horns``) multiply them by
their colour. ``tools/textures_to_ktx2.sh`` turns the sets into KTX2.

Breeds and coats (1919, an estancia of the pampa): by then the creole
cattle of the pampa had been crossed into the British breeds (pure creole
stock was gone around 1920): Shorthorn (the "Durham", the base of the
Argentine herd and of the chilled-beef trade, red, roan or white),
Hereford (red with a white face, crest, brisket, underline and switch,
horned) and Aberdeen Angus (black, polled), with a few overo (pied)
crossbreds left. ``--preview DIR`` renders them all (``COATS``).

Anatomy, in metres: a beef cow of the period, deep and wide in the barrel,
with a level topline, the hooks and the pins showing, a dewlap and a
brisket, a long tail with its switch reaching below the hocks, a broad
flat forehead with the poll ridge between the horns, horizontal ears,
prominent eyes, a wide moist nose pad, two-clawed feet with dewclaws, and
a modest udder; the bull with a crest of muscle over the neck, a heavier
forehand, a deeper dewlap, thicker shorter horns, the sheath with its tuft
and the scrotum. See tools/README.md.
"""

import math
import os
import re
import sys

try:
    import bpy
except ImportError:  # outside Blender: only the coats and the plans (tests)
    bpy = None

HERE = os.path.dirname(os.path.abspath(__file__))

# ``--ox``: the working ox of a carreta's team, a castrated criollo steer:
# the bull's frame and mass without its crest or scrotum (the sheath
# stays), long lyre horns, and the criollo coat (``ox_coats``). Its maps go to
# ``DIR/ox_body`` and ``DIR/ox_hair``, so the cow's and the bull's stay as
# they are.
OX = "--ox" in sys.argv
BULL = "--bull" in sys.argv or OX

# ---------------------------------------------------------------------------
# The head's frame: from the top of the poll down the face to the muzzle.
# ``at(t, u, s)`` is ``t`` metres along it, ``u`` up from it (toward the
# forehead) and ``s`` to the left. Plain tuples, so the coats work without
# Blender.

HEAD_O = (0.0, -1.28, 1.30)
_A = (0.0, -0.62, -0.785)
_n = math.sqrt(sum(c * c for c in _A))
HEAD_A = tuple(c / _n for c in _A)
HEAD_U = (0.0, HEAD_A[2], -HEAD_A[1])  # perpendicular, forward and up


HEAD_LEN = 1.0  # the face's length (``t`` is scaled by it)


def _at(t, u=0.0, s=0.0):
    t *= HEAD_LEN
    return (s, HEAD_O[1] + HEAD_A[1] * t + HEAD_U[1] * u, HEAD_O[2] + HEAD_A[2] * t + HEAD_U[2] * u)


def head_coords(p):
    """(t, u, s) of ``p`` (Blender metres) in the head's frame."""
    r = (p[0] - HEAD_O[0], p[1] - HEAD_O[1], p[2] - HEAD_O[2])
    return ((r[1] * HEAD_A[1] + r[2] * HEAD_A[2]) / HEAD_LEN, r[1] * HEAD_U[1] + r[2] * HEAD_U[2], r[0])


def smooth(t):
    t = max(0.0, min(1.0, t))
    return t * t * (3 - 2 * t)


# The build: the anatomy below is laid out on a taller frame, and the model is
# that layout with its legs shortened (``warp``): between the fetlocks and the
# belly every height is squashed, everything above drops by the difference.
# So the body, the head and the claws keep their shapes and a cow stands low
# and blocky on short legs, as the beef cattle of 1919 did. The coats (and
# their colour maps, ``bake_coats``) read a model point back with ``unwarp``.
LEG_FROM, LEG_TO, LEG_SQUASH = 0.14, 0.64, 0.80
LEG_DROP = (LEG_TO - LEG_FROM) * (1.0 - LEG_SQUASH)


def warp(p):
    """A point of the layout (Blender metres) to the model."""
    z = p[2]
    if z > LEG_TO:
        z -= LEG_DROP
    elif z > LEG_FROM:
        z = LEG_FROM + (z - LEG_FROM) * LEG_SQUASH
    return (p[0], p[1], z)


def unwarp(p):
    """A point of the model back to the layout."""
    z = p[2]
    if z > LEG_TO - LEG_DROP:
        z += LEG_DROP
    elif z > LEG_FROM:
        z = LEG_FROM + (z - LEG_FROM) / LEG_SQUASH
    return (p[0], p[1], z)


def from_body(p):
    """The game's body frame (+X forward, +Y up, +Z right) to Blender."""
    return (-p[2], -p[0], p[1])


def to_body(q):
    """Blender (facing -Y, Z up) to the game's body frame."""
    return (-q[1], q[2], -q[0])


# ---------------------------------------------------------------------------
# The coats, baked into each coat's colour map (``coat_pixels``); the
# one-colour parts are the game's ``paint`` (crates/render/src/fauna/
# livestock.rs), which keeps these values. ``p`` and ``n`` are in the body
# frame, in metres of the model (the bull's are the cow's: the game scales
# it up), sRGB 0..255.

COATS = {
    # Hereford: red, white face, crest, brisket, underline, lower legs and
    # switch; pink nose pad; waxy horns with dark tips.
    "hereford": dict(body=(108, 38, 20), white=(232, 226, 212), hair=(232, 226, 212), muzzle=(206, 156, 136),
                     skin=(214, 160, 142), hoof=(64, 54, 46), horns=(226, 212, 178)),
    # Aberdeen Angus: black all over, polled (the game hides ``Horns``).
    "angus": dict(body=(26, 23, 21), white=None, hair=(22, 20, 19), muzzle=(30, 27, 25), skin=(40, 34, 32),
                  hoof=(30, 27, 24), horns=(226, 212, 178)),
    # Shorthorn (Durham) roan: dark, dull red hairs mixed with white ones,
    # redder on the head and the neck, whiter on the belly and the legs. From
    # afar a steel grey (the reference roans). The game's warm light keeps
    # about all of the red and three quarters of the blue (measured on its
    # shots), so the white is a cool steel white and the mix a bluish grey
    # that it leaves grey, not pink.
    "roan": dict(body=(62, 46, 44), white=(182, 206, 228), hair=(132, 112, 106), muzzle=(200, 152, 132),
                 skin=(212, 166, 148), hoof=(78, 66, 54), horns=(228, 216, 184)),
    # A criollo overo crossbred: white with black patches (``overo_black``),
    # the face and the lower legs black.
    "overo": dict(body=(230, 226, 216), white=None, hair=(26, 24, 22), muzzle=(44, 38, 34), skin=(60, 52, 48),
                  hoof=(40, 34, 30), horns=(222, 206, 170)),
}

if OX:
    # The working oxen's criollo coats: the hosco colorado (red-brown with
    # the head, the neck and the legs gone dark, ``hosco_dark``) and the
    # criollo overo.
    COATS = {
        "hosco": dict(body=(118, 58, 28), white=(44, 28, 20), hair=(40, 26, 18), muzzle=(40, 32, 28), skin=(70, 50, 42),
                      hoof=(40, 34, 30), horns=(214, 196, 158)),
        "overo": COATS["overo"],
    }


def hosco_dark(q):
    """How dark the hosco coat goes at ``q``: the head and the throat, fading
    down the neck, and the lower legs."""
    head = smooth((-0.95 - q[1]) / 0.18)
    legs = smooth((0.48 - q[2]) / 0.12)
    return max(head, legs * 0.85)


# The cow's udder (Blender metres): the attachment under the belly and the
# four quarters, as ellipsoids (centre, radii), and the teats' tips. The
# sculpt builds it from these; the coats keep it bare skin.
# Laid out a quarter deeper than it is, as ``warp`` squashes it with the legs.
UDDER = [((0.0, 0.50, 0.69), (0.14, 0.18, 0.13))] + [
    ((sx * 0.066, y, 0.60), (0.078, 0.088, 0.094)) for sx in (1, -1) for y in (0.43, 0.58)
] + [((sx * 0.069, y - 0.012, 0.47), (0.016, 0.016, 0.065)) for sx in (1, -1) for y in (0.43, 0.58)]


def udder_gap(q):
    """About how far (m) ``q`` (Blender) is outside the cow's udder below
    the belly (negative inside); far for the bull, who has none."""
    if BULL or q[2] > 0.76:
        return 1.0
    best = 1.0
    for c, r in UDDER:
        d = math.sqrt(sum(((q[i] - c[i]) / r[i]) ** 2 for i in range(3)))
        best = min(best, (d - 1.0) * min(r))
    return best


def udder_skin(q):
    """How much of the udder's bare skin shows at ``q``: all of it on the
    udder and the teats, fading into the haired belly over a few cm."""
    if q[2] > 0.76:
        return 0.0
    return smooth((0.06 - udder_gap(q)) / 0.03) * smooth((0.76 - q[2]) / 0.04)


def overo_black(p):
    """Whether the overo's body point ``p`` falls in one of its few large
    black patches (crossed waves; the game's former ``overo_black``)."""
    wave = math.sin(p[0] * 3.1 + 0.7) * math.sin(p[1] * 2.3 + p[2] * 1.9 + 1.1) + 0.6 * math.sin(
        p[0] * 5.3 - p[2] * 3.7 + 2.0) * math.cos(p[1] * 4.1 + 0.3)
    return wave > 0.3


def wobble(p, k=1.0):
    """A wavy offset of about +-1, for ragged colour edges."""
    return 0.6 * math.sin(p[0] * 17.0 * k + p[2] * 9.0) + 0.4 * math.sin(p[1] * 23.0 * k - p[0] * 11.0 + 1.7)


def white_face(q):
    """How much of the Hereford's white face is at ``q`` (Blender): the face
    and the forehead to behind the poll and under the jaw; the ears and the
    ring round the eyes stay red."""
    t, u, s = head_coords(q)
    if q[2] < 0.85 or q[1] > -1.0:
        return 0.0
    face = smooth((t + 0.05 + 0.02 * wobble(q)) / 0.04)
    # Only the ear flaps themselves stay red, beyond the broad poll.
    ears = smooth((abs(s) - 0.15 - 0.008 * wobble(q, 2.0)) / 0.015) * smooth((0.12 - t) / 0.04)
    eye = math.sqrt((t - EYE[0]) ** 2 + (u - EYE[1]) ** 2 + (abs(s) - EYE[2]) ** 2)
    ring = smooth((0.03 + 0.006 * wobble(q, 3.0) - eye) / 0.008)
    return face * (1.0 - ears) * (1.0 - ring)


def hereford_white(q, n):
    """How much white the Hereford shows at ``q`` (Blender) facing ``n``."""
    x, y, z = q
    w = white_face(q)
    # The crest: a white stripe down the top of the neck to the withers.
    crest = smooth((0.05 + 0.015 * wobble(q) - abs(x)) / 0.02) * smooth((z - 1.24) / 0.03) * smooth((-0.62 - y) / 0.06) * smooth((y + 1.2) / 0.05)
    # The dewlap and the brisket, and the underline back to the udder.
    front = smooth((-0.80 - y) / 0.08) * smooth((1.06 + 0.03 * wobble(q) - z) / 0.04) * smooth((0.13 - abs(x)) / 0.04)
    brisket = smooth((-0.60 - y) / 0.06) * smooth((0.82 + 0.03 * wobble(q) - z) / 0.04) * smooth((0.16 - abs(x)) / 0.05)
    belly = smooth((0.70 + 0.04 * wobble(q, 0.7) - z) / 0.04) * smooth((0.55 - y) / 0.08) * smooth((-n[2] - 0.2) / 0.3)
    # The legs below the knees and the hocks.
    knee = 0.38 if y < 0.1 else 0.46
    legs = smooth((knee + 0.03 * wobble(q, 1.3) - z) / 0.03) if z < 0.6 else 0.0
    return max(w, crest, front, brisket, belly, legs)


def roan_white(q, n):
    """The share of white hairs in a Shorthorn roan at ``q``: about half
    over the body, so the fine mix of dark red and white hairs reads grey
    at a distance (as in the photographs), whiter under the belly, with
    faint broader patches; the head, the neck and the lower legs darker
    and nearly solid."""
    x, y, z = q
    base = 0.50 + 0.06 * smooth((0.9 - z) / 0.5) + 0.10 * smooth((-n[2] - 0.3) / 0.5)
    base -= 0.50 * smooth((-0.75 - y) / 0.25) * smooth((z - 0.85) / 0.2)
    base -= 0.45 * smooth((0.45 - z) / 0.1)
    blotch = 0.09 * math.sin(x * 23.0 + y * 7.0 + 0.4) * math.sin(y * 19.0 - z * 13.0 + 0.8)
    fleck = 0.05 * math.sin(x * 61.0 + y * 13.0) * math.sin(y * 57.0 - z * 29.0 + 0.8) + 0.03 * math.sin(z * 83.0 + x * 37.0)
    share = base + blotch + fleck
    return max(0.03, min(0.97, smooth((share - 0.5) / 0.5 + 0.5)))


def color_at(coat, p, n):
    """The coat's colour at ``p`` facing ``n`` (body frame); sRGB 0..255."""
    return coat_at(coat, from_body(p), from_body(n))


def coat_at(coat, q, m, roan=None):
    """The coat's colour at the model point ``q`` facing ``m`` (Blender
    metres, legs shortened); sRGB 0..255. ``roan``: the share of white hairs
    there from the hair-level map (``bake_roan``), instead of the smooth
    ``roan_white``."""
    c = COATS[coat]
    p, q = to_body(q), unwarp(q)
    body = c["body"]

    def mix(a, b, f):
        return tuple(round(a[i] + (b[i] - a[i]) * f) for i in range(3))

    if coat == "hereford":
        hair = mix(body, c["white"], hereford_white(q, m))
    elif coat == "roan":
        hair = mix(body, c["white"], roan_white(q, m) if roan is None else roan)
    elif coat == "hosco":
        hair = mix(body, c["white"], hosco_dark(q))
    elif coat == "overo":
        t, u, s = head_coords(q)
        black = q[2] < 0.42 or q[1] < -1.0 and q[2] > 0.9 and t > -0.02 or overo_black(p)
        hair = (26, 24, 22) if black else body
    else:
        hair = body
    # The udder is bare skin, never the coat: its haired edge fades into it.
    return mix(hair, c["skin"], udder_skin(q))


def to_linear(c):
    """sRGB 0..1 to linear (numpy arrays)."""
    import numpy as np

    return np.where(c <= 0.04045, c / 12.92, ((c + 0.055) / 1.055) ** 2.4)


def to_srgb(c):
    import numpy as np

    c = np.clip(c, 0.0, 1.0)
    return np.where(c <= 0.0031308, c * 12.92, 1.055 * c ** (1 / 2.4) - 0.055)


def coat_pixels(coat, lum, pos, nrm, roan=None):
    """The colour map the game draws ``Main`` with for ``coat``: the coat
    (``coat_at``) at each texel's model point times the neutral luminance
    ``lum``, as the game multiplies them (in linear light). ``lum``, ``pos``
    and ``nrm`` are (h, w, 3) arrays over the body's UVs: the sRGB
    luminance 0..1, the point and the normal there (``bake_geometry``; a
    texel no face covers has a normal far from unit length and takes the
    coat's body colour); ``roan`` the (h, w) white share of the roan's
    hair-level map. Returns (h, w, 3) sRGB 0..1. Baked once per coat, so the
    game needs no port of ``color_at``, ``warp`` or ``unwarp``, and the
    edges of the Hereford's white and the roan's hairs are as sharp as the
    map."""
    import numpy as np

    length = np.linalg.norm(nrm, axis=2)
    covered = (length > 0.5) & (length < 1.5)
    rgb = np.empty(pos.shape, np.float64)
    rgb[:] = np.array(COATS[coat]["body"]) / 255.0
    shares = [None] * int(covered.sum()) if roan is None else roan[covered].tolist()
    rgb[covered] = np.array(
        [coat_at(coat, q, m, w) for q, m, w in zip(pos[covered].tolist(), nrm[covered].tolist(), shares)]
    ).reshape(-1, 3) / 255.0
    return to_srgb(to_linear(rgb) * to_linear(lum))


def coat_part(coat, part):
    """The flat colour the game gives the one-colour parts of ``coat``."""
    c = COATS[coat]
    return {"Hair": c["hair"], "Main_Light": c["skin"], "Muzzle": c["muzzle"], "Hooves": c["hoof"],
            "Horns": c["horns"]}.get(part)


# ---------------------------------------------------------------------------
# The gaits (``horse.py``'s plans): seconds, duty factor, stance sweep (m),
# the phase each leg lands, the swing lift (fore, hind) and the claws' fold.
# A cow walks at about 1.1 m/s in a lateral four-beat walk; it gallops at
# about 6 m/s, over two strides a second on its short legs, the head low and
# the tail up. The sweeps fit the shortened legs (no standing claw falls
# short of its mark).

WALK = dict(seconds=50 / 48, duty=(0.56, 0.64), sweep=(0.65, 0.74), phase=dict(LH=0.0, LF=0.25, RH=0.5, RF=0.75),
            lift=(0.11, 0.09), fold=(70.0, 55.0), centre=dict(F=0.08, H=-0.04), strike=6.0, breakover=35.0, sling=1.0)
GALLOP = dict(seconds=21 / 48, duty=(0.29, 0.33), sweep=(0.74, 0.84), phase=dict(RH=0.0, LH=0.10, RF=0.42, LF=0.54),
              lift=(0.26, 0.14), fold=(60.0, 50.0), centre=dict(F=-0.04, H=-0.10), strike=8.0, breakover=45.0,
              reach=0.05, hip=0.6, hip_swing=0.72, hind_lead=0.8, hind_peak=0.6, fore_peak=1.3, sling=1.0)


def ground_speed(gait):
    """The speed the standing claws slide back at, m/s (fore legs)."""
    duty = gait["duty"][0] if isinstance(gait["duty"], tuple) else gait["duty"]
    sweep = gait["sweep"][0] if isinstance(gait["sweep"], tuple) else gait["sweep"]
    return sweep / (duty * gait["seconds"])


# ---------------------------------------------------------------------------
# The skeleton: the Quaternius quadruped's names and parents (the clips of
# ``horse.py`` drive them by name), set on a cow's proportions. Name, parent,
# head (Blender metres, facing -Y); a bone points at its first child (or at
# ``TAILS``). Every bone deforms.

BONES = [
    ("Body", None, (0.0, 0.0, 0.30)),
    ("Back", "Body", (0.0, 0.70, 1.22)),
    ("Torso", "Back", (0.0, 0.40, 1.10)),
    ("Torso2", "Torso", (0.0, 0.02, 1.00)),
    ("Torso3", "Torso2", (0.0, -0.28, 1.00)),
    ("Neck1", "Torso3", (0.0, -0.60, 1.03)),
    ("Neck2", "Neck1", (0.0, -0.80, 1.09)),
    ("Neck3", "Neck2", (0.0, -1.00, 1.15)),
    ("Head", "Neck3", (0.0, -1.22, 1.22)),
    ("FrontShoulder.L", "Torso2", (0.13, -0.60, 0.86)),
    ("FrontUpperLeg.L", "FrontShoulder.L", (0.19, -0.60, 0.84)),
    ("FrontLowerLeg.L", "FrontUpperLeg.L", (0.19, -0.635, 0.385)),
    ("FrontShoulder.R", "Torso2", (-0.13, -0.60, 0.86)),
    ("FrontUpperLeg.R", "FrontShoulder.R", (-0.19, -0.60, 0.84)),
    ("FrontLowerLeg.R", "FrontUpperLeg.R", (-0.19, -0.635, 0.385)),
    ("BackShoulder.L", "Back", (0.08, 0.60, 1.02)),
    ("BackLeg.L", "BackShoulder.L", (0.18, 0.60, 1.02)),
    ("BackUpperLeg.L", "BackLeg.L", (0.18, 0.50, 0.70)),
    ("BackLowerLeg.L", "BackUpperLeg.L", (0.18, 0.80, 0.47)),
    ("BackShoulder.R", "Back", (-0.08, 0.60, 1.02)),
    ("BackLeg.R", "BackShoulder.R", (-0.18, 0.60, 1.02)),
    ("BackUpperLeg.R", "BackLeg.R", (-0.18, 0.50, 0.70)),
    ("BackLowerLeg.R", "BackUpperLeg.R", (-0.18, 0.80, 0.47)),
    ("Tail1", "Back", (0.0, 0.78, 1.30)),
    ("Tail2", "Tail1", (0.0, 0.85, 1.29)),
    ("Tail3", "Tail2", (0.0, 0.905, 1.20)),
    ("Tail4", "Tail3", (0.0, 0.93, 1.08)),
    ("Tail5", "Tail4", (0.0, 0.938, 0.96)),
    ("Tail6", "Tail5", (0.0, 0.94, 0.84)),
    ("Tail7", "Tail6", (0.0, 0.94, 0.73)),
    ("PoleTargetBack.L", "Body", (0.18, -1.60, 0.30)),
    ("PoleTarget.L", "Body", (0.19, -1.45, 0.30)),
    ("PoleTargetBack.R", "Body", (-0.18, -1.60, 0.30)),
    ("PoleTarget.R", "Body", (-0.19, -1.45, 0.30)),
    ("IKBackLeg.L", None, (0.18, 0.772, 0.125)),
    ("FFB.L", "IKBackLeg.L", (0.18, 0.745, 0.045)),
    ("IKFrontLeg.L", None, (0.19, -0.628, 0.125)),
    ("FF.L", "IKFrontLeg.L", (0.19, -0.655, 0.045)),
    ("IKBackLeg.R", None, (-0.18, 0.772, 0.125)),
    ("FFB.R", "IKBackLeg.R", (-0.18, 0.745, 0.045)),
    ("IKFrontLeg.R", None, (-0.19, -0.628, 0.125)),
    ("FF.R", "IKFrontLeg.R", (-0.19, -0.655, 0.045)),
]

# Where the leaves point (and the ``_end`` nodes, as in the original).
ENDS = [
    ("FrontLowerLeg.L_end", "FrontLowerLeg.L", (0.19, -0.628, 0.125)),
    ("FrontLowerLeg.R_end", "FrontLowerLeg.R", (-0.19, -0.628, 0.125)),
    ("BackLowerLeg.L_end", "BackLowerLeg.L", (0.18, 0.772, 0.125)),
    ("BackLowerLeg.R_end", "BackLowerLeg.R", (-0.18, 0.772, 0.125)),
    ("Tail7_end", "Tail7", (0.0, 0.938, 0.62)),
    ("PoleTargetBack.L_end", "PoleTargetBack.L", (0.18, -1.60, 0.50)),
    ("PoleTarget.L_end", "PoleTarget.L", (0.19, -1.45, 0.50)),
    ("PoleTargetBack.R_end", "PoleTargetBack.R", (-0.18, -1.60, 0.50)),
    ("PoleTarget.R_end", "PoleTarget.R", (-0.19, -1.45, 0.50)),
    ("FFB.L_end", "FFB.L", (0.18, 0.725, 0.0)),
    ("FF.L_end", "FF.L", (0.19, -0.675, 0.0)),
    ("FFB.R_end", "FFB.R", (-0.18, 0.725, 0.0)),
    ("FF.R_end", "FF.R", (-0.19, -0.675, 0.0)),
]
TAILS = {"Head": _at(0.45, -0.04), "Body": (0.0, 0.0, 0.5)}

EYE = (0.165, -0.005, 0.104)  # head frame (t, u, |s|) of the eyeball centre
EYE_RADIUS = 0.022

# Horns: a path (base buried in the poll, out to the tip) and radii per
# point, the left one (the right mirrors it). A Hereford's or a
# Shorthorn's: out from the poll, then forward and up, the tip turned in.
HORNS = {
    "cow": ([(0.06, -1.275, 1.285), (0.13, -1.285, 1.30), (0.20, -1.31, 1.315), (0.255, -1.355, 1.345), (0.275, -1.41, 1.39), (0.262, -1.45, 1.43)],
            [0.040, 0.034, 0.029, 0.022, 0.014, 0.004]),
    "bull": ([(0.06, -1.27, 1.285), (0.14, -1.28, 1.30), (0.21, -1.31, 1.315), (0.25, -1.36, 1.335), (0.255, -1.405, 1.36)],
             [0.055, 0.050, 0.040, 0.026, 0.006]),
    # The criollo ox's long lyre horns: out, forward, then up.
    "ox": ([(0.06, -1.27, 1.285), (0.16, -1.285, 1.305), (0.27, -1.31, 1.34), (0.355, -1.35, 1.40), (0.40, -1.37, 1.48),
            (0.405, -1.355, 1.565)],
           [0.050, 0.046, 0.038, 0.029, 0.018, 0.005]),
}


def horn_key():
    return "ox" if OX else "bull" if BULL else "cow"



# ---------------------------------------------------------------------------
# Everything below runs in Blender.

if bpy is not None:
    from mathutils import Matrix, Quaternion, Vector

    sys.path.insert(0, HERE)
    import animal_rig as rig  # noqa: E402

    H = rig.Steps(os.path.join(HERE, "horse.py"), prefix="cattle:")



def at(t, u=0.0, s=0.0):
    return Vector(_at(t, u, s))


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
        elif name in first and (heads[first[name]] - h).length > 0.02:
            tail = heads[first[name]]
        elif name in ends:
            tail = ends[name]
        else:
            tail = h + Vector((0.0, 0.0, 0.05))
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


def extra_bones():
    """The bones the Quaternius cattle lack: the ears, the face, the longer
    tail, and the helpers (``muscles``)."""
    out = []
    for side, sx in (("L", 1.0), ("R", -1.0)):
        base = Vector(_at(0.04, -0.075, sx * 0.105))
        out.append((f"Ear1.{side}", "Head", tuple(base), (sx * 0.30, -1.225, 1.205)))
    out += [
        ("Jaw", "Head", tuple(at(0.10, -0.13)), tuple(at(0.44, -0.13))),
        ("LipUpper", "Head", tuple(at(0.48, -0.07)), tuple(at(0.55, -0.08))),
        ("Nostril.L", "Head", tuple(at(0.50, -0.03, 0.035)), tuple(at(0.51, -0.03, 0.075))),
        ("Nostril.R", "Head", tuple(at(0.50, -0.03, -0.035)), tuple(at(0.51, -0.03, -0.075))),
        ("Tail8", "Tail7", (0.0, 0.938, 0.62), (0.0, 0.935, 0.52)),
        ("Tail9", "Tail8", (0.0, 0.935, 0.52), (0.0, 0.932, 0.43)),
    ]
    return out + [(m["name"], m["parent"], m["head"], m["tail"]) for m in muscles()]


def muscles():
    """Muscle helpers (``horse.py``'s ``MUSCLES``: their names drive
    ``muscle_shape``) on a cow's legs, and the soft masses that swing of
    their own (the dewlap, the udder or the scrotum): each takes over its
    sources' weight inside its region (``weight_muscles``)."""
    out = []
    for s, x in (("L", 1.0), ("R", -1.0)):
        out += [
            dict(name=f"Quarter.{s}", parent=f"BackLeg.{s}", head=(x * 0.10, 0.70, 0.98), tail=(x * 0.12, 0.76, 0.78),
                 centre=(x * 0.17, 0.72, 1.00), radii=(0.14, 0.22, 0.22), side=0.08,
                 steal=[f"BackLeg.{s}", "Tail1", "Tail2", "Tail3"]),
            dict(name=f"Gaskin.{s}", parent=f"BackUpperLeg.{s}", head=(x * 0.18, 0.64, 0.62), tail=(x * 0.18, 0.77, 0.52),
                 centre=(x * 0.18, 0.68, 0.62), radii=(0.11, 0.15, 0.15)),
            dict(name=f"Scapula.{s}", parent=f"FrontShoulder.{s}", head=(x * 0.12, -0.52, 0.98), tail=(x * 0.12, -0.46, 0.80),
                 centre=(x * 0.19, -0.55, 0.96), radii=(0.14, 0.19, 0.24), steal=[f"FrontUpperLeg.{s}", "Torso2"]),
        ]
    deep = 1.3 if BULL else 1.0
    out.append(dict(name="Dewlap", parent="Neck1", head=(0.0, -0.86, 0.86), tail=(0.0, -0.86, 0.66),
                    centre=(0.0, -0.90, 0.76), radii=(0.10, 0.26, 0.17 * deep), side=-1.0,
                    steal=["Neck1", "Neck2", "Neck3", "Torso3"]))
    if BULL:
        out.append(dict(name="Scrotum", parent="Back", head=(0.0, 0.63, 0.67), tail=(0.0, 0.63, 0.49),
                        centre=(0.0, 0.63, 0.56), radii=(0.075, 0.075, 0.12), side=-1.0,
                        steal=["Back", "BackLeg.L", "BackLeg.R", "Torso"]))
    else:
        out.append(dict(name="Udder", parent="Back", head=(0.0, 0.50, 0.74), tail=(0.0, 0.50, 0.50),
                        centre=(0.0, 0.50, 0.59), radii=(0.16, 0.18, 0.15), side=-1.0,
                        steal=["Back", "BackLeg.L", "BackLeg.R", "Torso", "Torso2"]))
    return out


# ---------------------------------------------------------------------------
# The sculpt.

BOUNDS = ((-0.44, -1.72, -0.01), (0.44, 1.30, 1.62))


def rot_x(deg):
    return Matrix.Rotation(math.radians(deg), 3, "X")


def bulk(cow, bull):
    return bull if BULL else cow


def claws(sdf, x, heel_y, toe_y, sx, fore):
    """A cloven foot: two claws side by side, each a horn wall sloping
    forward to a pointed toe, flat on its inner face along the cleft, with
    a soft bulb at the heel and a flat sole; the coronet band where the
    hair starts; and the two dewclaws, small horny points behind the
    fetlock. Sized for beef cattle: each claw about 10 cm long and 5 cm
    wide, the foot 11 cm across."""
    E = rig.sdf_ellipsoid
    mid = (heel_y + toe_y) / 2
    back = 1.0 if toe_y < heel_y else -1.0  # +1: the toe is toward -Y
    L = bulk(1.0, 1.08)
    for side in (1.0, -1.0):
        cx = x + side * 0.027 * L
        # The wall, in three layers stepping back as they rise: the toe's
        # face slopes at about 50 degrees, the heel is low and soft.
        sdf.add(E((cx, mid - back * 0.004, 0.013), (0.026 * L, 0.055 * L, 0.02)), 0.008)
        sdf.add(E((cx, mid + back * 0.01, 0.036), (0.025 * L, 0.046 * L, 0.025)), 0.012)
        sdf.add(E((cx, mid + back * 0.024, 0.058), (0.023 * L, 0.035 * L, 0.018)), 0.012)
        sdf.add(E((cx - side * 0.004, toe_y + back * 0.016, 0.014), (0.017 * L, 0.024 * L, 0.016)), 0.01)  # the toe's point
        sdf.add(E((cx, heel_y - back * 0.006, 0.02), (0.024 * L, 0.022, 0.022)), 0.01)  # the bulb of the heel
        sdf.add(E((cx, mid + back * 0.026, 0.071), (0.027 * L, 0.04 * L, 0.012)), 0.01)  # the coronet's bulge
        # A dewclaw: a small horny point behind the fetlock, pointing down.
        sdf.add(rig.sdf_round_cone((x + side * 0.026, heel_y + back * 0.022, 0.12), (x + side * 0.03, heel_y + back * 0.03, 0.093), 0.014, 0.008), 0.006)
    # The cleft between the claws, open at the toe, and the flat sole.
    sdf.carve(E((x, mid - back * 0.012, 0.03), (0.0045, 0.07, 0.055)), 0.002)
    lo, hi = (x - 0.1, mid - 0.12, -0.1), (x + 0.1, mid + 0.12, 0.0)
    sdf.carve((lambda X, Y, Z: Z + 0.0 * X + 0.0 * Y, lo, hi), 0.004)


def legs(sdf, sx):
    """One side's legs, from the shoulder and the haunch down to the
    claws: thick columns of bone, tendon and muscle, as in beef cattle (a
    bull's thicker still)."""
    E, R, T = rig.sdf_ellipsoid, rig.sdf_round_cone, rig.sdf_tube
    k = bulk(1.18, 1.38)  # the muscle masses
    L = bulk(1.22, 1.38)  # the bones and the tendons below the knee and the hock
    x = 0.19 * sx
    # Fore: the shoulder blade, the point of the shoulder, the arm and the
    # triceps over the elbow, the forearm, the knee, a short cannon with its
    # tendons, the fetlock, a short pastern and the claws.
    sdf.add(E((sx * 0.20, -0.50, 1.12), (0.07 * k, 0.14, 0.25), rot_x(-28)), 0.09)
    sdf.add(E((sx * 0.175, -0.73, 0.94), (0.06 * k, 0.06, 0.07)), 0.06)
    sdf.add(E((sx * 0.20, -0.58, 0.84), (0.09 * k, 0.13, 0.15), rot_x(-15)), 0.08)
    sdf.add(E((sx * 0.20, -0.47, 0.82), (0.07 * k, 0.09, 0.11), rot_x(-25)), 0.05)  # long head of the triceps
    sdf.add(E((sx * 0.195, -0.52, 0.71), (0.05 * k, 0.055, 0.06)), 0.05)  # elbow
    sdf.add(R((x, -0.585, 0.77), (x, -0.628, 0.44), 0.088 * k, 0.05 * L), 0.05)
    sdf.add(E((x, -0.60, 0.62), (0.072 * k, 0.078 * k, 0.15), rot_x(-4)), 0.05)
    sdf.add(E((x, -0.635, 0.40), (0.052 * L, 0.054 * L, 0.06)), 0.03)  # knee
    sdf.add(T([(x, -0.636, 0.37), (x, -0.633, 0.26), (x, -0.628, 0.17)], [(0.033 * L, 0.041 * L), (0.03 * L, 0.037 * L), (0.034 * L, 0.042 * L)]), 0.02)
    sdf.add(R((x, -0.60, 0.35), (x, -0.592, 0.18), 0.017 * L, 0.02 * L), 0.02)  # flexor tendons
    sdf.add(E((x, -0.622, 0.13), (0.048 * L, 0.053 * L, 0.046)), 0.03)  # fetlock
    sdf.add(R((x, -0.626, 0.12), (x, -0.646, 0.07), 0.042 * L, 0.048 * L), 0.02)  # pastern
    claws(sdf, x, -0.598, -0.705, sx, True)
    # Hind: the thigh and the round of beef behind it down to the hock, the
    # stifle, the gaskin, the hock with its point, the cannon, the fetlock,
    # the pastern and the claws.
    x = 0.18 * sx
    sdf.add(E((sx * 0.17, 0.62, 1.00), (0.13 * k, 0.24, 0.28), rot_x(15)), 0.08)
    sdf.add(R((sx * 0.11, 0.80, 1.16), (sx * 0.14, 0.76, 0.78), 0.10 * k, 0.075 * k), 0.07)  # the round
    sdf.add(E((sx * 0.20, 0.49, 0.75), (0.045, 0.06, 0.06)), 0.08)  # stifle
    sdf.add(R((sx * 0.19, 0.62, 0.72), (x, 0.785, 0.50), 0.088 * k, 0.05 * L), 0.05)  # gaskin
    sdf.add(E((x, 0.80, 0.475), (0.045 * L, 0.064 * L, 0.06)), 0.03)  # hock
    sdf.add(R((x, 0.78, 0.64), (x, 0.85, 0.50), 0.032 * L, 0.026 * L), 0.03)  # tendon to the point of the hock
    sdf.add(T([(x, 0.80, 0.45), (x, 0.788, 0.30), (x, 0.778, 0.17)], [(0.033 * L, 0.044 * L), (0.03 * L, 0.039 * L), (0.034 * L, 0.042 * L)]), 0.02)
    sdf.add(R((x, 0.825, 0.43), (x, 0.808, 0.18), 0.017 * L, 0.02 * L), 0.02)
    sdf.add(E((x, 0.776, 0.13), (0.047 * L, 0.053 * L, 0.046)), 0.03)
    sdf.add(R((x, 0.772, 0.12), (x, 0.752, 0.07), 0.041 * L, 0.047 * L), 0.02)
    claws(sdf, x, 0.800, 0.695, sx, False)


def trunk(sdf):
    E, R, T = rig.sdf_ellipsoid, rig.sdf_round_cone, rig.sdf_tube
    w = bulk(1.04, 1.12)
    sdf.add(T(
        [(0, -0.74, 0.98), (0, -0.52, 1.00), (0, -0.20, 0.99), (0, 0.10, 1.00), (0, 0.40, 1.04), (0, 0.66, 1.08), (0, 0.84, 1.10)],
        [(0.18 * w, 0.31), (0.27 * w, 0.40), (0.32 * w, 0.42), (0.325 * w, 0.42), (0.29 * w, 0.36), (0.25 * w, 0.28), (0.17 * w, 0.19)],
    ))
    sdf.add(E((0, 0.05, 0.78), (0.30 * w, 0.50, bulk(0.25, 0.21))), 0.12)  # belly
    sdf.add(E((0.10, 0.10, 0.90), (0.20, 0.34, 0.24)), 0.10)  # the rumen, on the left
    sdf.add(E((0, 0.52, 1.24), (0.25 * w, 0.32, 0.14)), 0.10)  # loin and rump
    sdf.add(E((0, -0.42, 1.30), (0.08, 0.24, 0.09)), 0.08)  # withers
    sdf.add(E((0, -0.75, 0.68), (0.15 * w, 0.13, 0.15)), 0.09)  # brisket
    for sx in (1, -1):
        sdf.add(E((sx * 0.22, 0.44, 1.25), (0.05, 0.06, 0.045)), 0.06)  # hook
        sdf.add(E((sx * 0.10, 0.86, 1.21), (0.035, 0.04, 0.04)), 0.04)  # pin
        sdf.add(E((sx * 0.08, -0.72, 0.86), (0.07, 0.06, 0.10)), 0.06)  # pectoral
    sdf.add(T([(0, 0.70, 1.33), (0, 0.80, 1.335), (0, 0.86, 1.29)], [(0.05, 0.045), (0.045, 0.04), (0.035, 0.035)]), 0.05)  # tailhead
    # The tail, hanging behind the round between the pins.
    sdf.add(T([(0, 0.86, 1.29), (0, 0.91, 1.18), (0, 0.935, 1.02), (0, 0.94, 0.82), (0, 0.937, 0.62), (0, 0.932, 0.45)],
              [(0.032, 0.03), (0.025, 0.024), (0.02, 0.019), (0.017, 0.016), (0.015, 0.014), (0.014, 0.013)]), 0.02)
    # The neck, short and deep, its top line running straight to the poll.
    n = bulk(1.08, 1.32)
    sdf.add(T(
        [(0, -0.50, 1.05), (0, -0.78, 1.07), (0, -1.00, 1.12), (0, -1.17, 1.17)],
        [(0.22 * n, 0.36), (0.19 * n, 0.29 * n), (0.155 * n, 0.22 * n), (0.12, 0.15)],
    ), 0.10)
    sdf.add(T([(0, -0.40, 1.33), (0, -0.70, 1.29), (0, -0.98, 1.27), (0, -1.16, 1.28)],
              [(0.08, 0.05), (0.075, 0.05), (0.065, 0.045), (0.06, 0.04)]), 0.08)  # top of the neck
    if BULL:
        if not OX:
            # The crest: a hump of muscle over the neck and the withers.
            sdf.add(E((0, -0.82, 1.32), (0.17, 0.34, 0.17), rot_x(-8)), 0.13)
            sdf.add(E((0, -0.56, 1.38), (0.14, 0.26, 0.10)), 0.11)
            sdf.add(E((0, -1.02, 1.24), (0.13, 0.16, 0.12), rot_x(-20)), 0.10)  # the crest up to the poll
        # The sheath with its opening, and the scrotum between the hinds.
        sdf.add(R((0, 0.10, 0.64), (0, -0.10, 0.55), 0.045, 0.034), 0.06)
        sdf.carve(E((0, -0.115, 0.545), (0.01, 0.012, 0.01)), 0.004)
    if BULL and not OX:
        # (30 % smaller than a first try, at the user's request.)
        sdf.add(R((0, 0.62, 0.70), (0, 0.632, 0.56), 0.034, 0.053), 0.035)
        for sx in (1, -1):
            sdf.add(E((sx * 0.025, 0.632, 0.535), (0.035, 0.039, 0.053)), 0.025)
        sdf.groove([(0, 0.628, 0.59), (0, 0.632, 0.52), (0, 0.63, 0.485)], (0, 1, 0), 0.008, 0.003, 0.005)
    elif not BULL:
        # The udder: four quarters under the body between the thighs, each
        # with its teat, and the navel.
        sdf.add(E(*UDDER[0]), 0.10)
        for c, r in UDDER[1:5]:
            sdf.add(E(c, r), 0.06)
            sdf.add(R((c[0], c[1] - 0.01, 0.53), (c[0] * 1.04, c[1] - 0.013, 0.415), 0.016, 0.012), 0.012)
        sdf.add(E((0, -0.05, 0.60), (0.03, 0.04, 0.03)), 0.04)


def dewlap(sdf):
    """The dewlap: a fold of skin under the neck from the throat to the
    brisket, thin across, its edge loose."""
    E = rig.sdf_ellipsoid
    deep = bulk(1.1, 1.5)
    sdf.add(E((0, -0.92, 0.82 - 0.03 * (deep - 1)), (0.035, 0.21, 0.14 * deep), rot_x(-35)), 0.06)
    sdf.add(E((0, -0.78, 0.72 - 0.03 * (deep - 1)), (0.035, 0.12, 0.10 * deep), rot_x(-15)), 0.06)
    sdf.add(E((0, -1.07, 0.95), (0.03, 0.10, 0.07), rot_x(-45)), 0.05)


def head(sdf):
    E, T = rig.sdf_ellipsoid, rig.sdf_tube
    Hx = Matrix((Vector((1, 0, 0)), Vector(HEAD_A), Vector(HEAD_U))).transposed()
    b = bulk(1.2, 1.34)  # a broad, heavy beef head
    sdf.add(E(at(0.07, -0.01), (0.105 * b, 0.12, 0.10 * b), Hx), 0.06)  # cranium
    sdf.add(E(at(0.0, 0.0), (0.10 * b, 0.035, 0.045), Hx), 0.04)  # the poll ridge
    sdf.add(T([at(0.06, 0.01), at(0.25, -0.005), at(0.42, -0.03)], [(0.11 * b, 0.075), (0.095 * b, 0.07), (0.085, 0.064)]), 0.06)
    sdf.add(T([at(0.12, -0.09), at(0.30, -0.085), at(0.45, -0.08)], [(0.085 * b, 0.07), (0.07, 0.05), (0.07, 0.045)]), 0.05)  # lower face
    for sx in (1, -1):
        sdf.add(E(at(0.20, -0.12, sx * 0.075 * b), (0.05, 0.12, 0.10), Hx), 0.05)  # masseter
        sdf.add(E(at(EYE[0], EYE[1] + 0.012, sx * (EYE[2] - 0.004)), (0.03, 0.045, 0.038), Hx), 0.02)  # orbit
        sdf.add(T([at(0.12, -0.17, sx * 0.05), at(0.30, -0.14, sx * 0.04), at(0.46, -0.12, sx * 0.03)], [(0.02, 0.024), (0.018, 0.02), (0.018, 0.02)], side=(1, 0, 0)), 0.03)  # jaw edge
    sdf.add(E(at(0.48, -0.06), (0.095, 0.075, 0.08), Hx), 0.05)  # muzzle
    sdf.add(E(at(0.52, -0.06), (0.08, 0.035, 0.065), Hx), 0.03)  # the nose pad's flat front
    sdf.add(E(at(0.50, -0.115), (0.07, 0.05, 0.03), Hx), 0.03)  # upper lip
    sdf.add(E(at(0.47, -0.145), (0.05, 0.05, 0.025), Hx), 0.025)  # lower lip and chin
    sdf.add(E(at(0.22, -0.16), (0.055, 0.12, 0.06), Hx), 0.05)  # between the jaws
    mouth = E(at(0.50, -0.132, 0.0), (0.065, 0.07, 0.004), Hx)
    sdf.carve(mouth, 0.005)
    OPENINGS.append(mouth[0])
    for sx in (1, -1):
        nostril = E(at(0.535, -0.045, sx * 0.042), (0.012, 0.018, 0.026), Hx)
        sdf.carve(nostril, 0.006)
        OPENINGS.append(nostril[0])
        t, u, s = EYE
        sdf.add(E(at(t, u, sx * (s - 0.002)), (0.021, 0.034, 0.028), Hx), 0.01)  # lids
        sdf.carve(E(at(t, u, sx * (s + 0.013)), (0.024, 0.028, 0.015), Hx), 0.004)  # opening
        ring = [at(t + 0.030 * math.cos(a), u + 0.017 * math.sin(a), sx * (s + 0.003)) for a in (math.radians(d) for d in range(0, 361, 30))]
        sdf.add(rig.sdf_tube(ring, [(0.003, 0.003)] * len(ring), side=(1, 0, 0)), 0.003)
        ears(sdf, sx)


CAVITIES = []
OPENINGS = []


def ears(sdf, sx):
    """A horizontal ear: out from below the horn, broad and cupped, opening
    forward and down."""
    T = rig.sdf_tube
    base = Vector(_at(0.04, -0.075, sx * 0.11))
    mid = Vector((sx * 0.22, -1.235, 1.212))
    tip = Vector((sx * 0.33, -1.225, 1.195))
    sdf.add(T([base, mid, tip], [(0.034, 0.03), (0.024, 0.07), (0.007, 0.014)], side=(0, 1, 0.2)), 0.03)
    f = Vector((0.0, -0.013, -0.004))
    cavity = T([base + f + Vector((sx * 0.035, 0, 0)), mid + f, tip + f * 0.6 - Vector((sx * 0.014, 0, 0))],
               [(0.02, 0.018), (0.017, 0.057), (0.003, 0.009)], side=(0, 1, 0.2))
    sdf.carve(cavity, 0.005)
    CAVITIES.append(cavity[0])


def horns_sculpt(voxel):
    """The horns: surfaces of their own (not fused to the head), so the game
    can hide them (a polled Angus) and leave the poll whole. One per side
    (``Sdf.to_mesh`` keeps a field's largest piece)."""
    path, radii = HORNS[horn_key()]
    out = []
    for sx in (1, -1):
        lo, hi = (-0.36, -1.56, 1.18), (0.36, -1.18, 1.52)
        lo, hi = ((0.0, lo[1], lo[2]), hi) if sx > 0 else (lo, (0.0, hi[1], hi[2]))
        sdf = rig.Sdf(lo, hi, voxel)
        pts = [(sx * p[0], p[1], p[2]) for p in path]
        sdf.add(rig.sdf_tube(pts, [(r, r) for r in radii], side=(0, 1, 0)))
        # Rough rings toward the base.
        for k in range(4):
            c = Vector(pts[0]).lerp(Vector(pts[1]), 0.5 + 0.18 * k)
            sdf.add(rig.sdf_ellipsoid(c, (0.006, radii[0] * 0.98, radii[0] * 0.98)), 0.006)
        out.append(sdf.to_mesh(f"Horn{sx}"))
    return out


def merge(into, parts):
    """Joins ``parts`` into the mesh ``into`` (no weights yet)."""
    bpy.ops.object.select_all(action="DESELECT")
    for o in [into] + list(parts):
        o.select_set(True)
    bpy.context.view_layer.objects.active = into
    bpy.ops.object.join()
    return into


def horn_distance(p):
    """How far ``p`` is from a horn's axis, less its radius there."""
    path, radii = HORNS[horn_key()]
    best = 1.0
    x = abs(p[0])
    for (a, ra), (b, rb) in zip(zip(path, radii), zip(path[1:], radii[1:])):
        a, b = Vector(a), Vector(b)
        q = Vector((x, p[1], p[2]))
        ab = b - a
        t = max(0.0, min(1.0, (q - a).dot(ab) / ab.length_squared))
        best = min(best, (q - (a + ab * t)).length - (ra + (rb - ra) * t))
    return best


def horn_tip(p):
    """0 at a horn's base to 1 at its tip."""
    path, _ = HORNS[horn_key()]
    q = Vector((abs(p[0]), p[1], p[2]))
    total = sum((Vector(b) - Vector(a)).length for a, b in zip(path, path[1:]))
    best, along, run = None, 0.0, 0.0
    for a, b in zip(path, path[1:]):
        a, b = Vector(a), Vector(b)
        ab = b - a
        t = max(0.0, min(1.0, (q - a).dot(ab) / ab.length_squared))
        d = (q - (a + ab * t)).length
        if best is None or d < best:
            best, along = d, run + ab.length * t
        run += ab.length
    return along / total


def sculpt(voxel):
    """The dense sculpt: the body (one watertight surface) and the horns."""
    CAVITIES.clear()
    OPENINGS.clear()
    sdf = rig.Sdf(*BOUNDS, voxel)
    trunk(sdf)
    dewlap(sdf)
    for sx in (1, -1):
        legs(sdf, sx)
    head(sdf)
    details(sdf)
    obj = sdf.to_mesh("CattleSculpt")
    merge(obj, horns_sculpt(voxel))
    warp_mesh(obj)
    return obj, sdf


def warp_mesh(obj):
    """Shortens the legs of a mesh laid out on the taller frame (``warp``)."""
    for v in obj.data.vertices:
        v.co = warp(v.co)
    obj.data.update()


def warped(spots):
    """``spots`` (hair strands laid out on the taller frame, against its
    sculpt) with their points on the model."""
    def fn(sdf):
        return [(tuple(Vector(warp(p)) for p in s[0]),) + tuple(s[1:]) for s in spots(sdf)]
    return fn


def details(sdf):
    """Surface anatomy: the spine of the shoulder blade, the last rib,
    the hollow of the flank before the hook, the seams of the round, the
    milk vein (cow), the wrinkles of the neck and the dewlap, the tendons
    of the cannons and the hair whorl's crest on the forehead."""
    for sx in (1, -1):
        out = (sx, 0, 0)
        sdf.ridge([(0, -0.44, 1.30), (0, -0.52, 1.16), (0, -0.60, 1.02)], out, 0.03, 0.002, 0.015)
        sdf.groove([(0, 0.24, 1.20), (0, 0.29, 1.05), (0, 0.32, 0.92)], out, 0.06, 0.0006, 0.004)  # the last rib
        sdf.groove([(sx * 0.30, 0.30, 1.22), (sx * 0.31, 0.32, 1.10), (sx * 0.30, 0.30, 1.00)], out, 0.07, 0.008, 0.02)  # flank hollow
        sdf.groove([(0, 0.82, 1.18), (0, 0.80, 1.00), (0, 0.76, 0.82)], (sx, 0.6, 0), 0.03, 0.004, 0.008)
        sdf.groove([(0, -0.40, 1.20), (0, -0.44, 1.02), (0, -0.48, 0.90)], out, 0.045, 0.0015, 0.012)
        for k in range(4):
            y = -0.92 - 0.06 * k
            sdf.groove([(0, y, 1.20 - 0.01 * k), (0, y - 0.02, 1.08), (0, y - 0.03, 0.98)], out, 0.012, 0.0025, 0.004)  # neck folds
        # Skin folds: the dewlap's hanging folds, the crease behind the
        # elbow, the fold of the flank from the stifle up to the belly.
        for k in range(3):
            y = -0.80 - 0.09 * k
            sdf.groove([(0, y, 0.95 - 0.04 * k), (0, y - 0.03, 0.80 - 0.03 * k), (0, y - 0.04, 0.70)], out, 0.01, 0.003, 0.004)
        sdf.groove([(sx * 0.20, -0.44, 0.86), (sx * 0.22, -0.42, 0.76), (sx * 0.21, -0.43, 0.68)], out, 0.014, 0.004, 0.006)
        sdf.ridge([(sx * 0.22, 0.44, 0.74), (sx * 0.24, 0.36, 0.80), (sx * 0.25, 0.28, 0.86)], (sx, 0, -0.3), 0.018, 0.006, 0.01)
        if not BULL:
            sdf.ridge([(sx * 0.17, -0.25, 0.70), (sx * 0.16, 0.0, 0.66), (sx * 0.13, 0.20, 0.64), (sx * 0.10, 0.35, 0.66)], (sx, 0, -0.5), 0.008, 0.004, 0.005)
        for x0 in (0.19, 0.18):
            x = x0 * sx
            yc = -0.63 if x0 == 0.19 else 0.79
            sdf.groove([(x, yc + 0.012, 0.36), (x, yc + 0.012, 0.26), (x, yc + 0.012, 0.18)], (sx, 0, 0), 0.008, 0.0018, 0.003)
            sdf.groove([(x, yc + 0.012, 0.36), (x, yc + 0.012, 0.26), (x, yc + 0.012, 0.18)], (-sx, 0, 0), 0.008, 0.0018, 0.003)


# ---------------------------------------------------------------------------
# Regions of the coat, from the rest position.


def near_leg(p):
    return min(abs(abs(p[0]) - 0.19), abs(abs(p[0]) - 0.18))


def is_claw(p):
    return p[2] < 0.07 and near_leg(p) < 0.09


def is_dewclaw(p):
    """The horn of a dewclaw (``claws``: behind each fetlock, two per
    foot), not the skin round it."""
    if not 0.08 < p[2] < 0.13:
        return False
    for x, y in ((0.19, -0.572), (0.18, 0.826)):
        for side in (1.0, -1.0):
            c = (math.copysign(x, p[0]) + side * 0.028, y, 0.104)
            if math.dist(p, c) < 0.017:
                return True
    return False


def is_horn(p):
    return abs(p[0]) > 0.075 and p[2] > 1.2 and p[1] < -1.15 and horn_distance(p) < 0.004


def is_hoof(p):
    """Keratin: the claws, the dewclaws and the horns (the bake gives them
    tubules and growth rings)."""
    return is_claw(p) or is_dewclaw(p) or is_horn(p)


def ear_cavity(p):
    import numpy as np

    if p[2] < 1.12 or p[2] > 1.32 or p[1] > -1.12 or abs(p[0]) < 0.08 or not CAVITIES:
        return 1.0
    a = np.array([[[p[0]]]]), np.array([[[p[1]]]]), np.array([[[p[2]]]])
    return float(min(fn(*a).ravel()[0] for fn in CAVITIES))


def opening(p):
    import numpy as np

    if p[2] > 1.1 or p[1] > -1.45 or not OPENINGS:
        return 1.0
    a = np.array([[[p[0]]]]), np.array([[[p[1]]]]), np.array([[[p[2]]]])
    return float(min(fn(*a).ravel()[0] for fn in OPENINGS))


def nose_pad(p, n=None):
    """The bare, moist nose pad (planum nasolabiale): the front of the
    muzzle from above the nostrils to the upper lip."""
    t, u, s = head_coords(p)
    if p[1] > -1.45:
        return 0.0
    return smooth((t - 0.49) / 0.015) * smooth((0.075 - abs(s)) / 0.012) * smooth((u + 0.122) / 0.01) * smooth((0.005 - u) / 0.012)


def muzzle_skin(p):
    """Bare skin: the nose pad, round the nostrils and the lips (the pores
    and the print of the pad in the bake), the udder's, its haired edge
    fading out."""
    if p[1] > -1.4:
        return smooth(udder_skin(p) / 0.7)
    return max(nose_pad(p), smooth((0.012 - opening(p)) / 0.012))


def is_bare(p):
    """The udder and its teats, bare skin (the bull's scrotum is haired,
    ``Main``)."""
    return udder_skin(p) > 0.5


def material_of(p, n):
    if is_horn(p):
        return "Horns"
    if is_claw(p) or is_dewclaw(p):
        return "Hooves"
    if p[1] < -1.4 and (opening(p) < 0.003 or nose_pad(p) > 0.5):
        return "Muzzle"
    if abs(p[0]) > 0.12 and p[1] < -1.12 and ear_cavity(p) < 0.004:
        return "Main_Light"  # the pale, thin-haired skin inside the ear
    if is_bare(p):
        return "Main_Light"
    return "Main"


MATERIALS = ["Main", "Main_Light", "Muzzle", "Hooves", "Horns", "Hair", "Eye_Black", "Eye_White"]
FALLBACK = {
    "Main": (0.49, 0.19, 0.09),
    "Main_Light": (0.84, 0.63, 0.56),
    "Muzzle": (0.80, 0.61, 0.53),
    "Hooves": (0.25, 0.21, 0.18),
    "Horns": (0.89, 0.83, 0.70),
    "Hair": (0.91, 0.89, 0.83),
    "Eye_Black": (0.09, 0.06, 0.04),
    "Eye_White": (0.78, 0.74, 0.68),
}


FOREHEAD_WHORL = _at(0.13, 0.08)
FLANK_WHORL = (0.26, 0.18, 1.02)


def hair_direction(p, n):
    """Which way the short coat lies: back along the body and down the
    sides, down the neck and the dewlap, down the legs, forward along the
    belly, out from the whorl on the forehead (the Hereford's curls), round
    the whorls on the flanks and up the back of the hind legs (the
    escutcheon)."""
    p, n = Vector(p), Vector(n)
    t, u, s = head_coords(p)
    if p.y < -1.15 and p.z > 1.0 and t > -0.05:
        r = p - Vector(FOREHEAD_WHORL)
        r -= n * r.dot(n)
        d = Vector(HEAD_A) if r.length > 0.09 else Vector(HEAD_A).lerp(r.normalized(), 1.0 - r.length / 0.09)
    elif p.y < -0.5 and p.z > 0.9:
        d = Vector((0.0, 0.35, -1.0))  # the neck and the dewlap
    elif p.z < 0.85 and near_leg(p) < 0.11:
        if p.y > 0.55 and abs(p.x) < 0.17:
            d = Vector((0.0, 0.0, 1.0))  # the escutcheon behind the udder
        else:
            d = Vector((0.0, 0.05, -1.0))
    elif p.y < -0.6:
        d = Vector((0.0, 0.15, -1.0))
    elif p.z < 0.8:
        d = Vector((0.0, -0.8, -0.3))
    elif p.y > 0.7:
        d = Vector((0.0, 0.35, -1.0))
    else:
        d = Vector((0.0, 1.0, -0.4))
        c = Vector((math.copysign(FLANK_WHORL[0], p.x), FLANK_WHORL[1], FLANK_WHORL[2]))
        r = p - c
        if r.length < 0.16:
            swirl = Vector((0.0, -r.z, r.y)) * math.copysign(1.0, p.x) + r * 0.4
            if swirl.length > 1e-6:
                d = d.normalized().lerp(swirl.normalized(), 1.0 - r.length / 0.16)
    d -= n * d.dot(n)
    return d.normalized() if d.length > 1e-6 else Vector((0.0, 1.0, 0.0))


def tone(p, n):
    """A coat-independent tone (multiplied with the coat): a darker topline,
    lighter flanks and belly, the creases, a dark eye ring and lid rims,
    the ear's cup, the horns' dark tips, and broad soft mottling."""
    p, n = Vector(p), Vector(n)
    t, u, s = head_coords(p)
    if is_horn(p):
        return max(0.25, 0.95 - 0.7 * smooth((horn_tip(p) - 0.55) / 0.4))
    v = 0.86
    # A darker line down the spine, the upper sides bleached by the sun,
    # the belly lighter.
    v -= 0.14 * smooth((n.z - 0.45) / 0.4) * smooth((p.z - 1.2) / 0.15) * smooth((0.12 - abs(p.x)) / 0.06)
    v += 0.06 * smooth((n.z - 0.2) / 0.4) * smooth((abs(p.x) - 0.12) / 0.06) * smooth((p.z - 1.0) / 0.2)
    if p.z > 0.7 and near_leg(p) > 0.06 or p.z > 1.0:
        v += 0.10 * smooth((-n.z - 0.05) / 0.5)
    if p.y < -1.2:
        for sx in (1, -1):
            e = (p - at(EYE[0], EYE[1], sx * EYE[2])).length
            v -= 0.18 * smooth((0.045 - e) / 0.02)
            v *= 1.0 - 0.45 * smooth((0.036 - e) / 0.008) * smooth((e - 0.022) / 0.005)
    if p.z > 1.12 and abs(p.x) > 0.1 and p.y < -1.12:
        c = ear_cavity(p)
        v -= 0.18 * smooth((0.014 - abs(c - 0.008)) / 0.008)
    if is_bare(p):
        v = 0.9
    # Broad soft mottling, faint on the head (on a white face it reads
    # as dirt).
    m = 0.3 if p.y < -1.05 and p.z > 0.85 else 1.0
    v += m * (0.06 * rig.noise(p, (3.0, 3.0, 3.0), seed=2.7) + 0.035 * rig.noise(p, (7.0, 7.0, 7.0), seed=3.9))
    v += m * 0.03 * rig.noise(p, (13.0, 13.0, 13.0), seed=5.1)
    return max(0.3, min(1.0, v))


def dense_zone(p):
    """Where the coat's colour changes on a line (the Hereford's legs and
    brisket), round the eyes, the muzzle and the claws: the game mesh keeps
    more vertices there."""
    t, u, s = head_coords(p)
    if 0.30 < p[2] < 0.52 and near_leg(p) < 0.10 or p[2] < 0.15 and near_leg(p) < 0.10:
        return True
    if p[1] < -1.3 and (t > 0.42 or abs(t - EYE[0]) < 0.06 and abs(abs(s) - EYE[2]) < 0.04):
        return True
    # The Hereford's white face meets the red neck and ears here.
    if p[1] < -0.95 and p[2] > 0.85 and (abs(t + 0.05) < 0.06 or abs(s) > 0.12 and t < 0.14):
        return True
    return False


def elbows(body):
    """``horse.py``'s ``elbows`` for a cow's elbow: the trunk behind and
    above it follows the chest, the weights smoothed round it."""
    groups = {g.name: g for g in body.vertex_groups}
    torso = groups.get("Torso2") or body.vertex_groups.new(name="Torso2")
    for v in body.data.vertices:
        p = Vector(unwarp(v.co))
        trunk = max(smooth((p.y + 0.52) / 0.15), smooth((0.12 - abs(p.x)) / 0.05)) * smooth((p.z - 0.66) / 0.14)
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
    for v in body.data.vertices:
        v.select = any((Vector(unwarp(v.co)) - Vector((sx * 0.19, -0.50, 0.76))).length < 0.22 for sx in (1, -1))
    bpy.ops.object.mode_set(mode="EDIT")
    bpy.ops.object.vertex_group_smooth(group_select_mode="ALL", factor=0.5, repeat=6)
    bpy.ops.object.mode_set(mode="OBJECT")
    bpy.ops.object.vertex_group_limit_total(group_select_mode="ALL", limit=4)
    bpy.ops.object.vertex_group_normalize_all(lock_active=False)


def tail_only(body):
    """The tail is skin of its own: each vertex of it (behind the round,
    under the tailhead) takes its weight from the tail chain only, by
    height, so the body never drags it and it never drags the body."""
    chain = [(n, Vector(h)) for n, _, h in BONES if n.startswith("Tail")] + [("Tail8", Vector((0.0, 0.938, 0.62))),
                                                                              ("Tail9", Vector((0.0, 0.935, 0.52)))]
    groups = {g.name: g for g in body.vertex_groups}
    for v in body.data.vertices:
        p = Vector(unwarp(v.co))
        if abs(p.x) > 0.05 or p.y < 0.90 or p.z > 1.17:
            continue
        for g in list(v.groups):
            body.vertex_groups[g.group].remove([v.index])
        below = [c for c in chain if c[1].z >= p.z]
        name = below[-1][0] if below else chain[0][0]
        (groups.get(name) or body.vertex_groups.new(name=name)).add([v.index], 1.0, "REPLACE")
    bpy.ops.object.mode_set(mode="EDIT")
    bpy.ops.mesh.select_all(action="DESELECT")
    bpy.ops.object.mode_set(mode="OBJECT")
    for v in body.data.vertices:
        v.select = abs(v.co.x) < 0.06 and v.co.y > 0.86 and unwarp(v.co)[2] < 1.2
    bpy.ops.object.mode_set(mode="EDIT")
    bpy.ops.object.vertex_group_smooth(group_select_mode="ALL", factor=0.5, repeat=3)
    bpy.ops.object.mode_set(mode="OBJECT")
    bpy.ops.object.vertex_group_limit_total(group_select_mode="ALL", limit=4)
    bpy.ops.object.vertex_group_normalize_all(lock_active=False)


def skin(body, arm):
    """``horse.py``'s skinning on the cow: heat weights from the bones that
    move the skin, the elbows, the hind legs, the pelvis and the helpers;
    then the tail apart, and the folds smoothed."""
    for b in arm.data.bones:
        b.use_deform = b.name not in NOT_SKIN
    rig.bind(body, arm)
    for b in arm.data.bones:
        b.use_deform = True
    elbows(body)
    H.hinds(body, arm)
    H.pelvis(body, arm)
    H.weight_muscles(body)
    tail_only(body)
    folds = [(warp(c), r, n) for c, r, n in FOLDS]
    folds = [f for f in folds if any((v.co - Vector((math.copysign(f[0][0], v.co.x), f[0][1], f[0][2]))).length < f[1]
                                     for v in body.data.vertices)]
    if len(folds) < len(FOLDS):
        print(f"cattle: WARNING: {len(FOLDS) - len(folds)} folds with no skin")
    H.use(FOLDS=folds)
    H.smooth_folds(body)


FOLDS = [((0.24, 0.44, 1.28), 0.18, 8), ((0.22, 0.52, 0.72), 0.20, 16)]


# ---------------------------------------------------------------------------
# Hair cards.


def poll_spots(sdf):
    """Curls on the poll and the forehead (a bull's thick and curly), all
    six points long like the others in the group."""
    strands = []
    count = 52 if BULL else 34
    for i in range(count):
        j = H.jitter
        t = -0.03 + (0.22 if BULL else 0.14) * j(i, 3) ** 1.3
        s = (j(i, 5) - 0.5) * (0.17 if t < 0.04 else 0.12)
        root = sdf.surface(at(t, -0.04, s), Vector(HEAD_U), reach=0.2)
        length = (0.045 if BULL else 0.03) + 0.02 * j(i, 7)
        curl = 1.0 if j(i, 9) > 0.5 else -1.0
        pts, outs = [root], []
        d = (Vector(HEAD_A) * 0.6 + Vector(HEAD_U) * 0.5 + Vector((s * 2.0, 0, 0))).normalized()
        for k in range(5):
            d = H.rot(Vector(HEAD_U), 32.0 * curl) @ d
            pts.append(sdf.outside(pts[-1] + d * length / 5, 0.004 + 0.002 * k))
        outs = [sdf.sample(p)[1] for p in pts]
        width = 0.024 + 0.014 * j(i, 11)
        strands.append((pts, [width * (1.0 - 0.35 * k / 5) for k in range(6)], outs, H.WISPY[i % len(H.WISPY)], "Head", 0.0))
    if BULL:
        # The sheath's tuft, hanging from its opening.
        for i in range(22):
            j = H.jitter
            root = sdf.surface(Vector((0.03 * (j(i, 21) - 0.5), -0.06 - 0.06 * j(i, 23), 0.62)), (0, 0, -1), reach=0.2)
            pts, outs = H.fall(sdf, root, Vector((0.0, -0.3, -1.0)), 0.10 + 0.06 * j(i, 25), 0.004, steps=5,
                               gravity=(0.0, -0.1, -1.0), stiff=0.3)
            width = 0.03 + 0.015 * j(i, 27)
            strands.append((pts, [width * (1.0 - 0.4 * k / 5) for k in range(6)], outs, H.THICK[i % len(H.THICK)], "Torso2", 0.0))
    return strands


def whisker_spots(sdf):
    """The sparse stiff tactile hairs round the upper lip and on the chin,
    and the fringe inside the ears' rims."""
    strands = []
    for i in range(24):
        j = H.jitter
        s_ = (j(i, 31) - 0.5) * 0.15
        chin = i % 3 == 0
        t, u = (0.46 + 0.04 * j(i, 33), -0.16) if chin else (0.47 + 0.04 * j(i, 35), -0.105)
        root = sdf.surface(at(t, u + 0.04, s_ * (0.6 if chin else 1.0)), -Vector(HEAD_U) + Vector((s_ * 3.0, 0, 0)), reach=0.15)
        d = (-Vector(HEAD_U) * 0.6 + Vector(HEAD_A) * 0.5 + Vector((s_ * 4.0, 0, 0))).normalized()
        length = 0.025 + 0.02 * j(i, 37)
        pts = [root + d * length * k / 3 + Vector((0, 0, -0.004 * k * k / 9)) for k in range(4)]
        n = sdf.sample(root)[1]
        strands.append((pts, [0.006, 0.005, 0.004, 0.002], [n] * 4, H.WISPY[i % len(H.WISPY)], "Jaw" if chin else "Head", 0.0))
    # The fringe of long pale hair round the inside of each ear's rim.
    for sx, side in ((1, "L"), (-1, "R")):
        for i in range(10):
            f = 0.2 + 0.7 * i / 9
            root = Vector(_at(0.04, -0.075, sx * 0.11)).lerp(Vector((sx * 0.31, -1.24, 1.195)), f)
            root += Vector((0.0, -0.016, (0.05 if i % 2 else -0.045) * (1.0 - 0.7 * f)))
            d = Vector((sx * 0.35, -0.8, 0.45 if i % 2 else -0.45)).normalized()
            length = 0.03 + 0.015 * H.jitter(i, 87)
            pts = [root + d * length * k / 3 + Vector((0, 0, -0.003 * k)) for k in range(4)]
            o = Vector((0.0, -1.0, 0.0))
            strands.append((pts, [0.014, 0.012, 0.009, 0.005], [o] * 4, H.WISPY[i % len(H.WISPY)], f"Ear1.{side}", 1.0))
    return strands


def tail_spots(sdf):
    """The switch: long locks from the last third of the tail, falling
    below the hocks, full and coarse."""
    path = rig._catmull([Vector(p) for p in ((0, 0.94, 0.74), (0, 0.936, 0.60), (0, 0.932, 0.46))], 8)
    strands = []
    count = 96
    for i in range(count):
        f = H.jitter(i, 51) ** 0.7
        k = min(int(f * (len(path) - 1)), len(path) - 2)
        c = path[k][0].lerp(path[k + 1][0], f * (len(path) - 1) - k)
        ang = 2 * math.pi * H.jitter(i, 53)
        out = Vector((math.cos(ang), math.sin(ang), 0.0))
        root = sdf.surface(c, out, reach=0.1)
        bottom = 0.20 + 0.10 * H.jitter(i, 57) ** 1.5
        length = max(0.12, root.z - bottom)
        spread = Vector(((H.jitter(i, 59) - 0.5) * 0.18, 0.04 + 0.06 * H.jitter(i, 61), -1.0))
        pts, outs = H.fall(sdf, root, out * 0.3 + Vector((0, 0.05, -1.0)), length, 0.004 + 0.01 * H.jitter(i, 63), steps=6,
                           gravity=spread, stiff=0.2)
        outs = [Vector((p.x, p.y - 0.935, 0.0)).normalized() if abs(p.x) + abs(p.y - 0.935) > 1e-3 else Vector((0, 1, 0)) for p in pts]
        width = 0.05 + 0.03 * H.jitter(i, 67)
        widths = [width * (0.5 + 0.9 * (j / 6) ** 0.7) * (1.0 - 0.45 * (j / 6) ** 3) for j in range(7)]
        strands.append((pts, widths, outs, H.THICK[i % len(H.THICK)]))
    print(f"cattle: switch roots {min(s[0][0].z for s in strands):.2f}-{max(s[0][0].z for s in strands):.2f} m, "
          f"tips {min(s[0][-1].z for s in strands):.2f}-{max(s[0][-1].z for s in strands):.2f} m")
    return strands


# ---------------------------------------------------------------------------
# Clips: ``horse.py``'s machinery with a cow's carriage. The head hangs
# lower, the walk is slower and lateral, the cud is chewed sideways, the
# tail swishes at the flies, the dewlap and the udder swing.

NECK = ("Neck1", "Neck2", "Neck3", "Head")
TAIL_CHAIN = ["Tail2", "Tail3", "Tail4", "Tail5", "Tail6", "Tail7", "Tail8", "Tail9"]
PIVOT = (0.0, 0.0, 1.00)
HOOF = {"F": dict(toe=-0.73, heel=-0.598), "H": dict(toe=0.67, heel=0.80)}


def pose_hair(pose, tail=(0.0, 0.0), tail_lift=0.0, lag=0.0, t=0.0, cycles=1.0, tail_curve=0.0, tail_whip=0.0,
              dewlap=0.0, dewlap_phase=0.0, udder=0.0, swish=0.0, swish_at=0.0, **_):
    """Secondary motion: the tail swings ``tail`` (amplitude, phase) down
    its chain with ``lag`` cycles per bone, lifted ``tail_lift`` at the root
    and curving ``tail_curve`` back down, a wave ``tail_whip`` running down
    it; ``swish`` degrees is a flick at the flies, at phase ``swish_at``,
    round to the flank; the dewlap swings ``dewlap`` degrees sideways (in
    phase ``dewlap_phase`` with the stride) and the udder (or the scrotum)
    ``udder``."""
    amp, phase = tail
    flick = H.flick(t, swish_at, 0.09) if swish else 0.0
    for k, name in enumerate(TAIL_CHAIN):
        m = pose.posed(name).to_3x3() @ pose.rest(name).to_3x3().inverted()
        side = amp * H.wave(t, cycles, phase - lag * k) / len(TAIL_CHAIN) * (1.0 + 0.3 * k)
        side += swish * flick * (0.4 + 0.15 * k) * H.wave(t * 0.0 + 0.25 + 0.06 * k)
        lift = tail_lift * (0.6, 0.4)[k] if k < 2 else -tail_curve
        lift += tail_whip * H.wave(t, cycles, 0.25 - lag * k) * (0.4 + 0.15 * k)
        pose.turn(name, H.rot(m @ H.Z, side) @ H.rot(m @ H.X, lift))
    m = pose.posed("Dewlap").to_3x3() @ pose.rest("Dewlap").to_3x3().inverted()
    pose.turn("Dewlap", H.rot(m @ Vector((0, 1, 0)), dewlap * H.wave(t, cycles, dewlap_phase)))
    soft = "Scrotum" if BULL else "Udder"
    m = pose.posed(soft).to_3x3() @ pose.rest(soft).to_3x3().inverted()
    pose.turn(soft, H.rot(m @ H.X, udder * H.wave(t, cycles, 0.3)) @ H.rot(m @ Vector((0, 1, 0)), 0.6 * udder * H.wave(t, cycles, 0.05)))


def standing(arm, body, neck=(0, 0, 0, 0), yaw=0.0, t=0.0, ears=((0, 0), (0, 0)), jaw=0.0, grind=0.0, lip=0.0,
             offsets=None, rests=None, hair=None, flex=0.0, nostrils=0.0):
    pose = H.Pose(arm)
    pose.carry("Body", body)
    if flex:
        pose.turn("Torso2", H.rot(body.to_3x3() @ H.X, flex))
    H.pose_neck(pose, body, neck, yaw)
    H.pose_face(pose, ears, jaw, grind, lip, nostrils)
    pose_hair(pose, **dict(dict(tail=(5.0, 0.0), lag=0.06, dewlap=1.5), **(hair or {})), t=t)
    H.stand_legs(pose, body, offsets, rests)
    return pose


def chew(t, chews):
    """The cud: the jaw opens and grinds sideways ``chews`` times a clip
    (a cow chews about once a second, always to one side)."""
    c = max(0.0, H.wave(t, chews))
    return 5.0 * c, 4.0 * H.wave(t, chews, 0.25)


def idle(arm, t):
    # Chewing the cud: breathing, the ears turning, a swish of the tail at
    # the flies, the head shaken once.
    body = H.body_matrix(lift=0.004 * H.wave(t), nose_down=0.3 * H.wave(t, 1, 0.25))
    shake = H.flick(t, 0.62, 0.05)
    neck = (1.0 * H.wave(t, 1, 0.1), 0.6 * H.wave(t, 1, 0.2), 0.0, 2.0 * H.wave(t, 1, 0.3))
    ears = ((25 * H.flick(t, 0.3) + 10, 15 * H.flick(t, 0.3)), (10 + 30 * H.flick(t, 0.72, 0.08), 20 * H.flick(t, 0.72, 0.08)))
    jaw, grind = chew(t, 4)
    return standing(arm, body, neck, yaw=6.0 * H.wave(t, 1, 0.15) + 9.0 * shake * H.wave(t, 6), t=t, ears=ears, jaw=jaw, grind=grind,
                    hair=dict(swish=45.0, swish_at=0.4), nostrils=3.0 * max(0.0, H.wave(t)))


def idle_2(arm, t):
    # Resting a hind leg, the head turned to look back, the cud chewed, the
    # tail flicking.
    body = H.body_matrix(lift=0.003 * H.wave(t), roll=0.7, nose_down=0.3)
    look = H.ease(t / 0.25) * (1 - H.ease((t - 0.7) / 0.25))
    neck = (-3.0, -2.0, 0.0, -2.0 + 1.5 * H.wave(t, 1, 0.2))
    ears = ((5 + 25 * H.flick(t, 0.55), 10), (5, 10 + 25 * H.flick(t, 0.2)))
    jaw, grind = chew(t, 4)
    return standing(arm, body, neck, yaw=-28.0 * look, t=t, ears=ears, jaw=jaw, grind=grind, offsets={"RH": -0.08},
                    rests={"RH": -30.0}, hair=dict(swish=50.0, swish_at=0.85, tail=(6.0, 0.3)))


def idle_headlow(arm, t):
    # Dozing: the head low, the ears dropped, chewing slowly.
    body = H.body_matrix(roll=-0.6, nose_down=0.3 + 0.3 * H.wave(t))
    neck = (-18.0 + 1.0 * H.wave(t, 1, 0.1), -6.0, 4.0, 10.0 + 1.5 * H.wave(t, 1, 0.3))
    ears = ((30, 25 + 10 * H.flick(t, 0.5)), (30 + 25 * H.flick(t, 0.35), 25))
    jaw, grind = chew(t, 3)
    return standing(arm, body, neck, t=t, ears=ears, jaw=0.7 * jaw, grind=0.7 * grind, offsets={"LH": -0.06},
                    rests={"LH": -25.0}, hair=dict(tail=(3.0, 0.0)))


GRAZE_SHAPE = (-1.0, -0.2, 0.0, 0.6)
GRAZE_LIFT, GRAZE_PITCH = -0.01, 2.5
GRAZE_BEND = 50.0


def eating(arm, t):
    # 6 s: down to the grass, the tongue wrapping and the head jerking
    # forward and sideways to tear each mouthful, chewing, a step forward,
    # then up to look about.
    down = H.ease((t - 0.04) / 0.12) * (1 - H.ease((t - 0.78) / 0.14))
    tear = max(0.0, H.wave(t, 10)) ** 2 * down
    body = H.body_matrix(lift=GRAZE_LIFT * down, nose_down=GRAZE_PITCH * down)
    bend = GRAZE_BEND * down
    neck = tuple(w * bend for w in GRAZE_SHAPE)
    neck = (neck[0], neck[1], neck[2], neck[3] + 8.0 * tear)
    jaw, grind = chew(t, 14)
    ears = ((20 + 20 * H.flick(t, 0.45), 15), (20, 15 + 25 * H.flick(t, 0.62)))
    sweep = 10.0 * down * H.wave(t, 2)
    return standing(arm, body, neck, yaw=sweep, t=t, ears=ears, jaw=jaw * (0.5 + tear), grind=grind, lip=6.0 * tear,
                    offsets={"LF": -0.10 * down}, hair=dict(swish=40.0, swish_at=0.5, tail=(4.0, 0.2)))


def walk(arm, t):
    # A lateral four-beat walk: the head nods twice a stride, the body
    # rolls over the standing legs, the dewlap and the udder swing.
    body = H.body_matrix(lift=-0.015 + 0.006 * H.wave(t, 2, 0.1), nose_down=0.6 * H.wave(t, 2, 0.35), roll=1.6 * H.wave(t, 1, 0.2))
    nod = 4.0 * H.wave(t, 2, 0.05)
    neck = (-6.0 + nod, 0.5 * nod, 0.0, -0.6 * nod)
    hair = dict(tail=(8.0, 0.1), lag=0.05, cycles=2.0, dewlap=5.0, dewlap_phase=0.2, udder=4.0)
    ears = ((5 + 20 * H.flick(t, 0.4, 0.08), 15), (5, 15 + 20 * H.flick(t, 0.85, 0.08)))
    return H.locomotion(arm, t, WALK, body, neck, slide=0.08, hair=hair, ears=ears)


def gallop(arm, t):
    # A cow's gallop: the head low and pumping, the back flexing, the tail
    # lifted and streaming, the dewlap and the udder swinging hard.
    pitch = H.wave(t, 1, -0.25)
    body = H.body_matrix(lift=-0.04 + 0.04 * H.wave(t, 1, 0.1), nose_down=(4.5 if pitch < 0 else 2.5) * pitch)
    pump = 8.0 * H.wave(t, 1, 0.08)
    neck = (-4.0 + pump, -2.0 + 0.4 * pump, 0.0, 6.0 - 0.6 * pump)
    hair = dict(tail=(10.0, 0.0), tail_lift=35.0, lag=0.09, tail_curve=10.0, tail_whip=10.0, dewlap=10.0, dewlap_phase=0.1, udder=10.0)
    flex = -3.0 * H.wave(t, 1, 0.15)
    return H.locomotion(arm, t, GALLOP, body, neck, slide=0.07, flex=flex, hair=hair, ears=((30, 10), (30, 10)))


BUCK_SECONDS = 49 / 24


def gallop_jump(arm, t):
    # A buck: from the gallop the fores plant, the head goes down, the hind
    # legs kick up and out behind, then back into the gallop.
    kick = H.ease((t - 0.30) / 0.12) * (1 - H.ease((t - 0.55) / 0.15))
    gt = (t * BUCK_SECONDS / GALLOP["seconds"]) % 1.0
    body = H.body_matrix(lift=-0.04 + 0.12 * kick, nose_down=16.0 * kick, pivot=Vector((0, -0.55, 0.9)))
    pose = H.Pose(arm)
    pose.carry("Body", body)
    neck = (-4.0 - 14.0 * kick, -2.0, 0.0, 6.0 + 10.0 * kick)
    H.pose_neck(pose, body, neck, yaw=10.0 * kick * H.wave(t, 2))
    H.pose_face(pose, ((35, 15), (35, 15)))
    pose_hair(pose, tail=(10.0, 0.0), tail_lift=35.0 + 30.0 * kick, lag=0.09, t=gt, tail_curve=10.0, tail_whip=10.0,
              dewlap=10.0, udder=10.0)
    for leg in H.LEGS:
        f, toe = H.stride(GALLOP, leg, gt)
        if leg[1] == "F":
            planted = H.hoof_place(leg, -0.10, 0.0)
            f, toe = f.lerp(planted, kick), toe * (1 - kick)
        else:
            out = body @ (H.fetlock_rest(leg) + Vector((0.0, 0.42, 0.35)))
            f, toe = f.lerp(out, kick), toe + (-60.0 - toe) * kick
        H.pose_leg(pose, body, leg, f, toe, hip=0.55)
    return pose


def clips(arm):
    global GRAZE_BEND
    GRAZE_BEND = H.graze_bend(arm)
    H.GRAZE_BEND = GRAZE_BEND
    print(f"cattle: grazing bend {GRAZE_BEND:.1f} degrees")
    made = [
        rig.bake_clip(arm, "Idle", 80 / 24, idle, H.MOVED),
        rig.bake_clip(arm, "Idle_2", 80 / 24, idle_2, H.MOVED),
        rig.bake_clip(arm, "Idle_Headlow", 80 / 24, idle_headlow, H.MOVED),
        rig.bake_clip(arm, "Eating", 144 / 24, eating, H.MOVED),
    ]
    H.SHORT.clear()
    made.append(rig.bake_clip(arm, "Walk", WALK["seconds"], walk, H.MOVED))
    print(f"cattle: in the walk the standing claws fall short of their marks by at most "
          f"{ {k: round(v, 3) for k, v in sorted(H.SHORT.items())} } m")
    H.SHORT.clear()
    made += [
        rig.bake_clip(arm, "Gallop", GALLOP["seconds"], gallop, H.MOVED),
        rig.bake_clip(arm, "Gallop_Jump", BUCK_SECONDS, gallop_jump, H.MOVED),
    ]
    print(f"cattle: in the gallop the standing claws fall short of their marks by at most "
          f"{ {k: round(v, 3) for k, v in sorted(H.SHORT.items())} } m")
    for a, seconds in ((made[4], WALK["seconds"]), (made[5], GALLOP["seconds"])):
        speed, lows = H.measure(arm, a, seconds)
        print(f"cattle: {a.name} {seconds:.3f} s: claws slide back at {speed:.2f} m/s; lowest IK bones {[round(z, 3) for z in lows]}")
    print(f"cattle: planned walk {ground_speed(WALK):.2f} m/s, gallop {ground_speed(GALLOP):.2f} m/s")
    return made


def paint_sculpt(obj):
    """``horse.py``'s ``paint_sculpt``, the bare skin (the nose pad, the
    lips and the udder) a little glossier than its 0.6: moist, smooth; the
    hair strokes deeper and with more contrast."""
    HORSE_PAINT_SCULPT(obj)
    nodes = obj.data.materials[0].node_tree.nodes
    values = [n for n in nodes if n.type == "VALUE"]
    values[0].outputs[0].default_value = 0.42
    # A beef coat's short hair reads more than a horse's satin one: deeper
    # strokes in the relief and more contrast between hairs.
    bump = next(n for n in nodes if n.type == "BUMP")
    bump.inputs["Strength"].default_value = 1.0
    bump.inputs["Distance"].default_value = 0.003
    shade = next(n for n in nodes if n.type == "MAP_RANGE" and abs(n.inputs["To Min"].default_value - 0.58) < 1e-6)
    shade.inputs["To Min"].default_value = 0.46
    # A coarser lay of the hair (locks of about 2 cm) that reads at a few
    # metres, and a wider sheen: glossy along the hair, dull between.
    for n in nodes:
        if n.type == "MAPPING" and abs(n.inputs["Scale"].default_value[0] - 110.0) < 1e-3:
            n.inputs["Scale"].default_value = (55.0, 7.0, 55.0)
    sheen = next(n for n in nodes if n.type == "MAP_RANGE" and abs(n.inputs["To Min"].default_value - 0.9) < 1e-6)
    sheen.inputs["To Min"].default_value, sheen.inputs["To Max"].default_value = 0.95, 0.32


# ---------------------------------------------------------------------------
# Wiring ``horse.py``'s steps to the cow.

if bpy is not None:
    EXTRA = extra_bones()
    MUSCLES = muscles()
    NOT_SKIN = {"Body", "PoleTarget.L", "PoleTarget.R", "PoleTargetBack.L", "PoleTargetBack.R",
                "FrontShoulder.L", "FrontShoulder.R", "BackShoulder.L", "BackShoulder.R", "Torso"} | {m["name"] for m in MUSCLES}
    HORSE_PAINT_SCULPT = H.paint_sculpt
    HORSE_EYES, HORSE_LASHES, HORSE_ASSIGN = H.eyes, H.lash_spots, H.assign_materials

    def on_model(fn):
        """``fn`` of a layout point, called with a model point."""
        return lambda p, *rest: fn(Vector(unwarp(p)), *rest)

    def eyes():
        out = HORSE_EYES()
        for eye in out:
            warp_mesh(eye)
        return out

    def assign_materials(body):
        """By region, as the horse; and every face of the horns' own
        pieces (and only those) ``Horns``, so no face at their base takes
        the other's colour."""
        HORSE_ASSIGN(body)
        import bmesh as bm_

        bm = bm_.new()
        bm.from_mesh(body.data)
        bm.faces.ensure_lookup_table()
        seen, pieces = set(), []
        for f in bm.faces:
            if f.index in seen:
                continue
            piece, todo = [], [f]
            seen.add(f.index)
            while todo:
                g = todo.pop()
                piece.append(g.index)
                for e in g.edges:
                    for h in e.link_faces:
                        if h.index not in seen:
                            seen.add(h.index)
                            todo.append(h)
            pieces.append(piece)
        bm.free()
        pieces.sort(key=len, reverse=True)
        horns = MATERIALS.index("Horns")
        small = {i for piece in pieces[1:] for i in piece}
        for poly in body.data.polygons:
            if poly.index in small:
                poly.material_index = horns
            elif poly.material_index == horns:
                poly.material_index = MATERIALS.index("Main")
        print(f"cattle: {len(pieces)} pieces, {len(small)} horn faces")

    H.use(eyes=eyes, lash_spots=warped(HORSE_LASHES), assign_materials=assign_materials)
    H.use(
        paint_sculpt=paint_sculpt, QUATERNIUS=[], ENDS=[(n, b, warp(p)) for n, b, p in ENDS], EXTRA=EXTRA,
        MUSCLES=[dict(m, centre=warp(m["centre"])) for m in MUSCLES], MUSCLE_NAMES=[m["name"] for m in MUSCLES],
        HEAD_O=Vector(HEAD_O), HEAD_A=Vector(HEAD_A), HEAD_U=Vector(HEAD_U), EYE=EYE, EYE_RADIUS=EYE_RADIUS,
        bone_table=bone_table, sculpt=sculpt, near_leg=near_leg, is_hoof=on_model(is_hoof), ear_cavity=ear_cavity, opening=opening,
        muzzle_skin=on_model(muzzle_skin), material_of=on_model(material_of), MATERIALS=MATERIALS, FALLBACK=FALLBACK,
        hair_direction=on_model(hair_direction), tone=on_model(tone), dense_zone=on_model(dense_zone), BODY_TRIS=5900 if not BULL else 6100,
        skin=skin, NOT_SKIN=NOT_SKIN, TAIL_CHAIN=TAIL_CHAIN, FOLDS=FOLDS,
        mane_spots=warped(poll_spots), forelock_spots=lambda sdf: [], ear_hair_spots=warped(whisker_spots), tail_spots=warped(tail_spots),
        COATS=COATS, color_at=color_at, coat_part=coat_part,
        NECK=NECK, MUZZLE=Vector(warp(at(0.53, -0.09))), PIVOT=Vector(warp(PIVOT)), HOOF=HOOF,
        END_REST={name: Vector(warp(p)) for name, _, p in ENDS}, GRAZE_SHAPE=GRAZE_SHAPE, GRAZE_LIFT=GRAZE_LIFT,
        GRAZE_PITCH=GRAZE_PITCH, GRAZE_BEND=GRAZE_BEND, pose_hair=pose_hair,
    )


# ---------------------------------------------------------------------------
# Previews and the command line.


def parse(argv):
    import argparse

    parser = argparse.ArgumentParser(prog="cattle.py")
    parser.add_argument("output")
    parser.add_argument("--bull", action="store_true")
    parser.add_argument("--ox", action="store_true")
    parser.add_argument("--preview", default=None, metavar="DIR")
    parser.add_argument("--textures", default=None, metavar="DIR")
    parser.add_argument("--draft", action="store_true")
    parser.add_argument("--no-clips", action="store_true")
    parser.add_argument("--shots", default=None, metavar="REGEX")
    return parser.parse_args(argv)


def shots_for(actions):
    side, target = (4.6, -0.3, 1.0), (0.0, -0.3, 0.85)
    front34 = ((3.0, -3.6, 1.5), (0.0, -0.4, 0.85))
    shots = []
    for coat in COATS:
        shots.append((f"{coat}_side.png", coat, None, 0, side, target, 50))
        shots.append((f"{coat}_front34.png", coat, None, 0, *front34, 45))
    shots += [
        ("hereford_rear34.png", "hereford", None, 0, (2.6, 3.4, 1.6), (0.0, 0.4, 0.85), 45),
        ("close_head.png", "hereford", None, 0, (1.0, -2.6, 1.55), (0.0, -1.35, 1.15), 60),
        ("close_head_roan.png", "roan", None, 0, (1.4, -2.3, 1.45), (0.0, -1.35, 1.15), 60),
        ("close_head_angus.png", "angus", None, 0, (-1.4, -2.3, 1.45), (0.0, -1.35, 1.15), 60),
        ("close_head_side.png", "hereford", None, 0, (2.2, -1.35, 1.35), (0.0, -1.35, 1.15), 60),
        ("close_eye.png", "roan", None, 0, (0.95, -1.75, 1.35), (0.1, -1.37, 1.17), 75),
        ("close_coat.png", "roan", None, 0, (1.4, 0.0, 1.15), (0.2, 0.05, 1.0), 60),
        ("close_legs.png", "hereford", None, 0, (1.9, -1.1, 0.45), (0.1, -0.35, 0.32), 45),
        ("close_claws.png", "overo", None, 0, (0.9, -1.3, 0.25), (0.15, -0.66, 0.06), 60),
        ("close_hoof_side.png", "hereford", None, 0, (0.75, -0.66, 0.10), (0.19, -0.66, 0.06), 70),
        ("close_hoof_front.png", "roan", None, 0, (0.25, -1.25, 0.14), (0.19, -0.66, 0.05), 70),
        ("close_hind_leg.png", "hereford", None, 0, (1.3, 1.1, 0.45), (0.18, 0.75, 0.32), 50),
        ("close_hoof_side_hind.png", "angus", None, 0, (0.8, 0.75, 0.10), (0.18, 0.75, 0.06), 70),
        ("close_face.png", "hereford", None, 0, (0.55, -2.05, 1.25), (0.0, -1.38, 1.12), 55),
        ("roan_mid.png", "roan", None, 0, (5.5, -2.5, 1.6), (0.0, -0.2, 0.75), 50),
        ("close_rear.png", "hereford", None, 0, (0.9, 2.6, 0.95), (0.0, 0.75, 0.75), 45),
        ("close_tail.png", "hereford", None, 0, (1.6, 1.6, 0.8), (0.0, 0.9, 0.7), 50),
        ("close_udder.png", "hereford", None, 0, (1.9, 0.9, 0.55), (0.0, 0.45, 0.6), 45),
    ]
    if BULL:
        shots += [("close_scrotum.png", "hereford", None, 0, (1.2, 1.4, 0.6), (0.0, 0.63, 0.55), 50),
                  ("close_scrotum_rear.png", "angus", None, 0, (0.3, 2.0, 0.6), (0.0, 0.63, 0.55), 50)]
    # The udder's bare skin in every coat, from the side and from behind.
    for coat in ([] if BULL else COATS):
        shots.append((f"udder_{coat}.png", coat, None, 0, (1.3, 0.5, 0.5), (0.0, 0.5, 0.62), 50))
        shots.append((f"udder_rear_{coat}.png", coat, None, 0, (0.5, 1.9, 0.55), (0.0, 0.5, 0.62), 50))
    shots += [
    ]
    by_name = {a.name: a for a in actions}
    frames = {
        "Walk": ((0, 7, 14, 21), "hereford"),
        "Gallop": ((0, 2, 4, 6, 8, 10), "angus"),
        "Gallop_Jump": ((0, 12, 18, 24, 36), "roan"),
        "Eating": ((0, 30, 72, 140), "hereford"),
        "Idle": ((0, 20, 40, 60), "roan"),
        "Idle_2": ((0, 40), "overo"),
        "Idle_Headlow": ((0, 40), "hereford"),
    }
    for name, (fs, coat) in frames.items():
        if name in by_name:
            for f in fs:
                shots.append((f"{name.lower()}_{f:03}.png", coat, by_name[name], f, side, target, 50))
    return shots


def _bake_skin(body, image, emit):
    """Bakes into ``image``, over the body's UVs, the emission that
    ``emit(nodes, link)`` returns (a colour socket) on the skin only (the
    hair cards and the eyes have UVs of their own)."""
    work = body.copy()
    work.data = body.data.copy()
    work.modifiers.clear()
    work.parent = None
    bpy.context.collection.objects.link(work)
    keep = {MATERIALS.index(m) for m in ("Main", "Main_Light", "Muzzle", "Hooves", "Horns")}
    import bmesh as bm_

    bm = bm_.new()
    bm.from_mesh(work.data)
    bm_.ops.delete(bm, geom=[f for f in bm.faces if f.material_index not in keep], context="FACES")
    bm.to_mesh(work.data)
    bm.free()
    mat = bpy.data.materials.new("SkinBake")
    mat.use_nodes = True
    nt = mat.node_tree
    nodes = nt.nodes
    for n in list(nodes):
        if n.type != "OUTPUT_MATERIAL":
            nodes.remove(n)
    emit_node = nodes.new("ShaderNodeEmission")
    nt.links.new(emit(nodes, nt.links.new, work), emit_node.inputs["Color"])
    nt.links.new(emit_node.outputs[0], nodes["Material Output"].inputs["Surface"])
    work.data.materials.clear()
    work.data.materials.append(mat)
    rig.bake_to(work, image, "EMIT", samples=1)
    bpy.data.objects.remove(work)
    bpy.data.materials.remove(mat)


ROAN_SPECKLE = 650.0  # the noise's scale: speckles of 1.5 to 2.5 mm, at the 2048² map's texel


def bake_roan(body, size):
    """The roan's hair-level mix as a map over the body's UVs (``roan``,
    white = white hairs): a fine speckle whose share of white is
    ``roan_white`` at each point (whiter on the body, redder on the head
    and the legs). A vertex colour cannot carry hairs: ``coat_pixels``
    mixes the roan's red and white by this map."""

    def emit(nodes, link, work):
        share = work.data.color_attributes.new("Share", "FLOAT_COLOR", "POINT")
        for v in work.data.vertices:
            w = roan_white(unwarp(v.co), v.normal)
            share.data[v.index].color = (w, w, w, 1.0)
        coord = nodes.new("ShaderNodeTexCoord")
        noise = nodes.new("ShaderNodeTexNoise")
        noise.inputs["Scale"].default_value = ROAN_SPECKLE
        noise.inputs["Detail"].default_value = 1.0
        link(coord.outputs["Object"], noise.inputs["Vector"])
        attr = nodes.new("ShaderNodeVertexColor")
        attr.layer_name = "Share"
        # White where the noise (stretched about its mean) falls under the share.
        stretch = nodes.new("ShaderNodeMapRange")
        # The noise keeps near its mean: stretched over 0..1, the white share of
        # the speckle follows ``roan_white``.
        stretch.inputs["From Min"].default_value, stretch.inputs["From Max"].default_value = 0.4, 0.6
        link(noise.outputs["Fac"], stretch.inputs["Value"])
        diff = nodes.new("ShaderNodeMath")
        diff.operation = "SUBTRACT"
        link(attr.outputs["Color"], diff.inputs[0])
        link(stretch.outputs["Result"], diff.inputs[1])
        sharp = nodes.new("ShaderNodeMapRange")
        sharp.inputs["From Min"].default_value, sharp.inputs["From Max"].default_value = -0.15, 0.15
        link(diff.outputs[0], sharp.inputs["Value"])
        return sharp.outputs["Result"]

    image = rig.new_image("cattle_roan", size, data=True)
    _bake_skin(body, image, emit)
    return image


def bake_geometry(body, size):
    """Each texel's model point and normal over the body's UVs, as (size,
    size, 3) arrays (rows bottom up, like the images); a texel no face
    covers (beyond the bake's margin) gets the normal (-1, -1, -1)."""
    import numpy as np

    out = []
    for output, scale in (("Object", 0.125), ("Normal", 0.5)):
        image = bpy.data.images.new(f"cattle_{output.lower()}", size, size, alpha=False, float_buffer=True)
        image.colorspace_settings.name = "Non-Color"

        def emit(nodes, link, work, output=output, scale=scale):
            coord = nodes.new("ShaderNodeTexCoord")
            # Into 0..1 (the bake keeps no negative emission): x * scale + 0.5.
            fit = nodes.new("ShaderNodeVectorMath")
            fit.operation = "MULTIPLY_ADD"
            fit.inputs[1].default_value = (scale, scale, scale)
            fit.inputs[2].default_value = (0.5, 0.5, 0.5)
            link(coord.outputs[output], fit.inputs[0])
            return fit.outputs[0]

        _bake_skin(body, image, emit)
        px = np.empty(size * size * 4, np.float32)
        image.pixels.foreach_get(px)
        out.append((px.reshape(size, size, 4)[:, :, :3].astype(np.float64) - 0.5) / scale)
        bpy.data.images.remove(image)
    return out


def image_array(image):
    """``image``'s RGB as an (h, w, 3) numpy array, 0..1 as stored."""
    import numpy as np

    w, h = image.size
    px = np.empty(w * h * 4, np.float32)
    image.pixels.foreach_get(px)
    return px.reshape(h, w, 4)[:, :, :3].astype(np.float64)


def upsample(a, size):
    """``a`` (h, w, c) resized to (size, size, c) bilinearly."""
    import numpy as np

    h = a.shape[0]
    x = (np.arange(size) + 0.5) * h / size - 0.5
    i0 = np.clip(np.floor(x).astype(int), 0, h - 1)
    i1 = np.clip(i0 + 1, 0, h - 1)
    f = np.clip(x - i0, 0.0, 1.0)
    rows = a[i0] * (1 - f)[:, None, None] + a[i1] * f[:, None, None]
    return rows[:, i0] * (1 - f)[None, :, None] + rows[:, i1] * f[None, :, None]


def bake_coats(body, colour, roan, draft):
    """The colour map of each coat (``coat_pixels``) as Blender images, by
    coat: 1024² (512² as a draft), the roan's at its hair map's size."""
    import numpy as np

    lum = image_array(colour)
    out = {}
    geometry = {}
    for coat in COATS:
        size = roan.size[0] if coat == "roan" else lum.shape[0]
        if size not in geometry:
            geometry[size] = bake_geometry(body, size)
        pos, nrm = geometry[size]
        share = image_array(roan)[:, :, 0] if coat == "roan" else None
        lit = lum if size == lum.shape[0] else upsample(lum, size)
        rgb = coat_pixels(coat, lit, pos, nrm, share)
        out[coat] = rig.image_from_array(f"cattle_{coat}", rgb)
        print(f"cattle: baked the {coat} map, {size}²")
    return out


LIGHT_TRIS = 1400  # the light mesh's budget (the full one: about 8,200)


def light_mesh(body, arm):
    """The light mesh the game draws far off (``Cow_LOD``): the skin alone
    (no hair cards, no eyes) decimated to ``LIGHT_TRIS`` triangles, with its
    UVs, so the same maps and coats dress it, and its weights from the full
    one."""
    import bmesh as bm_

    skin_only = body.copy()
    skin_only.data = body.data.copy()
    skin_only.modifiers.clear()
    bpy.context.collection.objects.link(skin_only)
    keep = {MATERIALS.index(m) for m in ("Main", "Main_Light", "Muzzle", "Hooves", "Horns")}
    bm = bm_.new()
    bm.from_mesh(skin_only.data)
    bm_.ops.delete(bm, geom=[f for f in bm.faces if f.material_index not in keep], context="FACES")
    bm.to_mesh(skin_only.data)
    bm.free()
    light = rig.decimated_copy(skin_only, "Cow_LOD", LIGHT_TRIS)
    bpy.data.objects.remove(skin_only)
    bpy.ops.object.select_all(action="DESELECT")
    light.select_set(True)
    bpy.context.view_layer.objects.active = light
    bpy.ops.object.vertex_group_limit_total(group_select_mode="ALL", limit=4)
    bpy.ops.object.vertex_group_normalize_all(lock_active=False)
    light.parent = arm
    light.modifiers.new("Armature", "ARMATURE").object = arm
    print(f"cattle: light mesh {rig.triangles(light)} triangles")
    return light


def coat_materials(coat, images):
    """``horse.py``'s preview materials, with ``Main`` drawn by the coat's
    own colour map (``bake_coats``), as in the game; a polled Angus shows
    no horns, as in the game (``Paint::Hidden``)."""
    out = HORSE_COAT_MATERIALS(coat, images)
    if coat in images.get("coats", {}):
        nt = out["Main"].node_tree
        tex = nt.nodes.new("ShaderNodeTexImage")
        tex.image = images["coats"][coat]
        nt.links.new(tex.outputs["Color"], nt.nodes["Principled BSDF"].inputs["Base Color"])
    if coat == "angus":
        mat = bpy.data.materials.new("HornsPreview")  # ``use_materials`` finds it by name
        mat.use_nodes = True
        mat.node_tree.nodes["Principled BSDF"].inputs["Alpha"].default_value = 0.0
        out["Horns"] = mat
    return out


def main():
    argv = sys.argv[sys.argv.index("--") + 1 :] if "--" in sys.argv else []
    args = parse(argv)
    global HORSE_COAT_MATERIALS
    HORSE_COAT_MATERIALS = H.coat_materials
    H.use(coat_materials=coat_materials)
    mesh, arm, images = H.build(args.draft, None)
    mesh.name = mesh.data.name = "Cow"
    images["roan"] = bake_roan(mesh, 512 if args.draft else 2048)
    images["coats"] = bake_coats(mesh, images["colour"], images["roan"], args.draft)
    light = light_mesh(mesh, arm)
    light.hide_render = True
    if args.textures:
        animal = "ox" if OX else "bull" if BULL else "cow"
        for folder, maps in (
            (f"{animal}_body", (("color", images["colour"]), ("normal_gl", images["normal"]), ("roughness", images["rough"]),
                                ("ao", bpy.data.images["horse_body_ao"]))
             + tuple((coat, img) for coat, img in images["coats"].items())),
            ("ox_hair" if OX else "cattle_hair", (("color", images["hair"]["color"]), ("normal_gl", images["hair"]["normal"]),
                             ("roughness", images["hair"]["rough"]), ("opacity", images["hair"]["opacity"]))),
        ):
            for name, img in maps:
                rig.save_jpeg(img, os.path.join(args.textures, folder, f"{name}.jpg"))
    actions = [] if args.no_clips else clips(arm)
    for pb in arm.pose.bones:  # the muscles' last keyed scale
        pb.scale = (1.0, 1.0, 1.0)
    if args.preview:
        shots = shots_for(actions)
        if args.shots:
            shots = [s for s in shots if re.search(args.shots, s[0])]
        # The still shots first, while every bone (the muscles' scale too)
        # is at rest; then the clips' frames.
        H.render_shots(args.preview, arm, mesh, images, [s for s in shots if s[2] is None])
        H.render_shots(args.preview, arm, mesh, images, [s for s in shots if s[2] is not None])
        # The light mesh alone, as the game draws a herd far off.
        light.hide_render, mesh.hide_render = False, True
        far = [(f"light_{coat}_side.png", coat, None, 0, (4.6, -0.3, 1.0), (0.0, -0.3, 0.85), 50) for coat in COATS]
        H.render_shots(args.preview, arm, light, images, [s for s in far if not args.shots or re.search(args.shots, s[0])])
        mesh.hide_render = False
    light.hide_render = False
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
