# Regression gate. Cargo.lock is part of the pin: --locked refuses to resolve
# a different graph, and every test runs against the exact locked versions.
param(
    [switch]$SkipFlutter
)

$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
Push-Location $root
try {
    $ort = Join-Path $root '.refs\ort129-root\lib\onnxruntime.dll'
    if (Test-Path $ort) {
        $env:ORT_DYLIB_PATH = $ort
        Write-Host "ORT_DYLIB_PATH=$ort" -ForegroundColor DarkGray
    }

    Write-Host '[1/3] Cargo.lock is current' -ForegroundColor Yellow
    cargo metadata --locked --no-deps | Out-Null
    if ($LASTEXITCODE -ne 0) { throw 'Cargo.lock is stale; run cargo update and review the diff' }

    Write-Host '[2/3] Rust workspace (locked)' -ForegroundColor Yellow
    cargo test --workspace --locked
    if ($LASTEXITCODE -ne 0) { throw 'Rust tests failed' }

    if (-not $SkipFlutter) {
        Write-Host '[3/3] Flutter analyze + test' -ForegroundColor Yellow
        Push-Location (Join-Path $root 'crates\qr_race_gui')
        try {
            flutter analyze
            if ($LASTEXITCODE -ne 0) { throw 'flutter analyze failed' }
            flutter test
            if ($LASTEXITCODE -ne 0) { throw 'flutter test failed' }
        } finally {
            Pop-Location
        }
    }
    Write-Host 'verify: ok' -ForegroundColor Green
} finally {
    Pop-Location
}
