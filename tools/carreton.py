"""Build the carretón (1919 La Pampa) in Blender, by script.

    blender -b --factory-startup --python tools/carreton.py -- OUT.glb [--preview SHEET.png]

The carretón: the four-wheeled farm wagon of the chacras and the general
stores, drawn by a pair of horses abreast on a pole, that brought the
bags of wheat to the station and the stores' goods back (the "carro" or
"carretón" of the colonists, built in the towns by Italian and Spanish
wheelwrights after the European and American farm wagon). At period
sizes:

- front wheels 1.00 m and rear wheels 1.32 m over their iron tyres, twelve
  and fourteen staggered spokes, painted Indian red like the running gear;
- the running gear: a front axle under a bolster on the iron fifth wheel
  (the turntable that lets the front axle steer), a rear axle and bolster,
  the reach between them and the hounds bracing each axle;
- the box: 3.3 m long, 1.24 m wide, three green planks high on iron
  stakes and straps, a tailboard, and the driver's spring seat across its
  front with a backrest and a sloping footboard;
- the pole (lanza) between the two horses, the neck yoke at its tip, the
  doubletree and two singletrees, the traces to each horse's breast collar
  and the four lines from the bits to the driver's hands (static: the
  harness on the horses is ``tools/vehicle_harness.py``);
- the brake: a lever by the seat, its rod and the beam with a block
  against each rear tyre.

The origin is the ground midway between the two horses (``Animal.0`` the
left horse, ``Animal.1`` the right). The vehicle faces -Y, its left is +X.
See ``tools/vehicle.py``.
"""

import math
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)

import vehicle as V  # noqa: E402
import vehicle_harness as H  # noqa: E402

FRONT_R, REAR_R = 0.50, 0.66
FRONT_Y, REAR_Y = 1.95, 4.25
HUB_X = 0.80
HORSE_X = 0.62
BOX = (1.55, 4.85)
HALF = 0.62
FLOOR = 1.10
SEAT = (0.0, 1.88, 1.74)


def build():
    v = V.Vehicle("Carreton")
    m = v.body
    red, green, iron, wood, leather = "Paint_Red", "Paint_Green", "Iron", "Wood", "Leather"
    for name, y, r, spokes, sx in (("Wheel.FL", FRONT_Y, FRONT_R, 12, 1.0), ("Wheel.FR", FRONT_Y, FRONT_R, 12, -1.0),
                                   ("Wheel.RL", REAR_Y, REAR_R, 14, 1.0), ("Wheel.RR", REAR_Y, REAR_R, 14, -1.0)):
        w = V.wheel(r, spokes, out=sx, hub=(0.085, 0.26), felloe=(0.055, 0.055), tyre=(0.012, 0.058),
                    spoke=(0.050, 0.034), dish=0.03, stagger=0.012, wood=red, iron=iron)
        v.add_wheel(name, (HUB_X * sx, y, r), r, w)
    # Axles, bolsters, the fifth wheel, the reach and the hounds.
    for y, r in ((FRONT_Y, FRONT_R), (REAR_Y, REAR_R)):
        m.extend(V.beam((-0.74, y, r), (0.74, y, r), 0.10, 0.11, red, chamfer=0.012, up=(0, 1, 0)))
        for sx in (1.0, -1.0):
            m.extend(V.rod([(0.70 * sx, y, r), (0.95 * sx, y, r)], 0.03, iron, n=10))
        top = FLOOR - 0.07
        m.extend(V.beam((-HALF - 0.04, y, top - 0.06), (HALF + 0.04, y, top - 0.06), 0.10, 0.10, red, chamfer=0.012, up=(0, 1, 0)))
        if r == FRONT_R:
            m.extend(V.beam((-0.58, y, r + 0.10), (0.58, y, r + 0.10), 0.09, 0.09, red, chamfer=0.012, up=(0, 1, 0)))
            ring = [(0.30 * math.cos(t), y + 0.30 * math.sin(t), r + 0.17) for t in [2 * math.pi * k / 24 for k in range(24)]]
            m.extend(V.rod(ring, 0.012, iron, n=6, closed=True, caps=False))
            m.extend(V.rod([(0, y, r + 0.12), (0, y, top - 0.10)], 0.025, iron))
        for sx in (1.0, -1.0):
            m.extend(V.beam((HALF * sx, y, top - 0.10), (HALF * sx, y, top + 0.40), 0.035, 0.03, iron, up=(0, 1, 0)))
    m.extend(V.beam((0.0, FRONT_Y, FRONT_R + 0.02), (0.0, REAR_Y + 0.45, REAR_R + 0.02), 0.08, 0.07, red, chamfer=0.01))
    for sx in (1.0, -1.0):
        m.extend(V.beam((0.55 * sx, REAR_Y, REAR_R), (0.0, REAR_Y - 0.95, REAR_R + 0.01), 0.05, 0.05, red))
        m.extend(V.beam((0.50 * sx, FRONT_Y, FRONT_R), (0.0, FRONT_Y + 0.75, FRONT_R + 0.01), 0.05, 0.05, red))
        m.extend(V.beam((0.40 * sx, FRONT_Y, FRONT_R), (0.07 * sx, FRONT_Y - 0.80, 0.62), 0.05, 0.05, red))
    # The box: floor, sides of three planks, stakes and straps, ends.
    for k in range(7):
        x = -HALF + 2 * HALF * (k + 0.5) / 7
        m.extend(V.beam((x, BOX[0], FLOOR), (x, BOX[1], FLOOR), 2 * HALF / 7 - 0.004, 0.03, wood, chamfer=0.004))
    for sx in (1.0, -1.0):
        m.extend(V.beam((HALF * sx, BOX[0], FLOOR - 0.05), (HALF * sx, BOX[1], FLOOR - 0.05), 0.07, 0.09, green))
        for k in range(3):
            z = FLOOR + 0.09 + 0.155 * k
            m.extend(V.beam((HALF * sx, BOX[0], z), (HALF * sx, BOX[1], z), 0.03, 0.15, green, chamfer=0.005))
        for k in range(5):
            y = BOX[0] + 0.05 + (BOX[1] - BOX[0] - 0.1) * k / 4
            m.extend(V.beam((HALF * sx + 0.025 * sx, y, FLOOR - 0.08), (HALF * sx + 0.025 * sx, y, FLOOR + 0.50), 0.02, 0.05, iron, up=(0, 1, 0)))
        m.extend(V.beam((HALF * sx, BOX[0] - 0.02, FLOOR + 0.49), (HALF * sx, BOX[1] + 0.02, FLOOR + 0.49), 0.05, 0.03, red))
    for y in (BOX[0], BOX[1]):
        for k in range(3):
            z = FLOOR + 0.09 + 0.155 * k
            m.extend(V.beam((-HALF, y, z), (HALF, y, z), 0.03, 0.15, green, chamfer=0.005, up=(0, 1, 0)))
        m.extend(V.rod([(-HALF - 0.02, y + (0.03 if y == BOX[1] else -0.03), FLOOR + 0.40),
                        (HALF + 0.02, y + (0.03 if y == BOX[1] else -0.03), FLOOR + 0.40)], 0.008, iron))
    # The spring seat, its backrest and the footboard.
    sy, sz = SEAT[1], SEAT[2]
    for sx in (1.0, -1.0):
        for sign in (-1.0, 1.0):
            pts = [(0.48 * sx, sy + 0.18 * t, sz - 0.18 - sign * 0.045 * (1 - t * t)) for t in [-1 + k / 4 for k in range(9)]]
            m.extend(V.sweep(pts, V.rect(0.04, 0.008, 0.002), iron, (0.4, 0.1)))
        m.extend(V.beam((0.48 * sx, sy, FLOOR + 0.48), (0.48 * sx, sy, sz - 0.22), 0.04, 0.05, red, up=(0, 1, 0)))
    m.extend(V.beam((-0.60, sy, sz - 0.04), (0.60, sy, sz - 0.04), 0.34, 0.05, green, chamfer=0.01, up=(0, 0, 1)))
    m.extend(V.beam((-0.56, sy - 0.02, sz + 0.005), (0.56, sy - 0.02, sz + 0.005), 0.30, 0.03, leather, chamfer=0.012, up=(0, 0, 1)))
    for sx in (1.0, -1.0):
        m.extend(V.rod([(0.55 * sx, sy + 0.15, sz - 0.04), (0.57 * sx, sy + 0.20, sz + 0.30)], 0.012, iron))
    m.extend(V.beam((-0.58, sy + 0.20, sz + 0.24), (0.58, sy + 0.20, sz + 0.24), 0.13, 0.03, green, chamfer=0.008, up=(0, 1, 0)))
    fb0, fb1 = (BOX[0], FLOOR + 0.48), (BOX[0] - 0.42, FLOOR + 0.28)
    for k in range(5):
        t = (k + 0.5) / 5
        y = fb0[0] + (fb1[0] - fb0[0]) * t
        z = fb0[1] + (fb1[1] - fb0[1]) * t
        m.extend(V.beam((-0.50, y, z), (0.50, y, z), 0.085, 0.02, green, chamfer=0.004, up=(0, 0, 1)))
    for sx in (1.0, -1.0):
        m.extend(V.rod([(0.50 * sx, fb1[0], fb1[1]), (0.55 * sx, BOX[0], FLOOR - 0.04)], 0.01, iron))
    # The brake: lever, rod, beam and blocks.
    m.extend(V.rod([(-HALF - 0.06, sy + 0.05, FLOOR + 0.15), (-HALF - 0.10, sy - 0.05, sz + 0.25)], 0.015, iron))
    m.extend(V.rod([(-HALF - 0.06, sy + 0.05, FLOOR + 0.10), (-HALF - 0.06, REAR_Y - REAR_R - 0.02, REAR_R + 0.05)], 0.009, iron))
    by = REAR_Y - REAR_R - 0.06
    m.extend(V.beam((-0.86, by, REAR_R + 0.05), (0.86, by, REAR_R + 0.05), 0.07, 0.07, red, up=(0, 1, 0)))
    for sx in (1.0, -1.0):
        m.extend(V.beam((HUB_X * sx + 0.03 * sx, by + 0.03, REAR_R + 0.01), (HUB_X * sx + 0.03 * sx, by + 0.03, REAR_R + 0.18),
                        0.08, 0.06, wood, up=(0, 1, 0)))
    # The pole, neck yoke, doubletree and singletrees.
    pole_tip = (0.0, -0.88, 1.02)
    pole = [(0.0, FRONT_Y - 0.10, 0.63), (0.0, 1.0, 0.70), (0.0, 0.0, 0.88), pole_tip]
    m.extend(V.sweep(pole, V.rect(0.08, 0.09, 0.012), red, (1.2, 0.3), scales=[1.0, 0.95, 0.85, 0.7]))
    m.extend(V.rod([V.add(pole_tip, (0, -0.06, 0)), V.add(pole_tip, (0, 0.06, 0))], 0.04, iron, n=10))
    m.extend(V.sweep([(-0.48, -0.86, 1.05), (0.0, -0.88, 1.08), (0.48, -0.86, 1.05)], V.ellipse(0.06, 0.055, 10), wood, (0.8, 0.15),
                     up=(0, 1, 0), scales=[0.7, 1.0, 0.7]))
    for sx in (1.0, -1.0):
        a = (0.48 * sx, -0.86, 1.05)
        b = ((HORSE_X - H.TRACE[0] - 0.02) * sx, -0.85 + 0.0, 1.11)
        m.extend(V.sweep([a, V.lerp(a, b, 0.5), b], V.rect(0.03, 0.008, 0.002), leather, (0.4, 0.05), up=(0, 1, 0)))
    dt_y, dt_z = 1.30, 0.80
    m.extend(V.sweep([(-0.62, dt_y, dt_z), (0, dt_y, dt_z + 0.01), (0.62, dt_y, dt_z)], V.ellipse(0.07, 0.06, 10), wood, (0.8, 0.15),
                     up=(0, 1, 0), scales=[0.7, 1.0, 0.7]))
    for sx in (1.0, -1.0):
        cx = HORSE_X * sx
        m.extend(V.sweep([(cx - 0.34, dt_y - 0.10, dt_z + 0.15), (cx, dt_y - 0.10, dt_z + 0.16), (cx + 0.34, dt_y - 0.10, dt_z + 0.15)],
                         V.ellipse(0.05, 0.045, 10), wood, (0.7, 0.15), up=(0, 1, 0), scales=[0.7, 1.0, 0.7]))
        m.extend(V.rod([(0.62 * sx, dt_y, dt_z), (cx, dt_y - 0.10, dt_z + 0.15)], 0.01, iron))
        for side in (1.0, -1.0):
            a = (cx + H.TRACE[0] * side, H.TRACE[1], H.TRACE[2])
            b = (cx + 0.34 * side, dt_y - 0.12, dt_z + 0.15)
            mid = V.lerp(a, b, 0.5)
            flank = (cx + (H.SIDE[0] + 0.025) * side, mid[1], mid[2] - 0.05)
            trace = V.bezier(a, V.lerp(a, flank, 0.6), V.lerp(flank, b, 0.4), b, 14)
            m.extend(V.sweep(trace, V.rect(0.035, 0.008, 0.002), leather, (0.4, 0.05), up=(1, 0, 0)))
    # The lines: from each bit, through the pad's terrets, to the hands.
    for sx in (1.0, -1.0):
        cx = HORSE_X * sx
        for side in (1.0, -1.0):
            bit = (cx + H.BIT[0] * side, H.BIT[1], H.BIT[2])
            terret = (cx + H.TERRET[0] * side, H.TERRET[1], H.TERRET[2] + 0.02)
            hand = (0.12 * (1 if side * sx > 0 else -1) * (1 if sx > 0 else 1), sy - 0.45, sz + 0.18)
            line = V.bezier(bit, V.add(bit, (0, 0.35, -0.12)), V.add(terret, (0, -0.4, -0.06)), terret, 8)
            line += V.bezier(terret, V.add(terret, (0, 0.8, -0.10)), V.add(hand, (0, -0.8, -0.20)), hand, 12)[1:]
            m.extend(V.sweep(line, V.rect(0.005, 0.02, 0.0015), leather, (0.4, 0.05), up=(1, 0, 0)))
    v.markers["Seat"] = SEAT
    v.markers["Animal.0"] = (HORSE_X, 0.0, 0.0)
    v.markers["Animal.1"] = (-HORSE_X, 0.0, 0.0)
    v.extras = {"animals": 2, "animal": "horse"}
    return v


PREVIEW = [
    ((8.8, 2.0, 1.8), (0.0, 1.9, 1.0), 32),
    ((6.2, -5.0, 2.8), (0.0, 1.6, 1.0), 32),
    ((-4.0, 9.0, 2.8), (0.0, 2.5, 1.1), 34),
]

if __name__ == "__main__" and V.bpy is not None:
    argv = V.args()
    V.reset()
    v = build()
    V.instantiate(v)
    V.export(argv[0])
    print(f"carreton: {v.triangles()} triangles, materials {v.materials()}")
    sheet = V.option(argv, "--preview")
    if sheet:
        H.preview_horse((HORSE_X, 0.0, 0.0))
        H.preview_horse((-HORSE_X, 0.0, 0.0))
        V.render_sheet(sheet, PREVIEW)
