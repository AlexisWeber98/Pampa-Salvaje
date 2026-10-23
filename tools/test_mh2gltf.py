"""Tests of tools/mh2gltf.py.

Standard library and numpy: ``python3 -I -B -m unittest discover -s tools``.
The unit tests use small synthetic inputs; the conversion tests read the
installed MakeHuman data (``$MAKEHUMAN_DATA`` or the default folder) and
are skipped without it.
"""

import io
import json
import os
import struct
import tempfile
import unittest
import zlib
from contextlib import redirect_stderr, redirect_stdout

import mh2gltf

try:
    import numpy as np
except ImportError:  # pragma: no cover
    np = None

HERE = os.path.dirname(os.path.abspath(__file__))
CHARACTERS = os.path.join(HERE, "..", "assets", "characters")
DATA = os.path.realpath(os.path.expanduser(os.environ.get("MAKEHUMAN_DATA") or mh2gltf.DEFAULT_DATA))
HAVE_DATA = np is not None and os.path.isfile(os.path.join(DATA, "targets.npz"))


def png_with_filters(img, kinds):
    """A PNG of img whose rows use the given filter types, encoded by hand
    (independently of png_encode)."""
    h, w, c = img.shape
    out = bytearray()
    prev = [0] * (w * c)
    for y in range(h):
        row = [int(v) for v in img[y].reshape(-1)]
        kind = kinds[y % len(kinds)]
        line = bytearray([kind])
        for x in range(w * c):
            a = row[x - c] if x >= c else 0
            b = prev[x]
            cc = prev[x - c] if x >= c else 0
            if kind == 0:
                pred = 0
            elif kind == 1:
                pred = a
            elif kind == 2:
                pred = b
            elif kind == 3:
                pred = (a + b) // 2
            else:
                p = a + b - cc
                pa, pb, pc = abs(p - a), abs(p - b), abs(p - cc)
                pred = a if pa <= pb and pa <= pc else (b if pb <= pc else cc)
            line.append((row[x] - pred) & 255)
        out += line
        prev = row

    def chunk(kind, body):
        return struct.pack(">I", len(body)) + kind + body + struct.pack(">I", zlib.crc32(kind + body) & 0xFFFFFFFF)

    colour = {1: 0, 2: 4, 3: 2, 4: 6}[c]
    return (
        b"\x89PNG\r\n\x1a\n"
        + chunk(b"IHDR", struct.pack(">IIBBBBB", w, h, 8, colour, 0, 0, 0))
        + chunk(b"IDAT", zlib.compress(bytes(out)))
        + chunk(b"IEND", b"")
    )


@unittest.skipIf(np is None, "numpy is not installed")
class Units(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)

    def write(self, name, data):
        path = os.path.join(self.tmp.name, name)
        with open(path, "wb" if isinstance(data, bytes) else "w") as f:
            f.write(data)
        return path

    def test_png_decoding_undoes_every_filter(self):
        rng = np.random.default_rng(7)
        for channels in (1, 3, 4):
            img = rng.integers(0, 256, (9, 13, channels), dtype=np.uint8)
            for kinds in ([0, 1, 2], [3, 4], [4, 0, 3, 1, 2]):
                path = self.write("f.png", png_with_filters(img, kinds))
                np.testing.assert_array_equal(mh2gltf.png_decode(path), img, f"{channels} {kinds}")

    def test_png_encoding_round_trips(self):
        rng = np.random.default_rng(3)
        img = rng.integers(0, 256, (17, 11, 4), dtype=np.uint8)
        path = self.write("e.png", mh2gltf.png_encode(img))
        np.testing.assert_array_equal(mh2gltf.png_decode(path), img)
        self.assertEqual(mh2gltf.png_encode(img), mh2gltf.png_encode(img))

    def test_truncated_and_non_png_files_are_refused(self):
        img = np.zeros((8, 8, 3), np.uint8)
        data = mh2gltf.png_encode(img)
        with self.assertRaises(mh2gltf.ConvertError):
            mh2gltf.png_decode(self.write("t.png", data[:40]))
        with self.assertRaises(mh2gltf.ConvertError):
            mh2gltf.png_decode(self.write("n.png", b"GIF89a" + b"\0" * 32))

    def test_downscaling_keeps_colour_out_of_transparent_texels(self):
        img = np.zeros((4, 4, 4), np.uint8)
        img[..., 0] = 200  # red where opaque ...
        img[..., 3] = 0
        img[0, 0, 3] = 255
        img[1, 1, :] = (0, 0, 255, 0)  # ... blue only where fully transparent
        out = mh2gltf.downscale(img, 2)
        self.assertEqual(out.shape, (2, 2, 4))
        self.assertEqual(tuple(out[0, 0, :3]), (200, 0, 0))
        self.assertEqual(out[0, 0, 3], 64)
        self.assertIs(mh2gltf.downscale(img, 4), img)

    def test_macro_values_follow_makehuman(self):
        settings = dict.fromkeys(
            ["gender", "age", "muscle", "weight", "height", "bodyproportions", "breastsize", "breastfirmness"], 0.5
        )
        settings.update(african=0.0, asian=0.0, caucasian=2.0)
        v = mh2gltf.macro_values(settings)
        self.assertEqual((v["young"], v["old"], v["child"], v["baby"]), (1.0, 0.0, 0.0, 0.0))
        self.assertEqual((v["averageweight"], v["minweight"], v["maxweight"]), (1.0, 0.0, 0.0))
        self.assertEqual(v["caucasian"], 1.0)
        settings.update(age=0.769231, weight=0.75, gender=1.0)
        v = mh2gltf.macro_values(settings)
        self.assertAlmostEqual(v["old"], 0.538462, places=5)
        self.assertAlmostEqual(v["young"] + v["old"], 1.0)
        self.assertAlmostEqual(v["maxweight"], 0.5)
        self.assertAlmostEqual(v["averageweight"], 0.5)
        self.assertEqual((v["male"], v["female"]), (1.0, 0.0))

    def test_influences_are_the_strongest_four_summing_to_one(self):
        w = np.array([[0.1, 0.2, 0.3, 0.15, 0.25, 0.0], [0, 0, 0, 0, 0, 0], [0, 0, 1.0, 0, 0, 0]])
        joints, weights = mh2gltf.top_influences(w)
        self.assertEqual(joints.dtype, np.uint8)
        self.assertEqual(sorted(joints[0].tolist()), [1, 2, 3, 4])
        np.testing.assert_allclose(weights.sum(axis=1), 1.0, atol=1e-6)
        self.assertEqual(weights[1].tolist(), [1.0, 0.0, 0.0, 0.0])
        self.assertEqual(joints[2, 0], 2)

    def test_quaternions_round_trip_rotation_matrices(self):
        rng = np.random.default_rng(5)
        for _ in range(20):
            m, _ = np.linalg.qr(rng.normal(size=(3, 3)))
            if np.linalg.det(m) < 0:
                m[:, 0] = -m[:, 0]
            q = mh2gltf.quaternion(m)
            np.testing.assert_allclose(mh2gltf.rotation_matrix(q), m, atol=1e-6)

    def test_bone_matrix_is_orthonormal_with_y_along_the_bone(self):
        m = mh2gltf.bone_matrix(np.zeros(3), np.array([0.0, 2.0, 0.0]), np.array([0.0, 1.0, 0.0]))
        np.testing.assert_allclose(m[:3, :3] @ m[:3, :3].T, np.identity(3), atol=1e-9)
        np.testing.assert_allclose(m[:3, 1], [0, 1, 0])

    def test_mhm_lines_are_parsed_and_bad_ones_refused(self):
        path = self.write(
            "c.mhm",
            "# comment\nname pedro\nmodifier macrodetails/Gender 1.0\n"
            "clothes fedora 566dbd52-71d1-4d76-a799-0474a5a384db\ncamera 0 0\nwhatever x\n",
        )
        c = mh2gltf.read_mhm(path)
        self.assertEqual(c.name, "pedro")
        self.assertEqual(c.modifiers, {"macrodetails/Gender": 1.0})
        self.assertEqual(c.proxies, [("clothes", "fedora", "566dbd52-71d1-4d76-a799-0474a5a384db")])
        self.assertEqual(c.ignored, ["whatever"])
        for bad in ("modifier a/b nan\n", "hair short02 not-a-uuid!\n", "name ../escape\n"):
            with self.assertRaises(mh2gltf.ConvertError, msg=bad):
                mh2gltf.read_mhm(self.write("b.mhm", bad))

    def test_paths_stay_inside_the_data_or_the_mhm_folder(self):
        data = os.path.join(self.tmp.name, "data")
        local = os.path.join(self.tmp.name, "char")
        os.makedirs(os.path.join(data, "skins"))
        os.makedirs(local)
        self.write("secret.txt", "x")
        self.write("data/skins/a.mhmat", "x")
        self.write("char/b.mhmat", "x")
        files = mh2gltf.Files(data, local)
        self.assertTrue(files.find("skins/a.mhmat", []).endswith("a.mhmat"))
        self.assertTrue(files.find("data/skins/a.mhmat", []).endswith("a.mhmat"))
        self.assertTrue(files.find("b.mhmat", [local]).endswith("b.mhmat"))
        for bad in ("../secret.txt", "/etc/passwd", "skins\\a.mhmat", "skins/../../secret.txt"):
            with self.assertRaises(mh2gltf.ConvertError, msg=bad):
                files.find(bad, [local])

    def test_check_flags_bad_skin_weights(self):
        # One triangle, one joint, weights summing to 0.5.
        blob = bytearray()
        views, accessors = [], []

        def add(raw, component, kind, count):
            views.append({"buffer": 0, "byteOffset": len(blob), "byteLength": len(raw)})
            blob.extend(raw)
            accessors.append({"bufferView": len(views) - 1, "componentType": component, "count": count, "type": kind})
            return len(accessors) - 1

        pos = add(struct.pack("<9f", 0, 0, 0, 1, 0, 0, 0, 1, 0), 5126, "VEC3", 3)
        idx = add(struct.pack("<3H", 0, 1, 2) + b"\0\0", 5123, "SCALAR", 3)
        joints = add(bytes(12), 5121, "VEC4", 3)
        weights = add(struct.pack("<12f", *([0.5, 0, 0, 0] * 3)), 5126, "VEC4", 3)
        ibm = add(struct.pack("<16f", 1, 0, 0, 0, 0, 1, 0, 0, 0, 0, 1, 0, 0, 0, 0, 1), 5126, "MAT4", 1)
        g = {
            "asset": {"version": "2.0"},
            "nodes": [{"mesh": 0, "skin": 0}, {"name": "root"}],
            "skins": [{"joints": [1], "inverseBindMatrices": ibm}],
            "meshes": [{"name": "m", "primitives": [{"attributes": {"POSITION": pos, "JOINTS_0": joints, "WEIGHTS_0": weights}, "indices": idx}]}],
            "accessors": accessors,
            "bufferViews": views,
            "buffers": [{"uri": "m.bin", "byteLength": len(blob)}],
        }
        self.write("m.bin", bytes(blob))
        path = self.write("m.gltf", json.dumps(g))
        out = io.StringIO()
        with redirect_stdout(out):
            self.assertEqual(mh2gltf.check(path), 1)
        self.assertIn("do not sum to 1", out.getvalue())


def read_accessor(g, blob, i):
    a = g["accessors"][i]
    v = g["bufferViews"][a["bufferView"]]
    width = {"SCALAR": 1, "VEC2": 2, "VEC3": 3, "VEC4": 4, "MAT4": 16}[a["type"]]
    dtype = {5121: np.uint8, 5123: np.uint16, 5125: np.uint32, 5126: np.float32}[a["componentType"]]
    return np.frombuffer(blob, dtype, a["count"] * width, v.get("byteOffset", 0) + a.get("byteOffset", 0)).reshape(a["count"], width)


@unittest.skipUnless(HAVE_DATA, f"MakeHuman data not found at {DATA}")
class Conversion(unittest.TestCase):
    """Converts a shipped character with the installed MakeHuman data."""

    @classmethod
    def setUpClass(cls):
        cls.tmp = tempfile.TemporaryDirectory()
        cls.out = cls.tmp.name
        mhm = os.path.join(CHARACTERS, "farach", "farach.mhm")
        log = io.StringIO()
        with redirect_stdout(log), redirect_stderr(log):
            code = mh2gltf.main([mhm, "-o", cls.out, "--data", DATA, "--max-texture", "64", "--max-small-texture", "32", "--no-normal-maps"])
        if code != 0:
            raise AssertionError(log.getvalue())
        with open(os.path.join(cls.out, "model.gltf"), encoding="utf-8") as f:
            cls.g = json.load(f)
        with open(os.path.join(cls.out, "model.bin"), "rb") as f:
            cls.blob = f.read()

    @classmethod
    def tearDownClass(cls):
        cls.tmp.cleanup()

    def prims(self):
        return self.g["meshes"][0]["primitives"]

    def test_the_written_file_passes_its_check(self):
        with redirect_stdout(io.StringIO()):
            self.assertEqual(mh2gltf.check(os.path.join(self.out, "model.gltf")), 0)

    def test_skin_weights_sum_to_one_on_joints_in_range(self):
        joints = len(self.g["skins"][0]["joints"])
        for p in self.prims():
            j = read_accessor(self.g, self.blob, p["attributes"]["JOINTS_0"])
            w = read_accessor(self.g, self.blob, p["attributes"]["WEIGHTS_0"])
            self.assertLess(int(j.max()), joints)
            self.assertGreaterEqual(float(w.min()), 0.0)
            np.testing.assert_allclose(w.sum(axis=1), 1.0, atol=1e-5)

    def test_inverse_binds_undo_the_joints_rest_transforms(self):
        nodes = self.g["nodes"]
        skin = self.g["skins"][0]
        ibm = read_accessor(self.g, self.blob, skin["inverseBindMatrices"]).reshape(-1, 4, 4)
        parent = {c: i for i, n in enumerate(nodes) for c in n.get("children", [])}

        def world(i):
            n = nodes[i]
            m = np.identity(4)
            m[:3, :3] = mh2gltf.rotation_matrix(n.get("rotation", [0, 0, 0, 1]))
            m[:3, 3] = n.get("translation", [0, 0, 0])
            return world(parent[i]) @ m if i in parent else m

        for k, j in enumerate(skin["joints"]):
            inverse = ibm[k].T.astype(np.float64)  # stored column-major
            self.assertGreater(abs(np.linalg.det(inverse)), 1e-6)
            np.testing.assert_allclose(world(j) @ inverse, np.identity(4), atol=1e-4)

    def test_triangle_budget(self):
        tris = sum(self.g["accessors"][p["indices"]]["count"] // 3 for p in self.prims())
        self.assertGreaterEqual(tris, 6000)
        self.assertLessEqual(tris, 15000)

    def test_a_plausible_height_standing_on_the_ground(self):
        lows, highs = [], []
        for p in self.prims():
            a = self.g["accessors"][p["attributes"]["POSITION"]]
            lows.append(a["min"][1])
            highs.append(a["max"][1])
        self.assertAlmostEqual(min(lows), 0.0, places=4)
        body = self.g["accessors"][self.prims()[0]["attributes"]["POSITION"]]
        self.assertTrue(1.6 <= body["max"][1] <= 1.85, body["max"][1])

    def test_the_skeleton_is_the_game_engine_rig(self):
        names = {self.g["nodes"][j]["name"] for j in self.g["skins"][0]["joints"]}
        self.assertEqual(len(names), 53)
        self.assertTrue({"Root", "pelvis", "spine_02", "head", "upperarm_l", "hand_r"} <= names)
