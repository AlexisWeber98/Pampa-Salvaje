#!/usr/bin/env python3
"""Measure the height of the game's character models.

Standard library plus numpy; run it isolated:
``python3 -I tools/measure.py assets/characters/*/model.gltf``.

For each ``model.gltf`` (as ``tools/mh2gltf.py`` and ``tools/dress.py``
write them: metres, Y up, feet on y = 0) it prints, from the skeleton's
rest pose, the height of the ``pelvis`` and ``head`` joints, the leg length
(thigh to calf plus calf to foot, left side) and the estimated stature
without a hat: the head joint plus 0.20 m (about ±3 cm). The mesh is not
read, so a hat or a hairdo never counts.
"""

import os
import sys

if __package__ in (None, ""):
    # Run as a script with -I the tools folder is not on the path.
    sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))

import dress  # noqa: E402
import mh2gltf as M  # noqa: E402

np = M.np
HEAD_TO_TOP = 0.20  # metres from the head joint to the top of the skull
JOINTS = ("pelvis", "head", "thigh_l", "calf_l", "foot_l")


def joint_positions(path):
    """World rest positions, in metres, of the JOINTS of a model."""
    g = dress.Gltf(path)
    nodes = g.items("nodes")
    parent = {}
    for i, node in enumerate(nodes):
        children = node.get("children", [])
        if not isinstance(children, list):
            raise M.ConvertError(f"node {i}: 'children' is not a list")
        for c in children:
            g.item("nodes", c)
            if c in parent:
                raise M.ConvertError(f"node {c} has two parents")
            parent[c] = i
    names = {n.get("name"): i for i, n in enumerate(nodes)}
    found = {}
    for joint in JOINTS:
        if joint not in names:
            raise M.ConvertError(f"no {joint!r} joint")
        m, i, seen = np.identity(4), names[joint], set()
        while i is not None:
            if i in seen:
                raise M.ConvertError(f"node {i} is its own ancestor")
            seen.add(i)
            m = dress.local_matrix(nodes[i]) @ m
            i = parent.get(i)
        found[joint] = m[:3, 3] * M.METERS
    return found


def measure(path):
    """(pelvis, head, leg, stature) of a model, in metres."""
    p = joint_positions(path)
    leg = np.linalg.norm(p["calf_l"] - p["thigh_l"]) + np.linalg.norm(p["foot_l"] - p["calf_l"])
    return p["pelvis"][1], p["head"][1], leg, p["head"][1] + HEAD_TO_TOP


def main(argv=None):
    paths = sys.argv[1:] if argv is None else argv
    if not paths:
        print("usage: measure.py MODEL.gltf...", file=sys.stderr)
        return 2
    rows = []
    for path in paths:
        try:
            rows.append((path, *measure(path)))
        except (M.ConvertError, OSError) as e:
            print(f"{path}: {e}", file=sys.stderr)
            return 1
    width = max(len("model"), *(len(r[0]) for r in rows))
    print(f"{'model':<{width}}  pelvis   head    leg  stature")
    for path, pelvis, head, leg, stature in rows:
        print(f"{path:<{width}}  {pelvis:6.3f} {head:6.3f} {leg:6.3f}  {stature:7.2f}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
