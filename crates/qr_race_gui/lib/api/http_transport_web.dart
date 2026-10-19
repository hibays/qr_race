import 'package:http/http.dart' as http;
import 'package:web_socket_channel/web_socket_channel.dart';

/// Browsers own their trust store; the fingerprint is accepted but unused.
/// Users accept the self-signed certificate once in the browser.
http.Client createHttpClient(String? fingerprint) => http.Client();

WebSocketChannel openWebSocket(Uri uri, String? fingerprint) =>
    WebSocketChannel.connect(uri);
