import 'dart:convert';
import 'dart:ffi';
import 'dart:io';

import 'package:ffi/ffi.dart';
import 'package:flutter/services.dart';

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
    try {
      _startBackend = _library.lookupFunction<
          Int32 Function(Uint32, Uint32, Uint32, Uint32),
          int Function(int, int, int, int)>('qr_scanner_start_backend');
    } catch (_) {
      _startBackend = null;
    }
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

  static const _channel = MethodChannel('app.qrrace.qr_race_gui/camera');

  // The preview is now up to 1280px JPEG (~150 KB); state JSON is small.
  static const int _initialCapacity = 2 * 1024 * 1024;

  final DynamicLibrary _library;
  late final int Function(int, int, int, int) _start;
  int Function(int, int, int, int)? _startBackend;
  late final void Function() _stop;
  late final int Function(Pointer<Uint8>, int) _state;
  late final int Function(Pointer<Uint8>, int) _preview;
  late Pointer<Uint8> _buffer;
  late int _capacity;
  int _lastPreviewSeq = 0;
  Uint8List? _lastPreview;
  bool _started = false;
  int? _previewTextureId;

  @override
  int? get previewTextureId => _previewTextureId;

  @override
  void start({
    required int camera,
    required int width,
    required int height,
    required int fps,
    String backend = 'hybrid',
  }) {
    final mode = switch (backend) {
      'zxing' => 1,
      'wechat' => 2,
      _ => 0,
    };
    final code = _startBackend != null
        ? _startBackend!(camera, width, height, mode)
        : _start(camera, width, height, fps);
    if (code != 0) {
      throw StateError('qr_scanner_start returned $code');
    }
    _started = true;
    if (Platform.isAndroid) {
      _startAndroidCapture(width, height, fps);
    }
  }

  Future<void> _startAndroidCapture(int width, int height, int fps) async {
    try {
      await _channel.invokeMethod<int>('createPreviewTexture');
      final id = await _channel.invokeMethod<int>('startCapture', {
        'width': width,
        'height': height,
        'fps': fps,
      });
      _previewTextureId = id;
    } catch (_) {
      // The FFI state will report the failure; the UI stays usable.
    }
  }

  @override
  void stop() {
    if (!_started) return;
    if (Platform.isAndroid) {
      _stopAndroidCapture();
    }
    _stop();
    _started = false;
    _previewTextureId = null;
  }

  Future<void> _stopAndroidCapture() async {
    try {
      await _channel.invokeMethod<void>('stopCapture');
    } catch (_) {}
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
    // Keep the last frame between preview updates. Returning null here makes
    // Image.memory tear down its layer, which the eye sees as black flicker.
    Uint8List? preview = _lastPreview;
    if (seq != _lastPreviewSeq) {
      final bytes = _preview(_buffer, _capacity);
      if (bytes > 0) {
        preview = Uint8List.fromList(_buffer.asTypedList(bytes));
        _lastPreview = preview;
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
