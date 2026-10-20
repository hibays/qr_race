#!/usr/bin/env bash
# Build the macOS desktop app. Requires Xcode command line tools and Flutter macOS.
set -euo pipefail
root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
gui="$root/crates/qr_race_gui"
version="$(sed -n 's/^version:[[:space:]]*//p' "$gui/pubspec.yaml" | head -1 | cut -d+ -f1)"
dist="$root/dist/macos"
mkdir -p "$dist"

cargo build --release -p qr_race_ffi -p qr_race_server
(cd "$gui" && flutter pub get && flutter build macos --release)

app="$gui/build/macos/Build/Products/Release/qr_race_gui.app"
cp "$root/target/release/libqr_race_ffi.dylib" "$app/Contents/Frameworks/" 2>/dev/null || true
cp "$root/target/release/qr_race_server" "$app/Contents/MacOS/"
ditto -c -k --keepParent "$app" "$dist/qr_race-$version-macos.zip"
echo "macOS artifact: $dist/qr_race-$version-macos.zip"
