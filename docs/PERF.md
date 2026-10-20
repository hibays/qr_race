# Performance notes

Measurements from the development machine, kept honest by recording the method
next to the number. Re-run them before quoting them.

## Method

```powershell
# still images
cargo run --release -p qr_race_bench -- --input .refs/scene.png --iterations 200

# live camera, no publishing, 30 frames (always --release)
cargo run --release -p qr_race_cli -- --camera 0 --frames 30 --probe
cargo run --release -p qr_race_cli -- --camera 0 --frames 30 `
  --width 1280 --height 720 --probe
```

Machine: Windows x64 (MSVC toolchain), integrated webcam ("Integrated Webcam",
MediaFoundation). Scan numbers are `--release`, from the same `ScanEngine`
(zxing-cpp multi-symbol reader, no CNN detector yet).

## Still images (scan only, release)

| fixture | resolution | p50 | p95 |
|---|---|---|---|
| clean single symbol | 198×198 | 0.131 ms | 0.202 ms |
| scene with a 200px symbol | 1280×720 | 4.717 ms | 5.719 ms |
| same, rotated 30° | 1280×720 | 4.738 ms | 5.416 ms |
| same, `--try-harder` | 1280×720 | 4.974 ms | 5.978 ms |

## Live camera (real textured frames, no symbol in view, release)

Camera access was enabled later in the session, so these are real frames from the
integrated webcam, not the Windows privacy placeholder. No QR was in view; the
numbers are the capture + whole-frame scan loop on textured video.

```powershell
cargo run --release -p qr_race_cli -- --camera 0 --frames 60 --probe
cargo run --release -p qr_race_cli -- --camera 0 --width 1280 --height 720 `
  --fps 30 --frames 60 --probe
```

| capture | delivered | frames in | frames scanned | wall | scan avg |
|---|---|---|---|---|---|
| device default (1920×1080) | ~24 fps | 86 | 60 | 3.586 s | **6.01 ms** |
| 1280×720, 30 fps requested | ~28 fps | 111 | 60 | 3.915 s | **2.89 ms** |

**Earlier placeholder run:** before camera access was granted the device returned
the Windows privacy placeholder (a dark frame with the camera-off icon), and the
same loop measured 4.23 ms (1080p) / 1.95 ms (720p). Those are floor numbers for
featureless frames, superseded above for textured video.

**Correction:** an even earlier version of this file quoted 42 ms (1080p) and
19.5 ms (720p). Those runs were debug builds (`cargo run` without `--release`);
they are superseded by the numbers above. Debug builds are roughly an order of
magnitude slower for this workload and must never be used for budget claims.

## Coarse-to-fine pipeline (release, 1280×720 scene, one 200px symbol)

```powershell
cargo run --release -p qr_race_bench -- --input .refs/scene.png --iterations 150
cargo run --release -p qr_race_bench -- --input .refs/scene.png --iterations 150 `
  --detector pyramid
cargo run --release -p qr_race_bench -- --input .refs/scene.png --iterations 150 `
  --detector pyramid --sr bilinear --sr-min-area 100000
```

| strategy | p50 total | detect | super_res | decode |
|---|---|---|---|---|
| whole-frame decode | 4.629 ms | — | — | 4.628 ms |
| pyramid detector (scale 2) | **2.331 ms** | 2.184 ms | — | 0.127 ms |
| pyramid + bilinear super-res | 4.037 ms | 2.302 ms | 1.319 ms | 0.380 ms |

The coarse pass costs ~2.2 ms and shrinks decoding from 4.6 ms to **0.13 ms**,
because the engine now decodes only the crop. Super-resolution adds ~1.3 ms and
runs only when the detection box is below the area threshold — exactly the
"on demand, not every frame" rule.

## wasm scan core

Removed with the zxing-cpp migration; zxing-cpp is C++ and needs an Emscripten
toolchain for wasm. Native/Android use zxing-cpp directly.
## Perturbed texture suite (programmatic, release)

Ten deterministic scenes are generated in code (`qr_race_fixtures`) from zxing-cpp
QR codes: clean, noise, blur, low contrast, illumination gradient, rotation,
perspective, a small code, two codes, and a camera-like mix (texture + noise +
blur + gradient + rotation).

```powershell
cargo run --release -p qr_race_bench -- --suite --iterations 30 --warmup 3
cargo run --release -p qr_race_bench -- --suite --iterations 30 --warmup 3 `
  --detector pyramid
```

| scene | whole-frame p50 | pyramid p50 | hits |
|---|---|---|---|
| clean | 4.495 ms | 2.618 ms | 1/1 |
| noise | 5.534 ms | 3.007 ms | 1/1 |
| blur | 4.641 ms | 2.611 ms | 1/1 |
| low_contrast | 4.569 ms | 2.613 ms | 1/1 |
| gradient | 4.585 ms | 2.534 ms | 1/1 |
| rotated | 20.184 ms | 4.848 ms | 1/1 |
| perspective | 4.639 ms | 2.675 ms | 1/1 |
| small | 5.343 ms | 2.562 ms | 1/1 |
| multi (2 codes) | **44.249 ms** | **7.631 ms** | 2/2 |
| camera_like | 6.534 ms | 3.353 ms | 1/1 |

All scenes decode with both engines, but whole-frame multi-code (44 ms) misses the
33 ms budget while the pyramid pipeline does not (7.6 ms). This is the measurable
case for crop decoding. `cargo test -p qr_race_fixtures` asserts every scene's
payloads.

## What this means

1. **Whole-frame decoding is not the budget problem at these sizes.** Real
   textured 1080p frames scan in ~6.0 ms and 720p frames in ~2.9 ms in release;
   a synthetic 720p scene is ~4.7 ms. The 33 ms budget has room for detection and
   super-resolution on this machine.
2. **720p remains the scanning default.** Less data, less power, and more headroom
   for the CNN detector and super-resolution; the command-line client already
   requests it explicitly.
3. **Capture delivered ~24 fps (1080p) and ~28 fps (720p requested)** in this run,
   so the 30 fps requirement still needs a faster source (phone camera or a
   synthetic sequence) to be validated end-to-end.
4. **Cropping is what buys headroom.** The pyramid pass already pays for itself on
   a single-code 720p frame; with more codes the decode side stays cheap per crop
   while the detection pass stays flat.


