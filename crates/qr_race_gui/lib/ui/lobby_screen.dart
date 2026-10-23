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
    final choice = await showRoomPicker(
      context,
      mode: RoomPickerMode.create,
      onSubmit: (choice) => widget.state.createAndLight(choice.address),
    );
    if (choice == null || !mounted) return;
    // The dialog stays up until create + join + WS subscribe have succeeded,
    // then opens the room directly; no extra success toast.
    await Navigator.of(context).push(
      MaterialPageRoute(
        builder: (_) =>
            RoomScreen(state: widget.state, address: choice.address),
      ),
    );
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
                widget.state.session?.userId ?? '',
                style: const TextStyle(fontSize: 12, color: muted),
              ),
            ],
          ),
        ),
        actions: [
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
      floatingActionButton: Material(
        color: surfaceRaised,
        elevation: 8,
        shadowColor: Colors.black54,
        shape: RoundedRectangleBorder(
          borderRadius: BorderRadius.circular(28),
          side: const BorderSide(color: line),
        ),
        child: Padding(
          padding: const EdgeInsets.all(6),
          child: Row(
            mainAxisSize: MainAxisSize.min,
            children: [
              TextButton.icon(
                onPressed: _enterByPin,
                style: TextButton.styleFrom(
                  foregroundColor: accent,
                  backgroundColor: accent.withValues(alpha: 0.14),
                  minimumSize: const Size(0, 44),
                  padding: const EdgeInsets.symmetric(horizontal: 14),
                  shape: const StadiumBorder(),
                ),
                icon: const Icon(Icons.pin_outlined, size: 18),
                label: const Text('配对码'),
              ),
              const SizedBox(width: 4),
              FilledButton.icon(
                onPressed: _createRoom,
                style: FilledButton.styleFrom(
                  minimumSize: const Size(0, 44),
                  padding: const EdgeInsets.symmetric(horizontal: 16),
                  shape: const StadiumBorder(),
                ),
                icon: const Icon(Icons.add, size: 18),
                label: const Text('创建房间'),
              ),
            ],
          ),
        ),
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
                  body: '房主点亮房间后会出现在这里；也可以直接用右下角创建房间。',
                ),
              ),
            );
          }

          final entries = <_LobbyEntry>[
            for (final room in state.lobby)
              if (_LobbyAddress.tryParse(room.label) case final address?)
                _LobbyEntry(room, address),
          ];
          final capitals = _sortedValues(
            entries.map((entry) => entry.address.capital),
          );
          final pinOnly = state.lobbyFilterCapital == '南极';
          final districts = pinOnly
              ? const <String>[]
              : _sortedValues(
                  entries
                      .where(
                        (entry) =>
                            state.lobbyFilterCapital == null ||
                            entry.address.capital == state.lobbyFilterCapital,
                      )
                      .map((entry) => entry.address.district),
                );
          final levels = pinOnly
              ? const <String>[]
              : _sortedValues(
                  entries
                      .where(
                        (entry) =>
                            (state.lobbyFilterCapital == null ||
                                entry.address.capital ==
                                    state.lobbyFilterCapital) &&
                            (state.lobbyFilterDistrict == null ||
                                entry.address.district ==
                                    state.lobbyFilterDistrict),
                      )
                      .map((entry) => entry.address.level),
                );
          final visible = entries
              .where(
                (entry) =>
                    (state.lobbyFilterCapital == null ||
                        entry.address.capital == state.lobbyFilterCapital) &&
                    (state.lobbyFilterDistrict == null ||
                        entry.address.district == state.lobbyFilterDistrict) &&
                    (state.lobbyFilterLevel == null ||
                        entry.address.level == state.lobbyFilterLevel),
              )
              .map((entry) => entry.room)
              .toList(growable: false);
          final hasFilter =
              state.lobbyFilterCapital != null ||
              state.lobbyFilterDistrict != null ||
              state.lobbyFilterLevel != null;

          return Column(
            children: [
              if (entries.isNotEmpty)
                _LobbyFilterBar(
                  state: state,
                  capitals: capitals,
                  districts: districts,
                  levels: levels,
                  visible: visible.length,
                  active: hasFilter,
                  hideHierarchy: pinOnly,
                ),
              Expanded(
                child: RefreshIndicator(
                  onRefresh: state.refreshLobby,
                  child: ListView(
                    physics: const AlwaysScrollableScrollPhysics(),
                    padding: const EdgeInsets.fromLTRB(16, 12, 16, 124),
                    children: [
                      if (visible.isEmpty)
                        Padding(
                          padding: const EdgeInsets.symmetric(vertical: 28),
                          child: Column(
                            children: [
                              const Icon(
                                Icons.filter_alt_off_outlined,
                                color: muted,
                                size: 32,
                              ),
                              const SizedBox(height: 10),
                              const Text(
                                '没有符合筛选的房间',
                                style: TextStyle(fontWeight: FontWeight.w600),
                              ),
                              const SizedBox(height: 4),
                              const Text(
                                '换一组筛选条件，或清除筛选。',
                                style: TextStyle(color: muted, fontSize: 12.5),
                              ),
                              if (hasFilter)
                                TextButton(
                                  onPressed: state.clearLobbyFilters,
                                  child: const Text('清除筛选'),
                                ),
                            ],
                          ),
                        )
                      else
                        for (final room in visible)
                          Padding(
                            padding: const EdgeInsets.only(bottom: 10),
                            child: _RoomCard(
                              room: room,
                              badge: room.label.startsWith('南极') ? 'PIN' : null,
                              onTap: () => _enter(room.label),
                            ),
                          ),
                    ],
                  ),
                ),
              ),
            ],
          );
        },
      ),
    );
  }
}

class _LobbyEntry {
  const _LobbyEntry(this.room, this.address);

  final LobbyRoom room;
  final _LobbyAddress address;
}

class _LobbyAddress {
  const _LobbyAddress({
    required this.capital,
    required this.district,
    required this.level,
  });

  final String capital;
  final String district;
  final String level;

  static _LobbyAddress? tryParse(String label) {
    if (label.startsWith('南极')) {
      final pin = label.substring(2);
      if (pin.length == 4 && int.tryParse(pin) != null) {
        return const _LobbyAddress(capital: '南极', district: '', level: '');
      }
      return null;
    }
    final match = RegExp(r'^(东京|西京)(.+?)区(.+?)级(.+?)房$').firstMatch(label);
    if (match == null) return null;
    return _LobbyAddress(
      capital: match.group(1)!,
      district: match.group(2)!,
      level: match.group(3)!,
    );
  }
}

List<String> _sortedValues(Iterable<String> values) {
  final result = values.where((value) => value.isNotEmpty).toSet().toList()
    ..sort();
  return result;
}

class _LobbyFilterBar extends StatelessWidget {
  const _LobbyFilterBar({
    required this.state,
    required this.capitals,
    required this.districts,
    required this.levels,
    required this.visible,
    required this.active,
    this.hideHierarchy = false,
  });

  final AppState state;
  final List<String> capitals;
  final List<String> districts;
  final List<String> levels;
  final int visible;
  final bool active;
  final bool hideHierarchy;

  @override
  Widget build(BuildContext context) {
    return Container(
      width: double.infinity,
      padding: const EdgeInsets.fromLTRB(16, 10, 16, 10),
      decoration: const BoxDecoration(
        color: surface,
        border: Border(bottom: BorderSide(color: line)),
      ),
      child: Wrap(
        spacing: 8,
        runSpacing: 8,
        crossAxisAlignment: WrapCrossAlignment.center,
        children: [
          _FilterMenu(
            label: '地区',
            value: state.lobbyFilterCapital,
            options: capitals,
            onSelected: state.setLobbyCapitalFilter,
          ),
          if (!hideHierarchy) ...[
            _FilterMenu(
              label: '区',
              value: state.lobbyFilterDistrict,
              options: districts,
              onSelected: state.setLobbyDistrictFilter,
            ),
            _FilterMenu(
              label: '级',
              value: state.lobbyFilterLevel,
              options: levels,
              onSelected: state.setLobbyLevelFilter,
            ),
          ],
          if (active)
            TextButton(
              onPressed: state.clearLobbyFilters,
              style: TextButton.styleFrom(
                foregroundColor: muted,
                minimumSize: const Size(0, 34),
                padding: const EdgeInsets.symmetric(horizontal: 8),
              ),
              child: const Text('清除'),
            ),
          Text(
            '$visible 间',
            style: const TextStyle(color: muted, fontSize: 12),
          ),
        ],
      ),
    );
  }
}

class _FilterMenu extends StatelessWidget {
  const _FilterMenu({
    required this.label,
    required this.value,
    required this.options,
    required this.onSelected,
  });

  final String label;
  final String? value;
  final List<String> options;
  final ValueChanged<String?> onSelected;

  @override
  Widget build(BuildContext context) {
    final selected = value != null;
    return PopupMenuButton<String>(
      tooltip: '筛选$label',
      onSelected: (choice) => onSelected(choice == '全部' ? null : choice),
      itemBuilder: (context) => [
        const PopupMenuItem(value: '全部', child: Text('全部')),
        for (final option in options)
          PopupMenuItem(value: option, child: Text(option)),
      ],
      child: Container(
        padding: const EdgeInsets.symmetric(horizontal: 11, vertical: 7),
        decoration: BoxDecoration(
          color: surfaceRaised,
          borderRadius: BorderRadius.circular(18),
          border: Border.all(color: selected ? accent : line),
        ),
        child: Row(
          mainAxisSize: MainAxisSize.min,
          children: [
            Text('$label ', style: const TextStyle(color: muted, fontSize: 11)),
            Text(
              value ?? '全部',
              style: TextStyle(
                color: selected ? accent : ink,
                fontSize: 12,
                fontWeight: FontWeight.w600,
              ),
            ),
            const SizedBox(width: 3),
            const Icon(Icons.expand_more, size: 15, color: muted),
          ],
        ),
      ),
    );
  }
}

class _RoomCard extends StatelessWidget {
  const _RoomCard({required this.room, required this.onTap, this.badge});

  final LobbyRoom room;
  final VoidCallback onTap;
  final String? badge;

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
                    Row(
                      children: [
                        Flexible(
                          child: Text(
                            room.label,
                            style: const TextStyle(
                              fontSize: 17,
                              fontWeight: FontWeight.w600,
                            ),
                          ),
                        ),
                        if (badge != null) ...[
                          const SizedBox(width: 8),
                          Container(
                            padding: const EdgeInsets.symmetric(
                              horizontal: 6,
                              vertical: 2,
                            ),
                            decoration: BoxDecoration(
                              color: accent.withValues(alpha: 0.14),
                              borderRadius: BorderRadius.circular(5),
                            ),
                            child: Text(
                              badge!,
                              style: const TextStyle(
                                color: accent,
                                fontSize: 10,
                                fontWeight: FontWeight.w700,
                                letterSpacing: 0.5,
                              ),
                            ),
                          ),
                        ],
                      ],
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
