import 'dart:convert';
import 'dart:ffi';
import 'dart:typed_data';

import 'package:ffi/ffi.dart';

import 'ffi_library.dart';
import 'scanner_types.dart';

export 'scanner_types.dart';

/// Loads `qr_race_ffi` and talks to the C ABI.
class FfiScannerBackend implements ScannerBackend {
  FfiScannerBackend({String? libraryPath})
      : _library = openQrRaceFfi(libraryPath: libraryPath) {
    _start = _library.lookupFunction<
        Int32 Function(Uint32, Uint32, Uint32, Uint32),
        int Function(int, int, int, int)>('qr_scanner_start');
    _stop = _library.lookupFunction<Void Function(), void Function()>(
      'qr_scanner_stop',
    );
    _state = _library.lookupFunction<
        Int32 Function(Pointer<Uint8>, IntPtr),
        int Function(Pointer<Uint8>, int)>('qr_scanner_state');
    _preview = _library.lookupFunction<
        Int32 Function(Pointer<Uint8>, IntPtr),
        int Function(Pointer<Uint8>, int)>('qr_scanner_preview');
    _buffer = calloc<Uint8>(_initialCapacity);
    _capacity = _initialCapacity;
  }

  static const int _initialCapacity = 128 * 1024;

  final DynamicLibrary _library;
  late final int Function(int, int, int, int) _start;
  late final void Function() _stop;
  late final int Function(Pointer<Uint8>, int) _state;
  late final int Function(Pointer<Uint8>, int) _preview;
  late Pointer<Uint8> _buffer;
  late int _capacity;
  int _lastPreviewSeq = 0;
  bool _started = false;

  @override
  void start({
    required int camera,
    required int width,
    required int height,
    required int fps,
  }) {
    final code = _start(camera, width, height, fps);
    if (code != 0) {
      throw StateError('qr_scanner_start returned $code');
    }
    _started = true;
  }

  @override
  void stop() {
    if (!_started) return;
    _stop();
    _started = false;
  }

  @override
  ScannerSnapshot poll() {
    final length = _state(_buffer, _capacity);
    if (length <= 0) {
      throw StateError('qr_scanner_state returned $length');
    }
    final json = jsonDecode(utf8.decode(_buffer.asTypedList(length)))
        as Map<String, dynamic>;
    final seq = json['preview_seq'] as int? ?? 0;
    Uint8List? preview;
    if (seq != _lastPreviewSeq) {
      final bytes = _preview(_buffer, _capacity);
      if (bytes > 0) {
        preview = Uint8List.fromList(_buffer.asTypedList(bytes));
        _lastPreviewSeq = seq;
      }
    }
    return ScannerSnapshot.fromJson(json, preview: preview);
  }

  @override
  void dispose() {
    stop();
    calloc.free(_buffer);
  }
}

ScannerBackend createScannerBackend() => FfiScannerBackend();
