import 'package:shared_preferences/shared_preferences.dart';

/// Server connection details that survive an app restart. The token is
/// deliberately not stored: it is a credential, not a server setting.
class ServerSettings {
  static const _serverKey = 'server_url';
  static const _fingerprintKey = 'server_fingerprint';

  const ServerSettings({required this.server, required this.fingerprint});

  final String server;
  final String fingerprint;

  static Future<ServerSettings> load() async {
    final prefs = await SharedPreferences.getInstance();
    return ServerSettings(
      server: prefs.getString(_serverKey) ?? 'http://127.0.0.1:8080',
      fingerprint: prefs.getString(_fingerprintKey) ?? '',
    );
  }

  static Future<void> save({
    required String server,
    required String fingerprint,
  }) async {
    final prefs = await SharedPreferences.getInstance();
    await prefs.setString(_serverKey, server.trim());
    await prefs.setString(_fingerprintKey, fingerprint.trim());
  }
}
