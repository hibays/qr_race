# Build all Android ABIs and produce split APKs (one APK per ABI).
# Mirrors the rcrm pipeline: native Rust first, then Flutter split packaging.
param(
    [switch]$DebugBuild,
    [string[]]$Abis = @("arm64-v8a", "armeabi-v7a", "x86_64"),
    [string]$GradleDistribution = ""
)

$ErrorActionPreference = "Stop"
$root = Split-Path -Parent $PSScriptRoot
$gui = Join-Path $root "crates\qr_race_gui"
$dist = Join-Path $root "dist\android"
$version = ((Get-Content (Join-Path $gui "pubspec.yaml") | Select-String '^version:\s*(.+)$').Matches.Groups[1].Value).Split('+')[0]
New-Item -ItemType Directory -Force -Path $dist | Out-Null

$mode = if ($DebugBuild) { "debug" } else { "release" }
$gradleProps = Join-Path $gui "android\gradle\wrapper\gradle-wrapper.properties"
$gradleOriginal = if (Test-Path $gradleProps) { Get-Content $gradleProps -Raw } else { $null }

Write-Host "[1/4] Fetching Android native inputs..." -ForegroundColor Yellow
& (Join-Path $PSScriptRoot "fetch-android-deps.ps1")

Write-Host "[2/4] Building Rust FFI for $($Abis -join ', ')..." -ForegroundColor Yellow
foreach ($abi in $Abis) {
    & (Join-Path $PSScriptRoot "build-android-ffi.ps1") -Abi $abi
    if ($LASTEXITCODE -ne 0) { throw "FFI build failed for $abi" }
}

# Gradle can keep stale jni merge state after replacing .so files.
foreach ($stale in @(
        (Join-Path $gui "build\app\intermediates\merged_jni_libs"),
        (Join-Path $gui "build\app\intermediates\merged_native_libs"))) {
    if (Test-Path $stale) { Remove-Item $stale -Recurse -Force }
}

if ($GradleDistribution) {
    $uri = "file:///$($GradleDistribution.Replace('\','/'))"
    (Get-Content $gradleProps -Raw) -replace 'distributionUrl=.*', "distributionUrl=$uri" |
        Set-Content $gradleProps -Encoding utf8
    Write-Host "  using local Gradle distribution: $GradleDistribution" -ForegroundColor Gray
}

try {
    Write-Host "[3/4] Flutter pub get + split APK ($mode)..." -ForegroundColor Yellow
    Push-Location $gui
    flutter pub get
    if ($LASTEXITCODE -ne 0) { throw "flutter pub get failed" }
    flutter build apk --$mode --split-per-abi
    if ($LASTEXITCODE -ne 0) { throw "flutter build apk failed" }
    Pop-Location

    Write-Host "[4/4] Copying APKs to dist\android..." -ForegroundColor Yellow
    $apkDir = Join-Path $gui "build\app\outputs\flutter-apk"
    $map = @{
        "arm64-v8a" = "app-arm64-v8a-$mode.apk"
        "armeabi-v7a" = "app-armeabi-v7a-$mode.apk"
        "x86_64" = "app-x86_64-$mode.apk"
    }
    foreach ($abi in $Abis) {
        $src = Join-Path $apkDir $map[$abi]
        if (-not (Test-Path $src)) { throw "missing split APK: $src" }
        $dest = Join-Path $dist "qr_race-$version-$abi-$mode.apk"
        Copy-Item $src $dest -Force
        Write-Host ("  -> {0} ({1:N1} MB)" -f $dest, ((Get-Item $dest).Length / 1MB)) -ForegroundColor Green
    }
} finally {
    Pop-Location -ErrorAction SilentlyContinue
    if ($gradleOriginal -ne $null) { Set-Content $gradleProps -Value $gradleOriginal -Encoding utf8 }
}

Write-Host "Android artifacts: $dist" -ForegroundColor Cyan
