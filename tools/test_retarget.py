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
            self.assertEqual(missing, [], kind)
            self.assertEqual(len(set(names.values())), len(names), f"{kind}: two bones share a source")
            for game, src in names.items():
                # Left stays left: game_engine's _l is Rigify's .L.
                if game.endswith("_l"):
                    self.assertTrue(src.endswith((".L", "_l")), f"{kind} {game} -> {src}")
                if game.endswith("_r"):
                    self.assertTrue(src.endswith((".R", "_r")), f"{kind} {game} -> {src}")

    def test_the_maps_name_what_each_library_calls_its_bones(self):
        one, two = R.ual1_map(), R.ual2_map()
        self.assertEqual(one["pelvis"], "DEF-hips")
        self.assertEqual(one["calf_l"], "DEF-shin.L")
        self.assertEqual(one["ball_r"], "DEF-toe.R")
        self.assertEqual(one["middle_02_l"], "DEF-f_middle.02.L")
        self.assertEqual(one["thumb_03_r"], "DEF-thumb.03.R")
        self.assertEqual(two["head"], "Head")
        self.assertEqual(two["calf_l"], "calf_l")


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
            self.assertGreater(min(elbows), 8.0)
            self.assertLess(max(elbows), 27.0)
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
        for clip in R.CLEAR:
            for local, pelvis in self.keys(clip)[::5]:
                for model in clearance.for_clip(clip).models:
                    depth = clearance.depth(model, reference, local, pelvis)
                    self.assertLessEqual(depth, 0.0, f"{clip} {model['name']}: {depth:.3f} m in")
        extras = {a["name"]: a["extras"] for a in self.doc["animations"]}
        self.assertGreater(extras["idle_fold_arms"]["reach"], 0.05)

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
        cls.sources = {"ual1": one, "ual2": two}

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

    def test_the_walks_and_runs_have_plausible_speeds(self):
        doc, _, summary = R.build(self.sources, self.target)
        speeds = {game: m.get("ground_speed") for game, _, _, _, _, m in summary}
        self.assertTrue(0.9 < speeds["walk"] < 1.4, speeds)
        self.assertLess(speeds["walk"], speeds["jog"])
        self.assertLess(speeds["jog"], speeds["sprint"])


class Command(unittest.TestCase):
    def test_a_bad_input_is_one_line_on_stderr(self):
        err = io.StringIO()
        with redirect_stderr(err), redirect_stdout(io.StringIO()):
            code = R.main(["--ual1", "/nonexistent.glb", "--ual2", "/nonexistent.glb",
                           "--target", PROTAGONIST, "-o", os.devnull])
        self.assertEqual(code, 1)
        self.assertTrue(err.getvalue().startswith("retarget: "))
        self.assertEqual(err.getvalue().count("\n"), 1)



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
                mock.patch.object(R, "read_target"), redirect_stderr(io.StringIO()):
            R.main(["--ual1", "a", "--ual2", "b", "--target", "t", "-o", "o", "--talker", "w2", "--talker", "w1"])
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
