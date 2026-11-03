"""Tests of tools/pigeon.py that run without Blender: the two pigeons'
sizes, their pointed wings and the feathers' bones, the wingbeat's timing
and the flap-and-glide clip, the raptors left as they were, the plumage
painted on the body and the cards, the black scan sheets keyed and cut, the
face photos' eye-anchored mapping and the real pixels' credits.

    python3 -I -B -m unittest discover -s tools
"""

import os
import unittest

import numpy as np

import pigeon as P
import raptor as R

SPECIES = ("torcaza", "picazuro")


def lum(rgb):
    return float(np.mean(rgb))


def paint(points, normals):
    col, _, _ = R.plumage(np.array(points, float), np.array(normals, float))
    return col


class Pigeon(unittest.TestCase):
    def tearDown(self):
        R.configure("chimango")

    def test_each_species_measures_as_in_the_literature(self):
        # Eared dove 22-28 cm long, its folded wing 140-155 mm; picazuro
        # pigeon 34-36 cm and 205-230 mm; spans as the congeners'.
        for name in SPECIES:
            P.configure(name)
            for key, value in (("span", 2 * R.half_span()), ("length", R.total_length()), ("chord", P.chord())):
                lo, hi = P.SP[key]
                self.assertTrue(lo <= value <= hi, (name, key, value))

    def test_the_wing_is_pointed_and_every_feather_hangs_from_real_bones(self):
        # A pigeon's wing tip: the ninth primary the longest, the tenth and
        # the eighth nearly as long, no emarginated fingers. Every card (the
        # primary coverts on their own cells included) is skinned to the
        # rig's bones, its weights summing to 1, and has its atlas cells.
        bones = set(R.RAPTOR_BONES)
        for name in SPECIES:
            P.configure(name)
            cells = R.atlas_cells()
            prim = {fd["index"]: fd for fd in R.remiges() if fd["kind"] == "P"}
            tips = {i: R.length(R.sub(R.feather_tip(fd), R.wing_joints()[2])) for i, fd in prim.items()}
            self.assertEqual(max(tips, key=tips.get), 9, (name, tips))
            self.assertGreater(tips[10], 0.9 * tips[9])
            self.assertFalse(any(fd["finger"] for fd in prim.values()))
            kinds = set()
            for fd in R.feather_set():
                kinds.add(fd["kind"])
                self.assertTrue(set(fd["bones"]) <= bones, (name, fd["kind"], fd["index"]))
                self.assertAlmostEqual(sum(fd["bones"].values()), 1.0, places=6)
                for under in fd.get("layers", (False, True)):
                    self.assertIn((fd["kind"], fd["index"], under), cells)
            self.assertEqual(kinds, set("PSRGUMLK"))

    def test_the_raptors_are_left_as_they_were(self):
        # The pigeons' dispatch only answers while a pigeon is configured:
        # the caracaras get raptor's own wing, bill, legs, face fit and
        # coverts.
        P.configure("torcaza")
        for name in ("chimango", "carancho"):
            R.configure(name)
            self.assertEqual(R.remiges(), P.ORIGINAL["remiges"]())
            self.assertEqual(R.bill_tip(), P.ORIGINAL["bill_tip"]())
            self.assertEqual(R.leg_joints(), P.ORIGINAL["leg_joints"]())
            wings = R.remiges(1.0) + R.remiges(-1.0)
            self.assertEqual(R.coverts(wings), P.ORIGINAL["coverts"](wings))
            pts = np.array([R.eye_centre(), R.bill_tip()])
            np.testing.assert_allclose(R.face_mapping()(pts), P.ORIGINAL["face_mapping"]()(pts))

    def test_the_flap_beats_at_a_pigeons_measured_pace(self):
        # Calibrated on homing pigeons' measured 6.6 Hz in pairs (Taylor et
        # al. 2018), the 0.37 kg picazuro beats in the 5.5-7.5 Hz of the
        # pigeons' solo and paired flight, the smaller torcaza faster; the
        # clip holds whole beats to the frame; the downstroke sweeps the
        # extended wing down, the upstroke flexes it.
        f = {}
        for name in SPECIES:
            P.configure(name)
            f[name] = P.wingbeat()
            beats = P.flap_seconds() * f[name]
            self.assertAlmostEqual(beats, P.FLAP_BEATS, delta=0.12)
            self.assertAlmostEqual(P.flap_seconds() * P.FPS, round(P.flap_seconds() * P.FPS))
            top, bottom, up = P.flap_angles(0.0), P.flap_angles(P.DOWNSTROKE), P.flap_angles((1 + P.DOWNSTROKE) / 2)
            self.assertGreater(top["elev"] - bottom["elev"], 100.0)
            self.assertEqual(P.flap_angles(P.DOWNSTROKE / 2)["wrist"], 0.0)
            self.assertGreater(up["wrist"], 50.0)
        self.assertTrue(5.5 <= f["picazuro"] <= 7.5, f)
        self.assertGreater(f["torcaza"], f["picazuro"])

    def test_flap_glide_loops_and_holds_a_glide(self):
        # The clip flaps, stops half way down into a held glide and leaves
        # it on an upstroke: it loops without a jump, holds the glide's
        # pose, and no frame-to-frame step is much larger than the flap's
        # own.
        for name in SPECIES:
            P.configure(name)
            n = round(P.flap_glide_seconds() * P.FPS)
            frames = [P.flap_glide_angles(k / n) for k in range(n + 1)]
            elev = np.array([a["elev"] for a, g in frames])
            wrist = np.array([a["wrist"] for a, g in frames])
            share = np.array([g for a, g in frames])
            self.assertAlmostEqual(elev[0], elev[-1], places=6)
            self.assertEqual(share[0], 0.0)
            self.assertGreater((share > 0.999).sum() / P.FPS, 0.5 * P.HOLD_SECONDS)
            held = P.glide_angles(0.0)
            self.assertAlmostEqual(elev[share > 0.999].mean(), held["elev"], places=6)
            m = round(P.beat_seconds() * P.FPS)
            flap = [P.flap_angles(k / m) for k in range(m + 1)]
            for got, key in ((elev, "elev"), (wrist, "wrist")):
                own = np.abs(np.diff([a[key] for a in flap])).max()
                self.assertLess(np.abs(np.diff(got)).max(), 1.3 * own, (name, key))

    def test_the_torcaza_wears_its_face_marks_and_wing_spots(self):
        # Black line behind the eye, grey crown over a buff face, black
        # spots on the inner coverts (not on the outer wing), red legs.
        P.configure("torcaza")
        hc, (w, l, h) = R.head_centre(), P.SP["head"]
        e, r = R.eye_centre(), P.SP["eye"]
        side = (1.0, 0.0, 0.0)
        col = paint([(0.86 * w, e[1] + 2.2 * r, e[2] - 0.15 * r), (0.86 * w, e[1] - 0.2 * r, e[2] - 2.2 * r), (0.0, hc[1], hc[2] + 0.99 * h)],
                    [side, side, (0, 0, 1)])
        self.assertLess(lum(col[0]), 0.2)
        self.assertGreater(lum(col[1]), 0.45)
        self.assertLess(abs(col[2][0] - col[2][2]), 0.06)  # grey
        self.assertGreater(col[1][0] - col[1][2], 0.15)  # buff
        # The wing's upper surface: inner coverts against the hand's.
        s, ew, wr, t = R.wing_joints()
        dark = {}
        for name, x0, x1 in (("inner", 1.2 * P.SP["torso"][0], 0.42 * t[0]), ("outer", 0.70 * t[0], 0.95 * t[0])):
            xs = np.linspace(x0, x1, 40)
            pts = [(x, y, R.wing_top(x, y)) for x in xs for y in np.linspace(R.leading_edge()[0][1] + 0.003, R.leading_edge()[0][1] + 0.6 * P.SP["sec"], 40)]
            pts = [p for p in pts if p[2] > -0.5]
            c = paint(pts, [(0, 0, 1)] * len(pts))
            dark[name] = (c.mean(axis=1) < 0.15).mean()
        self.assertGreater(dark["inner"], 0.006)
        self.assertLess(dark["outer"], 0.005)
        hip, knee, heel, foot, toe = R.leg_joints()
        c = paint([R.lerp(heel, foot, 0.5)], [(0, 0, -1)])[0]
        self.assertGreater(c[0] - c[1], 0.3)

    def test_the_legs_fly_tucked_into_the_belly(self):
        # In flight a pigeon draws its legs up into the belly's feathers:
        # the heel and the foot well inside the torso, the clenched toes'
        # tips no further out than its surface.
        for name in SPECIES:
            P.configure(name)
            radii = np.array(P.SP["torso"])
            for side in (1.0, -1.0):
                hip, knee, heel, foot, toe = R.leg_joints(side)
                inside = [float(np.linalg.norm(np.array(j) / radii)) for j in (heel, foot, toe)]
                self.assertLess(max(inside[:2]), 0.9, (name, inside))
                self.assertLess(inside[2], 1.02, (name, inside))

    def test_the_torcaza_bill_is_dark_grey_and_decurved(self):
        # The photographs' slender, slate-grey bill (not black), its tip
        # curving down below the culmen's base.
        P.configure("torcaza")
        bb, tip = np.array(R.bill_base()), np.array(R.bill_tip())
        c = paint([bb + 0.45 * (tip - bb)], [(1, 0, 0)])[0]
        self.assertTrue(0.22 < lum(c) < 0.40, c)
        self.assertLess(abs(c[0] - c[2]), 0.05)
        self.assertLess(tip[2] - bb[2], -0.45 * P.SP["bill"][1])

    def test_the_torcaza_inner_coverts_carry_black_spots_seen_from_above(self):
        # The spotted coverts' cards (their own cells) lie on the inner
        # wing of both wings, and each cell carries a black spot its plain
        # twin lacks.
        P.configure("torcaza")
        span_x = R.wing_joints()[3][0]
        spotted = [fd for fd in R.feather_set() if fd["kind"] in "GM" and fd["index"] > 4]
        self.assertEqual({fd["side"] for fd in spotted}, {1.0, -1.0})
        self.assertLess(max(abs(fd["root"][0]) for fd in spotted), 0.5 * span_x)
        dims = {(fd["kind"], fd["index"]): fd for fd in R.feather_set()}

        def dark(kind, index):
            fd = dims[(kind, index)]
            c, a, _ = R.paint_feather(kind, index, False, fd["length"], fd["width"], fd["outer"], False)
            if index > 4:
                c = P.spot(c, a, kind, index, fd["length"], fd["width"], fd["outer"])
            return float(((c.mean(axis=2) < 0.12) & (a > 0.5)).sum() / (a > 0.5).sum())

        for kind in "GM":
            self.assertLess(dark(kind, 1), 0.005)
            for index in (5, 6, 7):
                self.assertGreater(dark(kind, index), 0.04, (kind, index))

    def test_the_picazuro_coverts_are_pale_fringed(self):
        # Each covert's edge paler than its middle: the photographs' scaly
        # coverts.
        P.configure("picazuro")
        col, alpha, _ = R.paint_feather("M", 1, False, 0.03, 0.03, 0.45, False)
        row = int(0.7 * R.CELL[1])
        cols = np.flatnonzero(alpha[row] > 0.5)
        self.assertGreater(col[row, cols[-1]].mean() - col[row, cols[len(cols) // 2]].mean(), 0.1)

    def test_the_picazuro_neck_is_scaled_on_the_nape_not_the_throat(self):
        # The black-and-white scaled patch on the hind neck and the neck's
        # sides; the throat and the breast plain vinaceous.
        P.configure("picazuro")
        nb, hc = np.array(R.neck_base()), np.array(R.head_centre())
        mid = nb
        hw = P.SP["torso"][0]
        rr = 0.75 * hw
        rng = np.random.default_rng(1)
        nape = mid + np.c_[rng.uniform(-0.3, 0.3, 300) * rr, rng.uniform(-0.002, 0.002, 300), np.full(300, rr)]
        throat = nb + 0.5 * (hc - nb) + np.c_[rng.uniform(-0.3, 0.3, 300) * rr, np.full(300, -0.4 * rr), np.full(300, -rr)]
        cn = paint(nape, np.tile((0, 0, 1), (300, 1)))
        ct = paint(throat, np.tile((0, -0.4, -1), (300, 1)))
        self.assertGreater((cn.mean(axis=1) < 0.2).mean(), 0.4)
        self.assertGreater((cn.mean(axis=1) > 0.6).mean(), 0.05)
        self.assertLess((ct.mean(axis=1) < 0.2).mean(), 0.02)
        self.assertGreater(ct[:, 0].mean() - ct[:, 1].mean(), 0.06)  # vinaceous

    def test_the_picazuro_greater_coverts_carry_the_white_band(self):
        # The greater coverts' cards white on their outer web's edge past
        # their root; the primary coverts' cards plain dark.
        P.configure("picazuro")
        for kind, white in (("G", True), ("U", False)):
            col, alpha, _ = R.paint_feather(kind, 1, False, 0.04, 0.012, 0.45, False)
            outer = col[int(0.75 * R.CELL[1]), 2:int(0.15 * R.CELL[0])]
            if white:
                self.assertGreater(outer.mean(), 0.75)
                self.assertLess(col[int(0.75 * R.CELL[1]), int(0.8 * R.CELL[0])].mean(), 0.5)
            else:
                self.assertLess(col[alpha > 0.5].mean(), 0.35)

    def test_a_black_scan_sheet_is_keyed_and_cut(self):
        # Two feathers on black under a thin white grid that crosses the
        # background and the feathers' translucent down: laid on the blue,
        # the grid gone from both (its crossings too), and cut into two feathers; one whose
        # rachis was found on its vane's edge gets it down its middle.
        h, w = 300, 240
        pic = np.zeros((h, w, 3))
        yy, xx = np.mgrid[0:h, 0:w]
        for cx in (70, 170):
            body = (((xx - cx) / 28.0) ** 2 + ((yy - 140) / 120.0) ** 2) < 1
            pic[body] = (0.42, 0.38, 0.33)
            pic[body & (yy > 230)] = (0.22, 0.20, 0.18)  # the down, translucent over the black
        pic[::40] = (0.75, 0.75, 0.75)
        pic[:, 5::40] = (0.75, 0.75, 0.75)
        spec = dict(background="black", crop=(0, h, 0, w), order=("a", "b"))
        out = P.sheet(pic, spec)
        self.assertTrue(np.allclose(out[80, 125], P.BLUE, atol=0.02))  # a crossing of the grid's lines over the background
        self.assertTrue(np.allclose(out[40, 120], P.BLUE, atol=0.02))
        self.assertLess(abs(out[240, 70] - out[243, 70]).max(), 0.05)  # the grid line through the down erased
        feathers = R.cut_scan(out, 2)
        self.assertEqual(len(feathers), 2)
        f = dict(feathers[0])
        f["shaft"] = lambda r, c=f["box"][2]: np.full_like(np.asarray(r, float), c + 1.0)
        f["left"], f["right"] = 2.0, 50.0
        fixed = P.fix_shaft(f)
        self.assertLess(abs(float(fixed["shaft"](140)) - 70), 6)
        self.assertGreater(min(fixed["left"], fixed["right"]), 0.5 * max(fixed["left"], fixed["right"]))

    def test_the_face_photos_land_eye_on_eye(self):
        # Each photo's eye on the model's eye, the visible eye's radius on
        # the photo's iris radius in every direction (no oval), and the
        # bill pointing the photo's way.
        for name in SPECIES:
            P.configure(name)
            photo = P.FACE_PHOTOS[name]
            f = R.face_mapping()
            e = np.array(R.eye_centre())
            r = P.SP["eye"]
            np.testing.assert_allclose(f(e)[0], photo["points"]["eye"], atol=1e-6)
            for d in ((0, r, 0), (0, 0, r), (0, r / 2 ** 0.5, r / 2 ** 0.5)):
                self.assertAlmostEqual(float(np.linalg.norm(f(e + np.array(d))[0] - f(e)[0])), photo["iris"], places=6)
            culmen = f(np.array(R.face_landmarks()["culmen"]))[0] - f(e)[0]
            want = np.array(photo["points"]["culmen"]) - photo["points"]["eye"]
            self.assertGreater(float(culmen @ want), 0.0)
            crown = f(np.array(R.face_landmarks()["crown"]))[0]
            self.assertLess(crown[1], photo["points"]["eye"][1])  # the crown up

    def test_every_real_pixel_is_free_and_credited(self):
        credits = open(os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "CREDITS.md"), encoding="utf-8").read()
        for spec in list(P.SCANS.values()) + list(P.FACE_PHOTOS.values()):
            self.assertIn(spec["licence"], R.PIXEL_LICENCES)
            self.assertIn(spec["page"], credits)
        for spec in P.SCANS.values():
            for sheet in spec["sheets"].values():
                self.assertIn(sheet["file"], credits)


if __name__ == "__main__":
    unittest.main()
