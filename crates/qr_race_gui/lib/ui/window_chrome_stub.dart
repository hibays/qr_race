import 'package:flutter/widgets.dart';

/// Web has no native window chrome to replace.
bool get isDesktopWindow => false;

Future<void> initWindowChrome() async {}

class WindowChromeShell extends StatelessWidget {
  const WindowChromeShell({super.key, required this.child});

  final Widget child;

  @override
  Widget build(BuildContext context) => child;
}
