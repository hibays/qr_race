/// Fixed venue inventory. Addresses are coordinates, not free text.
class RoomCatalog {
  static const capitals = ['东京', '西京'];

  static List<String> districts(String capital) =>
      capital == '西京' ? const ['Z'] : const ['A', 'B'];

  static List<String> levels(String capital) {
    if (capital == '西京') {
      return const ['A', 'B', 'C', 'D', 'E', 'F', 'G', 'H'];
    }
    return [
      for (var floor = 1; floor <= 6; floor++) ...[
        '${floor}A',
        '${floor}B',
      ],
    ];
  }

  static List<String> rooms(String capital) {
    final floors = capital == '西京' ? 5 : 6;
    return [
      for (var floor = 1; floor <= floors; floor++)
        for (var unit = 1; unit <= 9; unit++)
          '$floor${unit.toString().padLeft(2, '0')}',
    ];
  }

  static String address({
    required String capital,
    required String district,
    required String level,
    required String room,
  }) =>
      '$capital$district区$level级$room房';
}

class RoomChoice {
  const RoomChoice({required this.address, this.pin});

  final String address;
  final String? pin;
}
