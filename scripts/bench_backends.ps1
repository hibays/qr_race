# Compare raw detect/SR inference of ort 2.0.0-rc.15 against st-zrt 0.4.1.
# Both tools use the same ONNX Runtime 1.29 DLL; numbers are single-thread,
# zero-filled inputs, 20 warmup + 200 measured runs.
param(
    [string]$OrtDll = ""
)
$ErrorActionPreference = "Stop"
$root = Split-Path -Parent $PSScriptRoot
if (-not $OrtDll) { $OrtDll = Join-Path $root ".refs/ort129-root/lib/onnxruntime.dll" }

$ortRoot = Join-Path $root ".refs/ort129-root"
if (-not (Test-Path $OrtDll)) {
    Write-Host "Fetching ONNX Runtime 1.29 Windows package..." -ForegroundColor Yellow
    $nupkg = Join-Path $root ".refs/ort129.nupkg"
    if (-not (Test-Path $nupkg)) {
        Invoke-WebRequest "https://www.nuget.org/api/v2/package/Microsoft.ML.OnnxRuntime/1.29.0" -OutFile $nupkg -UseBasicParsing
    }
    $extract = Join-Path $root ".refs/ort129"
    if (-not (Test-Path (Join-Path $extract "build/native/include/onnxruntime_c_api.h"))) {
        if (Test-Path $extract) { Remove-Item $extract -Recurse -Force }
        New-Item -ItemType Directory $extract | Out-Null
        tar -xf $nupkg -C $extract
    }
    New-Item -ItemType Directory "$ortRoot/include","$ortRoot/lib" -Force | Out-Null
    Copy-Item (Join-Path $extract "build/native/include/*") "$ortRoot/include" -Force
    Copy-Item (Join-Path $extract "runtimes/win-x64/native/onnxruntime.lib") "$ortRoot/lib" -Force
    Copy-Item (Join-Path $extract "runtimes/win-x64/native/onnxruntime.dll") "$ortRoot/lib" -Force
}

$env:ORT_DYLIB_PATH = (Resolve-Path $OrtDll).Path
$env:ST_ZRT_ORT_PATH = (Resolve-Path (Join-Path $root ".refs/ort129-root")).Path
$stzrtDir = Join-Path $root "tools/backend_bench/stzrt"
Push-Location $stzrtDir
try {
    cargo build --release
    if ($LASTEXITCODE -ne 0) { throw "st-zrt benchmark build failed" }
    Copy-Item $OrtDll (Join-Path $stzrtDir "target/release/onnxruntime.dll") -Force
} finally {
    Pop-Location
}

Write-Host "== ort 2.0.0-rc.15 ==" -ForegroundColor Cyan
cargo run --release --manifest-path (Join-Path $root "tools/backend_bench/ort/Cargo.toml")
Write-Host "== st-zrt 0.4.1 ==" -ForegroundColor Cyan
Push-Location $stzrtDir
try {
    cargo run --release
} finally {
    Pop-Location
}


