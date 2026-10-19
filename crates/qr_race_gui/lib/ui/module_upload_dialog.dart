import 'package:flutter/material.dart';

import 'theme.dart';

class ModuleDraft {
  const ModuleDraft({
    required this.id,
    required this.version,
    required this.entry,
    required this.hooks,
    required this.capabilities,
    required this.source,
  });

  final String id;
  final String version;
  final String entry;
  final List<String> hooks;
  final List<String> capabilities;
  final String source;
}

/// Admin upload form. The dialog owns validation; the caller owns the request.
class ModuleUploadDialog extends StatefulWidget {
  const ModuleUploadDialog({super.key, required this.onSubmit});

  final Future<void> Function(ModuleDraft draft) onSubmit;

  @override
  State<ModuleUploadDialog> createState() => _ModuleUploadDialogState();
}

class _ModuleUploadDialogState extends State<ModuleUploadDialog> {
  final _formKey = GlobalKey<FormState>();
  final _id = TextEditingController();
  final _version = TextEditingController(text: '1.0.0');
  final _entry = TextEditingController(text: 'main.py');
  final _source = TextEditingController();
  final Set<String> _hooks = {'frame'};
  final Set<String> _capabilities = {'ui'};
  String? _error;
  bool _busy = false;

  @override
  void dispose() {
    _id.dispose();
    _version.dispose();
    _entry.dispose();
    _source.dispose();
    super.dispose();
  }

  Future<void> _submit() async {
    if (!_formKey.currentState!.validate()) return;
    if (_hooks.isEmpty) {
      setState(() => _error = '至少选择一个运行钩子');
      return;
    }
    setState(() {
      _busy = true;
      _error = null;
    });
    try {
      await widget.onSubmit(
        ModuleDraft(
          id: _id.text.trim(),
          version: _version.text.trim(),
          entry: _entry.text.trim(),
          hooks: _hooks.toList()..sort(),
          capabilities: _capabilities.toList()..sort(),
          source: _source.text,
        ),
      );
      if (mounted) Navigator.of(context).pop(true);
    } catch (error) {
      if (mounted) {
        setState(() {
          _error = error is StateError ? error.message : error.toString();
          _busy = false;
        });
      }
    }
  }

  @override
  Widget build(BuildContext context) {
    return Dialog(
      backgroundColor: surfaceRaised,
      child: ConstrainedBox(
        constraints: const BoxConstraints(maxWidth: 520),
        child: SingleChildScrollView(
          padding: const EdgeInsets.fromLTRB(22, 20, 22, 18),
          child: Form(
            key: _formKey,
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.stretch,
              mainAxisSize: MainAxisSize.min,
              children: [
                const Text(
                  '上传模组',
                  style: TextStyle(fontSize: 18, fontWeight: FontWeight.w700),
                ),
                const SizedBox(height: 4),
                const Text(
                  '仅管理员可上传。上传后房间可立即选用。',
                  style: TextStyle(color: muted, fontSize: 12.5),
                ),
                const SizedBox(height: 16),
                TextFormField(
                  controller: _id,
                  decoration: const InputDecoration(labelText: '模组 id'),
                  autocorrect: false,
                  validator: (value) =>
                      (value == null || value.trim().isEmpty) ? '请输入模组 id' : null,
                ),
                const SizedBox(height: 10),
                Row(
                  children: [
                    Expanded(
                      child: TextFormField(
                        controller: _version,
                        decoration: const InputDecoration(labelText: '版本'),
                        validator: (value) => (value == null || value.trim().isEmpty)
                            ? '请输入版本'
                            : null,
                      ),
                    ),
                    const SizedBox(width: 10),
                    Expanded(
                      child: TextFormField(
                        controller: _entry,
                        decoration: const InputDecoration(labelText: '入口文件'),
                        validator: (value) =>
                            (value == null || value.trim().isEmpty) ? '请输入入口文件' : null,
                      ),
                    ),
                  ],
                ),
                const SizedBox(height: 14),
                const Text('运行钩子', style: TextStyle(color: muted, fontSize: 12)),
                Wrap(
                  spacing: 8,
                  children: [
                    for (final hook in ['frame', 'action'])
                      FilterChip(
                        label: Text(hook),
                        selected: _hooks.contains(hook),
                        onSelected: (selected) => setState(() {
                          if (selected) {
                            _hooks.add(hook);
                          } else {
                            _hooks.remove(hook);
                          }
                        }),
                      ),
                  ],
                ),
                const SizedBox(height: 8),
                const Text('能力', style: TextStyle(color: muted, fontSize: 12)),
                Wrap(
                  spacing: 8,
                  children: [
                    for (final capability in ['camera', 'network', 'storage', 'ui'])
                      FilterChip(
                        label: Text(capability),
                        selected: _capabilities.contains(capability),
                        onSelected: (selected) => setState(() {
                          if (selected) {
                            _capabilities.add(capability);
                          } else {
                            _capabilities.remove(capability);
                          }
                        }),
                      ),
                  ],
                ),
                const SizedBox(height: 12),
                TextFormField(
                  controller: _source,
                  decoration: const InputDecoration(
                    labelText: '源码',
                    hintText: 'def on_frame(payload):\n    return payload',
                    alignLabelWithHint: true,
                  ),
                  minLines: 6,
                  maxLines: 10,
                  style: const TextStyle(fontFamily: 'monospace', fontSize: 12.5),
                  validator: (value) => (value == null || value.trim().isEmpty)
                      ? '请输入模组源码'
                      : null,
                ),
                if (_error != null) ...[
                  const SizedBox(height: 10),
                  Text(_error!, style: const TextStyle(color: danger, fontSize: 12.5)),
                ],
                const SizedBox(height: 14),
                Row(
                  mainAxisAlignment: MainAxisAlignment.end,
                  children: [
                    TextButton(
                      onPressed: _busy ? null : () => Navigator.of(context).pop(),
                      child: const Text('取消'),
                    ),
                    const SizedBox(width: 8),
                    SizedBox(
                      width: 120,
                      child: FilledButton(
                        onPressed: _busy ? null : _submit,
                        child: _busy
                            ? const SizedBox(
                                width: 18,
                                height: 18,
                                child: CircularProgressIndicator(
                                  strokeWidth: 2,
                                  color: accentInk,
                                ),
                              )
                            : const Text('上传'),
                      ),
                    ),
                  ],
                ),
              ],
            ),
          ),
        ),
      ),
    );
  }
}
