# Build the lele WASM detector used by web/scan/camera_scan.js.
$ErrorActionPreference = "Stop"
$root = Split-Path -Parent $PSScriptRoot
$out = Join-Path $root "crates/qr_race_gui/web/scan/lele"
cargo build -p qr_race_detect_lele --target wasm32-unknown-unknown --features wasm --release
if ($LASTEXITCODE -ne 0) { throw "qr_race_detect_lele wasm build failed" }
New-Item -ItemType Directory $out -Force | Out-Null
wasm-bindgen --target web --out-dir $out --no-typescript `
    (Join-Path $root "target/wasm32-unknown-unknown/release/qr_race_detect_lele.wasm")
if ($LASTEXITCODE -ne 0) { throw "wasm-bindgen failed" }
Write-Host "lele wasm -> $out"
