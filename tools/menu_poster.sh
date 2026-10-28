#!/usr/bin/env bash
# Bakes the pictures of the title and pause menus:
# - `assets/menu/poster.png`, the left half: Latzina's 1882 map of Argentina
#   in worn black and white, with a "Se busca" poster of Bairoletto and Mate
#   Cosido pinned over it, dusty and faded;
# - `assets/menu/band.png`, the right edge: a band of red paint brushed
#   down the screen, ragged on its left, that the choices are lettered on;
# - `assets/menu/vignette.png`: dark, grainy corners laid over the paused
#   world (which the game turns grey).
#
# Sources (see assets/menu/CREDITS.md): `assets/menu/sources/` holds the map,
# the restored 1928 Bairoletto photograph and the Mate Cosido police
# photographs; the lettering is Rye and IM Fell English from `assets/fonts/`.
#
# Needs ImageMagick 7 (`magick`). Run from anywhere:
#   tools/menu_poster.sh
set -euo pipefail

# The grain does not compress: every picture is cut to an 8-bit palette
# (dithered), about a fifteenth of its size and hard to tell apart in sepia.

root="$(cd "$(dirname "$0")/.." && pwd)"
src="$root/assets/menu/sources"
rye="$root/assets/fonts/rye/Rye-Regular.ttf"
fell="$root/assets/fonts/im_fell_english/IMFeENrm28P.ttf"
fell_italic="$root/assets/fonts/im_fell_english/IMFeENit28P.ttf"
out="$root/assets/menu/poster.png"
work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT

ink='#2a2118'
# Fixed seeds keep the noise, and so the baked image, the same run to run.
seed=1919

# Grain: grey noise round mid-grey, for an Overlay blend (mid-grey leaves
# the picture as it is). Arguments: width, height, seed, strength, blur.
grain() {
    magick -size "$1x$2" xc:gray50 -seed "$3" -attenuate "$4" +noise Gaussian \
        -colorspace Gray -blur "0x$5" "$6"
}

# Dust: sparse pale specks on transparent. Arguments: width, height, seed,
# share of the pixels left clear (percent), output.
dust() {
    magick -size "$1x$2" xc: -seed "$3" +noise Random -colorspace Gray \
        -threshold "$4%" -blur 0x0.7 \
        \( +clone -fill '#f4ead2' -colorize 100% \) +swap \
        -alpha off -compose CopyOpacity -composite "$5"
}

# The map: black and white, then mapped from dark ink to old paper.
magick "$src/latzina_1882.jpg" -shave 2%x1% -resize x1400 -colorspace Gray \
    -normalize -level 22%,88% -colorspace sRGB \
    +level-colors '#1e1810','#d6c6a2' \
    "$work/map.png"
map_size="$(magick identify -format '%wx%h' "$work/map.png")"
map_w="${map_size%x*}"
map_h="${map_size#*x}"
canvas_w=$((map_w * 3 / 2))

# The poster's paper: cream, blotched, darker at the edges.
w=760
h=1020
grain $w $h $seed 1.2 18 "$work/blotch.png"
grain $w $h $((seed + 1)) 0.6 0.6 "$work/fiber.png"
magick -size ${w}x${h} xc:'#e8dab8' \
    "$work/blotch.png" -compose Overlay -composite \
    "$work/fiber.png" -compose Overlay -composite \
    \( -size ${w}x${h} radial-gradient:white-'#c9b088' \) \
    -compose Multiply -composite \
    "$work/paper.png"

# A portrait: grey and hard like newsprint, cropped to head and shoulders,
# on the paper's tone, with an inked frame.
portrait() {
    local in="$1" crop="$2" out="$3"
    magick "$in" -crop "$crop" +repage -resize 300x360^ -gravity center -extent 300x360 \
        -colorspace Gray -normalize -level 5%,95% -sigmoidal-contrast 3,50% \
        -colorspace sRGB +level-colors "$ink",'#e8dab8' \
        -bordercolor "$ink" -border 4 \
        "$out"
}
portrait "$src/bairoletto_1928_restored.jpg" 900x1000+150+0 "$work/bairoletto.png"
# The front shot of the two, on the right of the police photographs.
portrait "$src/mate_cosido.png" 330x400+460+50 "$work/mate.png"

# The lettering and the portraits, set on the paper. Each bandit was
# wanted in his own national territory: Bairoletto in La Pampa, Peralta in
# the Chaco (where the song's reward of 2.000 was offered, in 1936).
magick "$work/paper.png" \
    -fill "$ink" -gravity north \
    -font "$rye" -pointsize 118 -annotate +0+40 'SE BUSCA' \
    -font "$fell" -pointsize 44 -annotate +0+190 'VIVOS O MUERTOS' \
    "$work/bairoletto.png" -gravity northwest -geometry +60+262 -composite \
    "$work/mate.png" -gravity northeast -geometry +60+262 -composite \
    -fill "$ink" -gravity northwest \
    -font "$fell" -pointsize 27 -annotate +60+640 'Juan Bautista' \
    -annotate +60+672 'BAIROLETTO' \
    -font "$fell_italic" -pointsize 24 -annotate +60+706 'La Pampa, 1919' \
    -gravity northeast \
    -font "$fell" -pointsize 27 -annotate +60+640 'Segundo David Peralta' \
    -font "$fell_italic" -annotate +60+672 'alias Mate Cosido' \
    -pointsize 24 -annotate +60+706 'Chaco, 1936' \
    -gravity north \
    -font "$rye" -pointsize 58 -annotate +0+768 '$ 2.000' \
    -font "$fell" -pointsize 34 -annotate +0+846 'DE RECOMPENSA' \
    -font "$fell_italic" -pointsize 24 -annotate +0+930 'Policía de los Territorios Nacionales' \
    "$work/poster.png"

# Worn: faded ink and dust on the poster.
dust $w $h $((seed + 2)) 98.8 "$work/poster_dust.png"
magick "$work/poster.png" -fill '#e8dab8' -colorize 12% \
    "$work/poster_dust.png" -compose Over -composite \
    "$work/worn.png"

# Pinned askew over the map with a soft shadow; grain and dust over it all.
grain "$canvas_w" "$map_h" $((seed + 3)) 0.5 0.7 "$work/grain.png"
dust "$canvas_w" "$map_h" $((seed + 4)) 99.2 "$work/map_dust.png"
# The map on the left of a wider sheet; the poster, a little smaller,
# overlaps its right edge so the whole country shows.
magick -size "${canvas_w}x${map_h}" xc:none \
    "$work/map.png" -gravity west -compose Over -composite \
    \( "$work/worn.png" -resize 88% -background none -rotate 3 \
       \( +clone -background black -shadow 60x10+12+14 \) +swap \
       -background none -layers merge +repage \) \
    -gravity east -geometry +20+40 -compose Over -composite \
    "$work/layout.png"
# Grain and dust only where there is paper: the sheet's own alpha cuts them.
magick "$work/layout.png" \
    \( +clone -alpha off "$work/grain.png" -compose Overlay -composite \
       "$work/map_dust.png" -compose Over -composite \) \
    \( -clone 0 -alpha extract \) -delete 0 \
    -alpha off -compose CopyOpacity -composite \
    -dither FloydSteinberg -colors 256 -strip "PNG8:$out"

# The band: oxblood paint brushed top to bottom. Its left edge wanders
# row by row: slow waves of the stroke plus the fine streaks of dry bristles.
band_w=640
band_h=1400
magick -size 1x${band_h} xc: -seed $((seed + 5)) +noise Random -colorspace Gray \
    -blur 0x40 -normalize "$work/edge_slow.png"
magick -size 1x${band_h} xc: -seed $((seed + 6)) +noise Random -colorspace Gray \
    -blur 0x1 -normalize "$work/edge_fine.png"
magick "$work/edge_slow.png" "$work/edge_fine.png" -fx '0.7*u + 0.3*v' "$work/edge.png"
magick -size ${band_w}x${band_h} xc:black "$work/edge.png" \
    -fx 'i > 40 + 220*v.p{0,j} ? 1 : 0' \
    -blur 0x0.8 "$work/band_mask.png"
magick -size ${band_w}x${band_h} xc:'#8e1a12' \
    \( -size ${band_w}x${band_h} xc:gray50 -seed $((seed + 7)) -attenuate 1.4 +noise Gaussian \
       -colorspace Gray -virtual-pixel mirror -motion-blur 0x60+90 -level 25%,75% \) \
    -compose Overlay -composite \
    "$work/band_mask.png" -alpha off -compose CopyOpacity -composite \
    -colors 128 -strip "PNG8:$root/assets/menu/band.png"

# The vignette: clear in the middle, dark and grainy towards the corners.
magick -size 1600x900 radial-gradient:'#00000000'-'#000000f0' \
    -strip "$root/assets/menu/vignette.png"

echo "baked $out ($(magick identify -format '%wx%h' "$out"))"
