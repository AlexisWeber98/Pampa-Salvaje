"""Tests of the gig (``tools/sulky_gig.py``) as shipped,
``assets/models/vehicles/sulky_gig.glb``: what the game reads from it, its
triangle budget, its wheels, and its shafts bent onto the sulky's line
along henry, without Blender.

    python3 -I -B -m unittest discover -s tools -p test_sulky_gig.py
"""

import math
import os
import re
import struct
import unittest

import glbtrim
import sulky
import sulky_gig as G
import vehicle_harness as H

GLB = os.path.join(G.HERE, "..", "assets", "models", "vehicles", "sulky_gig.glb")
MODELS_RS = os.path.join(G.HERE, "..", "crates", "render", "src", "vehicle", "models.rs")
# The gig's seat as the game reads it (``pampa_render::vehicle::Layout::of``
# for ``Variant::Gig``), body space.
GAME_SEAT = (-2.45, 1.47, 0.0)


def body_space(p):
    """Blender (x, y, z) in the game's vehicle body space."""
    return (-p[1], p[2], -p[0])


def blender(p):
    """A glTF point (Y up, the vehicle facing +Z) in Blender's frame."""
    return (p[0], -p[2], p[1])


class Gig(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        with open(GLB, "rb") as f:
            cls.doc, cls.bin = glbtrim.read_glb(f.read())
        cls.nodes = {n["name"]: n for n in cls.doc["nodes"]}

    def mesh(self, name):
        """{material name: [Blender points]} of the mesh of node ``name``."""
        mesh = self.doc["meshes"][self.nodes[name]["mesh"]]
        out = {}
        for prim in mesh["primitives"]:
            material = self.doc["materials"][prim["material"]]["name"]
            pts = glbtrim.read_accessor(self.doc, self.bin, prim["attributes"]["POSITION"])
            out.setdefault(material, []).extend(blender(p) for p in pts)
        return out

    def triangles(self, name):
        mesh = self.doc["meshes"][self.nodes[name]["mesh"]]
        return sum(self.doc["accessors"][p["indices"]]["count"] // 3 for p in mesh["primitives"])

    def test_it_has_the_nodes_and_markers_the_game_reads(self):
        for name in ("Body", "Body_Mesh", "Body_LOD", "Wheel.L", "Wheel.L_Mesh", "Wheel.L_LOD",
                     "Wheel.R", "Wheel.R_Mesh", "Wheel.R_LOD", "Seat", "Animal.0"):
            self.assertIn(name, self.nodes)
        self.assertNotIn("Animal.1", self.nodes)
        animal = blender(self.nodes["Animal.0"].get("translation", (0, 0, 0)))
        self.assertTrue(all(abs(x) < 1e-4 for x in animal), animal)
        seat = body_space(blender(self.nodes["Seat"]["translation"]))
        self.assertTrue(all(abs(a - b) < 1e-3 for a, b in zip(seat, GAME_SEAT)), seat)
        self.assertEqual(body_space(G.SEAT), GAME_SEAT)
        # Every material has its texture set in the game.
        with open(MODELS_RS, encoding="utf-8") as f:
            rs = f.read()
        for material in self.doc["materials"]:
            self.assertIn(f'"{material["name"]}" =>', rs, material["name"])

    def test_it_keeps_to_its_triangle_budget(self):
        full = sum(self.triangles(n) for n in ("Body_Mesh", "Wheel.L_Mesh", "Wheel.R_Mesh"))
        far = sum(self.triangles(n) for n in ("Body_LOD", "Wheel.L_LOD", "Wheel.R_LOD"))
        # The source has 284,382; the sulky 14,540.
        self.assertLessEqual(full, G.MOST_TRIANGLES)
        self.assertLess(far, full / 2)

    def test_its_wheels_are_the_sulkys_and_stand_on_the_ground(self):
        for name in ("Wheel.L", "Wheel.R"):
            node = self.nodes[name]
            self.assertAlmostEqual(node["extras"]["radius"], sulky.WHEEL_RADIUS, 4)
            hub = blender(node["translation"])
            self.assertAlmostEqual(hub[2], sulky.WHEEL_RADIUS, 4)
            self.assertAlmostEqual(hub[1], sulky.AXLE_Y, 4)
            pts = [p for ps in self.mesh(f"{name}_Mesh").values() for p in ps]
            reach = max(math.hypot(p[1], p[2]) for p in pts)
            self.assertLess(abs(reach - sulky.WHEEL_RADIUS), 0.012, (name, reach))
            # Its own side, every part of it.
            self.assertTrue(all((p[0] + hub[0]) * hub[0] > 0 for p in pts), name)

    def test_each_wheels_tyre_rim_and_hub_turn_round_its_middle_and_face_out(self):
        # The game draws one side of each face: a side ring facing into the
        # wheel is left out, and the far wheel showed through the near
        # one's felloe as a tyre off its rim (L67).
        for name in ("Wheel.L_Mesh", "Wheel.R_Mesh"):
            mesh = self.doc["meshes"][self.nodes[name]["mesh"]]
            for prim in mesh["primitives"]:
                material = self.doc["materials"][prim["material"]]["name"]
                pos = glbtrim.read_accessor(self.doc, self.bin, prim["attributes"]["POSITION"])
                ys, zs = [p[1] for p in pos], [p[2] for p in pos]
                mid = ((min(ys) + max(ys)) / 2, (min(zs) + max(zs)) / 2)
                self.assertLess(math.hypot(*mid), 0.003, (name, material, mid))
                index = [i[0] if isinstance(i, tuple) else i for i in
                         glbtrim.read_accessor(self.doc, self.bin, prim["indices"])]
                inward = 0
                for k in range(0, len(index), 3):
                    a, b, c = (pos[i] for i in index[k:k + 3])
                    u = [b[j] - a[j] for j in range(3)]
                    v = [c[j] - a[j] for j in range(3)]
                    n = (u[1] * v[2] - u[2] * v[1], u[2] * v[0] - u[0] * v[2], u[0] * v[1] - u[1] * v[0])
                    side = (a[0] + b[0] + c[0]) / 3
                    # A side face (along the axle), off the wheel's middle.
                    if abs(n[0]) > 0.9 * math.hypot(*n) > 0 and abs(side) > 0.004:
                        inward += (n[0] > 0) != (side > 0)
                self.assertEqual(inward, 0, (name, material))

    def shaft(self, y, reach=0.08):
        """How far each point of the left shaft within ``reach`` of ``y``
        (Blender) lies from the shaft's middle line at its own ``y``,
        sideways and up (the decimated shaft's rings lie up to 15 cm
        apart)."""
        out = []
        for p in self.mesh("Body_Mesh")["Gig_Paint"]:
            if abs(p[1] - y) < reach and p[0] > 0.2:
                c = G.shaft_at(1, p[1])
                if math.hypot(p[0] - c[0], p[2] - c[2]) < 0.08:
                    out.append((p[0] - c[0], p[2] - c[2]))
        return out

    def test_the_shafts_follow_their_line_through_the_tugs_rings(self):
        for y in (H.TUG[1], 0.0, G.BREECH_TIE, G.TRACE_HOOK_Y, 1.6):
            off = self.shaft(y)
            self.assertGreater(len(off), 12, y)
            mid = [(min(o[k] for o in off) + max(o[k] for o in off)) / 2 for k in (0, 1)]
            self.assertLess(math.hypot(*mid), 0.006, (y, mid))
        # Through the tug's ring (4.5 cm round its middle, a 1 cm strap):
        # nothing of the shaft within 5 cm of it reaches the strap.
        far = max(math.hypot(*o) for o in self.shaft(H.TUG[1], 0.05))
        self.assertLess(far, 0.035)
        c = G.shaft_at(1, H.TUG[1])
        self.assertLess(math.hypot(c[0] - H.TUG[0], c[2] - H.TUG[2]), 1e-6)

    def test_the_breeching_tie_the_game_reads_is_on_the_gigs_left_shaft(self):
        # The game draws each breeching strap to ``SULKY_BREECH_TIE`` on both
        # sulkies.
        with open(MODELS_RS, encoding="utf-8") as f:
            m = re.search(r"SULKY_BREECH_TIE: Vec3 = Vec3::new\(([^)]*)\)", f.read())
        game = [float(x) for x in m.group(1).split(",")]
        tie = body_space(G.shaft_at(1, G.BREECH_TIE))
        self.assertTrue(all(abs(x - y) < 2e-3 for x, y in zip(tie, game)), (tie, game))
        # And a leather band is buckled round the shaft there.
        c = G.shaft_at(1, G.BREECH_TIE)
        band = [p for p in self.mesh("Body_Mesh")["Leather"]
                if abs(p[1] - c[1]) < 0.02 and math.hypot(p[0] - c[0], p[2] - c[2]) < 0.05]
        self.assertGreater(len(band), 20)

    def test_each_texture_set_ships_at_its_size(self):
        textures = os.path.join(G.HERE, "..", "assets", "textures", "vehicles")
        for _, folder, _, _, _ in G.MATERIALS.values():
            for name in ("color", "normal_gl", "orm"):
                with open(os.path.join(textures, folder, f"{name}.ktx2"), "rb") as f:
                    head = f.read(28)
                # KTX2: 12-byte identifier, vkFormat, typeSize, pixelWidth, pixelHeight.
                width, height = struct.unpack_from("<II", head, 20)
                most = G.MAX_SIZE.get(folder)
                if most:
                    self.assertEqual((width, height), (most, most), f"{folder}/{name}")


if __name__ == "__main__":
    unittest.main()
