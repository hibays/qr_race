# End-to-end browser camera check: fake video (a QR frame) → Edge headless →
# camera_scan.js → wasm scan → hit, reported through CDP.
#
# Requirements: Edge, ffmpeg, Python with Pillow and websocket-client.
# Run from the repo root:
#
#   pwsh -File scripts/browser-scan-probe.ps1

$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
$probe = Join-Path $root '.refs/browser-probe'
$web = Join-Path $root 'crates/qr_race_gui/web'
$edge = 'C:\Program Files (x86)\Microsoft\Edge\Application\msedge.exe'
$httpPort = 18451
$debugPort = 9300 + (Get-Random -Maximum 200)

New-Item -ItemType Directory -Force $probe | Out-Null
if (-not (Test-Path $edge)) { throw "Edge not found at $edge" }
$stamp = Get-Date -Format 'yyyyMMddHHmmss'

# 1. A QR page image and a looping Y4M the fake camera can play.
cargo run --release -q -p qr_race_bench -- --generate "$probe/qr.png" --text "https://example.test/relay"
python -c @"
from PIL import Image
qr = Image.open(r'$probe/qr.png').convert('L').resize((360, 360), Image.NEAREST)
canvas = Image.new('L', (640, 480), 255)
canvas.paste(qr, (140, 60))
canvas.convert('RGB').save(r'$probe/frame.png')
"@
ffmpeg -y -loglevel error -loop 1 -i "$probe/frame.png" -t 2 -r 15 -pix_fmt yuv420p -f yuv4mpegpipe "$probe/probe.y4m"

# 2. Serve the web directory (localhost is a secure context for getUserMedia).
$server = Start-Process -FilePath python -ArgumentList '-m','http.server',"$httpPort",'--bind','127.0.0.1' -WorkingDirectory $web -PassThru -WindowStyle Hidden
Start-Sleep -Seconds 1

# 3. Headless Edge with the fake camera, then drive it over CDP.
$profile = Join-Path $probe "edge-profile-$stamp"
$edgeArgs = @(
  '--headless=new', '--disable-gpu', '--no-first-run', '--no-default-browser-check',
  "--remote-debugging-port=$debugPort",
  "--user-data-dir=$profile",
  '--use-fake-ui-for-media-stream',
  '--use-fake-device-for-media-stream',
  "--use-file-for-fake-video-capture=$probe/probe.y4m",
  '--autoplay-policy=no-user-gesture-required',
  'about:blank'
)
$edgeProcess = Start-Process -FilePath $edge -PassThru -ArgumentList $edgeArgs

$ready = $false
for ($i = 0; $i -lt 40; $i++) {
  try {
    Invoke-RestMethod "http://127.0.0.1:$debugPort/json/version" -TimeoutSec 2 | Out-Null
    $ready = $true
    break
  } catch {
    Start-Sleep -Milliseconds 500
  }
}
if (-not $ready) { throw "Edge CDP did not come up on port $debugPort" }

$code = 2
try {
  $env:EDGE_DEBUG_PORT = "$debugPort"
  $env:PROBE_URL = "http://127.0.0.1:$httpPort/scan_probe.html"
  python (Join-Path $root 'tools/browser_scan_probe.py')
  $code = $LASTEXITCODE
} finally {
  if ($edgeProcess -and -not $edgeProcess.HasExited) {
    Stop-Process -Id $edgeProcess.Id -Force -ErrorAction SilentlyContinue
  }
  Get-CimInstance Win32_Process -Filter "Name='msedge.exe'" |
    Where-Object { $_.CommandLine -like "*$profile*" } |
    ForEach-Object { Stop-Process -Id $_.ProcessId -Force -ErrorAction SilentlyContinue }
  if ($server -and -not $server.HasExited) { Stop-Process -Id $server.Id -Force }
}
exit $code
