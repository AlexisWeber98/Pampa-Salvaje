"""Tests of tools/vehicle.py and the vehicle scripts (sulky, carreta,
carretón, Ford T): their geometry and maps, without Blender.

    python3 -I -B -m unittest discover -s tools
"""

import math
import unittest

try:
    import numpy as np
except ImportError:  # pragma: no cover - numpy missing
    np = None

import vehicle as V
import carreta
import carreton
import ford_t
import sulky

# The draught animals' and the seat's places as the game reads them
# (``pampa_render::vehicle::Layout``, body space: Blender (x, y, z) is
# (-y, z, -x) there), and each wheel's radius (``models::wheel_radius``).
GAME = {
    "Sulky": (sulky, [(0.0, 0.0, 0.0)], (-2.32, 1.40, 0.0), {"Wheel.L": 0.70, "Wheel.R": 0.70}),
    "Carreton": (carreton, [(0.0, 0.0, -0.62), (0.0, 0.0, 0.62)], (-1.88, 1.74, 0.0),
                 {"Wheel.FL": 0.50, "Wheel.FR": 0.50, "Wheel.RL": 0.66, "Wheel.RR": 0.66}),
    "Carreta": (carreta, [(0.0, 0.0, -0.6255), (0.0, 0.0, 0.6255)], (-2.22, 1.42, 0.0), {"Wheel.L": 1.10, "Wheel.R": 1.10}),
    "FordT": (ford_t, [], (-0.05, 1.02, -0.30),
              {"Wheel.FL": 0.38, "Wheel.FR": 0.38, "Wheel.RL": 0.38, "Wheel.RR": 0.38}),
}


def body_space(p):
    return (-p[1], p[2], -p[0])


class Vehicles(unittest.TestCase):
    def test_each_vehicle_matches_what_the_game_reads(self):
        for name, (script, animals, seat, wheels) in GAME.items():
            v = script.build()
            self.assertEqual({k: r for k, (_, r, _) in v.wheels.items()}, wheels, name)
            found = [body_space(v.markers[f"Animal.{k}"]) for k in range(len(animals))]
            for a, b in zip(found, animals):
                self.assertTrue(all(abs(x - y) < 1e-3 for x, y in zip(a, b)), (name, a, b))
            self.assertNotIn(f"Animal.{len(animals)}", v.markers, name)
            got = body_space(v.markers["Seat"])
            self.assertTrue(all(abs(x - y) < 1e-3 for x, y in zip(got, seat)), (name, got, seat))

    def test_every_wheel_stands_on_the_ground_and_reaches_its_radius(self):
        for name, (script, _, _, _) in GAME.items():
            v = script.build()
            for wheel, (hub, radius, mesh) in v.wheels.items():
                self.assertAlmostEqual(hub[2], radius, places=4, msg=(name, wheel))
                reach = max(math.hypot(p[1], p[2]) for p in mesh.verts)
                self.assertLess(abs(reach - radius), 0.012, (name, wheel, reach))

    def test_every_face_uses_a_known_material(self):
        for name, (script, _, _, _) in GAME.items():
            self.assertTrue(set(script.build().materials()) <= set(V.MATERIALS), name)


@unittest.skipIf(np is None, "needs numpy")
class Maps(unittest.TestCase):
    def test_every_set_has_its_maps_at_its_size_and_is_not_flat(self):
        for name, paint in V.SETS.items():
            m = paint(64)
            for key in ("color", "normal_gl", "roughness", "ao"):
                self.assertEqual(m[key].shape[:2], (64, 64), (name, key))
            self.assertGreater(m["color"].std(), 0.01, name)
        self.assertEqual(set(V.SET_OF.values()), set(V.SETS), "a set for every material but glass")


if __name__ == "__main__":
    unittest.main()
