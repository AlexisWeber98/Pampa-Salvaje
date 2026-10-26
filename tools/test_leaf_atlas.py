"""Tests of tools/leaf_atlas.py: the layout on its own, and the atlas on a
tiny synthetic leaf sheet when ImageMagick is installed.

Standard library only: ``python3 -I -B -m unittest discover -s tools``.
"""

import os
import shutil
import subprocess
import tempfile
import unittest

import leaf_atlas as L

LISTING = """Objects (id: bounding-box centroid area mean-color):
  0: 1024x1024+0+0 520.9,511.1 844393 gray(0)
  1: 178x429+66+37 155.2,231.6 41477 gray(255)
  4: 180x404+410+567 499.3,757.1 40079 gray(255)
  7: 20x20+5+5 15.0,15.0 400 gray(255)
  9: 40x50+683+848 700.6,870.4 743 gray(0)
"""


def sheet_leaves(kind):
    return L.leaves([(66, 37, 178, 429), (410, 567, 180, 404)], kind)


def has_magick():
    return shutil.which("magick") is not None


def pixel(path, x, y):
    out = subprocess.run(
        ["magick", path, "-format",
         f"%[fx:round(255*p{{{x},{y}}}.r)],%[fx:round(255*p{{{x},{y}}}.g)],"
         f"%[fx:round(255*p{{{x},{y}}}.b)]", "info:"],
        check=True, capture_output=True, text=True,
    ).stdout
    return tuple(int(v) for v in out.split(","))


class Layout(unittest.TestCase):
    def test_only_the_big_white_objects_are_leaves(self):
        self.assertEqual(
            L.components(LISTING), [(66, 37, 178, 429), (410, 567, 180, 404)]
        )

    def test_a_turned_normal_map_turns_its_vectors_with_it(self):
        def apply(angle, rgb):
            values = [float(v) for v in L.normal_matrix(angle).split(":")[1].split()]
            rows = [values[6 * r: 6 * r + 6] for r in range(3)]
            return [sum(row[c] * rgb[c] for c in range(3)) + row[5] for row in rows]

        # Pointing right (x+), turned a quarter clockwise: pointing down
        # (y-, as OpenGL's y is up).
        r, g, b = apply(90, [1.0, 0.5, 0.7])
        self.assertAlmostEqual(r, 0.5, places=5)
        self.assertAlmostEqual(g, 0.0, places=5)
        self.assertAlmostEqual(b, 0.7, places=5)
        # Pointing up (y+), turned a quarter clockwise: pointing right.
        r, g, _ = apply(90, [0.5, 1.0, 1.0])
        self.assertAlmostEqual(r, 1.0, places=5)
        self.assertAlmostEqual(g, 0.5, places=5)
        # Pointing up, turned a half turn: pointing down.
        r, g, _ = apply(180, [0.5, 1.0, 1.0])
        self.assertAlmostEqual(r, 0.5, places=5)
        self.assertAlmostEqual(g, 0.0, places=5)
        # Unturned: untouched.
        self.assertEqual([round(v, 5) for v in apply(0, [0.2, 0.3, 0.9])], [0.2, 0.3, 0.9])

    def test_every_sprig_keeps_inside_its_quarter_and_is_reproducible(self):
        for kind in ("pinnate", "alternate", "hanging", "spray"):
            cut = sheet_leaves(kind)
            stamps, stems = L.layout(kind, cut, 7)
            self.assertEqual((stamps, stems), L.layout(kind, cut, 7), kind)
            self.assertNotEqual(stamps, L.layout(kind, cut, 8)[0], kind)
            quarters = set()
            for s in stamps:
                q = (int(s.at[0] // L.CELL), int(s.at[1] // L.CELL))
                quarters.add(q)
                lo = (q[0] * L.CELL + L.MARGIN - 1e-3, q[1] * L.CELL + L.MARGIN - 1e-3)
                hi = (lo[0] + L.CELL - 2 * L.MARGIN + 2e-3, lo[1] + L.CELL - 2 * L.MARGIN + 2e-3)
                for x, y in L.placed(cut[s.leaf], s):
                    self.assertTrue(lo[0] <= x <= hi[0] and lo[1] <= y <= hi[1], (kind, s, x, y))
            self.assertEqual(len(quarters), 4, kind)
            # Each sprig grows from its quarter's base.
            bases = {
                (q % 2 * L.CELL + L.BASE[0], q // 2 * L.CELL + L.BASE[1]) for q in range(4)
            }
            if kind == "spray":
                self.assertEqual({s.at for s in stamps}, bases)
            else:
                self.assertEqual({a for a, _, _, _ in stems[:: len(stems) // 4]}, bases)


@unittest.skipUnless(has_magick(), "needs ImageMagick 7")
class Atlas(unittest.TestCase):
    def setUp(self):
        self.dir = tempfile.mkdtemp()
        src = os.path.join(self.dir, "LeafSetTest")
        os.mkdir(src)
        self.src = src

        def make(name, fill, leaves):
            args = ["magick", "-size", "256x256", f"xc:{fill}"]
            # Two upright oval leaves.
            for x, y in ((65, 120), (175, 130)):
                args += ["-fill", leaves, "-draw", f"ellipse {x},{y} 25,90 0,360"]
            subprocess.run(args + [os.path.join(src, name)], check=True)

        make("opacity.jpg", "black", "white")
        make("color.jpg", "rgb(200,30,30)", "rgb(40,140,40)")
        # Every leaf texel's normal points right (+x).
        make("normal_gl.jpg", "rgb(128,128,255)", "rgb(255,128,128)")
        make("roughness.jpg", "gray(50%)", "gray(80%)")

    def tearDown(self):
        shutil.rmtree(self.dir)

    def test_a_leaf_turned_clockwise_lands_there_with_its_normals_turned(self):
        cut = L.find_leaves(self.src, "alternate")
        self.assertEqual(len(cut), 2)
        out = os.path.join(self.dir, "normal.png")
        stamp = L.Stamp(0, (300.0, 600.0), 90.0, 1.0)
        L.magick(
            "-size", f"{L.SIZE}x{L.SIZE}", "xc:rgb(128,128,255)",
            *L._layers(self.src, "normal_gl.jpg", cut, [stamp], True),
            "-background", "rgb(128,128,255)", "-flatten", "-alpha", "off", out,
        )
        # A quarter clockwise, the leaf reaches right of its base, its
        # normals pointing down (y-) in OpenGL's terms.
        r, g, b = pixel(out, 300 + 90, 600)
        self.assertLess(abs(r - 128), 12, (r, g, b))
        self.assertLess(g, 20, (r, g, b))
        # Nothing to its left.
        self.assertEqual(pixel(out, 300 - 90, 600), (128, 128, 255))

    def test_each_leaf_is_cut_out_along_its_outline(self):
        cut = L.find_leaves(self.src, "alternate")
        out = os.path.join(self.dir, "alpha.png")
        stamps = [L.Stamp(k, (200.0 + 300 * k, 400.0), 0.0, 1.0) for k in range(2)]
        L.magick(
            "-size", f"{L.SIZE}x{L.SIZE}", "xc:none",
            *L._layers(self.src, "color.jpg", cut, stamps, False),
            "-background", "none", "-flatten", "-alpha", "extract", out,
        )
        for k, s in enumerate(stamps):
            leaf = cut[s.leaf]
            # The middle of the oval is opaque, its box's corners clear.
            mid = (int(s.at[0]), int(s.at[1] - leaf.h / 2))
            self.assertGreater(pixel(out, *mid)[0], 240, k)
            for dx in (-1, 1):
                corner = (int(s.at[0] + dx * (leaf.w / 2 - 6)), int(s.at[1] - leaf.h + 10))
                self.assertLess(pixel(out, *corner)[0], 10, (k, corner))

    def test_the_atlas_holds_four_sprigs_with_empty_margins(self):
        out = os.path.join(self.dir, "test_leaves")
        L.build(self.src, out, "alternate", 3)
        for name in ("color.jpg", "opacity.jpg", "normal_gl.jpg", "roughness.jpg"):
            path = os.path.join(out, name)
            size = subprocess.run(
                ["magick", "identify", "-format", "%wx%h", path],
                check=True, capture_output=True, text=True,
            ).stdout
            self.assertEqual(size, f"{L.SIZE}x{L.SIZE}", name)
        opacity = os.path.join(out, "opacity.jpg")
        for x, y in ((2, 2), (L.CELL, L.CELL), (L.CELL - 4, 300), (300, L.CELL + 4)):
            self.assertLess(pixel(opacity, x, y)[0], 10, (x, y))
        for q in range(4):
            # Each quarter's stem starts at its base.
            x = q % 2 * L.CELL + int(L.BASE[0])
            y = q // 2 * L.CELL + int(L.BASE[1]) - 3
            self.assertGreater(pixel(opacity, x, y)[0], 200, q)
        # Outside the leaves the colour is the leaves' own, not the
        # sheet's red ground.
        r, g, _ = pixel(os.path.join(out, "color.jpg"), 4, 4)
        self.assertGreater(g, r)


if __name__ == "__main__":
    unittest.main()
