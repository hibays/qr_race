import 'dart:js_interop';
import 'dart:typed_data';

@JS('qrRaceQrWebp')
external JSPromise<JSAny?> _qrWebp(JSString text);

/// Encodes in wasm (rxing, 2048×2048 lossless WebP) and returns the bytes.
Future<Uint8List> renderQrWebp(String text) async {
  final result = await _qrWebp(text.toJS).toDart;
  return (result as JSUint8Array).toDart;
}
