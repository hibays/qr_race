# Extension points (no Python modules)

The product no longer includes a RustPython/mod layer. There is no Python
interpreter, no per-frame script hook, no admin module upload UI, and no room
module picker. This is intentional: the module runtime was blocked upstream and
is removed from the product scope.

The engineering seams that remain are ordinary Rust/Dart boundaries:

- `qr_race_scan::Decoder`, `Detector`, `SuperResolver` — replace the scan stages
  (the WeChat CNN pipeline uses this).
- `qr_race_capture` — platform capture backends behind one frame type.
- `qr_race_hub` / `qr_race_api` — room state and transport; a future policy can
  be added as a Rust service, not uploaded Python.
- Flutter navigation/state — screens depend on `AppState` and `ApiClient`, not
  on a plugin runtime.

The server still carries a small **internal extension catalog** (`ModuleManifest`,
`/v1/modules`) from earlier work; it is not exposed in the client, not executed,
and not part of the acceptance surface. It may be deleted or repurposed for
native extensions later. There is no `rustpython` dependency in the workspace.
