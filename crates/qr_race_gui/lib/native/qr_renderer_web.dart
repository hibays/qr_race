import 'dart:typed_data';

/// Web QR rendering used the removed Rust wasm module. Native Android/desktop
/// still render through the FFI; this stub keeps the web build honest until a
/// browser QR encoder is added.
Future<Uint8List> renderQrWebp(String text) async {
  throw UnsupportedError('Web 二维码生成暂不可用');
}
