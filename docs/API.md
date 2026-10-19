# qr_race HTTP + WebSocket API

Base URL: whatever the operator publishes (`https://…` or `http://…`). All
bodies are JSON, all payload bytes are base64 in JSON.

## Authentication

```
POST /v1/auth
{ "token": "<bearer token>", "code": "123456" }
```

- `200` → `{ "session_id": "s-…", "user_id": "…", "expires_at_ms": 123, "admin": false }`
- `401` on unknown token, wrong/expired code, or a code whose step was already
  used.

`admin` tells the client whether to show the module-upload action; the server
still enforces it on `/v1/admin/modules`.

Every other route requires `Authorization: Bearer <session_id>`. Sessions expire
after 12 hours by default; an expired session is `401` and the client must
authenticate again.

### WebSocket ticket

A browser cannot set an Authorization header on a WebSocket, so the upgrade
takes a single-use ticket instead:

```
POST /v1/ws-ticket        (bearer session)
→ { "ticket": "…", "expires_at_ms": 123 }
```

The ticket lives 60 seconds, is consumed by the upgrade, and cannot be reused.
Native clients may keep using the bearer header on the upgrade if they prefer.

## Rooms

A room address is the canonical string, e.g. `东京A区1B级103房`. Query parameters
carry it percent-encoded as UTF-8.

| Method | Path | Body | Result |
|---|---|---|---|
| `POST` | `/v1/rooms` | `{ "address": "东京A区1B级103房" }` | `201` + structured address; `409` if it exists |
| `GET` | `/v1/lobby` | — | `{ "rooms": [ { address, label, host, member_count, module_count } ] }` |
| `GET` | `/v1/room?address=…` | — | room view (below) |
| `POST` | `/v1/room/light?address=…` | — | `204`; host only |
| `POST` | `/v1/room/join?address=…` | — | `204`; room must be lit (host excepted) |
| `POST` | `/v1/room/leave?address=…` | — | `204`; the host cannot leave |
| `POST` | `/v1/room/promote?address=…` | `{ "target": "user-id" }` | `204`; host only |
| `POST` | `/v1/room/demote?address=…` | `{ "target": "user-id" }` | `204`; host only |
| `POST` | `/v1/room/modules?address=…` | `{ "modules": ["…"] }` | `204`; host only; ids must be uploaded |
| `POST` | `/v1/room/relay?address=…` | `{ "kind": "text", "payload_b64": "…" }` | `200` + relay; host/co-host only |
| `GET` | `/v1/room/events?address=…` | — | WebSocket event stream |

`kind` is one of `text`, `url`, `json`, `binary`. `compression` defaults to
`none` (`zstd` is reserved).

### Room view

```json
{
  "address": { "capital": "tokyo", "district": "A", "level": "1B", "room": "103" },
  "label": "东京A区1B级103房",
  "lit": true,
  "host": "host-a",
  "modules": [],
  "members": [
    { "user": "host-a", "role": "host", "last_relay": "r-1" },
    { "user": "m-1", "role": "co_host", "last_relay": null }
  ],
  "relays": [
    {
      "id": "r-1",
      "room": { "capital": "tokyo", "district": "A", "level": "1B", "room": "103" },
      "publisher": "host-a",
      "kind": "text",
      "compression": "none",
      "created_at_ms": 1700000000000,
      "payload_b64": "cmVsYXktYnl0ZXM="
    }
  ]
}
```

`last_relay` is what a client highlights on an avatar: a user with one has
scanned and may be tapped to relay onward. `role` is `host`, `co_host` or
`member`.

A room keeps its newest 64 relays in memory; older ones are dropped. A client
that was disconnected must re-fetch `GET /v1/room` after a gap.

## Modules

Modules are admin-uploaded Python sources. Only an account provisioned with
`"admin": true` may upload; room hosts select from what exists, and selecting an
unknown id is `400`.

| Method | Path | Body | Result |
|---|---|---|---|
| `POST` | `/v1/admin/modules` | `{ id, version, entry, hooks, capabilities, source }` | `201` + manifest; admin only (`403` otherwise) |
| `GET` | `/v1/modules` | — | `{ "modules": [ { id, version, entry, hooks, capabilities } ] }` |
| `GET` | `/v1/modules/{id}` | — | `{ "manifest": {…}, "source": "…" }`; `404` if absent |

`hooks` are `frame` / `action`; `capabilities` are `camera`, `network`,
`storage`, `ui`. Source must be non-empty and at most 256 KiB.

```json
{
  "id": "overlay",
  "version": "1.0.0",
  "entry": "main.py",
  "hooks": ["frame"],
  "capabilities": ["ui"],
  "source": "def on_frame(payload):\n    return payload\n"
}
```

Uploads and room selections survive a restart. The server distributes source to
clients; executing it is still pending (see [MODULES.md](MODULES.md)).

## Events (WebSocket)

Opening `/v1/room/events?address=…&ticket=…` (or with an `Authorization` header)
subscribes the caller to one room. The server sends one JSON object per text
frame:

```json
{ "type": "relay_published", "address": { … }, "relay": { … } }
```

Types: `room_created`, `room_lit`, `member_joined`, `member_left`,
`role_changed`, `modules_changed`, `relay_published`. A client that lags is
skipped forward by the server; it must re-sync with `GET /v1/room` when it sees
a gap.

## Status codes

| Status | Meaning |
|---|---|
| `400` | malformed address, bad base64, empty/oversized relay, bad module list |
| `401` | missing/invalid/expired session; failed or replayed TOTP |
| `403` | authenticated but not allowed (member publishing, non-participant view, non-host control) |
| `404` | unknown room |
| `409` | room exists, room not lit, already lit, host trying to leave, duplicate co-host |
| `429` | too many failed login codes; the account is locked for a minute |

Five wrong codes lock an account for 60 seconds (the JSON body carries the
retry delay); a successful login clears the counter.

## Config

The server reads a config file:

```json
{
  "listen": "127.0.0.1:8080",
  "database": "qr_race.db",
  "accounts": [
    { "user_id": "host-a", "token": "…", "totp_secret": "…", "admin": false }
  ]
}
```

`database` is optional: omit it for an in-memory server (tests, demos). When set,
rooms and relays survive a restart. Accounts always come from this file, and
sessions are in-memory either way — after a restart the client authenticates
again with a fresh TOTP code.

`tls` is optional. When present the server speaks HTTPS; if `cert` and `key` both
do not exist it generates a self-signed pair (valid for `hosts`, defaulting to
`localhost`, `127.0.0.1`, `::1`) and writes both files. Clients must trust
`cert.pem`: browsers show a warning to accept, `curl` accepts it with
`--cacert cert.pem`, and a team can install it as a root CA. The server prints
the certificate's SHA-256 fingerprint, which native clients can pin instead of
installing a root: the Flutter connection screen has a fingerprint field, and
`qr_scan --cacert <pem>` (or `--insecure` for local testing) does the same for
the CLI. Without `tls` the server speaks plain HTTP — terminate TLS in front of
it if it is exposed beyond localhost. Browser camera scanning (`getUserMedia`)
needs HTTPS or localhost.

```json
"tls": { "cert": "cert.pem", "key": "key.pem", "hosts": ["localhost", "127.0.0.1"] }
```

Mint an account with `qr_race_server provision --user host-a` (add `--admin` for
a module-upload account) and paste the JSON line into `accounts`.

CORS is permissive (any origin, any header) so the Flutter web build can call
the API from another origin. Tighten it if the server is exposed beyond a
trusted network.
