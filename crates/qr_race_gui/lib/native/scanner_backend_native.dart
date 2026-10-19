import 'dart:convert';
import 'dart:ffi';
import 'dart:io';
import 'dart:typed_data';

import 'package:ffi/ffi.dart';

import 'scanner_types.dart';

export 'scanner_types.dart';

/// Loads `qr_race_ffi` and talks to the C ABI.
class FfiScannerBackend implements ScannerBackend {
  FfiScannerBackend({String? libraryPath}) : _library = _open(libraryPath) {
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

  static DynamicLibrary _open(String? overridePath) {
    if (!Platform.isWindows && !Platform.isLinux && !Platform.isMacOS) {
      throw UnsupportedError('the native scanner is desktop-only for now');
    }
    final candidates = <String>[
      ?overridePath,
      ?Platform.environment['QR_RACE_FFI'],
      _besideExecutable(),
      _fromProjectTarget(),
      _platformName(),
    ];
    for (final candidate in candidates) {
      if (candidate.isEmpty || !File(candidate).existsSync()) continue;
      return DynamicLibrary.open(candidate);
    }
    throw StateError(
      'could not find ${_platformName()}; build it with '
      '`cargo build -p qr_race_ffi --release` or set QR_RACE_FFI',
    );
  }

  static String _besideExecutable() {
    final dir = File(Platform.resolvedExecutable).parent.path;
    return '$dir${Platform.pathSeparator}${_platformName()}';
  }

  static String _fromProjectTarget() {
    // build/windows/x64/runner/<cfg>/app.exe -> project root -> target/release
    var dir = File(Platform.resolvedExecutable).parent.path;
    for (var i = 0; i < 7; i++) {
      final parent = Directory(dir).parent.path;
      if (parent == dir) break;
      dir = parent;
    }
    return '$dir${Platform.pathSeparator}target${Platform.pathSeparator}release'
        '${Platform.pathSeparator}${_platformName()}';
  }

  static String _platformName() {
    if (Platform.isWindows) return 'qr_race_ffi.dll';
    if (Platform.isMacOS) return 'libqr_race_ffi.dylib';
    return 'libqr_race_ffi.so';
  }

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
