import 'package:flutter/material.dart';

import 'room_catalog.dart';
import '../ui/echo_signal.dart';
import '../ui/theme.dart';

enum RoomPickerMode { enter, create }

/// Shows the fixed venue picker. For creation, [onSubmit] keeps the dialog
/// open while the room is created and entered, so the primary button itself
/// can show the broadcast mark instead of closing into a silent wait.
Future<RoomChoice?> showRoomPicker(
  BuildContext context, {
  required RoomPickerMode mode,
  Future<String?> Function(RoomChoice choice)? onSubmit,
}) {
  return showDialog<RoomChoice>(
    context: context,
    barrierDismissible: false,
    builder: (_) => _RoomPickerDialog(mode: mode, onSubmit: onSubmit),
  );
}

class _RoomPickerDialog extends StatefulWidget {
  const _RoomPickerDialog({required this.mode, this.onSubmit});

  final RoomPickerMode mode;
  final Future<String?> Function(RoomChoice choice)? onSubmit;

  @override
  State<_RoomPickerDialog> createState() => _RoomPickerDialogState();
}

class _RoomPickerDialogState extends State<_RoomPickerDialog> {
  String _capital = '东京';
  String _district = 'A';
  String _level = '1A';
  String _room = '101';
  bool _submitting = false;
  String? _error;

  void _changeCapital(String value) {
    setState(() {
      _capital = value;
      _district = RoomCatalog.districts(value).first;
      _level = RoomCatalog.levels(value).first;
      _room = RoomCatalog.rooms(value).first;
    });
  }

  String get _address => RoomCatalog.address(
    capital: _capital,
    district: _district,
    level: _level,
    room: _room,
  );

  Future<void> _submit() async {
    if (_submitting) return;
    final choice = RoomChoice(address: _address);
    final submit = widget.onSubmit;
    if (submit == null) {
      Navigator.pop(context, choice);
      return;
    }

    setState(() {
      _submitting = true;
      _error = null;
    });
    final started = DateTime.now();
    final error = await submit(choice);
    // Keep the broadcast mark visible long enough to read even on a fast LAN;
    // failures should surface immediately.
    final elapsed = DateTime.now().difference(started);
    const minimum = Duration(milliseconds: 420);
    if (error == null && elapsed < minimum) {
      await Future<void>.delayed(minimum - elapsed);
    }
    if (!mounted) return;
    if (error != null) {
      setState(() {
        _submitting = false;
        _error = error;
      });
      return;
    }
    Navigator.pop(context, choice);
  }

  @override
  Widget build(BuildContext context) {
    final rooms = RoomCatalog.rooms(_capital);
    return PopScope(
      canPop: !_submitting,
      child: AlertDialog(
        title: Text(widget.mode == RoomPickerMode.create ? '创建房间' : '选择房间'),
        content: SizedBox(
          width: 440,
          child: SingleChildScrollView(
            child: Column(
              mainAxisSize: MainAxisSize.min,
              crossAxisAlignment: CrossAxisAlignment.stretch,
              children: [
                SegmentedButton<String>(
                  segments: [
                    for (final capital in RoomCatalog.capitals)
                      ButtonSegment(value: capital, label: Text(capital)),
                  ],
                  selected: {_capital},
                  onSelectionChanged: _submitting
                      ? null
                      : (value) => _changeCapital(value.first),
                ),
                const SizedBox(height: 12),
                Row(
                  children: [
                    Expanded(
                      child: DropdownButtonFormField<String>(
                        initialValue: _district,
                        decoration: const InputDecoration(labelText: '区'),
                        items: [
                          for (final district in RoomCatalog.districts(
                            _capital,
                          ))
                            DropdownMenuItem(
                              value: district,
                              child: Text('$district区'),
                            ),
                        ],
                        onChanged: _submitting
                            ? null
                            : (value) => setState(() => _district = value!),
                      ),
                    ),
                    const SizedBox(width: 8),
                    Expanded(
                      child: DropdownButtonFormField<String>(
                        initialValue: _level,
                        decoration: const InputDecoration(labelText: '级'),
                        items: [
                          for (final level in RoomCatalog.levels(_capital))
                            DropdownMenuItem(
                              value: level,
                              child: Text('$level级'),
                            ),
                        ],
                        onChanged: _submitting
                            ? null
                            : (value) => setState(() => _level = value!),
                      ),
                    ),
                    const SizedBox(width: 8),
                    Expanded(
                      child: DropdownButtonFormField<String>(
                        initialValue: _room,
                        decoration: const InputDecoration(labelText: '房号'),
                        items: [
                          for (final room in rooms)
                            DropdownMenuItem(value: room, child: Text(room)),
                        ],
                        onChanged: _submitting
                            ? null
                            : (value) => setState(() => _room = value!),
                      ),
                    ),
                  ],
                ),
                const SizedBox(height: 14),
                Container(
                  padding: const EdgeInsets.all(12),
                  decoration: BoxDecoration(
                    color: Theme.of(context)
                        .colorScheme
                        .surfaceContainerHighest,
                    borderRadius: BorderRadius.circular(10),
                  ),
                  child: Text(
                    _address,
                    style: const TextStyle(
                      fontWeight: FontWeight.w700,
                      fontSize: 16,
                    ),
                  ),
                ),
                if (_error != null) ...[
                  const SizedBox(height: 10),
                  Text(
                    _error!,
                    style: const TextStyle(color: danger, fontSize: 12.5),
                  ),
                ],
              ],
            ),
          ),
        ),
        actions: [
          TextButton(
            onPressed: _submitting ? null : () => Navigator.pop(context),
            child: const Text('取消'),
          ),
          FilledButton(
            onPressed: _submitting ? null : _submit,
            style: FilledButton.styleFrom(
              disabledBackgroundColor: accent,
              disabledForegroundColor: accentInk,
            ),
            child: _submitting
                ? Row(
                    mainAxisSize: MainAxisSize.min,
                    children: const [
                      EchoSignal(
                        size: 20,
                        mode: EchoSignalMode.broadcasting,
                        color: accentInk,
                        accent: accentInk,
                      ),
                      SizedBox(width: 8),
                      Text('正在创建'),
                    ],
                  )
                : Text(widget.mode == RoomPickerMode.create ? '创建并点亮' : '进入'),
          ),
        ],
      ),
    );
  }
}
