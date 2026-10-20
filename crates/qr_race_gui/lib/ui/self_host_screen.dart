import 'package:flutter/material.dart';
import 'package:flutter/services.dart';

import '../state/app_state.dart';
import '../selfhost/self_host_launcher.dart';
import 'lobby_screen.dart';
import 'theme.dart';

/// Desktop-only one-click server: provision accounts, write config, start
/// `qr_race_server`, show URL/fingerprint/credentials and connect.
class SelfHostScreen extends StatefulWidget {
  const SelfHostScreen({super.key, required this.state, this.launcher});

  final AppState state;
  final ServerLauncher? launcher;

  @override
  State<SelfHostScreen> createState() => _SelfHostScreenState();
}

class _SelfHostScreenState extends State<SelfHostScreen> {
  final _port = TextEditingController(text: '8443');
  ServerLauncher? _launcher;
  SelfHostInfo? _info;
  String? _error;
  bool _busy = false;

  @override
  void initState() {
    super.initState();
    if (selfHostSupported) {
      _launcher = widget.launcher ?? createServerLauncher();
    }
  }

  @override
  void dispose() {
    _port.dispose();
    super.dispose();
  }

  Future<void> _start() async {
    final launcher = _launcher;
    if (launcher == null) return;
    final port = int.tryParse(_port.text.trim());
    if (port == null || port < 1024 || port > 65535) {
      setState(() => _error = '端口需要在 1024–65535 之间');
      return;
    }
    setState(() {
      _busy = true;
      _error = null;
    });
    try {
      final info = await launcher.start(port: port);
      if (!mounted) return;
      setState(() => _info = info);
    } catch (error) {
      if (!mounted) return;
      setState(() => _error = error.toString());
    } finally {
      if (mounted) setState(() => _busy = false);
    }
  }

  Future<void> _stop() async {
    final launcher = _launcher;
    if (launcher == null) return;
    setState(() => _busy = true);
    try {
      await launcher.stop();
      if (mounted) setState(() => _info = null);
    } finally {
      if (mounted) setState(() => _busy = false);
    }
  }

  Future<void> _connectAsHost() async {
    final info = _info;
    if (info == null) return;
    final code = totpCode(info.hostSecret);
    final ok = await widget.state.connect(
      server: info.url,
      token: info.hostToken,
      code: code,
      fingerprint: info.fingerprint,
    );
    if (!mounted) return;
    if (!ok) {
      setState(() => _error = widget.state.error ?? '连接失败');
      return;
    }
    Navigator.of(context).pushReplacement(
      MaterialPageRoute(builder: (_) => LobbyScreen(state: widget.state)),
    );
  }

  @override
  Widget build(BuildContext context) {
    return Scaffold(
      appBar: AppBar(
        title: const Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Text('自部署服务器', style: TextStyle(fontSize: 18, fontWeight: FontWeight.w700)),
            Text(
              '在本机启动 qr_race_server（桌面版）',
              style: TextStyle(fontSize: 12, color: muted),
            ),
          ],
        ),
      ),
      body: ListView(
        padding: const EdgeInsets.fromLTRB(16, 14, 16, 28),
        children: [
          if (!selfHostSupported) _unsupported() else ..._supported(),
        ],
      ),
    );
  }

  Widget _unsupported() {
    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        const _Note(
          '手机与 Web 版本不能托管服务器（服务端是 Rust 二进制，需要桌面系统）。',
        ),
        const SizedBox(height: 16),
        const Text('在电脑上自部署', style: TextStyle(fontWeight: FontWeight.w700)),
        const SizedBox(height: 6),
        const Text(
          '1. 打开同目录的桌面版 qr_race（Windows）。\n'
          '2. 连接页点“自部署服务器”，启动后页面会给出 URL、证书指纹和账号。\n'
          '3. 手机连接页填入该 URL 与指纹，再用房主 token + 验证码连接。\n\n'
          '也可以手动运行：\n'
          '  cargo build --release -p qr_race_server\n'
          '  target\\release\\qr_race_server.exe serve --config config.json\n'
          '详见仓库根目录 DEPLOY.md。',
          style: TextStyle(color: muted, height: 1.55, fontSize: 12.5),
        ),
      ],
    );
  }

  List<Widget> _supported() {
    final info = _info;
    return [
      const _Note(
        '一键完成：生成两个账号 → 写 TLS 配置 → 启动服务器。'
        '服务器保持运行，直到你在这里点“停止”。',
      ),
      const SizedBox(height: 14),
      Row(
        children: [
          SizedBox(
            width: 150,
            child: TextField(
              controller: _port,
              decoration: const InputDecoration(labelText: '端口'),
              keyboardType: TextInputType.number,
              enabled: info == null && !_busy,
            ),
          ),
          const SizedBox(width: 12),
          Expanded(
            child: FilledButton(
              onPressed: info == null && !_busy ? _start : null,
              child: _busy && info == null
                  ? const SizedBox(
                      width: 18,
                      height: 18,
                      child: CircularProgressIndicator(strokeWidth: 2, color: accentInk),
                    )
                  : const Text('启动本机服务器'),
            ),
          ),
        ],
      ),
      if (_error != null) ...[
        const SizedBox(height: 12),
        Text(_error!, style: const TextStyle(color: danger, fontSize: 12.5, height: 1.4)),
      ],
      if (info != null) ...[
        const SizedBox(height: 18),
        _InfoCard(info: info),
        const SizedBox(height: 12),
        _AccountCard(
          title: '房主账号',
          token: info.hostToken,
          secret: info.hostSecret,
          onConnect: _connectAsHost,
        ),
        const SizedBox(height: 8),
        _AccountCard(
          title: '成员账号',
          token: info.memberToken,
          secret: info.memberSecret,
        ),
        const SizedBox(height: 16),
        OutlinedButton.icon(
          onPressed: _busy ? null : _stop,
          icon: const Icon(Icons.stop_circle_outlined),
          label: const Text('停止服务器'),
        ),
      ],
    ];
  }
}

class _Note extends StatelessWidget {
  const _Note(this.message);

  final String message;

  @override
  Widget build(BuildContext context) {
    return Container(
      padding: const EdgeInsets.all(12),
      decoration: BoxDecoration(
        color: surface,
        border: Border.all(color: line),
        borderRadius: BorderRadius.circular(10),
      ),
      child: Text(
        message,
        style: const TextStyle(color: muted, fontSize: 12.5, height: 1.45),
      ),
    );
  }
}

class _InfoCard extends StatelessWidget {
  const _InfoCard({required this.info});

  final SelfHostInfo info;

  @override
  Widget build(BuildContext context) {
    return Container(
      padding: const EdgeInsets.all(14),
      decoration: BoxDecoration(
        color: surfaceRaised,
        border: Border.all(color: line),
        borderRadius: BorderRadius.circular(12),
      ),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          const Text('服务器已启动', style: TextStyle(fontWeight: FontWeight.w700)),
          const SizedBox(height: 8),
          _CopyRow(label: 'URL', value: info.url),
          _CopyRow(label: '证书指纹', value: info.fingerprint),
          _CopyRow(label: '数据目录', value: info.dataDirectory),
        ],
      ),
    );
  }
}

class _CopyRow extends StatelessWidget {
  const _CopyRow({required this.label, required this.value});

  final String label;
  final String value;

  @override
  Widget build(BuildContext context) {
    return Padding(
      padding: const EdgeInsets.symmetric(vertical: 4),
      child: Row(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          SizedBox(
            width: 74,
            child: Text(label, style: const TextStyle(color: muted, fontSize: 12)),
          ),
          Expanded(
            child: SelectableText(
              value,
              style: const TextStyle(fontSize: 12, height: 1.35),
            ),
          ),
          IconButton(
            tooltip: '复制',
            iconSize: 18,
            onPressed: () => Clipboard.setData(ClipboardData(text: value)),
            icon: const Icon(Icons.copy_all_outlined),
          ),
        ],
      ),
    );
  }
}

class _AccountCard extends StatelessWidget {
  const _AccountCard({
    required this.title,
    required this.token,
    required this.secret,
    this.onConnect,
  });

  final String title;
  final String token;
  final String secret;
  final Future<void> Function()? onConnect;

  @override
  Widget build(BuildContext context) {
    return Container(
      padding: const EdgeInsets.all(12),
      decoration: BoxDecoration(
        color: surface,
        border: Border.all(color: line),
        borderRadius: BorderRadius.circular(10),
      ),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Row(
            children: [
              Expanded(
                child: Text(title, style: const TextStyle(fontWeight: FontWeight.w600)),
              ),
              if (onConnect != null)
                TextButton(
                  onPressed: onConnect,
                  child: const Text('用此账号连接'),
                ),
            ],
          ),
          _CopyRow(label: 'token', value: token),
          _CopyRow(label: 'TOTP secret', value: secret),
        ],
      ),
    );
  }
}
