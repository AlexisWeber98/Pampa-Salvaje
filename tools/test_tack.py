"""Tests of tools/tack.py: the procedural map sets of the recado and the
bridle, made small so they run in a second.

Needs numpy (skipped without it): ``python3 -I -B -m unittest discover -s tools``.
"""

import unittest

try:
    import numpy as np
except ImportError:  # pragma: no cover - numpy missing
    np = None
else:
    import tack as T


@unittest.skipIf(np is None, "needs numpy")
class TackMaps(unittest.TestCase):
    def test_every_set_has_colour_normal_and_roughness_at_its_size(self):
        for name in T.SETS:
            m = T.SETS[name](64)
            for key in ("color", "normal_gl", "roughness"):
                self.assertIn(key, m, name)
                self.assertEqual(m[key].shape[:2], (64, 64), (name, key))
            self.assertTrue(m["color"].std() > 0.02, f"{name} is not a flat colour")

    def test_noise_and_normals_wrap_round_the_edges(self):
        h = T.fbm(64, 4, seed=3)
        # The first column follows on from the last as smoothly as any two
        # neighbouring columns do.
        inner = np.abs(np.diff(h, axis=1)).mean()
        seam = np.abs(h[:, 0] - h[:, -1]).mean()
        self.assertLess(seam, 2.5 * inner)
        n = T.normals(h, 4.0)
        length = np.linalg.norm(n * 2.0 - 1.0, axis=2)
        self.assertTrue(np.all(np.abs(length - 1.0) < 0.02))
        self.assertTrue(np.all(n[..., 2] > 0.5), "normals face out")

    def test_the_matra_has_a_coloured_guard_at_its_lower_edges_and_the_bajera_stripes(self):
        m = T.blankets(256)["color"]
        matra = m[:, 128:]
        # The guarda: saturated red and indigo near both lower edges (the rows
        # at the ends hang on the flanks), not in the hidden middle.
        sat = matra.max(axis=2) - matra.min(axis=2)
        # The hidden middle: away from the edges and the front and back stripes.
        middle = sat[100:156, 40:-40].mean()
        self.assertGreater(sat[:24].mean(), 2.0 * middle)
        self.assertGreater(sat[-24:].mean(), 2.0 * middle)
        bajera = m[:, :128].mean(axis=(1, 2))
        # Stripes along the horse: the row brightness changes in bands.
        self.assertGreater(np.abs(np.diff(bajera[:40])).max(), 0.1)

    def test_the_matra_frays_into_a_fringe_at_its_front_and_back(self):
        m = T.blankets(256)
        a = m["opacity"]
        matra = a[:, 128:]
        # Opaque everywhere but the warp fringe at its two ends, where threads
        # of different lengths leave gaps.
        self.assertTrue(np.all(a[:, :128] > 0.99), "the bajera is whole")
        self.assertTrue(np.all(matra[:, 20:-20] > 0.99))
        for end in (matra[:, :5], matra[:, -5:]):
            self.assertTrue(0.2 < (end > 0.5).mean() < 0.8, (end > 0.5).mean())

    def test_the_carpincho_is_pebbled_with_darker_edges(self):
        c = T.carpincho(128)["color"].mean(axis=2)
        edge = np.concatenate([c[:3].ravel(), c[-3:].ravel(), c[:, :3].ravel(), c[:, -3:].ravel()])
        self.assertLess(edge.mean(), 0.85 * c[30:-30, 30:-30].mean())

    def test_the_cincha_is_one_band_with_a_cross_woven_panel(self):
        whole = T.rawhide(256)["height"]
        # The cinchón's plain strip (the third quarter) has no keepers: the
        # same profile all the way down.
        plain = whole[:, 128:192]
        self.assertLess(np.abs(np.diff(plain, axis=0)).mean(), 0.02)
        h = whole[:, 192:]
        # Along the strips the relief barely changes outside the panel; across
        # the panel the weft crosses them.
        across = np.abs(np.diff(h, axis=0)).mean(axis=1)
        panel = (across > 3.0 * np.median(across)).mean()
        self.assertTrue(0.15 < panel < 0.6, panel)

    def test_the_braid_repeats_down_the_strap(self):
        m = T.rawhide(128)
        h = m["height"][:, :64]
        # Tiles in v: the first row continues the last.
        self.assertLess(np.abs(h[0] - h[-1]).mean(), 2.5 * np.abs(np.diff(h, axis=0)).mean())
        # A chevron: each row's profile shifts along the strap.
        self.assertGreater(np.abs(h[0] - h[3]).mean(), 0.05)

    def test_the_fleece_has_locks_with_gaps_for_its_alpha_cards(self):
        m = T.fleece(128)
        cover = (m["opacity"] > 0.5).mean()
        self.assertTrue(0.45 < cover < 0.85, cover)

    def test_the_iron_is_dark_with_rust(self):
        c = T.iron(64)["color"]
        self.assertLess(c.mean(), 0.4)
        rust = (c[..., 0] - c[..., 2]) > 0.06
        self.assertTrue(0.03 < rust.mean() < 0.5, rust.mean())


if __name__ == "__main__":
    unittest.main()
