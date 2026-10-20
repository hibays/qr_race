param(
    [int]$Port = 8443,
    [switch]$SkipBuild,
    [switch]$SkipRust,
    [switch]$Demo
)

$ErrorActionPreference = "Stop"
$root = Split-Path -Parent $PSScriptRoot
$gui = Join-Path $root "crates\qr_race_gui"

$adbCandidates = @(
    $env:ANDROID_HOME,
    $env:ANDROID_SDK_ROOT,
    "D:\Dev\Android\Sdk"
) | Where-Object { $_ } | ForEach-Object { Join-Path $_ "platform-tools\adb.exe" }
$adb = $adbCandidates | Where-Object { Test-Path $_ } | Select-Object -First 1
if (-not $adb) {
    throw "adb not found; set ANDROID_HOME or install Android platform-tools"
}

& $adb start-server | Out-Null
$devices = @(& $adb devices | Where-Object { $_ -match "\sdevice$" })
if ($devices.Count -eq 0) {
    Write-Host "No authorized Android device." -ForegroundColor Yellow
    Write-Host "1. Enable Developer Options and USB debugging on the phone."
    Write-Host "2. Use a data cable and choose File Transfer / MTP, not charging-only."
    Write-Host "3. Accept the 'Allow USB debugging?' prompt on the phone."
    Write-Host "4. Re-run: & `"$adb`" kill-server; & `"$adb`" start-server; & `"$adb`" devices"
    exit 2
}
$deviceLine = $devices[0]
Write-Host "device: $deviceLine" -ForegroundColor Green

& $adb reverse "tcp:$Port" "tcp:$Port"
if ($LASTEXITCODE -ne 0) { throw "adb reverse failed" }
Write-Host "adb reverse tcp:$Port -> tcp:$Port (phone can use https://127.0.0.1:$Port)" -ForegroundColor Green

$apk = Join-Path $gui "build\app\outputs\flutter-apk\app-debug.apk"
if (-not $SkipBuild) {
    if (-not $SkipRust) {
        & pwsh -NoProfile -File (Join-Path $root "scripts\build-android-ffi.ps1")
        if ($LASTEXITCODE -ne 0) { throw "Android Rust library build failed" }
    }
    $props = Join-Path $gui "android\gradle\wrapper\gradle-wrapper.properties"
    $backup = "$props.qrrace-backup"
    Copy-Item $props $backup -Force
    try {
        $localZip = Join-Path $root ".refs\gradle-9.3.1-bin.zip"
        if (Test-Path $localZip) {
            $text = Get-Content $props -Raw
            $text = $text -replace 'distributionUrl=.*', "distributionUrl=file\:///$($localZip.Replace('\','/'))"
            Set-Content $props -Value $text -Encoding ascii
            Write-Host "using local Gradle distribution to avoid TLS downloads" -ForegroundColor DarkGray
        }
        Push-Location $gui
        try {
            $arguments = @("build", "apk", "--debug")
            if ($Demo) { $arguments += "--dart-define=DEMO=true" }
            & flutter @arguments
            if ($LASTEXITCODE -ne 0) { throw "flutter build apk failed" }
        } finally {
            Pop-Location
        }
    } finally {
        Copy-Item $backup $props -Force
        Remove-Item $backup -Force
    }
}

if (-not (Test-Path $apk)) { throw "APK not found: $apk" }
& $adb install -r $apk
if ($LASTEXITCODE -ne 0) { throw "adb install failed" }
& $adb shell am force-stop app.qrrace.qr_race_gui
& $adb shell am start -n app.qrrace.qr_race_gui/.MainActivity | Out-Null

Write-Host ""
Write-Host "installed and launched." -ForegroundColor Green
Write-Host "In the app connect screen use:"
Write-Host "  server:    https://127.0.0.1:$Port"
Write-Host "  fingerprint / tokens / TOTP: see the desktop self-host screen or deploy-server output"
Write-Host ""
Write-Host "Android can self-host: 连接页 -> 自部署服务器（本机），默认端口 18443。"
Write-Host "Or connect to a desktop/server URL and scan/publish automatically."
