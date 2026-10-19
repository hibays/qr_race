/// Picks the QR encoder for the current platform: native FFI (desktop/mobile)
/// or wasm (web). Both emit 2048×2048 lossless WebP from rxing.
library;

export 'qr_renderer_stub.dart'
    if (dart.library.io) 'qr_renderer_native.dart'
    if (dart.library.js_interop) 'qr_renderer_web.dart';
