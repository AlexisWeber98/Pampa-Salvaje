"""Build the intendente's Ford T (General Pico, 1919) in Blender, by script.

    blender -b --factory-startup --python tools/ford_t.py -- OUT.glb [--preview SHEET.png]

The one motor car of the setting (user, 2026-10-22): a Ford Model T
touring car, the 1915-1916 kind with the brass radiator and the electric
headlamps, as Ford's Buenos Aires branch sold them from 1913 and the
pampa's well-off bought them: black enamel, left-hand drive. At period
sizes (Ford's 1915 catalogue: 100 in wheelbase, 56 in track, 30 × 3½ in
tyres):

- four wooden artillery wheels, twelve spokes, 0.76 m over the clincher
  tyres (the diamond "non-skid" tread), a brass hub cap on each;
- the frame, the transverse leaf springs over each axle, the brass
  radiator with its filler cap and its dark core, the black hood with its
  louvres, the cowl, the brass-framed windshield in two panes;
- the touring body: front doors, the front and rear seats of tufted black
  leather, the steering wheel on its column on the left, the folding top
  up on its bows with the side straps;
- the crowned fenders sweeping into the running boards, the brass
  headlamps and kerosene side lamps, the tail lamp, the bulb horn and the
  starting crank.

The origin is the ground midway between its axles; it faces -Y, its left
is +X. ``Seat`` is the driver's place and ``Steering`` the wheel's centre.
See ``tools/vehicle.py``.
"""

import math
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)

import vehicle as V  # noqa: E402

WHEEL_RADIUS = 0.38
WHEELBASE = 2.54
TRACK = 1.42
FRONT_Y, REAR_Y = -WHEELBASE / 2, WHEELBASE / 2
HUB_X = TRACK / 2
SEAT = (0.30, 0.05, 1.02)
STEERING = (0.30, -0.32, 1.32)


def build():
    v = V.Vehicle("FordT")
    m = v.body
    black, iron, brass, leather, glass = "Paint_Black", "Iron", "Brass", "Leather", "Glass"
    for name, y, sx in (("Wheel.FL", FRONT_Y, 1.0), ("Wheel.FR", FRONT_Y, -1.0), ("Wheel.RL", REAR_Y, 1.0), ("Wheel.RR", REAR_Y, -1.0)):
        w = V.wheel(WHEEL_RADIUS, 12, out=sx, hub=(0.065, 0.14), felloe=(0.03, 0.05), tyre=(0.042, 0.085),
                    spoke=(0.055, 0.036), dish=0.0, stagger=0.0, wood="Wood", iron=iron, tyre_material="Rubber",
                    hub_material=brass)
        v.add_wheel(name, (HUB_X * sx, y, WHEEL_RADIUS), WHEEL_RADIUS, w)
    r = WHEEL_RADIUS
    # Frame, axles and transverse springs.
    for sx in (1.0, -1.0):
        m.extend(V.beam((0.38 * sx, FRONT_Y - 0.30, 0.52), (0.38 * sx, REAR_Y + 0.30, 0.52), 0.04, 0.09, black, chamfer=0.006))
    for y in (FRONT_Y, REAR_Y):
        m.extend(V.beam((-HUB_X + 0.06, y, r), (HUB_X - 0.06, y, r), 0.05, 0.06, black, up=(0, 1, 0)))
        for k in range(4):
            half = 0.62 - 0.1 * k
            pts = [(half * t, y, 0.56 + 0.07 * (1 - t * t) + 0.008 * k) for t in [-1 + j / 5 for j in range(11)]]
            m.extend(V.sweep(pts, V.rect(0.05, 0.007, 0.002), black, (0.5, 0.1), up=(0, 1, 0)))
        for sx in (1.0, -1.0):
            m.extend(V.rod([(0.62 * sx, y, 0.56), (0.62 * sx, y, r)], 0.012, iron))
    m.extend(V.rod([(0.0, -0.4, 0.42), (0.0, REAR_Y, r)], 0.04, black, n=10))  # torque tube
    m.extend(V.lathe([(0.0, -0.13), (0.16, -0.12), (0.17, 0.0), (0.16, 0.12), (0.0, 0.13)], 16, black).translated((0, REAR_Y, r)))
    # The radiator: brass shell, dark core, filler cap; and the crank.
    ry0, ry1 = FRONT_Y - 0.26, FRONT_Y - 0.12
    shell = [(-0.27, 0.55), (0.27, 0.55), (0.27, 1.12), (0.20, 1.20), (-0.20, 1.20), (-0.27, 1.12)]
    m.extend(V.sweep([(0, ry0, 0), (0, ry1, 0)], shell, brass, (0.4, 0.4), up=(0, 0, 1)))
    core = V.surface(lambda u, w: (-0.22 + 0.44 * u, ry0 - 0.004, 0.60 + 0.55 * w), 6, 6, iron, (0.2, 0.2))
    m.extend(core.tint(lambda p: (0.35, 0.35, 0.35)))
    m.extend(V.lathe([(0.0, 1.20), (0.045, 1.20), (0.05, 1.24), (0.035, 1.27), (0.0, 1.28)], 12, brass, axis="z").translated((0, (ry0 + ry1) / 2, 0)))
    m.extend(V.rod([(0.0, ry0, 0.50), (0.0, ry0 - 0.12, 0.50), (0.0, ry0 - 0.12, 0.38), (0.0, ry0 - 0.20, 0.38)], 0.012, iron))
    # The hood, with louvres, to the firewall; the cowl.
    hy0, hy1 = ry1, -0.62

    sect = [(0.255, 0.60)] + [(0.255 * math.cos(math.pi * k / 12), 0.78 + 0.37 * math.sin(math.pi * k / 12) ** 0.35)
                              for k in range(13)] + [(-0.255, 0.60)]
    hood = V.sweep([(0, hy0, 0), (0, hy1, 0)], sect, black, (0.8, 0.8), up=(0, 0, 1))
    m.extend(hood)
    for sx in (1.0, -1.0):
        for k in range(10):
            y = hy0 + 0.12 + 0.05 * k
            m.extend(V.beam((0.258 * sx, y, 0.72), (0.258 * sx, y, 0.86), 0.008, 0.015, black, chamfer=0.0, up=(1, 0, 0)))
    cowl = V.surface(lambda u, w: ((-0.30 + 0.60 * u) * (0.85 + 0.15 * w), hy1 + 0.25 * w,
                                   0.60 + (0.55 + 0.15 * w) * math.sin(math.pi * u) ** 0.3), 12, 4, black, (0.8, 0.8))
    m.extend(V.thick(cowl, 0.01))
    # The windshield: brass frame, two panes.
    wy = hy1 + 0.25
    frame = [(0.58, wy, 1.30), (0.58, wy, 1.82), (-0.58, wy, 1.82), (-0.58, wy, 1.30), (0.58, wy, 1.30)]
    m.extend(V.rod(frame, 0.016, brass, n=8))
    m.extend(V.rod([(0.58, wy, 1.56), (-0.58, wy, 1.56)], 0.012, brass, n=8))
    for z0, z1 in ((1.31, 1.55), (1.57, 1.81)):
        m.extend(V.surface(lambda u, w, z0=z0, z1=z1: (-0.57 + 1.14 * u, wy, z0 + (z1 - z0) * w), 1, 1, glass))
    # The body tub: sides with the front doors, the rounded back, the seats.
    by0, by1 = wy, REAR_Y + 0.35

    def side(u, w, sx):
        y = by0 + (by1 - by0) * u
        top = 1.28 if y < 0.55 else 1.34
        return (0.62 * sx - 0.03 * sx * w * w, y, 0.66 + (top - 0.66) * w)

    for sx in (1.0, -1.0):
        m.extend(V.thick(V.surface(lambda u, w, sx=sx: side(u, w, sx), 20, 4, black, (1.0, 0.8)), 0.02))
        m.extend(V.rod([(0.62 * sx, by0, 1.29), (0.62 * sx, 0.55, 1.29), (0.62 * sx, 0.62, 1.35), (0.62 * sx, by1, 1.35)], 0.02, leather, n=8))
        for y in (-0.15, 0.45):
            m.extend(V.beam((0.63 * sx, y, 0.70), (0.63 * sx, y, 1.26), 0.006, 0.01, black, chamfer=0.0, up=(0, 1, 0)))
        m.extend(V.lathe([(0.0, 0.0), (0.012, 0.0), (0.01, 0.02), (0.0, 0.025)], 8, brass, axis="z").moved(lambda p, sx=sx: (0.64 * sx + p[2] * sx, 0.40, 1.05 + p[1])))
    back = V.surface(lambda u, w: (0.62 * math.cos(math.pi * u), by1 + 0.12 * math.sin(math.pi * u), 0.66 + 0.69 * w), 12, 4, black, (1.0, 0.8))
    m.extend(V.thick(back, 0.02))
    m.extend(V.beam((-0.60, by0, 0.64), (0.60, by0, 0.64), 0.04, 0.06, black, up=(0, 1, 0)))
    m.extend(V.beam((-0.58, by0, 0.64), (-0.58, by1, 0.64), 0.04, 0.04, black))
    m.extend(V.beam((0.58, by0, 0.64), (0.58, by1, 0.64), 0.04, 0.04, black))
    m.extend(V.beam((0.0, by0 + 0.02, 0.66), (0.0, by1, 0.66), 1.20, 0.02, black, chamfer=0.0))
    for y0, depth, back_h in ((-0.05, 0.50, 0.42), (0.95, 0.55, 0.40)):
        m.extend(V.beam((-0.56, y0 + depth / 2, 0.90), (0.56, y0 + depth / 2, 0.90), depth, 0.14, leather, chamfer=0.04, up=(0, 0, 1)))
        m.extend(V.beam((-0.56, y0 + depth + 0.04, 0.97 + back_h / 2), (0.56, y0 + depth + 0.04, 0.97 + back_h / 2), 0.12, back_h,
                        leather, chamfer=0.04, up=(0, 1, 0)))
        for i in range(6):
            for j in range(2):
                b = (-0.45 + 0.18 * i, y0 + depth - 0.02, 1.05 + 0.15 * j)
                m.extend(V.lathe([(0.0, 0.0), (0.012, 0.0), (0.0, 0.008)], 6, leather, axis="z").moved(lambda p, b=b: (b[0] + p[0], b[1] + p[2], b[2] + p[1])))
    # The steering wheel and column, on the left.
    sxw, syw, szw = STEERING
    tilt = math.radians(40)
    # The wheel's plane faces the driver: up and back.
    u_dir = (0.0, math.cos(tilt), -math.sin(tilt))
    rim = [(sxw + 0.19 * math.cos(t), syw + 0.19 * math.sin(t) * u_dir[1], szw + 0.19 * math.sin(t) * u_dir[2])
           for t in [2 * math.pi * k / 24 for k in range(24)]]
    m.extend(V.rod(rim, 0.014, "Wood", n=8, closed=True, caps=False))
    for k in range(4):
        p = rim[k * 6]
        m.extend(V.rod([(sxw, syw, szw), p], 0.008, iron))
    m.extend(V.rod([(sxw, syw, szw), (sxw - 0.05, wy - 0.02, 0.62)], 0.022, black))
    # Fenders and running boards.
    for sx in (1.0, -1.0):
        x0, x1 = (HUB_X - 0.11) * sx, (HUB_X + 0.11) * sx

        def fender(u, w, y_c, a0, a1, sx=sx):
            a = math.radians(a0 + (a1 - a0) * u)
            rr = r + 0.09
            x = x0 + (x1 - x0) * w
            crown = 0.025 * math.sin(math.pi * w)
            return (x, y_c - (rr + crown) * math.cos(a), r + (rr + crown) * math.sin(a))

        m.extend(V.thick(V.surface(lambda u, w: fender(u, w, FRONT_Y, 20, 165), 18, 4, black, (0.8, 0.4)), 0.006))
        m.extend(V.thick(V.surface(lambda u, w: fender(u, w, REAR_Y, 15, 160), 18, 4, black, (0.8, 0.4)), 0.006))
        y_a = FRONT_Y + (r + 0.09) * math.cos(math.radians(165))
        y_b = REAR_Y - (r + 0.09) * math.cos(math.radians(15))
        z_a = r + (r + 0.09) * math.sin(math.radians(165))
        board = V.beam(((x0 + x1) / 2, y_a, z_a), ((x0 + x1) / 2, y_b, z_a), 0.24, 0.025, "Wood", chamfer=0.004, up=(0, 0, 1))
        m.extend(board)
        for y in (y_a + 0.05, y_b - 0.05, (y_a + y_b) / 2):
            m.extend(V.beam(((x0 + x1) / 2, y, z_a - 0.01), (0.39 * sx, y, 0.50), 0.03, 0.02, black))
        for k in range(10):
            y = y_a + 0.08 + (y_b - y_a - 0.16) * k / 9
            m.extend(V.beam(((x0 + x1) / 2 - 0.1, y, z_a + 0.014), ((x0 + x1) / 2 + 0.1, y, z_a + 0.014), 0.012, 0.004, iron, chamfer=0.0, up=(0, 0, 1)))
    # Lamps: brass headlamps, the kerosene side lamps, the tail lamp, the horn.
    for sx in (1.0, -1.0):
        at = (0.36 * sx, FRONT_Y - 0.20, 0.98)
        m.extend(V.lathe([(0.0, -0.10), (0.10, -0.10), (0.115, -0.06), (0.115, 0.06), (0.10, 0.09), (0.0, 0.11)], 16, brass)
                 .moved(lambda p, at=at: (at[0] + p[1], at[1] + p[0], at[2] + p[2])))
        m.extend(V.surface(lambda u, w, at=at: (at[0] + 0.1 * math.cos(2 * math.pi * u) * w, at[1] - 0.112, at[2] + 0.1 * math.sin(2 * math.pi * u) * w), 12, 1, glass))
        m.extend(V.rod([(at[0], at[1], at[2] - 0.11), (0.36 * sx, at[1], 0.55), (0.30 * sx, at[1] + 0.05, 0.52)], 0.012, black))
        sl = (0.34 * sx, wy - 0.02, 1.08)
        m.extend(V.lathe([(0.0, -0.08), (0.05, -0.08), (0.05, 0.07), (0.035, 0.10), (0.0, 0.12)], 8, black, axis="z").translated(sl))
        m.extend(V.lathe([(0.0, 0.10), (0.02, 0.11), (0.0, 0.14)], 8, brass, axis="z").translated(sl))
    m.extend(V.lathe([(0.0, -0.05), (0.04, -0.05), (0.04, 0.05), (0.0, 0.06)], 8, black, axis="z").translated((-0.45, REAR_Y + 0.42, 0.70)))
    horn = V.bezier((0.30, hy1 - 0.02, 1.02), (0.30, hy1 - 0.25, 1.02), (0.32, FRONT_Y - 0.05, 0.98), (0.33, FRONT_Y - 0.15, 0.96), 8)
    m.extend(V.sweep(horn, V.circle(0.02, 8), brass, (0.3, 0.1), up=(1, 0, 0), scales=[0.6 + 0.6 * (k / 8) ** 3 for k in range(9)]))
    m.extend(V.lathe([(0.0, 0.0), (0.04, 0.02), (0.035, 0.08), (0.0, 0.09)], 10, "Rubber", axis="z").translated((0.40, hy1 + 0.05, 1.07)))
    # The folding top: bows, the leatherette over them, the straps to the
    # windshield.
    ty0, ty1, tz = wy + 0.02, by1 + 0.05, 1.98
    for y in (0.10, 0.75, ty1 - 0.25):
        bow = [(0.63 * math.cos(math.pi * k / 10), y, 1.30 + (tz - 1.30) * min(1.0, 1.4 * math.sin(math.pi * k / 10))) for k in range(11)]
        m.extend(V.rod(bow, 0.012, "Wood", n=6))

    def top(u, w):
        y = ty0 + (ty1 - 0.25 - ty0) * u
        x = 0.64 * (1 - 2 * w)
        sag = 0.02 * math.sin(math.pi * ((u * 2) % 1.0))
        return (x, y, tz + 0.04 - 0.05 * (2 * w - 1) ** 2 - sag)

    m.extend(V.thick(V.surface(top, 16, 8, leather, (1.0, 1.0)), 0.008))
    m.extend(V.thick(V.surface(lambda u, w: (0.64 * (1 - 2 * u), ty1 - 0.25 + 0.26 * w, tz + 0.04 - 0.62 * w - 0.05 * (2 * u - 1) ** 2), 10, 4, leather, (1.0, 1.0)), 0.008))
    for sx in (1.0, -1.0):
        m.extend(V.rod([(0.60 * sx, ty0 + 0.02, tz), (0.58 * sx, wy, 1.82)], 0.006, leather, n=4))
    v.markers["Seat"] = SEAT
    v.markers["Steering"] = STEERING
    v.extras = {"animals": 0}
    return v


PREVIEW = [
    ((6.2, 0.2, 1.3), (0.0, 0.0, 0.95), 35),
    ((4.4, -4.2, 2.0), (0.0, -0.2, 0.95), 35),
    ((-3.0, 5.0, 2.0), (0.0, 0.3, 1.0), 35),
]

if __name__ == "__main__" and V.bpy is not None:
    argv = V.args()
    V.reset()
    v = build()
    V.instantiate(v)
    V.export(argv[0])
    print(f"ford_t: {v.triangles()} triangles, materials {v.materials()}")
    sheet = V.option(argv, "--preview")
    if sheet:
        V.render_sheet(sheet, PREVIEW)
