"""Tests of tools/raptor.py that run without Blender: the two caracaras'
sizes, the wing's feathers and their bones, the wingbeat's timing, the
plumage painted on the body and on the feather atlas, the scanned feathers
cut into the atlas, the face photographs' projection and their credits.

    python3 -I -B -m unittest discover -s tools
"""

import os
import unittest

import numpy as np

import raptor as R

SPECIES = ("chimango", "carancho")


def lum(rgb):
    return float(np.mean(rgb))


class Raptor(unittest.TestCase):
    def tearDown(self):
        R.configure("chimango")

    def test_each_species_spans_and_measures_as_in_the_literature(self):
        # Chimango 37-40 cm long, 85-95 cm across; carancho 50-64 cm and
        # 120-132 cm (bill tip to tail tip; wing tip to wing tip).
        for name in SPECIES:
            R.configure(name)
            lo, hi = R.SP["span"]
            self.assertTrue(lo <= 2 * R.half_span() <= hi, (name, 2 * R.half_span()))
            lo, hi = R.SP["length"]
            self.assertTrue(lo <= R.total_length() <= hi, (name, R.total_length()))

    def test_every_feather_hangs_from_real_bones(self):
        # Each card is skinned to bones of the rig, its weights summing to 1:
        # a name the armature lacks leaves that feather behind when the wing
        # moves.
        bones = set(R.RAPTOR_BONES)
        for name in SPECIES:
            R.configure(name)
            for fd in R.feather_set():
                self.assertTrue(set(fd["bones"]) <= bones, (name, fd["kind"], fd["index"], fd["bones"]))
                self.assertAlmostEqual(sum(fd["bones"].values()), 1.0, places=6)
            for bone, head, tail, parent in R.bone_table():
                self.assertTrue(parent is None or parent in bones, bone)
            self.assertEqual([b[0] for b in R.bone_table()], R.RAPTOR_BONES)

    def test_the_outer_primaries_are_slotted_fingers(self):
        # The caracaras' wing tip: the outer primaries' vanes narrow
        # (emarginate) toward their tips, five fingers on the chimango and
        # six on the carancho; the inner primaries keep their width.
        for name, fingers in (("chimango", 5), ("carancho", 6)):
            R.configure(name)
            narrowed = 0
            for fd in R.remiges():
                if fd["kind"] != "P":
                    continue
                # Near the tip, against the same feather's plain taper.
                width = []
                o = np.linspace(-fd["width"], fd["width"], 801)
                for finger in (False, fd["finger"]):
                    inside, _, _ = R.vane(np.full_like(o, 0.88), o, fd["length"], fd["width"], fd["outer"], "P", fd["index"], finger, 0.0)
                    width.append(inside.mean() * 2 * fd["width"])
                if width[1] < 0.75 * width[0]:
                    narrowed += 1
            self.assertEqual(narrowed, fingers, name)

    def test_the_flap_is_whole_wingbeats_at_a_raptors_pace(self):
        # Pennycuick's allometry for the model's mass, span and wing area
        # gives about 4-5 beats a second; the clip holds whole beats (it
        # loops) to the frame, and each beat's downstroke sweeps the
        # extended wing down while the upstroke flexes it.
        for name in SPECIES:
            R.configure(name)
            f = R.flap_frequency()
            self.assertTrue(3.5 <= f <= 5.5, (name, f))
            beats = R.flap_seconds() * f
            self.assertAlmostEqual(beats, R.FLAP_BEATS, delta=0.5 * f / R.FPS * R.FLAP_BEATS)
            self.assertEqual(R.flap_angles(0.0), R.flap_angles(1.0))
            down = [R.flap_angles(t) for t in np.linspace(0, R.DOWNSTROKE, 12)]
            self.assertTrue(all(a["elev"] >= b["elev"] for a, b in zip(down, down[1:])))
            self.assertTrue(all(a["elbow"] == 0 for a in down))
            up = R.flap_angles(R.DOWNSTROKE + 0.5 * (1 - R.DOWNSTROKE))
            self.assertGreater(up["wrist"], 30.0)

    def test_the_carancho_wears_its_cap_face_and_black_belly(self):
        # Black cap, cream cheeks and throat, orange-red bare face, barred
        # breast, black belly, white undertail coverts.
        R.configure("carancho")
        hc, (w, l, h) = R.head_centre(), R.SP["head"]
        e, r = R.eye_centre(), R.SP["eye"]
        hw, hl, hh = R.SP["torso"]
        pts = {
            "cap": (0.0, hc[1], hc[2] + 0.95 * h),
            "cheek": (0.8 * w, e[1] + 2.8 * r, e[2] - 2.6 * r),
            "face": (0.9 * w, e[1] - 1.5 * r, e[2]),
            "belly": (0.0, 0.35 * hl, -0.98 * hh),
            "vent": (0.0, R.tail_root()[1] + 0.05 * hl, -0.30 * hh),
        }
        down = {"cap": (0, 0, 1), "cheek": (1, 0, 0), "face": (1, 0, 0), "belly": (0, 0, -1), "vent": (0, 0, -1)}
        col, _, _ = R.plumage(np.array(list(pts.values())), np.array([down[k] for k in pts]))
        c = dict(zip(pts, col))
        self.assertLess(lum(c["cap"]), 0.15)
        self.assertGreater(lum(c["cheek"]), 0.65)
        self.assertGreater(c["face"][0], 0.75)
        self.assertLess(c["face"][2], 0.30)
        self.assertLess(lum(c["belly"]), 0.15)
        self.assertGreater(lum(c["vent"]), 0.75)
        # The breast barred: dark and pale bands down its front.
        zs = np.linspace(-0.2 * hh, -0.7 * hh, 60)
        breast = np.stack([np.zeros_like(zs), np.full_like(zs, -0.45 * hl), zs], axis=1)
        col, _, _ = R.plumage(breast, np.tile((0.0, -0.6, -0.8), (len(zs), 1)))
        lums = col.mean(axis=1)
        self.assertGreater((lums < 0.3).mean(), 0.15)
        self.assertGreater((lums > 0.6).mean(), 0.15)

    def test_the_chimango_has_its_eye_stripe_and_pale_vent(self):
        # A dark brown stripe back from the eye on a buff head; the vent and
        # undertail coverts paler than the brown mantle.
        R.configure("chimango")
        e, r = R.eye_centre(), R.SP["eye"]
        w = R.SP["head"][0]
        hw, hl, hh = R.SP["torso"]
        pts = np.array([
            (0.85 * w, e[1] + 2.0 * r, e[2] - 0.6 * r),  # the stripe
            (0.85 * w, e[1] + 2.0 * r, e[2] - 3.0 * r),  # the cheek under it
            (0.0, R.tail_root()[1] + 0.05 * hl, -0.30 * hh),  # vent
            (0.0, -0.30 * hl, 0.98 * hh),  # mantle
        ])
        nrm = np.array([(1, 0, 0), (1, 0, 0), (0, 0, -1), (0, 0, 1)])
        col, _, _ = R.plumage(pts, nrm)
        stripe, cheek, vent, mantle = (lum(c) for c in col)
        self.assertLess(stripe, cheek - 0.2)
        self.assertGreater(vent, mantle + 0.15)

    def test_the_tails_and_the_carancho_window(self):
        # Chimango: a pale barred tail, a broad dark band, then a pale tip.
        # Carancho: a white barred tail ending in a broad black band, and
        # a pale window across the outer primaries before their black tips.
        def along(name, kind, index, under=False):
            R.configure(name)
            fd = next(f for f in R.feather_set() if f["kind"] == kind and f["index"] == index)
            col, alpha, _ = R.paint_feather(kind, index, under, fd["length"], fd["width"], fd["outer"], fd["finger"])
            mid = col[:, int(R.CELL[0] * (fd["outer"] + 0.25))]  # down the inner vane
            return lambda t: lum(mid[int(t * (R.CELL[1] - 1))])

        tail = along("chimango", "R", 3)
        self.assertGreater(np.mean([tail(t) for t in np.linspace(0.3, 0.6, 20)]), 0.55)
        self.assertLess(tail(0.85), 0.30)
        self.assertGreater(tail(0.99), 0.60)
        tail = along("carancho", "R", 3)
        self.assertGreater(np.mean([tail(t) for t in np.linspace(0.3, 0.6, 20)]), 0.55)
        self.assertLess(tail(0.88), 0.15)
        prim = along("carancho", "P", 8)
        self.assertGreater(np.mean([prim(t) for t in np.linspace(0.25, 0.55, 20)]), 0.45)
        self.assertLess(prim(0.85), 0.20)

    def test_the_atlas_holds_every_card(self):
        # Every card's cell exists and no two cards' kinds share one.
        cells = R.atlas_cells()
        self.assertLessEqual(len(cells), (R.ATLAS[0] // R.CELL[0]) * (R.ATLAS[1] // R.CELL[1]))
        for name in SPECIES:
            R.configure(name)
            for fd in R.feather_set():
                for under in fd.get("layers", (False, True)):
                    self.assertIn((fd["kind"], fd["index"], under), cells)

    def test_the_light_mesh_keeps_the_flight_outline(self):
        # Far off a raptor is its outline: the light mesh keeps every
        # primary, secondary and tail feather, root to tip, both sides, in
        # under a thousand triangles with its skin.
        for name in SPECIES:
            R.configure(name)
            light = R.light_feathers()
            for kind in "PSR":
                whole = [fd for fd in R.feather_set() if fd["kind"] == kind]
                self.assertEqual(len([fd for fd in light if fd["kind"] == kind]), len(whole), (name, kind))
            self.assertEqual((R.LIGHT_ROWS[0], R.LIGHT_ROWS[-1]), (0, len(R.CARD_T) - 1))
            self.assertLessEqual(R.LIGHT_BODY + R.light_triangles(), 1000, name)

    def test_the_carancho_neck_bars_ring_the_neck(self):
        # On the neck's side the bars run square to the neck all round it
        # (the photographs' straight rings), not bent into contour lines by
        # the breast's and the mantle's planes: the barring's phase changes
        # along the neck's axis only.
        R.configure("carancho")
        nb, hc, ax = (np.asarray(v) for v in (R.neck_base(), R.head_centre(), R.neck_axis()))
        side = np.cross(ax, (0.0, 0.0, 1.0))
        side /= np.linalg.norm(side)
        up = np.cross(side, ax)
        r = 0.6 * R.SP["neck"] / 2
        P = np.array([nb + (hc - nb) * t + r * (abs(np.cos(a)) * -side + np.sin(a) * up)
                      for t in np.linspace(0.2, 0.8, 7) for a in np.linspace(-1.2, 1.2, 9)])
        e = 1e-5
        grad = np.stack([(R.bar_phase(P + e * d) - R.bar_phase(P - e * d)) / (2 * e) for d in np.eye(3)], axis=1)
        along = np.abs(grad @ ax) / np.linalg.norm(grad, axis=1)
        self.assertGreater(np.median(along), 0.95)
        self.assertGreater(np.percentile(along, 10), 0.85)

    def test_the_coverts_lie_on_the_wing_and_the_back(self):
        # The greater, median and lesser coverts and the scapulars rest just
        # over the surface they lie on (v3: the greater coverts' cards sat
        # inside the wing's slab, so the coverts read as a flat plate).
        for name in SPECIES:
            R.configure(name)
            lift = R.COVER_LIFT * R.bird_scale()
            for fd in R.feather_set():
                if fd["kind"] not in "GMLK":
                    continue
                x, y, z = fd["root"]
                top = R.wing_top(x, y) if fd["kind"] != "K" else max(R.back_top(x, y), R.wing_top(x, y) if abs(x) > 0.9 * R.SP["torso"][0] else -1.0)
                self.assertTrue(0.0 < z - top <= 1.5 * lift, (name, fd["kind"], fd["index"], z - top))

    def test_a_scan_sheet_is_cut_into_its_feathers(self):
        # Two feathers on the scans' blue, outer (left) vane brown, inner
        # vane white, a pale rachis, and a short label: two feathers left to
        # right; each card's outer side takes the outer vane (the other side
        # when mirrored), nothing of the background.
        h, w = 400, 300
        pic = np.tile((0.35, 0.47, 0.57), (h, w, 1))
        yy, xx = np.mgrid[0:h, 0:w]
        for cx in (90, 200):
            body = (((xx - cx) / 30.0) ** 2 + ((yy - 190) / 150.0) ** 2) < 1
            pic[body & (xx < cx)] = (0.30, 0.20, 0.12)
            pic[body & (xx >= cx)] = (0.92, 0.91, 0.88)
            pic[(np.abs(xx - cx) <= 1) & (yy > 50) & (yy < 380)] = (1.0, 1.0, 0.95)
        pic[370:390, 20:60] = (1.0, 1.0, 1.0)  # a label
        feathers = R.cut_scan(pic, 2)
        self.assertLess(feathers[0]["box"][3], feathers[1]["box"][2])
        with self.assertRaises(ValueError):
            R.cut_scan(pic, 3)
        fo = 0.35
        for flip in (False, True):
            col, alpha, _ = R.feather_cell(pic, feathers[1], fo, flip=flip)
            mid = slice(R.CELL[1] // 3, 2 * R.CELL[1] // 3)
            outer = col[mid, int(0.15 * R.CELL[0])].mean()
            inner = col[mid, int(0.85 * R.CELL[0])].mean()
            self.assertLess(outer, inner - 0.4) if not flip else self.assertGreater(outer, inner + 0.4)
            self.assertGreater(alpha[mid].mean(), 0.8)
            self.assertLess(alpha[-3:, :].mean(), 0.6)  # the tip's rounded end

    def test_the_face_photos_land_on_the_head(self):
        # Each photo's landmarks land exactly on their pixels; the photo
        # covers the head's side, both sides alike, and nothing where the
        # skin faces away from the side (the crown).
        for name in SPECIES:
            R.configure(name)
            photo = R.FACE_PHOTOS[name]
            f = R.face_mapping()
            marks = R.face_landmarks()
            for k, px in photo["points"].items():
                self.assertLess(np.hypot(*(f([marks[k]])[0] - px)), 0.5, (name, k))
            e = np.asarray(R.eye_centre())
            r = R.SP["eye"]
            P = np.array([e + (0.0, -1.6 * r, -0.8 * r), e * (-1, 1, 1) + (0.0, -1.6 * r, -0.8 * r), e + (0.0, -1.6 * r, -0.8 * r)])
            N = np.array([(1.0, 0, 0), (-1.0, 0, 0), (0, 0, 1.0)])
            wt, _ = R.photo_weight(P, N, f)
            self.assertGreater(wt[0], 0.9, name)
            self.assertAlmostEqual(wt[0], wt[1], places=6)
            self.assertEqual(wt[2], 0.0)

    def test_the_carancho_photo_is_warmed_to_orange(self):
        # The northern bird's red-orange face turns orange, its cere's
        # toward yellow; the cream cheeks keep their colour.
        R.configure("carancho")
        photo = R.FACE_PHOTOS["carancho"]
        nx, ny = photo["points"]["nostril"]
        pic = np.zeros((ny + 400, nx + 400, 3))
        pic[...] = (0.64, 0.21, 0.0)
        pic[:20, :20] = (0.92, 0.86, 0.75)  # the cream cheek
        out = R.warm_photo(pic, photo)

        def hue(c):
            r, g, b = c
            return 60.0 * (g - b) / (max(c) - min(c))

        self.assertGreater(hue(out[ny, nx]), hue(out[ny + 399, 0]) + 10)
        self.assertGreater(hue(out[ny + 399, 0]), hue(pic[0, 30]) + 5)
        np.testing.assert_allclose(out[:20, :20], pic[:20, :20])

    def test_the_chimango_photo_fades_out_without_a_border(self):
        # v5: the photo's weight fades smoothly from the cheek back down
        # the neck (a hard edge read as a pale patch's straight border).
        R.configure("chimango")
        e, r = np.asarray(R.eye_centre()), R.SP["eye"]
        ys = np.arange(-2 * r, 0.045, 0.00025)
        P = np.stack([np.full_like(ys, e[0]), e[1] + ys, np.full_like(ys, e[2] - 2.5 * r)], axis=1)
        w, _ = R.photo_weight(P, np.tile((1.0, 0.0, 0.0), (len(ys), 1)), R.face_mapping())
        self.assertGreater(w.max(), 0.6)
        self.assertEqual(w[-1], 0.0)
        self.assertLess(np.abs(np.diff(w)).max(), 0.05)

    def test_the_chimango_head_fades_into_its_neck(self):
        # v5: the buff head's colour fades back into the browner neck
        # (a hard cut behind the eye read as a straight border on the cheek
        # and the neck): averaged over 2 mm and over a band of heights, the
        # tone steps little from one window to the next.
        R.configure("chimango")
        e, r, w = np.asarray(R.eye_centre()), R.SP["eye"], R.SP["head"][0]
        ys = np.arange(0.0, 0.05, 0.0001)
        rng = np.random.default_rng(0)
        for dz in (2.5, 4.0, 5.5):  # under the eye's stripe
            rows = []
            for zz in e[2] - dz * r + rng.uniform(-0.002, 0.002, 16):
                P = np.stack([np.full_like(ys, 0.85 * w), e[1] + ys, np.full_like(ys, zz)], axis=1)
                col, _, _ = R.plumage(P, np.tile((1.0, 0.0, 0.0), (len(ys), 1)))
                rows.append(col.mean(axis=1))
            win = np.mean(rows, axis=0)[:480].reshape(-1, 20).mean(axis=1)
            self.assertGreater(win[0] - win[-1], 0.05, dz)
            self.assertLess(np.abs(np.diff(win)).max(), 0.08, dz)

    def test_the_chimango_photo_keeps_the_painted_colours(self):
        # Its photo is another population's (and its bill reads greenish):
        # only its luminance detail is taken, so where it lies the head
        # keeps the painted colours, on average and on the bill.
        R.configure("chimango")
        f = R.face_mapping()
        e, r = np.asarray(R.eye_centre()), R.SP["eye"]
        bb = np.asarray(R.bill_base())
        rng = np.random.default_rng(0)
        P = np.concatenate([e + rng.uniform(-3, 3, (400, 3)) * r * (0.0, 1.0, 1.0), bb + rng.uniform(0.3, 0.6, (200, 3)) * (0.0, -R.SP["bill"][0], 0.0)])
        P[:, 0] = np.where(np.arange(len(P)) < 400, e[0], 0.4 * R.SP["bill"][2])
        N = np.tile((1.0, 0.0, 0.0), (len(P), 1))
        col, height, _ = R.plumage(P, N)
        pic = np.clip((0.45, 0.55, 0.30) * (0.8 + 0.4 * rng.random((800, 2048, 1))), 0, 1)
        out, _, w = R.project_face(P, N, col, height, pic, 0.0002)
        on = w > 0.5
        self.assertGreater(on.sum(), 100)
        np.testing.assert_allclose(out[on].mean(axis=0), col[on].mean(axis=0), rtol=0.05)
        bill = on & R.regions(P, N)["bill"]
        self.assertGreater(bill.sum(), 20)
        self.assertGreaterEqual(out[bill, 2].mean(), out[bill, 0].mean() - 0.01)

    def test_the_chimango_bill_is_grey_horn_and_its_cere_warm(self):
        # Pale bluish-grey horn with a darker hook; the cere pale yellow to
        # pinkish orange.
        R.configure("chimango")
        L, D, Wb = R.SP["bill"]
        bb = np.asarray(R.bill_base())
        pts = np.array([bb + (0.0, -0.15 * L, 0.45 * D), bb + (0.0, -0.95 * L, -0.55 * D), bb + (0.0, 0.5 * R.SP["cere"], 0.5 * D)])
        col, _, _ = R.plumage(pts, np.tile((0.0, 0.0, 1.0), (3, 1)))
        base, tip, cere = col
        self.assertGreater(lum(base), 0.6)
        self.assertGreaterEqual(base[2], base[0] - 0.01)
        self.assertLess(lum(tip), lum(base) - 0.2)
        self.assertGreater(cere[0], 0.8)
        self.assertGreater(cere[0] - cere[2], 0.25)

    def test_the_chimango_breast_is_streaked(self):
        # Brown shaft streaks on some breast feathers and not others (v5:
        # one repeated scallop, the references' streaks missing).
        R.configure("chimango")
        hw, hl, hh = R.SP["torso"]
        ys, zs = np.meshgrid(np.linspace(-0.55 * hl, -0.05 * hl, 120), np.linspace(-0.85 * hh, -0.35 * hh, 120))
        P = np.stack([np.full(ys.size, 0.2 * hw), ys.ravel(), zs.ravel()], axis=1)
        col, _, _ = R.plumage(P, np.tile((0.0, -0.3, -0.95), (len(P), 1)))
        lums = col.mean(axis=1)
        dark = (lums < np.median(lums) - 0.12).mean()
        self.assertTrue(0.03 < dark < 0.35, dark)

    def test_the_chimango_wing_cards_have_no_bare_quills(self):
        # The cards' roots reach past the wing's leading edge: no opaque
        # quill before the vane starts (v5 review: needles there).
        R.configure("chimango")
        for fd in R.remiges():
            _, alpha, _ = R.paint_feather(fd["kind"], fd["index"], False, fd["length"], fd["width"], fd["outer"], fd["finger"])
            self.assertEqual(alpha[: int(0.05 * R.CELL[1])].max(), 0.0, (fd["kind"], fd["index"]))

    def test_the_chimango_atlas_takes_the_scans_grain(self):
        # The chimango has no scans of its own: its painted cells take the
        # carancho's scanned secondaries' fine luminance only, so their
        # colours keep their mean and their pattern, and none is replaced.
        R.configure("chimango")
        h, w = 600, 1000
        pic = np.tile((0.35, 0.47, 0.57), (h, w, 1))
        yy, xx = np.mgrid[0:h, 0:w]
        stripes = 0.20 + 0.10 * (np.sin(yy * 1.7 + xx * 0.6) > 0)
        for k in range(10):
            cx = 50 + 100 * k
            body = (((xx - cx) / 35.0) ** 2 + ((yy - 300) / 260.0) ** 2) < 1
            pic[body] = np.stack([stripes, 0.8 * stripes, 0.6 * stripes], -1)[body]
            pic[(np.abs(xx - cx) <= 1) & (yy > 60) & (yy < 560)] = (0.9, 0.9, 0.85)
        dims = {}
        for fd in R.feather_set():
            dims.setdefault((fd["kind"], fd["index"]), fd)
        self.assertEqual(R.scan_cells({"S": pic}, dims), {})
        grain = R.scan_grain({"S": pic}, dims)
        fd = dims[("P", 4)]
        col, alpha, height = R.paint_feather("P", 4, False, fd["length"], fd["width"], fd["outer"], fd["finger"])
        out, _ = R.regrain(col, alpha, height, grain[("P", 4)])
        inside = alpha > 0
        ratio = np.log(np.maximum(out[inside], 1e-3) / np.maximum(col[inside], 1e-3))
        self.assertGreater(ratio.std(), 0.04)
        self.assertLess(abs(ratio.mean()), 0.03)

    def test_every_real_pixel_is_free_and_credited(self):
        # The scans and the photographs whose pixels the maps carry: each
        # under a licence that allows it, each credited in CREDITS.md.
        credits = open(os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "CREDITS.md"), encoding="utf-8").read()
        for spec in list(R.SCANS.values()) + list(R.FACE_PHOTOS.values()):
            self.assertIn(spec["licence"], R.PIXEL_LICENCES)
            self.assertTrue(spec["page"] in credits, spec["page"])


if __name__ == "__main__":
    unittest.main()
