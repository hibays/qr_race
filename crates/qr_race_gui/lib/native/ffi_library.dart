import 'dart:ffi';
import 'dart:io';

/// Locates and opens `qr_race_ffi` on desktop platforms. Shared by the scanner
/// session and the QR renderer so both use the same library lookup rules.
DynamicLibrary openQrRaceFfi({String? libraryPath}) {
  if (!Platform.isWindows && !Platform.isLinux && !Platform.isMacOS) {
    throw UnsupportedError('the native qr_race library is desktop-only for now');
  }
  final candidates = <String>[
    ?libraryPath,
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

String _besideExecutable() {
  final dir = File(Platform.resolvedExecutable).parent.path;
  return '$dir${Platform.pathSeparator}${_platformName()}';
}

String _fromProjectTarget() {
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

String _platformName() {
  if (Platform.isWindows) return 'qr_race_ffi.dll';
  if (Platform.isMacOS) return 'libqr_race_ffi.dylib';
  return 'libqr_race_ffi.so';
}
