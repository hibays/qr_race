import 'package:flutter/material.dart';

import 'api/app_links.dart';
import 'state/app_state.dart';
import 'state/server_settings.dart';
import 'ui/connect_screen.dart';
import 'ui/lobby_screen.dart';
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
  final _navigatorKey = GlobalKey<NavigatorState>();

  @override
  void initState() {
    super.initState();
    AppLinks.listen((link) {
      _navigatorKey.currentState?.push(
        MaterialPageRoute(
          builder: (_) => ConnectScreen(state: _state, initialLink: link),
        ),
      );
    });
  }

  @override
  void dispose() {
    AppLinks.stop();
    _state.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    return MaterialApp(
      title: 'qr_race',
      debugShowCheckedModeBanner: false,
      theme: buildTheme(),
      navigatorKey: _navigatorKey,
      home: _demo
          ? RoomScreen(state: _state, address: '东京A区1B级103房')
          : _StartupGate(state: _state),
    );
  }
}

/// Restores the last validated connection before showing the login form, so a
/// returning client lands in the lobby instead of being asked to authenticate
/// again.
class _StartupGate extends StatefulWidget {
  const _StartupGate({required this.state});

  final AppState state;

  @override
  State<_StartupGate> createState() => _StartupGateState();
}

class _StartupGateState extends State<_StartupGate> {
  bool _checking = true;
  bool _connected = false;

  @override
  void initState() {
    super.initState();
    _restore();
  }

  Future<void> _restore() async {
    final settings = await ServerSettings.load();
    if (settings.token.isEmpty) {
      if (mounted) setState(() => _checking = false);
      return;
    }
    final ok = await widget.state.connect(
      server: settings.server,
      token: settings.token,
      fingerprint: settings.fingerprint,
    );
    if (!mounted) return;
    setState(() {
      _checking = false;
      _connected = ok;
    });
  }

  @override
  Widget build(BuildContext context) {
    if (_checking) {
      return const Scaffold(
        body: Center(child: CircularProgressIndicator()),
      );
    }
    if (_connected) return LobbyScreen(state: widget.state);
    return ConnectScreen(state: widget.state);
  }
}
