"""Tests of tools/retarget.py: synthetic rigs for the maths, the shipped
animation GLB for its contract, and Quaternius' libraries when they are
on this machine (``~/Downloads/quaternius/ual``; skipped otherwise).

Standard library only: ``python3 -I -B -m unittest discover -s tools``.
"""

import io
import json
import math
import os
import struct
import unittest
from contextlib import redirect_stderr, redirect_stdout

import glbtrim
import retarget as R
import run_cycle as RC

REPO = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
PROTAGONIST = os.path.join(REPO, "assets/characters/protagonist/model.gltf")
SHIPPED = os.path.join(REPO, "assets/characters/animations/humanoid.glb")
UAL = os.path.expanduser("~/Downloads/quaternius/ual")
UAL1 = os.path.join(UAL, "ual1/Animation Library[Standard]/Godot/AnimationLibrary_Godot_Standard.glb")
UAL2 = os.path.join(UAL, "ual2/Universal Animation Library 2 [Standard]/Unreal-Godot/UAL2_Standard.glb")
HAVE_UAL = os.path.isfile(UAL1) and os.path.isfile(UAL2)
MODELS = [os.path.join(REPO, f"assets/characters/{name}/model.gltf") for name in ("bairoletto", "farach", "hidalgo", "protagonist")]
# The townsfolk: they talk with their hands, but never fold their arms.
TALKERS = [
    os.path.join(REPO, f"assets/characters/townsfolk_{name}/model.gltf")
    for name in ("bun", "headscarf", "paisano", "peon", "volga")
]

# The game's clips, in the order the game loads them by index.
GAME_CLIPS = [
    "idle",
    "idle_talk",
    "walk",
    "walk_formal",
    "jog",
    "sprint",
    "sit_idle",
    "drive",
    "interact",
    "idle_fold_arms",
    "yes",
    "no",
    "harvest",
    "plant",
    "water",
    "chop",
    "walk_carry",
    "sit_talk",
    "sit_drink",
    "sit_smoke",
]


# --- synthetic rigs ----------------------------------------------------------

# A small body, in the shared world frame (Y up, +Z front, left +X): each
# bone's game_engine name, parent, head place, where it points, and its
# roll about that direction (each rig rolls its bones its own way).
TORSO = [
    ("pelvis", None, (0, 1.0, 0), (0, 1, 0)),
    ("spine_01", "pelvis", (0, 1.1, 0), (0, 1, 0)),
    ("spine_02", "spine_01", (0, 1.2, 0), (0, 1, 0)),
    ("spine_03", "spine_02", (0, 1.3, 0), (0, 1, 0)),
    ("neck_01", "spine_03", (0, 1.5, 0), (0, 1, 0)),
    ("head", "neck_01", (0, 1.6, 0), (0, 1, 0)),
    ("clavicle_l", "spine_03", (0.02, 1.45, 0), (1, 0, 0)),
]


def arm(t_pose):
    """The left arm: out to the side in a T-pose, else hanging down."""
    if t_pose:
        return [
            ("upperarm_l", "clavicle_l", (0.18, 1.45, 0), (1, 0, 0)),
            ("lowerarm_l", "upperarm_l", (0.46, 1.45, 0), (1, 0, 0)),
            ("hand_l", "lowerarm_l", (0.72, 1.45, 0), (1, 0, 0)),
        ]
    return [
        ("upperarm_l", "clavicle_l", (0.18, 1.45, 0), (0, -1, 0)),
        ("lowerarm_l", "upperarm_l", (0.18, 1.17, 0), (0, -1, 0)),
        ("hand_l", "lowerarm_l", (0.18, 0.91, 0), (0, -1, 0)),
    ]


def legs(scale=1.0):
    """Both legs, their thigh and shin `scale` times as long."""
    out = []
    for side, x in (("l", 0.1), ("r", -0.1)):
        knee = 0.95 - 0.42 * scale
        ankle = knee - 0.43 * scale
        out += [
            (f"thigh_{side}", "pelvis", (x, 0.95, 0), (0, -1, 0)),
            (f"calf_{side}", f"thigh_{side}", (x, knee, 0), (0, -1, 0)),
            (f"foot_{side}", f"calf_{side}", (x, ankle, 0), (0, -0.5, 0.86)),
            (f"ball_{side}", f"foot_{side}", (x, ankle - 0.08, 0.13), (0, 0, 1)),
        ]
    return out


def world_rest(direction, roll):
    """A bone's world rotation: its +Y laid along `direction`, then rolled."""
    d = R.unit(direction)
    return R.qnorm(R.qmul(R.qmul(R.qaxis_angle(d, roll), R.qarc(R.Y, d)), (0.0, 0.0, 0.0, 1.0)))


def nodes_of(bones, names, top, roll, shift=(0.0, 0.0, 0.0)):
    """glTF nodes for `bones` (game names renamed by `names`) under a top
    node `top` = (name, rotation), every joint moved by `shift`; local
    transforms from the world ones."""
    world = {None: ((0.0, 0.0, 0.0), top[1])}
    nodes = [{"name": top[0], "rotation": list(top[1]), "children": []}]
    index = {None: 0}
    for k, (game, parent, at, direction) in enumerate(bones):
        rot = world_rest(direction, roll * (1 + k % 3))
        at = R.add(at, shift)
        p_at, p_rot = world[parent]
        local_t = R.qrot(R.qinv(p_rot), R.sub(at, p_at))
        local_r = R.qnorm(R.qmul(R.qinv(p_rot), rot))
        world[game] = (at, rot)
        index[game] = len(nodes)
        nodes.append({"name": names(game), "translation": list(local_t), "rotation": list(local_r)})
        nodes[index[parent]].setdefault("children", []).append(index[game])
    return nodes


def target(t_pose=False, leg_scale=1.0):
    """A synthetic game_engine target: ``Root`` turned like MakeHuman's,
    arms down, bones rolled their own way."""
    bones = TORSO + arm(t_pose) + legs(leg_scale)
    # A little taller and further forward than the source, as a model is.
    nodes = nodes_of(bones, lambda g: g, ("Root", (0.5, 0.5, 0.5, 0.5)), 1.1, (0.0, 0.03, 0.01))
    data = json.dumps({"asset": {"version": "2.0"}, "nodes": nodes, "scenes": [{"nodes": [0]}]})
    return R.read_target(data.encode())


def floats(values):
    return struct.pack(f"<{len(values)}f", *values)


def source_glb(clips):
    """A synthetic UAL1 source GLB in a T-pose with ``clips``: name -> (times,
    {game bone: [local rotation per key]}, {game bone: [local translation
    per key]}); unanimated bones keep their rest."""
    names = R.ual1_map()
    bones = TORSO + arm(True) + legs()
    nodes = nodes_of(bones, lambda g: names[g], ("root", (-math.sqrt(0.5), 0.0, 0.0, math.sqrt(0.5))), -0.4)
    by_name = {n["name"]: k for k, n in enumerate(nodes)}
    blob = bytearray()
    views, accessors, animations = [], [], []

    def add(values, kind):
        data = b"".join(floats(v) for v in values)
        views.append({"buffer": 0, "byteOffset": len(blob), "byteLength": len(data)})
        blob.extend(data)
        accessor = {"bufferView": len(views) - 1, "componentType": 5126, "count": len(values), "type": kind}
        if kind == "SCALAR":
            accessor["min"], accessor["max"] = [min(values)[0]], [max(values)[0]]
        accessors.append(accessor)
        return len(accessors) - 1

    for name, (times, rotations, translations) in clips.items():
        stamp = add([(t,) for t in times], "SCALAR")
        samplers, channels = [], []
        for path, table, kind in (("rotation", rotations, "VEC4"), ("translation", translations, "VEC3")):
            for game, values in table.items():
                if values is None:
                    node = nodes[by_name[names[game]]]
                    values = [node["translation"]] * len(times)
                samplers.append({"input": stamp, "output": add(values, kind)})
                channels.append({"sampler": len(samplers) - 1, "target": {"node": by_name[names[game]], "path": path}})
        animations.append({"name": name, "samplers": samplers, "channels": channels})
    doc = {
        "asset": {"version": "2.0"},
        "nodes": nodes,
        "scenes": [{"nodes": [0]}],
        "animations": animations,
        "buffers": [{"byteLength": len(blob)}],
        "bufferViews": views,
        "accessors": accessors,
    }
    return glbtrim.write_glb(doc, bytes(blob)), nodes


def source(clips=None):
    clips = clips or {"Still": ([0.0, 1.0], {}, {"pelvis": None})}
    data, _ = source_glb(clips)
    return R.Source(data, "ual1")


def posed(rt, turns=None):
    """A source pose: the reference, each bone in `turns` (game name ->
    world rotation) turned further in the world. World rotations and
    places by source node, as `Retargeter.pose` takes them."""
    sk = rt.source.skeleton
    rot = list(rt.s_rest_r)
    for k in rt.bones:
        rot[rt.src[k]] = rt.reference[k]
    for game, turn in (turns or {}).items():
        k = rt.target.find(game)
        rot[rt.src[k]] = R.qnorm(R.qmul(turn, rt.reference[k]))
    return rot, list(rt.s_rest_p)


def target_world(rt, local, place=None):
    """The target's world rotations and places for a retargeted pose."""
    tk = rt.target
    rots, trans = list(tk.rest_r), list(tk.rest_t)
    for k, q in local.items():
        rots[k] = q
    if place is not None:
        trans[tk.find("pelvis")] = rt.pelvis_local(place)
    return tk.world(rots, trans)


def angle(u, v):
    d = sum(a * b for a, b in zip(R.unit(u), R.unit(v)))
    return math.acos(max(-1.0, min(1.0, d)))


# --- tests --------------------------------------------------------------------


class NameMaps(unittest.TestCase):
    def test_every_game_engine_bone_has_a_source_bone_in_both_libraries(self):
        with open(PROTAGONIST, "rb") as f:
            tk = R.read_target(f.read())
        root = tk.find("Root")
        bones = [n for k, n in enumerate(tk.names) if k != root and tk.parent[k] is not None]
        self.assertEqual(len(bones), 52)
        for kind, make in R.MAPS.items():
            names = make()
            missing = [b for b in bones if b not in names]
            # The capture has no fingers: only they are left out.
            fingers = [b for b in bones if b.startswith(R.FINGERS)] if kind == "cmu" else []
            self.assertEqual(missing, fingers, kind)
            self.assertEqual(len(set(names.values())), len(names), f"{kind}: two bones share a source")
            for game, src in names.items():
                # Left stays left: game_engine's _l is Rigify's .L, CMU's Left.
                if game.endswith("_l"):
                    self.assertTrue(src.endswith((".L", "_l")) or src.startswith("Left"), f"{kind} {game} -> {src}")
                if game.endswith("_r"):
                    self.assertTrue(src.endswith((".R", "_r")) or src.startswith("Right"), f"{kind} {game} -> {src}")

    def test_the_maps_name_what_each_library_calls_its_bones(self):
        one, two = R.ual1_map(), R.ual2_map()
        self.assertEqual(one["pelvis"], "DEF-hips")
        self.assertEqual(one["calf_l"], "DEF-shin.L")
        self.assertEqual(one["ball_r"], "DEF-toe.R")
        self.assertEqual(one["middle_02_l"], "DEF-f_middle.02.L")
        self.assertEqual(one["thumb_03_r"], "DEF-thumb.03.R")
        self.assertEqual(two["head"], "Head")
        self.assertEqual(two["calf_l"], "calf_l")
        cmu = R.cmu_map()
        self.assertEqual(cmu["neck_01"], "Neck1")
        self.assertEqual(cmu["thigh_l"], "LeftUpLeg")
        self.assertEqual(cmu["ball_r"], "RightToeBase")


class RestAlignment(unittest.TestCase):
    def setUp(self):
        self.rt = R.Retargeter(source(), target())

    def test_a_source_posed_as_the_reference_gives_the_target_rest(self):
        rt = self.rt
        local, place = rt.pose(*posed(rt))
        for k in rt.bones:
            self.assertLess(R.qangle(local[k], rt.target.rest_r[k]), 1e-5, rt.target.names[k])
        self.assertLess(R.length(R.sub(place, rt.t_rest_p[rt.target.find("pelvis")])), 1e-9)

    def test_the_source_t_pose_gives_a_t_posed_target_not_its_rest(self):
        # The T-pose rest is a pose like any other: retargeted it holds the
        # arm out, so a clip's arms down come out down.
        rt = self.rt
        sk = rt.source.skeleton
        local, _ = rt.pose(*sk.world())
        _, pos = target_world(rt, local)
        tk = rt.target
        reach = R.sub(pos[tk.find("hand_l")], pos[tk.find("upperarm_l")])
        self.assertLess(angle(reach, (1, 0, 0)), 1e-4, reach)

    def test_every_bone_points_where_its_source_bone_points(self):
        # Any turns on any bones: the target's bones point at their next
        # joint the way the source's do, so no sign is flipped anywhere.
        rt = self.rt
        turns = {
            "thigh_l": R.qaxis_angle((1, 0, 0), -0.6),
            "calf_l": R.qaxis_angle((1, 0.2, 0), 0.9),
            "upperarm_l": R.qaxis_angle((0, 0.3, 1), 0.5),
            "lowerarm_l": R.qaxis_angle((1, 0, 0), -1.1),
            "spine_02": R.qaxis_angle((1, 0, 0), 0.2),
            "pelvis": R.qaxis_angle((0, 1, 0), 0.3),
        }
        s_rot, s_pos = posed(rt, turns)
        sk = rt.source.skeleton
        local_r = [None] * len(sk.names)
        for k, q in enumerate(s_rot):
            p = sk.parent[k]
            local_r[k] = q if p is None else R.qnorm(R.qmul(R.qinv(s_rot[p]), q))
        s_rot, s_pos = sk.world(local_r)
        local, place = rt.pose(s_rot, s_pos)
        t_rot, t_pos = target_world(rt, local, place)
        tk = rt.target
        for game, child in R.CHAIN.items():
            if game not in tk.index or child not in tk.index:
                continue
            k, c = tk.find(game), tk.find(child)
            want = R.sub(s_pos[rt.src[c]], s_pos[rt.src[k]])
            have = R.sub(t_pos[c], t_pos[k])
            self.assertLess(angle(have, want), 1e-4, game)
        # And a hip flexed forward swings the knee forward.
        self.assertGreater(t_pos[tk.find("calf_l")][2], 0.15)

    def test_a_known_turn_maps_to_the_same_turn_on_the_target_rest(self):
        rt = self.rt
        turn = R.qaxis_angle((0, 0, 1), 0.5)
        local, _ = rt.pose(*posed(rt, {"upperarm_l": turn}))
        t_rot, _ = target_world(rt, local)
        k = rt.target.find("upperarm_l")
        want = R.qmul(turn, rt.t_rest_r[k])
        self.assertLess(R.qangle(t_rot[k], want), 1e-5)

    def test_an_elbow_bend_bends_without_twisting_the_forearm(self):
        # The rigs roll their bones differently; a pure hinge on the source
        # forearm is a pure hinge on the target's, about its own axis.
        rt = self.rt
        tk = rt.target
        bend = R.qaxis_angle((1, 0, 0), 0.7)
        local, _ = rt.pose(*posed(rt, {"lowerarm_l": bend}))
        k = tk.find("lowerarm_l")
        delta = R.qmul(R.qinv(tk.rest_r[k]), local[k])
        _, want = rt.aim(k)
        axis = R.unit(want)
        twist = 2 * math.atan2(sum(a * b for a, b in zip(delta[:3], axis)), delta[3])
        self.assertLess(abs(twist), 1e-4)
        self.assertAlmostEqual(R.qangle(delta, (0, 0, 0, 1)), 0.7, places=4)


def bob(t):
    """A hip bob through a one-second loop, up from the rest, meters."""
    return 0.025 * (1.0 - math.cos(2 * math.pi * t))


class PelvisAndLoops(unittest.TestCase):
    def test_the_pelvis_moves_by_the_leg_length_ratio(self):
        # The source pelvis drops 10 cm and sways 2 cm; the target's legs
        # are half as long again, so its pelvis does both half as much again.
        rt = R.Retargeter(source(), target(leg_scale=1.5))
        self.assertAlmostEqual(rt.ratio, 1.5, places=6)
        s_rot, s_pos = posed(rt)
        pelvis = rt.src[rt.target.find("pelvis")]
        s_pos[pelvis] = R.add(s_pos[pelvis], (0.02, -0.1, 0.0))
        _, place = rt.pose(s_rot, s_pos)
        moved = R.sub(place, rt.t_rest_p[rt.target.find("pelvis")])
        for a, b in zip(moved, (0.03, -0.15, 0.0)):
            self.assertAlmostEqual(a, b, places=6)

    def loop(self, forward, slide):
        """A one-second loop: the hips go `forward` m/s and bob up (never
        down: the legs are straight); both feet slide back at `slide` m/s."""
        times = [k / 10 for k in range(11)]
        sk = source().skeleton
        rest = sk.rest_t[sk.find("DEF-hips")]
        to_root = R.qinv(sk.rest_r[sk.find("root")])
        hips = [
            R.add(rest, R.qrot(to_root, (0.0, bob(t), forward * t)))
            for t in times
        ]
        feet = {}
        for side in ("l", "r"):
            k = sk.find(f"DEF-foot.{side.upper()}")
            calf = sk.find(f"DEF-shin.{side.upper()}")
            world_calf = sk.world()[0][calf]
            feet[f"foot_{side}"] = [
                R.add(sk.rest_t[k], R.qrot(R.qinv(world_calf), (0.0, 0.0, -slide * t))) for t in times
            ]
        return {"Loop": (times, {}, {"pelvis": hips, **feet})}

    def run_loop(self, forward, slide):
        rt = R.Retargeter(source(self.loop(forward, slide)), target(leg_scale=1.25))
        keys, measures = rt.clip("Loop", True, True)
        r = rt.t_rest_r[rt.root]
        places = [R.add(R.qrot(r, p), rt.t_rest_p[rt.root]) for _, _, p in keys]
        return rt, places, measures

    def test_a_loop_stays_in_place_and_keeps_its_bob(self):
        rt, places, _ = self.run_loop(1.0, 0.0)
        rest = rt.t_rest_p[rt.target.find("pelvis")]
        for p in places:
            self.assertAlmostEqual(p[2], rest[2], places=5)
            self.assertAlmostEqual(p[0], rest[0], places=5)
        rises = [p[1] - rest[1] for p in places]
        for k, rise in enumerate(rises):
            self.assertAlmostEqual(rise, 1.25 * bob(k / 10), places=5)
        self.assertGreater(max(rises), 0.05)

    def test_the_ground_speed_is_the_drift_or_the_planted_foots_slide(self):
        # Root motion: the feet stand still on the ground as the body goes.
        _, _, measures = self.run_loop(1.0, 0.0)
        self.assertAlmostEqual(measures["ground_speed"], 1.25, places=3)
        self.assertAlmostEqual(measures["stride"], 1.25, places=3)
        # In place: the planted feet slide back under the body.
        _, _, measures = self.run_loop(0.0, 1.2)
        self.assertAlmostEqual(measures["ground_speed"], 1.5, places=3)

    def test_no_foot_goes_through_the_ground(self):
        # The source crouches 10 cm with its legs straight: its feet would go
        # through the ground, so the target's hips stay up instead.
        times = [0.0, 0.5, 1.0]
        sk = source().skeleton
        rest = sk.rest_t[sk.find("DEF-hips")]
        to_root = R.qinv(sk.rest_r[sk.find("root")])
        hips = [R.add(rest, R.qrot(to_root, (0.0, -0.1 * t, 0.0))) for t in times]
        rt = R.Retargeter(source({"Sink": (times, {}, {"pelvis": hips})}), target())
        keys, _ = rt.clip("Sink", False, False)
        r = rt.t_rest_r[rt.root]
        for _, _, p in keys:
            place = R.add(R.qrot(r, p), rt.t_rest_p[rt.root])
            self.assertAlmostEqual(place[1], rt.t_rest_p[rt.target.find("pelvis")][1], places=5)
        # A hip raised keeps its height: only sinking is undone.
        hips = [R.add(rest, R.qrot(to_root, (0.0, 0.05 * t, 0.0))) for t in times]
        rt = R.Retargeter(source({"Rise": (times, {}, {"pelvis": hips})}), target())
        keys, _ = rt.clip("Rise", False, False)
        place = R.add(R.qrot(r, keys[-1][2]), rt.t_rest_p[rt.root])
        self.assertAlmostEqual(place[1] - rt.t_rest_p[rt.target.find("pelvis")][1], 0.05, places=5)

    def test_a_one_shot_clip_keeps_its_travel(self):
        rt = R.Retargeter(source(self.loop(1.0, 0.0)), target())
        keys, measures = rt.clip("Loop", False, False)
        self.assertEqual(measures, {})
        r = rt.t_rest_r[rt.root]
        first, last = (R.qrot(r, keys[i][2]) for i in (0, -1))
        self.assertAlmostEqual(last[2] - first[2], 1.0, places=4)


def front(pos, at):
    """A standing pose seen from the front: the ankles' spacing over the
    hip joints', how far one ankle is ahead of the other (m), the larger
    knee bend and the upper arms' mean angle out from vertical (degrees),
    and the smaller knee bend.
    `at(name)` is a game_engine bone's index into `pos`."""
    x = lambda n: pos[at(n)][0]  # noqa: E731
    stance = abs(x("foot_l") - x("foot_r")) / abs(x("thigh_l") - x("thigh_r"))
    stagger = abs(pos[at("foot_l")][2] - pos[at("foot_r")][2])
    knees = []
    for s in "lr":
        hip, knee, ankle = (pos[at(f"{b}_{s}")] for b in ("thigh", "calf", "foot"))
        knees.append(math.degrees(angle(R.sub(knee, hip), R.sub(ankle, knee))))
    arms = []
    for s, side in (("l", 1), ("r", -1)):
        try:
            v = R.sub(pos[at(f"lowerarm_{s}")], pos[at(f"upperarm_{s}")])
        except R.RetargetError:
            continue  # the synthetic rigs have a left arm only
        arms.append(math.degrees(math.atan2(side * v[0], -v[1])))
    return stance, stagger, max(knees), sum(arms) / len(arms), min(knees)


class Calm(unittest.TestCase):
    def setUp(self):
        self.rt = R.Retargeter(source(), target())

    def wide(self):
        """A boxer's stance: legs out and one ahead, knees bent, arms out."""
        x, z = (1, 0, 0), (0, 0, 1)
        turns = {
            "thigh_l": R.qmul(R.qaxis_angle(x, -0.35), R.qaxis_angle(z, 0.25)),
            "thigh_r": R.qmul(R.qaxis_angle(x, 0.25), R.qaxis_angle(z, -0.25)),
            "calf_l": R.qmul(R.qaxis_angle(x, 0.7), R.qaxis_angle(z, 0.25)),
            "calf_r": R.qmul(R.qaxis_angle(x, 0.6), R.qaxis_angle(z, -0.25)),
            "upperarm_l": R.qaxis_angle(z, 0.4),
        }
        return self.rt.pose(*posed(self.rt, turns))

    def test_a_wide_stance_is_drawn_in_and_straightened_on_the_same_ground(self):
        rt, tk = self.rt, self.rt.target
        local, place = self.wide()
        rot, pos = rt.posed_world(local, place)
        stance, stagger, knee, arm, _ = front(pos, tk.find)
        self.assertGreater(stance, 2.0)
        self.assertGreater(stagger, 0.15)
        self.assertGreater(knee, 25.0)
        self.assertGreater(arm, 20.0)
        calm_local, calm_place = rt.calm(local, place, ("legs", "arms"))
        calm_rot, calm_pos = rt.posed_world(calm_local, calm_place)
        stance, stagger, calm_knee, arm, _ = front(calm_pos, tk.find)
        self.assertAlmostEqual(stance, R.STANCE, places=4)
        self.assertAlmostEqual(stagger, R.STAGGER, places=4)
        self.assertLess(calm_knee, knee - 5.0)
        for s in "lr":
            foot = tk.find(f"foot_{s}")
            # The feet keep their heights and their turns; the hips rose.
            self.assertAlmostEqual(calm_pos[foot][1], pos[foot][1], places=5)
            self.assertLess(R.qangle(calm_rot[foot], rot[foot]), 1e-5)
        self.assertNotAlmostEqual(calm_place[1], place[1], places=3)
        # The arm held out hangs ARM_OUT out.
        left = R.sub(calm_pos[tk.find("lowerarm_l")], calm_pos[tk.find("upperarm_l")])
        self.assertAlmostEqual(math.degrees(math.atan2(left[0], -left[1])), R.ARM_OUT, places=4)

    def test_a_calm_stance_is_left_as_it_is(self):
        rt = self.rt
        local, place = rt.pose(*posed(rt))
        calm_local, calm_place = rt.calm(local, place, ("legs", "arms"))
        self.assertLess(R.length(R.sub(calm_place, place)), 1e-6)
        # Within a hundredth of a degree: the straight legs' solve stops a
        # hair short of full stretch.
        for k, q in local.items():
            self.assertLess(R.qangle(calm_local[k], q), 2e-4, rt.target.names[k])

    def test_reaching_out_brings_the_arms_forward_as_they_are(self):
        rt, tk = self.rt, self.rt.target
        turns = {"lowerarm_l": R.qaxis_angle((0, 1, 0), 1.2), "upperarm_l": R.qaxis_angle((1, 0, 0), -0.4)}
        local, place = rt.pose(*posed(rt, turns))
        rot, pos = rt.posed_world(local, place)
        moved_local = rt.reach_out(local, place, 0.05)
        moved_rot, moved_pos = rt.posed_world(moved_local, place)
        arm = [tk.find(n) for n in ("upperarm_l", "lowerarm_l", "hand_l")]
        shift = R.sub(moved_pos[arm[0]], pos[arm[0]])
        # The shoulder comes forward, by about that much (the clavicle
        # swings on its length, so it also comes in a little).
        self.assertGreater(shift[2], 0.04)
        self.assertLess(abs(shift[1]), 0.02)
        for k in arm:
            # The arm moves as one, its turn in the world kept.
            self.assertLess(R.length(R.sub(R.sub(moved_pos[k], pos[k]), shift)), 1e-6, tk.names[k])
            self.assertLess(R.qangle(moved_rot[k], rot[k]), 1e-6, tk.names[k])
        self.assertIs(rt.reach_out(local, place, 0.0), local)

    def test_only_the_standing_clips_are_calmed(self):
        self.assertEqual(set(R.CALM), {"idle", "idle_talk", "idle_fold_arms", "yes", "no"})
        self.assertNotIn("arms", R.CALM["idle_fold_arms"])
        self.assertNotIn("arms", R.CALM["idle_talk"])


class Output(unittest.TestCase):
    CLIPS = [("still", "ual1", "Still", True, False), ("again", "ual1", "Still", False, False)]

    def test_the_output_is_deterministic(self):
        builds = []
        for _ in range(2):
            doc, blob, _ = R.build({"ual1": source()}, target(), self.CLIPS)
            builds.append(glbtrim.write_glb(doc, blob))
        self.assertEqual(builds[0], builds[1])

    def test_the_output_holds_the_targets_bones_and_the_clips_in_order(self):
        doc, blob, _ = R.build({"ual1": source()}, target(), self.CLIPS)
        self.assertEqual([a["name"] for a in doc["animations"]], ["still", "again"])
        self.assertEqual(doc["nodes"][0]["name"], "Root")
        self.assertEqual(doc["scenes"], [{"name": "humanoid", "nodes": [0]}])
        self.assertNotIn("meshes", doc)
        bones = len(doc["nodes"]) - 1
        for a in doc["animations"]:
            paths = [c["target"]["path"] for c in a["channels"]]
            self.assertEqual(paths.count("rotation"), bones)
            self.assertEqual(paths.count("translation"), 1)
            moved = [c["target"]["node"] for c in a["channels"] if c["target"]["path"] == "translation"]
            self.assertEqual(doc["nodes"][moved[0]]["name"], "pelvis")
        # It reads back as a valid GLB with its key times bounded.
        back, data = glbtrim.read_glb(glbtrim.write_glb(doc, blob))
        glbtrim.check_buffers(back, data)
        times = back["accessors"][back["animations"][0]["samplers"][0]["input"]]
        self.assertEqual((times["min"], times["max"]), ([0.0], [1.0]))

    def test_appending_keeps_the_earlier_file_and_gives_the_whole_build(self):
        whole = R.build({"ual1": source()}, target(), self.CLIPS)
        first = R.build({"ual1": source()}, target(), self.CLIPS[:1])
        base = first[:2]
        doc, blob, summary = R.build({"ual1": source()}, R.Skeleton(base[0]), self.CLIPS, base=base)
        self.assertEqual(glbtrim.write_glb(doc, blob), glbtrim.write_glb(*whole[:2]))
        self.assertEqual([s[0] for s in summary], ["again"])
        with self.assertRaisesRegex(R.RetargetError, "holds"):
            R.build({"ual1": source()}, R.Skeleton(base[0]), self.CLIPS[1:], base=base)

    def test_bad_inputs_are_refused(self):
        rt = R.Retargeter(source(), target())
        with self.assertRaisesRegex(R.RetargetError, "no clip"):
            rt.clip("Missing", True, False)
        tk = target()
        tk.names.append("tail")
        tk.parent.append(tk.find("pelvis"))
        tk.children.append([])
        tk.children[tk.find("pelvis")].append(len(tk.names) - 1)
        tk.rest_t.append((0.0, 0.0, 0.0))
        tk.rest_r.append((0.0, 0.0, 0.0, 1.0))
        tk.order.append(len(tk.names) - 1)
        tk.index["tail"] = len(tk.names) - 1
        with self.assertRaisesRegex(R.RetargetError, "no ual1 name"):
            R.Retargeter(source(), tk)
        cubic, _ = source_glb({"Still": ([0.0, 1.0], {}, {"pelvis": None})})
        doc, blob = glbtrim.read_glb(cubic)
        doc["animations"] = [{"name": "Still", "samplers": [{"input": 0, "output": 0, "interpolation": "STEP"}],
                              "channels": []}]
        doc["accessors"] = [{"bufferView": 0, "componentType": 5126, "count": 2, "type": "SCALAR"}]
        with self.assertRaisesRegex(R.RetargetError, "LINEAR"):
            R.Source(glbtrim.write_glb(doc, blob), "ual1").frames("Still")
        with self.assertRaisesRegex(R.RetargetError, "not glTF JSON"):
            R.read_target(b"{not json")


def walk_posture(tk, keys):
    """The walk's posture through its keys (local rotations by node name,
    pelvis local place), on the target `tk`: each key's knee bends (degrees)
    and ankle heights, torso lean, head pitch from rest, hands ahead of
    the hip line (meters), elbow bends and the shoulders ahead of the
    chest's base."""
    f = tk.find
    rest_rot, rest_pos = tk.world()

    def pitch(q):
        up = R.qrot(q, (0.0, 1.0, 0.0))
        return math.degrees(math.atan2(up[2], up[1]))

    rows = []
    for local, pelvis in keys:
        rots, trans = list(tk.rest_r), list(tk.rest_t)
        for name, q in local.items():
            rots[f(name)] = q
        trans[f("pelvis")] = pelvis
        rot, pos = tk.world(rots, trans)

        def bend(a, b, c):
            return math.degrees(R.angle_between(R.sub(pos[f(b)], pos[f(a)]), R.sub(pos[f(c)], pos[f(b)])))

        lean = R.sub(pos[f("neck_01")], pos[f("pelvis")])
        hips = (pos[f("thigh_l")][2] + pos[f("thigh_r")][2]) / 2
        rows.append({
            "knee": [bend(f"thigh_{s}", f"calf_{s}", f"foot_{s}") for s in "lr"],
            "ankle": [pos[f(f"foot_{s}")][1] for s in "lr"],
            "lean": math.degrees(math.atan2(lean[2], lean[1])),
            "head": pitch(rot[f("head")]) - pitch(rest_rot[f("head")]),
            "hand": [pos[f(f"hand_{s}")][2] - hips for s in "lr"],
            "elbow": [bend(f"upperarm_{s}", f"lowerarm_{s}", f"hand_{s}") for s in "lr"],
            "shoulder": [pos[f(f"upperarm_{s}")][2] - pos[f("spine_03")][2] for s in "lr"],
        })
    rest_shoulder = rest_pos[f("upperarm_l")][2] - rest_pos[f("spine_03")][2]
    return rows, rest_shoulder


def stance_knees(rows):
    """The knee bends of the legs whose ankle is on the ground (within 2 cm
    of its lowest)."""
    low = [min(r["ankle"][k] for r in rows) for k in range(2)]
    return [r["knee"][k] for r in rows for k in range(2) if r["ankle"][k] < low[k] + 0.02]


def knee_track(tk, keys):
    """Each leg's knee bend (degrees, signed: positive bends the shin
    back; negative would be a knee bent backwards) per key, how far the
    shin leaves the plane of the thigh and the body's forward axis
    (degrees), and the pelvis's height (meters)."""
    f = tk.find
    bends, planes, heights = [[], []], [[], []], []
    for local, pelvis in keys:
        rots, trans = list(tk.rest_r), list(tk.rest_t)
        for name, q in local.items():
            rots[f(name)] = q
        trans[f("pelvis")] = pelvis
        _, pos = tk.world(rots, trans)
        heights.append(pos[f("pelvis")][1])
        for k, s in enumerate("lr"):
            thigh = R.unit(R.sub(pos[f(f"calf_{s}")], pos[f(f"thigh_{s}")]))
            shin = R.unit(R.sub(pos[f(f"foot_{s}")], pos[f(f"calf_{s}")]))
            angle = math.degrees(R.angle_between(thigh, shin))
            # Bent back: the shin turns from the thigh toward -z, about +x.
            side = R.cross(thigh, shin)[0]
            bends[k].append(angle if side >= 0 else -angle)
            normal = R.unit(R.cross(thigh, (0.0, 0.0, 1.0)))
            planes[k].append(math.degrees(math.asin(max(-1.0, min(1.0, sum(a * b for a, b in zip(shin, normal)))))))
    return bends, planes, heights


class WalkPosture:
    """Checks of an easy, upright 1919 walk (see ``Retargeter.upright``),
    on keys from the shipped file or straight from the tool."""

    def check_walk(self, tk, keys):
        rows, rest_shoulder = walk_posture(tk, keys)
        # Nearly straight on the ground; only the swinging leg folds.
        stance = stance_knees(rows)
        self.assertLess(max(stance), 20.0, stance)
        self.assertGreater(max(max(r["knee"]) for r in rows), 60.0)
        # The torso upright, the head level.
        self.assertLess(max(r["lean"] for r in rows), 5.0)
        self.assertLess(max(abs(r["head"]) for r in rows), 3.0)
        # The shoulders down and easy, not pulled back (the two together:
        # the trunk's turn through the stride takes one back, one forward).
        self.assertGreater(min(sum(r["shoulder"]) / 2 for r in rows), rest_shoulder - 0.01)
        for k in range(2):
            hands = [r["hand"][k] for r in rows]
            # Swinging forward in front of the hip line, never mostly
            # behind it, further forward than back.
            self.assertGreater(max(hands), 0.12)
            self.assertLess(sum(h < 0 for h in hands), len(hands) / 4)
            self.assertGreater(sum(hands) / len(hands), 0.03)
            elbows = [r["elbow"][k] for r in rows]
            self.assertGreater(min(elbows), R.ELBOW[0] - 2.0)
            self.assertLess(max(elbows), R.ELBOW[1] + 2.0)
        self.check_knees(tk, keys)

    def check_knees(self, tk, keys):
        """No knee locks, bends backwards, snaps or twists, and the hips
        never step: through the loop and round its wrap (the last key is
        the first again). The pack's own walk changes a knee by at most 13
        degrees a key (keys at 30 a second) and its rate of change by 9; at
        the game's fastest walk (1.3 times) a key is about two frames at 60
        a second; ours by at most 17 and 14 (the swing's fold is quicker:
        the stance is straighter), under 11 degrees a frame at 60 frames a
        second."""
        bends, planes, heights = knee_track(tk, keys)
        for k in range(2):
            loop = bends[k][:-1]
            self.assertAlmostEqual(bends[k][0], bends[k][-1], delta=0.5)
            self.assertGreater(min(loop), R.KNEE_MIN - 0.5, loop)
            steps = [loop[(i + 1) % len(loop)] - loop[i] for i in range(len(loop))]
            self.assertLess(max(abs(d) for d in steps), 17.0, steps)
            turns = [steps[(i + 1) % len(steps)] - steps[i] for i in range(len(steps))]
            self.assertLess(max(abs(d) for d in turns), 14.0, turns)
            self.assertLess(max(abs(p) for p in planes[k]), 3.0, planes[k])
        loop = heights[:-1]
        self.assertLess(max(abs(loop[(i + 1) % len(loop)] - loop[i]) for i in range(len(loop))), 0.02)


class Shipped(unittest.TestCase, WalkPosture):
    """The animation GLB the game loads."""

    def keys(self, clip):
        """Each key of the shipped `clip`: local rotations by bone name and
        the pelvis's place."""
        names = [n["name"] for n in self.doc["nodes"]]
        a = next(a for a in self.doc["animations"] if a["name"] == clip)
        rotations, pelvis = {}, None
        for c in a["channels"]:
            values = glbtrim.read_accessor(self.doc, self.blob, a["samplers"][c["sampler"]]["output"])
            if c["target"]["path"] == "rotation":
                rotations[names[c["target"]["node"]]] = values
            else:
                pelvis = values
        return [({n: v[k] for n, v in rotations.items()}, pelvis[k]) for k in range(len(pelvis))]

    def test_the_drink_and_the_smoke_bring_the_right_hand_to_the_mouth_once_a_loop(self):
        # At the mouth the wrist is where SIP puts it and the fingers point
        # at its aim: up past the lips with a glass, across the mouth with a
        # cigarette. Everything else is the seated rest, untouched.
        tk = R.Skeleton(self.doc)
        idle = self.keys("sit_idle")
        n = len(idle) - 1

        def world(rotations, pelvis):
            rots, trans = list(tk.rest_r), list(tk.rest_t)
            for name, q in rotations.items():
                rots[tk.find(name)] = q
            trans[tk.find("pelvis")] = pelvis
            return tk.world(rots, trans)[1]

        def from_head(head, at):
            # Right hand: out to its side is -X.
            return R.add(head, (-at[0], at[1], at[2]))

        for game, (side, wrist_at, aim, palm_to) in R.SIP.items():
            self.assertEqual(side, "r")
            keys = self.keys(game)
            self.assertEqual(len(keys), n * R.SIP_CYCLES + 1, game)
            a = next(a for a in self.doc["animations"] if a["name"] == game)
            self.assertEqual(a["extras"]["sip"], "r")
            times = glbtrim.read_accessor(self.doc, self.blob, a["samplers"][0]["input"])
            up = 0
            for k, ((rotations, pelvis), (t,)) in enumerate(zip(keys, times)):
                base_rotations, _ = idle[k % n]
                share = R.sip_share(t)
                moved = {"upperarm_r", "lowerarm_r", "hand_r"} if share > 0.0 else set()
                for name, q in rotations.items():
                    if name not in moved:
                        self.assertLess(R.qangle(q, base_rotations[name]), 2e-3, (game, t, name))
                p = world(rotations, pelvis)
                head, wrist, knuckle = (p[tk.find(b)] for b in ("head", "hand_r", "middle_01_r"))
                if share == 1.0:
                    up += 1
                    self.assertLess(R.length(R.sub(wrist, from_head(head, wrist_at))), 0.02, (game, t))
                    fingers = R.unit(R.sub(knuckle, wrist))
                    toward = R.unit(R.sub(from_head(head, aim), wrist))
                    self.assertGreater(R.dot(fingers, toward), 0.97, (game, t))
                    # The smoker's palm faces the face; the drinker's the
                    # middle of the body (right hand: +X), the glass in view.
                    palm = R.palm_of(p, tk, "r")
                    to = R.sub(head, wrist) if palm_to is None else (-palm_to[0], palm_to[1], palm_to[2])
                    self.assertGreater(R.dot(palm, R.unit(R.off_axis(to, fingers))), 0.9, (game, t))
                elif share == 0.0:
                    self.assertLess(wrist[1], head[1] - 0.4, (game, t))
            self.assertGreater(up, 30, game)
        # The smoker's fingers cross the mouth to the other side; the
        # drinker's point up.
        self.assertLess(R.SIP["sit_smoke"][2][0], 0.0)
        self.assertGreater(R.SIP["sit_drink"][2][1], R.SIP["sit_smoke"][2][1])

    def test_folded_arms_and_talking_hands_stay_out_of_every_chest(self):
        # The forearms and hands (not the pinched inside of the elbow) of
        # each of the four men, as the game poses them, never come nearer
        # the front of his chest than CLEARANCE; nor, talking, the
        # townsfolk's (the women's busts and shawls stand furthest out).
        with open(PROTAGONIST, "rb") as f:
            tk = R.read_target(f.read())
        k = tk.find("pelvis")
        reference = (tk.rest_t[k], R.length(tk.rest_t[tk.find("calf_l")]) + R.length(tk.rest_t[tk.find("foot_l")]))
        clearance = R.Clearance(MODELS, TALKERS)
        self.assertEqual(len(clearance.models), 9)
        for clip in R.CLEAR + R.SPREAD:
            for local, pelvis in self.keys(clip)[::5]:
                for model in clearance.for_clip(clip).models:
                    depth = clearance.depth(model, reference, local, pelvis)
                    self.assertLessEqual(depth, 0.0, f"{clip} {model['name']}: {depth:.3f} m in")
        extras = {a["name"]: a["extras"] for a in self.doc["animations"]}
        self.assertGreater(extras["idle_fold_arms"]["reach"], 0.05)
        self.assertGreaterEqual(extras["jog"]["spread"], 0.0)

    @classmethod
    def setUpClass(cls):
        with open(SHIPPED, "rb") as f:
            cls.doc, cls.blob = glbtrim.read_glb(f.read())

    def test_the_clips_are_the_games_in_its_order(self):
        self.assertEqual([c[0] for c in R.CLIPS], GAME_CLIPS)
        self.assertEqual([a["name"] for a in self.doc["animations"]], GAME_CLIPS)
        for name, (game, _, _, loop, locomotion) in zip(GAME_CLIPS, R.CLIPS):
            a = self.doc["animations"][GAME_CLIPS.index(name)]
            self.assertEqual(a["extras"]["loop"], loop, name)
            self.assertEqual(a["extras"].get("calmed"), list(R.CALM[name]) if name in R.CALM else None, name)
            if locomotion:
                self.assertGreater(a["extras"]["ground_speed"], 0.3, name)
                self.assertGreater(a["extras"]["stride"], 0.5, name)
            else:
                self.assertNotIn("ground_speed", a["extras"], name)

    def test_the_walk_is_an_easy_upright_street_walk(self):
        with open(PROTAGONIST, "rb") as f:
            tk = R.read_target(f.read())
        self.check_walk(tk, self.keys("walk"))
        walk = next(a for a in self.doc["animations"] if a["name"] == "walk")
        self.assertEqual(walk["extras"]["upright"], True)

    def test_its_bones_have_the_protagonists_names_and_paths(self):
        # Bevy binds a clip to a bone by its name path from the scene root.
        with open(PROTAGONIST, "rb") as f:
            tk = R.read_target(f.read())

        def paths(names, children, top):
            out = []

            def walk(k, path):
                path = path + (names[k],)
                out.append(path)
                for c in children[k]:
                    walk(c, path)

            walk(top, ())
            return sorted(out)

        nodes = self.doc["nodes"]
        mine = paths([n["name"] for n in nodes], [n.get("children", []) for n in nodes], 0)
        theirs = paths(tk.names, tk.children, tk.find("Root"))
        self.assertEqual(mine, theirs)
        self.assertEqual(len(mine), 53)


@unittest.skipUnless(HAVE_UAL, "Quaternius' Universal Animation Libraries are not in ~/Downloads")
class RealLibraries(unittest.TestCase, WalkPosture):
    @classmethod
    def setUpClass(cls):
        with open(UAL1, "rb") as f:
            one = R.Source(f.read(), "ual1")
        with open(UAL2, "rb") as f:
            two = R.Source(f.read(), "ual2")
        with open(PROTAGONIST, "rb") as f:
            cls.target = R.read_target(f.read())
        runner, frames, _ = RC.run(cls.target)
        cmu = R.BvhSource(RC.bvh(runner, frames).encode(), "run_cycle")
        runner, frames, _ = RC.run(cls.target, RC.SPRINT)
        sprint = R.BvhSource(RC.bvh(runner, frames).encode(), "sprint_cycle")
        cls.sources = {"ual1": one, "ual2": two, "cmu": cmu, "sprint": sprint}

    def test_rebuilding_gives_the_shipped_file_byte_for_byte(self):
        doc, blob, _ = R.build(self.sources, self.target, clearance=R.Clearance(MODELS, TALKERS))
        with open(SHIPPED, "rb") as f:
            self.assertEqual(glbtrim.write_glb(doc, blob), f.read())

    def test_the_packs_crouched_walk_is_stood_up(self):
        # Measured on the pack's walk as retargeted, then as stood up: its
        # stance knees bend 25 degrees on average (36 at most), its torso
        # leans 7 to 9 degrees and its shoulders are held 6 cm back.
        rt = R.Retargeter(self.sources["ual1"], self.target)
        tk = self.target
        names = lambda keys: [({tk.names[k]: q for k, q in local.items()}, p) for _, local, p in keys]  # noqa: E731
        raw, raw_measures = rt.clip("Walk_Loop", True, True)
        stood, measures = rt.clip("Walk_Loop", True, True, upright=True)
        before, rest_shoulder = walk_posture(tk, names(raw))
        self.assertGreater(max(stance_knees(before)), 30.0)
        self.assertGreater(min(r["lean"] for r in before), 6.0)
        self.assertLess(max(sum(r["shoulder"]) / 2 for r in before), rest_shoulder - 0.04)
        self.check_walk(tk, names(stood))
        # A longer stride: more ground covered in the same cycle.
        self.assertAlmostEqual(measures["ground_speed"] / raw_measures["ground_speed"], 1.15, delta=0.05)

    def test_the_torso_and_head_lean_as_the_sources_do(self):
        # Pitch from vertical of the torso (pelvis to neck) and of the neck
        # (neck to head), source against target, through each clip. Each
        # bone points as its source bone does (see RestAlignment), but the
        # two spines split their length differently (game_engine's spine_03
        # is 35 cm, the source's 17) and their pelvises are shaped
        # differently (at rest the source's torso leans 4 degrees, the
        # target's 1), so the torso line may differ by a few degrees; a bone
        # aimed along its axis instead of at its next joint leaned the
        # target's 7 degrees further forward than the source's.
        tk = self.target
        pelvis, neck, head = (tk.find(n) for n in ("pelvis", "neck_01", "head"))

        def pitch(a, b):
            v = R.sub(b, a)
            return math.degrees(math.atan2(v[2], v[1]))

        for kind, clip in (("ual1", "Walk_Loop"), ("ual1", "Idle_Loop"), ("ual2", "Idle_FoldArms_Loop")):
            rt = R.Retargeter(self.sources[kind], tk)
            times, rots, trans = rt.source.frames(clip)
            for k in range(0, len(times), 5):
                s_rot, s_pos = rt.source.skeleton.world(rots[k], trans[k])
                local, place = rt.pose(s_rot, s_pos)
                _, t_pos = target_world(rt, local, place)
                for a, b, limit in ((neck, head, 1.0), (pelvis, neck, 3.5)):
                    s = pitch(s_pos[rt.src[a]], s_pos[rt.src[b]])
                    t = pitch(t_pos[a], t_pos[b])
                    where = f"{clip} key {k} {tk.names[a]}-{tk.names[b]}: source {s:.1f}, target {t:.1f}"
                    self.assertLess(abs(t - s), limit, where)

    def poses(self, kind, clip, game=None):
        """Each fifth key of `clip`, as (source front, target front): the
        target as retargeted, then calmed as the game's `game` clip is."""
        tk = self.target
        rt = R.Retargeter(self.sources[kind], tk)
        times, rots, trans = rt.source.frames(clip)
        keys, _ = rt.clip(clip, True, False, R.CALM.get(game, ()))
        out = []
        for k in range(0, len(times), 5):
            _, s_pos = rt.source.skeleton.world(rots[k], trans[k])
            _, local, pelvis = keys[k]
            rots_t, trans_t = list(tk.rest_r), list(tk.rest_t)
            for b, q in local.items():
                rots_t[b] = q
            trans_t[tk.find("pelvis")] = pelvis
            _, t_pos = tk.world(rots_t, trans_t)
            out.append((front(s_pos, lambda n: rt.src[tk.find(n)]), front(t_pos, tk.find)))
        return out

    def test_seen_from_the_front_the_legs_and_arms_spread_as_the_sources_do(self):
        # Uncalmed, the target stands, walks and runs as wide as the
        # source: the stance (over each body's own hip spacing; the target's
        # hips are wider), the knees and the arms.
        for kind, clip in (("ual1", "Idle_Loop"), ("ual1", "Walk_Loop"), ("ual1", "Jog_Fwd_Loop"),
                           ("ual2", "Idle_FoldArms_Loop")):
            for s, t in self.poses(kind, clip):
                where = f"{clip}: source {s}, target {t}"
                self.assertLess(abs(t[0] - s[0]), 0.2, where)
                self.assertLess(abs(t[2] - s[2]), 1.0, where)
                self.assertLess(abs(t[3] - s[3]), 1.0, where)

    def test_the_standing_clips_stand_at_ease(self):
        # Quaternius' idles stand like a boxer (feet twice the hips apart,
        # one 42 cm ahead, knees bent up to 26 degrees, arms 17 out); calmed,
        # a 1919 man stands easy.
        # The knee limits: one leg bears the weight nearly straight; the
        # other may rest a little bent (the hips go only as high as the
        # straighter leg lets them), the idle's least.
        for kind, clip, game, knees in (
            ("ual1", "Idle_Loop", "idle", 10.0),
            ("ual1", "Idle_Talking_Loop", "idle_talk", 14.0),
            ("ual2", "Idle_FoldArms_Loop", "idle_fold_arms", 14.0),
            ("ual2", "Yes", "yes", 20.0),
            ("ual2", "Idle_No_Loop", "no", 20.0),
        ):
            for s, t in self.poses(kind, clip, game):
                where = f"{clip}: source {s}, target {t}"
                self.assertGreater(s[0], 1.8, where)
                self.assertLess(t[0], R.STANCE + 1e-3, where)
                self.assertLess(t[1], R.STAGGER + 1e-3, where)
                self.assertLess(t[2], knees, where)
                self.assertLess(t[4], 9.0, where)  # the MakeHuman rest is bent 6
                if "arms" in R.CALM[game]:
                    self.assertLess(t[3], R.ARM_OUT + 1e-3, where)

    def test_the_run_is_a_real_runners(self):
        # The keyed run (tools/run_cycle.py) on the target: its planted feet
        # slide back at the game's run speed, one stride cycle of 19 keys.
        # The same for the sprint.
        for kind, name, g in (("cmu", "run_cycle", RC.JOG), ("sprint", "sprint_cycle", RC.SPRINT)):
            rt = R.Retargeter(self.sources[kind], self.target)
            keys, measures = rt.clip(name, True, True)
            self.assertAlmostEqual(keys[-1][0], g.CYCLE_TIME, places=4, msg=name)
            self.assertAlmostEqual(measures["ground_speed"], g.SPEED, delta=0.02, msg=name)
            self.assertAlmostEqual(measures["stride"], g.SPEED * g.CYCLE_TIME, delta=0.02, msg=name)

    def test_the_walks_and_runs_have_plausible_speeds(self):
        doc, _, summary = R.build(self.sources, self.target)
        speeds = {game: m.get("ground_speed") for game, _, _, _, _, m in summary}
        self.assertTrue(0.9 < speeds["walk"] < 1.4, speeds)
        self.assertLess(speeds["walk"], speeds["jog"])
        self.assertLess(speeds["jog"], speeds["sprint"])


# A CMU-like BVH body (inches/0.45; Y up, facing +Z, left at +X): joint,
# parent, offset. Each chain ends in an End Site.
BVH_BODY = [
    ("Hips", None, (0, 0, 0)),
    ("LowerBack", "Hips", (0, 0, 0)),
    ("Spine", "LowerBack", (0, 2.1, 0)),
    ("Spine1", "Spine", (0, 2.1, 0)),
    ("Neck", "Spine1", (0, 0, 0)),
    ("Neck1", "Neck", (0, 1.6, 0.2)),
    ("Head", "Neck1", (0, 1.6, -0.1)),
] + [
    joint
    for side, x in (("Left", 1), ("Right", -1))
    for joint in (
        (f"{side}Shoulder", "Spine1", (0, 0, 0)),
        (f"{side}Arm", f"{side}Shoulder", (3.2 * x, 0.8, 0)),
        (f"{side}ForeArm", f"{side}Arm", (5.2 * x, 0, 0)),
        (f"{side}Hand", f"{side}ForeArm", (3.8 * x, 0, 0)),
        (f"{side[0]}HipJoint", "Hips", (0, 0, 0)),
        (f"{side}UpLeg", f"{side[0]}HipJoint", (1.5 * x, -1.8, 0.6)),
        (f"{side}Leg", f"{side}UpLeg", (0, -7.0, 0)),
        (f"{side}Foot", f"{side}Leg", (0, -7.5, 0)),
        (f"{side}ToeBase", f"{side}Foot", (0, -0.6, 2.1)),
    )
]


def bvh(travel=(0.0, 60.0), period=0.6, seconds=1.2, lean=0.0, still=0.0):
    """A synthetic CMU run: the hips go at `travel` (units a second, x and
    z) bobbing twice a `period`, the thighs swing and the knees bend in
    opposition, and the spine turns `lean` degrees a second (so no cycle
    closes on its own). For its first `still` seconds it stands."""
    out, order = ["HIERARCHY"], []

    def write(name, depth):
        parent, offset = next((p, o) for n, p, o in BVH_BODY if n == name)
        order.append((name, parent))
        pad = "\t" * depth
        out.append(f"{pad}{'ROOT' if parent is None else 'JOINT'} {name}")
        out.append(f"{pad}{{")
        out.append(f"{pad}\tOFFSET {offset[0]} {offset[1]} {offset[2]}")
        if parent is None:
            out.append(f"{pad}\tCHANNELS 6 Xposition Yposition Zposition Zrotation Yrotation Xrotation")
        else:
            out.append(f"{pad}\tCHANNELS 3 Zrotation Yrotation Xrotation")
        children = [n for n, p, _ in BVH_BODY if p == name]
        for c in children:
            write(c, depth + 1)
        if not children:
            out.append(f"{pad}\tEnd Site\n{pad}\t{{\n{pad}\t\tOFFSET 0 1 0\n{pad}\t}}")
        out.append(f"{pad}}}")

    write("Hips", 0)
    frames = int(seconds * 120)
    out += ["MOTION", f"Frames: {frames}", "Frame Time: 0.00833333"]
    for k in range(frames):
        t = max(0.0, k / 120 - still)
        phase = 2 * math.pi * t / period
        row = []
        for name, parent in order:
            if parent is None:
                row += [travel[0] * t, 17.5 + 0.4 * math.cos(2 * phase), travel[1] * t, 0, 0, 0]
            elif name in ("LeftUpLeg", "RightUpLeg"):
                row += [0, 0, (30 if name[0] == "L" else -30) * math.sin(phase)]
            elif name in ("LeftLeg", "RightLeg"):
                row += [0, 0, 20 * (1 + (1 if name[0] == "L" else -1) * math.cos(phase))]
            elif name == "Spine":
                row += [lean * t, 0, 0]
            else:
                row += [0, 0, 0]
        out.append(" ".join(f"{v:.5f}" for v in row))
    return ("\n".join(out) + "\n").encode()


class Capture(unittest.TestCase):
    def test_a_run_becomes_one_closed_loop_held_in_place_heading_forward(self):
        source = R.BvhSource(bvh(travel=(42.0, 42.0), lean=6.0), "16_46")
        times, rots, trans = source.frames("16_46")
        sk, hips = source.skeleton, source.skeleton.find("Hips")
        self.assertAlmostEqual(times[-1], 0.6, places=4)
        self.assertEqual(len(times), 19)
        # Closed: the gap is spread along the cycle, so the spine's steady
        # turn (not only the last key's) is taken out evenly.
        for j in range(len(sk.names)):
            self.assertLess(R.qangle(rots[0][j], rots[-1][j]), 1e-6, sk.names[j])
        spine = sk.find("Spine")
        for r in rots:
            self.assertLess(R.qangle(rots[0][spine], r[spine]), 1e-6)
        # Turned from the diagonal to +Z, and in place with its bob.
        self.assertLess(R.qangle(rots[0][hips], R.qaxis_angle(R.Y, -math.pi / 4)), 1e-6)
        for t in trans:
            self.assertAlmostEqual(t[hips][0], 0.0, places=6)
            self.assertAlmostEqual(t[hips][2], 0.0, places=6)
        heights = [t[hips][1] for t in trans]
        self.assertAlmostEqual(max(heights) - min(heights), 0.8 * R.BVH_UNIT, delta=0.002)

    def test_a_trial_that_starts_standing_loops_its_run_not_its_stillness(self):
        source = R.BvhSource(bvh(seconds=1.8, still=0.6), "16_46")
        times, rots, trans = source.frames("16_46")
        thigh = source.skeleton.find("LeftUpLeg")
        self.assertGreater(max(R.qangle(rots[0][thigh], r[thigh]) for r in rots), 0.5)

    def test_a_capture_without_fingers_holds_them_and_turns_the_body(self):
        with open(PROTAGONIST, "rb") as f:
            tk = R.read_target(f.read())
        rt = R.Retargeter(R.BvhSource(bvh(), "16_46"), tk)
        index, thigh = tk.find("index_02_l"), tk.find("thigh_l")
        keys, _ = rt.clip("16_46", True, False)
        self.assertEqual({local[index] for _, local, _ in keys}, {tk.rest_r[index]})
        self.assertGreater(R.qangle(keys[0][1][thigh], keys[4][1][thigh]), 0.3)
        curl = R.qaxis_angle((1.0, 0.0, 0.0), 0.5)
        rt.held = {index: curl}
        keys, _ = rt.clip("16_46", True, False)
        self.assertEqual({local[index] for _, local, _ in keys}, {curl})

    def test_the_arms_spread_out_from_the_body_and_keep_their_swing(self):
        with open(PROTAGONIST, "rb") as f:
            tk = R.read_target(f.read())
        rt = R.Retargeter(R.BvhSource(bvh(), "16_46"), tk)
        _, local, place = rt.clip("16_46", True, False)[0][0]
        place = R.add(R.qrot(rt.t_rest_r[rt.root], place), rt.t_rest_p[rt.root])
        spread = rt.spread_out(local, place, 10.0)
        before, after = rt.posed_world(local, place)[1], rt.posed_world(spread, place)[1]
        for s, sign in (("l", 1), ("r", -1)):
            shoulder, elbow = tk.find(f"upperarm_{s}"), tk.find(f"lowerarm_{s}")
            a = R.sub(before[elbow], before[shoulder])
            b = R.sub(after[elbow], after[shoulder])
            turn = math.degrees(math.atan2(b[0], -b[1]) - math.atan2(a[0], -a[1]))
            self.assertAlmostEqual(turn * sign, 10.0, delta=0.5, msg=s)
            self.assertEqual(spread[tk.find(f"lowerarm_{s}")], local[tk.find(f"lowerarm_{s}")])
        self.assertIs(rt.spread_out(local, place, 0.0), local)
        never = type("Never", (), {"clear": lambda self, tk, poses: False})()
        with self.assertRaisesRegex(R.RetargetError, r"^no reach up to 0.2 m keeps the arms out of the chest$"):
            rt.cleared(rt.clip("16_46", True, False)[0][:2], never)

    def test_the_shoulders_rest_and_the_arms_keep_their_swing(self):
        with open(PROTAGONIST, "rb") as f:
            tk = R.read_target(f.read())
        rt = R.Retargeter(R.BvhSource(bvh(), "16_46"), tk)
        _, local, place = rt.clip("16_46", True, False)[0][0]
        place = R.add(R.qrot(rt.t_rest_r[rt.root], place), rt.t_rest_p[rt.root])
        clavicle, upper = tk.find("clavicle_l"), tk.find("upperarm_l")
        shrugged = {**local, clavicle: R.qnorm(R.qmul(local[clavicle], R.qaxis_angle((0.0, 0.0, 1.0), 0.3)))}
        relaxed = rt.relax_shoulders(shrugged, place)
        self.assertEqual(relaxed[clavicle], tk.rest_r[clavicle])
        before, after = rt.posed_world(shrugged, place)[0], rt.posed_world(relaxed, place)[0]
        self.assertLess(R.qangle(before[upper], after[upper]), 1e-6)

    def held(self, back, bent, lean=0.0):
        """The protagonist on a synthetic capture, its clavicles shrugged,
        each upper arm hanging `back` degrees behind (plus a sway of 6 over
        the keys), each elbow bent `bent` degrees (plus a sway of 8), and the
        trunk leaning `lean` degrees to the side (plus a sway of 3): the
        retargeter, the target, and the keys with their pelvis world places
        and their pelvis local places."""
        with open(PROTAGONIST, "rb") as f:
            tk = R.read_target(f.read())
        rt = R.Retargeter(R.BvhSource(bvh(), "16_46"), tk)
        root = rt.t_rest_r[rt.root]
        world, local_keys = [], []
        clip = rt.clip("16_46", True, False)[0]
        for k, (t, local, p) in enumerate(clip):
            wave = math.sin(2 * math.pi * k / (len(clip) - 1))
            place = R.add(R.qrot(root, p), rt.t_rest_p[rt.root])
            local = dict(local)
            for s, side in (("l", 1.0), ("r", -1.0)):
                c = tk.find(f"clavicle_{s}")
                local[c] = R.qnorm(R.qmul(local[c], R.qaxis_angle((0.0, 0.0, 1.0), 0.3)))
            rot, _ = rt.posed_world(local, place)
            b = tk.find("spine_01")
            turned = R.qmul(R.qaxis_angle((0.0, 0.0, 1.0), math.radians(lean + 3.0 * wave)), rot[b])
            local[b] = R.qnorm(R.qmul(R.qinv(rot[tk.parent[b]]), turned))
            for s in "lr":
                u, lo, ha = (tk.find(f"{n}_{s}") for n in ("upperarm", "lowerarm", "hand"))
                rot, pos = rt.posed_world(local, place)
                d = R.unit(R.sub(pos[lo], pos[u]))
                a = math.radians(back + 6.0 * wave)
                hang = R.qnorm(R.qmul(R.qarc(d, (0.0, -math.cos(a), -math.sin(a))), rot[u]))
                local[u] = R.qnorm(R.qmul(R.qinv(rot[tk.parent[u]]), hang))
                rot, pos = rt.posed_world(local, place)
                f = R.unit(R.sub(pos[ha], pos[lo]))
                e = math.radians(bent + 8.0 * wave) - a
                fore = R.qnorm(R.qmul(R.qarc(f, (0.0, -math.cos(e), math.sin(e))), rot[lo]))
                local[lo] = R.qnorm(R.qmul(R.qinv(rot[u]), fore))
            world.append((t, local, place))
            local_keys.append((t, local, p))
        return rt, tk, world, local_keys

    def arms(self, rt, tk, keys):
        """Through `keys` (pelvis world places): each upper arm's pitch
        forward of hanging, each elbow's bend and the trunk's lean to the
        side, degrees."""
        pitch, bend, lean = [], [], []
        for _, local, place in keys:
            pos = rt.posed_world(local, place)[1]
            f = lambda n: pos[tk.find(n)]  # noqa: E731
            t = R.unit(R.sub(f("neck_01"), f("pelvis")))
            lean.append(math.degrees(math.atan2(t[0], t[1])))
            for s in "lr":
                u = R.unit(R.sub(f(f"lowerarm_{s}"), f(f"upperarm_{s}")))
                pitch.append(math.degrees(math.atan2(u[2], -u[1])))
                bend.append(math.degrees(R.angle_between(u, R.sub(f(f"hand_{s}"), f(f"lowerarm_{s}")))))
        return pitch, bend, lean

    def test_a_standing_clip_hangs_its_arms_at_ease(self):
        rt, tk, keys, _ = self.held(back=20.0, bent=40.0)
        hung = rt.hang_arms(keys)
        mean = lambda v: sum(v) / len(v)  # noqa: E731
        span = lambda v: max(v) - min(v)  # noqa: E731
        pitch, bend, _ = self.arms(rt, tk, keys)
        pitch2, bend2, _ = self.arms(rt, tk, hung)
        self.assertLess(mean(pitch), -15.0)
        self.assertAlmostEqual(mean(pitch2), R.HANG[0], delta=0.5)
        self.assertAlmostEqual(mean(bend2), R.HANG[1], delta=0.5)
        self.assertAlmostEqual(span(pitch2), span(pitch) * R.HANG[2], delta=1.0)
        self.assertAlmostEqual(span(bend2), span(bend) * R.HANG[2], delta=1.5)
        for _, local, _ in hung:
            for s in "lr":
                c, h = tk.find(f"clavicle_{s}"), tk.find(f"hand_{s}")
                self.assertEqual(local[c], tk.rest_r[c])
        self.assertEqual(hung[0][1][tk.find("hand_l")], keys[0][1][tk.find("hand_l")])

    def test_a_bad_bvh_is_refused(self):
        good = bvh().decode()  # 144 frames
        bad = [
            b"\xff\xfe",
            good.replace("HIERARCHY", "HIERARCHX").encode(),
            good.replace("Frames: 144", "Frames: 1").encode(),
            good.replace("CHANNELS 3 Zrotation", "CHANNELS 6 Zrotation", 1).encode(),
            good.replace("Zrotation Yrotation Xrotation\n", "Zrotation Yrotation Wrotation\n", 1).encode(),
            good.replace("17.5", "nan", 1).encode(),
            (good + "0.0\n").encode(),
            good.rsplit("\n", 3)[0].encode(),
        ]
        for data in bad:
            with self.assertRaises(R.RetargetError):
                R.BvhSource(data, "16_46").frames("16_46")
        with self.assertRaises(R.RetargetError):
            R.BvhSource(good.encode(), "16_46").frames("09_01")
        with self.assertRaises(R.RetargetError):
            R.BvhSource(bvh(seconds=0.4), "16_46").frames("16_46")


class Command(unittest.TestCase):
    def test_append_refuses_a_target_and_a_missing_source(self):
        err = io.StringIO()
        with redirect_stderr(err), self.assertRaises(SystemExit) as stop:
            R.main(["--ual1", "/nonexistent.glb", "--ual2", "/nonexistent.glb", "--append", SHIPPED,
                    "--target", PROTAGONIST, "-o", os.devnull])
        self.assertEqual(stop.exception.code, 2)
        self.assertIn("leave out --target", err.getvalue())
        # Appending a clip whose source was not given is a one-line error.
        with self.assertRaisesRegex(R.RetargetError, "needs the cmu source"):
            R.build({"ual1": source()}, target(), [("jog", "cmu", "run_cycle", True, True)])

    def test_a_bad_input_is_one_line_on_stderr(self):
        err = io.StringIO()
        with redirect_stderr(err), redirect_stdout(io.StringIO()):
            code = R.main(["--ual1", "/nonexistent.glb", "--ual2", "/nonexistent.glb", "--cmu", "/nonexistent.bvh",
                           "--sprint", "/nonexistent.bvh", "--target", PROTAGONIST, "-o", os.devnull])
        self.assertEqual(code, 1)
        self.assertTrue(err.getvalue().startswith("retarget: "))
        self.assertEqual(err.getvalue().count("\n"), 1)



def two_armed_target():
    """The synthetic target with both arms (the walk swings each)."""
    right = [
        ("clavicle_r", "spine_03", (-0.02, 1.45, 0), (-1, 0, 0)),
        ("upperarm_r", "clavicle_r", (-0.18, 1.45, 0), (0, -1, 0)),
        ("lowerarm_r", "upperarm_r", (-0.18, 1.17, 0), (0, -1, 0)),
        ("hand_r", "lowerarm_r", (-0.18, 0.91, 0), (0, -1, 0)),
    ]
    bones = TORSO + arm(False) + right + legs()
    nodes = nodes_of(bones, lambda g: g, ("Root", (0.5, 0.5, 0.5, 0.5)), 1.1, (0.0, 0.03, 0.01))
    data = json.dumps({"asset": {"version": "2.0"}, "nodes": nodes, "scenes": [{"nodes": [0]}]})
    return R.read_target(data.encode())


def stand_in(tk):
    """A Retargeter on the target alone: what the walk's upright pass and
    the target-side speed read."""
    rt = R.Retargeter.__new__(R.Retargeter)
    rt.target, rt.root = tk, tk.find("Root")
    rt.t_rest_r, rt.t_rest_p = tk.world()
    return rt


def walk_keys(rt, count=24, swing=0.4, lean=0.15, bend=0.35, reach=None):
    """A synthetic crouched walk loop on the target: `count` keys and the
    first again; the thighs swinging `swing` radians either way in
    opposition, the knees bent at least `bend`, folding more as the leg
    swings through, the torso leaning `lean` forward and the arms swinging
    against the legs; `reach` (key -> radians) sets a key's thighs that
    far ahead and behind, straight.
    Each key is (time, local rotations by node, pelvis world place)."""
    tk = rt.target
    f = tk.find
    rest_rot, rest_pos = tk.world()
    keys = []
    for k in range(count + 1):
        phase = 2 * math.pi * (k % count) / count
        turn = {}
        for s, sign in (("l", 1.0), ("r", -1.0)):
            thigh = -swing * sign * math.sin(phase)
            if reach and k % count in reach:
                thigh = -reach[k % count] * sign  # one leg out ahead, one behind
            knee = bend + 0.6 * max(0.0, sign * math.cos(phase))
            if reach and k % count in reach:
                knee = 0.0
            turn[f"thigh_{s}"] = R.qaxis_angle((1, 0, 0), thigh)
            for b in (f"calf_{s}", f"foot_{s}", f"ball_{s}"):
                turn[b] = R.qaxis_angle((1, 0, 0), thigh + knee)
        upper = R.qaxis_angle((1, 0, 0), lean)
        for b in ("spine_01", "spine_02", "spine_03", "neck_01", "head", "clavicle_l", "clavicle_r"):
            turn[b] = upper
        for s, sign in (("l", 1.0), ("r", -1.0)):
            arm_turn = R.qmul(upper, R.qaxis_angle((1, 0, 0), 0.3 * sign * math.sin(phase)))
            for b in (f"upperarm_{s}", f"lowerarm_{s}", f"hand_{s}"):
                turn[b] = arm_turn
        world = {}
        local = {}
        for i in tk.order:
            name = tk.names[i]
            if i == rt.root or not rt._under_root(i):
                continue
            world[i] = R.qnorm(R.qmul(turn.get(name, (0.0, 0.0, 0.0, 1.0)), rest_rot[i]))
            parent = tk.parent[i]
            parent_world = world.get(parent, rest_rot[parent])
            local[i] = R.qnorm(R.qmul(R.qinv(parent_world), world[i]))
        place = rest_pos[f("pelvis")]
        keys.append((k / 30.0, local, place))
    return keys


class UprightWalk(unittest.TestCase):
    """The walk's upright pass (``Retargeter.upright``) and the target-side
    speed, on synthetic rigs: no packs needed."""

    def setUp(self):
        self.tk = two_armed_target()
        self.rt = stand_in(self.tk)

    def tracked(self, keys):
        named = [({self.tk.names[k]: q for k, q in local.items()}, R.sub(place, (0, 0, 0))) for _, local, place in keys]
        return named

    def bends(self, keys):
        f = self.tk.find
        out = []
        for _, local, place in keys:
            _, pos = self.rt.posed_world(local, place)
            for s in "lr":
                out.append(math.degrees(R.angle_between(
                    R.sub(pos[f(f"calf_{s}")], pos[f(f"thigh_{s}")]), R.sub(pos[f(f"foot_{s}")], pos[f(f"calf_{s}")]))))
        return out

    def leans(self, keys):
        f = self.tk.find
        out = []
        for _, local, place in keys:
            _, pos = self.rt.posed_world(local, place)
            v = R.sub(pos[f("neck_01")], pos[f("pelvis")])
            out.append(math.degrees(math.atan2(v[2], v[1])))
        return out

    def test_it_is_the_same_every_time_and_loops(self):
        a = self.rt.upright(walk_keys(self.rt))
        b = self.rt.upright(walk_keys(self.rt))
        self.assertEqual(a, b)
        # The last key is the first again: no step at the wrap.
        for k, q in a[0][1].items():
            self.assertLess(R.qangle(q, a[-1][1][k]), 1e-6, self.tk.names[k])
        self.assertLess(R.length(R.sub(a[0][2], a[-1][2])), 1e-9)

    def test_no_knee_straightens_past_its_least_bend_and_the_torso_stands_up(self):
        keys = walk_keys(self.rt, bend=0.05, lean=0.2)
        self.assertGreater(min(self.leans(keys)), 8.0)
        out = self.rt.upright(keys)
        self.assertGreater(min(self.bends(out)), R.KNEE_MIN - 1e-6)
        leans = self.leans(out)
        # Turned back about spine_01 by the mean lean's excess: the line
        # from the pelvis (below the turn) to the neck ends near LEAN.
        self.assertAlmostEqual(sum(leans[:-1]) / len(leans[:-1]), R.LEAN, delta=1.5)
        self.assertLess(max(leans), 5.0)

    def test_the_arms_hang_and_swing_and_the_head_comes_level(self):
        out = self.rt.upright(walk_keys(self.rt))
        f = self.tk.find
        rest_rot, _ = self.tk.world()

        def pitch(q):
            up = R.qrot(q, (0.0, 1.0, 0.0))
            return math.degrees(math.atan2(up[2], up[1]))

        swings, elbows = [], []
        for _, local, place in out:
            rot, pos = self.rt.posed_world(local, place)
            self.assertAlmostEqual(pitch(rot[f("head")]), pitch(rest_rot[f("head")]), delta=0.5)
            for s in "lr":
                u = R.sub(pos[f(f"lowerarm_{s}")], pos[f(f"upperarm_{s}")])
                swings.append(math.degrees(math.atan2(u[2], -u[1])))
                elbows.append(math.degrees(R.angle_between(u, R.sub(pos[f(f"hand_{s}")], pos[f(f"lowerarm_{s}")]))))
        rest = R.sub(self.rt.t_rest_p[f("lowerarm_l")], self.rt.t_rest_p[f("upperarm_l")])
        hang = math.degrees(math.atan2(rest[2], -rest[1]))
        # From 10 degrees back to 14 forward of hanging; the elbows within
        # ELBOW, loose.
        self.assertAlmostEqual(min(swings), hang - 10.0, delta=1.5)
        self.assertAlmostEqual(max(swings), hang + 14.0, delta=1.5)
        self.assertGreater(min(elbows), R.ELBOW[0] - 0.5)
        self.assertLess(max(elbows), R.ELBOW[1] + 0.5)
        # Hanging WALK_OUT out from the body, not the rest's 11 degrees.
        for s, side in (("l", 1.0), ("r", -1.0)):
            wide = []
            for _, local, place in out:
                pos = self.rt.posed_world(local, place)[1]
                u = R.unit(R.sub(pos[f(f"lowerarm_{s}")], pos[f(f"upperarm_{s}")]))
                wide.append(math.degrees(math.atan2(side * u[0], -u[1])))
            self.assertAlmostEqual(sum(wide) / len(wide), R.WALK_OUT, delta=1.0, msg=s)

    def test_a_key_neither_leg_can_reach_takes_the_hips_from_its_neighbours(self):
        # Key 6's legs swing out level, too far to reach at any height: its
        # hips are the blend of the keys either side, never sunk to rest.
        keys = walk_keys(self.rt, reach={6: 1.55})
        legs = self.rt._legs(keys[6][1], keys[6][2])[1]
        self.assertTrue(all(legs[s].rise is None for s in "lr"))
        out = self.rt._strides(keys)
        heights = [place[1] for _, _, place in out]
        middle = (heights[5] + heights[7]) / 2
        self.assertAlmostEqual(heights[6], middle, delta=0.01)
        self.assertGreater(min(heights[5:8]), self.rt.t_rest_p[self.tk.find("pelvis")][1])
        # Before the loop's smoothing too: the key's own rise is its
        # neighbours' blend, not the rest's 0.
        smooth = R.RISE_SMOOTH
        self.addCleanup(setattr, R, "RISE_SMOOTH", smooth)
        R.RISE_SMOOTH = 0
        raw = [place[1] for _, _, place in self.rt._strides(keys)]
        self.assertAlmostEqual(raw[6], (raw[5] + raw[7]) / 2, delta=0.005)
        # No key reaches: an error, not a walk sunk to the ground.
        none = walk_keys(self.rt, reach={k: 1.55 for k in range(24)})
        with self.assertRaises(R.RetargetError):
            self.rt._strides(none)

    def test_holes_round_a_loop_are_filled_from_both_sides(self):
        self.assertEqual(R.fill_round([1.0, None, 3.0, None]), [1.0, 2.0, 3.0, 2.0])
        self.assertEqual(R.fill_round([None, 4.0, None, None]), [4.0, 4.0, 4.0, 4.0])
        with self.assertRaises(R.RetargetError):
            R.fill_round([None, None])

    def test_straight_legs_and_arms_are_solved_without_a_pole_or_a_hinge(self):
        # Legs straight under the hips (no knee bend: no pole to bend it
        # by), and a swing that keeps them straight (no hinge to smooth
        # about): still a finite walk, every knee at its least bend.
        keys = walk_keys(self.rt, swing=0.0, bend=0.0)
        out = self.rt.upright(keys)
        for _, local, place in out:
            for q in local.values():
                self.assertTrue(all(math.isfinite(c) for c in q))
            self.assertTrue(all(math.isfinite(c) for c in place))
        self.assertGreater(min(self.bends(out)), R.KNEE_MIN - 1e-6)
        # Straight legs lifted off the ground (the hips raised on every
        # other key): a swinging knee with no bend has no hinge to turn
        # about, and is left as it is.
        lifted = [(t, local, R.add(place, (0.0, 0.1 * (k % 2), 0.0))) for k, (t, local, place) in enumerate(keys)]
        for _, local, place in self.rt._smooth_swings(lifted):
            for q in local.values():
                self.assertTrue(all(math.isfinite(c) for c in q))

    def test_the_target_side_speed_is_a_planted_foots_slide(self):
        # The speed is how fast a planted foot slides back (toward -z). A
        # foot on the ground carried forward 0.03 m a key (+z) at 30 keys a
        # second slides back at -0.9 m/s; with no two keys on the ground,
        # an error.
        out = self.rt.upright(walk_keys(self.rt))
        self.assertGreater(self.rt.target_ground_speed(out), 0.0)
        rest = self.rt.t_rest_p[self.tk.find("pelvis")]
        still = {k: q for k, q in walk_keys(self.rt, swing=0.0)[0][1].items()}
        sliding = [(k / 30.0, still, (rest[0], rest[1], rest[2] + 0.03 * k)) for k in range(8)]
        self.assertAlmostEqual(self.rt.target_ground_speed(sliding), -0.9, places=6)
        hopping = [(k / 30.0, still, (rest[0], rest[1] + 0.5 * (k % 2), rest[2])) for k in range(8)]
        with self.assertRaises(R.RetargetError):
            self.rt.target_ground_speed(hopping)


class FollowUps(unittest.TestCase):
    """The review's follow-ups: every key checked for the chests, both
    measures kept, one-line errors from ``--model``, an atomic write and one
    leg length."""

    def test_every_key_is_checked_for_the_chests_not_only_the_sampled_ones(self):
        # Twelve keys; only key 3 (between the samples) needs the arms 1 cm
        # forward to clear.
        rt = R.Retargeter.__new__(R.Retargeter)
        rt.target, rt.root = None, 0
        rt.t_rest_r, rt.t_rest_p = {0: (0.0, 0.0, 0.0, 1.0)}, {0: (0.0, 0.0, 0.0)}
        rt.reach_out = lambda local, place, ahead: {"ahead": ahead}

        class Chest:
            def clear(self, target, poses):
                return all(p != (0.0, 0.0, 3.0) or local["ahead"] >= 0.01 - 1e-9 for local, p in poses)

        keys = [(k / 10, {}, (0.0, 0.0, float(k))) for k in range(12)]
        moved, ahead = rt.cleared(keys, Chest())
        self.assertAlmostEqual(ahead, 0.01)
        self.assertTrue(all(local["ahead"] == ahead for _, local, _ in moved))

    def test_a_cleared_walk_keeps_its_reach_and_its_speeds(self):
        class Clear:
            def clear(self, target, poses):
                return True

        rt = R.Retargeter(source(PelvisAndLoops().loop(1.0, 0.0)), target(leg_scale=1.25))
        _, measures = rt.clip("Loop", True, True, clearance=Clear())
        self.assertEqual(sorted(measures), ["ground_speed", "reach", "stride"])
        self.assertEqual(measures["reach"], 0.0)
        self.assertAlmostEqual(measures["ground_speed"], 1.25, places=3)

    def test_a_bad_model_is_a_one_line_error(self):
        import tempfile
        from unittest import mock

        with tempfile.TemporaryDirectory() as tmp:
            for name, data in (("text.gltf", b"\xff\xfe not utf-8"), ("json.gltf", b"{ not json\n at all")):
                path = os.path.join(tmp, name)
                with open(path, "wb") as f:
                    f.write(data)
                with self.assertRaises(R.RetargetError) as e:
                    R.Clearance([path])
                self.assertIn(path, str(e.exception))
                self.assertNotIn("\n", str(e.exception))
            # A model whose data will not take its shape (an index list not
            # of whole triangles, say).
            with mock.patch.object(R.Clearance, "_read", side_effect=ValueError("cannot reshape\nsize 7")):
                with self.assertRaises(R.RetargetError) as e:
                    R.Clearance(["model.gltf"])
            self.assertEqual(str(e.exception), "model.gltf: ValueError: cannot reshape")
        # Without numpy.
        with mock.patch.dict("sys.modules", {"numpy": None}):
            with self.assertRaises(R.RetargetError) as e:
                R.Clearance([PROTAGONIST])
            self.assertIn("numpy", str(e.exception))

    def test_a_talker_is_cleared_for_the_talk_only(self):
        # A model given with --talker joins the chest check of idle_talk,
        # never that of the folded arms.
        from unittest import mock

        def read(self, path):
            return {"name": os.path.basename(path)}

        with mock.patch.object(R.Clearance, "_read", read):
            both = R.Clearance(["man.gltf"], ["woman.gltf"])
        names = lambda c: [m["name"] for m in c.models]  # noqa: E731
        self.assertEqual(names(both.for_clip("idle_talk")), ["man.gltf", "woman.gltf"])
        self.assertEqual(names(both.for_clip("idle_fold_arms")), ["man.gltf"])
        self.assertEqual(names(both), ["man.gltf", "woman.gltf"], "the whole set is kept")
        with mock.patch.object(R.Clearance, "_read", read):
            talkers = R.Clearance([], ["woman.gltf"])
        self.assertIsNone(talkers.for_clip("idle_fold_arms"))
        # From the command line too: --talker alone is a check.
        with mock.patch.object(R, "Clearance") as made, mock.patch.object(R, "build", side_effect=R.RetargetError("stop")), \
                mock.patch.object(R, "read_file", return_value=b""), mock.patch.object(R, "Source"), \
                mock.patch.object(R, "BvhSource"), mock.patch.object(R, "read_target"), \
                redirect_stderr(io.StringIO()):
            R.main(["--ual1", "a", "--ual2", "b", "--cmu", "c.bvh", "--sprint", "s.bvh", "--target", "t", "-o", "o", "--talker", "w2",
                    "--talker", "w1"])
        made.assert_called_once_with([], ["w1", "w2"])

    def test_the_output_is_written_whole_or_not_at_all(self):
        import tempfile
        from unittest import mock

        with tempfile.TemporaryDirectory() as tmp:
            out = os.path.join(tmp, "humanoid.glb")
            R.write_atomically(out, b"first")
            with open(out, "rb") as f:
                self.assertEqual(f.read(), b"first")
            # Interrupted before it is in place: the old file stays whole,
            # and nothing else is left beside it.
            with mock.patch.object(R.os, "replace", side_effect=OSError("disk full")):
                with self.assertRaises(OSError):
                    R.write_atomically(out, b"second, longer")
            with open(out, "rb") as f:
                self.assertEqual(f.read(), b"first")
            self.assertEqual(os.listdir(tmp), ["humanoid.glb"])
            R.write_atomically(out, b"second")
            with open(out, "rb") as f:
                self.assertEqual(f.read(), b"second")
            self.assertEqual(os.listdir(tmp), ["humanoid.glb"])

    def test_the_leg_is_the_thigh_and_the_shin(self):
        tk = target(leg_scale=1.5)
        thigh = R.length(tk.rest_t[tk.find("calf_l")])
        shin = R.length(tk.rest_t[tk.find("foot_l")])
        self.assertAlmostEqual(R.leg_length(tk, "calf_l", "foot_l"), thigh + shin)
        self.assertGreater(thigh, 0.0)
        self.assertGreater(shin, 0.0)

if __name__ == "__main__":
    unittest.main()
