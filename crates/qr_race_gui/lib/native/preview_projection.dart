import 'dart:ui';

/// Projects a camera point through the same SurfaceTexture matrix that Flutter
/// uses to display the preview. Matrix is column-major (Android layout).
Offset projectPreviewPoint(
  List<double> matrix,
  double u,
  double v,
  Size view,
) {
  final x = matrix[0] * u + matrix[4] * v + matrix[12];
  final y = matrix[1] * u + matrix[5] * v + matrix[13];
  // SurfaceTexture coordinates are bottom-left; Flutter canvas is top-left.
  return Offset(x * view.width, (1.0 - y) * view.height);
}
