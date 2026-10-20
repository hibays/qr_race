import 'package:flutter_test/flutter_test.dart';
import 'package:qr_race_gui/state/relay_slot.dart';

void main() {
  test('prefixes identify dynamic slots', () {
    expect(relaySlot('A:https://example.test/one'), 'A');
    expect(relaySlot('cam1|payload'), 'cam1');
    expect(relayContent('A:https://example.test/one'), 'https://example.test/one');
    expect(relaySlot('https://example.test/one'), 'https://example.test/one');
    expect(relaySlot('A:'), 'A:');
  });
}
