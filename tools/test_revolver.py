"""Tests of the revolver (``tools/revolver.py``): how it finds the barrel,
the muzzle and the grip, and the shipped
``assets/models/weapons/colt_saa.glb`` as the game reads it, without
Blender.

    python3 -I -B -m unittest discover -s tools -p test_revolver.py
"""

import os
import re
import struct
import unittest

import numpy as np

import glbtrim
import revolver as R

GLB = os.path.join(R.HERE, "..", "assets", "models", "weapons", "colt_saa.glb")
GUNFIRE_RS = os.path.join(R.HERE, "..", "crates", "render", "src", "character", "gunfire.rs")


def l_shape(long, height, forward, down):
    """A revolver-like cloud: a thin barrel along axis ``long`` (the muzzle
    towards ``forward``), a thick frame at the back and a grip hanging
    towards ``down`` along axis ``height``."""
    rng = np.random.default_rng(7)
    other = ({0, 1, 2} - {long, height}).pop()
    pts = []
    for _ in range(400):
        p = np.zeros(3)
        p[long] = rng.uniform(0.0, 0.2) * forward
        p[height] = rng.uniform(-0.007, 0.007)
        p[other] = rng.uniform(-0.007, 0.007)
        pts.append(p)
    for _ in range(400):
        p = np.zeros(3)
        p[long] = rng.uniform(-0.1, 0.0) * forward
        p[height] = rng.uniform(-0.02, 0.02)
        p[other] = rng.uniform(-0.02, 0.02)
        pts.append(p)
    for _ in range(400):
        p = np.zeros(3)
        p[long] = rng.uniform(-0.1, -0.06) * forward
        p[height] = rng.uniform(0.02, 0.11) * down
        p[other] = rng.uniform(-0.015, 0.015)
        pts.append(p)
    return np.array(pts)


class Orient(unittest.TestCase):
    def test_the_barrel_points_ahead_the_top_up_and_the_grip_is_the_origin(self):
        for long, height, forward, down in [(1, 2, 1, -1), (0, 2, -1, -1), (2, 1, 1, 1), (1, 0, -1, 1)]:
            pts = l_shape(long, height, forward, down)
            r, grip, tip = R.orient(pts)
            game = (pts - grip) @ r.T
            muzzle = r @ (tip - grip)
            self.assertAlmostEqual(abs(np.linalg.det(r)), 1.0, places=6)
            self.assertGreater(np.linalg.det(r), 0.0, "no mirror")
            self.assertGreater(muzzle[1], 0.2, "the muzzle is ahead")
            self.assertGreater(muzzle[2], 0.03, "above the grip")
            self.assertLess(abs(muzzle[0]), 0.01)
            self.assertLess(game[:, 2].min(), muzzle[2] - 0.08, "the grip hangs down")
            self.assertLess(game[:, 1].min(), 0.0, "the hammer behind the hand")


class Shipped(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        with open(GLB, "rb") as f:
            cls.doc, cls.bin = glbtrim.read_glb(f.read())
        cls.nodes = {n["name"]: n for n in cls.doc["nodes"]}

    def points(self):
        mesh = self.doc["meshes"][self.nodes["Revolver"]["mesh"]]
        out = []
        for prim in mesh["primitives"]:
            out.extend(glbtrim.read_accessor(self.doc, self.bin, prim["attributes"]["POSITION"]))
        return np.array(out)

    def test_it_is_within_budget_and_life_size_with_the_barrel_along_minus_z(self):
        mesh = self.doc["meshes"][self.nodes["Revolver"]["mesh"]]
        tris = sum(self.doc["accessors"][p["indices"]]["count"] // 3 for p in mesh["primitives"])
        self.assertLessEqual(tris, R.BUDGET)
        pts = self.points()
        lo, hi = pts.min(axis=0), pts.max(axis=0)
        # A 7.5 in. barrel: about a third of a metre from the hammer to the muzzle.
        self.assertTrue(0.30 < hi[2] - lo[2] < 0.36, (lo, hi))
        self.assertLess(lo[2], -0.25, "the barrel ahead, towards -Z")
        self.assertLess(hi[2], 0.1, "the hammer just behind the hand")
        self.assertLess(lo[1], -0.03, "the grip hangs down")
        self.assertLess(hi[0] - lo[0], 0.06, "thin across")

    def test_the_muzzle_node_is_where_the_game_puts_the_flash(self):
        muzzle = self.nodes["Muzzle"]["translation"]
        with open(GUNFIRE_RS) as f:
            game = re.search(r"MUZZLE: Vec3 = Vec3::new\(([^)]*)\)", f.read()).group(1)
        game = [float(x) for x in game.split(",")]
        for a, b in zip(muzzle, game):
            self.assertAlmostEqual(a, b, places=3)
        pts = self.points()
        tip = pts[pts[:, 2] < muzzle[2] + 0.01]
        self.assertLess(np.linalg.norm(tip.mean(axis=0)[:2] - np.array(muzzle[:2])), 0.01, "on the bore")

    def test_its_maps_are_shrunk(self):
        for image in self.doc["images"]:
            view = self.doc["bufferViews"][image["bufferView"]]
            data = self.bin[view.get("byteOffset", 0) : view.get("byteOffset", 0) + view["byteLength"]]
            if data[:8] == b"\x89PNG\r\n\x1a\n":
                w, h = struct.unpack(">II", data[16:24])
            else:
                self.fail(f"unexpected image format {data[:4]!r}")
            self.assertLessEqual(max(w, h), R.MAPS)


if __name__ == "__main__":
    unittest.main()
