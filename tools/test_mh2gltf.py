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

    def test_obj_faces_keep_quads_repeat_a_triangles_first_vertex_and_fan_the_rest(self):
        path = self.write(
            "m.obj",
            "v 0 0 0\nv 1 0 0\nv 1 1 0\nv 0 1 0\nv 0.5 2 0\n"
            "vt 0 0\nvt 1 0\nvt 1 1\nvt 0 1\nvt 0.5 1\n"
            "f 1/1 2/2 3/3 4/4\nf 1/1 2/2 3/3\nf 1/1 2/2 3/3 5/5 4/4\n",
        )
        coord, texco, fvert, fuvs = mh2gltf.read_obj(path)
        self.assertEqual(coord.shape, (5, 3))
        self.assertEqual(texco.shape, (5, 2))
        self.assertEqual(fvert.tolist(), [[0, 1, 2, 3], [0, 1, 2, 0], [0, 1, 2, 0], [0, 2, 4, 0], [0, 4, 3, 0]])
        self.assertEqual(fuvs.tolist(), fvert.tolist())
        # No UVs at all: every corner on one (0, 0) UV.
        coord, texco, fvert, fuvs = mh2gltf.read_obj(self.write("n.obj", "v 0 0 0\nv 1 0 0\nv 0 1 0\nf 1 2 3\n"))
        self.assertEqual((texco.tolist(), fuvs.tolist()), ([[0.0, 0.0]], [[0, 0, 0, 0]]))
        for bad in ("v 0 0 0\nf 1 2 9\n", "v 0 nan 0\nf 1 1 1\n", "v 0 0\n"):
            with self.assertRaises(mh2gltf.ConvertError, msg=bad):
                mh2gltf.read_obj(self.write("b.obj", bad))

    def mhclo(self, folder, uuid="11111111-2222-3333-4444-555555555555", extra=""):
        os.makedirs(os.path.join(self.tmp.name, folder), exist_ok=True)
        self.write(f"{folder}/cap.obj", "v 0 0 0\nv 1 0 0\nv 0 1 0\nvt 0 0\nf 1/1 2/1 3/1\n")
        return self.write(
            f"{folder}/cap.mhclo",
            f"# author someone\nuuid {uuid}\nname Cap\ntag Hat\nobj_file cap.obj\n"
            "x_scale 0 1 2.0\nz_depth 61\nmaterial cap.mhmat\n" + extra
            + "delete_verts\n2 - 4 7\nverts 0\n5\n 1 2 3 0.5 0.25 0.25 0.1 0.2 0.3\n 6 7 8 1.0 0.0 0.0\n",
        )

    def test_a_text_proxy_reads_its_mhclo_and_obj(self):
        p = mh2gltf.Proxy(self.mhclo("data/clothes/cap"), "clothes", 10)
        self.assertEqual((p.name, p.uuid, p.z_depth, p.material_file), ("Cap", "11111111-2222-3333-4444-555555555555", 61, "cap.mhmat"))
        self.assertEqual(p.refs.tolist(), [[5, 0, 0], [1, 2, 3], [6, 7, 8]])
        self.assertEqual(p.weights.tolist(), [[1, 0, 0], [0.5, 0.25, 0.25], [1, 0, 0]])
        self.assertEqual(p.offsets.tolist(), [[0, 0, 0], [0.1, 0.2, 0.3], [0, 0, 0]])
        self.assertEqual(p.scale, [(0, 1, 2.0), None, None])
        self.assertEqual(np.flatnonzero(p.delete).tolist(), [2, 3, 4, 7])
        self.assertEqual(p.fvert.tolist(), [[0, 1, 2, 0]])
        coords = np.arange(30, dtype=float).reshape(10, 3)
        np.testing.assert_allclose(p.fit(coords)[0], coords[5])

    def test_a_text_proxy_with_shear_or_bad_references_is_refused(self):
        with self.assertRaises(mh2gltf.ConvertError):
            mh2gltf.Proxy(self.mhclo("data/clothes/a", extra="l_shear_x 0 1 0.1 0.2\n"), "clothes", 10)
        with self.assertRaises(mh2gltf.ConvertError):
            mh2gltf.Proxy(self.mhclo("data/clothes/b"), "clothes", 6)

    def test_assets_resolve_in_every_data_folder_user_folder_first(self):
        user = os.path.join(self.tmp.name, "user")
        data = os.path.join(self.tmp.name, "data")
        self.mhclo("user/clothes/cap")
        self.mhclo("data/clothes/cap")
        self.mhclo("data/clothes/other", uuid="99999999-2222-3333-4444-555555555555")
        index = mh2gltf.proxy_index([user, data])
        self.assertTrue(index[("clothes", "11111111-2222-3333-4444-555555555555")].startswith(os.path.realpath(user)))
        self.assertIn(("clothes", "99999999-2222-3333-4444-555555555555"), index)
        self.write("user/clothes/cap/cap.mhmat", "x")
        self.write("data/clothes/cap/tex.png", "x")
        files = mh2gltf.Files([user, data], os.path.join(self.tmp.name, "char"))
        self.assertTrue(files.find("data/clothes/cap/cap.mhmat", []).startswith(os.path.realpath(user)))
        self.assertTrue(files.find("tex.png", [os.path.join(data, "clothes", "cap")]).endswith("tex.png"))
        with self.assertRaises(mh2gltf.ConvertError):
            files.find("../secret.txt", [user])

    def test_cloth_with_little_specular_reads_matte(self):
        files = mh2gltf.Files(self.tmp.name, self.tmp.name)
        felt = mh2gltf.read_material(self.write("felt.mhmat", "specularColor 0.043 0.043 0.0\nshininess 0.8\n"), files)
        self.assertGreaterEqual(felt.roughness, 0.8)
        self.assertEqual(felt.metallic, 0.0)
        glossy = mh2gltf.read_material(self.write("gloss.mhmat", "specularColor 1 1 1\nshininess 0.8\n"), files)
        self.assertAlmostEqual(glossy.roughness, 0.35)
        plain = mh2gltf.read_material(self.write("plain.mhmat", "shininess 0.2\n"), files)
        self.assertAlmostEqual(plain.roughness, 0.8)
        given = mh2gltf.read_material(self.write("pbr.mhmat", "shininess 0.9\nroughness 0.7\nmetallic 0.25\n"), files)
        self.assertEqual((given.roughness, given.metallic), (0.7, 0.25))

    def test_a_greyscale_texture_is_stretched_to_white_before_the_tint(self):
        img = np.zeros((2, 2, 4), np.uint8)
        img[..., :3] = [[[20, 10, 0], [40, 30, 20]], [[60, 50, 40], [0, 0, 0]]]
        img[..., 3] = [[255, 255], [255, 0]]
        out = mh2gltf.greyscale(img)
        self.assertEqual(out.shape, (2, 2, 4))
        self.assertTrue(np.all(out[..., 0] == out[..., 1]) and np.all(out[..., 1] == out[..., 2]))
        self.assertEqual(int(out[1, 0, 0]), 255)  # the brightest opaque texel
        self.assertLess(int(out[0, 0, 0]), int(out[0, 1, 0]))
        np.testing.assert_array_equal(out[..., 3], img[..., 3])
        m = mh2gltf.read_material(self.write("grey.mhmat", "greyscaleTexture True\n"), mh2gltf.Files(self.tmp.name, self.tmp.name))
        self.assertTrue(m.greyscale)

    def test_converter_lines_of_the_mhm(self):
        path = self.write(
            "w.mhm",
            "name w\ndecimate pants 0a9bdba7-cd5b-4270-8a9b-f7d6ef2abaac 2500\n"
            "piece hat crown=1.1 color=0.3,0.2,0.1\npiece neckerchief\npiece jacket vneck=0 skirt=2.75\n",
        )
        c = mh2gltf.read_mhm(path)
        self.assertEqual(c.decimate, {"0a9bdba7-cd5b-4270-8a9b-f7d6ef2abaac": 2500})
        self.assertEqual(c.pieces, [
            ("hat", {"crown": [1.1], "color": [0.3, 0.2, 0.1]}), ("neckerchief", {}), ("jacket", {"vneck": [0.0], "skirt": [2.75]}),
        ])
        for bad in ("piece cape\n", "piece hat crown\n", "piece hat crown=x\n", "piece shirt brim=1\n", "decimate pants 0a9bdba7 lots\n"):
            with self.assertRaises(mh2gltf.ConvertError, msg=bad):
                mh2gltf.read_mhm(self.write("b.mhm", bad))

    def grid(self, n=31):
        """An n x n vertex sheet, gently curved, with a UV seam down its
        middle column (the vertices there have two UVs) and two joints
        blended left to right."""
        xs, ys = np.meshgrid(np.arange(n, dtype=float), np.arange(n, dtype=float))
        coords = np.stack([xs.ravel(), ys.ravel(), 0.02 * (xs.ravel() - n / 2) ** 2], axis=1)
        uv_of = {}
        texco = []

        def uv(v, right):
            key = (v, right and v % n == n // 2)
            if key not in uv_of:
                uv_of[key] = len(texco)
                texco.append([v % n / n + (0.5 if key[1] else 0.0), v // n / n])
            return uv_of[key]

        tris = []
        for y in range(n - 1):
            for x in range(n - 1):
                a, b, c, d = y * n + x, y * n + x + 1, (y + 1) * n + x + 1, (y + 1) * n + x
                right = x >= n // 2
                tris.append(((a, uv(a, right)), (b, uv(b, right)), (c, uv(c, right))))
                tris.append(((a, uv(a, right)), (c, uv(c, right)), (d, uv(d, right))))
        w = np.zeros((n * n, 6))
        w[:, 1] = xs.ravel() / (n - 1)
        w[:, 2] = 1 - w[:, 1]
        return coords, tris, np.array(texco), w

    @staticmethod
    def edges(coords, tris, uvs=False):
        """Boundary edges by position, and seam vertices by (position, uv)."""
        count = {}
        for t in tris:
            for i in range(3):
                e = tuple(sorted((t[i][0], t[(i + 1) % 3][0])))
                count[e] = count.get(e, 0) + 1
        boundary = {tuple(sorted((tuple(coords[a]), tuple(coords[b])))) for (a, b), k in count.items() if k == 1}
        corners = {}
        for t in tris:
            for v, u in t:
                corners.setdefault(v, set()).add(u)
        seams = {(tuple(coords[v]), tuple(uvs[u])) for v, us in corners.items() if len(us) > 1 for u in us} if uvs is not False else None
        return boundary, seams

    def test_decimation_reaches_its_target_keeping_boundary_and_seams(self):
        coords, tris, texco, w = self.grid()
        out_coords, out_tris, out_w = mh2gltf.decimate(coords, tris, w, 700)
        self.assertLessEqual(len(out_tris), 700)
        self.assertGreater(len(out_tris), 500)
        self.assertEqual(self.edges(out_coords, out_tris)[0], self.edges(coords, tris)[0])
        before = self.edges(coords, tris, texco)[1]
        after = self.edges(out_coords, out_tris, texco)[1]
        self.assertTrue(before and before <= after, "every seam vertex keeps its UVs")
        np.testing.assert_allclose(out_w.sum(axis=1), 1.0, atol=1e-9)
        self.assertLessEqual(int((out_w > 0).sum(axis=1).max()), mh2gltf.MAX_INFLUENCES)
        self.assertEqual(len(out_coords), len(out_w))
        used = {v for t in out_tris for v, _ in t}
        self.assertEqual(used, set(range(len(out_coords))), "no stray vertices")
        # No triangle turned over: all still face +z-ish like the sheet.
        t = np.array([[v for v, _ in tri] for tri in out_tris])
        n = np.cross(out_coords[t[:, 1]] - out_coords[t[:, 0]], out_coords[t[:, 2]] - out_coords[t[:, 0]])
        self.assertTrue(np.all(n[:, 2] > 0))

    def test_decimation_merges_weights_and_is_deterministic(self):
        coords, tris, _, w = self.grid(13)
        w[:, 3] = 0.1  # five influences before the merge
        w[:, 4] = 0.1
        w[:, 5] = 0.05
        w /= w.sum(axis=1, keepdims=True)
        a = mh2gltf.decimate(coords, tris, w, 120)
        b = mh2gltf.decimate(coords.copy(), list(tris), w.copy(), 120)
        np.testing.assert_array_equal(a[0], b[0])
        self.assertEqual(a[1], b[1])
        np.testing.assert_array_equal(a[2], b[2])
        np.testing.assert_allclose(a[2].sum(axis=1), 1.0, atol=1e-9)
        self.assertLessEqual(int((a[2] > 0).sum(axis=1).max()), 4)
        # Under the target already: unchanged.
        same = mh2gltf.decimate(coords, tris, w, 10_000)
        self.assertEqual(len(same[1]), len(tris))

    def test_an_outline_hugs_a_cylinder_and_fills_empty_sectors(self):
        a = np.linspace(0, 2 * np.pi, 400, endpoint=False)
        pts = np.stack([2 + np.sin(a), np.zeros_like(a), 1 + np.cos(a)], axis=1)
        r = mh2gltf.outline(pts, np.array([2.0, 0.0, 1.0]), -1, 1, 0.1)
        np.testing.assert_allclose(r, 1.1, atol=0.02)
        few = pts[[0, 100, 200, 300]]  # four sectors of twenty
        np.testing.assert_allclose(mh2gltf.outline(few, np.array([2.0, 0.0, 1.0]), -1, 1, 0.0), 1.0, atol=1e-9)
        with self.assertRaises(mh2gltf.ConvertError):
            mh2gltf.outline(pts, np.zeros(3), 5, 6, 0.1)

    class Head:
        """A stand-in body: a head, a sphere of radius 1 round y = 16, on
        a trunk, a cylinder of radius 1 from y = 10 up to the head."""

        JOINTS = {"spine_01": 12.0, "spine_02": 13.0, "spine_03": 14.0, "neck_01": 15.0, "head": 16.0}
        SIDES = {"upperarm_l": (1.2, 14.8), "upperarm_r": (-1.2, 14.8), "thigh_l": (0.5, 10.5), "thigh_r": (-0.5, 10.5)}

        def __init__(self):
            rng = np.random.default_rng(1)
            p = rng.normal(size=(3000, 3))
            a, y = rng.uniform(0, 2 * np.pi, 3000), rng.uniform(10, 15.5, 3000)
            trunk = np.stack([np.sin(a), y, np.cos(a)], axis=1)
            self.points = np.vstack([p / np.linalg.norm(p, axis=1, keepdims=True) + [0.0, 16.0, 0.0], trunk])

            class Rig:
                order = ["Root", "spine_02", "head"]
            self.rig = Rig()

        def select(self, names=None, arms=False, kind=None):
            return self.points if names != {"head"} else self.points[:3000]

        def surface(self, x, y, names, kind=None, back=False):
            """The trunk's surface along z, or None off it."""
            if abs(x) >= 1 or not 10 <= y <= 15.5:
                return None
            return -np.sqrt(1 - x * x) if back else np.sqrt(1 - x * x)

        def at(self, name):
            if name in self.SIDES:
                x, y = self.SIDES[name]
                return np.array([x, y, 0.0])
            return np.array([0.0, self.JOINTS[name], 0.0])

    def test_a_hat_sits_on_the_head_with_its_brim_out_and_its_crown_above(self):
        head = self.Head()
        (coords, tris), (ribbon, rtris), band = mh2gltf.make_hat(head, {"brim": [0.7], "droop": [0.3], "tilt": [0.0]}, mh2gltf.SEGMENTS)
        self.assertGreater(coords[:, 1].max(), 17.0)  # above the head's top
        self.assertLess(band, 17.0 - 1.0)
        reach = np.hypot(coords[:, 0], coords[:, 2])
        self.assertGreater(reach.max(), 1.6)  # the brim
        front = coords[np.argmax(coords[:, 2])]
        self.assertLess(front[1], band - 0.2)  # turned down at the front
        self.assertTrue(all(0 <= v < len(coords) for t in tris for v in t))
        self.assertTrue(rtris and np.hypot(ribbon[:, 0], ribbon[:, 2]).min() > 1.0)
        piece = mh2gltf.made("hat", mh2gltf.plain("felt", [0.3, 0.2, 0.1]), coords, tris, "head", self.rig())
        np.testing.assert_array_equal(piece.weights.sum(axis=1), 1.0)
        self.assertTrue(np.all(piece.weights[:, 2] == 1.0))
        self.assertGreaterEqual(piece.material.roughness, 0.8)
        with self.assertRaises(mh2gltf.ConvertError):
            mh2gltf.made("hat", piece.material, coords, tris, "no_such_joint", self.rig())

    def test_a_piece_over_several_bones_takes_the_skin_of_the_body_under_it(self):
        # A stand-in trunk: a column of points from the pelvis (y 0, all
        # pelvis) to the chest (y 10, all spine_03), blending between, and
        # an arm out to the side (all upperarm_l).
        class Rig:
            order = ["Root", "pelvis", "spine_03", "upperarm_l"]

        class Trunk:
            rig = Rig()

        ys = np.linspace(0.0, 10.0, 41)
        chest = np.clip(ys / 10.0, 0.0, 1.0)
        trunk = np.stack([np.zeros_like(ys), ys, np.zeros_like(ys)], axis=1)
        arm = np.stack([np.linspace(1.0, 4.0, 10), np.full(10, 9.0), np.zeros(10)], axis=1)
        body = Trunk()
        body.points = np.vstack([trunk, arm])
        body.weights = np.zeros((len(body.points), 4))
        body.weights[:41, 1], body.weights[:41, 2] = 1 - chest, chest
        body.weights[41:, 3] = 1.0
        # A strip from the hip up to the shoulder, just in front.
        coords = np.stack([np.full(11, 0.0), np.linspace(0.0, 10.0, 11), np.full(11, 0.3)], axis=1)
        tris = [(k, k + 1, k + 2) for k in range(9)]
        piece = mh2gltf.made("poncho", mh2gltf.plain("poncho", [1, 1, 1]), coords, tris, "spine_03", body.rig)
        piece = mh2gltf.inherit(piece, body, ("pelvis", "spine_03"))
        w = piece.weights
        np.testing.assert_allclose(w.sum(axis=1), 1.0, atol=1e-9)
        # More than one joint, blending along it as the body under it does.
        self.assertTrue(np.all((w[:, 1:3] > 0).sum(axis=1) >= 1))
        self.assertGreater(((w[:, 1:3] > 0.05).sum(axis=1) == 2).sum(), 5)
        np.testing.assert_allclose(w[:, 2], np.clip(coords[:, 1] / 10.0, 0, 1), atol=0.08)
        self.assertGreater(w[0, 1], 0.9)
        self.assertGreater(w[-1, 2], 0.9)
        # Never the arm, though the shoulder end is near it.
        np.testing.assert_array_equal(w[:, 3], 0.0)
        np.testing.assert_array_equal(w[:, 0], 0.0)
        # Joints the body has no points on leave the piece as it was.
        rigid = mh2gltf.made("poncho", piece.material, coords, tris, "spine_03", body.rig)
        self.assertIs(mh2gltf.inherit(rigid, body, ("Root",)), rigid)
        np.testing.assert_array_equal(rigid.weights[:, 2], 1.0)

    def test_the_poncho_is_kept_off_the_upper_arm_and_rides_on_it(self):
        # A stand-in left arm (decimeters): its bone from the shoulder at
        # (2, 10, 0) down to the elbow at (2, 7.5, 0), its surface a tube
        # 0.5 round it, on upperarm_l.
        class Rig:
            order = ["Root", "spine_03", "upperarm_l", "lowerarm_l"]

        class Arm:
            rig = Rig()

            def at(self, name):
                return {"upperarm_l": np.array([2.0, 10.0, 0.0]), "lowerarm_l": np.array([2.0, 7.5, 0.0])}[name]

            def select(self, names):
                a = np.linspace(0.0, 2 * np.pi, 12, endpoint=False)
                ys = np.linspace(7.5, 10.0, 11)
                ring = np.stack([2.0 + 0.5 * np.cos(a), np.zeros_like(a), 0.5 * np.sin(a)], axis=1)
                return np.vstack([ring + [0.0, y, 0.0] for y in ys]) if names == {"upperarm_l"} else np.zeros((0, 3))

        body = Arm()
        # A strip hanging down through the arm, 0.1 off its bone, and the
        # same strip on the chest by the armpit (in from the shoulder by
        # more than 0.7, though within the clearance of the arm's surface).
        ys = np.linspace(7.0, 10.5, 15)
        through = np.stack([np.full(15, 2.1), ys, np.zeros(15)], axis=1)
        chest = np.stack([np.full(15, 1.28), ys, np.zeros(15)], axis=1)
        coords = np.vstack([through, chest])
        tris = [(k, k + 1, k + 15) for k in range(14)]
        piece = mh2gltf.made("poncho", mh2gltf.plain("poncho", [1, 1, 1]), coords, tris, "spine_03", body.rig)
        before = piece.coords.copy()
        piece = mh2gltf.clear_arms(piece, body, sides=("l",))
        p, w = piece.coords, piece.weights
        np.testing.assert_allclose(w.sum(axis=1), 1.0, atol=1e-9)
        # Along the upper arm (and just past the elbow) it lies outside it,
        # the clearance off its surface, and takes most of its turn.
        along = (before[:15, 1] <= 10.0) & (before[:15, 1] >= 7.5 - 0.5)
        bone = np.stack([np.full(15, 2.0), np.clip(p[:15, 1], 7.5, 10.0), np.zeros(15)], axis=1)
        off = np.linalg.norm(p[:15] - bone, axis=1)
        np.testing.assert_allclose(off[along], 0.5 + mh2gltf.ARM_CLEARANCE, atol=1e-6)
        np.testing.assert_allclose(w[:15][along, 2], mh2gltf.ARM_SHARE, atol=1e-6)
        np.testing.assert_allclose(w[:15][along, 1], 1 - mh2gltf.ARM_SHARE, atol=1e-6)
        # Along the bone it is pushed straight out: the height is kept.
        beside = (before[:15, 1] <= 10.0) & (before[:15, 1] >= 7.5)
        np.testing.assert_allclose(p[:15][beside, 1], before[:15][beside, 1])
        # Well past the elbow and above the shoulder, it stays.
        far = (before[:15, 1] < 7.5 - 0.6) | (before[:15, 1] > 10.0 + 0.5)
        np.testing.assert_array_equal(p[:15][far], before[:15][far])
        # The chest is left as it was, on its own joint.
        np.testing.assert_array_equal(p[15:], before[15:])
        np.testing.assert_array_equal(w[15:, 1], 1.0)

    def test_the_other_pieces_fit_round_their_joint(self):
        head = self.Head()
        (crown, ctris), (leather, ltris), (buttons, btris), _ = mh2gltf.make_kepi(head, {}, mh2gltf.SEGMENTS)
        self.assertGreater(leather[:, 2].max(), crown[:, 2].max() + 0.4)  # the visor juts forward
        self.assertTrue(ltris and ctris and len(btris) == 2 * 6)
        neck, ntris = mh2gltf.make_neckerchief(head, {}, mh2gltf.SEGMENTS)
        self.assertGreater(len(ntris), mh2gltf.SEGMENTS * 2)
        coins = mh2gltf.make_rastra(head, {"coins": [6]}, mh2gltf.SEGMENTS, None)[1][0]
        self.assertGreater(coins[:, 2].min(), 0.0)  # all on the front
        buttons, btris = mh2gltf.make_buttons(head, {"count": [3], "spread": [0.4]}, mh2gltf.SEGMENTS)
        self.assertEqual(len(btris), 3 * 2 * 6)
        sash, stris, texco = mh2gltf.make_sash(head, {}, mh2gltf.SEGMENTS)
        self.assertEqual(texco.shape, (len(sash), 2))  # a UV per vertex for the stripes
        collar, ctris = mh2gltf.make_collar(head, {}, mh2gltf.SEGMENTS)
        self.assertGreater(len(ctris), 2 * mh2gltf.SEGMENTS)  # the band and its wing tips
        self.assertGreater(np.hypot(collar[:, 0], collar[:, 2]).min(), 0.9)
        strap, _ = mh2gltf.make_strap(head, {}, mh2gltf.SEGMENTS)
        self.assertTrue(strap[:, 2].max() > 1.0 and strap[:, 2].min() < -1.0)  # over the chest and the back
        chain, _ = mh2gltf.make_chain(head, {}, mh2gltf.SEGMENTS)
        self.assertGreater(chain[:, 2].min(), 0.8)  # on the front

    def test_a_poncho_lies_over_one_shoulder_and_hangs_below_the_hip(self):
        head = self.Head()
        coords, tris, texco = mh2gltf.make_poncho(head, {"side": [1]}, mh2gltf.SEGMENTS)
        self.assertEqual(texco.shape, (len(coords), 2))
        self.assertTrue(np.all(texco[:, 0] == 0.0), "the bands run along it: no change along u")
        top = coords[np.argmax(coords[:, 1])]
        self.assertGreater(top[0], 0.0)  # over the left shoulder
        self.assertGreater(top[1], 15.0)
        self.assertLess(coords[:, 1].min(), 10.5 - 0.5)  # its ends hang below the hip joint
        self.assertTrue(coords[:, 2].max() > 1.0 and coords[:, 2].min() < -1.0)

    def test_a_hat_has_a_flat_crown_and_a_flat_brim(self):
        n = mh2gltf.SEGMENTS
        (coords, _), _, band = mh2gltf.make_hat(self.Head(), {"droop": [0.0], "dent": [0.0], "tilt": [0.0], "taper": [1.0]}, n)
        top = coords[3 * n : 5 * n + 1]  # the top's edge, its inner ring and its middle
        self.assertLess(np.ptp(top[:, 1]), 1e-9)
        self.assertGreater(top[:, 1].min(), 17.0)
        sides = coords[: 2 * n]  # straight sides: as wide at the top as at the band
        np.testing.assert_allclose(np.hypot(sides[n:, 0], sides[n:, 2]), np.hypot(sides[:n, 0], sides[:n, 2]), rtol=1e-9)
        brim = coords[5 * n + 1 : 8 * n + 1]  # the brim's upper surface
        np.testing.assert_allclose(brim[:, 1], band)
        self.assertGreater(np.hypot(brim[:, 0], brim[:, 2]).max(), 1.7)
        # A dent creases the top front to back, deepest in the middle.
        (dented, _), _, _ = mh2gltf.make_hat(self.Head(), {"dent": [0.15], "tilt": [0.0]}, n)
        self.assertAlmostEqual(dented[5 * n, 1], dented[3 * n, 1] - 0.15)

    def test_a_kepi_has_a_flat_top_wider_than_its_band_and_a_visor_in_front(self):
        n = mh2gltf.SEGMENTS
        (crown, _), (leather, _), _, band = mh2gltf.make_kepi(self.Head(), {}, n)
        lower, upper = crown[:n], crown[n : 2 * n]
        self.assertTrue(np.all(np.hypot(upper[:, 0], upper[:, 2]) > np.hypot(lower[:, 0], lower[:, 2])))
        # The top is one plane, lower at the front than at the back.
        centered = upper - upper.mean(axis=0)
        self.assertLess(np.linalg.svd(centered, compute_uv=False)[2], 1e-9)
        self.assertLess(upper[0, 1], upper[n // 2, 1])
        visor = leather[2 * n :]
        tip = visor[np.argmax(visor[:, 2])]
        self.assertGreater(tip[2], lower[:, 2].max() + 0.4)
        self.assertLess(tip[1], band)  # angled down

    def test_colours_are_written_linear_as_gltf_wants(self):
        # A .mhm or .mhmat colour is a display (sRGB) colour; glTF's
        # baseColorFactor is linear, so a dark suit must stay dark.
        self.assertAlmostEqual(mh2gltf.linear(0.21), 0.0363, places=4)
        self.assertAlmostEqual(mh2gltf.linear(0.03), 0.03 / 12.92, places=9)
        self.assertEqual((mh2gltf.linear(0.0), mh2gltf.linear(1.0)), (0.0, 1.0))
        self.assertLess(mh2gltf.linear(0.5), 0.22)

    def test_cloth_textures_are_small_grey_and_the_same_every_time(self):
        for name in ("cotton", "wool", "stripe", "leather"):
            img = mh2gltf.cloth(name)
            self.assertEqual((img.shape, img.dtype), ((64, 64, 3), np.uint8))
            np.testing.assert_array_equal(img[..., 0], img[..., 2])
            np.testing.assert_array_equal(img, mh2gltf.cloth(name))
            self.assertGreater(int(img.max()) - int(img.min()), 8, name)
        stripe = mh2gltf.cloth("stripe").astype(int)
        self.assertLess(stripe[6].mean(), stripe[32].mean() - 40)  # a dark stripe across v
        with self.assertRaises(mh2gltf.ConvertError):
            mh2gltf.cloth("silk")

    class Skin:
        """A stand-in posed body: a cylinder of radius 1 from the soles
        (y = 0) up to y = 17, its vertices on the joint their height
        falls in (left or right by x), and an arm of loose vertices down
        from a shoulder at (1.5, 14.5)."""

        ORDER = ["Root", "pelvis", "spine_02", "neck_01", "head", "thigh_l", "thigh_r", "calf_l", "calf_r",
                 "foot_l", "foot_r", "upperarm_l", "upperarm_r", "lowerarm_l", "lowerarm_r", "hand_l", "hand_r"]
        AT = {"pelvis": 10.0, "spine_02": 11.5, "neck_01": 15.6, "head": 16.5, "thigh": 9.8, "calf": 5.5, "foot": 1.0}
        ARM = {"upperarm": 14.5, "lowerarm": 12.0, "hand": 9.5}

        def __init__(self, sectors=16, heights=None):
            class Rig:
                order = self.ORDER
            self.rig = Rig()
            self.rest = {}
            for name in self.ORDER:
                m = np.identity(4)
                base = name.rsplit("_", 1)[0] if name[-2:] in ("_l", "_r") else name
                side = 1.0 if name.endswith("_l") else -1.0 if name.endswith("_r") else 0.0
                if base in self.ARM:
                    m[:3, 3] = [1.5 * side, self.ARM[base], 0.0]
                else:
                    m[:3, 3] = [0.5 * side, self.AT.get(base, 0.0), 0.0]
                self.rest[name] = m
            heights = np.arange(0.0, 17.01, 0.5) if heights is None else heights
            a = np.arange(sectors) * 2 * np.pi / sectors
            coords = np.array([[np.sin(t), y, np.cos(t)] for y in heights for t in a])
            tris = []
            for i in range(len(heights) - 1):
                for k in range(sectors):
                    p, q = i * sectors + k, i * sectors + (k + 1) % sectors
                    tris += [(p, q, q + sectors), (p, q + sectors, p + sectors)]
            arm = np.array([[1.5, 14.5 - 0.5 * i, 0.0] for i in range(12)])
            joint_of = []
            for x, y, _ in coords:
                side = "l" if x >= 0 else "r"
                if y < 1.0:
                    joint_of.append(f"foot_{side}")
                elif y < 5.5:
                    joint_of.append(f"calf_{side}")
                elif y < 9.8:
                    joint_of.append(f"thigh_{side}")
                elif y < 15.5:
                    joint_of.append("spine_02")
                elif y < 16.3:
                    joint_of.append("neck_01")
                else:
                    joint_of.append("head")
            joint_of += ["upperarm_l" if y > 12 else "lowerarm_l" if y > 9.5 else "hand_l" for _, y, _ in arm]
            self.coords = np.vstack([coords, arm])
            self.weights = np.zeros((len(self.coords), len(self.ORDER)))
            for v, name in enumerate(joint_of):
                self.weights[v, self.ORDER.index(name)] = 0.75
                self.weights[v, self.ORDER.index("pelvis")] += 0.25
            self.triangles = [tuple((v, v) for v in t) for t in tris]
            self.all_triangles = self.triangles
            self.texco = np.stack([self.coords[:, 0], self.coords[:, 1] / 17], axis=1)

    def dressed(self, *pieces):
        skin = self.Skin()
        figure = mh2gltf.Figure(skin.rig, skin.rest, skin)

        class Character:
            pass

        c = Character()
        c.pieces = list(pieces)
        out, hide = mh2gltf.dress(c, figure, skin.rig)
        return figure, {pc.label: pc for pc in out}, hide

    def test_a_figure_finds_the_limbs_heights_and_reach(self):
        skin = self.Skin()
        f = mh2gltf.Figure(skin.rig, skin.rest, skin)
        body = f.limb == mh2gltf.BODY
        cylinder = np.arange(len(f.coords)) < len(f.coords) - 12
        self.assertTrue(np.all(f.limb[~cylinder] == mh2gltf.ARM))
        self.assertTrue(np.all(f.limb[cylinder & (f.coords[:, 1] > 16.3)] == mh2gltf.HEAD))
        at = [f.station[cylinder & body & (f.coords[:, 1] == y)][0] for y in (0.0, 1.0, 5.5, 9.5, 11.5, 14.5)]
        np.testing.assert_allclose(at, [0, 1, 2, 2 + 4 / 4.3, 4, 5], atol=1e-9)
        reach = f.reach[~cylinder]
        np.testing.assert_allclose(reach[[0, 5, 10]], [0.0, 1.0, 2.0], atol=1e-9)
        self.assertTrue(np.all(np.diff(reach) > 0))
        np.testing.assert_array_equal(f.side[~cylinder], 1)

    def test_a_garment_is_the_skin_pushed_out_keeping_its_weights(self):
        f, out, hide = self.dressed(("trousers", {}))
        pc = out["trousers"]
        np.testing.assert_array_equal(pc.weights, f.weights[pc.source])
        radius = np.hypot(pc.coords[:, 0], pc.coords[:, 2])
        shell = {v for t in pc.triangles[: len(pc.triangles)] for v, _ in t}
        offsets = radius - 1.0
        self.assertTrue(np.all((offsets > mh2gltf.RIM_INSET - 1e-9) & (offsets < 0.11)))
        station = f.station[pc.source]
        self.assertTrue(np.all((station >= 1.2 - 1e-9) & (station <= 4.3 + 1e-9)), "only the legs and hips")
        # The garment's faces are skin faces, which the skin then leaves out.
        self.assertTrue(hide and all(f.limb[list(t)].tolist() == [mh2gltf.BODY] * 3 for t in hide))
        self.assertTrue(shell)

    def test_rims_close_the_ends_of_a_garment(self):
        _, out, _ = self.dressed(("shirt", {"cuff": [0.0]}))
        pc = out["shirt"]
        count = {}
        for t in pc.triangles:
            for i in range(3):
                a, b = (tuple(np.round(pc.coords[t[j][0]], 9)) for j in (i, (i + 1) % 3))
                key = (min(a, b), max(a, b))
                count[key] = count.get(key, 0) + 1
        open_ = [e for e, k in count.items() if k == 1]
        self.assertTrue(open_)
        for a, b in open_:  # the only open edges lie on the skin, under the rims
            for p in (a, b):
                self.assertAlmostEqual(np.hypot(p[0], p[2]), 1.0 + mh2gltf.RIM_INSET, places=6)

    def test_the_outer_garment_takes_the_faces_where_garments_overlap(self):
        _, out, hide = self.dressed(("trousers", {}), ("boots", {}))
        faces = {name: {tuple(sorted(pc.source[v] for v, _ in t)) for t in pc.triangles} for name, pc in out.items()}
        self.assertFalse(faces["trousers"] & faces["boots"])
        boots = out["boots"]
        self.assertGreater(np.hypot(boots.coords[:, 0], boots.coords[:, 2]).max(), 1.12)  # flared at the top
        # The trousers are tucked in: none of their faces sits inside the
        # boots' height except at the rims.
        trousers = out["trousers"]
        low = [t for t in trousers.triangles if max(trousers.coords[v][1] for v, _ in t) < 5.0]
        self.assertFalse(low)
        self.assertTrue(len(hide) > len(faces["boots"]))

    def test_a_vneck_opens_the_front_and_the_shirt_shows_in_it(self):
        _, out, _ = self.dressed(("shirt", {}), ("waistcoat", {"vneck": [5.0], "vwidth": [0.6]}))
        vest, shirt = out["waistcoat"], out["shirt"]
        front = vest.coords[(vest.coords[:, 1] > 14.8) & (np.abs(vest.coords[:, 0]) < 0.2) & (vest.coords[:, 2] > 0)]
        self.assertFalse(len(front), "the V is open")
        shown = shirt.coords[(shirt.coords[:, 1] > 14.8) & (shirt.coords[:, 2] > 0.9)]
        self.assertTrue(len(shown), "the shirt fills the V")

    def test_garments_are_the_same_every_time(self):
        a = self.dressed(("trousers", {}), ("boots", {}), ("jacket", {"skirt": [2.5]}))
        b = self.dressed(("trousers", {}), ("boots", {}), ("jacket", {"skirt": [2.5]}))
        self.assertEqual(sorted(a[1]), sorted(b[1]))
        for name in a[1]:
            np.testing.assert_array_equal(a[1][name].coords, b[1][name].coords)
            self.assertEqual(a[1][name].triangles, b[1][name].triangles)
        self.assertEqual(a[2], b[2])

    def test_a_coat_skirt_hangs_from_the_waist_round_the_legs(self):
        f, out, _ = self.dressed(("jacket", {"skirt": [2.5], "open": [1]}))
        skirt = out["jacket skirt"]
        self.assertLess(skirt.coords[:, 1].min(), f.height(2.6))
        self.assertGreater(skirt.coords[:, 1].max(), f.height(3.9))
        self.assertGreater(np.hypot(skirt.coords[:, 0], skirt.coords[:, 2]).min(), 1.0)
        np.testing.assert_allclose(skirt.weights.sum(axis=1), 1.0)
        self.assertLess(skirt.coords[np.argmax(skirt.coords[:, 2])][2], 1.3 * np.cos(np.pi / 10) + 0.5)
        front = skirt.coords[(np.abs(skirt.coords[:, 0]) < 0.05) & (skirt.coords[:, 2] > 0)]
        self.assertFalse(len(front), "open at the front")

    def rig(self):
        class Rig:
            order = ["Root", "upperarm_l", "head"]
            bones = {"Root": {"parent": None}, "upperarm_l": {"parent": "Root"}, "head": {"parent": "Root"}}
        return Rig()

    def test_the_arms_are_lowered_to_hang_and_the_skin_follows(self):
        class Rig:
            order = ["Root", "upperarm_l", "lowerarm_l"]
            bones = {"Root": {"parent": None}, "upperarm_l": {"parent": "Root"}, "lowerarm_l": {"parent": "upperarm_l"}}

        # An arm straight out to +x from a shoulder at (1, 10, 0): the
        # bones' Y axes point along it.
        out = np.array([[0.0, 1.0, 0.0], [-1.0, 0.0, 0.0], [0.0, 0.0, 1.0]])  # columns x, y, z
        rest = {"Root": np.identity(4)}
        for name, x in (("upperarm_l", 1.0), ("lowerarm_l", 3.0)):
            m = np.identity(4)
            m[:3, :3] = out
            m[:3, 3] = [x, 10.0, 0.0]
            rest[name] = m
        w = np.array([[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0], [0.0, 0.5, 0.5]])
        coords = np.array([[0.0, 10.0, 0.0], [2.0, 10.0, 0.0], [5.0, 10.0, 0.0], [3.0, 10.0, 0.0]])
        piece = mh2gltf.Piece("arm", "proxy", None, coords.copy(), [], w, None)
        posed = mh2gltf.pose_arms(Rig(), rest, [piece])
        np.testing.assert_allclose(piece.coords[0], coords[0])  # the body stays
        hang = mh2gltf.normalize(np.array(mh2gltf.ARM_HANG))
        upper = mh2gltf.normalize(piece.coords[3] - [1.0, 10.0, 0.0])
        self.assertGreater(float(upper @ hang), 0.999)  # the elbow hangs below the shoulder
        np.testing.assert_allclose(posed["lowerarm_l"][:3, 3], piece.coords[3], atol=1e-9)
        fore = mh2gltf.normalize(piece.coords[2] - piece.coords[3])
        self.assertLess(fore[1], -0.9)
        self.assertGreater(fore[2], 0.05)  # bent a little forward
        for name in posed:
            np.testing.assert_allclose(posed[name][:3, :3] @ posed[name][:3, :3].T, np.identity(3), atol=1e-9)

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


class TownsfolkPieces(unittest.TestCase):
    """The townsfolk's pieces (the women's skirt, apron, shawl, headscarf
    and hair worn up; the boina and alpargatas) on the stand-in bodies."""

    def dressed(self, *pieces):
        skin = Units.Skin()
        figure = mh2gltf.Figure(skin.rig, skin.rest, skin)

        class Character:
            pass

        c = Character()
        c.pieces = list(pieces)
        out, hide = mh2gltf.dress(c, figure, skin.rig)
        return figure, {pc.label: pc for pc in out}, hide

    @staticmethod
    def rows(pc):
        """The skirt's rings, top down: the waistband's top, its rows, then
        the hem turned in."""
        n = mh2gltf.SKIRT_SEGMENTS
        return pc.coords.reshape(-1, n, 3)

    def test_a_long_skirt_hangs_as_a_bell_from_the_waist_to_the_ankles(self):
        f, out, hide = self.dressed(("skirt", {}))
        skirt = out["skirt"]
        rows = self.rows(skirt)
        self.assertEqual(len(rows), len(mh2gltf.SKIRT_ROWS) + 2)
        # From just above the waist (station 3.9) down to the ankles' 1.15.
        self.assertAlmostEqual(rows[1][:, 1].max(), f.height(3.9))
        self.assertAlmostEqual(rows[-2][:, 1].min(), f.height(1.15))
        radius = np.hypot(rows[:, :, 0], rows[:, :, 2])
        # Never in the body (a cylinder of radius 1), never narrower going
        # down, and its hem out by the flare.
        self.assertGreater(radius[:-1].min(), 1.0 + 0.05)
        means = radius[1:-1].mean(axis=1)
        self.assertTrue(np.all(np.diff(means) >= -1e-9), means)
        self.assertGreater(means[-1] - means[0], 0.8 * 0.9)
        # The hem turns in, a little up.
        self.assertTrue(np.all(radius[-1] < radius[-2]) and np.all(rows[-1][:, 1] > rows[-2][:, 1]))
        # Soft folds round the hem, none at the waist.
        self.assertGreater(np.ptp(radius[-2]), 0.04)
        self.assertLess(np.ptp(radius[1]), 1e-9)
        # The legs and hips under it are left out, from station 1.6 up to
        # its waistband; nothing else.
        self.assertTrue(hide)
        for t in hide:
            self.assertTrue(np.all(f.station[list(t)] >= 1.6 - 1e-9) and np.all(f.station[list(t)] <= 3.9 + 1e-9))
        # Legs narrower below the hips (the stand-in's knees and shins
        # half as wide): the skirt still never narrows going down.
        skin = Units.Skin()
        low = skin.coords[:, 1] < 9.0
        skin.coords[low, 0] *= 0.5
        skin.coords[low, 2] *= 0.5
        thin = mh2gltf.Figure(skin.rig, skin.rest, skin)

        class Character:
            pieces = [("skirt", {})]

        rows = self.rows(mh2gltf.dress(Character(), thin, skin.rig)[0][0])
        means = np.hypot(rows[1:-1, :, 0], rows[1:-1, :, 2]).mean(axis=1)
        self.assertTrue(np.all(np.diff(means) >= -1e-9), means)
        a = self.dressed(("skirt", {"flare": [0.5]}))[1]["skirt"]
        b = self.dressed(("skirt", {"flare": [0.5]}))[1]["skirt"]
        np.testing.assert_array_equal(a.coords, b.coords)
        self.assertEqual(a.triangles, b.triangles)
        with self.assertRaises(mh2gltf.ConvertError):
            self.dressed(("skirt", {"hem": [4.0]}))

    def test_a_skirt_with_too_few_body_points_round_it_is_refused(self):
        # Fewer points than a row needs, wherever it looks: an error, not a
        # search that never ends (its reach doubling to infinity).
        skin = Units.Skin()
        figure = mh2gltf.Figure(skin.rig, skin.rest, skin)
        y = figure.height(3.9)  # at the waistband, so its axis is found
        few = np.array([[0.0, y, 1.0], [1.0, y, 0.0], [0.0, y, -1.0]])
        with self.assertRaises(mh2gltf.ConvertError) as e:
            mh2gltf.make_long_skirt(figure, {}, few, skin.rig)
        self.assertIn("body points", str(e.exception))

    def test_an_apron_needs_the_skirts_own_ring_layout(self):
        f, out, _ = self.dressed(("skirt", {}))
        skirt = out["skirt"]
        self.assertEqual(skirt.rings, (mh2gltf.SKIRT_RINGS, mh2gltf.SKIRT_SEGMENTS))
        self.assertEqual(len(skirt.coords), mh2gltf.SKIRT_RINGS * mh2gltf.SKIRT_SEGMENTS)
        # A skirt of another layout (a ring short) is refused, not misread.
        skirt.coords = skirt.coords[: -mh2gltf.SKIRT_SEGMENTS]
        with self.assertRaises(mh2gltf.ConvertError):
            mh2gltf.make_apron(f, {}, skirt)

    def test_the_skirt_follows_the_thighs_more_toward_the_hem_and_the_shins_at_the_back(self):
        _, out, _ = self.dressed(("skirt", {}))
        skirt = out["skirt"]
        order = Units.Skin.ORDER
        w = skirt.weights.reshape(-1, mh2gltf.SKIRT_SEGMENTS, len(order))
        np.testing.assert_allclose(skirt.weights.sum(axis=1), 1.0, atol=1e-9)
        thigh = w[..., order.index("thigh_l")] + w[..., order.index("thigh_r")]
        calf = w[..., order.index("calf_l")] + w[..., order.index("calf_r")]
        # The waistband goes with the trunk alone; lower down the thighs'
        # share grows to the hem.
        self.assertTrue(np.all(thigh[:2] < 1e-9) and np.all(calf[:2] < 1e-9))
        front = 0  # angle 0 is the front
        column = thigh[1:-1, front]
        self.assertTrue(np.all(np.diff(column) >= -1e-9), column)
        self.assertAlmostEqual(column[-1], 0.8, places=6)
        # The front's middle takes the two thighs alike; to the left, the
        # left thigh more.
        hem = w[-2]
        self.assertAlmostEqual(hem[front, order.index("thigh_l")], hem[front, order.index("thigh_r")])
        left = mh2gltf.SKIRT_SEGMENTS // 4  # angle 90 degrees: +x, the left
        self.assertGreater(hem[left, order.index("thigh_l")], 2 * hem[left, order.index("thigh_r")])
        # The back of the hem takes the shins' turn too, the front none.
        back = mh2gltf.SKIRT_SEGMENTS // 2
        self.assertGreater(calf[-2, back], 0.3)
        self.assertLess(calf[-2, front], 1e-9)
        stiff = self.dressed(("skirt", {"follow": [0.0], "kick": [0.0]}))[1]["skirt"]
        np.testing.assert_array_equal(stiff.weights[:, order.index("thigh_l")], 0.0)
        np.testing.assert_array_equal(stiff.weights[:, order.index("calf_l")], 0.0)

    def test_an_apron_covers_the_skirts_front_just_outside_it(self):
        f, out, _ = self.dressed(("skirt", {}), ("apron", {"hem": [2.0], "width": [0.8]}))
        skirt, apron = out["skirt"], out["apron"]
        self.assertGreater(apron.coords[:, 2].min(), 0.0, "on the front only")
        self.assertAlmostEqual(apron.coords[:, 1].min(), f.height(2.0))
        self.assertAlmostEqual(apron.coords[:, 1].max(), skirt.coords[:, 1].max())
        # Just outside the skirt where it lies, skinned as the skirt is.
        rows = self.rows(skirt)
        for p, w in zip(apron.coords, apron.weights):
            ring = rows[np.argmin(np.abs(rows[:-1, 0, 1] - p[1]))]
            nearest = ring[np.argmin(np.linalg.norm(ring[:, [0, 2]] - p[[0, 2]], axis=1))]
            if abs(nearest[1] - p[1]) < 1e-9:
                self.assertGreater(np.hypot(p[0], p[2]), np.hypot(nearest[0], nearest[2]) + 0.03)
        np.testing.assert_allclose(apron.weights.sum(axis=1), 1.0, atol=1e-9)
        self.assertGreater(apron.weights[:, Units.Skin.ORDER.index("thigh_l")].max(), 0.1)
        with self.assertRaises(mh2gltf.ConvertError):
            self.dressed(("apron", {}))

    def test_a_shawl_covers_the_shoulders_and_a_point_down_the_back_over_the_blouse(self):
        f, out, _ = self.dressed(("shirt", {}), ("shawl", {}))
        shawl, shirt = out["shawl"], out["shirt"]
        src = shawl.source
        station, nz, x = f.station[src], f.normals[src, 2], f.coords[src, 0]
        trunk = f.limb[src] == mh2gltf.BODY
        # Round the shoulders all round; down the back to its point, at the
        # middle only; never on the front's chest below the shoulders.
        self.assertTrue(np.all(station[trunk & (nz >= 0.0)] >= 5.0 - 1e-9))
        low = trunk & (station < 4.8)
        self.assertTrue(low.any() and np.all(nz[low] < 1e-6))
        # Narrowing to its point: no wider than the line from the point
        # (station 3.9, the middle) to the shoulders' width (x 1.5) at 4.8.
        self.assertTrue(np.all(np.abs(x[low]) <= (station[low] - 3.9) / 0.9 * 1.5 + 1e-9))
        self.assertLess(station[trunk].min(), 4.1)
        # The outer layer: the blouse leaves the faces it covers.
        faces = {name: {tuple(sorted(pc.source[v] for v, _ in t)) for t in pc.triangles} for name, pc in out.items()}
        self.assertFalse(faces["shawl"] & faces["shirt"])
        self.assertTrue(faces["shirt"])

    def test_the_shawls_ends_cross_over_the_chest_one_over_the_other(self):
        head = Units.Head()
        (a, atris), (b, btris) = mh2gltf.make_shawl_ends(head, {})
        for coords, side in ((a, 1), (b, -1)):
            middle = coords.reshape(7, 10, 3)[3]  # along the band's middle
            top, bottom = middle[0], middle[-1]
            self.assertGreater(top[1], bottom[1])
            self.assertGreater(side * top[0], 0.0)  # from its own shoulder
            self.assertLess(side * bottom[0], 0.0)  # down to the other side
            # On the chest: in front of the trunk's surface.
            for p in coords:
                z = head.surface(p[0], p[1], None)
                if z is not None:
                    self.assertGreater(p[2], z + 0.05 - 1e-9)
        # Where they cross, the second lies over the first: it stands
        # further off the chest.
        def off(c):
            return np.median([p[2] - head.surface(p[0], p[1], None) for p in c if head.surface(p[0], p[1], None) is not None])

        self.assertGreater(off(b), off(a) + 0.02)
        self.assertTrue(atris and btris)

    @staticmethod
    def head_figure():
        """A stand-in for head_cut(): a sphere of radius 1 round y = 16 (the
        head) on a neck, the top of a cylinder of radius 0.5 (stations
        6.5 to 7)."""
        rng = np.random.default_rng(2)
        p = rng.normal(size=(4000, 3))
        p /= np.linalg.norm(p, axis=1, keepdims=True)
        a, y = rng.uniform(0, 2 * np.pi, 600), rng.uniform(14.0, 15.0, 600)
        neck = np.stack([0.5 * np.sin(a), y, 0.5 * np.cos(a)], axis=1)

        class F:
            pass

        f = F()
        f.coords = np.vstack([p + [0.0, 16.0, 0.0], neck])
        f.normals = np.vstack([p, np.stack([np.sin(a), np.zeros_like(a), np.cos(a)], axis=1)])
        f.limb = np.array([mh2gltf.HEAD] * 4000 + [mh2gltf.BODY] * 600)
        f.station = np.concatenate([np.full(4000, 7.0), 6.5 + (y - 14.0) / 2])
        return f

    def test_a_headscarf_frames_the_face_and_hair_worn_up_leaves_the_face_and_ears(self):
        f = self.head_figure()

        def g(kind, **over):
            return lambda key: over[key] if key in over else mh2gltf.setting({}, kind, key)

        covered, offset = mh2gltf.head_cut(f, "headscarf", g("headscarf"))
        head = f.limb == mh2gltf.HEAD
        y, nz, x = f.coords[:, 1], f.normals[:, 2], f.coords[:, 0]
        top, chin, _, height = mh2gltf.head_measures(f)
        brow = top - 0.3 * height
        face = head & (nz > 0.5) & (np.abs(x) < 0.25) & (np.abs(y - (brow + chin) / 2) < 0.3 * (brow - chin) / 2)
        self.assertFalse(covered[face].any(), "the face is bare")
        self.assertTrue(covered[head & (nz < -0.3)].all(), "the back of the head is covered")
        self.assertTrue(covered[head & (y > 16.9)].all(), "the top")
        self.assertTrue(covered[head & (np.abs(x) > 0.9)].all(), "the ears")
        self.assertTrue(covered[(f.limb == mh2gltf.BODY) & (f.station > 6.6)].all(), "round the top of the neck")
        self.assertTrue(np.all(offset[covered] >= 0.12 - 1e-9) and np.all(offset[~covered] == 0.0))
        self.assertGreater(offset[covered & (y > 16.95)].min(), offset[covered & (y < 15.5)].max())  # fuller at the crown
        hair, _ = mh2gltf.head_cut(f, "hair", g("hair"))
        self.assertFalse(hair[face].any())
        self.assertFalse(hair[head & (np.abs(x) > 0.9) & (np.abs(y - 15.7) < 0.1)].any(), "the ears show")
        self.assertTrue(hair[head & (nz < -0.5) & (y > 16.0)].all(), "the back of the head")
        self.assertFalse(hair[f.limb == mh2gltf.BODY].any())

    def test_a_headscarf_is_knotted_under_the_chin_with_its_point_down_the_neck(self):
        f, out, _ = self.dressed(("headscarf", {}))
        knot, point = out["headscarf knot"], out["headscarf point"]
        order = Units.Skin.ORDER
        np.testing.assert_array_equal(knot.weights[:, order.index("head")], 1.0)
        top, chin, middle, height = mh2gltf.head_measures(f)
        self.assertLess(knot.coords[:, 1].max(), chin + 0.1)
        np.testing.assert_allclose(point.weights.sum(axis=1), 1.0, atol=1e-9)
        allowed = [order.index(j) for j in mh2gltf.SCARF_POINT_FROM if j in order]
        self.assertAlmostEqual(float(point.weights[:, allowed].sum()), len(point.coords))
        self.assertTrue(np.all(point.coords[:, 2] < 0.0), "down the back")
        self.assertLess(point.coords[:, 1].min(), chin - 0.5)

    def test_a_boina_covers_the_crown_flat_and_wide_pulled_down_to_one_side(self):
        head = Units.Head()
        coords, tris, band = mh2gltf.make_boina(head, {}, mh2gltf.SEGMENTS)
        self.assertGreater(coords[:, 1].max(), 17.0 + 0.1)  # over the head's top
        self.assertLess(band, 17.0 - 0.5)
        n = mh2gltf.SEGMENTS
        band_r = np.hypot(coords[:n, 0], coords[:n, 2]).mean()
        rim = coords[2 * n : 3 * n]
        self.assertGreater(np.hypot(rim[:, 0] - rim[:, 0].mean(), rim[:, 2]).mean(), 1.2 * band_r)
        self.assertGreater(rim[:, 0].mean(), coords[:n, 0].mean() + 0.1)  # leaning to the left (+x)
        left, right = rim[np.argmax(rim[:, 0])], rim[np.argmin(rim[:, 0])]
        self.assertLess(left[1], right[1])  # its rim lower on that side
        self.assertTrue(all(0 <= v < len(coords) for t in tris for v in t))

    def test_a_bun_sits_behind_the_head_between_the_nape_and_the_crown(self):
        head = Units.Head()
        coords, tris = mh2gltf.make_bun(head, {"size": [0.8], "high": [0.5]}, mh2gltf.SEGMENTS)
        self.assertLess(coords[:, 2].max(), -0.5)  # at the back
        self.assertGreater(coords[:, 2].min(), -1.0 - 0.8)
        mid = coords[:, 1].mean()
        self.assertTrue(15.0 < mid < 17.0)
        self.assertAlmostEqual(np.ptp(coords[:, 0]), 0.8, places=6)
        self.assertTrue(tris)

    def test_alpargatas_are_canvas_shoes_with_a_rope_sole(self):
        f, out, _ = self.dressed(("alpargatas", {}))
        shoes = out["alpargatas"]
        self.assertEqual(shoes.material.pattern, "cotton")
        self.assertLessEqual(f.station[shoes.source].max(), 1.1 + 1e-9)
        self.assertGreater(f.station[shoes.source].max(), 0.9)
        # Under a bombacha gathered at the ankle, the shoes take the feet.
        _, out, _ = self.dressed(("bombacha", {"bottom": [0.98]}), ("alpargatas", {}))
        self.assertLess(out["bombacha"].coords[:, 1].min(), 1.5)
        self.assertTrue(out["alpargatas"].triangles)


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
        self.assertGreaterEqual(tris, 3000)
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

    def materials(self):
        return {self.g["materials"][p["material"]]["name"]: p for p in self.prims()}

    def test_the_pieces_on_one_bone_are_rigid_and_the_others_follow_the_body(self):
        names = [self.g["nodes"][j]["name"] for j in self.g["skins"][0]["joints"]]
        prims = self.materials()
        # The buttons down the chest and the strap from the shoulder to the
        # hip follow the trunk's joints under them, more than one.
        for material, joints in (("brass_buttons", mh2gltf.SKIN_FROM["buttons"]), ("strap_leather", mh2gltf.SKIN_FROM["strap"])):
            p = prims[material]
            j = read_accessor(self.g, self.blob, p["attributes"]["JOINTS_0"])
            w = read_accessor(self.g, self.blob, p["attributes"]["WEIGHTS_0"])
            used = {names[i] for i in np.unique(j[w > 0])}
            self.assertGreater(len(used), 1, material)
            self.assertTrue(used <= set(joints), (material, used))
        expect = {"kepi_cloth": "head", "kepi_leather": "head", "kepi_buttons": "head"}
        for material, joint in expect.items():
            p = prims[material]
            j = read_accessor(self.g, self.blob, p["attributes"]["JOINTS_0"])
            w = read_accessor(self.g, self.blob, p["attributes"]["WEIGHTS_0"])
            self.assertTrue(np.all(j[:, 0] == names.index(joint)), material)
            np.testing.assert_array_equal(w, np.tile([1.0, 0, 0, 0], (len(w), 1)))

    def test_the_arms_hang_at_the_sides(self):
        nodes = self.g["nodes"]
        parent = {c: i for i, n in enumerate(nodes) for c in n.get("children", [])}

        def world(i):
            n = nodes[i]
            m = np.identity(4)
            m[:3, :3] = mh2gltf.rotation_matrix(n.get("rotation", [0, 0, 0, 1]))
            m[:3, 3] = n.get("translation", [0, 0, 0])
            return world(parent[i]) @ m if i in parent else m

        index = {n["name"]: i for i, n in enumerate(nodes)}
        for side in "lr":
            shoulder = world(index[f"upperarm_{side}"])[:3, 3]
            elbow = world(index[f"lowerarm_{side}"])[:3, 3]
            hand = world(index[f"hand_{side}"])[:3, 3]
            for a, b in ((shoulder, elbow), (elbow, hand)):
                d = mh2gltf.normalize(b - a)
                self.assertLess(d[1], -0.95, f"{side}: {d}")

    def test_cloth_is_matte_and_only_brass_is_metal(self):
        for m in self.g["materials"]:
            pbr = m["pbrMetallicRoughness"]
            if m["name"] in ("brass_buttons", "kepi_buttons", "rastra_coins"):
                self.assertEqual(pbr["metallicFactor"], 1.0)
                continue
            self.assertEqual(pbr["metallicFactor"], 0.0, m["name"])
            if m["name"] in ("jacket", "trousers", "kepi_cloth"):
                self.assertGreaterEqual(pbr["roughnessFactor"], 0.8, m["name"])

    def test_the_uniform_colour_is_written_linear(self):
        # farach.mhm: jacket color=0.08,0.10,0.20 (sRGB), alpha untouched.
        jacket = self.g["materials"][self.materials()["jacket"]["material"]]
        factor = jacket["pbrMetallicRoughness"]["baseColorFactor"]
        np.testing.assert_allclose(factor[:3], [mh2gltf.linear(c) for c in (0.08, 0.10, 0.20)], atol=1e-5)
        self.assertEqual(factor[3], 1.0)

    def test_the_uniform_is_cut_from_the_body_with_cloth_textures(self):
        prims = self.materials()
        self.assertTrue({"jacket", "trousers", "boots"} <= set(prims))
        images = {self.g["images"][t["source"]]["uri"] for t in self.g["textures"]}
        self.assertTrue({"textures/wool.png", "textures/leather.png"} <= images)
        # The skin under the uniform is left out: fewer triangles than the
        # 3,140 of the whole body proxy.
        skin = self.prims()[0]
        self.assertLess(self.g["accessors"][skin["indices"]]["count"] // 3, 2200)


@unittest.skipUnless(HAVE_DATA, f"MakeHuman data not found at {DATA}")
class TownswomanConversion(unittest.TestCase):
    """Converts the shipped townswoman in a headscarf with the installed
    MakeHuman data: the women's pieces on a real female body."""

    @classmethod
    def setUpClass(cls):
        cls.tmp = tempfile.TemporaryDirectory()
        mhm = os.path.join(CHARACTERS, "townsfolk_headscarf", "townsfolk_headscarf.mhm")
        log = io.StringIO()
        with redirect_stdout(log), redirect_stderr(log):
            code = mh2gltf.main([mhm, "-o", cls.tmp.name, "--data", DATA, "--max-texture", "64", "--max-small-texture", "32", "--no-normal-maps"])
        if code != 0:
            raise AssertionError(log.getvalue())
        with open(os.path.join(cls.tmp.name, "model.gltf"), encoding="utf-8") as f:
            cls.g = json.load(f)
        with open(os.path.join(cls.tmp.name, "model.bin"), "rb") as f:
            cls.blob = f.read()

    @classmethod
    def tearDownClass(cls):
        cls.tmp.cleanup()

    def test_it_passes_its_check_within_the_townsfolk_budget(self):
        with redirect_stdout(io.StringIO()):
            self.assertEqual(mh2gltf.check(os.path.join(self.tmp.name, "model.gltf")), 0)
        prims = self.g["meshes"][0]["primitives"]
        triangles = sum(self.g["accessors"][p["indices"]]["count"] // 3 for p in prims)
        self.assertLessEqual(triangles, 11_000)
        names = {self.g["materials"][p["material"]]["name"] for p in prims}
        self.assertTrue({"shirt", "skirt", "apron", "shawl", "headscarf", "boots"} <= names, names)

    def test_the_skirt_goes_with_the_hips_at_the_waist_and_the_legs_below(self):
        joints = [self.g["nodes"][j]["name"] for j in self.g["skins"][0]["joints"]]
        prim = next(p for p in self.g["meshes"][0]["primitives"] if self.g["materials"][p["material"]]["name"] == "skirt")
        pos = read_accessor(self.g, self.blob, prim["attributes"]["POSITION"])
        jnt = read_accessor(self.g, self.blob, prim["attributes"]["JOINTS_0"])
        wgt = read_accessor(self.g, self.blob, prim["attributes"]["WEIGHTS_0"])

        def share(rows, prefix):
            on = np.array([[joints[j].startswith(prefix) for j in row] for row in jnt[rows]])
            return (wgt[rows] * on).sum(axis=1)

        top, hem = pos[:, 1] > pos[:, 1].max() - 0.02, pos[:, 1] < pos[:, 1].min() + 0.02
        self.assertLess(share(top, "thigh_").max(), 1e-6)
        self.assertGreater(share(hem, "thigh_").mean(), 0.2)
        back = hem & (pos[:, 2] < np.median(pos[:, 2]))
        self.assertGreater(share(back, "calf_").max(), 0.3)


TOWNSFOLK = ("paisano", "peon", "volga", "headscarf", "bun")
# What each outfit is made of (its .mhm's piece lines).
TOWNSFOLK_PIECES = {
    "paisano": {"shirt", "bombacha", "alpargatas", "sash", "neckerchief", "boina"},
    "peon": {"shirt", "waistcoat", "bombacha", "boots", "sash", "neckerchief", "hat"},
    "volga": {"shirt", "waistcoat", "trousers", "boots", "poncho", "hat"},
    "headscarf": {"shirt", "skirt", "apron", "shawl", "boots", "headscarf"},
    "bun": {"shirt", "collar", "skirt", "shawl", "boots", "hair", "bun"},
}


def tinted_garments():
    """The garments the game tints, per outfit, read from its table in
    crates/render/src/character/townsfolk.rs (the names must be the
    models' material names)."""
    import re
    path = os.path.join(HERE, "..", "crates", "render", "src", "character", "townsfolk.rs")
    with open(path, encoding="utf-8") as f:
        src = f.read()
    out = {}
    for name, const in zip(TOWNSFOLK, ("PAISANO", "PEON", "VOLGA", "HEADSCARF", "BUN")):
        block = src[src.index(f"const {const}: &[Garment] = &["):]
        out[name] = re.findall(r'\("(\w+)", \[', block[: block.index("];")])
    return out


class TownsfolkSources(unittest.TestCase):
    """The five townsfolk outfits as shipped: their sources parse with the
    pieces they are meant to wear, and their models keep the 11,000
    triangle cap and the materials the game tints. No MakeHuman data
    needed."""

    def test_every_outfit_wears_its_pieces_within_the_cap_and_its_tinted_cloth(self):
        tints = tinted_garments()
        for name in TOWNSFOLK:
            folder = os.path.join(CHARACTERS, f"townsfolk_{name}")
            c = mh2gltf.read_mhm(os.path.join(folder, f"townsfolk_{name}.mhm"))
            self.assertEqual({kind for kind, _ in c.pieces}, TOWNSFOLK_PIECES[name], name)
            self.assertEqual(c.ignored, [], name)
            with open(os.path.join(folder, "model.gltf"), encoding="utf-8") as f:
                g = json.load(f)
            prims = g["meshes"][0]["primitives"]
            triangles = sum(g["accessors"][p["indices"]]["count"] // 3 for p in prims)
            self.assertLessEqual(triangles, 11_000, name)
            self.assertGreater(triangles, 3_000, name)
            materials = {g["materials"][p["material"]]["name"] for p in prims}
            self.assertTrue(set(tints[name]) <= materials, f"{name}: {set(tints[name]) - materials}")
            # The women wear long skirts, the men none.
            self.assertEqual("skirt" in materials, name in ("headscarf", "bun"), name)

    def test_hair_worn_up_is_a_plain_colour_shell_over_the_head(self):
        # The stand-in's head ringed every 0.1 up to 18, so a hairline cuts it.
        skin = Units.Skin(heights=np.concatenate([np.arange(0.0, 16.5, 0.5), np.arange(16.5, 18.01, 0.1)]))
        figure = mh2gltf.Figure(skin.rig, skin.rest, skin)

        class Character:
            pieces = [("hair", {"color": [0.14, 0.10, 0.07]})]

        out, hide = mh2gltf.dress(Character(), figure, skin.rig)
        self.assertEqual(len(out), 1)
        hair = out[0]
        self.assertEqual(hair.material.pattern, None, "no cloth texture")
        self.assertEqual(hair.material.roughness, mh2gltf.ROUGHNESS[None])
        self.assertTrue(np.all(figure.limb[hair.source] == mh2gltf.HEAD))
        self.assertTrue(hair.triangles and hide)


@unittest.skipUnless(HAVE_DATA, f"MakeHuman data not found at {DATA}")
class BunConversion(unittest.TestCase):
    """Converts the young townswoman (hair worn up and a bun, a band collar
    with no wings, a skirt without an apron) and finds the shipped model's
    geometry and skin byte for byte. The other outfits take 8 to 17
    seconds each to convert; the headscarf woman is converted by
    TownswomanConversion."""

    def test_the_shipped_model_is_rebuilt_byte_for_byte(self):
        with tempfile.TemporaryDirectory() as out:
            mhm = os.path.join(CHARACTERS, "townsfolk_bun", "townsfolk_bun.mhm")
            log = io.StringIO()
            with redirect_stdout(log), redirect_stderr(log):
                code = mh2gltf.main([mhm, "-o", out, "--data", DATA, "--max-texture", "64", "--max-small-texture", "32", "--no-normal-maps"])
            self.assertEqual(code, 0, log.getvalue())
            with open(os.path.join(out, "model.bin"), "rb") as f, open(os.path.join(CHARACTERS, "townsfolk_bun", "model.bin"), "rb") as g:
                self.assertEqual(f.read(), g.read())
            with open(os.path.join(out, "model.gltf"), encoding="utf-8") as f:
                doc = json.load(f)
            hair = [m for m in doc["materials"] if m["name"] == "hair"]
            self.assertEqual(len(hair), 1)
            self.assertNotIn("baseColorTexture", hair[0]["pbrMetallicRoughness"])
