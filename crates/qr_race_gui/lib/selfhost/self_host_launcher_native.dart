import 'dart:async';
import 'dart:convert';
import 'dart:io';

import 'self_host_types.dart';

final bool selfHostSupported = true;

ServerLauncher createServerLauncher() => ProcessServerLauncher();

/// Starts `qr_race_server` with a freshly generated config, then reads the
/// printed certificate fingerprint. The server process stays alive until
/// [stop] is called.
class ProcessServerLauncher implements ServerLauncher {
  Process? _process;

  @override
  Future<SelfHostInfo> start({int port = 8443}) async {
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
    final config = <String, Object>{
      'listen': '0.0.0.0:$port',
      'database': '${dataDirectory.path}${Platform.pathSeparator}qr_race.db',
      'tls': {
        'cert': '${dataDirectory.path}${Platform.pathSeparator}cert.pem',
        'key': '${dataDirectory.path}${Platform.pathSeparator}key.pem',
        'hosts': hosts,
      },
      'accounts': [host.account, member.account],
    };
    final configFile =
        File('${dataDirectory.path}${Platform.pathSeparator}config.json');
    await configFile.writeAsString(const JsonEncoder.withIndent('  ').convert(config));

    final process = await Process.start(
      binary,
      ['serve', '--config', configFile.path],
      workingDirectory: dataDirectory.path,
    );
    _process = process;

    final fingerprint = Completer<String>();
    final stderr = StringBuffer();
    process.stdout
        .transform(utf8.decoder)
        .transform(const LineSplitter())
        .listen((line) {
      final marker = 'certificate SHA-256:';
      if (line.contains(marker) && !fingerprint.isCompleted) {
        fingerprint.complete(line.substring(line.indexOf(marker) + marker.length).trim());
      }
    });
    process.stderr.transform(utf8.decoder).listen(stderr.write);
    unawaited(process.exitCode.then((code) {
      if (!fingerprint.isCompleted) {
        fingerprint.completeError(
          StateError('server exited with $code before printing its certificate\n$stderr'),
        );
      }
    }));

    final print = await fingerprint.future.timeout(
      const Duration(seconds: 25),
      onTimeout: () {
        process.kill();
        throw StateError('server did not print a certificate in time\n$stderr');
      },
    );

    final address = lan ?? '127.0.0.1';
    return SelfHostInfo(
      url: 'https://$address:$port',
      fingerprint: print,
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

