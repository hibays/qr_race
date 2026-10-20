import 'package:flutter/material.dart';
import 'package:flutter/services.dart';

import '../native/external_links.dart';
import '../state/relay_slot.dart';
import 'theme.dart';

/// Opens a relay payload in another app. http(s) links go through the system
/// chooser (WeChat/WeCom/browser all appear if installed); custom schemes are
/// launched directly, with a copy-to-clipboard fallback.
Future<void> showOpenLinkSheet(BuildContext context, String payload) async {
  final content = relayContent(payload).trim();
  final uri = Uri.tryParse(content);
  if (uri == null || !uri.hasScheme) {
    await Clipboard.setData(ClipboardData(text: content));
    if (context.mounted) {
      ScaffoldMessenger.of(context).showSnackBar(
        const SnackBar(content: Text('不是可打开的链接，已复制内容')),
      );
    }
    return;
  }

  final isWeb = uri.scheme == 'http' || uri.scheme == 'https';
  if (!isWeb) {
    final opened = await openExternal(content);
    if (!context.mounted) return;
    if (!opened) {
      await Clipboard.setData(ClipboardData(text: content));
      if (!context.mounted) return;
      ScaffoldMessenger.of(context).showSnackBar(
        const SnackBar(content: Text('没有 App 能打开这个 scheme，已复制')),
      );
    }
    return;
  }

  final choice = await showModalBottomSheet<String>(
    context: context,
    backgroundColor: surfaceRaised,
    showDragHandle: true,
    builder: (context) => SafeArea(
      child: Column(
        mainAxisSize: MainAxisSize.min,
        children: [
          ListTile(
            leading: const Icon(Icons.open_in_new),
            title: const Text('系统选择 App'),
            subtitle: const Text('微信 / 企业微信 / 浏览器等已安装应用'),
            onTap: () => Navigator.pop(context, 'system'),
          ),
          ListTile(
            leading: const Icon(Icons.chat_bubble_outline),
            title: const Text('微信'),
            subtitle: const Text('打开微信；链接需要微信内可访问'),
            onTap: () => Navigator.pop(context, 'wechat'),
          ),
          ListTile(
            leading: const Icon(Icons.business_center_outlined),
            title: const Text('企业微信'),
            onTap: () => Navigator.pop(context, 'wxwork'),
          ),
          ListTile(
            leading: const Icon(Icons.public),
            title: const Text('浏览器'),
            onTap: () => Navigator.pop(context, 'browser'),
          ),
          ListTile(
            leading: const Icon(Icons.copy_all_outlined),
            title: const Text('复制链接'),
            onTap: () => Navigator.pop(context, 'copy'),
          ),
        ],
      ),
    ),
  );
  if (choice == null || !context.mounted) return;
  if (choice == 'copy') {
    await Clipboard.setData(ClipboardData(text: content));
    if (context.mounted) {
      ScaffoldMessenger.of(context).showSnackBar(
        const SnackBar(content: Text('链接已复制')),
      );
    }
    return;
  }
  final target = switch (choice) {
    'wechat' => 'weixin://',
    'wxwork' => 'wxwork://',
    _ => content,
  };
  final opened = await openExternal(target);
  if (!context.mounted) return;
  if (!opened) {
    await Clipboard.setData(ClipboardData(text: content));
    if (!context.mounted) return;
    ScaffoldMessenger.of(context).showSnackBar(
      const SnackBar(content: Text('没有找到能打开它的 App，链接已复制')),
    );
  }
}
