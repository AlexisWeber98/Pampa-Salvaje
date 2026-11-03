"""Convert an animated FBX into a GLB the game can load, inside Blender.

Run it headless, never from inside the folder that holds the input:

    blender -b --factory-startup --python tools/fbx2glb.py -- \
        INPUT.fbx OUTPUT.glb [--turn DEGREES] [--triangles N] \
        [--rename OLD=NEW ...] [--rest-clip NAME]

The FBX is treated as untrusted data: Blender's importer parses it and no
script inside it runs (``--factory-startup`` keeps auto-run off). Steps:

- ``--turn`` spins the whole model about the vertical axis and applies it,
  so the exported meshes and bones face glTF +Z, like the Quaternius
  FBX2glTF exports the game already loads (Blender's -Y);
- ``--triangles`` decimates each mesh down to about that many triangles
  (collapse), before the armature deforms it;
- every action named ``…|CLIP`` is renamed to ``CLIP``, then ``--rename``
  renames clips (``OLD=NEW``);
- ``--rest-clip`` adds a still clip of the rest pose (for example the
  spread wings of a bird built gliding);
- the GLB is written Y up, with skins, every action as its own clip, and
  plain materials.

Run ``tools/glbtrim.py`` on the result afterwards to order the clips and
smooth the normals. See tools/README.md.
"""

import argparse
import math
import sys

import bpy
import mathutils


def parse(argv):
    parser = argparse.ArgumentParser(prog="fbx2glb.py")
    parser.add_argument("input")
    parser.add_argument("output")
    parser.add_argument("--turn", type=float, default=0.0, metavar="DEGREES")
    parser.add_argument("--triangles", type=int, default=0, metavar="N")
    parser.add_argument("--rename", nargs="*", default=[], metavar="OLD=NEW")
    parser.add_argument("--rest-clip", default=None, metavar="NAME")
    return parser.parse_args(argv)


def triangles(mesh):
    return sum(len(p.vertices) - 2 for p in mesh.data.polygons)


def main():
    argv = sys.argv[sys.argv.index("--") + 1 :] if "--" in sys.argv else []
    args = parse(argv)
    bpy.ops.wm.read_factory_settings(use_empty=True)
    bpy.ops.import_scene.fbx(filepath=args.input)
    objects = list(bpy.data.objects)
    armatures = [o for o in objects if o.type == "ARMATURE"]
    meshes = [o for o in objects if o.type == "MESH"]
    if len(armatures) != 1 or not meshes:
        raise SystemExit(f"expected one armature and its meshes, found {len(armatures)} and {len(meshes)}")
    armature = armatures[0]

    if args.turn:
        # About the world origin, not the armature's own origin.
        turn = mathutils.Matrix.Rotation(math.radians(args.turn), 4, "Z")
        armature.matrix_world = turn @ armature.matrix_world
        bpy.context.view_layer.update()
        bpy.ops.object.select_all(action="DESELECT")
        for o in objects:
            o.select_set(True)
        bpy.context.view_layer.objects.active = armature
        bpy.ops.object.transform_apply(location=True, rotation=True, scale=True)

    if args.triangles:
        for mesh in meshes:
            have = triangles(mesh)
            if have <= args.triangles:
                continue
            bpy.ops.object.select_all(action="DESELECT")
            mesh.select_set(True)
            bpy.context.view_layer.objects.active = mesh
            decimate = mesh.modifiers.new("Decimate", "DECIMATE")
            decimate.ratio = args.triangles / have
            # Before the armature deforms the mesh.
            bpy.ops.object.modifier_move_to_index(modifier=decimate.name, index=0)
            bpy.ops.object.modifier_apply(modifier=decimate.name)
            print(f"fbx2glb: {mesh.name}: {have} -> {triangles(mesh)} triangles")

    renames = dict(r.split("=", 1) for r in args.rename)
    for action in bpy.data.actions:
        name = action.name.rsplit("|", 1)[-1]
        action.name = renames.get(name, name)

    if args.rest_clip:
        rest = bpy.data.actions.new(args.rest_clip)
        armature.animation_data_create()
        armature.animation_data.action = rest
        for bone in armature.pose.bones:
            bone.location = (0.0, 0.0, 0.0)
            bone.rotation_mode = "QUATERNION"
            bone.rotation_quaternion = (1.0, 0.0, 0.0, 0.0)
            bone.scale = (1.0, 1.0, 1.0)
            for frame in (1, 25):
                bone.keyframe_insert("location", frame=frame)
                bone.keyframe_insert("rotation_quaternion", frame=frame)
        armature.animation_data.action = None

    for action in bpy.data.actions:
        action.use_fake_user = True
    bpy.ops.export_scene.gltf(
        filepath=args.output,
        export_format="GLB",
        export_yup=True,
        export_skins=True,
        export_animations=True,
        export_animation_mode="ACTIONS",
        export_force_sampling=True,
        export_image_format="NONE",
        export_cameras=False,
        export_lights=False,
    )
    print(f"fbx2glb: wrote {args.output}: clips {[a.name for a in bpy.data.actions]}")


main()
