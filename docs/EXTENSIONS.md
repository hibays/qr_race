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

The old module manifest/catalog, upload API, store rows and Flutter API models
were removed. There is no `ModuleManifest`, no `/v1/modules`, and no
`rustpython` dependency in the workspace. Extension work now means a new Rust
crate/trait or a documented protocol change, not uploaded scripts.

