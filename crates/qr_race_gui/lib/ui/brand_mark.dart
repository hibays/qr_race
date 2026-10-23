import 'package:flutter/material.dart';

import 'theme.dart';

/// The smallest usable lockup: the relay mark on its own rounded tile.
///
/// The mark asset is tightly cropped, so [size] is the tile, not the image.
class BrandMark extends StatelessWidget {
  const BrandMark({
    super.key,
    this.size = 30,
    this.padding = 4,
    this.radius = 9,
  });

  final double size;
  final double padding;
  final double radius;

  @override
  Widget build(BuildContext context) {
    return Container(
      width: size,
      height: size,
      padding: EdgeInsets.all(padding),
      decoration: BoxDecoration(
        color: const Color(0xFF0B0F0C),
        borderRadius: BorderRadius.circular(radius),
        border: Border.all(color: line, width: 1),
      ),
      child: Image.asset(
        'assets/brand/qr_race-mark-tile.png',
        fit: BoxFit.contain,
        filterQuality: FilterQuality.high,
      ),
    );
  }
}
