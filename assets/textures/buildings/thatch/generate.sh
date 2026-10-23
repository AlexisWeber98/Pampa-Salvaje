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
# Usage: [FORCE=1] sh generate.sh OUTPUT_FOLDER
set -eu

out="${1:?usage: [FORCE=1] sh generate.sh OUTPUT_FOLDER}"
case "$(magick -version | head -n 1)" in
    *HDRI*) ;;
    *) echo "generate.sh: needs an HDRI build of ImageMagick 7" >&2; exit 1 ;;
esac
if [ "${FORCE:-0}" != 1 ]; then
    for f in color.jpg normal_gl.jpg roughness.jpg; do
        if [ -e "$out/$f" ]; then
            echo "generate.sh: $out/$f exists; set FORCE=1 to overwrite" >&2
            exit 1
        fi
    done
fi
mkdir -p "$out"
height="$(mktemp --suffix=.png)"
trap 'rm -f "$height"' EXIT

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

# Normal map, OpenGL convention (+X right, +Y up the image): red is the
# Sobel:180 slope and green the Sobel:90 slope, so a surface rising to the
# right tilts its normal left (red below 50%) and one rising up the image
# tilts it down (green below 50%); both were checked on known ramps.
# `multiply 0.5` sets the bump strength and `add 50%` re-centres zero slope
# on mid-grey.
magick "$height" -virtual-pixel tile -define convolve:scale='!' \
    \( -clone 0 -morphology Convolve Sobel:180 -evaluate multiply 0.5 -evaluate add 50% \) \
    \( -clone 0 -morphology Convolve Sobel:90 -evaluate multiply 0.5 -evaluate add 50% \) \
    \( -clone 0 -evaluate set 100% \) \
    -delete 0 -combine -quality 90 "$out/normal_gl.jpg"
