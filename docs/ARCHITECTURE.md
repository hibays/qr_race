# Architecture

## Layers

```
Android Camera2 (Kotlin)            Desktop / Web
  ImageReader YUV_420_888             nokhwa / browser getUserMedia
        │ Y/U/V planes (color)               │
        ▼                                    ▼
  JNI nativePushFrame                 qr_race_capture / wasm
        │                                    │
        └──────────────┬─────────────────────┘
                       ▼
              qr_race_scan (Rust)
        PyramidDetector → crop → zxing-cpp decode
              (optional bilinear SR)
                       │
             hits / stats / preview
                       │ FFI / wasm
                       ▼
                 Flutter UI
```

No camera frame enters Dart. Dart polls a small JSON state (hits, stats, error,
preview sequence) plus a low-rate JPEG preview.

## Android camera path (fast path)

1. `CameraCapture.kt` opens the back camera with `ImageReader` at the requested
   size (default 2560×1440, falling back to the closest supported YUV size)
   and YUV_420_888.
2. On each image it copies **Y, U and V planes** (tightly packed, honoring `rowStride`/`pixelStride`).
3. `nativePushFrame(width, height, rotation, y, u, v)` crosses JNI once. Rust
   converts YUV to tightly packed RGB, keeps luma, and queues both in a bounded
   `sync_channel(2)`. If the worker is behind, the frame is dropped instead of
   queued.
4. The Rust worker first decodes RGB with zxing-cpp. On a miss it runs
   `ScanEngine` (pyramid detector + zxing-cpp on luma), then the CNN tiers.
   Frames are **not rotated** here; QR detection is rotation-independent.
   Sensor rotation is metadata used only when encoding the preview.
5. The preview is downscaled to at most 1280 px, rotated at that small size, and
   JPEG-encoded every 60 ms. Scanning never waits for the preview.
6. `addImageAvailableListener` uses `acquireLatestImage()` and a busy flag, so
   the camera stays live and the UI never builds an unbounded backlog.

Hits are deduplicated by **both** exact bytes and decoded text. A repeated
symbol in view produces one hit.

## Why `image` and PNG appear in the dependency graph

- The scan path is raw `GrayFrame` data; no compression, PNG or JPEG is involved
  in decoding.
- The only image encoding in the FFI is the **preview JPEG** (`image` with the
  `jpeg` feature). Compression ratio is irrelevant there; it exists only because
  Flutter's `Image.memory` needs an encoded image.
- `qr_race_ffi` now builds `qr_race_scan` with `default-features = false`, so
  the scan crate's own PNG/BMP file decoders are gone.
- zxing-cpp is C++ and pulls no Rust `png`/`imageproc` path; the only Rust
  image dependency is the preview JPEG encoder.

## Embedded server (Android self-host)

`qr_race_embedded` is compiled into the same `libqr_race_ffi.so`:

- starts a current-thread Tokio runtime on a Rust thread;
- binds **127.0.0.1 only** (single-device self-host);
- supports HTTP, generated self-signed TLS, or user-picked cert/key;
- uses the in-memory hub (rooms/relays reset when the app process dies);
- is controlled from Dart through `qr_server_start_config` / `qr_server_info` /
  `qr_server_stop`.

The desktop build keeps using the separate `qr_race_server` process (and SQLite),
selected by platform in `ServerLauncher`.

## Desktop bridge and lifecycle

The desktop GUI does not spawn a server in Dart. `ProcessServerLauncher` asks the
Rust bridge to start `qr_race_server`; on Windows the bridge puts the child in a
Job Object with `KILL_ON_JOB_CLOSE`. If the GUI crashes or is closed, the job
handle closes and the child is killed, so port 8443 can never be orphaned.

Config/accounts/SQLite live in `%APPDATA%\qr_race\selfhost\desktop` and are
reused on the next start, so tokens and rooms survive GUI restarts. The Android
build uses the in-process `qr_race_embedded` path instead.

Desktop text theme supplies `Microsoft YaHei / PingFang SC / SimHei / Noto Sans
CJK SC` as `fontFamilyFallback` (the rcrm approach), fixing CJK tofu on Windows.

## Identity, rooms and invitations

- A **server account** is an identity (token). It is not a room
  role.
- Creating a room makes that account the room's **host**; joining makes it a
  **member/co-host**. The same identity can be host in one room and member in
  another.
- The host can call `POST /v1/room/guest?address=...`; the server provisions a
  fresh identity and returns its token. That guest is scoped by the
  invite link to one room and joins as a member.
- Invite links use `qrrace://connect?...&room=...`; the app fills the form,
  logs in with the token, joins the room and opens it.

## Dynamic relays

- Scan hits are published automatically, no button.
- Payload prefix (`A:https://...`, `cam1|...`) identifies a slot. The room
  shows only the newest relay per slot; multiple prefixes coexist.
- The server keeps an append-only relay history (64 per room); grouping is a
  client concern today. A server-side slot/upsert API is a possible next step.
- Opening a relay/scan result uses Android's `ACTION_VIEW` chooser for http(s)
  (WeChat/WeCom/browser), direct launch for custom schemes, and copies the link
  when no app handles it.

## WeChat CNN: tiered, not the whole frame every frame

`qr_race_wechat` is our copy of the WeChat pipeline; it does not depend on the
registry `wxscan` crate. The algorithm modules (`detector`, `scale`,
`decodermgr`, `wechat_qrcode`) live in the crate, with the upstream sources
vendored under `crates/cvlite` only; wxscan/wxing were deleted after the zxing-cpp swap
(see `docs/VENDORED.md`).

The scan worker runs tiers:

1. **fast** — zxing-cpp on the full frame (RGB color from Camera2, luma fallback); desktop synthetic suite is 10–13 ms, Android re-measure pending and
   the 30 fps path is never blocked by CNN work;
2. **cnn** — only after a miss (every other frame), the WeChat detector at
   384×384, then each candidate is cropped from the **full-resolution** frame
   and decoded by zxing-cpp;
3. **cnn+sr** — when a crop is smaller than 120×120, pad to square, resize to
   the SR graph's fixed 224×224, run the ONNX super-resolution net, then decode;
4. **tiled far-code pass** — every fourth CNN attempt, split the frame into
   four overlapping 2×2 tiles and run each through the same 384 detector. This
   preserves small modules without needing a dynamic-shape model.

The backend sits behind `crate::net::Net`: Android uses `OrtNet` with ONNX
Runtime 1.31 (`libonnxruntime.so` packaged in `jniLibs`); desktop uses the pure
Rust tract backend as fallback. The session is mutex-guarded because ORT `run`
takes `&mut Session`.

Device HUD (OnePlus PLF110, ORT): fast 2.9–6.8 ms; CNN 384 6.3 ms (empty scene)
to 15.9 ms (candidate + SR); tiled far-code pass 21.4 ms. A 11-vmin QR on the PC
screen decoded `https://example.test/far-small-live` at 1280×720 in the tiled
path, under the 60 ms CNN budget. Clear codes stay on the fast path.
A real-camera run (1075 frames) measured HUD p95 17.4 ms, CNN 59.1 ms and
5 dropped frames, and decoded the far QR. This is one run, not a multi-device
rate curve.

## On-device numbers (OnePlus PLF110)

| requested capture | negotiated | scan p50-ish | notes |
|---|---|---|---|
| 640×480 | 640×480 | ~2–3 ms | fastest; fewer pixels per QR module |
| 960×540 | 960×720 (closest) | ~7.8 ms | middle ground |
| 1280×720 | 1280×720 | ~6–16 ms | default; decoded a QR at desk distance |

These are Rust scan times from the scanner HUD (`帧 / scan_ms`). Total latency
also includes camera exposure and the 120 ms Dart poll; a frame is never queued
if scanning is behind. 1280×720 is the default because it preserves recognition
margin for small/distant codes while staying well under the 33 ms budget in the
measured runs. 640×480 is available with
`--dart-define=SCAN_WIDTH=640 --dart-define=SCAN_HEIGHT=480` for battery/testing.

## WeChat CNN: what is actually running

`models/detect.onnx` and `models/sr.onnx` are loaded from `include_bytes!` by
`qr_race_wechat`. The pipeline is real: prior decode/NMS, full-resolution crop,
zxing-cpp decode, and on-demand SR run on device. It is **not** ONNX Runtime Mobile;
the current backend is tract, which is why the 384 detector costs ~23–34 ms and
the 768 pass ~90 ms. The fast zxing-cpp path remains the 30 fps target; color/Android timing is being re-measured.

The next backend step is to replace `src/backend.rs` with an ORT session using
the Android `libonnxruntime.so`, and to compare 384/768 schedules against the
current tract implementation. TFLite/FFI upstream crates are vendored but not
built. See `docs/MODELS.md` and `docs/VENDORED.md`.








