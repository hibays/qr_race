# Building qr_race

This guide builds the Rust workspace, Android APK, Windows desktop app and web
bundle from a clean checkout. The deployment steps are in
[DEPLOY.md](DEPLOY.md).

## 1. Prerequisites

| Tool | Version used by this repository |
| --- | --- |
| Rust | 1.99 (`rust-version` in `Cargo.toml`); pinned by `Cargo.lock` |
| Flutter | 3.47.x, Dart 3.13.x |
| Android SDK | Platform 36, Build Tools 36.x |
| Android NDK | 30.x |
| PowerShell | 7.x for the build scripts |
| Python | 3.11+ (release helper scripts) |

The Rust workspace vendors nothing; dependencies are fetched from the configured
registry (`rsproxy-sparse` in this environment). Flutter uses the pinned
`pubspec.lock`.

## 2. Rust dependencies are locked

`Cargo.lock` is committed and is part of the build contract.

```powershell
# Fetch exactly what the lockfile selects; fail if Cargo.lock is stale.
cargo fetch --locked
cargo metadata --locked --no-deps | Out-Null
```

### Updating dependencies

Do this in a dedicated commit:

```powershell
cargo update -v
git diff -- Cargo.lock
```

`cargo update` respects `rust-version = "1.99"` in the workspace manifest. It may
therefore **downgrade** a transitive crate that was previously resolved with a
newer compiler; that is expected and is why the lockfile diff must be reviewed.

After any update, the regression gate is mandatory:

```powershell
pwsh -File scripts\verify.ps1
```

The gate runs:

```text
cargo test --workspace --locked
flutter analyze
flutter test
```

Never ship a release built with a different lockfile than the one tested.

## 3. ONNX Runtime for Rust tests

The detector/SR tests need the ORT 1.29 shared library. The repository keeps it
under `.refs/ort129-root` for local development:

```powershell
$env:ORT_DYLIB_PATH = "D:\Belong\Case\qr_race\.refs\ort129-root\lib\onnxruntime.dll"
cargo test --workspace --locked
```

Without `ORT_DYLIB_PATH`, tests that load `models/detect.onnx` and
`models/sr.onnx` fail with `Unsupported model IR version`.

## 4. Build the Rust workspace

```powershell
cargo build --release --locked
```

Useful binaries:

```text
target\release\qr_race_server.exe
target\release\qr_scan.exe
target\release\qr_bench.exe
```

## 5. Android

### 5.1 Native FFI

The Android app scans in Rust, so the ABI-specific FFI library must be built
before Gradle packages the APK:

```powershell
pwsh -File scripts\build-android-ffi.ps1 -Abi arm64-v8a
```

Repeat for `armeabi-v7a` and `x86_64` when producing all split APKs.
`scripts\build_android.ps1` does this for every ABI.

The local Gradle distribution can be used when the machine has no working
Gradle download:

```powershell
pwsh -File scripts\build_android.ps1 `
  -GradleDistribution D:\Belong\Case\qr_race\.refs\gradle-9.3.1-bin.zip
```

Artifacts:

```text
dist\android\qr_race-1.0.0-arm64-v8a-release.apk
dist\android\qr_race-1.0.0-armeabi-v7a-release.apk
dist\android\qr_race-1.0.0-x86_64-release.apk
```

### 5.2 Plugin/native mismatch

If a release APK throws `NoSuchMethodError` from a plugin registrar (for example
`SharedPreferencesApiRegistrar.getAll`), the Gradle/Flutter plugin cache is
stale. Rebuild clean:

```powershell
cd crates\qr_race_gui
flutter clean
flutter pub get
cd ..\..
pwsh -File scripts\build_android.ps1
```

Do not ship the APK from before `flutter clean`.

## 6. Windows desktop

```powershell
pwsh -File scripts\build_windows.ps1
```

Output:

```text
dist\windows\qr_race-1.0.0-windows-x64-release.zip
```

The ZIP contains `qr_race_gui.exe`, `qr_race_server.exe`, `qr_race_ffi.dll`,
`onnxruntime.dll` and the Flutter data directory.

## 7. Web (scanner fallback)

```powershell
pwsh -File scripts\build_web.ps1
```

Output:

```text
dist\web\qr_race-1.0.0-web.zip
```

The web build uses the browser camera and the WASM/ZXing fallback; it does not
carry the Android Camera2 zero-copy path.

## 8. Regression gate

`scripts\verify.ps1` is the single command to run before a release:

```powershell
pwsh -File scripts\verify.ps1
```

It sets `ORT_DYLIB_PATH` when the local runtime exists, runs Rust with
`--locked`, then Flutter analyze and test. A dependency update is only accepted
when this passes and the `Cargo.lock` diff is intentional.

## 9. Test layers

| Layer | Command | Covers |
| --- | --- | --- |
| Address/domain | `cargo test -p qr_race_core` | room addresses, 南极 PIN namespace, relays |
| Hub | `cargo test -p qr_race_hub` | rooms, members, relays, TTL, PIN mapping |
| API | `cargo test -p qr_race_api` | auth, lobby, room lifecycle, PIN start |
| Pipeline | `cargo test -p qr_race_pipeline` | hybrid/zxing/CNN modes, SR routing |
| GUI | `flutter test` | connect, persistence, lit banner, inactive strip, QR dialog |

