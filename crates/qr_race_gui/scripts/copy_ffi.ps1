# Copies the native scanner next to the Flutter executable so the packaged app
# finds it without the dev-only target/release fallback.
#
#   cargo build -p qr_race_ffi --release
#   pwsh crates/qr_race_gui/scripts/copy_ffi.ps1 -Config Release

param(
    [ValidateSet("Debug", "Release", "Profile")]
    [string]$Config = "Release"
)

$root = Resolve-Path (Join-Path $PSScriptRoot "..\..\..")
$dll = Join-Path $root "target\release\qr_race_ffi.dll"
$destDir = Join-Path $root "crates\qr_race_gui\build\windows\x64\runner\$Config"

if (-not (Test-Path $dll)) {
    Write-Error "missing $dll - run: cargo build -p qr_race_ffi --release"
    exit 1
}
if (-not (Test-Path $destDir)) {
    Write-Error "missing $destDir - build the Flutter app first"
    exit 1
}

Copy-Item $dll (Join-Path $destDir "qr_race_ffi.dll") -Force
Write-Host "copied qr_race_ffi.dll -> $destDir"
