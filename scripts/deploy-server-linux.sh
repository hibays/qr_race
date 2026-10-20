#!/usr/bin/env bash
# Deploy qr_race on Ubuntu/Debian with a self-signed HTTPS certificate.
#
#   sudo ./scripts/deploy-server-linux.sh --port 8443 --host race.example.lan
#
# The script prints the server URL, host token and the certificate SHA-256
# fingerprint. Put the fingerprint in every qrrace://connect link (the client
# refuses an unpinned self-signed link).
set -euo pipefail

PORT="${PORT:-8443}"
PUBLIC_HOST="${PUBLIC_HOST:-}"
DATA="${DATA:-/var/lib/qr_race}"
BIN="${BIN:-/usr/local/bin/qr_race_server}"
SERVICE="${SERVICE:-qr_race}"
SYSTEMD=1

usage() {
  sed -n '2,9p' "$0" | sed 's/^# \{0,1\}//'
  cat <<'EOF'

Options:
  --port <port>       HTTPS port (default 8443, env PORT)
  --host <name>       Public DNS name for the certificate (env PUBLIC_HOST)
  --data <dir>        State directory (default /var/lib/qr_race)
  --bin <path>        Server binary (default /usr/local/bin/qr_race_server)
  --foreground        Do not install a systemd unit; run in this shell
EOF
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --port) PORT="$2"; shift 2 ;;
    --host) PUBLIC_HOST="$2"; shift 2 ;;
    --data) DATA="$2"; shift 2 ;;
    --bin) BIN="$2"; shift 2 ;;
    --foreground) SYSTEMD=0; shift ;;
    -h|--help) usage; exit 0 ;;
    *) echo "unknown option: $1" >&2; usage >&2; exit 2 ;;
  esac
done

if [[ ! -x "$BIN" ]]; then
  echo "server binary not found/executable: $BIN" >&2
  echo "build it with: cargo build --release -p qr_race_server" >&2
  exit 1
fi
if ! command -v python3 >/dev/null; then
  echo "python3 is required to generate config JSON" >&2
  exit 1
fi

mkdir -p "$DATA"
chmod 700 "$DATA"

if [[ -f "$DATA/config.json" ]]; then
  TOKEN="$(python3 -c 'import json,sys; print(json.load(open(sys.argv[1]))["accounts"][0]["token"])' "$DATA/config.json")"
  echo "reusing existing config and host token in $DATA"
else
  TOKEN="$("$BIN" provision --user host | python3 -c 'import json,sys; print(json.load(sys.stdin)["token"])')"
  LAN_IP="$(hostname -I 2>/dev/null | awk '{print $1}' || true)"
  HOSTS="$PUBLIC_HOST"
  if [[ -n "$LAN_IP" ]]; then HOSTS="${HOSTS:+$HOSTS,}$LAN_IP"; fi
  HOSTS="${HOSTS:+$HOSTS,}localhost,127.0.0.1"
  HOSTS_JSON="$(python3 -c 'import json,sys; print(json.dumps([x for x in sys.argv[1].split(",") if x]))' "$HOSTS")"
  cat > "$DATA/config.json" <<EOF
{
  "listen": "0.0.0.0:$PORT",
  "database": "$DATA/qr_race.db",
  "accounts": [
    { "user_id": "host", "token": "$TOKEN", "admin": false }
  ],
  "tls": {
    "cert": "$DATA/cert.pem",
    "key": "$DATA/key.pem",
    "hosts": $HOSTS_JSON
  }
}
EOF
  chmod 600 "$DATA/config.json"
fi

print_summary() {
  local fingerprint
  fingerprint="$("$BIN" fingerprint --cert "$DATA/cert.pem")"
  local host="$PUBLIC_HOST"
  if [[ -z "$host" ]]; then
    host="$(hostname -I 2>/dev/null | awk '{print $1}' || true)"
    host="${host:-127.0.0.1}"
  fi
  echo
  echo "server URL:   https://$host:$PORT"
  echo "host token:   $TOKEN"
  echo "fingerprint:  $fingerprint"
  echo "data:         $DATA"
  echo
  echo "Use this fingerprint in qrrace://connect links; clients pin it."
}

if [[ "$SYSTEMD" == "0" ]]; then
  echo "running in foreground; Ctrl-C stops the server"
  "$BIN" serve --config "$DATA/config.json" &
  SERVER_PID=$!
  trap 'kill "$SERVER_PID" 2>/dev/null || true' INT TERM EXIT
  for _ in $(seq 1 100); do
    [[ -f "$DATA/cert.pem" ]] && break
    sleep 0.1
  done
  print_summary
  wait "$SERVER_PID"
  exit 0
fi

if [[ "$(id -u)" != "0" ]]; then
  echo "systemd install needs root; rerun with sudo or use --foreground" >&2
  exit 1
fi
if ! command -v systemctl >/dev/null; then
  echo "systemctl not found; use --foreground" >&2
  exit 1
fi

if ! id -u qr_race >/dev/null 2>&1; then
  useradd --system --home-dir "$DATA" --shell /usr/sbin/nologin qr_race
fi
chown -R qr_race:qr_race "$DATA"
install -m 0755 "$BIN" /usr/local/bin/qr_race_server
BIN=/usr/local/bin/qr_race_server

cat > "/etc/systemd/system/$SERVICE.service" <<EOF
[Unit]
Description=qr_race relay server
After=network-online.target
Wants=network-online.target

[Service]
Type=simple
User=qr_race
Group=qr_race
WorkingDirectory=$DATA
ExecStart=$BIN serve --config $DATA/config.json
Restart=on-failure
RestartSec=2
NoNewPrivileges=true
PrivateTmp=true

[Install]
WantedBy=multi-user.target
EOF

systemctl daemon-reload
systemctl enable --now "$SERVICE"
for _ in $(seq 1 100); do
  [[ -f "$DATA/cert.pem" ]] && break
  sleep 0.1
done
if ! systemctl is-active --quiet "$SERVICE"; then
  echo "service failed; inspect: journalctl -u $SERVICE -n 80 --no-pager" >&2
  exit 1
fi
print_summary
echo "status:       systemctl status $SERVICE"
echo "logs:         journalctl -u $SERVICE -f"
