import 'dart:convert';

import 'package:crypto/crypto.dart';

/// Normalizes a certificate fingerprint typed by a human: accepts separators
/// and upper case, and returns lowercase hex. Empty means "not pinned".
String? normalizeFingerprint(String? raw) {
  if (raw == null) return null;
  final cleaned = raw.replaceAll(RegExp('[^0-9a-fA-F]'), '').toLowerCase();
  if (cleaned.isEmpty) return null;
  if (cleaned.length != 64) {
    throw const FormatException('证书指纹必须是 64 位十六进制（SHA-256）');
  }
  return cleaned;
}

String fingerprintOfDer(List<int> der) => sha256.convert(der).toString();

/// True when [der] (the server certificate's DER) matches the pinned value.
bool matchesFingerprint(List<int> der, String? expected) {
  if (expected == null) return false;
  return fingerprintOfDer(der) == expected;
}

/// The fingerprint as the server prints it (lowercase hex, no separators).
String displayFingerprint(String fingerprint) => fingerprint.toLowerCase();

/// A fingerprint out of a PEM file, for tooling; the API server prints the same.
String? fingerprintOfPem(String pem) {
  final match = RegExp(
    r'-----BEGIN CERTIFICATE-----([\s\S]*?)-----END CERTIFICATE-----',
  ).firstMatch(pem);
  if (match == null) return null;
  final body = match.group(1)!.replaceAll(RegExp(r'\s'), '');
  return fingerprintOfDer(base64.decode(body));
}
