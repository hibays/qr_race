import 'dart:convert';
import 'dart:ffi';
import 'dart:typed_data';

import 'package:ffi/ffi.dart';

import 'ffi_library.dart';

/// Renders QR codes with qrcode through the native C ABI: 2048×2048 lossless
/// WebP, quality 90 / method 4 — the same parameters as rcrm's bridge.
class NativeQrRenderer {
  NativeQrRenderer() : _library = openQrRaceFfi() {
    _render = _library.lookupFunction<
        Int32 Function(Pointer<Uint8>, IntPtr, Pointer<Uint8>, IntPtr),
        int Function(Pointer<Uint8>, int, Pointer<Uint8>, int)>('qr_qr_webp');
  }

  final DynamicLibrary _library;
  late final int Function(Pointer<Uint8>, int, Pointer<Uint8>, int) _render;

  Uint8List render(String text) {
    final payload = utf8.encode(text);
    if (payload.isEmpty) throw ArgumentError('empty QR payload');

    final textPointer = calloc<Uint8>(payload.length);
    textPointer.asTypedList(payload.length).setAll(0, payload);
    try {
      var capacity = 32 * 1024;
      while (true) {
        final output = calloc<Uint8>(capacity);
        try {
          final written =
              _render(textPointer, payload.length, output, capacity);
          if (written == 0 || written == -1) {
            throw StateError('qr_qr_webp failed ($written)');
          }
          if (written < 0) {
            capacity = -written;
            continue;
          }
          return Uint8List.fromList(output.asTypedList(written));
        } finally {
          calloc.free(output);
        }
      }
    } finally {
      calloc.free(textPointer);
    }
  }
}

final NativeQrRenderer _renderer = NativeQrRenderer();

Future<Uint8List> renderQrWebp(String text) =>
    Future.sync(() => _renderer.render(text));

