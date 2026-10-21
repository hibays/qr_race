# Build the Flutter Web bundle and package it as a ZIP.
param()
$ErrorActionPreference = "Stop"
$root = Split-Path -Parent $PSScriptRoot
$gui = Join-Path $root "crates\qr_race_gui"
$dist = Join-Path $root "dist\web"
$version = ((Get-Content (Join-Path $gui "pubspec.yaml") | Select-String '^version:\s*(.+)$').Matches.Groups[1].Value).Split('+')[0]
New-Item -ItemType Directory -Force -Path $dist | Out-Null
& (Join-Path $PSScriptRoot "build_lele_wasm.ps1")

Push-Location $gui
try {
    flutter pub get
    if ($LASTEXITCODE -ne 0) { throw "flutter pub get failed" }
    flutter build web --release
    if ($LASTEXITCODE -ne 0) { throw "flutter build web failed" }
} finally {
    Pop-Location
}

$web = Join-Path $gui "build\web"
$zip = Join-Path $dist "qr_race-$version-web.zip"
if (Test-Path $zip) { Remove-Item $zip -Force }
Compress-Archive -Path (Join-Path $web "*") -DestinationPath $zip -CompressionLevel Optimal
Write-Host ("Web artifact: {0} ({1:N1} MB)" -f $zip, ((Get-Item $zip).Length / 1MB)) -ForegroundColor Green

