import 'package:http/http.dart' as http;
import 'package:web_socket_channel/web_socket_channel.dart';

/// Fallback for platforms with neither `dart:io` nor browser interop.
http.Client createHttpClient(String? fingerprint) => http.Client();

WebSocketChannel openWebSocket(Uri uri, String? fingerprint) =>
    WebSocketChannel.connect(uri);
