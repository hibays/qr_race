import 'dart:math';

import 'package:flutter/material.dart';

import 'room_catalog.dart';

enum RoomPickerMode { enter, create }

Future<RoomChoice?> showRoomPicker(
  BuildContext context, {
  required RoomPickerMode mode,
}) {
  return showDialog<RoomChoice>(
    context: context,
    builder: (_) => _RoomPickerDialog(mode: mode),
  );
}

class _RoomPickerDialog extends StatefulWidget {
  const _RoomPickerDialog({required this.mode});

  final RoomPickerMode mode;

  @override
  State<_RoomPickerDialog> createState() => _RoomPickerDialogState();
}

class _RoomPickerDialogState extends State<_RoomPickerDialog> {
  String _capital = '东京';
  String _district = 'A';
  String _level = '1A';
  String _room = '101';
  final _pin = TextEditingController();

  @override
  void initState() {
    super.initState();
    if (widget.mode == RoomPickerMode.create) {
      _pin.text = _randomPin();
    }
  }

  @override
  void dispose() {
    _pin.dispose();
    super.dispose();
  }

  static String _randomPin() {
    return Random.secure().nextInt(10000).toString().padLeft(4, '0');
  }

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

  @override
  Widget build(BuildContext context) {
    final rooms = RoomCatalog.rooms(_capital);
    return AlertDialog(
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
                onSelectionChanged: (value) => _changeCapital(value.first),
              ),
              const SizedBox(height: 12),
              Row(
                children: [
                  Expanded(
                    child: DropdownButtonFormField<String>(
                      initialValue: _district,
                      decoration: const InputDecoration(labelText: '区'),
                      items: [
                        for (final district in RoomCatalog.districts(_capital))
                          DropdownMenuItem(value: district, child: Text('$district区')),
                      ],
                      onChanged: (value) => setState(() => _district = value!),
                    ),
                  ),
                  const SizedBox(width: 8),
                  Expanded(
                    child: DropdownButtonFormField<String>(
                      initialValue: _level,
                      decoration: const InputDecoration(labelText: '级'),
                      items: [
                        for (final level in RoomCatalog.levels(_capital))
                          DropdownMenuItem(value: level, child: Text('$level级')),
                      ],
                      onChanged: (value) => setState(() => _level = value!),
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
                      onChanged: (value) => setState(() => _room = value!),
                    ),
                  ),
                ],
              ),
              const SizedBox(height: 14),
              Container(
                padding: const EdgeInsets.all(12),
                decoration: BoxDecoration(
                  color: Theme.of(context).colorScheme.surfaceContainerHighest,
                  borderRadius: BorderRadius.circular(10),
                ),
                child: Text(
                  _address,
                  style: const TextStyle(fontWeight: FontWeight.w700, fontSize: 16),
                ),
              ),
              if (widget.mode == RoomPickerMode.create) ...[
                const SizedBox(height: 14),
                TextField(
                  controller: _pin,
                  keyboardType: TextInputType.number,
                  maxLength: 4,
                  decoration: const InputDecoration(
                    labelText: '4 位配对码',
                    helperText: '房间成员可凭此码进入；可修改',
                  ),
                ),
              ],
            ],
          ),
        ),
      ),
      actions: [
        TextButton(
          onPressed: () => Navigator.pop(context),
          child: const Text('取消'),
        ),
        FilledButton(
          onPressed: () {
            final pin = _pin.text.trim();
            if (widget.mode == RoomPickerMode.create &&
                (pin.length != 4 || int.tryParse(pin) == null)) {
              return;
            }
            Navigator.pop(
              context,
              RoomChoice(
                address: _address,
                pin: widget.mode == RoomPickerMode.create ? pin : null,
              ),
            );
          },
          child: Text(widget.mode == RoomPickerMode.create ? '创建并点亮' : '进入'),
        ),
      ],
    );
  }
}

Future<String?> showPinEntry(BuildContext context) {
  final controller = TextEditingController();
  return showDialog<String>(
    context: context,
    builder: (context) => AlertDialog(
      title: const Text('输入 4 位配对码'),
      content: TextField(
        controller: controller,
        autofocus: true,
        keyboardType: TextInputType.number,
        maxLength: 4,
        decoration: const InputDecoration(hintText: '0000'),
      ),
      actions: [
        TextButton(
          onPressed: () => Navigator.pop(context),
          child: const Text('取消'),
        ),
        FilledButton(
          onPressed: () {
            final pin = controller.text.trim();
            if (pin.length == 4 && int.tryParse(pin) != null) {
              Navigator.pop(context, pin);
            }
          },
          child: const Text('进入'),
        ),
      ],
    ),
  ).whenComplete(controller.dispose);
}


