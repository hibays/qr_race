/// Desktop-only self-hosting launcher; the stub reports unsupported.
library;

export 'self_host_launcher_stub.dart'
    if (dart.library.io) 'self_host_launcher_native.dart';

export 'self_host_types.dart';
