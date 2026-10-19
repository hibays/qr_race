# Modules (mod layer)

## What the brief requires

Modules are Python programs that the core exposes an interface to. They run per
frame / per action, interact with users by role, may hold native capabilities,
only an admin uploads them, and the server distributes them to clients. The
intended runtime is RustPython.

## Intended host contract

- One interpreter per module instance, so module-level state survives calls.
- One JSON string in, one optional string out:
  `on_event(payload_json) -> str | None`, `on_frame(payload_json) -> str | None`.
- No imports and no stdlib by default; the host grants capabilities explicitly
  (the manifest's `capabilities` list is the request, not the permission).
- Manifest lives in `qr_race_core::ModuleManifest` (id, version, entry, hooks,
  capabilities) and is validated before a room may select the module.
- Admin-only upload; rooms select from what exists; clients fetch by id+version.

## Attempt log (2026-10-20, Windows x64, Rust 1.99)

1. `rustpython-vm 0.6.0` does not compile against `rustpython-ruff_python_ast
   0.16.10` (the published `rustpython-codegen` targets the 0.16.5 AST).
   Pinning `ruff_python_ast`, `ruff_python_parser`, `ruff_python_trivia`,
   `ruff_source_file`, `ruff_text_size` to `=0.16.5` fixes compilation.
2. With `Interpreter::without_stdlib`, the first VM creation panics with
   `essential initialization failed`.
3. With the `rustpython` crate (`stdlib` + `freeze-stdlib` + `importlib` +
   `encodings`), VM creation fails with
   `RecursionError: maximum recursion depth exceeded while hashing` while
   importing `codecs`.
4. `rustpython 0.5` mixed with the 0.6 stdlib fails with two
   `malachite_bigint` versions in the graph.

The experimental `qr_race_mod` crate was removed so the workspace stays green.
The host design above stands; only the runtime is blocked, by an upstream
initialization bug rather than by our interface.

## Options (decision pending)

- **Track upstream.** Retry when RustPython publishes a fix (or pin a known-good
  git commit). Preferred, since it keeps the brief's runtime choice.
- **Different embedded runtime** (e.g. CPython via pyo3). Much heavier to ship on
  six targets and contradicts the RustPython requirement.
- **Process sandbox** (`python.exe` child with a restricted protocol). Robust and
  language-agnostic, but adds an external runtime dependency: a product call.

## What is already in place

- `ModuleManifest` + hook/capability types in `qr_race_core`, with validation.
- **Admin-only upload and distribution:** accounts carry an `admin` flag
  (`provision --admin`), `POST /v1/admin/modules` stores manifest + source,
  `GET /v1/modules` lists them and `GET /v1/modules/{id}` returns the source.
- **Persistence:** modules live in SQLite next to rooms and relays; uploads and
  room selections survive a restart.
- **Room selection:** the host picks from uploaded ids; unknown ids are rejected
  (`400`) instead of silently stored.
- **Client consumption:** the Flutter room screen shows the selected module ids
  and gives the host a picker fed by `GET /v1/modules` (manifest version, hooks,
  capabilities); saving calls `POST /v1/room/modules`. An admin sees an upload
  action in the lobby that posts manifest + source without curl.
- Rooms carry a module-id list and are the only role that may change it.
- `RoomEvent` is the payload shape modules will receive server-side.

Execution is the only missing half: with the runtime blocker above, the server
never runs module code yet.
