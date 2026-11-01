"""Tests of tools/cattle.py that run without Blender: the coats the game
will paint (``color_at``, in the body frame) and the gaits' ground speeds.

    python3 -I -B -m unittest discover -s tools
"""

import unittest

import cattle as C


def body(p):
    """Blender metres (facing -Y, Z up) to the game's body frame."""
    return (-p[1], p[2], -p[0])


# Points on the cow at rest, in the layout's Blender metres (``cattle.warp``
# puts them on the model), with their outward normal.
FACE = (C._at(0.30, 0.07), (0.0, -0.6, 0.8))
BARREL = ((0.30, 0.0, 1.0), (1.0, 0.0, 0.0))
BACK = ((0.0, 0.2, 1.39), (0.0, 0.0, 1.0))
CANNON = ((0.22, -0.63, 0.25), (1.0, 0.0, 0.0))
BRISKET = ((0.0, -0.80, 0.68), (0.0, -0.5, -0.8))
BELLY = ((0.0, 0.05, 0.58), (0.0, 0.0, -1.0))
NECK = ((0.17, -0.85, 1.10), (1.0, 0.0, 0.0))


def colour(coat, point):
    """The game's paint at a layout point, on the model (legs shortened)."""
    p, n = point
    return C.color_at(coat, body(C.warp(p)), body(n))


def whiteness(rgb):
    return min(rgb) / 255.0


class Coats(unittest.TestCase):
    def test_a_hereford_is_red_with_a_white_face_brisket_underline_and_lower_legs(self):
        for point in (FACE, CANNON, BRISKET, BELLY):
            self.assertGreater(whiteness(colour("hereford", point)), 0.8, point)
        for point in (BARREL, BACK, NECK):
            r, g, b = colour("hereford", point)
            self.assertTrue(r > 2 * g and r > 2 * b, (point, (r, g, b)))

    def test_an_angus_is_black_all_over(self):
        for point in (FACE, BARREL, BACK, CANNON, BRISKET, BELLY, NECK):
            self.assertLess(max(colour("angus", point)), 40, point)

    def test_a_roan_mixes_red_and_white_redder_on_the_head_than_under_the_belly(self):
        shares = [C.roan_white((0.30, y / 10, 1.0), (1.0, 0.0, 0.0)) for y in range(-6, 8)]
        mean = sum(shares) / len(shares)
        # A roan's body: mostly white hairs with red ones among them (the
        # reference photographs' roan Shorthorns), neither red nor white.
        self.assertTrue(0.45 < mean < 0.85, mean)
        self.assertGreater(max(shares) - min(shares), 0.1, "flecked, not flat")
        self.assertLess(C.roan_white(FACE[0], FACE[1]), C.roan_white(*BELLY))

    def test_an_overo_has_a_black_face_and_legs_and_the_games_patches(self):
        # Points off the game's patches: black only because they are the face
        # and a lower leg.
        face, hock = (C._at(0.35, 0.04), FACE[1]), ((-0.22, 0.80, 0.30), (-1.0, 0.0, 0.0))
        for point in (face, hock):
            self.assertFalse(C.overo_black(body(C.warp(point[0]))))
            self.assertLess(max(colour("overo", point)), 40, point)
        for point in (BARREL, BACK, NECK, BELLY):
            black = C.overo_black(body(C.warp(point[0])))
            self.assertEqual(max(colour("overo", point)) < 40, black, point)


    def test_the_udder_is_bare_skin_in_every_coat_fading_into_the_belly(self):
        quarter = ((0.144, 0.43, 0.60), (1.0, 0.0, 0.0))  # on its surface
        teat = ((0.068, 0.42, 0.46), (0.0, 0.0, -1.0))
        for coat in C.COATS:
            skin = C.COATS[coat]["skin"]
            for point in (quarter, teat):
                self.assertEqual(colour(coat, point), skin, (coat, point))
            # The flank above it keeps the coat.
            self.assertNotEqual(colour(coat, ((0.30, 0.45, 0.85), (1.0, 0.0, 0.0))), skin, coat)
        # A soft edge: between the udder and the belly the colour is between.
        edge = C.udder_skin((0.0, 0.50, 0.745))
        self.assertTrue(0.0 < edge < 1.0, edge)


    def test_the_legs_are_shortened_and_read_back(self):
        # Short legs: the belly drops, the claws stay on the ground, and a
        # model point reads back to its layout point (what the coats use).
        self.assertEqual(C.warp((0.2, -0.6, 0.05)), (0.2, -0.6, 0.05))
        self.assertLess(C.warp((0.0, 0.0, 1.4))[2], 1.4 - 0.05)
        for z in (0.05, 0.3, 0.6, 0.9, 1.4):
            self.assertAlmostEqual(C.unwarp(C.warp((0.0, 0.0, z)))[2], z)


class CoatMaps(unittest.TestCase):
    def test_a_coat_map_is_the_coat_times_the_luminance_and_a_roan_follows_its_hair_map(self):
        import numpy as np

        # The face, the barrel and a texel no face covers, on the model.
        pos = np.array([[C.warp(FACE[0]), C.warp(BARREL[0]), (0.0, 0.0, 0.0)]])
        nrm = np.array([[FACE[1], BARREL[1], (-1.0, -1.0, -1.0)]])
        white = np.ones((1, 3, 3))
        out = np.round(C.coat_pixels("hereford", white, pos, nrm) * 255).astype(int)
        self.assertEqual(tuple(out[0, 0]), colour("hereford", FACE))
        self.assertEqual(tuple(out[0, 1]), colour("hereford", BARREL))
        self.assertEqual(tuple(out[0, 2]), C.COATS["hereford"]["body"])
        # The luminance darkens it as the game multiplies them (linear light):
        # sRGB grey 0.5 is 21 % of white.
        grey = C.coat_pixels("hereford", white * 0.5, pos, nrm)
        self.assertAlmostEqual(float(C.to_linear(grey[0, 0, 1])), 0.214 * float(C.to_linear(out[0, 0, 1] / 255)), 3)
        # A roan's texel is red or white as its hair map says.
        roan = np.round(C.coat_pixels("roan", white[:, :2], pos[:, 1:3], nrm[:, :2], roan=np.array([[0.0, 1.0]])) * 255)
        self.assertEqual(tuple(roan[0, 0].astype(int)), C.COATS["roan"]["body"])
        self.assertEqual(tuple(roan[0, 1].astype(int)), C.COATS["roan"]["white"])


    def test_a_roan_reads_steel_grey_not_pink_from_afar(self):
        import numpy as np

        # Its red and white hairs as the eye averages them (linear light) at
        # the body's share of white: a cool grey, as the game's warm light
        # keeps about all of the red and three quarters of the blue (measured
        # on its shots); under it, the grey of the reference roans (red over
        # green and green over blue by under a fifth), never pink.
        roan = C.COATS["roan"]
        red, white = (C.to_linear(np.array(roan[k]) / 255.0) for k in ("body", "white"))
        lit = C.to_srgb(red + (white - red) * 0.55) * 255 * np.array([1.01, 0.87, 0.73])
        r, g, b = lit
        self.assertTrue(1.0 <= r / g < 1.1 and 1.0 <= g / b < 1.15, lit)
        # The red hairs a dark, dull maroon: red, never a bright red.
        body = roan["body"]
        self.assertTrue(body[0] > body[1] and max(body) - min(body) < 30, body)


class Gaits(unittest.TestCase):
    def test_the_walk_and_the_gallop_slide_at_a_cows_speeds(self):
        # A cow walks at about 1 m/s and gallops at about 6 m/s.
        self.assertTrue(0.9 <= C.ground_speed(C.WALK) <= 1.3, C.ground_speed(C.WALK))
        self.assertTrue(5.0 <= C.ground_speed(C.GALLOP) <= 7.0, C.ground_speed(C.GALLOP))


if __name__ == "__main__":
    unittest.main()
