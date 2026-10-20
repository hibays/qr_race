// Starts a real qr_race_server through the desktop self-host launcher, then
// authenticates over HTTPS with the generated account and pinned certificate.
//
//   cargo build --release -p qr_race_server
//   dart run tool/selfhost_probe.dart
//
// ignore_for_file: avoid_print

import 'package:qr_race_gui/api/api_client.dart';
import 'package:qr_race_gui/selfhost/self_host_launcher_native.dart';
import 'package:qr_race_gui/selfhost/self_host_types.dart';

Future<void> main() async {
  final launcher = ProcessServerLauncher();
  try {
    final info = await launcher.start(port: 18470);
    print('url: ${info.url}');
    print('fingerprint: ${info.fingerprint}');
    print('data: ${info.dataDirectory}');

    final api = ApiClient(baseUrl: info.url, fingerprint: info.fingerprint);
    final session = await api.authenticate(
      token: info.hostToken,
      code: totpCode(info.hostSecret),
    );
    print('authenticated as ${session.userId} (admin: ${session.admin})');
    final rooms = await api.lobby();
    print('lobby rooms: ${rooms.length}');
  } finally {
    await launcher.stop();
    print('server stopped');
  }
}
