"""Turn the Sketchfab "Colt Single Action Army .45" by Guy in a Poncho
(ponchoguy, CC-BY-4.0) into the game's revolver, in Blender, by script.

    blender -b --factory-startup --python tools/revolver.py -- SOURCE.gltf OUT.glb

SOURCE is the Sketchfab glTF download (``scene.gltf`` of
``colt_single_action_army_.45.zip``,
https://sketchfab.com/3d-models/colt-single-action-army-45-b02548a3ecb84e3cbcfc6f4e2cf03992):
one mesh of 52,616 vertices (about 74,000 triangles), one PBR material
(``Colt_SAA``: base colour, metallic-roughness and normal maps, 4096 px), no
armature, already about life size (0.33 m from the hammer to the muzzle). It
is read only by Blender's glTF importer (data, never executed); nothing is
written next to it.

The game holds it in a rider's right hand (``crates/render/src/character/
gunfire.rs``), so OUT keeps this contract:

- the barrel points along -Z (glTF) and the top of the frame up, +Y; the
  origin is the middle of the grip, where the hand closes on it
  (:func:`orient` finds the barrel, the muzzle end and the grip from the
  vertices alone);
- the muzzle, where the flash comes out, is the node ``Muzzle``; its place
  is printed, and the game's ``MUZZLE`` must match it
  (``tools/test_revolver.py`` checks both);
- the mesh is decimated to ``BUDGET`` triangles and its maps shrunk to
  ``MAPS`` pixels: it is a hand-held item seen from a few metres.
"""

import os
import sys

import numpy as np

HERE = os.path.dirname(os.path.abspath(__file__))

# Triangles of the shipped mesh.
BUDGET = 6000
# Largest side of each map, in pixels.
MAPS = 1024
# Share of the length, at each end, that tells the muzzle from the hammer.
END = 0.12
# Grip vertices: below the barrel by more than this share of the grip's
# drop, in the rear half.
GRIP_DROP = 0.45


def orient(verts):
    """How to turn the source's ``verts`` (Blender frame, N x 3) into the
    game's revolver frame in Blender (the muzzle towards +Y, which glTF
    writes as -Z; the top of the frame +Z, glTF +Y): returns the rotation
    ``r`` (3 x 3, rows are the new axes in source coordinates), the grip
    point to put at the origin and the muzzle point, both in source
    coordinates.

    The barrel runs along the longest side of the bounding box; the muzzle
    is the end that is thinner across; the height is the next longest side,
    and the grip hangs on the side of it farther from the barrel's line."""
    v = np.asarray(verts, dtype=float)
    lo, hi = v.min(axis=0), v.max(axis=0)
    size = hi - lo
    long = int(np.argmax(size))
    rest = [a for a in range(3) if a != long]
    length = size[long]
    ends = [v[v[:, long] <= lo[long] + END * length], v[v[:, long] >= hi[long] - END * length]]
    thickness = [np.ptp(e[:, rest], axis=0).max() for e in ends]
    forward = 1.0 if thickness[1] < thickness[0] else -1.0
    muzzle_end = ends[1] if forward > 0 else ends[0]
    height = max(rest, key=lambda a: size[a])
    line = np.median(muzzle_end[:, height])
    down = -1.0 if line - lo[height] > hi[height] - line else 1.0
    y = np.zeros(3)
    y[long] = forward
    z = np.zeros(3)
    z[height] = -down
    x = np.cross(y, z)
    r = np.array([x, y, z])
    # The grip: the rear half, well below the barrel.
    drop = (v[:, height] - line) * down
    rear = (v[:, long] - (lo[long] + hi[long]) / 2.0) * forward < 0.0
    grip = v[rear & (drop > GRIP_DROP * drop.max())].mean(axis=0)
    tip = muzzle_end.mean(axis=0)
    tip[long] = hi[long] if forward > 0 else lo[long]
    return r, grip, tip


def to_gltf(p):
    """A point of the revolver frame in Blender in glTF (Y up)."""
    return (p[0], p[2], -p[1])


def main(argv):
    import bmesh
    import bpy
    from mathutils import Matrix, Vector

    src, out = argv
    bpy.ops.wm.read_factory_settings(use_empty=True)
    bpy.ops.import_scene.gltf(filepath=os.path.abspath(src))
    meshes = [o for o in bpy.data.objects if o.type == "MESH"]
    for o in meshes:
        o.data.transform(o.matrix_world)
        o.parent = None
        o.matrix_world = Matrix.Identity(4)
    for o in list(bpy.data.objects):
        if o.type != "MESH":
            bpy.data.objects.remove(o)
    for o in bpy.context.scene.objects:
        o.select_set(True)
    bpy.context.view_layer.objects.active = meshes[0]
    if len(meshes) > 1:
        bpy.ops.object.join()
    obj = bpy.context.view_layer.objects.active
    obj.name = obj.data.name = "Revolver"

    verts = np.array([v.co[:] for v in obj.data.vertices])
    r, grip, tip = orient(verts)
    turn = Matrix([list(row) for row in r]).to_4x4()
    turn.translation = Vector(-(r @ grip))
    obj.data.transform(turn)
    muzzle = r @ (tip - grip)

    # Welded, or the collapse tears holes along the UV seams.
    bm = bmesh.new()
    bm.from_mesh(obj.data)
    bmesh.ops.remove_doubles(bm, verts=bm.verts, dist=1e-5)
    bm.to_mesh(obj.data)
    bm.free()
    have = sum(len(p.vertices) - 2 for p in obj.data.polygons)
    if have > BUDGET:
        mod = obj.modifiers.new("Decimate", "DECIMATE")
        mod.ratio = BUDGET / have
        mod.use_collapse_triangulate = True
        bpy.ops.object.modifier_apply(modifier=mod.name)
    if obj.data.has_custom_normals:
        bpy.ops.mesh.customdata_custom_splitnormals_clear()
    for p in obj.data.polygons:
        p.use_smooth = True
    obj.data.set_sharp_from_angle(angle=np.radians(35.0))

    for image in bpy.data.images:
        w, h = image.size
        if max(w, h) > MAPS:
            image.scale(w * MAPS // max(w, h), h * MAPS // max(w, h))

    marker = bpy.data.objects.new("Muzzle", None)
    marker.location = Vector(muzzle)
    bpy.context.scene.collection.objects.link(marker)

    bpy.ops.export_scene.gltf(
        filepath=os.path.abspath(out),
        export_format="GLB",
        export_yup=True,
        export_image_format="AUTO",
        export_apply=True,
    )
    tris = sum(len(p.vertices) - 2 for p in obj.data.polygons)
    print(f"revolver: {tris} triangles, muzzle (glTF) = {tuple(round(c, 4) for c in to_gltf(muzzle))}")


if __name__ == "__main__":
    main(sys.argv[sys.argv.index("--") + 1 :])
