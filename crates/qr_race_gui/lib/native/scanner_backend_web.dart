import 'scanner_types.dart';

export 'scanner_types.dart';

/// The Rust wasm scanner was removed with the zxing-cpp migration. The native
/// Android/desktop builds use zxing-cpp directly; a browser backend needs an
/// Emscripten build or the zxing-wasm package.
ScannerBackend createScannerBackend() => const UnsupportedScannerBackend(
      'Web 扫码在 zxing-cpp 迁移后暂不可用',
    );

class UnsupportedScannerBackend implements ScannerBackend {
  const UnsupportedScannerBackend(this.reason);

  final String reason;

  @override
  void start({
    required int camera,
    required int width,
    required int height,
    required int fps,
  }) {
    throw UnsupportedError(reason);
  }

  @override
  void stop() {}

  @override
  ScannerSnapshot poll() => throw StateError('the scanner never started');

  @override
  void dispose() {}
}
