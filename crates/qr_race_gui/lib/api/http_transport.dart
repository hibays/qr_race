/// Picks the transport for the current platform.
///
/// The native one pins a self-signed certificate by SHA-256 fingerprint; the web
/// one relies on the browser's trust prompt.
library;

export 'http_transport_stub.dart'
    if (dart.library.io) 'http_transport_native.dart'
    if (dart.library.js_interop) 'http_transport_web.dart';
