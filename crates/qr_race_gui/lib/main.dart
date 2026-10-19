import 'package:flutter/material.dart';

import 'state/app_state.dart';
import 'ui/connect_screen.dart';
import 'ui/room_screen.dart';
import 'ui/theme.dart';

const _demo = bool.fromEnvironment('DEMO');

void main() {
  runApp(const QrRaceApp());
}

class QrRaceApp extends StatefulWidget {
  const QrRaceApp({super.key});

  @override
  State<QrRaceApp> createState() => _QrRaceAppState();
}

class _QrRaceAppState extends State<QrRaceApp> {
  final AppState _state = _demo ? AppState.demo() : AppState();

  @override
  void dispose() {
    _state.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    return MaterialApp(
      title: 'qr_race',
      debugShowCheckedModeBanner: false,
      theme: buildTheme(),
      home: _demo
          ? RoomScreen(state: _state, address: '东京A区1B级103房')
          : ConnectScreen(state: _state),
    );
  }
}
