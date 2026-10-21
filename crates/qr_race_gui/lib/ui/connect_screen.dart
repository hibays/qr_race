import 'package:flutter/material.dart';

import '../api/app_links.dart';
import '../api/cert_fingerprint.dart';
import '../api/connect_link.dart';
import '../state/app_state.dart';
import '../state/server_settings.dart';
import 'lobby_screen.dart';
import 'room_screen.dart';
import 'self_host_screen.dart';
import 'theme.dart';

class ConnectScreen extends StatefulWidget {
  const ConnectScreen({super.key, required this.state, this.initialLink});

  final AppState state;

  /// Tests can inject a `qrrace://connect?...` link; Android passes the intent
  /// through the `app.qrrace.qr_race_gui/links` channel.
  final String? initialLink;

  @override
  State<ConnectScreen> createState() => _ConnectScreenState();
}

class _ConnectScreenState extends State<ConnectScreen> {
  final _formKey = GlobalKey<FormState>();
  final _server = TextEditingController(text: 'http://127.0.0.1:8080');
  final _fingerprint = TextEditingController();
  final _token = TextEditingController();
  String? _linkRoom;
  @override
  void initState() {
    super.initState();

    final initial = widget.initialLink;
    if (initial != null) {
      _applyLink(initial);
    } else {
      AppLinks.initial().then((link) {
        if (!mounted) return;
        if (link != null) {
          _applyLink(link);
        } else {
          _loadSaved();
        }
      });
    }
  }

  Future<void> _loadSaved() async {
    final settings = await ServerSettings.load();
    if (!mounted) return;
    setState(() {
      _server.text = settings.server;
      _fingerprint.text = settings.fingerprint;
    });
  }

  void _applyLink(String raw) {
    final link = parseConnectLink(raw);
    if (link == null) return;
    setState(() {
      _server.text = link.server;
      if (link.fingerprint != null) _fingerprint.text = link.fingerprint!;
      if (link.token != null) _token.text = link.token!;
      _linkRoom = link.room;
    });
    WidgetsBinding.instance.addPostFrameCallback((_) {
      if (!mounted) return;
      ScaffoldMessenger.of(context).showSnackBar(
        const SnackBar(content: Text('已填充连接，直接点连接')),
      );
    });
  }

  Future<void> _askLink() async {
    final raw = await showDialog<String>(
      context: context,
      builder: (context) => const _LinkDialog(),
    );
    if (raw == null || !mounted) return;
    if (parseConnectLink(raw) == null) {
      ScaffoldMessenger.of(context).showSnackBar(
        const SnackBar(content: Text('无法识别这个连接链接')),
      );
      return;
    }
    _applyLink(raw);
  }

  @override
  void dispose() {
    _server.dispose();
    _fingerprint.dispose();
    _token.dispose();
    super.dispose();
  }

  Future<void> _connect() async {
    if (!_formKey.currentState!.validate()) return;
    final ok = await widget.state.connect(
      server: _server.text,
      token: _token.text,
      fingerprint: _fingerprint.text,
    );
    if (!mounted) return;
    if (ok) {
      await ServerSettings.save(
        server: _server.text,
        fingerprint: _fingerprint.text,
      );
      if (!mounted) return;
      final room = _linkRoom;
      if (room != null) {
        final entered = await widget.state.enterRoom(room);
        if (!mounted) return;
        if (!entered) {
          ScaffoldMessenger.of(context).showSnackBar(
            SnackBar(content: Text(widget.state.error ?? '进入房间失败')),
          );
          return;
        }
        Navigator.of(context).pushReplacement(
          MaterialPageRoute(
            builder: (_) => RoomScreen(state: widget.state, address: room),
          ),
        );
        return;
      }
      Navigator.of(context).pushReplacement(
        MaterialPageRoute(builder: (_) => LobbyScreen(state: widget.state)),
      );
    }
  }

  @override
  Widget build(BuildContext context) {
    return Scaffold(
      body: ListenableBuilder(
        listenable: widget.state,
        builder: (context, _) {
          final state = widget.state;
          return Center(
            child: SingleChildScrollView(
              padding: const EdgeInsets.symmetric(horizontal: 24, vertical: 40),
              child: ConstrainedBox(
                constraints: const BoxConstraints(maxWidth: 430),
                child: Form(
                  key: _formKey,
                  child: Column(
                    crossAxisAlignment: CrossAxisAlignment.stretch,
                    children: [
                      const Text(
                        'qr_race',
                        style: TextStyle(
                          fontSize: 34,
                          fontWeight: FontWeight.w800,
                          letterSpacing: -0.5,
                          color: ink,
                        ),
                      ),
                      const SizedBox(height: 6),
                      const Text(
                        '现场扫码接力 · 房主扫一次，全房间亮屏',
                        style: TextStyle(color: muted, height: 1.4),
                      ),
                      const SizedBox(height: 32),
                      TextFormField(
                        controller: _server,
                        decoration: const InputDecoration(
                          labelText: '服务器地址',
                          hintText: 'https://example.test',
                        ),
                        keyboardType: TextInputType.url,
                        autocorrect: false,
                        validator: (value) =>
                            (value == null || value.trim().isEmpty) ? '请输入服务器地址' : null,
                      ),
                      const SizedBox(height: 14),
                      TextFormField(
                        controller: _fingerprint,
                        decoration: const InputDecoration(
                          labelText: '证书指纹（可选）',
                          hintText: 'SHA-256，自签名 HTTPS 时填写',
                        ),
                        autocorrect: false,
                        validator: (value) {
                          try {
                            normalizeFingerprint(value);
                            return null;
                          } on FormatException catch (error) {
                            return error.message;
                          }
                        },
                      ),
                      const SizedBox(height: 14),
                      TextFormField(
                        controller: _token,
                        decoration: const InputDecoration(labelText: '访问 token'),
                        autocorrect: false,
                        obscureText: true,
                        validator: (value) =>
                            (value == null || value.trim().isEmpty) ? '请输入 token' : null,
                        onFieldSubmitted: (_) => _connect(),
                      ),
                      const SizedBox(height: 8),
                      FilledButton(
                        onPressed: state.busy ? null : _connect,
                        child: state.busy
                            ? const SizedBox(
                                width: 20,
                                height: 20,
                                child: CircularProgressIndicator(
                                  strokeWidth: 2,
                                  color: accentInk,
                                ),
                              )
                            : const Text('连接'),
                      ),
                      if (state.error != null) ...[
                        const SizedBox(height: 16),
                        _ErrorLine(message: state.error!),
                      ],
                      const SizedBox(height: 12),
                      TextButton.icon(
                        onPressed: _askLink,
                        icon: const Icon(Icons.link_outlined, size: 18),
                        label: const Text('从连接链接填充'),
                      ),
                      TextButton.icon(
                        onPressed: () => Navigator.of(context).push(
                          MaterialPageRoute(
                            builder: (_) => SelfHostScreen(state: widget.state),
                          ),
                        ),
                        icon: const Icon(Icons.dns_outlined, size: 18),
                        label: const Text('自部署服务器（本机）'),
                      ),
                      const SizedBox(height: 28),
                      const Text(
                        'token 由运营方发放。自签名 HTTPS 时把服务端打印的证书指纹填进上方，浏览器端则接受一次证书警告。',
                        style: TextStyle(color: muted, fontSize: 12.5, height: 1.5),
                      ),
                    ],
                  ),
                ),
              ),
            ),
          );
        },
      ),
    );
  }
}

class _ErrorLine extends StatelessWidget {
  const _ErrorLine({required this.message});

  final String message;

  @override
  Widget build(BuildContext context) {
    return Container(
      padding: const EdgeInsets.symmetric(horizontal: 12, vertical: 10),
      decoration: BoxDecoration(
        color: danger.withValues(alpha: 0.12),
        border: Border.all(color: danger.withValues(alpha: 0.5)),
        borderRadius: BorderRadius.circular(10),
      ),
      child: Text(message, style: const TextStyle(color: danger, height: 1.4)),
    );
  }
}

class _LinkDialog extends StatefulWidget {
  const _LinkDialog();

  @override
  State<_LinkDialog> createState() => _LinkDialogState();
}

class _LinkDialogState extends State<_LinkDialog> {
  final _controller = TextEditingController();

  @override
  void dispose() {
    _controller.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    return AlertDialog(
      title: const Text('从连接链接填充'),
      content: TextField(
        controller: _controller,
        autofocus: true,
        minLines: 1,
        maxLines: 4,
        decoration: const InputDecoration(
          labelText: 'qrrace://connect?...',
          hintText: '粘贴服务器、指纹和 token',
        ),
      ),
      actions: [
        TextButton(
          onPressed: () => Navigator.pop(context),
          child: const Text('取消'),
        ),
        FilledButton(
          onPressed: () => Navigator.pop(context, _controller.text),
          child: const Text('填充'),
        ),
      ],
    );
  }
}






