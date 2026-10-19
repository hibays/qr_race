import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:qr_race_gui/ui/module_upload_dialog.dart';
import 'package:qr_race_gui/ui/theme.dart';

void main() {
  testWidgets('the upload dialog validates and passes a draft', (tester) async {
    tester.view.physicalSize = const Size(900, 1400);
    tester.view.devicePixelRatio = 1.0;
    addTearDown(tester.view.reset);

    ModuleDraft? captured;
    await tester.pumpWidget(
      MaterialApp(
        theme: buildTheme(),
        home: Scaffold(
          body: Center(
            child: ModuleUploadDialog(
              onSubmit: (draft) async => captured = draft,
            ),
          ),
        ),
      ),
    );
    await tester.pumpAndSettle();

    await tester.tap(find.text('上传'));
    await tester.pump();
    expect(find.text('请输入模组 id'), findsOneWidget);
    expect(find.text('请输入模组源码'), findsOneWidget);

    final fields = find.byType(TextFormField);
    await tester.enterText(fields.at(0), 'overlay');
    await tester.enterText(fields.at(3), 'def on_frame(payload):\n    return payload');
    await tester.tap(find.text('上传'));
    await tester.pumpAndSettle();

    expect(captured, isNotNull);
    expect(captured!.id, 'overlay');
    expect(captured!.version, '1.0.0');
    expect(captured!.entry, 'main.py');
    expect(captured!.hooks, ['frame']);
    expect(captured!.capabilities, ['ui']);
    expect(captured!.source, contains('on_frame'));
  });
}
