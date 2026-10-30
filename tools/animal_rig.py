"""Shared helpers to model, rig and animate an animal in Blender, by script.

Imported by the per-species scripts (``tools/rhea.py``); run those headless:

    blender -b --factory-startup --python tools/rhea.py -- OUT.glb [--preview DIR]

The model is built in metres, Z up, facing Blender's -Y (glTF +Z once
exported Y up, like the other fauna models the game loads). Steps:

- ``loft`` builds a closed tube along a smooth path with an elliptical
  cross-section per control point (bodies, necks, legs, toes, feather lobes);
- ``fuse_dense`` fuses the parts into one dense watertight sculpt (voxel
  remesh); ``decimated_copy`` and ``unwrap`` make the game mesh from it;
- ``bake_to`` bakes the sculpt's colour, normals and occlusion into images
  over the game mesh's UVs (Cycles), ``save_jpeg`` writes them as the
  sources of a KTX2 set (``tools/textures_to_ktx2.sh``);
- ``feather_atlas`` paints loose feathers with an opacity mask, and
  ``cards`` builds the alpha-tested feather cards that use it;
- ``build_armature``, ``bind`` and ``transfer_weights`` add the bones and
  skin the meshes (automatic heat weights, at most four bones a vertex);
- ``Pose`` turns per-frame bone directions into the bones' local pose,
  so a clip is a plain Python function of time (``bake_clip``);
- ``render_previews`` renders still PNGs with Eevee, and ``export`` writes the
  GLB (Y up, skinned, UVs and tangents, one clip per action, no images: the
  game puts the KTX2 maps on the materials).

Everything is deterministic: the same script gives the same model.
"""

import math
import os

import bpy
import bmesh
import mathutils
from mathutils import Matrix, Quaternion, Vector

FPS = 30


def reset():
    bpy.ops.wm.read_factory_settings(use_empty=True)
    scene = bpy.context.scene
    scene.render.fps = FPS
    scene.unit_settings.system = "METRIC"
    return scene


def _catmull(points, samples):
    """Points on a Catmull-Rom spline through ``points``, ``samples`` per span,
    with the parameter (span index plus fraction) of each."""
    out = []
    n = len(points)
    for i in range(n - 1):
        p0 = points[max(i - 1, 0)]
        p1, p2 = points[i], points[i + 1]
        p3 = points[min(i + 2, n - 1)]
        for s in range(samples):
            t = s / samples
            t2, t3 = t * t, t * t * t
            p = 0.5 * (
                2 * p1
                + (-p0 + p2) * t
                + (2 * p0 - 5 * p1 + 4 * p2 - p3) * t2
                + (-p0 + 3 * p1 - 3 * p2 + p3) * t3
            )
            out.append((p, i + t))
    out.append((points[-1], n - 1.0))
    return out


def loft(name, points, radii, side=(1.0, 0.0, 0.0), ring=16, samples=4, caps=True):
    """A closed tube through ``points`` (Catmull-Rom), with the cross-section
    of control point ``i`` an ellipse of half-widths ``radii[i] = (a, b)``:
    ``a`` along ``side`` (made perpendicular to the path), ``b`` across it.
    Rounded caps close both ends."""
    pts = [Vector(p) for p in points]
    path = _catmull(pts, samples)
    side = Vector(side).normalized()

    def radius_at(u):
        i = min(int(u), len(radii) - 2)
        f = u - i
        a = radii[i][0] * (1 - f) + radii[i + 1][0] * f
        b = radii[i][1] * (1 - f) + radii[i + 1][1] * f
        return a, b

    rings = []
    for k, (p, u) in enumerate(path):
        prev = path[max(k - 1, 0)][0]
        nxt = path[min(k + 1, len(path) - 1)][0]
        tangent = (nxt - prev).normalized()
        s = (side - tangent * side.dot(tangent)).normalized()
        up = tangent.cross(s)
        rings.append((p, tangent, s, up, radius_at(u)))

    def ring_at(p, s, up, a, b):
        return [
            p + s * (a * math.cos(2 * math.pi * j / ring)) + up * (b * math.sin(2 * math.pi * j / ring))
            for j in range(ring)
        ]

    verts, faces, loops = [], [], []
    if caps:
        # Rounded start: rings shrinking back along the tangent.
        p, t, s, up, (a, b) = rings[0]
        r = 0.5 * (a + b)
        for c in (3, 2, 1):
            ang = c * math.pi / 8
            loops.append(ring_at(p - t * r * math.sin(ang), s, up, a * math.cos(ang), b * math.cos(ang)))
    for p, t, s, up, (a, b) in rings:
        loops.append(ring_at(p, s, up, a, b))
    if caps:
        p, t, s, up, (a, b) = rings[-1]
        r = 0.5 * (a + b)
        for c in (1, 2, 3):
            ang = c * math.pi / 8
            loops.append(ring_at(p + t * r * math.sin(ang), s, up, a * math.cos(ang), b * math.cos(ang)))
    for loop in loops:
        verts.extend(loop)
    for li in range(len(loops) - 1):
        base, nxt = li * ring, (li + 1) * ring
        for j in range(ring):
            faces.append((base + j, base + (j + 1) % ring, nxt + (j + 1) % ring, nxt + j))
    # Close the ends with a centre vertex each.
    first = len(verts)
    verts.append(sum(loops[0], Vector()) / ring)
    last = len(verts)
    verts.append(sum(loops[-1], Vector()) / ring)
    for j in range(ring):
        faces.append((first, (j + 1) % ring, j))
        tail = (len(loops) - 1) * ring
        faces.append((last, tail + j, tail + (j + 1) % ring))
    mesh = bpy.data.meshes.new(name)
    mesh.from_pydata([tuple(v) for v in verts], [], faces)
    mesh.update()
    obj = bpy.data.objects.new(name, mesh)
    bpy.context.collection.objects.link(obj)
    # Make the normals point out whatever the winding.
    bm = bmesh.new()
    bm.from_mesh(mesh)
    bmesh.ops.recalc_face_normals(bm, faces=bm.faces)
    bm.to_mesh(mesh)
    bm.free()
    return obj


def triangles(obj):
    return sum(len(p.vertices) - 2 for p in obj.data.polygons)


def _apply(obj, modifier):
    bpy.ops.object.select_all(action="DESELECT")
    obj.select_set(True)
    bpy.context.view_layer.objects.active = obj
    bpy.ops.object.modifier_apply(modifier=modifier.name)


def keep_largest_piece(obj):
    """Deletes every loose piece but the largest (crumbs the remesh leaves
    where two thin parts nearly touch)."""
    bm = bmesh.new()
    bm.from_mesh(obj.data)
    bm.verts.ensure_lookup_table()
    seen, pieces = set(), []
    for start in bm.verts:
        if start.index in seen:
            continue
        piece, stack = [], [start]
        seen.add(start.index)
        while stack:
            v = stack.pop()
            piece.append(v)
            for e in v.link_edges:
                w = e.other_vert(v)
                if w.index not in seen:
                    seen.add(w.index)
                    stack.append(w)
        pieces.append(piece)
    pieces.sort(key=len, reverse=True)
    crumbs = [v for piece in pieces[1:] for v in piece]
    if crumbs:
        bmesh.ops.delete(bm, geom=crumbs, context="VERTS")
        bm.to_mesh(obj.data)
        print(f"animal_rig: dropped {len(pieces) - 1} loose pieces ({len(crumbs)} vertices)")
    bm.free()




def build_armature(name, bones):
    """An armature from ``bones``: ``(name, head, tail, parent or None)``, in
    order, parents first; every bone deforms, roll 0, unconnected."""
    data = bpy.data.armatures.new(name)
    arm = bpy.data.objects.new(name, data)
    bpy.context.collection.objects.link(arm)
    bpy.context.view_layer.objects.active = arm
    bpy.ops.object.mode_set(mode="EDIT")
    for bone, head, tail, parent in bones:
        eb = data.edit_bones.new(bone)
        eb.head = head
        eb.tail = tail
        eb.roll = 0.0
        if parent:
            eb.parent = data.edit_bones[parent]
    bpy.ops.object.mode_set(mode="OBJECT")
    return arm


def bind(mesh, arm):
    """Parents ``mesh`` to ``arm`` with automatic (heat) weights, at most four
    bones a vertex."""
    bpy.ops.object.select_all(action="DESELECT")
    mesh.select_set(True)
    arm.select_set(True)
    bpy.context.view_layer.objects.active = arm
    bpy.ops.object.parent_set(type="ARMATURE_AUTO")
    missing = [v.index for v in mesh.data.vertices if not v.groups]
    if missing:
        raise SystemExit(f"animal_rig: {len(missing)} vertices got no bone weight")
    bpy.context.view_layer.objects.active = mesh
    bpy.ops.object.vertex_group_limit_total(group_select_mode="ALL", limit=4)
    bpy.ops.object.vertex_group_normalize_all(lock_active=False)






class Pose:
    """One frame's pose, built in armature space and turned into each bone's
    local transform. ``place(bone, head, direction)`` aims a bone (its rest
    orientation turned the shortest way onto ``direction``);
    ``carry(bone, matrix)`` moves it rigidly with a matrix (its parent's
    motion, say); bones never placed follow their parent unchanged."""

    def __init__(self, arm):
        self.arm = arm
        self.world = {}

    def rest(self, bone):
        return self.arm.data.bones[bone].matrix_local

    def rest_head(self, bone):
        return self.arm.data.bones[bone].head_local.copy()

    def rest_tail(self, bone):
        return self.arm.data.bones[bone].tail_local.copy()

    def length(self, bone):
        return self.arm.data.bones[bone].length

    def carry(self, bone, matrix):
        self.world[bone] = matrix @ self.rest(bone)
        return self.world[bone]

    def place(self, bone, head, direction, twist=0.0):
        rest = self.rest(bone)
        y = (rest.to_3x3() @ Vector((0, 1, 0))).normalized()
        turn = y.rotation_difference(Vector(direction).normalized())
        rot = turn.to_matrix() @ rest.to_3x3()
        if twist:
            rot = Matrix.Rotation(twist, 3, Vector(direction).normalized()) @ rot
        m = rot.to_4x4()
        m.translation = Vector(head)
        self.world[bone] = m
        return m

    def tail(self, bone):
        m = self.world[bone]
        return m.translation + m.to_3x3() @ Vector((0, self.length(bone), 0))

    def posed(self, bone):
        """The bone's armature-space matrix in this pose: placed, or carried
        by its parent's pose, or at rest."""
        if bone in self.world:
            return self.world[bone]
        b = self.arm.data.bones[bone]
        if b.parent is None:
            return b.matrix_local.copy()
        return self.posed(b.parent.name) @ b.parent.matrix_local.inverted() @ b.matrix_local

    def turn(self, bone, rotation, pivot=None):
        """Forward kinematics: turns ``bone`` (and so everything below it) by
        ``rotation`` (3x3 or quaternion, armature space) about its posed
        head, or about ``pivot``."""
        m = self.posed(bone)
        at = m.translation.copy() if pivot is None else Vector(pivot)
        rot = rotation.to_matrix() if hasattr(rotation, "to_matrix") else rotation
        self.world[bone] = Matrix.Translation(at) @ rot.to_4x4() @ Matrix.Translation(-at) @ m
        return self.world[bone]

    def move(self, bone, offset):
        """Shifts ``bone`` (and everything below it) by ``offset`` (armature
        space); key its location with ``apply``'s ``root``."""
        self.world[bone] = Matrix.Translation(Vector(offset)) @ self.posed(bone)
        return self.world[bone]

    def aim(self, bone, point, target):
        """Turns ``bone`` the shortest way about its posed head so that
        ``point`` (armature space, carried rigidly by the bone in its
        current pose) heads toward ``target``."""
        head = self.posed(bone).translation
        now = Vector(point) - head
        want = Vector(target) - head
        if now.length < 1e-9 or want.length < 1e-9:
            return self.posed(bone)
        return self.turn(bone, now.rotation_difference(want))

    def apply(self, frame, root):
        """Keys every bone's local rotation (and the root's location) at
        ``frame``. ``root`` is one bone name or a collection of them: each
        gets its location keyed too."""
        roots = {root} if isinstance(root, str) else set(root)
        bones = self.arm.data.bones
        posed = {}
        for bone in bones:  # parents come first
            parent = bone.parent
            if bone.name in self.world:
                world = self.world[bone.name]
            elif parent:
                world = posed[parent.name] @ parent.matrix_local.inverted() @ bone.matrix_local
            else:
                world = bone.matrix_local.copy()
            posed[bone.name] = world
            if parent:
                rest_rel = parent.matrix_local.inverted() @ bone.matrix_local
                pose_rel = posed[parent.name].inverted() @ world
            else:
                rest_rel, pose_rel = bone.matrix_local, world
            basis = rest_rel.inverted() @ pose_rel
            pb = self.arm.pose.bones[bone.name]
            pb.rotation_mode = "QUATERNION"
            loc, rot, _ = basis.decompose()
            pb.rotation_quaternion = rot
            pb.keyframe_insert("rotation_quaternion", frame=frame)
            if bone.name in roots:
                pb.location = loc
                pb.keyframe_insert("location", frame=frame)


def bake_clip(arm, name, seconds, pose_at, root):
    """Bakes an action ``name`` of ``seconds`` that loops: ``pose_at(arm, t)``
    returns the ``Pose`` at phase ``t`` in [0, 1]; the last frame equals the
    first."""
    action = bpy.data.actions.new(name)
    action.use_fake_user = True
    arm.animation_data_create()
    arm.animation_data.action = action
    frames = max(2, round(seconds * FPS))
    for f in range(frames + 1):
        pose = pose_at(arm, (f % frames) / frames)
        pose.apply(f, root)
    arm.animation_data.action = None
    return action




def noise(p, scale=(1.0, 1.0, 1.0), seed=0.0):
    """Perlin noise in about [-1, 1] at ``p`` stretched by ``scale``."""
    q = Vector((p[0] * scale[0] + seed, p[1] * scale[1] + 2 * seed, p[2] * scale[2] - seed))
    return mathutils.noise.noise(q)


def look_at(cam, target):
    d = Vector(target) - cam.location
    cam.rotation_euler = d.to_track_quat("-Z", "Y").to_euler()


def render_previews(out_dir, arm, shots, size=(1000, 800)):
    """Renders ``shots``: ``(file, action or None, frame, camera position,
    target, lens mm)``, with Eevee, on a neutral grey ground and sky."""
    os.makedirs(out_dir, exist_ok=True)
    scene = bpy.context.scene
    scene.render.engine = "BLENDER_EEVEE"
    scene.render.resolution_x, scene.render.resolution_y = size
    scene.render.film_transparent = False
    scene.render.image_settings.file_format = "PNG"
    scene.render.image_settings.color_mode = "RGB"
    scene.view_settings.view_transform = "AgX"
    world = bpy.data.worlds.new("Sky")
    world.use_nodes = True
    world.node_tree.nodes["Background"].inputs["Color"].default_value = (0.55, 0.58, 0.62, 1)
    world.node_tree.nodes["Background"].inputs["Strength"].default_value = 0.6
    scene.world = world
    bpy.ops.mesh.primitive_plane_add(size=20)
    ground = bpy.context.active_object
    gm = bpy.data.materials.new("Ground")
    gm.use_nodes = True
    gm.node_tree.nodes["Principled BSDF"].inputs["Base Color"].default_value = (0.36, 0.34, 0.3, 1)
    ground.data.materials.append(gm)
    sun_data = bpy.data.lights.new("Sun", "SUN")
    sun_data.energy = 3.5
    sun_data.angle = math.radians(8)
    sun = bpy.data.objects.new("Sun", sun_data)
    sun.rotation_euler = (math.radians(50), math.radians(10), math.radians(-35))
    scene.collection.objects.link(sun)
    cam = bpy.data.objects.new("Cam", bpy.data.cameras.new("Cam"))
    scene.collection.objects.link(cam)
    scene.camera = cam
    for file, action, frame, eye, target, lens in shots:
        arm.animation_data_create()
        arm.animation_data.action = action
        if action is None:
            for pb in arm.pose.bones:
                pb.rotation_quaternion = Quaternion()
                pb.location = (0, 0, 0)
        scene.frame_set(frame)
        cam.location = eye
        cam.data.lens = lens
        look_at(cam, target)
        scene.render.filepath = os.path.join(out_dir, file)
        bpy.ops.render.render(write_still=True)
    arm.animation_data.action = None
    for o in (ground, sun, cam):
        bpy.data.objects.remove(o)


def export(path, actions):
    """Writes the GLB: Y up, skinned, ``actions`` as clips (sorted by name:
    reorder them with ``tools/glbtrim.py --keep``), UVs, tangents and the
    active vertex colours if any, no images."""
    # Only our clips, nothing left over from the preview renders.
    for a in list(bpy.data.actions):
        if a not in actions:
            bpy.data.actions.remove(a)
    bpy.ops.export_scene.gltf(
        filepath=path,
        export_format="GLB",
        export_yup=True,
        export_skins=True,
        export_animations=True,
        export_animation_mode="ACTIONS",
        export_force_sampling=True,
        export_vertex_color="ACTIVE",
        export_texcoords=True,
        export_tangents=True,
        export_image_format="NONE",
        export_cameras=False,
        export_lights=False,
        export_def_bones=True,
    )
    print(f"animal_rig: wrote {path}: clips {[a.name for a in actions]}")


# ---------------------------------------------------------------------------
# Detail: a dense sculpt baked onto a lighter, unwrapped mesh, and feather
# cards with an alpha-tested atlas.


def fuse_dense(name, parts, voxel):
    """Like ``fuse`` without the decimation: the dense, watertight sculpt
    that the textures are baked from."""
    bpy.ops.object.select_all(action="DESELECT")
    for p in parts:
        p.select_set(True)
    bpy.context.view_layer.objects.active = parts[0]
    bpy.ops.object.join()
    obj = bpy.context.view_layer.objects.active
    obj.name = name
    obj.data.name = name
    remesh = obj.modifiers.new("Remesh", "REMESH")
    remesh.mode = "VOXEL"
    remesh.voxel_size = voxel
    remesh.adaptivity = 0.0
    _apply(obj, remesh)
    keep_largest_piece(obj)
    for poly in obj.data.polygons:
        poly.use_smooth = True
    return obj


def decimated_copy(src, name, budget, keep=None, keep_factor=1.0, invert_keep=False):
    """A copy of ``src`` decimated (collapse) to about ``budget`` triangles,
    shaded smooth. ``keep``: a vertex group of ``src`` whose weight
    (times ``keep_factor``) holds detail there, so the triangles go
    where it is high."""
    obj = src.copy()
    obj.data = src.data.copy()
    obj.name = name
    obj.data.name = name
    bpy.context.collection.objects.link(obj)
    dense = triangles(obj)
    decimate = obj.modifiers.new("Decimate", "DECIMATE")
    decimate.decimate_type = "COLLAPSE"
    decimate.ratio = min(1.0, budget / max(dense, 1))
    decimate.use_collapse_triangulate = True
    if keep:
        decimate.vertex_group = keep
        decimate.vertex_group_factor = keep_factor
        decimate.invert_vertex_group = invert_keep
    _apply(obj, decimate)
    for poly in obj.data.polygons:
        poly.use_smooth = True
    print(f"animal_rig: {name}: {dense} -> {triangles(obj)} triangles")
    return obj


def unwrap(obj, margin=0.004):
    """UVs by smart projection, islands packed with ``margin``."""
    bpy.ops.object.select_all(action="DESELECT")
    obj.select_set(True)
    bpy.context.view_layer.objects.active = obj
    bpy.ops.object.mode_set(mode="EDIT")
    bpy.ops.mesh.select_all(action="SELECT")
    bpy.ops.uv.smart_project(angle_limit=math.radians(60), island_margin=margin, area_weight=0.6)
    bpy.ops.object.mode_set(mode="OBJECT")


def new_image(name, size, data=False):
    img = bpy.data.images.new(name, size, size, alpha=False, float_buffer=False)
    img.colorspace_settings.name = "Non-Color" if data else "sRGB"
    return img


def bake_to(target, image, kind, source=None, extrusion=0.02, samples=16, pass_filter=None):
    """Bakes ``kind`` (Cycles: NORMAL, DIFFUSE, AO...) into ``image`` over
    ``target``'s UVs; from ``source`` (selected to active) when given."""
    scene = bpy.context.scene
    scene.render.engine = "CYCLES"
    scene.cycles.device = "CPU"
    scene.cycles.samples = samples
    for mat in target.data.materials:
        nodes = mat.node_tree.nodes
        node = nodes.get("BakeTarget") or nodes.new("ShaderNodeTexImage")
        node.name = "BakeTarget"
        node.image = image
        nodes.active = node
    bpy.ops.object.select_all(action="DESELECT")
    if source:
        source.select_set(True)
    target.select_set(True)
    bpy.context.view_layer.objects.active = target
    kwargs = dict(type=kind, use_selected_to_active=bool(source), cage_extrusion=extrusion, margin=8)
    if source:
        kwargs["max_ray_distance"] = extrusion * 2.5
    if pass_filter:
        kwargs["pass_filter"] = pass_filter
    bpy.ops.object.bake(**kwargs)
    for mat in target.data.materials:
        mat.node_tree.nodes.remove(mat.node_tree.nodes["BakeTarget"])


def save_jpeg(image, path, quality=92):
    """Saves ``image`` as a JPEG (the editable source of a KTX2 set)."""
    os.makedirs(os.path.dirname(path), exist_ok=True)
    scene = bpy.context.scene
    settings = scene.render.image_settings
    settings.file_format = "JPEG"
    settings.color_mode = "RGB"
    settings.quality = quality
    image.save_render(path, scene=scene)


def image_from_array(name, rgb, data=False):
    """A Blender image from a numpy array (rows bottom up, values 0..1) of
    shape (h, w, 3)."""
    import numpy as np

    h, w, _ = rgb.shape
    img = bpy.data.images.new(name, w, h, alpha=False, float_buffer=False)
    img.colorspace_settings.name = "Non-Color" if data else "sRGB"
    rgba = np.ones((h, w, 4), dtype=np.float32)
    rgba[:, :, :3] = np.clip(rgb, 0.0, 1.0)
    img.pixels.foreach_set(rgba.ravel())
    img.update()
    return img


# How the atlas feathers are drawn. ``soft`` is the default; ``v8`` keeps the
# first textured rhea (thin, sparse barbs, a wide pale rachis and, through a
# flipped row order, the feather drawn root down).
FEATHER_STYLES = {
    "soft": dict(barb_cut=0.3, wisps=True, rachis=0.02, gaps=(0.10, 0.45), root_up=True),
    "v8": dict(barb_cut=0.45, wisps=False, rachis=0.035, gaps=(0.18, 0.35), root_up=False),
    # Each feather readable on its own: v8's open barbs and visible rachis
    # with the soft wisps, and a darker edge around the vane.
    "plume": dict(barb_cut=0.40, wisps=True, rachis=0.03, gaps=(0.14, 0.40), root_up=True, outline=0.35),
}


def feather_atlas_columns(palettes, size, seed=1, style="soft"):
    """``feather_atlas`` with one column per ``(base, tip, shaft)`` palette."""
    return feather_atlas(len(palettes), size, palettes, seed, style)


def feather_atlas(columns, size, palettes, seed=1, style="soft"):
    """Paints an atlas of ``columns`` loose, ragged feathers hanging from the
    top of the image (soft barbs that never hook together, like a rhea's):
    returns colour, opacity, normal (OpenGL) and roughness arrays, rows
    bottom up. ``base`` and ``tip`` are the sRGB colours at the root and the
    end of the vane, ``shaft`` the rachis."""
    import numpy as np

    st = FEATHER_STYLES[style]
    rng = np.random.default_rng(seed)
    h = size
    w = size // columns
    colour = np.zeros((h, size, 3), np.float32)
    alpha = np.zeros((h, size), np.float32)
    height = np.zeros((h, size), np.float32)
    ys, xs = np.mgrid[0:h, 0:w].astype(np.float32)
    # 0 at the root (top row), 1 at the tip (bottom).
    v = ys / (h - 1) if st["root_up"] else 1.0 - ys / (h - 1)
    u = xs / (w - 1) * 2.0 - 1.0  # -1..1 across
    for c in range(columns):
        base, tip, shaft = palettes[c]
        bend = rng.uniform(-0.15, 0.15)
        length = rng.uniform(0.82, 0.97)
        spine = bend * v * v  # the rachis drifts sideways toward the tip
        across = u - spine
        # The vane: widest past the middle, rounded at the tip.
        half = 0.92 * np.sqrt(np.clip(v / length, 0, 1)) * np.clip((length - v) / 0.25, 0, 1) ** 0.6
        half = np.maximum(half, 0.0)
        inside = np.abs(across) < half
        # Barbs: thin strands leaving the rachis down and out at an angle,
        # wavy, some missing (ragged), the outer ends thinning out.
        slope = 0.55 + 0.25 * rng.uniform()
        count = rng.uniform(55, 75)
        phase = (v * 1.0 + np.abs(across) * slope) * count
        wobble = 0.35 * np.sin(np.abs(across) * 9.0 + rng.uniform(0, 6))
        if st["wisps"]:
            # Uneven, clumping barbs rather than a regular herringbone.
            wobble = wobble + 0.6 * np.sin(v * 23.0 + rng.uniform(0, 6)) * np.abs(across) + 0.25 * np.sin(xs * 0.9 + v * 40.0)
        strand = 0.5 + 0.5 * np.cos(2 * np.pi * (phase + wobble))
        # Soft, loose barbs: wide strands, some fused into wisps.
        if st["wisps"]:
            strand = np.maximum(strand, 0.5 + 0.5 * np.cos(2 * np.pi * (phase * 0.5 + 0.25 + wobble)))
        gaps = rng.uniform(size=int(count * 2) + 4)
        index = np.floor(phase).astype(int) % len(gaps)
        g0, g1 = st["gaps"]
        missing = gaps[index] < g0 + g1 * (np.abs(across) / np.maximum(half, 1e-3)) ** 3
        barb = (strand > st["barb_cut"]) & inside & ~missing
        rachis = (np.abs(across) < st["rachis"] * (1 - 0.7 * v)) & (v < length)
        a = np.where(barb | rachis, 1.0, 0.0)
        # Soft downy base near the root.
        down = (v < 0.12) & (np.abs(across) < half + 0.2) & (strand > 0.2)
        a = np.maximum(a, np.where(down, 1.0, 0.0))
        t = np.clip(v / length, 0, 1)[..., None]
        col = np.asarray(base, np.float32) * (1 - t) + np.asarray(tip, np.float32) * t
        col = col * (0.85 + 0.15 * strand[..., None])
        edge = np.clip(np.abs(across) / np.maximum(half, 1e-3), 0, 1)
        col = col * (1 - st.get("outline", 0.0) * edge[..., None] ** 3)
        col = np.where(rachis[..., None], np.asarray(shaft, np.float32), col)
        hgt = np.where(rachis, 1.0, strand * 0.6) * a
        sl = slice(c * w, (c + 1) * w)
        colour[:, sl] = col
        alpha[:, sl] = a
        height[:, sl] = hgt
    # Rows bottom up for Blender: the root at the top row.
    colour, alpha, height = colour[::-1], alpha[::-1], height[::-1]
    gy, gx = np.gradient(height)
    n = np.stack([-gx * 1.5, -gy * 1.5, np.ones_like(gx)], axis=-1)
    n /= np.linalg.norm(n, axis=-1, keepdims=True)
    normal = n * 0.5 + 0.5
    rough = np.full((h, size, 3), 0.88, np.float32)
    # Bleed the colour under the transparent texels so mips do not darken.
    filled = colour.copy()
    mean = colour[alpha > 0.5].mean(axis=0) if (alpha > 0.5).any() else np.zeros(3)
    filled[alpha < 0.5] = mean
    return filled, np.repeat(alpha[..., None], 3, axis=-1), normal, rough


def cards(name, spots, columns, lift=(0.06, 0.10)):
    """Feather cards: a two-segment strip per spot, ``(root, hang, side,
    out, width, length, column)`` (vectors in metres; ``hang`` the unit
    direction it hangs, ``side`` across it, ``out`` the surface normal),
    lifting off the surface toward the tip (``lift``: the middle and the tip,
    as fractions of the length); UVs on atlas column ``column``, root at the
    top. Normals point ``out`` on both faces."""
    verts, faces, uvs, normals = [], [], [], []
    for root, hang, side, out, width, length, column in spots:
        root, hang, side, out = Vector(root), Vector(hang), Vector(side), Vector(out)
        mid = root + hang * (length * 0.5) + out * (length * lift[0])
        tip = root + hang * length + out * (length * lift[1])
        u0, u1 = column / columns, (column + 1) / columns
        base = len(verts)
        for p, v, w in ((root, 1.0, 0.8), (mid, 0.5, 1.0), (tip, 0.0, 0.9)):
            verts += [p - side * (width * w / 2), p + side * (width * w / 2)]
            uvs += [(u0, v), (u1, v)]
            normals += [out, out]
        faces += [(base, base + 1, base + 3, base + 2), (base + 2, base + 3, base + 5, base + 4)]
    mesh = bpy.data.meshes.new(name)
    mesh.from_pydata([tuple(v) for v in verts], [], faces)
    mesh.update()
    uv = mesh.uv_layers.new(name="UVMap")
    for poly in mesh.polygons:
        poly.use_smooth = True
        for li in poly.loop_indices:
            uv.data[li].uv = uvs[mesh.loops[li].vertex_index]
    mesh.normals_split_custom_set_from_vertices([tuple(n) for n in normals])
    obj = bpy.data.objects.new(name, mesh)
    bpy.context.collection.objects.link(obj)
    return obj


def transfer_weights(src, dst, arm):
    """Skins ``dst`` like the nearest surface of ``src`` and parents it to
    ``arm``."""
    for g in src.vertex_groups:
        if g.name not in dst.vertex_groups:
            dst.vertex_groups.new(name=g.name)
    mod = dst.modifiers.new("Weights", "DATA_TRANSFER")
    mod.object = src
    mod.use_vert_data = True
    mod.data_types_verts = {"VGROUP_WEIGHTS"}
    mod.vert_mapping = "POLYINTERP_NEAREST"
    mod.layers_vgroup_select_src = "ALL"
    mod.layers_vgroup_select_dst = "NAME"
    _apply(dst, mod)
    dst.parent = arm
    skin = dst.modifiers.new("Armature", "ARMATURE")
    skin.object = arm


def join(into, other):
    bpy.ops.object.select_all(action="DESELECT")
    other.select_set(True)
    into.select_set(True)
    bpy.context.view_layer.objects.active = into
    bpy.ops.object.join()
    bpy.ops.object.vertex_group_limit_total(group_select_mode="ALL", limit=4)
    bpy.ops.object.vertex_group_normalize_all(lock_active=False)
    return into


def textured_material(name, colour, normal=None, alpha=None, rough=0.9):
    """A preview material: ``colour`` image, an optional OpenGL normal map
    and an optional alpha-tested opacity image."""
    mat = bpy.data.materials.new(name)
    mat.use_nodes = True
    nt = mat.node_tree
    bsdf = nt.nodes["Principled BSDF"]
    bsdf.inputs["Roughness"].default_value = rough
    tex = nt.nodes.new("ShaderNodeTexImage")
    tex.image = colour
    nt.links.new(tex.outputs["Color"], bsdf.inputs["Base Color"])
    if normal is not None:
        ntex = nt.nodes.new("ShaderNodeTexImage")
        ntex.image = normal
        nmap = nt.nodes.new("ShaderNodeNormalMap")
        nt.links.new(ntex.outputs["Color"], nmap.inputs["Color"])
        nt.links.new(nmap.outputs["Normal"], bsdf.inputs["Normal"])
    if alpha is not None:
        atex = nt.nodes.new("ShaderNodeTexImage")
        atex.image = alpha
        clip = nt.nodes.new("ShaderNodeMath")
        clip.operation = "GREATER_THAN"
        clip.inputs[1].default_value = 0.5
        nt.links.new(atex.outputs["Color"], clip.inputs[0])
        nt.links.new(clip.outputs[0], bsdf.inputs["Alpha"])
        mat.use_backface_culling = False
    return mat


def plain_material(name, rgb):
    """The exported material: a plain colour (the game puts the KTX2 maps
    on it; the colour is its fallback)."""
    mat = bpy.data.materials.new(name)
    mat.use_nodes = True
    bsdf = mat.node_tree.nodes["Principled BSDF"]
    bsdf.inputs["Base Color"].default_value = (*rgb, 1.0)
    bsdf.inputs["Roughness"].default_value = 0.9
    return mat


# ---------------------------------------------------------------------------
# A rig copied from another model: bones with a given orientation, and the
# leaf ``_end`` nodes some exporters add.


def build_oriented_armature(name, bones):
    """An armature from ``bones``: ``(name, parent or None, head, rotation,
    length)`` in order, parents first; ``rotation`` the bone's rest
    orientation in armature space (a quaternion ``(w, x, y, z)``: its +Y
    runs along the bone), so the exported joints keep the local axes of the
    rig they copy. Every bone deforms, unconnected."""
    data = bpy.data.armatures.new(name)
    arm = bpy.data.objects.new(name, data)
    bpy.context.collection.objects.link(arm)
    bpy.context.view_layer.objects.active = arm
    bpy.ops.object.mode_set(mode="EDIT")
    for bone, parent, head, rotation, length in bones:
        eb = data.edit_bones.new(bone)
        eb.head = (0, 0, 0)
        eb.tail = (0, length, 0)
        m = Quaternion(rotation).to_matrix().to_4x4()
        m.translation = Vector(head)
        eb.matrix = m
        eb.length = length
        if parent:
            eb.parent = data.edit_bones[parent]
    bpy.ops.object.mode_set(mode="OBJECT")
    return arm


def bone_ends(arm, ends):
    """Empties ``(name, bone, position)`` parented to ``bone`` at
    ``position`` (armature space): the exporter writes them as plain child
    nodes of the joint, like the ``_end`` leaves of FBX conversions."""
    out = []
    for name, bone, position in ends:
        e = bpy.data.objects.new(name, None)
        e.empty_display_size = 0.02
        bpy.context.collection.objects.link(e)
        e.parent = arm
        e.parent_type = "BONE"
        e.parent_bone = bone
        bpy.context.view_layer.update()
        e.matrix_world = arm.matrix_world @ Matrix.Translation(Vector(position))
        out.append(e)
    return out


# ---------------------------------------------------------------------------
# Sculpting with signed distances: smooth unions and carvings of simple
# shapes on a voxel grid, turned into the dense surface (OpenVDB).


def _smin(a, b, k):
    import numpy as np

    if k <= 0:
        return np.minimum(a, b)
    h = np.maximum(k - np.abs(a - b), 0.0) / k
    return np.minimum(a, b) - h * h * k * 0.25


class Sdf:
    """A signed distance field (negative inside) on a regular grid from
    ``lo`` to ``hi`` (metres) with ``voxel`` spacing. ``add`` blends a shape
    in with a smooth union of radius ``k``, ``carve`` cuts one out; each
    shape is ``(fn, lo, hi)``: ``fn(x, y, z)`` its distance over numpy
    arrays, and its bounds. Only the voxels near a shape are evaluated."""

    def __init__(self, lo, hi, voxel):
        import numpy as np

        self.voxel = voxel
        self.lo = np.asarray(lo, np.float64)
        self.n = np.ceil((np.asarray(hi) - self.lo) / voxel).astype(int) + 1
        self.d = np.full(tuple(self.n), 1.0, np.float32)

    def _block(self, lo, hi, margin):
        import numpy as np

        a = np.clip(np.floor((np.asarray(lo) - margin - self.lo) / self.voxel).astype(int), 0, self.n)
        b = np.clip(np.ceil((np.asarray(hi) + margin - self.lo) / self.voxel).astype(int) + 1, 0, self.n)
        if np.any(b <= a):
            return None
        axes = [self.lo[i] + self.voxel * np.arange(a[i], b[i]) for i in range(3)]
        x = axes[0][:, None, None]
        y = axes[1][None, :, None]
        z = axes[2][None, None, :]
        return (slice(a[0], b[0]), slice(a[1], b[1]), slice(a[2], b[2])), x, y, z

    def add(self, shape, k=0.0):
        fn, lo, hi = shape
        blk = self._block(lo, hi, k + 2 * self.voxel)
        if blk is None:
            return
        s, x, y, z = blk
        self.d[s] = _smin(self.d[s], fn(x, y, z).astype("float32"), k)

    def carve(self, shape, k=0.0):
        fn, lo, hi = shape
        blk = self._block(lo, hi, k + 2 * self.voxel)
        if blk is None:
            return
        s, x, y, z = blk
        self.d[s] = -_smin(-self.d[s], fn(x, y, z).astype("float32"), k)

    def sample(self, p):
        """The distance at ``p`` (trilinear), and its gradient."""
        import numpy as np

        f = (np.asarray(p, np.float64) - self.lo) / self.voxel
        i = np.clip(np.floor(f).astype(int), 0, self.n - 2)
        t = f - i

        def at(dx, dy, dz):
            return float(self.d[i[0] + dx, i[1] + dy, i[2] + dz])

        c = [[[at(a, b, c) for c in (0, 1)] for b in (0, 1)] for a in (0, 1)]
        def lerp(u, v, w):
            return u + (v - u) * w
        d = lerp(
            lerp(lerp(c[0][0][0], c[0][0][1], t[2]), lerp(c[0][1][0], c[0][1][1], t[2]), t[1]),
            lerp(lerp(c[1][0][0], c[1][0][1], t[2]), lerp(c[1][1][0], c[1][1][1], t[2]), t[1]),
            t[0],
        )
        g = Vector((
            lerp(lerp(c[1][0][0], c[1][1][0], t[1]), lerp(c[1][0][1], c[1][1][1], t[1]), t[2])
            - lerp(lerp(c[0][0][0], c[0][1][0], t[1]), lerp(c[0][0][1], c[0][1][1], t[1]), t[2]),
            lerp(lerp(c[0][1][0], c[1][1][0], t[0]), lerp(c[0][1][1], c[1][1][1], t[0]), t[2])
            - lerp(lerp(c[0][0][0], c[1][0][0], t[0]), lerp(c[0][0][1], c[1][0][1], t[0]), t[2]),
            lerp(lerp(c[0][0][1], c[1][0][1], t[0]), lerp(c[0][1][1], c[1][1][1], t[0]), t[1])
            - lerp(lerp(c[0][0][0], c[1][0][0], t[0]), lerp(c[0][1][0], c[1][1][0], t[0]), t[1]),
        ))
        return d, (g.normalized() if g.length > 1e-12 else Vector((0, 0, 1)))

    def surface(self, p, direction, reach=0.5):
        """Where the ray from ``p`` (inside) along ``direction`` leaves the
        surface, by bisection."""
        p, d = Vector(p), Vector(direction).normalized()
        lo, hi = 0.0, reach
        for _ in range(30):
            mid = 0.5 * (lo + hi)
            if self.sample(p + d * mid)[0] < 0:
                lo = mid
            else:
                hi = mid
        return p + d * hi

    def groove(self, points, direction, radius, depth, k):
        """Presses a groove ``depth`` deep and ``radius`` wide along the
        surface where rays from the (inside) ``points`` along ``direction``
        leave it."""
        d = Vector(direction).normalized()
        path = [self.surface(p, d) + d * (radius - depth) for p in points]
        self.carve(sdf_tube(path, [(radius, radius)] * len(path), side=d), k)

    def ridge(self, points, direction, radius, height, k):
        """Raises a ridge (a vein, a bony crest) ``height`` high and
        ``radius`` wide along the surface where rays from the (inside)
        ``points`` along ``direction`` leave it."""
        d = Vector(direction).normalized()
        path = [self.surface(p, d) - d * (radius - height) for p in points]
        self.add(sdf_tube(path, [(radius, radius)] * len(path), side=d), k)

    def outside(self, p, gap, steps=6):
        """``p`` pushed out along the gradient until it is ``gap`` off the
        surface (unchanged if already further)."""
        p = Vector(p)
        for _ in range(steps):
            d, g = self.sample(p)
            if d >= gap:
                break
            p = p + g * (gap - d)
        return p

    def to_mesh(self, name):
        """The zero surface as a smooth-shaded mesh object (OpenVDB)."""
        import numpy as np
        import openvdb

        # Inside positive; the far outside (and beyond the grid) negative.
        grid = openvdb.FloatGrid(-1.0)
        grid.copyFromArray(-self.d)
        points, tris, quads = grid.convertToPolygons(isovalue=0.0, adaptivity=0.0)
        points = points * self.voxel + self.lo
        faces = [tuple(int(i) for i in f) for f in tris] + [tuple(int(i) for i in f) for f in quads]
        mesh = bpy.data.meshes.new(name)
        mesh.from_pydata(points.tolist(), [], faces)
        mesh.update()
        obj = bpy.data.objects.new(name, mesh)
        bpy.context.collection.objects.link(obj)
        bm = bmesh.new()
        bm.from_mesh(mesh)
        bmesh.ops.remove_doubles(bm, verts=bm.verts, dist=self.voxel * 0.01)
        bmesh.ops.recalc_face_normals(bm, faces=bm.faces)
        bm.to_mesh(mesh)
        bm.free()
        keep_largest_piece(obj)
        for poly in mesh.polygons:
            poly.use_smooth = True
        print(f"animal_rig: {name}: {len(mesh.polygons)} faces from a {tuple(self.n)} grid")
        return obj


def sdf_ellipsoid(centre, radii, rotation=None):
    """An ellipsoid (approximate distance); ``rotation`` a 3x3 matrix whose
    columns are its axes, in the order of ``radii``."""
    import numpy as np

    c = np.asarray(centre, np.float64)
    r = np.asarray(radii, np.float64)
    m = np.eye(3) if rotation is None else np.asarray(rotation, np.float64)

    def fn(x, y, z):
        dx, dy, dz = x - c[0], y - c[1], z - c[2]
        u = (dx * m[0, 0] + dy * m[1, 0] + dz * m[2, 0]) / r[0]
        v = (dx * m[0, 1] + dy * m[1, 1] + dz * m[2, 1]) / r[1]
        w = (dx * m[0, 2] + dy * m[1, 2] + dz * m[2, 2]) / r[2]
        k0 = np.sqrt(u * u + v * v + w * w)
        k1 = np.sqrt((u / r[0]) ** 2 + (v / r[1]) ** 2 + (w / r[2]) ** 2)
        return k0 * (k0 - 1.0) / np.maximum(k1, 1e-9)

    big = float(r.max())
    return fn, c - big, c + big


def sdf_round_cone(a, b, ra, rb):
    """A capsule from ``a`` (radius ``ra``) to ``b`` (radius ``rb``)."""
    import numpy as np

    a, b = np.asarray(a, np.float64), np.asarray(b, np.float64)
    ab = b - a
    ll = float(ab.dot(ab))

    def fn(x, y, z):
        px, py, pz = x - a[0], y - a[1], z - a[2]
        t = np.clip((px * ab[0] + py * ab[1] + pz * ab[2]) / ll, 0.0, 1.0)
        qx, qy, qz = px - ab[0] * t, py - ab[1] * t, pz - ab[2] * t
        return np.sqrt(qx * qx + qy * qy + qz * qz) - (ra + (rb - ra) * t)

    big = max(ra, rb)
    return fn, np.minimum(a, b) - big, np.maximum(a, b) + big


def sdf_tube(points, radii, side=(1.0, 0.0, 0.0), samples=6):
    """A tube along a smooth path through ``points`` with an elliptical
    section ``radii[i] = (a, b)`` at each control point (``a`` along
    ``side``, ``b`` across it), rounded at the ends. It is a chain of
    round cones (exact distances, so it blends without ripples) in a space
    squeezed along ``side`` by the tube's mean ``a / b``: the sections are
    ellipses of that one ratio, ``b`` across."""
    import numpy as np

    sv = np.asarray(Vector(side).normalized())
    ratio = sum(r[0] / r[1] for r in radii) / len(radii)
    path = _catmull([Vector(p) for p in points], samples)

    def radius_at(u):
        i = min(int(u), len(radii) - 2)
        f = u - i
        return radii[i][1] * (1 - f) + radii[i + 1][1] * f

    def squeeze(p):
        p = np.asarray(p, np.float64)
        return p + sv * (p.dot(sv) / ratio - p.dot(sv))

    nodes = [(squeeze(tuple(p)), radius_at(u)) for p, u in path]
    segs = []
    for (a, ra), (b, rb) in zip(nodes, nodes[1:]):
        ab = b - a
        ll = float(ab.dot(ab))
        if ll > 1e-12:
            segs.append((a, ab, ll, ra, rb))

    def fn(x, y, z):
        # Squeeze the query points the same way.
        along = x * sv[0] + y * sv[1] + z * sv[2]
        extra = along / ratio - along
        qx, qy, qz = x + sv[0] * extra, y + sv[1] * extra, z + sv[2] * extra
        best = None
        for a, ab, ll, ra, rb in segs:
            px, py, pz = qx - a[0], qy - a[1], qz - a[2]
            t = np.clip((px * ab[0] + py * ab[1] + pz * ab[2]) / ll, 0.0, 1.0)
            dx, dy, dz = px - ab[0] * t, py - ab[1] * t, pz - ab[2] * t
            dist = np.sqrt(dx * dx + dy * dy + dz * dz) - (ra + (rb - ra) * t)
            best = dist if best is None else np.minimum(best, dist)
        return best

    big = max(max(r) for r in radii)
    arr = np.array([tuple(p) for p, _ in path])
    return fn, arr.min(axis=0) - big, arr.max(axis=0) + big


# ---------------------------------------------------------------------------
# Hair: strand cards along curved paths, and an atlas of hair locks.


def strand_cards(name, strands, columns):
    """Ribbon cards along polylines: each strand ``(points, widths,
    normals, column)``, one width and one facing normal per point (root
    first). UVs on atlas column ``column``, the root at the top (v = 1),
    the normals the given ones on both faces."""
    verts, faces, uvs, normals = [], [], [], []
    for points, widths, outs, column in strands:
        pts = [Vector(p) for p in points]
        n = len(pts)
        u0, u1 = column / columns, (column + 1) / columns
        base = len(verts)
        for i, (p, w, o) in enumerate(zip(pts, widths, outs)):
            t = (pts[min(i + 1, n - 1)] - pts[max(i - 1, 0)]).normalized()
            o = Vector(o).normalized()
            side = t.cross(o).normalized()
            v = 1.0 - i / (n - 1)
            verts += [p - side * (w / 2), p + side * (w / 2)]
            uvs += [(u0, v), (u1, v)]
            normals += [o, o]
        for i in range(n - 1):
            a = base + 2 * i
            faces.append((a, a + 1, a + 3, a + 2))
    mesh = bpy.data.meshes.new(name)
    mesh.from_pydata([tuple(v) for v in verts], [], faces)
    mesh.update()
    uv = mesh.uv_layers.new(name="UVMap")
    for poly in mesh.polygons:
        poly.use_smooth = True
        for li in poly.loop_indices:
            uv.data[li].uv = uvs[mesh.loops[li].vertex_index]
    mesh.normals_split_custom_set_from_vertices([tuple(n) for n in normals])
    obj = bpy.data.objects.new(name, mesh)
    bpy.context.collection.objects.link(obj)
    return obj


def hair_atlas(columns, size, seed=1, strands=90, clump=0.55, wave=0.012, clumps=1):
    """Paints ``columns`` locks of long hair hanging from the top of the
    image: many thin strands of their own length, gathering toward the tip
    (``clump``) into ``clumps`` strands-of-strands, and waving a little. Returns a neutral luminance (to be
    multiplied by the coat's mane colour), opacity, normal (OpenGL) and
    roughness arrays, rows bottom up."""
    import numpy as np

    rng = np.random.default_rng(seed)
    h = size
    w = size // columns
    lum = np.zeros((h, size), np.float32)
    alpha = np.zeros((h, size), np.float32)
    height = np.zeros((h, size), np.float32)
    ys, xs = np.mgrid[0:h, 0:w].astype(np.float32)
    v = ys / (h - 1)  # 0 at the root (top row), 1 at the tip
    u = xs / (w - 1)
    for c in range(columns):
        col_l = np.zeros((h, w), np.float32)
        col_a = np.zeros((h, w), np.float32)
        col_h = np.zeros((h, w), np.float32)
        centres = np.sort(rng.uniform(0.15, 0.85, clumps)) if clumps > 1 else np.array([0.5 + rng.uniform(-0.08, 0.08)])
        for k in range(strands):
            x0 = rng.uniform(0.08, 0.92)
            centre = centres[np.argmin(np.abs(centres - x0))] + rng.uniform(-0.02, 0.02)
            length = rng.uniform(0.55, 0.99) if k % 4 else rng.uniform(0.85, 0.99)
            gather = clump * rng.uniform(0.6, 1.2)
            phase, freq = rng.uniform(0, 6.3), rng.uniform(4, 11)
            path = x0 + (centre - x0) * gather * v ** 1.4 + wave * np.sin(freq * v * 6.28 + phase) * v
            path = path + rng.uniform(-0.05, 0.05) * v * v
            width = rng.uniform(1.0, 2.2) / w * (1.0 - 0.6 * np.clip(v / length, 0, 1))
            dist = np.abs(u - path) / width
            on = (dist < 1.0) & (v < length)
            fade = np.clip((length - v) / 0.08, 0, 1)
            shade = rng.uniform(0.72, 1.0) * (0.78 + 0.22 * np.clip(v * 3, 0, 1))
            hgt = np.where(on, (1.0 - dist * dist) * fade, 0.0)
            take = hgt > col_h
            col_h = np.where(take, hgt, col_h)
            col_l = np.where(take, shade * (0.9 + 0.1 * (1 - dist)), col_l)
            col_a = np.maximum(col_a, np.where(on & (fade > 0.3), 1.0, 0.0))
        sl = slice(c * w, (c + 1) * w)
        lum[:, sl] = col_l
        alpha[:, sl] = col_a
        height[:, sl] = col_h
    lum, alpha, height = lum[::-1], alpha[::-1], height[::-1]
    gy, gx = np.gradient(height)
    n = np.stack([-gx * 2.0, -gy * 0.5, np.ones_like(gx)], axis=-1)
    n /= np.linalg.norm(n, axis=-1, keepdims=True)
    normal = n * 0.5 + 0.5
    mean = lum[alpha > 0.5].mean() if (alpha > 0.5).any() else 0.8
    lum = np.where(alpha > 0.5, lum, mean)
    rough = np.full((h, size, 3), 0.62, np.float32)
    return np.repeat(lum[..., None], 3, axis=-1), np.repeat(alpha[..., None], 3, axis=-1), normal, rough


def sdf_capped_cone(a, b, ra, rb, rounding=0.0):
    """A truncated cone from ``a`` (radius ``ra``) to ``b`` (radius
    ``rb``) with flat ends, its edges rounded by ``rounding``."""
    import numpy as np

    a, b = np.asarray(a, np.float64), np.asarray(b, np.float64)
    ra, rb = ra - rounding, rb - rounding
    ba = b - a
    baba = float(ba.dot(ba))
    rba = rb - ra
    kk = rba * rba + baba

    def fn(x, y, z):
        px, py, pz = x - a[0], y - a[1], z - a[2]
        papa = px * px + py * py + pz * pz
        paba = (px * ba[0] + py * ba[1] + pz * ba[2]) / baba
        xx = np.sqrt(np.maximum(papa - paba * paba * baba, 0.0))
        cax = np.maximum(0.0, xx - np.where(paba < 0.5, ra, rb))
        cay = np.abs(paba - 0.5) - 0.5
        f = np.clip((rba * (xx - ra) + paba * baba) / kk, 0.0, 1.0)
        cbx = xx - ra - f * rba
        cby = paba - f
        s = np.where((cbx < 0.0) & (cay < 0.0), -1.0, 1.0)
        return s * np.sqrt(np.minimum(cax * cax + cay * cay * baba, cbx * cbx + cby * cby * baba)) - rounding

    big = max(ra, rb) + rounding
    return fn, np.minimum(a, b) - big, np.maximum(a, b) + big
