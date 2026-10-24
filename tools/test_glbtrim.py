"""Tests of tools/glbtrim.py on tiny synthetic GLB files.

Standard library only: ``python3 -I -B -m unittest discover -s tools``.
"""

import io
import json
import math
import os
import struct
import tempfile
import unittest
from contextlib import redirect_stderr, redirect_stdout

import glbtrim


def floats(*values):
    return struct.pack(f"<{len(values)}f", *values)


def animal():
    """A GLB like the Quaternius exports, shrunk: one skinned triangle,
    two bones, a bare clip and its `AnimalArmature|` duplicate, and an
    unused clip, each with its own sampler data."""
    blob = bytearray()
    views, accessors = [], []

    def add(data, count, kind, component=5126, **extra):
        views.append({"buffer": 0, "byteOffset": len(blob), "byteLength": len(data)})
        blob.extend(data)
        blob.extend(b"\0" * (-len(blob) % 4))
        accessors.append(
            {"bufferView": len(views) - 1, "componentType": component, "count": count, "type": kind}
            | extra
        )
        return len(accessors) - 1

    position = add(floats(0, 0, 0, 1, 0, 0, 0, 1, 0), 3, "VEC3", min=[0, 0, 0], max=[1, 1, 0])
    indices = add(struct.pack("<3H", 0, 1, 2), 3, "SCALAR", component=5123)
    joints = add(bytes([0, 1, 0, 0] * 3), 3, "VEC4", component=5121)
    weights = add(floats(*([1, 0, 0, 0] * 3)), 3, "VEC4")
    bind = add(floats(*([1, 0, 0, 0, 0, 1, 0, 0, 0, 0, 1, 0, 0, 0, 0, 1] * 2)), 2, "MAT4")

    def clip(name, value):
        times = add(floats(0.0, value), 2, "SCALAR", min=[0.0], max=[value])
        turns = add(floats(0, 0, 0, 1, 0, value, 0, 1), 2, "VEC4")
        return {
            "name": name,
            "samplers": [{"input": times, "output": turns, "interpolation": "LINEAR"}],
            "channels": [{"sampler": 0, "target": {"node": 1, "path": "rotation"}}],
        }

    animations = [
        clip("Attack", 0.5),
        clip("Walk", 1.0),
        clip("Idle", 3.0),
        clip("AnimalArmature|Walk", 1.0),
        clip("AnimalArmature|Idle", 3.0),
    ]
    doc = {
        "asset": {"generator": "FBX2glTF v0.9.7", "version": "2.0"},
        "scene": 0,
        "scenes": [{"nodes": [0, 3]}],
        "nodes": [
            {"name": "Root", "children": [1]},
            {"name": "Body", "children": [2]},
            {"name": "Head"},
            {"name": "Cow", "mesh": 0, "skin": 0},
        ],
        "meshes": [
            {
                "name": "Cow",
                "primitives": [
                    {
                        "attributes": {"POSITION": position, "JOINTS_0": joints, "WEIGHTS_0": weights},
                        "indices": indices,
                        "material": 0,
                    }
                ],
            }
        ],
        "materials": [{"name": "Main"}],
        "samplers": [{}],
        "skins": [{"joints": [1, 2], "inverseBindMatrices": bind}],
        "animations": animations,
        "accessors": accessors,
        "bufferViews": views,
        "buffers": [{"byteLength": len(blob)}],
    }
    return glbtrim.write_glb(doc, bytes(blob))


def accessor_bytes(doc, blob, a):
    accessor = doc["accessors"][a]
    view = doc["bufferViews"][accessor["bufferView"]]
    return blob[view["byteOffset"] : view["byteOffset"] + view["byteLength"]]


def faceted(primitives, joints=None):
    """A GLB of one mesh with a primitive per triangle list in
    `primitives`, flat-shaded: every triangle has its own three vertices
    and its face normal, and an index list. `joints`, if given, holds one
    joint index per primitive (each vertex fully weighted to it)."""
    blob = bytearray()
    views, accessors, prims = [], [], []

    def add(data, count, kind, component=5126):
        views.append({"buffer": 0, "byteOffset": len(blob), "byteLength": len(data)})
        blob.extend(data)
        blob.extend(b"\0" * (-len(blob) % 4))
        accessors.append(
            {"bufferView": len(views) - 1, "componentType": component, "count": count, "type": kind}
        )
        return len(accessors) - 1

    for k, triangles in enumerate(primitives):
        points, normals = [], []
        for a, b, c in triangles:
            n = glbtrim.unit(glbtrim.cross(glbtrim.sub(b, a), glbtrim.sub(c, a)))
            points += [a, b, c]
            normals += [n, n, n]
        count = len(points)
        attributes = {
            "POSITION": add(floats(*[x for p in points for x in p]), count, "VEC3"),
            "NORMAL": add(floats(*[x for n in normals for x in n]), count, "VEC3"),
        }
        if joints is not None:
            attributes["JOINTS_0"] = add(bytes([joints[k], 0, 0, 0] * count), count, "VEC4", 5121)
            attributes["WEIGHTS_0"] = add(floats(*([1, 0, 0, 0] * count)), count, "VEC4")
        indices = add(struct.pack(f"<{count}H", *range(count)), count, "SCALAR", 5123)
        prims.append({"attributes": attributes, "indices": indices})
    doc = {
        "asset": {"version": "2.0"},
        "scenes": [{"nodes": [0]}],
        "nodes": [{"mesh": 0}],
        "meshes": [{"primitives": prims}],
        "animations": [],
        "accessors": accessors,
        "bufferViews": views,
        "buffers": [{"byteLength": len(blob)}],
    }
    return glbtrim.write_glb(doc, bytes(blob))


def vertices(data):
    """Every vertex of the mesh as (position, normal), in order."""
    doc, blob = glbtrim.read_glb(data)
    out = []
    for prim in doc["meshes"][0]["primitives"]:
        points = glbtrim.read_accessor(doc, blob, prim["attributes"]["POSITION"])
        normals = glbtrim.read_accessor(doc, blob, prim["attributes"]["NORMAL"])
        out += list(zip(points, normals))
    return out


def sphere(segments=8, rings=6):
    """A low-poly UV sphere of unit radius, as triangles facing out."""
    def at(i, j):
        # The seam's and the poles' points exactly equal (welding is by
        # exact position).
        if j in (0, rings):
            return (0.0, 1.0 if j == 0 else -1.0, 0.0)
        theta, phi = math.pi * j / rings, 2 * math.pi * (i % segments) / segments
        return (math.sin(theta) * math.cos(phi), math.cos(theta), math.sin(theta) * math.sin(phi))

    triangles = []
    for j in range(rings):
        for i in range(segments):
            a, b, c, d = at(i, j), at(i + 1, j), at(i + 1, j + 1), at(i, j + 1)
            if j > 0:
                triangles.append((a, b, d))
            if j < rings - 1:
                triangles.append((b, c, d))
    return triangles


def cube():
    """A unit cube as 12 triangles facing out."""
    triangles = []
    for axis in range(3):
        for side in (-1.0, 1.0):
            def corner(u, v):
                p = [0.0, 0.0, 0.0]
                p[axis], p[(axis + 1) % 3], p[(axis + 2) % 3] = side, u, v
                return tuple(p)

            a, b, c, d = corner(-1, -1), corner(1, -1), corner(1, 1), corner(-1, 1)
            quad = [(a, b, c), (a, c, d)]
            if side < 0:
                quad = [(x, z, y) for x, y, z in quad]
            triangles += quad
    return triangles


class TrimTest(unittest.TestCase):
    def test_keeps_only_the_named_clips_in_order_and_their_data(self):
        source = animal()
        out = glbtrim.trim(source, ["Idle", "Walk"])
        doc, blob = glbtrim.read_glb(out)
        self.assertEqual([a["name"] for a in doc["animations"]], ["Idle", "Walk"])
        # 5 scene accessors and 2 per kept clip; the other 3 clips' data is gone.
        self.assertEqual(len(doc["accessors"]), 5 + 2 * 2)
        self.assertEqual(len(doc["bufferViews"]), len(doc["accessors"]))
        self.assertLess(len(out), len(source))
        self.assertEqual(doc["buffers"], [{"byteLength": len(blob)}])
        self.assertNotIn("samplers", doc, "texture samplers without textures")
        self.assertIn(glbtrim.TRIMMER, doc["asset"]["generator"])
        # Every kept clip still reads its own key frames.
        old, old_blob = glbtrim.read_glb(source)
        for new_clip in doc["animations"]:
            old_clip = next(a for a in old["animations"] if a["name"] == new_clip["name"])
            for new_s, old_s in zip(new_clip["samplers"], old_clip["samplers"]):
                for key in ("input", "output"):
                    self.assertEqual(
                        accessor_bytes(doc, blob, new_s[key]),
                        accessor_bytes(old, old_blob, old_s[key]),
                    )
        # The mesh and skin still read their data, at 4-byte aligned views.
        prim = doc["meshes"][0]["primitives"][0]
        old_prim = old["meshes"][0]["primitives"][0]
        for key in ("POSITION", "JOINTS_0", "WEIGHTS_0"):
            self.assertEqual(
                accessor_bytes(doc, blob, prim["attributes"][key]),
                accessor_bytes(old, old_blob, old_prim["attributes"][key]),
            )
        self.assertEqual(
            accessor_bytes(doc, blob, prim["indices"]), accessor_bytes(old, old_blob, old_prim["indices"])
        )
        self.assertEqual(
            accessor_bytes(doc, blob, doc["skins"][0]["inverseBindMatrices"]),
            accessor_bytes(old, old_blob, old["skins"][0]["inverseBindMatrices"]),
        )
        self.assertTrue(all(v["byteOffset"] % 4 == 0 for v in doc["bufferViews"]))
        self.assertEqual(doc["nodes"], old["nodes"])
        self.assertEqual(doc["materials"], old["materials"])

    def test_is_deterministic_and_idempotent(self):
        once = glbtrim.trim(animal(), ["Idle", "Walk"])
        self.assertEqual(once, glbtrim.trim(animal(), ["Idle", "Walk"]))
        twice = glbtrim.trim(once, ["Idle", "Walk"])
        self.assertEqual(twice, once)
        self.assertEqual(len(once) % 4, 0)

    def test_refuses_what_it_cannot_trim_safely(self):
        source = animal()
        bad = {
            "missing clip": (source, ["Gallop"]),
            "clip twice": (source, ["Idle", "Idle"]),
            "bad magic": (b"glTX" + source[4:], ["Idle"]),
            "truncated": (source[:-8], ["Idle"]),
            "not GLB": (b"{}", ["Idle"]),
        }
        doc, blob = glbtrim.read_glb(source)
        external = json.loads(json.dumps(doc))
        external["buffers"][0]["uri"] = "../../secret.bin"
        bad["external buffer"] = (glbtrim.write_glb(external, blob), ["Idle"])
        out_of_range = json.loads(json.dumps(doc))
        out_of_range["animations"][2]["samplers"][0]["output"] = 999
        bad["accessor out of range"] = (glbtrim.write_glb(out_of_range, blob), ["Idle"])
        past_end = json.loads(json.dumps(doc))
        past_end["bufferViews"][0]["byteLength"] = 1 << 20
        bad["view past the buffer"] = (glbtrim.write_glb(past_end, blob), ["Idle"])
        for name, (data, keep) in bad.items():
            with self.subTest(name), self.assertRaises(glbtrim.TrimError):
                glbtrim.trim(data, keep)

    def test_the_command_line_writes_the_file_and_reports_sizes(self):
        with tempfile.TemporaryDirectory() as tmp:
            source = os.path.join(tmp, "cow_a.glb")
            with open(source, "wb") as f:
                f.write(animal())
            target = os.path.join(tmp, "out", "cow.glb")
            out = io.StringIO()
            with redirect_stdout(out):
                self.assertEqual(glbtrim.main([source, target, "--keep", "Idle", "Walk"]), 0)
            self.assertIn("cow.glb", out.getvalue())
            self.assertIn("clips Idle, Walk (from 5)", out.getvalue())
            with open(target, "rb") as f:
                _, triangles, joints, clips = glbtrim.summary(f.read())
            self.assertEqual((triangles, joints, clips), (1, 2, ["Idle", "Walk"]))
            err = io.StringIO()
            with redirect_stderr(err):
                self.assertEqual(glbtrim.main([source, target, "--keep", "Death"]), 1)
            self.assertIn("no animation named Death", err.getvalue())

    def test_smooth_normals_are_continuous_under_the_angle_and_split_over_it(self):
        def close(a, b):
            return all(abs(x - y) < 1e-5 for x, y in zip(a, b))

        # A low-poly sphere, split over two primitives (two materials):
        # its faces meet at 45 degrees or less, so it turns smooth, the
        # same normal at every copy of a point, pointing outward.
        triangles = sphere()
        half = len(triangles) // 2
        source = faceted([triangles[:half], triangles[half:]])
        out = glbtrim.trim(source, [])
        before, after = vertices(source), vertices(out)
        self.assertEqual(len(after), len(before), "no vertex merged")
        by_point = {}
        for point, normal in after:
            by_point.setdefault(point, []).append(normal)
            self.assertGreater(glbtrim.dot(normal, glbtrim.unit(point)), 0.97, point)
        self.assertTrue(all(close(n, ns[0]) for ns in by_point.values() for n in ns))
        self.assertTrue(
            any(not close(b[1], a[1]) for a, b in zip(before, after)), "the facets changed"
        )
        self.assertEqual([p for p, _ in after], [p for p, _ in before], "positions kept")

        # A cube's faces meet at 90 degrees: every corner keeps its face's
        # normal, shared only by the two triangles of the same face.
        source = faceted([cube()])
        self.assertEqual(vertices(glbtrim.trim(source, [])), vertices(source))
        # Over a wider angle the corners round off.
        rounded = vertices(glbtrim.trim(source, [], smooth_angle=100.0))
        third = 1 / math.sqrt(3)
        self.assertTrue(all(close([abs(x) for x in n], [third] * 3) for _, n in rounded))

    def test_smoothing_never_welds_across_different_skinning(self):
        # Two triangles folded 20 degrees along a shared edge.
        fold = math.radians(20)
        a, b = (0.0, 0.0, 0.0), (1.0, 0.0, 0.0)
        left = (0.5, 1.0, 0.0)
        right = (0.5, -math.cos(fold), math.sin(fold))
        triangles = [[(a, b, left)], [(b, a, right)]]
        same = vertices(glbtrim.trim(faceted(triangles, joints=[1, 1]), []))
        apart = vertices(glbtrim.trim(faceted(triangles, joints=[1, 2]), []))
        flat = vertices(faceted(triangles, joints=[1, 2]))
        # Same bone: the shared edge's copies agree.
        self.assertEqual(same[0][1], same[4][1])
        # Different bones: each keeps its own face normal.
        self.assertEqual(apart, flat)

    def test_smoothing_is_deterministic_and_can_be_turned_off(self):
        source = faceted([sphere(12, 8)])
        self.assertEqual(glbtrim.trim(source, []), glbtrim.trim(source, []))
        once = glbtrim.trim(source, [])
        self.assertEqual(glbtrim.trim(once, []), once, "smoothing smooth normals changes nothing")
        self.assertEqual(vertices(glbtrim.trim(source, [], smooth_angle=0)), vertices(source))
        with self.assertRaises(glbtrim.TrimError):
            glbtrim.trim(source, [], smooth_angle=200)


if __name__ == "__main__":
    unittest.main()
