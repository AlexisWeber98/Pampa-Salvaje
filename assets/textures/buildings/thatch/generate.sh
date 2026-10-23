#!/bin/sh
# Generates the thatch (paja) texture set: color.jpg, normal_gl.jpg and
# roughness.jpg, 1024x1024, with ImageMagick 7 (`magick`).
#
# These are the exact commands and seeds (1919 and 7) the shipped files
# were made with on 2026-10-22. With the same ImageMagick build they give
# the same bytes; a different build may differ slightly in the JPEG
# encoding. Write into a scratch folder and compare before replacing the
# shipped files.
#
# Usage: sh generate.sh OUTPUT_FOLDER
set -eu

out="${1:?usage: sh generate.sh OUTPUT_FOLDER}"
mkdir -p "$out"
height="$out/height.png"

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

# Normal map, OpenGL convention: Sobel slopes of the height field.
magick "$height" -virtual-pixel tile -define convolve:scale='!' \
    \( -clone 0 -morphology Convolve Sobel:0 -evaluate multiply 0.5 -evaluate add 50% \) \
    \( -clone 0 -morphology Convolve Sobel:90 -evaluate multiply 0.5 -evaluate add 50% \) \
    \( -clone 0 -evaluate set 100% \) \
    -delete 0 -combine -quality 90 "$out/normal_gl.jpg"

rm -f "$height"
