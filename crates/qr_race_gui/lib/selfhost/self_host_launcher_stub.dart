import 'self_host_types.dart';

/// Web and mobile builds cannot spawn the server binary; self-hosting runs on
/// the desktop app (or a separately deployed server).
final bool selfHostSupported = false;

ServerLauncher createServerLauncher() => throw UnsupportedError(
      'self-hosting needs the desktop build; run qr_race_server on a PC',
    );
