import 'package:flutter/foundation.dart';
import 'package:flutter/services.dart';
import 'package:shared_preferences/shared_preferences.dart';

/// Connection details that survive an app restart. The token is intentionally
/// persisted so a validated client can reopen the lobby without asking again.
class ServerSettings {
  static const _serverKey = 'server_url';
  static const _fingerprintKey = 'server_fingerprint';
  static const _tokenKey = 'server_token';
  static const _channel = MethodChannel('app.qrrace.qr_race_gui/storage');

  const ServerSettings({
    required this.server,
    required this.fingerprint,
    this.token = '',
  });

  final String server;
  final String fingerprint;
  final String token;

  static bool get _useAndroidStore =>
      !kIsWeb && defaultTargetPlatform == TargetPlatform.android;

  static Future<ServerSettings> load() async {
    if (_useAndroidStore) {
      try {
        final values = await _channel.invokeMapMethod<String, String>('load');
        if (values != null) {
          return ServerSettings(
            server: values['server']?.isNotEmpty == true
                ? values['server']!
                : 'http://127.0.0.1:8080',
            fingerprint: values['fingerprint'] ?? '',
            token: values['token'] ?? '',
          );
        }
      } catch (_) {
        // Fall through to the plugin store.
      }
    }
    try {
      final prefs = await SharedPreferences.getInstance();
      return ServerSettings(
        server: prefs.getString(_serverKey) ?? 'http://127.0.0.1:8080',
        fingerprint: prefs.getString(_fingerprintKey) ?? '',
        token: prefs.getString(_tokenKey) ?? '',
      );
    } catch (_) {
      // A broken platform prefs store must not block the connect screen.
      return const ServerSettings(
        server: 'http://127.0.0.1:8080',
        fingerprint: '',
      );
    }
  }

  static Future<void> save({
    required String server,
    required String fingerprint,
    String token = '',
  }) async {
    if (_useAndroidStore) {
      try {
        await _channel.invokeMethod<void>('save', {
          'server': server.trim(),
          'fingerprint': fingerprint.trim(),
          'token': token.trim(),
        });
        return;
      } catch (_) {
        // Fall through to the plugin store.
      }
    }
    try {
      final prefs = await SharedPreferences.getInstance();
      await prefs.setString(_serverKey, server.trim());
      await prefs.setString(_fingerprintKey, fingerprint.trim());
      await prefs.setString(_tokenKey, token.trim());
    } catch (_) {
      // Persisting convenience settings is best-effort; the session is live.
    }
  }
}

