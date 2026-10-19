import 'dart:convert';

import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:http/http.dart' as http;
import 'package:http/testing.dart';
import 'package:qr_race_gui/api/api_client.dart';
import 'package:qr_race_gui/state/app_state.dart';
import 'package:qr_race_gui/ui/connect_screen.dart';
import 'package:qr_race_gui/ui/theme.dart';

void main() {
  test('normalizes the server url and authenticates', () async {
    final requests = <http.Request>[];
    final client = MockClient((request) async {
      requests.add(request);
      if (request.url.path == '/v1/auth') {
        return _json({
          'session_id': 's-1',
          'user_id': 'host-a',
          'expires_at_ms': 123,
          'admin': true,
        });
      }
      if (request.url.path == '/v1/lobby') {
        return _json({
          'rooms': [
            {
              'label': '东京A区1B级103房',
              'host': 'host-a',
              'member_count': 2,
              'module_count': 0,
            }
          ],
        });
      }
      return http.Response('not found', 404);
    });

    final api = ApiClient(baseUrl: ' 127.0.0.1:8080/ ', httpClient: client);
    expect(api.baseUrl, 'http://127.0.0.1:8080');

    final session = await api.authenticate(token: ' token ', code: '123456');
    expect(session.userId, 'host-a');
    expect(session.admin, isTrue);
    expect(requests.first.headers['content-type'], contains('application/json'));

    final rooms = await api.lobby();
    expect(rooms.single.label, '东京A区1B级103房');
    expect(requests.last.headers['authorization'], 'Bearer s-1');
  });

  test('lists modules and downloads their source', () async {
    final client = MockClient((request) async {
      if (request.url.path == '/v1/modules') {
        return _json({
          'modules': [
            {
              'id': 'overlay',
              'version': '1.0.0',
              'entry': 'main.py',
              'hooks': ['frame'],
              'capabilities': ['ui'],
            }
          ],
        });
      }
      if (request.url.path == '/v1/modules/overlay') {
        return _json({
          'manifest': {
            'id': 'overlay',
            'version': '1.0.0',
            'entry': 'main.py',
            'hooks': ['frame'],
            'capabilities': ['ui'],
          },
          'source': 'print(1)',
        });
      }
      return http.Response('not found', 404);
    });
    final api = ApiClient(baseUrl: 'http://example.test', httpClient: client)
      ..sessionId = 's-1';

    final modules = await api.modules();
    expect(modules.single.id, 'overlay');
    expect(modules.single.hooks, ['frame']);

    final source = await api.moduleSource('overlay');
    expect(source.source, 'print(1)');
    expect(source.manifest.capabilities, ['ui']);
  });

  test('uploads a module manifest and source as an admin', () async {
    final requests = <http.Request>[];
    final client = MockClient((request) async {
      requests.add(request);
      return _json({
        'id': 'overlay',
        'version': '1.0.0',
        'entry': 'main.py',
        'hooks': ['frame'],
        'capabilities': ['ui'],
      });
    });
    final api = ApiClient(baseUrl: 'http://example.test', httpClient: client)
      ..sessionId = 's-1';

    final module = await api.uploadModule(
      id: 'overlay',
      version: '1.0.0',
      entry: 'main.py',
      hooks: ['frame'],
      capabilities: ['ui'],
      source: 'print(1)',
    );

    expect(module.id, 'overlay');
    expect(requests.single.method, 'POST');
    final body = jsonDecode(requests.single.body) as Map<String, dynamic>;
    expect(body['source'], 'print(1)');
    expect(body['hooks'], ['frame']);
  });

  test('server errors become ApiException with the server message', () async {
    final client = MockClient(
      (_) async => _json({'error': 'totp code is invalid or expired'}, status: 401),
    );
    final api = ApiClient(baseUrl: 'http://example.test', httpClient: client);

    await expectLater(
      api.authenticate(token: 't', code: '000000'),
      throwsA(
        isA<ApiException>()
            .having((error) => error.status, 'status', 401)
            .having((error) => error.message, 'message',
                'totp code is invalid or expired'),
      ),
    );
  });

  test('host actions hit the documented endpoints', () async {
    final calls = <http.Request>[];
    final client = MockClient((request) async {
      calls.add(request);
      return _json(<String, Object?>{});
    });
    final api = ApiClient(baseUrl: 'http://example.test', httpClient: client);

    await api.createRoom('东京A区1B级103房');
    await api.lightRoom('东京A区1B级103房');
    await api.promote('东京A区1B级103房', 'm-1');
    await api.demote('东京A区1B级103房', 'm-1');
    await api.setModules('东京A区1B级103房', ['overlay']);

    expect(calls[0].url.path, '/v1/rooms');
    expect(jsonDecode(calls[0].body)['address'], '东京A区1B级103房');
    expect(calls[1].url.path, '/v1/room/light');
    expect(calls[1].url.queryParameters['address'], '东京A区1B级103房');
    expect(calls[2].url.path, '/v1/room/promote');
    expect(jsonDecode(calls[2].body)['target'], 'm-1');
    expect(calls[3].url.path, '/v1/room/demote');
    expect(calls[4].url.path, '/v1/room/modules');
    expect(jsonDecode(calls[4].body)['modules'], ['overlay']);
  });

  test('a room view keeps the member flags and relay bytes', () async {    final client = MockClient((request) async {
      if (request.url.path == '/v1/room') {
        expect(request.url.queryParameters['address'], '西京Z区B级103房');
        return _json({
          'label': '西京Z区B级103房',
          'lit': true,
          'host': 'host-a',
          'modules': <String>[],
          'members': [
            {'user': 'host-a', 'role': 'host', 'last_relay': 'r-1'},
            {'user': 'm-1', 'role': 'co_host', 'last_relay': null},
          ],
          'relays': [
            {
              'id': 'r-1',
              'publisher': 'host-a',
              'kind': 'text',
              'payload_b64': base64Encode(utf8.encode('hello relay')),
              'created_at_ms': 1700000000000,
            }
          ],
        });
      }
      return _json({'error': 'unexpected'}, status: 500);
    });

    final api = ApiClient(baseUrl: 'http://example.test', httpClient: client);
    final room = await api.room('西京Z区B级103房');

    expect(room.members.first.canPublish, isTrue);
    expect(room.members.last.canPublish, isTrue);
    expect(room.members.first.hasRelayed, isTrue);
    expect(room.members.last.hasRelayed, isFalse);
    expect(room.relays.single.text, 'hello relay');
    expect(room.relays.single.payloadBytes, utf8.encode('hello relay'));
  });

  testWidgets('the connect screen asks for server, token and code', (tester) async {
    final state = AppState();
    await tester.pumpWidget(
      MaterialApp(theme: buildTheme(), home: ConnectScreen(state: state)),
    );

    expect(find.text('qr_race'), findsOneWidget);
    expect(find.text('连接'), findsOneWidget);
    expect(find.byType(TextFormField), findsNWidgets(4));
    expect(find.text('证书指纹（可选）'), findsOneWidget);

    await tester.tap(find.text('连接'));
    await tester.pump();
    expect(find.text('请输入 token'), findsOneWidget);
    expect(find.text('请输入 6 位验证码'), findsOneWidget);
  });
}

http.Response _json(Map<String, Object?> body, {int status = 200}) {
  return http.Response.bytes(
    utf8.encode(jsonEncode(body)),
    status,
    headers: {'content-type': 'application/json; charset=utf-8'},
  );
}
