#!/usr/bin/env sh
# Regenerate FurFiles raster app-icons from the canonical vector, favicon.svg.
#
# favicon.svg holds the white paw on a coral (#F2624C) field; its path is the
# SAME PAW_PATH exported from frontend/src/format.js (a frontend test enforces
# this), so every raster below traces back to one source and can't drift.
#
# Run it whenever favicon.svg (or PAW_PATH) changes. Needs rsvg-convert
# (librsvg2-bin) + icotool (icoutils):
#   docker run --rm -v "$PWD/frontend:/fe" debian:trixie-slim sh -c \
#     'apt-get update && apt-get install -y --no-install-recommends librsvg2-bin icoutils && /fe/scripts/gen-icons.sh /fe/public'
set -eu

PUB="${1:-$(CDPATH= cd "$(dirname "$0")/../public" && pwd)}"

# Both the paw path AND the coral field are lifted from the canonical
# favicon.svg, so it is the single source for every raster below.
PAW=$(sed -n 's/.*<path[^d]*d="\([^"]*\)".*/\1/p' "$PUB/favicon.svg")
BRAND=$(sed -n 's/.*<rect[^>]*fill="\([^"]*\)".*/\1/p' "$PUB/favicon.svg")
[ -n "$PAW" ] || { echo "could not read paw path from $PUB/favicon.svg" >&2; exit 1; }
[ -n "$BRAND" ] || { echo "could not read brand color from $PUB/favicon.svg" >&2; exit 1; }

# Build an SVG variant: $1=output file, $2=corner radius, $3=paw scale.
# scale 16 fills the tile (Apple/browser mask the corners); scale 11 keeps the
# paw inside the maskable safe zone so Android's mask never clips it.
variant() {
  cat > "$1" <<EOF
<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 512 512" width="512" height="512">
  <rect width="512" height="512" rx="$2" fill="$BRAND"/>
  <path transform="translate(256 256) scale($3) translate(-12 -11.65)" fill="#FFFFFF" d="$PAW"/>
</svg>
EOF
}

TMP=$(mktemp -d)
trap 'rm -rf "$TMP"' EXIT
variant "$TMP/square.svg"   0  16   # apple-touch: full-bleed, corners masked by iOS
variant "$TMP/maskable.svg" 0  11   # maskable: paw within the safe zone

# Rounded tile (favicon.svg) -> "any"-purpose PNGs + the .ico source sizes.
for s in 192 512; do rsvg-convert -w $s -h $s "$PUB/favicon.svg" -o "$PUB/icon-$s.png"; done
rsvg-convert -w 180 -h 180 "$TMP/square.svg"   -o "$PUB/apple-touch-icon.png"
rsvg-convert -w 512 -h 512 "$TMP/maskable.svg" -o "$PUB/maskable-512.png"

for s in 16 32 48; do rsvg-convert -w $s -h $s "$PUB/favicon.svg" -o "$TMP/f-$s.png"; done
icotool -c -o "$PUB/favicon.ico" "$TMP/f-16.png" "$TMP/f-32.png" "$TMP/f-48.png"

echo "wrote: favicon.ico icon-192.png icon-512.png apple-touch-icon.png maskable-512.png -> $PUB"
