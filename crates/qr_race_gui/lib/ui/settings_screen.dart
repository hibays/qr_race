import 'package:flutter/material.dart';

import '../state/app_state.dart';
import '../state/server_settings.dart';
import 'theme.dart';

class SettingsScreen extends StatefulWidget {
  const SettingsScreen({super.key, required this.state});

  final AppState state;

  @override
  State<SettingsScreen> createState() => _SettingsScreenState();
}

class _SettingsScreenState extends State<SettingsScreen> {
  final _server = TextEditingController();
  final _fingerprint = TextEditingController();
  final _token = TextEditingController();
  bool _loaded = false;
  bool _busy = false;

  @override
  void initState() {
    super.initState();
    ServerSettings.load().then((settings) {
      if (!mounted) return;
      setState(() {
        _server.text = settings.server;
        _fingerprint.text = settings.fingerprint;
        _token.text = settings.token;
        _loaded = true;
      });
    });
  }

  @override
  void dispose() {
    _server.dispose();
    _fingerprint.dispose();
    _token.dispose();
    super.dispose();
  }

  Future<void> _save({bool reconnect = false}) async {
    if (_busy) return;
    setState(() => _busy = true);
    try {
      if (reconnect) {
        final ok = await widget.state.connect(
          server: _server.text,
          token: _token.text,
          fingerprint: _fingerprint.text,
        );
        if (!mounted) return;
        if (!ok) {
          ScaffoldMessenger.of(context).showSnackBar(
            SnackBar(content: Text(widget.state.error ?? '连接失败')),
          );
          return;
        }
        await widget.state.refreshLobby();
      }
      await ServerSettings.save(
        server: _server.text,
        fingerprint: _fingerprint.text,
        token: _token.text,
      );
      if (!mounted) return;
      if (reconnect) {
        Navigator.of(context).pop();
      } else {
        ScaffoldMessenger.of(context).showSnackBar(
          const SnackBar(content: Text('连接信息已保存')),
        );
      }
    } finally {
      if (mounted) setState(() => _busy = false);
    }
  }

  @override
  Widget build(BuildContext context) {
    return Scaffold(
      appBar: AppBar(title: const Text('设置')),
      body: !_loaded
          ? const Center(child: CircularProgressIndicator())
          : ListView(
              padding: const EdgeInsets.all(20),
              children: [
                const Text(
                  '服务器与账号',
                  style: TextStyle(fontSize: 18, fontWeight: FontWeight.w700),
                ),
                const SizedBox(height: 8),
                const Text(
                  '服务器地址、证书指纹和 token 都会保存在本机。重启后会自动验证并直接进入大厅，不需要重复登录。',
                  style: TextStyle(color: muted, fontSize: 13, height: 1.45),
                ),
                const SizedBox(height: 18),
                TextField(
                  controller: _server,
                  decoration: const InputDecoration(
                    labelText: '服务器地址',
                    hintText: 'https://example.test',
                  ),
                  keyboardType: TextInputType.url,
                  autocorrect: false,
                ),
                const SizedBox(height: 14),
                TextField(
                  controller: _fingerprint,
                  decoration: const InputDecoration(
                    labelText: '证书指纹（可选）',
                    hintText: 'SHA-256，自签名 HTTPS 时填写',
                  ),
                  autocorrect: false,
                ),
                const SizedBox(height: 14),
                TextField(
                  controller: _token,
                  decoration: const InputDecoration(
                    labelText: '访问 token',
                    helperText: '仅保存在本机',
                  ),
                  autocorrect: false,
                  obscureText: true,
                ),
                const SizedBox(height: 22),
                FilledButton(
                  onPressed: _busy ? null : () => _save(reconnect: true),
                  child: _busy
                      ? const SizedBox(
                          width: 20,
                          height: 20,
                          child: CircularProgressIndicator(strokeWidth: 2),
                        )
                      : const Text('保存并重新连接'),
                ),
                const SizedBox(height: 10),
                OutlinedButton(
                  onPressed: _busy ? null : _save,
                  child: const Text('仅保存'),
                ),
              ],
            ),
    );
  }
}
