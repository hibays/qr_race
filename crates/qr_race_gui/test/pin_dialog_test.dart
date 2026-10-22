import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:qr_race_gui/ui/pin_dialog.dart';
import 'package:qr_race_gui/ui/theme.dart';

Finder _cell(int index) => find.byKey(ValueKey('pin-cell-$index'));

void main() {
  testWidgets(
    'PIN dialog is one sequential field with no selector or confirm',
    (tester) async {
      String? result;
      await tester.pumpWidget(
        MaterialApp(
          theme: buildTheme(),
          home: Builder(
            builder: (context) => Scaffold(
              body: Center(
                child: FilledButton(
                  onPressed: () async {
                    result = await showPinDialog(context, title: '输入 4 位配对码');
                  },
                  child: const Text('打开'),
                ),
              ),
            ),
          ),
        ),
      );

      await tester.tap(find.text('打开'));
      await tester.pump();
      await tester.pump(const Duration(milliseconds: 300));

      expect(_cell(0), findsOneWidget);
      expect(_cell(1), findsOneWidget);
      expect(_cell(2), findsOneWidget);
      expect(_cell(3), findsOneWidget);
      expect(find.byType(TextField), findsOneWidget);
      expect(find.text('确定'), findsNothing);

      // Tapping a later box must not move the input point: the field is
      // sequential, so the first digit still lands in cell 1.
      await tester.tap(_cell(3));
      await tester.pump();
      await tester.enterText(find.byType(TextField), '12a3');
      await tester.pump();

      expect(
        find.descendant(of: _cell(0), matching: find.text('1')),
        findsOneWidget,
      );
      expect(
        find.descendant(of: _cell(1), matching: find.text('2')),
        findsOneWidget,
      );
      expect(
        find.descendant(of: _cell(2), matching: find.text('3')),
        findsOneWidget,
      );
      expect(
        find.descendant(of: _cell(3), matching: find.text('1')),
        findsNothing,
      );
      expect(result, isNull);

      await tester.tap(find.text('取消'));
      await tester.pump();
      await tester.pump(const Duration(milliseconds: 300));
    },
  );

  testWidgets('four digits complete the dialog without a confirm button', (
    tester,
  ) async {
    String? result;
    await tester.pumpWidget(
      MaterialApp(
        theme: buildTheme(),
        home: Builder(
          builder: (context) => Scaffold(
            body: Center(
              child: FilledButton(
                onPressed: () async {
                  result = await showPinDialog(context, title: '输入 4 位配对码');
                },
                child: const Text('打开'),
              ),
            ),
          ),
        ),
      ),
    );

    await tester.tap(find.text('打开'));
    await tester.pump();
    await tester.pump(const Duration(milliseconds: 300));
    await tester.enterText(find.byType(TextField), '1234');
    await tester.pump(const Duration(milliseconds: 220));
    await tester.pumpAndSettle();

    expect(result, '1234');
    expect(find.byType(Dialog), findsNothing);
  });
}
