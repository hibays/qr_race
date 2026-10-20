import 'package:flutter/foundation.dart';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';

import '../api/connect_link.dart';
import '../native/storage_access.dart';
import '../selfhost/self_host_launcher.dart';
import '../state/app_state.dart';
import 'lobby_screen.dart';
import 'theme.dart';

/// One-click server: provision accounts, choose HTTP/self-signed/custom TLS,
/// start `qr_race_server` (desktop) or the in-process server (Android), show
/// URL/fingerprint/credentials and connect.
class SelfHostScreen extends StatefulWidget {
  const SelfHostScreen({super.key, required this.state, this.launcher});

  final AppState state;
  final ServerLauncher? launcher;

  @override
  State<SelfHostScreen> createState() => _SelfHostScreenState();
}

class _SelfHostScreenState extends State<SelfHostScreen>
    with WidgetsBindingObserver {
  final _port = TextEditingController(text: '8443');
  final _cert = TextEditingController();
  final _key = TextEditingController();
  ServerLauncher? _launcher;
  SelfHostInfo? _info;
  SelfHostTls _tlsMode = SelfHostTls.selfSigned;
  String? _error;
  bool _busy = false;
  bool _allFiles = false;

  bool get _isAndroid => defaultTargetPlatform == TargetPlatform.android;

  @override
  void initState() {
    super.initState();
    WidgetsBinding.instance.addObserver(this);
    if (selfHostSupported) {
      _launcher = widget.launcher ?? createServerLauncher();
    }
    if (_isAndroid) {
      // 8443 is often taken on Android (VPN/other tools); pick a quieter default
      // while keeping the port editable.
      if (_port.text == '8443') _port.text = '18443';
      _refreshAllFiles();
    }
  }

  @override
  void dispose() {
    WidgetsBinding.instance.removeObserver(this);
    _port.dispose();
    _cert.dispose();
    _key.dispose();
    super.dispose();
  }

  @override
  void didChangeAppLifecycleState(AppLifecycleState state) {
    if (state == AppLifecycleState.resumed && _isAndroid) {
      _refreshAllFiles();
    }
  }

  Future<void> _refreshAllFiles() async {
    final granted = await StorageAccess.hasAllFilesAccess();
    if (mounted) setState(() => _allFiles = granted);
  }

  Future<void> _requestAllFiles() async {
    await StorageAccess.requestAllFilesAccess();
    await _refreshAllFiles();
  }

  Future<void> _pickFile(TextEditingController controller) async {
    final path = await StorageAccess.pickFile();
    if (path != null && mounted) {
      setState(() => controller.text = path);
    }
  }

  Future<void> _start() async {
    final launcher = _launcher;
    if (launcher == null) return;
    final port = int.tryParse(_port.text.trim());
    if (port == null || port < 1024 || port > 65535) {
      setState(() => _error = '端口需要在 1024–65535 之间');
      return;
    }
    final TlsOptions tls;
    switch (_tlsMode) {
      case SelfHostTls.none:
        tls = const TlsOptions.none();
      case SelfHostTls.selfSigned:
        tls = const TlsOptions.selfSigned();
      case SelfHostTls.files:
        final cert = _cert.text.trim();
        final key = _key.text.trim();
        if (cert.isEmpty || key.isEmpty) {
          setState(() => _error = '请选择或填写证书和私钥文件');
          return;
        }
        tls = TlsOptions.files(certPath: cert, keyPath: key);
    }
    setState(() {
      _busy = true;
      _error = null;
    });
    try {
      final info = await launcher.start(port: port, tls: tls);
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
    final ok = await widget.state.connect(
      server: info.url,
      token: info.hostToken,
      fingerprint: info.fingerprint.isEmpty ? null : info.fingerprint,
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
              '在本机启动 qr_race_server（仅监听 127.0.0.1）',
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
          'Web 版本不能托管服务端（浏览器里没有 Rust 进程）。请用桌面版或 Android App 自部署。',
        ),
        const SizedBox(height: 16),
        const Text('在电脑上自部署', style: TextStyle(fontWeight: FontWeight.w700)),
        const SizedBox(height: 6),
        const Text(
          '1. 打开同目录的桌面版 qr_race（Windows）。\n'
          '2. 连接页点“自部署服务器”，启动后页面会给出 URL、证书指纹和账号。\n'
          '3. 手机连接页填入该 URL 与指纹，再用房主 token 连接。\n\n'
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
        '一键完成：生成服务器身份 → 配置 TLS/监听 → 启动服务器。'
        '成员不预置账号：房主进房间后点“邀请成员”，服务端为该房间动态生成临时身份。',
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
      const SizedBox(height: 18),
      const Text('连接方式', style: TextStyle(fontWeight: FontWeight.w600)),
      const SizedBox(height: 8),
      SegmentedButton<SelfHostTls>(
        segments: const [
          ButtonSegment(
            value: SelfHostTls.selfSigned,
            label: Text('自签名'),
            icon: Icon(Icons.auto_awesome_outlined, size: 16),
          ),
          ButtonSegment(
            value: SelfHostTls.files,
            label: Text('已有证书'),
            icon: Icon(Icons.folder_outlined, size: 16),
          ),
          ButtonSegment(
            value: SelfHostTls.none,
            label: Text('仅 HTTP'),
            icon: Icon(Icons.lan_outlined, size: 16),
          ),
        ],
        selected: {_tlsMode},
        onSelectionChanged: info == null && !_busy
            ? (selection) => setState(() => _tlsMode = selection.first)
            : null,
      ),
      if (_tlsMode == SelfHostTls.files) ...[
        const SizedBox(height: 10),
        TextField(
          controller: _cert,
          enabled: info == null && !_busy,
          decoration: InputDecoration(
            labelText: '证书 PEM 路径',
            suffixIcon: _isAndroid
                ? IconButton(
                    tooltip: '选择证书',
                    onPressed: () => _pickFile(_cert),
                    icon: const Icon(Icons.folder_open_outlined),
                  )
                : null,
          ),
        ),
        const SizedBox(height: 8),
        TextField(
          controller: _key,
          enabled: info == null && !_busy,
          decoration: InputDecoration(
            labelText: '私钥 PEM 路径',
            suffixIcon: _isAndroid
                ? IconButton(
                    tooltip: '选择私钥',
                    onPressed: () => _pickFile(_key),
                    icon: const Icon(Icons.folder_open_outlined),
                  )
                : null,
          ),
        ),
        const SizedBox(height: 6),
        const Text(
          '证书必须包含客户端实际使用的地址（本机自部署通常用 127.0.0.1）。',
          style: TextStyle(color: muted, fontSize: 11.5, height: 1.4),
        ),
      ],
      if (_isAndroid && _tlsMode != SelfHostTls.none) ...[
        const SizedBox(height: 10),
        Container(
          padding: const EdgeInsets.all(10),
          decoration: BoxDecoration(
            color: surface,
            border: Border.all(color: line),
            borderRadius: BorderRadius.circular(10),
          ),
          child: Row(
            children: [
              Expanded(
                child: Column(
                  crossAxisAlignment: CrossAxisAlignment.start,
                  children: [
                    const Text(
                      '所有文件访问权限',
                      style: TextStyle(fontWeight: FontWeight.w600, fontSize: 13),
                    ),
                    Text(
                      _allFiles ? '已授权，可选任意目录的证书' : '未授权；文件选择器会复制到应用缓存',
                      style: const TextStyle(color: muted, fontSize: 11.5),
                    ),
                  ],
                ),
              ),
              TextButton(
                onPressed: _allFiles ? null : _requestAllFiles,
                child: Text(_allFiles ? '已授权' : '去申请'),
              ),
            ],
          ),
        ),
      ],
      if (_tlsMode == SelfHostTls.none) ...[
        const SizedBox(height: 6),
        const Text(
          'HTTP 只建议用于 127.0.0.1 本机调试；局域网/生产请用 HTTPS。',
          style: TextStyle(color: warn, fontSize: 11.5, height: 1.4),
        ),
      ],
      if (info != null) ...[
        const SizedBox(height: 18),
        _InfoCard(info: info),
        const SizedBox(height: 12),
        _AccountCard(
          title: '服务器账号',
          token: info.hostToken,
          onConnect: _connectAsHost,
          link: buildConnectLink(
            server: info.url,
            fingerprint: info.fingerprint,
            token: info.hostToken,
          ),
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
          if (info.fingerprint.isNotEmpty)
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
    this.onConnect,
    this.link,
  });

  final String title;
  final String token;
  final Future<void> Function()? onConnect;
  final String? link;

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
          if (link != null)
            Align(
              alignment: Alignment.centerLeft,
              child: TextButton.icon(
                onPressed: () => Clipboard.setData(ClipboardData(text: link!)),
                icon: const Icon(Icons.link_outlined, size: 18),
                label: const Text('复制连接链接'),
              ),
            ),
        ],
      ),
    );
  }
}
