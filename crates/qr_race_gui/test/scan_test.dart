import 'dart:convert';
import 'dart:typed_data';

import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:http/http.dart' as http;
import 'package:http/testing.dart';
import 'package:qr_race_gui/api/api_client.dart';
import 'package:qr_race_gui/native/camera_permission.dart';
import 'package:qr_race_gui/native/scanner_backend.dart';
import 'package:qr_race_gui/state/app_state.dart';
import 'package:qr_race_gui/state/scan_controller.dart';
import 'package:qr_race_gui/ui/scan_screen.dart';
import 'package:qr_race_gui/ui/theme.dart';

ScanHit _hit(String text) => ScanHit(
  text: text,
  raw: Uint8List.fromList(utf8.encode(text)),
  atMs: 1700000000000,
);

http.Response _json(Object body, {int status = 200}) => http.Response.bytes(
  utf8.encode(jsonEncode(body)),
  status,
  headers: {'content-type': 'application/json; charset=utf-8'},
);

void main() {
  test('the controller polls the backend and stops on dispose', () async {
    final backend = FakeScannerBackend(
      snapshots: [
        fakeSnapshot(hits: [_hit('hello relay')]),
      ],
    );
    final controller = ScanController(
      backend: backend,
      interval: const Duration(milliseconds: 5),
    );
    controller.start();
    await Future<void>.delayed(const Duration(milliseconds: 40));

    expect(controller.snapshot?.hits.single.text, 'hello relay');
    expect(controller.fatalError, isNull);

    controller.dispose();
    expect(backend.disposed, isTrue);
  });

  test('stale hit history is not republished as a heartbeat', () async {
    final calls = <http.Request>[];
    final controller = ScanController(
      backend: FakeScannerBackend(
        snapshots: [
          fakeSnapshot(hits: [_hit('cam1:old')], latestHits: const []),
        ],
      ),
      interval: const Duration(milliseconds: 5),
    );
    controller.enableAutoPublish(
      ApiClient(
        baseUrl: 'http://example.test',
        httpClient: MockClient((request) async {
          calls.add(request);
          return _json({});
        }),
      ),
      '东京A区1B级103房',
    );
    controller.start();
    await Future<void>.delayed(const Duration(milliseconds: 60));

    expect(calls, isEmpty);
    controller.dispose();
  });

  testWidgets('the scan screen auto-publishes prefixed hits', (tester) async {
    final hit = _hit('cam1:https://example.test/scan');
    final calls = <http.Request>[];
    final state = AppState.demo();
    state.api = ApiClient(
      baseUrl: 'http://example.test',
      httpClient: MockClient((request) async {
        calls.add(request);
        return _json({
          'id': 'r-9',
          'publisher': 'host-a',
          'kind': 'url',
          'payload_b64': base64Encode(hit.raw),
          'created_at_ms': 1700000000000,
          'hash': '0123456789abcdef01234567',
        });
      }),
    );

    await tester.pumpWidget(
      MaterialApp(
        theme: buildTheme(),
        home: ScanScreen(
          state: state,
          address: '东京A区1B级103房',
          backend: FakeScannerBackend(
            snapshots: [
              fakeSnapshot(
                hits: [
                  hit,
                  ScanHit(
                    text: hit.text,
                    raw: Uint8List.fromList([1, 2, 3]),
                    atMs: hit.atMs,
                  ),
                ],
              ),
            ],
          ),
          permission: const GrantedCameraPermission(),
        ),
      ),
    );

    await tester.pump(const Duration(milliseconds: 200));
    await tester.pump(const Duration(milliseconds: 100));

    expect(find.text('识别结果 · 自动发布'), findsOneWidget);
    expect(find.textContaining('cam1'), findsWidgets);
    expect(find.textContaining('已自动发布'), findsWidgets);
    expect(find.text('发布'), findsNothing);
    expect(calls, hasLength(1));
    expect(calls.single.url.path, '/v1/room/relay');
    expect(calls.single.url.queryParameters['address'], '东京A区1B级103房');
    final body = jsonDecode(calls.single.body) as Map<String, dynamic>;
    expect(body['payload_b64'], base64Encode(hit.raw));

    // Dispose the screen so its periodic poll timer is cancelled.
    await tester.pumpWidget(const SizedBox());
  });

  testWidgets('the scan screen requests 4:3 2K and can switch backends', (
    tester,
  ) async {
    final backend = RecordingBackend();
    await tester.pumpWidget(
      MaterialApp(
        theme: buildTheme(),
        home: ScanScreen(
          state: AppState.demo(),
          address: '东京A区1B级103房',
          backend: backend,
          permission: const GrantedCameraPermission(),
        ),
      ),
    );
    await tester.pump();
    await tester.pump(const Duration(milliseconds: 20));

    expect(backend.width, 1920);
    expect(backend.height, 1440);
    expect(backend.backend, 'hybrid');

    await tester.tap(find.byTooltip('扫码后端'));
    await tester.pumpAndSettle();
    await tester.tap(find.text('仅 zxing-cpp'));
    await tester.pumpAndSettle();

    expect(backend.backend, 'zxing');
    await tester.pumpWidget(const SizedBox());
  });
  testWidgets('a missing native scanner is explained, not crashed', (
    tester,
  ) async {
    final state = AppState.demo();
    await tester.pumpWidget(
      MaterialApp(
        theme: buildTheme(),
        home: ScanScreen(
          state: state,
          address: '东京A区1B级103房',
          backend: FailingBackend(),
          permission: const GrantedCameraPermission(),
        ),
      ),
    );
    await tester.pump();
    await tester.pump();
    expect(find.text('原生扫描器没有加载'), findsOneWidget);
    expect(find.textContaining('boom'), findsOneWidget);
  });

  testWidgets('a denied camera permission offers Settings and retry', (
    tester,
  ) async {
    final permission = _DeniedPermission();
    await tester.pumpWidget(
      MaterialApp(
        theme: buildTheme(),
        home: ScanScreen(
          state: AppState.demo(),
          address: '东京A区1B级103房',
          backend: FailingBackend(),
          permission: permission,
        ),
      ),
    );
    await tester.pump();
    await tester.pump(const Duration(milliseconds: 50));

    expect(find.textContaining('永久拒绝'), findsWidgets);

    await tester.tap(find.text('去设置'));
    await tester.pumpAndSettle();
    expect(permission.settingsOpened, isTrue);
    expect(find.text('打开系统设置'), findsOneWidget);

    await tester.tap(find.text('重新申请'));
    await tester.pump();
    expect(permission.requests, 2);
  });
}

class FailingBackend implements ScannerBackend {
  @override
  void start({
    required int camera,
    required int width,
    required int height,
    required int fps,
    String backend = 'hybrid',
  }) {
    throw StateError('boom');
  }

  @override
  int? get previewTextureId => null;

  @override
  void stop() {}

  @override
  ScannerSnapshot poll() => throw StateError('never polls');

  @override
  void dispose() {}
}

class _DeniedPermission implements CameraPermission {
  int requests = 0;
  bool settingsOpened = false;

  @override
  Future<CameraPermissionStatus> request() async {
    requests++;
    return CameraPermissionStatus.permanentlyDenied;
  }

  @override
  Future<void> openSettings() async {
    settingsOpened = true;
  }
}

class RecordingBackend implements ScannerBackend {
  int width = 0;
  int height = 0;
  String backend = '';

  @override
  void start({
    required int camera,
    required int width,
    required int height,
    required int fps,
    String backend = 'hybrid',
  }) {
    this.width = width;
    this.height = height;
    this.backend = backend;
  }

  @override
  int? get previewTextureId => null;

  @override
  void stop() {}

  @override
  ScannerSnapshot poll() => fakeSnapshot();

  @override
  void dispose() {}
}
