"""Tests of tools/building_straw.py: the seeded stalks and their wrapping.

Standard library only: ``python3 -I -B -m unittest discover -s tools``.
"""

import math
import unittest

import building_straw as B


class Stalks(unittest.TestCase):
    def test_the_stalks_lie_along_the_layers_at_straw_length(self):
        pieces = B.strands(7, 300)
        self.assertGreater(len(pieces), 300)
        for x0, y0, x1, y1, width, _, opacity in pieces:
            length = math.hypot(x1 - x0, y1 - y0)
            # A piece is never longer than a whole stalk, nor a speck.
            self.assertLessEqual(length, B.LENGTH[1] + 1e-6)
            self.assertGreaterEqual(length, 4.0)
            tilt = math.degrees(math.atan2(abs(y1 - y0), abs(x1 - x0)))
            self.assertLessEqual(tilt, B.TILT + 1e-6)
            # Half sunk in the mud: never opaque.
            self.assertLess(opacity, 1.0)
            self.assertGreater(width, 0.0)
        self.assertEqual(pieces, B.strands(7, 300))

    def test_a_stalk_over_an_edge_is_drawn_again_from_the_far_side(self):
        size = B.SIZE
        inside = (10.0, 10.0, 40.0, 20.0, 2.0, (1, 2, 3), 0.5)
        self.assertEqual(B.wrapped(inside), [inside])
        corner = (size - 10.0, size - 5.0, size + 20.0, size + 8.0, 2.0, (1, 2, 3), 0.5)
        copies = B.wrapped(corner)
        # Shifted left, up, and both: every part of the stalk lands inside.
        self.assertEqual(len(copies), 4)
        self.assertIn((-10.0, -5.0, 20.0, 8.0, 2.0, (1, 2, 3), 0.5), copies)


if __name__ == "__main__":
    unittest.main()
