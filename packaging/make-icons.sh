#!/bin/sh
# Rasterises assets/icon.svg into assets/icon.icns (macOS) and assets/icon-256.png (window icon).
# Needs rsvg-convert (brew install librsvg) and iconutil (ships with macOS).
set -eu
cd "$(dirname "$0")/.."
tmp=$(mktemp -d)
set="$tmp/icon.iconset"
mkdir -p "$set"
for s in 16 32 128 256 512; do
  rsvg-convert -w $s -h $s assets/icon.svg -o "$set/icon_${s}x${s}.png"
  d=$((s * 2))
  rsvg-convert -w $d -h $d assets/icon.svg -o "$set/icon_${s}x${s}@2x.png"
done
iconutil -c icns "$set" -o assets/icon.icns
cp "$set/icon_256x256.png" assets/icon-256.png
rm -rf "$tmp"
echo "wrote assets/icon.icns and assets/icon-256.png"
