import 'scanner_types.dart';

export 'scanner_types.dart';

ScannerBackend createScannerBackend() => const UnsupportedScannerBackend(
      '当前平台没有原生扫码后端',
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
    String backend = 'hybrid',
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
