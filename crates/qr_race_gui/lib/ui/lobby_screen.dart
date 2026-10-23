import 'dart:async';

import 'package:flutter/material.dart';

import '../api/api_client.dart';
import '../rooms/room_picker.dart';
import 'pin_dialog.dart';
import '../state/app_state.dart';
import 'connect_screen.dart';
import 'room_screen.dart';
import 'settings_screen.dart';
import 'echo_signal.dart';
import 'theme.dart';

class LobbyScreen extends StatefulWidget {
  const LobbyScreen({super.key, required this.state});

  final AppState state;

  @override
  State<LobbyScreen> createState() => _LobbyScreenState();
}

class _LobbyScreenState extends State<LobbyScreen> {
  Timer? _refresh;

  @override
  void initState() {
    super.initState();
    widget.state.refreshLobby();
    _refresh = Timer.periodic(const Duration(seconds: 5), (_) {
      if (mounted && !widget.state.busy) {
        widget.state.refreshLobby();
      }
    });
  }

  @override
  void dispose() {
    _refresh?.cancel();
    super.dispose();
  }

  Future<void> _disconnect() async {
    await widget.state.disconnect();
    if (!mounted) return;
    Navigator.of(context).pushAndRemoveUntil(
      MaterialPageRoute(builder: (_) => ConnectScreen(state: widget.state)),
      (_) => false,
    );
  }

  Future<void> _enter(String address) async {
    final state = widget.state;
    final messenger = ScaffoldMessenger.of(context);
    messenger.hideCurrentSnackBar();
    messenger.showSnackBar(
      const SnackBar(content: Text('正在进入房间…'), duration: Duration(minutes: 1)),
    );
    final ok = await state.enterRoom(address);
    if (!mounted) return;
    messenger.hideCurrentSnackBar();
    if (!ok) {
      messenger.showSnackBar(
        SnackBar(content: Text(state.error ?? '房间不存在或无法进入')),
      );
      return;
    }
    await Navigator.of(context).push(
      MaterialPageRoute(
        builder: (_) => RoomScreen(state: state, address: address),
      ),
    );
  }

  Future<void> _enterByAddress() async {
    final rooms = widget.state.lobby;
    if (rooms.isEmpty) {
      ScaffoldMessenger.of(
        context,
      ).showSnackBar(const SnackBar(content: Text('大厅还没有已点亮的房间，请先用右上角 + 创建。')));
      return;
    }
    final address = await showDialog<String>(
      context: context,
      builder: (dialogContext) => AlertDialog(
        title: const Text('进入已有房间'),
        content: SizedBox(
          width: 420,
          child: ListView.separated(
            shrinkWrap: true,
            itemCount: rooms.length,
            separatorBuilder: (_, _) => const SizedBox(height: 8),
            itemBuilder: (context, index) {
              final room = rooms[index];
              return ListTile(
                shape: RoundedRectangleBorder(
                  borderRadius: BorderRadius.circular(10),
                  side: const BorderSide(color: line),
                ),
                title: Text(room.label),
                subtitle: Text('房主 ${room.host} · ${room.memberCount} 人'),
                trailing: const Icon(Icons.chevron_right),
                onTap: () => Navigator.pop(dialogContext, room.label),
              );
            },
          ),
        ),
        actions: [
          TextButton(
            onPressed: () => Navigator.pop(dialogContext),
            child: const Text('取消'),
          ),
        ],
      ),
    );
    if (address != null) await _enter(address);
  }

  Future<void> _enterByPin() async {
    final pin = await showPinDialog(context, title: '输入 4 位配对码');
    if (pin == null || !mounted) return;
    final address = await widget.state.startByPin(pin);
    if (address == null) {
      if (!mounted) return;
      ScaffoldMessenger.of(
        context,
      ).showSnackBar(SnackBar(content: Text(widget.state.error ?? '配对码开房失败')));
      return;
    }
    await _enter(address);
  }

  Future<void> _createRoom() async {
    final choice = await showRoomPicker(context, mode: RoomPickerMode.create);
    if (choice == null || !mounted) return;

    final messenger = ScaffoldMessenger.of(context);
    messenger.hideCurrentSnackBar();
    messenger.showSnackBar(
      const SnackBar(
        content: Text('正在创建并点亮房间…'),
        duration: Duration(minutes: 1),
      ),
    );
    final error = await widget.state.createAndLight(choice.address);
    if (!mounted) return;
    messenger.hideCurrentSnackBar();
    if (error != null) {
      messenger.showSnackBar(SnackBar(content: Text(error)));
      return;
    }
    await _enter(choice.address);
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
              Row(
                children: [
                  Image.asset(
                    'assets/brand/qr_race-mark.png',
                    width: 24,
                    height: 24,
                  ),
                  const SizedBox(width: 8),
                  const Text(
                    '大厅',
                    style: TextStyle(fontSize: 20, fontWeight: FontWeight.w700),
                  ),
                ],
              ),
              Text(
                '${widget.state.session?.userId ?? ''} · ${widget.state.api?.baseUrl ?? ''}',
                style: const TextStyle(fontSize: 12, color: muted),
              ),
            ],
          ),
        ),
        actions: [
          IconButton(
            tooltip: '创建房间',
            onPressed: _createRoom,
            icon: const Icon(Icons.add),
          ),
          IconButton(
            tooltip: 'PIN 开房/进房',
            onPressed: _enterByPin,
            icon: const Icon(Icons.pin_outlined),
          ),
          IconButton(
            tooltip: '设置',
            onPressed: () => Navigator.of(context).push(
              MaterialPageRoute(
                builder: (_) => SettingsScreen(state: widget.state),
              ),
            ),
            icon: const Icon(Icons.settings_outlined),
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
        label: const Text('快速选择房间'),
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
              return const Center(
                child: EchoSignal(size: 40, mode: EchoSignalMode.loading),
              );
            }
            return RefreshIndicator(
              onRefresh: state.refreshLobby,
              child: const _ScrollableFill(
                child: _CenteredNote(
                  icon: Icons.meeting_room_outlined,
                  title: '大厅还没有点亮的房间',
                  body: '房主点亮房间后会出现在这里；也可以直接输入房间地址进入。',
                ),
              ),
            );
          }
          return RefreshIndicator(
            onRefresh: state.refreshLobby,
            child: ListView.separated(
              physics: const AlwaysScrollableScrollPhysics(),
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
                decoration: const BoxDecoration(
                  color: accent,
                  shape: BoxShape.circle,
                ),
              ),
              const SizedBox(width: 12),
              Expanded(
                child: Column(
                  crossAxisAlignment: CrossAxisAlignment.start,
                  children: [
                    Text(
                      room.label,
                      style: const TextStyle(
                        fontSize: 17,
                        fontWeight: FontWeight.w600,
                      ),
                    ),
                    const SizedBox(height: 4),
                    Text(
                      '房主 ${room.host} · ${room.memberCount} 人',
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

class _ScrollableFill extends StatelessWidget {
  const _ScrollableFill({required this.child});

  final Widget child;

  @override
  Widget build(BuildContext context) {
    return LayoutBuilder(
      builder: (context, constraints) => ListView(
        physics: const AlwaysScrollableScrollPhysics(),
        children: [SizedBox(height: constraints.maxHeight, child: child)],
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
                style: const TextStyle(
                  fontSize: 18,
                  fontWeight: FontWeight.w600,
                ),
              ),
              const SizedBox(height: 8),
              Text(
                body,
                textAlign: TextAlign.center,
                style: const TextStyle(color: muted, height: 1.5),
              ),
              if (action != null) ...[const SizedBox(height: 20), action!],
            ],
          ),
        ),
      ),
    );
  }
}
