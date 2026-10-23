import 'dart:async';
import 'dart:io';
import 'dart:math' as math;

import 'package:flutter/material.dart';
import 'package:screen_retriever/screen_retriever.dart';
import 'package:window_manager/window_manager.dart';

import 'brand_mark.dart';
import 'theme.dart';

/// True for the desktop targets where the native caption is replaced by the
/// app's own chrome. Android/iOS keep their platform window.
bool get isDesktopWindow =>
    Platform.isWindows || Platform.isLinux || Platform.isMacOS;

/// Removes the native caption before the first frame. The shell below draws the
/// replacement; window_manager still owns move/resize/minimize/maximize/close.
Future<void> initWindowChrome() async {
  if (!isDesktopWindow) return;
  await windowManager.ensureInitialized();
  await windowManager.setTitleBarStyle(
    TitleBarStyle.hidden,
    windowButtonVisibility: false,
  );
  final display = await screenRetriever.getPrimaryDisplay();
  final available = display.visibleSize ?? display.size;
  final width = math.min(1120.0, math.max(680.0, available.width * 0.86));
  final height = math.min(760.0, math.max(440.0, available.height * 0.86));
  await windowManager.setSize(Size(width, height));
  await windowManager.center();
}

/// Desktop-only custom app bar. The 38px strip carries the brand tile and the
/// window buttons; the routed app sits underneath it unchanged.
class WindowChromeShell extends StatefulWidget {
  const WindowChromeShell({super.key, required this.child});

  final Widget child;

  @override
  State<WindowChromeShell> createState() => _WindowChromeShellState();
}

class _WindowChromeShellState extends State<WindowChromeShell>
    with WindowListener {
  bool _maximized = false;

  @override
  void initState() {
    super.initState();
    if (!isDesktopWindow) return;
    windowManager.addListener(this);
    unawaited(_readMaximized());
  }

  @override
  void dispose() {
    if (isDesktopWindow) windowManager.removeListener(this);
    super.dispose();
  }

  Future<void> _readMaximized() async {
    final maximized = await windowManager.isMaximized();
    if (mounted) setState(() => _maximized = maximized);
  }

  @override
  void onWindowMaximize() {
    if (mounted) setState(() => _maximized = true);
  }

  @override
  void onWindowUnmaximize() {
    if (mounted) setState(() => _maximized = false);
  }

  @override
  void onWindowRestore() {
    unawaited(_readMaximized());
  }

  @override
  Widget build(BuildContext context) {
    if (!isDesktopWindow) return widget.child;
    return Column(
      children: [
        _ChromeBar(maximized: _maximized),
        Expanded(child: ClipRect(child: widget.child)),
      ],
    );
  }
}

class _ChromeBar extends StatelessWidget {
  const _ChromeBar({required this.maximized});

  final bool maximized;

  Future<void> _toggleMaximize() async {
    if (await windowManager.isFullScreen()) return;
    if (await windowManager.isMaximized()) {
      await windowManager.unmaximize();
    } else {
      await windowManager.maximize();
    }
  }

  @override
  Widget build(BuildContext context) {
    return Material(
      color: surface,
      child: Container(
        height: 38,
        decoration: const BoxDecoration(
          border: Border(bottom: BorderSide(color: line)),
        ),
        child: Row(
          children: [
            Expanded(
              child: GestureDetector(
                behavior: HitTestBehavior.translucent,
                onPanStart: (_) => unawaited(windowManager.startDragging()),
                onDoubleTap: () => unawaited(_toggleMaximize()),
                child: Padding(
                  padding: const EdgeInsets.only(left: 10),
                  child: Row(
                    children: [
                      const BrandMark(size: 26, padding: 3.5, radius: 7.5),
                      const SizedBox(width: 8),
                      const Text(
                        'qr_race',
                        style: TextStyle(
                          fontSize: 11,
                          fontWeight: FontWeight.w600,
                          color: muted,
                          letterSpacing: 0.3,
                        ),
                      ),
                      const Spacer(),
                    ],
                  ),
                ),
              ),
            ),
            _ChromeButton(
              icon: Icons.remove,
              onTap: () => unawaited(windowManager.minimize()),
            ),
            _ChromeButton(
              icon: maximized ? Icons.filter_none : Icons.crop_square,
              onTap: () => unawaited(_toggleMaximize()),
            ),
            _ChromeButton(
              icon: Icons.close,
              onTap: () => unawaited(windowManager.close()),
              hoverBackground: const Color(0xD9FF0000),
            ),
          ],
        ),
      ),
    );
  }
}

class _ChromeButton extends StatefulWidget {
  const _ChromeButton({
    required this.icon,
    required this.onTap,
    this.hoverBackground,
  });

  final IconData icon;
  final VoidCallback onTap;
  final Color? hoverBackground;

  @override
  State<_ChromeButton> createState() => _ChromeButtonState();
}

class _ChromeButtonState extends State<_ChromeButton> {
  bool _hovered = false;
  bool _pressed = false;

  @override
  Widget build(BuildContext context) {
    final background = _pressed
        ? Colors.white.withValues(alpha: 0.15)
        : _hovered
        ? (widget.hoverBackground ?? Colors.white.withValues(alpha: 0.10))
        : Colors.transparent;

    return MouseRegion(
      onEnter: (_) => setState(() => _hovered = true),
      onExit: (_) => setState(() => _hovered = false),
      child: GestureDetector(
        onTapDown: (_) => setState(() => _pressed = true),
        onTapUp: (_) => setState(() => _pressed = false),
        onTapCancel: () => setState(() => _pressed = false),
        onTap: widget.onTap,
        child: Container(
          width: 46,
          height: 38,
          color: background,
          alignment: Alignment.center,
          child: Icon(widget.icon, color: Colors.white70, size: 16),
        ),
      ),
    );
  }
}
