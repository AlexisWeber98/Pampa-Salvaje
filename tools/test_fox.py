"""Tests of tools/fox.py that run without Blender: the coat painted on the
sculpt (``coat_at``), the colours of the cards' hairs, the gaits' ground
speeds and the skeleton the game's livestock path needs.

    python3 -I -B -m unittest discover -s tools
"""

import unittest

import fox as F


def lum(rgb):
    return sum(rgb) / 3.0


def warm(rgb):
    """How much redder than blue (0..255)."""
    return rgb[0] - rgb[2]


# Points on the fox at rest (Blender metres, facing -Y) with their outward
# normal.
FLANK = ((0.062, 0.0, 0.30), (1.0, 0.0, 0.0))
BACK = ((0.0, 0.0, 0.372), (0.0, 0.0, 1.0))
BELLY = ((0.0, 0.0, 0.225), (0.0, 0.0, -1.0))
THROAT = ((0.0, -0.245, 0.300), (0.0, -0.6, -0.8))
CHIN = (F._at(0.11, -0.062), (0.0, -0.3, -0.95))
FORE_OUT = ((0.058, -0.172, 0.14), (1.0, 0.0, 0.0))
FORE_IN = ((0.032, -0.172, 0.14), (-1.0, 0.0, 0.0))
EAR_BACK = (F._ear(F.EAR_MID, 1.0), (0.0, 1.0, 0.0))
EAR_IN = (F._ear(F.EAR_MID, 1.0), (0.0, -1.0, 0.0))


class Coat(unittest.TestCase):
    def test_the_back_is_grizzled_grey_the_throat_white_and_the_chin_dark(self):
        for point in (FLANK, BACK):
            rgb, grizzle = F.coat_at(*point)
            self.assertGreater(grizzle, 0.5, point)
            self.assertLess(abs(warm(rgb)), 40, (point, rgb))
        throat, g = F.coat_at(*THROAT)
        self.assertGreater(min(throat), 200, throat)
        self.assertLess(g, 0.05)  # no agouti speckle on the white throat
        self.assertLess(lum(F.coat_at(*CHIN)[0]), 80)
        self.assertGreater(lum(F.coat_at(*BELLY)[0]), 190)

    def test_ears_are_rufous_behind_and_pale_inside_the_legs_rufous_outside(self):
        for outside, inside, pale in ((EAR_BACK, EAR_IN, 175), (FORE_OUT, FORE_IN, 190)):
            out_rgb, in_rgb = F.coat_at(*outside)[0], F.coat_at(*inside)[0]
            self.assertGreater(warm(out_rgb), 70, (outside, out_rgb))
            self.assertGreater(lum(in_rgb), pale, (inside, in_rgb))
        # The fur inside the ear is pale but duller than the white throat
        # (the photographs: buff white, never as bright as the bib).
        self.assertLess(lum(F.coat_at(*EAR_IN)[0]), lum(F.coat_at(*THROAT)[0]) - 20)

    def test_the_tail_is_grey_darker_on_top_with_a_black_tip(self):
        mid = F.TAIL[4]
        top = F.coat_at((0.0, mid[1] - 0.02, mid[2] + 0.04), (0.0, -0.5, 0.86))[0]
        under = F.coat_at((0.0, mid[1] + 0.03, mid[2] - 0.04), (0.0, 0.5, -0.86))[0]
        self.assertLess(lum(top), lum(under))
        tip, g = F.coat_at(F.TAIL[9], (0.0, 0.3, -0.95))
        self.assertLess(max(tip), 60, tip)
        self.assertLess(g, 0.1)


class Cards(unittest.TestCase):
    def test_an_agouti_hair_is_grey_then_pale_then_black_tipped_and_bands_shift(self):
        root, band, tip = (F.card_colour(F.GRIZZLED, f) for f in (0.1, 0.62, 0.98))
        self.assertTrue(lum(tip) < lum(root) < lum(band), (root, band, tip))
        self.assertLess(lum(tip), 70)
        # Each hair's band moves: the same place is pale on one, grey on
        # another, so no stripe runs across a tuft.
        self.assertGreater(lum(F.card_colour(F.GRIZZLED, 0.55, -0.1)), lum(F.card_colour(F.GRIZZLED, 0.55, 0.1)) + 30)
        self.assertLess(max(F.card_colour(F.BLACK, 0.5)), 50)
        # The chest's tufts pale but never brighter than the coat's white
        # bib under them, so they read as fur, not as white strips.
        cream = F.card_colour(F.CREAM, 0.8)
        self.assertGreater(min(cream), 185)
        self.assertLess(lum(cream), lum(F.COAT["white"]) - 15)


class Chest(unittest.TestCase):
    def test_the_chest_is_flat_in_front_and_no_lower_than_the_elbows(self):
        # The photographs (r01, r33): below the point of the shoulder the
        # throat runs nearly straight down into the forelegs; no pigeon
        # breast ahead of them (the user on v3: "mira ese pecho").
        shoulder_y, shoulder_z = F.SHOULDER_POINT
        for k in range(10):
            z = F.ELBOW_Z - 0.012 + k * (shoulder_z - F.ELBOW_Z) / 10
            front = F.chest_front(z)
            if front is not None:
                self.assertGreater(front, shoulder_y - 0.02, z)
        self.assertGreater(F.chest_bottom(), F.ELBOW_Z - 0.015)


class Gaits(unittest.TestCase):
    def test_a_fox_walks_under_a_metre_a_second_and_gallops_near_its_flight(self):
        walk, gallop = F.ground_speed(F.WALK), F.ground_speed(F.GALLOP)
        self.assertTrue(0.5 < walk < 0.9, walk)
        # The game flees at 7 m/s and plays a gallop at most 2.5 times faster
        # than it was made (``Species::max_pace``).
        self.assertTrue(3.5 < gallop < 6.0 and 7.0 / gallop < 2.5, gallop)


class Skeleton(unittest.TestCase):
    def test_the_rig_keeps_the_quaternius_names_parents_first_and_unique(self):
        names = F.joint_names()
        self.assertEqual(len(names), len(set(names)))
        self.assertEqual(len(names), 49)
        self.assertFalse(set(F.QUATERNIUS_NAMES) - set(names))
        # The face the clips move (``pose_face``) and the tail they swing.
        self.assertFalse(({"Jaw", "Ear2.L", "Ear2.R"} | set(F.TAIL_CHAIN)) - set(names))
        seen = set()
        for name, parent, _ in F.BONES:
            self.assertTrue(parent is None or parent in seen, name)
            seen.add(name)
        for name, parent, _, _ in F.EXTRA:
            self.assertIn(parent, seen | {n for n, _, _, _ in F.EXTRA})


if __name__ == "__main__":
    unittest.main()
