/// Platform scanner entry point.
///
/// The FFI implementation imports `dart:ffi`/`dart:io`, which do not exist on
/// the web; conditional imports keep the six-target build honest while the wasm
/// scanner is still pending.
library;

export 'scanner_types.dart';

import 'scanner_backend_stub.dart'
    if (dart.library.io) 'scanner_backend_native.dart'
    if (dart.library.js_interop) 'scanner_backend_web.dart' as impl;
import 'scanner_types.dart';

ScannerBackend createScannerBackend() => impl.createScannerBackend();
