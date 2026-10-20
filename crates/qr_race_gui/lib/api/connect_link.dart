/// A shareable connection link that fills the connect form.
///
///     qrrace://connect?server=https%3A%2F%2F...&fingerprint=...&token=...
///
/// A pasted bare query (`?server=...`) is also accepted so a link can be sent
/// through chat and copied without its scheme.
class ConnectLink {
  const ConnectLink({required this.server, this.fingerprint, this.token});

  final String server;
  final String? fingerprint;
  final String? token;
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
  return ConnectLink(
    server: server,
    fingerprint: fingerprint.isEmpty ? null : fingerprint,
    token: token.isEmpty ? null : token,
  );
}

String buildConnectLink({
  required String server,
  String? fingerprint,
  String? token,
}) {
  final params = <String, String>{'server': server};
  if (fingerprint != null && fingerprint.isNotEmpty) {
    params['fingerprint'] = fingerprint;
  }
  if (token != null && token.isNotEmpty) {
    params['token'] = token;
  }
  return Uri(scheme: 'qrrace', host: 'connect', queryParameters: params)
      .toString();
}
