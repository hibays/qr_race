param(
    [string]$Abi = "arm64-v8a"
)

$ErrorActionPreference = "Stop"
$root = Split-Path -Parent $PSScriptRoot

$sdkCandidates = @($env:ANDROID_HOME, $env:ANDROID_SDK_ROOT, "D:\Dev\Android\Sdk") |
    Where-Object { $_ }
$sdk = $sdkCandidates | Where-Object { Test-Path $_ } | Select-Object -First 1
if (-not $sdk) { throw "Android SDK not found; set ANDROID_HOME" }

$ndk = Get-ChildItem (Join-Path $sdk "ndk") -Directory -ErrorAction SilentlyContinue |
    Sort-Object Name | Select-Object -Last 1
if (-not $ndk) { throw "Android NDK not found under $sdk\ndk" }

$target = switch ($Abi) {
    "arm64-v8a" { "aarch64-linux-android" }
    "armeabi-v7a" { "armv7-linux-androideabi" }
    "x86_64" { "x86_64-linux-android" }
    default { throw "unsupported ABI $Abi" }
}
$toolchain = Join-Path $ndk.FullName "toolchains\llvm\prebuilt\windows-x86_64\bin"
$clang = Join-Path $toolchain "aarch64-linux-android21-clang.cmd"
if ($target -ne "aarch64-linux-android") {
    $prefix = switch ($target) {
        "armv7-linux-androideabi" { "armv7a-linux-androideabi21-clang.cmd" }
        "x86_64-linux-android" { "x86_64-linux-android21-clang.cmd" }
    }
    $clang = Join-Path $toolchain $prefix
}
if (-not (Test-Path $clang)) { throw "NDK clang not found: $clang" }

$env:CARGO_TARGET_AARCH64_LINUX_ANDROID_LINKER = $clang
$env:CC_aarch64_linux_android = $clang
$env:AR_aarch64_linux_android = Join-Path $toolchain "llvm-ar.exe"
$env:CC_armv7_linux_androideabi = $clang
$env:AR_armv7_linux_androideabi = Join-Path $toolchain "llvm-ar.exe"
$env:CC_x86_64_linux_android = $clang
$env:AR_x86_64_linux_android = Join-Path $toolchain "llvm-ar.exe"
if ($target -eq "aarch64-linux-android") {
    $env:CARGO_TARGET_AARCH64_LINUX_ANDROID_LINKER = $clang
}

Write-Host "building qr_race_ffi for $target ($Abi)..." -ForegroundColor Cyan
Push-Location $root
try {
    & cargo build --release --target $target -p qr_race_ffi --no-default-features
    if ($LASTEXITCODE -ne 0) { throw "cargo build failed" }
} finally {
    Pop-Location
}

$source = Join-Path $root "target\$target\release\libqr_race_ffi.so"
if (-not (Test-Path $source)) { throw "missing $source" }
$dest = Join-Path $root "crates\qr_race_gui\android\app\src\main\jniLibs\$Abi"
New-Item -ItemType Directory -Force -Path $dest | Out-Null
Copy-Item $source $dest -Force
Write-Host "packaged $dest\libqr_race_ffi.so" -ForegroundColor Green
