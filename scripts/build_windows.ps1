# Build and package the Windows x64 desktop app.
# Rust FFI + server are built first, then Flutter, then the final portable ZIP.
param(
    [switch]$DebugBuild
)

$ErrorActionPreference = "Stop"
$root = Split-Path -Parent $PSScriptRoot
$gui = Join-Path $root "crates\qr_race_gui"
$dist = Join-Path $root "dist\windows"
$version = ((Get-Content (Join-Path $gui "pubspec.yaml") | Select-String '^version:\s*(.+)$').Matches.Groups[1].Value).Split('+')[0]
$mode = if ($DebugBuild) { "debug" } else { "release" }
$runner = if ($DebugBuild) { "Debug" } else { "Release" }
New-Item -ItemType Directory -Force -Path $dist | Out-Null

Write-Host "[1/3] Building Rust FFI + server ($mode)..." -ForegroundColor Yellow
Push-Location $root
try {
    if ($DebugBuild) {
        & cargo build -p qr_race_ffi -p qr_race_server
    } else {
        & cargo build --release -p qr_race_ffi -p qr_race_server
    }
    if ($LASTEXITCODE -ne 0) { throw "cargo build failed" }
} finally {
    Pop-Location
}

$profileDir = if ($DebugBuild) { "debug" } else { "release" }
$dll = Join-Path $root "target\$profileDir\qr_race_ffi.dll"
$server = Join-Path $root "target\$profileDir\qr_race_server.exe"
if (-not (Test-Path $dll)) { throw "missing $dll" }
if (-not (Test-Path $server)) { throw "missing $server" }

Write-Host "[2/3] Building Flutter Windows ($mode)..." -ForegroundColor Yellow
Push-Location $gui
try {
    flutter build windows --$mode
    if ($LASTEXITCODE -ne 0) { throw "flutter build windows failed" }
} finally {
    Pop-Location
}

$releaseDir = Join-Path $gui "build\windows\x64\runner\$runner"
if (-not (Test-Path (Join-Path $releaseDir "qr_race_gui.exe"))) {
    throw "missing Flutter Windows runner"
}
Copy-Item $dll $releaseDir -Force
Copy-Item $server $releaseDir -Force
$ortDll = Join-Path $root ".refs/ort129-root/lib/onnxruntime.dll"
if (-not (Test-Path $ortDll)) {
    throw "onnxruntime.dll 1.29 missing at $ortDll; run scripts/bench_backends.ps1 once or download the ORT NuGet package"
}
Copy-Item $ortDll $releaseDir -Force

Write-Host "[3/3] Packaging portable ZIP..." -ForegroundColor Yellow
$zip = Join-Path $dist "qr_race-$version-windows-x64-$mode.zip"
if (Test-Path $zip) { Remove-Item $zip -Force }
Compress-Archive -Path (Join-Path $releaseDir "*") -DestinationPath $zip -CompressionLevel Optimal
Write-Host ("  -> {0} ({1:N1} MB)" -f $zip, ((Get-Item $zip).Length / 1MB)) -ForegroundColor Green
Write-Host "Windows artifact: $zip" -ForegroundColor Cyan


