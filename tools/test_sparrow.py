"""Tests of tools/sparrow.py that run without Blender: the two sparrows'
sizes, their passerine wings and the cards' bones, the folded wing along
the flank, the standing legs, the raptors and the pigeons left as they
were, each species' marks, the clips' loops and the real pixels' credits.

    python3 -I -B -m unittest discover -s tools
"""

import os
import unittest

import numpy as np

import pigeon as P
import raptor as R
import sparrow as S

SPECIES = ("gorrion", "chingolo")


def paint(points, normals):
    col, _, _ = R.plumage(np.array(points, float), np.array(normals, float))
    return col


def lum(rgb):
    return float(np.mean(rgb))


class Sparrow(unittest.TestCase):
    def tearDown(self):
        R.configure("chimango")

    def test_each_species_measures_as_in_the_literature(self):
        # House sparrow 14-16 cm long, 21-25.5 cm across, its folded wing
        # 72-82 mm, about 30 g; chingolo 13-15 cm, wing 62-72 mm, about
        # 22 g; the tarsus about 19 and 21 mm.
        for name, mass, tarsus in (("gorrion", (0.028, 0.032), 0.019), ("chingolo", (0.020, 0.025), 0.021)):
            S.configure(name)
            for key, value in (("span", 2 * R.half_span()), ("length", R.total_length()), ("chord", S.chord())):
                lo, hi = S.SP[key]
                self.assertTrue(lo <= value <= hi, (name, key, value))
            self.assertTrue(mass[0] <= S.SP["mass"] <= mass[1], name)
            hip, knee, heel, foot, toe = R.leg_joints()
            self.assertAlmostEqual(R.length(R.sub(foot, heel)), tarsus, delta=0.0015)

    def test_the_wing_is_a_passerines_and_every_card_hangs_from_real_bones(self):
        # Nine primaries (P6 to P8 the longest, a round wing tip), six
        # secondaries and three tertials; every card skinned to the
        # armature's bones (its weights summing to 1) with its atlas
        # cells; the folded wing's cards on ``Fold.*`` alone.
        for name in SPECIES:
            S.configure(name)
            bones = {b[0] for b in R.bone_table()}
            cells = R.atlas_cells()
            prim = {fd["index"]: fd for fd in R.remiges() if fd["kind"] == "P"}
            sec = [fd for fd in R.remiges() if fd["kind"] == "S"]
            self.assertEqual(sorted(prim), list(range(1, 10)))
            self.assertEqual(sorted(fd["index"] for fd in sec), list(range(1, 10)))
            tips = {i: R.length(R.sub(R.feather_tip(fd), R.wing_joints()[2])) for i, fd in prim.items()}
            self.assertIn(max(tips, key=tips.get), (6, 7, 8))
            for fd in R.feather_set():
                self.assertTrue(set(fd["bones"]) <= bones, (name, fd["kind"], fd["index"]))
                self.assertAlmostEqual(sum(fd["bones"].values()), 1.0, places=6)
                if fd.get("fold"):
                    self.assertEqual(set(fd["bones"]), {"Fold.L" if fd["side"] > 0 else "Fold.R"})
                for under in fd.get("layers", (False, True)):
                    self.assertIn((fd["kind"], fd["index"], under), cells)

    def test_the_folded_wing_lies_along_the_flank_its_primaries_past_the_tertials(self):
        # On the ground the wing lies folded on the flank: its cards rooted
        # on the body's side, the primaries' tips projecting past the
        # tertials' and reaching past the tail's root but not half way
        # down the tail; the scapulars on the body alone.
        for name in SPECIES:
            S.configure(name)
            hw = S.SP["torso"][0]
            fold = [fd for fd in R.feather_set() if fd.get("fold") and fd["side"] > 0]
            for fd in fold:
                x, y, z = fd["root"]
                self.assertLess(abs(x - S.flank(y, z)), 0.30 * hw, (name, fd["kind"], fd["index"]))
            prim = max(R.feather_tip(fd)[1] for fd in fold if fd["kind"] == "P")
            tert = max(R.feather_tip(fd)[1] for fd in fold if fd["kind"] == "S" and fd["index"] in S.TERTIALS)
            tail = R.tail_root()[1]
            self.assertGreater(prim - tert, 0.008)
            self.assertTrue(tail < prim < tail + 0.5 * S.SP["tail"], (name, prim, tail))
            for fd in R.feather_set():
                if fd["kind"] == "K":
                    self.assertEqual(fd["bones"], {"Body": 1.0})

    def test_the_legs_stand_visible_and_tuck_in_flight(self):
        # A perching bird's legs show: standing, the heel and the foot well
        # outside the body, the tarsus painted bare (pinkish brown) and
        # the toes reaching forward along the ground; the flight clips'
        # tuck draws the foot inside the belly's feathers, within the
        # shin's and the tarsus' reach.
        for name in SPECIES:
            S.configure(name)
            radii = np.array(S.SP["torso"])
            hip, knee, heel, foot, toe = R.leg_joints()
            for j in (heel, foot):
                self.assertGreater(float(np.linalg.norm(np.array(j) / radii)), 1.15, name)
            self.assertLess(toe[1], foot[1] - 0.8 * S.SP["leg"][2])
            c = paint([R.lerp(heel, foot, 0.5)], [(0, -1, 0)])[0]
            self.assertTrue(c[0] - c[2] > 0.08 and lum(c) > 0.4, (name, c))
            hw, hl, hh = S.SP["torso"]
            tuck = np.array(S.TUCK) * radii
            self.assertLess(float(np.linalg.norm(tuck / radii)), 1.0)
            ti, ta, to = S.SP["leg"]
            self.assertLess(np.linalg.norm(tuck - np.array(knee)), ti + ta)
            self.assertGreater(S.stand_height(), -foot[2] * 0.8)

    def test_the_raptors_and_the_pigeons_are_left_as_they_were(self):
        # The sparrows' dispatch only answers while a sparrow is
        # configured: the caracaras get raptor's own layout, the pigeons
        # pigeon's.
        S.configure("gorrion")
        for name in ("chimango", "carancho", "torcaza", "picazuro"):
            if name in P.SPECIES:
                P.configure(name)
                self.assertEqual(R.remiges(), P.remiges())
                self.assertEqual(R.leg_joints(), P.leg_joints())
                self.assertEqual(R.bill_tip(), P.bill_tip())
            else:
                R.configure(name)
                self.assertEqual(R.remiges(), P.ORIGINAL["remiges"]())
                self.assertEqual(R.leg_joints(), P.ORIGINAL["leg_joints"]())
            self.assertEqual(R.feather_set(), S.ORIGINAL["feather_set"]())
            self.assertEqual(R.bone_table(), S.ORIGINAL["bone_table"]())
            wings = R.remiges(1.0) + R.remiges(-1.0)
            self.assertEqual(R.coverts(wings), S.ORIGINAL["coverts"](wings))
            fd = R.feather_set()[0]
            self.assertEqual(R.card_points(fd), S.ORIGINAL["card_points"](fd))

    def test_the_gorrion_wears_a_male_house_sparrows_marks(self):
        # Grey crown, chestnut behind the eye, a small white spot over the
        # eye's rear, a white cheek, black chin, a grey-buff belly (warm,
        # not the v1 bluish white); the median coverts' white tips (the
        # wing bar).
        S.configure("gorrion")
        hc, (w, l, h) = np.array(R.head_centre()), S.SP["head"]
        e, r = np.array(R.eye_centre()), S.SP["eye"]
        bb, (L, D, Wb) = np.array(R.bill_base()), S.SP["bill"]
        hw, hl, hh = S.SP["torso"]
        side = (1, 0, 0)
        crown, band, spot, cheek, chin, belly = paint(
            [(0, hc[1], hc[2] + 0.99 * h), (0.86 * w, e[1] + 2.2 * r, e[2] + 0.6 * r), (0.85 * w, e[1] + 1.2 * r, e[2] + 1.0 * r),
             (0.85 * w, e[1] + 1.5 * r, e[2] - 1.6 * r), (0, bb[1] + 3 * r, bb[2] - D), (0, 0.2 * hl, -0.98 * hh)],
            [(0, 0, 1), side, side, side, (0, -0.3, -1), (0, 0, -1)])
        self.assertLess(abs(crown[0] - crown[2]), 0.05)
        self.assertGreater(band[0] - band[2], 0.25)
        self.assertGreater(lum(spot), 0.75)
        self.assertGreater(lum(cheek), 0.75)
        self.assertLess(lum(chin), 0.12)
        self.assertTrue(0.5 < lum(belly) < 0.8 and 0.02 < belly[0] - belly[2] < 0.16, belly)
        col, alpha, _ = R.paint_feather("M", 1, False, 0.0085, 0.006, 0.45, False)
        c = int(0.65 * R.CELL[0])  # on the inner vane, off the rachis
        self.assertGreater(col[int(0.85 * R.CELL[1]), c].mean(), 0.8)
        self.assertLess(col[int(0.4 * R.CELL[1]), c].mean(), 0.2)

    def test_the_gorrions_bib_is_a_ragged_throat_patch(self):
        # The black bib covers the throat and stops at the top of the
        # breast (v1 review: a tie down the whole breast): black on the
        # throat's front, none on the neck's sides nor on the breast below
        # it; its lower edge ragged, feather by feather.
        S.configure("gorrion")
        hw, hl, hh = S.SP["torso"]
        rng = np.random.default_rng(5)
        pts = np.c_[rng.uniform(-0.03, 0.03, 30000), rng.uniform(-0.045, 0.0, 30000), rng.uniform(-0.02, 0.02, 30000)]
        s_n, around = P._neck_coord(pts)
        nb, ax = np.array(R.neck_base()), np.array(R.head_centre()) - np.array(R.neck_base())
        radial = np.linalg.norm(pts - (nb + s_n[:, None] * ax), axis=1)
        on = np.abs(radial - 0.78 * hw) < 0.25 * hw
        black = paint(pts, np.tile((0, -0.3, -1), (len(pts), 1))).mean(axis=1) < 0.15
        throat = on & (s_n > 0.15) & (s_n < 0.6) & (around > 2.85)
        sides = on & (s_n > 0.15) & (s_n < 0.6) & (np.abs(around - 1.9) < 0.15)
        breast = on & (s_n < -0.45) & (around > 2.85)
        edge = on & (np.abs(s_n + 0.18) < 0.05) & (around > 2.9)
        self.assertGreater(black[throat].mean(), 0.9)
        self.assertLess(black[sides].mean(), 0.05)
        self.assertLess(black[breast].mean(), 0.02)
        self.assertTrue(0.15 < black[edge].mean() < 0.85, black[edge].mean())

    def test_the_chingolo_wears_its_collar_crown_stripes_and_breast_patches(self):
        # A rufous collar on the hind neck and the neck's sides, not the
        # throat; the throat white; a black patch on each side of the
        # breast, none on its middle; the crown's black lateral stripes
        # either side of its grey middle; the black line behind the eye.
        S.configure("chingolo")
        hc, (w, l, h) = np.array(R.head_centre()), S.SP["head"]
        e, r = np.array(R.eye_centre()), S.SP["eye"]
        bb, (L, D, Wb) = np.array(R.bill_base()), S.SP["bill"]
        rng = np.random.default_rng(3)
        pts = np.c_[rng.uniform(-0.03, 0.03, 20000), rng.uniform(-0.04, 0.0, 20000), rng.uniform(-0.02, 0.02, 20000)]
        s_n, around = P._neck_coord(pts)
        # On the neck's surface: about its radius off its axis.
        nb, ax = np.array(R.neck_base()), np.array(R.head_centre()) - np.array(R.neck_base())
        radial = np.linalg.norm(pts - (nb + s_n[:, None] * ax), axis=1)
        on = np.abs(radial - 0.78 * S.SP["torso"][0]) < 0.25 * S.SP["torso"][0]
        col = paint(pts, np.tile((0, 0, 1), (len(pts), 1)))
        rufous = (col[:, 0] - col[:, 2] > 0.3) & (col[:, 1] < 0.45)
        black = col.mean(axis=1) < 0.12
        hind = on & (s_n > 0.1) & (s_n < 0.45) & (around < 1.2)
        throat = on & (s_n > 0.1) & (s_n < 0.45) & (around > 2.8)
        sides = on & (np.abs(s_n) < 0.15) & (np.abs(around - 2.3) < 0.15)
        front = on & (np.abs(s_n) < 0.15) & (around > 3.0)
        self.assertGreater(rufous[hind].mean(), 0.9)
        self.assertLess(rufous[throat].mean(), 0.05)
        self.assertGreater(black[sides].mean(), 0.6)
        self.assertLess(black[front].mean(), 0.05)
        mid, stripe, line, throat_c = paint(
            [(0, hc[1] - 0.2 * l, hc[2] + 0.99 * h), (0.40 * w, hc[1] - 0.2 * l, hc[2] + 0.92 * h), (0.85 * w, e[1] + 2.5 * r, e[2] - 0.2 * r),
             (0, bb[1] + 3 * r, bb[2] - 1.2 * D)],
            [(0, 0, 1), (0.4, 0, 1), (1, 0, 0), (0, -0.3, -1)])
        self.assertTrue(0.45 < lum(mid) < 0.65 and abs(mid[0] - mid[2]) < 0.05, mid)
        self.assertLess(lum(stripe), 0.15)
        self.assertLess(lum(line), 0.15)
        self.assertGreater(lum(throat_c), 0.85)

    def test_the_clips_loop_and_flap_bound_holds_its_wings_folded(self):
        # Every clip ends where it starts. ``FlapBound`` flaps whole beats
        # at the allometry's pace and holds the folded wing through the
        # bound; ``Hop`` lifts the feet off the ground only between the
        # push and the landing; the smaller chingolo beats faster.
        f = {}
        for name in SPECIES:
            S.configure(name)
            f[name] = S.wingbeat()
            self.assertAlmostEqual(S.flap_seconds() * S.FPS, round(S.flap_seconds() * S.FPS))
            n = round(S.flap_bound_seconds() * S.FPS)
            states = [S.flap_bound_state(k / n) for k in range(n + 1)]
            self.assertAlmostEqual(states[0][0], states[-1][0] % 1.0, places=6)
            self.assertEqual(states[0][1], states[-1][1])
            held = sum(1 for phase, g, bound in states if g == 1.0) / S.FPS
            self.assertGreater(held, 0.8 * S.BOUND_SECONDS)
            self.assertLess(sum(1 for phase, g, bound in states if 0 < g < 1), 6)
            for fn in (S.hop_heights, S.peck_angles, S.idle_angles):
                a, b = fn(0.0), fn(1.0 - 1e-9)
                for x, y in zip(a.values() if isinstance(a, dict) else a, b.values() if isinstance(b, dict) else b):
                    self.assertAlmostEqual(x, y, places=4, msg=fn.__name__)
            lo, hi = S.hop_airborne()
            self.assertTrue(0.25 < lo < hi < 0.75, (lo, hi))
            self.assertGreater(max(S.hop_heights(k / 100)[1] for k in range(100)), 0.25 * S.SP["leg"][1])
        self.assertGreater(f["chingolo"], f["gorrion"])

    def test_peck_tips_the_body_forward_on_planted_feet(self):
        # Pecking, the bird tips forward on its legs: in every frame each
        # foot stays where it stands, on the ground, and the knee and the
        # heel stay within a millimetre of standing (v1 review: the femur
        # swung forward with the body and the bare shin showed in a Z under
        # the belly).
        for name in SPECIES:
            S.configure(name)
            stand = {side: S.ground_leg(side) for side in (1.0, -1.0)}
            tipped = 0.0
            for k in range(57):
                pitch = S.peck_angles(k / 56)["pitch"]
                tipped = min(tipped, pitch)
                for side, (h0, k0, he0, f0) in stand.items():
                    hip, knee, heel, foot = S.ground_leg(side, pitch)
                    rest = R.leg_joints(side)[3]
                    self.assertLess(R.length(R.sub(foot, (rest[0], rest[1], S.foot_lift()))), 1e-6, (name, k))
                    self.assertLess(R.length(R.sub(knee, k0)), 0.001, (name, k))
                    self.assertLess(R.length(R.sub(heel, he0)), 0.001, (name, k))
            self.assertLess(tipped, -30.0)

    def test_the_folded_wings_white_bar_lies_over_the_greater_coverts(self):
        # The median coverts' white tips (the wing bar) lie over the
        # greater coverts wherever they cross them (v1 review: they curled
        # in under them and the bar barely showed): sampled over each
        # folded median covert's tip, its card stands outside every
        # greater covert's card there, nearly everywhere.
        def dense(fd, n=12):
            rows, ts = np.array(R.card_points(fd)[0], float), np.array(R.card_rows(fd))
            out = []
            for t in np.linspace(0, 1, n):
                i = min(np.searchsorted(ts, t, side="right") - 1, len(ts) - 2)
                row = rows[i] + (rows[i + 1] - rows[i]) * (t - ts[i]) / (ts[i + 1] - ts[i])
                for c in np.linspace(0, 1, n):
                    j = min(int(c * (len(row) - 1)), len(row) - 2)
                    g = c * (len(row) - 1) - j
                    out.append((t, c, *(row[j] + (row[j + 1] - row[j]) * g)))
            return np.array(out)

        for name in SPECIES:
            S.configure(name)
            fold = S.fold_wing(1.0)
            greater = np.concatenate([dense(fd)[:, 2:] for fd in fold if fd["kind"] == "G"])
            over = []
            for fd in fold:
                if fd["kind"] == "M":
                    for t, c, x, y, z in dense(fd):
                        if t > 0.65 and 0.3 < c < 0.7:
                            near = greater[np.linalg.norm(greater[:, 1:] - (y, z), axis=1) < 0.0006]
                            if len(near):
                                over.append(x > near[:, 0].max())
            self.assertGreater(len(over), 40)
            self.assertGreater(np.mean(over), 0.9, name)

    def test_the_face_photos_are_anchored_on_the_eye(self):
        # Each head carries its photo's face (as the pigeons'): the photo's
        # eye on the model's eye, the iris' radius on the eye's (no oval
        # eye), the other landmarks within two iris radii; the photo kept
        # off the bill and off the photo's background (sky, foliage).
        for name in SPECIES:
            S.configure(name)
            photo = S.FACE_PHOTOS[name]
            f, marks = R.face_mapping(), S.face_landmarks()
            self.assertLess(np.linalg.norm(f(marks["eye"])[0] - photo["points"]["eye"]), 1e-6)
            self.assertAlmostEqual(f.scale * S.SP["eye"], photo["iris"], places=6)
            for key, px in photo["points"].items():
                self.assertLess(np.linalg.norm(f(marks[key])[0] - px), 2.0 * photo["iris"], (name, key))
            tip = np.array([R.lerp(R.bill_base(), S.bill_tip(), 0.5)])
            self.assertEqual(R.photo_weight(tip, np.array([(1.0, 0, 0)]), f)[0][0], 0.0)
        sky = np.zeros((100, 100, 3)) + (0.35, 0.55, 0.80)
        sky[20:80, 20:80] = (0.55, 0.42, 0.30)
        leaves = np.zeros((100, 100, 3)) + (0.40, 0.55, 0.30)
        leaves[20:80, 20:80] = (0.50, 0.50, 0.50)
        for pic, key in ((sky, "blue"), (leaves, "green")):
            bg = S.face_background(pic, key)
            self.assertGreater(bg[5, 5], 0.95)
            self.assertLess(bg[50, 50], 0.05)

    def test_a_slit_in_a_scanned_vane_is_closed_and_a_gap_is_not(self):
        # The House Sparrow's tail sheet: a one-pixel line of background
        # down a vane is filled from both sides; the gap between two
        # feathers stays.
        pic = np.tile(np.asarray(P.BLUE), (40, 60, 1))
        pic[:, 5:25] = (0.40, 0.35, 0.30)
        pic[:, 35:55] = (0.40, 0.35, 0.30)
        pic[:, 15] = P.BLUE
        out = S.close_gaps(pic)
        self.assertTrue(np.allclose(out[:, 15], (0.40, 0.35, 0.30)))
        self.assertTrue(np.allclose(out[:, 30], P.BLUE))

    def test_every_real_pixel_and_reference_is_free_and_credited(self):
        credits = open(os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "CREDITS.md"), encoding="utf-8").read()
        for spec in S.SCANS.values():
            self.assertIn(spec["licence"], R.PIXEL_LICENCES)
            self.assertIn(spec["page"], credits)
            for sheet in spec["sheets"].values():
                self.assertIn(sheet["file"], credits)
        for photo in S.FACE_PHOTOS.values():
            self.assertIn(photo["licence"], R.PIXEL_LICENCES)
            self.assertIn(photo["page"], credits)
        for ref in S.REFERENCES:
            self.assertIn(ref["licence"], S.REFERENCE_LICENCES)
            self.assertIn(ref["page"], credits)


if __name__ == "__main__":
    unittest.main()
