import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:qr_race_gui/ui/echo_signal.dart';

void main() {
  testWidgets('echo signal animates while loading and stops when idle', (
    tester,
  ) async {
    await tester.pumpWidget(
      const MaterialApp(
        home: Center(
          child: EchoSignal(size: 48, mode: EchoSignalMode.loading),
        ),
      ),
    );
    await tester.pump(const Duration(milliseconds: 200));
    expect(find.byType(EchoSignal), findsOneWidget);
    expect(tester.takeException(), isNull);

    await tester.pumpWidget(
      const MaterialApp(
        home: Center(
          child: EchoSignal(size: 48, mode: EchoSignalMode.idle),
        ),
      ),
    );
    await tester.pump(const Duration(milliseconds: 200));
    expect(tester.takeException(), isNull);
  });
}
