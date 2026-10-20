/// A shareable connection link that fills the connect form.
///
///     qrrace://connect?server=https%3A%2F%2F...&fingerprint=...&token=...
///
/// A pasted bare query (`?server=...`) is also accepted so a link can be sent
/// through chat and copied without its scheme.
class ConnectLink {
  const ConnectLink({
    required this.server,
    this.fingerprint,
    this.token,
    this.room,
  });

  final String server;
  final String? fingerprint;
  final String? token;

  /// Room address for an invitation link; after login the app enters it.
  final String? room;
}

ConnectLink? parseConnectLink(String raw) {
  final text = raw.trim();
  if (text.isEmpty) return null;
  final uri = Uri.tryParse(text);
  if (uri == null) return null;
  final isScheme = uri.scheme == 'qrrace' && uri.host == 'connect';
  final hasQuery = uri.queryParameters.containsKey('server') ||
      uri.queryParameters.containsKey('url');
  if (!isScheme && !hasQuery) return null;

  final params = uri.queryParameters;
  final server = (params['server'] ?? params['url'] ?? '').trim();
  if (server.isEmpty) return null;
  final fingerprint = (params['fingerprint'] ?? '').trim();
  final token = (params['token'] ?? '').trim();
  final room = (params['room'] ?? '').trim();
  return ConnectLink(
    server: server,
    fingerprint: fingerprint.isEmpty ? null : fingerprint,
    token: token.isEmpty ? null : token,
    room: room.isEmpty ? null : room,
  );
}

final _sha256Hex = RegExp(r'^[0-9a-fA-F]{64}$');

bool _isIpOrLoopback(String host) {
  if (host == 'localhost' || host == '127.0.0.1' || host == '::1') return true;
  return RegExp(r'^\d{1,3}(\.\d{1,3}){3}$').hasMatch(host);
}

/// Builds a shareable connect link. Self-signed deployments use an IP/localhost
/// URL, so an HTTPS link to one must pin the certificate fingerprint; refusing
/// to build an unpinned link prevents a link that can only fail the handshake.
String buildConnectLink({
  required String server,
  String? fingerprint,
  String? token,
  String? room,
}) {
  final uri = Uri.tryParse(server);
  final pinRequired = uri != null &&
      uri.scheme == 'https' &&
      _isIpOrLoopback(uri.host);
  final cleanFingerprint = fingerprint?.trim() ?? '';
  if (pinRequired && !_sha256Hex.hasMatch(cleanFingerprint)) {
    throw ArgumentError(
      'self-signed HTTPS connect links must carry a 64-hex certificate fingerprint',
    );
  }
  final params = <String, String>{'server': server};
  if (cleanFingerprint.isNotEmpty) {
    params['fingerprint'] = cleanFingerprint;
  }
  if (token != null && token.isNotEmpty) {
    params['token'] = token;
  }
  if (room != null && room.isNotEmpty) {
    params['room'] = room;
  }
  return Uri(scheme: 'qrrace', host: 'connect', queryParameters: params)
      .toString();
}
