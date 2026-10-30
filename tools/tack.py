"""Paint the map sets of the horse's recado criollo and bridle (1919, La
Pampa), by script, with numpy: no Blender and no third-party images.

    python3 -I tools/tack.py assets/models/tack
    FORCE=1 sh tools/textures_to_ktx2.sh assets/models/tack/*/

Writes the editable JPEG sources of four sets under OUT (``color.jpg``,
``normal_gl.jpg``, ``roughness.jpg``, ``ao.jpg`` and, for the blankets and
the fleece, ``opacity.jpg``); ``textures_to_ktx2.sh`` turns each into the
``color``/``normal_gl``/``orm`` KTX2 maps the game loads
(``pampa_render::character::tack``). The game repeats the fleece and the
rawhide over their pieces; the blankets, the carona and the sobrepuesto
map their image once (their borders are painted in).

- ``tack_blankets`` (1024²): the two woven wool blankets side by side.
  Left half, the bajera: a coarse twill jerga in natural browns with cream
  and madder-red stripes along its lower edges. Right half, the matra: a
  warp-faced Indian-loom blanket (Zeballos, 1878: "tejido grueso de lana de
  1x0,50m., salido del telar de los indios") with a guarda of stepped
  rhombs in madder red, indigo and cream along both lower edges, stripes
  at its front and back and the warp fringe where the weave ends (in the
  opacity: the game draws the blankets with alpha test). Rows run round the horse (row 0 hangs on
  its left flank, the last row on its right); columns run front to back.
- ``tack_leather`` (512²): suela, the vegetable-tanned sole leather of the
  carona, the bastos and the stirrup leathers: honey brown, mottled and
  scratched by work, with a fire-tooled double line and stamped dots round
  its border (Hutchinson, 1862: "adornado con figuras hechas en la suela con
  hierro candente") and a burnished edge. The carona maps the whole image;
  the other pieces its plain middle.
- ``carpincho`` and ``fleece`` (no set of their own): the capybara leather
  of the sobrepuesto and the cojinillo's long wool, painted here for
  ``tools/recado.py``, which bakes them into the recado's atlas.
- ``tack_rawhide`` (512²): cuero crudo. Left half, a four-strand round braid
  (trenza) whose chevrons run down the strap, for the reins and the
  headstall; third quarter, a plain flat strip for the cinchón and the
  látigo; last quarter, the cincha: ten thin strips (tientos) held together
  by a cross-woven panel.
- ``tack_iron`` (256²): forged iron for the bit and the stirrups, dark and
  hammered, with rust in its hollows.

The noise is seeded, so the output is the same on every run.
"""

import os
import subprocess
import sys
import tempfile

import numpy as np

# ---------------------------------------------------------------------------
# Tiling noise and normals.


def value_noise(n, cells, seed, m=None):
    """Smooth value noise on an ``n`` × ``m`` image that wraps on both axes,
    with ``cells`` lattice cells across (an int, or a pair (across, down))."""
    m = n if m is None else m
    cx, cy = (cells, cells) if np.isscalar(cells) else cells
    cx, cy = max(1, int(cx)), max(1, int(cy))
    grid = np.random.default_rng(seed).random((cy, cx))
    x = np.arange(n) * cx / n
    y = np.arange(m) * cy / m
    x0, y0 = np.floor(x).astype(int), np.floor(y).astype(int)
    fx, fy = x - x0, y - y0
    fx, fy = fx * fx * (3 - 2 * fx), fy * fy * (3 - 2 * fy)
    x1, y1 = (x0 + 1) % cx, (y0 + 1) % cy
    x0, y0 = x0 % cx, y0 % cy
    a = grid[np.ix_(y0, x0)] * (1 - fx) + grid[np.ix_(y0, x1)] * fx
    b = grid[np.ix_(y1, x0)] * (1 - fx) + grid[np.ix_(y1, x1)] * fx
    return a * (1 - fy[:, None]) + b * fy[:, None]


def fbm(n, cells, seed, octaves=4, m=None, stretch=(1, 1)):
    """Fractal value noise in 0..1 that wraps, ``octaves`` doublings from
    ``cells`` cells across (times ``stretch`` across and down)."""
    cx, cy = (cells, cells) if np.isscalar(cells) else cells
    total, amp, norm = 0.0, 1.0, 0.0
    for k in range(octaves):
        c = (cx * 2**k * stretch[0], cy * 2**k * stretch[1])
        total = total + amp * value_noise(n, c, seed + 17 * k, m)
        norm += amp
        amp *= 0.5
    return total / norm


def normals(height, strength):
    """An OpenGL (+Y up the image) normal map, encoded 0..1, from ``height``
    (wrapping), ``strength`` scaling its slopes."""
    dx = (np.roll(height, -1, axis=1) - np.roll(height, 1, axis=1)) * 0.5 * strength
    dy = (np.roll(height, -1, axis=0) - np.roll(height, 1, axis=0)) * 0.5 * strength
    n = np.stack([-dx, dy, np.ones_like(height)], axis=2)
    n /= np.linalg.norm(n, axis=2, keepdims=True)
    return n * 0.5 + 0.5


def mix(a, b, t):
    """``a`` to ``b`` (colours) by ``t`` (a mask, 0..1)."""
    t = np.asarray(t, dtype=float)
    if t.ndim == 2:
        t = t[..., None]
    return np.asarray(a, dtype=float) * (1 - t) + np.asarray(b, dtype=float) * t


def smooth(e0, e1, x):
    t = np.clip((x - e0) / (e1 - e0), 0.0, 1.0)
    return t * t * (3 - 2 * t)


def grid(n, m=None):
    """Pixel-centre coordinates in 0..1: (x across, y down)."""
    m = n if m is None else m
    x = (np.arange(n) + 0.5) / n
    y = (np.arange(m) + 0.5) / m
    return np.meshgrid(x, y)


def bands(e, stops, default):
    """The colour of each value of ``e`` (distance from an edge) from
    ``stops`` [(up to, colour)], ``default`` past the last."""
    out = np.empty(e.shape + (3,))
    out[...] = default
    for limit, colour in reversed(stops):
        out[e < limit] = colour
    return out


# ---------------------------------------------------------------------------
# The sets.

CREAM = (0.80, 0.74, 0.62)
GREIGE = (0.66, 0.62, 0.55)
BROWN = (0.30, 0.22, 0.16)
DARK = (0.16, 0.13, 0.11)
MADDER = (0.50, 0.15, 0.10)
INDIGO = (0.15, 0.17, 0.30)


def weave(n, m, pitch, seed, twill):
    """The relief of a woven blanket, ``pitch`` pixels a thread: a twill's
    diagonal ribs or a warp-faced weave's ribs down the rows, with yarns of
    uneven thickness."""
    x = np.arange(n)[None, :]
    y = np.arange(m)[:, None]
    if twill:
        rib = 0.5 + 0.5 * np.cos(2 * np.pi * (x + y) / (2 * pitch))
    else:
        warp = 0.5 + 0.5 * np.cos(2 * np.pi * x / pitch)
        weft = 0.5 + 0.5 * np.cos(np.pi * y / pitch + np.pi * (x // pitch % 2))
        rib = 0.75 * warp + 0.25 * weft * warp
    yarn = value_noise(n, (max(1, n // pitch), 3), seed, m)
    return rib * (0.7 + 0.3 * yarn)


def blankets(size):
    """The bajera (left half) and the matra (right half)."""
    half = size // 2
    x, y = grid(half, size)
    e = np.minimum(y, 1 - y)
    # The bajera: a jerga, stripes along the horse near its lower edges and
    # a field of broad browns.
    field = np.where(((e * 16).astype(int) % 2 == 0)[..., None], BROWN, DARK)
    bajera = bands(
        e,
        [(0.012, DARK), (0.03, CREAM), (0.045, BROWN), (0.058, MADDER), (0.07, BROWN), (0.085, CREAM)],
        (0.0, 0.0, 0.0),
    )
    bajera = np.where((e >= 0.085)[..., None], field, bajera)
    # The matra: greige field, its guarda along both lower edges and stripes
    # at the front and back.
    side = np.minimum(x, 1 - x)
    # The warp fringe (flecos) where the weave ends, front and back: one
    # thread every few texels, each cut to its own length.
    fray = 0.07
    thread = np.floor(y * size / max(2, size // 256 * 3))
    length = fray * (0.35 + 0.65 * ((np.sin(thread * 12.9898) * 43758.547) % 1.0))
    in_thread = (np.floor(y * size) % max(2, size // 256 * 3)) < max(1, size // 256 * 2)
    opacity = np.ones((size, half))
    opacity[(side < fray) & ~((side >= fray - length) & in_thread)] = 0.0
    side = side - fray
    matra = np.empty((size, half, 3))
    matra[...] = GREIGE
    stripes = bands(side, [(0.0, CREAM), (0.02, DARK), (0.045, MADDER), (0.06, CREAM), (0.08, INDIGO)], GREIGE)
    matra = np.where((side < 0.08)[..., None], stripes, matra)
    guard_from, guard_to = 0.022, 0.082
    edge = bands(e, [(0.01, DARK), (guard_from, MADDER)], (0, 0, 0))
    edge = np.where((e >= guard_to)[..., None], bands(e, [(0.092, MADDER), (0.1, DARK)], GREIGE), edge)
    # Stepped rhombs (escalonado) along the guarda: indigo with a cream
    # ring and a madder heart on madder.
    r = np.clip((e - guard_from) / (guard_to - guard_from), 0, 1)
    period = 0.09 / 0.5  # rhombs every ~9 cm, the matra half 0.5 m long
    s = (x / period) % 1.0
    steps = 6
    d = (np.floor(np.abs(s - 0.5) * 2 * steps) + np.floor(np.abs(r - 0.5) * 2 * steps)) / steps
    motif = np.empty_like(matra)
    motif[...] = MADDER
    motif[d < 1.0] = INDIGO
    motif[d < 0.67] = CREAM
    motif[d < 0.34] = MADDER
    in_guard = (e >= guard_from) & (e < guard_to)
    edge = np.where(in_guard[..., None], motif, edge)
    matra = np.where((e < 0.1)[..., None], edge, matra)
    pitch = max(1, round(2.6 * size / 1024))
    h_b = weave(half, size, pitch, 11, twill=True)
    h_m = weave(half, size, pitch, 12, twill=False)
    colour = np.concatenate([bajera, matra], axis=1)
    height = np.concatenate([h_b, h_m], axis=1)
    wool = fbm(size, 12, 21, 4)
    yarn = value_noise(size, (size // max(1, pitch), 2), 23)
    colour = colour * (0.82 + 0.18 * height)[..., None] * (0.9 + 0.1 * wool + 0.06 * (yarn - 0.5))[..., None]
    # Dust and sweat darken the edges that hang on the flanks.
    ye = np.minimum(grid(size)[1], 1 - grid(size)[1])
    colour *= (0.86 + 0.14 * smooth(0.0, 0.2, ye))[..., None]
    return {
        "color": np.clip(colour, 0, 1),
        "opacity": np.concatenate([np.ones((size, half)), opacity], axis=1),
        "normal_gl": normals(height * 1.5 + wool * 2.0, 1.6),
        "roughness": np.clip(0.9 + 0.08 * (1 - height), 0, 1),
        "ao": 0.72 + 0.28 * height,
        "height": height,
    }


def scratches(n, count, seed):
    """Thin wrapping lines of random length and angle, 0..1."""
    rng = np.random.default_rng(seed)
    out = np.zeros((n, n))
    for _ in range(count):
        x, y = rng.random(2) * n
        a = rng.random() * np.pi
        length = rng.uniform(0.02, 0.12) * n
        t = np.linspace(0, length, int(length * 2) + 2)
        xs = (np.round(x + np.cos(a) * t) % n).astype(int)
        ys = (np.round(y + np.sin(a) * t) % n).astype(int)
        np.maximum.at(out, (ys, xs), rng.uniform(0.3, 1.0))
    return out


def leather(size):
    """Suela: the carona's whole image, the plain middle for the rest."""
    x, y = grid(size)
    base = np.array((0.50, 0.32, 0.18))
    mottle = fbm(size, 5, 31, 5)
    stains = smooth(0.55, 0.75, fbm(size, 3, 32, 3))
    grain = fbm(size, size // 6, 33, 2)
    scratch = scratches(size, 50, 34)
    wear = smooth(0.45, 0.7, fbm(size, 4, 35, 4))
    colour = base * (0.78 + 0.4 * mottle)[..., None]
    colour = mix(colour, colour * 0.62, stains * 0.8)
    colour = mix(colour, colour * 1.25, wear * 0.5)
    colour = mix(colour, (0.62, 0.45, 0.3), scratch * 0.3)
    colour *= (0.93 + 0.07 * grain)[..., None]
    d = np.minimum(np.minimum(x, 1 - x), np.minimum(y, 1 - y))
    px = 1.0 / size
    # The burnished edge, darker and smoother.
    burnish = 1 - smooth(0.004, 0.012, d)
    colour = mix(colour, colour * 0.55, burnish)
    # The fire-tooled double line and its stamped dots.
    line = np.zeros_like(d)
    for at in (0.032, 0.048):
        line = np.maximum(line, 1 - smooth(0.6 * px, 2.2 * px, np.abs(d - at)))
    edge_x = np.where(np.minimum(x, 1 - x) < np.minimum(y, 1 - y), y, x)
    dots = ((np.abs(d - 0.04) < 0.0045) & (np.abs((edge_x / 0.02) % 1.0 - 0.5) < 0.18)).astype(float)
    tooled = np.maximum(line, dots)
    colour = mix(colour, (0.20, 0.11, 0.06), tooled * 0.85)
    height = 0.25 * mottle + 0.12 * grain - 0.35 * scratch - 0.8 * tooled - 0.3 * burnish
    return {
        "color": np.clip(colour, 0, 1),
        "normal_gl": normals(height, 3.0),
        "roughness": np.clip(0.62 - 0.14 * wear - 0.1 * burnish + 0.1 * stains + 0.06 * grain, 0, 1),
        "ao": np.clip(1.0 - 0.45 * tooled - 0.15 * scratch, 0, 1),
        "height": height,
    }


def stamp(height, xs, ys, radius, depth):
    """Dimples of ``radius`` pixels at (xs, ys), wrapping, into ``height``."""
    n = height.shape[0]
    r = int(np.ceil(radius * 2))
    oy, ox = np.mgrid[-r : r + 1, -r : r + 1]
    for cx, cy, d in zip(xs, ys, depth):
        ix, iy = int(np.floor(cx)), int(np.floor(cy))
        dist2 = (ox + ix - cx) ** 2 + (oy + iy - cy) ** 2
        k = np.exp(-dist2 / (radius * radius))
        rows = (oy + iy) % n
        cols = (ox + ix) % n
        np.subtract.at(height, (rows, cols), d * k)


def carpincho(size, edges=True):
    """The sobrepuesto's capybara leather, the whole image over the piece:
    pebbled grain, its pores in little groups between the pebbles, and edges
    darkened by handling (without ``edges``, a tile that repeats)."""
    rng = np.random.default_rng(41)
    # The pebbles: rounded bumps a few millimetres across, packed.
    pebbles = np.zeros((size, size))
    k = int(size * size / 30)
    stamp(pebbles, rng.random(k) * size, rng.random(k) * size, max(0.8, size / 170), -rng.uniform(0.3, 1.0, k))
    pebbles = np.clip(pebbles / (pebbles.max() + 1e-6), 0, 1)
    cells = max(4, size // 14)
    pitch = size / cells
    xs, ys = [], []
    for j in range(cells):
        for i in range(cells):
            count = rng.integers(2, 5)
            cx = (i + 0.5 + rng.uniform(-0.25, 0.25)) * pitch
            cy = (j + 0.5 + (i % 2) * 0.5 + rng.uniform(-0.25, 0.25)) * pitch
            a = rng.uniform(0, np.pi)
            for p in range(count):
                off = (p - (count - 1) / 2) * pitch * 0.22
                xs.append(cx + np.cos(a) * off)
                ys.append(cy + np.sin(a) * off)
    pores = np.zeros((size, size))
    stamp(pores, xs, ys, max(0.7, pitch * 0.09), np.ones(len(xs)))
    pores = np.clip(-pores, 0, 1)
    x, y = grid(size)
    d = np.minimum(np.minimum(x, 1 - x), np.minimum(y, 1 - y))
    edge = (1 - smooth(0.0, 0.07, d)) if edges else np.zeros_like(d)
    tone = fbm(size, 4, 44, 4)
    colour = np.array((0.50, 0.37, 0.25)) * (0.84 + 0.26 * tone)[..., None]
    colour *= (0.9 + 0.14 * pebbles)[..., None]
    colour = mix(colour, (0.27, 0.18, 0.11), pores * 0.75)
    colour = mix(colour, colour * 0.55, edge)
    height = 0.5 * pebbles - 0.7 * pores + 0.15 * fbm(size, (3, 10), 43, 3)
    return {
        "color": np.clip(colour, 0, 1),
        "normal_gl": normals(height, 2.5 * size / 512 + 1.5),
        "roughness": np.clip(0.72 + 0.12 * pores - 0.1 * pebbles - 0.12 * edge, 0, 1),
        "ao": np.clip(1.0 - 0.5 * pores - 0.15 * (1 - pebbles), 0, 1),
        "height": height,
    }


def fleece(size):
    """The cojinillo's long wool: curly locks of random length, width and
    lean hanging down the rows, creamy, darker at the roots."""
    rng = np.random.default_rng(51)
    height = np.zeros((size, size))
    rx, ry = size / 46, size / 9
    # Enough locks to cover the image about one and a half times over.
    count = int(1.6 * size * size / (np.pi * rx * ry))
    for _ in range(count):
        cx, cy = rng.random(2) * size
        sx, sy = rx * rng.uniform(0.55, 1.5), ry * rng.uniform(0.45, 1.7)
        lean = rng.uniform(-0.6, 0.6)
        curl, phase = rng.uniform(2.0, 6.0), rng.uniform(0, 2 * np.pi)
        wave = sx * rng.uniform(0.2, 0.7)
        r = int(np.ceil(sy * 1.1))
        w = int(np.ceil(sx * 1.4 + abs(lean) * sy + wave)) + 1
        oy, ox = np.mgrid[-r : r + 1, -w : w + 1]
        ix, iy = int(cx), int(cy)
        dy = oy + iy - cy
        dx = ox + ix - cx - lean * dy - wave * np.sin(dy / sy * curl + phase)
        d2 = (dx / sx) ** 2 + (dy / sy) ** 2
        # A lock: full at its root, thinning to a curled tip.
        along = np.clip(dy / sy * 0.5 + 0.5, 0, 1)
        lock = np.clip(1 - d2, 0, 1) ** 0.5 * (1 - 0.4 * along) + 0.08 * rng.random()
        rows, cols = (oy + iy) % size, (ox + ix) % size
        np.maximum.at(height, (rows, cols), lock)
    fibre = value_noise(size, (size // 3, size // 24), 52)
    strands = value_noise(size, (size // 2, size // 40), 54)
    detail = 0.62 + 0.22 * fibre + 0.16 * strands
    level = np.quantile(height, 0.35)
    opacity = smooth(level - 0.02, level + 0.02, height)
    tips = smooth(0.15, 0.85, 1 - height)
    colour = mix((0.89, 0.83, 0.69), (0.70, 0.60, 0.45), tips * 0.75)
    colour = mix((0.68, 0.60, 0.47), colour, smooth(0.0, level + 0.1, height))
    colour = colour * detail[..., None] * (0.9 + 0.14 * fbm(size, 4, 53, 3))[..., None]
    h = height * detail
    return {
        "color": np.clip(colour, 0, 1),
        "normal_gl": normals(h, 2.0 * size / 512),
        "roughness": np.clip(0.88 + 0.1 * tips, 0, 1),
        "ao": np.clip(0.45 + 0.55 * height, 0, 1),
        "opacity": opacity,
        "height": h,
    }


def rawhide(size):
    """Three strips side by side: the first half, a round braid; the third
    quarter, a plain flat strip (the cinchón and the látigo); the last
    quarter, the cincha: many thin strips held together by a cross-woven
    panel."""
    half, quarter = size // 2, size // 4
    a, v = grid(half, size)
    repeats = 8  # braid periods down the image
    # Two columns of chevrons, the second half a period down: the face of a
    # four-strand round braid.
    col = (a * 2) % 1.0
    shift = np.where(a < 0.5, 0.0, 0.5)
    s = (v * repeats * 2 + np.abs(col - 0.5) * 1.6 + shift) % 1.0
    strand = np.sin(np.pi * s) ** 0.6
    groove = smooth(0.0, 0.08, np.abs(col - 0.5)) * smooth(0.0, 0.08, 0.5 - np.abs(col - 0.5))
    braid = strand * (0.35 + 0.65 * groove)
    # The plain strip: rounded across, its edges thinner.
    a2, v2 = grid(quarter, size)
    plain = np.sin(np.pi * np.clip(a2, 0.02, 0.98)) ** 0.3 * 0.8 + 0.2
    # The cincha: ten strips, crossed by a woven panel a quarter of each
    # period long, twice down the image.
    strips = np.sin(np.pi * ((a2 * 10) % 1.0)) ** 0.35
    panel_v = (v2 * 2) % 1.0
    panel = smooth(0.0, 0.03, 0.14 - np.abs(panel_v - 0.5))
    over = ((np.floor(a2 * 10) + np.floor(v2 * size / 6)) % 2 == 0).astype(float)
    weft = np.sin(np.pi * ((v2 * size / 6) % 1.0)) ** 0.5
    woven = np.where(over > 0, strips, weft)
    cincha = strips * (1 - panel) + woven * panel
    height = np.concatenate([braid, plain, cincha], axis=1)
    grime = fbm(size, 6, 61, 4)
    tone = fbm(size, (3, 12), 62, 3)
    colour = mix((0.42, 0.29, 0.16), (0.80, 0.69, 0.50), np.clip(height, 0, 1))
    colour = colour * (0.85 + 0.22 * tone)[..., None]
    colour = mix(colour, colour * 0.72, smooth(0.6, 0.85, grime) * 0.6)
    return {
        "color": np.clip(colour, 0, 1),
        "normal_gl": normals(height, 3.0 * size / 512),
        "roughness": np.clip(0.55 + 0.25 * (1 - height) + 0.1 * grime, 0, 1),
        "ao": np.clip(0.5 + 0.5 * height, 0, 1),
        "height": height,
    }


def iron(size):
    """Forged iron, hammered, rust in its hollows."""
    rng = np.random.default_rng(71)
    height = 0.3 * fbm(size, 4, 72, 4)
    k = max(8, size * size // 180)
    stamp(
        height,
        rng.random(k) * size,
        rng.random(k) * size,
        size / 40,
        rng.uniform(0.05, 0.25, k),
    )
    pits = fbm(size, size // 8, 73, 2)
    rust = 0.75 * smooth(0.58, 0.76, fbm(size, (3, 9), 74, 5) + 0.25 * (0.3 - height))
    colour = np.array((0.33, 0.32, 0.31)) * (0.8 + 0.4 * fbm(size, 8, 75, 3))[..., None]
    colour = mix(colour, (0.36, 0.22, 0.13) * (0.8 + 0.4 * pits)[..., None], rust)
    h = height - 0.15 * rust * pits
    return {
        "color": np.clip(colour, 0, 1),
        "normal_gl": normals(h, 6.0 * size / 256),
        "roughness": np.clip(0.42 + 0.48 * rust + 0.1 * pits, 0, 1),
        "ao": np.clip(0.75 + 0.25 * (h - h.min()) / (np.ptp(h) + 1e-6), 0, 1),
        "height": h,
    }


# The sets the game loads. ``carpincho`` and ``fleece`` paint no set of
# their own any more: ``tools/recado.py`` bakes the carpincho into the
# recado's atlas, and its wool replaced the fleece shells.
SETS = {
    "tack_blankets": blankets,
    "tack_leather": leather,
    "tack_rawhide": rawhide,
    "tack_iron": iron,
}

SIZES = {
    "tack_blankets": 1024,
    "tack_leather": 512,
    "tack_rawhide": 512,
    "tack_iron": 256,
}

# ---------------------------------------------------------------------------
# Output.


def save(array, path, quality=95):
    """``array`` (0..1, grey or RGB) as a JPEG, through a PPM/PGM and
    ImageMagick."""
    data = np.clip(np.round(np.asarray(array) * 255), 0, 255).astype(np.uint8)
    h, w = data.shape[:2]
    grey = data.ndim == 2
    with tempfile.NamedTemporaryFile(suffix=".pgm" if grey else ".ppm", delete=False) as f:
        f.write(f"{'P5' if grey else 'P6'}\n{w} {h}\n255\n".encode())
        f.write(data.tobytes())
        tmp = f.name
    try:
        subprocess.run(["magick", tmp, "-quality", str(quality), path], check=True)
    finally:
        os.unlink(tmp)


def main(argv):
    if len(argv) != 1:
        print("usage: python3 -I tools/tack.py OUT_DIR", file=sys.stderr)
        return 1
    for name, paint in SETS.items():
        maps = paint(SIZES[name])
        folder = os.path.join(argv[0], name)
        os.makedirs(folder, exist_ok=True)
        for key in ("color", "normal_gl", "roughness", "ao", "opacity"):
            if key in maps:
                save(maps[key], os.path.join(folder, f"{key}.jpg"))
        print(f"{folder}: {', '.join(k for k in maps if k != 'height')}")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
