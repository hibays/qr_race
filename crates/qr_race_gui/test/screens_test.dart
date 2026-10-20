import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:qr_race_gui/api/api_client.dart';
import 'package:qr_race_gui/state/app_state.dart';
import 'package:qr_race_gui/ui/lobby_screen.dart';
import 'package:qr_race_gui/ui/qr_image.dart';
import 'package:qr_race_gui/ui/room_screen.dart';
import 'package:qr_race_gui/ui/theme.dart';

void main() {
  Future<void> pumpAt(
    WidgetTester tester,
    Size size,
    Widget child,
  ) async {
    tester.view.physicalSize = size;
    tester.view.devicePixelRatio = 1;
    addTearDown(tester.view.resetPhysicalSize);
    addTearDown(tester.view.resetDevicePixelRatio);
    await tester.pumpWidget(MaterialApp(theme: buildTheme(), home: child));
    await tester.pumpAndSettle();
  }

  testWidgets('room screen shows relays and members on desktop', (tester) async {
    final state = AppState.demo();
    await pumpAt(
      tester,
      const Size(1280, 800),
      RoomScreen(state: state, address: '东京A区1B级103房'),
    );

    expect(find.text('东京A区1B级103房'), findsOneWidget);
    expect(find.byType(QrImage), findsNWidgets(2));
    expect(find.text('mika'), findsOneWidget);
    expect(find.text('房间成员'), findsOneWidget);
    expect(tester.takeException(), isNull);
  });

  testWidgets('the room exposes no Python/module UI', (tester) async {
    final state = AppState.demo();
    await pumpAt(
      tester,
      const Size(1280, 800),
      RoomScreen(state: state, address: '东京A区1B级103房'),
    );

    expect(find.text('未配置模组'), findsNothing);
    expect(find.text('配置'), findsNothing);
    expect(find.byIcon(Icons.extension_outlined), findsNothing);
    expect(find.text('房间模组'), findsNothing);
    expect(tester.takeException(), isNull);
  });

  testWidgets('room screen shows relays and a member rail on a phone', (tester) async {
    final state = AppState.demo();
    await pumpAt(
      tester,
      const Size(390, 844),
      RoomScreen(state: state, address: '东京A区1B级103房'),
    );

    expect(find.byType(QrImage), findsNWidgets(2));
    expect(find.text('mika'), findsOneWidget);
    // The wide panel heading is absent; the bottom rail replaces it.
    expect(find.text('房间成员'), findsNothing);
    expect(tester.takeException(), isNull);
  });

  testWidgets('lobby lists lit rooms and opens one', (tester) async {
    final state = AppState.demo();
    await pumpAt(tester, const Size(1280, 800), LobbyScreen(state: state));

    expect(find.text('大厅'), findsOneWidget);
    expect(find.text('东京A区1B级103房'), findsOneWidget);
    expect(find.textContaining('房主 host-a'), findsOneWidget);
    expect(
      find.byTooltip('上传模组'),
      findsNothing,
      reason: 'only admins see the module upload action',
    );

    await tester.tap(find.text('东京A区1B级103房'));
    await tester.pumpAndSettle();

    expect(find.byType(RoomScreen), findsOneWidget);
    expect(find.byType(QrImage), findsNWidgets(2));
    expect(tester.takeException(), isNull);
  });

  testWidgets('the host can promote and demote from the member panel', (tester) async {
    final state = AppState.demo();
    await pumpAt(
      tester,
      const Size(1280, 800),
      RoomScreen(state: state, address: '东京A区1B级103房'),
    );

    // The host's own tile has no menu; the other three do.
    final menus = find.byTooltip('管理成员');
    expect(menus, findsNWidgets(3));

    await tester.tap(menus.at(0));
    await tester.pumpAndSettle();
    expect(find.text('取消副房主'), findsOneWidget);
    expect(tester.takeException(), isNull);
  });

  testWidgets('a dark room offers the host a light action', (tester) async {
    final state = AppState.demo();
    state.room = const RoomView(
      label: '西京Z区B级103房',
      lit: false,
      host: 'host-a',
      members: [RoomMember(user: 'host-a', role: 'host', lastRelay: null)],
      relays: [],
    );
    await pumpAt(
      tester,
      const Size(1280, 800),
      RoomScreen(state: state, address: '西京Z区B级103房'),
    );

    expect(find.textContaining('房间尚未点亮'), findsOneWidget);
    expect(find.text('点亮'), findsOneWidget);
    expect(tester.takeException(), isNull);
  });

  testWidgets('creating a room explains offline demo mode', (tester) async {
    final state = AppState.demo();
    await pumpAt(tester, const Size(1280, 800), LobbyScreen(state: state));

    await tester.tap(find.byIcon(Icons.add));
    await tester.pumpAndSettle();
    expect(find.text('创建房间'), findsOneWidget);

    await tester.enterText(find.byType(TextField), '西京Z区B级103房');
    await tester.tap(find.text('创建并点亮'));
    await tester.pumpAndSettle();

    expect(find.text('离线演示模式不能建房'), findsOneWidget);
  });
}

