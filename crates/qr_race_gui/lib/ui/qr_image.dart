import 'dart:typed_data';

import 'package:flutter/material.dart';

import '../native/qr_renderer.dart';

/// Renders a QR code through the native/wasm rxing encoder and shows the WebP
/// bytes with nearest-neighbour scaling. The module edges stay crisp because
/// the encoder is lossless and the filter is `FilterQuality.none`.
class QrImage extends StatefulWidget {
  const QrImage({super.key, required this.text, required this.size});

  final String text;
  final double size;

  @override
  State<QrImage> createState() => _QrImageState();
}

class _QrImageState extends State<QrImage> {
  late Future<Uint8List> _future;

  @override
  void initState() {
    super.initState();
    _future = renderQrWebp(widget.text);
  }

  @override
  void didUpdateWidget(covariant QrImage oldWidget) {
    super.didUpdateWidget(oldWidget);
    if (oldWidget.text != widget.text) {
      _future = renderQrWebp(widget.text);
    }
  }

  Widget _fallback() => SizedBox(
        width: widget.size,
        height: widget.size,
        child: Center(
          child: Icon(
            Icons.qr_code_2,
            size: widget.size * 0.6,
            color: Colors.black26,
          ),
        ),
      );

  @override
  Widget build(BuildContext context) {
    return FutureBuilder<Uint8List>(
      future: _future,
      builder: (context, snapshot) {
        final data = snapshot.data;
        if (data != null) {
          return Image.memory(
            data,
            width: widget.size,
            height: widget.size,
            fit: BoxFit.contain,
            filterQuality: FilterQuality.none,
            gaplessPlayback: true,
            errorBuilder: (_, _, _) => _fallback(),
          );
        }
        if (snapshot.hasError) return _fallback();
        return SizedBox(
          width: widget.size,
          height: widget.size,
          child: const Center(
            child: SizedBox(
              width: 18,
              height: 18,
              child: CircularProgressIndicator(strokeWidth: 2),
            ),
          ),
        );
      },
    );
  }
}
