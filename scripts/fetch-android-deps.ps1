# Fetch the Android native inputs that are not committed:
#   * ONNX Runtime 1.29 AAR (headers + jni libs for arm64-v8a/armeabi-v7a/x86_64)
#   * zxing-cpp source tree for the NDK CMake build
# Both land under .refs/ and are consumed by build-android-ffi.ps1.
param(
    [string]$OrtVersion = "1.29.0"
)

$ErrorActionPreference = "Stop"
$root = Split-Path -Parent $PSScriptRoot
$downloads = Join-Path $root ".refs\downloads"
$ortRoot = Join-Path $root ".refs\ort-android-$OrtVersion"
New-Item -ItemType Directory -Force -Path $downloads, $ortRoot | Out-Null

$aar = Join-Path $downloads "onnxruntime-android-$OrtVersion.aar"
if (-not (Test-Path $aar) -or (Get-Item $aar).Length -eq 0) {
    $url = "https://repo1.maven.org/maven2/com/microsoft/onnxruntime/onnxruntime-android/$OrtVersion/onnxruntime-android-$OrtVersion.aar"
    Write-Host "Downloading ONNX Runtime $OrtVersion AAR..." -ForegroundColor Yellow
    Invoke-WebRequest -Uri $url -OutFile $aar -UseBasicParsing
}

$extract = Join-Path $ortRoot "aar"
if (-not (Test-Path (Join-Path $extract "headers\onnxruntime_c_api.h"))) {
    if (Test-Path $extract) { Remove-Item $extract -Recurse -Force }
    New-Item -ItemType Directory -Force -Path $extract | Out-Null
    tar -xf $aar -C $extract
}

foreach ($abi in @("arm64-v8a", "armeabi-v7a", "x86_64")) {
    $abiRoot = Join-Path $ortRoot $abi
    $include = Join-Path $abiRoot "include"
    $lib = Join-Path $abiRoot "lib"
    New-Item -ItemType Directory -Force -Path $include, $lib | Out-Null
    Copy-Item (Join-Path $extract "headers\*") $include -Force
    $so = Join-Path $extract "jni\$abi\libonnxruntime.so"
    if (-not (Test-Path $so)) { throw "ONNX Runtime AAR has no $abi library" }
    Copy-Item $so (Join-Path $lib "libonnxruntime.so") -Force
}

$zxingSource = Join-Path $root ".refs\zxing-cpp-src"
if (-not (Test-Path (Join-Path $zxingSource "src\Barcode.cpp")) -or -not (Test-Path (Join-Path $zxingSource "..\zint\backend\zint.h"))) {
    Write-Host "Fetching zxing-cpp 0.5.3 source (core + zint)..." -ForegroundColor Yellow
    $crate = Join-Path $downloads "zxing-cpp-0.5.3.crate"
    if (-not (Test-Path $crate)) {
        Invoke-WebRequest -Uri "https://crates.io/api/v1/crates/zxing-cpp/0.5.3/download" -OutFile $crate -UseBasicParsing
    }
    $tmp = Join-Path $ortRoot "zxing-crate"
    if (Test-Path $tmp) { Remove-Item $tmp -Recurse -Force }
    New-Item -ItemType Directory -Force -Path $tmp | Out-Null
    tar -xf $crate -C $tmp
    $crateRoot = Join-Path $tmp "zxing-cpp-0.5.3"
    $core = Join-Path $crateRoot "core"
    if (-not (Test-Path (Join-Path $core "CMakeLists.txt"))) { throw "zxing-cpp crate has no core/CMakeLists.txt" }
    if (Test-Path $zxingSource) { Remove-Item $zxingSource -Recurse -Force }
    New-Item -ItemType Directory -Force -Path $zxingSource | Out-Null
    Copy-Item (Join-Path $core "*") $zxingSource -Recurse
    Copy-Item (Join-Path $crateRoot "zint") (Join-Path $zxingSource "..\zint") -Recurse -Force
    $cmake = Join-Path $zxingSource "CMakeLists.txt"
    $body = Get-Content $cmake -Raw
    $body = $body.Replace("-Wl,-map,", "-Wl,-Map=")
    Set-Content $cmake -Value $body -Encoding utf8
}

Write-Host "Android inputs ready under .refs:" -ForegroundColor Green
Write-Host "  ONNX Runtime: $ortRoot" -ForegroundColor Gray
Write-Host "  zxing-cpp:    $zxingSource" -ForegroundColor Gray



