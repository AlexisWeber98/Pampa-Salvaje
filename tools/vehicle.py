"""The shared kit of the horse- and ox-drawn vehicles of 1919 La Pampa (the
sulky, the carreta, the carretón) and of the intendente's Ford T, by
script: plain-Python geometry, numpy-painted texture sets, and the Blender
steps that turn them into a GLB and render their previews.

    python3 -I tools/vehicle.py assets/textures/vehicles
    FORCE=1 sh tools/textures_to_ktx2.sh assets/textures/vehicles/*/

paints the texture sets (``SETS``); each vehicle script (``tools/sulky.py``,
``tools/carreta.py``, ``tools/carreton.py``, ``tools/ford_t.py``) and
``tools/vehicle_harness.py`` (the horse's harness and the oxen's yoke)
builds its meshes with ``Mesh`` and calls ``build`` under Blender.

Geometry (``Mesh``) is plain Python, so the tests run without Blender:
``sweep`` drags a 2D profile along a polyline (beams, shafts, straps,
hoops), ``lathe`` turns a profile round the axle (hubs, felloes, tyres),
``surface`` lays a grid (the carreta's hide roof), and ``wheel`` puts a
period wheel together: a turned hub with its iron bands, staggered and
dished oval spokes, a felloe and, optionally, an iron (or rubber) tyre.
Every face carries its material name and UVs in metres over the set's
tile, the wood's grain along U (along the beam, round the felloe), and
every vertex a tint (``Mesh.tint``): road dirt that rises from the ground
on a body and from the rim on a wheel. The GLB keeps that tint as vertex
colours, which the game multiplies into the material.

Frames: Blender metres, Z up, the vehicle facing -Y (glTF +Z, like the
animals of ``tools/horse.py`` and ``tools/cattle.py``), +X its left. The
origin is the ground under its draught animals (between the two of a team),
so the game moves a vehicle by its animals' position; the Ford T's is the
ground midway between its axles. Each GLB holds the nodes:

- ``Body`` (an empty) with the meshes ``Body_Mesh`` and ``Body_LOD``;
- one empty per wheel, ``Wheel.L``/``Wheel.R`` (two wheels) or
  ``Wheel.FL``/``Wheel.FR``/``Wheel.RL``/``Wheel.RR`` (four), at its hub,
  with ``<name>_Mesh`` and ``<name>_LOD``: the game spins the empty about
  its local X (glTF X) by the distance rolled over its radius (its
  ``extras.radius``);
- markers (empties): ``Seat`` (the driver's seat, top of the cushion),
  ``Animal.0``/``Animal.1`` (where each draught animal stands, the
  ground under its body), and the Ford's ``Steering``.

The materials are named after their texture sets (``MATERIALS``): the GLB
has no images; the game loads ``assets/textures/vehicles/<set>/`` (KTX2).

Quality: no third-party asset; every board has its chamfer, every wheel its
real proportions (sources in each script's docstring), and the maps their
grain, checks, chipped paint, rust and road dirt. See tools/README.md.
"""

import math
import os
import subprocess
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)

try:
    import numpy as np
except ImportError:  # pragma: no cover - numpy missing: geometry only
    np = None

try:
    import bpy
except ImportError:  # outside Blender: geometry and maps only (tests)
    bpy = None

# ---------------------------------------------------------------------------
# Small vector maths on tuples (no numpy, no mathutils: the tests run on
# plain Python).


def add(a, b):
    return (a[0] + b[0], a[1] + b[1], a[2] + b[2])


def sub(a, b):
    return (a[0] - b[0], a[1] - b[1], a[2] - b[2])


def mul(a, k):
    return (a[0] * k, a[1] * k, a[2] * k)


def dot(a, b):
    return a[0] * b[0] + a[1] * b[1] + a[2] * b[2]


def cross(a, b):
    return (a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0])


def length(a):
    return math.sqrt(dot(a, a))


def unit(a):
    n = length(a)
    return (0.0, 0.0, 0.0) if n < 1e-12 else mul(a, 1.0 / n)


def lerp(a, b, t):
    return add(a, mul(sub(b, a), t))


def bezier(p0, p1, p2, p3, n):
    """``n + 1`` points along a cubic Bézier."""
    out = []
    for k in range(n + 1):
        t = k / n
        s = 1 - t
        out.append(
            tuple(
                s * s * s * p0[i] + 3 * s * s * t * p1[i] + 3 * s * t * t * p2[i] + t * t * t * p3[i]
                for i in range(3)
            )
        )
    return out


def srgb_to_linear(c):
    """One sRGB channel (0..1) to linear."""
    return c / 12.92 if c <= 0.04045 else ((c + 0.055) / 1.055) ** 2.4


def rot_x(p, a):
    c, s = math.cos(a), math.sin(a)
    return (p[0], p[1] * c - p[2] * s, p[1] * s + p[2] * c)


def rot_y(p, a):
    c, s = math.cos(a), math.sin(a)
    return (p[0] * c + p[2] * s, p[1], -p[0] * s + p[2] * c)


def rot_z(p, a):
    c, s = math.cos(a), math.sin(a)
    return (p[0] * c - p[1] * s, p[0] * s + p[1] * c, p[2])


# ---------------------------------------------------------------------------
# Profiles (2D, counter-clockwise, in the (side, up) plane of a sweep).


def rect(w, h, chamfer=0.0):
    """A ``w`` × ``h`` rectangle centred on the path, its corners cut by
    ``chamfer`` (eight points, so every board edge catches the light)."""
    a, b = w / 2, h / 2
    if chamfer <= 0:
        return [(-a, -b), (a, -b), (a, b), (-a, b)]
    c = min(chamfer, a * 0.45, b * 0.45)
    return [(-a + c, -b), (a - c, -b), (a, -b + c), (a, b - c), (a - c, b), (-a + c, b), (-a, b - c), (-a, -b + c)]


def ellipse(w, h, n=8):
    return [(w / 2 * math.cos(2 * math.pi * k / n), h / 2 * math.sin(2 * math.pi * k / n)) for k in range(n)]


def circle(r, n=8):
    return ellipse(2 * r, 2 * r, n)


# ---------------------------------------------------------------------------
# Meshes.


class Mesh:
    """Vertices, and faces that each carry a material and their corner UVs.

    ``tints[i]`` is vertex ``i``'s colour multiplier (white by default),
    painted by ``tint``.
    """

    def __init__(self):
        self.verts = []
        self.faces = []  # (vertex indices, material, [(u, v)] per corner)
        self.tints = []

    def vertex(self, p):
        self.verts.append(tuple(float(c) for c in p))
        self.tints.append((1.0, 1.0, 1.0))
        return len(self.verts) - 1

    def face(self, idx, material, uvs):
        assert len(idx) == len(uvs) >= 3
        self.faces.append((tuple(idx), material, [tuple(uv) for uv in uvs]))

    def extend(self, other):
        base = len(self.verts)
        self.verts.extend(other.verts)
        self.tints.extend(other.tints)
        for idx, mat, uvs in other.faces:
            self.faces.append((tuple(i + base for i in idx), mat, uvs))
        return self

    def moved(self, fn):
        """A copy with every vertex mapped by ``fn`` (a rigid move: a mirror
        flips the faces back so they still face out)."""
        out = Mesh()
        out.verts = [tuple(fn(v)) for v in self.verts]
        out.tints = list(self.tints)
        out.faces = list(self.faces)
        return out

    def translated(self, d):
        return self.moved(lambda v: add(v, d))

    def mirrored_x(self):
        """Its mirror image across X = 0 (faces reversed to keep facing out)."""
        out = self.moved(lambda v: (-v[0], v[1], v[2]))
        out.faces = [(idx[::-1], mat, uvs[::-1]) for idx, mat, uvs in self.faces]
        return out

    def tint(self, fn):
        """Multiplies each vertex's tint by ``fn(position)``, an RGB triple."""
        self.tints = [tuple(t * c for t, c in zip(tint, fn(v))) for tint, v in zip(self.tints, self.verts)]
        return self

    def bounds(self):
        xs, ys, zs = zip(*self.verts)
        return (min(xs), min(ys), min(zs)), (max(xs), max(ys), max(zs))

    def triangles(self):
        return sum(len(f[0]) - 2 for f in self.faces)

    def materials(self):
        return sorted({f[1] for f in self.faces})


def merge(*meshes):
    out = Mesh()
    for m in meshes:
        out.extend(m)
    return out


def frames(path, up):
    """A (tangent, side, normal) frame at each point of ``path``: ``side`` is
    ``up`` × tangent made unit, ``normal`` tangent × side; where the path
    runs along ``up`` the previous side carries on."""
    out = []
    side_prev = None
    n = len(path)
    for i in range(n):
        if i == 0:
            t = sub(path[1], path[0])
        elif i == n - 1:
            t = sub(path[-1], path[-2])
        else:
            t = add(unit(sub(path[i], path[i - 1])), unit(sub(path[i + 1], path[i])))
        t = unit(t)
        side = cross(up, t)
        if length(side) < 1e-6:
            side = side_prev or (1.0, 0.0, 0.0)
        side = unit(side)
        normal = unit(cross(t, side))
        out.append((t, side, normal))
        side_prev = side
    return out


def sweep(path, profile, material, tile=(1.0, 1.0), up=(0.0, 0.0, 1.0), scales=None, caps=True, closed=False):
    """``profile`` dragged along ``path``: U runs along the path (the grain),
    V round the profile, both in metres over ``tile``. ``scales`` tapers the
    profile at each point; a ``closed`` path joins its ends (a hoop)."""
    m = Mesh()
    pts = list(path)
    if closed:
        fr = frames(pts + [pts[0], pts[1]], up)[: len(pts)]
        # The first point's tangent, from the last and the second.
        t0 = unit(add(unit(sub(pts[0], pts[-1])), unit(sub(pts[1], pts[0]))))
        s0 = unit(cross(up, t0))
        fr[0] = (t0, s0, unit(cross(t0, s0)))
    else:
        fr = frames(pts, up)
    scales = scales or [1.0] * len(pts)
    along = [0.0]
    for a, b in zip(pts, pts[1:] + ([pts[0]] if closed else [])):
        along.append(along[-1] + length(sub(b, a)))
    perim = [0.0]
    ring = list(profile) + [profile[0]]
    for a, b in zip(ring, ring[1:]):
        perim.append(perim[-1] + math.hypot(b[0] - a[0], b[1] - a[1]))
    np_ = len(profile)
    rows = []
    for (p, (t, s, n), k) in zip(pts, fr, scales):
        rows.append([m.vertex(add(p, add(mul(s, a * k), mul(n, b * k)))) for a, b in profile])
    count = len(pts) if closed else len(pts) - 1
    for i in range(count):
        r0, r1 = rows[i], rows[(i + 1) % len(pts)]
        u0, u1 = along[i] / tile[0], along[i + 1] / tile[0]
        for j in range(np_):
            j1 = (j + 1) % np_
            v0, v1 = perim[j] / tile[1], perim[j + 1] / tile[1]
            m.face([r0[j], r0[j1], r1[j1], r1[j]], material, [(u0, v0), (u0, v1), (u1, v1), (u1, v0)])
    if caps and not closed:
        sx = [a for a, _ in profile]
        sy = [b for _, b in profile]
        cx, cy = (min(sx) + max(sx)) / 2, (min(sy) + max(sy)) / 2
        uvs = [((a - cx) / tile[1] + 0.5, (b - cy) / tile[1] + 0.5) for a, b in profile]
        m.face(rows[0][::-1], material, uvs[::-1])
        m.face(rows[-1], material, uvs)
    return m


def beam(a, b, w, h, material, chamfer=0.008, up=(0.0, 0.0, 1.0), tile=(1.0, 0.5)):
    """A sawn piece from ``a`` to ``b``: ``w`` wide (sideways) by ``h`` deep
    (towards ``up``), with chamfered edges."""
    if length(cross(unit(sub(b, a)), up)) < 1e-3:
        up = (0.0, 1.0, 0.0)
    return sweep([a, b], rect(w, h, chamfer), material, tile, up)


def rod(path, r, material, n=8, tile=(0.5, 0.25), up=(0.0, 0.0, 1.0), caps=True, closed=False):
    return sweep(path, circle(r, n), material, tile, up, caps=caps, closed=closed)


def lathe(profile, n, material, tile=(1.0, 1.0), closed=False, axis="x", phase=0.0):
    """``profile`` [(radius, along the axis)] turned round the axis (X by
    default: the axle) in ``n`` steps; U runs round (the felloe's grain), V
    along the profile. A ``closed`` profile is a loop (a felloe's section)."""
    m = Mesh()
    pts = list(profile) + ([profile[0]] if closed else [])
    seg = [0.0]
    for a, b in zip(pts, pts[1:]):
        seg.append(seg[-1] + math.hypot(b[0] - a[0], b[1] - a[1]))
    rmax = max(r for r, _ in profile)
    rows = []
    for k in range(n):
        a = phase + 2 * math.pi * k / n
        c, s = math.cos(a), math.sin(a)
        row = []
        for r, x in profile:
            if axis == "x":
                row.append(m.vertex((x, r * c, r * s)))
            else:  # "z"
                row.append(m.vertex((r * c, r * s, x)))
        rows.append(row)
    circ = 2 * math.pi * rmax
    for k in range(n):
        r0, r1 = rows[k], rows[(k + 1) % n]
        u0, u1 = circ * k / n / tile[0], circ * (k + 1) / n / tile[0]
        for j in range(len(pts) - 1):
            j1 = (j + 1) % len(profile)
            v0, v1 = seg[j] / tile[1], seg[j + 1] / tile[1]
            m.face([r0[j], r1[j], r1[j1], r0[j1]], material, [(u0, v0), (u1, v0), (u1, v1), (u0, v1)])
    return m


def surface(fn, nu, nv, material, tile=(1.0, 1.0), closed_u=False):
    """A grid of ``fn(u, v)`` for u, v in 0..1: UVs in metres along each
    direction over ``tile`` (measured along the grid's middle lines)."""
    m = Mesh()
    cols = nu if closed_u else nu + 1
    grid_ = [[fn(i / nu, j / nv) for j in range(nv + 1)] for i in range(cols)]
    mid_v = nv // 2
    mid_u = cols // 2
    us = [0.0]
    for i in range(1, cols + (1 if closed_u else 0)):
        us.append(us[-1] + length(sub(grid_[i % cols][mid_v], grid_[i - 1][mid_v])))
    vs = [0.0]
    for j in range(1, nv + 1):
        vs.append(vs[-1] + length(sub(grid_[mid_u][j], grid_[mid_u][j - 1])))
    idx = [[m.vertex(p) for p in col] for col in grid_]
    for i in range(nu):
        i1 = (i + 1) % cols
        for j in range(nv):
            m.face(
                [idx[i][j], idx[i1][j], idx[i1][j + 1], idx[i][j + 1]],
                material,
                [
                    (us[i] / tile[0], vs[j] / tile[1]),
                    (us[i + 1] / tile[0], vs[j] / tile[1]),
                    (us[i + 1] / tile[0], vs[j + 1] / tile[1]),
                    (us[i] / tile[0], vs[j + 1] / tile[1]),
                ],
            )
    return m


def thick(m, depth):
    """``m`` (a one-sided surface) given its back and its edges: a sheet
    ``depth`` thick, pushed back along each vertex's averaged normal."""
    normals = [(0.0, 0.0, 0.0)] * len(m.verts)
    edges = {}
    for idx, _, _ in m.faces:
        p = [m.verts[i] for i in idx]
        n = unit(cross(sub(p[1], p[0]), sub(p[-1], p[0])))
        for i in idx:
            normals[i] = add(normals[i], n)
        for a, b in zip(idx, idx[1:] + idx[:1]):
            edges[(a, b)] = edges.get((a, b), 0) + 1
    out = Mesh()
    out.extend(m)
    back = [out.vertex(sub(v, mul(unit(n), depth))) for v, n in zip(m.verts, normals)]
    for idx, mat, uvs in m.faces:
        out.face([back[i] for i in idx[::-1]], mat, uvs[::-1])
    mat0 = m.faces[0][1]
    for (a, b) in edges:
        if (b, a) not in edges:
            out.face([b, a, back[a], back[b]], mat0, [(0, 0), (0.05, 0), (0.05, depth), (0, depth)])
    return out


# ---------------------------------------------------------------------------
# Wheels.


def wheel(
    radius,
    spokes,
    out=1.0,
    hub=(0.09, 0.24),
    felloe=(0.06, 0.055),
    tyre=(0.012, 0.05),
    spoke=(0.05, 0.03),
    dish=0.03,
    stagger=0.012,
    wood="Paint_Red",
    iron="Iron",
    tyre_material=None,
    hub_material=None,
    segments=None,
):
    """A period wheel at the origin, its axle along X, the outer face
    towards ``out`` (+1: +X, the vehicle's left).

    ``radius`` is to the tread (outside the tyre); ``hub`` its nave's
    (radius, length); ``felloe`` the rim's (depth, width); ``tyre`` the iron
    band's (thickness, width) or ``None`` for a bare wooden felloe;
    ``spoke`` each spoke's (width in the wheel's plane at the hub, thickness
    along the axle); ``dish`` how far the felloe stands out past the hub's
    middle (the dished wheel of a cart); ``stagger`` how far the spokes
    alternate along the nave. ``tyre_material`` "Rubber" makes a Ford tyre
    (a round section)."""
    m = Mesh()
    hub_r, hub_l = hub
    fd, fw = felloe
    n = segments or max(48, spokes * 6)
    rim_out = radius - (tyre[0] if tyre else 0.0)
    if tyre_material == "Rubber":
        rim_out = radius - tyre[0] * 2
    rim_in = rim_out - fd
    hub_mat = hub_material or wood
    # The felloe: a chamfered rectangle turned round, offset by the dish.
    c = min(fd, fw) * 0.18
    sect = [
        (rim_in, -fw / 2 + c),
        (rim_in + c, -fw / 2),
        (rim_out - c, -fw / 2),
        (rim_out, -fw / 2 + c),
        (rim_out, fw / 2 - c),
        (rim_out - c, fw / 2),
        (rim_in + c, fw / 2),
        (rim_in, fw / 2 - c),
    ]
    felloe_m = lathe([(r, x * out + dish * out) for r, x in sect], n, wood, tile=(1.0, 0.25), closed=True)
    if out < 0:
        felloe_m.faces = [(i[::-1], mt, uv[::-1]) for i, mt, uv in felloe_m.faces]
    m.extend(felloe_m)
    # The tyre: an iron band shrunk on, a little wider than the felloe; or a
    # Ford's clincher tyre, a round rubber section.
    if tyre and tyre_material == "Rubber":
        t, w = tyre
        sect = [(rim_out - 0.004 + t * (1 - math.cos(a)), dish * out + w / 2 * math.sin(a))
                for a in [math.pi * k / 10 for k in range(11)]]
        sect += [(rim_out - 0.004, dish * out - w / 2 * 0.8)]
        tm = lathe(sect, n, "Rubber", tile=(0.6, 0.12), closed=True)
        if out > 0:
            tm.faces = [(i[::-1], mt, uv[::-1]) for i, mt, uv in tm.faces]
        m.extend(tm)
    elif tyre:
        t, w = tyre
        sect = [(rim_out - 0.002, -w / 2), (radius, -w / 2 + 0.002), (radius, w / 2 - 0.002), (rim_out - 0.002, w / 2)]
        tm = lathe([(r, x + dish * out) for r, x in sect], n, iron, tile=(0.5, 0.25), closed=True)
        if out < 0:
            tm.faces = [(i[::-1], mt, uv[::-1]) for i, mt, uv in tm.faces]
        m.extend(tm)
        # The bolts through tyre and felloe, one each side of every joint.
        joints = max(5, spokes // 2)
        for k in range(joints * 2):
            a = 2 * math.pi * (k // 2 + (0.5 if k % 2 else 0.0) * 0.18 - 0.09) / joints + math.pi / spokes
            head = lathe([(0.0, 0.0), (0.009, 0.0), (0.008, 0.004), (0.0, 0.006)], 6, iron, axis="z")
            head = head.moved(lambda v, a=a: rot_x(add(v, (dish * out, 0.0, radius)), a))
            m.extend(head)
    # The nave: turned, swelling to the spokes' mortises, an iron band at
    # each end, the axle box's cap outside.
    L = hub_l
    prof = [
        (0.035, -L / 2 - 0.005),
        (hub_r * 0.78, -L / 2),
        (hub_r * 0.86, -L / 2 + L * 0.12),
        (hub_r, -L * 0.12),
        (hub_r, L * 0.12),
        (hub_r * 0.84, L / 2 - L * 0.12),
        (hub_r * 0.72, L / 2),
        (hub_r * 0.42, L / 2 + 0.02),
        (0.0, L / 2 + 0.03),
    ]
    nave = lathe([(r, x * out) for r, x in prof], 24, hub_mat, tile=(0.5, 0.25))
    if out < 0:
        nave.faces = [(i[::-1], mt, uv[::-1]) for i, mt, uv in nave.faces]
    m.extend(nave)
    for x in (-L / 2 + L * 0.06, L / 2 - L * 0.08):
        r = hub_r * (0.86 if x < 0 else 0.84) + 0.004
        band = lathe([(r - 0.004, x * out - 0.012), (r, x * out - 0.01), (r, x * out + 0.01), (r - 0.004, x * out + 0.012)],
                     24, iron, tile=(0.3, 0.1), closed=True)
        if out < 0:
            band.faces = [(i[::-1], mt, uv[::-1]) for i, mt, uv in band.faces]
        m.extend(band)
    cap = lathe([(0.0, (L / 2 + 0.035) * out), (hub_r * 0.40, (L / 2 + 0.025) * out), (hub_r * 0.44, (L / 2 + 0.005) * out)],
                12, iron, tile=(0.3, 0.1))
    if out < 0:
        cap.faces = [(i[::-1], mt, uv[::-1]) for i, mt, uv in cap.faces]
    m.extend(cap)
    # The spokes: oval, tapering from the nave to the felloe, alternately
    # in front of and behind the nave's middle, leaning out to the dish.
    sw, st = spoke
    for k in range(spokes):
        a = 2 * math.pi * k / spokes
        x0 = (stagger if k % 2 else -stagger) * out
        p0 = (x0, 0.0, hub_r * 0.92)
        p1 = (dish * out, 0.0, rim_in + 0.01)
        mid = lerp(p0, p1, 0.5)
        s = sweep([p0, mid, p1], ellipse(sw, st, 10), wood, tile=(1.0, 0.2), up=(1.0, 0.0, 0.0),
                  scales=[1.0, 0.86, 0.74], caps=False)
        m.extend(s.moved(lambda v, a=a: rot_x(v, a)))
    return m


def rim_dirt(radius, colour=(0.62, 0.52, 0.40), start=0.55):
    """A wheel's tint: road dirt caked towards the rim."""

    def fn(v):
        r = math.hypot(v[1], v[2]) / radius
        t = max(0.0, min(1.0, (r - start) / (1.0 - start)))
        t = t * t
        return tuple(1.0 - t * (1.0 - c) for c in colour)

    return fn


def ground_dirt(top=0.7, colour=(0.66, 0.57, 0.46)):
    """A body's tint: dust and splashed mud thickest near the ground."""

    def fn(v):
        t = max(0.0, min(1.0, 1.0 - v[2] / top))
        t = t * t
        return tuple(1.0 - t * (1.0 - c) for c in colour)

    return fn


# ---------------------------------------------------------------------------
# A vehicle, as the GLB will hold it.


class Vehicle:
    """``body`` (a Mesh), ``wheels`` {name: (hub position, radius, Mesh at
    its hub)}, ``markers`` {name: position}, and ``extras`` (a dict exported
    on the root)."""

    def __init__(self, name):
        self.name = name
        self.body = Mesh()
        self.wheels = {}
        self.markers = {}
        self.extras = {}

    def add_wheel(self, name, hub, radius, mesh):
        mesh.tint(rim_dirt(radius))
        self.wheels[name] = (tuple(hub), radius, mesh)

    def triangles(self):
        return self.body.triangles() + sum(w[2].triangles() for w in self.wheels.values())

    def materials(self):
        return sorted(set(self.body.materials()).union(*[w[2].materials() for w in self.wheels.values()]))


# The materials of every vehicle, named after their texture sets under
# ``assets/textures/vehicles`` (``SET_OF``); ``Glass`` has no set.
MATERIALS = [
    "Wood",
    "Paint_Black",
    "Paint_Red",
    "Paint_Green",
    "Paint_Yellow",
    "Iron",
    "Hide",
    "Cane",
    "Leather",
    "Brass",
    "Rubber",
    "Glass",
]

SET_OF = {
    "Wood": "veh_wood",
    "Paint_Black": "veh_paint_black",
    "Paint_Red": "veh_paint_red",
    "Paint_Green": "veh_paint_green",
    "Paint_Yellow": "veh_paint_yellow",
    "Iron": "veh_iron",
    "Hide": "veh_hide",
    "Cane": "veh_cane",
    "Leather": "veh_leather",
    "Brass": "veh_brass",
    "Rubber": "veh_rubber",
}

# Preview-only shading per material: (metallic, roughness scale).
SHADING = {"Iron": (0.6, 1.0), "Brass": (1.0, 0.8), "Glass": (0.0, 0.05), "Paint_Black": (0.0, 0.85)}

# ---------------------------------------------------------------------------
# The texture sets (numpy), on the noise of tools/tack.py.

if np is not None:
    import tack as T


def _grain(size, seed, rings=36, wobble=0.06, m=None):
    """Wood grain running along U (across the image): the growth rings of a
    flat-sawn board, 0..1, and the fine fibre."""
    m = size if m is None else m
    x, y = T.grid(size, m)
    warp = T.fbm(size, (2, 5), seed, 5, m) - 0.5
    drift = T.fbm(size, (1, 3), seed + 7, 3, m) - 0.5
    fibre = T.fbm(size, (size // 2, 12), seed + 1, 3, m)
    streak = T.fbm(size, (4, size // 6), seed + 2, 2, m)
    phase = (y + wobble * warp + 0.35 * drift / rings * 8) * rings
    ring = 0.5 + 0.5 * np.cos(2 * np.pi * phase)
    ring = ring**4 * (0.6 + 0.4 * streak)
    return ring, 0.6 * fibre + 0.4 * streak


def _knots(size, seed, count):
    rng = np.random.default_rng(seed)
    x, y = T.grid(size)
    out = np.zeros((size, size))
    for _ in range(count):
        cx, cy = rng.random(2)
        rx, ry = rng.uniform(0.02, 0.05), rng.uniform(0.008, 0.02)
        dx = (x - cx + 0.5) % 1.0 - 0.5
        dy = (y - cy + 0.5) % 1.0 - 0.5
        d = np.sqrt((dx / rx) ** 2 + (dy / ry) ** 2)
        out = np.maximum(out, 1 - T.smooth(0.6, 1.2, d))
    return out


def _checks(size, seed, count):
    """Weather checks: thin dark splits along the grain (U)."""
    rng = np.random.default_rng(seed)
    out = np.zeros((size, size))
    for _ in range(count):
        x0, y0 = rng.random(2) * size
        ln = rng.uniform(0.05, 0.35) * size
        t = np.linspace(0, ln, int(ln) + 2)
        xs = (np.round(x0 + t) % size).astype(int)
        ys = (np.round(y0 + 1.5 * np.sin(t / ln * np.pi * rng.uniform(0.5, 2))) % size).astype(int)
        depth = np.sin(np.linspace(0, np.pi, len(t))) * rng.uniform(0.5, 1.0)
        np.maximum.at(out, (ys, xs), depth)
    soft = (out + np.roll(out, 1, 0) * 0.5 + np.roll(out, -1, 0) * 0.5) / 2.0
    return np.clip(soft, 0, 1)


def wood(size):
    """Weathered hardwood (algarrobo, quebracho): the bare boards of a
    carreta or a cart's running gear, silvered grey-brown by sun and rain,
    the grain raised where the soft earlywood wore away, with checks and
    knots and dust in every hollow."""
    ring, fibre = _grain(size, 101, rings=22, wobble=0.12)
    knots = _knots(size, 102, 5)
    checks = _checks(size, 103, 40)
    mottle = T.fbm(size, 5, 104, 4)
    silver = T.smooth(0.35, 0.75, T.fbm(size, (3, 8), 105, 4))
    base = np.array((0.42, 0.33, 0.24))
    grey = np.array((0.50, 0.48, 0.44))
    colour = T.mix(base, grey, 0.45 + 0.4 * silver)
    colour = colour * (0.78 + 0.22 * (1 - ring) + 0.1 * (mottle - 0.5))[..., None]
    colour = colour * (0.80 + 0.28 * fibre)[..., None]
    colour = T.mix(colour, (0.20, 0.14, 0.09), knots * 0.7)
    colour = T.mix(colour, (0.10, 0.08, 0.06), checks * 0.85)
    height = 0.5 * (1 - ring) + 0.25 * fibre - 1.4 * checks + 0.4 * knots
    return {
        "color": np.clip(colour, 0, 1),
        "normal_gl": T.normals(height, 2.5 * size / 512),
        "roughness": np.clip(0.80 + 0.12 * silver - 0.08 * knots, 0, 1),
        "ao": np.clip(1 - 0.6 * checks - 0.12 * ring, 0, 1),
        "height": height,
    }


def _painted(size, paint, seed, gloss=0.42, chips=0.6):
    """Coach paint (``paint``, sRGB 0..1) over wood: brush marks along the
    grain, the grain telegraphing through, chipped to the grey wood at the
    knocks and worn thin where hands and harness rub, grime in the pores."""
    ring, fibre = _grain(size, seed, rings=24, wobble=0.03)
    bare = wood(size)
    brush = T.fbm(size, (2, size // 8), seed + 2, 2)
    wear = T.smooth(0.50, 0.80, T.fbm(size, 4, seed + 3, 5))
    chip_n = T.fbm(size, 22, seed + 4, 3)
    chip = T.smooth(0.80 - 0.06 * chips, 0.82 - 0.06 * chips, chip_n + 0.12 * (wear - 0.5))
    grime = T.smooth(0.4, 0.8, T.fbm(size, 6, seed + 5, 4))
    colour = np.array(paint) * (0.93 + 0.07 * brush + 0.05 * (T.fbm(size, 3, seed + 6, 3) - 0.5))[..., None]
    colour = T.mix(colour, colour * 0.75 + np.array(paint) * 0.0 + 0.08, wear * 0.35)
    colour = T.mix(colour, bare["color"], chip)
    colour = T.mix(colour, colour * 0.72, grime * 0.35)
    height = 0.12 * ring + 0.1 * brush - 0.8 * chip + 0.6 * chip * bare["height"] * 0.3
    return {
        "color": np.clip(colour, 0, 1),
        "normal_gl": T.normals(height, 2.0 * size / 512),
        "roughness": np.clip(gloss + 0.18 * wear + 0.15 * grime + chip * (bare["roughness"] - gloss), 0, 1),
        "ao": np.clip(1 - 0.35 * chip * (1 - bare["ao"]) - 0.08 * grime, 0, 1),
        "height": height,
    }


def paint_black(size):
    """Black coach paint (the sulky's body and shafts, the Ford's
    enamel), worn to a dull sheen."""
    return _painted(size, (0.035, 0.035, 0.035), 120, gloss=0.30, chips=0.0)


def paint_red(size):
    """Indian red, the running gear and wheels of a sulky or a cart."""
    return _painted(size, (0.42, 0.10, 0.07), 130)


def paint_green(size):
    """The dark green of a carretón's box."""
    return _painted(size, (0.14, 0.24, 0.16), 140)


def paint_yellow(size):
    """The ochre yellow of a sulky's wheels."""
    return _painted(size, (0.66, 0.46, 0.12), 150)


def iron(size):
    return T.iron(size)


def hide(size):
    """Rawhide (cuero crudo) stretched over the carreta's cane hoops: dry,
    translucent amber-tan, creased and stained, with patches where the hair
    was left on (short dark and white hair), and a sewn seam where two
    hides meet (along U, a third of the way across)."""
    x, y = T.grid(size)
    mottle = T.fbm(size, 4, 201, 5)
    stain = T.smooth(0.55, 0.8, T.fbm(size, 3, 202, 4))
    crease_n = T.fbm(size, (3, 12), 203, 4)
    crease = np.abs(np.sin(crease_n * 18.0)) ** 8
    hair_patch = T.smooth(0.74, 0.79, T.fbm(size, 3, 204, 4))
    strands = T.fbm(size, (size // 2, size // 24), 205, 2)
    hair_col = T.mix((0.22, 0.15, 0.10), (0.52, 0.40, 0.28), T.smooth(0.45, 0.6, T.fbm(size, 5, 206, 3)))
    colour = np.array((0.62, 0.47, 0.30)) * (0.80 + 0.35 * mottle)[..., None]
    colour = T.mix(colour, colour * 0.6, stain * 0.7)
    colour = T.mix(colour, colour * 0.75, crease * 0.6)
    colour = T.mix(colour, hair_col * (0.8 + 0.3 * strands)[..., None], hair_patch * 0.9)
    seam_d = np.abs(y - 0.33)
    seam = 1 - T.smooth(0.004, 0.012, seam_d)
    holes = (seam_d < 0.012) & (np.abs((x * 40) % 1.0 - 0.5) < 0.12)
    colour = T.mix(colour, (0.30, 0.20, 0.12), seam * 0.6)
    colour = T.mix(colour, (0.12, 0.08, 0.05), holes.astype(float) * 0.9)
    height = 0.3 * mottle - 0.6 * crease + 0.25 * hair_patch * strands - 0.5 * seam
    return {
        "color": np.clip(colour, 0, 1),
        "normal_gl": T.normals(height, 2.0 * size / 512),
        "roughness": np.clip(0.70 + 0.15 * hair_patch - 0.1 * stain, 0, 1),
        "ao": np.clip(1 - 0.3 * crease - 0.4 * holes, 0, 1),
        "height": height,
    }


def cane(size):
    """Cane (caña tacuara) laid side by side, running along U: round culms
    with their nodes, straw-yellow gone grey, shadow between them."""
    x, y = T.grid(size)
    n_canes = 22
    jitter = T.fbm(size, (2, n_canes), 301, 3) - 0.5
    cy = (y * n_canes + 0.15 * jitter)
    f = cy % 1.0
    idx = np.floor(cy)
    rng = np.random.default_rng(302)
    tone = rng.uniform(0.75, 1.1, n_canes + 2)[idx.astype(int) % n_canes]
    offset = rng.random(n_canes + 2)[idx.astype(int) % n_canes]
    roundness = np.sin(np.pi * f)
    node = np.exp(-(((x * 3 + offset) % 1.0 - 0.5) / 0.008) ** 2) * (0.6 + 0.4 * roundness)
    streak = T.fbm(size, (size // 4, n_canes * 2), 303, 2)
    colour = np.array((0.66, 0.58, 0.40)) * (tone * (0.55 + 0.45 * roundness) * (0.9 + 0.1 * streak))[..., None]
    colour = T.mix(colour, (0.50, 0.48, 0.42), 0.35 * T.fbm(size, 4, 304, 3))
    colour = T.mix(colour, colour * 0.75, node * 0.5)
    height = roundness + 0.15 * node
    return {
        "color": np.clip(colour, 0, 1),
        "normal_gl": T.normals(height, 4.0 * size / 512),
        "roughness": np.clip(0.55 + 0.25 * (1 - roundness), 0, 1),
        "ao": np.clip(0.35 + 0.65 * roundness ** 0.5, 0, 1),
        "height": height,
    }


def leather(size):
    """Black harness leather: straps (V runs across a strap, round its
    section) with a stitched row near each edge of the face, creased and
    rubbed brown where it works, dried by sun."""
    x, y = T.grid(size)
    grain = T.fbm(size, size // 6, 401, 2)
    mottle = T.fbm(size, 5, 402, 4)
    rub = T.smooth(0.55, 0.8, T.fbm(size, (8, 3), 403, 4))
    crease = np.abs(np.sin(T.fbm(size, (14, 3), 404, 3) * 30)) ** 12
    colour = np.array((0.06, 0.05, 0.045)) * (0.85 + 0.3 * mottle)[..., None]
    colour = T.mix(colour, (0.20, 0.12, 0.07), rub * 0.6)
    stitch_rows = np.zeros_like(x)
    for at in (0.06, 0.44):
        stitch_rows = np.maximum(stitch_rows, 1 - T.smooth(0.003, 0.008, np.abs(y - at)))
    stitches = stitch_rows * (np.abs((x * 80) % 1.0 - 0.5) < 0.3)
    colour = T.mix(colour, (0.32, 0.27, 0.20), stitches * 0.7)
    height = 0.2 * grain + 0.15 * mottle - 0.5 * crease - 0.3 * stitch_rows + 0.2 * stitches
    return {
        "color": np.clip(colour, 0, 1),
        "normal_gl": T.normals(height, 2.5 * size / 512),
        "roughness": np.clip(0.52 + 0.2 * rub + 0.1 * grain - 0.1 * crease, 0, 1),
        "ao": np.clip(1 - 0.35 * crease - 0.3 * stitch_rows, 0, 1),
        "height": height,
    }


def brass(size):
    """Brass, polished where hands reach and tarnished brown-green in the
    hollows (the Ford's radiator and lamps, harness hames' knobs)."""
    tarnish = T.smooth(0.45, 0.75, T.fbm(size, 4, 501, 5))
    verdigris = T.smooth(0.72, 0.8, T.fbm(size, 7, 502, 4)) * tarnish
    scratch = T.scratches(size, 120, 503)
    colour = np.array((0.78, 0.60, 0.28)) * (0.9 + 0.15 * T.fbm(size, 12, 504, 3))[..., None]
    colour = T.mix(colour, (0.42, 0.32, 0.17), tarnish * 0.7)
    colour = T.mix(colour, (0.30, 0.42, 0.33), verdigris * 0.6)
    colour = T.mix(colour, (0.9, 0.75, 0.45), scratch * 0.3)
    height = -0.3 * scratch + 0.1 * tarnish
    return {
        "color": np.clip(colour, 0, 1),
        "normal_gl": T.normals(height, 1.5 * size / 512),
        "roughness": np.clip(0.25 + 0.45 * tarnish + 0.2 * verdigris, 0, 1),
        "ao": np.clip(1 - 0.2 * verdigris, 0, 1),
        "height": height,
    }


def rubber(size):
    """A 1919 Ford tyre: grey-black rubber with the diamond 'non-skid'
    tread across the middle third (V across the tyre), dusty."""
    x, y = T.grid(size)
    tread_band = T.smooth(0.30, 0.34, y) * (1 - T.smooth(0.66, 0.70, y))
    d = (np.abs(((x * 24) % 1.0) - 0.5) + np.abs(((y - 0.5) * 6) % 1.0 - 0.5))
    diamonds = T.smooth(0.30, 0.40, d) * tread_band
    dust = T.smooth(0.4, 0.8, T.fbm(size, 6, 601, 4))
    colour = np.array((0.09, 0.09, 0.09)) * (0.9 + 0.2 * T.fbm(size, 20, 602, 2))[..., None]
    colour = T.mix(colour, (0.36, 0.32, 0.27), dust * 0.35 + 0.12 * (1 - diamonds) * tread_band)
    height = diamonds * 0.8
    return {
        "color": np.clip(colour, 0, 1),
        "normal_gl": T.normals(height, 3.0 * size / 512),
        "roughness": np.clip(0.85 + 0.1 * dust, 0, 1),
        "ao": np.clip(1 - 0.4 * (1 - diamonds) * tread_band, 0, 1),
        "height": height,
    }


SETS = {
    "veh_wood": wood,
    "veh_paint_black": paint_black,
    "veh_paint_red": paint_red,
    "veh_paint_green": paint_green,
    "veh_paint_yellow": paint_yellow,
    "veh_iron": iron,
    "veh_hide": hide,
    "veh_cane": cane,
    "veh_leather": leather,
    "veh_brass": brass,
    "veh_rubber": rubber,
}

SIZES = {name: (1024 if name in ("veh_wood", "veh_hide") else 512) for name in SETS}

TEXTURE_DIR = os.path.join(HERE, "..", "assets", "textures", "vehicles")


def paint_sets(out_dir, names=None):
    for name in names or SETS:
        maps = SETS[name](SIZES[name])
        folder = os.path.join(out_dir, name)
        os.makedirs(folder, exist_ok=True)
        for key in ("color", "normal_gl", "roughness", "ao"):
            T.save(maps[key], os.path.join(folder, f"{key}.jpg"))
        print(f"{folder}: color, normal_gl, roughness, ao")


# ---------------------------------------------------------------------------
# Blender: meshes, materials, LOD, export, previews.

if bpy is not None:
    from mathutils import Vector

    def reset():
        bpy.ops.wm.read_factory_settings(use_empty=True)

    def material(name, textures=TEXTURE_DIR, label=None):
        """The material ``name``: for the GLB only its name counts; for the
        previews, its set's colour (times the vertex tint), normal and
        roughness maps. ``label`` names the Blender material (the previews
        of imported pieces, whose own materials took ``name``)."""
        label = label or name
        mat = bpy.data.materials.get(label)
        if mat:
            return mat
        mat = bpy.data.materials.new(label)
        mat.use_nodes = True
        nt = mat.node_tree
        bsdf = nt.nodes["Principled BSDF"]
        metal, rough_k = SHADING.get(name, (0.0, 1.0))
        bsdf.inputs["Metallic"].default_value = metal
        if name == "Glass":
            bsdf.inputs["Base Color"].default_value = (0.6, 0.65, 0.65, 1)
            bsdf.inputs["Roughness"].default_value = 0.05
            bsdf.inputs["Alpha"].default_value = 0.25
            return mat
        folder = os.path.join(textures, SET_OF[name])
        attr = nt.nodes.new("ShaderNodeVertexColor")
        attr.layer_name = ""  # the active colours: "Tint", or "Color" once imported

        def image(key, colour):
            path = os.path.join(folder, f"{key}.jpg")
            if not os.path.exists(path):
                return None
            node = nt.nodes.new("ShaderNodeTexImage")
            node.image = bpy.data.images.load(path, check_existing=True)
            node.image.colorspace_settings.name = "sRGB" if colour else "Non-Color"
            return node

        col = image("color", True)
        if col:
            mix = nt.nodes.new("ShaderNodeMix")
            mix.data_type = "RGBA"
            mix.blend_type = "MULTIPLY"
            mix.inputs["Factor"].default_value = 1.0
            nt.links.new(col.outputs["Color"], mix.inputs[6])
            nt.links.new(attr.outputs["Color"], mix.inputs[7])
            nt.links.new(mix.outputs[2], bsdf.inputs["Base Color"])
        rough = image("roughness", False)
        if rough:
            k = nt.nodes.new("ShaderNodeMath")
            k.operation = "MULTIPLY"
            k.inputs[1].default_value = rough_k
            nt.links.new(rough.outputs["Color"], k.inputs[0])
            nt.links.new(k.outputs[0], bsdf.inputs["Roughness"])
        nrm = image("normal_gl", False)
        if nrm:
            nm = nt.nodes.new("ShaderNodeNormalMap")
            nt.links.new(nrm.outputs["Color"], nm.inputs["Color"])
            nt.links.new(nm.outputs["Normal"], bsdf.inputs["Normal"])
        return mat

    def to_object(name, mesh, parent=None, smooth_angle=35.0):
        me = bpy.data.meshes.new(name)
        me.from_pydata(mesh.verts, [], [f[0] for f in mesh.faces])
        me.update()
        mats = mesh.materials()
        for mname in mats:
            me.materials.append(material(mname))
        index = {n: i for i, n in enumerate(mats)}
        uv = me.uv_layers.new(name="UVMap")
        col = me.color_attributes.new("Tint", "BYTE_COLOR", "CORNER")
        li = 0
        for poly, (idx, mname, uvs) in zip(me.polygons, mesh.faces):
            poly.material_index = index[mname]
            for k, loop in enumerate(poly.loop_indices):
                uv.data[loop].uv = uvs[k]
                t = mesh.tints[idx[k]]
                col.data[loop].color = (t[0], t[1], t[2], 1.0)
                li += 1
        # Every closed part faces out, whichever way its profile ran (the
        # loops keep their UVs and tints).
        import bmesh

        bm = bmesh.new()
        bm.from_mesh(me)
        bmesh.ops.remove_doubles(bm, verts=bm.verts, dist=1e-6)
        bmesh.ops.recalc_face_normals(bm, faces=bm.faces)
        bm.to_mesh(me)
        bm.free()
        obj = bpy.data.objects.new(name, me)
        bpy.context.scene.collection.objects.link(obj)
        if parent:
            obj.parent = parent
        # Hard edges above ``smooth_angle``, smooth below (chamfers shade
        # like rounded edges, turned parts look round).
        for p in me.polygons:
            p.use_smooth = True
        me.set_sharp_from_angle(angle=math.radians(smooth_angle))
        return obj

    def empty(name, at, parent=None, size=0.1):
        obj = bpy.data.objects.new(name, None)
        obj.empty_display_size = size
        obj.location = at
        bpy.context.scene.collection.objects.link(obj)
        if parent:
            obj.parent = parent
        return obj

    def lod(obj, ratio, name):
        """A decimated copy of ``obj`` (the far mesh)."""
        copy = obj.copy()
        copy.data = obj.data.copy()
        copy.name = name
        copy.data.name = name
        bpy.context.scene.collection.objects.link(copy)
        mod = copy.modifiers.new("Decimate", "DECIMATE")
        mod.ratio = ratio
        mod.use_collapse_triangulate = True
        bpy.context.view_layer.objects.active = copy
        for o in bpy.context.view_layer.objects:
            o.select_set(o is copy)
        bpy.ops.object.modifier_apply(modifier=mod.name)
        return copy

    def instantiate(vehicle, lod_ratio=0.3):
        """The Blender objects of ``vehicle``: its root, ``Body`` and wheel
        empties with their meshes and LODs, and its markers."""
        root = empty(vehicle.name, (0, 0, 0), size=0.5)
        for k, v in vehicle.extras.items():
            root[k] = v
        body = empty("Body", (0, 0, 0), root)
        vehicle.body.tint(ground_dirt())
        bm = to_object("Body_Mesh", vehicle.body, body)
        lod(bm, lod_ratio, "Body_LOD").parent = body
        for name, (hub, radius, mesh) in vehicle.wheels.items():
            w = empty(name, hub, root, size=radius)
            w["radius"] = radius
            wm = to_object(f"{name}_Mesh", mesh, w)
            lod(wm, lod_ratio, f"{name}_LOD").parent = w
        for name, at in vehicle.markers.items():
            empty(name, at, root, size=0.15)
        return root

    def export(path):
        bpy.ops.export_scene.gltf(
            filepath=path,
            export_format="GLB",
            export_yup=True,
            export_skins=False,
            export_animations=False,
            export_vertex_color="ACTIVE",
            export_all_vertex_colors=False,
            export_texcoords=True,
            export_tangents=True,
            export_image_format="NONE",
            export_cameras=False,
            export_lights=False,
            export_extras=True,
            export_materials="EXPORT",
        )

    def _maps_material(name, colour, normal, rough, tint=None, vertex=None, opacity=None):
        """A preview material from a shipped map set, as the game draws it:
        the colour map times ``tint`` (a material's own colour) or the
        vertex colours ``vertex`` (a coat), its normal and roughness maps,
        and an alpha test from ``opacity`` (hair cards)."""
        mat = bpy.data.materials.new(name)
        mat.use_nodes = True
        nt = mat.node_tree
        bsdf = nt.nodes["Principled BSDF"]

        def image(path, colour_space):
            node = nt.nodes.new("ShaderNodeTexImage")
            node.image = bpy.data.images.load(path, check_existing=True)
            node.image.colorspace_settings.name = colour_space
            return node

        col = image(colour, "sRGB")
        out = col.outputs["Color"]
        if tint is not None or vertex is not None:
            mix = nt.nodes.new("ShaderNodeMix")
            mix.data_type = "RGBA"
            mix.blend_type = "MULTIPLY"
            mix.inputs["Factor"].default_value = 1.0
            nt.links.new(out, mix.inputs[6])
            if vertex is not None:
                vc = nt.nodes.new("ShaderNodeVertexColor")
                vc.layer_name = vertex
                nt.links.new(vc.outputs["Color"], mix.inputs[7])
            else:
                mix.inputs[7].default_value = tint
            out = mix.outputs[2]
        nt.links.new(out, bsdf.inputs["Base Color"])
        if rough and os.path.exists(rough):
            nt.links.new(image(rough, "Non-Color").outputs["Color"], bsdf.inputs["Roughness"])
        if normal and os.path.exists(normal):
            nm = nt.nodes.new("ShaderNodeNormalMap")
            nt.links.new(image(normal, "Non-Color").outputs["Color"], nm.inputs["Color"])
            nt.links.new(nm.outputs["Normal"], bsdf.inputs["Normal"])
        if opacity and os.path.exists(opacity):
            nt.links.new(image(opacity, "Non-Color").outputs["Color"], bsdf.inputs["Alpha"])
            mat.blend_method = "CLIP" if hasattr(mat, "blend_method") else None
        return mat

    def import_animal(path, at=(0, 0, 0), body=None, hair=None, scale=1.0, heading=0.0, coat_map=None,
                      paint=None, hair_tint=(1, 1, 1, 1), action="Walk", frame=0, part_tint=None):
        """For the previews: one of our animals as the game draws it: its GLB
        (``tools/horse.py``'s horse or ``tools/cattle.py --ox``'s ox) at
        ``at``, posed at ``frame`` of its clip ``action``, its skin from the
        shipped map sets ``body`` and ``hair``: ``Main`` with ``coat_map``
        (a baked coat) or the vertex coat ``paint(mesh)`` paints, the
        one-colour parts with their own colour over the body's map, the hair
        cards tinted ``hair_tint`` and alpha-tested (``part_tint(name)``
        gives a part's coat colour, as the game tints it). Returns (objects,
        armature)."""
        before = set(bpy.data.objects)
        bpy.ops.import_scene.gltf(filepath=path)
        new = [o for o in bpy.data.objects if o not in before]
        arm = next((o for o in new if o.type == "ARMATURE"), None)
        for r in (o for o in new if o.parent is None):
            r.location = Vector(at) + r.location
            r.scale = (scale, scale, scale)
            r.rotation_euler[2] += heading
        for o in new:
            if o.type != "MESH":
                continue
            if o.name.endswith("_LOD") or o.name.startswith("Icosphere"):
                o.hide_render = True
                continue
            if paint:
                paint(o)
            for slot in o.material_slots:
                m = slot.material
                if not m or m.name.startswith("Eye"):
                    continue
                base = m.name.split(".")[0]
                own = tuple(m.node_tree.nodes["Principled BSDF"].inputs["Base Color"].default_value) if m.use_nodes else (1, 1, 1, 1)
                if base == "Hair" and hair:
                    slot.material = _maps_material(f"{base}_{o.name}", os.path.join(hair, "color.jpg"),
                                                   os.path.join(hair, "normal_gl.jpg"), os.path.join(hair, "roughness.jpg"),
                                                   tint=hair_tint, opacity=os.path.join(hair, "opacity.jpg"))
                elif body:
                    main = base == "Main"
                    if part_tint and part_tint(base):
                        own = part_tint(base)
                    colour = os.path.join(body, coat_map if (main and coat_map) else "color.jpg")
                    slot.material = _maps_material(
                        f"{base}_{o.name}", colour, os.path.join(body, "normal_gl.jpg"), os.path.join(body, "roughness.jpg"),
                        tint=None if (main and (coat_map or paint)) else own, vertex="Coat" if (main and paint and not coat_map) else None)
        if arm is not None and arm.animation_data is None:
            arm.animation_data_create()
        clip = next((a for a in bpy.data.actions if a.name.split("_")[0] == action or a.name == action), None)
        if arm is not None and clip is not None:
            arm.animation_data.action = clip
            if hasattr(arm.animation_data, "action_slot") and clip.slots:
                arm.animation_data.action_slot = clip.slots[0]
            bpy.context.scene.frame_set(frame)
        return new, arm

    def import_pieces(path, arm, parent_at=(0, 0, 0), scale=1.0, heading=0.0):
        """For the previews: a harness or yoke GLB on the animal ``arm``:
        each piece ``<name>@<bone>`` follows its bone through the pose, as
        the game hangs it from that joint."""
        before = set(bpy.data.objects)
        bpy.ops.import_scene.gltf(filepath=path)
        new = [o for o in bpy.data.objects if o not in before]
        for o in new:
            if o.parent is None:
                o.location = Vector(parent_at) + Vector(o.location) * scale
                o.scale = Vector(o.scale) * scale
                o.rotation_euler[2] += heading
            for slot in getattr(o, "material_slots", []):
                base = slot.material.name.split(".")[0] if slot.material else ""
                if base in SET_OF:
                    slot.material = material(base, label=f"{base}_preview")
            if o.type == "MESH" and "@" in o.name and arm is not None:
                bone = o.name.split("@")[1].split(".")[0]
                group = o.vertex_groups.new(name=bone)
                group.add(range(len(o.data.vertices)), 1.0, "REPLACE")
                mod = o.modifiers.new("Armature", "ARMATURE")
                mod.object = arm
        return new

    def render_sheet(out_png, shots, size=(1100, 700), samples=64):
        """Renders ``shots`` [(eye, target, lens)] with Cycles on a dusty
        ground under a low sun, and joins them side by side into
        ``out_png`` (``magick``)."""
        scene = bpy.context.scene
        scene.render.engine = "CYCLES"
        scene.cycles.samples = samples
        scene.cycles.use_denoising = True
        scene.cycles.device = "CPU"
        # Many hair cards overlap in a mane or a tail.
        scene.cycles.transparent_max_bounces = 96
        scene.render.resolution_x, scene.render.resolution_y = size
        scene.render.image_settings.file_format = "PNG"
        scene.view_settings.view_transform = "AgX"
        world = bpy.data.worlds.new("Sky")
        world.use_nodes = True
        bg = world.node_tree.nodes["Background"]
        bg.inputs["Color"].default_value = (0.52, 0.60, 0.72, 1)
        bg.inputs["Strength"].default_value = 0.9
        scene.world = world
        bpy.ops.mesh.primitive_plane_add(size=60)
        ground = bpy.context.active_object
        gm = bpy.data.materials.new("Ground")
        gm.use_nodes = True
        gm.node_tree.nodes["Principled BSDF"].inputs["Base Color"].default_value = (0.42, 0.36, 0.28, 1)
        gm.node_tree.nodes["Principled BSDF"].inputs["Roughness"].default_value = 0.95
        ground.data.materials.append(gm)
        sun_data = bpy.data.lights.new("Sun", "SUN")
        sun_data.energy = 4.0
        sun_data.angle = math.radians(3)
        sun = bpy.data.objects.new("Sun", sun_data)
        sun.rotation_euler = (math.radians(52), math.radians(8), math.radians(-40))
        scene.collection.objects.link(sun)
        cam = bpy.data.objects.new("Cam", bpy.data.cameras.new("Cam"))
        scene.collection.objects.link(cam)
        scene.camera = cam
        parts = []
        base, _ = os.path.splitext(out_png)
        for k, (eye, target, lens) in enumerate(shots):
            cam.location = eye
            cam.data.lens = lens
            d = Vector(target) - cam.location
            cam.rotation_euler = d.to_track_quat("-Z", "Y").to_euler()
            part = f"{base}_{k}.png"
            scene.render.filepath = part
            bpy.ops.render.render(write_still=True)
            parts.append(part)
        subprocess.run(["magick", *parts, "+append", out_png], check=True)
        for p in parts:
            os.unlink(p)
        for o in (ground, sun, cam):
            bpy.data.objects.remove(o)


def args():
    """The arguments after Blender's ``--``."""
    argv = sys.argv
    return argv[argv.index("--") + 1:] if "--" in argv else []


def option(argv, flag, default=None):
    if flag in argv:
        i = argv.index(flag)
        return argv[i + 1]
    return default


if __name__ == "__main__" and bpy is None:
    if len(sys.argv) != 2:
        print("usage: python3 -I tools/vehicle.py OUT_DIR", file=sys.stderr)
        sys.exit(1)
    paint_sets(sys.argv[1])
