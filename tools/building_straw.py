#!/usr/bin/env python3
"""Lays chopped straw into the mud of a rancho de barro y paja's wall.

The walls of a 1919 pampa rancho de barro y paja (chorizo or quincha) are
mud kneaded with chopped straw and packed in layers, so short golden stalks
show all over the dried, cracked surface, lying mostly along the layers.
No free scan shows that, so this script draws the straw over a scan of
dried, cracked mud (Poly Haven's `clay_block_wall`, CC0, kept as
`scan_color.jpg`): seeded stalks 4 to 14 cm long on the scan's 2 m tile,
within 35 degrees of the horizontal, each a mud-dulled gold stalk over a
faint shadow line, half sunk in the mud: partly transparent, and showing in
one to three pieces along its line. Strands that cross
an edge are drawn again from the opposite edge, so the colour still tiles.

The set folder keeps the scan's `normal_gl.jpg`, `roughness.jpg` and
`ao.jpg` as they are; this writes `color.jpg` next to them, ready for
`tools/textures_to_ktx2.sh`. The output is reproducible with the same seed
and ImageMagick. Needs ImageMagick 7 (`magick`); standard library only.

Usage: python3 -I tools/building_straw.py SET_FOLDER [--seed N] [--count N]
  e.g. python3 -I tools/building_straw.py assets/textures/buildings/mud_straw
"""

import argparse
import math
import os
import random
import subprocess
import sys
import tempfile

# The scan's size in texels; it covers 2 m.
SIZE = 1024
# Stalk lengths in texels: 4 to 14 cm on a 2 m tile.
LENGTH = (20.0, 72.0)
# At most this far from the horizontal, in degrees: the stalks lie along the
# packed layers.
TILT = 35.0
# Pale gold straw, a little varied; the shadow it casts on the mud.
STRAW = (194, 168, 110)
SHADOW = (52, 36, 20)


def strands(seed, count, size=SIZE):
    """The seeded visible pieces of stalks: (x0, y0, x1, y1, width,
    (r, g, b), opacity). Each stalk is half buried in the mud, so it shows
    as one to three pieces along its line with mud between them."""
    rng = random.Random(seed)
    out = []
    for _ in range(count):
        x, y = rng.uniform(0, size), rng.uniform(0, size)
        length = rng.uniform(*LENGTH)
        angle = math.radians(rng.uniform(-TILT, TILT))
        ux, uy = math.cos(angle), math.sin(angle)
        shade = rng.uniform(0.85, 1.1)
        colour = tuple(min(255, round(c * shade)) for c in STRAW)
        width = rng.uniform(1.0, 2.2)
        opacity = rng.uniform(0.4, 0.8)
        # Cut the stalk into pieces: points along it where it dips under.
        cuts = sorted(rng.uniform(0.0, 1.0) for _ in range(rng.randint(0, 4)))
        marks = [0.0] + cuts + [1.0]
        for k in range(0, len(marks) - 1, 2):
            a, b = marks[k], marks[k + 1]
            if (b - a) * length < 4.0:
                continue
            t0, t1 = (a - 0.5) * length, (b - 0.5) * length
            out.append(
                (x + ux * t0, y + uy * t0, x + ux * t1, y + uy * t1, width, colour, opacity)
            )
    return out


def wrapped(strand, size=SIZE):
    """`strand` and its copies shifted a tile over each edge it crosses, so
    the drawing tiles."""
    x0, y0, x1, y1 = strand[:4]
    shifts_x = [0.0]
    shifts_y = [0.0]
    if min(x0, x1) < 0:
        shifts_x.append(size)
    if max(x0, x1) > size:
        shifts_x.append(-size)
    if min(y0, y1) < 0:
        shifts_y.append(size)
    if max(y0, y1) > size:
        shifts_y.append(-size)
    return [
        (x0 + sx, y0 + sy, x1 + sx, y1 + sy) + tuple(strand[4:])
        for sx in shifts_x
        for sy in shifts_y
    ]


def mvg(stalks):
    """An ImageMagick draw script for `stalks`: each stalk's shadow, a texel
    down and right, then the stalk."""
    lines = ["stroke-linecap round", "fill none"]
    for x0, y0, x1, y1, width, (r, g, b), opacity in stalks:
        sr, sg, sb = SHADOW
        lines.append(f"stroke rgba({sr},{sg},{sb},{opacity * 0.4:.3f})")
        lines.append(f"stroke-width {width + 1.0:.2f}")
        lines.append(f"line {x0 + 1:.2f},{y0 + 1:.2f} {x1 + 1:.2f},{y1 + 1:.2f}")
        lines.append(f"stroke rgba({r},{g},{b},{opacity:.3f})")
        lines.append(f"stroke-width {width:.2f}")
        lines.append(f"line {x0:.2f},{y0:.2f} {x1:.2f},{y1:.2f}")
    return "\n".join(lines) + "\n"


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("set_folder")
    parser.add_argument("--seed", type=int, default=1919)
    parser.add_argument("--count", type=int, default=1300)
    args = parser.parse_args(argv)
    scan = os.path.join(args.set_folder, "scan_color.jpg")
    if not os.path.isfile(scan):
        sys.exit(f"building_straw.py: {scan} is missing")
    stalks = [w for s in strands(args.seed, args.count) for w in wrapped(s)]
    with tempfile.TemporaryDirectory() as scratch:
        script = os.path.join(scratch, "straw.mvg")
        with open(script, "w") as f:
            f.write(mvg(stalks))
        subprocess.run(
            [
                "magick",
                scan,
                "-draw",
                f"@{script}",
                "-quality",
                "92",
                os.path.join(args.set_folder, "color.jpg"),
            ],
            check=True,
        )
    print(f"{args.set_folder}/color.jpg: {len(stalks)} stalks")


if __name__ == "__main__":
    main()
