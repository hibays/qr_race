import 'dart:ui';

import 'package:flutter_test/flutter_test.dart';
import 'package:qr_race_gui/native/preview_projection.dart';

void main() {
  test('surface texture matrix maps QR points into the displayed frame', () {
    const matrix = [
      0.0, -1.0, 0.0, 0.0,
      -1.0, 0.0, 0.0, 0.0,
      0.0, 0.0, 1.0, 0.0,
      1.0, 1.0, 0.0, 1.0,
    ];
    const view = Size(1540, 1090);
    final point = projectPreviewPoint(matrix, 672 / 1920, 162 / 1440, view);
    // 1 - v for x, u for y after the bottom-left flip.
    expect(point.dx, closeTo((1 - 162 / 1440) * 1540, 1));
    expect(point.dy, closeTo((672 / 1920) * 1090, 1));
  });
}
