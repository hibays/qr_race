# Build the Android Rust FFI for one ABI. Native binaries are output to
# crates/qr_race_gui/android/app/src/main/jniLibs/<abi>/ and are git-ignored.
param(
    [Parameter(Mandatory = $true)]
    [ValidateSet("arm64-v8a", "armeabi-v7a", "x86_64")]
    [string]$Abi,
    [switch]$Clean
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

$target, $clangName, $cargoEnv = switch ($Abi) {
    "arm64-v8a" { "aarch64-linux-android", "aarch64-linux-android24-clang.cmd", "AARCH64_LINUX_ANDROID" }
    "armeabi-v7a" { "armv7-linux-androideabi", "armv7a-linux-androideabi24-clang.cmd", "ARMV7_LINUX_ANDROIDEABI" }
    "x86_64" { "x86_64-linux-android", "x86_64-linux-android24-clang.cmd", "X86_64_LINUX_ANDROID" }
}

$toolchain = Join-Path $ndk.FullName "toolchains\llvm\prebuilt\windows-x86_64\bin"
$clang = Join-Path $toolchain $clangName
if (-not (Test-Path $clang)) { throw "NDK clang not found: $clang" }
$envName = $target.Replace("-", "_")
Set-Item -Path "Env:CC_$envName" -Value $clang
Set-Item -Path "Env:AR_$envName" -Value (Join-Path $toolchain "llvm-ar.exe")
Set-Item -Path "Env:CARGO_TARGET_$($cargoEnv)_LINKER" -Value $clang

$zxingSource = Join-Path $root ".refs\zxing-cpp-src"
if (-not (Test-Path (Join-Path $zxingSource "CMakeLists.txt"))) {
    throw "zxing-cpp source missing; run scripts/fetch-android-deps.ps1"
}
# zxing-cpp passes -Wl,-map; lld only accepts -Wl,-Map on Android.
$zxingCmake = Join-Path $zxingSource "CMakeLists.txt"
if (Test-Path $zxingCmake) {
    $body = Get-Content $zxingCmake -Raw
    if ($body -match "-Wl,-map,") {
        $body = $body.Replace("-Wl,-map,", "-Wl,-Map=")
        Set-Content $zxingCmake -Value $body -Encoding utf8
    }
}
$zxingBuild = Join-Path $root ".refs\zxing-android-$Abi"
$zxingLib = Join-Path $zxingBuild "libZXing.so"
if ($Clean -or -not (Test-Path $zxingLib)) {
    if (Test-Path $zxingBuild) { Remove-Item $zxingBuild -Recurse -Force }
    $toolchainFile = Join-Path $ndk.FullName "build\cmake\android.toolchain.cmake"
    & cmake -S $zxingSource -B $zxingBuild -G Ninja `
        "-DCMAKE_TOOLCHAIN_FILE=$toolchainFile" "-DANDROID_ABI=$Abi" `
        "-DANDROID_PLATFORM=android-24" "-DBUILD_SHARED_LIBS=ON" `
        "-DZXING_READERS=ON" "-DZXING_WRITERS=ON" "-DZXING_WRITERS_OLD=OFF" "-DZXING_C_API=ON" `
        "-DZXING_USE_BUNDLED_ZINT=ON" "-DCMAKE_BUILD_TYPE=Release"
    if ($LASTEXITCODE -ne 0) { throw "zxing-cpp cmake configure failed" }
    & cmake --build $zxingBuild -j 8
    if ($LASTEXITCODE -ne 0) { throw "zxing-cpp build failed" }
}
$env:ZXING_CPP_LIB_DIR = $zxingBuild

$ortRoot = Join-Path $root ".refs\ort-android-1.29.0\$Abi"
if (-not (Test-Path (Join-Path $ortRoot "lib\libonnxruntime.so"))) {
    throw "ONNX Runtime 1.29 $Abi lib missing; run scripts/fetch-android-deps.ps1"
}
$env:ST_ZRT_ORT_PATH = $ortRoot

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
Copy-Item $zxingLib $dest -Force
Copy-Item (Join-Path $ortRoot "lib\libonnxruntime.so") $dest -Force
$cxxAbi = switch ($Abi) {
    "arm64-v8a" { "aarch64-linux-android" }
    "armeabi-v7a" { "arm-linux-androideabi" }
    "x86_64" { "x86_64-linux-android" }
}
$cxx = Join-Path $ndk.FullName "toolchains\llvm\prebuilt\windows-x86_64\sysroot\usr\lib\$cxxAbi\libc++_shared.so"
if (Test-Path $cxx) { Copy-Item $cxx $dest -Force }
Write-Host "packaged $dest (ffi, ZXing, onnxruntime, libc++)" -ForegroundColor Green





