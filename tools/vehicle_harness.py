"""Fit the draught harness to the horse and the yoke to the oxen (1919 La
Pampa), in Blender, by script.

    blender -b --factory-startup --python tools/vehicle_harness.py -- OUT_DIR [--preview DIR]

Writes ``OUT_DIR/harness.glb`` (on henry's horse, ``tools/henry_horse.py``)
and ``OUT_DIR/yoke.glb`` (on ``tools/cattle.py``'s bull, the ox of a
carreta's team).

The harness is the one the pampa's sulkies and carts wore (the
"guarnición" of black harness leather, after the English and American
gig harness the trade sold): a breast collar (pechera) with its neck
strap, the harness pad (sillín) with its two terrets and the tugs that
hold the shafts, the belly band (barriguera), the back strap and crupper,
the breeching (retranca) with its hip strap, and the bridle with its
blinkers (anteojeras), noseband, throatlatch and a snaffle bit. Every
strap is fitted to the horse's skin (a ray cast onto the mesh, without its
hair cards, eyes and ears, then lifted ``GAP`` off it) and swept as a flat
strap; the pad and the blinkers are padded leather with iron fittings.

It is built like ``tools/recado.py``'s recado: on henry standing as in the
first frame of ``Idle_1`` (``REST_CLIP``; the nodes' own rest has the head
down by the hooves), in game metres (``SCALE`` times the file's). The
straps (``Collar``, ``Bands``, ``BackStrap``, ``Breeching``, ``Bridle``) take
the weights of the skin under them (never the ears' nor the eyelids'), so
they bend with the horse; the stiff pieces ride bones whole: the pad
(``Saddle``) the two bones of the back under it, the ``Crupper`` the tail's
first bone, the ``Blinkers`` and the ``Bit`` the head. Then the horse is
posed in numpy (``recado.Glb``) in every other frame of each clip a draught
horse plays (``DRAUGHT_CLIPS``), and each vertex the moving skin came closer
to than ``GAP_MOVING`` is lifted off it, ``LIFT_MOST`` at most (never the
bit, whose mouthpiece goes through the mouth). The exported pieces are skinned to a
skeleton named like henry's that rests in that standing pose: the game
skins them to the horse's own joints of the same names
(``pampa_render::vehicle::models::dress_animals``).

The yoke is the carreta's head yoke (yugo de cabeza): a beam of hardwood
lying on both oxen's napes behind the horns, hollowed where it bears,
lashed to each pair of horns with rawhide thongs (coyundas), with the
rawhide ring in the middle (the barzón) that takes the end of the
carreta's pole. It is built in the left ox's space; the right ox stands
``OX_SPACING`` model metres to its right (``-X``). It is one rigid piece,
``Yoke@Head``, that the game hangs from the ox's ``Head`` joint by its
inverse bind matrix.

The constants below (the tug, the trace's start on the collar, the
breeching's hold on the shaft, the bit and the terret) are measured on the
standing horse in game metres and shared with the vehicle scripts, which
run the traces, the shafts and the reins from them
(``tools/test_vehicle_harness.py`` checks them against henry's skin). See
tools/README.md.
"""

import math
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)

import vehicle as V  # noqa: E402

HORSE_GLB = os.path.join(HERE, "..", "assets", "models", "fauna", "henry", "horse.glb")
HENRY_DIR = os.path.join(HERE, "..", "assets", "models", "fauna", "henry")
BULL_GLB = os.path.join(HERE, "..", "assets", "models", "fauna", "own", "bull.glb")
FAUNA_DIR = os.path.join(HERE, "..", "assets", "models", "fauna", "own")

# The game draws henry at 1.5 m to the withers, 1.60 in its file
# (``livestock::HORSE_WITHERS``): the harness is built and measured in game
# metres, the file's times this.
SCALE = 1.5 / 1.60
# The pose the harness is fitted on and its skeleton rests in.
REST_CLIP = "Idle_1"
# The clips a draught horse plays: it stands still in harness
# (``HorseBody::idle``), walks and gallops.
DRAUGHT_CLIPS = ("Idle_1", "Walk", "Gallop")

# How far a strap stands off the standing skin, and the least it keeps off
# the moving skin in any frame of ``DRAUGHT_CLIPS``, in metres.
GAP = 0.010
GAP_MOVING = 0.006
# The most a vertex is lifted off the moving skin, in metres.
LIFT_MOST = 0.03

# Points on the standing horse (game metres, Blender's frame: the horse
# facing -Y, +X its left, the ground under its body at the origin), for the
# left side; the right mirrors X.
TUG = (0.31, -0.27, 1.12)  # the middle of a tug, the shaft runs through it
TRACE = (0.25, -0.50, 1.18)  # the breast collar's end, where a trace starts
SIDE = (0.273, -0.10, 1.12)  # the barrel's flank, where a trace runs along
BREECH = (0.277, 0.34, 1.20)  # the breeching's end, strapped to a shaft
BIT = (0.055, -1.32, 1.41)  # a ring of the snaffle, at the mouth's corner
TERRET = (0.095, -0.27, 1.53)  # a terret on the pad, the rein runs through

# Where the pad's rays start (the barrel's axis), how far round from the top
# its terrets stand (radians), and the bones its tree rides.
PAD_AXIS = 1.19
TERRET_AROUND = 0.28
PAD_WEIGHTS = {"BN_Spine_01_03_03": 0.5, "BN_Spine_02_04_04": 0.5}
# The head: its joint at the poll and the upper lip's (``HorseJoint::Poll``,
# ``HorseJoint::UpperLip``), the left eye's cornea, and how far out a blinker
# stands from it.
HEAD = "BN_Head_00_016_015"
POLL = (0.0, -1.211, 1.74)
LIP = (0.0, -1.407, 1.379)
EYE = (0.088, -1.288, 1.679)
BLINKER_OFF = 0.04
# The tail's first bone, the middle of the dock (where the crupper goes
# round it), the dock's direction and the crupper's radius.
TAIL = "BN_Tail_00_061_067"
DOCK = (0.0, 0.78, 1.33)
DOCK_ALONG = (0.0, 0.116, -0.112)
DOCK_RADIUS = 0.04

# The breeching goes round the quarters from this far back (its ends, at
# ``BREECH``, are a little ahead of it).
BREECH_Y = 0.39

# The ox (the bull model; the game scales it to ``Species::Bull``):
# model metres between the two oxen of a team, and where the yoke bears.
BULL_SCALE = 1.55 / 1.40
OX_SPACING = 1.13
YOKE = (0.0, -1.20, 1.36)  # the yoke's middle line over one ox's nape

# Henry's skin, and the bones whose skin the straps never lie on: the ears
# and the eyelids move on their own (the eyelids' skin even flicks out of the
# head for a frame in ``Idle_1`` and ``Gallop``).
SKIN = {"Horse"}
EARS = ("BN_L_Ear", "BN_R_Ear", "BN_L_UP_Eyelid", "BN_R_UP_Eyelid", "BN_L_BT_Eyelid", "BN_R_BT_Eyelid")


if V.bpy is not None:
    import bmesh
    import bpy
    import numpy as np
    from mathutils import Matrix, Vector
    from mathutils.bvhtree import BVHTree

    import recado as R

    def skin_tree(obj, skip=EARS):
        """A BVH of henry's skin (``SKIN``, no hair cards, eyes nor shoes),
        without the ears."""
        me = obj.data
        names = {g.index: g.name for g in obj.vertex_groups}
        mats = [s.material.name if s.material else "" for s in obj.material_slots]
        bad = set()
        for v in me.vertices:
            for g in v.groups:
                if g.weight > 0.3 and names[g.group].startswith(skip):
                    bad.add(v.index)
        verts = [obj.matrix_world @ v.co for v in me.vertices]
        polys = [list(p.vertices) for p in me.polygons
                 if mats[p.material_index] in SKIN and not any(i in bad for i in p.vertices)]
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

        def wrap(self, points, inside, gap=GAP, reach=0.08):
            """Each of ``points`` moved onto the skin along the ray from the
            point ``inside`` the body through it, keeping only those whose
            hit lies within ``reach`` of the guess (a ray that found another
            part of the body is dropped)."""
            kept = []
            c = Vector(inside)
            for p in points:
                d = (Vector(p) - c).normalized()
                hit, normal, _, _ = self.tree.ray_cast(Vector(p) + d * 0.4, -d, 0.8)
                if hit is not None and (hit - Vector(p)).length < reach:
                    kept.append(tuple(hit + normal * gap))
            return kept

        def ring(self, centre, a, b, degrees, gap=GAP, reach=0.45):
            """The skin round ``centre`` in the plane of ``a`` and ``b``, at
            each angle of ``degrees`` (0 along ``a``, 90 along ``b``)."""
            c, a, b = Vector(centre), Vector(a).normalized(), Vector(b).normalized()
            out = []
            for deg in degrees:
                t = math.radians(deg)
                d = a * math.cos(t) + b * math.sin(t)
                hit, normal, _, _ = self.tree.ray_cast(c + d * 1.0, -d, 1.0)
                # A ray that only met a leg (or nothing) leaves a gap.
                if hit is not None and (hit - c).length < reach:
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

    def strap(pts, width, up, thick=0.006, closed=False):
        """A flat strap ``width`` wide (along ``up``) and ``thick`` thick
        along ``pts``: lying flat on the skin when ``up`` is across it."""
        return V.sweep(smooth_path(pts), V.rect(thick, width, 0.0015), "Leather", (0.4, 0.05), up=tuple(up),
                       closed=closed)

    def ring_strap(fit, centre, a, b, degrees, width, gap=GAP, reach=0.45):
        pts = fit.ring(centre, a, b, degrees, gap, reach)
        steps = [(Vector(p) - Vector(q)).length for p, q in zip(pts, pts[1:])]
        if steps and max(steps) > 3 * sorted(steps)[len(steps) // 2]:
            print(f"harness: a strap round {centre} skips {max(steps) * 100:.1f} cm of skin")
        n = Vector(a).cross(Vector(b)).normalized()
        return strap(pts, width, n), pts

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
        """{name: (Mesh, weights)} of the whole harness on the standing horse:
        ``weights`` ({bone: weight}) carries a stiff piece whole; ``None``, the
        piece takes the weights of the skin under it."""
        pieces = {}
        X, Y, Z = (1, 0, 0), (0, 1, 0), (0, 0, 1)
        # Breast collar: a broad band round the chest from flank to flank,
        # above the points of the shoulders, and the neck strap that holds
        # it up over the neck in front of the withers.
        collar, _ = ring_strap(fit, (0, TRACE[1] - 0.04, TRACE[2]), (0, -1, 0), (1, 0, 0), range(-100, 101, 4), 0.065)
        neck, _ = ring_strap(fit, (0, -0.60, 1.32), (1, 0, 0), (0, -0.45, 1), range(-14, 195, 5), 0.035, reach=0.6)
        piece = V.merge(collar, neck)
        for sx in (1, -1):
            # The trace's buckle at each end of the collar.
            piece.extend(buckle((TRACE[0] * sx + 0.004 * sx, TRACE[1] + 0.03, TRACE[2]), Y, Z, 0.05))
        pieces["Collar"] = (piece, None)
        # The pad over the back behind the withers: stiff, its tree on the
        # two bones of the back under it, wider down the sides than over the
        # spine, with a brass terret on each side of its top.
        y0 = TUG[1]

        def pad_point(u, w):
            th = -0.9 + 1.8 * u
            half = 0.065 + 0.055 * abs(2 * u - 1) ** 1.5
            return fit.onto((0.0, y0 + half * (2 * w - 1), PAD_AXIS), (math.sin(th), 0.0, math.cos(th)), gap=0.008 + 0.025)

        pad = V.thick(V.surface(pad_point, 16, 6, "Leather", (0.4, 0.4)), 0.025)
        for sx in (1, -1):
            base = Vector(pad_point(0.5 + sx * TERRET_AROUND / 1.8, 0.5))
            t = Vector((TERRET[0] * sx, TERRET[1], TERRET[2]))
            pad.extend(ring_at(tuple(t), Y, 0.026, 0.006, "Brass"))
            pad.extend(V.rod([tuple(base + Vector((0, 0, -0.01))), tuple(t + Vector((0, 0, -0.024)))], 0.011, "Brass"))
        pieces["Saddle"] = (pad, PAD_WEIGHTS)
        # The back band round the barrel under the pad, the tugs on it, and
        # the belly band behind the elbows.
        bands, _ = ring_strap(fit, (0, y0, PAD_AXIS), X, Z, range(-40, 221, 5), 0.05)
        for sx in (1, -1):
            bands.extend(ring_at((TUG[0] * sx, TUG[1], TUG[2]), Y, 0.045, 0.010, "Leather"))
            bands.extend(buckle(((TUG[0] - 0.035) * sx, TUG[1], TUG[2] + 0.08), Z, Y))
        belly, _ = ring_strap(fit, (0, y0 + 0.02, 1.15), X, Z, range(-20, -161, -5), 0.06)
        bands.extend(belly)
        pieces["Bands"] = (bands, None)
        # The back strap along the spine from the pad to the crupper.
        spine = [fit.onto((0.0, y, 1.9), Z) for y in [y0 + 0.12 + 0.05 * k for k in range(21)] if y < DOCK[1] - 0.02]
        pieces["BackStrap"] = (strap(spine, 0.035, Y), None)
        # The crupper: a padded leather roll round the dock, under the tail;
        # it goes with the tail's first bone.
        dock, along = Vector(DOCK), Vector(DOCK_ALONG).normalized()
        side = along.cross(Vector(X)).normalized()
        loop = [tuple(dock + (Vector(X) * math.cos(t) + side * math.sin(t)) * DOCK_RADIUS)
                for t in [2 * math.pi * k / 20 for k in range(20)]]
        pieces["Crupper"] = (V.rod(loop, 0.012, "Leather", n=8, closed=True, caps=False, up=tuple(along)), {TAIL: 1.0})
        # The breeching round the quarters below the points of the buttocks,
        # and the hip strap over the croup that holds it up.
        breech, _ = ring_strap(fit, (0, BREECH_Y, BREECH[2]), (0, 1, 0), (1, 0, 0), range(-100, 101, 4), 0.06)
        hip, _ = ring_strap(fit, (0, BREECH_Y + 0.03, 1.25), X, (0, 0.25, 1), range(-25, 206, 5), 0.035, reach=0.6)
        pieces["Breeching"] = (V.merge(breech, hip), None)
        # The bridle: its straps on the skin, the blinkers and the bit stiff
        # on the head.
        straps, stiff, snaffle = bridle(fit)
        pieces["Bridle"] = (straps, None)
        pieces["Blinkers"] = (stiff, {HEAD: 1.0})
        pieces["Bit"] = (snaffle, {HEAD: 1.0})
        return pieces

    def bridle(fit):
        """The straps (the crownpiece over the poll behind the ears, the
        browband in front of them, the throatlatch under the throat, a
        cheekpiece down each side of the face to the bit and the noseband),
        the blinkers beside the eyes, and the snaffle with its rings, as
        three meshes."""
        poll, lip = Vector(POLL), Vector(LIP)
        axis = (lip - poll).normalized()
        front = Vector((0.0, axis.z, -axis.y))
        if front.z < 0:
            front = -front
        X = Vector((1, 0, 0))

        def arc(centre, a, b, ra, rb, degrees):
            c = Vector(centre)
            return [tuple(c + a * (ra * math.cos(math.radians(d))) + b * (rb * math.sin(math.radians(d)))) for d in degrees]

        inner = Vector((0.0, -1.16, 1.70))
        crown_dir = Vector((0.0, 0.15, 1.0)).normalized()
        crown = fit.wrap(arc(inner, X, crown_dir, 0.12, 0.15, range(-30, 211, 5)), inner, reach=0.12)
        throat_c = Vector((0.0, -1.10, 1.64))
        throat_b = Vector((0.0, 0.45, -1.0)).normalized()
        throat = fit.wrap(arc(throat_c, X, throat_b, 0.12, 0.16, range(-5, 186, 5)), throat_c, reach=0.12)
        nose_c = Vector((0.0, -1.335, 1.52))
        nose = fit.wrap(arc(nose_c, X, front, 0.1, 0.1, range(0, 360, 8)), nose_c, reach=0.12)
        brow_c = Vector((0.0, -1.20, 1.73))
        brow_b = Vector((0.0, -0.75, 0.66)).normalized()
        brow = fit.wrap(arc(brow_c, X, brow_b, 0.11, 0.11, range(15, 166, 6)), brow_c, reach=0.12)
        out = V.merge(
            strap(crown, 0.03, crown_dir.cross(X)),
            strap(throat, 0.022, throat_b.cross(X)),
            strap(nose + nose[:1], 0.03, axis),
            strap(brow, 0.025, brow_b.cross(X)),
        )
        stiff, snaffle = V.Mesh(), V.Mesh()
        for sx in (1, -1):
            start = Vector(crown[0] if sx > 0 else crown[-1])
            bit = Vector((BIT[0] * sx, BIT[1], BIT[2]))
            guess = [tuple(start.lerp(bit, k / 14)) for k in range(15)]
            cheek_in = Vector((0.0, 0.0, 0.0))
            cheek = []
            for p in guess:
                cheek_in = Vector((0.0, p[1], p[2])) - front * 0.02
                got = fit.wrap([p], cheek_in, reach=0.1)
                if got:
                    cheek.append(got[0])
            cheek = [tuple(start)] + cheek[1:-1] + [tuple(bit + Vector((0.012 * sx, 0, 0.02)))]
            out.extend(strap(cheek, 0.025, Vector((sx, 0, 0))))
            # The blinker: a padded leather cup with rounded corners beside
            # the eye, on the cheekpiece, standing off the eye's bulge.
            eye = Vector((EYE[0] * sx, EYE[1], EYE[2]))
            look = Vector((sx, -0.12, 0.06)).normalized()
            side = look.cross(Vector((0, 0, 1))).normalized()
            up = side.cross(look).normalized()
            centre = eye + look * BLINKER_OFF + Vector((0, 0.012, 0.006))

            def cup(u, w, centre=centre, side=side, up=up, look=look):
                a, b = 2 * u - 1, 2 * w - 1
                # The square's edge onto a rounded square (a 4-norm circle).
                n2 = math.hypot(a, b)
                k = max(abs(a), abs(b)) / ((a ** 4 + b ** 4) ** 0.25) if n2 > 1e-9 else 1.0
                a, b = a * k * 0.05, b * k * 0.042
                return tuple(centre + side * a + up * b - look * (a * a + b * b) * 2.2)

            stiff.extend(V.thick(V.surface(cup, 10, 8, "Leather", (0.2, 0.2)), 0.008))
            snaffle.extend(ring_at(tuple(bit), (1, 0, 0), 0.032, 0.0055))
        snaffle.extend(V.rod([(BIT[0], BIT[1], BIT[2]), (0.0, BIT[1] - 0.004, BIT[2] - 0.006), (-BIT[0], BIT[1], BIT[2])],
                             0.006, "Iron", n=8))
        return out, stiff, snaffle

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

    # --- henry, standing --------------------------------------------------------

    def load_henry(game=True):
        """Henry's horse imported in Blender with its armature resting as at
        the start of ``REST_CLIP`` and its body standing so with it (in game
        metres when ``game``, else in the file's). Returns (armature, body,
        ``recado.Glb``, the standing node worlds)."""
        before = set(bpy.data.objects)
        bpy.ops.import_scene.gltf(filepath=os.path.abspath(HORSE_GLB), guess_original_bind_pose=False)
        new = [o for o in bpy.data.objects if o not in before]
        arm = next(o for o in new if o.type == "ARMATURE")
        body = next(o for o in new if o.type == "MESH" and o.parent is arm and len(o.data.vertices) > 1000)
        if not (R._identity(arm.matrix_world) and R._identity(body.matrix_world)):
            raise SystemExit("harness: the horse's armature and body must sit at the origin")
        horse = R.Glb(HORSE_GLB)
        stand = horse.worlds(REST_CLIP, 0.0)
        R._rest_on(arm, horse, stand)
        # No clip of Blender's own: a render would play it over the pose.
        arm.animation_data_clear()
        arm.data.pose_position = "REST"
        co = R._scaled(horse.skinned(stand))
        if not game:
            co = co / SCALE
        if len(co) != len(body.data.vertices):
            raise SystemExit("harness: the horse's vertices are not the GLB's")
        body.data.vertices.foreach_set("co", co.ravel())
        body.data.update()
        return arm, body, horse, stand

    def skin_pieces(objects, arm, body):
        """Each piece takes the weights of the skin under it (never the
        mane's, the eyes' nor the ears'), four at most."""
        import animal_rig as rig

        mats = [s.material.name if s.material else "" for s in body.material_slots]
        skin = body.copy()
        skin.data = body.data.copy()
        bpy.context.collection.objects.link(skin)
        names = {g.index: g.name for g in skin.vertex_groups}
        bm = bmesh.new()
        bm.from_mesh(skin.data)
        deform = bm.verts.layers.deform.active
        ears = [f for f in bm.faces if mats[f.material_index] not in SKIN
                or any(any(names[g].startswith(EARS) and w > 0.3 for g, w in v[deform].items()) for v in f.verts)]
        bmesh.ops.delete(bm, geom=ears, context="FACES")
        bm.to_mesh(skin.data)
        bm.free()
        for obj in objects:
            rig.transfer_weights(skin, obj, arm)
            for g in [g for g in obj.vertex_groups if g.name.startswith(EARS)]:
                obj.vertex_groups.remove(g)
            bpy.ops.object.select_all(action="DESELECT")
            obj.select_set(True)
            bpy.context.view_layer.objects.active = obj
            bpy.ops.object.vertex_group_limit_total(group_select_mode="ALL", limit=4)
            bpy.ops.object.vertex_group_normalize_all(lock_active=False)
        bpy.data.objects.remove(skin)

    def _ears_apart(body):
        """Gives the ears' and the eyelids' faces a material of their own, so
        the settle leaves them out: they move on their own, and may brush a
        blinker or the crownpiece for a moment as real ones do."""
        names = {g.index: g.name for g in body.vertex_groups}
        ears = {v.index for v in body.data.vertices
                if any(names[g.group].startswith(EARS) and g.weight > 0.3 for g in v.groups)}
        body.data.materials.append(bpy.data.materials.new("Ears"))
        slot = len(body.data.materials) - 1
        for p in body.data.polygons:
            if any(i in ears for i in p.vertices):
                p.material_index = slot

    def fit_harness():
        """Builds, skins and settles the harness on henry standing (game
        metres). Returns (armature, body, ``recado.Glb``, standing worlds,
        the pieces' objects, their triangles)."""
        V.reset()
        arm, body, horse, stand = load_henry()
        fit = Fitter(skin_tree(body))
        _ears_apart(body)
        pieces = harness_pieces(fit)
        tris = {k: m.triangles() for k, (m, _) in pieces.items()}
        objects = {name: V.to_object(name, mesh) for name, (mesh, _) in pieces.items()}
        skinned = [objects[n] for n, (_, w) in pieces.items() if w is None]
        # The bit goes through the mouth: it is never lifted out of it.
        stiff = [objects[n] for n, (_, w) in pieces.items() if w is not None and n != "Bit"]
        skin_pieces(skinned, arm, body)
        for name, (_, weights) in pieces.items():
            if weights is None:
                continue
            obj = objects[name]
            for bone, w in weights.items():
                obj.vertex_groups.new(name=bone).add(range(len(obj.data.vertices)), w, "REPLACE")
            obj.parent = arm
            obj.modifiers.new("Armature", "ARMATURE").object = arm
        pose = (horse, stand, SKIN)
        # A strap gives way where the moving skin comes up under it; a lift
        # bigger than ``LIFT_MOST`` would be a strap fitted in the wrong place.
        R._settle(skinned, body, pose, GAP_MOVING, most=LIFT_MOST, clips=DRAUGHT_CLIPS)
        R._settle(stiff, body, pose, GAP_MOVING, most=LIFT_MOST, clips=DRAUGHT_CLIPS, reweight=False)
        return arm, body, horse, stand, list(objects.values()), tris

    def build_harness(out_path):
        """Builds, skins, settles and exports the harness; returns its
        triangles per piece."""
        arm, body, _, _, objects, tris = fit_harness()
        # Back to the file's metres, where the armature rests.
        for obj in objects:
            co = np.empty(len(obj.data.vertices) * 3)
            obj.data.vertices.foreach_get("co", co)
            obj.data.vertices.foreach_set("co", co / SCALE)
            obj.data.update()
        bpy.data.objects.remove(body)
        bpy.ops.object.select_all(action="DESELECT")
        for o in [arm, *objects]:
            o.select_set(True)
        bpy.ops.export_scene.gltf(
            filepath=os.path.abspath(out_path),
            export_format="GLB",
            use_selection=True,
            export_yup=True,
            export_skins=True,
            export_animations=False,
            export_vertex_color="ACTIVE",
            export_all_vertex_colors=False,
            export_texcoords=True,
            export_tangents=True,
            export_image_format="NONE",
            export_cameras=False,
            export_lights=False,
            export_materials="EXPORT",
        )
        return tris

    # --- previews -----------------------------------------------------------------

    _TO_B = R._TO_BLENDER if V.bpy is not None else None

    def pose_horse(arm, horse, stand, clip=None, time=0.0):
        """Poses ``arm`` (resting as ``stand``) as the game poses henry at
        ``time`` of ``clip`` (``None``: standing)."""
        worlds = stand if clip is None else horse.worlds(clip, time)
        from_b = np.linalg.inv(_TO_B)
        # Each bone is set over its parent's pose: in the pose position.
        arm.data.pose_position = "POSE"
        bones = sorted(arm.pose.bones, key=lambda b: len(b.parent_recursive))
        for pb in bones:
            k = horse.names.index(pb.name)
            move = _TO_B @ worlds[k] @ np.linalg.inv(stand[k]) @ from_b
            pb.matrix = Matrix((move @ np.array(pb.bone.matrix_local)).tolist())
            bpy.context.view_layer.update()

    def preview_horse(at, harness=None, heading=0.0, clip=None, time=0.0, coat="zaino.jpg", hair=(0.09, 0.07, 0.06, 1.0)):
        """For the vehicles' previews: henry at ``at`` (game metres) with his
        game skin in coat ``coat`` (his mane and tail tinted ``hair``),
        standing (or at ``time`` of ``clip``), in the harness."""
        harness = harness or os.path.join(HERE, "..", "assets", "models", "vehicles", "harness.glb")
        before = set(bpy.data.objects)
        arm, body, horse, stand = load_henry(game=False)
        body_dir, hair_dir = os.path.join(HENRY_DIR, "horse_body"), os.path.join(HENRY_DIR, "horse_hair")
        for slot in body.material_slots:
            name = slot.material.name.split(".")[0] if slot.material else ""
            if name == "Horse":
                slot.material = V._maps_material(f"Henry_{coat}", os.path.join(body_dir, coat),
                                                  os.path.join(body_dir, "normal_gl.jpg"), os.path.join(body_dir, "roughness.jpg"))
            elif name == "Hair":
                slot.material = V._maps_material("Henry_hair", os.path.join(hair_dir, "color.jpg"),
                                                  os.path.join(hair_dir, "normal_gl.jpg"), os.path.join(hair_dir, "roughness.jpg"),
                                                  tint=hair, opacity=os.path.join(hair_dir, "opacity.jpg"))
        if os.path.exists(harness):
            mine = set(bpy.data.objects)
            bpy.ops.import_scene.gltf(filepath=os.path.abspath(harness), guess_original_bind_pose=False)
            new = [o for o in bpy.data.objects if o not in mine]
            for o in new:
                if o.type != "MESH":
                    continue
                o.parent = arm
                o.matrix_parent_inverse = Matrix()
                for mod in o.modifiers:
                    if mod.type == "ARMATURE":
                        mod.object = arm
                for slot in o.material_slots:
                    base = slot.material.name.split(".")[0] if slot.material else ""
                    if base in V.SET_OF:
                        slot.material = V.material(base, label=f"{base}_preview")
            for o in new:
                if o.type == "ARMATURE":
                    bpy.data.objects.remove(o)
        arm.location = Vector(at)
        arm.scale = (SCALE, SCALE, SCALE)
        arm.rotation_euler[2] += heading
        pose_horse(arm, horse, stand, clip, time)
        return [o for o in bpy.data.objects if o not in before], arm

    def preview_ox(at, with_yoke=False, yoke=None, frame=6, coat="hosco.jpg"):
        """Our ox (``tools/cattle.py --ox``) at ``at`` walking, its baked
        criollo coat, and on the left ox the yoke."""
        ox_dir = os.path.join(HERE, "..", "assets", "models", "vehicles")
        _, arm = V.import_animal(os.path.join(ox_dir, "ox.glb"), at, os.path.join(ox_dir, "ox_body"),
                                 os.path.join(ox_dir, "ox_hair"), scale=BULL_SCALE, coat_map=coat, frame=frame)
        yoke = yoke or os.path.join(ox_dir, "yoke.glb")
        if with_yoke and os.path.exists(yoke):
            V.import_pieces(yoke, arm, at, scale=BULL_SCALE)


if __name__ == "__main__" and V.bpy is not None:
    argv = V.args()
    out_dir = argv[0]
    os.makedirs(out_dir, exist_ok=True)
    tris = build_harness(os.path.join(out_dir, "harness.glb"))
    print(f"harness: {sum(tris.values())} triangles: {tris}")
    yoke = yoke_pieces()
    export_pieces(yoke, os.path.join(out_dir, "yoke.glb"))
    print(f"yoke: {yoke['Yoke@Head'].triangles()} triangles")
    prev = V.option(argv, "--preview")
    if prev:
        shots = [((3.4, -0.4, 1.5), (0, -0.35, 1.25), 38), ((2.0, -3.4, 1.9), (0, -0.6, 1.35), 42),
                 ((-2.4, 2.6, 2.0), (0, 0.1, 1.3), 40), ((0.9, -2.2, 1.85), (0, -1.25, 1.62), 60)]
        for clip, time, name in ((None, 0.0, "harness"), ("Walk", 0.25, "harness_walk"), ("Gallop", 0.4, "harness_gallop")):
            V.reset()
            preview_horse((0, 0, 0), os.path.join(out_dir, "harness.glb"), clip=clip, time=time)
            V.render_sheet(os.path.join(prev, f"{name}.png"), shots)
        V.reset()
        preview_ox((OX_SPACING * BULL_SCALE / 2, 0, 0), True, os.path.join(out_dir, "yoke.glb"))
        preview_ox((-OX_SPACING * BULL_SCALE / 2, 0, 0), coat="overo.jpg")
        V.render_sheet(os.path.join(prev, "yoke.png"),
                       [((2.1, -4.6, 2.3), (0.0, -1.1, 1.3), 40), ((4.0, -1.6, 2.0), (0.0, -0.9, 1.2), 40)])
