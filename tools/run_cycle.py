#!/usr/bin/env python3
"""A runner's stride cycle, keyed from real runners' measurements onto the
protagonist's own skeleton, written as a BVH capture for ``retarget.py``.

The run follows the drawn eight-pose cycle (contact, down, passing, up and
flight, for each foot) with each joint's angles taken from treadmill
runners near the game's run speed (Fukuchi, Fukuchi & Duarte 2017, PeerJ
5:e3298, Table 4, 4.5 m/s: hip from 60.5 degrees flexed to 11.8 extended,
the swinging knee up to 119, 91.7 strides a minute; Novacheck 1998, Gait &
Posture 7:77: toe-off at about a third of the cycle):

- the planted foot stays put on the ground, flat from just after contact,
  then rolling up onto the ball of the foot until toe-off; the leg lands
  nearly straight, gives under the body and straightens as it pushes off
  (``STANCE_KNEE``), the hips riding at the height that asks, and between
  toe-off and the other foot's contact the body flies as a thrown one;
- the swinging leg's thigh, knee and ankle follow knots read off the
  measured curves (``SWING``), joined to the stance at both ends;
- the pelvis turns with the swinging leg, drops on the swinging side and
  sways over the planted foot; the chest turns against it, the trunk leans
  ``LEAN`` degrees forward and the head holds its gaze;
- each arm swings against its own side's leg, the elbow near square, more
  bent in front, the forearm crossing in toward the middle but not over it.

The skeleton is the target's (``--target``), so the capture's joints sit
where the target's do and its rest is the target's rest: ``retarget.py``
reads it through ``BvhSource`` as it reads a CMU trial (the same joint
names, Y up, facing +Z, the left at +X, in ``BVH_UNIT``). Run it isolated:

    python3 -I tools/run_cycle.py --target model.gltf -o run_cycle.bvh
"""

import argparse
import math
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))

import retarget as R  # noqa: E402


class Gait:
    """One gait's numbers (see ``JOG``); ``but`` copies it with some
    changed."""

    def __init__(self, **values):
        self.__dict__.update(values)

    def but(self, **changes):
        return Gait(**{**self.__dict__, **changes})


# The run at the player's 5 m/s.
JOG = Gait(
    # The run: speed (m/s, the game's), and the stride cycle (two steps), s: 19
    # keys at 30 a second, 189 steps a minute (Fukuchi's runners at 4.5 m/s
    # took 183, cadence rising with speed), so the stride is 3.17 m.
    SPEED=5.0,
    CYCLE_TIME=19.0 / 30.0,
    # The share of the cycle each foot is on the ground: 0.19 s. Runners near
    # 5 m/s touch down 0.16 to 0.21 s (31% of the cycle at 4.5 m/s); at 25%
    # the body flew half the cycle and the run read as bounding. On the
    # target's 0.865 m legs a longer touch has the hips sink until a foot goes
    # through the ground (33%), so the foot lands further ahead instead.
    STANCE=0.30,
    # How far ahead of its hip joint a foot lands, m (its ankle, flat), and
    # how far out of the body's middle line each foot is planted. With
    # ``STANCE`` it sets the hips' rise and fall: 7 cm, as measured.
    FOOT_AHEAD=0.36,
    FOOT_OUT=0.06,
    # The stance foot over its time on the ground (0 contact, 1 toe-off): its
    # toes up (degrees) at contact, pivoting on the heel down to flat by
    # ``HEEL_DOWN``; flat until ``HEEL_RISE``, then rolling up on the ball of
    # the foot to ``PUSH_OFF`` degrees heel up at toe-off.
    TOES_UP=6.0,
    HEEL_DOWN=0.12,
    HEEL_RISE=0.45,
    PUSH_OFF=60.0,
    # The stance knee's bend (degrees) over the stance, as (time, bend) knots:
    # landing nearly straight, giving to about 40 (the measured runners' 40 to
    # 45; a captured runner crouched at 57) and straightening as it pushes
    # off. The hips ride at the height that asks; from toe-off to the other
    # foot's contact the body flies as a thrown one (``GRAVITY``, m/s^2).
    STANCE_KNEE=((0.0, 8.0), (0.45, 38.0), (0.8, 22.0), (1.0, 6.0)),
    GRAVITY=9.81,
    # The swinging leg over its time in the air (0 toe-off, 1 contact): the
    # thigh's angle forward of hanging, the knee's bend, and the ankle's turn
    # up from its rest (degrees), as (time, value) knots; each runs from where
    # the stance left it at toe-off to where the next stance takes it. As in
    # the drawn cycle: the thigh still back under the body as the heel folds
    # up (the down pose), driven up to 56 by the other foot's push-off, then
    # the shin reaching out ahead in the flight; the foot hangs near level
    # (pointed, -25, it read as a flipper).
    SWING={
        "thigh": ((0.3, -12.0), (0.47, 2.0), (0.68, 56.0), (0.86, 36.0)),
        "knee": ((0.29, 95.0), (0.47, 112.0), (0.68, 100.0), (0.86, 28.0)),
        "ankle": ((0.12, -12.0), (0.5, -2.0), (0.85, 5.0)),
    },
    # The toes' bend at toe-off springs back over this share of the swing.
    TOES_BACK=0.12,
    # The trunk: the pelvis tilts ``PELVIS_TILT`` forward and turns
    # ``PELVIS_TURN`` (degrees, its swinging side's hip forward), its swinging
    # side dropping ``PELVIS_DROP``; the hips sway ``SWAY`` m over the planted
    # foot. The chest leans ``LEAN`` forward and turns ``CHEST_TURN`` against
    # the pelvis; the neck leans ``NECK_LEAN`` more, and the head looks
    # ``GAZE`` below its rest line, held still in the world.
    PELVIS_TILT=4.0,
    PELVIS_TURN=10.0,
    PELVIS_DROP=4.0,
    SWAY=0.015,
    LEAN=12.0,
    CHEST_TURN=15.0,
    NECK_LEAN=4.0,
    GAZE=6.0,
    # The arms (degrees): each upper arm swings from ``ARM_BACK`` behind its
    # trunk line to ``ARM_FORWARD`` in front, back ``ARM_LEAD`` of a cycle
    # before its own side's foot lands; the elbow bends ``ELBOW`` on average,
    # ``ELBOW_SWAY`` more in front and less behind; the forearm swings in
    # ``CROSS`` toward the middle. As drawn, the arms reach their ends in the
    # push-off and the flight, the fist up by the head: turning at the very
    # contact, with the legs, the whole body reversed at once, square.
    ARM_FORWARD=60.0,
    ARM_BACK=40.0,
    ELBOW=90.0,
    ELBOW_SWAY=15.0,
    CROSS=15.0,
    ARM_LEAD=0.1,
)

# The sprint: the player at full tilt, 7.5 m/s (27 km/h, a fit man's
# sprint; the game blends the jog into it from 6.5 m/s). Sprinters take
# more and longer steps and touch down shorter (Dorn et al. 2012, J Exp
# Biol 215:1944: contact time falls steadily from 3.5 to 9 m/s; Mann 2011,
# The Mechanics of Sprinting and Hurdling): 200 steps a minute, a 4.5 m
# stride, 0.12 s on the ground on the forefoot, the heel barely touching;
# the knee folds the heel up under the seat (125 degrees and more) and the
# thigh drives up nearly level; the trunk leans more, and the arms swing
# wide, the fist up to the chin in front and the elbow well behind, as in
# the drawn eight-pose cycle.
SPRINT = JOG.but(
    SPEED=7.5,
    CYCLE_TIME=18.0 / 30.0,
    STANCE=0.2,
    FOOT_AHEAD=0.30,
    TOES_UP=0.0,
    HEEL_DOWN=0.05,
    HEEL_RISE=0.2,
    STANCE_KNEE=((0.0, 12.0), (0.4, 40.0), (0.8, 25.0), (1.0, 12.0)),
    SWING={
        "thigh": ((0.4, 30.0), (0.72, 72.0), (0.88, 62.0)),
        "knee": ((0.2, 95.0), (0.42, 128.0), (0.72, 85.0), (0.9, 40.0)),
        "ankle": ((0.12, -25.0), (0.5, -5.0), (0.85, 0.0)),
    },
    LEAN=16.0,
    CHEST_TURN=18.0,
    GAZE=8.0,
    ARM_FORWARD=65.0,
    ARM_BACK=60.0,
    ELBOW=90.0,
    ELBOW_SWAY=20.0,
    ARM_LEAD=0.0,
    CROSS=10.0,
)
GAITS = {"jog": JOG, "sprint": SPRINT}

# The capture: frames a second (``BvhSource`` keeps every
# ``BVH_KEY_STEP``th), and how many cycles are run.
FPS = 120
CYCLES = 3

X = (1.0, 0.0, 0.0)
UP = (0.0, 1.0, 0.0)

# BVH joint -> (game_engine bone, parent joint), parents first; each chain
# ends at an ``End Site`` (see ``Runner.ends``).
JOINTS = [("Hips", "pelvis", None)]
for _s, _side in (("l", "Left"), ("r", "Right")):
    JOINTS += [
        (f"{_side}UpLeg", f"thigh_{_s}", "Hips"),
        (f"{_side}Leg", f"calf_{_s}", f"{_side}UpLeg"),
        (f"{_side}Foot", f"foot_{_s}", f"{_side}Leg"),
        (f"{_side}ToeBase", f"ball_{_s}", f"{_side}Foot"),
    ]
JOINTS += [
    ("LowerBack", "spine_01", "Hips"),
    ("Spine", "spine_02", "LowerBack"),
    ("Spine1", "spine_03", "Spine"),
    ("Neck1", "neck_01", "Spine1"),
    ("Head", "head", "Neck1"),
]
for _s, _side in (("l", "Left"), ("r", "Right")):
    JOINTS += [
        (f"{_side}Shoulder", f"clavicle_{_s}", "Spine1"),
        (f"{_side}Arm", f"upperarm_{_s}", f"{_side}Shoulder"),
        (f"{_side}ForeArm", f"lowerarm_{_s}", f"{_side}Arm"),
        (f"{_side}Hand", f"hand_{_s}", f"{_side}ForeArm"),
    ]


def rad(d):
    return math.radians(d)


def turn(axis, degrees):
    return R.qaxis_angle(axis, rad(degrees))


def dot(a, b):
    return a[0] * b[0] + a[1] * b[1] + a[2] * b[2]


def mat_quat(m):
    """The unit quaternion of a rotation matrix (rows)."""
    t = m[0][0] + m[1][1] + m[2][2]
    if t > 0.0:
        s = math.sqrt(t + 1.0) * 2.0
        q = ((m[2][1] - m[1][2]) / s, (m[0][2] - m[2][0]) / s, (m[1][0] - m[0][1]) / s, s / 4.0)
    elif m[0][0] > m[1][1] and m[0][0] > m[2][2]:
        s = math.sqrt(1.0 + m[0][0] - m[1][1] - m[2][2]) * 2.0
        q = (s / 4.0, (m[0][1] + m[1][0]) / s, (m[0][2] + m[2][0]) / s, (m[2][1] - m[1][2]) / s)
    elif m[1][1] > m[2][2]:
        s = math.sqrt(1.0 + m[1][1] - m[0][0] - m[2][2]) * 2.0
        q = ((m[0][1] + m[1][0]) / s, s / 4.0, (m[1][2] + m[2][1]) / s, (m[0][2] - m[2][0]) / s)
    else:
        s = math.sqrt(1.0 + m[2][2] - m[0][0] - m[1][1]) * 2.0
        q = ((m[0][2] + m[2][0]) / s, (m[1][2] + m[2][1]) / s, s / 4.0, (m[1][0] - m[0][1]) / s)
    return R.qnorm(q)


def frame(d, n):
    """The rotation whose columns are bone direction `d`, side `n` (made
    square to it) and their cross."""
    d = R.unit(d)
    n = R.unit(R.sub(n, R.scale(d, dot(n, d))))
    c = R.cross(d, n)
    return [[d[k], n[k], c[k]] for k in range(3)]


def swing_to(rest, posed):
    """The world turn taking the rest frame onto the posed one, each given
    as (direction, side)."""
    a, b = frame(*rest), frame(*posed)
    m = [[sum(b[i][k] * a[j][k] for k in range(3)) for j in range(3)] for i in range(3)]
    return mat_quat(m)


def pchip(knots, x):
    """Monotone cubic through (x, y) `knots` (sorted, no overshoot)."""
    xs, ys = [k[0] for k in knots], [k[1] for k in knots]
    if x <= xs[0]:
        return ys[0]
    if x >= xs[-1]:
        return ys[-1]
    n = len(xs)
    h = [xs[i + 1] - xs[i] for i in range(n - 1)]
    delta = [(ys[i + 1] - ys[i]) / h[i] for i in range(n - 1)]
    m = [0.0] * n
    m[0], m[-1] = delta[0], delta[-1]
    for i in range(1, n - 1):
        if delta[i - 1] * delta[i] > 0.0:
            w1, w2 = 2 * h[i] + h[i - 1], h[i] + 2 * h[i - 1]
            m[i] = (w1 + w2) / (w1 / delta[i - 1] + w2 / delta[i])
    i = max(k for k in range(n - 1) if xs[k] <= x)
    t = (x - xs[i]) / h[i]
    t2, t3 = t * t, t * t * t
    return (
        (2 * t3 - 3 * t2 + 1) * ys[i]
        + (t3 - 2 * t2 + t) * h[i] * m[i]
        + (-2 * t3 + 3 * t2) * ys[i + 1]
        + (t3 - t2) * h[i] * m[i + 1]
    )


def smoothstep(a, b, x):
    t = min(1.0, max(0.0, (x - a) / (b - a)))
    return t * t * (3.0 - 2.0 * t)


def knee_place(hip, ankle, a, b, pole):
    """Two-bone reach: the knee between `hip` and `ankle` (thigh `a`, shin
    `b` long), bent toward `pole`."""
    span = R.sub(ankle, hip)
    d = R.length(span)
    if d >= a + b:
        raise R.RetargetError(f"the leg cannot reach its foot ({d:.3f} m of {a + b:.3f})")
    u = R.scale(span, 1.0 / d)
    x = (a * a - b * b + d * d) / (2.0 * d)
    r = math.sqrt(max(0.0, a * a - x * x))
    p = R.unit(R.sub(pole, R.scale(u, dot(pole, u))))
    return R.add(R.add(hip, R.scale(u, x)), R.scale(p, r))


def bend(a, b):
    """The angle between two directions, degrees."""
    return math.degrees(math.acos(max(-1.0, min(1.0, dot(R.unit(a), R.unit(b))))))


def pitch(v):
    """How far `v` points forward of straight down, degrees (sagittal)."""
    return math.degrees(math.atan2(v[2], -v[1]))


class Runner:
    """The target's rest and the run posed on it."""

    def __init__(self, target, gait):
        self.target = target
        self.g = gait
        rot, pos = target.world()
        self.rest_rot = {n: rot[target.find(n)] for n in target.names}
        self.rest = {n: pos[target.find(n)] for n in target.names}
        p = self.rest
        self.thigh = R.length(R.sub(p["calf_l"], p["thigh_l"]))
        self.shin = R.length(R.sub(p["foot_l"], p["calf_l"]))
        self.hip_off = {s: R.sub(p[f"thigh_{s}"], p["pelvis"]) for s in "lr"}
        # The heel's bottom, under the ankle and 4 cm behind it.
        self.heel = {s: (p[f"foot_{s}"][0], 0.0, p[f"foot_{s}"][2] - 0.04) for s in "lr"}

    def ends(self):
        """Each chain's end site: the tip of the head and of the balls of
        the feet (along their bones' +Y, as the target aims them) and the
        hand's middle finger root."""
        out = {}
        for bone, length in (("head", 0.2), ("ball_l", 0.08), ("ball_r", 0.08)):
            out[bone] = R.add(self.rest[bone], R.qrot(self.rest_rot[bone], (0.0, length, 0.0)))
        for s in "lr":
            out[f"hand_{s}"] = self.rest[f"middle_01_{s}"]
        return out

    # --- the trunk ------------------------------------------------------

    def trunk(self, phase):
        """The pelvis's, spine's, neck's and head's world turns from rest
        at cycle `phase` (0 the right foot's contact)."""
        g = self.g
        c = math.cos(2.0 * math.pi * phase)
        yaw = g.PELVIS_TURN * c
        roll = -g.PELVIS_DROP * math.sin(2.0 * math.pi * (phase + 0.1))
        chest = -g.CHEST_TURN * c
        out = {"pelvis": R.qmul(turn(UP, yaw), R.qmul(turn((0, 0, 1), roll), turn(X, g.PELVIS_TILT)))}
        for k in (1, 2, 3):
            f = k / 3.0
            q = R.qmul(turn(UP, yaw + (chest - yaw) * f), R.qmul(turn((0, 0, 1), roll * (1 - f)), turn(X, g.LEAN)))
            out[f"spine_0{k}"] = q
        out["neck_01"] = R.qmul(turn(UP, chest * 0.5), turn(X, g.LEAN + g.NECK_LEAN))
        out["head"] = turn(X, g.GAZE)
        return out, chest

    def place(self, bone, parent, turns, at):
        """`bone`'s joint, carried by its parent's turn from rest."""
        offset = R.sub(self.rest[bone], self.rest[parent])
        return R.add(at[parent], R.qrot(turns[parent], offset))

    # --- the legs -------------------------------------------------------

    def foot_pose(self, s, ankle_flat, u):
        """The planted foot at stance time `u`: its world turn and ankle,
        rolling on the heel, flat, then on the ball (see ``JOG``)."""
        g = self.g
        rest_ankle = self.rest[f"foot_{s}"]
        shift = R.sub(ankle_flat, rest_ankle)
        if u < g.HEEL_DOWN:
            up = g.TOES_UP * (1.0 - smoothstep(0.0, g.HEEL_DOWN, u))
            pivot = self.heel[s]
        elif u < g.HEEL_RISE:
            up, pivot = 0.0, rest_ankle
        else:
            up = -g.PUSH_OFF * smoothstep(g.HEEL_RISE, 1.0, u) ** 1.5
            pivot = self.rest[f"ball_{s}"]
        q = turn(X, -up)
        pivot = R.add(pivot, shift)
        ankle = R.add(pivot, R.qrot(q, R.sub(R.add(rest_ankle, shift), pivot)))
        return q, ankle, up

    def stance_hip_height(self, hip_at_zero, ankle, knee):
        """The pelvis height that puts the hip joint at the length a knee
        bent `knee` degrees gives the leg over `ankle`."""
        a, b = self.thigh, self.shin
        reach = math.sqrt(a * a + b * b + 2.0 * a * b * math.cos(rad(knee)))
        dx, dz = hip_at_zero[0] - ankle[0], hip_at_zero[2] - ankle[2]
        flat = dx * dx + dz * dz
        if flat >= reach * reach:
            raise R.RetargetError("the stance foot is out of the leg's reach")
        return ankle[1] + math.sqrt(reach * reach - flat) - hip_at_zero[1]


def run(target, g=JOG):
    """The run's frames: per frame, each BVH joint's world turn from rest
    and the hips' place; plus a summary of what was measured."""
    runner = Runner(target, g)
    p = runner.rest
    dt = 1.0 / FPS
    count = int(round(CYCLES * g.CYCLE_TIME * FPS)) + 1
    stride = g.SPEED * g.CYCLE_TIME
    side_phase = {"r": 0.0, "l": 0.5}
    side_sign = {"l": 1.0, "r": -1.0}

    def body(t):
        phase = (t / g.CYCLE_TIME) % 1.0
        turns, chest = runner.trunk(phase)
        sway = -g.SWAY * side_sign["r"] * math.cos(2.0 * math.pi * (phase - 0.15))
        return phase, turns, chest, (sway, 0.0, g.SPEED * t)

    def hip(s, turns, pelvis):
        return R.add(pelvis, R.qrot(turns["pelvis"], runner.hip_off[s]))

    def contact(s, t):
        """The last contact time of foot `s` at or before `t`, and where its
        ankle lands flat."""
        start = side_phase[s] * g.CYCLE_TIME
        k = math.floor((t - start) / g.CYCLE_TIME + 1e-9)
        tc = start + k * g.CYCLE_TIME
        _, turns, _, pelvis = body(tc)
        h = hip(s, turns, pelvis)
        return tc, (side_sign[s] * g.FOOT_OUT, p[f"foot_{s}"][1], h[2] + g.FOOT_AHEAD)

    stance_time = g.STANCE * g.CYCLE_TIME

    def stance_height(s, t, knee):
        tc, flat = contact(s, t)
        u = (t - tc) / stance_time
        _, turns, _, pelvis = body(t)
        _, ankle, _ = runner.foot_pose(s, flat, u)
        return runner.stance_hip_height(hip(s, turns, pelvis), ankle, knee)

    flight = g.CYCLE_TIME / 2.0 - stance_time

    def height(t):
        for s in "rl":
            tc, _ = contact(s, t)
            if t - tc <= stance_time:
                return stance_height(s, t, pchip(g.STANCE_KNEE, (t - tc) / stance_time))
        # In the air: thrown from the last toe-off to the other foot's contact.
        s = min("rl", key=lambda s: t - contact(s, t)[0] - stance_time)
        t0 = contact(s, t)[0] + stance_time
        h0 = stance_height(s, t0, g.STANCE_KNEE[-1][1])
        h1 = stance_height("l" if s == "r" else "r", t0 + flight, g.STANCE_KNEE[0][1])
        since = t - t0
        rise = (h1 - h0) / flight + g.GRAVITY * flight / 2.0
        return h0 + rise * since - g.GRAVITY * since * since / 2.0

    def angles2d(hip_at, knee, ankle):
        thigh = pitch(R.sub(knee, hip_at))
        shin = pitch(R.sub(ankle, knee))
        return thigh, thigh - shin

    def stance_leg(s, t, turns, pelvis):
        tc, flat = contact(s, t)
        u = min(1.0, (t - tc) / stance_time)
        q, ankle, up = runner.foot_pose(s, flat, u)
        h = hip(s, turns, pelvis)
        knee = knee_place(h, ankle, runner.thigh, runner.shin, (0.0, 0.0, 1.0))
        return h, knee, ankle, q, up, u

    def swing_leg(s, t, turns, pelvis):
        """The swinging leg: its angles from the knots, joined to the
        stances at toe-off and at the next contact."""
        tc, _ = contact(s, t)
        t_off = tc + stance_time
        t_on = tc + g.CYCLE_TIME
        w = (t - t_off) / (t_on - t_off)
        ends = []
        for te in (t_off, t_on):
            _, et, _, ep = body(te)
            eh = height(te)
            ep = (ep[0], eh, ep[2])
            h, k, a, _, up, _ = stance_leg(s, te, et, ep)
            th, kn = angles2d(h, k, a)
            ends.append((h, a, th, kn, up - (th - kn)))
        (h0, a0, th0, kn0, an0), (h1, a1, th1, kn1, an1) = ends
        thigh = pchip(((0.0, th0),) + g.SWING["thigh"] + ((1.0, th1),), w)
        knee = pchip(((0.0, kn0),) + g.SWING["knee"] + ((1.0, kn1),), w)
        ankle_turn = pchip(((0.0, an0),) + g.SWING["ankle"] + ((1.0, an1),), w)
        h = hip(s, turns, pelvis)

        def reach(hp, th, kn):
            k = (0.0, -runner.thigh * math.cos(rad(th)), runner.thigh * math.sin(rad(th)))
            sh = th - kn
            return R.add(hp, R.add(k, (0.0, -runner.shin * math.cos(rad(sh)), runner.shin * math.sin(rad(sh)))))

        # The 2-D reach misses the stance's ankle by its sideways lean: the
        # miss at each end is spread across the swing.
        miss0 = R.sub(a0, reach(h0, th0, kn0))
        miss1 = R.sub(a1, reach(h1, th1, kn1))
        ankle = R.add(reach(h, thigh, knee), R.add(R.scale(miss0, 1.0 - w), R.scale(miss1, w)))
        knee_at = knee_place(h, ankle, runner.thigh, runner.shin, (0.0, 0.0, 1.0))
        up = ankle_turn + (thigh - knee)
        return h, knee_at, ankle, turn(X, -up), up, w

    rest_leg = {}
    for s in "lr":
        hd = R.sub(p[f"calf_{s}"], p[f"thigh_{s}"])
        sd = R.sub(p[f"foot_{s}"], p[f"calf_{s}"])
        rest_leg[s] = (hd, sd, R.cross(hd, sd))

    def arm(s, phase, turns, at, chest):
        """The upper arm's and forearm's world turns, swinging against the
        same side's leg."""
        own = (phase - side_phase[s] + g.ARM_LEAD) % 1.0
        c = -math.cos(2.0 * math.pi * own)  # -1 back ARM_LEAD before its foot lands
        swing = (g.ARM_FORWARD - g.ARM_BACK) / 2.0 + (g.ARM_FORWARD + g.ARM_BACK) / 2.0 * c
        elbow = g.ELBOW + g.ELBOW_SWAY * c
        frame_turn = R.qmul(turn(UP, chest), turn(X, g.LEAN))
        d0 = R.unit(R.sub(p[f"lowerarm_{s}"], p[f"upperarm_{s}"]))
        f0 = R.unit(R.sub(p[f"hand_{s}"], p[f"lowerarm_{s}"]))
        d = R.qrot(turn(X, -swing), d0)
        inward = (-side_sign[s] * math.sin(rad(g.CROSS)), 0.0, math.cos(rad(g.CROSS)))
        w = R.unit(R.sub(inward, R.scale(d, dot(inward, d))))
        f = R.add(R.scale(d, math.cos(rad(elbow))), R.scale(w, math.sin(rad(elbow))))
        d, f = R.qrot(frame_turn, d), R.qrot(frame_turn, f)
        upper = swing_to((d0, R.cross(d0, f0)), (d, R.cross(d, f)))
        fore = swing_to((f0, R.cross(d0, f0)), (f, R.cross(d, f)))
        return upper, fore

    frames = []
    stats = {"height": [], "knee": [], "thigh": [], "toe": []}
    for i in range(count):
        t = i * dt
        phase, turns, chest, pelvis = body(t)
        pelvis = (pelvis[0], height(t), pelvis[2])
        at = {"pelvis": pelvis}
        for bone, parent in (("spine_01", "pelvis"), ("spine_02", "spine_01"), ("spine_03", "spine_02"),
                             ("neck_01", "spine_03"), ("head", "neck_01")):
            at[bone] = runner.place(bone, parent, turns, at)
        for s in "lr":
            tc, _ = contact(s, t)
            if t - tc <= stance_time:
                h, k, a, q, up, u = stance_leg(s, t, turns, pelvis)
                toes = turn(X, up) if up < 0.0 else (0.0, 0.0, 0.0, 1.0)
            else:
                h, k, a, q, up, w = swing_leg(s, t, turns, pelvis)
                back = 1.0 - smoothstep(0.0, g.TOES_BACK, w)
                toes = turn(X, g.PUSH_OFF * back)
            hd, sd, n0 = rest_leg[s]
            n = R.cross(R.sub(k, h), R.sub(a, k))
            turns[f"thigh_{s}"] = swing_to((hd, n0), (R.sub(k, h), n))
            turns[f"calf_{s}"] = swing_to((sd, n0), (R.sub(a, k), n))
            turns[f"foot_{s}"] = q
            turns[f"ball_{s}"] = R.qnorm(R.qmul(q, toes))
            at[f"thigh_{s}"] = h
            stats["knee"].append(bend(R.sub(k, h), R.sub(a, k)))
            stats["thigh"].append(pitch(R.sub(k, h)))
            ball = R.add(a, R.qrot(q, R.sub(p[f"ball_{s}"], p[f"foot_{s}"])))
            stats["toe"].append(ball[1] - p[f"ball_{s}"][1])
            turns[f"clavicle_{s}"] = turns["spine_03"]
            turns[f"upperarm_{s}"], turns[f"lowerarm_{s}"] = arm(s, phase, turns, at, chest)
            turns[f"hand_{s}"] = turns[f"lowerarm_{s}"]
        stats["height"].append(pelvis[1])
        frames.append((turns, pelvis))
    summary = {
        "cycle_s": g.CYCLE_TIME,
        "stride_m": stride,
        "steps_per_min": 120.0 / g.CYCLE_TIME,
        "bob_cm": 100.0 * (max(stats["height"]) - min(stats["height"])),
        "knee_deg": (min(stats["knee"]), max(stats["knee"])),
        "thigh_deg": (min(stats["thigh"]), max(stats["thigh"])),
        "lowest_ball_cm": 100.0 * min(stats["toe"]),
    }
    return runner, frames, summary


def euler_zyx(q):
    """Angles (degrees) z, y, x with `q` = Rz(z) Ry(y) Rx(x)."""
    x, y, z, w = q
    m20 = 2.0 * (x * z - w * y)
    m21 = 2.0 * (y * z + w * x)
    m22 = 1.0 - 2.0 * (x * x + y * y)
    m10 = 2.0 * (x * y + w * z)
    m00 = 1.0 - 2.0 * (y * y + z * z)
    b = math.asin(max(-1.0, min(1.0, -m20)))
    return math.degrees(math.atan2(m10, m00)), math.degrees(b), math.degrees(math.atan2(m21, m22))


def bvh(runner, frames):
    """The run as BVH text (see ``BvhSource``)."""
    rest = runner.rest
    ends = runner.ends()
    unit = 1.0 / R.BVH_UNIT
    children = {}
    for name, bone, parent in JOINTS:
        children.setdefault(parent, []).append((name, bone))
    lines = ["HIERARCHY"]
    # The hips rest at their height over the lowest joint (the capture's
    # floor): written relative to the floor the run stands on.
    lowest = min(v[1] for v in list(rest.values()) + list(ends.values()))

    def fmt(v):
        return " ".join(f"{c * unit:.6f}" for c in v)

    def joint(name, bone, parent_bone, depth):
        pad = "\t" * depth
        offset = (0.0, 0.0, 0.0) if parent_bone is None else R.sub(rest[bone], rest[parent_bone])
        lines.append(f"{pad}{'ROOT' if parent_bone is None else 'JOINT'} {name}")
        lines.append(pad + "{")
        lines.append(f"{pad}\tOFFSET {fmt(offset)}")
        if parent_bone is None:
            lines.append(f"{pad}\tCHANNELS 6 Xposition Yposition Zposition Zrotation Yrotation Xrotation")
        else:
            lines.append(f"{pad}\tCHANNELS 3 Zrotation Yrotation Xrotation")
        kids = children.get(name, [])
        for child, child_bone in kids:
            joint(child, child_bone, bone, depth + 1)
        if not kids:
            lines.append(f"{pad}\tEnd Site")
            lines.append(pad + "\t{")
            lines.append(f"{pad}\t\tOFFSET {fmt(R.sub(ends[bone], rest[bone]))}")
            lines.append(pad + "\t}")
        lines.append(pad + "}")

    joint("Hips", "pelvis", None, 0)
    lines += ["MOTION", f"Frames: {len(frames)}", f"Frame Time: {1.0 / FPS:.8f}"]
    parent_of = {name: parent for name, _, parent in JOINTS}
    bone_of = {name: bone for name, bone, _ in JOINTS}
    order = []

    def walk(name):
        order.append(name)
        for child, _ in children.get(name, []):
            walk(child)

    walk("Hips")
    for turns, pelvis in frames:
        row = [(pelvis[0]) * unit, (pelvis[1] - lowest) * unit, pelvis[2] * unit]
        for name in order:
            q = turns[bone_of[name]]
            parent = parent_of[name]
            if parent is not None:
                q = R.qmul(R.qinv(turns[bone_of[parent]]), q)
            row += euler_zyx(R.qnorm(q))
        lines.append(" ".join(f"{v:.6f}" for v in row))
    return "\n".join(lines) + "\n"


def main(argv=None):
    ap = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    ap.add_argument("--target", required=True, help="the protagonist's glTF (its skeleton)")
    ap.add_argument("-o", "--output", required=True, help="the BVH to write")
    ap.add_argument("--gait", choices=sorted(GAITS), default="jog", help="the jog (5 m/s) or the sprint (7.5 m/s)")
    args = ap.parse_args(argv)
    with open(args.target, "rb") as f:
        target = R.read_target(f.read())
    runner, frames, summary = run(target, GAITS[args.gait])
    text = bvh(runner, frames)
    with open(args.output, "w", encoding="ascii") as f:
        f.write(text)
    for k, v in summary.items():
        print(f"{k}: {v}")


if __name__ == "__main__":
    main()
