import 'dart:io';

import 'package:flutter/services.dart';

const _channel = MethodChannel('app.qrrace.qr_race_gui/links');

/// Opens `url` in the platform's default handler. Android shows the system app
/// chooser for http(s), so WeChat/WeCom/browser can all appear. Returns false
/// when no activity can handle the scheme.
Future<bool> openExternal(String url) async {
  final trimmed = url.trim();
  if (trimmed.isEmpty) return false;
  if (Platform.isAndroid) {
    try {
      return await _channel.invokeMethod<bool>(
            'openExternal',
            {'url': trimmed},
          ) ??
          false;
    } catch (_) {
      return false;
    }
  }
  try {
    if (Platform.isWindows) {
      await Process.run('rundll32', ['url.dll,FileProtocolHandler', trimmed]);
    } else if (Platform.isMacOS) {
      await Process.run('open', [trimmed]);
    } else if (Platform.isLinux) {
      await Process.run('xdg-open', [trimmed]);
    } else {
      return false;
    }
    return true;
  } catch (_) {
    return false;
  }
}
