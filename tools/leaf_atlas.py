#!/usr/bin/env python3
"""Composes the plaza trees' leaf-card atlases from ambientCG leaf sheets.

An ambientCG leaf set is a sheet of single leaves (or, for LeafSet019,
three sprays) photographed flat. A tree card needs a whole sprig, so this
script cuts every leaf out of a sheet by its opacity map and lays the cut
leaves along drawn stems into four sprigs, one to a quarter of a square
atlas:

    paraiso    LeafSet013 leaflets in opposite pairs along a rachis: the
               paraíso's pinnate leaf, which no scanned set has
    olmo       LeafSet001 leaves alternating along a twig
    eucalipto  LeafSet003 long leaves hanging close to a twig
    aromo      LeafSet019's sprays, stood upright

Every sprig grows from the middle of its quarter, 2 * MARGIN texels over
its bottom edge (BASE), toward the top; the game's cards hang on that point (see
`crates/render/src/tree.rs`). Nothing crosses into the next quarter's
MARGIN, so the mips do not bleed between sprigs.

Each set folder holds the renamed sources `color.jpg`, `opacity.jpg`,
`normal_gl.jpg` and `roughness.jpg`. The atlas lands in
`<TREES_DIR>/<species>_leaves/` with the same four files, ready for
`tools/textures_to_ktx2.sh` (which packs the opacity into the colour's
alpha). Leaves are rotated, so the normal map's tangent-space x and y are
rotated with them. Outside the leaves the colour is the leaves' mean
colour, so the mips do not darken the cut edges.

The layout is seeded and the output is reproducible with the same
ImageMagick. Needs ImageMagick 7 (`magick`); standard library only.

Usage: python3 -I tools/leaf_atlas.py TREES_DIR [SPECIES...]
   e.g. python3 -I tools/leaf_atlas.py assets/textures/trees
"""

import math
import os
import random
import re
import subprocess
import sys
import tempfile
from dataclasses import dataclass

# Atlas side, texels: four 512 x 512 sprigs.
SIZE = 1024
# Side of a quarter, texels.
CELL = SIZE // 2
# Empty band round each sprig inside its quarter, texels.
MARGIN = 16
# Where each sprig grows from, in its quarter's texels: the middle,
# 2 * MARGIN over the bottom edge.
BASE = (CELL / 2, CELL - 2 * MARGIN)
# Leaves smaller than this, in texels, are specks of the cut-out.
MIN_AREA = 4000

# Species: (source set, layout, seed).
RECIPES = {
    "paraiso": ("LeafSet013", "pinnate", 13),
    "olmo": ("LeafSet001", "alternate", 1),
    "eucalipto": ("LeafSet003", "hanging", 3),
    "aromo": ("LeafSet019", "spray", 19),
}

# Stem colours, sRGB.
TWIG = (88, 76, 50)
RACHIS = (112, 122, 64)


@dataclass(frozen=True)
class Leaf:
    """One cut-out leaf of a sheet: its box and where it grows from, in the
    box's own texels."""

    x: int
    y: int
    w: int
    h: int
    base: tuple

    def corners(self):
        return [(0, 0), (self.w, 0), (0, self.h), (self.w, self.h)]


@dataclass(frozen=True)
class Stamp:
    """A leaf placed on the atlas: its base at `at`, scaled by `scale` and
    turned `angle` degrees clockwise (0: tip up)."""

    leaf: int
    at: tuple
    angle: float
    scale: float


def turn(p, angle):
    """Point `p` (image texels, y down) turned `angle` degrees clockwise."""
    a = math.radians(angle)
    c, s = math.cos(a), math.sin(a)
    return (p[0] * c - p[1] * s, p[0] * s + p[1] * c)


def placed(leaf, stamp):
    """The atlas positions of `leaf`'s box corners under `stamp`."""
    out = []
    for corner in leaf.corners():
        d = turn((corner[0] - leaf.base[0], corner[1] - leaf.base[1]), stamp.angle)
        out.append((stamp.at[0] + stamp.scale * d[0], stamp.at[1] + stamp.scale * d[1]))
    return out


def components(text, min_area=MIN_AREA):
    """The white objects of ImageMagick's verbose `-connected-components`
    listing `text`, as (x, y, w, h), largest first (ties: left to right)."""
    found = []
    pattern = re.compile(
        r"^\s*\d+:\s*(\d+)x(\d+)\+(\d+)\+(\d+)\s+\S+\s+(\d+)\s+(?:gray|srgb)\((\d+)"
    )
    for line in text.splitlines():
        m = pattern.match(line)
        if not m:
            continue
        w, h, x, y, area, value = map(int, m.groups())
        if value == 255 and area >= min_area:
            found.append((area, x, y, w, h))
    found.sort(key=lambda f: (-f[0], f[1], f[2]))
    return [(x, y, w, h) for _, x, y, w, h in found]


def leaves(boxes, kind, pad=4):
    """`Leaf`s for the component `boxes` of a sheet, padded by `pad`
    texels. Sheets stand their leaves upright, stalk down; a spray sheet
    lays them across, stalk to the right."""
    out = []
    for x, y, w, h in boxes:
        x, y, w, h = x - pad, y - pad, w + 2 * pad, h + 2 * pad
        base = (w, h / 2) if kind == "spray" else (w / 2, h - pad)
        out.append(Leaf(x, y, w, h, base))
    return out


def normal_matrix(angle):
    """ImageMagick `-color-matrix` turning an OpenGL tangent-space normal
    map's x and y as its image turns `angle` degrees clockwise. With y up,
    a clockwise turn of the image is a turn by -angle."""
    a = math.radians(angle)
    c, s = math.cos(a), math.sin(a)
    rows = [
        (c, s, 0.0, 0.0, 0.0, 0.5 - 0.5 * c - 0.5 * s),
        (-s, c, 0.0, 0.0, 0.0, 0.5 + 0.5 * s - 0.5 * c),
        (0.0, 0.0, 1.0, 0.0, 0.0, 0.0),
    ]
    return "6x3: " + " ".join(f"{v:.6f}" for row in rows for v in row)


def _stem(points, width, color):
    return [(points[k], points[k + 1], width, color) for k in range(len(points) - 1)]


def _curve(base, length, bend, steps=8):
    """Points of a stem `length` texels up from `base`, curving `bend`
    texels sideways by its tip."""
    return [
        (base[0] + bend * (k / steps) ** 2, base[1] - length * k / steps)
        for k in range(steps + 1)
    ]


def _along(points, t):
    """Point and clockwise heading (degrees, 0 up) at `t` (0..1) of a stem."""
    n = len(points) - 1
    k = min(int(t * n), n - 1)
    f = t * n - k
    (x0, y0), (x1, y1) = points[k], points[k + 1]
    heading = math.degrees(math.atan2(x1 - x0, y0 - y1))
    return (x0 + (x1 - x0) * f, y0 + (y1 - y0) * f), heading


def sprig(kind, cut, rng):
    """Stamps and stem segments of one sprig of `kind` in a quarter's own
    texels (its base at the bottom middle), from the `cut` leaves."""
    cell = CELL
    base = BASE
    reach = BASE[1] - MARGIN
    stamps, stems = [], []

    def leaf_at(t, points, side, spread, length):
        p, heading = _along(points, t)
        k = rng.randrange(len(cut))
        angle = heading + side * spread
        stamps.append(Stamp(k, p, angle, length / cut[k].h))

    if kind == "spray":
        k = rng.randrange(len(cut))
        angle = 90 + rng.uniform(-6, 6)
        stamps.append(Stamp(k, base, angle, reach / cut[k].w))
        return stamps, stems
    bend = rng.uniform(-40, 40)
    if kind == "pinnate":
        points = _curve(base, 0.9 * reach, bend)
        stems = _stem(points, 4, RACHIS)
        for j in range(6):
            t = 0.2 + 0.13 * j
            length = 0.3 * cell * (1 - 0.35 * t) * rng.uniform(0.92, 1.08)
            for side in (-1, 1):
                leaf_at(t, points, side, rng.uniform(56, 68), length)
        leaf_at(1.0, points, 0, 0, 0.27 * cell)
    elif kind == "alternate":
        points = _curve(base, 0.82 * reach, bend)
        stems = _stem(points, 5, TWIG)
        for j in range(6):
            t = 0.16 + 0.14 * j
            length = 0.36 * cell * (1 - 0.3 * t) * rng.uniform(0.9, 1.1)
            leaf_at(t, points, 1 if j % 2 else -1, rng.uniform(40, 56), length)
        leaf_at(1.0, points, 0, 0, 0.28 * cell)
    elif kind == "hanging":
        points = _curve(base, 0.7 * reach, bend * 0.5)
        stems = _stem(points, 4, TWIG)
        for j in range(4):
            t = 0.22 + 0.2 * j
            length = 0.5 * cell * (1 - 0.25 * t) * rng.uniform(0.9, 1.1)
            leaf_at(t, points, 1 if j % 2 else -1, rng.uniform(16, 28), length)
        leaf_at(1.0, points, 0, 0, 0.42 * cell)
    else:
        raise ValueError(f"unknown layout {kind}")
    return stamps, stems


def fit(stamps, stems, cut):
    """Shrinks a sprig about its base until every leaf and stem keeps
    MARGIN texels inside its quarter."""
    base = BASE
    points = [p for s in stamps for p in placed(cut[s.leaf], s)]
    points += [p for a, b, _, _ in stems for p in (a, b)]
    k = 1.0
    for x, y in points:
        dx, dy = x - base[0], y - base[1]
        if dx:
            k = min(k, (base[0] - MARGIN) / abs(dx))
        if dy < 0:
            k = min(k, (base[1] - MARGIN) / -dy)
        elif dy > 0:
            k = min(k, (CELL - MARGIN - base[1]) / dy)

    def shrink(p):
        return (base[0] + (p[0] - base[0]) * k, base[1] + (p[1] - base[1]) * k)

    stamps = [Stamp(s.leaf, shrink(s.at), s.angle, s.scale * k) for s in stamps]
    stems = [(shrink(a), shrink(b), w, c) for a, b, w, c in stems]
    return stamps, stems


def layout(kind, cut, seed):
    """The four sprigs of an atlas, in atlas texels: stamps and stems."""
    rng = random.Random(seed)
    cell = CELL
    stamps, stems = [], []
    for q in range(4):
        ox, oy = (q % 2) * cell, (q // 2) * cell
        s, t = fit(*sprig(kind, cut, rng), cut)
        stamps += [Stamp(a.leaf, (a.at[0] + ox, a.at[1] + oy), a.angle, a.scale) for a in s]
        stems += [((a[0] + ox, a[1] + oy), (b[0] + ox, b[1] + oy), w, c) for a, b, w, c in t]
    return stamps, stems


def magick(*args, capture=False):
    run = subprocess.run(["magick", *map(str, args)], check=True, capture_output=True, text=True)
    return run.stdout if capture else None


def find_leaves(src, kind):
    text = magick(
        os.path.join(src, "opacity.jpg"),
        "-threshold", "50%",
        "-define", "connected-components:verbose=true",
        "-define", "connected-components:area-threshold=64",
        "-connected-components", "8",
        "null:",
        capture=True,
    )
    return leaves(components(text), kind)


def _layers(src, map_name, cut, stamps, normal):
    """ImageMagick arguments for one stamp layer per stamp: map `map_name`
    of the sheet cut to its leaf, alpha from the opacity, turned and
    scaled onto the whole atlas."""
    args = []
    for s in stamps:
        leaf = cut[s.leaf]
        crop = f"{leaf.w}x{leaf.h}+{leaf.x}+{leaf.y}"
        args += ["(", os.path.join(src, map_name), "-crop", crop, "+repage"]
        if normal:
            args += ["-color-matrix", normal_matrix(s.angle)]
        args += [
            "(", os.path.join(src, "opacity.jpg"), "-crop", crop, "+repage",
            "-channel", "R", "-separate", "+channel", ")",
            "-alpha", "off", "-compose", "CopyOpacity", "-composite", "-compose", "Over",
            "-virtual-pixel", "transparent",
            "-define", f"distort:viewport={SIZE}x{SIZE}+0+0",
            "-distort", "SRT",
            f"{leaf.base[0]:.2f},{leaf.base[1]:.2f} {s.scale:.5f} {s.angle:.3f} "
            f"{s.at[0]:.2f},{s.at[1]:.2f}",
            # Settings outlive parentheses: the next cut-out must not
            # inherit a transparent edge, which would give its opacity an
            # alpha channel of its own for CopyOpacity to copy instead.
            "+virtual-pixel",
            ")",
        ]
    return args


def _draw_stems(stems):
    args = []
    for a, b, width, color in stems:
        args += [
            "-stroke", f"rgb({color[0]},{color[1]},{color[2]})",
            "-strokewidth", str(width),
            "-draw", f"line {a[0]:.1f},{a[1]:.1f} {b[0]:.1f},{b[1]:.1f}",
        ]
    return args


def build(src, out, kind, seed):
    """Writes the atlas of sheet folder `src` laid out as `kind` into `out`."""
    cut = find_leaves(src, kind)
    if not cut:
        raise SystemExit(f"leaf_atlas.py: no leaves found in {src}/opacity.jpg")
    stamps, stems = layout(kind, cut, seed)
    os.makedirs(out, exist_ok=True)
    with tempfile.TemporaryDirectory() as scratch:
        rgba = os.path.join(scratch, "rgba.png")
        magick(
            "-size", f"{SIZE}x{SIZE}", "xc:none", "-fill", "none", *_draw_stems(stems),
            *_layers(src, "color.jpg", cut, stamps, False),
            "-background", "none", "-flatten", f"PNG32:{rgba}",
        )
        magick(rgba, "-alpha", "extract", "-quality", "95", os.path.join(out, "opacity.jpg"))
        mean = magick(
            rgba, "-resize", "1x1!", "-alpha", "off", "-format", "%[pixel:p{0,0}]", "info:",
            capture=True,
        ).strip()
        magick(
            rgba, "-background", mean, "-alpha", "remove", "-alpha", "off",
            "-quality", "95", os.path.join(out, "color.jpg"),
        )
        flat = "rgb(128,128,255)"
        magick(
            "-size", f"{SIZE}x{SIZE}", f"xc:{flat}",
            *_layers(src, "normal_gl.jpg", cut, stamps, True),
            "-background", flat, "-flatten", "-alpha", "off",
            "-quality", "95", os.path.join(out, "normal_gl.jpg"),
        )
        magick(
            "-size", f"{SIZE}x{SIZE}", "xc:gray(70%)", "-fill", "none",
            *[a if not a.startswith("rgb(") else "gray(70%)" for a in _draw_stems(stems)],
            *_layers(src, "roughness.jpg", cut, stamps, False),
            "-background", "gray(70%)", "-flatten", "-alpha", "off",
            "-colorspace", "Gray", "-quality", "95", os.path.join(out, "roughness.jpg"),
        )
    return stamps


def main(argv):
    if not argv or argv[0] in ("-h", "--help"):
        print(__doc__.strip().splitlines()[-2], file=sys.stderr)
        return 1
    trees, species = argv[0], argv[1:] or list(RECIPES)
    for name in species:
        if name not in RECIPES:
            print(f"leaf_atlas.py: unknown species {name}", file=sys.stderr)
            return 1
        source, kind, seed = RECIPES[name]
        out = os.path.join(trees, f"{name}_leaves")
        stamps = build(os.path.join(trees, source), out, kind, seed)
        print(f"{out}: {len(stamps)} leaves from {source} ({kind})")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
