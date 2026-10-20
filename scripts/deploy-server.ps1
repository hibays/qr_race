param(
    [int]$Port = 8443,
    [string]$Config = ""
)

$ErrorActionPreference = "Stop"
$root = Split-Path -Parent $PSScriptRoot
Push-Location $root
try {
    Write-Host "building qr_race_server (release)..." -ForegroundColor Cyan
    & cargo build --release -p qr_race_server
    if ($LASTEXITCODE -ne 0) { throw "cargo build failed" }
    $server = Join-Path $root "target\release\qr_race_server.exe"
    if (-not (Test-Path $server)) { throw "missing $server" }

    if ([string]::IsNullOrWhiteSpace($Config)) {
        $Config = Join-Path $root "config.json"
    }
    $Config = [System.IO.Path]::GetFullPath($Config)

    if (-not (Test-Path $Config)) {
        $hostJson = (& $server provision --user host | Select-Object -First 1) | ConvertFrom-Json

        $data = Join-Path $root "data"
        New-Item -ItemType Directory -Force -Path $data | Out-Null
        $candidates = @(Get-NetIPAddress -AddressFamily IPv4 -ErrorAction SilentlyContinue |
            Where-Object {
                $_.IPAddress -notlike "127.*" -and
                $_.IPAddress -notlike "169.254.*" -and
                $_.IPAddress -notlike "100.*"
            } | Select-Object -ExpandProperty IPAddress)
        $ip = $candidates | Where-Object { $_ -like "192.168.*" } | Select-Object -First 1
        if (-not $ip) { $ip = $candidates | Where-Object { $_ -like "10.*" } | Select-Object -First 1 }
        if (-not $ip) { $ip = $candidates | Where-Object { $_ -match '^172\.(1[6-9]|2[0-9]|3[01])\.' } | Select-Object -First 1 }

        $hosts = @("localhost", "127.0.0.1")
        if ($ip) { $hosts += $ip }

        $configObject = [ordered]@{
            listen   = "0.0.0.0:$Port"
            database = (Join-Path $data "qr_race.db")
            tls      = [ordered]@{
                cert  = (Join-Path $data "cert.pem")
                key   = (Join-Path $data "key.pem")
                hosts = $hosts
            }
            accounts = @(
                [ordered]@{ user_id = "host"; token = $hostJson.token; admin = $false }
            )
        }
        [System.IO.File]::WriteAllText($Config, ($configObject | ConvertTo-Json -Depth 8))

        $urlHost = if ($ip) { $ip } else { "127.0.0.1" }
        Write-Host ""
        Write-Host "wrote $Config" -ForegroundColor Green
        Write-Host "server URL: https://${urlHost}:$Port"
        Write-Host "host token:   $($hostJson.token)"
        Write-Host ""
        Write-Host "Members are created per room by the host's 邀请成员 button."
        Write-Host ""
        if ($ip) {
            Write-Host "If Windows Firewall blocks the LAN URL, run as admin:"
            Write-Host "  New-NetFirewallRule -DisplayName `"qr_race $Port`" -Direction Inbound -Protocol TCP -LocalPort $Port -Action Allow"
        } else {
            Write-Host "No LAN IPv4 found; use USB + adb reverse and https://127.0.0.1:$Port"
        }
        Write-Host ""
    }

    Write-Host "starting server; press Ctrl+C to stop" -ForegroundColor Cyan
    & $server serve --config $Config
} finally {
    Pop-Location
}
