import 'package:flutter/services.dart';

/// Receives `qrrace://connect?...` intents from Android and exposes the launch
/// link to the connect screen.
class AppLinks {
  static const _channel = MethodChannel('app.qrrace.qr_race_gui/links');

  static Future<String?> initial() async {
    try {
      return await _channel.invokeMethod<String>('getInitialLink');
    } catch (_) {
      return null;
    }
  }

  static void listen(void Function(String link) onLink) {
    _channel.setMethodCallHandler((call) async {
      if (call.method == 'link' && call.arguments is String) {
        onLink(call.arguments as String);
      }
    });
  }

  static void stop() {
    _channel.setMethodCallHandler(null);
  }
}
