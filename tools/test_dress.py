"""Tests of tools/dress.py.

Standard library and numpy: ``python3 -I -B -m unittest discover -s tools -p test_dress.py``.
They build a small glTF body in a temporary folder; no MakeHuman data or
Blender needed.
"""

import io
import json
import os
import tempfile
import unittest
from contextlib import redirect_stderr

import dress

try:
    import numpy as np
except ImportError:  # pragma: no cover
    np = None


class Rig:
    """A stand-in skeleton: Root, pelvis under it, spine_01 under that."""

    order = ["Root", "pelvis", "spine_01"]
    bones = {"Root": {"parent": None}, "pelvis": {"parent": "Root"}, "spine_01": {"parent": "pelvis"}}


@unittest.skipIf(np is None, "numpy is required")
class Body(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)

    def body(self, **change):
        """A glTF body: a quad of two triangles in two primitives (two
        materials), its shared edge's vertices repeated with other UVs, in
        meters, skinned to spine_01 and pelvis (listed in that order)."""
        pos = np.array([[0, 0, 0], [1, 0, 0], [0, 1, 0], [1, 0, 0], [0, 1, 0], [1, 1, 0]], np.float32) * 0.1
        uv = np.array([[0, 0], [1, 0], [0, 1], [0.5, 0], [0.5, 1], [1, 1]], np.float32)
        joints = np.array([[0, 1, 0, 0]] * 6, np.uint8)
        weights = np.array([[0.75, 0.25, 0, 0]] * 6, np.float32)
        first, second = np.array([0, 1, 2], np.uint16), np.array([3, 5, 4], np.uint16)
        blob, views, accessors = b"", [], []
        for arr, kind, comp in ((pos, "VEC3", 5126), (uv, "VEC2", 5126), (joints, "VEC4", 5121),
                                (weights, "VEC4", 5126), (first, "SCALAR", 5123), (second, "SCALAR", 5123)):
            blob += b"\0" * (-len(blob) % 4)
            views.append({"buffer": 0, "byteOffset": len(blob), "byteLength": arr.nbytes})
            accessors.append({"bufferView": len(views) - 1, "componentType": comp, "count": len(arr), "type": kind})
            blob += arr.tobytes()
        attrs = {"POSITION": 0, "TEXCOORD_0": 1, "JOINTS_0": 2, "WEIGHTS_0": 3}
        doc = {
            "asset": {"version": "2.0"},
            "nodes": [{"name": "body", "mesh": 0, "skin": 0},
                      {"name": "Root", "translation": [0, 0.5, 0], "children": [2]},
                      {"name": "pelvis", "translation": [0, 0.25, 0], "children": [3]},
                      {"name": "spine_01", "translation": [0, 0.1, 0]}],
            "meshes": [{"primitives": [{"attributes": attrs, "indices": 4, "material": 0},
                                       {"attributes": attrs, "indices": 5, "material": 1}]}],
            "skins": [{"joints": [3, 2, 1]}],
            "materials": [{"name": "m_body"}, {"name": "m_head"}],
            "accessors": accessors,
            "bufferViews": views,
            "buffers": [{"uri": "body.bin", "byteLength": len(blob)}],
        }
        doc.update(change)
        folder = os.path.join(self.tmp.name, "body")
        os.makedirs(folder, exist_ok=True)
        for where in (folder, self.tmp.name):  # and one outside its folder
            with open(os.path.join(where, "body.bin"), "wb") as f:
                f.write(blob)
        path = os.path.join(folder, "body.gltf")
        with open(path, "w") as f:
            json.dump(doc, f)
        return dress.Gltf(path)

    def test_the_body_is_welded_in_decimetres_with_its_weights_in_skeleton_order(self):
        skin, rest, material = dress.read_body(self.body(), Rig())
        # One vertex per place, in decimetres; the UV corners kept apart.
        np.testing.assert_allclose(skin.coords, [[0, 0, 0], [0, 1, 0], [1, 0, 0], [1, 1, 0]], atol=1e-6)
        self.assertEqual(skin.triangles, [((0, 0), (2, 1), (1, 2)), ((2, 3), (3, 5), (1, 4))])
        self.assertEqual(skin.all_triangles, skin.triangles)
        np.testing.assert_allclose(skin.texco[[0, 3, 5]], [[0, 1], [0.5, 1], [1, 0]])  # V flipped
        # Skin joint 0 is spine_01, 1 pelvis: in rig order (Root, pelvis, spine_01).
        np.testing.assert_allclose(skin.weights, [[0, 0.25, 0.75]] * 4)
        # Rest matrices from the nodes, down their parents, in decimetres.
        np.testing.assert_allclose(rest["spine_01"][:3, 3], [0, 8.5, 0])
        self.assertEqual(sorted(set(material.values())), [0, 1])
        self.assertEqual(material[skin.triangles[1]], 1)

    def test_a_bad_body_is_refused(self):
        def fewer_vertices(d):
            # The second triangle's 3, 5, 4 past three vertices.
            for a in range(4):
                d["accessors"][a]["count"] = 3

        bad = {
            "an index past the vertices": fewer_vertices,
            "a buffer outside its folder": lambda d: d["buffers"][0].__setitem__("uri", "../body.bin"),
            "a bone missing": lambda d: d["nodes"][3].__setitem__("name", "chest"),
            "a joint off the skeleton": lambda d: d["skins"][0].__setitem__("joints", [0, 2, 1]),
            "an accessor past its view": lambda d: d["bufferViews"][0].__setitem__("byteLength", 8),
        }
        for what, spoil in bad.items():
            with self.subTest(what):
                g = self.body()
                spoil(g.doc)
                with self.assertRaises(dress.M.ConvertError):
                    dress.read_body(g, Rig())

    def test_the_command_reports_a_bad_input_in_one_line(self):
        path = os.path.join(self.tmp.name, "nothing.gltf")
        with open(path, "w") as f:
            f.write("[]")
        mhm = os.path.join(self.tmp.name, "a.mhm")
        with open(mhm, "w") as f:
            f.write("version v1.1.1\nskeleton game_engine.mhskel\n")
        err = io.StringIO()
        with redirect_stderr(err):
            code = dress.main([path, mhm, "-o", os.path.join(self.tmp.name, "out"), "--data", self.tmp.name])
        self.assertEqual(code, 1)
        self.assertTrue(err.getvalue().startswith("dress: ") and err.getvalue().count("\n") == 1, err.getvalue())


if __name__ == "__main__":
    unittest.main()
