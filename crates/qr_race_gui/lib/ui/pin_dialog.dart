import 'dart:async';

import 'package:flutter/material.dart';
import 'package:flutter/services.dart';

import 'theme.dart';

/// A 4-digit PIN prompt whose controller lives with the route, so it is never
/// disposed while the dialog is still animating out.
///
/// The four cells are one sequential input, not four tappable selectors:
/// digits fill left-to-right and four digits complete the dialog without a
/// confirm button.
Future<String?> showPinDialog(BuildContext context, {required String title}) {
  return showDialog<String>(
    context: context,
    barrierDismissible: true,
    builder: (_) => _PinDialog(title: title),
  );
}

class _PinDialog extends StatefulWidget {
  const _PinDialog({required this.title});

  final String title;

  @override
  State<_PinDialog> createState() => _PinDialogState();
}

class _PinDialogState extends State<_PinDialog> {
  static const _length = 4;

  late final TextEditingController _controller;
  late final FocusNode _focusNode;
  Timer? _completeTimer;
  Timer? _caretTimer;
  bool _caretOn = true;
  bool _completed = false;

  @override
  void initState() {
    super.initState();
    _controller = TextEditingController();
    _focusNode = FocusNode()..addListener(_onFocusChanged);
  }

  @override
  void dispose() {
    _completeTimer?.cancel();
    _caretTimer?.cancel();
    _focusNode
      ..removeListener(_onFocusChanged)
      ..dispose();
    _controller.dispose();
    super.dispose();
  }

  void _onFocusChanged() {
    if (!mounted) return;
    _syncCaret();
    setState(() {});
  }

  void _syncCaret() {
    _caretTimer?.cancel();
    if (!_focusNode.hasFocus) return;
    _caretOn = true;
    _caretTimer = Timer.periodic(const Duration(milliseconds: 520), (_) {
      if (mounted) setState(() => _caretOn = !_caretOn);
    });
  }

  void _onChanged(String value) {
    if (_completed) return;
    _completeTimer?.cancel();
    setState(() {});
    if (value.length == _length) {
      _completeTimer = Timer(const Duration(milliseconds: 160), _complete);
    }
  }

  void _complete() {
    if (_completed || !mounted) return;
    final value = _controller.text;
    if (value.length != _length) return;
    _completed = true;
    _completeTimer?.cancel();
    Navigator.of(context).pop(value);
  }

  @override
  Widget build(BuildContext context) {
    final text = _controller.text;
    final focused = _focusNode.hasFocus;
    final active = text.length.clamp(0, _length - 1);

    return Dialog(
      backgroundColor: Colors.transparent,
      insetPadding: const EdgeInsets.symmetric(horizontal: 24, vertical: 24),
      child: ConstrainedBox(
        constraints: const BoxConstraints(maxWidth: 390),
        child: DecoratedBox(
          decoration: BoxDecoration(
            color: surfaceRaised,
            borderRadius: BorderRadius.circular(20),
            border: Border.all(color: line),
            boxShadow: const [
              BoxShadow(
                color: Color(0x66000000),
                blurRadius: 28,
                offset: Offset(0, 16),
              ),
            ],
          ),
          child: Padding(
            padding: const EdgeInsets.fromLTRB(24, 22, 24, 16),
            child: Column(
              mainAxisSize: MainAxisSize.min,
              crossAxisAlignment: CrossAxisAlignment.stretch,
              children: [
                Text(
                  widget.title,
                  style: const TextStyle(
                    color: ink,
                    fontSize: 19,
                    fontWeight: FontWeight.w700,
                    letterSpacing: -0.2,
                  ),
                ),
                const SizedBox(height: 20),
                Semantics(
                  label: '4 位配对码',
                  textField: true,
                  child: Stack(
                    children: [
                      Row(
                        mainAxisAlignment: MainAxisAlignment.center,
                        children: List.generate(_length, (index) {
                          return Padding(
                            padding: EdgeInsets.only(
                              right: index == _length - 1 ? 0 : 10,
                            ),
                            child: _PinCell(
                              key: ValueKey('pin-cell-$index'),
                              index: index,
                              digit: index < text.length ? text[index] : null,
                              active: focused && index == active,
                              caretOn: _caretOn,
                            ),
                          );
                        }),
                      ),
                      Positioned.fill(
                        child: TextField(
                          controller: _controller,
                          focusNode: _focusNode,
                          autofocus: true,
                          keyboardType: const TextInputType.numberWithOptions(
                            signed: false,
                            decimal: false,
                          ),
                          textInputAction: TextInputAction.done,
                          inputFormatters: [
                            FilteringTextInputFormatter.digitsOnly,
                            LengthLimitingTextInputFormatter(_length),
                          ],
                          onChanged: _onChanged,
                          onSubmitted: (_) => _complete(),
                          showCursor: false,
                          enableInteractiveSelection: false,
                          autocorrect: false,
                          enableSuggestions: false,
                          style: const TextStyle(
                            color: Colors.transparent,
                            fontSize: 1,
                          ),
                          cursorColor: Colors.transparent,
                          decoration: const InputDecoration(
                            border: InputBorder.none,
                            enabledBorder: InputBorder.none,
                            focusedBorder: InputBorder.none,
                            filled: false,
                            isDense: true,
                            counterText: '',
                            contentPadding: EdgeInsets.zero,
                          ),
                        ),
                      ),
                    ],
                  ),
                ),
                const SizedBox(height: 12),
                const Text(
                  '输入满 4 位自动完成',
                  textAlign: TextAlign.center,
                  style: TextStyle(color: muted, fontSize: 12, height: 1.4),
                ),
                const SizedBox(height: 8),
                Align(
                  alignment: Alignment.centerRight,
                  child: TextButton(
                    onPressed: () => Navigator.of(context).pop(),
                    style: TextButton.styleFrom(
                      foregroundColor: muted,
                      padding: const EdgeInsets.symmetric(
                        horizontal: 12,
                        vertical: 10,
                      ),
                    ),
                    child: const Text('取消'),
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

class _PinCell extends StatelessWidget {
  const _PinCell({
    super.key,
    required this.index,
    required this.digit,
    required this.active,
    required this.caretOn,
  });

  final int index;
  final String? digit;
  final bool active;
  final bool caretOn;

  @override
  Widget build(BuildContext context) {
    // Alternating shallow/deep cells keep the four positions legible.
    final base = index.isEven ? surface : surfaceRaised;
    final cellColor = active
        ? Color.alphaBlend(accent.withValues(alpha: 0.10), base)
        : base;

    return AnimatedContainer(
      duration: const Duration(milliseconds: 140),
      curve: Curves.easeOutCubic,
      width: 58,
      height: 68,
      alignment: Alignment.center,
      decoration: BoxDecoration(
        color: cellColor,
        borderRadius: BorderRadius.circular(14),
        border: Border.all(
          color: active ? accent : line,
          width: active ? 1.4 : 1,
        ),
        boxShadow: active
            ? [
                BoxShadow(
                  color: accent.withValues(alpha: 0.12),
                  blurRadius: 14,
                  offset: const Offset(0, 4),
                ),
              ]
            : null,
      ),
      child: digit != null
          ? Text(
              digit!,
              textAlign: TextAlign.center,
              style: const TextStyle(
                color: ink,
                fontSize: 30,
                fontWeight: FontWeight.w700,
                height: 1,
                fontFeatures: [FontFeature.tabularFigures()],
              ),
            )
          : active && caretOn
          ? Container(
              width: 2,
              height: 28,
              decoration: BoxDecoration(
                color: accent,
                borderRadius: BorderRadius.circular(1),
              ),
            )
          : null,
    );
  }
}
