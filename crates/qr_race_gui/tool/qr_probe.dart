// Renders a QR through the native FFI and writes the WebP bytes.
//
//   cargo build --release -p qr_race_ffi
//   $env:QR_RACE_FFI = "target\release\qr_race_ffi.dll"
//   dart run tool/qr_probe.dart "https://example.test/relay" .refs/qr-probe.webp
//
// ignore_for_file: avoid_print

import 'dart:io';

import 'package:qr_race_gui/native/qr_renderer.dart';

Future<void> main(List<String> args) async {
  final payload = args.isNotEmpty ? args[0] : 'https://example.test/relay';
  final path = args.length > 1 ? args[1] : '.refs/qr-probe.webp';
  final bytes = await renderQrWebp(payload);
  await File(path).writeAsBytes(bytes);
  final header = String.fromCharCodes(bytes.take(4));
  print('$path: ${bytes.length} bytes, header=$header');
}
