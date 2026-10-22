import 'dart:math' as math;

import 'package:flutter/material.dart';

import 'theme.dart';

enum EchoSignalMode { idle, loading, broadcasting }

/// The 05 "Echo Signal" mark, drawn from the same 512 grid as the SVG.
class EchoSignal extends StatefulWidget {
  const EchoSignal({
    super.key,
    this.size = 48,
    this.mode = EchoSignalMode.idle,
    this.color = ink,
    this.accent = const Color(0xFF3DDC84),
  });

  final double size;
  final EchoSignalMode mode;
  final Color color;
  final Color accent;

  @override
  State<EchoSignal> createState() => _EchoSignalState();
}

class _EchoSignalState extends State<EchoSignal>
    with SingleTickerProviderStateMixin {
  late final AnimationController _controller = AnimationController(
    vsync: this,
    duration: const Duration(milliseconds: 1200),
  );

  @override
  void initState() {
    super.initState();
    _sync();
  }

  @override
  void didUpdateWidget(covariant EchoSignal oldWidget) {
    super.didUpdateWidget(oldWidget);
    if (oldWidget.mode != widget.mode) _sync();
  }

  void _sync() {
    if (widget.mode == EchoSignalMode.idle) {
      _controller.stop();
      _controller.value = 1;
    } else {
      _controller.duration = widget.mode == EchoSignalMode.broadcasting
          ? const Duration(milliseconds: 760)
          : const Duration(milliseconds: 1200);
      _controller.repeat();
    }
  }

  @override
  void dispose() {
    _controller.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    return SizedBox.square(
      dimension: widget.size,
      child: AnimatedBuilder(
        animation: _controller,
        builder: (context, _) => CustomPaint(
          painter: _EchoPainter(
            value: _controller.value,
            mode: widget.mode,
            color: widget.color,
            accent: widget.accent,
          ),
        ),
      ),
    );
  }
}

class _EchoPainter extends CustomPainter {
  const _EchoPainter({
    required this.value,
    required this.mode,
    required this.color,
    required this.accent,
  });

  final double value;
  final EchoSignalMode mode;
  final Color color;
  final Color accent;

  @override
  void paint(Canvas canvas, Size size) {
    final s = size.width / 512;
    final square = Paint()
      ..style = PaintingStyle.stroke
      ..strokeWidth = 24 * s
      ..strokeCap = StrokeCap.round
      ..color = color;
    final inner = Paint()..color = color;
    canvas.drawRRect(
      RRect.fromRectAndRadius(
        Rect.fromLTWH(128 * s, 272 * s, 136 * s, 136 * s),
        Radius.circular(12 * s),
      ),
      square,
    );
    canvas.drawRect(
      Rect.fromLTWH(168 * s, 312 * s, 56 * s, 56 * s),
      inner,
    );

    final center = Offset(264 * s, 272 * s);
    final arcPaint = Paint()
      ..style = PaintingStyle.stroke
      ..strokeWidth = 20 * s
      ..strokeCap = StrokeCap.round
      ..color = color;
    const radii = [64.0, 112.0, 160.0];

    if (mode == EchoSignalMode.broadcasting) {
      for (var i = 0; i < radii.length; i++) {
        final phase = ((value - i * 0.16) / 0.56).clamp(0.0, 1.0);
        if (phase <= 0) continue;
        final eased = Curves.easeOutCubic.transform(phase);
        arcPaint.color = color.withValues(alpha: 0.32 + 0.68 * eased);
        canvas.drawArc(
          Rect.fromCircle(center: center, radius: radii[i] * s),
          -math.pi / 2,
          math.pi / 2 * eased,
          false,
          arcPaint,
        );
      }
    } else if (mode == EchoSignalMode.loading) {
      for (var i = 0; i < radii.length; i++) {
        final phase = (value + i * 0.18) % 1.0;
        final pulse = 0.30 + 0.70 * (0.5 + 0.5 * math.sin(phase * math.pi * 2));
        arcPaint.color = color.withValues(alpha: pulse);
        canvas.drawArc(
          Rect.fromCircle(center: center, radius: radii[i] * s),
          -math.pi / 2,
          math.pi / 2,
          false,
          arcPaint,
        );
      }
    } else {
      for (final radius in radii) {
        canvas.drawArc(
          Rect.fromCircle(center: center, radius: radius * s),
          -math.pi / 2,
          math.pi / 2,
          false,
          arcPaint,
        );
      }
    }

    final dotPulse = mode == EchoSignalMode.idle
        ? 1.0
        : mode == EchoSignalMode.broadcasting
            ? 0.65 + 0.55 * math.sin(value * math.pi * 2).abs()
            : 0.80 + 0.20 * math.sin(value * math.pi * 2).abs();
    canvas.drawCircle(
      Offset(424 * s, 272 * s),
      17 * s * dotPulse,
      Paint()..color = accent,
    );
  }

  @override
  bool shouldRepaint(covariant _EchoPainter old) =>
      old.value != value ||
      old.mode != mode ||
      old.color != color ||
      old.accent != accent;
}

