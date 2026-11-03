"""Tests of tools/recado.py: the profiles the recado is built from and its
maps, without Blender.

Needs numpy (skipped without it): ``python3 -I -B -m unittest discover -s tools``.
"""

import unittest

try:
    import numpy as np
except ImportError:  # pragma: no cover - numpy missing
    np = None
else:
    import recado as R


@unittest.skipIf(np is None, "needs numpy")
class Profiles(unittest.TestCase):
    def grid(self):
        t = np.linspace(R.COJINILLO_ALONG[0], R.COJINILLO_ALONG[1], 41)
        th = np.linspace(-R.COJINILLO_AROUND, R.COJINILLO_AROUND, 81)
        return np.meshgrid(t, th)

    def test_the_cojinillo_rests_on_the_carona_and_over_both_bastos(self):
        t, th = self.grid()
        inner, outer = R.cojinillo(t, th)
        self.assertTrue(np.all(inner >= R.UNDER - 1e-9), "never into the carona")
        self.assertTrue(np.all(outer - inner >= 0.004), "always some wool")
        # Over each basto the wool rises a little: the seat's two rolls, just
        # higher than the packed spine between them (the rider sits on them).
        spine = R.cojinillo(np.array(0.03), np.array(0.0))[1]
        for side in (-1, 1):
            roll = R.cojinillo(np.array(0.03), np.array(side * R.BASTO_THETA))[1]
            self.assertGreater(roll - spine, 0.01)
        # The ends and the hem curve down.
        self.assertLess(R.cojinillo(np.array(R.COJINILLO_ALONG[0]), np.array(0.0))[1], spine - 0.015)
        self.assertLess(R.cojinillo(np.array(0.03), np.array(R.COJINILLO_AROUND))[1], spine - 0.02)

    def test_the_bastos_fit_under_the_cojinillo_and_show_their_ends(self):
        t = np.linspace(R.COJINILLO_ALONG[0], R.COJINILLO_ALONG[1], 41)
        for side in (-1, 1):
            th = np.full_like(t, side * R.BASTO_THETA)
            top = R.UNDER + R.basto_envelope(t, th)
            inner, _ = R.cojinillo(t, th)
            self.assertTrue(np.all(top <= inner + 1e-9))
        # Longer than the cojinillo: their ends stick out front and back.
        self.assertLess(R.BASTO_ALONG[0], R.COJINILLO_ALONG[0] - 0.02)
        self.assertGreater(R.BASTO_ALONG[1], R.COJINILLO_ALONG[1] + 0.02)

    def test_the_sobrepuesto_lies_on_the_wool_and_the_seat_is_between_the_rolls(self):
        t, th = self.grid()
        inside = (np.abs(th) <= R.SOBREPUESTO_AROUND) & (t >= R.SOBREPUESTO_ALONG[0]) & (t <= R.SOBREPUESTO_ALONG[1])
        _, outer = R.cojinillo(t, th)
        sobre = R.sobrepuesto(t, th)
        self.assertTrue(np.all(sobre[inside] >= outer[inside] - 1e-9), "on the wool, never in it")
        seat = R.sobrepuesto(np.array(R.SEAT_X), np.array(0.0)) + R.SOBREPUESTO_THICK
        roll = R.sobrepuesto(np.array(R.SEAT_X), np.array(R.BASTO_THETA))
        self.assertGreater(roll, seat)
        self.assertTrue(0.13 < seat < 0.18, seat)


@unittest.skipIf(np is None, "needs numpy")
class Maps(unittest.TestCase):
    def test_the_atlas_regions_do_not_overlap_and_fill_the_image(self):
        regions = list(R.REGIONS.values())
        for i, a in enumerate(regions):
            for b in regions[i + 1 :]:
                overlap = (min(a[2], b[2]) - max(a[0], b[0]), min(a[3], b[3]) - max(a[1], b[1]))
                self.assertFalse(overlap[0] > 0 and overlap[1] > 0, (a, b))
        area = sum((r[2] - r[0]) * (r[3] - r[1]) for r in regions)
        self.assertGreater(area, 0.95)

    def test_the_fleece_is_curly_creamy_and_never_pure_white(self):
        m = R.atlas(256)
        u0, v0, u1, v1 = R.REGIONS["cojinillo"]
        c = m["color"][int(v0 * 256) : int(v1 * 256), int(u0 * 256) : int(u1 * 256)]
        self.assertLess(c.max(), 0.95, "never pure white")
        self.assertGreater(c[..., 0].mean() - c[..., 2].mean(), 0.05, "creamy")
        h = m["height"][int(v0 * 256) : int(v1 * 256), int(u0 * 256) : int(u1 * 256)]
        # Curls: the relief changes in both directions, small and everywhere.
        self.assertGreater(np.abs(np.diff(h, axis=0)).mean(), 0.02)
        self.assertGreater(np.abs(np.diff(h, axis=1)).mean(), 0.02)

    def test_the_bastos_have_stitching_and_tooled_ends(self):
        m = R.atlas(512)
        u0, v0, u1, v1 = R.REGIONS["basto_l"]
        h = m["height"][int(v0 * 512) : int(v1 * 512), int(u0 * 512) : int(u1 * 512)]
        across = h.mean(axis=1)
        along = h.mean(axis=0)
        # A seam groove down the middle of the roll and rings near both ends.
        mid = len(across) // 2
        self.assertLess(across[mid - 2 : mid + 3].min(), np.median(across) - 0.05)
        n = len(along)
        self.assertLess(along[: n // 8].min(), np.median(along) - 0.05)
        self.assertLess(along[-n // 8 :].min(), np.median(along) - 0.05)

    def test_the_wool_cards_hang_in_clumps_of_uneven_length(self):
        m = R.wool_atlas(256)
        a = m["opacity"]
        self.assertTrue(0.15 < (a > 0.5).mean() < 0.7)
        cols = R.WOOL_COLUMNS
        w = 256 // cols
        ends = []
        for c in range(cols):
            rows = np.nonzero((a[:, c * w : (c + 1) * w] > 0.5).any(axis=1))[0]
            ends.append(rows.max())
        self.assertGreater(np.ptp(ends), 256 * 0.15, "locks of different lengths")


if __name__ == "__main__":
    unittest.main()
