"""Tests of tools/henry_coats.py.

numpy: ``python3 -I -B -m unittest discover -s tools -p test_henry_coats.py``.
The coat tests paint a stand-in horse (the one the game's Rust tests used
for the vertex coats this tool replaced).
"""

import unittest

import numpy as np

import henry_coats as hc


def coat_points():
    """A stand-in horse's surface in the body frame: its barrel, croup and
    shoulders (a tube along +X about 1.14 m up), its neck and its four legs,
    each point with its outward normal."""
    p, n = [], []
    for i in range(40):
        x = -0.85 + 1.45 * i / 39
        for k in range(32):
            a = k / 32 * 2 * np.pi
            nn = np.array([0.0, np.cos(a), np.sin(a)])
            p.append([x, 1.14 + 0.29 * nn[1], 0.23 * nn[2]])
            n.append(nn)
    for i in range(20):
        t = i / 19
        c = np.array([0.5 + 0.45 * t, 1.3 + 0.45 * t, 0.0])
        for k in range(16):
            a = k / 16 * 2 * np.pi
            nn = np.array([-0.7 * np.cos(a), 0.7 * np.cos(a), np.sin(a)])
            nn /= np.linalg.norm(nn)
            p.append(c + nn * 0.13)
            n.append(nn)
    for x, z in ((0.55, -0.17), (0.55, 0.17), (-0.65, -0.17), (-0.65, 0.17)):
        for j in range(20):
            for k in range(10):
                a = k / 10 * 2 * np.pi
                nn = np.array([np.cos(a), 0.0, np.sin(a)])
                p.append(np.array([x, 0.9 * j / 19, z]) + nn * 0.06)
                n.append(nn)
    return np.array(p), np.array(n)


P, N = coat_points()


def colours(coat):
    return hc.coat_rgb(coat, P, N) * 255


def luma(c):
    return 0.3 * c[..., 0] + 0.59 * c[..., 1] + 0.11 * c[..., 2]


class Coats(unittest.TestCase):
    def test_every_coat_looks_different(self):
        up = P[:, 1] > hc.POINTS_HEIGHT + 0.1
        means = {c: colours(c)[up].mean(0) for c in hc.COATS}
        names = list(means)
        for i, a in enumerate(names):
            for b in names[i + 1:]:
                d = np.linalg.norm(means[a] - means[b])
                self.assertGreater(d, 20, f"{a} vs {b}")

    def test_every_coat_has_depth_a_darker_topline_lighter_underside_and_mottling(self):
        barrel = (P[:, 0] > -0.6) & (P[:, 0] < 0.4) & (P[:, 1] > 0.9)
        for coat in hc.COATS:
            c = colours(coat)
            white = np.all(np.abs(c - np.array(hc.WHITE)) < 2, axis=1)
            keep = barrel & ~white
            if coat == "tordillo":
                keep &= hc.dapples(P) < 0.01
            lum = luma(c[keep])
            n = N[keep]
            spread = (np.percentile(lum, 95) - np.percentile(lum, 5)) / lum.mean()
            top, belly = lum[n[:, 1] > 0.8].mean(), lum[n[:, 1] < -0.6].mean()
            self.assertGreater(spread, 0.15, f"{coat}: a flat coat")
            self.assertLess(top, belly * 0.88, f"{coat}: topline {top}, belly {belly}")

    def test_the_bayo_has_black_points_and_a_dorsal_stripe(self):
        c = colours("bayo")
        legs = P[:, 1] < 0.35
        self.assertTrue((luma(c[legs]) < 60).all(), "black legs")
        flank = (P[:, 1] > 0.9) & (np.abs(N[:, 2]) > 0.8) & (P[:, 0] < 0.45)
        self.assertGreater(flank.sum(), 20)
        self.assertTrue((luma(c[flank]) > 140).all(), "sandy")
        stripe = (N[:, 1] > 0.8) & (np.abs(P[:, 2]) < 0.02) & (P[:, 0] > -0.6) & (P[:, 0] < 0.2)
        self.assertGreaterEqual(stripe.sum(), 3)
        self.assertTrue((luma(c[stripe]) < 100).all(), "a dark stripe")
        # The alazán's topline is only shaded, never striped.
        a = colours("alazan")
        self.assertGreater(luma(a[stripe]).min(), luma(c[stripe]).max())

    def test_the_overo_shows_white_patches_from_every_three_quarter_view(self):
        c = colours("overo")
        for x, z in ((1, 1), (1, -1), (-1, 1), (-1, -1)):
            toward = np.array([x, 0.6, z]) / np.linalg.norm([x, 0.6, z])
            seen = (N @ toward > 0.3) & (P[:, 1] > hc.POINTS_HEIGHT + 0.1)
            share = (luma(c[seen]) > 200).mean()
            self.assertTrue(0.08 < share < 0.7, f"from ({x}, {z}): {share}")
        # White stockings over pale hooves on the left fore (x > 0, z < 0)
        # and the right hind only.
        sole = np.array([[0.55, 0.05, -0.17], [-0.65, 0.05, 0.17], [0.55, 0.05, 0.17], [-0.65, 0.05, -0.17]])
        hooves = hc.coat_rgb("overo", sole, np.tile([1.0, 0, 0], (4, 1))) * 255
        sock = np.array([[0.55, 0.3, -0.17], [-0.65, 0.3, 0.17], [0.55, 0.3, 0.17], [-0.65, 0.3, -0.17]])
        socks = luma(hc.coat_rgb("overo", sock, np.tile([1.0, 0, 0], (4, 1))) * 255)
        self.assertTrue((socks[:2] > 200).all() and (socks[2:] < 150).all(), socks)
        self.assertGreater(luma(hooves[:2]).min(), luma(hooves[2:]).max())

    def test_the_tordillo_is_dappled(self):
        c = colours("tordillo")
        barrel = (P[:, 1] > 0.9) & (P[:, 0] < 0.6)
        lum = luma(c[barrel])
        light, dark = (lum > 140).mean(), (lum < 115).mean()
        self.assertTrue(light > 1 / 7 and dark > 1 / 7, f"{light} light, {dark} dark")


class FurDetail(unittest.TestCase):
    def test_the_pinto_goes_and_the_fur_and_the_pink_skin_stay(self):
        # Henry's map in small: the left half white hair, the right half
        # brown, both with the same fine pattern, and a pink patch.
        size = 96
        rng = np.random.default_rng(1)
        grain = 1 + 0.2 * rng.standard_normal((size, size))
        img = np.empty((size, size, 3))
        img[:, : size // 2] = np.array([0.56, 0.54, 0.53])
        img[:, size // 2 :] = np.array([0.30, 0.18, 0.12])
        img = np.clip(img * grain[..., None] ** 0.5, 0, 1)
        img[70:80, 70:80] = [0.85, 0.5, 0.55]
        detail, pink = hc.fur_detail(img, radius=8, edge_radius=2)
        left, right = detail[:60, 8:40], detail[:60, 56:88]
        # The same level on both sides: the white patch is gone.
        self.assertAlmostEqual(left.mean(), right.mean(), delta=0.05)
        self.assertAlmostEqual(left.mean(), 1.0, delta=0.08)
        # The fine pattern stays.
        self.assertGreater(left.std(), 0.05)
        self.assertGreater(np.corrcoef(left.ravel(), grain[:60, 8:40].ravel())[0, 1], 0.8)
        # The pink skin is kept as it is; the hair is not.
        self.assertGreater(pink[73:77, 73:77].min(), 0.95)
        self.assertLess(pink[:60].max(), 0.05)
        out = hc.coat_map("zaino", np.zeros((size, size, 3)), np.tile([0, 1.0, 0], (size, size, 1)), detail, pink, img)
        self.assertTrue(np.allclose(out[74, 74], img[74, 74], atol=0.01))


class Geometry(unittest.TestCase):
    def test_texels_get_their_point_and_normal_and_the_margin_is_filled(self):
        # One quad over the left half of the UVs, a metre square at x = 0..1.
        pos = np.array([[0, 0, 0], [1, 0, 0], [1, 1, 0], [0, 1, 0]], float)
        uv = np.array([[0, 0], [0.5, 0], [0.5, 1], [0, 1]], float)
        tris = np.array([[0, 1, 2], [0, 2, 3]])
        nrm = hc.vertex_normals(pos, tris)
        self.assertTrue(np.allclose(nrm, [0, 0, 1]))
        P, N, covered = hc.geometry(pos, nrm, uv, tris, size=32)
        self.assertTrue(covered[:, :16].all() and not covered[:, 17 + hc.MARGIN :].any())
        # Texel (row 8, column 4) is at uv (4.5/32, 8.5/32).
        self.assertTrue(np.allclose(P[8, 4], [4.5 / 16, 8.5 / 32, 0]))
        self.assertTrue(np.allclose(N[8, 4], [0, 0, 1]))
        # The margin takes the edge's point; beyond it, nothing.
        self.assertTrue(np.allclose(P[8, 16 + hc.MARGIN - 1, 0], 15.5 / 16))
        self.assertTrue(np.allclose(P[8, 16 + hc.MARGIN + 2], 0))


if __name__ == "__main__":
    unittest.main()
