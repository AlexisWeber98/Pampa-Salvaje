"""Model, rig and animate the greater rhea (ñandú, Rhea americana) in Blender.

Run it headless:

    blender -b --factory-startup --python tools/rhea.py -- OUT.glb \
        [--textures DIR] [--preview DIR] [--look v13|v12|v8] [--no-clips]

Writes OUT.glb: one skinned mesh with two materials, ``Body`` (5,200
triangles, unwrapped) and ``Feathers`` (324 alpha-tested feather cards, two
segments each), 6,496 triangles in all; an armature of 20 bones; and the
clips ``Idle``, ``Idle_Look``, ``Graze``, ``Walk`` and ``Run``, each an
in-place loop (the exporter sorts them by name: ``tools/glbtrim.py --keep``
puts them in the game's order). ``--textures DIR`` writes the JPEG sources
of the two KTX2 sets, ``DIR/rhea_body`` (baked from a dense sculpt) and
``DIR/rhea_feathers`` (the feather atlas); ``tools/textures_to_ktx2.sh``
turns them into the maps the game loads. ``--preview DIR`` renders PNGs
(side, three-quarter front and rear, close-ups at 2 to 3 m, and frames of
the walk, run, graze and alert idle). ``--look`` picks a set of coat and
feather knobs (``LOOKS``): ``v13`` (default, the shipped model and the floor
every new model must reach), ``v12`` or ``v8``. Helpers live in ``tools/animal_rig.py``. See
tools/README.md.

Anatomy, in metres (the bird stands at the origin, facing -Y): the back
about 1.0 m high, the top of the head about 1.5 m; a big rounded body with
loose plumage hanging over the thighs and drooping wing plumes on its
sides, no tail; long thick grey legs with three forward toes; a long neck,
thick and feathered at its base, in a forward S curve; a small flattish
head with a short broad bill. Grey-brown, paler underneath, with the
male's black base of the neck and upper breast.
"""

import math
import os
import sys

import bpy
from mathutils import Matrix, Vector

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import animal_rig as rig  # noqa: E402

# Bones: name, head, tail, parent. Legs and wings are mirrored for ``.R``.
SPINE = [
    ("Body", (0, 0.30, 0.84), (0, -0.22, 0.86), None),
    ("Rump", (0, 0.36, 0.88), (0, 0.50, 0.68), "Body"),
    ("Neck1", (0, -0.30, 0.91), (0, -0.46, 1.03), "Body"),
    ("Neck2", (0, -0.46, 1.03), (0, -0.54, 1.20), "Neck1"),
    ("Neck3", (0, -0.54, 1.20), (0, -0.545, 1.40), "Neck2"),
    ("Head", (0, -0.545, 1.40), (0, -0.70, 1.415), "Neck3"),
]
LEG = [
    ("Thigh", (0.12, 0.06, 0.80), (0.13, -0.05, 0.58), "Body"),
    ("Shank", (0.13, -0.05, 0.58), (0.12, 0.10, 0.31), "Thigh"),
    ("Tarsus", (0.12, 0.10, 0.31), (0.12, 0.03, 0.045), "Shank"),
    ("Toe", (0.12, 0.03, 0.045), (0.12, -0.13, 0.015), "Tarsus"),
]
SKIRT = [
    ("Skirt", (0.22, 0.10, 0.74), (0.27, 0.16, 0.55), "Body"),
]
WING = [
    ("Wing", (0.20, -0.20, 0.93), (0.25, 0.10, 0.82), "Body"),
    ("Plume", (0.25, 0.10, 0.82), (0.27, 0.36, 0.66), "Wing"),
]

# The design coordinates above put the knee at 0.58 m; ``warp`` stretches
# the legs and lifts the rest of the bird by RAISE, so the leg length can be
# tuned in one place.
KNEE = 0.58
RAISE = 0.09


def warp(p):
    x, y, z = p
    return (x, y, z + RAISE * min(1.0, max(0.0, z) / KNEE))


def unwarp(p):
    x, y, z = p
    low = z * KNEE / (KNEE + RAISE)
    return (x, y, low if low < KNEE else z - RAISE)


GROUND = warp((0, 0, 0.045))[2]  # the height of the ankle (tarsus tail)


def bones():
    out = list(SPINE)
    for side, sx in (("L", 1), ("R", -1)):
        for name, h, t, parent in LEG + WING + SKIRT:
            p = parent if parent == "Body" else f"{parent}.{side}"
            out.append((f"{name}.{side}", (sx * h[0], h[1], h[2]), (sx * t[0], t[1], t[2]), p))
    return [(name, warp(h), warp(t), parent) for name, h, t, parent in out]


def body_parts():
    """The parts, before they are fused into one surface."""
    parts = []
    # The body: a big egg, highest over the shoulders and rounding down to the
    # rump, narrower at the breast.
    parts.append(
        rig.loft(
            "body",
            [(0, 0.58, 0.76), (0, 0.47, 0.80), (0, 0.30, 0.82), (0, 0.08, 0.83), (0, -0.12, 0.84), (0, -0.28, 0.87), (0, -0.36, 0.91)],
            [(0.08, 0.07), (0.22, 0.17), (0.29, 0.22), (0.31, 0.25), (0.28, 0.25), (0.19, 0.18), (0.12, 0.12)],
            ring=24,
        )
    )
    # The neck: carried forward in a gentle S, thick and shaggy in its lower
    # half, slim near the head.
    parts.append(
        rig.loft(
            "neck",
            [(0, -0.24, 0.87), (0, -0.38, 0.95), (0, -0.48, 1.05), (0, -0.53, 1.17), (0, -0.545, 1.29), (0, -0.54, 1.37), (0, -0.55, 1.42)],
            [(0.13, 0.14), (0.105, 0.11), (0.08, 0.085), (0.058, 0.062), (0.045, 0.047), (0.04, 0.042), (0.04, 0.042)],
        )
    )
    # A small flat head, a big dark eye on each side, and a short, broad,
    # slightly hooked bill.
    parts.append(
        rig.loft(
            "head",
            [(0, -0.51, 1.425), (0, -0.56, 1.44), (0, -0.605, 1.435)],
            [(0.042, 0.03), (0.046, 0.033), (0.033, 0.024)],
            ring=20,
        )
    )
    for sx in (1, -1):
        parts.append(rig.loft("eye", [(sx * 0.036, -0.572, 1.446), (sx * 0.044, -0.574, 1.446)], [(0.011, 0.011), (0.011, 0.011)], ring=10, samples=2))
    parts.append(
        rig.loft(
            "bill",
            [(0, -0.595, 1.432), (0, -0.64, 1.428), (0, -0.675, 1.42), (0, -0.69, 1.411)],
            [(0.026, 0.012), (0.022, 0.010), (0.014, 0.008), (0.007, 0.006)],
            ring=12,
        )
    )
    for sx in (1, -1):
        # Legs: the drumstick hidden in the plumage, then the thick bare
        # shank, the knobbly heel and the long tarsus.
        parts.append(
            rig.loft(
                "thigh",
                [(sx * 0.12, 0.06, 0.80), (sx * 0.13, -0.02, 0.66), (sx * 0.13, -0.04, 0.58)],
                [(0.09, 0.10), (0.08, 0.085), (0.06, 0.06)],
            )
        )
        parts.append(
            rig.loft(
                "shank",
                [(sx * 0.13, -0.05, 0.60), (sx * 0.125, 0.03, 0.45), (sx * 0.12, 0.10, 0.31)],
                [(0.075, 0.08), (0.05, 0.056), (0.037, 0.041)],
            )
        )
        parts.append(
            rig.loft(
                "tarsus",
                [(sx * 0.12, 0.10, 0.31), (sx * 0.12, 0.065, 0.17), (sx * 0.12, 0.03, 0.045)],
                [(0.033, 0.038), (0.026, 0.029), (0.026, 0.028)],
                ring=12,
            )
        )
        # Three long forward toes, the middle one longest, knobbly at the
        # joints, ending in a short claw.
        for dx, reach in ((0.0, 0.17), (0.06, 0.12), (-0.055, 0.12)):
            path = [(sx * (0.12 + dx * f), 0.03 - (reach + 0.03) * f, 0.032 - 0.02 * f) for f in (0.0, 0.33, 0.5, 0.67, 0.85, 1.0)]
            parts.append(
                rig.loft(
                    "toe",
                    path,
                    [(0.022, 0.02), (0.019, 0.016), (0.016, 0.013), (0.0175, 0.0145), (0.013, 0.011), (0.006, 0.005)],
                    ring=10,
                    samples=2,
                )
            )
    parts += plumage()
    for part in parts:
        for v in part.data.vertices:
            v.co = warp(v.co)
    return parts


def jitter(i, salt):
    """A deterministic pseudo-random number in [0, 1)."""
    return (math.sin(i * 12.9898 + salt * 78.233) * 43758.5453) % 1.0


def plumage():
    """Loose feather lobes: a shaggy skirt over the thighs, drooping wing
    plumes on the sides, and a bustle over the rump."""
    lobes = []
    # The skirt: broad overlapping lobes hanging from the lower edge of the
    # body, from the flanks round to the rump, longer behind.
    count = 18
    for sx in (1, -1):
        for i in range(count):
            f = i / (count - 1)
            ang = math.radians(-70 + 150 * f)  # round the body seen from above
            x = sx * 0.27 * math.cos(ang) * (1.0 - 0.25 * f)
            y = 0.10 + 0.40 * math.sin(ang)
            x += sx * 0.02 * jitter(i, sx)
            top = (x, y, 0.74 + 0.03 * jitter(i, 9 + sx))
            # Short in front, longest over the flanks, ragged.
            drop = 0.08 + 0.15 * math.sin(math.pi * (0.12 + 0.72 * f)) + 0.11 * jitter(i, 3 + sx)
            out = 1.0 + 0.10 * jitter(i, 5 + sx)
            mid = (x * (out + 0.05), y + 0.02 + 0.03 * f, 0.74 - drop * 0.55)
            low = (x * out, y + 0.05 * f + 0.03, 0.74 - drop)
            w = 0.07 + 0.03 * jitter(i, 7 + sx)
            lobes.append(rig.loft("skirt", [top, mid, low], [(0.028, w), (0.022, w * 0.9), (0.012, w * 0.6)], side=(math.cos(ang) * sx, math.sin(ang), 0), ring=8, samples=2))
    # Wing plumes: a fan of loose lobes from the shoulder drooping back over
    # the side, each ending at its own height.
    for sx in (1, -1):
        for k in range(6):
            f = k / 5
            y0 = -0.16 + 0.10 * f
            tip_y = 0.08 + 0.24 * f + 0.04 * jitter(k, 19 + sx)
            tip_z = 0.66 - 0.06 * math.sin(f * math.pi) - 0.05 * jitter(k, 23 + sx)
            pts = [
                (sx * 0.23, y0, 0.92 - 0.03 * f),
                (sx * (0.285 - 0.015 * f), (y0 + tip_y) * 0.5, 0.84 - 0.04 * f),
                (sx * (0.27 - 0.03 * f), tip_y, tip_z),
            ]
            w = 0.075 + 0.02 * jitter(k, 29 + sx)
            lobes.append(rig.loft("plume", pts, [(0.022, w), (0.026, w), (0.012, w * 0.6)], ring=10, samples=3))
    # The bustle: rump feathers hanging down behind.
    for k in range(9):
        x = (k - 4) * 0.055
        pts = [(x, 0.44, 0.97), (x * 1.1, 0.58, 0.82), (x * 1.15, 0.58, 0.58 + 0.05 * jitter(k, 11))]
        lobes.append(rig.loft("bustle", pts, [(0.065, 0.035), (0.06, 0.03), (0.035, 0.015)], ring=8, samples=2))
    # Shaggy back: a few raised feather tufts lying backwards.
    for k in range(8):
        x = (jitter(k, 13) - 0.5) * 0.36
        y = -0.10 + 0.45 * jitter(k, 17)
        z = 1.07 - 0.25 * max(0.0, y - 0.05) - 0.5 * x * x
        pts = [(x, y - 0.08, z), (x * 1.05, y + 0.06, z + 0.012), (x * 1.1, y + 0.18, z - 0.03)]
        lobes.append(rig.loft("tuft", pts, [(0.06, 0.02), (0.055, 0.02), (0.02, 0.008)], ring=8, samples=2))
    return lobes


# Looks: every knob of the coat and the feathers, by version. ``v8`` is the
# first textured rhea, which the user set as the floor for every later
# version ("ese nivel de detalle debe considerarse lo más bajo"): build it
# again with ``--look v8``. Colours are sRGB.
V8_PALETTE = dict(
    BACK=(0.44, 0.40, 0.34),
    FLANK=(0.58, 0.54, 0.48),
    DARK=(0.33, 0.30, 0.26),
    PALE=(0.80, 0.77, 0.71),
    TIPS=(0.66, 0.63, 0.58),
    WING=(0.33, 0.30, 0.26),
    BLACK=(0.09, 0.085, 0.08),
    SCALES=(0.52, 0.50, 0.46),
    SCALES_DARK=(0.36, 0.34, 0.31),
    BILL=(0.62, 0.56, 0.46),
    NECK_GREY=(0.68, 0.67, 0.64),
)
LOOKS = {
    "v8": dict(
        palette=V8_PALETTE,
        neck_edge="hard",  # the black collar ends at a straight edge
        extrusion=0.02,
        ao_through_sculpt=True,  # the sculpt shades the body: a patchy mottle
        ao_samples=64,
        atlas="v8",
        shafts="pale",
        wing_cards=((0, 0.8, -0.6), 0.085, (0.18, 0.28), "brown"),
        mottle=0.0,
        rows=0.45,
        wing_dark=0.0,
    ),
    "v12": dict(
        palette=dict(
            V8_PALETTE,
            BACK=(0.36, 0.32, 0.27),
            FLANK=(0.50, 0.46, 0.40),
            DARK=(0.26, 0.23, 0.19),
            PALE=(0.72, 0.69, 0.63),
            TIPS=(0.60, 0.57, 0.52),
            WING=(0.24, 0.21, 0.17),
            BILL=(0.42, 0.38, 0.32),
            NECK_GREY=(0.56, 0.55, 0.52),
        ),
        neck_edge="soft",
        extrusion=0.012,
        ao_through_sculpt=False,
        ao_samples=32,
        atlas="soft",
        shafts="matched",
        wing_cards=((0, 0.7, -0.7), 0.10, (0.15, 0.24), "dark"),
        # Feather-sized tone patches (10 to 25 cm), scale-like, darker
        # outlines between them.
        mottle=0.55,
        rows=0.2,
        # The drooping wing plumes darker, with paler ends: layered over the
        # body as in the earliest versions.
        wing_dark=0.75,
    ),
}
# v13: v12's coat with feathers told apart as in v8 ("la V8 tenía eso de que
# las plumas eran más identificables"): open barbs, a lighter rachis, darker
# vane edges, more and longer drooping wing plumes, tips lifted off the body.
LOOKS["v13"] = dict(
    LOOKS["v12"],
    atlas="plume",
    shafts="light",
    wing_cards=((0, 0.7, -0.7), 0.10, (0.20, 0.32), "dark"),
    card_counts=dict(skirt=120, wing=90, bustle=40, back=50, neck=24),
    card_lift=(0.08, 0.16),
    rows=0.3,
    # The dark crown and nape of Rhea americana.
    crown=1.5,
    nape=0.8,
)
LOOK = LOOKS["v13"]


def mix(a, b, t):
    t = max(0.0, min(1.0, t))
    return tuple(x * (1 - t) + y * t for x, y in zip(a, b))


LEG_LINE = [(0.13, -0.04, 0.62), (0.125, 0.03, 0.45), (0.12, 0.10, 0.31), (0.12, 0.065, 0.17), (0.12, 0.03, 0.045)]
EYES = [Vector((sx * 0.04, -0.573, 1.446)) for sx in (1, -1)]


def near_leg(p):
    """How far ``p`` (design coordinates) is from the bare leg's axis."""
    q = Vector((abs(p[0]), p[1], p[2]))
    best = 1e9
    for a, b in zip(LEG_LINE, LEG_LINE[1:]):
        a, b = Vector(a), Vector(b)
        t = max(0.0, min(1.0, (q - a).dot(b - a) / (b - a).length_squared))
        best = min(best, (q - (a + (b - a) * t)).length)
    return best


def colour_at(p, n):
    """The coat at ``p`` (warped coordinates) facing ``n``: sRGB colour and
    whether it is plumage (1) or bare skin, scales or bill (0)."""
    P = LOOK["palette"]
    BACK, FLANK, DARK, PALE, BLACK = P["BACK"], P["FLANK"], P["DARK"], P["PALE"], P["BLACK"]
    SCALES, SCALES_DARK, BILL, NECK_GREY = P["SCALES"], P["SCALES_DARK"], P["BILL"], P["NECK_GREY"]
    p = Vector(unwarp(p))
    x, y, z = p
    fleck = rig.noise(p, (40.0, 40.0, 40.0), seed=7.3)
    streak = rig.noise(p, (14.0, 8.0, 2.2), seed=3.1)
    if z < 0.05 or (z < 0.62 and near_leg(p) < 0.085):
        # Scaled grey legs, darker on the toes.
        c = mix(SCALES, SCALES_DARK, 0.3 + 0.3 * fleck)
        return mix(c, SCALES_DARK, 1.0 if z < 0.035 else 0.0), 0.0
    if any((p - e).length < 0.0135 for e in EYES):
        return (0.03, 0.025, 0.02), 0.0
    if y < -0.598 and z > 1.36:
        return mix(BILL, DARK, 0.15 + 0.2 * fleck), 0.0
    if z > 1.33 and y < -0.45:
        # Grey cheeks, a dark crown and nape.
        crown = (max(0.0, n.z) * 0.7 + max(0.0, n.y) * 0.4) * LOOK.get("crown", 1.0)
        return mix(NECK_GREY, BLACK, crown + 0.08 * fleck), 1.0
    # Body plumage: grey-brown on the back, paler on the flanks and belly,
    # streaked along the hanging feathers.
    c = mix(FLANK, BACK, n.z * 1.3 + 0.1)
    c = mix(c, DARK, 0.15 + 0.35 * streak)
    c = mix(c, PALE, max(0.0, -n.z) * 1.3 - 0.1 + 0.1 * fleck - max(0.0, -y - 0.15) * 2.0)
    # The drooping wing plumes: darker, with paler loose ends.
    wing = smooth((abs(x) - 0.2) / 0.05) * smooth((z - 0.62) / 0.06) * smooth((0.42 - y) / 0.08) * max(0.0, abs(n.x) - 0.2) * 1.4
    c = mix(c, mix(P["WING"], P["TIPS"], (0.78 - z) / 0.14), LOOK["wing_dark"] * wing)
    # The loose ends of the hanging feathers are greyer and paler.
    c = mix(c, P["TIPS"], (0.70 - z) / 0.2 + 0.15 * fleck)
    # The male's black base of the neck and upper breast, fading into the
    # grey of the upper neck.
    neck = mix(BLACK, NECK_GREY, (z - 1.04) / 0.16 + 0.05 * fleck)
    # A dark stripe down the back of the neck, from the nape to the collar.
    neck = mix(neck, DARK, LOOK.get("nape", 0.0) * smooth((n.y - 0.2) / 0.3))
    if LOOK["neck_edge"] == "hard":
        black = 1.0 if (y < -0.30 and z > 0.84) or (y < -0.22 and z > 0.9) else 0.0
    else:
        # Softly and raggedly into the breast.
        black = smooth((-y - 0.22) / 0.10 + 0.25 * streak + 0.1 * fleck) * smooth((z - 0.80) / 0.08 + 0.3 * streak)
    return mix(c, neck, black), 1.0


def smooth(t):
    t = max(0.0, min(1.0, t))
    return t * t * (3 - 2 * t)


def paint_sculpt(obj):
    """Vertex colours on the dense sculpt (alpha: the plumage mask) and the
    material that turns them into feathers for the bake: fine hanging
    barbs and rows of overlapping feather ends (colour and bump) on the
    plumage, small scales on the legs."""
    attr = obj.data.color_attributes.new("Col", "BYTE_COLOR", "POINT")
    for v in obj.data.vertices:
        rgb, plume = colour_at(v.co, v.normal)
        attr.data[v.index].color_srgb = (*rgb, plume)
    mat = bpy.data.materials.new("Sculpt")
    mat.use_nodes = True
    nt = mat.node_tree
    nodes, link = nt.nodes, nt.links.new
    bsdf = nodes["Principled BSDF"]
    col = nodes.new("ShaderNodeVertexColor")
    col.layer_name = "Col"
    coord = nodes.new("ShaderNodeTexCoord")
    # Barbs: noise stretched along the hang (vertical) of the feathers.
    barbs_map = nodes.new("ShaderNodeMapping")
    barbs_map.inputs["Scale"].default_value = (140.0, 140.0, 14.0)
    link(coord.outputs["Object"], barbs_map.inputs["Vector"])
    barbs = nodes.new("ShaderNodeTexNoise")
    barbs.inputs["Detail"].default_value = 6.0
    barbs.inputs["Roughness"].default_value = 0.6
    link(barbs_map.outputs["Vector"], barbs.inputs["Vector"])
    # Rows of feather ends: bands along the height, broken by noise.
    rows = nodes.new("ShaderNodeTexWave")
    rows.wave_type = "BANDS"
    rows.bands_direction = "Z"
    rows.wave_profile = "SAW"
    rows.inputs["Scale"].default_value = 9.0
    rows.inputs["Distortion"].default_value = 9.0
    rows.inputs["Detail"].default_value = 3.0
    link(coord.outputs["Object"], rows.inputs["Vector"])
    feather = nodes.new("ShaderNodeMix")
    feather.data_type = "FLOAT"
    feather.inputs["Factor"].default_value = LOOK["rows"]
    link(barbs.outputs["Fac"], feather.inputs["A"])
    link(rows.outputs["Fac"], feather.inputs["B"])
    scales = nodes.new("ShaderNodeTexVoronoi")
    scales.inputs["Scale"].default_value = 220.0
    link(coord.outputs["Object"], scales.inputs["Vector"])
    height = nodes.new("ShaderNodeMix")
    height.data_type = "FLOAT"
    link(col.outputs["Alpha"], height.inputs["Factor"])
    link(scales.outputs["Distance"], height.inputs["A"])
    link(feather.outputs["Result"], height.inputs["B"])
    bump = nodes.new("ShaderNodeBump")
    bump.inputs["Strength"].default_value = 0.8
    bump.inputs["Distance"].default_value = 0.004
    link(height.outputs["Result"], bump.inputs["Height"])
    link(bump.outputs["Normal"], bsdf.inputs["Normal"])
    # Colour: darker in the grooves between barbs and under each row.
    shade = nodes.new("ShaderNodeMapRange")
    shade.inputs["To Min"].default_value = 0.62
    shade.inputs["To Max"].default_value = 1.18
    link(height.outputs["Result"], shade.inputs["Value"])
    tint = nodes.new("ShaderNodeMix")
    tint.data_type = "RGBA"
    tint.blend_type = "MULTIPLY"
    tint.inputs["Factor"].default_value = 1.0
    link(col.outputs["Color"], tint.inputs["A"])
    link(shade.outputs["Result"], tint.inputs["B"])
    # Mottle: each feather (a Voronoi cell some 10 to 25 cm long, stretched
    # along its hang) its own tone, with a darker outline where the next
    # one overlaps it; only on the plumage.
    cells_map = nodes.new("ShaderNodeMapping")
    cells_map.inputs["Scale"].default_value = (9.0, 9.0, 5.0)
    link(coord.outputs["Object"], cells_map.inputs["Vector"])
    cells = nodes.new("ShaderNodeTexVoronoi")
    cells.inputs["Randomness"].default_value = 0.9
    link(cells_map.outputs["Vector"], cells.inputs["Vector"])
    edges = nodes.new("ShaderNodeTexVoronoi")
    edges.feature = "DISTANCE_TO_EDGE"
    link(cells_map.outputs["Vector"], edges.inputs["Vector"])
    tone = nodes.new("ShaderNodeMapRange")
    tone.inputs["To Min"].default_value = 0.62
    tone.inputs["To Max"].default_value = 1.22
    link(cells.outputs["Color"], tone.inputs["Value"])
    rim = nodes.new("ShaderNodeMapRange")
    rim.inputs["From Max"].default_value = 0.06
    rim.inputs["To Min"].default_value = 0.55
    link(edges.outputs["Distance"], rim.inputs["Value"])
    patch = nodes.new("ShaderNodeMath")
    patch.operation = "MULTIPLY"
    link(tone.outputs["Result"], patch.inputs[0])
    link(rim.outputs["Result"], patch.inputs[1])
    amount = nodes.new("ShaderNodeMath")
    amount.operation = "MULTIPLY"
    amount.inputs[1].default_value = LOOK["mottle"]
    link(col.outputs["Alpha"], amount.inputs[0])
    mottle = nodes.new("ShaderNodeMix")
    mottle.data_type = "FLOAT"
    mottle.inputs["A"].default_value = 1.0
    link(amount.outputs["Value"], mottle.inputs["Factor"])
    link(patch.outputs["Value"], mottle.inputs["B"])
    final = nodes.new("ShaderNodeMix")
    final.data_type = "RGBA"
    final.blend_type = "MULTIPLY"
    final.inputs["Factor"].default_value = 1.0
    link(tint.outputs["Result"], final.inputs["A"])
    link(mottle.outputs["Result"], final.inputs["B"])
    link(final.outputs["Result"], bsdf.inputs["Base Color"])
    obj.data.materials.clear()
    obj.data.materials.append(mat)


# The feather atlas: six columns, four grey-brown and two black (the base
# of the neck); sRGB root, tip and shaft colours.
ATLAS_COLUMNS = [
    ((0.46, 0.42, 0.36), (0.68, 0.65, 0.59), (0.52, 0.48, 0.42)),
    ((0.30, 0.27, 0.23), (0.44, 0.41, 0.36), (0.34, 0.31, 0.27)),
    ((0.52, 0.48, 0.42), (0.74, 0.71, 0.65), (0.58, 0.54, 0.48)),
    ((0.24, 0.21, 0.18), (0.38, 0.35, 0.31), (0.28, 0.25, 0.21)),
    ((0.08, 0.075, 0.07), (0.16, 0.15, 0.14), (0.12, 0.11, 0.10)),
    ((0.10, 0.095, 0.09), (0.22, 0.21, 0.20), (0.15, 0.14, 0.13)),
]
# v8's shafts were pale, almost white.
PALE_SHAFTS = [(0.80, 0.76, 0.68), (0.74, 0.70, 0.62), (0.82, 0.78, 0.70), (0.70, 0.66, 0.58), (0.25, 0.23, 0.21), (0.30, 0.28, 0.26)]
BROWN, DARK_BROWN, BLACK_COLUMNS = (0, 1, 2, 3), (1, 3), (4, 5)


def atlas_columns():
    if LOOK["shafts"] == "pale":
        bases = [b for b, _, _ in ATLAS_COLUMNS]
        tips = [t for _, t, _ in ATLAS_COLUMNS]
        bases[1], tips[1] = (0.40, 0.36, 0.31), (0.60, 0.56, 0.50)
        bases[3], tips[3] = (0.36, 0.33, 0.29), (0.55, 0.52, 0.47)
        return list(zip(bases, tips, PALE_SHAFTS))
    if LOOK["shafts"] == "light":
        # Halfway to v8's pale shafts: seen, not white.
        return [(b, t, mix(s, p, 0.5)) for (b, t, s), p in zip(ATLAS_COLUMNS, PALE_SHAFTS)]
    return ATLAS_COLUMNS


def card_spots(sculpt, seed=5):
    """Where the loose feather cards go, sampled over the sculpt: the lower
    edge of the skirt, the drooping wing plumes, the bustle, the back and
    the black base of the neck, each hanging its own way and at its own
    length."""
    import random

    rng = random.Random(seed)
    want = LOOK.get("card_counts", {"skirt": 110, "wing": 70, "bustle": 34, "back": 36, "neck": 24})
    got = {k: 0 for k in want}
    # (hang direction, width, length range, columns) per region.
    style = {
        "skirt": ((0, 0.25, -1), 0.075, (0.15, 0.26), BROWN),
        "wing": (*LOOK["wing_cards"][:3], DARK_BROWN if LOOK["wing_cards"][3] == "dark" else BROWN),
        "bustle": ((0, 0.3, -1), 0.08, (0.15, 0.24), BROWN),
        "back": ((0, 1, -0.3), 0.06, (0.10, 0.15), DARK_BROWN if LOOK["wing_cards"][3] == "dark" else BROWN),
        "neck": ((0, 0.25, -1), 0.05, (0.07, 0.11), BLACK_COLUMNS),
    }
    polys = list(sculpt.data.polygons)
    rng.shuffle(polys)
    spots = []
    for poly in polys:
        c, n = poly.center.copy(), poly.normal.copy()
        x, y, z = unwarp(c)
        if z < 0.05 or near_leg((x, y, z)) < 0.1:
            continue
        if (y < -0.30 and 0.86 < z < 1.0) or (y < -0.22 and 0.9 < z < 1.0):
            region = "neck"
        elif z > 1.0 or y < -0.30:
            region = "back" if (n.z > 0.55 and z > 0.95 and y > -0.2) else None
        elif 0.47 < z < 0.68 and n.z < 0.3:
            region = "skirt"
        elif abs(x) > 0.2 and 0.68 < z < 0.92 and -0.15 < y < 0.4 and abs(n.x) > 0.5:
            region = "wing"
        elif y > 0.38 and 0.55 < z < 0.95 and n.y > 0.3:
            region = "bustle"
        elif n.z > 0.55 and z > 0.95:
            region = "back"
        else:
            region = None
        if region is None or got[region] >= want[region]:
            continue
        got[region] += 1
        hang, width, (lo, hi), columns = style[region]
        hang = Vector(hang).normalized()
        hang = (hang - n * hang.dot(n)).normalized()
        side = hang.cross(n).normalized()
        length = lo + (hi - lo) * rng.random()
        spots.append((c + n * 0.006, hang, side, n, width * (0.8 + 0.4 * rng.random()), length, rng.choice(columns)))
    print(f"rhea: feather cards {got}")
    return spots


def build(textures=None):
    """The skinned rhea: the body (unwrapped, its maps baked from a dense
    sculpt) and the feather cards, joined in one mesh with two materials,
    ``Body`` and ``Feathers``. With ``textures``, writes the JPEG sources of
    the two KTX2 sets there (``rhea_body``, ``rhea_feathers``)."""
    import numpy as np

    rig.reset()
    sculpt = rig.fuse_dense("RheaSculpt", body_parts(), voxel=0.006)
    paint_sculpt(sculpt)
    body = rig.decimated_copy(sculpt, "Rhea", budget=5200)
    for a in list(body.data.color_attributes):
        body.data.color_attributes.remove(a)
    body.data.materials.clear()
    body.data.materials.append(rig.plain_material("Body", (0.5, 0.46, 0.4)))
    rig.unwrap(body)
    size = 1024
    colour = rig.new_image("rhea_body_color", size)
    normal = rig.new_image("rhea_body_normal", size, data=True)
    occlusion = rig.new_image("rhea_body_ao", size, data=True)
    rig.bake_to(body, normal, "NORMAL", source=sculpt, extrusion=LOOK["extrusion"])
    rig.bake_to(body, colour, "DIFFUSE", source=sculpt, extrusion=LOOK["extrusion"], pass_filter={"COLOR"})
    # Otherwise the sculpt shades the body it overlaps.
    sculpt.hide_render = not LOOK["ao_through_sculpt"]
    rig.bake_to(body, occlusion, "AO", samples=LOOK["ao_samples"])
    # Darken the colour a little by the occlusion too: the game's occlusion
    # map only dims the ambient light.
    px = np.empty(size * size * 4, np.float32)
    colour.pixels.foreach_get(px)
    ao = np.empty(size * size * 4, np.float32)
    occlusion.pixels.foreach_get(ao)
    px = px.reshape(-1, 4)
    px[:, :3] *= (1.0 - 0.35 * (1.0 - ao.reshape(-1, 4)[:, :1]))
    colour.pixels.foreach_set(px.ravel())
    colour.update()
    rough = rig.image_from_array("rhea_body_rough", np.full((size, size, 3), 0.9, np.float32), data=True)

    atlas_colour, atlas_alpha, atlas_normal, atlas_rough = rig.feather_atlas_columns(atlas_columns(), 1024, style=LOOK["atlas"])
    f_colour = rig.image_from_array("rhea_feathers_color", atlas_colour)
    f_alpha = rig.image_from_array("rhea_feathers_opacity", atlas_alpha, data=True)
    f_normal = rig.image_from_array("rhea_feathers_normal", atlas_normal, data=True)
    f_rough = rig.image_from_array("rhea_feathers_rough", atlas_rough, data=True)
    if textures:
        for folder, maps in (
            ("rhea_body", (("color", colour), ("normal_gl", normal), ("roughness", rough), ("ao", occlusion))),
            ("rhea_feathers", (("color", f_colour), ("normal_gl", f_normal), ("roughness", f_rough), ("opacity", f_alpha))),
        ):
            for name, img in maps:
                rig.save_jpeg(img, os.path.join(textures, folder, f"{name}.jpg"))

    arm = rig.build_armature("RheaArmature", bones())
    rig.bind(body, arm)
    feathers = rig.cards("RheaFeathers", card_spots(sculpt), len(ATLAS_COLUMNS), LOOK.get("card_lift", (0.06, 0.10)))
    feathers.data.materials.append(rig.plain_material("Feathers", (0.5, 0.46, 0.4)))
    rig.transfer_weights(body, feathers, arm)
    rig.join(body, feathers)
    bpy.data.objects.remove(sculpt)
    print(f"rhea: {rig.triangles(body)} triangles in all")
    previews = {
        "Body": rig.textured_material("BodyPreview", colour, normal),
        "Feathers": rig.textured_material("FeathersPreview", f_colour, f_normal, alpha=f_alpha),
    }
    return body, arm, previews


def parse(argv):
    import argparse

    parser = argparse.ArgumentParser(prog="rhea.py")
    parser.add_argument("output")
    parser.add_argument("--preview", default=None, metavar="DIR")
    parser.add_argument("--textures", default=None, metavar="DIR")
    parser.add_argument("--no-clips", action="store_true")
    parser.add_argument("--look", default="v13", choices=sorted(LOOKS))
    return parser.parse_args(argv)


def use_materials(mesh, by_name):
    for i, mat in enumerate(mesh.data.materials):
        name = mat.name.split(".")[0].replace("Preview", "")
        mesh.data.materials[i] = by_name[name]


def main():
    argv = sys.argv[sys.argv.index("--") + 1 :] if "--" in sys.argv else []
    args = parse(argv)
    global LOOK
    LOOK = LOOKS[args.look]
    mesh, arm, previews = build(args.textures)
    exported = {m.name: m for m in mesh.data.materials}
    actions = [] if args.no_clips else clips(arm)
    if args.preview:
        use_materials(mesh, previews)
        side, target = (3.4, -0.3, 0.95), (0, -0.05, 0.8)
        shots = [
            ("side.png", None, 0, side, target, 50),
            ("front34.png", None, 0, (2.0, -2.6, 1.25), (0, -0.1, 0.8), 50),
            ("rear34.png", None, 0, (-2.2, 2.4, 1.35), (0, 0, 0.8), 50),
            # Close-ups at 2 to 3 m.
            ("close_head.png", None, 0, (1.1, -2.3, 1.55), (0, -0.5, 1.3), 70),
            ("close_skirt.png", None, 0, (2.2, -0.6, 0.8), (0.1, 0.1, 0.72), 60),
            ("close_rear.png", None, 0, (-1.4, 2.0, 1.1), (0, 0.25, 0.8), 55),
        ]
        by_name = {a.name: a for a in actions}
        for name, frames in (("Walk", (0, 8, 16, 24)), ("Run", (0, 3, 6, 9)), ("Graze", (0, 22)), ("Idle_Look", (30,))):
            if name in by_name:
                for f in frames:
                    shots.append((f"{name.lower()}_{f:02}.png", by_name[name], f, side, target, 50))
        rig.render_previews(args.preview, arm, shots)
        use_materials(mesh, exported)
    for mat in list(bpy.data.materials):
        if mat.name not in exported:
            bpy.data.materials.remove(mat)
    rig.export(args.output, actions)


PIVOT = Vector(warp((0, 0.05, 0.82)))  # the body pitches and rolls about here
NECK = ("Neck1", "Neck2", "Neck3", "Head")


def rest_angle(head, tail):
    """A bone's angle above the horizontal, forward, in degrees."""
    return math.degrees(math.atan2(tail[2] - head[2], head[1] - tail[1]))


# The neck's rest angles above the horizontal, forward, in degrees.
REST_NECK = tuple(rest_angle(h, t) for name, h, t, _ in SPINE if name in NECK)

# Clips: name, seconds, and the speed over the ground their feet show.
WALK = dict(seconds=1.1, duty=0.62, stride=0.60, lift=0.10)
RUN = dict(seconds=0.42, duty=0.28, stride=0.85, lift=0.24)


def ground_speed(gait):
    """The speed over the ground of a gait's stance feet, in m/s."""
    return gait["stride"] / (gait["duty"] * gait["seconds"])


def body_matrix(lift=0.0, pitch=0.0, roll=0.0, yaw=0.0, forward=0.0):
    """The body's motion: up by ``lift`` m, nose down by ``pitch`` degrees,
    rolled and turned about the pivot, and ``forward`` m ahead."""
    rot = (
        Matrix.Rotation(math.radians(yaw), 4, "Z")
        @ Matrix.Rotation(math.radians(roll), 4, "Y")
        @ Matrix.Rotation(math.radians(pitch), 4, "X")
    )
    return Matrix.Translation(Vector((0, -forward, lift)) + PIVOT) @ rot @ Matrix.Translation(-PIVOT)


def direction(angle, yaw=0.0):
    """Forward (-Y) tilted ``angle`` degrees up and turned ``yaw`` degrees
    toward +X."""
    a, y = math.radians(angle), math.radians(yaw)
    return Vector((math.sin(y) * math.cos(a), -math.cos(y) * math.cos(a), math.sin(a)))


def pose_neck(pose, body, angles, yaw=0.0):
    """Lays the neck and head along ``angles`` (degrees above the
    horizontal, world space), turned ``yaw`` degrees, more toward the head."""
    head = body @ pose.rest_head("Neck1")
    for k, (bone, angle) in enumerate(zip(NECK, angles)):
        pose.place(bone, head, direction(angle, yaw * (k + 1) / len(NECK)))
        head = pose.tail(bone)


def pose_leg(pose, body, side, foot, toe_pitch, reach=0.35):
    """Plants the leg on ``side`` with its ankle at ``foot`` (world): the
    thigh swings part way (``reach``) toward the foot, the shank and tarsus
    bend at the heel (always backwards) to reach it, and the toes point
    ``toe_pitch`` degrees below the horizontal."""
    names = [f"{n}.{side}" for n in ("Thigh", "Shank", "Tarsus", "Toe")]
    hip = body @ pose.rest_head(names[0])
    rest_dir = body.to_3x3() @ (pose.rest_tail(names[0]) - pose.rest_head(names[0])).normalized()
    foot = Vector(foot)
    thigh = (rest_dir * (1 - reach) + (foot - hip).normalized() * reach).normalized()
    pose.place(names[0], hip, thigh)
    knee = pose.tail(names[0])
    l2, l3 = pose.length(names[1]), pose.length(names[2])
    v = foot - knee
    d = max(abs(l2 - l3) + 1e-3, min(v.length, l2 + l3 - 1e-3))
    cos_a = (l2 * l2 + d * d - l3 * l3) / (2 * l2 * d)
    a = math.acos(max(-1.0, min(1.0, cos_a)))
    axis = Vector((1, 0, 0))
    heels = [knee + (Matrix.Rotation(s * a, 3, axis) @ v.normalized()) * l2 for s in (1, -1)]
    heel = max(heels, key=lambda h: h.y)  # the heel bends backwards
    pose.place(names[1], knee, heel - knee)
    pose.place(names[2], heel, foot - heel)
    pose.place(names[3], pose.tail(names[2]), direction(-toe_pitch))


def about(at, rotation):
    """``rotation`` (4x4) about the point ``at``."""
    return Matrix.Translation(at) @ rotation @ Matrix.Translation(-at)


def pose_wings(pose, body, spread=0.0, lift=0.0, shake=(0.0, 0.0)):
    """The wings held out ``spread`` degrees and up ``lift`` degrees; their
    loose plumes flick up and down by ``shake`` degrees (left, right)."""
    for (side, sx), flick in zip((("L", 1), ("R", -1)), shake):
        bone = f"Wing.{side}"
        turn = Matrix.Rotation(math.radians(-sx * spread), 4, "Z") @ Matrix.Rotation(math.radians(lift), 4, "X")
        wing = body @ about(pose.rest_head(bone), turn)
        pose.carry(bone, wing)
        plume = f"Plume.{side}"
        pose.carry(plume, wing @ about(pose.rest_head(plume), Matrix.Rotation(math.radians(flick), 4, "X")))


def pose_skirt(pose, body, swing=(0.0, 0.0), flare=0.0):
    """The loose skirt over each thigh swings back and forth by ``swing``
    degrees (left, right) and flares out by ``flare``."""
    for (side, sx), angle in zip((("L", 1), ("R", -1)), swing):
        bone = f"Skirt.{side}"
        turn = Matrix.Rotation(math.radians(sx * flare), 4, "Y") @ Matrix.Rotation(math.radians(angle), 4, "X")
        pose.carry(bone, body @ about(pose.rest_head(bone), turn))


def pose_rump(pose, body, sway=0.0):
    at = pose.rest_head("Rump")
    pose.carry("Rump", body @ Matrix.Translation(at) @ Matrix.Rotation(math.radians(sway), 4, "X") @ Matrix.Translation(-at))


def rest_foot(pose, side):
    return pose.rest_tail(f"Tarsus.{side}")


REST_TOE = 10.6


def stand(arm, body, neck, yaw=0.0, wings=(0.0, 0.0), sway=0.0, t=0.0):
    pose = rig.Pose(arm)
    pose.carry("Body", body)
    pose_neck(pose, body, neck, yaw)
    for side in ("L", "R"):
        pose_leg(pose, body, side, rest_foot(pose, side), REST_TOE, reach=0.0)
    # The loose plumage stirs a little, as in a breeze.
    pose_wings(pose, body, *wings, shake=(1.5 * wave(t, 1), 1.5 * wave(t, 1, 0.3)))
    pose_skirt(pose, body, swing=(1.5 * wave(t, 1, 0.6), 1.5 * wave(t, 1, 0.1)))
    pose_rump(pose, body, sway)
    return pose


def wave(t, cycles=1.0, phase=0.0):
    return math.sin(2 * math.pi * (t * cycles + phase))


def idle(arm, t):
    # Breathing, a slow sway of the neck and a glance to each side.
    body = body_matrix(lift=0.004 * wave(t, 2), pitch=0.6 * wave(t, 2, 0.25))
    neck = [a + 2.0 * wave(t, 1, 0.1 * k) for k, a in enumerate(REST_NECK)]
    return stand(arm, body, neck, yaw=14.0 * wave(t), sway=1.5 * wave(t, 2), t=t)


def idle_look(arm, t):
    # Alert: the neck straight up, the head turning to look about.
    body = body_matrix(lift=0.006, pitch=-3.0)
    neck = (66.0, 80.0, 88.0, 6.0 + 4.0 * wave(t, 2))
    turn = 40.0 * math.sin(2 * math.pi * t) ** 3
    return stand(arm, body, neck, yaw=turn, t=t)


def graze(arm, t):
    # Nose down, the neck reaching to the ground, pecking twice a cycle.
    body = body_matrix(lift=-0.03, pitch=20.0 + 1.0 * wave(t, 2))
    peck = max(0.0, wave(t, 2)) ** 3
    neck = (-25.0, -62.0 - 6.0 * peck, -84.0 - 4.0 * peck, -70.0 - 15.0 * peck)
    return stand(arm, body, neck, yaw=8.0 * wave(t, 1, 0.3), sway=-3.0, t=t)


def stride(gait, t, center=-0.02):
    """The ankle of a leg at phase ``t``: (y, z, toe pitch)."""
    duty, half = gait["duty"], gait["stride"] / 2
    if t < duty:
        q = t / duty
        return center - half + 2 * half * q, GROUND, REST_TOE
    q = (t - duty) / (1 - duty)
    ease = q * q * (3 - 2 * q)
    y = center + half - 2 * half * ease
    z = GROUND + gait["lift"] * math.sin(math.pi * q) ** 0.8
    return y, z, REST_TOE + 55.0 * math.sin(math.pi * q)


def locomotion(arm, t, gait, body, neck, reach, wings=(0.0, 0.0), sway=0.0, shake=(0.0, 0.0), swing=(0.0, 0.0), flare=0.0):
    pose = rig.Pose(arm)
    pose.carry("Body", body)
    pose_neck(pose, body, neck)
    for side, offset in (("L", 0.0), ("R", 0.5)):
        y, z, toe = stride(gait, (t + offset) % 1.0)
        x = rest_foot(pose, side).x
        pose_leg(pose, body, side, (x, y, z), toe, reach=reach)
    pose_wings(pose, body, *wings, shake=shake)
    pose_skirt(pose, body, swing=swing, flare=flare)
    pose_rump(pose, body, sway)
    return pose


def head_bob(t):
    """A bird's head bob, two a cycle: the head holds still while the body
    walks under it (the neck tilts back), then thrusts forward. In
    [-1, 1]."""
    s = (2 * t) % 1.0
    hold = 0.7
    if s < hold:
        q = s / hold
        return -1 + 2 * q * q * (3 - 2 * q) * 0.5 + q
    q = (s - hold) / (1 - hold)
    return 1 - 2 * q * q * (3 - 2 * q)


def walk(arm, t):
    # Two steps a cycle: the body dips at each footfall and rolls over the
    # standing leg; the head bobs, holding still then thrusting forward; the
    # skirt swings with each thigh.
    body = body_matrix(lift=-0.012 + 0.012 * wave(t, 2, 0.1), pitch=2.0, roll=2.0 * wave(t, 1, 0.3))
    bob = 5.0 * head_bob(t)
    neck = (REST_NECK[0] - 6 + bob, REST_NECK[1] - 4 + bob, REST_NECK[2] - 3 - 0.5 * bob, REST_NECK[3] - 0.6 * bob)
    swing = 5.0 * wave(t, 1, -0.1)
    return locomotion(arm, t, WALK, body, neck, reach=0.3, sway=3.0 * wave(t, 2, 0.4), shake=(2.0 * wave(t, 2), 2.0 * wave(t, 2, 0.5)), swing=(swing, -swing))


def run(arm, t):
    # A long striding run: the body low and leaning forward, the neck
    # stretched ahead, the wings a little out from the body.
    body = body_matrix(lift=-0.05 + 0.03 * wave(t, 2, -0.1), pitch=12.0 + 2.0 * wave(t, 2), roll=2.5 * wave(t, 1, 0.3))
    neck = (32.0 + 3.0 * wave(t, 2), 42.0 + 3.0 * wave(t, 2, 0.1), 58.0, 4.0)
    # The loose plumes and skirt shake with each footfall, lagging behind it.
    shake = (9.0 * wave(t, 2, -0.15) + 3.0 * wave(t, 4), 9.0 * wave(t, 2, 0.1) + 3.0 * wave(t, 4, 0.3))
    swing = 10.0 * wave(t, 1, -0.15)
    return locomotion(
        arm, t, RUN, body, neck, reach=0.55, wings=(14.0 + 3.0 * wave(t, 2), 10.0), sway=7.0 * wave(t, 2, 0.4),
        shake=shake, swing=(swing, -swing), flare=6.0 + 3.0 * wave(t, 2, -0.2),
    )


def clips(arm):
    made = [
        rig.bake_clip(arm, "Idle", 4.0, idle, "Body"),
        rig.bake_clip(arm, "Idle_Look", 5.0, idle_look, "Body"),
        rig.bake_clip(arm, "Graze", 3.0, graze, "Body"),
        rig.bake_clip(arm, "Walk", WALK["seconds"], walk, "Body"),
        rig.bake_clip(arm, "Run", RUN["seconds"], run, "Body"),
    ]
    print(f"rhea: walk {ground_speed(WALK):.2f} m/s, run {ground_speed(RUN):.2f} m/s")
    return made

main()
