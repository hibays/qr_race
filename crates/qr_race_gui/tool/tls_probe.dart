// ignore_for_file: avoid_print
// Connects to a running qr_race server over HTTPS, pinning its certificate
// fingerprint. Proves the Dart client path used by the Flutter app works with
// the built-in self-signed server:
//
//   dart run tool/tls_probe.dart --url https://127.0.0.1:18454 \
//     --token <token> --fingerprint <sha256-hex>

import 'package:qr_race_gui/api/api_client.dart';

Map<String, String> _options(List<String> args) {
  final options = <String, String>{};
  for (var index = 0; index < args.length - 1; index += 2) {
    final key = args[index];
    if (!key.startsWith('--')) throw FormatException('unexpected $key');
    options[key.substring(2)] = args[index + 1];
  }
  return options;
}

Future<void> main(List<String> args) async {
  final options = _options(args);
  final api = ApiClient(
    baseUrl: options['url'] ?? (throw ArgumentError('--url is required')),
    fingerprint: options['fingerprint'],
  );
  final session = await api.authenticate(
    token: options['token'] ?? (throw ArgumentError('--token is required')),
  );
  print('connected as ${session.userId} (${session.sessionId.substring(0, 8)}…)');
  print('lobby rooms: ${(await api.lobby()).length}');
}
