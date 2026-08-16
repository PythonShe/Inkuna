#!/usr/bin/env bash
# Regenerate raster icon assets from the brand sources in assets/brand/.
# Requires librsvg (rsvg-convert) and imagemagick (magick): brew install librsvg imagemagick
#
# The iOS Liquid Glass icon (apps/ios/AppIcon.icon) is authored in Icon
# Composer from assets/brand/ios-icon-layers/ and is NOT generated here.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
BRAND="$ROOT/assets/brand"
FLAT="$BRAND/appicon-flat.svg"
MARK="$BRAND/inkuna-mark.svg"
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT

# Legacy iOS app icon (iOS 18-25 fallback; App Store requires no alpha).
rsvg-convert -w 1024 -h 1024 "$FLAT" -o "$TMP/ios-1024.png"
magick "$TMP/ios-1024.png" -alpha off \
  "$ROOT/apps/ios/Inkuna/Assets.xcassets/AppIcon.appiconset/AppIcon-1024.png"

# Play Store listing icon (not shipped in the APK).
rsvg-convert -w 512 -h 512 "$FLAT" -o "$TMP/play-512.png"
magick "$TMP/play-512.png" -alpha off "$BRAND/play-store-512.png"

# Website: homepage beta callout uses the Liquid Glass render (exported from
# Icon Composer to appicon-glass-256.png), downscaled for a 3rem display size.
magick "$BRAND/appicon-glass-256.png" -resize 144x144 -depth 8 -strip \
  "$ROOT/website/public/appicon-glass.png"

# Website: apple-touch-icon (full-bleed opaque) + favicon.ico (transparent mark).
rsvg-convert -w 180 -h 180 "$FLAT" -o "$TMP/touch-180.png"
magick "$TMP/touch-180.png" -alpha off "$ROOT/website/public/apple-touch-icon.png"
rsvg-convert -w 16 -h 16 "$MARK" -o "$TMP/fav-16.png"
rsvg-convert -w 32 -h 32 "$MARK" -o "$TMP/fav-32.png"
magick "$TMP/fav-16.png" "$TMP/fav-32.png" "$ROOT/website/public/favicon.ico"

echo "Icons regenerated."
