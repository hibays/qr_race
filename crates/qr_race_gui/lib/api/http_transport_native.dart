import 'dart:io';

import 'package:http/http.dart' as http;
import 'package:http/io_client.dart';
import 'package:web_socket_channel/io.dart';
import 'package:web_socket_channel/web_socket_channel.dart';

import 'cert_fingerprint.dart';

HttpClient _httpClient(String? fingerprint) {
  final client = HttpClient();
  if (fingerprint != null) {
    client.badCertificateCallback = (certificate, host, port) =>
        matchesFingerprint(certificate.der, fingerprint);
  }
  return client;
}

http.Client createHttpClient(String? fingerprint) => IOClient(_httpClient(fingerprint));

WebSocketChannel openWebSocket(Uri uri, String? fingerprint) =>
    IOWebSocketChannel.connect(uri, customClient: _httpClient(fingerprint));
