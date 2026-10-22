import 'package:flutter_test/flutter_test.dart';
import 'package:qr_race_gui/state/app_state.dart';

void main() {
  test('relay_seen updates an existing relay in place', () {
    final state = AppState.demo();
    state.handleRoomEventForTest(
      '{"type":"relay_seen","relay":"r-1","created_at_ms":123456}',
    );

    expect(
      state.room?.relays.firstWhere((relay) => relay.id == 'r-1').createdAtMs,
      123456,
    );
  });

  test('relay_seen for an unknown relay does not corrupt the room', () async {
    final state = AppState.demo();
    final before = state.room;
    state.handleRoomEventForTest(
      '{"type":"relay_seen","relay":"r-missing","created_at_ms":123456}',
    );
    await Future<void>.delayed(Duration.zero);

    expect(state.room, same(before));
    expect(state.error, isNull);
  });

  test('relay_seen before any room snapshot is harmless', () async {
    final state = AppState();
    state.handleRoomEventForTest(
      '{"type":"relay_seen","relay":"r-1","created_at_ms":123456}',
    );
    await Future<void>.delayed(Duration.zero);

    expect(state.room, isNull);
    expect(state.error, isNull);
  });
}
