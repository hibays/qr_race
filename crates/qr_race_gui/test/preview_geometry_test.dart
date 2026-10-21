import 'dart:ui';

import 'package:flutter_test/flutter_test.dart';
import 'package:qr_race_gui/native/preview_geometry.dart';

void main() {
  test('sensor rotation 90 needs three quarter turns', () {
    expect(quarterTurnsForRotation(0), 0);
    expect(quarterTurnsForRotation(90), 3);
    expect(quarterTurnsForRotation(180), 2);
    expect(quarterTurnsForRotation(270), 1);
    expect(quarterTurnsForRotation(-90), 1);
  });

  test('contain maps the frame centre to the view centre', () {
    for (final rotation in [0, 90, 180, 270]) {
      final geometry = PreviewGeometry(
        frameWidth: 1920,
        frameHeight: 1440,
        rotation: rotation,
        view: const Size(800, 600),
        contain: true,
      );
      final centre = geometry.mapPoint((960, 720));
      expect(centre.dx, closeTo(400, 1), reason: 'rotation $rotation');
      expect(centre.dy, closeTo(300, 1), reason: 'rotation $rotation');
      expect(geometry.drawnRect.left, greaterThanOrEqualTo(0));
      expect(geometry.drawnRect.right, lessThanOrEqualTo(800));
    }
  });

  test('cover fills the view and keeps the centre centred', () {
    final geometry = PreviewGeometry(
      frameWidth: 1920,
      frameHeight: 1440,
      rotation: 90,
      view: const Size(800, 600),
      contain: false,
    );
    final centre = geometry.mapPoint((960, 720));
    expect(centre.dx, closeTo(400, 1));
    expect(centre.dy, closeTo(300, 1));
    expect(geometry.drawnRect.width, greaterThanOrEqualTo(800));
    expect(geometry.drawnRect.height, greaterThanOrEqualTo(600));
  });

  test('rotating the frame preserves its aspect ratio', () {
    final upright = PreviewGeometry(
      frameWidth: 1920,
      frameHeight: 1440,
      rotation: 0,
      view: const Size(800, 600),
      contain: true,
    );
    final turned = PreviewGeometry(
      frameWidth: 1920,
      frameHeight: 1440,
      rotation: 90,
      view: const Size(800, 600),
      contain: true,
    );
    expect(upright.sourceSize.aspectRatio, closeTo(4 / 3, 1e-9));
    expect(turned.sourceSize.aspectRatio, closeTo(3 / 4, 1e-9));
    expect(turned.drawnRect.width, closeTo(450, 0.01));
    expect(turned.drawnRect.height, closeTo(600, 0.01));
  });
}

