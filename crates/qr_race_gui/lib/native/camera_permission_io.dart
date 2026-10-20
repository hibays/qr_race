import 'dart:io';

import 'package:flutter/services.dart';

import 'camera_permission_types.dart';

CameraPermission createCameraPermission() {
  if (Platform.isAndroid) return AndroidCameraPermission();
  return const GrantedCameraPermission();
}

/// Bridges to MainActivity's CAMERA runtime permission handling.
class AndroidCameraPermission implements CameraPermission {
  static const _channel = MethodChannel('app.qrrace.qr_race_gui/camera');

  @override
  Future<CameraPermissionStatus> request() async {
    final value = await _channel.invokeMethod<String>('request');
    return _parse(value);
  }

  @override
  Future<void> openSettings() async {
    await _channel.invokeMethod<void>('openSettings');
  }

  CameraPermissionStatus _parse(String? value) => switch (value) {
        'granted' => CameraPermissionStatus.granted,
        'permanentlyDenied' => CameraPermissionStatus.permanentlyDenied,
        _ => CameraPermissionStatus.denied,
      };
}
