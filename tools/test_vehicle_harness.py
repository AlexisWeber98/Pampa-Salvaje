"""Tests of the draught harness (``tools/vehicle_harness.py``,
``assets/models/vehicles/harness.glb``) on henry's horse, and of the sulky's
shafts around him, without Blender.

    python3 -I -B -m unittest discover -s tools -p test_vehicle_harness.py

Henry is posed in numpy as the game poses him (``recado.Glb``); points are
in Blender's frame in game metres, as the harness constants are.
"""

import os
import unittest

try:
    import numpy as np
except ImportError:  # pragma: no cover - numpy missing
    np = None

import vehicle_harness as H

HARNESS = os.path.join(H.HERE, "..", "assets", "models", "vehicles", "harness.glb")


class Henry:
    """Henry's skin (no ears nor eyelids, which move on their own) posed by
    clip and time."""

    def __init__(self):
        import recado as R

        self.R = R
        self.glb = R.Glb(H.HORSE_GLB)
        doc = self.glb.doc
        mesh = next(m for m in doc["meshes"] if any(doc["materials"][p["material"]]["name"] in H.SKIN
                                                      for p in m["primitives"]))
        prim = next(p for p in mesh["primitives"] if doc["materials"][p["material"]]["name"] in H.SKIN)
        tris = self.glb._read(prim["indices"]).reshape(-1, 3).astype(int)
        joints = self.glb._read(prim["attributes"]["JOINTS_0"]).astype(int)
        weights = self.glb._read(prim["attributes"]["WEIGHTS_0"]).astype(float)
        loose = {k for k, j in enumerate(self.glb.joints) if self.glb.names[j].startswith(H.EARS)}
        moving = np.zeros(len(joints), bool)
        for c in range(4):
            moving |= np.isin(joints[:, c], list(loose)) & (weights[:, c] > 0.3)
        self.tris = tris
        self.keep = ~moving
        self.stand = self.glb.worlds(H.REST_CLIP, 0.0)

    def skin(self, worlds):
        """The skin's vertices and their normals, game metres."""
        v = self.R._scaled(self.glb.skinned(worlds, H.SKIN))
        a, b, c = v[self.tris[:, 0]], v[self.tris[:, 1]], v[self.tris[:, 2]]
        face = np.cross(b - a, c - a)
        n = np.zeros_like(v)
        for k in range(3):
            np.add.at(n, self.tris[:, k], face)
        n /= np.maximum(np.linalg.norm(n, axis=1, keepdims=True), 1e-12)
        return v[self.keep], n[self.keep]

    @staticmethod
    def off(points, skin):
        """How far each point stands off the skin (negative: inside it),
        along the normal of the nearest skin vertex."""
        v, n = skin
        out = np.empty(len(points))
        for at in range(0, len(points), 2000):
            p = points[at:at + 2000]
            d = ((p[:, None, :] - v[None, :, :]) ** 2).sum(-1)
            k = d.argmin(1)
            out[at:at + 2000] = ((p - v[k]) * n[k]).sum(1)
        return out


@unittest.skipIf(np is None, "needs numpy")
class Fittings(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.henry = Henry()
        cls.standing = cls.henry.skin(cls.henry.stand)

    def off(self, p):
        return float(Henry.off(np.array([p], float), self.standing)[0])

    def test_the_fittings_the_vehicles_meet_lie_on_henrys_skin(self):
        # (point, least and most it stands off the skin, metres)
        for name, lo, hi in (("TRACE", 0.0, 0.03), ("SIDE", -0.005, 0.02), ("BREECH", 0.0, 0.03),
                             ("TUG", 0.02, 0.08), ("TERRET", 0.03, 0.08), ("BIT", -0.005, 0.03)):
            p = getattr(H, name)
            for side in (1, -1):
                d = self.off((p[0] * side, p[1], p[2]))
                self.assertTrue(lo <= d <= hi, f"{name} {side:+}: {d:.3f} m off the skin, not {lo}..{hi}")

    def test_the_sulky_shafts_clear_henry_from_tip_to_breeching(self):
        import sulky

        for side in (1, -1):
            path = np.array(sulky.shaft_path(side), float)
            # Along the horse, from the tip to its quarters.
            fine = np.concatenate([np.linspace(a, b, 8, endpoint=False) for a, b in zip(path, path[1:])])
            fine = fine[fine[:, 1] < H.BREECH[1] + 0.4]
            d = Henry.off(fine, self.standing)
            # The shaft is 5 cm across (``sulky.build``): its side keeps off
            # the skin.
            self.assertGreater(d.min(), 0.035, f"shaft {side:+} at {fine[d.argmin()].round(3)}: {d.min():.3f} m")


@unittest.skipIf(np is None, "needs numpy")
class Harness(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        import recado as R

        cls.henry = Henry()
        try:
            cls.harness = R.Glb(HARNESS)
        except KeyError:  # no skin: rigid pieces
            cls.harness = None

    def setUp(self):
        self.assertIsNotNone(self.harness, "harness.glb has no skin")

    def test_every_piece_is_skinned_to_henrys_joints_resting_standing(self):
        doc, glb = self.harness.doc, self.harness
        meshes = [n for n in doc["nodes"] if "mesh" in n]
        self.assertTrue(meshes)
        for node in meshes:
            self.assertIn("skin", node, node.get("name"))
        henry = self.henry.glb
        for k, j in enumerate(glb.joints):
            name = glb.names[j]
            self.assertIn(name, henry.names)
            want = np.linalg.inv(self.henry.stand[henry.names.index(name)])
            self.assertTrue(np.allclose(glb.ibm[k], want, atol=1e-4), name)

    def test_no_strap_sinks_into_the_skin_as_the_horse_stands_walks_and_gallops(self):
        glb, henry = self.harness, self.henry.glb
        doc = glb.doc
        index = [henry.names.index(glb.names[j]) for j in glb.joints]
        # Every piece but the bit, whose mouthpiece goes through the mouth.
        pos, joints, weights = [], [], []
        for node in doc["nodes"]:
            if "mesh" not in node or node["name"] == "Bit":
                continue
            for prim in doc["meshes"][node["mesh"]]["primitives"]:
                a = prim["attributes"]
                pos.append(glb._read(a["POSITION"]).astype(float))
                joints.append(glb._read(a["JOINTS_0"]).astype(int))
                weights.append(glb._read(a["WEIGHTS_0"]).astype(float))
        ph = np.c_[np.concatenate(pos), np.ones(sum(len(p) for p in pos))]
        joints, weights = np.concatenate(joints), np.concatenate(weights)

        def posed(worlds):
            mats = np.stack([worlds[i] for i in index]) @ glb.ibm
            m = np.einsum("nc,ncij->nij", weights, mats[joints])
            return self.henry.R._scaled(np.einsum("nij,nj->ni", m, ph)[:, :3])

        # The skin a vertex can meet is the skin near it on the standing
        # horse (it moves with it): its 24 nearest skin vertices.
        v, _ = self.henry.skin(self.henry.stand)
        at = posed(self.henry.stand)
        near = np.empty((len(at), 24), int)
        for k in range(0, len(at), 2000):
            d = ((at[k:k + 2000, None, :] - v[None, :, :]) ** 2).sum(-1)
            near[k:k + 2000] = np.argpartition(d, 24, axis=1)[:, :24]
        worst = (1.0, None)
        for clip in H.DRAUGHT_CLIPS:
            for frame in range(int(round(henry.duration(clip) * 24)) + 1):
                worlds = henry.worlds(clip, frame / 24)
                v, n = self.henry.skin(worlds)
                p = posed(worlds)
                d = ((p[:, None, :] - v[near]) ** 2).sum(-1)
                k = near[np.arange(len(p)), d.argmin(1)]
                off = ((p - v[k]) * n[k]).sum(1)
                if off.min() < worst[0]:
                    worst = (off.min(), (clip, frame, at[off.argmin()].round(3)))
        # Measured against the tangent plane at the nearest skin vertex,
        # which runs a few millimetres off the skin's faces where it bends.
        self.assertGreater(worst[0], -0.004, worst)


if __name__ == "__main__":
    unittest.main()
