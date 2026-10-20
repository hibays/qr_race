import 'dart:async';
import 'dart:convert';
import 'dart:ffi';
import 'dart:io';

import 'package:ffi/ffi.dart';

import '../native/ffi_library.dart';
import 'self_host_types.dart';

final bool selfHostSupported = true;

ServerLauncher createServerLauncher() => Platform.isAndroid
    ? EmbeddedServerLauncher()
    : ProcessServerLauncher();

/// Desktop: the bridge owns the server process in a Windows job object, so
/// closing the GUI kills it and frees the port. Config/accounts/DB live in a
/// stable directory and are reused on the next start.
class ProcessServerLauncher implements ServerLauncher {
  DynamicLibrary? _library;
  int Function(Pointer<Uint8>, int, Pointer<Uint8>, int)? _start;
  void Function()? _stop;
  int Function()? _status;
  int Function(Pointer<Uint8>, int, Pointer<Uint8>, int)? _fingerprint;

  void _load() {
    if (_library != null) return;
    final library = openQrRaceFfi();
    _library = library;
    _start = library.lookupFunction<
        Int32 Function(Pointer<Uint8>, IntPtr, Pointer<Uint8>, IntPtr),
        int Function(Pointer<Uint8>, int, Pointer<Uint8>, int)>(
      'qr_desktop_server_start',
    );
    _stop = library.lookupFunction<Void Function(), void Function()>(
      'qr_desktop_server_stop',
    );
    _status = library.lookupFunction<Int32 Function(), int Function()>(
      'qr_desktop_server_status',
    );
    _fingerprint = library.lookupFunction<
        Int32 Function(Pointer<Uint8>, IntPtr, Pointer<Uint8>, IntPtr),
        int Function(Pointer<Uint8>, int, Pointer<Uint8>, int)>(
      'qr_cert_fingerprint',
    );
  }

  @override
  Future<SelfHostInfo> start({
    int port = 8443,
    TlsOptions tls = const TlsOptions.selfSigned(),
  }) async {
    await stop();
    _load();
    final binary = _findServerBinary();
    final dataDirectory = Directory(
      '${_dataRoot()}${Platform.pathSeparator}desktop',
    );
    await dataDirectory.create(recursive: true);
    final configFile =
        File('${dataDirectory.path}${Platform.pathSeparator}config.json');

    Map<String, Object>? host;
    if (await configFile.exists()) {
      try {
        final existing =
            jsonDecode(await configFile.readAsString()) as Map<String, dynamic>;
        final accounts = existing['accounts'] as List<dynamic>? ?? const [];
        if (accounts.isNotEmpty) {
          host = (accounts.first as Map<String, dynamic>)
              .map((key, value) => MapEntry(key, value as Object));
        }
      } catch (_) {
        // Corrupt config: mint a fresh identity below.
      }
    }
    host ??= (await _provision(binary, 'host')).account;

    final lan = await _lanAddress();
    final hosts = <String>['localhost', '127.0.0.1', ?lan];
    final useTls = tls.mode != SelfHostTls.none;
    String? certPath;
    final config = <String, Object>{
      'listen': '0.0.0.0:$port',
      'database': '${dataDirectory.path}${Platform.pathSeparator}qr_race.db',
      'accounts': [host],
    };
    if (tls.mode == SelfHostTls.selfSigned) {
      certPath = '${dataDirectory.path}${Platform.pathSeparator}cert.pem';
      config['tls'] = {
        'cert': certPath,
        'key': '${dataDirectory.path}${Platform.pathSeparator}key.pem',
        'hosts': hosts,
      };
    } else if (tls.mode == SelfHostTls.files) {
      final cert = tls.certPath;
      final key = tls.keyPath;
      if (cert == null || key == null) {
        throw StateError('certificate and key paths are required');
      }
      certPath = File(cert).absolute.path;
      config['tls'] = {
        'cert': certPath,
        'key': File(key).absolute.path,
        'hosts': hosts,
      };
    }
    await configFile.writeAsString(
      const JsonEncoder.withIndent('  ').convert(config),
    );

    final binaryPtr = binary.toNativeUtf8();
    final configPtr = configFile.absolute.path.toNativeUtf8();
    int code;
    try {
      code = _start!(
        binaryPtr.cast<Uint8>(),
        utf8.encode(binary).length,
        configPtr.cast<Uint8>(),
        utf8.encode(configFile.absolute.path).length,
      );
    } finally {
      malloc.free(binaryPtr);
      malloc.free(configPtr);
    }
    if (code != 0) {
      throw StateError('bridge could not start the desktop server');
    }

    final address = lan ?? '127.0.0.1';
    await _waitListening(address, port);
    String fingerprint = '';
    if (useTls && certPath != null) {
      fingerprint = await _waitFingerprint(File(certPath));
    }
    final scheme = useTls ? 'https' : 'http';
    return SelfHostInfo(
      url: '$scheme://$address:$port',
      fingerprint: fingerprint,
      dataDirectory: dataDirectory.path,
      hostToken: host['token'] as String,
      hostSecret: (host['totp_secret'] as String?) ?? '',
      memberToken: '',
      memberSecret: '',
    );
  }

  @override
  Future<void> stop() async {
    _load();
    _stop?.call();
  }

  Future<void> _waitListening(String host, int port) async {
    final deadline = DateTime.now().add(const Duration(seconds: 25));
    while (DateTime.now().isBefore(deadline)) {
      try {
        final socket = await Socket.connect(
          host,
          port,
          timeout: const Duration(milliseconds: 400),
        );
        socket.destroy();
        return;
      } catch (_) {
        if (_status?.call() == 0) {
          throw StateError('desktop server exited before listening');
        }
        await Future<void>.delayed(const Duration(milliseconds: 200));
      }
    }
    await stop();
    throw StateError('desktop server did not listen on $host:$port');
  }

  Future<String> _waitFingerprint(File cert) async {
    final deadline = DateTime.now().add(const Duration(seconds: 10));
    while (DateTime.now().isBefore(deadline)) {
      if (await cert.exists()) {
        final fingerprint = _readFingerprint(cert.absolute.path);
        if (fingerprint.isNotEmpty) return fingerprint;
      }
      await Future<void>.delayed(const Duration(milliseconds: 150));
    }
    await stop();
    throw StateError('desktop server did not generate a certificate');
  }

  String _readFingerprint(String path) {
    final pathPtr = path.toNativeUtf8();
    final buffer = malloc<Uint8>(128);
    try {
      final length = _fingerprint!(
        pathPtr.cast<Uint8>(),
        utf8.encode(path).length,
        buffer,
        128,
      );
      if (length <= 0) return '';
      return utf8.decode(buffer.asTypedList(length));
    } finally {
      malloc.free(pathPtr);
      malloc.free(buffer);
    }
  }

  String _findServerBinary() {
    final separator = Platform.pathSeparator;
    final name = Platform.isWindows ? 'qr_race_server.exe' : 'qr_race_server';
    final executableDir = File(Platform.resolvedExecutable).parent.path;
    final candidates = <String>[
      ?Platform.environment['QR_RACE_SERVER'],
      '$executableDir$separator$name',
      ..._projectTargets(executableDir, name, separator),
      ..._projectTargets(Directory.current.path, name, separator),
    ];
    for (final candidate in candidates) {
      if (candidate.isEmpty || !File(candidate).existsSync()) continue;
      return candidate;
    }
    throw StateError(
      'could not find $name; build it with '
      '`cargo build --release -p qr_race_server` or set QR_RACE_SERVER',
    );
  }

  List<String> _projectTargets(String start, String name, String separator) {
    final targets = <String>[];
    var directory = start;
    for (var depth = 0; depth < 9; depth++) {
      for (final profile in ['release', 'debug']) {
        targets.add(
          '$directory${separator}target$separator$profile$separator$name',
        );
      }
      final parent = Directory(directory).parent.path;
      if (parent == directory) break;
      directory = parent;
    }
    return targets;
  }

  Future<_ProvisionedAccount> _provision(String binary, String user) async {
    final result = await Process.run(
      binary,
      ['provision', '--user', user],
      stdoutEncoding: utf8,
    );
    if (result.exitCode != 0) {
      throw StateError(
        'provision $user failed (${result.exitCode}): ${result.stderr}',
      );
    }
    final line = (result.stdout as String)
        .split('\n')
        .map((line) => line.trim())
        .firstWhere((line) => line.startsWith('{'), orElse: () => '');
    final decoded = jsonDecode(line) as Map<String, dynamic>;
    return _ProvisionedAccount({
      'user_id': decoded['user_id'] as String,
      'token': decoded['token'] as String,
      'totp_secret': decoded['totp_secret'] as String,
      'admin': decoded['admin'] as bool? ?? false,
    });
  }

  Future<String?> _lanAddress() async {
    final interfaces = await NetworkInterface.list(
      type: InternetAddressType.IPv4,
      includeLoopback: false,
    );
    final candidates = <String>[];
    for (final interface in interfaces) {
      for (final address in interface.addresses) {
        if (!address.isLoopback && !address.isLinkLocal) {
          candidates.add(address.address);
        }
      }
    }
    int score(String address) {
      if (address.startsWith('192.168.')) return 300;
      if (address.startsWith('10.')) return 200;
      final parts = address.split('.').map(int.tryParse).toList();
      final second = parts.length > 1 ? parts[1] ?? 0 : 0;
      if (parts.length == 4 &&
          parts[0] == 172 &&
          second >= 16 &&
          second <= 31) {
        return 250;
      }
      // CGNAT/Tailscale/VPN and public addresses are poor LAN candidates.
      if (address.startsWith('100.')) return 0;
      return 10;
    }

    candidates.sort((a, b) => score(b).compareTo(score(a)));
    return candidates.isEmpty ? null : candidates.first;
  }

  String _dataRoot() {
    final separator = Platform.pathSeparator;
    final appData = Platform.environment['APPDATA'];
    if (appData != null && appData.isNotEmpty) {
      return '$appData${separator}qr_race${separator}selfhost';
    }
    final home = Platform.environment['HOME'] ??
        Platform.environment['USERPROFILE'] ??
        Directory.current.path;
    return '$home$separator.qr_race${separator}selfhost';
  }
}

class _ProvisionedAccount {
  _ProvisionedAccount(this.account);

  final Map<String, Object> account;

  String get token => account['token'] as String;
  String get totpSecret => account['totp_secret'] as String;
}

/// Android: the server is compiled into `libqr_race_ffi.so` and runs on a Rust
/// thread inside the app. It binds 127.0.0.1 only and keeps state in memory.
class EmbeddedServerLauncher implements ServerLauncher {
  DynamicLibrary? _library;
  int Function(Pointer<Uint8>, int)? _start;
  int Function(Pointer<Uint8>, int)? _info;
  int Function(Pointer<Uint8>, int)? _lastError;
  void Function()? _stop;

  void _load() {
    if (_library != null) return;
    final library = openQrRaceFfi();
    _library = library;
    _start = library.lookupFunction<
        Int32 Function(Pointer<Uint8>, IntPtr),
        int Function(Pointer<Uint8>, int)>('qr_server_start_config');
    _info = library.lookupFunction<
        Int32 Function(Pointer<Uint8>, IntPtr),
        int Function(Pointer<Uint8>, int)>('qr_server_info');
    _lastError = library.lookupFunction<
        Int32 Function(Pointer<Uint8>, IntPtr),
        int Function(Pointer<Uint8>, int)>('qr_server_last_error');
    _stop = library.lookupFunction<Void Function(), void Function()>(
      'qr_server_stop',
    );
  }

  @override
  Future<SelfHostInfo> start({
    int port = 8443,
    TlsOptions tls = const TlsOptions.selfSigned(),
  }) async {
    _load();
    final certificateDir = Directory(
      '${Directory.systemTemp.path}${Platform.pathSeparator}qr_race_selfhost_certs',
    );
    await certificateDir.create(recursive: true);
    final tlsConfig = switch (tls.mode) {
      SelfHostTls.none => {'mode': 'none'},
      SelfHostTls.selfSigned => {
          'mode': 'self-signed',
          'dir': certificateDir.path,
        },
      SelfHostTls.files => () {
          if (tls.certPath == null || tls.keyPath == null) {
            throw StateError('请选择证书和私钥文件');
          }
          return {
            'mode': 'files',
            'cert': tls.certPath,
            'key': tls.keyPath,
          };
        }(),
    };
    final config = utf8.encode(
      jsonEncode({'port': port, 'tls': tlsConfig}),
    );
    final pointer = calloc<Uint8>(config.length);
    try {
      pointer.asTypedList(config.length).setAll(0, config);
      final code = _start!(pointer, config.length);
      if (code != 0) {
        final errorBuffer = calloc<Uint8>(8 * 1024);
        try {
          final length = _lastError!(errorBuffer, 8 * 1024);
          final message = length > 0
              ? utf8.decode(errorBuffer.asTypedList(length))
              : 'unknown error';
          throw StateError('qr_server_start failed: $message');
        } finally {
          calloc.free(errorBuffer);
        }
      }
    } finally {
      calloc.free(pointer);
    }

    final buffer = calloc<Uint8>(64 * 1024);
    try {
      final length = _info!(buffer, 64 * 1024);
      if (length <= 0) {
        _stop!();
        throw StateError('qr_server_info returned $length');
      }
      final json = jsonDecode(utf8.decode(buffer.asTypedList(length)))
          as Map<String, dynamic>;
      return SelfHostInfo(
        url: json['url'] as String,
        fingerprint: json['fingerprint'] as String? ?? '',
        dataDirectory: tls.mode == SelfHostTls.none
            ? 'Android 进程内（HTTP · 内存存储）'
            : certificateDir.path,
        hostToken: json['host_token'] as String,
        hostSecret: json['host_secret'] as String,
        memberToken: json['member_token'] as String,
        memberSecret: json['member_secret'] as String,
      );
    } finally {
      calloc.free(buffer);
    }
  }

  @override
  Future<void> stop() async {
    _stop?.call();
  }
}


