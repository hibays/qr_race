import 'package:flutter/services.dart';

/// Android "All files access" and document picker bridge. On desktop/web the
/// channel is absent and the methods return safe defaults.
class StorageAccess {
  static const _channel = MethodChannel('app.qrrace.qr_race_gui/files');

  static Future<bool> hasAllFilesAccess() async {
    try {
      return await _channel.invokeMethod<bool>('hasAllFilesAccess') ?? false;
    } catch (_) {
      return false;
    }
  }

  static Future<bool> requestAllFilesAccess() async {
    try {
      return await _channel.invokeMethod<bool>('requestAllFilesAccess') ?? false;
    } catch (_) {
      return false;
    }
  }

  /// Opens the Android document picker and returns a cached absolute path.
  static Future<String?> pickFile() async {
    try {
      return await _channel.invokeMethod<String>('pickFile');
    } catch (_) {
      return null;
    }
  }
}
