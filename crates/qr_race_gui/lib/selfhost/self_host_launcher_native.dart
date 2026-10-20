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

/// Starts `qr_race_server` with a freshly generated config, then reads the
/// printed certificate fingerprint. The server process stays alive until
/// [stop] is called.
class ProcessServerLauncher implements ServerLauncher {
  Process? _process;

  @override
  Future<SelfHostInfo> start({
    int port = 8443,
    TlsOptions tls = const TlsOptions.selfSigned(),
  }) async {
    await stop();
    final binary = _findServerBinary();
    final stamp = DateTime.now().millisecondsSinceEpoch;
    final dataDirectory = Directory(
      '${_dataRoot()}${Platform.pathSeparator}selfhost-$stamp',
    );
    await dataDirectory.create(recursive: true);

    final host = await _provision(binary, 'host');
    final member = await _provision(binary, 'member');
    final lan = await _lanAddress();
    final hosts = <String>[
      'localhost',
      '127.0.0.1',
      ?lan,
    ];
    final useTls = tls.mode != SelfHostTls.none;
    final config = <String, Object>{
      'listen': '0.0.0.0:$port',
      'database': '${dataDirectory.path}${Platform.pathSeparator}qr_race.db',
      'accounts': [host.account, member.account],
    };
    if (tls.mode == SelfHostTls.selfSigned) {
      config['tls'] = {
        'cert': '${dataDirectory.path}${Platform.pathSeparator}cert.pem',
        'key': '${dataDirectory.path}${Platform.pathSeparator}key.pem',
        'hosts': hosts,
      };
    } else if (tls.mode == SelfHostTls.files) {
      final cert = tls.certPath;
      final key = tls.keyPath;
      if (cert == null || key == null) {
        throw StateError('certificate and key paths are required');
      }
      config['tls'] = {
        'cert': File(cert).absolute.path,
        'key': File(key).absolute.path,
        'hosts': hosts,
      };
    }
    final configFile =
        File('${dataDirectory.path}${Platform.pathSeparator}config.json');
    await configFile.writeAsString(
      const JsonEncoder.withIndent('  ').convert(config),
    );

    final process = await Process.start(
      binary,
      ['serve', '--config', configFile.path],
      workingDirectory: dataDirectory.path,
    );
    _process = process;

    // null means HTTP is listening; a string is the TLS fingerprint.
    final ready = Completer<String?>();
    final stderr = StringBuffer();
    process.stdout
        .transform(utf8.decoder)
        .transform(const LineSplitter())
        .listen((line) {
      final marker = 'certificate SHA-256:';
      if (line.contains(marker) && !ready.isCompleted) {
        ready.complete(
          line.substring(line.indexOf(marker) + marker.length).trim(),
        );
      } else if (!useTls &&
          line.contains('listening on http') &&
          !ready.isCompleted) {
        ready.complete(null);
      }
    });
    process.stderr.transform(utf8.decoder).listen(stderr.write);
    unawaited(process.exitCode.then((code) {
      if (!ready.isCompleted) {
        ready.completeError(
          StateError('server exited with $code before becoming ready\n$stderr'),
        );
      }
    }));

    final fingerprint = await ready.future.timeout(
      const Duration(seconds: 25),
      onTimeout: () {
        process.kill();
        throw StateError('server did not become ready in time\n$stderr');
      },
    );

    final address = lan ?? '127.0.0.1';
    final scheme = useTls ? 'https' : 'http';
    return SelfHostInfo(
      url: '$scheme://$address:$port',
      fingerprint: fingerprint ?? '',
      dataDirectory: dataDirectory.path,
      hostToken: host.token,
      hostSecret: host.totpSecret,
      memberToken: member.token,
      memberSecret: member.totpSecret,
    );
  }

  @override
  Future<void> stop() async {
    final process = _process;
    _process = null;
    if (process != null) {
      process.kill();
      await process.exitCode.timeout(
        const Duration(seconds: 3),
        onTimeout: () => -1,
      );
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

