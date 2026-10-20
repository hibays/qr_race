# Build every artifact this Windows host can produce.
# Linux/macOS use the shell scripts in this directory.
param(
    [switch]$SkipAndroid,
    [switch]$SkipWindows,
    [switch]$SkipWeb
)
$ErrorActionPreference = "Stop"
$root = Split-Path -Parent $PSScriptRoot

if (-not $SkipAndroid) {
    & (Join-Path $PSScriptRoot "build_android.ps1")
    if ($LASTEXITCODE -ne 0) { throw "Android packaging failed" }
}
if (-not $SkipWindows) {
    & (Join-Path $PSScriptRoot "build_windows.ps1")
    if ($LASTEXITCODE -ne 0) { throw "Windows packaging failed" }
}
if (-not $SkipWeb) {
    & (Join-Path $PSScriptRoot "build_web.ps1")
    if ($LASTEXITCODE -ne 0) { throw "Web packaging failed" }
}
Write-Host "Artifacts under $root\dist" -ForegroundColor Cyan
