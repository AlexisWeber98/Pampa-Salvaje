"""Tests for run_cycle.py: python3 -m unittest discover -s tools -p test_run_cycle.py"""

import math
import os
import sys
import unittest

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))

import retarget as R  # noqa: E402
import run_cycle as RC  # noqa: E402

REPO = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
PROTAGONIST = os.path.join(REPO, "assets/characters/protagonist/model.gltf")


class Run(unittest.TestCase):
    """The jog. Fukuchi et al. 2017 near 4.5 m/s: 183 steps a minute, the
    swinging knee up to 108-119 degrees, the hip flexed to 53-60; the
    pelvis rises and falls 6 to 9 cm."""

    gait = RC.JOG
    steps = (180.0, 195.0)
    knee = (100.0, 120.0)
    thigh = (50.0, 61.0)

    @classmethod
    def setUpClass(cls):
        with open(PROTAGONIST, "rb") as f:
            cls.target = R.read_target(f.read())
        cls.runner, cls.frames, cls.summary = RC.run(cls.target, cls.gait)

    def ball(self, frame, s):
        """Foot `s`'s ball of the foot in the world, carried down the leg."""
        turns, pelvis = self.frames[frame]
        p = self.runner.rest
        at = R.add(pelvis, R.qrot(turns["pelvis"], R.sub(p[f"thigh_{s}"], p["pelvis"])))
        for bone, child in (("thigh", "calf"), ("calf", "foot"), ("foot", "ball")):
            at = R.add(at, R.qrot(turns[f"{bone}_{s}"], R.sub(p[f"{child}_{s}"], p[f"{bone}_{s}"])))
        return at

    def test_the_run_is_within_the_measured_runners(self):
        s, g = self.summary, self.gait
        self.assertAlmostEqual(s["stride_m"], g.SPEED * g.CYCLE_TIME)
        self.assertTrue(self.steps[0] < s["steps_per_min"] < self.steps[1], s)
        self.assertTrue(5.0 < s["bob_cm"] < 9.0, s)
        self.assertTrue(self.knee[0] < s["knee_deg"][1] < self.knee[1], s)
        self.assertTrue(self.thigh[0] < s["thigh_deg"][1] < self.thigh[1], s)
        # The landing leg nearly straight but never locked, and no foot
        # through the ground.
        self.assertTrue(5.0 <= s["knee_deg"][0] <= 15.0, s)
        self.assertGreater(s["lowest_ball_cm"], -1e-6)

    def test_a_planted_ball_of_the_foot_stays_put(self):
        # From the heel coming down until toe-off, the ball of the foot
        # does not slide: the body runs over it.
        g = self.gait
        stance = g.STANCE * g.CYCLE_TIME * RC.FPS
        for s, start in (("r", 0.0), ("l", g.CYCLE_TIME * RC.FPS / 2.0)):
            first = math.ceil(start + g.HEEL_DOWN * stance)
            last = math.floor(start + stance)
            planted = self.ball(first, s)
            for k in range(first, last + 1):
                self.assertLess(R.length(R.sub(self.ball(k, s), planted)), 1e-3, (s, k))

    def test_each_arm_is_furthest_forward_arm_lead_before_the_other_foot_lands(self):
        # The right arm swings against the right leg: furthest forward as
        # the left foot comes down (half a cycle), or ``ARM_LEAD`` earlier.
        g, p = self.gait, self.runner.rest
        per = g.CYCLE_TIME * RC.FPS

        def forward(k):
            turns, _ = self.frames[k]
            v = R.qrot(turns["upperarm_r"], R.sub(p["lowerarm_r"], p["upperarm_r"]))
            return math.atan2(v[2], -v[1])

        ahead = max(range(int(per)), key=forward)
        self.assertAlmostEqual(ahead / per, 0.5 - g.ARM_LEAD, delta=0.03)

    def test_the_capture_reads_back_as_the_run_at_the_game_speed(self):
        source = R.BvhSource(RC.bvh(self.runner, self.frames).encode(), "cycle")
        sk = source.skeleton
        # The BVH's joints, posed by its channels, are where the run put them.
        for k in (0, 37, 101):
            r, t = source._pose(source._values[k])
            _, pos = sk.world(r, t)
            ball = pos[sk.find("RightToeBase")]
            hips = pos[sk.find("Hips")]
            want = R.sub(self.ball(k, "r"), self.frames[k][1])
            self.assertLess(R.length(R.sub(R.sub(ball, hips), want)), 1e-4, k)
        # Read as a capture onto the target, it runs at the game's speed.
        keys, measures = R.Retargeter(source, self.target).clip("cycle", True, True)
        self.assertAlmostEqual(keys[-1][0], self.gait.CYCLE_TIME, places=4)
        self.assertAlmostEqual(measures["ground_speed"], self.gait.SPEED, delta=0.02)


class Sprint(Run):
    """The sprint: sprinters take 190 to 220 steps a minute, fold the knee
    past 120 degrees and drive the thigh up near level."""

    gait = RC.SPRINT
    steps = (190.0, 220.0)
    knee = (120.0, 135.0)
    thigh = (65.0, 80.0)


if __name__ == "__main__":
    unittest.main()
