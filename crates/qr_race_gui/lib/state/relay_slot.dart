/// A relay payload can carry an ownership prefix: `A:https://...` or `cam1|...`.
/// The prefix identifies the dynamic slot; a new payload with the same prefix
/// replaces that slot in the room, while different prefixes coexist.
String relaySlot(String text) {
  final trimmed = text.trim();
  if (trimmed.startsWith('http://') || trimmed.startsWith('https://')) {
    return trimmed;
  }
  final colon = trimmed.indexOf(':');
  final pipe = trimmed.indexOf('|');
  final index = switch ((colon, pipe)) {
    (< 0, < 0) => -1,
    (< 0, _) => pipe,
    (_, < 0) => colon,
    _ => colon < pipe ? colon : pipe,
  };
  if (index <= 0 || index >= trimmed.length - 1 || index > 32) {
    return trimmed;
  }
  return trimmed.substring(0, index).trim();
}

/// The content after the prefix, or the whole payload when there is none.
String relayContent(String text) {
  final slot = relaySlot(text);
  if (slot == text) return text;
  return text.substring(slot.length + 1).trim();
}
