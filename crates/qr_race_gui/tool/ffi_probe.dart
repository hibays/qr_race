// Manual probe: drives the real native scanner through the same Dart FFI
// bindings the app uses. Not a test (it needs a camera and the built DLL).
//
//   cargo build -p qr_race_ffi --release
//   cd crates/qr_race_gui && dart run tool/ffi_probe.dart

import 'dart:io';

import 'package:qr_race_gui/native/scanner_backend_native.dart';

Future<void> main() async {
  final path = Platform.environment['QR_RACE_FFI'] ??
      (Platform.isWindows
          ? r'..\..\target\release\qr_race_ffi.dll'
          : '../../target/release/libqr_race_ffi.so');

  final backend = FfiScannerBackend(libraryPath: path);
  backend.start(camera: 0, width: 1280, height: 720, fps: 30);
  var lastPreview = 0;
  for (var i = 0; i < 10; i++) {
    await Future<void>.delayed(const Duration(milliseconds: 300));
    final snapshot = backend.poll();
    lastPreview = snapshot.preview?.length ?? lastPreview;
    stdout.writeln(
      'running=${snapshot.running} ${snapshot.width}x${snapshot.height} '
      'frames=${snapshot.frames} scanMs=${snapshot.scanMs.toStringAsFixed(2)} '
      'hits=${snapshot.hits.length} preview=$lastPreview '
      'error=${snapshot.error}',
    );
  }
  backend.dispose();
  stdout.writeln('stopped');
}
