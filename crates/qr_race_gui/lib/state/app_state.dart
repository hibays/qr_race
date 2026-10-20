import 'dart:async';
import 'dart:convert';

import 'package:flutter/foundation.dart';
import 'package:web_socket_channel/web_socket_channel.dart';

import '../api/api_client.dart';

/// One connection's worth of state: who we are, the lobby, the open room.
class AppState extends ChangeNotifier {
  AppState();

  /// Offline state for design work (`--dart-define=DEMO=true`); never talks to
  /// a server and is not part of the shipped flow.
  factory AppState.demo() {
    final state = AppState();
    state.session = const AuthSession(
      sessionId: 'demo',
      userId: 'host-a',
      expiresAtMs: 0,
    );
    state.lobby = const [
      LobbyRoom(
        label: '东京A区1B级103房',
        host: 'host-a',
        memberCount: 4,
        moduleCount: 0,
      ),
    ];
    state.room = RoomView(
      label: '东京A区1B级103房',
      lit: true,
      host: 'host-a',
      modules: const [],
      members: const [
        RoomMember(user: 'host-a', role: 'host', lastRelay: 'r-1'),
        RoomMember(user: 'mika', role: 'co_host', lastRelay: 'r-2'),
        RoomMember(user: 'lin', role: 'member', lastRelay: null),
        RoomMember(user: 'yuu', role: 'member', lastRelay: null),
      ],
      relays: [
        Relay(
          id: 'r-1',
          publisher: 'host-a',
          kind: 'url',
          payloadB64: base64Encode(
            utf8.encode('https://example.test/relay?n=42&room=103'),
          ),
          createdAtMs: 1700000000000,
        ),
        Relay(
          id: 'r-2',
          publisher: 'mika',
          kind: 'text',
          payloadB64: base64Encode(utf8.encode('第二枚码的接力内容')),
          createdAtMs: 1700000060000,
        ),
      ],
    );
    return state;
  }

  ApiClient? api;
  AuthSession? session;
  List<LobbyRoom> lobby = const [];
  RoomView? room;
  bool busy = false;
  String? error;

  WebSocketChannel? _events;
  StreamSubscription<dynamic>? _eventsSub;
  String? _roomAddress;

  bool get isConnected => session != null;

  Future<bool> connect({
    required String server,
    required String token,
    String? fingerprint,
  }) async {
    busy = true;
    error = null;
    notifyListeners();
    try {
      final client = ApiClient(baseUrl: server, fingerprint: fingerprint);
      final auth = await client.authenticate(token: token);
      api = client;
      session = auth;
      await refreshLobby();
      return true;
    } catch (e) {
      error = _describe(e);
      return false;
    } finally {
      busy = false;
      notifyListeners();
    }
  }

  Future<void> refreshLobby() async {
    final client = api;
    if (client == null) return;
    try {
      lobby = await client.lobby();
      error = null;
    } catch (e) {
      error = _describe(e);
    }
    notifyListeners();
  }

  /// Joins the room (idempotent server-side) and subscribes to its events.
  Future<bool> enterRoom(String address) async {
    final client = api;
    if (client == null) {
      // Offline demo state already holds a room; nothing to join.
      return room != null;
    }
    busy = true;
    error = null;
    notifyListeners();
    try {
      await client.join(address);
      room = await client.room(address);
      _roomAddress = address;
      await refreshModuleCatalog();
      await _subscribe(client, address);
      return true;
    } catch (e) {
      error = _describe(e);
      return false;
    } finally {
      busy = false;
      notifyListeners();
    }
  }

  Future<void> refreshRoom() async {
    final client = api;
    final address = _roomAddress;
    if (client == null || address == null) return;
    try {
      room = await client.room(address);
      error = null;
    } catch (e) {
      error = _describe(e);
    }
    notifyListeners();
  }

  /// What the admin has uploaded; used by the host's module picker.
  List<ModuleInfo> moduleCatalog = const [];

  Future<void> refreshModuleCatalog() async {
    final client = api;
    if (client == null) return;
    try {
      moduleCatalog = await client.modules();
    } catch (_) {
      // A catalog failure must not hide the room itself.
    }
    notifyListeners();
  }

  Future<void> setModules(List<String> modules) async {
    final client = api;
    final address = _roomAddress;
    if (client == null || address == null) return;
    try {
      await client.setModules(address, modules);
      await refreshRoom();
    } catch (e) {
      error = _describe(e);
      notifyListeners();
    }
  }

  /// Creates the room, or reuses it if it already exists, then lights it.
  /// Both "already exists" and "already lit" are fine: the host is re-entering.
  Future<String?> createAndLight(String address) async {
    final client = api;
    if (client == null) return '离线演示模式不能建房';
    busy = true;
    error = null;
    notifyListeners();
    try {
      try {
        await client.createRoom(address);
      } on ApiException catch (e) {
        if (e.status != 409) rethrow;
      }
      try {
        await client.lightRoom(address);
      } on ApiException catch (e) {
        if (e.status != 409) rethrow;
      }
      await refreshLobby();
      return null;
    } catch (e) {
      error = _describe(e);
      return error;
    } finally {
      busy = false;
      notifyListeners();
    }
  }

  Future<void> lightRoom() async {
    final client = api;
    final address = _roomAddress;
    if (client == null || address == null) return;
    try {
      await client.lightRoom(address);
      await refreshRoom();
    } catch (e) {
      error = _describe(e);
      notifyListeners();
    }
  }

  Future<void> promote(String user) async {
    final client = api;
    final address = _roomAddress;
    if (client == null || address == null) return;
    try {
      await client.promote(address, user);
      await refreshRoom();
    } catch (e) {
      error = _describe(e);
      notifyListeners();
    }
  }

  Future<void> demote(String user) async {
    final client = api;
    final address = _roomAddress;
    if (client == null || address == null) return;
    try {
      await client.demote(address, user);
      await refreshRoom();
    } catch (e) {
      error = _describe(e);
      notifyListeners();
    }
  }

  /// Leaving is best-effort: the host cannot leave their own room, and a
  /// network failure should not trap the user on the screen.
  Future<void> exitRoom() async {
    final client = api;
    final address = _roomAddress;
    await _unsubscribe();
    _roomAddress = null;
    room = null;
    if (client != null && address != null) {
      try {
        await client.leave(address);
      } catch (_) {
        // Ignore: closing the screen always wins.
      }
    }
    notifyListeners();
  }

  Future<void> _subscribe(ApiClient client, String address) async {
    await _unsubscribe();
    final channel = await client.openEvents(address);
    _events = channel;
    _eventsSub = channel.stream.listen(
      (_) => refreshRoom(),
      onError: (Object _) {},
      cancelOnError: false,
    );
  }

  /// Drops the session and returns the app to the connect screen.
  Future<void> disconnect() async {
    await _unsubscribe();
    api?.close();
    api = null;
    session = null;
    lobby = const [];
    room = null;
    _roomAddress = null;
    error = null;
    notifyListeners();
  }

  Future<void> _unsubscribe() async {
    await _eventsSub?.cancel();
    _eventsSub = null;
    await _events?.sink.close();
    _events = null;
  }

  String _describe(Object error) {
    if (error is ApiException) {
      if (error.status == 401) return '认证失败：token 不对或已失效';
      if (error.status == 429) return '尝试失败次数过多，账户已锁定 60 秒，请稍后再试';
      return error.message;
    }
    final text = error.toString();
    if (text.contains('HandshakeException') || text.contains('CERTIFICATE')) {
      return 'TLS 握手失败：证书指纹不匹配，或 URL 的地址不在证书里。'
          '请重新复制自部署页的指纹；本机自部署用 https://127.0.0.1:<端口>。';
    }
    if (text.contains('SocketException') ||
        text.contains('Connection refused') ||
        text.contains('Failed host lookup')) {
      return '连不上服务器：检查地址/端口、服务端是否在运行、手机与电脑是否同一网络；'
          'USB 方式需要先执行 adb reverse。';
    }
    if (text.contains('TimeoutException')) {
      return '连接超时：服务端未响应或网络被拦截。';
    }
    if (text.contains('FormatException')) {
      return '服务器地址格式不对，需要 http:// 或 https:// 开头。';
    }
    return text;
  }

  @override
  void dispose() {
    _unsubscribe();
    api?.close();
    super.dispose();
  }
}
