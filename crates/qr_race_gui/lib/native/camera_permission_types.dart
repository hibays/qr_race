/// Camera permission contract shared by the scan page.
enum CameraPermissionStatus { granted, denied, permanentlyDenied }

abstract class CameraPermission {
  Future<CameraPermissionStatus> request();

  Future<void> openSettings();
}

class GrantedCameraPermission implements CameraPermission {
  const GrantedCameraPermission();

  @override
  Future<CameraPermissionStatus> request() async =>
      CameraPermissionStatus.granted;

  @override
  Future<void> openSettings() async {}
}
