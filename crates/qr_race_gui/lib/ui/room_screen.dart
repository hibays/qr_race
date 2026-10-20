import 'package:flutter/material.dart';
import 'package:flutter/services.dart';

import '../api/api_client.dart';
import '../api/connect_link.dart';
import '../state/app_state.dart';
import '../state/relay_slot.dart';
import 'open_link_sheet.dart';
import 'qr_image.dart';
import 'scan_screen.dart';
import 'theme.dart';

class RoomScreen extends StatefulWidget {
  const RoomScreen({super.key, required this.state, required this.address});

  final AppState state;
  final String address;

  @override
  State<RoomScreen> createState() => _RoomScreenState();
}

class _RoomScreenState extends State<RoomScreen> {
  @override
  void initState() {
    super.initState();
    widget.state.enterRoom(widget.address);
  }

  @override
  void dispose() {
    widget.state.exitRoom();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    return ListenableBuilder(
      listenable: widget.state,
      builder: (context, _) {
        final state = widget.state;
        final room = state.room;
        final me = _me(room, state.session?.userId);
        final canLeave = me?.role != 'host';

        return Scaffold(
          appBar: AppBar(
            title: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                Text(
                  room?.label ?? widget.address,
                  style: const TextStyle(fontSize: 18, fontWeight: FontWeight.w700),
                ),
                Text(
                  room == null
                      ? '进入中…'
                      : '${room.members.length} 人 · ${room.relays.map((relay) => relaySlot(relay.text)).toSet().length} 个动态码',
                  style: const TextStyle(fontSize: 12, color: muted),
                ),
              ],
            ),
            actions: [
              if (room != null && me?.role == 'host')
                IconButton(
                  tooltip: '配置模组',
                  onPressed: () => _configureModules(room),
                  icon: const Icon(Icons.extension_outlined),
                ),
              if (room != null && (me?.canPublish ?? false))
                IconButton(
                  tooltip: '扫码发布',
                  onPressed: () => _openScanner(room.label),
                  icon: const Icon(Icons.qr_code_scanner),
                ),
              if (room != null && me?.role == 'host')
                IconButton(
                  tooltip: '邀请成员',
                  onPressed: () => _invite(room),
                  icon: const Icon(Icons.person_add_alt_1_outlined),
                ),
              IconButton(
                tooltip: '刷新',
                onPressed: state.refreshRoom,
                icon: const Icon(Icons.refresh),
              ),
              if (canLeave)
                IconButton(
                  tooltip: '离开房间',
                  onPressed: () => Navigator.of(context).pop(),
                  icon: const Icon(Icons.exit_to_app),
                ),
            ],
          ),
          body: room == null
              ? _loading(state)
              : Column(
                  children: [
                    if (state.error != null) _ErrorBar(message: state.error!),
                    if (!room.lit && me?.role == 'host')
                      _UnlitBanner(onLight: state.lightRoom),
                    if (room.modules.isNotEmpty || me?.role == 'host')
                      _ModuleStrip(
                        modules: room.modules,
                        canEdit: me?.role == 'host',
                        onEdit: () => _configureModules(room),
                      ),
                    Expanded(child: _RoomBody(state: state, room: room)),
                  ],
                ),
        );
      },
    );
  }

  RoomMember? _me(RoomView? room, String? userId) {
    if (room == null || userId == null) return null;
    for (final member in room.members) {
      if (member.user == userId) return member;
    }
    return null;
  }

  void _openScanner(String address) {
    Navigator.of(context).push(
      MaterialPageRoute(
        builder: (_) => ScanScreen(state: widget.state, address: address),
      ),
    );
  }

  Future<void> _invite(RoomView room) async {
    final api = widget.state.api;
    if (api == null) {
      ScaffoldMessenger.of(context).showSnackBar(
        const SnackBar(content: Text('演示模式没有服务器，不能邀请')),
      );
      return;
    }
    try {
      final guest = await api.createGuest(room.label);
      if (!mounted) return;
      final link = buildConnectLink(
        server: api.baseUrl,
        fingerprint: api.fingerprint,
        token: guest.token,
        room: room.label,
      );
      await showDialog<void>(
        context: context,
        builder: (dialogContext) => AlertDialog(
          title: const Text('邀请成员'),
          content: Column(
            mainAxisSize: MainAxisSize.min,
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              const Text(
                '这是一个只属于本房间的临时成员身份；打开链接会自动登录并进入房间。',
                style: TextStyle(fontSize: 12.5, height: 1.45),
              ),
              const SizedBox(height: 10),
              SelectableText(link, style: const TextStyle(fontSize: 12)),
              const SizedBox(height: 10),
              const Text(
                '链接含该房间的临时 token，等同密码，只发给该成员。',
                style: TextStyle(fontSize: 11.5, color: muted),
              ),
            ],
          ),
          actions: [
            TextButton(
              onPressed: () {
                Clipboard.setData(ClipboardData(text: link));
                Navigator.pop(dialogContext);
                if (!mounted) return;
                ScaffoldMessenger.of(context).showSnackBar(
                  const SnackBar(content: Text('邀请链接已复制')),
                );
              },
              child: const Text('复制链接'),
            ),
            TextButton(
              onPressed: () => Navigator.pop(dialogContext),
              child: const Text('关闭'),
            ),
          ],
        ),
      );
    } on ApiException catch (error) {
      if (!mounted) return;
      ScaffoldMessenger.of(context).showSnackBar(
        SnackBar(content: Text(error.message)),
      );
    }
  }

  Future<void> _configureModules(RoomView room) async {
    await widget.state.refreshModuleCatalog();
    if (!mounted) return;
    final selected = await showDialog<Set<String>>(
      context: context,
      builder: (_) => _ModulePicker(
        catalog: widget.state.moduleCatalog,
        selected: room.modules.toSet(),
      ),
    );
    if (selected == null) return;
    final sorted = selected.toList()..sort();
    await widget.state.setModules(sorted);
  }

  Widget _loading(AppState state) {
    if (state.error != null) {
      return Center(
        child: Padding(
          padding: const EdgeInsets.all(28),
          child: Column(
            mainAxisSize: MainAxisSize.min,
            children: [
              const Icon(Icons.lock_outline, color: muted, size: 40),
              const SizedBox(height: 16),
              Text(
                state.error!,
                textAlign: TextAlign.center,
                style: const TextStyle(color: muted, height: 1.5),
              ),
            ],
          ),
        ),
      );
    }
    return const Center(child: CircularProgressIndicator());
  }
}

class _RoomBody extends StatelessWidget {
  const _RoomBody({required this.state, required this.room});

  final AppState state;
  final RoomView room;

  @override
  Widget build(BuildContext context) {
    return LayoutBuilder(
      builder: (context, constraints) {
        final wide = constraints.maxWidth >= 900;
        // Dynamic codes share an ownership prefix; only the newest relay for a
        // prefix is shown, so a changing code updates in place instead of
        // filling the wall with history.
        final bySlot = <String, Relay>{};
        for (final relay in room.relays) {
          bySlot.putIfAbsent(relaySlot(relay.text), () => relay);
        }
        final relays = bySlot.values.toList();
        final relayArea = relays.isEmpty
            ? const _EmptyRelays()
            : GridView.builder(
                padding: const EdgeInsets.all(16),
                gridDelegate: const SliverGridDelegateWithMaxCrossAxisExtent(
                  maxCrossAxisExtent: 280,
                  mainAxisExtent: 330,
                  crossAxisSpacing: 14,
                  mainAxisSpacing: 14,
                ),
                itemCount: relays.length,
                itemBuilder: (context, index) =>
                    _RelayCard(relay: relays[index], state: state),
              );

        if (wide) {
          return Row(
            children: [
              Expanded(child: relayArea),
              Container(width: 1, color: line),
              SizedBox(
                width: 280,
                child: _MembersPanel(room: room, state: state),
              ),
            ],
          );
        }
        return Column(
          children: [
            Expanded(child: relayArea),
            Container(height: 1, color: line),
            _MemberRail(room: room, state: state),
          ],
        );
      },
    );
  }
}

class _EmptyRelays extends StatelessWidget {
  const _EmptyRelays();

  @override
  Widget build(BuildContext context) {
    return Center(
      child: ConstrainedBox(
        constraints: const BoxConstraints(maxWidth: 360),
        child: Padding(
          padding: const EdgeInsets.all(28),
          child: Column(
            mainAxisSize: MainAxisSize.min,
            children: [
              const Icon(Icons.qr_code_2, size: 48, color: muted),
              const SizedBox(height: 16),
              const Text(
                '还没有接力内容',
                style: TextStyle(fontSize: 18, fontWeight: FontWeight.w600),
              ),
              const SizedBox(height: 8),
              const Text(
                '房主或副房主扫码后，内容会在这里变成可被扫描的二维码。',
                textAlign: TextAlign.center,
                style: TextStyle(color: muted, height: 1.5),
              ),
            ],
          ),
        ),
      ),
    );
  }
}

class _RelayCard extends StatelessWidget {
  const _RelayCard({required this.relay, required this.state});

  final Relay relay;
  final AppState state;

  @override
  Widget build(BuildContext context) {
    return Container(
      decoration: BoxDecoration(
        color: surfaceRaised,
        border: Border.all(color: line),
        borderRadius: BorderRadius.circular(14),
      ),
      padding: const EdgeInsets.all(12),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Expanded(
            child: GestureDetector(
              onTap: () => _showRelay(context),
              child: Container(
                decoration: BoxDecoration(
                  color: Colors.white,
                  borderRadius: BorderRadius.circular(8),
                ),
                padding: const EdgeInsets.all(10),
                child: LayoutBuilder(
                  builder: (context, constraints) {
                    final side = constraints.biggest.shortestSide;
                    return Center(
                      child: QrImage(text: relay.text, size: side),
                    );
                  },
                ),
              ),
            ),
          ),
          const SizedBox(height: 10),
          if (relaySlot(relay.text) != relay.text)
            Container(
              margin: const EdgeInsets.only(bottom: 4),
              padding: const EdgeInsets.symmetric(horizontal: 6, vertical: 2),
              decoration: BoxDecoration(
                color: accent.withValues(alpha: 0.14),
                borderRadius: BorderRadius.circular(4),
              ),
              child: Text(
                relaySlot(relay.text),
                style: const TextStyle(
                  color: accent,
                  fontSize: 10.5,
                  fontWeight: FontWeight.w600,
                ),
              ),
            ),
          Text(
            relay.text,
            maxLines: 2,
            overflow: TextOverflow.ellipsis,
            style: const TextStyle(fontSize: 12.5, height: 1.35),
          ),
          const SizedBox(height: 6),
          Row(
            children: [
              Expanded(
                child: Text(
                  '${relay.publisher} · ${relay.timeLabel}',
                  style: const TextStyle(fontSize: 11.5, color: muted),
                ),
              ),
              IconButton(
                tooltip: '打开链接',
                visualDensity: VisualDensity.compact,
                onPressed: () => showOpenLinkSheet(context, relay.text),
                icon: const Icon(Icons.open_in_new, size: 18),
              ),
            ],
          ),
        ],
      ),
    );
  }

  void _showRelay(BuildContext context) {
    showDialog<void>(
      context: context,
      builder: (context) => Dialog(
        backgroundColor: surfaceRaised,
        child: ConstrainedBox(
          constraints: const BoxConstraints(maxWidth: 420),
          child: Padding(
            padding: const EdgeInsets.all(20),
            child: Column(
              mainAxisSize: MainAxisSize.min,
              crossAxisAlignment: CrossAxisAlignment.stretch,
              children: [
                Container(
                  color: Colors.white,
                  padding: const EdgeInsets.all(16),
                  child: AspectRatio(
                    aspectRatio: 1,
                    child: LayoutBuilder(
                      builder: (context, constraints) => QrImage(
                        text: relay.text,
                        size: constraints.biggest.shortestSide,
                      ),
                    ),
                  ),
                ),
                const SizedBox(height: 14),
                SelectableText(
                  relay.text,
                  style: const TextStyle(fontSize: 13, height: 1.45),
                ),
                const SizedBox(height: 10),
                Text(
                  '${relay.publisher} · ${relay.timeLabel}',
                  style: const TextStyle(fontSize: 12, color: muted),
                ),
                const SizedBox(height: 8),
                Align(
                  alignment: Alignment.centerRight,
                  child: TextButton.icon(
                    onPressed: () => showOpenLinkSheet(context, relay.text),
                    icon: const Icon(Icons.open_in_new, size: 18),
                    label: const Text('打开链接'),
                  ),
                ),
              ],
            ),
          ),
        ),
      ),
    );
  }
}

class _MembersPanel extends StatelessWidget {
  const _MembersPanel({required this.room, required this.state});

  final RoomView room;
  final AppState state;

  @override
  Widget build(BuildContext context) {
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        const Padding(
          padding: EdgeInsets.fromLTRB(16, 18, 16, 10),
          child: Text(
            '房间成员',
            style: TextStyle(fontSize: 12, letterSpacing: 1.2, color: muted),
          ),
        ),
        Expanded(
          child: ListView.separated(
            padding: const EdgeInsets.fromLTRB(12, 0, 12, 16),
            itemCount: room.members.length,
            separatorBuilder: (_, _) => const SizedBox(height: 8),
            itemBuilder: (context, index) => _MemberTile(
              member: room.members[index],
              room: room,
              state: state,
              canManage: _canManage(room.members[index]),
            ),
          ),
        ),
      ],
    );
  }

  bool _canManage(RoomMember member) =>
      state.session?.userId == room.host && member.user != room.host;
}

class _MemberRail extends StatelessWidget {
  const _MemberRail({required this.room, required this.state});

  final RoomView room;
  final AppState state;

  @override
  Widget build(BuildContext context) {
    return SizedBox(
      height: 104,
      child: ListView.separated(
        scrollDirection: Axis.horizontal,
        padding: const EdgeInsets.symmetric(horizontal: 12, vertical: 10),
        itemCount: room.members.length,
        separatorBuilder: (_, _) => const SizedBox(width: 8),
        itemBuilder: (context, index) => SizedBox(
          width: 172,
          child: _MemberTile(
            member: room.members[index],
            room: room,
            state: state,
            canManage: _canManage(room.members[index]),
          ),
        ),
      ),
    );
  }

  bool _canManage(RoomMember member) =>
      state.session?.userId == room.host && member.user != room.host;
}

class _MemberTile extends StatelessWidget {
  const _MemberTile({
    required this.member,
    required this.room,
    required this.state,
    required this.canManage,
  });

  final RoomMember member;
  final RoomView room;
  final AppState state;
  final bool canManage;

  @override
  Widget build(BuildContext context) {
    final active = member.hasRelayed;
    return Material(
      color: active ? accent.withValues(alpha: 0.08) : surface,
      borderRadius: BorderRadius.circular(12),
      child: InkWell(
        onTap: active ? () => _showMemberRelay(context) : null,
        borderRadius: BorderRadius.circular(12),
        child: Container(
          padding: const EdgeInsets.symmetric(horizontal: 12, vertical: 10),
          decoration: BoxDecoration(
            border: Border.all(color: active ? accent : line),
            borderRadius: BorderRadius.circular(12),
          ),
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.start,
            mainAxisSize: MainAxisSize.min,
            children: [
              Row(
                children: [
                  Container(
                    width: 7,
                    height: 7,
                    decoration: BoxDecoration(
                      color: active ? accent : muted,
                      shape: BoxShape.circle,
                    ),
                  ),
                  const SizedBox(width: 8),
                  Expanded(
                    child: Text(
                      member.user,
                      maxLines: 1,
                      overflow: TextOverflow.ellipsis,
                      style: const TextStyle(fontWeight: FontWeight.w600, fontSize: 13.5),
                    ),
                  ),
                  if (canManage)
                    MenuAnchor(
                      menuChildren: [
                        MenuItemButton(
                          onPressed: () => member.role == 'co_host'
                              ? state.demote(member.user)
                              : state.promote(member.user),
                          child: Text(
                            member.role == 'co_host' ? '取消副房主' : '设为副房主',
                          ),
                        ),
                      ],
                      builder: (context, controller, child) => IconButton(
                        tooltip: '管理成员',
                        onPressed: () =>
                            controller.isOpen ? controller.close() : controller.open(),
                        icon: const Icon(Icons.more_horiz, size: 18, color: muted),
                        padding: EdgeInsets.zero,
                        constraints:
                            const BoxConstraints.tightFor(width: 28, height: 28),
                        visualDensity: VisualDensity.compact,
                      ),
                    ),
                ],
              ),
              const SizedBox(height: 6),
              Text(
                '${member.roleLabel}${active ? ' · 已接力' : ''}',
                style: TextStyle(
                  fontSize: 11.5,
                  color: active ? accent : muted,
                ),
              ),
            ],
          ),
        ),
      ),
    );
  }

  void _showMemberRelay(BuildContext context) {
    Relay? relay;
    for (final candidate in room.relays) {
      if (candidate.id == member.lastRelay) relay = candidate;
    }
    if (relay == null) return;
    showDialog<void>(
      context: context,
      builder: (dialogContext) => AlertDialog(
        backgroundColor: surfaceRaised,
        title: Text('${member.user} 的接力'),
        content: SizedBox(
          width: 320,
          child: Column(
            mainAxisSize: MainAxisSize.min,
            children: [
              Container(
                color: Colors.white,
                padding: const EdgeInsets.all(14),
                child: QrImage(text: relay!.text, size: 260),
              ),
              const SizedBox(height: 12),
              SelectableText(
                relay.text,
                style: const TextStyle(fontSize: 13, height: 1.45),
              ),
            ],
          ),
        ),
      ),
    );
  }
}

class _ErrorBar extends StatelessWidget {
  const _ErrorBar({required this.message});

  final String message;

  @override
  Widget build(BuildContext context) {
    return Container(
      width: double.infinity,
      color: danger.withValues(alpha: 0.12),
      padding: const EdgeInsets.symmetric(horizontal: 16, vertical: 10),
      child: Text(
        message,
        style: const TextStyle(color: danger, fontSize: 12.5, height: 1.4),
      ),
    );
  }
}

class _UnlitBanner extends StatelessWidget {
  const _UnlitBanner({required this.onLight});

  final VoidCallback onLight;

  @override
  Widget build(BuildContext context) {
    return Container(
      width: double.infinity,
      color: warn.withValues(alpha: 0.12),
      padding: const EdgeInsets.fromLTRB(16, 8, 12, 8),
      child: Row(
        children: [
          const Icon(Icons.lightbulb_outline, color: warn, size: 18),
          const SizedBox(width: 10),
          const Expanded(
            child: Text(
              '房间尚未点亮，只有你能进入。点亮后才会出现在大厅。',
              style: TextStyle(color: warn, fontSize: 12.5, height: 1.4),
            ),
          ),
          TextButton(
            onPressed: onLight,
            child: const Text('点亮'),
          ),
        ],
      ),
    );
  }
}

class _ModuleStrip extends StatelessWidget {
  const _ModuleStrip({
    required this.modules,
    required this.canEdit,
    required this.onEdit,
  });

  final List<String> modules;
  final bool canEdit;
  final VoidCallback onEdit;

  @override
  Widget build(BuildContext context) {
    return Container(
      width: double.infinity,
      padding: const EdgeInsets.symmetric(horizontal: 16, vertical: 8),
      decoration: const BoxDecoration(
        color: surface,
        border: Border(bottom: BorderSide(color: line)),
      ),
      child: Row(
        children: [
          const Icon(Icons.extension_outlined, size: 16, color: muted),
          const SizedBox(width: 8),
          Expanded(
            child: modules.isEmpty
                ? const Text(
                    '未配置模组',
                    style: TextStyle(color: muted, fontSize: 12.5),
                  )
                : Wrap(
                    spacing: 6,
                    runSpacing: 6,
                    children: [
                      for (final id in modules) _ModuleChip(id: id),
                    ],
                  ),
          ),
          if (canEdit)
            TextButton(
              onPressed: onEdit,
              child: Text(modules.isEmpty ? '配置' : '修改'),
            ),
        ],
      ),
    );
  }
}

class _ModuleChip extends StatelessWidget {
  const _ModuleChip({required this.id});

  final String id;

  @override
  Widget build(BuildContext context) {
    return Container(
      padding: const EdgeInsets.symmetric(horizontal: 8, vertical: 3),
      decoration: BoxDecoration(
        color: surfaceRaised,
        borderRadius: BorderRadius.circular(999),
        border: Border.all(color: line),
      ),
      child: Text(id, style: const TextStyle(fontSize: 12, color: ink)),
    );
  }
}

class _ModulePicker extends StatefulWidget {
  const _ModulePicker({required this.catalog, required this.selected});

  final List<ModuleInfo> catalog;
  final Set<String> selected;

  @override
  State<_ModulePicker> createState() => _ModulePickerState();
}

class _ModulePickerState extends State<_ModulePicker> {
  late final Set<String> _selected = {...widget.selected};

  @override
  Widget build(BuildContext context) {
    return AlertDialog(
      backgroundColor: surfaceRaised,
      title: const Text('房间模组'),
      content: SizedBox(
        width: 420,
        child: widget.catalog.isEmpty
            ? const Text(
                '服务器还没有上传任何模组。管理员用 POST /v1/admin/modules 上传后再来配置。',
                style: TextStyle(color: muted, height: 1.5),
              )
            : ListView(
                shrinkWrap: true,
                children: [
                  for (final module in widget.catalog)
                    CheckboxListTile(
                      value: _selected.contains(module.id),
                      onChanged: (checked) => setState(() {
                        if (checked ?? false) {
                          _selected.add(module.id);
                        } else {
                          _selected.remove(module.id);
                        }
                      }),
                      controlAffinity: ListTileControlAffinity.leading,
                      contentPadding: EdgeInsets.zero,
                      title: Text(module.id),
                      subtitle: Text(
                        '${module.version} · ${module.hooks.join('/')}'
                        ' · ${module.capabilities.isEmpty ? '无能力' : module.capabilities.join('/')}',
                        style: const TextStyle(color: muted, fontSize: 12),
                      ),
                    ),
                ],
              ),
      ),
      actions: [
        TextButton(
          onPressed: () => Navigator.of(context).pop(),
          child: const Text('取消'),
        ),
        TextButton(
          onPressed: widget.catalog.isEmpty
              ? null
              : () => Navigator.of(context).pop(_selected),
          child: const Text('保存'),
        ),
      ],
    );
  }
}

