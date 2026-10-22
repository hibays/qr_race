import 'package:flutter_test/flutter_test.dart';
import 'package:shared_preferences/shared_preferences.dart';

import 'package:qr_race_gui/state/server_settings.dart';

void main() {
  TestWidgetsFlutterBinding.ensureInitialized();

  test('server, fingerprint and token survive save/load', () async {
    SharedPreferences.setMockInitialValues({});
    await ServerSettings.save(
      server: 'https://race.example.test:8443',
      fingerprint: 'abc123',
      token: 'token-value',
    );

    final loaded = await ServerSettings.load();
    expect(loaded.server, 'https://race.example.test:8443');
    expect(loaded.fingerprint, 'abc123');
    expect(loaded.token, 'token-value');
  });
}

