"""Build the carreta (1919 La Pampa) in Blender, by script.

    blender -b --factory-startup --python tools/carreta.py -- OUT.glb [--preview SHEET.png]

The pampa's carreta: the big two-wheeled ox cart that hauled wool, hides
and grain to the railway stations, drawn by a yoke of oxen (a yunta)
under a head yoke tied to the horns, its carretero walking beside with
the picana. After the carretas tucumanas and mendocinas of the 19th
century, still on the roads of the young territory in 1919 (Bossi, "La
carreta", 1960; the carretas in the photographs of the Archivo General
de la Nación). At period sizes:

- two very tall wheels, 2.20 m across, of hardwood (algarrobo or
  quebracho), twelve heavy spokes in a massive turned nave (the maza)
  bound with iron, a deep wooden felloe without an iron tyre, on a square
  axle (eje) of hardwood under the floor;
- the floor frame (lecho): two side beams and, in the middle, the pole
  (pértigo), one long beam that runs the whole length of the box and on
  forward, tapering, to the barzón under the middle of the yoke, with
  crossbars and floor boards;
- the box, 3.8 m long and 1.5 m wide: a row of stakes each side, walls of
  cane (quincha) between them and a top rail, and over it the arched roof
  of bent hoops covered with sewn rawhides, overhanging at each end;
- the picana (a long cane goad with its iron point) hung along the roof,
  a rawhide bucket under the back and the coyunda wrapped round the pole's
  end.

The origin is the ground midway between the two oxen (``Animal.0`` the
left ox, ``Animal.1`` the right, each where ``tools/cattle.py``'s bull
stands, scaled to the game's ox); the yoke is ``tools/vehicle_harness.py``.
The vehicle faces -Y, its left is +X. See ``tools/vehicle.py``.
"""

import math
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)

import vehicle as V  # noqa: E402
import vehicle_harness as H  # noqa: E402

WHEEL_RADIUS = 1.10
AXLE_Y = 4.00
HUB_X = 1.02
BOX = (2.10, 5.90)  # front and back of the box, along Y
HALF = 0.72  # the box's half width, to the side beams' middle
FLOOR = 1.30
WALL_TOP = 2.15
OX_X = H.OX_SPACING * H.BULL_SCALE / 2
YOKE_TIE = (0.0, H.YOKE[1] * H.BULL_SCALE, (H.YOKE[2] - 0.10) * H.BULL_SCALE)
SEAT = (0.0, BOX[0] + 0.12, FLOOR + 0.12)


def build():
    v = V.Vehicle("Carreta")
    m = v.body
    wood, iron, hide, cane = "Wood", "Iron", "Hide", "Cane"
    for name, sx in (("Wheel.L", 1.0), ("Wheel.R", -1.0)):
        w = V.wheel(
            WHEEL_RADIUS,
            12,
            out=sx,
            hub=(0.20, 0.52),
            felloe=(0.13, 0.10),
            tyre=None,
            spoke=(0.095, 0.065),
            dish=0.045,
            stagger=0.03,
            wood=wood,
            iron=iron,
            segments=84,
        )
        v.add_wheel(name, (HUB_X * sx, AXLE_Y, WHEEL_RADIUS), WHEEL_RADIUS, w)
    z_axle = WHEEL_RADIUS
    # The axle, square and massive, its ends round into the naves.
    m.extend(V.beam((-0.80, AXLE_Y, z_axle), (0.80, AXLE_Y, z_axle), 0.17, 0.17, wood, chamfer=0.02, up=(0, 1, 0)))
    for sx in (1.0, -1.0):
        m.extend(V.rod([(0.78 * sx, AXLE_Y, z_axle), (1.30 * sx, AXLE_Y, z_axle)], 0.06, iron, n=12))
        # The linchpin through the axle's end.
        m.extend(V.beam((1.32 * sx, AXLE_Y, z_axle - 0.10), (1.32 * sx, AXLE_Y, z_axle + 0.10), 0.025, 0.03, iron))
    # The floor frame: side beams on the axle, the pole down the middle and
    # on to the yoke, crossbars, and the floor boards.
    fz = FLOOR - 0.08
    for sx in (1.0, -1.0):
        m.extend(V.beam((HALF * sx, BOX[0], fz), (HALF * sx, BOX[1], fz), 0.12, 0.15, wood, chamfer=0.015))
    pole = [(0.0, BOX[1] + 0.05, fz), (0.0, BOX[0], fz), (0.0, 1.0, fz + 0.04), YOKE_TIE]
    m.extend(V.sweep(pole, V.rect(0.15, 0.15, 0.02), wood, (1.5, 0.4), scales=[1.0, 1.0, 0.85, 0.62]))
    for k in range(10):
        y = BOX[0] + 0.08 + (BOX[1] - BOX[0] - 0.16) * k / 9
        m.extend(V.beam((-HALF - 0.08, y, fz + 0.10), (HALF + 0.08, y, fz + 0.10), 0.09, 0.07, wood, chamfer=0.012, up=(0, 1, 0)))
    for k in range(8):
        x = -HALF + 0.05 + (2 * HALF - 0.10) * (k + 0.5) / 8
        m.extend(V.beam((x, BOX[0] - 0.02, FLOOR + 0.02), (x, BOX[1] + 0.02, FLOOR + 0.02), 0.17, 0.035, wood, chamfer=0.006))
    # The coyunda round the pole's end, tied to the barzón.
    tip = YOKE_TIE
    for k in range(6):
        y = tip[1] + 0.04 + 0.03 * k
        ring = [(0.085 * math.cos(t), y, tip[2] + 0.085 * math.sin(t)) for t in [2 * math.pi * j / 10 for j in range(10)]]
        m.extend(V.rod(ring, 0.009, hide, n=5, closed=True, caps=False))
    # Stakes, cane walls and top rails.
    stakes = 9
    for sx in (1.0, -1.0):
        for k in range(stakes):
            y = BOX[0] + 0.06 + (BOX[1] - BOX[0] - 0.12) * k / (stakes - 1)
            m.extend(V.beam((HALF * sx, y, fz), (HALF * sx, y, WALL_TOP + 0.04), 0.07, 0.07, wood, chamfer=0.012, up=(0, 1, 0)))
        wall = V.beam((HALF * sx - 0.005 * sx, BOX[0] + 0.06, FLOOR + 0.48), (HALF * sx - 0.005 * sx, BOX[1] - 0.06, FLOOR + 0.48),
                      0.035, WALL_TOP - FLOOR - 0.06, cane, chamfer=0.0, tile=(1.0, 0.55))
        m.extend(wall)
        m.extend(V.beam((HALF * sx, BOX[0] - 0.05, WALL_TOP), (HALF * sx, BOX[1] + 0.05, WALL_TOP), 0.08, 0.07, wood, chamfer=0.012))
        # Lashings of the cane to each stake.
        for k in range(stakes):
            y = BOX[0] + 0.06 + (BOX[1] - BOX[0] - 0.12) * k / (stakes - 1)
            for z in (FLOOR + 0.25, FLOOR + 0.65):
                loop = [(HALF * sx + 0.05 * math.cos(t), y + 0.05 * math.sin(t), z) for t in [2 * math.pi * j / 8 for j in range(8)]]
                m.extend(V.rod(loop, 0.007, hide, n=4, closed=True, caps=False))
    # A low front board, the back left open under the hide flap.
    m.extend(V.beam((-HALF, BOX[0] + 0.03, FLOOR + 0.17), (HALF, BOX[0] + 0.03, FLOOR + 0.17), 0.04, 0.30, wood, up=(0, 1, 0)))
    # The roof: hoops at every stake, the hides over them sagging a little
    # between, overhanging front and back.
    rise = 0.62

    def arch(t):
        a = math.pi * t
        return (HALF * math.cos(a), WALL_TOP + rise * math.sin(a))

    for k in range(stakes):
        y = BOX[0] + 0.06 + (BOX[1] - BOX[0] - 0.12) * k / (stakes - 1)
        hoop = [(arch(j / 16)[0], y, arch(j / 16)[1] + 0.02) for j in range(17)]
        m.extend(V.rod(hoop, 0.022, wood, n=8))
    y0, y1 = BOX[0] - 0.35, BOX[1] + 0.25

    def roof(u, w):
        y = y0 + (y1 - y0) * u
        x, z = arch(w)
        # Sag between hoops; the overhang droops.
        span = (BOX[1] - BOX[0] - 0.12) / (stakes - 1)
        phase = ((y - BOX[0] - 0.06) / span) % 1.0
        inside = BOX[0] < y < BOX[1]
        sag = 0.035 * math.sin(math.pi * phase) if inside else 0.0
        droop = 0.0 if inside else 0.25 * min(abs(y - BOX[0]), abs(y - BOX[1])) ** 1.2
        scale = 1.0 + 0.04 if not inside else 1.0
        return (x * scale * (1.0 - 0.02 * sag), y, z + 0.045 - sag - droop)

    m.extend(V.thick(V.surface(roof, 72, 18, hide, (1.4, 1.2)), 0.012))
    # The rolled hide flap over the back.
    flap = [(HALF * 0.9 * math.cos(math.pi * j / 12), BOX[1] + 0.2, WALL_TOP + rise * 0.9 * math.sin(math.pi * j / 12) - 0.05)
            for j in range(13)]
    m.extend(V.rod(flap, 0.05, hide, n=10, tile=(0.6, 0.3)))
    # The picana, hung along the left of the roof on two hide loops.
    pic = [(HALF + 0.10, BOX[0] - 0.9, WALL_TOP + 0.10), (HALF + 0.10, BOX[1] - 0.3, WALL_TOP + 0.02)]
    m.extend(V.sweep(pic, V.circle(0.018, 8), cane, (0.5, 0.2), up=(1, 0, 0), scales=[0.7, 1.0]))
    m.extend(V.lathe([(0.0, 0.0), (0.012, 0.0), (0.0, -0.12)], 6, iron).moved(lambda p: (pic[0][0] + p[0], pic[0][1] + p[2], pic[0][2] + p[1])))
    # The bucket under the back, hung from the axle by a thong.
    bx, by, bz = 0.35, BOX[1] - 0.45, 0.55
    m.extend(V.lathe([(0.0, bz), (0.13, bz), (0.15, bz + 0.25), (0.14, bz + 0.27)], 14, hide, axis="z").translated((bx, by, 0)))
    m.extend(V.rod([(bx, by, bz + 0.27), (bx, by, fz)], 0.006, hide, n=4))
    v.markers["Seat"] = SEAT
    v.markers["Animal.0"] = (OX_X, 0.0, 0.0)
    v.markers["Animal.1"] = (-OX_X, 0.0, 0.0)
    v.extras = {"animals": 2, "animal": "ox"}
    return v


PREVIEW = [
    ((11.5, 2.8, 2.2), (0.0, 2.6, 1.4), 30),
    ((7.6, -6.0, 3.0), (0.0, 2.0, 1.4), 30),
    ((-5.0, 11.5, 3.0), (0.0, 3.2, 1.5), 32),
]

if __name__ == "__main__" and V.bpy is not None:
    argv = V.args()
    V.reset()
    v = build()
    V.instantiate(v)
    V.export(argv[0])
    print(f"carreta: {v.triangles()} triangles, materials {v.materials()}")
    sheet = V.option(argv, "--preview")
    if sheet:
        H.preview_ox((OX_X, 0.0, 0.0), True)
        H.preview_ox((-OX_X, 0.0, 0.0), coat="overo.jpg")
        V.render_sheet(sheet, PREVIEW)
