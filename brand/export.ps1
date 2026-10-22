# Render the selected identity (02 Relay Loop) into every platform icon.
#
# SVG -> PNG/WebP uses ffmpeg; the SVG width/height is 1024, so every output is
# a Lanczos-scaled square. The ICO container is assembled from those same PNGs
# by assemble_ico.py because ffmpeg's ICO muxer writes a single image.
param(
    [string]$GuiRoot = ''
)

$ErrorActionPreference = 'Stop'
$brand = Split-Path -Parent $PSCommandPath
$root = Split-Path -Parent $brand
if (-not $GuiRoot) {
    $GuiRoot = Join-Path $root 'crates\qr_race_gui'
}
$icon = Join-Path $brand '02-relay-loop-icon.svg'
$lockup = Join-Path $brand '02-relay-loop-lockup.svg'
$foreground = Join-Path $brand '02-relay-loop-foreground.svg'

function Render([string]$svg, [int]$w, [int]$h, [string]$out) {
    $dir = Split-Path -Parent $out
    New-Item -ItemType Directory -Force $dir | Out-Null
    ffmpeg -hide_banner -loglevel error -i $svg `
        -vf "scale=${w}:${h}:flags=lanczos" -frames:v 1 -y $out
    if ($LASTEXITCODE -ne 0) { throw "ffmpeg failed: $out" }
}

# Android legacy launcher + adaptive foreground.
$res = Join-Path $GuiRoot 'android\app\src\main\res'
$android = @{
    'mipmap-mdpi' = 48
    'mipmap-hdpi' = 72
    'mipmap-xhdpi' = 96
    'mipmap-xxhdpi' = 144
    'mipmap-xxxhdpi' = 192
}
foreach ($bucket in $android.Keys) {
    Render $icon $android[$bucket] $android[$bucket] (Join-Path $res "$bucket\ic_launcher.png")
}
$foregroundSizes = @{
    'drawable-mdpi' = 108
    'drawable-hdpi' = 162
    'drawable-xhdpi' = 216
    'drawable-xxhdpi' = 324
    'drawable-xxxhdpi' = 432
}
foreach ($bucket in $foregroundSizes.Keys) {
    Render $foreground $foregroundSizes[$bucket] $foregroundSizes[$bucket] `
        (Join-Path $res "$bucket\ic_launcher_foreground.png")
}
$anydpi = Join-Path $res 'mipmap-anydpi-v26'
New-Item -ItemType Directory -Force $anydpi | Out-Null
@'
<?xml version="1.0" encoding="utf-8"?>
<adaptive-icon xmlns:android="http://schemas.android.com/apk/res/android">
    <background android:drawable="@color/ic_launcher_background" />
    <foreground android:drawable="@drawable/ic_launcher_foreground" />
</adaptive-icon>
'@ | Set-Content (Join-Path $anydpi 'ic_launcher.xml') -Encoding utf8
Copy-Item (Join-Path $anydpi 'ic_launcher.xml') (Join-Path $anydpi 'ic_launcher_round.xml') -Force
@'
<?xml version="1.0" encoding="utf-8"?>
<resources>
    <color name="ic_launcher_background">#0B0F0C</color>
</resources>
'@ | Set-Content (Join-Path $res 'values\colors.xml') -Encoding utf8

# iOS.
$ios = Join-Path $GuiRoot 'ios\Runner\Assets.xcassets\AppIcon.appiconset'
$iosSizes = @{
    'Icon-App-20x20@1x.png' = 20
    'Icon-App-20x20@2x.png' = 40
    'Icon-App-20x20@3x.png' = 60
    'Icon-App-29x29@1x.png' = 29
    'Icon-App-29x29@2x.png' = 58
    'Icon-App-29x29@3x.png' = 87
    'Icon-App-40x40@1x.png' = 40
    'Icon-App-40x40@2x.png' = 80
    'Icon-App-40x40@3x.png' = 120
    'Icon-App-60x60@2x.png' = 120
    'Icon-App-60x60@3x.png' = 180
    'Icon-App-76x76@1x.png' = 76
    'Icon-App-76x76@2x.png' = 152
    'Icon-App-83.5x83.5@2x.png' = 167
    'Icon-App-1024x1024@1x.png' = 1024
}
foreach ($name in $iosSizes.Keys) {
    Render $icon $iosSizes[$name] $iosSizes[$name] (Join-Path $ios $name)
}

# macOS.
$macos = Join-Path $GuiRoot 'macos\Runner\Assets.xcassets\AppIcon.appiconset'
foreach ($size in @(16, 32, 64, 128, 256, 512, 1024)) {
    Render $icon $size $size (Join-Path $macos "app_icon_$size.png")
}

# Windows ICO.
$icoDir = Join-Path $env:TEMP 'qr_race_ico'
Remove-Item $icoDir -Recurse -Force -ErrorAction SilentlyContinue
foreach ($size in @(16, 24, 32, 48, 64, 128, 256)) {
    Render $icon $size $size (Join-Path $icoDir "icon-$size.png")
}
python (Join-Path $brand 'assemble_ico.py') $icoDir
Copy-Item (Join-Path $icoDir 'icon.ico') `
    (Join-Path $GuiRoot 'windows\runner\resources\app_icon.ico') -Force
Copy-Item (Join-Path $icoDir 'icon.ico') (Join-Path $macos 'app_icon.ico') -Force

# Web / PWA.
$web = Join-Path $GuiRoot 'web'
Render $icon 32 32 (Join-Path $web 'favicon.png')
Render $icon 192 192 (Join-Path $web 'icons\Icon-192.png')
Render $icon 512 512 (Join-Path $web 'icons\Icon-512.png')
Render $icon 192 192 (Join-Path $web 'icons\Icon-maskable-192.png')
Render $icon 512 512 (Join-Path $web 'icons\Icon-maskable-512.png')
ffmpeg -hide_banner -loglevel error -i $icon `
    -vf "scale=512:512:flags=lanczos" -frames:v 1 `
    -c:v libwebp -lossless 1 -y (Join-Path $web 'icons\Icon-512.webp')

# In-app lockup and mark.
$assets = Join-Path $GuiRoot 'assets\brand'
Render $lockup 1200 375 (Join-Path $assets 'qr_race-lockup.png')
Render $foreground 512 512 (Join-Path $assets 'qr_race-mark.png')

Write-Host 'brand icons applied' -ForegroundColor Green

