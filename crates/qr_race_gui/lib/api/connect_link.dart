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
    this.secret,
    this.room,
  });

  final String server;
  final String? fingerprint;
  final String? token;

  /// Optional TOTP secret. When present the connect screen generates the code
  /// itself, so opening a self-host link is one tap.
  final String? secret;

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
  final secret = (params['secret'] ?? '').trim();
  final room = (params['room'] ?? '').trim();
  return ConnectLink(
    server: server,
    fingerprint: fingerprint.isEmpty ? null : fingerprint,
    token: token.isEmpty ? null : token,
    secret: secret.isEmpty ? null : secret,
    room: room.isEmpty ? null : room,
  );
}

String buildConnectLink({
  required String server,
  String? fingerprint,
  String? token,
  String? secret,
  String? room,
}) {
  final params = <String, String>{'server': server};
  if (fingerprint != null && fingerprint.isNotEmpty) {
    params['fingerprint'] = fingerprint;
  }
  if (token != null && token.isNotEmpty) {
    params['token'] = token;
  }
  if (secret != null && secret.isNotEmpty) {
    params['secret'] = secret;
  }
  if (room != null && room.isNotEmpty) {
    params['room'] = room;
  }
  return Uri(scheme: 'qrrace', host: 'connect', queryParameters: params)
      .toString();
}
