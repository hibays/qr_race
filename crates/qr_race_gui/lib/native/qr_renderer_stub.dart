import 'dart:typed_data';

/// Platforms without a native or wasm encoder cannot render QR codes here.
Future<Uint8List> renderQrWebp(String text) =>
    Future.error(UnsupportedError('no QR encoder on this platform'));
