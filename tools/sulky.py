"""Build the sulky (1919 La Pampa) in Blender, by script.

    blender -b --factory-startup --python tools/sulky.py -- OUT.glb [--preview SHEET.png]

The pampa's sulky: a light two-wheeled, one-horse gig with a single
sprung seat for one or two, the everyday carriage of a chacarero, a
doctor or a storekeeper going to town (Argentine trade catalogues of the
1910s sold it as the "sulky americano", after the American road cart and
the trotting sulky). Its parts, at period sizes:

- two tall wheels, 1.40 m over the iron tyre, fourteen staggered oval
  hickory spokes in a Sarven-style turned nave with iron bands, a 4.5 cm
  felloe, 1.50 m between the hubs, painted Indian red;
- an arched iron axle (so the seat sits low between the wheels), two full
  elliptic springs on it, and over them the seat: a black box with a
  padded leather cushion and a rolled leather back on an iron rail, its
  arm rails curling forward;
- two long shafts (varas) of bent hickory, red like the wheels, running
  from in front of the horse's shoulders, through the tugs of its harness
  pad, past its quarters, and out to the seat, joined by a front and a
  rear bar, an iron ferrule on each tip and the singletree (balancín)
  with its hooks on the front bar;
- a sloping slatted footboard, a leather dash on a bent iron frame, a
  mudguard over each wheel, an iron step on the left, and a whip in its
  socket on the dash;
- the traces (tiros) from the horse's breast collar to the singletree and
  the reins from its bit, over the pad's terrets, to the driver's hands
  (static: the harness on the horse is ``tools/vehicle_harness.py``).

The origin is the ground under the horse's body (``tools/horse.py``'s
origin), so the game moves the sulky by its horse; ``Animal.0`` marks it.
The vehicle faces -Y, its left is +X. See ``tools/vehicle.py`` for the
nodes, materials and the preview.
"""

import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)

import vehicle as V  # noqa: E402
import vehicle_harness as H  # noqa: E402

WHEEL_RADIUS = 0.70
AXLE_Y = 2.30
HUB_X = 0.75
SEAT = (0.0, 2.32, 1.40)
SINGLETREE_Y = 1.24


def shaft_path(sx):
    """The left shaft (``sx`` = +1) or the right (-1), tip to tail."""
    tip = (0.30 * sx, -0.78, 1.16)
    tug = (H.TUG[0] * sx + 0.07 * sx, H.TUG[1], H.TUG[2] - 0.02)
    hind = (0.37 * sx, 0.70, 1.02)
    bar = (0.44 * sx, 1.30, 0.98)
    seat = (0.46 * sx, 2.10, 0.98)
    tail = (0.46 * sx, 2.78, 0.96)
    p = V.bezier(tip, (0.31 * sx, -0.55, 1.10), (tug[0], tug[1] - 0.25, tug[2]), tug, 6)
    p += V.bezier(tug, (tug[0], tug[1] + 0.35, tug[2]), (hind[0], hind[1] - 0.3, hind[2]), hind, 5)[1:]
    p += V.bezier(hind, (hind[0], hind[1] + 0.25, hind[2]), (bar[0] - 0.03 * sx, bar[1] - 0.2, bar[2]), bar, 4)[1:]
    p += [seat, tail]
    return p


def build():
    v = V.Vehicle("Sulky")
    m = v.body
    red, black, iron, leather = "Paint_Red", "Paint_Black", "Iron", "Leather"
    # Wheels.
    for name, sx in (("Wheel.L", 1.0), ("Wheel.R", -1.0)):
        w = V.wheel(
            WHEEL_RADIUS,
            14,
            out=sx,
            hub=(0.068, 0.21),
            felloe=(0.045, 0.040),
            tyre=(0.010, 0.036),
            spoke=(0.036, 0.024),
            dish=0.022,
            stagger=0.010,
            wood=red,
            iron=iron,
            hub_material=black,
        )
        v.add_wheel(name, (HUB_X * sx, AXLE_Y, WHEEL_RADIUS), WHEEL_RADIUS, w)
    # The arched axle and its two elliptic springs.
    z0 = WHEEL_RADIUS
    axle = V.bezier((-HUB_X + 0.08, AXLE_Y, z0), (-0.45, AXLE_Y, z0), (-0.40, AXLE_Y, z0 + 0.20), (0.0, AXLE_Y, z0 + 0.20), 6)
    axle = axle + [(-p[0], p[1], p[2]) for p in reversed(axle[:-1])]
    m.extend(V.sweep(axle, V.rect(0.035, 0.045, 0.006), black, (0.6, 0.2), up=(0, 1, 0)))
    zc, bow = z0 + 0.32, 0.10
    for sx in (1.0, -1.0):
        # Each bow of the ellipse: three leaves, the longest outside; the
        # lower bow sits on the axle, the upper carries the seat.
        for sign in (-1.0, 1.0):
            for leaf in range(3):
                half = 0.28 - 0.06 * leaf
                pts = []
                for j in range(9):
                    t = -1.0 + 2.0 * j / 8
                    z = zc + sign * (bow * (1 - t * t) - 0.009 * leaf)
                    pts.append((0.33 * sx, AXLE_Y + half * t, z))
                m.extend(V.sweep(pts, V.rect(0.045, 0.008, 0.002), black, (0.5, 0.1), up=(0, 0, 1)))
        # Spring clips.
        for zz in (zc - bow, zc + bow):
            m.extend(V.beam((0.33 * sx, AXLE_Y, zz - 0.03), (0.33 * sx, AXLE_Y, zz + 0.03), 0.06, 0.05, iron))
    # Shafts, their ferrules, and the front and rear bars.
    for sx in (1.0, -1.0):
        path = shaft_path(sx)
        n = len(path)
        scales = [0.72 + 0.28 * min(1.0, k / 6) for k in range(n)]
        m.extend(V.sweep(path, V.rect(0.042, 0.052, 0.008), red, (1.2, 0.2), scales=scales))
        tip = path[0]
        m.extend(V.rod([V.add(tip, (0, -0.02, 0.005)), V.add(tip, (0, 0.10, -0.01))], 0.019, iron, n=10))
        # The shaft's loop (a tug stop) behind the tug, holding the pad.
        stop = (path[6][0], path[6][1] + 0.18, path[6][2])
        m.extend(V.beam(V.add(stop, (0, 0, -0.03)), V.add(stop, (0, 0, 0.03)), 0.06, 0.012, iron))
    m.extend(V.beam((-0.44, 1.30, 0.98), (0.44, 1.30, 0.98), 0.07, 0.06, red, up=(0, 1, 0)))
    m.extend(V.beam((-0.46, 2.76, 0.96), (0.46, 2.76, 0.96), 0.06, 0.05, red, up=(0, 1, 0)))
    for sx in (1.0, -1.0):
        # Brace from the shafts down to the axle's ends.
        m.extend(V.rod([(0.45 * sx, 2.05, 0.97), (0.62 * sx, AXLE_Y, z0 + 0.02)], 0.011, black))
        m.extend(V.rod([(0.45 * sx, 2.55, 0.97), (0.62 * sx, AXLE_Y, z0 + 0.02)], 0.011, black))
    # The singletree, with its hooks and the clip to the front bar.
    st = SINGLETREE_Y
    m.extend(V.sweep(
        [(-0.36, st, 1.00), (-0.15, st, 1.005), (0.15, st, 1.005), (0.36, st, 1.00)],
        V.ellipse(0.05, 0.045, 10), "Wood", (0.7, 0.15), up=(0, 1, 0), scales=[0.7, 1.0, 1.0, 0.7]))
    for sx in (1.0, -1.0):
        m.extend(V.rod([(0.36 * sx, st, 1.0), (0.40 * sx, st, 1.0), (0.41 * sx, st - 0.04, 1.0)], 0.007, iron, n=6))
    m.extend(V.rod([(0.0, st, 1.0), (0.0, 1.30, 0.98)], 0.012, iron))
    # The seat: a black box riser on the springs, a padded cushion, the
    # rolled back on its iron rail and the arm rails.
    sy, sz = SEAT[1], SEAT[2]
    m.extend(V.beam((-0.44, sy - 0.24, sz - 0.30), (0.44, sy - 0.24, sz - 0.30), 0.06, 0.04, black, up=(0, 1, 0)))
    m.extend(V.beam((-0.44, sy + 0.22, sz - 0.30), (0.44, sy + 0.22, sz - 0.30), 0.06, 0.04, black, up=(0, 1, 0)))
    box = [((-0.42, sy - 0.24), (0.42, sy - 0.24)), ((0.42, sy - 0.24), (0.42, sy + 0.22)),
           ((0.42, sy + 0.22), (-0.42, sy + 0.22)), ((-0.42, sy + 0.22), (-0.42, sy - 0.24))]
    for (ax, ay), (bx, by) in box:
        m.extend(V.beam((ax, ay, sz - 0.17), (bx, by, sz - 0.17), 0.022, 0.22, black, chamfer=0.006))
    # Moulding round the top of the box.
    for (ax, ay), (bx, by) in box:
        m.extend(V.beam((ax, ay, sz - 0.055), (bx, by, sz - 0.055), 0.034, 0.02, black, chamfer=0.006))
    cushion = V.sweep([(-0.41, sy - 0.02, sz - 0.035), (0.41, sy - 0.02, sz - 0.035)],
                      V.rect(0.46, 0.07, 0.03), leather, (0.4, 0.4), up=(0, 0, 1))
    m.extend(cushion)
    # Tufting: buttons in two rows.
    for i in range(5):
        for j in range(2):
            b = (-0.32 + 0.16 * i, sy - 0.12 + 0.2 * j, sz + 0.001)
            m.extend(V.lathe([(0.0, 0.0), (0.012, 0.0), (0.0, 0.006)], 6, leather, axis="z").translated(b))
    rail = V.bezier((-0.40, sy + 0.12, sz - 0.02), (-0.46, sy + 0.30, sz + 0.25), (-0.30, sy + 0.32, sz + 0.36), (0.0, sy + 0.32, sz + 0.36), 8)
    rail = rail + [(-p[0], p[1], p[2]) for p in reversed(rail[:-1])]
    m.extend(V.rod(rail, 0.011, black, n=8))
    back = V.surface(
        lambda u, w: (
            (-0.36 + 0.72 * u),
            sy + 0.17 + 0.10 * w + 0.03 * (2 * u - 1) ** 2,
            sz + 0.04 + 0.28 * w,
        ),
        10, 4, leather, (0.4, 0.4))
    m.extend(V.thick(back, 0.05))
    for sx in (1.0, -1.0):
        arm = V.bezier((0.43 * sx, sy + 0.18, sz + 0.22), (0.47 * sx, sy - 0.05, sz + 0.24),
                       (0.47 * sx, sy - 0.18, sz + 0.16), (0.43 * sx, sy - 0.22, sz + 0.02), 6)
        m.extend(V.rod(arm, 0.009, black, n=8))
    # The footboard (slats on two bearers) and the dash.
    fy0, fz0, fy1, fz1 = sy - 0.26, sz - 0.30, 1.52, 1.02
    for sx in (1.0, -1.0):
        m.extend(V.beam((0.30 * sx, fy0, fz0), (0.30 * sx, fy1, fz1), 0.03, 0.04, black))
        m.extend(V.rod([(0.30 * sx, fy1, fz1), (0.44 * sx, 1.30, 0.99)], 0.01, iron))
    for k in range(7):
        t = (k + 0.5) / 7
        y = fy0 + (fy1 - fy0) * t
        z = fz0 + (fz1 - fz0) * t
        m.extend(V.beam((-0.33, y, z + 0.025), (0.33, y, z + 0.025), 0.07, 0.016, black, chamfer=0.004, up=(0, 0, 1)))
    dash_frame = [(-0.32, fy1 + 0.02, fz1 + 0.02), (-0.34, fy1 - 0.04, fz1 + 0.34),
                  (0.34, fy1 - 0.04, fz1 + 0.34), (0.32, fy1 + 0.02, fz1 + 0.02)]
    m.extend(V.rod(dash_frame, 0.008, iron, n=6))
    dash = V.surface(lambda u, w: (-0.32 + 0.64 * u, fy1 + 0.01 - 0.05 * w, fz1 + 0.03 + 0.30 * w), 6, 3, leather, (0.5, 0.5))
    m.extend(V.thick(dash, 0.006))
    # The whip socket and the whip.
    sock = (0.30, fy1 - 0.02, fz1 + 0.32)
    m.extend(V.rod([V.add(sock, (0, 0, -0.14)), sock], 0.018, black, n=8))
    whip = V.bezier(V.add(sock, (0, 0, -0.10)), V.add(sock, (0.05, -0.05, 0.5)), V.add(sock, (0.16, -0.20, 0.95)), V.add(sock, (0.30, -0.48, 1.15)), 10)
    m.extend(V.sweep(whip, V.circle(0.011, 6), "Wood", (0.5, 0.05), up=(1, 0, 0), scales=[1.0 - 0.75 * k / 10 for k in range(11)]))
    lash = V.bezier(whip[-1], V.add(whip[-1], (0.08, -0.12, -0.1)), V.add(whip[-1], (0.10, -0.15, -0.6)), V.add(whip[-1], (0.05, -0.05, -0.85)), 10)
    m.extend(V.rod(lash, 0.003, leather, n=4, caps=False))
    # Mudguards over the wheels, on iron stays.
    import math
    for sx in (1.0, -1.0):
        r = WHEEL_RADIUS + 0.06

        def guard(u, w, sx=sx, r=r):
            a = math.radians(35 + 110 * u)
            return (HUB_X * sx - 0.06 * sx + 0.12 * sx * w, AXLE_Y - r * math.cos(a), WHEEL_RADIUS + r * math.sin(a))

        m.extend(V.thick(V.surface(guard, 16, 2, black, (0.6, 0.3)), 0.005))
        for a in (50, 130):
            ar = math.radians(a)
            p = (HUB_X * sx - 0.05 * sx, AXLE_Y - r * math.cos(ar), WHEEL_RADIUS + r * math.sin(ar))
            m.extend(V.rod([p, (0.46 * sx, AXLE_Y + (p[1] - AXLE_Y) * 0.5, 1.0)], 0.007, iron, n=6))
    # The step, on the left.
    m.extend(V.rod([(0.44, sy - 0.30, 0.97), (0.56, sy - 0.32, 0.62)], 0.012, iron))
    m.extend(V.beam((0.50, sy - 0.40, 0.62), (0.62, sy - 0.24, 0.62), 0.14, 0.012, iron, up=(0, 0, 1)))
    # Traces from the breast collar to the singletree, the breeching straps
    # to the shafts, and the reins from the bit to the driver's hands.
    for sx in (1.0, -1.0):
        a = (H.TRACE[0] * sx, H.TRACE[1], H.TRACE[2])
        b = (0.40 * sx, st - 0.04, 1.0)
        mid = V.lerp(a, b, 0.5)
        side = (H.SIDE[0] * sx + 0.025 * sx, mid[1], mid[2] - 0.04)
        trace = V.bezier(a, V.lerp(a, side, 0.6), V.lerp(side, b, 0.4), b, 14)
        m.extend(V.sweep(trace, V.rect(0.035, 0.008, 0.002), leather, (0.4, 0.05), up=(1, 0, 0)))
        hold = (H.BREECH[0] * sx, H.BREECH[1], H.BREECH[2])
        on_shaft = (0.37 * sx, 0.72, 1.02)
        m.extend(V.sweep([hold, V.lerp(hold, on_shaft, 0.5), on_shaft], V.rect(0.03, 0.006, 0.002), leather, (0.4, 0.05), up=(0, 1, 0)))
        bit = (H.BIT[0] * sx, H.BIT[1], H.BIT[2])
        terret = (H.TERRET[0] * sx, H.TERRET[1], H.TERRET[2] + 0.02)
        hand = (0.11 * sx, sy - 0.40, sz + 0.12)
        rein = V.bezier(bit, V.add(bit, (0.03 * sx, 0.35, -0.12)), V.add(terret, (0.0, -0.4, -0.06)), terret, 10)
        rein += V.bezier(terret, V.add(terret, (0, 0.6, -0.08)), V.add(hand, (0, -0.6, -0.15)), hand, 10)[1:]
        m.extend(V.sweep(rein, V.rect(0.005, 0.022, 0.0015), leather, (0.4, 0.05), up=(1, 0, 0)))
    v.markers["Seat"] = SEAT
    v.markers["Animal.0"] = (0.0, 0.0, 0.0)
    v.extras = {"animals": 1, "animal": "horse"}
    return v


PREVIEW = [
    ((7.2, 1.0, 1.4), (0.0, 0.9, 1.0), 38),
    ((5.2, -4.2, 2.3), (0.0, 0.8, 1.0), 36),
    ((-3.4, 6.6, 2.2), (0.0, 1.4, 1.0), 38),
]

if __name__ == "__main__" and V.bpy is not None:
    argv = V.args()
    out = argv[0]
    V.reset()
    v = build()
    V.instantiate(v)
    V.export(out)
    print(f"sulky: {v.triangles()} triangles, materials {v.materials()}")
    sheet = V.option(argv, "--preview")
    if sheet:
        H.preview_horse((0.0, 0.0, 0.0))
        V.render_sheet(sheet, PREVIEW)
