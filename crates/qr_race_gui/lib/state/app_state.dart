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
      LobbyRoom(label: '东京A区1B级103房', host: 'host-a', memberCount: 4),
    ];
    state.room = RoomView(
      label: '东京A区1B级103房',
      lit: true,
      host: 'host-a',
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
          createdAtMs: DateTime.now().millisecondsSinceEpoch - 2000,
        ),
        Relay(
          id: 'r-2',
          publisher: 'mika',
          kind: 'text',
          payloadB64: base64Encode(utf8.encode('第二枚码的接力内容')),
          createdAtMs: DateTime.now().millisecondsSinceEpoch - 1000,
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
  String? pairingPin;

  /// Lobby filters are deliberately process memory only: they survive screen
  /// rebuilds for this session, but are not written to disk.
  String? lobbyFilterCapital;
  String? lobbyFilterDistrict;
  String? lobbyFilterLevel;

  void setLobbyCapitalFilter(String? value) {
    lobbyFilterCapital = value;
    lobbyFilterDistrict = null;
    lobbyFilterLevel = null;
    notifyListeners();
  }

  void setLobbyDistrictFilter(String? value) {
    lobbyFilterDistrict = value;
    lobbyFilterLevel = null;
    notifyListeners();
  }

  void setLobbyLevelFilter(String? value) {
    lobbyFilterLevel = value;
    notifyListeners();
  }

  void clearLobbyFilters() {
    lobbyFilterCapital = null;
    lobbyFilterDistrict = null;
    lobbyFilterLevel = null;
    notifyListeners();
  }

  WebSocketChannel? _events;
  StreamSubscription<dynamic>? _eventsSub;
  int _eventCycle = 0;
  String? _roomAddress;
  String? _server;
  String? _token;
  String? _fingerprint;

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
      _server = server;
      _token = token;
      _fingerprint = fingerprint;
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
      // The server keeps sessions in memory. After a server restart an already
      // open app holds a stale session id; recover with the saved token instead
      // of leaving the lobby stuck on a 401 until a full app restart.
      if (e is ApiException && e.status == 401 && await _reauthenticate()) {
        try {
          lobby = await api!.lobby();
          error = null;
        } catch (retryError) {
          error = _describe(retryError);
        }
      } else {
        error = _describe(e);
      }
    }
    notifyListeners();
  }

  Future<bool> _reauthenticate() async {
    final server = _server;
    final token = _token;
    if (server == null || token == null) return false;
    try {
      // Keep the same ApiClient: ScanController and pending relays hold a
      // reference to it. Closing it here would leave the scanner publishing
      // through a closed transport for the rest of the session.
      final client =
          api ?? ApiClient(baseUrl: server, fingerprint: _fingerprint);
      final auth = await client.authenticate(token: token);
      api = client;
      session = auth;
      return true;
    } catch (_) {
      return false;
    }
  }

  /// Joins the room (idempotent server-side) and subscribes to its events.
  Future<bool> enterRoom(String address) async {
    final client = api;
    if (client == null) {
      // Offline demo state already holds a room; nothing to join.
      return room != null;
    }
    if (_roomAddress == address && room != null) {
      // Already joined and subscribed; re-entering would close the socket and
      // now also close the room.
      return true;
    }
    busy = true;
    error = null;
    notifyListeners();
    try {
      RoomEntry entry;
      try {
        entry = await client.enter(address);
      } on ApiException catch (e) {
        if (e.status != 401 || !await _reauthenticate()) rethrow;
        entry = await api!.enter(address);
      }
      room = entry.room;
      _roomAddress = address;
      await _subscribe(client, address, entry.ticket);
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
      if (e is ApiException && e.status == 401 && await _reauthenticate()) {
        try {
          await _resubscribe(api!, address);
          room = await api!.room(address);
          error = null;
        } catch (retryError) {
          error = _describe(retryError);
        }
      } else {
        error = _describe(e);
      }
    }
    notifyListeners();
  }

  /// Creates the room, or reuses it if it already exists, then lights it.
  /// Both "already exists" and "already lit" are fine: the host is re-entering.
  Future<String?> createAndLight(String address, {String? pin}) async {
    final client = api;
    if (client == null) return '离线演示模式不能建房';
    busy = true;
    error = null;
    notifyListeners();
    try {
      final entry = await client.openRoom(address, pin: pin);
      room = entry.room;
      _roomAddress = address;
      await _subscribe(client, address, entry.ticket);
      pairingPin = pin;
      notifyListeners();
      return null;
    } catch (e) {
      error = _describe(e);
      return error;
    } finally {
      busy = false;
      notifyListeners();
    }
  }

  Future<String?> startByPin(String pin) async {
    final client = api;
    if (client == null) return null;
    try {
      final started = await client.startByPin(pin);
      // Caller enters the returned address exactly once. Entering here too
      // would close the first event socket and delete the room server-side.
      pairingPin = started.created ? pin : null;
      notifyListeners();
      return started.address;
    } catch (e) {
      error = _describe(e);
      notifyListeners();
      return null;
    }
  }

  Future<String?> resolvePin(String pin) async {
    final client = api;
    if (client == null) return null;
    try {
      return await client.resolvePin(pin);
    } catch (e) {
      error = _describe(e);
      notifyListeners();
      return null;
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

  Future<void> _subscribe(
    ApiClient client,
    String address,
    String ticket,
  ) async {
    final channel = await client.openEventsWithTicket(address, ticket);
    await _listen(channel);
  }

  Future<void> _resubscribe(ApiClient client, String address) async {
    final channel = await client.openEvents(address);
    await _listen(channel);
  }

  Future<void> _listen(WebSocketChannel channel) async {
    final cycle = ++_eventCycle;
    await _closeEvents();
    if (cycle != _eventCycle) {
      await channel.sink.close();
      return;
    }
    _events = channel;
    _eventsSub = channel.stream.listen(
      _onRoomEvent,
      onError: (Object _) {},
      cancelOnError: false,
    );
  }

  /// Relay events already carry the relay, so the hot path does not need a
  /// follow-up GET /v1/room round trip. Other events still resync fully.
  @visibleForTesting
  void handleRoomEventForTest(String raw) => _onRoomEvent(raw);

  void _onRoomEvent(dynamic raw) {
    try {
      final body = jsonDecode(raw as String) as Map<String, dynamic>;
      if (body['type'] == 'relay_seen') {
        final id = body['relay'] as String?;
        final createdAtMs = body['created_at_ms'] as int?;
        final current = room;
        if (current != null && id != null && createdAtMs != null) {
          var found = false;
          final relays = current.relays
              .map((entry) {
                if (entry.id != id) return entry;
                found = true;
                return entry.copyWith(createdAtMs: createdAtMs);
              })
              .toList(growable: false);
          if (found) {
            room = current.copyWith(relays: relays);
            notifyListeners();
            return;
          }
        }
      }
      if (body['type'] == 'relay_published') {
        final relay = Relay.fromJson(body['relay'] as Map<String, dynamic>);
        final current = room;
        if (current != null) {
          room = current.copyWith(
            relays: [
              ...current.relays.where((entry) => entry.id != relay.id),
              relay,
            ],
          );
          notifyListeners();
          return;
        }
      }
    } catch (_) {
      // Fall through to a full resync.
    }
    refreshRoom();
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
    _server = null;
    _token = null;
    _fingerprint = null;
    error = null;
    notifyListeners();
  }

  Future<void> _unsubscribe() async {
    _eventCycle++;
    await _closeEvents();
  }

  Future<void> _closeEvents() async {
    // Snapshot both references before yielding; a concurrent _listen may
    // install a fresh channel while cancellation is in flight.
    final subscription = _eventsSub;
    final events = _events;
    _eventsSub = null;
    _events = null;
    await subscription?.cancel();
    await events?.sink.close();
  }

  String _describe(Object error) {
    if (error is ApiException) {
      if (error.status == 401) return '认证失败：token 不对或已失效';
      if (error.status == 429) return '尝试失败次数过多，账户已锁定 60 秒，请稍后再试';
      if (error.status == 503) return '服务器房间已满，请稍后再试';
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
