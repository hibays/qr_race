import 'dart:js_interop';

@JS('window.open')
external JSAny? _windowOpen(String url, String target);

Future<bool> openExternal(String url) async {
  if (url.trim().isEmpty) return false;
  _windowOpen(url.trim(), '_blank');
  return true;
}
