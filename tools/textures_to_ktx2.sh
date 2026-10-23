#!/bin/sh
# Converts building texture sets to GPU-compressed KTX2 for the game.
#
# Each SET_FOLDER holds the editable sources `color.jpg`, `normal_gl.jpg`,
# `roughness.jpg` and, optionally, `ao.jpg` (greyscale maps may be stored as
# grey or as RGB with equal channels: the red channel is read). Next to them
# it writes what the game loads:
#
#   color.ktx2      sRGB colour
#   normal_gl.ktx2  linear OpenGL (+Y) normal map, in basisu's normal-map mode
#   orm.ktx2        linear, packed for Bevy: R = ambient occlusion (white
#                   without ao.jpg), G = roughness, B = metallic (white: the
#                   material's own `metallic` scales it per surface)
#
# All three are UASTC LDR 4x4 with their mip chain baked in (sRGB-correct
# filtering for colour, linear for the rest) and Zstandard supercompression
# (basisu's default for UASTC in KTX2). Bevy transcodes UASTC at load to the
# GPU's block format: BC7 on desktop GPUs, 1 byte a texel instead of RGBA8's
# 4. No RDO: it saved ~3% of disk on these sets for ~1 dB of PSNR.
#
# It refuses to overwrite existing .ktx2 files unless FORCE=1 is set. The
# sources stay as they are; re-run it after editing one.
#
# `--check-orm` instead converts a synthetic 8x8 set, unpacks its orm.ktx2
# to BC7 (what Bevy transcodes it to on desktop GPUs) and fails unless AO
# lands in red, roughness in green and white in blue, with and without
# ao.jpg.
#
# Needs basisu (Basis Universal 2.x) and ImageMagick 7 (`magick`).
#
# Usage: [FORCE=1] sh tools/textures_to_ktx2.sh SET_FOLDER...
#        sh tools/textures_to_ktx2.sh --check-orm
#   e.g. sh tools/textures_to_ktx2.sh assets/textures/buildings/*/
set -eu

usage="usage: [FORCE=1] sh textures_to_ktx2.sh SET_FOLDER... | sh textures_to_ktx2.sh --check-orm"
[ $# -gt 0 ] || { echo "$usage" >&2; exit 1; }
for tool in basisu magick; do
    if ! command -v "$tool" >/dev/null 2>&1; then
        echo "textures_to_ktx2.sh: needs \`$tool\` on PATH" >&2
        exit 1
    fi
done
# A private scratch folder (portable: no GNU-only `mktemp --suffix`).
scratch="$(mktemp -d)"
trap 'rm -rf "$scratch"' EXIT

# Packs the ORM map of set SET into PNG OUTPUT: AO (or white) in red,
# roughness in green, white in blue. Each source gives its red channel as
# is (a greyscale JPEG and an RGB one with equal channels agree), and
# `-set colorspace sRGB` only labels the channels for `-combine`: the values
# are copied untouched.
pack_orm() { # SET OUTPUT
    set_dir="$1" dest="$2"
    size="$(magick identify -format '%wx%h' "$set_dir/roughness.jpg")"
    if [ -f "$set_dir/ao.jpg" ]; then
        magick "$set_dir/ao.jpg" -channel R -separate +channel "$scratch/ao.miff"
    else
        magick -size "$size" xc:white -channel R -separate +channel "$scratch/ao.miff"
    fi
    magick "$scratch/ao.miff" \
        \( "$set_dir/roughness.jpg" -channel R -separate +channel \) \
        \( -size "$size" xc:white -channel R -separate +channel \) \
        -set colorspace sRGB -combine -depth 8 "PNG24:$dest"
}

# UASTC KTX2 with mips: encode SOURCE into OUTPUT with extra basisu options.
encode() { # SOURCE OUTPUT [basisu options...]
    src="$1" dest="$2"
    shift 2
    basisu -ktx2 -uastc -uastc_level 2 -mipmap -quiet "$@" \
        -file "$src" -output_file "$dest" >/dev/null
}

if [ "$1" = "--check-orm" ]; then
    # Prints the BC7-transcoded red,green,blue (0-255) of KTX2 file FILE.
    bc7_rgb() { # FILE
        mkdir "$scratch/unpacked"
        # basisu unpacks into the current folder; 6 is BC7.
        (cd "$scratch/unpacked" && basisu -unpack -no_ktx -format_only 6 \
            -file "$1" >/dev/null)
        magick "$scratch"/unpacked/*_rgb_BC7_*level_0_*.png -format \
            '%[fx:round(255*p{4,4}.r)],%[fx:round(255*p{4,4}.g)],%[fx:round(255*p{4,4}.b)]' info:
        rm -rf "$scratch/unpacked"
    }
    expect() { # SET EXPECTED_RGB
        pack_orm "$1" "$scratch/orm.png"
        encode "$scratch/orm.png" "$scratch/orm.ktx2" -linear
        got="$(bc7_rgb "$scratch/orm.ktx2")"
        # JPEG and block rounding: allow two steps either way.
        echo "$got $2" | awk '{ split($1, g, ","); split($2, e, ",");
            for (i = 1; i <= 3; i++) if (g[i] - e[i] > 2 || e[i] - g[i] > 2) exit 1 }' || {
            echo "textures_to_ktx2.sh: ORM came out as $got, expected $2" >&2
            exit 1
        }
    }
    mkdir "$scratch/with" "$scratch/without"
    for set_dir in "$scratch/with" "$scratch/without"; do
        magick -size 8x8 xc:'gray(25%)' "$set_dir/roughness.jpg"
    done
    magick -size 8x8 xc:'gray(80%)' "$scratch/with/ao.jpg"
    expect "$scratch/with" "204,64,255"
    expect "$scratch/without" "255,64,255"
    echo "ORM map: AO in red, roughness in green, white in blue"
    exit 0
fi

for set_dir in "$@"; do
    set_dir="${set_dir%/}"
    for name in color normal_gl roughness; do
        if [ ! -f "$set_dir/$name.jpg" ]; then
            echo "textures_to_ktx2.sh: $set_dir has no $name.jpg" >&2
            exit 1
        fi
    done
    if [ "${FORCE:-0}" != 1 ]; then
        for name in color normal_gl orm; do
            if [ -e "$set_dir/$name.ktx2" ]; then
                echo "textures_to_ktx2.sh: $set_dir/$name.ktx2 exists (FORCE=1 to overwrite)" >&2
                exit 1
            fi
        done
    fi
    encode "$set_dir/color.jpg" "$scratch/color.ktx2" -srgb
    encode "$set_dir/normal_gl.jpg" "$scratch/normal_gl.ktx2" -normal_map
    pack_orm "$set_dir" "$scratch/orm.png"
    encode "$scratch/orm.png" "$scratch/orm.ktx2" -linear
    # Replaced together, once all three encoded.
    for name in color normal_gl orm; do
        mv "$scratch/$name.ktx2" "$set_dir/$name.ktx2"
    done
    echo "$set_dir: color.ktx2 normal_gl.ktx2 orm.ktx2"
done
