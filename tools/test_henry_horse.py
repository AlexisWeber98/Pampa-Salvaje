"""Tests of tools/henry_horse.py on a tiny synthetic Sketchfab download.

Standard library only: ``python3 -I -B -m unittest discover -s tools -p test_henry_horse.py``.
"""

import io
import itertools
import json
import math
import os
import shutil
import struct
import subprocess
import tempfile
import unittest
from contextlib import redirect_stdout
from unittest import mock

import glbtrim
import henry_horse as hh

# The synthetic horse, in the source's centimetres, built facing +X: the
# wrapper turns it a quarter about Y and scales it by 0.1, as Sketchfab's
# wrappers leave the real one in decimetres.
JOINTS = [
    # name, parent, translation
    ("_rootJoint", None, (0, 0, 0)),
    (hh.ROOT, 0, (0, 0, 0)),
    (hh.PELVIS, 1, (0, 120, -50)),
    (hh.HEAD, 1, (0, 180, 80)),
    (hh.LEGS[0], 1, (10, 10, 60)),
    (hh.LEGS[1], 1, (-10, 10, 60)),
    (hh.LEGS[2], 1, (10, 10, -50)),
    (hh.LEGS[3], 1, (-10, 10, -50)),
    ("BN_L_Stirrup_048_051", 1, (20, 100, 0)),
    ("BN_L_Stirrup_048_end_093", 8, (0, -30, 0)),
]
QUARTER_Y = (0.0, math.sin(math.pi / 4), 0.0, math.cos(math.pi / 4))
WRAPPER_SCALE = 0.1


def leg_shoe(k):
    x, _, z = JOINTS[4 + k][2]
    return [(x - 4, 2, z), (x + 4, 2, z), (x, 2, z + 6)], 4 + k


# material -> (triangle, joint per corner)
PRIMS = {
    "Saddle": ([(20, 110, 0), (20, 90, 10), (20, 90, -10)], [8, 8, 8]),
    "Horse": ([(0, 120, -50), (0, 180, 60), (0, 100, 20)], [2, 3, 1]),
    "Hair": ([(0, 190, 70), (0, 200, 70), (0, 190, 75)], [3, 3, 3]),
    "Cornea": ([(5, 185, 85), (5, 186, 85), (5, 185, 86)], [3, 3, 3]),
}


def floats(values):
    return struct.pack(f"<{len(values)}f", *values)


def build(folder, uri="scene.bin", stirrup_weight=0.0):
    """Write scene.gltf and scene.bin of the synthetic horse."""
    blob = bytearray()
    views, accessors = [], []

    def add(data, count, kind, component=5126, **extra):
        views.append({"buffer": 0, "byteOffset": len(blob), "byteLength": len(data)})
        blob.extend(data)
        blob.extend(b"\0" * (-len(blob) % 4))
        accessors.append({"bufferView": len(views) - 1, "componentType": component,
                          "count": count, "type": kind, **extra})
        return len(accessors) - 1

    names = [j[0] for j in JOINTS]
    nodes = [
        {"name": "Sketchfab_model", "children": [1], "rotation": list(QUARTER_Y),
         "scale": [WRAPPER_SCALE] * 3},
        {"name": "Object_9", "children": []},
        {"name": names[0]},
    ]
    for name, parent, t in JOINTS[1:]:
        nodes.append({"name": name, "translation": list(t)})
    joint_nodes = [2] + list(range(3, 3 + len(JOINTS) - 1))
    for k, (_, parent, _) in enumerate(JOINTS):
        if parent is not None:
            nodes[joint_nodes[parent]].setdefault("children", []).append(joint_nodes[k])
    chains = []
    for k, (_, parent, t) in enumerate(JOINTS):
        base = hh.identity() if parent is None else chains[parent]
        chains.append(hh.mul(base, hh.trs(t)))
    ibm = add(floats([x for c in chains for x in hh.to_gltf(hh.inverse(c))]), len(JOINTS), "MAT4")

    prims = dict(PRIMS)
    shoes = [leg_shoe(k) for k in range(4)]
    prims["Horseshoe"] = ([p for s in shoes for p in s[0]], [s[1] for s in shoes for _ in range(3)])
    materials, meshes = [], []
    for material, (points, joints) in prims.items():
        n = len(points)
        pos = add(floats([x for p in points for x in p]), n, "VEC3",
                  min=[min(p[i] for p in points) for i in range(3)],
                  max=[max(p[i] for p in points) for i in range(3)])
        nrm = add(floats([0, 0, 1] * n), n, "VEC3")
        uv = add(floats([0.5, 0.5] * n), n, "VEC2")
        weights = []
        for j in joints:
            if material == "Horse" and stirrup_weight:
                weights.append(((j, 8), (1 - stirrup_weight, stirrup_weight)))
            else:
                weights.append(((j, 0), (1.0, 0.0)))
        jnt = add(struct.pack(f"<{4 * n}H", *[x for (a, b), _ in weights for x in (a, b, 0, 0)]), n, "VEC4", 5123)
        wgt = add(floats([x for _, (a, b) in weights for x in (a, b, 0, 0)]), n, "VEC4")
        idx = add(struct.pack(f"<{n}H", *range(n)), n, "SCALAR", 5123)
        materials.append({"name": material, "pbrMetallicRoughness": {"roughnessFactor": 0.82}})
        meshes.append({"name": material, "primitives": [{
            "attributes": {"POSITION": pos, "NORMAL": nrm, "TEXCOORD_0": uv, "TEXCOORD_1": uv,
                           "JOINTS_0": jnt, "WEIGHTS_0": wgt},
            "indices": idx, "material": len(materials) - 1, "mode": 4}]})
        nodes.append({"name": f"Object_{material}", "mesh": len(meshes) - 1, "skin": 0})
        nodes[1]["children"].append(len(nodes) - 1)
    nodes[1]["children"].append(2)

    def clip(name, channels):
        samplers, chans = [], []
        for joint, path, keys in channels:
            times = [t for t, _ in keys]
            values = [x for _, v in keys for x in v]
            width = 4 if path == "rotation" else 3
            i = add(floats(times), len(times), "SCALAR", min=[min(times)], max=[max(times)])
            o = add(floats(values), len(keys), f"VEC{width}")
            samplers.append({"input": i, "output": o, "interpolation": "LINEAR"})
            chans.append({"sampler": len(samplers) - 1, "target": {"node": joint_nodes[joint], "path": path}})
        return {"name": name, "samplers": samplers, "channels": chans}

    bend = (math.sin(0.2), 0.0, 0.0, math.cos(0.2))
    animations = [
        clip("Skeleton|1 Ilde", [
            (2, "rotation", [(0.0, (0, 0, 0, 1)), (1.0, bend)]),
            (1, "scale", [(0.0, (1, 1, 1)), (1.0, (1, 1, 1))]),
        ]),
        clip("Skeleton|Walk", [
            (1, "translation", [(0.0, (0, 0, 0)), (0.5, (0, 5, 10)), (1.0, (0, 0, 0))]),
            (4, "rotation", [(0.0, (0, 0, 0, 1)), (1.0, bend)]),
        ]),
    ]
    doc = {
        "asset": {"version": "2.0"},
        "scene": 0,
        "scenes": [{"nodes": [0]}],
        "nodes": nodes,
        "meshes": meshes,
        "materials": materials,
        "skins": [{"inverseBindMatrices": ibm, "joints": joint_nodes, "skeleton": 2}],
        "animations": animations,
        "accessors": accessors,
        "bufferViews": views,
        "buffers": [{"byteLength": len(blob), "uri": uri}],
    }
    with open(os.path.join(folder, "scene.gltf"), "w") as f:
        json.dump(doc, f)
    with open(os.path.join(folder, "scene.bin"), "wb") as f:
        f.write(bytes(blob))
    return doc, bytes(blob)


CLIPS = ("Idle_1", "Walk")


def source_skinned(doc, binary, clip, time):
    """The source's skinned vertices (glTF: joint world * IBM * vertex), by
    material, at `time` of animation `clip`."""
    nodes = doc["nodes"]
    local = {}
    for k, n in enumerate(nodes):
        local[k] = [tuple(n.get("translation", (0, 0, 0))), tuple(n.get("rotation", (0, 0, 0, 1))),
                    tuple(n.get("scale", (1, 1, 1)))]
    anim = doc["animations"][clip]
    for c in anim["channels"]:
        s = anim["samplers"][c["sampler"]]
        times = [t[0] for t in glbtrim.read_accessor(doc, binary, s["input"])]
        values = glbtrim.read_accessor(doc, binary, s["output"])
        slot = {"translation": 0, "rotation": 1, "scale": 2}[c["target"]["path"]]
        local[c["target"]["node"]][slot] = hh.sample(times, values, time, c["target"]["path"])
    return skin(doc, binary, local)


def skin(doc, binary, local):
    nodes = doc["nodes"]
    parent = {c: k for k, n in enumerate(nodes) for c in n.get("children", [])}

    def world(k):
        m = hh.trs(*local[k])
        return m if k not in parent else hh.mul(world(parent[k]), m)

    s = doc["skins"][0]
    ibm = [hh.from_gltf(m) for m in glbtrim.read_accessor(doc, binary, s["inverseBindMatrices"])]
    mats = [hh.mul(world(j), i) for j, i in zip(s["joints"], ibm)]
    out = {}
    for mesh in doc["meshes"]:
        for prim in mesh["primitives"]:
            a = prim["attributes"]
            pos = glbtrim.read_accessor(doc, binary, a["POSITION"])
            jnt = glbtrim.read_accessor(doc, binary, a["JOINTS_0"])
            wgt = glbtrim.read_accessor(doc, binary, a["WEIGHTS_0"])
            pts = []
            for p, j4, w4 in zip(pos, jnt, wgt):
                q = [0.0, 0.0, 0.0]
                for j, w in zip(j4, w4):
                    q = [x + w * y for x, y in zip(q, hh.apply(mats[j], p))]
                pts.append(tuple(q))
            out[doc["materials"][prim["material"]]["name"]] = pts
    return out


def output_skinned(doc, binary, clip, time):
    local = {k: [tuple(n.get("translation", (0, 0, 0))), tuple(n.get("rotation", (0, 0, 0, 1))),
                 tuple(n.get("scale", (1, 1, 1)))] for k, n in enumerate(doc["nodes"])}
    anim = next(a for a in doc["animations"] if a["name"] == clip)
    for c in anim["channels"]:
        s = anim["samplers"][c["sampler"]]
        times = [t[0] for t in glbtrim.read_accessor(doc, binary, s["input"])]
        values = glbtrim.read_accessor(doc, binary, s["output"])
        slot = {"translation": 0, "rotation": 1}[c["target"]["path"]]
        local[c["target"]["node"]][slot] = hh.sample(times, values, time, c["target"]["path"])
    return skin(doc, binary, local)


def convert(folder, **kw):
    src = hh.Source(*hh.load_source(folder))
    return glbtrim.read_glb(hh.write(hh.convert(src, CLIPS)))


class CleanNames(unittest.TestCase):
    def test_every_source_clip_gets_its_clean_name(self):
        raw = ["1 Ilde", "2 Ilde", "3 Ilde", "4 Attack_1", "5 Attack_2", "6 Eat", "Arise",
               "Buck_1", "Buck_2", "Buck_3", "Death", "Fallen", "Falling", "Gallop", "Gallop_L",
               "Gallop_R", "Jump", "Jump_Land", "Jump_Run", "Run_Stop", "S.Jump", "Sleep", "Swim",
               "Swim_L", "Swim_R", "Walk", "Walk_L", "Walk_R"]
        names = [hh.clean_clip_name("Skeleton|" + r) for r in raw]
        self.assertEqual(sorted(names), sorted(hh.CLIPS))
        self.assertEqual(names[:6], ["Idle_1", "Idle_2", "Idle_3", "Attack_1", "Attack_2", "Eat"])
        self.assertIn("S_Jump", names)


class Convert(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.folder = self.tmp.name

    def tearDown(self):
        self.tmp.cleanup()

    def test_the_saddle_and_the_stirrups_are_dropped(self):
        build(self.folder)
        doc, _ = convert(self.folder)
        self.assertEqual([m["name"] for m in doc["materials"]], list(hh.MATERIALS))
        names = [n["name"] for n in doc["nodes"]]
        self.assertFalse([n for n in names if "Stirrup" in n])
        self.assertEqual(len(doc["skins"][0]["joints"]), len(JOINTS) - 2)

    def test_a_stirrup_the_body_is_weighted_to_is_refused(self):
        build(self.folder, stirrup_weight=0.25)
        with self.assertRaisesRegex(hh.ConvertError, "Stirrup"):
            convert(self.folder)

    def test_joints_are_rigid_and_the_skinned_result_is_the_source_in_metres(self):
        src_doc, src_bin = build(self.folder)
        doc, binary = convert(self.folder)
        joints = doc["skins"][0]["joints"]
        for j in joints:
            self.assertNotIn("scale", doc["nodes"][j])
        self.assertEqual({k for k in doc["nodes"][joints[0]] if k != "children"}, {"name"})
        for m in glbtrim.read_accessor(doc, binary, doc["skins"][0]["inverseBindMatrices"]):
            scale, _, _ = hh.similarity(hh.from_gltf(m), "IBM")
            self.assertAlmostEqual(scale, 1.0, places=5)
        for clip, name in ((0, "Idle_1"), (1, "Walk")):
            for time in (0.0, 0.3, 0.75):
                before = source_skinned(src_doc, src_bin, clip, time)
                after = output_skinned(doc, binary, name, time)
                points = [p for m in hh.MATERIALS for p in before[m]]
                moved = [p for m in hh.MATERIALS for p in after[m]]
                for (a, b), (c, d) in zip(itertools.combinations(points, 2), itertools.combinations(moved, 2)):
                    self.assertAlmostEqual(math.dist(c, d), hh.METRES * math.dist(a, b), places=4)

    def test_the_horse_faces_z_and_stands_on_the_ground(self):
        build(self.folder)
        doc, binary = convert(self.folder)
        pts = output_skinned(doc, binary, "Idle_1", 0.0)
        horse = pts["Horse"]
        head, pelvis = horse[1], horse[0]  # the vertices on the head and pelvis joints
        self.assertGreater(head[2] - pelvis[2], 1.0)
        self.assertAlmostEqual(head[0], pelvis[0], places=4)
        shoes = pts["Horseshoe"]
        self.assertAlmostEqual(min(p[1] for p in shoes + horse), 0.0, places=5)
        centre = [sum(p[i] for p in shoes) / len(shoes) for i in (0, 2)]
        self.assertAlmostEqual(centre[0], 0.0, places=5)
        self.assertAlmostEqual(centre[1], 0.0, places=5)

    def test_clips_keep_the_order_and_lose_the_scale_channels(self):
        build(self.folder)
        doc, _ = convert(self.folder)
        self.assertEqual([a["name"] for a in doc["animations"]], list(CLIPS))
        paths = {c["target"]["path"] for a in doc["animations"] for c in a["channels"]}
        self.assertEqual(paths, {"rotation", "translation"})

    def test_a_loop_missing_its_closing_frame_gets_its_first_key_one_frame_after_its_last(self):
        build(self.folder)
        with mock.patch.object(hh, "CLOSE_LOOPS", ("Walk",)):
            doc, binary = convert(self.folder)
        for anim in doc["animations"]:
            for c in anim["channels"]:
                s = anim["samplers"][c["sampler"]]
                times = [t[0] for t in glbtrim.read_accessor(doc, binary, s["input"])]
                values = glbtrim.read_accessor(doc, binary, s["output"])
                if anim["name"] == "Walk":
                    self.assertAlmostEqual(times[-1], 1.0 + 1 / 24, places=6)
                    self.assertAlmostEqual(times[-2], 1.0, places=6)
                    if c["target"]["path"] == "rotation":
                        # The same turn as the first key, on the side of the
                        # last one so it turns the short way.
                        dot = lambda a, b: sum(x * y for x, y in zip(a, b))
                        self.assertAlmostEqual(abs(dot(values[-1], values[0])), 1.0, places=6)
                        self.assertGreaterEqual(dot(values[-1], values[-2]), 0.0)
                    else:
                        for a, b in zip(values[-1], values[0]):
                            self.assertAlmostEqual(a, b, places=6)
                else:
                    self.assertAlmostEqual(times[-1], 1.0, places=6, msg="other clips untouched")

    def test_a_buffer_outside_the_folder_is_refused(self):
        build(self.folder, uri="../scene.bin")
        with self.assertRaisesRegex(hh.ConvertError, "plain .bin"):
            hh.load_source(self.folder)


@unittest.skipUnless(shutil.which("magick"), "needs ImageMagick 7")
class Textures(unittest.TestCase):
    def test_maps_are_shrunk_to_their_set_size_and_never_grown(self):
        with tempfile.TemporaryDirectory() as tmp:
            os.makedirs(os.path.join(tmp, "textures"))
            images = {i for files in hh.TEXTURES.values() for i, _, _ in files.values()}
            for image in images:
                subprocess.run(["magick", "-size", "64x64", "plasma:", os.path.join(tmp, "textures", image)], check=True)
            sizes = {"horse_body": 32, "horse_hair": 32, "horse_shoe": 128}
            small = {s: {f: (i, sizes[s], k) for f, (i, _, k) in files.items()} for s, files in hh.TEXTURES.items()}
            with mock.patch.object(hh, "TEXTURES", small), redirect_stdout(io.StringIO()):
                hh.write_textures(tmp, tmp)
            for s, files in small.items():
                for f in files:
                    out = subprocess.run(["magick", "identify", "-format", "%w", os.path.join(tmp, s, f)],
                                         capture_output=True, text=True, check=True).stdout
                    self.assertEqual(int(out), min(64, sizes[s]), f"{s}/{f}")
        self.assertEqual({s: {x[1] for x in f.values()} for s, f in hh.TEXTURES.items()},
                         {"horse_body": {2048}, "horse_hair": {2048}, "horse_shoe": {512}})


if __name__ == "__main__":
    unittest.main()
