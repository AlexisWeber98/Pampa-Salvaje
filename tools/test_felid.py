"""Tests of tools/felid.py that run without Blender: the three species'
build (``warp``), the trunk's profile, the coats painted on the sculpt
(``coat_at``), the colours of the cards' hairs, the gaits' ground speeds
and the skeleton the game's livestock path needs.

    python3 -I -B -m unittest discover -s tools
"""

import unittest

import felid as F

SPECIES = ("gato", "puma", "yagua")


def lum(rgb):
    return sum(rgb) / 3.0


def model_z(z):
    return F.SP["S"] * F._wz(z)


class Species(unittest.TestCase):
    def tearDown(self):
        F.configure("puma")

    def test_each_species_stands_at_its_size(self):
        # The spec: a gato montés of 25-30 cm at the shoulder, a puma and a
        # yaguareté of 65-75 cm.
        for name, lo, hi in (("gato", 0.25, 0.30), ("puma", 0.65, 0.75), ("yagua", 0.65, 0.75)):
            F.configure(name)
            self.assertTrue(lo <= F.withers_height() <= hi, (name, F.withers_height()))

    def test_the_yaguarete_is_stockier_than_the_puma(self):
        # Short thick legs under a deep chest, a massive head, a short tail.
        out = {}
        for name in ("puma", "yagua"):
            F.configure(name)
            w = F.withers_height()
            tail = sum(F._dist(a, b) for a, b in zip(F.TAIL, F.TAIL[1:])) * F.SP["S"]
            out[name] = (model_z(F.chest_bottom()) / w, F.SP["HEAD"], tail / w, model_z(F.ELBOW_Z) / w)
        self.assertLess(out["yagua"][0], out["puma"][0] - 0.03)
        self.assertLess(out["yagua"][3], out["puma"][3] - 0.03)
        self.assertGreater(out["yagua"][1], out["puma"][1] * 1.25)
        self.assertLess(out["yagua"][2], out["puma"][2] * 0.75)

    def test_the_yaguarete_legs_are_as_long_as_in_the_photographs(self):
        # L107: "patas demaciado cortas". From the side, the chest's height
        # over the ground against its depth, as in the longest-legged photo,
        # and still well under the puma's.
        F.configure("yagua")
        self.assertAlmostEqual(F.leg_ratio(), F.LEG_RATIO_TARGET["yagua"], delta=0.02)
        yagua = F.leg_ratio()
        F.configure("puma")
        self.assertLess(yagua, F.leg_ratio() - 0.15)

    def test_warp_and_unwarp_are_inverse(self):
        for name in SPECIES:
            F.configure(name)
            for p in ((0.05, -0.6, 0.62), (0.07, 0.0, 0.2), (0.0, 0.5, 0.04), (0.08, 0.3, 0.35)):
                q = F.unwarp(F.warp(p))
                self.assertLess(F._dist(p, q), 1e-9, (name, p, q))


class Head(unittest.TestCase):
    def tearDown(self):
        F.configure("puma")

    def test_eyes_and_ears_are_in_proportion_with_the_photographs(self):
        # Measured on the photographs (``HEAD_RATIOS``): felid eyes are
        # small in a broad skull (the reviews of v1 and v3: an owl's eyes).
        for name in SPECIES:
            F.configure(name)
            eye, ear = F.head_ratios()
            want_eye, want_ear = F.HEAD_RATIOS[name]
            self.assertLess(abs(eye - want_eye), 0.02, (name, eye))
            self.assertLess(abs(ear - want_ear), 0.03, (name, ear))


class FacePhoto(unittest.TestCase):
    def tearDown(self):
        F.configure("puma")

    def test_the_photos_are_free_to_use_and_credited(self):
        # The user's authorization (L94): Commons, CC0, public domain or
        # CC BY only.
        for name in SPECIES:
            photo = F.FACE_PHOTOS[name]
            self.assertIn(photo["licence"], F.FACE_LICENCES, name)
            self.assertFalse(any(x in photo["licence"] for x in ("SA", "NC", "ND")), name)
            self.assertTrue(photo["page"].startswith(("https://commons.wikimedia.org/wiki/File:", "https://www.inaturalist.org/photos/")), name)
            self.assertTrue(photo["author"], name)

    def test_the_mapping_puts_every_landmark_on_its_pixel(self):
        import numpy as np

        for name in SPECIES:
            F.configure(name)
            photo = F.FACE_PHOTOS[name]
            marks = F.face_landmarks()
            names = sorted(photo["points"])
            p3 = [F.warp(marks[n]) for n in names]
            _, f = F.fit_mapping(p3, [photo["points"][n] for n in names], 0.035 * F.SP["HEAD"] * F.SP["S"])
            for n, got in zip(names, f(p3)):
                self.assertLess(float(np.hypot(*(got - photo["points"][n]))), 0.01, (name, n))
        # A true affine camera is recovered: points off the landmarks land
        # where it puts them.
        cam = np.array([[900.0, 30.0, -40.0, 500.0], [20.0, -60.0, -880.0, 900.0]])
        p3 = [F.warp(marks[n]) for n in names]
        A, f = F.fit_mapping(p3, [cam @ np.array(list(p) + [1.0]) for p in p3], 0.05)
        other = np.array(F.warp(F._at(0.10, -0.02, 0.03)))
        self.assertLess(float(np.hypot(*(f(other)[0] - cam @ np.append(other, 1.0)))), 0.5)

    def test_the_photo_covers_the_face_and_fades_out_smoothly(self):
        F.configure("puma")
        view = (0.0, -1.0, 0.15)
        bridge = F._at(0.150, 0.010)
        self.assertGreater(F.face_weight(bridge, (0.0, -0.8, 0.6), view), 0.9)
        self.assertEqual(F.face_weight(F._ear(F.EAR_MID, 1.0), (0.0, -1.0, 0.0), view), 0.0)
        # Over the head from the nose back to the nape the weight never
        # jumps: no seam.
        prev, steps, top = None, [], 0.0
        for k in range(200):
            t = 0.19 - 0.25 * k / 199
            q = F._at(t, 0.050 - 0.012 * (t - 0.12) ** 2)
            w = F.face_weight(q, (0.0, -0.6, 0.8), view)
            top = max(top, w)
            if prev is not None:
                steps.append(abs(w - prev))
            prev = w
        self.assertGreater(top, 0.8)
        self.assertLess(max(steps), 0.05)
        self.assertEqual(prev, 0.0)
        # A cheek turned 75 degrees from the photo's view takes none of it
        # (a front photo smeared across it: the v5 3/4 views).
        cheek = F._at(0.120, -0.040, 0.050)
        turned = (0.966, -0.259, 0.0)
        self.assertEqual(F.face_weight(cheek, turned, view), 0.0)

    def test_the_head_holds_its_share_of_the_colour_map(self):
        # v5's puma face had about 5,000 texels (L95): the head's islands
        # are scaled up to HEAD_UV_SHARE of the map's, never shrunk.
        for h in (0.036, 0.10):
            k = F.head_uv_scale(h, 1.0)
            self.assertAlmostEqual(h * k * k / (1.0 - h + h * k * k), F.HEAD_UV_SHARE, places=6)
        self.assertGreater(F.HEAD_UV_SHARE, 0.15)
        self.assertEqual(F.head_uv_scale(0.3, 1.0), 1.0)
        self.assertTrue(F.head_island(0.6, 1.0))
        self.assertFalse(F.head_island(0.4, 1.0))
        self.assertTrue(F.on_head(F._at(0.10)) and not F.on_head((0.15, 0.0, 0.50)))


class Profile(unittest.TestCase):
    def tearDown(self):
        F.configure("puma")

    def test_the_chest_is_flat_in_front_and_not_below_the_elbows(self):
        # The fox's lesson (L83): below the point of the shoulder the neck
        # runs down into the forelegs, no pigeon breast ahead of them.
        for name in SPECIES:
            F.configure(name)
            shoulder_y, shoulder_z = F.SHOULDER_POINT
            for k in range(10):
                z = F.ELBOW_Z - 0.02 + k * (shoulder_z - F.ELBOW_Z) / 10
                front = F.chest_front(z)
                if front is not None:
                    self.assertGreater(front, shoulder_y - 0.02, (name, z))
            drop = model_z(F.ELBOW_Z) - model_z(F.chest_bottom())
            self.assertLess(drop, 0.06 * F.withers_height(), name)

    def test_the_back_is_level_and_the_pouch_hangs_before_the_thighs(self):
        for name in SPECIES:
            F.configure(name)
            self.assertLess(abs(F.hips_height() - F.withers_height()), 0.03 * F.withers_height(), name)
            self.assertLess(F.belly_line(0.16), F.belly_line(0.30), name)
        # The big cats' loose skin of the belly sags in front of the thighs,
        # lower than the belly just behind the ribs (the gato montés's is
        # slight).
        for name in ("puma", "yagua"):
            F.configure(name)
            self.assertLess(F.belly_line(0.16), F.belly_line(0.08), name)


FLANK = ((0.15, 0.0, 0.50), (1.0, 0.0, 0.0))
BELLY = ((0.0, 0.0, 0.355), (0.0, 0.0, -1.0))


def flank_grid():
    return [((0.155, -0.15 + 0.01 * i, 0.44 + 0.012 * j), (1.0, 0.0, 0.0)) for i in range(30) for j in range(12)]


def dark_share(points):
    spot = lum(F.COATS[F.NAME]["spot"])
    ground = lum(F.COATS[F.NAME]["ground"])
    return sum(1 for q, m in points if lum(F.coat_at(q, m)[0]) < (spot + ground) / 2) / len(points)


class Coats(unittest.TestCase):
    def tearDown(self):
        F.configure("puma")

    def test_the_puma_is_plain_with_a_white_chin_and_a_dark_tail_tip(self):
        F.configure("puma")
        self.assertEqual(dark_share(flank_grid()), 0.0)
        chin = F.coat_at(F._at(0.150, -0.104), (0.0, -0.3, -0.95))[0]
        self.assertGreater(min(chin), 200, chin)
        flank = F.coat_at(*FLANK)[0]
        self.assertTrue(120 < lum(flank) < 170 and flank[0] > flank[2], flank)
        tip = F.coat_at(F.TAIL[9], (0.0, 0.3, -0.95))[0]
        self.assertLess(max(tip), 60, tip)

    def test_the_spotted_cats_are_spotted_the_gato_densely_the_yaguarete_in_rosettes(self):
        F.configure("gato")
        gato = dark_share(flank_grid())
        F.configure("yagua")
        yagua = dark_share(flank_grid())
        # Dense small spots on the gato montés; the rosettes' blotches
        # cover a good part of the yaguareté's flank.
        self.assertTrue(0.12 < gato < 0.45, gato)
        # Small and many: along three rows across the flank, over a dozen
        # separate spots or streaks (big sparse spots give a handful).
        F.configure("gato")
        spot, ground = lum(F.COATS["gato"]["spot"]), lum(F.COATS["gato"]["ground"])
        count = 0
        for z in (0.46, 0.50, 0.54):
            row = [lum(F.coat_at((0.155, -0.15 + 0.003 * i, z), (1.0, 0.0, 0.0))[0]) < (spot + ground) / 2 for i in range(130)]
            count += sum(1 for a, b in zip(row, row[1:]) if a and not b)
        self.assertGreater(count, 15)
        self.assertTrue(0.15 < yagua < 0.5, yagua)
        # A rosette: a dark ring with a deeper gold, unspotted ground inside.
        inside = [F.rosettes((0.155, -0.15 + 0.004 * i, 0.44 + 0.01 * j), F.ROSETTE_CELL, 1.0)
                  for i in range(80) for j in range(10)]
        self.assertTrue(any(d < 0.1 and ins > 0.9 for d, ins in inside))
        self.assertTrue(any(d > 0.9 for d, _ in inside))

    def test_the_gato_has_rings_on_its_tail_and_stripes_on_its_cheeks(self):
        F.configure("gato")
        along = []
        for k in range(60):
            f = 0.1 + 0.8 * k / 60
            i = min(int(f * 9), 8)
            a, b = F.TAIL[i], F.TAIL[i + 1]
            g = f * 9 - i
            p = tuple(a[j] + (b[j] - a[j]) * g for j in range(3))
            along.append(lum(F.coat_at((0.02, p[1], p[2]), (1.0, 0.0, 0.0))[0]) < 80)
        changes = sum(1 for x, y in zip(along, along[1:]) if x != y)
        self.assertGreaterEqual(changes, 10, along)
        stripe = F.coat_at(F._at(0.090, -0.016, 0.050), (1.0, 0.0, 0.0))[0]
        self.assertLess(lum(stripe), 80, stripe)

    def test_the_puma_has_no_dark_line_back_from_the_eye(self):
        # The v5 review: a stray dark line ran back from each eye.
        F.configure("puma")
        m = (0.7, -0.5, 0.5)
        on = F.coat_at(F._at(0.124, -0.007, 0.050), m)[0]
        beside = F.coat_at(F._at(0.124, 0.006, 0.050), m)[0]
        self.assertLess(abs(lum(on) - lum(beside)), 15, (on, beside))

    def test_the_yaguarete_spots_shrink_gradually_up_the_neck(self):
        # The v5 review: the body's large spots met the face's small ones
        # at once behind the jaw. The spots' mean width along rows on the
        # neck, at five places from behind the shoulders to the head:
        # smaller at each, by steps of the same order.
        F.configure("yagua")
        spot, ground = lum(F.COATS["yagua"]["spot"]), lum(F.COATS["yagua"]["ground"])

        def width(y):
            runs = []
            for j in range(40):
                n = 0
                for i in range(151):
                    if i < 150 and lum(F.coat_at((0.002 * i, y, 0.38 + 0.006 * j), (1.0, 0.0, 0.0))[0]) < (spot + ground) / 2:
                        n += 1
                    elif n:
                        runs.append(n)
                        n = 0
            return 0.002 * sum(runs) / len(runs)

        widths = [width(y) for y in (-0.37, -0.39, -0.41, -0.43, -0.45)]
        steps = [a - b for a, b in zip(widths, widths[1:])]
        self.assertTrue(all(d > 0.0 for d in steps), widths)
        self.assertGreater(widths[0], 1.4 * widths[-1], widths)
        self.assertLess(max(steps), 0.5 * sum(steps), widths)

    def test_the_bellies_are_pale(self):
        for name in SPECIES:
            F.configure(name)
            rgb = F.coat_at(*BELLY)[0]
            spot = lum(F.COATS[name]["spot"])
            self.assertTrue(lum(rgb) > 190 or lum(rgb) < spot + 20, (name, rgb))


class Cards(unittest.TestCase):
    def tearDown(self):
        F.configure("puma")

    def test_each_hair_has_its_own_band_so_no_stripe_runs_across_a_tuft(self):
        F.configure("gato")
        root, band, tip = (F.card_colour(F.GROUND, f) for f in (0.1, 0.6, 0.98))
        self.assertTrue(lum(tip) < lum(root) < lum(band), (root, band, tip))
        # The same place is in the pale band on one hair, not on another.
        self.assertGreater(lum(F.card_colour(F.GROUND, 0.5, -0.1)), lum(F.card_colour(F.GROUND, 0.5, 0.1)) + 25)
        # Pale tufts never brighter than the white they lie on.
        self.assertLess(lum(F.card_colour(F.PALE, 0.8)), lum(F.COATS["gato"]["white"]))

    def test_no_tail_lock_grows_on_the_roots_underside(self):
        # It would hang behind the thighs, off the tail (the fox, L82).
        self.assertFalse(F.tail_lock_ok(0.05, -0.9))
        self.assertTrue(F.tail_lock_ok(0.05, 0.9))
        self.assertTrue(F.tail_lock_ok(0.5, -0.9))


class Gaits(unittest.TestCase):
    def tearDown(self):
        F.configure("puma")

    def test_the_cats_walk_slowly_and_sprint_near_their_flee_speeds(self):
        # The game flees at the cats' sprint speeds, 45 (gato), 72 (puma)
        # and 65 km/h (yaguareté, ``CritterKind::profile``), and plays a
        # gallop only a little faster than it was made (``Species::max_pace``):
        # the sprint's long stride carries most of the speed (the puma and
        # the yaguareté under 1.7 times); the small gato montés takes more
        # strides a second, under its own cap (``GATO_GALLOP_RATE``, 2.4). A
        # sprint, many times the walk, made at 2.5 to 3.5 strides a second.
        for name, kmh, pace in (("gato", 45.0, 2.4), ("puma", 72.0, 1.7), ("yagua", 65.0, 1.7)):
            F.configure(name)
            walk, gallop = F.ground_speed(F.WALK), F.ground_speed(F.GALLOP)
            self.assertTrue(0.3 < walk < 1.0, (name, walk))
            self.assertGreater(gallop, 7.0 * walk, name)
            self.assertLess(kmh / 3.6 / gallop, pace, name)
            self.assertTrue(2.5 <= 1.0 / F.GALLOP["seconds"] <= 3.5, name)

    def test_the_walk_is_the_footages_lateral_sequence(self):
        # T4h3, the Patagonia puma's walk (the gato montés its size) and the
        # Iguazú jaguar's: a stride of 1.03 and 1.0 s; each hind lands, then
        # its own fore 0.20 and 0.22 of the stride later (LH, LF, RH, RF);
        # a fore stands 0.70 and 0.64 of the stride, a hind a little less;
        # and the hind lands in the print its fore has just left (or, the
        # yaguareté, half a paw behind it, never crouched under the belly).
        for name, seconds, lateral, duty in (("gato", 0.66, 0.20, 0.70), ("puma", 1.03, 0.20, 0.70),
                                             ("yagua", 1.00, 0.22, 0.64)):
            F.configure(name)
            falls = F.footfalls(F.WALK)
            self.assertAlmostEqual(F.WALK["seconds"], seconds, delta=0.02, msg=name)
            order = sorted(falls, key=lambda leg: falls[leg]["down"])
            self.assertEqual(order, ["LH", "LF", "RH", "RF"], name)
            for side in "LR":
                hind, fore = falls[side + "H"], falls[side + "F"]
                self.assertAlmostEqual(fore["down"] - hind["down"], lateral, delta=0.02, msg=name)
                self.assertAlmostEqual(fore["duty"], duty, delta=0.02, msg=name)
                self.assertTrue(duty - 0.06 <= hind["duty"] < fore["duty"], (name, hind["duty"]))
                # The fore has left before its hind lands a stride on.
                self.assertLess(fore["down"] + fore["duty"], hind["down"] + 1.0, name)
                behind = hind["print"] - hind["stride"] - fore["print"]
                self.assertTrue(-0.02 * F.SP["S"] < behind < 0.06 * F.SP["S"], (name, behind))
            # The hinds stand no further under the belly than 5 % of the size.
            self.assertGreater(F.WALK["centre"]["H"], -0.05 * F.SP["S"], name)


def check_fence_leap(case, M):
    """The fence leap (``Fence_Leap``) keyed to the puma over the wire,
    taken out of the gallop (shared by the fox's tests)."""
    # A second in the air, about the game's flight (``Leap::flight``,
    # 1.0-1.1 s), so the game plays it near 1x.
    case.assertAlmostEqual(M.FENCE_LEAP_AIR, 1.0)
    case.assertAlmostEqual(M.leap_u(M.FENCE_LEAP_FRAMES[0] / M.FENCE_LEAP_FRAMES[2]), 0.0)
    case.assertAlmostEqual(M.leap_u(M.FENCE_LEAP_FRAMES[1] / M.FENCE_LEAP_FRAMES[2]), 1.0)
    off, top, down = M.leap_pose(0.0), M.leap_pose(0.45), M.leap_pose(1.0)
    # The hinds push off the ground, the fores already up; the body rises
    # nearly flat, never rearing as in a pounce.
    case.assertEqual((off["hind"][3], off["fore"][3]), (1.0, 0.0))
    case.assertTrue(-12.0 <= min(M.leap_pose(u)["pitch"] for u in (0.0, 0.1, 0.2, 0.3)) < 0.0)
    # At the top the back is long: the hinds trail straight back, the
    # forearms reach forward, the tail is out.
    case.assertGreater(top["hind"][0], 0.35)
    case.assertLess(top["fore"][0], -0.25)
    case.assertLess(top["flex"], -10.0)
    # Nose down on the way down, the forepaws on the ground at the end with
    # the hinds still folded up behind, then the hinds land.
    case.assertTrue(15.0 <= down["pitch"] <= 25.0, down["pitch"])
    case.assertEqual((down["fore"][3], down["hind"][3]), (1.0, 0.0))
    case.assertGreater(down["hind"][1], 0.2)
    case.assertEqual(M.leap_pose(1.33)["hind"][3], 1.0)
    # No gather or crouch: out of the stride it never sinks lower than in
    # the gallop (``gallop``: 0.04 at most), nor stops; it loops smoothly.
    case.assertGreaterEqual(min(M.leap_pose(u / 10)["lift"] for u in range(-2, 16)), -0.045)
    period = M.FENCE_LEAP_SECONDS / M.FENCE_LEAP_AIR
    for u in (-0.1, 0.3, 1.25):
        a, b = M.leap_pose(u), M.leap_pose(u + period)
        case.assertAlmostEqual(a["pitch"], b["pitch"])
        case.assertAlmostEqual(a["fore"][1], b["fore"][1])


class Leap(unittest.TestCase):
    def test_the_fence_leap_is_a_flat_bound_out_of_the_gallop_over_a_second(self):
        check_fence_leap(self, F)


class Skeleton(unittest.TestCase):
    def tearDown(self):
        F.configure("puma")

    def test_the_rig_keeps_the_quaternius_names_parents_first_and_unique(self):
        for name in SPECIES:
            F.configure(name)
            names = F.joint_names()
            self.assertEqual(len(names), len(set(names)))
            self.assertEqual(len(names), 49)
            self.assertFalse(set(F.QUATERNIUS_NAMES) - set(names))
            self.assertFalse(({"Jaw", "Ear2.L", "Ear2.R"} | set(F.TAIL_CHAIN)) - set(names))
            seen = set()
            for bone, parent, _ in F.BONES:
                self.assertTrue(parent is None or parent in seen, bone)
                seen.add(bone)
            for bone, parent, _, _ in F.EXTRA:
                self.assertIn(parent, seen | {n for n, _, _, _ in F.EXTRA})


if __name__ == "__main__":
    unittest.main()
