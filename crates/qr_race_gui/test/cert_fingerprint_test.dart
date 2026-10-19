import 'dart:convert';

import 'package:flutter_test/flutter_test.dart';
import 'package:qr_race_gui/api/cert_fingerprint.dart';

const _abcDigest =
    'ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad';

void main() {
  test('a fingerprint can be typed with colons and upper case', () {
    final pretty = _abcDigest
        .toUpperCase()
        .replaceAllMapped(RegExp('(.{2})'), (match) => '${match[1]}:')
        .replaceAll(RegExp(r':$'), '');
    expect(normalizeFingerprint(pretty), _abcDigest);
    expect(normalizeFingerprint('  '), isNull, reason: 'empty means unpinned');
  });

  test('a short fingerprint is rejected', () {
    expect(() => normalizeFingerprint('abcd'), throwsFormatException);
  });

  test('matching is a plain SHA-256 comparison', () {
    expect(matchesFingerprint(utf8.encode('abc'), _abcDigest), isTrue);
    expect(matchesFingerprint(utf8.encode('abd'), _abcDigest), isFalse);
    expect(matchesFingerprint(utf8.encode('abc'), null), isFalse);
  });

  test('a PEM certificate yields the same fingerprint as its DER', () {
    final pem = '-----BEGIN CERTIFICATE-----\n${base64.encode(utf8.encode('abc'))}\n'
        '-----END CERTIFICATE-----\n';
    expect(fingerprintOfPem(pem), _abcDigest);
  });
}
