#!/usr/bin/env python3
"""Retarget Quaternius' Universal Animation Library clips to MakeHuman's
``game_engine`` skeleton, into one shared animation GLB.

Standard library only, but for the optional ``--model`` chest check, which
uses numpy (as ``tools/mh2gltf.py`` does); run it isolated:
``python3 -I tools/retarget.py``.
The inputs are treated as untrusted data: they are parsed (with
``glbtrim``'s bounded GLB reader), never executed, and every index, name
and number is checked. See tools/README.md.

Sources:

- the Universal Animation Library 1 GLB, with Rigify ``DEF-*`` bone names
  (CC0, by Quaternius);
- the Universal Animation Library 2 GLB, with Unreal names (CC0, by
  Quaternius);
- for the run and the sprint, BVHs with the CMU Graphics Lab Motion
  Capture Database's joint names (see ``BvhSource``): ``run_cycle.bvh``
  and ``sprint_cycle.bvh``, keyed from real runners' measurements by
  ``tools/run_cycle.py`` (the pack's jog leaps
  5.7 m a cycle, and every captured runner tried crouched, minced or held
  the hands at the chest).

The two packs share one rest skeleton, a T-pose. The target is a character built by
``tools/mh2gltf.py``: its ``Root`` node and the ``game_engine`` bones under
it, resting with the arms down.

How a clip is retargeted, per frame:

1. The source skeleton is posed by the clip and its model-space (world)
   rotations and places worked out down the hierarchy.
2. Rest alignment: the source is posed once like the target's rest. Down the
   hierarchy, each source bone keeps its rest turn relative to its parent,
   then swings by the shortest arc that points it where the target bone
   points: at the next joint of its chain (the spine at the neck, the upper
   arm at the elbow), or along its +Y at a chain's end. That is the source's *reference*:
   its T-pose arms lowered to the sides as the target's are, with no twist
   added. The pelvis is not swung: both rigs stand upright, and which way
   its axis points is only each rig's convention.
3. Each bone's world turn away from its reference, ``D = W(t) * Ref^-1``,
   is applied to the target bone's world rest, ``D * Rest``, and turned
   back into a local rotation under the target parent's posed world
   rotation. Working in the shared world frame (both rigs are Y-up, facing
   +Z, left at +X) means a bone's roll in either rig never matters, so no
   limb twists and no sign flips; the reference means the T-pose rest never
   leaks in.
4. The pelvis's world move from its rest is scaled by the target's leg
   length over the source's (hip to ankle). Looping clips have their
   horizontal drift taken out, so they stay in place: the game moves the
   entity. The vertical bob and the sway stay. Where the differently built
   target would put a foot through the ground (an ankle or a ball of the
   foot below its rest height), the pelvis is raised until it is not.

Only the chosen clips are kept, renamed to stable game names, in a fixed
order (the game loads them by index). For the locomotion clips the natural
ground speed (m/s at the target's scale) and stride (meters per cycle) are
measured from the source's feet: the planted foot of an in-place loop slides
back at the ground speed. They are stored in each animation's ``extras``.

The output holds the target's node hierarchy (``Root`` and its bones, with
their rest transforms) and the animations; no mesh. Bevy binds a clip by
each node's name path from the scene root (``Root/pelvis/...``), the same
path the character models' bones have. The output is deterministic: the
same inputs give the same bytes.
"""

import argparse
import collections
import copy
import json
import math
import os
import struct
import sys

if __package__ in (None, ""):
    # Run as a script with -I the tools folder is not on the path; it is
    # our own code, never the input's folder.
    sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))

import glbtrim  # noqa: E402

GENERATOR = "pampa-seca tools/retarget.py"
MAX_TARGET_BYTES = 16 * 1024 * 1024
MAX_FRAMES = 2000
FINGERS = ("index", "middle", "ring", "pinky", "thumb")

# The game's clips, in output order: (game name, source, source clip, loop,
# locomotion). The order is part of the contract with the game (it loads
# clips by index): append, never reorder.
CLIPS = [
    ("idle", "ual1", "Idle_Loop", True, False),
    ("idle_talk", "ual1", "Idle_Talking_Loop", True, False),
    ("walk", "ual1", "Walk_Loop", True, True),
    ("walk_formal", "ual1", "Walk_Formal_Loop", True, True),
    ("jog", "cmu", "run_cycle", True, True),
    ("sprint", "sprint", "sprint_cycle", True, True),
    ("sit_idle", "ual1", "Sitting_Idle_Loop", True, False),
    ("drive", "ual1", "Driving_Loop", True, False),
    ("interact", "ual1", "Interact", False, False),
    ("idle_fold_arms", "ual2", "Idle_FoldArms_Loop", True, False),
    ("yes", "ual2", "Yes", False, False),
    ("no", "ual2", "Idle_No_Loop", True, False),
    ("harvest", "ual2", "Farm_Harvest", False, False),
    ("plant", "ual2", "Farm_PlantSeed", False, False),
    ("water", "ual2", "Farm_Watering", False, False),
    ("chop", "ual2", "TreeChopping_Loop", True, False),
    ("walk_carry", "ual2", "Walk_Carry_Loop", True, True),
]


# The standing clips calmed for 1919 townsfolk (game name -> what is
# calmed): Quaternius' idles all stand in a game "ready" stance, the feet
# twice the hips apart, the knees bent about 20 degrees and the arms held
# out from the body. See `Retargeter.calm`.
CALM = {
    "idle": ("legs", "arms"),
    "idle_talk": ("legs",),
    "idle_fold_arms": ("legs",),
    "yes": ("legs", "arms"),
    "no": ("legs", "arms"),
}
# Calmed, the ankles stand at most this many times the hip joints' spacing
# apart (about hip to shoulder width) and at most this far one ahead of the
# other, meters (the idles stand a boxer's 42 cm), the knees keep this share of their
# bend, and the upper arms hang at most this far out from vertical, degrees
# (the MakeHuman rest's own is about 11, clear of the hips and clothes).
STANCE = 1.3
STAGGER = 0.12
KNEE_KEEP = 0.35
ARM_OUT = 10.0
# The calmed arms also hang at ease (see ``Retargeter.hang_arms``): the packs'
# idles hold the upper arms 8 to 29 degrees back of hanging, the elbows bent
# 28 to 44 and the clavicles turned 25 to 37 degrees off rest, the shoulder
# blades pinched (it showed on a bare torso). The clavicles rest; each upper
# arm's mean pitch is brought to ``HANG[0]`` degrees (forward of hanging) and
# each elbow's mean bend to ``HANG[1]``, keeping ``HANG[2]`` of their sway.
HANG = (-2.0, 15.0, 0.5)


# The walks stood up from the pack's crouched game walk into an easy 1919
# street walk (see ``Retargeter.upright``): the stride lengthened this many
# times (so the game plays it slower: its 1.10 m/s becomes 1.26), the knees
# straightened by this many degrees but never below this share of their
# bend, the torso's mean lean from the vertical (degrees; the pack leans
# 7 to 9), the upper arms' swing from back to forward of hanging and the
# elbows' bend from least to most (degrees; the pack holds the shoulders
# 6 cm back and the elbows bent 32 to 41).
UPRIGHT = ("walk",)
# How far out from hanging (seen from the front) the stood-up walk's arms
# hang, degrees: they hung at the MakeHuman rest's 11.4, held out.
WALK_OUT = 5.0
STRIDE = 1.15
KNEE_EASE = 19.0
KNEE_KEEP_WALK = 0.3
# The least a walking knee bends, degrees: a knee pulled straight locks and
# snaps back, which reads as a dislocation. How the hips' rise is shared by
# the legs (meters of ankle height for most of the weight to shift to the
# lower foot), and how many keys either side it is smoothed over.
KNEE_MIN = 4.0
STAND_BLEND = 0.012
RISE_SMOOTH = 2
# How often a swinging knee's bend is smoothed round the loop, and how far
# (meters) its ankle lifts before it counts as swinging.
SWING_SMOOTH = 2
CONTACT_LIFT = 0.02
LEAN = 2.0
ARM_SWING = (-10.0, 14.0)
ELBOW = (18.0, 35.0)


# The clips that bring the hands to the chest, and how far in front of
# every character's chest (and clothes) their forearms and hands must stay,
# meters. The source mannequin's chest is slimmer than the MakeHuman men's
# in their waistcoats and guerrera, so its folded arms sink 8 to 10 cm into
# theirs. The arms are brought forward as they are, the shoulders rounding,
# by the least multiple of ``REACH_STEP`` (meters) that clears every model
# given with ``--model`` (see ``Clearance`` and ``Retargeter.reach_out``).
CLEAR = ("idle_fold_arms", "idle_talk")
# What the townsfolk play of them (given with ``--talker``): they never fold
# their arms, so those need not clear their chests.
TALK = ("idle_talk",)
# Every this many keys a reach is tried on first (see Retargeter.cleared).
CLEAR_EVERY = 5
CLEARANCE = 0.015
REACH_STEP = 0.005
REACH_MAX = 0.2
# The runs whose swinging hands come in front of the chest: a slim
# runner's hands can sink into the men's waistcoats and Hidalgo's frock
# coat (CMU 16_46's by 3 to 23 cm). Both
# upper arms swing out from the body by the least multiple of
# ``SPREAD_STEP`` (degrees) that clears every ``--model`` (see
# ``Retargeter.spread_out``); the forearms and hands keep their swing.
SPREAD = ("jog", "sprint")
SPREAD_STEP = 1.0
SPREAD_MAX = 30.0


# A CMU trial (see ``BvhSource``): its length unit, meters (the database's
# ASF files measure in 1/0.45 inch); the most joints and frames read; every
# how many of its 120 frames a second one is kept (30 a second, as the
# packs' clips); and how long a run's stride cycle (two steps) may last,
# seconds. The loop is the cycle within the trial whose last pose is nearest
# its first.
BVH_UNIT = 0.0254 / 0.45
MAX_JOINTS = 128
MAX_BVH_FRAMES = 20000
BVH_KEY_STEP = 4
CYCLE = (0.5, 0.95)
# The least speed (m/s) a cycle's hips must cover: a trial that starts or
# ends standing would otherwise loop its stillness, whose ends match best.
CYCLE_PACE = 2.0
# How much a meter of the hips' height counts against a radian of joint
# turn when the cycle's ends are compared.
CYCLE_RISE = 10.0
# Whose fingers a capture without them holds (see ``Retargeter.held``): the
# pack's walk's first key, loosely curled; held flat at the MakeHuman rest,
# the running hands showed open, splayed palms.
HANDS = ("ual1", "Walk_Loop")


class RetargetError(Exception):
    """The inputs cannot be retargeted safely."""


# --- bone names -------------------------------------------------------------


def _body_map(spine, neck, head, clavicle, upperarm, lowerarm, hand, thigh, calf, foot, ball):
    """game_engine name -> source name, for the body bones."""
    out = {"pelvis": spine[0], "neck_01": neck, "head": head}
    for k in (1, 2, 3):
        out[f"spine_0{k}"] = spine[k]
    for side in ("l", "r"):
        for game, src in (
            ("clavicle", clavicle),
            ("upperarm", upperarm),
            ("lowerarm", lowerarm),
            ("hand", hand),
            ("thigh", thigh),
            ("calf", calf),
            ("foot", foot),
            ("ball", ball),
        ):
            out[f"{game}_{side}"] = src(side)
    return out


def ual1_map():
    """game_engine bone name -> UAL1 (Rigify ``DEF-*``) bone name."""

    def rigify(stem):
        return lambda side: f"DEF-{stem}.{side.upper()}"

    out = _body_map(
        ["DEF-hips", "DEF-spine.001", "DEF-spine.002", "DEF-spine.003"],
        "DEF-neck",
        "DEF-head",
        rigify("shoulder"),
        rigify("upper_arm"),
        rigify("forearm"),
        rigify("hand"),
        rigify("thigh"),
        rigify("shin"),
        rigify("foot"),
        rigify("toe"),
    )
    for finger in FINGERS:
        stem = "thumb" if finger == "thumb" else f"f_{finger}"
        for k in (1, 2, 3):
            for side in ("l", "r"):
                out[f"{finger}_0{k}_{side}"] = f"DEF-{stem}.0{k}.{side.upper()}"
    return out


def ual2_map():
    """game_engine bone name -> UAL2 (Unreal) bone name: the same names but
    ``Head``; the ``*_leaf`` and ``*_04`` tips have no game_engine bone."""
    out = {name: name for name in ual1_map()}
    out["head"] = "Head"
    return out


def cmu_map():
    """game_engine bone name -> CMU BVH joint name, for the body only: the
    capture has no fingers, so they hold their rest under the hand (see
    ``BvhSource``). Its ``Neck`` sits on ``Spine1``, so the neck is its
    ``Neck1``; the hip roots have no game_engine bone."""

    def side(stem):
        return lambda s: f"{'Left' if s == 'l' else 'Right'}{stem}"

    return _body_map(
        ["Hips", "LowerBack", "Spine", "Spine1"],
        "Neck1",
        "Head",
        side("Shoulder"),
        side("Arm"),
        side("ForeArm"),
        side("Hand"),
        side("UpLeg"),
        side("Leg"),
        side("Foot"),
        side("ToeBase"),
    )


MAPS = {"ual1": ual1_map, "ual2": ual2_map, "cmu": cmu_map}


def chain():
    """Each game_engine bone whose direction is set by the next joint along
    its chain: bone name -> that joint's bone name. The ends of the chains
    (the head, the balls of the feet, the finger tips) have none."""
    out = {"spine_01": "spine_02", "spine_02": "spine_03", "spine_03": "neck_01", "neck_01": "head"}
    for side in ("l", "r"):
        limbs = [
            ("clavicle", "upperarm"),
            ("upperarm", "lowerarm"),
            ("lowerarm", "hand"),
            ("hand", "middle_01"),
            ("thigh", "calf"),
            ("calf", "foot"),
            ("foot", "ball"),
        ]
        for bone, child in limbs:
            out[f"{bone}_{side}"] = f"{child}_{side}"
        for finger in FINGERS:
            for k in (1, 2):
                out[f"{finger}_0{k}_{side}"] = f"{finger}_0{k + 1}_{side}"
    return out


CHAIN = chain()


# --- quaternions (x, y, z, w) and vectors -----------------------------------


def qmul(a, b):
    ax, ay, az, aw = a
    bx, by, bz, bw = b
    return (
        aw * bx + ax * bw + ay * bz - az * by,
        aw * by - ax * bz + ay * bw + az * bx,
        aw * bz + ax * by - ay * bx + az * bw,
        aw * bw - ax * bx - ay * by - az * bz,
    )


def qinv(q):
    """The inverse of a unit quaternion."""
    return (-q[0], -q[1], -q[2], q[3])


def qnorm(q):
    n = math.sqrt(sum(c * c for c in q))
    if not n > 1e-12 or not math.isfinite(n):
        raise RetargetError(f"degenerate rotation {q!r}")
    return tuple(c / n for c in q)


def qrot(q, v):
    """`v` turned by unit quaternion `q`."""
    p = qmul(qmul(q, (v[0], v[1], v[2], 0.0)), qinv(q))
    return (p[0], p[1], p[2])


def qangle(a, b):
    """The angle between two unit rotations, radians."""
    d = abs(sum(x * y for x, y in zip(a, b)))
    return 2.0 * math.acos(min(1.0, d))


def qaxis_angle(axis, angle):
    n = math.sqrt(sum(c * c for c in axis))
    s = math.sin(angle / 2.0) / n
    return (axis[0] * s, axis[1] * s, axis[2] * s, math.cos(angle / 2.0))


def qarc(u, v):
    """The shortest rotation taking unit vector `u` onto unit vector `v`."""
    d = sum(a * b for a, b in zip(u, v))
    if d < -1.0 + 1e-9:
        # Opposite: any axis square to `u` will do; pick one deterministically.
        other = (1.0, 0.0, 0.0) if abs(u[0]) < 0.9 else (0.0, 1.0, 0.0)
        return qnorm(qaxis_angle(cross(u, other), math.pi))
    c = cross(u, v)
    return qnorm((c[0], c[1], c[2], 1.0 + d))


def qlerp_sign(prev, q):
    """`q` or `-q`, whichever is nearer `prev` (keeps keyframes on one
    hemisphere, so interpolation never takes the long way round)."""
    if prev is not None and sum(a * b for a, b in zip(prev, q)) < 0.0:
        return tuple(-c for c in q)
    return q


def qpow(q, f):
    """`f` of the way along unit rotation `q`, by the shorter way round."""
    if q[3] < 0.0:
        q = tuple(-c for c in q)
    angle = 2.0 * math.acos(min(1.0, q[3]))
    if angle < 1e-9 or length(q[:3]) < 1e-12:
        return (0.0, 0.0, 0.0, 1.0)
    return qaxis_angle(q[:3], angle * f)


def cross(a, b):
    return (a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0])


def add(a, b):
    return (a[0] + b[0], a[1] + b[1], a[2] + b[2])


def sub(a, b):
    return (a[0] - b[0], a[1] - b[1], a[2] - b[2])


def scale(a, k):
    return (a[0] * k, a[1] * k, a[2] * k)


def length(a):
    return math.sqrt(a[0] * a[0] + a[1] * a[1] + a[2] * a[2])


def unit(a):
    n = length(a)
    if not n > 1e-12:
        raise RetargetError("zero-length bone axis")
    return scale(a, 1.0 / n)


Y = (0.0, 1.0, 0.0)


# --- skeletons --------------------------------------------------------------


def finite(values, what, count):
    """`values` checked as a list of `count` finite numbers."""
    if not isinstance(values, list) or len(values) != count:
        raise RetargetError(f"{what} is not a list of {count} numbers")
    for v in values:
        if isinstance(v, bool) or not isinstance(v, (int, float)) or not math.isfinite(v):
            raise RetargetError(f"{what} holds a non-finite or non-number value")
    return tuple(float(v) for v in values)


class Skeleton:
    """A node hierarchy at rest: names, parents and local transforms."""

    def __init__(self, doc):
        nodes = glbtrim.items(doc, "nodes")
        self.names = []
        self.parent = [None] * len(nodes)
        self.rest_t = []
        self.rest_r = []
        for k, node in enumerate(nodes):
            name = node.get("name", f"node{k}")
            if not isinstance(name, str) or not 0 < len(name) <= 128:
                raise RetargetError(f"node {k} has a bad name")
            if "matrix" in node:
                raise RetargetError(f"node {name!r} uses a matrix; only TRS is supported")
            t = finite(node.get("translation", [0, 0, 0]), f"node {name!r} translation", 3)
            r = qnorm(finite(node.get("rotation", [0, 0, 0, 1]), f"node {name!r} rotation", 4))
            s = finite(node.get("scale", [1, 1, 1]), f"node {name!r} scale", 3)
            if any(abs(c - 1.0) > 1e-3 for c in s):
                raise RetargetError(f"node {name!r} is scaled; only unit scale is supported")
            self.names.append(name)
            self.rest_t.append(t)
            self.rest_r.append(r)
            children = node.get("children", [])
            if not isinstance(children, list):
                raise RetargetError(f"node {name!r} children is not a list")
            for c in children:
                c = glbtrim.index(c, len(nodes), f"node {name!r} child")
                if self.parent[c] is not None or c == k:
                    raise RetargetError(f"node {c} has two parents")
                self.parent[c] = k
        if len(set(self.names)) != len(self.names):
            raise RetargetError("two nodes share a name")
        self.index = {n: k for k, n in enumerate(self.names)}
        self.children = [[] for _ in nodes]
        for k, node in enumerate(nodes):
            self.children[k] = list(node.get("children", []))
        # Parents before children, or a cycle.
        self.order = []
        state = [0] * len(nodes)
        for k in range(len(nodes)):
            chain = []
            at = k
            while at is not None and state[at] == 0:
                state[at] = 1
                chain.append(at)
                at = self.parent[at]
            if at is not None and state[at] == 1 and at in chain:
                raise RetargetError("the node hierarchy has a cycle")
            for c in reversed(chain):
                state[c] = 2
                self.order.append(c)

    def find(self, name):
        if name not in self.index:
            raise RetargetError(f"no bone named {name!r}")
        return self.index[name]

    def world(self, local_r=None, local_t=None):
        """Each node's world rotation and place, posed by `local_r` and
        `local_t` (lists by node index; the rest where None)."""
        local_r = local_r or self.rest_r
        local_t = local_t or self.rest_t
        rot = [None] * len(self.names)
        pos = [None] * len(self.names)
        for k in self.order:
            p = self.parent[k]
            if p is None:
                rot[k], pos[k] = local_r[k], local_t[k]
            else:
                rot[k] = qnorm(qmul(rot[p], local_r[k]))
                pos[k] = add(pos[p], qrot(rot[p], local_t[k]))
        return rot, pos


def angle_between(a, b):
    """The angle between vectors a and b, radians."""
    c = sum(x * y for x, y in zip(unit(a), unit(b)))
    return math.acos(max(-1.0, min(1.0, c)))


def remap(value, values, into):
    """`value` moved from the span of `values` onto the span `into` (low,
    high), linearly; the middle of `into` when `values` do not vary."""
    lo, hi = min(values), max(values)
    if hi - lo < 1e-9:
        return (into[0] + into[1]) / 2.0
    return into[0] + (value - lo) / (hi - lo) * (into[1] - into[0])


# What the walk's stride step knows of one leg at one key (see
# ``Retargeter._legs``): its hip, knee and ankle, the ankle's stretched
# goal, the thigh and shin lengths, the hips' rise that would give it its
# eased bend (None: out of reach at any rise) and the reach of that bend.
Leg = collections.namedtuple("Leg", "hip knee ankle goal thigh shin rise reach")


def fill_round(values):
    """`values` (a loop: the last is followed by the first) with each None
    replaced by the linear blend of the nearest known values before and
    after it, round the loop; RetargetError when none is known."""
    known = [k for k, v in enumerate(values) if v is not None]
    if not known:
        raise RetargetError("no key of the walk has a leg that reaches its foot")
    n = len(values)
    out = list(values)
    for k, v in enumerate(values):
        if v is not None:
            continue
        before = max((j for j in known if j < k), default=known[-1] - n)
        after = min((j for j in known if j > k), default=known[0] + n)
        t = (k - before) / (after - before)
        out[k] = values[before % n] * (1 - t) + values[after % n] * t
    return out


def leg_length(skeleton, calf, foot):
    """Hip to ankle, meters: the rest offsets of the knee (`calf`, the
    thigh's length) and of the ankle (`foot`, the shin's)."""
    return length(skeleton.rest_t[skeleton.find(calf)]) + length(skeleton.rest_t[skeleton.find(foot)])


# --- the source --------------------------------------------------------------


class Source:
    """A UAL GLB: its skeleton and its clips, sampled per frame."""

    # Its map names every target bone (see ``BvhSource.partial``).
    partial = False

    def __init__(self, data, kind):
        self.doc, self.binary = glbtrim.read_glb(data)
        glbtrim.check_buffers(self.doc, self.binary)
        self.kind = kind
        self.skeleton = Skeleton(self.doc)
        self.map = MAPS[kind]()
        self.animations = {}
        for a in glbtrim.items(self.doc, "animations"):
            name = a.get("name")
            if isinstance(name, str):
                self.animations.setdefault(name, a)

    def frames(self, clip):
        """The clip's key times and, per time, each node's local rotation
        and translation."""
        if clip not in self.animations:
            raise RetargetError(f"{self.kind} has no clip {clip!r}")
        a = self.animations[clip]
        samplers = a.get("samplers", [])
        channels = a.get("channels", [])
        if not isinstance(samplers, list) or not isinstance(channels, list):
            raise RetargetError(f"clip {clip!r} is malformed")
        inputs = set()
        for s in samplers:
            if not isinstance(s, dict):
                raise RetargetError(f"clip {clip!r} has a malformed sampler")
            if s.get("interpolation", "LINEAR") != "LINEAR":
                raise RetargetError(f"clip {clip!r}: only LINEAR samplers are supported")
            inputs.add(s.get("input"))
        if len(inputs) != 1:
            raise RetargetError(f"clip {clip!r}: its channels do not share one key time list")
        times = [t[0] for t in glbtrim.read_accessor(self.doc, self.binary, inputs.pop())]
        if not 2 <= len(times) <= MAX_FRAMES:
            raise RetargetError(f"clip {clip!r} has {len(times)} keys")
        if any(not math.isfinite(t) for t in times) or any(b <= a for a, b in zip(times, times[1:])):
            raise RetargetError(f"clip {clip!r}: key times not finite and increasing")
        sk = self.skeleton
        rots = [list(sk.rest_r) for _ in times]
        trans = [list(sk.rest_t) for _ in times]
        for c in channels:
            if not isinstance(c, dict) or not isinstance(c.get("target"), dict):
                raise RetargetError(f"clip {clip!r} has a malformed channel")
            node = glbtrim.index(c["target"].get("node"), len(sk.names), f"clip {clip!r} node")
            path = c["target"].get("path")
            sampler = samplers[glbtrim.index(c.get("sampler"), len(samplers), "sampler")]
            values = glbtrim.read_accessor(self.doc, self.binary, sampler.get("output"))
            if len(values) != len(times):
                raise RetargetError(f"clip {clip!r}: a channel's values do not match its keys")
            for k, v in enumerate(values):
                if path == "rotation":
                    rots[k][node] = qnorm(finite(list(v), "rotation key", 4))
                elif path == "translation":
                    trans[k][node] = finite(list(v), "translation key", 3)
                elif path == "scale":
                    if any(abs(x - 1.0) > 1e-3 for x in v):
                        raise RetargetError(f"clip {clip!r} scales a bone; not supported")
                else:
                    raise RetargetError(f"clip {clip!r} animates {path!r}; not supported")
        return times, rots, trans


class BvhSource:
    """One CMU motion-capture trial, read from BVH: its skeleton and its one
    clip, named after the file (``run_cycle``), held as one loop.

    The BVH's joints rest unturned, at their offsets (in ``BVH_UNIT``,
    turned into meters), each turned by its rotation channels in their
    order; the capture is Y-up, facing +Z with its left at +X, as the
    packs. Each joint's ``End Site`` is a node of its own, ``<joint>_end``,
    so a chain's last bone points at it. The hips rest at their standing
    height over the lowest joint, so the capture's own height above the
    floor is the pelvis's move.

    A trial is a straight run of a second or so, not a loop. Its clip is
    the stride cycle (``CYCLE``), running (``CYCLE_PACE``), whose last pose
    is nearest its first, every
    ``BVH_KEY_STEP``th frame of it: the gap left between its ends is spread
    along it, each joint turning its share of the way, so the last key is
    the first; the run is turned to head along +Z and held in place over
    the origin, as the packs' loops are, so its planted foot slides back at
    the run's speed (see ``ground_speed``).

    The capture has no fingers and its hands are stiff: its map leaves the
    fingers out (``partial``), and they hold the target's rest under the
    hand."""

    partial = True

    def __init__(self, data, name):
        self.kind = "cmu"
        self.name = name
        self.map = cmu_map()
        try:
            tokens = data.decode("ascii").split()
        except UnicodeDecodeError:
            raise RetargetError("BVH is not ASCII text") from None
        self._tokens, self._at = tokens, 0
        self._nodes = []  # [name, parent, offset, channels]
        self._expect("HIERARCHY")
        self._expect("ROOT")
        self._joint(None)
        self._expect("MOTION")
        self._expect("Frames:")
        count = self._number()
        self._expect("Frame")
        self._expect("Time:")
        self.frame_time = self._number()
        if count != int(count) or not 2 <= count <= MAX_BVH_FRAMES:
            raise RetargetError(f"BVH has {count} frames")
        if not 0.0 < self.frame_time < 1.0:
            raise RetargetError("BVH frame time out of range")
        width = sum(len(n[3]) for n in self._nodes)
        values = [self._number() for _ in range(int(count) * width)]
        if self._at != len(tokens):
            raise RetargetError("BVH has data after its frames")
        self._values = [values[k * width : (k + 1) * width] for k in range(int(count))]
        low = 0.0
        place = []
        for name, parent, offset, _ in self._nodes:
            at = offset if parent is None else add(place[parent], offset)
            place.append(at)
            low = min(low, at[1])
        nodes = []
        for k, (name, parent, offset, _) in enumerate(self._nodes):
            node = {"name": name, "translation": list(offset if parent is not None else (0.0, -low, 0.0))}
            children = [c for c, n in enumerate(self._nodes) if n[1] == k]
            if children:
                node["children"] = children
            nodes.append(node)
        self.skeleton = Skeleton({"nodes": nodes})
        self.root = 0

    def _expect(self, word):
        if self._at >= len(self._tokens) or self._tokens[self._at] != word:
            raise RetargetError(f"BVH: expected {word!r}")
        self._at += 1

    def _word(self):
        if self._at >= len(self._tokens):
            raise RetargetError("BVH ends early")
        self._at += 1
        return self._tokens[self._at - 1]

    def _number(self):
        word = self._word()
        try:
            v = float(word)
        except ValueError:
            raise RetargetError(f"BVH: {word[:20]!r} is not a number") from None
        if not math.isfinite(v):
            raise RetargetError("BVH holds a non-finite number")
        return v

    def _joint(self, parent, end=False):
        name = f"{self._nodes[parent][0]}_end" if end else self._word()
        if not 0 < len(name) <= 128 or len(self._nodes) >= MAX_JOINTS:
            raise RetargetError("BVH has a bad joint name or too many joints")
        self._expect("{")
        self._expect("OFFSET")
        offset = tuple(self._number() * BVH_UNIT for _ in range(3))
        channels = []
        if not end:
            self._expect("CHANNELS")
            n = self._number()
            if n not in (3.0, 6.0) or (n == 6.0) != (parent is None):
                raise RetargetError(f"BVH joint {name!r}: 6 channels for the root, 3 for the rest")
            for _ in range(int(n)):
                c = self._word()
                if c not in ("Xposition", "Yposition", "Zposition", "Xrotation", "Yrotation", "Zrotation"):
                    raise RetargetError(f"BVH joint {name!r}: unknown channel {c[:20]!r}")
                channels.append(c)
        k = len(self._nodes)
        self._nodes.append([name, parent, offset, channels])
        while True:
            word = self._word()
            if word == "}":
                return
            if end:
                raise RetargetError("BVH: an End Site has children")
            if word == "JOINT":
                self._joint(k)
            elif word == "End":
                self._expect("Site")
                self._joint(k, end=True)
            else:
                raise RetargetError(f"BVH: unexpected {word[:20]!r}")

    def _pose(self, row):
        """One frame's local rotations and translations, by node index."""
        sk = self.skeleton
        rots, trans = list(sk.rest_r), list(sk.rest_t)
        at = 0
        for k, (_, parent, offset, channels) in enumerate(self._nodes):
            r = (0.0, 0.0, 0.0, 1.0)
            moved = [0.0, 0.0, 0.0]
            for c in channels:
                v = row[at]
                at += 1
                axis = "XYZ".index(c[0])
                if c.endswith("rotation"):
                    a = [0.0, 0.0, 0.0]
                    a[axis] = 1.0
                    r = qmul(r, qaxis_angle(a, math.radians(v)))
                else:
                    moved[axis] = v * BVH_UNIT
            rots[k] = qnorm(r)
            if parent is None:
                trans[k] = add(offset, tuple(moved))
        return rots, trans

    def frames(self, clip):
        """The trial's loop (see the class): key times and, per key, each
        node's local rotation and translation."""
        if clip != self.name:
            raise RetargetError(f"cmu has no clip {clip!r}; this trial is {self.name!r}")
        poses = [self._pose(row) for row in self._values]
        moving = sorted({self.skeleton.find(self.map[b]) for b in self.map})
        dt, step = self.frame_time, BVH_KEY_STEP
        shortest = math.ceil(CYCLE[0] / dt / step) * step
        longest = int(CYCLE[1] / dt)
        best = None
        for a in range(len(poses)):
            for b in range(a + shortest, min(a + longest, len(poses) - 1) + 1, step):
                (ra, ta), (rb, tb) = poses[a], poses[b]
                moved = sub(tb[self.root], ta[self.root])
                if math.hypot(moved[0], moved[2]) < CYCLE_PACE * (b - a) * dt:
                    continue
                gap = sum(qangle(ra[j], rb[j]) for j in moving)
                gap += CYCLE_RISE * abs(ta[self.root][1] - tb[self.root][1])
                if best is None or gap < best[0]:
                    best = (gap, a, b)
        if best is None:
            raise RetargetError(f"BVH trial {self.name!r} is shorter than a stride cycle")
        _, a, b = best
        (ra, ta), (rb, tb) = poses[a], poses[b]
        close = [qmul(ra[j], qinv(rb[j])) for j in range(len(ra))]
        rise = ta[self.root][1] - tb[self.root][1]
        travel = sub(tb[self.root], ta[self.root])
        heading = qaxis_angle(Y, -math.atan2(travel[0], travel[2]))
        start = qrot(heading, ta[self.root])
        times, rots, trans = [], [], []
        for k in range(a, b + 1, step):
            f = (k - a) / (b - a)
            r, t = poses[k]
            r = [qnorm(qmul(qpow(close[j], f), r[j])) for j in range(len(r))]
            r[self.root] = qnorm(qmul(heading, r[self.root]))
            t = list(t)
            root = qrot(heading, add(t[self.root], (-travel[0] * f, rise * f, -travel[2] * f)))
            t[self.root] = (root[0] - start[0], root[1], root[2] - start[2])
            times.append((k - a) * dt)
            rots.append(r)
            trans.append(t)
        return times, rots, trans


# --- the target --------------------------------------------------------------


def read_target(data):
    """The target glTF's skeleton: the nodes under ``Root``."""
    if len(data) > MAX_TARGET_BYTES:
        raise RetargetError("target glTF too large")
    try:
        doc = json.loads(data.decode("utf-8"))
    except (UnicodeDecodeError, json.JSONDecodeError) as e:
        raise RetargetError(f"target is not glTF JSON: {e}") from None
    if not isinstance(doc, dict):
        raise RetargetError("target is not a JSON object")
    return Skeleton(doc)


class Retargeter:
    """One source onto the target, with the rest alignment worked out."""

    def __init__(self, source, target):
        self.source = source
        self.target = target
        sk, tk = source.skeleton, target
        self.root = tk.find("Root")
        self.bones = [k for k in tk.order if self._under_root(k) and k != self.root]
        missing = [tk.names[k] for k in self.bones if tk.names[k] not in source.map]
        if missing and not source.partial:
            raise RetargetError(f"target bones with no {source.kind} name: {missing}")
        # A bone its source leaves out holds its rest under its parent, or
        # the local rotation `held` gives it (see ``HANDS``).
        self.src = {k: sk.find(source.map[tk.names[k]]) for k in self.bones if tk.names[k] in source.map}
        self.held = {}
        self.s_rest_r, self.s_rest_p = sk.world()
        self.t_rest_r, self.t_rest_p = tk.world()
        self.reference = self._reference()
        self.ratio = leg_length(tk, "calf_l", "foot_l") / leg_length(
            sk, source.map["calf_l"], source.map["foot_l"]
        )

    def _under_root(self, k):
        while k is not None:
            if k == self.root:
                return True
            k = self.target.parent[k]
        return False

    def _reference(self):
        """Each mapped source bone's world rotation, posed like the target's
        rest: its rest turn under its parent, swung by the shortest arc so
        that it points where the target bone points (see [`aim`])."""
        ref = {}
        tk = self.target
        for k in self.bones:
            if k not in self.src:
                continue
            s = self.src[k]
            p = tk.parent[k]
            if p == self.root:
                # The pelvis: both rigs stand upright at rest, and its axis
                # is only a convention (the source's leans forward, the
                # target's back), so it is not swung.
                ref[k] = self.s_rest_r[s]
                continue
            ps = self.src[p]
            relative = qmul(qinv(self.s_rest_r[ps]), self.s_rest_r[s])
            candidate = qmul(ref[p], relative)
            have, want = self.aim(k)
            have = unit(qrot(candidate, have))
            want = unit(qrot(self.t_rest_r[k], want))
            ref[k] = qnorm(qmul(qarc(have, want), candidate))
        return ref

    def aim(self, k):
        """Where target bone `k` and its source bone point, each in its own
        bone's frame: toward the next joint of the chain (``CHAIN``) when it
        has one, else along the bone's +Y. The next joint, not the axis: a
        game_engine bone's child can sit well off its +Y (``spine_03``'s
        neck is 16 degrees forward of it), and the joints' places are what
        the body's shape follows.

        A ``partial`` source's joints rest unturned, so their +Y says
        nothing: where the next joint of the chain is not mapped (the hand,
        whose fingers are left out) or there is none (the head, the balls
        of the feet), its bone points at its own first child, or that
        child's first child where it sits on the joint (the hand's
        ``FingerBase``)."""
        tk, sk = self.target, self.source.skeleton
        s = self.src[k]
        child = CHAIN.get(tk.names[k])
        c = tk.find(child) if child is not None and child in tk.index else None
        if c is not None and c in self.src:
            have = qrot(qinv(self.s_rest_r[s]), sub(self.s_rest_p[self.src[c]], self.s_rest_p[s]))
        elif self.source.partial and sk.children[s]:
            at = sk.children[s][0]
            while length(sub(self.s_rest_p[at], self.s_rest_p[s])) < 1e-6 and sk.children[at]:
                at = sk.children[at][0]
            have = qrot(qinv(self.s_rest_r[s]), sub(self.s_rest_p[at], self.s_rest_p[s]))
        else:
            return Y, Y
        if c is not None:
            want = qrot(qinv(self.t_rest_r[k]), sub(self.t_rest_p[c], self.t_rest_p[k]))
        else:
            want = Y
        return have, want

    def pose(self, s_rot, s_pos):
        """The target's local rotations (by node index) and the pelvis's
        world place, for a source pose given as world rotations and places."""
        tk = self.target
        world = {self.root: self.t_rest_r[self.root]}
        local = {}
        for k in self.bones:
            if k not in self.src:
                local[k] = self.held.get(k, tk.rest_r[k])
                world[k] = qnorm(qmul(world[tk.parent[k]], local[k]))
                continue
            s = self.src[k]
            turn = qmul(s_rot[s], qinv(self.reference[k]))
            world[k] = qnorm(qmul(turn, self.t_rest_r[k]))
            local[k] = qnorm(qmul(qinv(world[tk.parent[k]]), world[k]))
        pelvis = tk.find("pelvis")
        moved = sub(s_pos[self.src[pelvis]], self.s_rest_p[self.src[pelvis]])
        place = add(self.t_rest_p[pelvis], scale(moved, self.ratio))
        return local, place

    def pelvis_local(self, place):
        """The pelvis's local translation under ``Root`` for a world place."""
        r = self.t_rest_r[self.root]
        return qrot(qinv(r), sub(place, self.t_rest_p[self.root]))

    def posed_world(self, local, place):
        """The target's world rotations and places, posed by `local` (by
        node) with the pelvis at world `place`."""
        tk = self.target
        rots, trans = list(tk.rest_r), list(tk.rest_t)
        for k, q in local.items():
            rots[k] = q
        trans[tk.find("pelvis")] = self.pelvis_local(place)
        return tk.world(rots, trans)

    def calm(self, local, place, parts):
        """A standing pose calmed (see ``CALM``): with "legs", the feet
        drawn in to at most ``STANCE`` times the hips' spacing apart and
        ``STAGGER`` one ahead of the other, at the same heights, and the knees
        straightened to ``KNEE_KEEP`` of their bend, the hips rising (or,
        with the feet drawn in under them, sinking) as far as the leg that
        allows least lets them; with "arms", the upper arms lowered to ``ARM_OUT`` from
        vertical (seen from the front) when held out further. The feet,
        forearms and hands keep their own turns."""
        local = dict(local)
        if "legs" in parts:
            local, place = self._calm_legs(local, place)
        if "arms" in parts:
            local = self._calm_arms(local, place)
        return local, place

    def hang_arms(self, keys):
        """`keys` (time, local rotations, pelvis world place) with the arms
        at ease (see ``HANG``): the clavicles at rest, each upper arm's
        pitch (forward of hanging, seen from the side) and each elbow's bend
        moved to their ease about the clip's mean, keeping a share of their
        sway; how far out the arm hangs, and the hand under the forearm, are
        kept."""
        tk = self.target
        if any(f"{b}_{s}" not in tk.index for b in ("clavicle", "upperarm", "lowerarm", "hand") for s in "lr"):
            return keys
        pitch_mean, share = math.radians(HANG[0]), HANG[2]
        keys = [(t, self.relax_shoulders(local, place), place) for t, local, place in keys]

        def measure(local, place, s):
            pos = self.posed_world(local, place)[1]
            sh, el, ha = (pos[tk.find(f"{b}_{s}")] for b in ("upperarm", "lowerarm", "hand"))
            u = unit(sub(el, sh))
            return math.atan2(u[2], -u[1]), angle_between(sub(el, sh), sub(ha, el))

        out = [(t, dict(local), place) for t, local, place in keys]
        for s, side in (("l", 1.0), ("r", -1.0)):
            upper, lower = tk.find(f"upperarm_{s}"), tk.find(f"lowerarm_{s}")
            seen = [measure(local, place, s)[0] for _, local, place in out]
            mp = sum(seen) / len(seen)
            for (_, local, place), p in zip(out, seen):
                rot, pos = self.posed_world(local, place)
                u = unit(sub(pos[lower], pos[upper]))
                want = pitch_mean + (p - mp) * share
                # The same angle out from hanging (seen from the front), the
                # new one forward (seen from the side).
                spread = math.atan2(side * u[0], -u[1])
                goal = (side * math.tan(spread), -1.0, math.tan(want))
                world = qnorm(qmul(qarc(u, unit(goal)), rot[upper]))
                local[upper] = qnorm(qmul(qinv(rot[tk.parent[upper]]), world))
        out, _ = self.bend_elbows(out, HANG[1], share)
        return out

    def _calm_legs(self, local, place):
        tk = self.target
        f = tk.find
        rot, pos = self.posed_world(local, place)
        hip = {s: pos[f(f"thigh_{s}")] for s in "lr"}
        knee = {s: pos[f(f"calf_{s}")] for s in "lr"}
        ankle = {s: pos[f(f"foot_{s}")] for s in "lr"}
        spacing = abs(hip["l"][0] - hip["r"][0])
        apart = ankle["l"][0] - ankle["r"][0]
        middle = (ankle["l"][0] + ankle["r"][0]) / 2.0
        half = min(apart, STANCE * spacing) / 2.0
        ahead = ankle["l"][2] - ankle["r"][2]
        centre = (ankle["l"][2] + ankle["r"][2]) / 2.0
        step = math.copysign(min(abs(ahead), STAGGER), ahead) / 2.0
        goal = {
            "l": (middle + half, ankle["l"][1], centre + step),
            "r": (middle - half, ankle["r"][1], centre - step),
        }
        a = {s: length(tk.rest_t[f(f"calf_{s}")]) for s in "lr"}
        b = {s: length(tk.rest_t[f(f"foot_{s}")]) for s in "lr"}
        # How high each hip may rise for its knee to keep its share of bend.
        rises = []
        for s in "lr":
            bend = math.acos(
                max(-1.0, min(1.0, sum(x * y for x, y in zip(unit(sub(knee[s], hip[s])), unit(sub(ankle[s], knee[s]))))))
            )
            reach = math.sqrt(a[s] ** 2 + b[s] ** 2 + 2 * a[s] * b[s] * math.cos(KNEE_KEEP * bend))
            across = (goal[s][0] - hip[s][0]) ** 2 + (goal[s][2] - hip[s][2]) ** 2
            if reach * reach > across:
                rises.append(goal[s][1] + math.sqrt(reach * reach - across) - hip[s][1])
        # The hips go where the leg that wants them lowest allows (down, too,
        # when the feet come in under them), so neither leg overstretches.
        rise = min(rises) if rises else 0.0
        place = add(place, (0.0, rise, 0.0))
        pelvis_rot = rot[f("pelvis")]
        for s in "lr":
            h = add(hip[s], (0.0, rise, 0.0))
            u = unit(sub(goal[s], h))
            d = min(max(length(sub(goal[s], h)), abs(a[s] - b[s]) + 1e-9), a[s] + b[s] - 1e-9)
            cos_hip = (a[s] ** 2 + d * d - b[s] ** 2) / (2 * a[s] * d)
            # The knee bends the way it did, square to the hip-to-ankle line.
            pole = sub(knee[s], hip[s])
            pole = sub(pole, scale(u, sum(x * y for x, y in zip(pole, u))))
            if length(pole) < 1e-6:
                # A straight leg: its knee points ahead.
                ahead = (0.0, 0.0, 1.0)
                pole = sub(ahead, scale(u, sum(x * y for x, y in zip(ahead, u))))
            pole = unit(pole)
            k = add(h, scale(add(scale(u, cos_hip), scale(pole, math.sqrt(max(0.0, 1 - cos_hip**2)))), a[s]))
            thigh, calf, foot = f(f"thigh_{s}"), f(f"calf_{s}"), f(f"foot_{s}")
            thigh_w = qnorm(qmul(qarc(unit(sub(knee[s], hip[s])), unit(sub(k, h))), rot[thigh]))
            calf_w = qnorm(qmul(qarc(unit(sub(ankle[s], knee[s])), unit(sub(goal[s], k))), rot[calf]))
            local[thigh] = qnorm(qmul(qinv(pelvis_rot), thigh_w))
            local[calf] = qnorm(qmul(qinv(thigh_w), calf_w))
            local[foot] = qnorm(qmul(qinv(calf_w), rot[foot]))
        return local, place

    def _calm_arms(self, local, place):
        tk = self.target
        rot, pos = self.posed_world(local, place)
        most = math.radians(ARM_OUT)
        for s, side in (("l", 1.0), ("r", -1.0)):
            if f"upperarm_{s}" not in tk.index or f"lowerarm_{s}" not in tk.index:
                continue
            arm = tk.find(f"upperarm_{s}")
            v = sub(pos[tk.find(f"lowerarm_{s}")], pos[arm])
            out = math.atan2(side * v[0], -v[1])
            if out <= most:
                continue
            frontal = math.hypot(v[0], v[1])
            lowered = (side * frontal * math.sin(most), -frontal * math.cos(most), v[2])
            turn = qarc(unit(v), unit(lowered))
            local[arm] = qnorm(qmul(qinv(rot[tk.parent[arm]]), qmul(turn, rot[arm])))
        return local

    def below_ground(self, local, place):
        """How far the target's feet, posed by `local` with the pelvis at
        `place`, have sunk into the ground: the most any ankle or ball of a
        foot is below its rest height, else zero. The pelvis is raised by
        that much: the bodies differ (the target's hips are wider and its
        feet shorter), so a foot the source keeps just on the ground can
        end up a few centimeters through it."""
        tk = self.target
        _, pos = self.posed_world(local, place)
        sunk = 0.0
        for bone in ("foot_l", "foot_r", "ball_l", "ball_r"):
            if bone in tk.index:
                k = tk.find(bone)
                sunk = max(sunk, self.t_rest_p[k][1] - pos[k][1])
        return sunk

    def reach_out(self, local, place, ahead):
        """`local` with both arms brought `ahead` meters forward (the body's
        +z) as they are: each clavicle swings forward until its shoulder is
        as near that far ahead as its length allows, and the arm keeps its
        turn in the world. Folded arms come off the chest without rising
        or tilting, the shoulders a little rounded, as a man folding his
        arms over a big chest holds them."""
        if not ahead:
            return local
        tk = self.target
        rot, pos = self.posed_world(local, place)
        local = dict(local)
        for s in "lr":
            if f"clavicle_{s}" not in tk.index or f"upperarm_{s}" not in tk.index:
                continue
            clavicle, upper = tk.find(f"clavicle_{s}"), tk.find(f"upperarm_{s}")
            root, shoulder = pos[clavicle], pos[upper]
            goal = add(shoulder, (0.0, 0.0, ahead))
            swing = qarc(unit(sub(shoulder, root)), unit(sub(goal, root)))
            clavicle_w = qnorm(qmul(swing, rot[clavicle]))
            local[clavicle] = qnorm(qmul(qinv(rot[tk.parent[clavicle]]), clavicle_w))
            local[upper] = qnorm(qmul(qinv(clavicle_w), rot[upper]))
        return local

    def relax_shoulders(self, local, place):
        """`local` with both clavicles at the target's rest, each upper arm
        keeping its turn in the world: the shoulders drop out of a shrug
        and the arms hang from them as they swung."""
        tk = self.target
        rot, _ = self.posed_world(local, place)
        local = dict(local)
        for s in "lr":
            if f"clavicle_{s}" not in tk.index or f"upperarm_{s}" not in tk.index:
                continue
            clavicle, upper = tk.find(f"clavicle_{s}"), tk.find(f"upperarm_{s}")
            local[clavicle] = tk.rest_r[clavicle]
            rested = qnorm(qmul(rot[tk.parent[clavicle]], local[clavicle]))
            local[upper] = qnorm(qmul(qinv(rested), rot[upper]))
        return local

    def bend_elbows(self, keys, mean, share):
        """`keys` (time, local rotations, pelvis world place) with each
        elbow's bend moved to centre on `mean` degrees, keeping `share` of
        its sway about the clip's own mean: each forearm turned about the
        elbow's hinge, the hand keeping its turn under it; and how far the
        elbows opened on average, degrees."""
        tk = self.target
        out = [(t, dict(local), place) for t, local, place in keys]
        opened = []
        for s in "lr":
            if any(f"{b}_{s}" not in tk.index for b in ("upperarm", "lowerarm", "hand")):
                continue
            upper, lower, hand = (tk.find(f"{b}_{s}") for b in ("upperarm", "lowerarm", "hand"))

            def bend(local, place):
                pos = self.posed_world(local, place)[1]
                return angle_between(sub(pos[lower], pos[upper]), sub(pos[hand], pos[lower]))

            seen = [bend(local, place) for _, local, place in out]
            own = sum(seen) / len(seen)
            for (_, local, place), b in zip(out, seen):
                rot, pos = self.posed_world(local, place)
                hinge = cross(sub(pos[lower], pos[upper]), sub(pos[hand], pos[lower]))
                if length(hinge) < 1e-9:
                    continue
                opening = b - (math.radians(mean) + (b - own) * share)
                fore = qnorm(qmul(qaxis_angle(hinge, -opening), rot[lower]))
                local[lower] = qnorm(qmul(qinv(rot[upper]), fore))
            opened.append(math.degrees(own) - mean)
        return out, (sum(opened) / len(opened) if opened else 0.0)

    def spread_out(self, local, place, angle):
        """`local` with both upper arms swung `angle` degrees out from the
        body, about its front (+z) at the shoulder; the forearms and hands
        keep their turn under them, so the swing stays as it was, only
        wider."""
        if not angle:
            return local
        tk = self.target
        rot, _ = self.posed_world(local, place)
        local = dict(local)
        for s, sign in (("l", 1.0), ("r", -1.0)):
            if f"upperarm_{s}" not in tk.index:
                continue
            upper = tk.find(f"upperarm_{s}")
            out = qmul(qaxis_angle((0.0, 0.0, 1.0), math.radians(sign * angle)), rot[upper])
            local[upper] = qnorm(qmul(qinv(rot[tk.parent[upper]]), out))
        return local

    def cleared(self, keys, clearance, move=None, step=REACH_STEP, most=REACH_MAX, what=("reach", "m")):
        """`keys` with the arms moved by the least multiple of `step` (up to
        `most`) that keeps the forearms and hands of every `clearance` model
        in front of its chest at every key, and that amount. The move is
        ``reach_out`` (meters forward) unless another is given, as
        ``spread_out`` (degrees out). Each amount is first tried on every
        ``CLEAR_EVERY``-th key, which rules most out cheaply, then on them
        all."""
        move = move or self.reach_out
        tk = self.target
        root = self.t_rest_r[self.root]
        places = [add(qrot(root, p), self.t_rest_p[self.root]) for _, _, p in keys]
        steps = int(round(most / step))

        def clear(amount, sample):
            poses = [(move(keys[k][1], places[k], amount), keys[k][2]) for k in sample]
            return clearance.clear(tk, poses)

        for k in range(steps + 1):
            amount = k * step
            if clear(amount, range(0, len(keys), CLEAR_EVERY)) and clear(amount, range(len(keys))):
                break
        else:
            raise RetargetError(f"no {what[0]} up to {most} {what[1]} keeps the arms out of the chest")
        moved = [(t, move(local, place, amount), p) for (t, local, p), place in zip(keys, places)]
        return moved, amount

    def clip(self, name, loop, locomotion, calm=(), clearance=None, upright=False, spread=None):
        """One clip retargeted: key times, per-key local rotations by node,
        per-key pelvis local translations, and its measures. With a
        `clearance` (a ``Clearance``), the arms come forward until they
        clear its models' chests; `upright`, a walk is stood up and its
        stride lengthened (see ``upright``), and its speed measured on the
        result; with a `spread` (a ``Clearance``), the upper arms
        swing out until the hands clear its models' chests (see
        ``SPREAD``)."""
        times, rots, trans = self.source.frames(name)
        sk = self.source.skeleton
        posed = [sk.world(r, t) for r, t in zip(rots, trans)]
        pelvis_s = self.src[self.target.find("pelvis")]
        start = times[0]
        span = times[-1] - start
        drift = (0.0, 0.0, 0.0)
        if loop:
            first, last = posed[0][1][pelvis_s], posed[-1][1][pelvis_s]
            drift = (last[0] - first[0], 0.0, last[2] - first[2])
        raw = []
        for t, (s_rot, s_pos) in zip(times, posed):
            back = scale(drift, (t - start) / span)
            s_pos = [sub(p, back) for p in s_pos]
            local, place = self.pose(s_rot, s_pos)
            if calm:
                local, place = self.calm(local, place, calm)
            raw.append((t - start, local, place))
        if "arms" in calm:
            raw = self.hang_arms(raw)
        if upright:
            raw = self.upright(raw)
        keys = []
        for t, local, place in raw:
            place = add(place, (0.0, self.below_ground(local, place), 0.0))
            keys.append((t, local, place))
        measures = {}
        if locomotion:
            if upright:
                speed = self.target_ground_speed(keys) + drift[2] * self.ratio / span
            else:
                speed = ground_speed(self.source, times, posed, drift) * self.ratio
            measures.update(ground_speed=round(speed, 4), stride=round(speed * span, 4))
        keys = [(t, local, self.pelvis_local(place)) for t, local, place in keys]
        if clearance is not None:
            keys, ahead = self.cleared(keys, clearance)
            measures["reach"] = round(ahead, 4)
        if spread is not None:
            keys, angle = self.cleared(keys, spread, self.spread_out, SPREAD_STEP, SPREAD_MAX, ("spread", "degrees"))
            measures["spread"] = round(angle, 4)
        return keys, {k: measures[k] for k in ("reach", "spread", "ground_speed", "stride") if k in measures}

    def target_ground_speed(self, keys):
        """How fast a planted foot of the *target* slides back, m/s at the
        target's own scale, posed by `keys` (time, local rotations, pelvis
        world place) that are already held in place: ``ground_speed``'s
        median measure, on the target's own ankles and balls, with no
        ratio to apply and no drift to take out (the caller adds the loop's
        drift speed, scaled to the target). Used for a clip changed on the
        target (``UPRIGHT``), whose feet no longer move as the source's."""
        tk = self.target
        points = [tk.find(f"{bone}_{s}") for bone in ("foot", "ball") for s in ("l", "r")]
        posed = [self.posed_world(local, place)[1] for _, local, place in keys]
        lowest = {p: min(pos[p][1] for pos in posed) for p in points}
        slides = []
        for k in range(len(keys) - 1):
            dt = keys[k + 1][0] - keys[k][0]
            for p in points:
                if max(posed[k][p][1], posed[k + 1][p][1]) > lowest[p] + CONTACT:
                    continue
                slides.append(-(posed[k + 1][p][2] - posed[k][p][2]) / dt)
        if not slides:
            raise RetargetError("no foot on the ground to measure the ground speed")
        slides.sort()
        middle = len(slides) // 2
        return slides[middle] if len(slides) % 2 else (slides[middle - 1] + slides[middle]) / 2.0

    def upright(self, keys):
        """A walk (``UPRIGHT``) stood up from the pack's crouched, hurried
        game walk into an easy 1919 street walk, key by key (`keys`: time,
        local rotations, pelvis world place):

        - the stride lengthened ``STRIDE`` times: each ankle's place ahead
          of or behind its hip is stretched, at the same height, so the clip
          covers more ground per step and plays slower in the game;
        - the knees straightened by ``KNEE_EASE`` (never below
          ``KNEE_KEEP_WALK`` of their bend, nor ``KNEE_MIN`` degrees), the
          hips rising as the legs ask, blended by which stands and smoothed
          round the loop (``_strides``): the standing leg nearly straight,
          the swinging one still folding;
        - the torso stood up to a mean lean of ``LEAN`` degrees, turned
          back about its base (``spine_01``), its sway through the stride
          kept;
        - the shoulders down and relaxed: the clavicles at rest;
        - the upper arms hanging from the shoulders as at rest, swinging
          about it between ``ARM_SWING`` degrees back and forward, in step
          with the pack's swing; the elbows bent between ``ELBOW``
          degrees, most on the forward swing;
        - the head level: turned back to its rest pitch."""
        tk = self.target
        f = tk.find
        out = self._strides(keys)
        # The torso's lean and the arms' swing, over the whole cycle.
        leans, swings, bends = [], {"l": [], "r": []}, {"l": [], "r": []}
        for _, local, place in out:
            _, pos = self.posed_world(local, place)
            v = sub(pos[f("neck_01")], pos[f("pelvis")])
            leans.append(math.atan2(v[2], v[1]))
            for s in "lr":
                u = sub(pos[f(f"lowerarm_{s}")], pos[f(f"upperarm_{s}")])
                swings[s].append(math.atan2(u[2], -u[1]))
                bends[s].append(angle_between(u, sub(pos[f(f"hand_{s}")], pos[f(f"lowerarm_{s}")])))
        back = sum(leans) / len(leans) - math.radians(LEAN)
        result = []
        for k, (t, local, place) in enumerate(out):
            local = self._stand_up(local, place, back)
            for s in "lr":
                swing = remap(swings[s][k], swings[s], [math.radians(a) for a in ARM_SWING])
                bend = remap(bends[s][k], bends[s], [math.radians(a) for a in ELBOW])
                local = self._swing_arm(local, place, s, swing, bend)
            local = self._level_head(local, place)
            result.append((t, local, place))
        return result

    def _legs(self, local, place):
        """What the stride step needs of one key: each side's hip, knee and
        ankle, the ankle's goal (stretched ``STRIDE`` times ahead of or
        behind its hip), the thigh and shin lengths, and the hips' rise
        that would give that leg its eased bend (None when it cannot reach
        its goal at any rise)."""
        tk = self.target
        f = tk.find
        rot, pos = self.posed_world(local, place)
        legs = {}
        for s in "lr":
            hip, knee, ankle = pos[f(f"thigh_{s}")], pos[f(f"calf_{s}")], pos[f(f"foot_{s}")]
            goal = (ankle[0], ankle[1], hip[2] + STRIDE * (ankle[2] - hip[2]))
            a, b = length(tk.rest_t[f(f"calf_{s}")]), length(tk.rest_t[f(f"foot_{s}")])
            bend = angle_between(sub(knee, hip), sub(ankle, knee))
            eased = max(bend - math.radians(KNEE_EASE), KNEE_KEEP_WALK * bend, math.radians(KNEE_MIN))
            reach = math.sqrt(a * a + b * b + 2 * a * b * math.cos(eased))
            across = (goal[0] - hip[0]) ** 2 + (goal[2] - hip[2]) ** 2
            rise = goal[1] + math.sqrt(reach * reach - across) - hip[1] if reach * reach > across else None
            legs[s] = Leg(hip, knee, ankle, goal, a, b, rise, reach)
        return rot, legs

    def _strides(self, keys):
        """`keys` (a loop) with the stride lengthened and the knees eased
        (see ``upright``). The hips' rise is a blend of what each leg asks,
        weighed by how much nearer the ground its ankle is (``STAND_BLEND``),
        never a switch from one leg to the other; then smoothed round the
        loop (``RISE_SMOOTH`` keys either side, the last key the first
        again), so the hips never step. A leg that cannot reach its goal at
        that height reaches as far as its eased bend lets it (at least
        ``KNEE_MIN``), its foot lifting a little: no knee is pulled
        straighter than its own eased bend, so none locks or snaps."""
        solved = [self._legs(local, place) for _, local, place in keys]
        rises = []
        for _, legs in solved:
            w = {
                s: 1.0 / (1.0 + math.exp((legs[s].ankle[1] - legs[o].ankle[1]) / STAND_BLEND))
                for s, o in (("l", "r"), ("r", "l"))
            }
            known = [s for s in "lr" if legs[s].rise is not None]
            if not known:
                rises.append(None)
                continue
            total = sum(w[s] for s in known)
            rises.append(sum(w[s] * legs[s].rise for s in known) / total)
        n = len(rises) - 1 if len(rises) > 2 else len(rises)
        ring = fill_round(rises[:n])
        smooth = [
            sum(ring[(k + j) % n] for j in range(-RISE_SMOOTH, RISE_SMOOTH + 1)) / (2 * RISE_SMOOTH + 1)
            for k in range(n)
        ]
        smooth += [smooth[0]] * (len(rises) - n)
        out = []
        for (t, local, place), (rot, legs), rise in zip(keys, solved, smooth):
            out.append((t,) + self._solve_legs(dict(local), place, rot, legs, rise))
        return self._smooth_swings(out)

    def _smooth_swings(self, keys):
        """`keys` (a loop) with each knee's bend smoothed round the loop
        (a 1-2-1 average, ``SWING_SMOOTH`` times) while its foot is off the
        ground (its ankle over ``CONTACT_LIFT`` above its lowest): the
        swinging shin folds and unfolds at an even pace, never in a flick.
        A foot on the ground keeps its place."""
        tk = self.target
        f = tk.find
        n = len(keys) - 1 if len(keys) > 2 else len(keys)
        posed = [self.posed_world(local, place) for _, local, place in keys]
        out = [(t, dict(local), place) for t, local, place in keys]
        for s in "lr":
            thigh, calf = f(f"thigh_{s}"), f(f"calf_{s}")
            ankle = f(f"foot_{s}")
            bend = [angle_between(sub(pos[calf], pos[thigh]), sub(pos[ankle], pos[calf])) for _, pos in posed[:n]]
            low = min(pos[ankle][1] for _, pos in posed[:n])
            smooth = list(bend)
            for _ in range(SWING_SMOOTH):
                smooth = [(smooth[(k - 1) % n] + 2 * smooth[k] + smooth[(k + 1) % n]) / 4 for k in range(n)]
            for k in range(n):
                rot, pos = posed[k]
                lift = pos[ankle][1] - low
                if lift <= CONTACT_LIFT:
                    continue
                # Fade in over the next 2 cm, so the foot leaves the ground
                # as the clip lifts it.
                w = min(1.0, (lift - CONTACT_LIFT) / 0.02)
                turn = w * (smooth[k] - bend[k])
                hinge = cross(sub(pos[calf], pos[thigh]), sub(pos[ankle], pos[calf]))
                if length(hinge) < 1e-9:
                    continue  # a straight knee: no hinge to turn about
                hinge = unit(hinge)
                calf_w = qnorm(qmul(qaxis_angle(hinge, turn), rot[calf]))
                local = out[k][1]
                local[calf] = qnorm(qmul(qinv(rot[thigh]), calf_w))
        if len(keys) > n:
            out[-1] = (out[-1][0], dict(out[0][1]), out[-1][2])
        return out

    def _solve_legs(self, local, place, rot, legs, rise):
        tk = self.target
        f = tk.find
        place = add(place, (0.0, rise, 0.0))
        pelvis_rot = rot[f("pelvis")]
        for s in "lr":
            hip, knee, ankle, goal, a, b, _, reach = legs[s]  # a Leg
            h = add(hip, (0.0, rise, 0.0))
            u = unit(sub(goal, h))
            # Never straighter than its eased bend: hips set higher than
            # this leg asked lift its foot instead of locking its knee.
            d = min(max(length(sub(goal, h)), abs(a - b) + 1e-9), reach)
            end = add(h, scale(u, d))
            cos_hip = (a * a + d * d - b * b) / (2 * a * d)
            # The knee bends the way it did, square to the hip-to-ankle line.
            pole = sub(knee, hip)
            pole = sub(pole, scale(u, sum(x * y for x, y in zip(pole, u))))
            if length(pole) < 1e-6:
                ahead = (0.0, 0.0, 1.0)
                pole = sub(ahead, scale(u, sum(x * y for x, y in zip(ahead, u))))
            pole = unit(pole)
            kn = add(h, scale(add(scale(u, cos_hip), scale(pole, math.sqrt(max(0.0, 1 - cos_hip**2)))), a))
            thigh, calf, foot = f(f"thigh_{s}"), f(f"calf_{s}"), f(f"foot_{s}")
            thigh_w = qnorm(qmul(qarc(unit(sub(knee, hip)), unit(sub(kn, h))), rot[thigh]))
            calf_w = qnorm(qmul(qarc(unit(sub(ankle, knee)), unit(sub(end, kn))), rot[calf]))
            local[thigh] = qnorm(qmul(qinv(pelvis_rot), thigh_w))
            local[calf] = qnorm(qmul(qinv(thigh_w), calf_w))
            local[foot] = qnorm(qmul(qinv(calf_w), rot[foot]))
        return local, place

    def _stand_up(self, local, place, back):
        """`local` with the torso turned back by `back` radians about the
        body's side axis at its base, and the clavicles at rest."""
        tk = self.target
        f = tk.find
        rot, _ = self.posed_world(local, place)
        local = dict(local)
        base = f("spine_01")
        turned = qnorm(qmul(qaxis_angle((1.0, 0.0, 0.0), -back), rot[base]))
        local[base] = qnorm(qmul(qinv(rot[tk.parent[base]]), turned))
        for s in "lr":
            local[f(f"clavicle_{s}")] = tk.rest_r[f(f"clavicle_{s}")]
        return local

    def _swing_arm(self, local, place, s, swing, bend):
        """`local` with side `s`'s upper arm hanging as at rest, swung
        `swing` radians forward about the body's side axis, and its elbow
        bent `bend` radians forward; the hand keeps its turn on the
        forearm."""
        tk = self.target
        f = tk.find
        upper, lower = f(f"upperarm_{s}"), f(f"lowerarm_{s}")
        rest_r, rest_p = self.t_rest_r, self.t_rest_p
        hang = unit(sub(rest_p[lower], rest_p[upper]))
        # The rest's pitch, but hanging only ``WALK_OUT`` out.
        side = 1.0 if s == "l" else -1.0
        hang = unit((side * math.tan(math.radians(WALK_OUT)), -1.0, hang[2] / -hang[1]))
        want = qrot(qaxis_angle((1.0, 0.0, 0.0), -swing), hang)
        rot, pos = self.posed_world(local, place)
        have = unit(sub(pos[lower], pos[upper]))
        upper_w = qnorm(qmul(qarc(have, want), rot[upper]))
        local = dict(local)
        local[upper] = qnorm(qmul(qinv(rot[tk.parent[upper]]), upper_w))
        rot, pos = self.posed_world(local, place)
        forward = sub((0.0, 0.0, 1.0), scale(want, want[2]))
        forward = unit(forward) if length(forward) > 1e-6 else (0.0, 0.0, 1.0)
        fore_want = add(scale(want, math.cos(bend)), scale(forward, math.sin(bend)))
        hand = f(f"hand_{s}")
        fore_have = unit(sub(pos[hand], pos[lower]))
        lower_w = qnorm(qmul(qarc(fore_have, unit(fore_want)), rot[lower]))
        local[lower] = qnorm(qmul(qinv(rot[upper]), lower_w))
        return local

    def _level_head(self, local, place):
        """`local` with the head turned back to its rest pitch (about the
        body's side axis), keeping its turn aside."""
        tk = self.target
        head = tk.find("head")
        rot, _ = self.posed_world(local, place)
        up = qrot(rot[head], (0.0, 1.0, 0.0))
        rest = qrot(self.t_rest_r[head], (0.0, 1.0, 0.0))
        pitch = math.atan2(up[2], up[1]) - math.atan2(rest[2], rest[1])
        turned = qnorm(qmul(qaxis_angle((1.0, 0.0, 0.0), -pitch), rot[head]))
        local = dict(local)
        local[head] = qnorm(qmul(qinv(rot[tk.parent[head]]), turned))
        return local


CONTACT = 0.01

# --- chest clearance (numpy) ---------------------------------------------------

ARM_BONES = ("lowerarm_", "hand_", "thumb_", "index_", "middle_", "ring_", "pinky_")
ARM_FIRM = 0.75
TORSO_BONES = ("pelvis", "spine_01", "spine_02", "spine_03")


class Clearance:
    """The character models whose chests the contact clips must clear:
    each one's skeleton, its skinned vertices split into forearms and hands
    and torso (by each vertex's strongest joint; clothes included), and its
    torso triangles. Uses numpy, as tools/mh2gltf.py does; only this step
    needs it. The models are our own converter's output, still read with
    every index checked."""

    def __init__(self, paths, talkers=()):
        try:
            import numpy as np
        except ImportError as e:
            raise RetargetError(f"--model needs numpy: {e}") from None
        self.np = np
        self.models = []
        for path, clips in [(p, CLEAR + SPREAD) for p in paths] + [(p, TALK) for p in talkers]:
            try:
                self.models.append(self._read(path))
                self.models[-1]["clips"] = clips
            except (json.JSONDecodeError, UnicodeDecodeError, ValueError) as e:
                # One line, like every other bad input (a malformed file's
                # JSON or text, or an accessor that is not whole vectors).
                raise RetargetError(f"{path}: {type(e).__name__}: {e}".splitlines()[0]) from None

    def _read(self, path):
        np = self.np
        doc = json.loads(read_file(path, MAX_TARGET_BYTES).decode("utf-8"))
        buffers = glbtrim.items(doc, "buffers")
        uri = buffers[0].get("uri") if len(buffers) == 1 else None
        if not isinstance(uri, str) or "/" in uri or "\\" in uri or uri.startswith("."):
            raise RetargetError(f"{path}: one buffer in a plain file beside it is expected")
        binary = read_file(os.path.join(os.path.dirname(path), uri), glbtrim.MAX_INPUT_BYTES)
        if buffers[0].get("byteLength") != len(binary):
            raise RetargetError(f"{path}: its buffer is not {uri}'s size")
        skeleton = Skeleton(doc)

        def read(a):
            return np.array(glbtrim.read_accessor(doc, binary, a), dtype=float)

        skins = glbtrim.items(doc, "skins")
        if len(skins) != 1:
            raise RetargetError(f"{path}: one skin is expected")
        joints = [glbtrim.index(j, len(skeleton.names), "skin joint") for j in skins[0].get("joints", [])]
        binds = read(skins[0].get("inverseBindMatrices")).reshape(-1, 4, 4).transpose(0, 2, 1)
        if len(binds) != len(joints):
            raise RetargetError(f"{path}: one inverse bind matrix per joint is expected")
        meshes = glbtrim.items(doc, "meshes")
        points, influences, weights, triangles = [], [], [], []
        base = 0
        for prim in meshes[0].get("primitives", []) if meshes else []:
            at = prim.get("attributes", {})
            pos = read(at.get("POSITION"))
            jnt = read(at.get("JOINTS_0")).astype(int)
            if jnt.size and (jnt.min() < 0 or jnt.max() >= len(joints)):
                raise RetargetError(f"{path}: a vertex's joint is out of range")
            tri = read(prim.get("indices")).astype(int).reshape(-1, 3)
            if tri.size and (tri.min() < 0 or tri.max() >= len(pos)):
                raise RetargetError(f"{path}: a triangle's corner is out of range")
            points.append(pos)
            influences.append(jnt)
            weights.append(read(at.get("WEIGHTS_0")))
            triangles.append(tri + base)
            base += len(pos)
        if not points:
            raise RetargetError(f"{path}: no mesh")
        points, influences, weights = np.vstack(points), np.vstack(influences), np.vstack(weights)
        triangles = np.vstack(triangles)
        names = [skeleton.names[j] for j in joints]
        strongest = [names[j] for j in influences[np.arange(len(weights)), weights.argmax(axis=1)]]
        # The forearms and hands, not the blend round the elbow: bent as far
        # as folded arms bend it, linear skinning pinches the inside of the
        # elbow into the body whatever the arm's place.
        firm = weights.max(axis=1) >= ARM_FIRM
        arm = np.array([n.startswith(ARM_BONES) for n in strongest]) & firm
        torso = np.array([n in TORSO_BONES for n in strongest])
        leg = leg_length(skeleton, "calf_l", "foot_l")
        return {
            "name": os.path.basename(os.path.dirname(path)) or path,
            "skeleton": skeleton,
            "joints": joints,
            "binds": binds,
            "points": points,
            "influences": influences,
            "weights": weights,
            "arm": arm,
            "torso": triangles[torso[triangles].all(axis=1)],
            "leg": leg,
        }

    def for_clip(self, game):
        """These models, only those that play the clip ``game``; None when
        no model does."""
        out = copy.copy(self)
        out.models = [m for m in self.models if game in m["clips"]]
        return out if out.models else None

    def skinned(self, model, reference, local, pelvis):
        """`model`'s vertices posed by `local` (game_engine bone name ->
        local rotation) with the clip's pelvis place `pelvis` (fitted to
        its legs as the game does), and its bones' world places by name."""
        np = self.np
        sk = model["skeleton"]
        rots, trans = list(sk.rest_r), list(sk.rest_t)
        for name, q in local.items():
            if name in sk.index:
                rots[sk.find(name)] = q
        k = sk.find("pelvis")
        ref_place, ref_leg = reference
        trans[k] = add(sk.rest_t[k], scale(sub(pelvis, ref_place), model["leg"] / ref_leg))
        w_rot, w_pos = sk.world(rots, trans)
        mats = []
        for j, bind in zip(model["joints"], model["binds"]):
            x, y, z, w = w_rot[j]
            m = np.identity(4)
            m[:3, :3] = [
                [1 - 2 * (y * y + z * z), 2 * (x * y - z * w), 2 * (x * z + y * w)],
                [2 * (x * y + z * w), 1 - 2 * (x * x + z * z), 2 * (y * z - x * w)],
                [2 * (x * z - y * w), 2 * (y * z + x * w), 1 - 2 * (x * x + y * y)],
            ]
            m[:3, 3] = w_pos[j]
            mats.append(m @ bind)
        mats = np.array(mats)[model["influences"]]
        blend = np.einsum("vk,vkij->vij", model["weights"], mats)
        out = np.einsum("vij,vj->vi", blend[:, :3, :3], model["points"]) + blend[:, :3, 3]
        return out, {n: w_pos[i] for i, n in enumerate(sk.names)}

    def depth(self, model, reference, local, pelvis):
        """How far `model`'s forearms and hands, posed so, are inside its
        chest or nearer its front than ``CLEARANCE`` (meters; negative:
        clear by that much more). The chest is the torso between spine_02
        and the neck, within 10 cm either side of its middle, sampled along
        its normal (the body's front, +z) through each arm vertex in it."""
        np = self.np
        p, bones = self.skinned(model, reference, local, pelvis)
        low, high = bones["spine_02"][1], bones["neck_01"][1]
        middle = bones["spine_03"][0]
        a = p[model["arm"]]
        a = a[(a[:, 1] > low) & (a[:, 1] < high) & (np.abs(a[:, 0] - middle) < 0.1)]
        if not len(a):
            return -1.0
        t = model["torso"]
        A, B, C = p[t[:, 0]], p[t[:, 1]], p[t[:, 2]]
        d = (B[:, 0] - A[:, 0]) * (C[:, 1] - A[:, 1]) - (C[:, 0] - A[:, 0]) * (B[:, 1] - A[:, 1])
        ok = np.abs(d) > 1e-12
        A, B, C, d = A[ok], B[ok], C[ok], d[ok]
        x, y = a[:, 0:1], a[:, 1:2]
        u = ((x - A[:, 0]) * (C[:, 1] - A[:, 1]) - (C[:, 0] - A[:, 0]) * (y - A[:, 1])) / d
        v = ((B[:, 0] - A[:, 0]) * (y - A[:, 1]) - (x - A[:, 0]) * (B[:, 1] - A[:, 1])) / d
        hit = (u >= 0) & (v >= 0) & (u + v <= 1)
        z = A[:, 2] + u * (B[:, 2] - A[:, 2]) + v * (C[:, 2] - A[:, 2])
        front = np.where(hit, z, -np.inf).max(axis=1)
        back = np.where(hit, z, np.inf).min(axis=1)
        near = np.isfinite(front) & (a[:, 2] > back)
        if not near.any():
            return -1.0
        return float((front + CLEARANCE - a[:, 2])[near].max())

    def clear(self, target, poses):
        """Whether every model's forearms and hands clear its chest in each
        of `poses` (local rotations by `target` node, pelvis local place)."""
        k = target.find("pelvis")
        reference = (target.rest_t[k], leg_length(target, "calf_l", "foot_l"))
        for local, pelvis in poses:
            named = {target.names[i]: q for i, q in local.items()}
            for model in self.models:
                if self.depth(model, reference, named, pelvis) > 0.0:
                    return False
        return True


def ground_speed(source, times, posed, drift):
    """The ground speed (m/s, at the *source*'s scale: the caller multiplies
    it by the leg ratio) a loop shows, measured on the source's own posed
    keys, which still carry the loop's `drift` (taken out of each slide,
    then added back as its speed): how fast a point
    of a foot on the ground slides back once the drift is taken out, plus
    the drift. The points are each ankle and each ball of the foot; one is
    on the ground while it is within ``CONTACT`` of its lowest height in the
    clip, so a heel rolling up or a toe swinging through never counts. The
    median of those slides is the speed."""
    sk = source.skeleton
    m = source.map
    points = [sk.find(m[f"{bone}_{s}"]) for bone in ("foot", "ball") for s in ("l", "r")]
    span = times[-1] - times[0]
    forward = drift[2] / span
    lowest = {p: min(pos[p][1] for _, pos in posed) for p in points}
    slides = []
    for k in range(len(times) - 1):
        (_, p0), (_, p1) = posed[k], posed[k + 1]
        dt = times[k + 1] - times[k]
        for p in points:
            if max(p0[p][1], p1[p][1]) > lowest[p] + CONTACT:
                continue
            dz = (p1[p][2] - p0[p][2]) - drift[2] * dt / span
            slides.append(-dz / dt)
    if not slides:
        raise RetargetError("no foot on the ground to measure the ground speed")
    slides.sort()
    middle = len(slides) // 2
    median = slides[middle] if len(slides) % 2 else (slides[middle - 1] + slides[middle]) / 2.0
    return forward + median


# --- output -----------------------------------------------------------------


def build(sources, target, clips=CLIPS, clearance=None):
    """The output GLB's JSON document and binary chunk, and a summary. With
    a `clearance`, the ``CLEAR`` clips' arms are brought clear of its
    models' chests."""
    retargeters = {kind: Retargeter(src, target) for kind, src in sources.items()}
    for rt in retargeters.values():
        if rt.source.partial:
            kind, clip = HANDS
            first = retargeters[kind].clip(clip, True, False)[0][0][1]
            rt.held = {k: first[k] for k in rt.bones if k not in rt.src}
    tk = target
    root = tk.find("Root")
    order = []

    def visit(k):
        order.append(k)
        for c in tk.children[k]:
            visit(c)

    visit(root)
    out_index = {k: i for i, k in enumerate(order)}
    nodes = []
    for k in order:
        node = {"name": tk.names[k]}
        if any(tk.rest_t[k]):
            node["translation"] = list(tk.rest_t[k])
        if tk.rest_r[k] != (0.0, 0.0, 0.0, 1.0):
            node["rotation"] = list(tk.rest_r[k])
        if tk.children[k]:
            node["children"] = [out_index[c] for c in tk.children[k]]
        nodes.append(node)

    blob = bytearray()
    views, accessors, animations, summary = [], [], [], []

    def put(values, kind, minmax=False):
        fmt = "<" + "f" * len(values[0])
        view = {"buffer": 0, "byteOffset": len(blob), "byteLength": 0}
        for v in values:
            blob.extend(struct.pack(fmt, *v))
        view["byteLength"] = len(blob) - view["byteOffset"]
        views.append(view)
        accessor = {
            "bufferView": len(views) - 1,
            "componentType": 5126,
            "count": len(values),
            "type": kind,
        }
        if minmax:
            stored = [struct.unpack(fmt, struct.pack(fmt, *v)) for v in values]
            accessor["min"] = [min(c) for c in zip(*stored)]
            accessor["max"] = [max(c) for c in zip(*stored)]
        accessors.append(accessor)
        return len(accessors) - 1

    pelvis = tk.find("pelvis")
    for game, kind, name, loop, locomotion in clips:
        rt = retargeters[kind]
        clear = clearance.for_clip(game) if clearance is not None and game in CLEAR else None
        spread = clearance.for_clip(game) if clearance is not None and game in SPREAD else None
        keys, measures = rt.clip(
            name, loop, locomotion, CALM.get(game, ()), clear, upright=game in UPRIGHT, spread=spread,
        )
        times = put([(t,) for t, _, _ in keys], "SCALAR", minmax=True)
        samplers, channels = [], []
        for k in sorted(rt.bones, key=lambda b: out_index[b]):
            prev = None
            values = []
            for _, local, _ in keys:
                prev = qlerp_sign(prev, local[k])
                values.append(prev)
            samplers.append({"input": times, "output": put(values, "VEC4"), "interpolation": "LINEAR"})
            channels.append({"sampler": len(samplers) - 1, "target": {"node": out_index[k], "path": "rotation"}})
            if k == pelvis:
                places = [p for _, _, p in keys]
                samplers.append({"input": times, "output": put(places, "VEC3"), "interpolation": "LINEAR"})
                channels.append(
                    {"sampler": len(samplers) - 1, "target": {"node": out_index[k], "path": "translation"}}
                )
        extras = {"source": f"{kind} {name}", "loop": loop}
        if game in CALM:
            extras["calmed"] = list(CALM[game])
        if game in UPRIGHT:
            extras["upright"] = True
        extras.update(measures)
        animations.append({"name": game, "channels": channels, "samplers": samplers, "extras": extras})
        summary.append((game, kind, name, keys[-1][0], len(keys), measures))

    doc = {
        "asset": {"version": "2.0", "generator": GENERATOR},
        "scene": 0,
        "scenes": [{"name": "humanoid", "nodes": [0]}],
        "nodes": nodes,
        "animations": animations,
        "buffers": [{"byteLength": len(blob)}],
        "bufferViews": views,
        "accessors": accessors,
        "extras": {
            "leg_length": round(leg_length(tk, "calf_l", "foot_l"), 6),
        },
    }
    return doc, bytes(blob), summary


def read_file(path, limit):
    with open(path, "rb") as f:
        data = f.read(limit + 1)
    if len(data) > limit:
        raise RetargetError(f"{path}: over {limit} bytes")
    return data


def write_atomically(path, data):
    """Writes `data` to `path` all at once: into a new file beside it, then
    renamed over it, so a failed or interrupted run never leaves half a
    file (the old one stays as it was)."""
    temporary = f"{path}.{os.getpid()}.tmp"
    try:
        with open(temporary, "xb") as f:
            f.write(data)
            f.flush()
            os.fsync(f.fileno())
        os.replace(temporary, path)
    except BaseException:
        try:
            os.remove(temporary)
        except OSError:
            pass
        raise


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    parser.add_argument("--ual1", required=True, help="UAL1 AnimationLibrary_Godot_Standard.glb")
    parser.add_argument("--ual2", required=True, help="UAL2 UAL2_Standard.glb")
    parser.add_argument("--cmu", required=True, help="the run: run_cycle.bvh, from tools/run_cycle.py (CMU joint names)")
    parser.add_argument("--sprint", required=True, help="the sprint: sprint_cycle.bvh, from tools/run_cycle.py --gait sprint")
    parser.add_argument("--target", required=True, help="a character's model.gltf (its skeleton)")
    parser.add_argument("-o", "--output", required=True, help="the animation GLB to write")
    parser.add_argument(
        "--model",
        action="append",
        default=[],
        help="a character's model.gltf whose chest the arms must clear (repeat for each)",
    )
    parser.add_argument(
        "--talker",
        action="append",
        default=[],
        help="a model.gltf whose chest only the talking hands must clear (repeat for each)",
    )
    args = parser.parse_args(argv)
    try:
        sources = {
            "ual1": Source(read_file(args.ual1, glbtrim.MAX_INPUT_BYTES), "ual1"),
            "ual2": Source(read_file(args.ual2, glbtrim.MAX_INPUT_BYTES), "ual2"),
            "cmu": BvhSource(
                read_file(args.cmu, glbtrim.MAX_INPUT_BYTES), os.path.splitext(os.path.basename(args.cmu))[0]
            ),
            "sprint": BvhSource(
                read_file(args.sprint, glbtrim.MAX_INPUT_BYTES), os.path.splitext(os.path.basename(args.sprint))[0]
            ),
        }
        target = read_target(read_file(args.target, MAX_TARGET_BYTES))
        clearance = Clearance(sorted(args.model), sorted(args.talker)) if args.model or args.talker else None
        doc, blob, summary = build(sources, target, clearance=clearance)
    except (RetargetError, glbtrim.TrimError, OSError) as e:
        print(f"retarget: {e}", file=sys.stderr)
        return 1
    data = glbtrim.write_glb(doc, blob)
    try:
        write_atomically(args.output, data)
    except OSError as e:
        print(f"retarget: {e}", file=sys.stderr)
        return 1
    print(f"{args.output}: {len(data)} bytes, {len(summary)} clips")
    for game, kind, name, duration, count, measures in summary:
        extra = ""
        if "ground_speed" in measures:
            extra = f"  {measures['ground_speed']:.3f} m/s, stride {measures['stride']:.3f} m"
        if "reach" in measures:
            extra += f"  arms {100 * measures['reach']:.1f} cm forward to clear the chests"
        if "elbows" in measures:
            extra += f"  elbows {measures['elbows']:.0f} degrees open"
        if "spread" in measures:
            extra += f"  arms {measures['spread']:.0f} degrees out to clear the chests"
        print(f"  {game:<15} {kind} {name:<20} {duration:6.3f} s {count:4d} keys{extra}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
