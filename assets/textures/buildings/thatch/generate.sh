#!/bin/sh
# Generates the thatch (paja) texture set: color.jpg, normal_gl.jpg and
# roughness.jpg, 1024x1024, with ImageMagick 7 (`magick`).
#
# These are the commands and seeds (1919 and 7) the shipped files were made
# with on 2026-10-22; the normal map's red channel was corrected the same
# day (see below). The normal map needs an HDRI build of ImageMagick: the
# Sobel slopes are signed, and a non-HDRI build clamps the negative half to
# zero before the +50% shift, silently flattening the map.
#
# It refuses to overwrite an existing set unless FORCE=1 is set, so run it
# into a scratch folder and compare before replacing the shipped files.
#
# `--check-signs` instead runs the normal-map step on two known ramps and
# fails unless a surface rising to the right has red below 50% and one
# rising up the image has green below 50% (OpenGL convention).
#
# Usage: [FORCE=1] sh generate.sh OUTPUT_FOLDER
#        sh generate.sh --check-signs
set -eu

usage="usage: [FORCE=1] sh generate.sh OUTPUT_FOLDER | sh generate.sh --check-signs"
out="${1:?$usage}"
if ! command -v magick >/dev/null 2>&1; then
    echo "generate.sh: needs ImageMagick 7 (\`magick\`) on PATH" >&2
    exit 1
fi
case "$(magick -version | head -n 1)" in
    *HDRI*) ;;
    *) echo "generate.sh: needs an HDRI build of ImageMagick 7" >&2; exit 1 ;;
esac
# A private scratch folder (portable: no GNU-only `mktemp --suffix`).
scratch="$(mktemp -d)"
trap 'rm -rf "$scratch"' EXIT

# Normal map, OpenGL convention (+X right, +Y up the image): red is the
# Sobel:180 slope and green the Sobel:90 slope, so a surface rising to the
# right tilts its normal left (red below 50%) and one rising up the image
# tilts it down (green below 50%); `--check-signs` proves both on ramps.
# `multiply 0.5` sets the bump strength and `add 50%` re-centres zero slope
# on mid-grey.
normal_map() { # HEIGHT OUTPUT [extra output options...]
    src="$1" dest="$2"
    shift 2
    magick "$src" -virtual-pixel tile -define convolve:scale='!' \
        \( -clone 0 -morphology Convolve Sobel:180 -evaluate multiply 0.5 -evaluate add 50% \) \
        \( -clone 0 -morphology Convolve Sobel:90 -evaluate multiply 0.5 -evaluate add 50% \) \
        \( -clone 0 -evaluate set 100% \) \
        -delete 0 -combine "$@" "$dest"
}

if [ "$out" = "--check-signs" ]; then
    # Steep ramps from 0 to 1 (1/63 a pixel, so the slope is clear of
    # rounding): left to right, and bottom to top. The centre crop
    # keeps the tile wrap at the edges out of the means.
    magick -size 64x64 gradient:black-white -rotate -90 "$scratch/right.miff"
    magick -size 64x64 gradient:black-white -flip "$scratch/up.miff"
    normal_map "$scratch/right.miff" "$scratch/right_n.miff"
    normal_map "$scratch/up.miff" "$scratch/up_n.miff"
    centre='-gravity center -crop 48x48+0+0 +repage'
    # shellcheck disable=SC2086 # $centre is deliberately split.
    red="$(magick "$scratch/right_n.miff" $centre -format '%[fx:mean.r]' info:)"
    # shellcheck disable=SC2086
    green="$(magick "$scratch/up_n.miff" $centre -format '%[fx:mean.g]' info:)"
    echo "rising right: mean red $red (want < 0.5); rising up: mean green $green (want < 0.5)"
    if awk -v r="$red" -v g="$green" 'BEGIN { exit !(r < 0.5 && g < 0.5) }'; then
        echo "generate.sh: normal-map signs OK (OpenGL)"
        exit 0
    fi
    echo "generate.sh: normal-map signs WRONG" >&2
    exit 1
fi

if [ "${FORCE:-0}" != 1 ]; then
    for f in color.jpg normal_gl.jpg roughness.jpg; do
        if [ -e "$out/$f" ]; then
            echo "generate.sh: $out/$f exists; set FORCE=1 to overwrite" >&2
            exit 1
        fi
    done
fi
mkdir -p "$out"
height="$scratch/height.png"

# Height: random noise streaked vertically into straw (motion blur
# wrapping round the tile), multiplied by a blotchy layer for darker,
# older patches.
magick -size 1024x1024 xc: -seed 1919 +noise Random -colorspace gray \
    -virtual-pixel tile -motion-blur 0x120+90 -auto-level -sigmoidal-contrast 4x50% \
    \( -size 1024x1024 xc: -seed 7 +noise Random -colorspace gray \
       -virtual-pixel tile -blur 0x10 -auto-level +level 40%,100% \) \
    -compose multiply -composite -auto-level "$height"

# Colour: the height tinted dry straw.
magick "$height" -colorspace sRGB -type TrueColor +level 30%,100% \
    \( -size 1024x1024 xc:'#b39258' \) -compose multiply -composite \
    -quality 90 "$out/color.jpg"

# Roughness: dry straw is rough, a little less on the bright tops.
magick "$height" -negate +level 55%,95% -quality 90 "$out/roughness.jpg"

normal_map "$height" "$out/normal_gl.jpg" -quality 90
