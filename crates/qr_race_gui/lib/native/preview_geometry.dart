import 'dart:ui';

/// Pure projection between a camera frame (sensor coordinates) and the widget
/// that paints its preview. One implementation is shared by the texture layout
/// and the hit/detection overlay, so a preview/overlay mismatch is a test
/// failure rather than a visual guess.
class PreviewGeometry {
  PreviewGeometry({
    required int frameWidth,
    required int frameHeight,
    required int rotation,
    required this.view,
    required this.contain,
  }) : frameWidth = frameWidth.toDouble(),
       frameHeight = frameHeight.toDouble(),
       rotation = ((rotation % 360) + 360) % 360;

  final double frameWidth;
  final double frameHeight;
  final int rotation;
  final Size view;
  final bool contain;

  Size get sourceSize =>
      rotation == 90 || rotation == 270
          ? Size(frameHeight, frameWidth)
          : Size(frameWidth, frameHeight);

  double get scale {
    final sx = view.width / sourceSize.width;
    final sy = view.height / sourceSize.height;
    return contain ? (sx < sy ? sx : sy) : (sx > sy ? sx : sy);
  }

  Size get drawnSize => Size(sourceSize.width * scale, sourceSize.height * scale);

  Offset get offset => Offset(
    (view.width - drawnSize.width) / 2,
    (view.height - drawnSize.height) / 2,
  );

  Offset mapPoint((double, double) raw) {
    final rotated = switch (rotation) {
      90 => Offset(frameHeight - 1 - raw.$2, raw.$1),
      180 => Offset(frameWidth - 1 - raw.$1, frameHeight - 1 - raw.$2),
      270 => Offset(raw.$2, frameWidth - 1 - raw.$1),
      _ => Offset(raw.$1, raw.$2),
    };
    return offset + Offset(rotated.dx * scale, rotated.dy * scale);
  }

  List<Offset> mapPoints(List<(double, double)> points) =>
      [for (final point in points) mapPoint(point)];

  /// The rectangle the preview occupies inside [view]. Bars are outside it.
  Rect get drawnRect => offset & drawnSize;
}

/// Quarter turns to apply to the raw camera buffer. Sensor orientation 90
/// means the buffer is one quarter turn away from upright, so the preview gets
/// the inverse (three turns).
int quarterTurnsForRotation(int rotation) {
  final normalized = ((rotation % 360) + 360) % 360;
  return ((360 - normalized) % 360) ~/ 90;
}
