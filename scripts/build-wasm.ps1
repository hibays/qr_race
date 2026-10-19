# Builds the wasm scan core and its JS glue for the Flutter web app.
#
#   pwsh -File scripts/build-wasm.ps1
#
# Run this before `flutter build web`; the generated files live in
# crates/qr_race_gui/web/scan/.

$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot

cargo build -p qr_race_scan_wasm --target wasm32-unknown-unknown --release
if ($LASTEXITCODE -ne 0) { throw 'wasm build failed' }

$wasm = Join-Path $root 'target/wasm32-unknown-unknown/release/qr_race_scan_wasm.wasm'
$webOut = Join-Path $root 'crates/qr_race_gui/web/scan'
wasm-bindgen $wasm --target web --out-dir $webOut
if ($LASTEXITCODE -ne 0) { throw 'wasm-bindgen (web) failed' }

$nodeOut = Join-Path $root '.refs/wasm-node'
wasm-bindgen $wasm --target nodejs --out-dir $nodeOut
if ($LASTEXITCODE -ne 0) { throw 'wasm-bindgen (nodejs) failed' }

Write-Host "web glue:  $webOut"
Write-Host "node glue: $nodeOut"
