#!/usr/bin/env python3
"""Lays chopped straw into the mud of a rancho de barro y paja's wall.

The walls of a 1919 pampa rancho de barro y paja (chorizo or quincha) are
mud kneaded with chopped straw and packed in layers, so short golden stalks
show all over the dried, cracked surface, lying mostly along the layers.
No free scan shows that, so this script draws the straw over a scan of
hand-plastered mud (Poly Haven's `clay_plaster`, CC0, kept as
`scan_color.jpg`; the game's shader cracks each wall its own way): seeded
stalks 4 to 14 cm long on the scan's 2 m tile,
within 35 degrees of the horizontal, each a mud-dulled gold stalk over a
faint shadow line, half sunk in the mud: partly transparent, and showing in
one to three pieces along its line. Strands that cross
an edge are drawn again from the opposite edge, so the colour still tiles.

The set folder keeps the scan's `normal_gl.jpg`, `roughness.jpg` and
`ao.jpg` as they are; this writes `color.jpg` next to them, ready for
`tools/textures_to_ktx2.sh`. The output is reproducible with the same seed
and ImageMagick. Needs ImageMagick 7 (`magick`); standard library only.

With `--thatch` it instead draws a whole long-straw thatch set from
scratch, for the roof of the mud-and-straw rancho (`straw_thatch`): seeded
stalks 12 to 42 cm long on a 1 m tile, lying within 6 degrees of the
slope, layer over layer, each a weathered grey-to-gold stalk with a dark
shadow and a pale lit edge; the later (upper) layers lighter and higher.
It writes `color.jpg`, `normal_gl.jpg` (OpenGL, from the stalks' height,
the same Sobel step as `thatch/generate.sh`), `roughness.jpg` and `ao.jpg`.
The normal map needs an HDRI build of ImageMagick 7.

Usage: python3 -I tools/building_straw.py SET_FOLDER [--seed N] [--count N]
       python3 -I tools/building_straw.py --thatch SET_FOLDER [--seed N] [--count N]
  e.g. python3 -I tools/building_straw.py assets/textures/buildings/mud_straw
       python3 -I tools/building_straw.py --thatch assets/textures/buildings/straw_thatch
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


# Thatch stalk lengths in texels: 12 to 42 cm on a 1 m tile.
THATCH_LENGTH = (120.0, 420.0)
# At most this far from the line of the slope, in degrees.
THATCH_TILT = 6.0
# Old thatch weathers from gold to a silvery grey-brown: the palette of its
# stalks, most of them weathered, a few still golden, some dark and rotten.
THATCH_PALETTE = (
    ((122, 108, 86), 4.0),
    ((146, 136, 116), 3.0),
    ((164, 142, 102), 2.0),
    ((186, 156, 100), 1.0),
    ((86, 72, 54), 1.5),
)
# The dark of the gaps between the stalks.
THATCH_GAP = (44, 37, 28)


def thatch_stalks(seed, count, size=SIZE):
    """The seeded thatch stalks, in drawing order (bottom layer first):
    (x0, y0, x1, y1, width, (r, g, b), height). `height` rises from about
    0.15 for the first stalk to about 0.95 for the last, so a later stalk
    lies over the earlier ones, and the lower layers are darker."""
    rng = random.Random(seed)
    weights = [w for _, w in THATCH_PALETTE]
    out = []
    for k in range(count):
        rank = k / max(count - 1, 1)
        x, y = rng.uniform(0, size), rng.uniform(0, size)
        length = rng.uniform(*THATCH_LENGTH)
        angle = math.radians(90.0 + rng.uniform(-THATCH_TILT, THATCH_TILT))
        ux, uy = math.cos(angle), math.sin(angle)
        base = rng.choices([c for c, _ in THATCH_PALETTE], weights)[0]
        # The deeper layers are in the shade of the ones over them.
        shade = (0.55 + 0.45 * rank) * rng.uniform(0.88, 1.08)
        colour = tuple(min(255, round(c * shade)) for c in base)
        width = rng.uniform(2.0, 4.6)
        height = min(0.98, 0.15 + 0.8 * rank + rng.uniform(-0.1, 0.1))
        half = length / 2.0
        out.append(
            (x - ux * half, y - uy * half, x + ux * half, y + uy * half, width, colour, height)
        )
    return out


def thatch_mvg(stalks, height=False):
    """An ImageMagick draw script for `stalks`, wrapped round the tile: the
    colour (a shadow beside each stalk, the stalk, a pale lit edge), or, with
    `height`, each stalk's height, rounded across its width."""
    lines = ["stroke-linecap round", "fill none"]
    for x0, y0, x1, y1, width, (r, g, b), h in stalks:
        if height:
            for w, v in ((width, h - 0.08), (width * 0.5, h)):
                grey = round(255 * max(0.0, v))
                lines.append(f"stroke rgb({grey},{grey},{grey})")
                lines.append(f"stroke-width {w:.2f}")
                lines.append(f"line {x0:.2f},{y0:.2f} {x1:.2f},{y1:.2f}")
            continue
        sr, sg, sb = THATCH_GAP
        lines.append(f"stroke rgba({sr},{sg},{sb},0.55)")
        lines.append(f"stroke-width {width + 1.6:.2f}")
        lines.append(f"line {x0 + 1.5:.2f},{y0:.2f} {x1 + 1.5:.2f},{y1:.2f}")
        lines.append(f"stroke rgb({r},{g},{b})")
        lines.append(f"stroke-width {width:.2f}")
        lines.append(f"line {x0:.2f},{y0:.2f} {x1:.2f},{y1:.2f}")
        lr, lg, lb = (min(255, round(c * 1.18 + 12)) for c in (r, g, b))
        lines.append(f"stroke rgba({lr},{lg},{lb},0.6)")
        lines.append(f"stroke-width {width * 0.3:.2f}")
        lines.append(f"line {x0 - width * 0.25:.2f},{y0:.2f} {x1 - width * 0.25:.2f},{y1:.2f}")
    return "\n".join(lines) + "\n"


def write_thatch(folder, seed, count):
    """Draws the long-straw thatch set into `folder`."""
    os.makedirs(folder, exist_ok=True)
    stalks = [w for s in thatch_stalks(seed, count) for w in wrapped(s)]
    gap = "rgb({},{},{})".format(*THATCH_GAP)
    with tempfile.TemporaryDirectory() as scratch:
        colour, height = (os.path.join(scratch, f) for f in ("colour.mvg", "height.mvg"))
        with open(colour, "w") as f:
            f.write(thatch_mvg(stalks))
        with open(height, "w") as f:
            f.write(thatch_mvg(stalks, height=True))
        size = f"{SIZE}x{SIZE}"
        run = lambda *a: subprocess.run(["magick", *a], check=True)  # noqa: E731
        run("-size", size, f"xc:{gap}", "-draw", f"@{colour}", "-quality", "92",
            os.path.join(folder, "color.jpg"))
        h = os.path.join(scratch, "height.png")
        run("-size", size, "xc:black", "-draw", f"@{height}", "-alpha", "off",
            "-colorspace", "gray", "-blur", "0x0.7", "-auto-level", h)
        # OpenGL normals from the height, as in thatch/generate.sh.
        run(h, "-virtual-pixel", "tile", "-define", "convolve:scale=!",
            "(", "-clone", "0", "-morphology", "Convolve", "Sobel:180",
            "-evaluate", "multiply", "0.6", "-evaluate", "add", "50%", ")",
            "(", "-clone", "0", "-morphology", "Convolve", "Sobel:90",
            "-evaluate", "multiply", "0.6", "-evaluate", "add", "50%", ")",
            "(", "-clone", "0", "-evaluate", "set", "100%", ")",
            "-delete", "0", "-combine", "-quality", "92",
            os.path.join(folder, "normal_gl.jpg"))
        # Dry straw is rough; the gaps a little rougher still.
        run(h, "-negate", "+level", "72%,96%", "-quality", "92",
            os.path.join(folder, "roughness.jpg"))
        # The gaps deep between the stalks are occluded.
        run(h, "-virtual-pixel", "tile", "-blur", "0x3", "+level", "45%,100%", "-quality", "92", os.path.join(folder, "ao.jpg"))
    print(f"{folder}: thatch of {len(stalks)} stalks")


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("set_folder")
    parser.add_argument("--thatch", action="store_true", help="draw a thatch set")
    parser.add_argument("--seed", type=int, default=1919)
    parser.add_argument("--count", type=int, default=None)
    args = parser.parse_args(argv)
    if args.thatch:
        write_thatch(args.set_folder, args.seed, args.count or 9000)
        return
    scan = os.path.join(args.set_folder, "scan_color.jpg")
    if not os.path.isfile(scan):
        sys.exit(f"building_straw.py: {scan} is missing")
    stalks = [w for s in strands(args.seed, args.count or 1300) for w in wrapped(s)]
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
