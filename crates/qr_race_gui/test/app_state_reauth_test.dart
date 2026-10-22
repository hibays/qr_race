import 'dart:convert';
import 'dart:io';

import 'package:flutter_test/flutter_test.dart';
import 'package:qr_race_gui/state/app_state.dart';

void main() {
  test(
    'lobby re-authenticates once when the server forgot the session',
    () async {
      var authCount = 0;
      final server = await HttpServer.bind(InternetAddress.loopbackIPv4, 0);
      addTearDown(() => server.close(force: true));

      server.listen((request) async {
        final body = await utf8.decoder.bind(request).join();
        if (request.uri.path == '/v1/auth' && request.method == 'POST') {
          authCount++;
          final token = (jsonDecode(body) as Map)['token'];
          expect(token, 'host-token');
          request.response.headers.contentType = ContentType.json;
          request.response.write(
            jsonEncode({
              'session_id': 's-$authCount',
              'user_id': 'host',
              'expires_at_ms': 9999999999999,
              'admin': true,
            }),
          );
          await request.response.close();
          return;
        }
        if (request.uri.path == '/v1/lobby') {
          final authorization = request.headers.value('authorization');
          if (authorization != 'Bearer s-2') {
            request.response.statusCode = HttpStatus.unauthorized;
            request.response.write('session expired');
            await request.response.close();
            return;
          }
          request.response.headers.contentType = ContentType.json;
          request.response.write('{"rooms":[]}');
          await request.response.close();
          return;
        }
        request.response.statusCode = HttpStatus.notFound;
        await request.response.close();
      });

      final state = AppState();
      addTearDown(state.disconnect);
      final address = 'http://127.0.0.1:${server.port}';
      expect(await state.connect(server: address, token: 'host-token'), isTrue);
      expect(state.session?.sessionId, 's-1');
      final clientBeforeRetry = state.api;

      await state.refreshLobby();

      expect(authCount, 2);
      expect(state.session?.sessionId, 's-2');
      expect(identical(state.api, clientBeforeRetry), isTrue);
      expect(state.error, isNull);
      expect(state.lobby, isEmpty);
    },
  );
}
