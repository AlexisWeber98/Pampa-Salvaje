"""Fit the draught harness to the horse and the yoke to the oxen (1919 La
Pampa), in Blender, by script.

    blender -b --factory-startup --python tools/vehicle_harness.py -- OUT_DIR [--preview DIR]

Writes ``OUT_DIR/harness.glb`` (on ``tools/horse.py``'s horse) and
``OUT_DIR/yoke.glb`` (on ``tools/cattle.py``'s bull, the ox of a carreta's
team). Each piece is a rigid mesh named ``<Piece>@<Joint>``, its vertices
in the animal model's own space at its rest pose: the game hangs it under
the animal's joint ``<Joint>`` by that joint's inverse bind matrix, so it
rides the trunk, the hindquarters or the head through every clip.

The harness is the one the pampa's sulkies and carts wore (the
"guarnición" of black harness leather, after the English and American
gig harness the trade sold): a breast collar (pechera) with its neck
strap, the harness pad (sillín) with its two terrets and the tugs that
hold the shafts, the belly band (barriguera), the back strap and crupper,
the breeching (retranca) with its hip strap, and the bridle with its
blinkers (anteojeras), noseband, throatlatch and a snaffle bit. Every
strap is fitted to the horse's skin (a ray cast onto the mesh, without its
hair cards and ears, then lifted ``GAP`` off it) and swept as a flat
strap; the pad and the blinkers are padded leather with iron fittings.

The yoke is the carreta's head yoke (yugo de cabeza): a beam of hardwood
lying on both oxen's napes behind the horns, hollowed where it bears,
lashed to each pair of horns with rawhide thongs (coyundas), with the
rawhide ring in the middle (the barzón) that takes the end of the
carreta's pole. It is built in the left ox's space; the right ox stands
``OX_SPACING`` model metres to its right (``-X``).

The constants below (the tug, the trace's start on the collar, the
breeching's hold on the shaft, the bit and the terret) are measured on
the rest horse and shared with the vehicle scripts, which run the traces
and the reins from them. See tools/README.md.
"""

import math
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)

import vehicle as V  # noqa: E402

HORSE_GLB = os.path.join(HERE, "..", "assets", "models", "fauna", "own", "horse.glb")
BULL_GLB = os.path.join(HERE, "..", "assets", "models", "fauna", "own", "bull.glb")
FAUNA_DIR = os.path.join(HERE, "..", "assets", "models", "fauna", "own")

# How far a strap stands off the skin, in metres.
GAP = 0.010

# Points on the rest horse (Blender metres, the horse facing -Y, +X its
# left), for the left side; the right mirrors X.
TUG = (0.27, -0.16, 1.10)  # where the tug hangs off the pad, on the barrel
TRACE = (0.262, -0.46, 1.07)  # the breast collar's end, where a trace starts
SIDE = (0.25, 0.10, 1.04)  # the barrel's flank, where a trace runs along
BREECH = (0.285, 0.50, 1.00)  # the breeching's end, strapped to a shaft
BIT = (0.085, -1.37, 1.405)  # a ring of the snaffle, at the mouth's corner
TERRET = (0.075, -0.17, 1.49)  # a terret on the pad, the rein runs through

# The ox (the bull model; the game scales it to ``Species::Bull``):
# model metres between the two oxen of a team, and where the yoke bears.
BULL_SCALE = 1.55 / 1.40
OX_SPACING = 1.13
YOKE = (0.0, -1.20, 1.36)  # the yoke's middle line over one ox's nape


if V.bpy is not None:
    import bmesh
    import bpy
    from mathutils import Vector
    from mathutils.bvhtree import BVHTree

    def skin_tree(obj, skip_groups=("Ear2", "Ear3", "Ear4"), skip_materials=("Hair", "Eye_Black", "Eye_White")):
        """A BVH of ``obj``'s skin: no hair cards, eyes or ears."""
        me = obj.data
        names = {g.index: g.name for g in obj.vertex_groups}
        skip_mat = {i for i, m in enumerate(me.materials) if m and m.name in skip_materials}
        bad = set()
        for v in me.vertices:
            for g in v.groups:
                if g.weight > 0.3 and any(names[g.group].startswith(s) for s in skip_groups):
                    bad.add(v.index)
        verts = [v.co.copy() for v in me.vertices]
        polys = [list(p.vertices) for p in me.polygons
                 if p.material_index not in skip_mat and not any(i in bad for i in p.vertices)]
        return BVHTree.FromPolygons(verts, polys)

    class Fitter:
        def __init__(self, tree):
            self.tree = tree

        def onto(self, p, out, gap=GAP):
            """``p`` moved onto the skin from outside along ``out``, lifted by
            ``gap`` along the surface normal; ``p`` itself if nothing is hit."""
            d = Vector(out).normalized()
            hit, normal, _, _ = self.tree.ray_cast(Vector(p) + d * 0.6, -d, 1.2)
            if hit is None:
                return tuple(p)
            return tuple(hit + normal * gap)

        def drape(self, points, outs, gap=GAP, reach=0.06):
            """Each of ``points`` moved onto the skin along its ``outs``
            direction, keeping only those whose hit lies within ``reach`` of
            the guess (a ray that found another part of the body is dropped)."""
            kept = []
            for p, out in zip(points, outs):
                d = Vector(out).normalized()
                hit, normal, _, _ = self.tree.ray_cast(Vector(p) + d * 0.4, -d, 0.8)
                if hit is not None and (hit - Vector(p)).length < reach:
                    kept.append(tuple(hit + normal * gap))
            return kept

        def ring(self, centre, a, b, degrees, gap=GAP):
            """The skin round ``centre`` in the plane of ``a`` and ``b``, at
            each angle of ``degrees`` (0 along ``a``, 90 along ``b``)."""
            c, a, b = Vector(centre), Vector(a).normalized(), Vector(b).normalized()
            out = []
            for deg in degrees:
                t = math.radians(deg)
                d = a * math.cos(t) + b * math.sin(t)
                hit, normal, _, _ = self.tree.ray_cast(c + d * 1.0, -d, 1.0)
                # A ray that only met a leg (or nothing) leaves a gap.
                if hit is not None and (hit - c).length < 0.45:
                    out.append(tuple(hit + normal * gap))
            return out

    def smooth_path(points, passes=2):
        pts = [tuple(p) for p in points]
        for _ in range(passes):
            if len(pts) < 3:
                break
            pts = [pts[0]] + [tuple((Vector(pts[i - 1]) + Vector(pts[i]) * 2 + Vector(pts[i + 1])) / 4)
                              for i in range(1, len(pts) - 1)] + [pts[-1]]
        return pts

    def ring_strap(fit, centre, a, b, degrees, width, gap=GAP, material="Leather"):
        pts = fit.ring(centre, a, b, degrees, gap)
        n = Vector(a).cross(Vector(b)).normalized()
        return V.sweep(smooth_path(pts), V.rect(0.006, width, 0.0015), material, (0.4, 0.05), up=tuple(n)), pts

    def buckle(at, along, across, size=0.035):
        """A small iron buckle frame lying on a strap at ``at``."""
        a, c = Vector(along).normalized(), Vector(across).normalized()
        p = Vector(at)
        corners = [p + a * size / 2 + c * size / 2, p - a * size / 2 + c * size / 2,
                   p - a * size / 2 - c * size / 2, p + a * size / 2 - c * size / 2]
        loop = [tuple(q) for q in corners]
        return V.rod(loop + [loop[0]], 0.0035, "Iron", n=6, caps=False)

    def ring_at(centre, normal, r, thick=0.005, material="Iron"):
        """A ring (a terret, a bit ring) round ``normal``."""
        n = Vector(normal).normalized()
        a = n.orthogonal().normalized()
        b = n.cross(a)
        pts = [tuple(Vector(centre) + (a * math.cos(t) + b * math.sin(t)) * r)
               for t in [2 * math.pi * k / 16 for k in range(16)]]
        return V.rod(pts, thick, material, n=6, closed=True, caps=False)

    def harness_pieces(fit):
        """{name@joint: Mesh} of the whole harness on the rest horse."""
        pieces = {}
        X, Y, Z = (1, 0, 0), (0, 1, 0), (0, 0, 1)
        # Breast collar: a broad band round the chest from flank to flank,
        # and the neck strap that holds it up over the neck.
        collar, cpts = ring_strap(fit, (0, -0.52, 1.13), (0, -1, 0), (1, 0, 0), range(-100, 101, 5), 0.065)
        neck, _ = ring_strap(fit, (0, -0.66, 1.30), (1, 0, 0), (0, -0.25, 1), range(-12, 193, 6), 0.035)
        pieces["Collar@Torso3"] = V.merge(collar, neck)
        # The pad over the back, its tree, terrets and the tugs below it.
        pad = V.Mesh()

        def pad_point(u, w):
            x = -0.22 + 0.44 * u
            y = -0.30 + 0.28 * w
            z_out = (x, 0.0, 0.25) if abs(x) < 1e-3 else (x * 1.4, 0.0, 0.25)
            return fit.onto((x, y, 1.40), z_out, gap=0.012)

        top = V.surface(pad_point, 12, 5, "Leather", (0.4, 0.4))
        pad.extend(V.thick(top, 0.035))
        for sx in (1, -1):
            pad.extend(ring_at((TERRET[0] * sx, TERRET[1], TERRET[2]), Y, 0.026, 0.006, "Brass"))
            pad.extend(V.rod([(TERRET[0] * sx, TERRET[1], TERRET[2] - 0.035), (TERRET[0] * sx, TERRET[1], TERRET[2] - 0.02)],
                             0.01, "Iron"))
        # The back band: from pad to pad round the barrel, the tugs on it,
        # and the belly band below.
        band, bpts = ring_strap(fit, (0, -0.16, 1.12), (1, 0, 0), (0, 0, 1), range(-30, 211, 6), 0.045)
        pad.extend(band)
        for sx in (1, -1):
            tug_c = (0.34 * sx, TUG[1], TUG[2] - 0.02)
            pad.extend(ring_at(tug_c, Y, 0.045, 0.010, "Leather"))
            pad.extend(buckle((0.29 * sx, TUG[1], 1.18), Z, Y))
        belly, _ = ring_strap(fit, (0, -0.16, 1.05), (1, 0, 0), (0, 0, 1), range(-10, -171, -6), 0.05)
        pad.extend(belly)
        pieces["Pad@Torso2"] = pad
        # The back strap along the spine to the crupper round the dock.
        spine = [fit.onto((0.0, y, 1.6), Z) for y in [-0.04 + 0.09 * k for k in range(11)]]
        back = V.sweep(smooth_path(spine), V.rect(0.035, 0.006, 0.0015), "Leather", (0.4, 0.05), up=Z)
        pieces["BackStrap@Torso"] = back
        # The breeching round the quarters, and the hip strap over the croup.
        breech, _ = ring_strap(fit, (0, 0.58, 1.00), (0, 1, 0), (1, 0, 0), range(-105, 106, 5), 0.06)
        hip, _ = ring_strap(fit, (0, 0.52, 1.12), (1, 0, 0), (0, 0, 1), range(-15, 196, 6), 0.035)
        crupper = ring_at(fit.onto((0.0, 0.86, 1.40), (0, 0.3, 1), gap=0.02), (0, 0.6, 0.8), 0.035, 0.012, "Leather")
        pieces["Breeching@Back"] = V.merge(breech, hip, crupper)
        # The bridle: crownpiece behind the ears and throatlatch under the
        # jaw, browband, cheekpieces, noseband, blinkers and the bit; each
        # strap draped from guesses round the head (bind pose).
        axis = (Vector((0, -1.477, 1.43)) - Vector((0, -1.10, 1.86))).normalized()
        front = Vector((0, axis.z, -axis.y))
        if front.z < 0:
            front = -front

        def loop(centre, rx, rz, degrees, y_of=lambda t: 0.0, normal=None):
            pts, outs = [], []
            for deg in degrees:
                t = math.radians(deg)
                if normal is None:
                    d = Vector((math.cos(t), 0.0, math.sin(t)))
                    pts.append((centre[0] + rx * d.x, centre[1] + y_of(t), centre[2] + rz * d.z))
                else:
                    d = Vector((math.cos(t), 0, 0)) + normal * math.sin(t)
                    pts.append(tuple(Vector(centre) + Vector((rx * math.cos(t), 0, 0)) + normal * (rz * math.sin(t))))
                outs.append(tuple(d))
            return pts, outs

        def flat(pts, width, up):
            return V.sweep(smooth_path(pts), V.rect(0.006, width, 0.0015), "Leather", (0.4, 0.05), up=up)

        crown_pts = fit.drape(*loop((0, -1.10, 1.74), 0.15, 0.14, range(-25, 206, 5)), reach=0.09)
        throat_pts = fit.drape(*loop((0, -1.06, 1.66), 0.14, 0.16, range(-20, -161, -5),
                                     y_of=lambda t: 0.06 * math.sin(t)), reach=0.09)
        nose_pts = fit.drape(*loop((0, -1.355, 1.53), 0.16, 0.12, range(0, 361, 8), normal=front), reach=0.09)
        brow_guess = [(x, -1.175, 1.83 - 2.0 * x * x) for x in [-0.13 + 0.026 * k for k in range(11)]]
        brow_pts = fit.drape(brow_guess, [(x * 4, front.y, front.z) for x, _, _ in brow_guess], reach=0.08)
        bridle = V.merge(
            flat(crown_pts, 0.03, (0, 1, 0)),
            flat(throat_pts, 0.022, (0, 1, 0)),
            flat(nose_pts, 0.03, tuple(axis)),
            V.sweep(smooth_path(brow_pts), V.rect(0.025, 0.006, 0.0015), "Leather", (0.4, 0.05), up=tuple(front)),
        )
        for sx in (1, -1):
            start = Vector(crown_pts[0] if sx > 0 else crown_pts[-1])
            bit = Vector((BIT[0] * sx, BIT[1], BIT[2]))
            guess = [tuple(start.lerp(bit, k / 10)) for k in range(11)]
            cheek = fit.drape(guess, [(sx, 0, 0)] * 11, reach=0.08)
            cheek = [tuple(start)] + cheek[1:-1] + [tuple(bit)]
            bridle.extend(V.sweep(smooth_path(cheek), V.rect(0.025, 0.006, 0.0015), "Leather", (0.4, 0.05), up=(sx, 0, 0)))
            # The blinker: a padded leather cup beside the eye, on the
            # cheekpiece.
            eye = Vector((0.125 * sx, -1.21, 1.71))
            out = Vector((sx, -0.2, 0.15)).normalized()
            side = out.cross(Vector((0, 0, 1))).normalized()
            up = side.cross(out).normalized()
            centre = eye + out * 0.04

            def cup(u, w, centre=centre, side=side, up=up, out=out):
                a, b = (u - 0.5) * 0.11, (w - 0.5) * 0.09
                return tuple(centre + side * a + up * b - out * (a * a + b * b) * 1.6)

            bridle.extend(V.thick(V.surface(cup, 6, 5, "Leather", (0.2, 0.2)), 0.008))
            bridle.extend(ring_at(tuple(bit), (1, 0, 0), 0.032, 0.0055))
        bridle.extend(V.rod([(BIT[0], BIT[1], BIT[2]), (0.0, BIT[1] - 0.01, BIT[2] - 0.01), (-BIT[0], BIT[1], BIT[2])],
                            0.006, "Iron", n=8))
        pieces["Bridle@Head"] = bridle
        return pieces

    def yoke_pieces():
        """{name@joint: Mesh}: the yoke, in the left ox's model space."""
        x0 = -OX_SPACING / 2  # the yoke's middle, in the left ox's space
        y, z = YOKE[1], YOKE[2]
        half = OX_SPACING / 2 + 0.38
        pts, scales = [], []
        for k in range(21):
            t = -1.0 + 2.0 * k / 20
            x = x0 + half * t
            # A gentle bow, lower over each nape (the hollows), tapered ends.
            d = min(abs(x - 0.0), abs(x - (-OX_SPACING)))
            hollow = 0.025 * math.exp(-((d / 0.12) ** 2))
            pts.append((x, y, z + 0.03 * (1 - t * t) - hollow))
            scales.append(0.75 + 0.25 * math.cos(t * math.pi / 2) ** 0.5)
        beam = V.sweep(pts, V.rect(0.13, 0.14, 0.035), "Wood", (0.9, 0.3), up=(0, 0, 1), scales=scales)
        beam.tint(lambda p: (0.92, 0.88, 0.84))
        yoke = V.Mesh().extend(beam)
        # Coyundas: rawhide thongs wound round the yoke and each horn base.
        for ox in (0.0, -OX_SPACING):
            for sx in (1, -1):
                horn = (ox + 0.07 * sx, -1.27, 1.29)
                for k in range(4):
                    a = 2 * math.pi * k / 4
                    loop = []
                    for j in range(13):
                        t = 2 * math.pi * j / 12
                        # A figure of eight round the beam and the horn.
                        px = horn[0] + 0.025 * sx * math.cos(a) + 0.03 * sx * math.sin(t)
                        py = (y + horn[1]) / 2 + 0.06 * math.cos(t)
                        pz = (z + horn[2]) / 2 + 0.06 * math.sin(2 * t) + 0.004 * k
                        loop.append((px, py, pz))
                    yoke.extend(V.rod(loop, 0.007, "Hide", n=5, closed=True, caps=False))
        # The barzón: the rawhide ring in the middle, for the pole.
        ring = [(x0, y + 0.05 * math.cos(t), z - 0.10 + 0.05 * math.sin(t))
                for t in [2 * math.pi * k / 16 for k in range(16)]]
        yoke.extend(V.rod(ring, 0.012, "Hide", n=6, closed=True, caps=False))
        return {"Yoke@Head": yoke}

    def export_pieces(pieces, path):
        V.reset()
        for name, mesh in pieces.items():
            V.to_object(name, mesh)
        V.export(path)

    # The zaino (dark bay-brown) of ``tools/horse.py``'s ``COATS``, as the
    # game paints it (``Coat::color_at``): the body, the dark points below
    # half a metre, the mane and tail, the hooves.
    ZAINO = dict(body=(78, 50, 34), points=(44, 32, 26), mane=(34, 28, 24), hoof=(38, 32, 28))

    def _linear(rgb):
        return None if rgb is None else tuple(V.srgb_to_linear(c / 255) for c in rgb) + (1.0,)

    def paint_horse(mesh, coat=ZAINO):
        """The coat as vertex colours, as the game paints them."""
        attr = mesh.data.color_attributes.new("Coat", "BYTE_COLOR", "POINT")
        for v in mesh.data.vertices:
            rgb = coat["points"] if v.co.z < 0.5 else coat["body"]
            attr.data[v.index].color_srgb = (rgb[0] / 255, rgb[1] / 255, rgb[2] / 255, 1.0)
        mesh.data.color_attributes.active_color = attr

    def preview_horse(at, harness=None, heading=0.0, frame=6, coat=ZAINO):
        """For the vehicles' previews: our horse at ``at`` walking (frame
        ``frame`` of ``Walk``) in its harness, with its game skin."""
        harness = harness or os.path.join(HERE, "..", "assets", "models", "vehicles", "harness.glb")
        parts = {"Main_Dark": coat["points"], "Main_Light": coat["body"], "Hooves": coat["hoof"]}
        _, arm = V.import_animal(HORSE_GLB, at, os.path.join(FAUNA_DIR, "horse_body"), os.path.join(FAUNA_DIR, "horse_hair"),
                                 heading=heading, paint=lambda m: paint_horse(m, coat), hair_tint=_linear(coat["mane"]),
                                 frame=frame, part_tint=lambda part: _linear(parts.get(part)))
        if os.path.exists(harness):
            V.import_pieces(harness, arm, at, heading=heading)

    def preview_ox(at, with_yoke=False, yoke=None, frame=6, coat="hosco.jpg"):
        """Our ox (``tools/cattle.py --ox``) at ``at`` walking, its baked
        criollo coat, and on the left ox the yoke."""
        ox_dir = os.path.join(HERE, "..", "assets", "models", "vehicles")
        _, arm = V.import_animal(os.path.join(ox_dir, "ox.glb"), at, os.path.join(ox_dir, "ox_body"),
                                 os.path.join(ox_dir, "ox_hair"), scale=BULL_SCALE, coat_map=coat, frame=frame)
        yoke = yoke or os.path.join(ox_dir, "yoke.glb")
        if with_yoke and os.path.exists(yoke):
            V.import_pieces(yoke, arm, at, scale=BULL_SCALE)

    def load_horse():
        V.reset()
        bpy.ops.import_scene.gltf(filepath=HORSE_GLB)
        return [o for o in bpy.data.objects if o.type == "MESH" and o.name == "Horse"][0]


if __name__ == "__main__" and V.bpy is not None:
    argv = V.args()
    out_dir = argv[0]
    os.makedirs(out_dir, exist_ok=True)
    horse = load_horse()
    fit = Fitter(skin_tree(horse))
    pieces = harness_pieces(fit)
    tris = {k: m.triangles() for k, m in pieces.items()}
    export_pieces(pieces, os.path.join(out_dir, "harness.glb"))
    print(f"harness: {tris}")
    yoke = yoke_pieces()
    export_pieces(yoke, os.path.join(out_dir, "yoke.glb"))
    print(f"yoke: {yoke['Yoke@Head'].triangles()} triangles")
    prev = V.option(argv, "--preview")
    if prev:
        V.reset()
        preview_horse((0, 0, 0), os.path.join(out_dir, "harness.glb"))
        V.render_sheet(os.path.join(prev, "harness.png"),
                       [((3.6, -1.2, 1.6), (0, -0.3, 1.2), 40), ((2.0, -3.6, 1.9), (0, -0.7, 1.35), 45),
                        ((0.9, -2.3, 1.8), (0, -1.25, 1.6), 60)])
        V.reset()
        preview_ox((OX_SPACING * BULL_SCALE / 2, 0, 0), True, os.path.join(out_dir, "yoke.glb"))
        preview_ox((-OX_SPACING * BULL_SCALE / 2, 0, 0), coat="overo.jpg")
        V.render_sheet(os.path.join(prev, "yoke.png"),
                       [((2.1, -4.6, 2.3), (0.0, -1.1, 1.3), 40), ((4.0, -1.6, 2.0), (0.0, -0.9, 1.2), 40)])
