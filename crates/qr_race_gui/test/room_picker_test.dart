import 'dart:async';

import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:qr_race_gui/rooms/room_picker.dart';
import 'package:qr_race_gui/ui/echo_signal.dart';
import 'package:qr_race_gui/ui/theme.dart';

void main() {
  testWidgets('create button shows the broadcast mark while submitting', (
    tester,
  ) async {
    final pending = Completer<String?>();
    await tester.pumpWidget(
      MaterialApp(
        theme: buildTheme(),
        home: Builder(
          builder: (context) => Scaffold(
            body: Center(
              child: FilledButton(
                onPressed: () => showRoomPicker(
                  context,
                  mode: RoomPickerMode.create,
                  onSubmit: (_) => pending.future,
                ),
                child: const Text('打开'),
              ),
            ),
          ),
        ),
      ),
    );

    await tester.tap(find.text('打开'));
    await tester.pumpAndSettle();
    await tester.tap(find.text('创建并点亮'));
    await tester.pump();

    expect(find.text('正在创建'), findsOneWidget);
    final signal = tester.widget<EchoSignal>(find.byType(EchoSignal));
    expect(signal.mode, EchoSignalMode.broadcasting);

    pending.complete(null);
    await tester.pump(const Duration(milliseconds: 450));
    await tester.pumpAndSettle();
    expect(find.text('创建并点亮'), findsNothing);
  });

  testWidgets('create errors stay on the picker', (tester) async {
    await tester.pumpWidget(
      MaterialApp(
        theme: buildTheme(),
        home: Builder(
          builder: (context) => Scaffold(
            body: Center(
              child: FilledButton(
                onPressed: () => showRoomPicker(
                  context,
                  mode: RoomPickerMode.create,
                  onSubmit: (_) async => '房间创建失败',
                ),
                child: const Text('打开'),
              ),
            ),
          ),
        ),
      ),
    );

    await tester.tap(find.text('打开'));
    await tester.pumpAndSettle();
    await tester.tap(find.text('创建并点亮'));
    await tester.pump(const Duration(milliseconds: 450));
    await tester.pumpAndSettle();

    expect(find.text('房间创建失败'), findsOneWidget);
    expect(find.text('创建并点亮'), findsOneWidget);
  });
}
