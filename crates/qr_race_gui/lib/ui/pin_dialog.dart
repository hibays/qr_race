import 'package:flutter/material.dart';

/// A 4-digit PIN prompt whose controller lives with the route, so it is never
/// disposed while the dialog is still animating out.
Future<String?> showPinDialog(
  BuildContext context, {
  required String title,
  String initial = '',
}) {
  return showDialog<String>(
    context: context,
    builder: (_) => _PinDialog(title: title, initial: initial),
  );
}

class _PinDialog extends StatefulWidget {
  const _PinDialog({required this.title, required this.initial});

  final String title;
  final String initial;

  @override
  State<_PinDialog> createState() => _PinDialogState();
}

class _PinDialogState extends State<_PinDialog> {
  late final TextEditingController _controller;

  @override
  void initState() {
    super.initState();
    _controller = TextEditingController(text: widget.initial);
  }

  @override
  void dispose() {
    _controller.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    return AlertDialog(
      title: Text(widget.title),
      content: TextField(
        controller: _controller,
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
            final value = _controller.text.trim();
            if (value.length == 4 && int.tryParse(value) != null) {
              Navigator.pop(context, value);
            }
          },
          child: const Text('确定'),
        ),
      ],
    );
  }
}
