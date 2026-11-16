#!/usr/bin/env python3
"""Bake the creole coats onto the body of henrysteve973's horse.

The game's horse (``tools/henry_horse.py``) comes with its own painted
pinto. The game needs five coats (``Coat`` in
``crates/render/src/character/horse.rs``: zaino, tordillo, overo, alazán,
bayo), so this tool bakes one colour map per coat into the body's set,
``horse_body/<coat>.jpg``, which the game draws ``Horse`` with
(``Paint::Map``):

- ``geometry`` rasterises the standing horse (the first frame of
  ``Idle_1``) over the body's UVs: each texel's point and normal in the
  body frame of the game (metres at the game's scale, +X forward, +Y up,
  +Z to the horse's right);
- ``fur_detail`` takes the fur out of henry's colour map without its
  pinto: each texel's linear luminance over the local mean of its own
  kind of hair (white or brown, with a soft mask, so the edges of the
  patches leave no halo); the gums, the tongue and the inside of the
  nostrils (pink) keep henry's own colour;
- ``coat_rgb`` paints a coat at a point (the patches, the dapples, the
  dark points, the dorsal stripe, the darker topline and lighter belly);
- each map is the coat times the fur detail, in linear light.

numpy and ImageMagick 7 (``magick``); run it isolated, after
``henry_horse.py`` has written the body's ``color.jpg``:

    python3 -I tools/henry_coats.py ~/Downloads/sketchfab/henry_horse assets/models/fauna/henry

then turn the maps into KTX2 (``tools/README.md``). The output is
deterministic. See tools/README.md.
"""

import argparse
import os
import subprocess
import sys

import numpy as np

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import henry_horse as hh  # noqa: E402

SIZE = 2048
# The game draws the horse 1.5 m at the withers; the file's is 1.6 m
# (``HORSE_WITHERS`` and ``Species::Horse.withers`` in livestock.rs).
SCALE = 1.5 / 1.6
# Uncovered texels within this many texels of a face take its nearest
# texel's point, so the mips do not bleed the background into the seams.
MARGIN = 8
# The radius (texels at SIZE) of the local mean the fur detail divides by:
# wider than the hair's own pattern, narrower than a patch.
DETAIL_RADIUS = 40
# The half width (texels at SIZE) of the band along the edges of henry's
# patches where the fur detail is blurred.
EDGE_RADIUS = 6
# How far the fur detail may stray from its local mean.
DETAIL_RANGE = (0.25, 1.6)

Rgb = tuple

# The coats, in sRGB 0..255 (from the coats the game had on its own horse):
# the body; the lower legs below POINTS_HEIGHT; the hoof horn.
COATS = {
    # Dark brown all over, no white.
    "zaino": {"body": (78, 50, 34), "points": (44, 32, 26), "hoof": (38, 32, 28)},
    # Grey with lighter dapples, a lighter head.
    "tordillo": {"body": (118, 118, 114), "points": (92, 92, 90), "hoof": (78, 68, 58),
                 "dapple": (178, 176, 170), "head": (150, 150, 146)},
    # Bay with large white patches, white stockings left fore and right hind.
    "overo": {"body": (142, 84, 48), "points": (142, 84, 48), "hoof": (78, 68, 58)},
    # Chestnut (Don Hidalgo's horse).
    "alazan": {"body": (154, 80, 42), "points": (132, 70, 38), "hoof": (38, 32, 28)},
    # Sand yellow with black points and a dark stripe along the back.
    "bayo": {"body": (198, 162, 104), "points": (40, 32, 26), "hoof": (38, 32, 28),
             "stripe": (104, 76, 46)},
}
WHITE = (232, 228, 218)
# The bare skin of the nostrils and the lips, and the pale horn of a hoof
# under a white stocking.
MUZZLE = (48, 42, 38)
PALE_HOOF = (150, 128, 98)
# Below this height (m) the legs show the coat's points; below HOOF_TOP,
# the hoof.
POINTS_HEIGHT = 0.5
HOOF_TOP = 0.11
# How far apart a tordillo's dapples are (m).
DAPPLE_CELL = 0.1
# Ahead of this the head begins (behind the jaw, m along +X).
HEAD_FROM = 1.0
# The muzzle's centre and radius (m): around the nostrils and the lips.
MUZZLE_AT = (1.42, 1.36, 0.0)
MUZZLE_RADIUS = 0.13

# The overo's white: large patches on the belly, both sides of the barrel,
# the right of the croup, both shoulders and both sides of the neck, so
# some white shows from any side (centre and radii, m).
OVERO_BLOBS = (
    ((0.0, 0.8, 0.0), (0.5, 0.2, 0.4)),
    ((-0.15, 1.08, -0.26), (0.36, 0.26, 0.16)),
    ((0.12, 1.02, 0.26), (0.38, 0.32, 0.18)),
    ((-0.56, 1.18, 0.24), (0.22, 0.22, 0.16)),
    ((0.78, 1.5, -0.12), (0.2, 0.24, 0.12)),
    ((0.52, 1.1, 0.22), (0.2, 0.26, 0.14)),
    ((0.9, 1.62, 0.1), (0.18, 0.2, 0.1)),
    ((0.45, 1.0, -0.22), (0.2, 0.26, 0.14)),
)


class CoatError(Exception):
    pass


# --- geometry --------------------------------------------------------------


def standing(source):
    """The body primitive of the standing horse: points and normals in the
    game's body frame, UVs and triangles."""
    src = hh.Source(*hh.load_source(source))
    model = hh.convert(src)
    worlds = model.pose(hh.REST_CLIP, 0.0)
    raw = np.array(model.skinned(worlds, ("Horse",)), np.float64)
    data = next(d for m, d, _ in model.prims if m == "Horse")
    uv = np.array(data["TEXCOORD_0"], np.float64)
    tris = np.array(data["indices"], np.int64).reshape(-1, 3)
    return body_frame(raw), uv, tris


def body_frame(raw):
    """GLB points (x to the horse's left, y up, z forward, file metres) in
    the game's body frame (``body_point`` in livestock.rs)."""
    return np.stack([raw[:, 2], raw[:, 1], -raw[:, 0]], axis=1) * SCALE


def vertex_normals(pos, tris):
    """Area-weighted vertex normals of the mesh `pos`, `tris` (outward
    when the triangles wind counter-clockwise, as glTF's do)."""
    a, b, c = pos[tris[:, 0]], pos[tris[:, 1]], pos[tris[:, 2]]
    face = np.cross(b - a, c - a)
    out = np.zeros_like(pos)
    for k in range(3):
        np.add.at(out, tris[:, k], face)
    length = np.linalg.norm(out, axis=1, keepdims=True)
    return out / np.maximum(length, 1e-12)


def shift(a, dy, dx):
    """`a` moved `dy` rows and `dx` columns (each -1, 0 or 1), zeros (or
    False) moving in at the border: nothing wraps around."""
    out = np.zeros_like(a)
    h, w = a.shape[:2]
    out[max(dy, 0) : h + min(dy, 0), max(dx, 0) : w + min(dx, 0)] = a[
        max(-dy, 0) : h + min(-dy, 0), max(-dx, 0) : w + min(-dx, 0)
    ]
    return out


def geometry(pos, nrm, uv, tris, size=SIZE):
    """Each texel's point and normal over the UVs (rows top down, as the
    image), and which texels a face covers; texels within `MARGIN` of a
    face take the nearest covered texel's."""
    P = np.zeros((size, size, 3))
    N = np.zeros((size, size, 3))
    covered = np.zeros((size, size), bool)
    px = uv * size
    for t in tris:
        a, b, c = px[t]
        lo = np.clip(np.floor(np.minimum(np.minimum(a, b), c)).astype(int), 0, size - 1)
        hi = np.clip(np.ceil(np.maximum(np.maximum(a, b), c)).astype(int), 0, size)
        if hi[0] <= lo[0] or hi[1] <= lo[1]:
            continue
        d = (b[1] - c[1]) * (a[0] - c[0]) + (c[0] - b[0]) * (a[1] - c[1])
        if abs(d) < 1e-12:
            continue
        xs, ys = np.meshgrid(np.arange(lo[0], hi[0]) + 0.5, np.arange(lo[1], hi[1]) + 0.5)
        w0 = ((b[1] - c[1]) * (xs - c[0]) + (c[0] - b[0]) * (ys - c[1])) / d
        w1 = ((c[1] - a[1]) * (xs - c[0]) + (a[0] - c[0]) * (ys - c[1])) / d
        w2 = 1 - w0 - w1
        inside = (w0 >= -1e-9) & (w1 >= -1e-9) & (w2 >= -1e-9)
        if not inside.any():
            continue
        w = np.stack([w0[inside], w1[inside], w2[inside]], axis=1)
        rows, cols = np.nonzero(inside)
        rows, cols = rows + lo[1], cols + lo[0]
        P[rows, cols] = w @ pos[t]
        N[rows, cols] = w @ nrm[t]
        covered[rows, cols] = True
    if not covered.any():
        raise CoatError("the body covers no texel")
    filled = covered.copy()
    for _ in range(MARGIN):
        grow = np.zeros_like(filled)
        for dy, dx in ((-1, 0), (1, 0), (0, -1), (0, 1)):
            new = shift(filled, dy, dx) & ~filled & ~grow
            P[new] = shift(P, dy, dx)[new]
            N[new] = shift(N, dy, dx)[new]
            grow |= new
        filled |= grow
    N /= np.maximum(np.linalg.norm(N, axis=-1, keepdims=True), 1e-12)
    return P, N, covered


# --- fur detail --------------------------------------------------------------


def to_linear(c):
    c = np.asarray(c, np.float64)
    return np.where(c <= 0.04045, c / 12.92, ((c + 0.055) / 1.055) ** 2.4)


def to_srgb(c):
    c = np.clip(c, 0.0, 1.0)
    return np.where(c <= 0.0031308, c * 12.92, 1.055 * c ** (1 / 2.4) - 0.055)


def smoothstep(e0, e1, x):
    t = np.clip((x - e0) / (e1 - e0), 0.0, 1.0)
    return t * t * (3 - 2 * t)


def box_blur(a, r):
    """`a` (h, w) averaged over a (2r+1)² box, clamped at the borders."""
    for axis in (0, 1):
        pad = [(0, 0), (0, 0)]
        pad[axis] = (r + 1, r)
        s = np.cumsum(np.pad(a, pad, mode="edge"), axis=axis)
        n = a.shape[axis]
        hi = np.take(s, np.arange(2 * r + 1, 2 * r + 1 + n), axis=axis)
        lo = np.take(s, np.arange(0, n), axis=axis)
        a = (hi - lo) / (2 * r + 1)
    return a


def hair_masks(srgb):
    """Soft masks of henry's white hair and of his pink skin (the gums, the
    tongue and the inside of the nostrils), from its sRGB colour."""
    r, g, b = srgb[..., 0], srgb[..., 1], srgb[..., 2]
    chroma = (r - b) / (srgb.sum(-1) + 1e-6)
    value = srgb.max(-1)
    white = smoothstep(0.09, 0.04, chroma) * smoothstep(0.34, 0.46, value)
    pink = smoothstep(0.45, 0.6, r) * smoothstep(0.12, 0.22, r - g) * smoothstep(-0.06, 0.0, b - g)
    return white * (1 - pink), pink


def fur_detail(srgb, radius=DETAIL_RADIUS, edge_radius=EDGE_RADIUS):
    """The fur of henry's colour map without its pinto: each texel's linear
    luminance over the local mean of its own kind of hair; and the pink
    mask (``hair_masks``)."""
    lum = to_linear(srgb) @ np.array([0.2126, 0.7152, 0.0722])
    white, pink = hair_masks(srgb)
    brown = (1 - white) * (1 - pink)
    eps = 1e-4
    mean_w = box_blur(lum * white, radius) / (box_blur(white, radius) + eps)
    mean_b = box_blur(lum * brown, radius) / (box_blur(brown, radius) + eps)
    # Where one kind has no texels about, the other's mean stands in.
    has_w = box_blur(white, radius) > 0.02
    has_b = box_blur(brown, radius) > 0.02
    mean_w = np.where(has_w, mean_w, mean_b)
    mean_b = np.where(has_b, mean_b, mean_w)
    expected = white * mean_w + (1 - white) * mean_b
    detail = np.clip(lum / np.maximum(expected, eps), *DETAIL_RANGE)
    # Along the edges of henry's patches (a rim of darker hair, and texels
    # of both kinds) the detail would trace the old pattern: there it is
    # blurred.
    near = box_blur(white, edge_radius)
    edge = smoothstep(0.0, 0.5, 4 * near * (1 - near))
    soft = box_blur(detail, edge_radius)
    detail = detail + edge * (soft - detail)
    return detail, pink


# --- the coats ---------------------------------------------------------------


def _hash3(i):
    """A value in 0..1 per integer lattice point (n, 3)."""
    h = (i[..., 0] * 73856093) ^ (i[..., 1] * 19349663) ^ (i[..., 2] * 83492791)
    h = (h ^ (h >> 13)) * 1274126177
    return ((h ^ (h >> 16)) & 0xFFFF) / 65535.0


def value_noise(p, freq):
    """Smooth value noise in -1..1 at points `p` (n, 3), `freq` per metre."""
    q = p * freq
    i = np.floor(q).astype(np.int64)
    f = q - i
    u = f * f * (3 - 2 * f)
    out = 0.0
    for dx in (0, 1):
        for dy in (0, 1):
            for dz in (0, 1):
                w = (u[:, 0] if dx else 1 - u[:, 0]) * (u[:, 1] if dy else 1 - u[:, 1]) * (u[:, 2] if dz else 1 - u[:, 2])
                out = out + w * _hash3(i + np.array([dx, dy, dz]))
    return out * 2 - 1


def depth(p, n):
    """The depth of the coat (a factor on its colour): darker along the
    topline and the crest of the neck, lighter on the belly, the girth and
    the flanks, with soft mottling the size of a part of the body."""
    x, y, z = p[:, 0], p[:, 1], p[:, 2]
    topline = smoothstep(0.35, 0.8, n[:, 1]) * smoothstep(1.15, 1.35, y)
    crest = smoothstep(0.45, 0.65, x) * smoothstep(1.35, 1.5, y) * smoothstep(-0.1, 0.4, n[:, 1])
    under = smoothstep(0.15, 0.65, -n[:, 1]) * smoothstep(1.3, 1.1, y)
    flank = smoothstep(0.6, 0.9, np.abs(n[:, 2])) * smoothstep(1.15, 1.0, y)
    mottle = (np.sin(x * 3.1 + 1.3) * np.sin(y * 2.7 + 0.4) * np.sin(z * 3.4 + 2.2)
              + 0.5 * np.sin(x * 7.3 - z * 5.9 + 0.7) * np.cos(y * 6.1 + 1.9))
    return 1.0 - 0.16 * topline - 0.12 * crest + 0.12 * under + 0.06 * flank + 0.06 * mottle


def overo_white(p):
    """How white the overo is at `p` (0..1): its blobs, with wavy edges."""
    wobble = 0.35 * value_noise(p, 9.0) + 0.15 * value_noise(p + 3.7, 23.0)
    inside = np.full(len(p), -np.inf)
    for c, r in OVERO_BLOBS:
        d = (p - np.array(c)) / np.array(r)
        inside = np.maximum(inside, 1.0 - (d * d).sum(1))
    return smoothstep(-0.04, 0.04, inside + wobble * 0.5)


def cell_distances(p, cell):
    """The distances (m) from each point `p` to the nearest and the second
    nearest of the seeds, one jittered seed per `cell`-sized cube."""
    q = p / cell
    base = np.floor(q).astype(np.int64)
    f1 = np.full(len(p), np.inf)
    f2 = np.full(len(p), np.inf)
    for dx in (-1, 0, 1):
        for dy in (-1, 0, 1):
            for dz in (-1, 0, 1):
                k = base + np.array([dx, dy, dz])
                seed = k + 0.15 + 0.7 * np.stack([_hash3(k), _hash3(k + 101), _hash3(k + 211)], axis=1)
                d = np.linalg.norm(q - seed, axis=1)
                f2 = np.where(d < f1, f1, np.minimum(f2, d))
                f1 = np.minimum(f1, d)
    return f1 * cell, f2 * cell


def dapples(p):
    """How much of a tordillo's lighter dapple shows at `p` (0..1): soft
    light rounds about `DAPPLE_CELL` apart inside a darker net, plainest on
    the barrel and the quarters, fading on the neck, the head, the belly
    and the legs."""
    f1, f2 = cell_distances(p + 0.01 * value_noise(p, 40.0)[:, None], DAPPLE_CELL)
    size = DAPPLE_CELL * (0.62 + 0.1 * value_noise(p, 11.0))
    light = smoothstep(size, size * 0.4, f1) * smoothstep(0.006, 0.03, f2 - f1)
    x, y = p[:, 0], p[:, 1]
    where = smoothstep(0.75, 0.95, y) * smoothstep(1.05, 0.8, x)
    return light * where


def coat_rgb(coat, p, n):
    """The sRGB colour (0..1, (n, 3)) of `coat` at the body points `p` with
    normals `n`."""
    c = COATS[coat]
    x, y, z = p[:, 0], p[:, 1], p[:, 2]
    col = lambda rgb: np.array(rgb, np.float64) / 255.0  # noqa: E731
    out = np.tile(col(c["body"]), (len(p), 1))
    if coat == "tordillo":
        out += dapples(p)[:, None] * (col(c["dapple"]) - out)
        head = smoothstep(HEAD_FROM - 0.05, HEAD_FROM + 0.1, x)
        out += head[:, None] * (col(c["head"]) - out)
    if coat == "bayo":
        stripe = (smoothstep(0.5, 0.7, n[:, 1]) * smoothstep(0.06, 0.035, np.abs(z))
                  * smoothstep(-0.85, -0.75, x) * smoothstep(0.5, 0.4, x))
        out += stripe[:, None] * (col(c["stripe"]) - out)
    out *= depth(p, n)[:, None]
    # The lower legs, the points fading up the leg with a ragged edge.
    top = POINTS_HEIGHT + 0.06 * value_noise(p, 14.0)
    points = smoothstep(top + 0.07, top - 0.07, y)
    out += points[:, None] * (col(c["points"]) - out)
    # The bare, dark skin of the muzzle.
    d = np.linalg.norm(p - np.array(MUZZLE_AT), axis=1) / MUZZLE_RADIUS
    muzzle = 0.85 * smoothstep(1.0, 0.55, d)
    out += muzzle[:, None] * (col(MUZZLE) - out)
    hoof_colour = np.tile(col(c["hoof"]), (len(p), 1))
    if coat == "overo":
        # White has no pigment to shade; stockings on the left fore and the
        # right hind, over pale hooves.
        white = np.maximum(overo_white(p) * (1 - muzzle), 0.0)
        stocking = ((x > 0) & (z < 0)) | ((x < 0) & (z > 0))
        sock = stocking * smoothstep(POINTS_HEIGHT + 0.04, POINTS_HEIGHT - 0.04, y)
        white = np.maximum(white, sock)
        out += white[:, None] * (col(WHITE) - out)
        hoof_colour = np.where(stocking[:, None], col(PALE_HOOF), hoof_colour)
    hoof = smoothstep(HOOF_TOP + 0.015, HOOF_TOP - 0.015, y)
    out += hoof[:, None] * (hoof_colour - out)
    return np.clip(out, 0.0, 1.0)


def coat_map(coat, P, N, detail, pink, henry):
    """The colour map of `coat` (sRGB 0..1, (h, w, 3)): the coat at each
    texel times the fur detail in linear light; henry's own pink skin."""
    h, w = detail.shape
    rgb = coat_rgb(coat, P.reshape(-1, 3), N.reshape(-1, 3)).reshape(h, w, 3)
    lin = to_linear(rgb) * detail[..., None]
    out = to_srgb(lin)
    return out + pink[..., None] * (henry - out)


# --- images ------------------------------------------------------------------


def read_image(path, size):
    try:
        raw = subprocess.run(
            ["magick", path, "-resize", f"{size}x{size}!", "-colorspace", "sRGB", "-depth", "8", "rgb:-"],
            capture_output=True, check=True, stdin=subprocess.DEVNULL,
        ).stdout
    except (OSError, subprocess.CalledProcessError) as e:
        raise CoatError(f"magick could not read {path}: {e}") from None
    if len(raw) != size * size * 3:
        raise CoatError(f"{path}: unexpected size")
    return np.frombuffer(raw, np.uint8).reshape(size, size, 3).astype(np.float64) / 255.0


def write_image(path, a):
    a = (np.clip(a, 0, 1) * 255 + 0.5).astype(np.uint8)
    h, w = a.shape[:2]
    try:
        subprocess.run(
            ["magick", "-size", f"{w}x{h}", "-depth", "8", "rgb:-", "-strip", "-quality", "92",
             "-sampling-factor", "1x1", "jpg:" + path],
            input=a.tobytes(), check=True,
        )
    except (OSError, subprocess.CalledProcessError) as e:
        raise CoatError(f"magick could not write {path}: {e}") from None


def main(argv=None):
    parser = argparse.ArgumentParser(description="Bake the creole coats onto henry's horse body.")
    parser.add_argument("source", help="the extracted Sketchfab download (scene.gltf, scene.bin)")
    parser.add_argument("out", help="the horse's asset folder (horse_body/color.jpg is read there)")
    parser.add_argument("--size", type=int, default=SIZE, help="the maps' size in texels")
    parser.add_argument("--coat", action="append", choices=sorted(COATS), help="bake only these")
    args = parser.parse_args(argv)
    try:
        pos, uv, tris = standing(args.source)
        nrm = vertex_normals(pos, tris)
        P, N, _ = geometry(pos, nrm, uv, tris, args.size)
        folder = os.path.join(args.out, "horse_body")
        henry = read_image(os.path.join(folder, "color.jpg"), args.size)
        detail, pink = fur_detail(
            henry, max(1, DETAIL_RADIUS * args.size // SIZE), max(1, EDGE_RADIUS * args.size // SIZE)
        )
        for coat in args.coat or COATS:
            path = os.path.join(folder, f"{coat}.jpg")
            write_image(path, coat_map(coat, P, N, detail, pink, henry))
            print(f"henry_coats: {path}")
    except (OSError, hh.ConvertError, CoatError) as e:
        print(f"henry_coats: {e}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
