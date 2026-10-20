import 'dart:convert';
import 'dart:typed_data';

import 'package:crypto/crypto.dart';

/// One running self-hosted server and the credentials it was provisioned with.
class SelfHostInfo {
  const SelfHostInfo({
    required this.url,
    required this.fingerprint,
    required this.dataDirectory,
    required this.hostToken,
    required this.hostSecret,
    required this.memberToken,
    required this.memberSecret,
  });

  final String url;
  final String fingerprint;
  final String dataDirectory;
  final String hostToken;
  final String hostSecret;
  final String memberToken;
  final String memberSecret;
}

/// Starts and stops a server binary on the local machine.
abstract class ServerLauncher {
  Future<SelfHostInfo> start({int port = 8443});

  Future<void> stop();
}

/// RFC 6238 TOTP, six digits, 30-second steps (SHA-1).
String totpCode(String base32Secret, {DateTime? at, int digits = 6}) {
  final key = _base32Decode(base32Secret);
  final seconds = (at ?? DateTime.now()).millisecondsSinceEpoch ~/ 1000;
  final counter = seconds ~/ 30;
  final message = ByteData(8)..setUint64(0, counter);
  final digest = Hmac(sha1, key).convert(message.buffer.asUint8List());
  final offset = digest.bytes.last & 0x0f;
  final value = ((digest.bytes[offset] & 0x7f) << 24) |
      ((digest.bytes[offset + 1] & 0xff) << 16) |
      ((digest.bytes[offset + 2] & 0xff) << 8) |
      (digest.bytes[offset + 3] & 0xff);
  final modulus = _pow10(digits);
  return (value % modulus).toString().padLeft(digits, '0');
}

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

int _pow10(int digits) {
  var value = 1;
  for (var index = 0; index < digits; index++) {
    value *= 10;
  }
  return value;
}

String encodeSelfHostJson(SelfHostInfo info) => jsonEncode({
      'url': info.url,
      'fingerprint': info.fingerprint,
      'host_token': info.hostToken,
    });
