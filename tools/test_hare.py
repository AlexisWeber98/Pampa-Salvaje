"""Tests of tools/hare.py that run without Blender: the model's measures
against the literature, the coat painted on the sculpt (``coat_at``), the
gaits' plans and the skeleton the game's livestock path needs, and the
photographs' licences.

    python3 -I -B -m unittest discover -s tools
"""

import math
import unittest

import hare as H


def lum(rgb):
    return sum(rgb) / 3.0


def warm(rgb):
    """How much redder than blue (0..255)."""
    return rgb[0] - rgb[2]


# Points on the hare at rest (layout metres, facing -Y) with their outward
# normal.
BACK = ((0.0, 0.0, 0.272), (0.0, 0.0, 1.0))
FLANK = ((0.060, 0.0, 0.200), (1.0, 0.0, 0.1))
BELLY = ((0.0, -0.02, 0.110), (0.0, 0.0, -1.0))
CHEST = ((0.025, -0.175, 0.170), (0.3, -0.9, 0.0))


def mean_coat(point, n):
    """The coat's mean colour and grizzle over a patch about a centimetre
    across at ``point`` (the flecks averaged out)."""
    rgbs, gs = [], []
    for i in range(5):
        for j in range(5):
            q = (point[0], point[1] + 0.0023 * (i - 2), point[2] + 0.0023 * (j - 2))
            rgb, g = H.coat_at(q, n)
            rgbs.append(rgb)
            gs.append(g)
    return tuple(sum(c[k] for c in rgbs) / len(rgbs) for k in range(3)), sum(gs) / len(gs)


def ear_point(f, front):
    """A point ``f`` of the way up the left ear, on its hollow (``front``)
    or its back."""
    t, u, _ = H.head_coords(H._mix(H._ear(H.EAR_BASE, 1.0), H._ear(H.EAR_TIP, 1.0), f))
    n = H.ear_front(1.0)
    return H._at(t, u, 0.022), n if front else tuple(-c for c in n)


def mean_ear(f, front):
    point, n = ear_point(f, front)
    rgbs = [H.coat_at((point[0], point[1] + 0.0015 * (i - 2), point[2] + 0.0015 * (j - 2)), n)[0] for i in range(5) for j in range(5)]
    return tuple(sum(c[k] for c in rgbs) / len(rgbs) for k in range(3))


def ear_across_point(f, across):
    """A point on the back of the left ear, ``f`` up it and ``across`` its
    half width toward the outer edge."""
    base, mid, tip = (H._ear(p, 1.0) for p in (H.EAR_BASE, H.EAR_MID, H.EAR_TIP))
    a, b = (base, mid) if f < 0.5 else (mid, tip)
    h = f / 0.5 if f < 0.5 else (f - 0.5) / 0.5
    c = tuple(a[i] + (b[i] - a[i]) * h for i in range(3))
    ab = tuple(b[i] - a[i] for i in range(3))
    fr = H.ear_front(1.0)
    side = H._norm((fr[1] * ab[2] - fr[2] * ab[1], fr[2] * ab[0] - fr[0] * ab[2], fr[0] * ab[1] - fr[1] * ab[0]))
    if side[0] < 0:
        side = tuple(-v for v in side)
    lo, hi = H.EAR_SECTION[2], H.EAR_SECTION[3]  # 0.45 and 0.75 of the way up
    w = lo[1] + (hi[1] - lo[1]) * (f - 0.45) / 0.30
    return tuple(c[i] + side[i] * w * across for i in range(3))


class Measures(unittest.TestCase):
    def test_the_model_is_a_european_hare_of_life_size(self):
        for name, value in (("head_body", H.spine_length()), ("ear", H.ear_notch_length()), ("hind_foot", H.hind_foot_length()),
                            ("tail", H.tail_length())):
            lo, hi = H.SIZE[name]
            self.assertTrue(lo <= value <= hi, (name, round(value, 3), (lo, hi)))


class Coat(unittest.TestCase):
    def test_the_chest_is_tawny_not_orange(self):
        import colorsys

        chest, _ = mean_coat(*CHEST)
        h, sat, v = colorsys.rgb_to_hsv(*(c / 255 for c in chest))
        self.assertLess(sat, 0.50, chest)  # tawny, not the orange of v5 (0.6)
        self.assertLess(warm(chest), 75, chest)

    def test_the_back_is_agouti_the_flanks_warmer_the_chest_rufous_and_the_belly_white(self):
        back, g_back = mean_coat(*BACK)
        flank, g_flank = mean_coat(*FLANK)
        chest, g_chest = mean_coat(*CHEST)
        belly, g_belly = mean_coat(*BELLY)
        self.assertGreater(g_back, 0.5)
        self.assertGreater(g_back, g_chest)  # the rufous chest little grizzled
        self.assertGreater(warm(chest), warm(back) + 15, (chest, back))
        self.assertGreater(warm(flank), warm(back), (flank, back))
        self.assertGreater(warm(chest), warm(flank) + 12, (chest, flank))
        self.assertGreater(min(belly), 210, belly)
        self.assertEqual(g_belly, 0.0)

    def test_the_back_is_dark_agouti_and_the_flank_greyer_than_the_chest(self):
        back, _ = mean_coat(*BACK)
        flank, _ = mean_coat(*FLANK)
        chest, _ = mean_coat(*CHEST)
        self.assertLess(lum(back), lum(flank) - 15, (back, flank))  # the back the darkest agouti
        self.assertLess(lum(flank), lum(chest), (flank, chest))

    def test_the_trunks_coat_is_as_light_as_the_photographs_hare_de_lit(self):
        import math

        def linear(c):
            c /= 255.0
            return c / 12.92 if c <= 0.04045 else ((c + 0.055) / 1.055) ** 2.4

        values = []
        for i in range(9):
            for a in range(0, 100, 15):  # from the back down the flank
                r = math.radians(a)
                n = (math.sin(r), 0.0, math.cos(r))
                for k in range(12):
                    q = (0.055 * n[0], -0.10 + 0.03 * i + 0.0013 * k, 0.20 + 0.07 * n[2] + 0.0009 * k)
                    rgb, _ = H.coat_at(q, n)
                    values.append(0.2126 * linear(rgb[0]) + 0.7152 * linear(rgb[1]) + 0.0722 * linear(rgb[2]))
        mean = sum(values) / len(values)
        self.assertTrue(0.85 * H.COAT_ALBEDO < mean < 1.15 * H.COAT_ALBEDO, mean)  # not v6c's chocolate 0.07

    def test_the_agouti_coat_is_flecked_dark_and_pale_but_the_belly_is_not(self):
        def spread(point, n):
            values = [lum(H.coat_at((point[0] + 0.0007 * k, point[1] + 0.0011 * k, point[2]), n)[0]) for k in range(40)]
            return max(values) - min(values)

        self.assertGreater(spread(*BACK), 40)
        self.assertLess(spread(*BELLY), 3)

    def test_the_ears_are_brown_grey_behind_darker_in_the_hollow_and_black_only_at_the_tip(self):
        mid_back = mean_ear(0.5, False)
        mid_in = mean_ear(0.5, True)
        tip_back = mean_ear(0.92, False)
        self.assertLess(lum(tip_back), 50, tip_back)
        self.assertTrue(90 < lum(mid_back) < 140, mid_back)
        self.assertGreater(warm(mid_back), 8, mid_back)  # brown grey, not the pale grey paddles of v5
        self.assertLess(lum(mid_in), lum(mid_back) - 10, (mid_in, mid_back))
        # Black only at the tip: the outer edge lower down not black, the
        # front edge fringed pale.
        n = tuple(-c for c in H.ear_front(1.0))
        self.assertAlmostEqual(H.ear_across(ear_across_point(0.6, 0.95)), 0.95, delta=0.05)

        def along(across, f0):  # the mean over a strip up the ear (the flecks averaged out)
            return sum(lum(H.coat_at(ear_across_point(f0 + 0.005 * k, across), n)[0]) for k in range(20)) / 20

        self.assertGreater(along(0.95, 0.70), along(0.0, 0.70) - 15)
        self.assertGreater(along(0.95, 0.35), along(0.0, 0.35) - 15)
        self.assertGreater(along(-0.95, 0.45), along(0.0, 0.45) + 30)

    def test_the_tail_is_black_on_top_and_white_below(self):
        # The tail lies down the rump: its top faces back, its underside the rump.
        a, b = H.TAIL[1], H.TAIL[2]
        mid = tuple((a[i] + b[i]) / 2 for i in range(3))
        ax = H._norm(tuple(b[i] - a[i] for i in range(3)))
        up = (0.0, -ax[2], ax[1]) if ax[1] > 0 else (0.0, ax[2], -ax[1])
        top = H.coat_at(tuple(mid[i] + 0.016 * up[i] for i in range(3)), up)[0]
        under = H.coat_at(tuple(mid[i] - 0.016 * up[i] for i in range(3)), tuple(-c for c in up))[0]
        self.assertLess(lum(top), 50, top)
        self.assertGreater(lum(under), 210, under)

    def test_a_narrow_pale_ring_round_the_eye_paler_than_the_cheek(self):
        t, u, s = H.EYE
        ring = H._at(t - (H.EYE_RADIUS + H.EYE_RING) / 2, u, s)  # behind the eye, clear of the streak below it
        beyond = H._at(t - H.EYE_RADIUS - 0.0045, u, s)  # 4.5 mm out from the eye's edge
        cheek = H._at(t + 0.010, u - 0.034, s + 0.002)
        n = (1.0, 0.0, 0.0)
        self.assertGreater(lum(H.coat_at(ring, n)[0]), lum(H.coat_at(cheek, n)[0]) + 40)
        self.assertLess(lum(H.coat_at(beyond, n)[0]), lum(H.coat_at(ring, n)[0]) - 40)  # a narrow ring, not a wide pale patch


class Proportions(unittest.TestCase):
    def test_the_side_view_has_the_photographs_proportions(self):
        # A short deep trunk, a large long head, ears longer than the head.
        got = H.proportions()
        for name, (lo, hi) in H.PHOTO_RATIOS.items():
            self.assertTrue(lo <= got[name] <= hi, (name, round(got[name], 3), (lo, hi)))

    def test_the_thigh_is_a_deep_plate_standing_out_of_the_tucked_flank(self):
        # v6c's hind leg was a stick under a ball: the thigh must stand out of
        # the haunch by 2 cm, be deep (nearly a quarter of the nose-to-rump
        # length, the photographs and the reference sculpt) and its lower part
        # hang below the tucked flank.
        length = H.SIDE_RUMP[1] - H.warp(H._at(H.NOSE_T, -0.030))[1]
        for i, ((x, y, z), w, front, back) in enumerate(H.HIND_RINGS[1:4]):
            trunk = [r for r in H.TRUNK_RINGS if abs(r[0][0] - y) < 0.02]
            self.assertGreater(x + w - max(r[2] for r in trunk), 0.020, y)
            if i > 0:
                self.assertLess(z - back * 0.6, min(r[1][1] for r in trunk), y)
        self.assertGreater(max(f + b for _, _, f, b in H.HIND_RINGS[1:4]) / length, 0.23)
        # The hamstrings end inside the calf, not hanging behind the shank.
        end = H.SEAT_RINGS[-1][0]
        mids = [((x, y + (b - f) / 2, z), (f + b) / 2) for (x, y, z), w, f, b in H.HIND_RINGS[4:9]]
        self.assertLess(min(math.dist(m, end) - r for m, r in mids), 0.0)

    def test_the_haunch_is_one_mass_whose_back_runs_down_to_the_heel(self):
        # v7d: a lobe hung off the back of the thigh above the hock, like a
        # kangaroo's tail. Seen from the side the haunch's back edge must
        # come down from the rump to the heel without turning back out, and
        # end at the heel, not behind it.
        parts = [H.leg_cage(H.SEAT_RINGS), H.leg_cage(H.HIND_RINGS), H.trunk_cage()]
        heel = H.warp(H.HEEL)
        levels = [heel[2] + 0.010 + 0.005 * i for i in range(int((0.19 - heel[2]) / 0.005) + 1)]
        backs = [side_rear(parts, z) for z in levels]
        for lower, upper, z in zip(backs, backs[1:], levels):
            self.assertLessEqual(lower, upper + 0.001, z)
        self.assertLess(backs[0], heel[1] + 0.015)

    def test_the_waist_is_narrow_and_tucked_up_between_the_chest_and_the_haunch(self):
        # v7d's trunk was a barrel as wide at the waist as at the chest.
        def ring(y):
            return min(H.TRUNK_RINGS, key=lambda r: abs(r[0][0] - y))
        chest = max(r[2] for r in H.TRUNK_RINGS if -0.07 < r[0][0] < -0.02)
        waist = ring(0.045)
        haunch = max(x + w for (x, y, z), w, f, b in H.HIND_RINGS[1:4])
        self.assertLess(waist[2], 0.8 * chest)
        self.assertLess(waist[2], 0.5 * haunch)
        keel = min(r[1][1] for r in H.TRUNK_RINGS if -0.10 < r[0][0] < 0.0)
        self.assertGreater(waist[1][1] - keel, 0.06)  # the belly tucked up behind the ribs


def side_rear(parts, z):
    """How far back (model y) the cages reach at height ``z``, seen from
    the side: each pair of rings taken as its convex hull."""
    best = -1.0
    for rings in parts:
        for a, b in zip(rings, rings[1:]):
            pts = sorted({(p[1], p[2]) for p in a + b})
            lo, up = [], []
            for p in pts:
                while len(lo) >= 2 and (lo[-1][0] - lo[-2][0]) * (p[1] - lo[-2][1]) - (lo[-1][1] - lo[-2][1]) * (p[0] - lo[-2][0]) <= 0:
                    lo.pop()
                lo.append(p)
            for p in reversed(pts):
                while len(up) >= 2 and (up[-1][0] - up[-2][0]) * (p[1] - up[-2][1]) - (up[-1][1] - up[-2][1]) * (p[0] - up[-2][0]) <= 0:
                    up.pop()
                up.append(p)
            hull = lo[:-1] + up[:-1]
            for (y0, z0), (y1, z1) in zip(hull, hull[1:] + hull[:1]):
                if z0 != z1 and (z0 - z) * (z1 - z) <= 0:
                    best = max(best, y0 + (y1 - y0) * (z - z0) / (z1 - z0))
    return best


class Eye(unittest.TestCase):
    def test_a_large_dark_pupil_in_an_amber_iris_with_a_dark_rim(self):
        import colorsys
        import math

        rim, pupil = H.IRIS
        iris_r, pupil_r = math.sqrt(1 - rim * rim), math.sqrt(1 - pupil * pupil)
        self.assertTrue(0.42 <= pupil_r / iris_r <= 0.6, pupil_r / iris_r)  # the pupil large in the iris (about half), the iris a band round it
        self.assertLess(iris_r, 0.85)  # the rim shows dark round the iris inside the lids
        h, sat, v = colorsys.rgb_to_hsv(*H.FALLBACK["Eye_Iris"])
        self.assertTrue(20 <= h * 360 <= 45, h * 360)  # amber, golden brown
        self.assertLess(v, 0.6)  # not the bright orange disc of v5
        self.assertLess(max(H.FALLBACK["Eye_Black"]), 0.05)

    def test_the_iris_is_painted_darker_at_its_collar_and_rim_streaked_with_a_catchlight(self):
        # v7d's iris was one flat orange: the photographs' is darker round
        # the pupil and at its outer edge, streaked, with the sky in the cornea.
        pupil = math.degrees(math.acos(H.IRIS[1]))
        rim = math.degrees(math.acos(H.IRIS[0]))

        def lum_at(f, phi=200.0):
            return lum(H.eye_colour(pupil + (rim - pupil) * f, phi))

        self.assertLess(lum_at(0.05), 0.8 * lum_at(0.45))
        self.assertLess(lum_at(0.97), 0.8 * lum_at(0.45))
        self.assertLess(lum(H.eye_colour(rim + 4.0, 200.0)), 0.4 * lum_at(0.45))
        ring = [lum(H.eye_colour(pupil + (rim - pupil) * 0.45, 11.25 * k)) for k in range(32)]
        self.assertGreater(max(ring) - min(ring), 15.0)
        self.assertGreater(min(H.eye_colour(*H.CATCHLIGHT[:2])), 220)
        self.assertLess(max(H.eye_colour(pupil * 0.5, H.CATCHLIGHT[1] + 180.0)), 20)


class Gaits(unittest.TestCase):
    def test_the_hop_is_half_a_metre_a_stride_its_hinds_landing_beside_the_fores(self):
        stride = H.HOP_SPEED * H.HOP["seconds"]
        self.assertAlmostEqual(stride, 0.5, delta=0.01)  # the game's Hare::lift: one hop every 0.5 m
        p = H.prints(H.HOP, H.HOP_SPEED)
        fores = (p["LF"], p["RF"])
        for hind in ("LH", "RH"):
            self.assertLess(min(abs(p[hind] - f) for f in fores), 0.06, p)

    def test_in_the_flight_the_hinds_land_far_ahead_of_the_fores_prints(self):
        self.assertAlmostEqual(1.0 / H.FLIGHT["seconds"], 4.0, delta=0.3)  # strides a second at 11 m/s, a 4 kg hare
        p = H.prints(H.FLIGHT, H.FLIGHT_SPEED)
        for hind in ("LH", "RH"):
            self.assertLess(p[hind], min(p["LF"], p["RF"]) - 0.30, p)
        # Each foot stands for a few hundredths of a second.
        for leg in ("LF", "LH"):
            stance = H.FLIGHT["duty"][0 if leg[1] == "F" else 1] * H.FLIGHT["seconds"]
            self.assertLess(stance, 0.05, leg)


class Skeleton(unittest.TestCase):
    def test_the_livestock_paths_joint_names_and_the_hares_own(self):
        names = H.joint_names()
        self.assertEqual(len(names), len(set(names)))
        for name in ["Body", "Back", "Torso", "Torso2", "Torso3", "Neck1", "Neck2", "Neck3", "Head", "Jaw"] + [
                f"{b}.{s}" for s in ("L", "R") for b in ("FrontShoulder", "FrontUpperLeg", "FrontLowerLeg", "BackShoulder", "BackLeg",
                                                         "BackUpperLeg", "BackLowerLeg", "IKBackLeg", "IKFrontLeg", "Ear1", "Ear2")]:
            self.assertIn(name, names)
        self.assertEqual([n for n in names if n.startswith("Tail")], ["Tail1", "Tail2", "Tail3"])


class Ears(unittest.TestCase):
    def test_each_ear_rises_from_a_rolled_tube_into_a_thin_blade(self):
        base_t, base_w = H.EAR_SECTION[0]
        self.assertGreater(base_t / base_w, 0.8)  # round at the base
        widest = max(H.EAR_SECTION, key=lambda s: s[1])
        self.assertLess(widest[0] / widest[1], 0.25)
        self.assertGreater(2 * widest[0], 0.006)  # but not a sheet: some thickness


    def test_from_the_side_both_ears_show_and_each_is_the_literatures_length(self):
        tips = [H.warp(H._ear(H.EAR_TIP, sx)) for sx in (1.0, -1.0)]
        self.assertGreater(abs(tips[0][1] - tips[1][1]), 0.02)  # apart along the body at rest
        lo, hi = H.SIZE["ear"]
        for sx in (1.0, -1.0):
            for p in (H.EAR_MID, H.EAR_TIP):  # each ear grows whole with the head, however it leans
                q = H._ear(p, sx)
                self.assertGreater(H._head_w(q[1], q[2]), 0.99, (sx, p))
            self.assertTrue(lo <= H.ear_notch_length(sx) <= hi, (sx, H.ear_notch_length(sx)))


class Head(unittest.TestCase):
    def test_in_the_alert_sit_the_chest_front_lies_near_the_vertical_through_the_ears(self):
        # Hałun 5163: the head over the chest, the breast near the ears' vertical.
        got = H.alert_front()
        self.assertTrue(0.0 < got["chest"] < 0.10, got)
        self.assertTrue(0.10 < got["eye"] < 0.18 and 0.22 < got["nose"] < 0.34, got)

    def test_the_head_is_long_tapering_to_the_nose_with_the_eye_half_way(self):
        first, last = H.HEAD_RINGS[0][0], H.HEAD_RINGS[-1][0]
        self.assertTrue(0.4 < (H.EYE[0] - first) / (last - first) < 0.65)
        widest = max(r[3] for r in H.HEAD_RINGS)
        muzzle = min(H.HEAD_RINGS, key=lambda r: abs(r[0] - (H.NOSE_T - 0.010)))
        self.assertLess(muzzle[3], 0.55 * widest)  # a tapering muzzle, not a box
        self.assertLess(muzzle[1] - muzzle[2], 0.55 * max(r[1] - r[2] for r in H.HEAD_RINGS))


class Photos(unittest.TestCase):
    def test_projected_pixels_come_only_from_free_credited_photos(self):
        by_file = {f: (a, lic, use) for f, a, lic, use in H.PHOTOS}
        for spec in H.PROJECTIONS:
            author, licence, use = by_file[spec["file"]]
            self.assertEqual(use, "pixels", spec["file"])
            self.assertIn(licence, H.FREE, spec["file"])
        used = {spec["file"] for spec in H.PROJECTIONS}
        for f, _, _, use in H.PHOTOS:
            self.assertEqual(use == "pixels", f in used, f)

    def test_the_photo_lands_on_its_landmarks_and_never_past_70_degrees(self):
        import math

        for spec in H.PROJECTIONS:
            model = [tuple(H.warp((0.0, y, z))[1:]) for (y, z), _ in spec["landmarks"]]
            got = H.fit_mapping(model, [uv for _, uv in spec["landmarks"]])(model)
            for (_, uv), g in zip(spec["landmarks"], got):
                self.assertLess(math.dist(uv, g), 0.5, spec["region"])
        self.assertEqual(H.photo_weight(math.cos(math.radians(70.0))), 0.0)
        self.assertEqual(H.photo_weight(math.cos(math.radians(80.0))), 0.0)
        self.assertEqual(H.photo_weight(math.cos(math.radians(50.0))), 1.0)


    def test_only_free_photos_may_give_pixels_and_every_photo_is_credited(self):
        for file, author, licence, use in H.PHOTOS:
            self.assertTrue(file and author and licence, file)
            self.assertIn(use, ("reference", "pixels"), file)
            if use == "pixels":
                self.assertIn(licence, H.FREE, file)


if __name__ == "__main__":
    unittest.main()
