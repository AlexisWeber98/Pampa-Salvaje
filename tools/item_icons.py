"""Render the inventory's item icons from 3D models, inside Blender.

Run it headless, from the repository root:

    blender -b --factory-startup --python tools/item_icons.py -- \
        --polyhaven ~/Downloads/polyhaven -o assets/textures/ui/items \
        [--only NAME ...] [--size 256] [--samples 96]

Each icon is one model alone in an empty scene, laid out for a small picture
(``tilt``: long tools lie across the frame, from corner to corner; upright
things stand), framed by an orthographic camera a little above, three
quarters on, lit by a warm key, a cool fill and a rim, and rendered by Cycles
with a transparent background into ``NAME.png`` (RGBA, ``--size`` pixels a
side). ``ICONS`` are Poly Haven models; ``MODELS`` are our own, modelled
below in Python at their real size: the drinks, food, sacks, rawhide,
clothing, arms, the letter and the coins (``coins.png``, the money row).
Their randomness is seeded, so every run gives the same icons.

Models from Poly Haven (CC0) are read from ``--polyhaven``, one folder per
asset as its 1K glTF (``ID/ID_1k.gltf``); they are not kept in the
repository, only the icons are; some of our models wear Poly Haven textures
(``brown_leather``, ``rough_linen``, ``poly_wool_herringbone``) read from the
same folder as ``ID/ID_KIND_2k.png``. These files are treated as untrusted data:
Blender's importer parses them and ``--factory-startup`` keeps scripts from
running.
"""

import argparse
import math
import os
import random
import sys

import bpy
import mathutils

# The icons: file name -> (Poly Haven id, object to keep (None: all), degrees
# to tilt the model's long axis from vertical across the frame, fix-up).
ICONS = {
    "axe": ("wooden_axe_03", None, -45.0, None),
    "shovel": ("rusted_spade_01", None, -45.0, None),
    "knife": ("fish_knife", None, 0.0, "knife"),
    "candles": ("wooden_candlestick", None, 0.0, None),
    "cane_bottle": ("wine_bottles_01", "wine_bottles_01_burgundy", 0.0, "cane"),
}

# How far above the horizon the camera looks down, and how far round the
# model it stands, degrees.
ELEVATION = 22.0
AZIMUTH = 30.0
# Room left round the model, as a share of the picture.
MARGIN = 0.08


def parse(argv):
    parser = argparse.ArgumentParser(prog="item_icons.py")
    parser.add_argument("--polyhaven", required=True, help="folder of Poly Haven glTF assets, one folder each")
    parser.add_argument("-o", "--output", required=True, help="folder for the PNG icons")
    parser.add_argument("--only", nargs="*", default=None, metavar="NAME")
    parser.add_argument("--size", type=int, default=256)
    parser.add_argument("--samples", type=int, default=96)
    return parser.parse_args(argv)


def meshes():
    return [o for o in bpy.context.scene.objects if o.type == "MESH"]


def keep_only(name):
    for o in meshes():
        if o.name != name:
            bpy.data.objects.remove(o, do_unlink=True)


def fix_knife():
    """The knife lies along +X: stand it up so the common tilt lays it
    across the frame like the other tools."""
    for o in meshes():
        o.rotation_euler.rotate(mathutils.Euler((0.0, math.radians(-90.0), 0.0)))
    tilt_all(-45.0)


def fix_cane():
    """A plain bottle of caña: the modern wine label taken off (its faces
    removed), the wine turned to a pale amber spirit."""
    bottle = meshes()[0]

    def part(m):
        return m.name.removeprefix("wine_bottles_01_") if m else ""

    label = [i for i, m in enumerate(bottle.data.materials) if not part(m).startswith(("glass", "seal", "wine"))]
    mesh = bottle.data
    import bmesh

    bm = bmesh.new()
    bm.from_mesh(mesh)
    gone = [f for f in bm.faces if f.material_index in label]
    bmesh.ops.delete(bm, geom=gone, context="FACES")
    bm.to_mesh(mesh)
    bm.free()
    for m in mesh.materials:
        if part(m).startswith("wine"):
            for node in m.node_tree.nodes:
                if node.type == "BSDF_PRINCIPLED":
                    node.inputs["Base Color"].default_value = (0.85, 0.55, 0.18, 1.0)


FIXES = {"knife": fix_knife, "cane": fix_cane}


def tilt_all(degrees):
    """Turns every mesh about the camera's line of sight, round the models'
    middle, so a long vertical thing lies across the frame."""
    if degrees == 0.0:
        return
    turn = mathutils.Matrix.Rotation(math.radians(degrees), 4, view_axis())
    for o in meshes():
        o.matrix_world = turn @ o.matrix_world


def view_axis():
    el, az = math.radians(ELEVATION), math.radians(AZIMUTH)
    return mathutils.Vector((math.sin(az) * math.cos(el), -math.cos(az) * math.cos(el), math.sin(el)))


def corners():
    out = []
    bpy.context.view_layer.update()
    for o in meshes():
        out.extend(o.matrix_world @ mathutils.Vector(c) for c in o.bound_box)
    return out


def frame(size):
    """An orthographic camera looking at the models along ``view_axis``,
    just wide enough for them and ``MARGIN``."""
    look = -view_axis()
    cam_data = bpy.data.cameras.new("icon")
    cam_data.type = "ORTHO"
    cam = bpy.data.objects.new("icon", cam_data)
    bpy.context.scene.collection.objects.link(cam)
    rot = look.to_track_quat("-Z", "Y")
    cam.rotation_euler = rot.to_euler()
    pts = corners()
    to_cam = rot.to_matrix().inverted()
    local = [to_cam @ p for p in pts]
    lo = mathutils.Vector((min(v.x for v in local), min(v.y for v in local), min(v.z for v in local)))
    hi = mathutils.Vector((max(v.x for v in local), max(v.y for v in local), max(v.z for v in local)))
    middle = rot.to_matrix() @ ((lo + hi) / 2.0)
    cam.location = middle - look * (hi.z - lo.z + 2.0)
    cam_data.ortho_scale = max(hi.x - lo.x, hi.y - lo.y) / (1.0 - 2.0 * MARGIN)
    cam_data.clip_end = (hi.z - lo.z) + 10.0
    bpy.context.scene.camera = cam
    scene = bpy.context.scene
    scene.render.resolution_x = size
    scene.render.resolution_y = size
    return cam, max(hi.x - lo.x, hi.y - lo.y)


def light(name, energy, color, direction, size):
    data = bpy.data.lights.new(name, "AREA")
    data.energy = energy
    data.color = color
    data.size = size
    lamp = bpy.data.objects.new(name, data)
    bpy.context.scene.collection.objects.link(lamp)
    d = mathutils.Vector(direction).normalized()
    lamp.location = d * 4.0
    lamp.rotation_euler = (-d).to_track_quat("-Z", "Y").to_euler()


def lights():
    """A warm key above and to the left, a cool dim fill from the right, and a
    rim from behind, all as far from the model, so every icon is lit alike
    whatever its size."""
    light("key", 520.0, (1.0, 0.92, 0.80), (-1.0, -1.2, 1.4), 2.0)
    light("fill", 150.0, (0.80, 0.86, 1.0), (1.4, -0.8, 0.3), 3.0)
    light("rim", 300.0, (1.0, 0.95, 0.88), (0.3, 1.5, 1.0), 1.5)
    world = bpy.data.worlds.new("icon")
    world.node_tree.nodes["Background"].inputs["Color"].default_value = (0.22, 0.20, 0.18, 1.0)
    world.node_tree.nodes["Background"].inputs["Strength"].default_value = 0.6
    bpy.context.scene.world = world


def render(path, samples):
    scene = bpy.context.scene
    scene.render.engine = "CYCLES"
    scene.cycles.device = "CPU"
    scene.cycles.samples = samples
    scene.cycles.use_denoising = True
    scene.render.film_transparent = True
    scene.view_settings.view_transform = "Standard"
    scene.render.image_settings.file_format = "PNG"
    scene.render.image_settings.color_mode = "RGBA"
    scene.render.filepath = path
    bpy.ops.render.render(write_still=True)


# ---------------------------------------------------------------------------
# Our own models: the goods no CC0 model fits, modelled here in Blender at
# their real size (metres), standing on z = 0, their front towards -Y (the
# camera stands at +X, -Y, above). Each builder makes plain mesh objects
# (``bake`` applies their modifiers), so the framing above sees them whole.
# ---------------------------------------------------------------------------

# Principled BSDF inputs by short name.
PRINCIPLED = {
    "color": "Base Color",
    "rough": "Roughness",
    "metal": "Metallic",
    "ior": "IOR",
    "transmission": "Transmission Weight",
    "coat": "Coat Weight",
    "coat_rough": "Coat Roughness",
    "sheen": "Sheen Weight",
    "sheen_rough": "Sheen Roughness",
    "sss": "Subsurface Weight",
    "sss_radius": "Subsurface Radius",
    "sss_scale": "Subsurface Scale",
    "specular": "Specular IOR Level",
}


def rgba(c):
    return tuple(c) + (1.0,) if len(c) == 3 else tuple(c)


class Shader:
    """A Principled BSDF material and a few node helpers to drive it: noise,
    waves, colour ramps, bumps, image maps by box projection."""

    def __init__(self, name, **inputs):
        self.mat = bpy.data.materials.new(name)
        if self.mat.node_tree is None:
            self.mat.use_nodes = True
        self.nt = self.mat.node_tree
        self.bsdf = next(n for n in self.nt.nodes if n.type == "BSDF_PRINCIPLED")
        self.out = next(n for n in self.nt.nodes if n.type == "OUTPUT_MATERIAL")
        self.normal = None
        self.set(**inputs)

    def node(self, kind, **attrs):
        n = self.nt.nodes.new(kind)
        for k, v in attrs.items():
            setattr(n, k, v)
        return n

    def put(self, socket, value):
        if isinstance(value, bpy.types.NodeSocket):
            self.nt.links.new(value, socket)
        elif socket.type == "RGBA":
            socket.default_value = rgba(value)
        else:
            socket.default_value = value

    def set(self, **inputs):
        for k, v in inputs.items():
            self.put(self.bsdf.inputs[PRINCIPLED[k]], v)
        return self

    def coord(self, scale=(1.0, 1.0, 1.0), kind="Object", offset=(0.0, 0.0, 0.0)):
        tc = self.node("ShaderNodeTexCoord")
        mp = self.node("ShaderNodeMapping")
        mp.inputs["Scale"].default_value = scale
        mp.inputs["Location"].default_value = offset
        self.nt.links.new(tc.outputs[kind], mp.inputs["Vector"])
        return mp.outputs["Vector"]

    def noise(self, scale, detail=4.0, rough=0.55, vec=None, out="Fac", distortion=0.0):
        n = self.node("ShaderNodeTexNoise")
        n.inputs["Scale"].default_value = scale
        n.inputs["Detail"].default_value = detail
        n.inputs["Roughness"].default_value = rough
        n.inputs["Distortion"].default_value = distortion
        if vec is not None:
            self.nt.links.new(vec, n.inputs["Vector"])
        return n.outputs[out]

    def voronoi(self, scale, vec=None, out="Distance", randomness=1.0):
        n = self.node("ShaderNodeTexVoronoi")
        n.inputs["Scale"].default_value = scale
        n.inputs["Randomness"].default_value = randomness
        if vec is not None:
            self.nt.links.new(vec, n.inputs["Vector"])
        return n.outputs[out]

    def wave(self, scale, vec=None, direction="X", distortion=0.0, detail=2.0, profile="SIN", rings=False):
        n = self.node("ShaderNodeTexWave", wave_type="RINGS" if rings else "BANDS", wave_profile=profile)
        if rings:
            n.rings_direction = "SPHERICAL" if direction == "S" else direction
        else:
            n.bands_direction = direction
        n.inputs["Scale"].default_value = scale
        n.inputs["Distortion"].default_value = distortion
        n.inputs["Detail"].default_value = detail
        if vec is not None:
            self.nt.links.new(vec, n.inputs["Vector"])
        return n.outputs["Fac"]

    def ramp(self, fac, stops, interpolation="LINEAR"):
        r = self.node("ShaderNodeValToRGB")
        r.color_ramp.interpolation = interpolation
        els = r.color_ramp.elements
        stops = sorted(stops, key=lambda st: st[0])
        els[0].position, els[0].color = stops[0][0], rgba(stops[0][1])
        els[1].position, els[1].color = stops[-1][0], rgba(stops[-1][1])
        for pos, col in stops[1:-1]:
            els.new(pos).color = rgba(col)
        self.put(r.inputs["Fac"], fac)
        return r.outputs["Color"]

    def mix(self, fac, a, b, blend="MIX"):
        n = self.node("ShaderNodeMix", data_type="RGBA", blend_type=blend)
        ins = {s.identifier: s for s in n.inputs}
        self.put(ins["Factor_Float"], fac)
        self.put(ins["A_Color"], a)
        self.put(ins["B_Color"], b)
        return next(s for s in n.outputs if s.identifier == "Result_Color")

    def math(self, op, a, b=0.0, clamp=False):
        n = self.node("ShaderNodeMath", operation=op, use_clamp=clamp)
        self.put(n.inputs[0], a)
        self.put(n.inputs[1], b)
        return n.outputs[0]

    def axis(self, vec, name):
        n = self.node("ShaderNodeSeparateXYZ")
        self.nt.links.new(vec, n.inputs["Vector"])
        return n.outputs[name]

    def band(self, value, lo, hi, soft=0.0):
        """1 where ``lo < value < hi``, softened by ``soft``."""
        if soft == 0.0:
            a = self.math("GREATER_THAN", value, lo)
            b = self.math("LESS_THAN", value, hi)
        else:
            a = self._smooth(value, lo - soft, lo + soft)
            b = self.math("SUBTRACT", 1.0, self._smooth(value, hi - soft, hi + soft))
        return self.math("MULTIPLY", a, b)

    def _smooth(self, value, lo, hi):
        n = self.node("ShaderNodeMapRange", interpolation_type="SMOOTHSTEP")
        self.put(n.inputs["Value"], value)
        n.inputs["From Min"].default_value = lo
        n.inputs["From Max"].default_value = hi
        return n.outputs["Result"]

    def _chain(self, normal_in, normal_out):
        if self.normal is not None:
            self.nt.links.new(self.normal, normal_in)
        self.normal = normal_out
        self.nt.links.new(normal_out, self.bsdf.inputs["Normal"])

    def bump(self, height, strength=1.0, distance=0.001):
        n = self.node("ShaderNodeBump")
        n.inputs["Strength"].default_value = strength
        n.inputs["Distance"].default_value = distance
        self.put(n.inputs["Height"], height)
        self._chain(n.inputs["Normal"], n.outputs["Normal"])
        return self

    def image(self, path, scale, non_color=False):
        img = bpy.data.images.load(path, check_existing=True)
        if non_color:
            img.colorspace_settings.name = "Non-Color"
        n = self.node("ShaderNodeTexImage", projection="BOX", projection_blend=0.3)
        n.image = img
        self.nt.links.new(self.coord((scale, scale, scale)), n.inputs["Vector"])
        return n.outputs["Color"]

    def normal_map(self, path, scale, strength=1.0):
        col = self.image(path, scale, non_color=True)
        n = self.node("ShaderNodeNormalMap")
        n.inputs["Strength"].default_value = strength
        self.nt.links.new(col, n.inputs["Color"])
        # A normal map starts the chain: later bumps perturb it.
        self.normal = n.outputs["Normal"]
        self.nt.links.new(self.normal, self.bsdf.inputs["Normal"])
        return self

    def ao(self, distance):
        n = self.node("ShaderNodeAmbientOcclusion")
        n.inputs["Distance"].default_value = distance
        return n.outputs["AO"]

    def volume(self, color, density):
        n = self.node("ShaderNodeVolumeAbsorption")
        n.inputs["Color"].default_value = rgba(color)
        n.inputs["Density"].default_value = density
        self.nt.links.new(n.outputs["Volume"], self.out.inputs["Volume"])
        return self


def mottled(name, color, rough, var=0.25, scale=60.0, grain=0.0, grain_scale=400.0, distance=0.0004, **inputs):
    """A material of one colour, mottled lighter and darker by noise, with a
    fine grain bump: the common case."""
    s = Shader(name, rough=rough, **inputs)
    lo = tuple(c * (1.0 - var) for c in color)
    hi = tuple(min(1.0, c * (1.0 + var)) for c in color)
    s.set(color=s.ramp(s.noise(scale, 6.0, 0.6), [(0.3, lo), (0.7, hi)]))
    if grain:
        s.bump(s.noise(grain_scale, 8.0, 0.7), grain, distance)
    return s


def texture_path(args, asset, kind):
    path = os.path.join(os.path.expanduser(args.polyhaven), asset, f"{asset}_{kind}_2k.png")
    if not os.path.isfile(path):
        sys.exit(f"item_icons: {path} not found (download it first)")
    return path


# --- Geometry ---------------------------------------------------------------

V = mathutils.Vector


class Geo:
    """Vertices and faces gathered before they become one mesh."""

    def __init__(self, verts=(), faces=()):
        self.verts = [V(v) for v in verts]
        self.faces = [tuple(f) for f in faces]

    def add(self, other):
        base = len(self.verts)
        self.verts += other.verts
        self.faces += [tuple(i + base for i in f) for f in other.faces]
        return self

    def moved(self, matrix):
        return Geo([matrix @ v for v in self.verts], self.faces)

    def object(self, name, mat=None, smooth=True):
        me = bpy.data.meshes.new(name)
        me.from_pydata([tuple(v) for v in self.verts], [], self.faces)
        me.validate()
        return mesh_object(name, me, mat, smooth)


def mesh_object(name, me, mat=None, smooth=True):
    import bmesh

    bm = bmesh.new()
    bm.from_mesh(me)
    bmesh.ops.remove_doubles(bm, verts=bm.verts, dist=1e-7)
    bmesh.ops.recalc_face_normals(bm, faces=bm.faces)
    bm.to_mesh(me)
    bm.free()
    if mat is not None:
        me.materials.append(mat.mat if isinstance(mat, Shader) else mat)
    me.polygons.foreach_set("use_smooth", [smooth] * len(me.polygons))
    o = bpy.data.objects.new(name, me)
    bpy.context.scene.collection.objects.link(o)
    return o


def bm_object(name, bm, mat=None, smooth=True):
    me = bpy.data.meshes.new(name)
    bm.to_mesh(me)
    bm.free()
    return mesh_object(name, me, mat, smooth)


def lathe(profile, segments=64, start=0.0):
    """A solid of revolution about Z from ``(r, z)`` points; points on the
    axis (r = 0) close it with a fan."""
    g = Geo()
    rings = []
    for r, z in profile:
        if r < 1e-7:
            rings.append([len(g.verts)])
            g.verts.append(V((0.0, 0.0, z)))
        else:
            ring = []
            for j in range(segments):
                a = start + 2.0 * math.pi * j / segments
                ring.append(len(g.verts))
                g.verts.append(V((r * math.cos(a), r * math.sin(a), z)))
            rings.append(ring)
    for a, b in zip(rings, rings[1:]):
        for j in range(segments):
            j2 = (j + 1) % segments
            if len(a) == 1:
                g.faces.append((a[0], b[j2], b[j]))
            elif len(b) == 1:
                g.faces.append((a[j], a[j2], b[0]))
            else:
                g.faces.append((a[j], a[j2], b[j2], b[j]))
    return g


def spline(points, steps=8):
    """A Catmull-Rom curve through ``points``."""
    p = [V(q) for q in points]
    p = [p[0]] + p + [p[-1]]
    out = []
    for i in range(1, len(p) - 2):
        p0, p1, p2, p3 = p[i - 1], p[i], p[i + 1], p[i + 2]
        for s in range(steps):
            t = s / steps
            out.append(
                0.5
                * (
                    2.0 * p1
                    + (p2 - p0) * t
                    + (2.0 * p0 - 5.0 * p1 + 4.0 * p2 - p3) * t * t
                    + (3.0 * p1 - p0 - 3.0 * p2 + p3) * t * t * t
                )
            )
    out.append(p[-2])
    return out


def frames(path, up):
    """Tangents and normals along ``path`` by parallel transport."""
    n = len(path)
    ts = [(path[min(i + 1, n - 1)] - path[max(i - 1, 0)]).normalized() for i in range(n)]
    up = V(up)
    first = up - up.dot(ts[0]) * ts[0]
    if first.length < 1e-6:
        first = ts[0].orthogonal()
    ns = [first.normalized()]
    for t in ts[1:]:
        v = ns[-1] - ns[-1].dot(t) * t
        ns.append(v.normalized() if v.length > 1e-9 else ns[-1])
    return ts, ns


def sweep(path, profile, up=(0.0, 0.0, 1.0), shape=None, twist=None, caps=True):
    """A tube along ``path`` with the cross-section ``profile`` (``(u, v)``
    points round it, u along the transported normal); ``shape(t)`` scales u
    and v, ``twist(t)`` turns the section (radians), t from 0 to 1."""
    path = [V(p) for p in path]
    ts, ns = frames(path, up)
    k, n = len(profile), len(path)
    g = Geo()
    for i, (p, t, nv) in enumerate(zip(path, ts, ns)):
        bv = t.cross(nv)
        f = i / max(n - 1, 1)
        su, sv = shape(f) if shape else (1.0, 1.0)
        a = twist(f) if twist else 0.0
        c, s = math.cos(a), math.sin(a)
        for u, v in profile:
            u, v = u * su, v * sv
            g.verts.append(p + nv * (u * c - v * s) + bv * (u * s + v * c))
    for i in range(n - 1):
        a, b = i * k, (i + 1) * k
        for j in range(k):
            j2 = (j + 1) % k
            g.faces.append((a + j, a + j2, b + j2, b + j))
    if caps:
        g.faces.append(tuple(reversed(range(k))))
        g.faces.append(tuple((n - 1) * k + j for j in range(k)))
    return g


def circle(r, k=10, squash=1.0):
    return [(r * math.cos(2 * math.pi * j / k), squash * r * math.sin(2 * math.pi * j / k)) for j in range(k)]


def rounded_rect(w, h, r, k=4):
    """A rectangle ``w`` by ``h`` with corners of radius ``r``, round."""
    r = min(r, w / 2.0, h / 2.0)
    pts = []
    for cx, cy, a0 in ((w / 2 - r, h / 2 - r, 0.0), (-w / 2 + r, h / 2 - r, 90.0), (-w / 2 + r, -h / 2 + r, 180.0), (w / 2 - r, -h / 2 + r, 270.0)):
        for j in range(k + 1):
            a = math.radians(a0 + 90.0 * j / k)
            pts.append((cx + r * math.cos(a), cy + r * math.sin(a)))
    return pts


def prism(points, depth, axis="Y"):
    """A flat shape (``(x, z)`` points, one loop) pushed ``depth`` across
    ``axis`` Y, centred on it: the side view of a gun's parts."""
    g = Geo()
    n = len(points)
    for side in (-0.5, 0.5):
        for x, z in points:
            g.verts.append(V((x, side * depth, z)))
    g.faces.append(tuple(range(n)))
    g.faces.append(tuple(reversed(range(n, 2 * n))))
    for j in range(n):
        j2 = (j + 1) % n
        g.faces.append((j, j2, n + j2, n + j))
    return g


def box(size, center=(0.0, 0.0, 0.0)):
    sx, sy, sz = (s / 2.0 for s in size)
    cx, cy, cz = center
    verts = [(cx + x * sx, cy + y * sy, cz + z * sz) for x in (-1, 1) for y in (-1, 1) for z in (-1, 1)]
    faces = [(0, 1, 3, 2), (4, 6, 7, 5), (0, 4, 5, 1), (2, 3, 7, 6), (0, 2, 6, 4), (1, 5, 7, 3)]
    return Geo(verts, faces)


def grid(nx, ny, w, h):
    """A flat grid in XY, ``w`` by ``h``, centred."""
    g = Geo()
    for j in range(ny + 1):
        for i in range(nx + 1):
            g.verts.append(V(((i / nx - 0.5) * w, (j / ny - 0.5) * h, 0.0)))
    for j in range(ny):
        for i in range(nx):
            a = j * (nx + 1) + i
            g.faces.append((a, a + 1, a + nx + 2, a + nx + 1))
    return g


def ellipsoid(r, seg=24, rings=12):
    """A UV sphere scaled to the radii ``r``."""
    g = lathe([(0.0, -1.0)] + [(math.sin(math.pi * i / rings), -math.cos(math.pi * i / rings)) for i in range(1, rings)] + [(0.0, 1.0)], seg)
    return Geo([V((v.x * r[0], v.y * r[1], v.z * r[2])) for v in g.verts], g.faces)


def roughen(g, amount, scale, seed=0.0, axes=(1.0, 1.0, 1.0)):
    """Pushes every vertex by smooth noise: wrinkles, dents, a hand-made look."""
    off = V((seed * 17.3, seed * 5.1, seed * 11.7))
    for v in g.verts:
        d = mathutils.noise.noise_vector(v * scale + off)
        v.x += d.x * amount * axes[0]
        v.y += d.y * amount * axes[1]
        v.z += d.z * amount * axes[2]
    return g


def place(o, loc=(0.0, 0.0, 0.0), rot=(0.0, 0.0, 0.0)):
    o.location = loc
    o.rotation_euler = tuple(math.radians(a) for a in rot)
    return o


def modifier(o, kind, **attrs):
    m = o.modifiers.new(kind.lower(), kind)
    for k, v in attrs.items():
        setattr(m, k, v)
    return m


def bevel(o, width, segments=3, angle=35.0):
    return modifier(o, "BEVEL", width=width, segments=segments, limit_method="ANGLE", angle_limit=math.radians(angle), harden_normals=True)


def subsurf(o, levels=1):
    return modifier(o, "SUBSURF", levels=levels, render_levels=levels)


def cut(o, cutter):
    """Takes ``cutter`` out of ``o`` (the cutter is removed by ``bake``)."""
    cutter["cutter"] = True
    cutter.hide_render = True
    return modifier(o, "BOOLEAN", operation="DIFFERENCE", object=cutter, solver="EXACT")


def bake():
    """Applies every modifier, so the framing measures what renders, and
    removes the boolean cutters."""
    scene = bpy.context.scene
    bpy.context.view_layer.update()
    dg = bpy.context.evaluated_depsgraph_get()
    done = {}
    for o in scene.objects:
        if o.type == "MESH" and o.modifiers and not o.get("cutter"):
            done[o] = bpy.data.meshes.new_from_object(o.evaluated_get(dg), preserve_all_data_layers=True, depsgraph=dg)
    for o, me in done.items():
        o.modifiers.clear()
        o.data = me
    for o in list(scene.objects):
        if o.get("cutter"):
            bpy.data.objects.remove(o, do_unlink=True)


def turn(degrees, axis="Z"):
    return mathutils.Matrix.Rotation(math.radians(degrees), 4, axis)


def at(x, y, z):
    return mathutils.Matrix.Translation((x, y, z))


def exposure(stops):
    """Darkens the picture by ``stops`` (negative): for things with broad
    flat tops, which the key light from above would wash out."""
    bpy.context.scene.view_settings.exposure = stops


# --- Drinks ------------------------------------------------------------------


def glass_material(tint=(0.93, 0.97, 0.94)):
    s = Shader("glass", color=tint, rough=0.02, transmission=1.0, ior=1.52)
    # Pressed glass is never quite flat: a faint ripple.
    s.bump(s.noise(90.0, 2.0, 0.5), 0.08, 0.0006)
    return s


def tumbler(name, h, r_top, r_bot, base, wall, fill, liquid, facets=0):
    """A thick, plain pulpería tumbler of ``h`` metres, ``fill`` of it (above
    the heavy base) holding ``liquid``; ``facets`` flat sides, or round."""
    seg = facets or 72

    def r_out(z):
        return r_bot + (r_top - r_bot) * z / h

    def r_in(z):
        return r_out(z) - wall

    outer = [(0.0, 0.0025), (r_bot - 0.006, 0.0), (r_bot - 0.0012, 0.0003), (r_bot, 0.0028)]
    outer += [(r_out(h * k / 10.0), h * k / 10.0) for k in range(1, 10)]
    outer += [(r_out(h) - 0.0003, h - 0.0008), (r_out(h) - wall * 0.5, h), (r_in(h) + 0.0003, h - 0.0008)]
    inner = [(r_in(z), z) for z in (h - 0.004, h * 0.7, h * 0.45, base + 0.006)]
    inner += [(r_in(base) - 0.0035, base + 0.0008), (0.0, base)]
    g = lathe(outer + inner, seg)
    glass = g.object(name, glass_material(), smooth=not facets)
    # The liquid: just inside the glass, its top drawn up at the wall.
    top = base + (h - base) * fill
    gap = 0.00025
    prof = [(0.0, base + gap), (r_in(base) - 0.0036, base + 0.0009 + gap)]
    prof += [(r_in(z) - gap, z) for z in (base + 0.006, (base + top) / 2.0, top - 0.0006)]
    prof += [(r_in(top) - gap, top + 0.0004), (r_in(top) - 0.002, top), (0.0, top - 0.0002)]
    lathe(prof, seg).object(name + "_liquid", liquid, smooth=not facets)
    return glass


def spirit(name, color, density, tint=(1.0, 1.0, 1.0)):
    s = Shader(name, color=tint, rough=0.0, transmission=1.0, ior=1.36)
    return s.volume(color, density)


def cane_glass(args):
    bpy.context.scene.cycles.film_transparent_glass = True
    tumbler("cane_glass", 0.095, 0.036, 0.031, 0.013, 0.0032, 0.62, spirit("cane", (0.93, 0.62, 0.22), 26.0))


def gin_glass(args):
    """A smaller, faceted glass of clear ginebra, the glass a little green."""
    bpy.context.scene.cycles.film_transparent_glass = True
    g = tumbler("gin_glass", 0.072, 0.029, 0.025, 0.015, 0.003, 0.6, spirit("gin", (0.92, 0.97, 1.0), 4.0), facets=10)
    g.data.materials[0] = glass_material((0.86, 0.96, 0.90)).mat


def wine_glass(args):
    bpy.context.scene.cycles.film_transparent_glass = True
    tumbler("wine_glass", 0.095, 0.036, 0.031, 0.013, 0.0032, 0.72, spirit("wine", (0.70, 0.04, 0.10), 55.0))


# --- Tobacco, matches -------------------------------------------------------


def tobacco_material():
    s = Shader("tobacco", rough=0.75, sheen=0.3)
    fib = s.coord((1.0, 1.0, 1.0))
    s.set(color=s.ramp(s.noise(900.0, 3.0, 0.6, vec=fib, distortion=2.0), [(0.25, (0.16, 0.08, 0.025)), (0.5, (0.32, 0.17, 0.05)), (0.75, (0.52, 0.31, 0.09))]))
    s.bump(s.wave(350.0, vec=fib, direction="X", distortion=18.0, detail=4.0), 0.6, 0.0006)
    return s


def tobacco(args):
    """A soft leather tabaquera, open, cut tobacco heaped in its mouth and
    spilt in front, and a booklet of rolling papers beside it."""
    exposure(-0.5)
    rng = random.Random(7)
    leather = Shader("pouch", rough=0.6, sheen=0.15)
    hide = leather.image(texture_path(args, "brown_leather", "albedo"), 9.0)
    worn = leather.ramp(leather.noise(40.0, 5.0), [(0.35, (0.22, 0.15, 0.10)), (0.7, (0.42, 0.32, 0.24))])
    leather.set(color=leather.mix(1.0, hide, worn, "MULTIPLY"))
    leather.normal_map(texture_path(args, "brown_leather", "nor_gl"), 9.0, 1.0)
    leather.bump(leather.noise(45.0, 4.0, 0.5, distortion=1.0), 0.3, 0.0015)
    # The pouch: an egg squashed on the ground, its top cut open.
    g = ellipsoid((1.0, 1.0, 1.0), 48, 24)
    for v in g.verts:
        v.x *= 0.062
        v.y *= 0.046
        v.z = v.z * (0.012 if v.z < 0 else 0.034) + 0.012
    g.faces = [f for f in g.faces if min(g.verts[i].z for i in f) < 0.012 + 0.034 * 0.72]
    roughen(g, 0.006, 28.0, 1.0)
    # Soft leather creases round the mouth, where it was gathered.
    for v in g.verts:
        a = math.atan2(v.y, v.x)
        k = max(0.0, (v.z - 0.02) / 0.022)
        f = 1.0 + 0.07 * k * math.sin(11.0 * a + 2.0 * math.sin(3.0 * a))
        v.x *= f
        v.y *= f
    pouch = g.object("pouch", leather)
    modifier(pouch, "SOLIDIFY", thickness=0.0016, offset=-1.0)
    subsurf(pouch, 1)
    # The mouth's rim and its thong, tied, an end hanging over the front.
    rim_z = 0.012 + 0.034 * 0.72
    rim_r = math.sqrt(1.0 - 0.72**2)
    ring = [V((0.062 * rim_r * 1.03 * math.cos(a), 0.046 * rim_r * 1.03 * math.sin(a), rim_z - 0.002)) for a in (2 * math.pi * k / 40 for k in range(41))]
    thong = mottled("thong", (0.20, 0.10, 0.05), 0.5, 0.2)
    sweep(spline(ring, 2), circle(0.0013, 8)).object("thong", thong)
    # The tobacco: a heap in the mouth and loose shreds spilt in front.
    leaf = tobacco_material()
    heap = ellipsoid((0.040, 0.029, 0.017), 40, 16).moved(at(0.0, 0.0, rim_z - 0.002))
    roughen(heap, 0.005, 120.0, 3.0)
    roughen(heap, 0.0015, 600.0, 4.0)
    heap.object("heap", leaf)
    shreds = Geo()
    for _ in range(420):
        a = rng.uniform(0, 2 * math.pi)
        r = math.sqrt(rng.uniform(0.0, 1.0))
        p = V((0.042 * r * math.cos(a), 0.031 * r * math.sin(a), rim_z - 0.002 + 0.017 * math.sqrt(max(0.0, 1 - r * r)) + 0.001))
        if rng.random() < 0.4:
            # Spilt over the lip onto the ground in front.
            p = V((rng.uniform(-0.02, 0.035), rng.uniform(-0.062, -0.045), 0.0006))
        pts = [p]
        d = V((rng.uniform(-1, 1), rng.uniform(-1, 1), 0.0)).normalized()
        for _ in range(4):
            d = (d + V((rng.uniform(-0.6, 0.6), rng.uniform(-0.6, 0.6), rng.uniform(-0.3, 0.3)))).normalized()
            pts.append(pts[-1] + d * rng.uniform(0.002, 0.004))
        shreds.add(sweep(spline(pts, 3), circle(0.0006, 5, 0.45), shape=lambda t: (1.0 - 0.5 * t, 1.0)))
    shreds.object("shreds", leaf)
    # The papers: a little booklet with a printed cover, a leaf drawn out.
    cover = Shader("papers_cover", rough=0.55)
    cy = cover.axis(cover.coord(), "Y")
    cx = cover.axis(cover.coord(), "X")
    label = cover.math("MULTIPLY", cover.band(cx, -0.026, 0.026, 0.0004), cover.band(cy, -0.011, 0.011, 0.0004))
    cover.set(color=cover.mix(label, (0.62, 0.12, 0.07), (0.86, 0.76, 0.52)))
    book = box((0.068, 0.036, 0.0032), (0.0, 0.0, 0.0016))
    bo = book.object("papers", cover)
    bevel(bo, 0.0005, 2)
    place(bo, (0.055, -0.07, 0.0), (0.0, 0.0, 18.0))
    paper = mottled("paper", (0.93, 0.92, 0.88), 0.7, 0.04, sss=0.2, sss_radius=(0.01, 0.01, 0.01))
    leafp = grid(10, 4, 0.05, 0.034)
    for v in leafp.verts:
        v.z = 0.0034 + 0.004 * max(0.0, v.x + 0.01) / 0.035
    lo = leafp.object("paper_leaf", paper)
    place(lo, (0.075, -0.068, 0.0), (0.0, 0.0, 18.0))
    modifier(lo, "SOLIDIFY", thickness=0.0002)


def matches(args):
    """A small card matchbox, its drawer pushed half out, the matches in it,
    and two loose in front, one struck."""
    rng = random.Random(3)
    L, W, H, t = 0.056, 0.037, 0.016, 0.0007
    card = mottled("card", (0.78, 0.70, 0.54), 0.8, 0.08)
    strike = mottled("strike", (0.22, 0.12, 0.08), 0.95, 0.3, 300.0, 0.6, 900.0, 0.0002)
    sleeve = Geo().add(box((L, W, t), (0, 0, H - t / 2))).add(box((L, W, t), (0, 0, t / 2)))
    sleeve.object("sleeve", card, smooth=False)
    box((L, t, H - 2 * t), (0, W / 2 - t / 2, H / 2)).object("side_back", strike, smooth=False)
    box((L, t, H - 2 * t), (0, -W / 2 + t / 2, H / 2)).object("side_front", strike, smooth=False)
    # The label: blue paper, a cream panel, a red medallion with a star.
    blue = mottled("label_blue", (0.08, 0.15, 0.38), 0.6, 0.12)
    cream = mottled("label_cream", (0.88, 0.80, 0.60), 0.6, 0.06)
    red = mottled("label_red", (0.62, 0.08, 0.05), 0.55, 0.1)
    box((L - 0.003, W - 0.003, 0.0002), (0, 0, H + 0.0001)).object("label", blue, smooth=False)
    box((L - 0.011, W - 0.011, 0.0002), (0, 0, H + 0.0003)).object("panel", cream, smooth=False)
    lathe([(0.0, H + 0.0004), (0.0075, H + 0.0004), (0.0075, H + 0.0006), (0.0, H + 0.0006)], 32).moved(mathutils.Matrix.Diagonal((1.3, 1.0, 1.0, 1.0))).object("medallion", red)
    star = Geo()
    pts = [V((0.0045 * (1 if k % 2 == 0 else 0.42) * math.cos(math.pi * k / 5 + math.pi / 2), 0.0045 * (1 if k % 2 == 0 else 0.42) * math.sin(math.pi * k / 5 + math.pi / 2), H + 0.0007)) for k in range(10)]
    star.verts = pts
    star.faces = [tuple(range(10))]
    star.object("star", mottled("star", (0.92, 0.78, 0.30), 0.4, 0.05), smooth=False)
    # The drawer, out towards +X, and its matches.
    out = 0.027
    drawer = Geo().add(box((L - 0.002, W - 0.003, t), (out, 0, t * 1.5)))
    for x in (-1, 1):
        drawer.add(box((t, W - 0.003, H - 0.004), (out + x * (L / 2 - 0.001 - t / 2), 0, (H - 0.004) / 2 + t)))
    for y in (-1, 1):
        drawer.add(box((L - 0.002, t, H - 0.004), (out, y * ((W - 0.003) / 2 - t / 2), (H - 0.004) / 2 + t)))
    drawer.object("drawer", card, smooth=False)
    wood = mottled("match_wood", (0.86, 0.72, 0.50), 0.7, 0.12, 400.0, 0.2, 2000.0, 0.0001)
    head = mottled("match_head", (0.42, 0.07, 0.04), 0.5, 0.25, 900.0, 0.5, 3000.0, 0.0002)
    sticks, heads = Geo(), Geo()
    for layer in range(3):
        for k in range(8):
            y = -0.0145 + k * 0.0041 + rng.uniform(-0.0006, 0.0006)
            z = 0.0026 + layer * 0.0023
            x0 = out + rng.uniform(-0.002, 0.002)
            yaw = turn(rng.uniform(-3, 3))
            sticks.add(box((0.046, 0.0021, 0.0021)).moved(at(x0, y, z) @ yaw))
            heads.add(ellipsoid((0.0026, 0.0016, 0.0015), 12, 6).moved(at(x0, y, z) @ yaw @ at(0.0235, 0, 0)))
    # Two loose: one whole, one struck, its head and half its stick burnt.
    for x, y, a in ((0.02, -0.045, 12.0), (-0.01, -0.035, -25.0)):
        m = at(x, y, 0.00105) @ turn(a)
        sticks.add(box((0.046, 0.0021, 0.0021)).moved(m))
        if a > 0:
            heads.add(ellipsoid((0.0026, 0.0016, 0.0015), 12, 6).moved(m @ at(0.0235, 0, 0)))
            continue
        char = Shader("char", color=(0.025, 0.02, 0.018), rough=0.9)
        char.bump(char.noise(3000.0, 4.0), 0.8, 0.0002)
        burnt = Geo().add(box((0.018, 0.0017, 0.0017)).moved(m @ at(0.032, 0, 0) @ turn(-8, "Y")))
        burnt.add(roughen(ellipsoid((0.0022, 0.0013, 0.0012), 12, 6), 0.0004, 1800.0).moved(m @ at(0.0415, 0, 0.0013)))
        burnt.object("burnt", char)
    sticks.object("sticks", wood, smooth=False)
    heads.object("heads", head)


# --- Food -------------------------------------------------------------------


def biscuit(args):
    """A galleta de campo: a round, domed hard bread, its layered sides and
    its top scored in a cross and pricked, the crust golden to brown."""
    exposure(-0.6)
    g = ellipsoid((0.052, 0.052, 1.0), 96, 64)
    for v in g.verts:
        r = math.hypot(v.x, v.y) / 0.052
        if v.z < 0:
            v.z = -0.004 * (1.0 - r**4)
        else:
            v.z = 0.031 * v.z**0.75
        v.z += 0.004
        # The laminated dough shows in rings round the side.
        side = max(0.0, 1.0 - v.z / 0.026)
        f = 1.0 + 0.012 * side * math.sin(v.z * 1100.0 + 1.5 * mathutils.noise.noise(v * 60.0))
        v.x *= f
        v.y *= f
    roughen(g, 0.0018, 40.0, 2.0)
    bread = g.object("galleta", None)
    subsurf(bread, 1)
    for a in (35.0, 125.0):
        c = box((0.07, 0.003, 0.016)).object("score", None)
        place(c, (0.0, 0.0, 0.039), (0.0, 0.0, a))
        cut(bread, c)
    s = Shader("crust", rough=0.62, sss=0.05, sss_radius=(0.004, 0.003, 0.002))
    z = s.axis(s.coord(), "Z")
    # Darker on the dome where it baked most, pale in the layered sides.
    height = s.math("DIVIDE", z, 0.036)
    tone = s.math("ADD", s.math("MULTIPLY", height, 0.8), s.math("MULTIPLY", s.noise(90.0, 6.0, 0.6), 0.35))
    layers = s.wave(160.0, s.coord(), "Z", distortion=4.0, detail=3.0)
    tone = s.math("ADD", tone, s.math("MULTIPLY", layers, s.math("SUBTRACT", 0.55, height, clamp=True)))
    s.set(color=s.ramp(tone, [(0.2, (0.62, 0.45, 0.24)), (0.55, (0.50, 0.30, 0.11)), (0.85, (0.36, 0.19, 0.06)), (1.0, (0.24, 0.12, 0.04))]))
    pricks = s.math("LESS_THAN", s.voronoi(140.0, s.coord(), randomness=0.6), 0.08)
    s.bump(s.math("ADD", s.noise(220.0, 8.0, 0.65), s.math("MULTIPLY", pricks, -0.8)), 0.5, 0.0008)
    s.bump(layers, 0.25, 0.0005)
    # Blisters raised by the oven.
    s.bump(s.math("SUBTRACT", 1.0, s.voronoi(45.0, s.coord(), randomness=0.9)), 0.35, 0.0012)
    bread.data.materials.append(s.mat)


def cheese(args):
    """A wedge cut from a round country cheese: pale yellow paste with a few
    eyes, an ochre rind on the round and the faces, a thin rind line at the
    cut."""
    exposure(-0.6)
    R, H, span = 0.11, 0.075, math.radians(52.0)
    n = 28
    g = Geo()
    arc = [(R * math.cos(-span / 2 + span * k / n), R * math.sin(-span / 2 + span * k / n)) for k in range(n + 1)]
    for z in (0.0, H):
        g.verts.append(V((0.0, 0.0, z)))
        g.verts += [V((x, y, z)) for x, y in arc]
    m = n + 2
    g.faces.append(tuple(range(m)))
    g.faces.append(tuple(reversed(range(m, 2 * m))))
    for j in range(m):
        j2 = (j + 1) % m
        g.faces.append((j, m + j, m + j2, j2))
    spin = turn(-150.0)
    w = g.moved(spin).object("cheese", None)
    bevel(w, 0.004, 4, 30.0)
    s = Shader("cheese", rough=0.55, sss=0.35, sss_radius=(0.012, 0.008, 0.003), sss_scale=0.02)
    p = s.coord()
    r = s.math("SQRT", s.math("ADD", s.math("POWER", s.axis(p, "X"), 2.0), s.math("POWER", s.axis(p, "Y"), 2.0)))
    z = s.axis(p, "Z")
    skin = s.math("MAXIMUM", s.math("GREATER_THAN", r, R - 0.0032), s.math("MAXIMUM", s.math("LESS_THAN", z, 0.0028), s.math("GREATER_THAN", z, H - 0.0028)))
    paste = s.ramp(s.noise(70.0, 4.0), [(0.3, (0.90, 0.64, 0.15)), (0.7, (0.95, 0.72, 0.22))])
    rind = s.ramp(s.noise(45.0, 6.0, 0.65), [(0.25, (0.30, 0.15, 0.04)), (0.55, (0.46, 0.26, 0.06)), (0.85, (0.38, 0.24, 0.08))])
    # The eyes: small round holes in the paste, darker inside.
    cell = s.voronoi(48.0, p, randomness=1.0)
    size = s.math("ADD", 0.06, s.math("MULTIPLY", s.noise(9.0, 1.0), 0.16))
    eye = s.math("MULTIPLY", s.math("LESS_THAN", cell, size), s.math("SUBTRACT", 1.0, skin))
    paste = s.mix(s.math("MULTIPLY", eye, 0.55), paste, (0.40, 0.25, 0.05))
    s.set(color=s.mix(skin, paste, rind))
    s.set(rough=s.mix(skin, (0.45, 0.45, 0.45), (0.8, 0.8, 0.8)))
    s.bump(s.math("ADD", s.noise(300.0, 6.0), s.math("MULTIPLY", skin, s.noise(60.0, 5.0))), 0.35, 0.0008)
    s.bump(s.math("MULTIPLY", eye, -1.0), 1.0, 0.002)
    w.data.materials.append(s.mat)


# --- Sacks and bags ---------------------------------------------------------


def kraft(name, color, var=0.12):
    """Paper: mottled, fibrous, crumpled."""
    s = Shader(name, rough=0.82, sss=0.05, sss_radius=(0.003, 0.003, 0.003))
    s.set(color=s.ramp(s.noise(80.0, 6.0, 0.6), [(0.3, tuple(c * (1 - var) for c in color)), (0.7, tuple(c * (1 + var) for c in color))]))
    s.bump(s.noise(25.0, 5.0, 0.6, distortion=1.5), 0.35, 0.003)
    s.bump(s.noise(900.0, 4.0, 0.6), 0.25, 0.0002)
    return s


def bag(name, w, d, h, mat, bulge=0.04, seed=0.0, wrinkle=0.0012, steps=34, z0=0.0):
    """An open paper bag ``w`` by ``d``, ``h`` tall: a tube of rounded
    rectangle, bulging a little, crumpled."""
    path = [V((0.0, 0.0, z0 + h * k / steps)) for k in range(steps + 1)]
    prof = rounded_rect(w, d, 0.006, 4)
    g = sweep(path, prof, up=(1.0, 0.0, 0.0), shape=lambda t: (1.0 + bulge * math.sin(math.pi * t) ** 0.7,) * 2, caps=False)
    # Bottom.
    n = len(prof)
    g.faces.append(tuple(reversed(range(n))))
    roughen(g, wrinkle, 60.0, seed)
    o = g.object(name, mat)
    modifier(o, "SOLIDIFY", thickness=0.0006, offset=-1.0)
    return o


def plate(name, w, h, mat, target, offset=0.0004, segs=12):
    """A label, ``w`` by ``h``, upright facing -Y, wrapped onto ``target``."""
    g = grid(segs, segs, w, h).moved(turn(90.0, "X"))
    o = g.object(name, mat)
    modifier(o, "SHRINKWRAP", target=target, wrap_method="PROJECT", use_negative_direction=True, use_positive_direction=True, offset=offset, use_project_y=True)
    return o


def yerba(args):
    """A kilo of yerba in a kraft paper bag, its top rolled down, the green
    yerba with its pale sticks (palo) showing; a printed label."""
    exposure(-0.5)
    W, D, H = 0.105, 0.072, 0.17
    paper = kraft("kraft", (0.42, 0.29, 0.17))
    body = bag("yerba_bag", W, D, H, paper, 0.035, 1.0)
    # The rolled cuff.
    bag("cuff", W + 0.004, D + 0.004, 0.022, paper, 0.0, 2.0, 0.0008, 6, H - 0.02)
    roll = rounded_rect(W + 0.006, D + 0.006, 0.008, 4)
    ring = [V((u * 1.035, v * 1.035, H - 0.02)) for u, v in roll] + [V((roll[0][0] * 1.035, roll[0][1] * 1.035, H - 0.02))]
    sweep(ring, circle(0.0028, 10), up=(0, 0, 1)).object("roll", paper)
    # The yerba: a mound of fine ground leaf and sticks.
    g = grid(40, 28, W - 0.004, D - 0.004)
    for v in g.verts:
        fx, fy = 2 * v.x / W, 2 * v.y / D
        v.z = H - 0.012 + 0.016 * max(0.0, 1 - fx * fx) * max(0.0, 1 - fy * fy) + 0.002 * mathutils.noise.noise(v * 120.0)
    s = Shader("yerba", rough=0.9)
    grain = s.voronoi(900.0, s.coord(), "Distance")
    leaf = s.ramp(s.noise(500.0, 4.0), [(0.3, (0.10, 0.13, 0.03)), (0.55, (0.20, 0.25, 0.07)), (0.8, (0.30, 0.33, 0.11))])
    stick = s.math("GREATER_THAN", s.noise(250.0, 2.0, 0.5, vec=s.coord((1.0, 6.0, 1.0))), 0.66)
    s.set(color=s.mix(stick, leaf, (0.55, 0.47, 0.25)))
    s.bump(grain, 0.6, 0.0004)
    g.object("yerba", s)
    # The label: green border, cream field, a red band.
    green = mottled("label_green", (0.06, 0.16, 0.07), 0.6, 0.1)
    cream = mottled("label_cream", (0.78, 0.70, 0.50), 0.6, 0.08)
    red = mottled("label_red", (0.50, 0.06, 0.04), 0.55, 0.1)
    for name, w, h, z, mat, off in (("l_green", 0.082, 0.085, 0.075, green, 0.0004), ("l_cream", 0.072, 0.075, 0.075, cream, 0.0007), ("l_red", 0.072, 0.014, 0.088, red, 0.001)):
        o = plate(name, w, h, mat, body, off)
        place(o, (0.0, -0.06, z))


def sugar(args):
    """A kilo of sugar in a cucurucho of the almacén's blue paper, a cone
    rolled from a sheet, lying with the white sugar spilling from its
    mouth."""
    exposure(-0.4)
    rng = random.Random(9)
    L, R = 0.17, 0.05
    blue = kraft("blue_paper", (0.035, 0.06, 0.20), 0.2)
    prof = [(0.0, 0.0)] + [(R * k / 24, L * k / 24) for k in range(1, 25)]
    g = lathe(prof, 64)
    # The rolled sheet's edge: the outer layer stands proud along a spiral.
    for v in g.verts:
        a = math.atan2(v.y, v.x)
        f = ((a + math.pi) / (2 * math.pi) + v.z / L * 0.35) % 1.0
        v.x *= 1.0 + 0.03 * f
        v.y *= 1.0 + 0.03 * f
        # A ragged, slightly flared mouth.
        if v.z > L * 0.9:
            k = (v.z - L * 0.9) / (L * 0.1)
            v.x *= 1.0 + 0.08 * k
            v.y *= 1.0 + 0.08 * k
            v.z += 0.006 * k * mathutils.noise.noise(V((a * 3.0, 0.0, 0.0)))
    roughen(g, 0.0025, 35.0, 4.0)
    roughen(g, 0.0008, 140.0, 5.0)
    lie = turn(-14.0, "Y") @ turn(90.0, "Y")
    yaw = turn(8.0)
    cone = g.moved(yaw @ at(-L / 2, 0.0, 0.006) @ lie).object("cucurucho", blue)
    modifier(cone, "SOLIDIFY", thickness=0.0005, offset=1.0)
    snow = Shader("sugar", color=(0.80, 0.80, 0.78), rough=0.35, sss=0.4, sss_radius=(0.002, 0.002, 0.002))
    snow.bump(snow.voronoi(2500.0, snow.coord()), 0.8, 0.0002)
    # The sugar inside the mouth, and spilt in a heap before it.
    inside = roughen(ellipsoid((0.03, 0.044, 0.026), 32, 12), 0.002, 150.0, 2.0).moved(yaw @ at(L / 2 - 0.022, 0.0, 0.03))
    inside.object("inside", snow)
    mouth = yaw @ V((L / 2 + 0.035, 0.0, 0.0))
    heap = roughen(ellipsoid((0.045, 0.035, 0.012), 40, 12), 0.003, 120.0, 2.0).moved(at(mouth.x, mouth.y, 0.0) @ yaw)
    heap.verts = [V((v.x, v.y, max(0.0, v.z))) for v in heap.verts]
    heap.object("heap", snow)
    crystals = Geo()
    for _ in range(320):
        a = rng.uniform(0, 2 * math.pi)
        r = abs(rng.gauss(0.0, 0.03))
        x, y = mouth.x + r * math.cos(a) * 1.3, mouth.y + r * math.sin(a)
        z = max(0.0006, 0.012 * (1 - min(1.0, (r / 0.04) ** 2)) * 0.8)
        m = at(x, y, z) @ mathutils.Euler((rng.uniform(0, 6.3), rng.uniform(0, 6.3), rng.uniform(0, 6.3))).to_matrix().to_4x4()
        crystals.add(box((0.0012, 0.0012, 0.0012)).moved(m))
    crystals.object("crystals", Shader("crystal", color=(0.97, 0.97, 0.97), rough=0.05, transmission=0.85, ior=1.55), smooth=False)


def flour(args):
    """A kilo of flour in a white cotton sack, gathered and tied at the neck
    with twine, faded blue stripes printed on it, dusted with flour."""
    exposure(-0.5)
    prof = [(0.0, 0.0), (0.05, 0.0), (0.066, 0.006), (0.074, 0.03), (0.075, 0.07), (0.07, 0.11), (0.055, 0.145), (0.034, 0.17), (0.022, 0.185), (0.024, 0.192), (0.034, 0.205), (0.042, 0.222), (0.044, 0.23)]
    g = lathe(prof, 72)
    g.faces = [f for f in g.faces if not all(g.verts[i].z > 0.229 for i in f)]
    for v in g.verts:
        a = math.atan2(v.y, v.x)
        z = v.z
        gather = max(0.0, min(1.0, (z - 0.12) / 0.06)) if z < 0.19 else 1.0
        pleat = 1.0 + 0.12 * gather * math.sin(13.0 * a + 1.5 * math.sin(4.0 * a + z * 40.0))
        sag = 1.0 + 0.04 * max(0.0, 1.0 - z / 0.06)
        v.x *= pleat * sag
        v.y *= pleat * sag
    roughen(g, 0.004, 30.0, 8.0)
    s = Shader("cotton", rough=0.85, sheen=0.5, sheen_rough=0.6)
    linen = s.image(texture_path(args, "rough_linen", "diff"), 6.0)
    cloth = s.mix(0.75, linen, (0.78, 0.75, 0.68))
    z = s.axis(s.coord(), "Z")
    stripes = s.math("MAXIMUM", s.band(z, 0.062, 0.07, 0.0008), s.band(z, 0.078, 0.086, 0.0008))
    stripes = s.math("MULTIPLY", stripes, s.ramp(s.noise(80.0, 5.0), [(0.3, (0.55, 0.55, 0.55)), (0.7, (0.95, 0.95, 0.95))]))
    cloth = s.mix(stripes, cloth, (0.10, 0.16, 0.36))
    dusted = s.math("MULTIPLY", s.math("SUBTRACT", 1.0, s.math("DIVIDE", z, 0.05), clamp=True), s.noise(40.0, 4.0))
    s.set(color=s.mix(dusted, cloth, (0.80, 0.78, 0.74)))
    s.normal_map(texture_path(args, "rough_linen", "nor_gl"), 6.0, 1.2)
    sack = g.object("sack", s)
    modifier(sack, "SOLIDIFY", thickness=0.001, offset=-1.0)
    subsurf(sack, 1)
    twine(V((0.0, 0.0, 0.186)), 0.0255, 0.0255, 3, 0.0012, "Z", tails=True)
    # Flour spilt round the foot of the sack.
    dust = roughen(ellipsoid((0.035, 0.025, 0.004), 32, 8), 0.006, 70.0, 3.0)
    dust.verts = [V((v.x, v.y, max(0.0, v.z))) for v in dust.verts]
    fl = Shader("flour", color=(0.70, 0.68, 0.64), rough=0.95)
    fl.bump(fl.noise(400.0, 6.0), 0.5, 0.0003)
    dust.moved(at(0.03, -0.075, 0.0)).object("flour", fl)


def twine_material():
    s = Shader("twine", rough=0.85, sheen=0.4)
    s.set(color=s.ramp(s.noise(300.0, 4.0), [(0.3, (0.38, 0.28, 0.15)), (0.7, (0.55, 0.42, 0.24))]))
    s.bump(s.wave(2600.0, s.coord(), "DIAGONAL", distortion=2.0), 0.6, 0.0002)
    return s


def twine(center, ru, rv, turns, r, axis="Z", tails=False, pitch=0.0022):
    """Twine wound ``turns`` times round ``axis`` through ``center`` (radii
    ``ru``, ``rv`` of the wound bundle), and tied off with two hanging ends."""
    pts = []
    n = 48 * turns
    for k in range(n + 1):
        a = 2 * math.pi * k / 48
        off = (k / n - 0.5) * pitch * turns
        u, v = ru * math.cos(a), rv * math.sin(a)
        if axis == "Z":
            p = V((u, v, off))
        elif axis == "X":
            p = V((off, u, v))
        else:
            p = V((u, off, v))
        pts.append(center + p)
    mat = twine_material()
    sweep(pts, circle(r, 8), up=(0, 0, 1) if axis != "Z" else (1, 0, 0)).object("twine", mat)
    knot_at = pts[(36 if axis == "Z" else 12) + 48 * (turns - 1)]
    knot = ellipsoid((r * 2.6, r * 2.6, r * 2.2), 12, 8).moved(at(*knot_at))
    knot.object("knot", mat)
    if tails:
        for k, dz in ((0, -0.03), (1, -0.022)):
            out = (knot_at - center)
            out.z = 0.0
            out.normalize()
            side = V((-out.y, out.x, 0.0)) * (0.006 if k else -0.006)
            ends = spline([knot_at, knot_at + out * 0.006 + side, knot_at + out * 0.012 + side * 1.5 + V((0, 0, dz * 0.5)), knot_at + out * 0.016 + side * 2 + V((0, 0, dz))], 6)
            sweep(ends, circle(r, 8)).object("tail", mat)
    return mat


def jerky(args):
    """A kilo of charqui: dark dried strips of beef, stiff and twisted, in a
    bundle tied round the middle with twine."""
    exposure(-0.4)
    rng = random.Random(11)
    s = Shader("charqui", rough=0.4, sss=0.05, sss_radius=(0.003, 0.0008, 0.0004))
    fibre = s.wave(500.0, s.coord(), "X", distortion=8.0, detail=4.0)
    meat = s.ramp(s.noise(120.0, 6.0, 0.6), [(0.25, (0.03, 0.012, 0.006)), (0.5, (0.08, 0.028, 0.012)), (0.75, (0.16, 0.06, 0.025))])
    fat = s.math("GREATER_THAN", s.noise(60.0, 3.0, 0.5, vec=s.coord((0.25, 1.0, 1.0))), 0.66)
    meat = s.mix(s.math("MULTIPLY", fat, 0.4), meat, (0.30, 0.18, 0.09))
    salt = s.math("GREATER_THAN", s.noise(45.0, 5.0, 0.7), 0.64)
    s.set(color=s.mix(s.math("MULTIPLY", salt, 0.3), meat, (0.62, 0.58, 0.52)))
    s.bump(fibre, 0.5, 0.0006)
    s.bump(s.noise(200.0, 6.0, 0.7), 0.5, 0.0008)
    strips = Geo()
    for k in range(7):
        y = -0.024 + k * 0.008 + rng.uniform(-0.003, 0.003)
        z = 0.004 + (k % 3) * 0.0045 + rng.uniform(0, 0.002)
        L = rng.uniform(0.17, 0.21)
        x0 = rng.uniform(-0.015, 0.015)
        yaw = rng.uniform(-6.0, 6.0)
        ph = rng.uniform(0, 6.0)
        pts = []
        for j in range(25):
            t = j / 24
            x = x0 + (t - 0.5) * L
            pts.append(V((x, y + 0.004 * math.sin(t * 7 + ph) + math.tan(math.radians(yaw)) * x, z + 0.004 * math.sin(t * 5 + ph * 2) + 0.012 * abs(t - 0.5) ** 2)))
        w = rng.uniform(0.016, 0.03)
        end = lambda t: min(1.0, 0.35 + 3.0 * min(t, 1 - t))
        g = sweep(pts, rounded_rect(w, 0.0026, 0.0012, 3), up=(0, 0, 1), shape=lambda t, s0=rng.uniform(0, 6), end=end: (end(t) * (0.8 + 0.25 * math.sin(t * 9 + s0) + 0.1 * math.sin(t * 31 + s0)), 0.75 + 0.35 * math.sin(t * 13 + s0)), twist=lambda t, tw=rng.uniform(-0.9, 0.9): tw * math.sin(math.pi * t * 1.5))
        roughen(g, 0.0022, 70.0, k)
        roughen(g, 0.0007, 300.0, k + 20)
        strips.add(g)
    strips.object("charqui", s)
    twine(V((0.0, 0.0, 0.0095)), 0.034, 0.0125, 3, 0.0011, "X", tails=True)


# --- Rawhide, clothing -------------------------------------------------------


def rawhide(name, color, var=0.25):
    """Cuero crudo: hairless, dried hide, a little translucent, polished
    where hands rubbed it, darker where they soiled it."""
    s = Shader(name, rough=0.45, sss=0.15, sss_radius=(0.004, 0.002, 0.001), coat=0.2, coat_rough=0.35)
    lo = tuple(c * (1 - var) for c in color)
    hi = tuple(min(1.0, c * (1 + var)) for c in color)
    soil = s.noise(30.0, 5.0, 0.6)
    s.set(color=s.ramp(s.math("ADD", soil, s.math("MULTIPLY", s.noise(400.0, 3.0), 0.2)), [(0.3, lo), (0.55, color), (0.8, hi)]))
    s.bump(s.noise(350.0, 6.0, 0.7), 0.3, 0.0004)
    return s


def coil_path(loops, a, b, step, seed, wobble=0.003):
    """A coil hung from its top (the origin): ``loops`` teardrop loops in
    the XZ plane, ``a`` wide and ``2 b`` tall, each a little off the last
    and ``step`` further back (+Y)."""
    rng = random.Random(seed)
    pts = []
    for i in range(loops):
        ai = a * rng.uniform(0.9, 1.1)
        bi = b * rng.uniform(0.92, 1.06)
        tilt = math.radians(rng.uniform(-6.0, 6.0))
        for k in range(48):
            t = 2 * math.pi * k / 48
            x = ai * math.sin(t) * (0.32 + 0.68 * (1 - math.cos(t)) / 2)
            z = -bi * (1 - math.cos(t))
            x, z = x * math.cos(tilt) - z * math.sin(tilt), x * math.sin(tilt) + z * math.cos(tilt)
            y = (i + k / 48) * step
            p = V((x, y, z))
            p += mathutils.noise.noise_vector(p * 25.0 + V((seed, 0, 0))) * wobble
            pts.append(p)
    return pts


def binding(pts_center, ry, rz, turns, profile, mat, name="binding"):
    """Wraps round the coil's bundle at its top, about the X axis."""
    path = []
    n = 32 * turns
    for k in range(n + 1):
        a = 2 * math.pi * k / 32
        path.append(pts_center + V(((k / n - 0.5) * 0.024, ry * math.cos(a), rz * math.sin(a))))
    return sweep(path, profile, up=(1, 0, 0)).object(name, mat)


def maneador(args):
    """A maneador: a long flat strap of rawhide, coiled and hung, bound at
    the top, its end hanging free."""
    exposure(-0.4)
    hide = rawhide("rawhide", (0.48, 0.36, 0.20))
    step = 0.005
    pts = coil_path(6, 0.085, 0.11, step, 3)
    tail = spline([pts[-1], pts[-1] + V((0.02, 0.0, -0.03)), V((0.05, 6 * step + 0.004, -0.17)), V((0.06, 6 * step + 0.008, -0.24))], 10)
    strap = rounded_rect(0.0026, 0.016, 0.0009, 3)
    g = sweep(spline(pts, 2) + tail[1:], strap, up=(0, 1, 0), twist=lambda t: 0.25 * math.sin(t * 40.0))
    roughen(g, 0.0008, 80.0, 1.0)
    g.object("maneador", hide)
    binding(V((0.0, 3 * step, 0.002)), 3 * step + 0.008, 0.010, 4, rounded_rect(0.002, 0.009, 0.0008, 3), hide)


def lasso(args):
    """A lazo: a long round rope braided of four rawhide tientos, coiled and
    hung, bound at the top, its iron argolla hanging at the end."""
    exposure(-0.4)
    hide = rawhide("lazo", (0.40, 0.26, 0.13), 0.3)
    step = 0.0095
    loops = 6
    centre = spline(coil_path(loops, 0.10, 0.14, step, 8), 2)
    end = centre[-1]
    tail = spline([end, end + V((0.02, 0.0, -0.03)), V((0.07, loops * step + 0.006, -0.20)), V((0.075, loops * step + 0.01, -0.27))], 12)
    path = centre + tail[1:]
    # Resample evenly so the braid's pitch is even.
    even = [path[0]]
    for p in path[1:]:
        while (p - even[-1]).length > 0.0012:
            even.append(even[-1] + (p - even[-1]).normalized() * 0.0012)
    ts, ns = frames(even, (0, 1, 0))
    r, pitch = 0.0026, 0.011
    strands = Geo()
    for phase, hand in ((0.0, 1), (math.pi, 1), (math.pi / 2, -1), (3 * math.pi / 2, -1)):
        pts = []
        s_len = 0.0
        for i, (p, t, nv) in enumerate(zip(even, ts, ns)):
            if i:
                s_len += (p - even[i - 1]).length
            a = phase + hand * 2 * math.pi * s_len / pitch
            bv = t.cross(nv)
            # Over and under: each strand rises as it crosses the others.
            lift = 1.0 + 0.18 * math.cos(2 * a)
            pts.append(p + (nv * math.cos(a) + bv * math.sin(a)) * r * lift)
        strands.add(sweep(pts, circle(0.0019, 6, 0.6), up=tuple(ns[0])))
    strands.add(sweep(even, circle(0.0028, 8)))
    strands.object("lazo", hide)
    binding(V((0.0, loops * step / 2, 0.002)), loops * step / 2 + 0.01, 0.012, 4, circle(0.0016, 6, 0.6), hide)
    # The argolla, its rope end turned through it and lashed.
    iron = Shader("iron", color=(0.16, 0.15, 0.14), metal=1.0, rough=0.5)
    rust = iron.math("GREATER_THAN", iron.noise(80.0, 6.0, 0.6), 0.6)
    iron.set(color=iron.mix(rust, (0.16, 0.15, 0.14), (0.24, 0.10, 0.04)), metal=iron.math("SUBTRACT", 1.0, rust), rough=iron.mix(rust, (0.4, 0.4, 0.4), (0.9, 0.9, 0.9)))
    iron.bump(iron.noise(500.0, 5.0), 0.3, 0.0003)
    tip = tail[-1]
    ring = lathe([(0.026 + 0.0045 * math.cos(2 * math.pi * k / 16), 0.0045 * math.sin(2 * math.pi * k / 16)) for k in range(16)], 48)
    ring.moved(at(tip.x + 0.004, tip.y, tip.z - 0.024) @ turn(90.0, "X") @ turn(10.0, "Y")).object("argolla", iron)


def alpargata(name, canvas, jute, trim, matrix):
    """One alpargata: a jute sole of coiled braid and a canvas upper over a
    straight last, the toe covered, the heel held by a low counter."""
    L, Wd = 0.26, 0.094

    def half_width(u):
        # Narrow heel, broad tread, round toe.
        return Wd / 2 * (0.72 + 0.28 * math.sin(math.pi * min(1.0, u / 0.72) / 2))

    def height(u):
        return 0.03 + 0.042 * math.exp(-((u - 0.42) / 0.33) ** 2) if u > 0.3 else 0.068 - 0.03 * u

    # The sole: an outline, pushed up.
    outline = []
    for k in range(64):
        a = 2 * math.pi * k / 64
        u = (1 - math.cos(a)) / 2
        outline.append((-L / 2 + u * L, math.sin(a) * half_width(u)))
    T = 0.022
    sole = Geo()
    n = len(outline)
    for z in (0.0, T * 0.5, T):
        for x, y in outline:
            sole.verts.append(V((x * (1.0 + 0.012 * (z == T * 0.5)), y * (1.0 + 0.04 * (z == T * 0.5)), z)))
    sole.faces.append(tuple(reversed(range(n))))
    sole.faces.append(tuple(range(2 * n, 3 * n)))
    for ring in range(2):
        for j in range(n):
            j2 = (j + 1) % n
            a, b = ring * n, (ring + 1) * n
            sole.faces.append((a + j, a + j2, b + j2, b + j))
    so = sole.moved(matrix).object(name + "_sole", jute)
    subsurf(so, 2)
    # The upper: a shell over the last, cut open over the heel.
    upper = Geo()
    nu, nv = 60, 36
    idx = {}
    for i in range(nu + 1):
        u = i / nu
        x = -L / 2 + (1 - math.cos(math.pi * u)) / 2 * L
        uu = (x + L / 2) / L
        for j in range(nv + 1):
            v = -1.0 + 2.0 * j / nv
            hw = half_width(uu) * math.sqrt(max(0.0, 1 - (2 * uu - 1) ** 8)) * 1.01
            if uu < 0.02 or uu > 0.98:
                hw = half_width(uu) * math.sqrt(max(0.0, 1 - (2 * uu - 1) ** 2)) * 1.01
            zf = (1 - abs(v) ** 3) ** (1 / 3)
            idx[i, j] = len(upper.verts)
            upper.verts.append(V((x, v * hw, T * 0.9 + height(uu) * zf)))
    for i in range(nu):
        uu = (i + 0.5) / nu
        x = -L / 2 + (1 - math.cos(math.pi * uu)) / 2 * L
        uu = (x + L / 2) / L
        for j in range(nv):
            v = -1.0 + 2.0 * (j + 0.5) / nv
            zf = (1 - abs(v) ** 3) ** (1 / 3)
            if uu < 0.4 and zf > 0.62:
                continue
            upper.faces.append((idx[i, j], idx[i + 1, j], idx[i + 1, j + 1], idx[i, j + 1]))
    roughen(upper, 0.0012, 60.0, len(name))
    up = upper.moved(matrix).object(name + "_upper", canvas)
    modifier(up, "SOLIDIFY", thickness=0.0022, offset=1.0)
    subsurf(up, 1)
    # Piping round the opening.
    edge = []
    for i in range(nu + 1):
        u = i / nu
        x = -L / 2 + (1 - math.cos(math.pi * u)) / 2 * L
        uu = (x + L / 2) / L
        if uu > 0.4:
            break
        vv = (1 - 0.62**3) ** (1 / 3)
        hw = half_width(uu) * math.sqrt(max(0.0, 1 - (2 * uu - 1) ** 8)) * 1.01
        edge.append((x, hw * vv, T * 0.9 + height(uu) * 0.62 + 0.001))
    loop = [V((x, -y, z)) for x, y, z in reversed(edge)] + [V(e) for e in edge]
    front = V((edge[-1][0] + 0.004, 0.0, T * 0.9 + height(0.42) + 0.002))
    sweep(spline([V((edge[-1][0], -edge[-1][1], edge[-1][2]))] + [front] + [V(edge[-1])], 6), circle(0.0016, 6)).moved(matrix).object(name + "_vamp_edge", trim)
    sweep(spline(loop, 2), circle(0.0016, 6)).moved(matrix).object(name + "_rim", trim)


def alpargatas(args):
    """A pair of alpargatas criollas: black canvas uppers, braided jute
    soles."""
    exposure(-0.3)
    canvas = Shader("canvas", rough=0.9, sheen=0.12, sheen_rough=0.5, specular=0.25)
    weave = canvas.image(texture_path(args, "rough_linen", "diff"), 14.0)
    canvas.set(color=canvas.mix(1.0, weave, (0.045, 0.045, 0.055), "MULTIPLY"))
    canvas.normal_map(texture_path(args, "rough_linen", "nor_gl"), 14.0, 1.5)
    jute = Shader("jute", rough=0.9, sheen=0.3)
    braid = jute.wave(900.0, jute.coord(), "Z", distortion=0.0, profile="TRI")
    plait = jute.wave(1400.0, jute.coord((1.0, 1.0, 0.0)), "DIAGONAL", distortion=1.0)
    jute.set(color=jute.ramp(jute.math("ADD", jute.math("MULTIPLY", plait, 0.5), jute.math("MULTIPLY", jute.noise(200.0, 4.0), 0.5)), [(0.3, (0.38, 0.28, 0.15)), (0.7, (0.58, 0.46, 0.28))]))
    jute.bump(jute.math("ADD", braid, plait), 0.8, 0.0008)
    trim = mottled("trim", (0.025, 0.025, 0.03), 0.7, 0.2)
    alpargata("left", canvas, jute, trim, turn(-8.0) @ at(0.0, 0.055, 0.0))
    alpargata("right", canvas, jute, trim, at(0.035, -0.055, 0.0) @ turn(6.0))


def poncho(args):
    """A pampa poncho of dark wool, folded: a red and cream border with a
    stepped guarda along the ends, a fringe."""
    exposure(-0.5)
    s = Shader("wool", rough=0.9, sheen=0.12, sheen_rough=0.5, specular=0.25)
    p = s.coord()
    x = s.axis(p, "X")
    y = s.axis(p, "Y")
    ax = s.math("ABSOLUTE", x)
    base = s.ramp(s.noise(40.0, 5.0), [(0.3, (0.035, 0.025, 0.02)), (0.7, (0.07, 0.05, 0.04))])
    red = (0.36, 0.035, 0.03)
    cream = (0.62, 0.55, 0.42)
    col = base
    # Stripes, inwards from the folded ends: cream, red, guarda, red, cream.
    edge = 0.13
    col = s.mix(s.band(ax, edge - 0.004, edge - 0.001), col, cream)
    col = s.mix(s.band(ax, edge - 0.012, edge - 0.006), col, red)
    col = s.mix(s.band(ax, edge - 0.058, edge - 0.052), col, red)
    col = s.mix(s.band(ax, edge - 0.064, edge - 0.061), col, cream)
    # The guarda: a cream stepped zigzag on the dark wool.
    inband = s.band(ax, edge - 0.05, edge - 0.014)
    t = s.math("DIVIDE", s.math("SUBTRACT", ax, edge - 0.05), 0.036)
    # A stepped zigzag: a triangle wave across the band, in steps.
    tri = s.math("MULTIPLY", s.math("ABSOLUTE", s.math("SUBTRACT", s.math("FRACT", s.math("MULTIPLY", y, 16.0)), 0.5)), 2.0)
    stepped = s.math("ADD", s.math("DIVIDE", s.math("FLOOR", s.math("MULTIPLY", tri, 5.0)), 5.0), 0.1)
    line = s.math("LESS_THAN", s.math("ABSOLUTE", s.math("SUBTRACT", t, stepped)), 0.13)
    col = s.mix(s.math("MULTIPLY", inband, line), col, cream)
    s.set(color=col)
    s.normal_map(texture_path(args, "poly_wool_herringbone", "nor_gl"), 7.0, 1.0)
    s.bump(s.noise(120.0, 6.0, 0.7), 0.3, 0.0008)
    # The folded stack: three layers, their folds rounded, soft.
    for k, (w, d, z, dx) in enumerate(((0.27, 0.21, 0.0, 0.0), (0.265, 0.205, 0.022, 0.003), (0.262, 0.2, 0.044, -0.002))):
        nx = 40
        prof = rounded_rect(d, 0.024, 0.011, 6)
        path = [V((-w / 2 + w * i / nx + dx, 0.0, 0.0)) for i in range(nx + 1)]
        g = sweep(path, prof, up=(0, 1, 0))
        g = g.moved(at(0.0, 0.0, z + 0.012))
        roughen(g, 0.0025, 25.0, k)
        o = g.object(f"fold{k}", s)
        subsurf(o, 1)
    # The fringe along the front end.
    fringe = Geo()
    rng = random.Random(4)
    for i in range(70):
        y = -0.1 + 0.2 * i / 69
        x0 = 0.135 + 0.002
        pts = [V((x0, y, 0.06)), V((x0 + 0.012, y + rng.uniform(-0.002, 0.002), 0.05)), V((x0 + 0.018, y + rng.uniform(-0.004, 0.004), 0.03)), V((x0 + 0.02, y + rng.uniform(-0.006, 0.006), 0.012 + rng.uniform(0, 0.006)))]
        fringe.add(sweep(spline(pts, 4), circle(0.0011, 6)))
    fringe.object("fringe", s)


# --- Arms -------------------------------------------------------------------
# Guns are drawn muzzle towards -X, their right side towards +Y, then turned
# half round, so the right side (the ejector, the loading gate) faces the
# camera and the muzzle points right.


def rod(r, x0, x1, y=0.0, z=0.0, seg=32):
    """A round bar along X from ``x0`` to ``x1``."""
    g = lathe([(0.0, 0.0), (r * 0.9, 0.0), (r, r * 0.12), (r, x1 - x0 - r * 0.12), (r * 0.9, x1 - x0), (0.0, x1 - x0)], seg)
    return g.moved(at(x0, y, z) @ turn(90.0, "Y"))


def blued(name="blued"):
    """Blued steel, the bluing worn silver on the high spots."""
    s = Shader(name, metal=1.0, rough=0.3, coat=0.3, coat_rough=0.15)
    wear = s.math("GREATER_THAN", s.noise(120.0, 6.0, 0.6), 0.64)
    s.set(color=s.mix(wear, (0.07, 0.075, 0.09), (0.45, 0.45, 0.46)), rough=s.mix(wear, (0.3, 0.3, 0.3), (0.22, 0.22, 0.22)))
    s.bump(s.noise(2500.0, 3.0), 0.12, 0.0001)
    return s


def case_hardened():
    """Colour case hardening: mottled blues, purples and straw on grey."""
    s = Shader("case_hardened", metal=1.0, rough=0.25, coat=0.3, coat_rough=0.1)
    s.set(color=s.ramp(s.noise(90.0, 5.0, 0.65, distortion=2.0), [(0.3, (0.16, 0.18, 0.30)), (0.45, (0.30, 0.17, 0.20)), (0.55, (0.45, 0.36, 0.20)), (0.7, (0.38, 0.38, 0.40))]))
    return s


def walnut(name="walnut", color=(0.20, 0.09, 0.04)):
    s = Shader(name, rough=0.4, coat=0.35, coat_rough=0.25)
    grain = s.noise(1.0, 8.0, 0.6, vec=s.coord((25.0, 220.0, 220.0)), distortion=1.5)
    s.set(color=s.ramp(grain, [(0.3, tuple(c * 0.55 for c in color)), (0.55, color), (0.75, tuple(c * 1.5 for c in color))]))
    s.bump(grain, 0.2, 0.0004)
    s.bump(s.noise(300.0, 5.0), 0.15, 0.0003)
    return s


def part(g, name, mat, matrix, bevel_width=0.0, segments=3, smooth=True):
    o = g.moved(matrix).object(name, mat, smooth)
    if bevel_width:
        bevel(o, bevel_width, segments)
    return o


def revolver(args):
    """A Colt Single Action Army of 1873, 5 1/2 inch barrel: blued barrel
    and cylinder, case-hardened frame, walnut grips."""
    exposure(-0.3)
    M = turn(16.0) @ turn(180.0)
    steel, frame = blued(), case_hardened()
    wood = walnut()
    part(rod(0.0085, -0.170, -0.026), "barrel", steel, M)
    part(rod(0.0042, -0.150, -0.032, 0.0068, -0.0105, 16), "ejector", steel, M)
    part(ellipsoid((0.004, 0.003, 0.003), 12, 6).moved(at(-0.152, 0.0068, -0.0105)), "ejector_head", steel, M)
    part(box((0.007, 0.0018, 0.005), (-0.164, 0.0, 0.0095)), "front_sight", steel, M, 0.0006, 2)
    # The cylinder: six flutes between the chambers.
    cyl = lathe([(0.0, 0.0), (0.018, 0.0), (0.0205, 0.0025)] + [(0.0205, 0.0025 + 0.034 * k / 12) for k in range(1, 12)] + [(0.0205, 0.0365), (0.018, 0.039), (0.0, 0.039)], 96)
    for v in cyl.verts:
        if 0.006 < v.z < 0.033:
            a = math.atan2(v.y, v.x)
            k = min(1.0, (v.z - 0.006) / 0.004, (0.033 - v.z) / 0.004)
            v.x *= 1.0 - 0.13 * k * max(0.0, math.cos(6 * a)) ** 6
            v.y *= 1.0 - 0.13 * k * max(0.0, math.cos(6 * a)) ** 6
    part(cyl.moved(at(-0.0195, 0.0, -0.0135) @ turn(90.0, "Y")), "cylinder", steel, M)
    part(prism([(-0.034, 0.011), (0.030, 0.012), (0.036, 0.009), (0.044, -0.008), (0.042, -0.030), (0.034, -0.045), (0.010, -0.046), (-0.020, -0.040), (-0.034, -0.032), (-0.036, -0.006)], 0.024), "frame", frame, M, 0.002)
    part(prism([(0.032, 0.006), (0.035, 0.016), (0.042, 0.021), (0.053, 0.023), (0.054, 0.018), (0.046, 0.014), (0.042, 0.004)], 0.008), "hammer", frame, M, 0.0012)
    guard = [V((0.012 + 0.017 * math.cos(math.pi * k / 24), 0.0, -0.045 - 0.013 * math.sin(math.pi * k / 24))) for k in range(25)]
    part(sweep(guard, rounded_rect(0.007, 0.0032, 0.0014, 3), up=(0, 1, 0)), "guard", steel, M)
    part(sweep(spline([(0.010, 0.0, -0.044), (0.007, 0.0, -0.052), (0.010, 0.0, -0.060)], 6), circle(0.0018, 8, 0.5), up=(0, 1, 0)), "trigger", steel, M)
    grip = [(0.026, -0.044), (0.032, -0.062), (0.041, -0.086), (0.053, -0.108), (0.062, -0.122), (0.088, -0.118), (0.089, -0.104), (0.079, -0.082), (0.064, -0.058), (0.051, -0.039), (0.042, -0.031), (0.034, -0.042)]
    part(prism(grip, 0.020), "backstrap", steel, M, 0.002)
    inner = [(x + (0.06 - x) * 0.07, z + (-0.083 - z) * 0.07) for x, z in grip]
    part(prism(inner, 0.033), "grips", wood, M, 0.008, 4)
    for x, z in ((-0.026, -0.022), (0.030, -0.034), (0.038, -0.004)):
        part(lathe([(0.0, 0.0), (0.0018, 0.0), (0.0016, 0.0007), (0.0, 0.0009)], 16).moved(at(x, 0.0119, z) @ turn(-90.0, "X")), "screw", steel, M)


def winchester(args):
    """A Winchester Model 1892 saddle-ring carbine: 20 inch round barrel,
    full magazine tube, blued, a walnut forend and stock."""
    exposure(-0.3)
    M = turn(20.0) @ turn(180.0)
    steel = blued()
    wood = walnut("walnut", (0.24, 0.11, 0.05))
    part(rod(0.0095, -0.51, 0.0), "barrel", steel, M)
    part(rod(0.0085, -0.49, 0.0, 0.0, -0.0185), "magazine", steel, M)
    part(prism([(-0.29, 0.006), (0.0, 0.008), (0.0, -0.031), (-0.29, -0.029)], 0.034), "forend", wood, M, 0.009, 4)
    part(box((0.008, 0.036, 0.044), (-0.293, 0.0, -0.010)), "band", steel, M, 0.004, 3)
    part(box((0.006, 0.022, 0.036), (-0.472, 0.0, -0.009)), "magazine_band", steel, M, 0.003, 3)
    part(box((0.006, 0.0022, 0.007), (-0.503, 0.0, 0.0115)), "front_sight", steel, M, 0.0008, 2)
    part(box((0.024, 0.009, 0.006), (-0.36, 0.0, 0.011)), "rear_sight", steel, M, 0.0015, 2)
    part(prism([(0.0, 0.014), (0.155, 0.016), (0.168, 0.008), (0.172, -0.006), (0.165, -0.036), (0.150, -0.042), (0.060, -0.044), (0.0, -0.034)], 0.030), "receiver", steel, M, 0.003)
    part(box((0.045, 0.0012, 0.013), (0.045, 0.0152, -0.019)), "gate", blued("gate"), M, 0.0005, 2)
    part(prism([(0.150, 0.014), (0.158, 0.030), (0.170, 0.036), (0.176, 0.032), (0.168, 0.022), (0.166, 0.012)], 0.008), "hammer", steel, M, 0.0012)
    lever = spline([(0.02, 0.0, -0.044), (0.07, 0.0, -0.048), (0.11, 0.0, -0.050), (0.145, 0.0, -0.050), (0.185, 0.0, -0.056), (0.198, 0.0, -0.072), (0.185, 0.0, -0.088), (0.150, 0.0, -0.086), (0.128, 0.0, -0.074), (0.123, 0.0, -0.058), (0.130, 0.0, -0.050)], 6)
    part(sweep(lever, rounded_rect(0.009, 0.0055, 0.002, 3), up=(0, 1, 0)), "lever", steel, M)
    part(sweep(spline([(0.135, 0.0, -0.044), (0.132, 0.0, -0.052), (0.136, 0.0, -0.060)], 6), circle(0.0017, 8, 0.5), up=(0, 1, 0)), "trigger", steel, M)
    part(prism([(0.165, 0.012), (0.25, 0.002), (0.49, -0.012), (0.497, -0.02), (0.495, -0.125), (0.47, -0.127), (0.30, -0.078), (0.23, -0.062), (0.19, -0.048), (0.165, -0.038)], 0.038), "stock", wood, M, 0.012, 4)
    part(prism([(0.492, -0.011), (0.503, -0.015), (0.501, -0.129), (0.490, -0.129)], 0.040), "butt_plate", steel, M, 0.003)


def cartridge():
    """A .44-40 WCF round, as the Winchester and the Colt both fire: brass
    case and round-nosed lead bullet, standing on z = 0."""
    case = lathe([(0.0, 0.0), (0.0065, 0.0), (0.0065, 0.0016), (0.0058, 0.0017), (0.0058, 0.0024), (0.0060, 0.0026), (0.00595, 0.024), (0.0057, 0.0262), (0.0057, 0.0336), (0.0, 0.0336)], 32)
    lead = lathe([(0.0, 0.030), (0.00548, 0.030), (0.00548, 0.0352), (0.0051, 0.0372), (0.0040, 0.0393), (0.0020, 0.0404), (0.0, 0.0407)], 32)
    return case, lead


def cartridges(args):
    """A cardboard box of 25 rounds, its lid off beside it, three rounds out
    of it, two of them lying in front."""
    exposure(-0.4)
    brass = Shader("brass", metal=1.0, rough=0.28)
    tarnish = brass.noise(150.0, 5.0)
    brass.set(color=brass.ramp(tarnish, [(0.3, (0.48, 0.32, 0.12)), (0.6, (0.75, 0.55, 0.24)), (0.8, (0.82, 0.62, 0.30))]))
    lead = mottled("lead", (0.30, 0.30, 0.32), 0.5, 0.2, 300.0, 0.2, 2000.0, 0.0001, metal=0.6)
    card = kraft("box_card", (0.42, 0.34, 0.22), 0.1)
    case, bullet = cartridge()
    cases, bullets = Geo(), Geo()
    pitch, n = 0.0138, 5
    side = pitch * n
    missing = {(4, 0), (4, 1), (3, 0)}
    for i in range(n):
        for j in range(n):
            if (i, j) in missing:
                continue
            m = at(-side / 2 + pitch * (i + 0.5), -side / 2 + pitch * (j + 0.5), 0.0015)
            cases.add(case.moved(m))
            bullets.add(bullet.moved(m))
    for x, y, a in ((0.012, -0.062, 15.0), (0.05, -0.05, -50.0)):
        m = at(x, y, 0.0065) @ turn(a) @ turn(90.0, "Y")
        cases.add(case.moved(m))
        bullets.add(bullet.moved(m))
    cases.object("cases", brass)
    bullets.object("bullets", lead)
    # The tray.
    t, h = 0.0012, 0.03
    tray = Geo().add(box((side + 2 * t, side + 2 * t, t), (0, 0, t / 2)))
    for s in (-1, 1):
        tray.add(box((t, side + 2 * t, h), (s * (side / 2 + t / 2), 0, h / 2)))
        tray.add(box((side, t, h), (0, s * (side / 2 + t / 2), h / 2)))
    o = tray.object("tray", card)
    bevel(o, 0.0003, 1)
    # The lid, face up, its printed label.
    lw, lh = side + 4 * t, 0.014
    lid = Geo().add(box((lw, lw, t), (0, 0, lh - t / 2)))
    for s in (-1, 1):
        lid.add(box((t, lw, lh), (s * (lw / 2 - t / 2), 0, lh / 2)))
        lid.add(box((lw, t, lh), (0, s * (lw / 2 - t / 2), lh / 2)))
    L = at(0.02, 0.085, 0.0) @ turn(-18.0)
    green = kraft("lid_card", (0.12, 0.16, 0.09), 0.12)
    lo = lid.moved(L).object("lid", green)
    bevel(lo, 0.0003, 1)
    box((lw - 0.012, lw - 0.03, 0.0003), (0, 0, lh + 0.00015)).moved(L).object("label_border", mottled("label_red", (0.48, 0.05, 0.04), 0.6, 0.1), smooth=False)
    box((lw - 0.016, lw - 0.034, 0.0003), (0, 0, lh + 0.0003)).moved(L).object("label", mottled("label_cream", (0.74, 0.64, 0.44), 0.6, 0.08), smooth=False)
    box((lw - 0.024, 0.006, 0.0003), (0, 0, lh + 0.00045)).moved(L).object("label_band", mottled("label_ink", (0.06, 0.05, 0.04), 0.6, 0.1), smooth=False)


# --- Paper and money --------------------------------------------------------


def letter(args):
    """A letter of introduction in a cream envelope, seen from the back: its
    folded flaps, the pointed top flap sealed with red wax."""
    exposure(-1.1)
    W, H = 0.16, 0.105
    paper = Shader("envelope", rough=0.75, sss=0.1, sss_radius=(0.002, 0.002, 0.002))
    stain = paper.noise(18.0, 4.0)
    paper.set(color=paper.ramp(paper.math("ADD", stain, paper.math("MULTIPLY", paper.noise(400.0, 3.0), 0.2)), [(0.35, (0.70, 0.62, 0.46)), (0.55, (0.78, 0.71, 0.55)), (0.9, (0.66, 0.56, 0.38))]))
    paper.bump(paper.noise(900.0, 5.0), 0.15, 0.0002)
    # The flaps' edges shade the paper under them.
    paper.set(color=paper.mix(paper.math("SUBTRACT", 1.0, paper.ao(0.004)), paper.bsdf.inputs["Base Color"].links[0].from_socket, (0.32, 0.26, 0.17)))
    M = turn(28.0) @ turn(52.0, "X")
    body = box((W, H, 0.0018), (0, 0, 0.0009))
    for v in body.verts:
        v.z += 0.0006 * (1 - (2 * v.x / W) ** 2) * (1 - (2 * v.y / H) ** 2)
    o = body.moved(M).object("envelope", paper)
    bevel(o, 0.0004, 2)

    def flap(points, z, name):
        g = Geo([V((x, y, z)) for x, y in points] + [V((x, y, z + 0.0004)) for x, y in points])
        n = len(points)
        g.faces = [tuple(range(n)), tuple(reversed(range(n, 2 * n)))] + [(j, (j + 1) % n, n + (j + 1) % n, n + j) for j in range(n)]
        part(g, name, paper, M, 0.0001, 1)

    flap([(-W / 2, -H / 2), (W / 2, -H / 2), (0.025, 0.004), (-0.025, 0.004)], 0.0019, "bottom_flap")
    flap([(-W / 2, -H / 2), (-0.012, 0.0), (-W / 2, H / 2)], 0.0024, "left_flap")
    flap([(W / 2, -H / 2), (W / 2, H / 2), (0.012, 0.0)], 0.0024, "right_flap")
    flap([(-W / 2, H / 2), (0.0, -0.012), (W / 2, H / 2)], 0.0029, "top_flap")
    # The seal: a blob of wax pressed with a ringed stamp.
    prof = [(0.0, 0.0028), (0.0062, 0.0028), (0.0067, 0.0037), (0.0085, 0.0034), (0.011, 0.0024), (0.0124, 0.001), (0.0128, 0.0)]
    seal = lathe(prof, 64)
    for v in seal.verts:
        a = math.atan2(v.y, v.x)
        r = math.hypot(v.x, v.y)
        if r > 0.008:
            f = 1.0 + 0.09 * mathutils.noise.noise(V((math.cos(a) * 2.0, math.sin(a) * 2.0, 0.3)))
            v.x *= f
            v.y *= f
    wax = Shader("wax", rough=0.3, coat=0.4, coat_rough=0.2, sss=0.2, sss_radius=(0.002, 0.0005, 0.0005))
    wax.set(color=wax.ramp(wax.noise(200.0, 4.0), [(0.3, (0.26, 0.012, 0.01)), (0.7, (0.42, 0.03, 0.02))]))
    p = wax.coord()
    rings = wax.wave(1.0, wax.coord((900.0, 900.0, 0.0)), "S", rings=True)
    wax.bump(wax.math("MULTIPLY", rings, wax.math("LESS_THAN", wax.math("ADD", wax.math("POWER", wax.axis(p, "X"), 2.0), wax.math("POWER", wax.axis(p, "Y"), 2.0)), 0.0062**2)), 0.6, 0.0003)
    part(seal.moved(at(0.0, -0.012, 0.0031)), "seal", wax, M)


def coin_body():
    prof = [(0.0, -0.0009), (0.0155, -0.0009), (0.0163, -0.0014), (0.0180, -0.0014), (0.0185, -0.0010), (0.0185, 0.0010), (0.0180, 0.0014), (0.0163, 0.0014), (0.0155, 0.0009), (0.0, 0.0009)]
    g = lathe(prof, 96)
    for k in range(64):
        a = 2 * math.pi * k / 64
        g.add(ellipsoid((0.00055, 0.00055, 0.00035), 8, 4).moved(at(0.0149 * math.cos(a), 0.0149 * math.sin(a), 0.0009)))
    return g


def coins(args):
    """Two silver pesos (the patacón of 1881-83): one face up showing
    Liberty in her Phrygian cap, one leaning on it showing the arms in a
    laurel wreath."""
    exposure(-0.2)
    s = Shader("silver", metal=1.0, rough=0.38)
    ao = s.ao(0.003)
    s.set(color=s.ramp(s.math("ADD", ao, s.math("MULTIPLY", s.noise(150.0, 4.0), 0.15)), [(0.45, (0.22, 0.20, 0.18)), (0.9, (0.62, 0.62, 0.63))]))
    s.set(rough=s.ramp(s.noise(200.0, 4.0), [(0.3, (0.28, 0.28, 0.28)), (0.7, (0.48, 0.48, 0.48))]))
    s.bump(s.noise(1500.0, 4.0), 0.15, 0.0001)
    head = [(-2, -9), (4, -9), (4, -5), (7, -1), (8, 4), (6, 8), (2, 10), (-1, 9.5), (-3.5, 7.5), (-4, 5), (-4.5, 3), (-6.5, 1), (-4.5, 0), (-5, -1.5), (-4.3, -2.5), (-4.5, -4), (-2, -5)]
    face = coin_body()
    relief = prism([(x * 0.001, y * 0.001) for x, y in head], 0.0007).moved(at(0.0, 0.0, 0.0009) @ turn(-90.0, "X"))
    for v in relief.verts:
        if v.z > 0.0011:
            v.z += 0.0003 * (1 - (math.hypot(v.x, v.y) / 0.011) ** 2)
    face.object("peso_face", s)
    r = relief.object("liberty", s)
    bevel(r, 0.0003, 2)
    back = coin_body()
    # The laurel wreath: two branches of leaves up the two sides.
    for k in range(20):
        for side in (-1, 1):
            t = math.radians(-100 + 10 * k) * side + (0 if side > 0 else math.pi)
            leaf = ellipsoid((0.0016, 0.0007, 0.0004), 10, 5).moved(at(0.0118 * math.cos(t), 0.0118 * math.sin(t), 0.0009) @ turn(math.degrees(t) + 90 + 35 * side))
            back.add(leaf)
    shield = lathe([(0.0044, 0.0), (0.0052, 0.0), (0.0052, 0.0005), (0.0044, 0.0005)], 48).moved(at(0.0, 0.0, 0.0009) @ mathutils.Matrix.Diagonal((0.85, 1.1, 1.0, 1.0)))
    back.add(shield)
    # The clasped hands across the oval.
    back.add(box((0.0086, 0.0016, 0.0005), (0.0, 0.0, 0.0011)))
    lean = at(0.016, 0.024, 0.009) @ turn(-10.0) @ turn(32.0, "X")
    back.moved(lean).object("peso_back", s)


# Our models: icon name -> (builder, degrees to tilt across the frame).
MODELS = {
    "cane_glass": (cane_glass, 0.0),
    "gin_glass": (gin_glass, 0.0),
    "wine_glass": (wine_glass, 0.0),
    "tobacco": (tobacco, 0.0),
    "matches": (matches, 0.0),
    "biscuit": (biscuit, 0.0),
    "cheese": (cheese, 0.0),
    "yerba": (yerba, 0.0),
    "sugar": (sugar, 0.0),
    "flour": (flour, 0.0),
    "jerky": (jerky, 0.0),
    "maneador": (maneador, 0.0),
    "lasso": (lasso, 0.0),
    "alpargatas": (alpargatas, 0.0),
    "poncho": (poncho, 0.0),
    "revolver": (revolver, 0.0),
    "winchester": (winchester, 30.0),
    "cartridges": (cartridges, 0.0),
    "letter": (letter, 0.0),
    "coins": (coins, 0.0),
}


def icon(name, args):
    bpy.ops.wm.read_factory_settings(use_empty=True)
    if name in ICONS:
        asset, keep, tilt, fix = ICONS[name]
        gltf = os.path.join(os.path.expanduser(args.polyhaven), asset, f"{asset}_1k.gltf")
        if not os.path.isfile(gltf):
            sys.exit(f"item_icons: {gltf} not found (download it first)")
        bpy.ops.import_scene.gltf(filepath=gltf)
        if keep:
            keep_only(keep)
        if fix:
            FIXES[fix]()
    else:
        build, tilt = MODELS[name]
        build(args)
        bake()
    tilt_all(tilt)
    frame(args.size)
    lights()
    path = os.path.join(os.path.abspath(args.output), f"{name}.png")
    render(path, args.samples)
    print(f"item_icons: {path}")


def main():
    argv = sys.argv[sys.argv.index("--") + 1 :] if "--" in sys.argv else []
    args = parse(argv)
    os.makedirs(args.output, exist_ok=True)
    names = list(ICONS) + list(MODELS)
    for name in args.only or names:
        if name not in names:
            sys.exit(f"item_icons: no icon named {name!r}")
        icon(name, args)


if __name__ == "__main__":
    main()
