# Deploying qr_race

There are two deployment shapes:

1. **Hosted control server** (this repository's current deployment): one HTTPS
   process, all clients connect to it.
2. **Self-host on desktop/Android**: the GUI starts `qr_race_server` locally.

This document covers the hosted shape and the current public instance. Build
instructions are in [BUILDING.md](BUILDING.md).

## 1. Current deployment

```text
URL:         https://<PUBLIC_HOST>:8100
Certificate: self-signed
SHA-256:     cdf01363ac885204857d25016afb69a2a1bc646f57ebcb60e95fe3968eece893
Host account: host
Token:       <HOST_TOKEN>
```

The token is a credential. Rotate it with `qr_race_server provision --user host`
and update `config.json` if it leaks.

Health check:

```bash
curl -k https://<PUBLIC_HOST>:8100/v1/health
# ok
```

The client must fill both the URL and the **certificate fingerprint**; the
self-signed certificate is pinned, not blindly trusted.

## 2. Server layout

```text
/root/Case/qr_scan_server/
  target/release/qr_race_server
  config.json
  cert.pem
  key.pem
  qr_scan.db
  server.log
```

Config used by the public instance:

```json
{
  "listen": "0.0.0.0:8100",
  "database": "qr_scan.db",
  "accounts": [
    {
      "user_id": "host",
      "token": "<token>",
      "admin": true
    }
  ],
  "tls": {
    "cert": "cert.pem",
    "key": "key.pem",
    "hosts": ["<PUBLIC_HOST>", "127.0.0.1", "localhost"]
  }
}
```

`hosts` must contain the public IP/DNS name or clients fail certificate
verification even with the correct fingerprint.

## 3. Build and install

On a Linux host:

```bash
cargo build --release -p qr_race_server
install -m 0755 target/release/qr_race_server /root/Case/qr_scan_server/
```

If the source tree lives elsewhere, copy only the binary plus `config.json` and
the certificate pair.

Generate a token:

```bash
qr_race_server provision --user host --admin
```

The command prints a JSON account; paste it into `config.json`. Never commit
`config.json` or `key.pem`.

## 4. Run under screen

The current host uses a detached screen session:

```bash
screen -dmS qr_scan bash -lc \
  'cd /root/Case/qr_scan_server && \
   exec ./target/release/qr_race_server serve --config config.json >> server.log 2>&1'
```

Inspect:

```bash
screen -R qr_scan
```

Detach with `Ctrl-A d`.

Screen does not survive a reboot. For production, use systemd:

```ini
[Unit]
Description=qr_race server
After=network-online.target
Wants=network-online.target

[Service]
WorkingDirectory=/root/Case/qr_scan_server
ExecStart=/root/Case/qr_scan_server/target/release/qr_race_server serve --config config.json
Restart=on-failure
NoNewPrivileges=true

[Install]
WantedBy=multi-user.target
```

```bash
systemctl enable --now qr-race
journalctl -u qr-race -f
```

## 5. TLS

`qr_race_server` generates a self-signed certificate on first start when
`tls.cert`/`tls.key` do not exist, and prints the SHA-256 fingerprint:

```text
certificate SHA-256: cdf01363...
```

Read it later:

```bash
qr_race_server fingerprint --cert cert.pem
```

The client's "证书指纹" field must match exactly (64 hex characters). If the
certificate is regenerated, every client must update the fingerprint once; the
saved token/URL can remain.

## 6. Firewall / port

The public instance uses **TCP 8100**; the cloud security group must allow it.
There is no plaintext listener in this deployment. If the port is blocked, the
client shows a connection timeout rather than a token error.

Check locally and publicly:

```bash
ss -ltnp | grep 8100
curl -k https://127.0.0.1:8100/v1/health
curl -k https://<PUBLIC_HOST>:8100/v1/health
```

## 7. Client setup

Android/Windows connect form:

```text
服务器地址: https://<PUBLIC_HOST>:8100
证书指纹:  cdf01363ac885204857d25016afb69a2a1bc646f57ebcb60e95fe3968eece893
访问 token: <host token>
```

The server address, fingerprint and token are persisted. On the next launch the
client validates them and opens the lobby directly. Settings has the same three
fields and a "保存并重新连接" action.

## 8. Publish an APK to AList

The shared directory is:

```text
http://<PUBLIC_HOST>:5244/Shared/码力APP
```

Upload (guest write is enabled on this instance):

```powershell
$dest = '/Shared/%E7%A0%81%E5%8A%9BAPP'
Get-ChildItem dist\android\*release.apk | ForEach-Object {
  $name = [uri]::EscapeDataString($_.Name)
  curl.exe -sS -X PUT `
    -H "File-Path: $dest/$name" `
    --data-binary "@$($_.FullName)" `
    'http://<PUBLIC_HOST>:5244/api/fs/put'
}
```

Verify:

```bash
curl -I 'http://<PUBLIC_HOST>:5244/p/Shared/...apk?sign=...'
# 200, application/vnd.android.package-archive
```

## 9. Room lifecycle

- A PIN starts the synthetic namespace `南极<pin>` (e.g. `南极1234`); it never
  consumes a geographic inventory room.
- A room is listed only while it has a live event socket. When the last socket
  closes, the server deletes the room, its relays and its PIN mapping.
- Relays are active for 15 s, inactive 15–40 s, and removed after 40 s. A
  publisher re-pushing the same payload refreshes it instead of creating a
  duplicate.
- Active members keep a room alive without a new code; an empty room does not
  linger for a 60 s timeout.

## 10. Operational checks

```bash
# Service up
screen -ls | grep qr_scan

# Log
tail -f /root/Case/qr_scan_server/server.log

# TLS + API
curl -k https://127.0.0.1:8100/v1/health

# Authenticate and inspect lobby
TOKEN=...
SESSION=$(curl -sk -X POST https://127.0.0.1:8100/v1/auth \
  -H 'content-type: application/json' \
  -d "{\"token\":\"$TOKEN\"}" | jq -r .session_id)
curl -sk https://127.0.0.1:8100/v1/lobby \
  -H "authorization: Bearer $SESSION"
```

An empty lobby (`{"rooms":[]}`) is correct when no one is inside a room.

## 11. Rotation and failure recovery

- **Token leak:** provision a new token, replace `accounts[].token`, restart.
- **Certificate change:** regenerate `cert.pem/key.pem`, publish the new
  fingerprint, clients reconnect once.
- **Database corruption:** stop the service, move `qr_scan.db` aside, restart.
  Rooms are ephemeral by design; only accounts are in `config.json`.
- **Port conflict:** `ss -ltnp | grep 8100`; stop the old screen/process before
  starting the new binary.

## 12. Current checks

```text
health: ok
lobby:  rooms=[]
screen: qr_scan detached
cert:   cdf01363ac885204857d25016afb69a2a1bc646f57ebcb60e95fe3968eece893
```

