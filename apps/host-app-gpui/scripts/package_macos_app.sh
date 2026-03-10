#!/usr/bin/env bash
set -euo pipefail

if [[ "$(uname -s)" != "Darwin" ]]; then
  printf 'This script only supports macOS.\n' >&2
  exit 1
fi

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
APP_DIR="$ROOT_DIR/apps/host-app-gpui"
DIST_DIR="$APP_DIR/dist"

APP_NAME="${APP_NAME:-AI Meeting Host}"
APP_VERSION="${APP_VERSION:-0.1.0}"
APP_BUILD_NUMBER="${APP_BUILD_NUMBER:-$APP_VERSION}"
BUNDLE_IDENTIFIER="${BUNDLE_IDENTIFIER:-com.liuscraft.ai-meeting-host}"
CODESIGN_IDENTITY="${CODESIGN_IDENTITY:--}"

BUNDLE_DIR="$DIST_DIR/$APP_NAME.app"
CONTENTS_DIR="$BUNDLE_DIR/Contents"
MACOS_DIR="$CONTENTS_DIR/MacOS"
RESOURCES_DIR="$CONTENTS_DIR/Resources"

BINARY_PATH="$ROOT_DIR/target/release/host-app-gpui"
ICON_PATH="$APP_DIR/assets/icons/app-taskbar-logo.icns"

if [[ ! -f "$ICON_PATH" ]]; then
  printf 'Missing icon file: %s\n' "$ICON_PATH" >&2
  exit 1
fi

cargo build -p host-app-gpui --release --manifest-path "$ROOT_DIR/Cargo.toml"

rm -rf "$BUNDLE_DIR"
mkdir -p "$MACOS_DIR" "$RESOURCES_DIR"

cp "$BINARY_PATH" "$MACOS_DIR/host-app-gpui"
chmod +x "$MACOS_DIR/host-app-gpui"
cp "$ICON_PATH" "$RESOURCES_DIR/app-taskbar-logo.icns"

cat > "$CONTENTS_DIR/Info.plist" <<EOF
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleDevelopmentRegion</key>
  <string>en</string>
  <key>CFBundleDisplayName</key>
  <string>$APP_NAME</string>
  <key>CFBundleExecutable</key>
  <string>host-app-gpui</string>
  <key>CFBundleIconFile</key>
  <string>app-taskbar-logo.icns</string>
  <key>CFBundleIdentifier</key>
  <string>$BUNDLE_IDENTIFIER</string>
  <key>CFBundleInfoDictionaryVersion</key>
  <string>6.0</string>
  <key>CFBundleName</key>
  <string>$APP_NAME</string>
  <key>CFBundlePackageType</key>
  <string>APPL</string>
  <key>CFBundleShortVersionString</key>
  <string>$APP_VERSION</string>
  <key>CFBundleVersion</key>
  <string>$APP_BUILD_NUMBER</string>
  <key>NSMicrophoneUsageDescription</key>
  <string>AI Meeting Host needs microphone access to capture meeting audio.</string>
  <key>LSMinimumSystemVersion</key>
  <string>12.0</string>
  <key>NSHighResolutionCapable</key>
  <true/>
</dict>
</plist>
EOF

codesign --force --deep --sign "$CODESIGN_IDENTITY" "$BUNDLE_DIR"
codesign --verify --deep --strict --verbose=2 "$BUNDLE_DIR"

printf 'Signed app bundle with identity: %s\n' "$CODESIGN_IDENTITY"
if [[ "$CODESIGN_IDENTITY" == "-" ]]; then
  printf 'Note: ad-hoc signature is used; notarization is still required for smooth Gatekeeper launch on other Macs.\n'
fi

printf 'Created app bundle: %s\n' "$BUNDLE_DIR"
