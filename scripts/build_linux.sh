#!/usr/bin/env bash
# Build the Linux desktop app. Run from any directory.
set -euo pipefail
root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
gui="$root/crates/qr_race_gui"
version="$(sed -n 's/^version:[[:space:]]*//p' "$gui/pubspec.yaml" | head -1 | cut -d+ -f1)"
dist="$root/dist/linux"
mkdir -p "$dist"

cargo build --release -p qr_race_ffi -p qr_race_server
(cd "$gui" && flutter pub get && flutter build linux --release)

bundle="$gui/build/linux/x64/release/bundle"
cp "$root/target/release/libqr_race_ffi.so" "$bundle/"
cp "$root/target/release/qr_race_server" "$bundle/"
cp "$root/.refs/ort129-root/lib/libonnxruntime.so" "$bundle/" 2>/dev/null || true
tar -C "$bundle" -czf "$dist/qr_race-$version-linux-x64.tar.gz" .
echo "Linux artifact: $dist/qr_race-$version-linux-x64.tar.gz"

