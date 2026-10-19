// ignore_for_file: avoid_print
// Connects to a running qr_race server over HTTPS, pinning its certificate
// fingerprint. Proves the Dart client path used by the Flutter app works with
// the built-in self-signed server:
//
//   dart run tool/tls_probe.dart --url https://127.0.0.1:18454 \
//     --token <token> --secret <base32-totp-secret> --fingerprint <sha256-hex>

import 'dart:typed_data';

import 'package:crypto/crypto.dart';
import 'package:qr_race_gui/api/api_client.dart';

Uint8List _base32Decode(String input) {
  const alphabet = 'ABCDEFGHIJKLMNOPQRSTUVWXYZ234567';
  var buffer = 0;
  var bits = 0;
  final output = <int>[];
  for (final character in input.toUpperCase().replaceAll('=', '').split('')) {
    final value = alphabet.indexOf(character);
    if (value < 0) throw FormatException('not base32: $character');
    buffer = (buffer << 5) | value;
    bits += 5;
    if (bits >= 8) {
      bits -= 8;
      output.add((buffer >> bits) & 0xff);
    }
  }
  return Uint8List.fromList(output);
}

String _totp(String secret) {
  final counter = DateTime.now().millisecondsSinceEpoch ~/ 1000 ~/ 30;
  final message = ByteData(8)..setUint64(0, counter);
  final digest = Hmac(sha1, _base32Decode(secret))
      .convert(message.buffer.asUint8List());
  final offset = digest.bytes.last & 0x0f;
  final value = ((digest.bytes[offset] & 0x7f) << 24) |
      ((digest.bytes[offset + 1] & 0xff) << 16) |
      ((digest.bytes[offset + 2] & 0xff) << 8) |
      (digest.bytes[offset + 3] & 0xff);
  return (value % 1000000).toString().padLeft(6, '0');
}

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
    code: _totp(options['secret'] ?? (throw ArgumentError('--secret is required'))),
  );
  print('connected as ${session.userId} (${session.sessionId.substring(0, 8)}…)');
  print('lobby rooms: ${(await api.lobby()).length}');
}
