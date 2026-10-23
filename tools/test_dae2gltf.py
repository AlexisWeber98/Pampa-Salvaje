"""Tests of tools/dae2gltf.py on tiny synthetic COLLADA documents.

Standard library only: ``python3 -I -B -m unittest discover -s tools``.
"""

import io
import json
import math
import os
import struct
import tempfile
import unittest
import zlib
from contextlib import redirect_stderr, redirect_stdout
from unittest import mock

import dae2gltf

XMLNS = "http://www.collada.org/2005/11/COLLADASchema"


def f32(x):
    return struct.unpack("<f", struct.pack("<f", x))[0]


# --- building synthetic inputs -------------------------------------------------


def document(body, up="Z_UP", unit=1.0):
    return (
        '<?xml version="1.0" encoding="utf-8"?>\n'
        f'<COLLADA xmlns="{XMLNS}" version="1.4.1">'
        f'<asset><unit meter="{unit}"/><up_axis>{up}</up_axis></asset>{body}</COLLADA>'
    )


def source(sid, values, stride=3):
    text = " ".join(str(v) for v in values)
    return (
        f'<source id="{sid}"><float_array id="{sid}-a" count="{len(values)}">{text}</float_array>'
        f'<technique_common><accessor source="#{sid}-a" count="{len(values) // stride}" '
        f'stride="{stride}"/></technique_common></source>'
    )


def geometry(gid, positions, prims, extra=""):
    """One mesh with a position source '<gid>-pos' and vertices '<gid>-v'."""
    return (
        f'<geometry id="{gid}"><mesh>{source(gid + "-pos", positions)}{extra}'
        f'<vertices id="{gid}-v"><input semantic="POSITION" source="#{gid}-pos"/></vertices>'
        f"{prims}</mesh></geometry>"
    )


def triangles(p, vertices, material="m", normals=None):
    inputs = f'<input semantic="VERTEX" source="#{vertices}" offset="0"/>'
    if normals:
        inputs += f'<input semantic="NORMAL" source="#{normals}" offset="0"/>'
    count = len(p.split()) // 3
    return f'<triangles material="{material}" count="{count}">{inputs}<p>{p}</p></triangles>'


def instance(gid, material=None):
    bind = ""
    if material:
        bind = (
            "<bind_material><technique_common>"
            f'<instance_material symbol="m" target="#{material}"/>'
            "</technique_common></bind_material>"
        )
    return f'<instance_geometry url="#{gid}">{bind}</instance_geometry>'


def scene(geometries, nodes, extra=""):
    return (
        f"{extra}<library_geometries>{geometries}</library_geometries>"
        f'<library_visual_scenes><visual_scene id="s">{nodes}</visual_scene></library_visual_scenes>'
        '<scene><instance_visual_scene url="#s"/></scene>'
    )


def textured(image):
    """Image, effect and material 'mat' with a diffuse texture file."""
    return (
        f'<library_images><image id="img"><init_from>{image}</init_from></image></library_images>'
        '<library_effects><effect id="fx"><profile_COMMON><technique sid="common"><lambert>'
        '<diffuse><texture texture="img" texcoord="uv"/></diffuse>'
        "</lambert></technique></profile_COMMON></effect></library_effects>"
        '<library_materials><material id="mat"><instance_effect url="#fx"/></material>'
        "</library_materials>"
    )


TRI = "0 0 0  1 0 0  0 1 0"
ONE_TRIANGLE = document(
    scene(geometry("g", TRI.split(), triangles("0 1 2", "g-v")), f'<node id="n">{instance("g")}</node>')
)


def png_bytes(rows, channels=3):
    """A minimal 8-bit PNG: rows of pixel tuples."""
    h, w = len(rows), len(rows[0])
    colour = {3: 2, 4: 6}[channels]
    raw = b"".join(b"\x00" + bytes(c for px in row for c in px) for row in rows)

    def chunk(kind, body):
        return struct.pack(">I", len(body)) + kind + body + struct.pack(">I", zlib.crc32(kind + body))

    return (
        b"\x89PNG\r\n\x1a\n"
        + chunk(b"IHDR", struct.pack(">IIBBBBB", w, h, 8, colour, 0, 0, 0))
        + chunk(b"IDAT", zlib.compress(raw))
        + chunk(b"IEND", b"")
    )


def normal_rows(dx, n=64, k=100.0):
    """The normal map of a smooth height field h(x, row), in OpenGL (green
    up) or DirectX (green down) convention. Rows run down the image."""
    a, b = 0.2, 0.15
    rows = []
    for r in range(n):
        row = []
        for x in range(n):
            hx = a * math.cos(a * x) * math.cos(b * r)
            hr = -b * math.sin(a * x) * math.sin(b * r)
            red = 128 - k * hx
            green = 128 + (-k * hr if dx else k * hr)
            row.append((int(round(red)), int(round(green)), 255))
        rows.append(row)
    return rows


# --- the tests -------------------------------------------------------------------


class Converter(unittest.TestCase):
    def setUp(self):
        tmp = tempfile.TemporaryDirectory()
        self.addCleanup(tmp.cleanup)
        self.tmp = tmp.name
        self.src = os.path.join(self.tmp, "src")
        self.out = os.path.join(self.tmp, "out")
        os.makedirs(self.src)

    def put(self, name, data):
        path = os.path.join(self.src, name)
        with open(path, "wb") as f:
            f.write(data if isinstance(data, bytes) else data.encode("utf-8"))
        return path

    def run_main(self, argv):
        out, err = io.StringIO(), io.StringIO()
        with redirect_stdout(out), redirect_stderr(err):
            code = dae2gltf.main(argv)
        return code, out.getvalue(), err.getvalue()

    def convert(self, text, *args):
        path = self.put("m.dae", text)
        return self.run_main([path, "-o", self.out, *args])

    def converted(self, text, *args):
        code, out, err = self.convert(text, *args)
        self.assertEqual(code, 0, err)
        return out

    def load(self):
        with open(os.path.join(self.out, "model.gltf"), encoding="utf-8") as f:
            g = json.load(f)
        with open(os.path.join(self.out, "model.bin"), "rb") as f:
            blob = f.read()
        return g, blob

    def positions(self, name):
        g, blob = self.load()
        node = next(n for n in g["nodes"] if n["name"] == name)
        found = []
        for p in g["meshes"][node["mesh"]]["primitives"]:
            a = g["accessors"][p["attributes"]["POSITION"]]
            v = g["bufferViews"][a["bufferView"]]
            for i in range(a["count"]):
                found.append(struct.unpack_from("<3f", blob, v["byteOffset"] + 12 * i))
        return node, found

    def assertPoints(self, got, want):
        self.assertEqual(len(got), len(want))
        for p, q in zip(sorted(got), sorted(want)):
            for a, b in zip(p, q):
                self.assertAlmostEqual(a, b, places=5)

    def check(self):
        return self.run_main(["--check", os.path.join(self.out, "model.gltf")])

    def rewrite(self, edit):
        g, _ = self.load()
        edit(g)
        with open(os.path.join(self.out, "model.gltf"), "w", encoding="utf-8") as f:
            json.dump(g, f)

    # Geometry.

    def test_z_up_and_unit_become_y_up_meters(self):
        text = document(
            scene(
                geometry("g", [0, 0, 2, 2, 0, 2, 0, 2, 2], triangles("0 1 2", "g-v")),
                f'<node id="n">{instance("g")}</node>',
            ),
            up="Z_UP",
            unit=0.5,
        )
        self.converted(text)
        _, points = self.positions("body")
        self.assertPoints(points, [(0, 1, 0), (1, 1, 0), (0, 1, -1)])

    def test_polylist_quad_is_fanned_into_two_triangles(self):
        prims = (
            '<polylist material="m" count="1"><input semantic="VERTEX" source="#g-v" offset="0"/>'
            "<vcount>4</vcount><p>0 1 2 3</p></polylist>"
        )
        text = document(
            scene(geometry("g", [0, 0, 0, 1, 0, 0, 1, 1, 0, 0, 1, 0], prims), f'<node id="n">{instance("g")}</node>'),
            up="Y_UP",
        )
        out = self.converted(text)
        self.assertIn("total: 2 triangles", out)
        self.assertEqual(self.check()[0], 0)

    def test_accessor_bounds_are_the_stored_float32_values(self):
        text = document(
            scene(geometry("g", [0.1, 0.2, 0.3, 1.1, 0.2, 0.3, 0.1, 1.3, 0.3], triangles("0 1 2", "g-v")),
                  f'<node id="n">{instance("g")}</node>'),
            up="Y_UP",
        )
        self.converted(text)
        g, _ = self.load()
        position = g["accessors"][g["meshes"][0]["primitives"][0]["attributes"]["POSITION"]]
        self.assertEqual(position["min"], [f32(0.1), f32(0.2), f32(0.3)])
        self.assertEqual(position["max"], [f32(1.1), f32(1.3), f32(0.3)])

    def test_vertices_from_different_sources_are_not_merged(self):
        # Two primitives of one material share index values but read
        # different position sources: their vertices must stay apart.
        # The normal input shares offset 0, so it needs one normal per vertex.
        extra = (
            source("g-pos2", [5, 5, 5, 6, 5, 5, 5, 6, 5])
            + source("g-nrm", [0, 0, 1] * 3)
            + '<vertices id="g-v2"><input semantic="POSITION" source="#g-pos2"/></vertices>'
        )
        prims = triangles("0 1 2", "g-v", normals="g-nrm") + triangles("0 1 2", "g-v2", normals="g-nrm")
        text = document(
            scene(geometry("g", TRI.split(), prims, extra), f'<node id="n">{instance("g")}</node>'),
            up="Y_UP",
        )
        self.converted(text)
        _, points = self.positions("body")
        self.assertPoints(points, [(0, 0, 0), (1, 0, 0), (0, 1, 0), (5, 5, 5), (6, 5, 5), (5, 6, 5)])

    # The rotor.

    def rotor_document(self):
        wheel = (
            '<node id="Wheel" name="Wheel"><translate sid="t">1 2 3</translate>'
            f'<rotate sid="spin">0 1 0 0</rotate>{instance("w")}</node>'
        )
        animations = (
            '<library_animations><animation id="a">'
            '<channel source="#sampler" target="Wheel/spin.ANGLE"/></animation></library_animations>'
        )
        return document(
            scene(
                geometry("g", TRI.split(), triangles("0 1 2", "g-v"))
                + geometry("w", [0, 0, 0, 1, 0, 0, 0, 0, 1], triangles("0 1 2", "w-v")),
                f'<node id="Tower">{instance("g")}</node>{wheel}',
                animations,
            ),
            up="Z_UP",
        )

    def test_rotor_is_split_at_its_pivot_with_its_animated_axis(self):
        out = self.converted(self.rotor_document(), "--rotor", "Wheel")
        node, points = self.positions("rotor")
        self.assertPoints([tuple(node["translation"])], [(1, 3, -2)])
        self.assertPoints(points, [(0, 0, 0), (1, 0, 0), (0, 1, 0)])
        self.assertIn("axis (0.000, 0.000, -1.000)", out)
        _, body = self.positions("body")
        self.assertEqual(len(body), 3)

    def test_rotor_name_is_independent_of_internal_group_keys(self):
        self.converted(self.rotor_document(), "--rotor", "Wheel", "--rotor-name", "wheel")
        g, _ = self.load()
        self.assertEqual(sorted(n["name"] for n in g["nodes"]), ["body", "wheel"])

    def test_rotor_name_body_is_refused(self):
        path = self.put("m.dae", self.rotor_document())
        with self.assertRaises(SystemExit), redirect_stderr(io.StringIO()):
            dae2gltf.main([path, "-o", self.out, "--rotor", "Wheel", "--rotor-name", "body"])

    # Hostile input.

    def test_dtd_and_entities_are_refused(self):
        text = ONE_TRIANGLE.replace("\n", '\n<!DOCTYPE COLLADA [<!ENTITY x "y">]>\n', 1)
        code, _, err = self.convert(text)
        self.assertEqual(code, 1)
        self.assertIn("DTD", err)

    def test_non_utf8_encodings_are_refused(self):
        # In UTF-16 the byte check for '<!DOCTYPE' would see nothing.
        text = ONE_TRIANGLE.replace("utf-8", "utf-16").replace("\n", '\n<!DOCTYPE COLLADA [<!ENTITY x "y">]>\n', 1)
        code, _, err = self.convert(text.encode("utf-16"))
        self.assertEqual(code, 1)
        self.assertIn("encoding", err)

    def test_out_of_range_index_is_refused(self):
        code, _, err = self.convert(ONE_TRIANGLE.replace("<p>0 1 2</p>", "<p>0 1 5</p>"))
        self.assertEqual(code, 1)
        self.assertIn("out of range", err)

    def test_unexpected_errors_show_their_traceback(self):
        path = self.put("m.dae", ONE_TRIANGLE)
        with mock.patch.object(dae2gltf, "convert", side_effect=KeyError("bug")):
            with self.assertRaises(KeyError):
                self.run_main([path, "-o", self.out])

    # Textures.

    def textured_document(self, image):
        return document(
            scene(
                geometry("g", TRI.split(), triangles("0 1 2", "g-v")),
                f'<node id="n">{instance("g", "mat")}</node>',
                textured(image),
            ),
            up="Y_UP",
        )

    def test_textures_other_than_png_or_jpeg_are_refused(self):
        self.put("tex.bmp", b"BM" + bytes(64))
        code, _, err = self.convert(self.textured_document("tex.bmp"))
        self.assertEqual(code, 1)
        self.assertIn("PNG or JPEG", err)

    def test_texture_lookup_prefers_the_exact_case(self):
        self.put("Tex.png", png_bytes([[(1, 1, 1)]]))
        self.put("tex.png", png_bytes([[(2, 2, 2)]]))
        self.converted(self.textured_document("tex.png"))
        g, _ = self.load()
        self.assertEqual(g["images"][0]["uri"], "textures/tex.png")
        with open(os.path.join(self.out, "textures", "tex.png"), "rb") as f:
            self.assertEqual(f.read(), png_bytes([[(2, 2, 2)]]))

    def test_texture_lookup_refuses_ambiguous_case_only_matches(self):
        self.put("Tex.png", png_bytes([[(1, 1, 1)]]))
        self.put("tex.png", png_bytes([[(2, 2, 2)]]))
        code, _, err = self.convert(self.textured_document("TEX.png"))
        self.assertEqual(code, 1)
        self.assertIn("ambiguous", err)

    # Normal maps.

    def test_green_convention_is_detected_on_both_conventions(self):
        for dx, want in ((False, "gl"), (True, "dx")):
            rows = [bytearray(c for px in row for c in px) for row in normal_rows(dx)]
            found, ratio = dae2gltf.green_convention(64, 64, 3, rows)
            self.assertEqual(found, want)
            self.assertTrue(ratio < 0.2 or ratio > 5, ratio)

    def normal_mapped(self, dx):
        self.put("tex.png", png_bytes([[(9, 9, 9)]]))
        self.put("tex_NRM.png", png_bytes(normal_rows(dx)))
        return self.textured_document("tex.png")

    def normal_out(self):
        w, h, channels, rows = dae2gltf.png_read(os.path.join(self.out, "textures", "tex_NRM.png"))
        return rows

    def test_directx_normal_map_gets_its_green_flipped(self):
        out = self.converted(self.normal_mapped(dx=True))
        self.assertIn("applied DX (detected", out)
        source_rows = normal_rows(True)
        self.assertEqual(self.normal_out()[5][3 * 7 + 1], 255 - source_rows[5][7][1])
        self.assertEqual(self.check()[0], 0)

    def test_opengl_normal_map_is_copied(self):
        out = self.converted(self.normal_mapped(dx=False))
        self.assertIn("applied GL (detected", out)
        self.assertEqual(self.normal_out()[5][3 * 7 + 1], normal_rows(False)[5][7][1])

    def test_forced_convention_is_reported_as_forced(self):
        out = self.converted(self.normal_mapped(dx=False), "--normal", "dx")
        self.assertIn("applied DX (forced", out)
        self.assertEqual(self.normal_out()[5][3 * 7 + 1], 255 - normal_rows(False)[5][7][1])

    def test_forced_gl_does_not_decode_the_map(self):
        self.put("tex.png", png_bytes([[(9, 9, 9)]]))
        self.put("tex_NRM.png", b"\x89PNG\r\n\x1a\n" + b"not a real png")
        out = self.converted(self.textured_document("tex.png"), "--normal", "gl")
        self.assertIn("applied GL (forced", out)

    # --check.

    def test_check_passes_on_converted_output(self):
        self.converted(ONE_TRIANGLE)
        code, out, _ = self.check()
        self.assertEqual(code, 0)
        self.assertIn("1 triangles", out)

    def test_check_rejects_negative_or_missing_buffer_view(self):
        self.converted(ONE_TRIANGLE)
        self.rewrite(lambda g: g["accessors"][-1].update(bufferView=-1))
        self.assertEqual(self.check()[0], 1)
        self.rewrite(lambda g: g["accessors"][-1].pop("bufferView"))
        self.assertEqual(self.check()[0], 1)

    def test_check_rejects_negative_buffer(self):
        self.converted(ONE_TRIANGLE)
        self.rewrite(lambda g: g["bufferViews"][0].update(buffer=-1))
        self.assertEqual(self.check()[0], 1)

    def test_check_rejects_images_that_are_not_png_or_jpeg(self):
        self.put("tex.png", png_bytes([[(9, 9, 9)]]))
        self.converted(self.textured_document("tex.png"))
        self.assertEqual(self.check()[0], 0)
        with open(os.path.join(self.out, "textures", "tex.png"), "wb") as f:
            f.write(b"BM" + bytes(64))
        code, out, _ = self.check()
        self.assertEqual(code, 1)
        self.assertIn("not a PNG", out)


if __name__ == "__main__":
    unittest.main()
