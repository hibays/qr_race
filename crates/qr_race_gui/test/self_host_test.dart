import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:qr_race_gui/selfhost/self_host_launcher.dart';
import 'package:qr_race_gui/state/app_state.dart';
import 'package:qr_race_gui/ui/self_host_screen.dart';
import 'package:qr_race_gui/ui/theme.dart';

class _FakeLauncher implements ServerLauncher {
  bool stopped = false;

  @override
  Future<SelfHostInfo> start({int port = 8443}) async => SelfHostInfo(
        url: 'https://192.168.1.9:$port',
        fingerprint: List.filled(64, 'a').join(),
        dataDirectory: r'C:\tmp\selfhost',
        hostToken: 'host-token-0123456789',
        hostSecret: 'JBSWY3DPEHPK3PXPJBSWY3DPEHPK3PXP',
        memberToken: 'member-token-0123456789',
        memberSecret: 'GEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQ',
      );

  @override
  Future<void> stop() async => stopped = true;
}

void main() {
  test('TOTP matches the RFC 6238 vector at 59 seconds', () {
    expect(
      totpCode(
        'GEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQ',
        at: DateTime.fromMillisecondsSinceEpoch(59000),
      ),
      '287082',
    );
  });

  testWidgets('self-host starts a server and shows connection material', (tester) async {
    tester.view.physicalSize = const Size(1100, 1600);
    tester.view.devicePixelRatio = 1.0;
    addTearDown(tester.view.reset);

    final launcher = _FakeLauncher();
    await tester.pumpWidget(
      MaterialApp(
        theme: buildTheme(),
        home: SelfHostScreen(state: AppState(), launcher: launcher),
      ),
    );

    await tester.tap(find.text('启动本机服务器'));
    await tester.pumpAndSettle();

    expect(find.text('服务器已启动'), findsOneWidget);
    expect(find.textContaining('192.168.1.9:8443'), findsOneWidget);
    expect(find.text('房主账号'), findsOneWidget);
    expect(find.text('成员账号'), findsOneWidget);
    expect(find.text('用此账号连接'), findsOneWidget);

    await tester.tap(find.text('停止服务器'));
    await tester.pumpAndSettle();
    expect(launcher.stopped, isTrue);
    expect(find.text('服务器已启动'), findsNothing);
  });
}
