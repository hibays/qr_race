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
(rxing multi-symbol reader, no CNN detector yet).

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

## wasm scan core (Node 24, release)

`node tools/wasm_scan_probe.cjs` scans a dumped 1280×720 grayscale scene through
the wasm build, including the JS↔wasm copies:

| run | wall time | hits |
|---|---|---|
| 1 | 21.74 ms | 1 |
| 2 | 18.28 ms | 1 |

This is the scan core only. A browser adds the RGBA→gray loop and canvas
readback; the web camera path has not been measured in a real browser yet.

**Update:** the browser camera path now has a headless check
(`scripts/browser-scan-probe.ps1`): Edge + fake camera at 640×480, QR detected
on the third frame, `scan_ms` **1.8 ms** for the wasm scan. That is the whole
`getUserMedia` → canvas → grayscale → wasm path, with the caveat that the
"camera" is a Y4M file.

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
