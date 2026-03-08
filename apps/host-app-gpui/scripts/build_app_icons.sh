#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
APP_DIR="$ROOT_DIR/apps/host-app-gpui"
SVG_PATH="$APP_DIR/assets/svg/app-taskbar-logo.svg"
ICON_DIR="$APP_DIR/assets/icons"
ICONSET_DIR="$ICON_DIR/app-taskbar-logo.iconset"

if [[ ! -f "$SVG_PATH" ]]; then
  printf 'Missing source SVG: %s\n' "$SVG_PATH" >&2
  exit 1
fi

if ! command -v rsvg-convert >/dev/null 2>&1; then
  printf 'rsvg-convert not found. Install librsvg first.\n' >&2
  exit 1
fi

mkdir -p "$ICON_DIR" "$ICONSET_DIR"

for size in 16 24 32 48 64 128 256 512 1024; do
  rsvg-convert -w "$size" -h "$size" "$SVG_PATH" -o "$ICON_DIR/app-taskbar-logo-${size}.png"
done

cp "$ICON_DIR/app-taskbar-logo-16.png" "$ICONSET_DIR/icon_16x16.png"
cp "$ICON_DIR/app-taskbar-logo-32.png" "$ICONSET_DIR/icon_16x16@2x.png"
cp "$ICON_DIR/app-taskbar-logo-32.png" "$ICONSET_DIR/icon_32x32.png"
cp "$ICON_DIR/app-taskbar-logo-64.png" "$ICONSET_DIR/icon_32x32@2x.png"
cp "$ICON_DIR/app-taskbar-logo-128.png" "$ICONSET_DIR/icon_128x128.png"
cp "$ICON_DIR/app-taskbar-logo-256.png" "$ICONSET_DIR/icon_128x128@2x.png"
cp "$ICON_DIR/app-taskbar-logo-256.png" "$ICONSET_DIR/icon_256x256.png"
cp "$ICON_DIR/app-taskbar-logo-512.png" "$ICONSET_DIR/icon_256x256@2x.png"
cp "$ICON_DIR/app-taskbar-logo-512.png" "$ICONSET_DIR/icon_512x512.png"
cp "$ICON_DIR/app-taskbar-logo-1024.png" "$ICONSET_DIR/icon_512x512@2x.png"

if command -v iconutil >/dev/null 2>&1; then
  iconutil -c icns "$ICONSET_DIR" -o "$ICON_DIR/app-taskbar-logo.icns"
else
  printf 'iconutil not found, skipped .icns generation.\n' >&2
fi

if command -v python3 >/dev/null 2>&1; then
  if python3 -c "from PIL import Image" >/dev/null 2>&1; then
    python3 -c "from pathlib import Path; from PIL import Image; base=Path(r'${ICON_DIR}'); src=Image.open(base/'app-taskbar-logo-1024.png').convert('RGBA'); src.save(base/'app-taskbar-logo.ico', format='ICO', sizes=[(16,16),(24,24),(32,32),(48,48),(64,64),(128,128),(256,256)])"
  else
    printf 'Pillow not found, skipped .ico generation.\n' >&2
  fi
else
  printf 'python3 not found, skipped .ico generation.\n' >&2
fi

printf 'Generated app icons under: %s\n' "$ICON_DIR"
