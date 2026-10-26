"""Tests of tools/leaf_atlas.py: the layout on its own, and the atlas on a
tiny synthetic leaf sheet when ImageMagick is installed.

Standard library only: ``python3 -I -B -m unittest discover -s tools``.
"""

import os
import random
import re
import shutil
import struct
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


def has_basisu():
    return shutil.which("basisu") is not None


TREES = os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "assets", "textures", "trees")


def strokes(side, seed):
    """A `side` x `side` RGBA image of short, mostly opaque strokes one or
    two texels wide on a clear ground, leaf-green: thin like the paraíso's
    leaflets, which plain mips thin out."""
    rng = random.Random(seed)
    alpha = bytearray(side * side)
    for _ in range(side // 2):
        x, y = rng.randrange(side), rng.randrange(side)
        dx, dy = rng.choice(((1, 0), (0, 1), (1, 1)))
        width = rng.choice((1, 2))
        for k in range(rng.randrange(4, side // 3)):
            for w in range(width):
                px, py = (x + k * dx + w * dy) % side, (y + k * dy + w * dx) % side
                alpha[py * side + px] = rng.randrange(160, 256)
    return bytes(v for a in alpha for v in (60, 110, 40, a))


def bc7_levels(ktx2):
    """The levels of `ktx2` transcoded to BC7, as the game's desktop GPU
    holds them: (width, height, RGBA bytes) from the largest down."""
    with tempfile.TemporaryDirectory() as scratch:
        subprocess.run(
            ["basisu", "-unpack", "-no_ktx", "-format_only", "6", "-file", os.path.abspath(ktx2)],
            cwd=scratch, check=True, capture_output=True,
        )
        files = {}
        for name in os.listdir(scratch):
            found = re.search(r"_rgba_BC7_.*level_(\d+)_", name)
            if found:
                files[int(found.group(1))] = os.path.join(scratch, name)
        levels = []
        for level in sorted(files):
            path = files[level]
            size = subprocess.run(
                ["magick", "identify", "-format", "%w %h", path],
                check=True, capture_output=True, text=True,
            ).stdout.split()
            data = subprocess.run(
                ["magick", path, "-depth", "8", "RGBA:-"], check=True, capture_output=True,
            ).stdout
            levels.append((int(size[0]), int(size[1]), data))
        return levels


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


class Mips(unittest.TestCase):
    def test_a_texel_is_drawn_from_half_alpha_up(self):
        # The game's LEAF_CUTOFF, 0.5: 128 of 255 is drawn, 127 is not.
        self.assertEqual(L.coverage(bytes([60, 0, 0, 127, 60, 0, 0, 128])), 0.5)
        self.assertEqual(L.coverage(bytes([0, 0, 0, 255] * 3 + [0, 0, 0, 0])), 0.75)

    def test_rescaled_alpha_draws_the_wanted_share_and_keeps_its_order(self):
        rgba = bytes(v for a in range(0, 256, 4) for v in (9, 8, 7, a))
        for share in (0.0, 0.1, 0.25, 0.5, 0.9):
            out = L.keep_coverage(rgba, share)
            self.assertAlmostEqual(L.coverage(out), share, delta=1 / 64, msg=share)
            # Colour untouched, alpha still rising.
            self.assertEqual(out[0::4], rgba[0::4])
            self.assertEqual(list(out[3::4]), sorted(out[3::4]))

    def test_the_mips_keep_the_cutoff_coverage_of_the_top_level(self):
        side = 128
        levels = L.mip_chain(side, side, strokes(side, 5))
        self.assertEqual([w for w, _, _ in levels], [128, 64, 32, 16, 8, 4, 2, 1])
        top = L.coverage(levels[0][2])
        self.assertGreater(top, 0.1)
        for w, h, data in levels:
            self.assertEqual(len(data), w * h * 4)
            if w >= 8:
                self.assertAlmostEqual(L.coverage(data), top, delta=0.02, msg=w)

    def test_colour_mips_average_in_linear_light(self):
        # Black and white texels average to sRGB 188 (linear 0.5), not 128.
        rgba = bytes([0, 0, 0, 255, 255, 255, 255, 255] * 2)
        (_, _, top), (w, h, mip) = L.mip_chain(2, 2, rgba)
        self.assertEqual(top, rgba)
        self.assertEqual((w, h), (1, 1))
        self.assertEqual(list(mip), [188, 188, 188, 255])

    def test_the_dds_holds_every_level_as_rgba(self):
        levels = L.mip_chain(4, 4, bytes(range(64)))
        with tempfile.TemporaryDirectory() as scratch:
            path = os.path.join(scratch, "x.dds")
            L.write_dds(path, levels)
            with open(path, "rb") as f:
                data = f.read()
        self.assertEqual(data[:4], b"DDS ")
        size, _, height, width, pitch, _, count = struct.unpack("<7I", data[4:32])
        self.assertEqual((size, height, width, pitch, count), (124, 4, 4, 16, 3))
        masks = struct.unpack("<4I", data[92:108])
        self.assertEqual(masks, (0xFF, 0xFF00, 0xFF0000, 0xFF000000))
        self.assertEqual(data[128:], b"".join(d for _, _, d in levels))


@unittest.skipUnless(has_magick() and has_basisu(), "needs ImageMagick 7 and basisu")
class ShippedAtlases(unittest.TestCase):
    def test_distant_crowns_keep_the_coverage_of_the_near_ones(self):
        # As the GPU samples them (BC7), every level of 8 x 8 texels and up
        # draws the share of texels the top level draws, within a tenth:
        # far crowns neither thin out nor fill in.
        for species in ("paraiso", "olmo", "eucalipto", "aromo"):
            levels = bc7_levels(os.path.join(TREES, f"{species}_leaves", "color.ktx2"))
            self.assertEqual(levels[0][0], L.SIZE, species)
            top = L.coverage(levels[0][2])
            for w, _, data in levels:
                if w >= 8:
                    got = L.coverage(data)
                    self.assertAlmostEqual(got, top, delta=0.1 * top, msg=(species, w, got, top))


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
