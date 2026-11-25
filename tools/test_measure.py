"""Tests of tools/measure.py.

Standard library and numpy: ``python3 -I -B -m unittest discover -s tools -p test_measure.py``.
"""

import io
import json
import os
import tempfile
import unittest
from contextlib import redirect_stdout, redirect_stderr

import measure

try:
    import numpy as np
except ImportError:  # pragma: no cover
    np = None


def write_model(folder, nodes):
    path = os.path.join(folder, "model.gltf")
    with open(path, "w", encoding="utf-8") as f:
        json.dump({"asset": {"version": "2.0"}, "nodes": nodes}, f)
    return path


# A figure whose pelvis is at 0.9 m, head joint at 1.6 m and leg 0.8 m long;
# the root node is lifted 0.1 m and its child lowered back, so world
# positions have to follow the parents.
FIGURE = [
    {"name": "figure", "translation": [0, 0.1, 0], "children": [1]},
    {"name": "Root", "translation": [0, -0.1, 0], "children": [2]},
    {"name": "pelvis", "translation": [0, 0.9, 0], "children": [3, 6]},
    {"name": "thigh_l", "translation": [0.1, -0.05, 0], "children": [4]},
    {"name": "calf_l", "translation": [0, -0.3, 0.4], "children": [5]},
    {"name": "foot_l", "translation": [0, -0.4, 0]},
    {"name": "head", "translation": [0, 0.7, 0]},
]


@unittest.skipIf(np is None, "numpy is required")
class Measure(unittest.TestCase):
    def test_a_model_reports_its_joint_heights_leg_and_stature(self):
        with tempfile.TemporaryDirectory() as tmp:
            path = write_model(tmp, FIGURE)
            out = io.StringIO()
            with redirect_stdout(out):
                code = measure.main([path])
        self.assertEqual(code, 0)
        self.assertEqual(out.getvalue().split(), [
            "model", "pelvis", "head", "leg", "stature",
            path, "0.900", "1.600", "0.900", "1.80"])

    def test_a_model_without_a_head_joint_is_refused(self):
        with tempfile.TemporaryDirectory() as tmp:
            headless = [dict(n) for n in FIGURE[:6]]
            headless[2]["children"] = [3]
            path = write_model(tmp, headless)
            err = io.StringIO()
            with redirect_stderr(err), redirect_stdout(io.StringIO()):
                code = measure.main([path])
        self.assertEqual(code, 1)
        self.assertIn("no 'head' joint", err.getvalue())


if __name__ == "__main__":
    unittest.main()
