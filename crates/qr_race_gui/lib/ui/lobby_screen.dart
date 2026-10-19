import 'package:flutter/material.dart';

import '../api/api_client.dart';
import '../state/app_state.dart';
import 'connect_screen.dart';
import 'module_upload_dialog.dart';
import 'room_screen.dart';
import 'theme.dart';

class LobbyScreen extends StatefulWidget {
  const LobbyScreen({super.key, required this.state});

  final AppState state;

  @override
  State<LobbyScreen> createState() => _LobbyScreenState();
}

class _LobbyScreenState extends State<LobbyScreen> {
  @override
  void initState() {
    super.initState();
    widget.state.refreshLobby();
  }

  Future<void> _disconnect() async {
    await widget.state.disconnect();
    if (!mounted) return;
    Navigator.of(context).pushAndRemoveUntil(
      MaterialPageRoute(builder: (_) => ConnectScreen(state: widget.state)),
      (_) => false,
    );
  }

  Future<void> _uploadModule() async {
    final api = widget.state.api;
    if (api == null) return;
    final uploaded = await showDialog<bool>(
      context: context,
      builder: (_) => ModuleUploadDialog(
        onSubmit: (draft) async {
          await api.uploadModule(
            id: draft.id,
            version: draft.version,
            entry: draft.entry,
            hooks: draft.hooks,
            capabilities: draft.capabilities,
            source: draft.source,
          );
        },
      ),
    );
    if (!mounted || uploaded != true) return;
    await widget.state.refreshModuleCatalog();
    if (!mounted) return;
    ScaffoldMessenger.of(context).showSnackBar(
      const SnackBar(content: Text('模组已上传')),
    );
  }

  Future<void> _enter(String address) async {
    final state = widget.state;
    final ok = await state.enterRoom(address);
    if (!mounted || !ok) return;
    await Navigator.of(context).push(
      MaterialPageRoute(
        builder: (_) => RoomScreen(state: state, address: address),
      ),
    );
  }

  Future<String?> _promptAddress({
    required String title,
    required String action,
  }) {
    return showDialog<String>(
      context: context,
      builder: (context) => _AddressDialog(title: title, action: action),
    );
  }

  Future<void> _enterByAddress() async {
    final address = await _promptAddress(title: '输入房间地址', action: '进入');
    final trimmed = address?.trim() ?? '';
    if (trimmed.isNotEmpty) {
      await _enter(trimmed);
    }
  }

  Future<void> _createRoom() async {
    final address = await _promptAddress(title: '创建房间', action: '创建并点亮');
    final trimmed = address?.trim() ?? '';
    if (trimmed.isEmpty || !mounted) return;

    final error = await widget.state.createAndLight(trimmed);
    if (!mounted) return;
    if (error != null) {
      ScaffoldMessenger.of(context).showSnackBar(SnackBar(content: Text(error)));
      return;
    }
    await _enter(trimmed);
  }

  @override
  Widget build(BuildContext context) {
    return Scaffold(
      appBar: AppBar(
        title: ListenableBuilder(
          listenable: widget.state,
          builder: (context, _) => Column(
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              const Text('大厅', style: TextStyle(fontSize: 20, fontWeight: FontWeight.w700)),
              Text(
                '${widget.state.session?.userId ?? ''} · ${widget.state.api?.baseUrl ?? ''}',
                style: const TextStyle(fontSize: 12, color: muted),
              ),
            ],
          ),
        ),
        actions: [
          ListenableBuilder(
            listenable: widget.state,
            builder: (context, _) {
              if (!(widget.state.session?.admin ?? false)) {
                return const SizedBox.shrink();
              }
              return IconButton(
                tooltip: '上传模组',
                onPressed: _uploadModule,
                icon: const Icon(Icons.upload_file_outlined),
              );
            },
          ),
          IconButton(
            tooltip: '创建房间',
            onPressed: _createRoom,
            icon: const Icon(Icons.add),
          ),
          IconButton(
            tooltip: '刷新',
            onPressed: widget.state.refreshLobby,
            icon: const Icon(Icons.refresh),
          ),
          IconButton(
            tooltip: '断开连接',
            onPressed: _disconnect,
            icon: const Icon(Icons.logout),
          ),
        ],
      ),
      floatingActionButton: FloatingActionButton.extended(
        onPressed: _enterByAddress,
        backgroundColor: accent,
        foregroundColor: accentInk,
        icon: const Icon(Icons.keyboard_alt_outlined),
        label: const Text('输入房间地址'),
      ),
      body: ListenableBuilder(
        listenable: widget.state,
        builder: (context, _) {
          final state = widget.state;
          if (state.error != null && state.lobby.isEmpty) {
            return _CenteredNote(
              icon: Icons.cloud_off,
              title: '大厅暂时打不开',
              body: state.error!,
              action: FilledButton(
                onPressed: state.refreshLobby,
                child: const Text('重试'),
              ),
            );
          }
          if (state.lobby.isEmpty) {
            if (state.busy) {
              return const Center(child: CircularProgressIndicator());
            }
            return const _CenteredNote(
              icon: Icons.meeting_room_outlined,
              title: '大厅还没有点亮的房间',
              body: '房主点亮房间后会出现在这里；也可以直接输入房间地址进入。',
            );
          }
          return RefreshIndicator(
            onRefresh: state.refreshLobby,
            child: ListView.separated(
              padding: const EdgeInsets.fromLTRB(16, 12, 16, 96),
              itemCount: state.lobby.length,
              separatorBuilder: (_, _) => const SizedBox(height: 10),
              itemBuilder: (context, index) {
                final room = state.lobby[index];
                return _RoomCard(room: room, onTap: () => _enter(room.label));
              },
            ),
          );
        },
      ),
    );
  }
}

class _RoomCard extends StatelessWidget {
  const _RoomCard({required this.room, required this.onTap});

  final LobbyRoom room;
  final VoidCallback onTap;

  @override
  Widget build(BuildContext context) {
    return Material(
      color: surface,
      borderRadius: BorderRadius.circular(14),
      child: InkWell(
        onTap: onTap,
        borderRadius: BorderRadius.circular(14),
        child: Container(
          padding: const EdgeInsets.symmetric(horizontal: 16, vertical: 14),
          decoration: BoxDecoration(
            border: Border.all(color: line),
            borderRadius: BorderRadius.circular(14),
          ),
          child: Row(
            children: [
              Container(
                width: 8,
                height: 8,
                decoration: const BoxDecoration(color: accent, shape: BoxShape.circle),
              ),
              const SizedBox(width: 12),
              Expanded(
                child: Column(
                  crossAxisAlignment: CrossAxisAlignment.start,
                  children: [
                    Text(
                      room.label,
                      style: const TextStyle(fontSize: 17, fontWeight: FontWeight.w600),
                    ),
                    const SizedBox(height: 4),
                    Text(
                      '房主 ${room.host} · ${room.memberCount} 人'
                      '${room.moduleCount > 0 ? ' · ${room.moduleCount} 个模组' : ''}',
                      style: const TextStyle(fontSize: 12.5, color: muted),
                    ),
                  ],
                ),
              ),
              const Icon(Icons.chevron_right, color: muted),
            ],
          ),
        ),
      ),
    );
  }
}

class _CenteredNote extends StatelessWidget {
  const _CenteredNote({
    required this.icon,
    required this.title,
    required this.body,
    this.action,
  });

  final IconData icon;
  final String title;
  final String body;
  final Widget? action;

  @override
  Widget build(BuildContext context) {
    return Center(
      child: ConstrainedBox(
        constraints: const BoxConstraints(maxWidth: 380),
        child: Padding(
          padding: const EdgeInsets.all(28),
          child: Column(
            mainAxisSize: MainAxisSize.min,
            children: [
              Icon(icon, size: 40, color: muted),
              const SizedBox(height: 16),
              Text(
                title,
                textAlign: TextAlign.center,
                style: const TextStyle(fontSize: 18, fontWeight: FontWeight.w600),
              ),
              const SizedBox(height: 8),
              Text(
                body,
                textAlign: TextAlign.center,
                style: const TextStyle(color: muted, height: 1.5),
              ),
              if (action != null) ...[
                const SizedBox(height: 20),
                action!,
              ],
            ],
          ),
        ),
      ),
    );
  }
}

class _AddressDialog extends StatefulWidget {
  const _AddressDialog({required this.title, required this.action});

  final String title;
  final String action;

  @override
  State<_AddressDialog> createState() => _AddressDialogState();
}

class _AddressDialogState extends State<_AddressDialog> {
  final _controller = TextEditingController();

  @override
  void dispose() {
    _controller.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    return AlertDialog(
      backgroundColor: surfaceRaised,
      title: Text(widget.title),
      content: TextField(
        controller: _controller,
        autofocus: true,
        decoration: const InputDecoration(hintText: '东京A区1B级103房'),
        onSubmitted: (value) => Navigator.of(context).pop(value),
      ),
      actions: [
        TextButton(
          onPressed: () => Navigator.of(context).pop(),
          child: const Text('取消'),
        ),
        FilledButton(
          onPressed: () => Navigator.of(context).pop(_controller.text),
          child: Text(widget.action),
        ),
      ],
    );
  }
}
