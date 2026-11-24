"""Turn the Sketchfab "Horse Drawn Carriage 5" by gogiart (CC-BY-4.0) into
the game's second sulky, the gig, in Blender, by script.

    blender -b --factory-startup --python tools/sulky_gig.py -- SOURCE.glb OUT.glb [--textures DIR] [--preview SHEET.png]

SOURCE is the Sketchfab GLB download (``horse_drawn_carriage_5.glb``,
https://sketchfab.com/3d-models/horse-drawn-carriage-5-f3db55361be842e49db81c444f78518f):
a two-wheeled, one-horse gig with a seat for two on elliptic springs, red
wheels, shafts and dash, a planked body and a cowhide cushion; 284,382
triangles in 12 meshes, 9 materials with one colour map each (256 to 1024
px), no armature. It is read only by Blender's glTF importer (data, never
executed); nothing is written next to it.

It lives alongside ``tools/sulky.py``'s sulky (the game draws about half of
its sulkies with each, ``pampa_render::vehicle::Variant``) and keeps the
sulky's contract, so the same horse, harness, driver, reins and breeching
straps fit it:

- the whole model is turned from the import's frame (Z up, shafts towards
  -X, the axle along Y) to the vehicles' (facing -Y, +X its left) and
  scaled by ``SCALE`` so that its wheels are the sulky's, 0.70 m over the
  tyre (``WHEEL_RADIUS``), on the sulky's axle line (``AXLE_Y``); the origin
  is the ground under the horse, ``Animal.0``;
- each wheel (its tyre, felloe, spokes and hub: every loose part further
  out than ``WHEEL_Y`` on one side; not the axle, which runs across) is its own node, ``Wheel.L`` and
  ``Wheel.R``, at its hub, which the game spins with the distance rolled;
  the rest is ``Body``, both with their light ``_LOD`` meshes (30 %), and
  the ``Seat`` marker (``SEAT``) is the top of the cushion where the driver
  sits;
- the shafts are bent onto henry: in front of ``JOIN_Y`` (where they leave
  the body) each runs on the sulky's line along the horse
  (``sulky.shaft_front``: through the middle of its harness's tug, past the
  breeching's end), tapering to ``TIP_TAPER`` of its section at the tip;
  behind it they keep the gig's own line (``SOURCE_SHAFT``). So the
  breeching straps the game draws tie to them where they tie to the
  sulky's (``BREECH_TIE``, ``SULKY_BREECH_TIE`` in the game), and a
  leather band is buckled round each shaft there; the traces run from the
  breast collar along the flank (``sulky.trace``) to an iron hook on the
  inner side of each shaft (``TRACE_HOOK_Y``): the gig has no singletree;
- the heavy meshes are decimated to ``BUDGET`` (the cushion, the dash, the
  springs, the seat's roll, the shafts after bending); the wheels, a few
  thousand triangles, are kept whole so they stay round;
- every vertex gets the vehicles' road-dirt tint (``vehicle.ground_dirt``,
  ``vehicle.rim_dirt`` on the wheels), which the game multiplies into the
  materials;
- the materials are renamed after their texture sets (``MATERIALS``); with
  ``--textures DIR`` each source colour map is written to
  ``DIR/<set>/color.jpg`` (halved down to ``MAX_SIZE`` where listed) with a ``roughness.jpg`` and a ``normal_gl.jpg``
  made from it (``maps``), which ``tools/textures_to_ktx2.sh`` turns into
  the KTX2 the game loads.

The output is deterministic. See tools/README.md and
``assets/models/vehicles/CREDITS.md``.
"""

import math
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)

import vehicle as V  # noqa: E402
import vehicle_harness as H  # noqa: E402
import sulky as S  # noqa: E402

TITLE = "Horse Drawn Carriage 5"
AUTHOR = "gogiart"
SOURCE_URL = "https://sketchfab.com/3d-models/horse-drawn-carriage-5-f3db55361be842e49db81c444f78518f"
LICENSE = "CC-BY-4.0"

# The source as Blender's glTF importer places it (its units): the wheels'
# hubs (x, z) and their radius over the tyre, measured on its tyres.
SOURCE_HUB = (4.1277, -1.5258)
SOURCE_RADIUS = 2.5432
# A loose part every vertex of which lies further than this from the
# middle (the source's |y|) belongs to a wheel.
WHEEL_Y = 2.0

WHEEL_RADIUS = S.WHEEL_RADIUS
SCALE = WHEEL_RADIUS / SOURCE_RADIUS
AXLE_Y = S.AXLE_Y

# Where the shafts leave the gig's body: behind it they keep their own line,
# in front of it they are bent onto the sulky's along henry.
JOIN_Y = 1.40
# The source shafts' middle line behind ``JOIN_Y``, in game metres: straight
# in plan, ``SHAFT_X`` out, and (y, z) along it (measured on their rings).
SHAFT_X = 0.4343
SOURCE_SHAFT = [(1.40, 0.9909), (1.5733, 0.9611), (1.7460, 0.9387), (1.9056, 0.9261), (2.0561, 0.9205),
                (2.2009, 0.9191), (2.8444, 0.9191)]
# Its slope dz/dy at ``JOIN_Y``, so the bent stretch leaves it smoothly.
JOIN_SLOPE = -0.189
# The source shafts' half width and half height (6.1 by 3.7 cm), and the
# share of it left at the tips (as the sulky's taper): they run through the
# tugs' rings, 7 cm across inside.
SECTION = (0.0307, 0.0183)
TIP_TAPER = 0.72
# Where along each shaft its trace is hooked, on the shaft's inner side.
TRACE_HOOK_Y = 1.20
# Where the breeching straps are tied: on the sulky's line, so the game's
# ``SULKY_BREECH_TIE`` holds for both.
BREECH_TIE = S.BREECH_TIE

# The top of the cushion in the middle, where the driver sits: 15 cm ahead
# of where the back's roll rises (measured on the cushion).
SEAT = (0.0, 2.45, 1.47)

# Source material: (the game's material name, its texture set, roughness,
# bump strength of the normal map made from the colour, metal).
MATERIALS = {
    "acmat_0": ("Gig_Paint", "gig_paint", 0.42, 0.6, 0.0),  # red paint: wheels, shafts, dash
    "acmat_1": ("Gig_Planks", "gig_planks", 0.78, 4.0, 0.0),  # the body's planks
    "acmat_2": ("Gig_Iron", "gig_iron", 0.55, 1.5, 0.7),  # springs, axle, bolts
    "acmat_3": ("Gig_Steel", "gig_steel", 0.5, 1.0, 0.7),  # the tyres
    "acmat_4": ("Gig_Hub", "gig_hub", 0.42, 0.4, 0.0),  # the red hubs
    "acmat_5": ("Gig_Leather", "gig_leather", 0.6, 5.0, 0.0),  # the seat's black roll
    "acmat_6": ("Gig_Hide", "gig_hide", 0.85, 1.5, 0.0),  # the cowhide cushion
    "acmat_7": ("Gig_Wood", "gig_wood", 0.78, 4.0, 0.0),  # the body's sides
    "acmat_8": ("Gig_Floor", "gig_floor", 0.8, 4.0, 0.0),  # the step
}
# Texture sets written smaller than their source map, at most this many
# pixels a side: the step's floor comes at 1024 px for a few small faces.
MAX_SIZE = {"gig_floor": 512}

# Triangles each heavy source mesh keeps (on the body; the shafts' count is
# both shafts' after bending). The others keep theirs.
BUDGET = {
    "Object_11": 3200,  # the dash
    "Object_6": 2400,  # the cushion and the seat's sides
    "Object_13": 1500,  # five springs
    "Object_3": 1400,  # the seat's rolled back
    "Object_7": 500,  # the step
    "Object_8": 2400,  # the body's frame and sides
    "Object_10": 2000,  # the shafts
}
# The whole gig's triangles at most (the sulky has 14,540), LODs apart.
MOST_TRIANGLES = 21000


def to_game(p):
    """A point of the source (Blender's import of it) in game metres, in the
    vehicles' frame: the source faces -X with its left on -Y."""
    return (-p[1] * SCALE, (p[0] - SOURCE_HUB[0]) * SCALE + AXLE_Y, (p[2] - SOURCE_HUB[1]) * SCALE + WHEEL_RADIUS)


def join_point(sx):
    return (SHAFT_X * sx, JOIN_Y, SOURCE_SHAFT[0][1])


def shaft_front(sx):
    """The bent stretch of shaft ``sx``, tip to ``JOIN_Y``: the sulky's line
    along henry, then on to where the gig's own line leaves the body."""
    p = S.shaft_front(sx)
    hind, join = p[-1], join_point(sx)
    back = 0.25
    p += V.bezier(hind, (hind[0], hind[1] + back, hind[2]),
                  (join[0], join[1] - back, join[2] - JOIN_SLOPE * back), join, 6)[1:]
    return p


def shaft_path(sx):
    """The middle line of shaft ``sx`` (+1 the left), tip to tail."""
    return shaft_front(sx) + [(SHAFT_X * sx, y, z) for y, z in SOURCE_SHAFT[1:]]


def shaft_at(sx, y):
    """The point of a gig shaft's middle line at ``y``."""
    return S.shaft_at(sx, y, shaft_path(sx))


def half_section(y):
    """A shaft's half width and half height at ``y``: the source's, tapered
    in front of ``JOIN_Y`` by the share of the way from there to the tip
    (as ``bend`` tapers it)."""
    if y >= JOIN_Y:
        return SECTION
    p = shaft_front(1)[::-1]
    lengths = [V.length(V.sub(b, a)) for a, b in zip(p, p[1:])]
    done = 0.0
    for (a, b), d in zip(zip(p, p[1:]), lengths):
        if b[1] <= y:
            done += d * (a[1] - y) / (a[1] - b[1])
            break
        done += d
    taper = 1.0 - (1.0 - TIP_TAPER) * min(1.0, done / sum(lengths))
    return (SECTION[0] * taper, SECTION[1] * taper)


def trace_hook(sx, half_width):
    """Where trace ``sx`` is hooked: on the inner side of its shaft (of
    ``half_width``) at ``TRACE_HOOK_Y``."""
    c = shaft_at(sx, TRACE_HOOK_Y)
    return (c[0] - (half_width + 0.008) * sx, c[1], c[2])


def fittings():
    """What the gig carries from the harness: the traces to their hooks,
    the hooks, and the breeching straps' ends buckled round the shafts (the
    game draws the straps themselves, as on the sulky)."""
    m = V.Mesh()
    width = half_section(TRACE_HOOK_Y)[0]
    for sx in (1.0, -1.0):
        hook = trace_hook(sx, width)
        m.extend(S.trace(sx, hook))
        c = shaft_at(sx, TRACE_HOOK_Y)
        side = (c[0] - width * sx, c[1], c[2])
        m.extend(V.rod([side, V.add(hook, (0.0, 0.0, 0.012)), V.add(hook, (0.0, 0.025, 0.0))], 0.006, "Iron", n=6))
        m.extend(S.breeching_wrap(sx, shaft_at(sx, BREECH_TIE), half_section(BREECH_TIE)))
    return m


def shrink(rgb, most):
    """``rgb`` (rows top-down) halved, each pixel the mean of its 2x2
    block, until no side is over ``most`` pixels. An odd side would lose a
    row or column (and shift the map against its UVs), so it is refused."""
    while max(rgb.shape[:2]) > most:
        h, w = rgb.shape[:2]
        if h % 2 or w % 2:
            raise ValueError(f"cannot halve a {h}x{w} map evenly")
        rgb = rgb.reshape(h // 2, 2, w // 2, 2, -1).mean(axis=(1, 3))
    return rgb


def maps(rgb, roughness, bump):
    """The roughness and the OpenGL normal map of a colour map (``rgb``,
    0..1, rows top-down): its luminance, softly blurred, as the height of
    the grooves and grain (darker is lower), and the roughness a little
    rougher where it is darker."""
    import numpy as np
    import tack as T

    lum = rgb[..., :3] @ np.array([0.2126, 0.7152, 0.0722])
    h = lum.copy()
    for _ in range(2):
        h = (h + np.roll(h, 1, 0) + np.roll(h, -1, 0) + np.roll(h, 1, 1) + np.roll(h, -1, 1)) / 5.0
    normal = T.normals(h, bump)
    rough = np.clip(roughness + 0.25 * (lum.mean() - lum), 0.05, 1.0)
    return rough, normal


if V.bpy is not None:
    import bpy
    import bmesh
    from mathutils import Matrix

    try:
        import numpy as np
    except ImportError:  # pragma: no cover - Blender ships numpy
        np = None

    def game_matrix():
        """``to_game`` as a matrix (a turn about Z, a scale, a move)."""
        turn = Matrix(((0, -1, 0, 0), (1, 0, 0, 0), (0, 0, 1, 0), (0, 0, 0, 1)))
        move = Matrix.Translation((0.0, AXLE_Y - SOURCE_HUB[0] * SCALE, WHEEL_RADIUS - SOURCE_HUB[1] * SCALE))
        return move @ Matrix.Scale(SCALE, 4) @ turn

    def triangles(obj):
        return sum(len(p.vertices) - 2 for p in obj.data.polygons)

    def select(objs, active):
        for o in bpy.context.scene.objects:
            o.select_set(o in objs)
        bpy.context.view_layer.objects.active = active

    def import_source(path):
        """The source's meshes in game metres, unparented: {name: object}."""
        bpy.ops.import_scene.gltf(filepath=os.path.abspath(path))
        m = game_matrix()
        meshes = {}
        for o in list(bpy.data.objects):
            if o.type != "MESH":
                continue
            o.data = o.data.copy()
            o.data.transform(m @ o.matrix_world)
            o.parent = None
            o.matrix_world = Matrix()
            meshes[o.name] = o
        for o in list(bpy.data.objects):
            if o.type != "MESH":
                bpy.data.objects.remove(o)
        return meshes

    def split(obj):
        """``obj`` cut into its loose parts."""
        select([obj], obj)
        bpy.ops.object.mode_set(mode="EDIT")
        bpy.ops.mesh.select_all(action="SELECT")
        bpy.ops.mesh.separate(type="LOOSE")
        bpy.ops.object.mode_set(mode="OBJECT")
        return list(bpy.context.selected_objects)

    def join(objs, name):
        objs = [o for o in objs if o is not None]
        if len(objs) > 1:
            select(objs, objs[0])
            bpy.ops.object.join()
        obj = objs[0]
        obj.name = name
        obj.data.name = name
        return obj

    def coords(obj):
        co = np.empty(len(obj.data.vertices) * 3)
        obj.data.vertices.foreach_get("co", co)
        return co.reshape(-1, 3)

    def set_coords(obj, co):
        obj.data.vertices.foreach_set("co", co.reshape(-1))
        obj.data.update()

    def loops(obj, step=0.04):
        """Edge loops across ``obj`` (a shaft) every ``step`` in front of
        ``JOIN_Y``, so its bend is smooth (its own rings lie 10 cm apart)."""
        bm = bmesh.new()
        bm.from_mesh(obj.data)
        y0 = min(v.co.y for v in bm.verts)
        y = JOIN_Y
        while y > y0 + 0.01:
            geom = bm.verts[:] + bm.edges[:] + bm.faces[:]
            bmesh.ops.bisect_plane(bm, geom=geom, plane_co=(0, y, 0), plane_no=(0, 1, 0))
            y -= step
        bmesh.ops.triangulate(bm, faces=[f for f in bm.faces if len(f.verts) > 4])
        bm.to_mesh(obj.data)
        bm.free()

    def polyline(points):
        """Points along a line and their distances along it."""
        p = np.array(points, float)
        s = np.concatenate([[0.0], np.cumsum(np.linalg.norm(np.diff(p, axis=0), axis=1))])
        return p, s

    def along(p, s, at):
        """The point and the unit tangent at distance ``at`` along
        (``p``, ``s``), beyond its ends along their last stretch."""
        k = int(np.clip(np.searchsorted(s, at) - 1, 0, len(s) - 2))
        d = p[k + 1] - p[k]
        t = (at - s[k]) / (s[k + 1] - s[k])
        return p[k] + d * t, d / np.linalg.norm(d)

    def turn(a, b):
        """The rotation taking unit ``a`` to unit ``b``."""
        v, c = np.cross(a, b), float(np.dot(a, b))
        k = np.array([[0, -v[2], v[1]], [v[2], 0, -v[0]], [-v[1], v[0], 0]])
        return np.eye(3) + k + k @ k / (1.0 + c)

    def bend(obj, sx):
        """Bends shaft ``sx`` (its loose part ``obj``) in front of
        ``JOIN_Y`` onto ``shaft_front``: each vertex keeps its offset from
        the shaft's middle line (turned with the line, tapering towards the
        tip) at the same share of the way from ``JOIN_Y`` to the tip."""
        co = coords(obj)
        # The source's middle line from the join (a loop lies there) to the
        # tip: the middle of each ring (its vertices a few mm apart in y,
        # the rings 4 cm apart or more).
        front = co[co[:, 1] < JOIN_Y + 0.001]
        front = front[np.argsort(-front[:, 1])]
        cuts = np.flatnonzero(-np.diff(front[:, 1]) > 0.003) + 1
        rings = [((r[:, 0].min() + r[:, 0].max()) / 2, r[:, 1].mean(), (r[:, 2].min() + r[:, 2].max()) / 2)
                 for r in np.split(front, cuts)]
        if np.linalg.norm(np.array(rings[0]) - np.array(join_point(sx))) > 0.003:
            raise SystemExit(f"sulky_gig: the source shaft {sx:+} leaves the body at {rings[0]}, not {join_point(sx)}")
        src, src_s = polyline(rings)
        dst, dst_s = polyline(shaft_front(sx)[::-1])
        k = dst_s[-1] / src_s[-1]
        out = co.copy()
        for i, v in enumerate(co):
            if v[1] >= JOIN_Y:
                continue
            # The nearest point of the source line (its rings lie across it).
            j = int(np.clip(np.searchsorted(-src[:, 1], -v[1]) - 1, 0, len(src) - 2))
            d = src[j + 1] - src[j]
            t = np.clip((v[1] - src[j][1]) / d[1], 0.0, 1.0)
            c = src[j] + d * t
            s = src_s[j] + t * (src_s[j + 1] - src_s[j])
            q, tangent = along(dst, dst_s, s * k)
            taper = 1.0 - (1.0 - TIP_TAPER) * min(1.0, s * k / dst_s[-1])
            out[i] = q + taper * (turn(d / np.linalg.norm(d), tangent) @ (v - c))
        set_coords(obj, out)

    def weld(obj, outward=False):
        """Welds ``obj``'s vertices (the source's meshes come split at every
        UV and normal seam; the UVs stay on the corners) and, ``outward``,
        turns every closed part's faces to face out: the source's wheels
        have their side rings (the tyre's, the felloe's, the hubs') facing
        into the wheel, which the game, drawing one side of each face,
        left out on the outside, so the far wheel showed through the near
        one's felloe."""
        bm = bmesh.new()
        bm.from_mesh(obj.data)
        bmesh.ops.remove_doubles(bm, verts=bm.verts, dist=1e-4)
        if outward:
            bmesh.ops.recalc_face_normals(bm, faces=bm.faces)
        bm.to_mesh(obj.data)
        bm.free()

    def decimate(obj, tris):
        """``obj`` collapsed down to ``tris`` triangles, symmetric about X,
        welded first, or the collapse tears holes along the seams."""
        weld(obj)
        have = triangles(obj)
        if have <= tris:
            return
        mod = obj.modifiers.new("Decimate", "DECIMATE")
        mod.ratio = tris / have
        mod.use_collapse_triangulate = True
        mod.use_symmetry = True
        mod.symmetry_axis = "X"
        select([obj], obj)
        bpy.ops.object.modifier_apply(modifier=mod.name)

    def shade(obj, angle=35.0):
        """Smooth below ``angle``, sharp above (the source's own normals are
        dropped: the bends and the decimation changed the faces)."""
        select([obj], obj)
        if obj.data.has_custom_normals:
            bpy.ops.mesh.customdata_custom_splitnormals_clear()
        for p in obj.data.polygons:
            p.use_smooth = True
        obj.data.set_sharp_from_angle(angle=math.radians(angle))

    def tint(obj, fn, origin=(0.0, 0.0, 0.0)):
        """The vehicles' ``Tint`` colours on ``obj``: ``fn`` of each
        vertex's position from ``origin``."""
        me = obj.data
        col = me.color_attributes.get("Tint") or me.color_attributes.new("Tint", "BYTE_COLOR", "CORNER")
        me.color_attributes.active_color = col
        for loop in me.loops:
            p = me.vertices[loop.vertex_index].co
            c = fn((p.x - origin[0], p.y - origin[1], p.z - origin[2]))
            col.data[loop.index].color = (c[0], c[1], c[2], 1.0)

    def write_maps(out):
        """Each source colour map and the maps made from it, as the JPG
        sources of the KTX2 sets."""
        import tack as T

        for mat in bpy.data.materials:
            if mat.name not in MATERIALS or not mat.node_tree:
                continue
            _, folder, rough, bump, _ = MATERIALS[mat.name]
            img = next(n.image for n in mat.node_tree.nodes if n.type == "TEX_IMAGE" and n.image)
            w, h = img.size
            px = np.empty(w * h * 4, np.float32)
            img.pixels.foreach_get(px)
            # Blender's rows run bottom-up; the colour stays as stored (sRGB).
            rgb = px.reshape(h, w, 4)[::-1, :, :3].astype(float)
            if folder in MAX_SIZE:
                rgb = shrink(rgb, MAX_SIZE[folder])
                # Slopes are per pixel: fewer pixels, steeper; keep the grooves as deep.
                bump *= rgb.shape[1] / w
            target = os.path.join(out, folder)
            os.makedirs(target, exist_ok=True)
            roughness, normal = maps(rgb, rough, bump)
            T.save(rgb, os.path.join(target, "color.jpg"), quality=92)
            T.save(roughness, os.path.join(target, "roughness.jpg"), quality=92)
            T.save(normal, os.path.join(target, "normal_gl.jpg"), quality=92)
            print(f"{folder}: {rgb.shape[1]}x{rgb.shape[0]} from {img.name} ({w}x{h})")

    def rename_materials(textures):
        """Each source material becomes its game material (by name); for the
        previews, drawn with the written maps (times the tint) when there."""
        for mat in list(bpy.data.materials):
            if mat.name not in MATERIALS:
                continue
            name, folder, _, _, metal = MATERIALS[mat.name]
            maps_dir = os.path.join(textures or "", folder)
            new = None
            if textures and os.path.exists(os.path.join(maps_dir, "color.jpg")):
                new = V._maps_material(name, *(os.path.join(maps_dir, f"{k}.jpg") for k in ("color", "normal_gl", "roughness")),
                                       vertex="Tint")
                new.node_tree.nodes["Principled BSDF"].inputs["Metallic"].default_value = metal
            for o in bpy.data.objects:
                for slot in getattr(o, "material_slots", []):
                    if slot.material is mat and new is not None:
                        slot.material = new
            if new is None:
                mat.name = name
            else:
                bpy.data.materials.remove(mat)

    def build(source, textures=None):
        """The gig's Blender objects (``textures``: where its maps go; the
        previews draw them from there, or from the game's): its root."""
        meshes = import_source(source)
        if textures:
            write_maps(textures)
        wheels = {"Wheel.L": [], "Wheel.R": []}
        body, shafts = [], {1.0: None, -1.0: None}
        for name in sorted(meshes):
            obj = meshes[name]
            parts = split(obj)
            mine = []
            for part in parts:
                co = coords(part)
                if co[:, 0].min() > WHEEL_Y * SCALE:
                    wheels["Wheel.L"].append(part)
                elif co[:, 0].max() < -WHEEL_Y * SCALE:
                    wheels["Wheel.R"].append(part)
                elif name == "Object_10":
                    shafts[1.0 if co[:, 0].mean() > 0 else -1.0] = part
                else:
                    mine.append(part)
            if mine:
                body.append((name, join(mine, name)))
        for sx, part in shafts.items():
            loops(part)
            bend(part, sx)
        shaft = join(list(shafts.values()), "Object_10")
        body.append(("Object_10", shaft))
        for name, obj in body:
            if name in BUDGET:
                decimate(obj, BUDGET[name])
            shade(obj)
            tint(obj, V.ground_dirt())
            print(f"sulky_gig: {name} {triangles(obj)} triangles")
        root = V.empty("SulkyGig", (0, 0, 0), size=0.5)
        root["animals"] = 1
        root["animal"] = "horse"
        root["source"] = f"{TITLE} by {AUTHOR}, {SOURCE_URL}, {LICENSE}"
        node = V.empty("Body", (0, 0, 0), root)
        extra = V.to_object("Body_Fittings", fittings().tint(V.ground_dirt()))
        mesh = join([o for _, o in body] + [extra], "Body_Mesh")
        mesh.parent = node
        V.lod(mesh, 0.3, "Body_LOD").parent = node
        for name, parts in wheels.items():
            w = join(parts, f"{name}_Mesh")
            weld(w, outward=True)
            lo, hi = coords(w).min(0), coords(w).max(0)
            hub = ((lo[0] + hi[0]) / 2, AXLE_Y, WHEEL_RADIUS)
            set_coords(w, coords(w) - np.array(hub))
            shade(w)
            tint(w, V.rim_dirt(WHEEL_RADIUS))
            e = V.empty(name, hub, root, size=WHEEL_RADIUS)
            e["radius"] = WHEEL_RADIUS
            w.parent = e
            V.lod(w, 0.3, f"{name}_LOD").parent = e
        V.empty("Seat", SEAT, root, size=0.15)
        V.empty("Animal.0", (0.0, 0.0, 0.0), root, size=0.15)
        rename_materials(textures or V.TEXTURE_DIR)
        return root

    def report():
        tris = {o.name: triangles(o) for o in bpy.data.objects if o.type == "MESH"}
        full = sum(n for k, n in tris.items() if not k.endswith("_LOD"))
        print(f"sulky_gig: {full} triangles ({', '.join(f'{k} {n}' for k, n in sorted(tris.items()))})")
        print(f"sulky_gig: materials {sorted({s.material.name for o in bpy.data.objects if o.type == 'MESH' for s in o.material_slots})}")
        if full > MOST_TRIANGLES:
            raise SystemExit(f"sulky_gig: {full} triangles, over {MOST_TRIANGLES}")


PREVIEW = [
    ((7.2, 1.0, 1.4), (0.0, 0.9, 1.0), 38),
    ((5.2, -4.2, 2.3), (0.0, 0.8, 1.0), 36),
    ((-3.4, 6.6, 2.2), (0.0, 1.4, 1.0), 38),
]

if __name__ == "__main__" and V.bpy is not None:
    argv = V.args()
    source, out = argv[0], argv[1]
    V.reset()
    build(source, V.option(argv, "--textures"))
    report()
    V.export(out)
    sheet = V.option(argv, "--preview")
    if sheet:
        H.preview_horse((0.0, 0.0, 0.0))
        V.render_sheet(sheet, PREVIEW)
