import 'package:flutter/material.dart';

import '../state/server_settings.dart';
import 'theme.dart';

class SettingsScreen extends StatefulWidget {
  const SettingsScreen({super.key});

  @override
  State<SettingsScreen> createState() => _SettingsScreenState();
}

class _SettingsScreenState extends State<SettingsScreen> {
  final _server = TextEditingController();
  final _fingerprint = TextEditingController();
  bool _loaded = false;

  @override
  void initState() {
    super.initState();
    ServerSettings.load().then((settings) {
      if (!mounted) return;
      setState(() {
        _server.text = settings.server;
        _fingerprint.text = settings.fingerprint;
        _loaded = true;
      });
    });
  }

  @override
  void dispose() {
    _server.dispose();
    _fingerprint.dispose();
    super.dispose();
  }

  Future<void> _save() async {
    await ServerSettings.save(
      server: _server.text,
      fingerprint: _fingerprint.text,
    );
    if (!mounted) return;
    ScaffoldMessenger.of(context).showSnackBar(
      const SnackBar(content: Text('服务器设置已保存，下次连接生效')),
    );
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
                  '服务器',
                  style: TextStyle(fontSize: 18, fontWeight: FontWeight.w700),
                ),
                const SizedBox(height: 8),
                const Text(
                  '这些信息保存在本机；token 不落盘。修改后重新连接即可使用新服务器。',
                  style: TextStyle(color: muted, fontSize: 13),
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
                const SizedBox(height: 22),
                FilledButton(
                  onPressed: _save,
                  child: const Text('保存'),
                ),
              ],
            ),
    );
  }
}
