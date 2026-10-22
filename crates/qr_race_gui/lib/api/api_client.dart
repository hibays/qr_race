import 'dart:convert';

import 'package:http/http.dart' as http;
import 'package:web_socket_channel/web_socket_channel.dart';

import 'cert_fingerprint.dart';
import 'http_transport.dart' as transport;

/// REST + WebSocket client for the qr_race server. One instance per session.
class ApiClient {
  ApiClient({
    required String baseUrl,
    http.Client? httpClient,
    String? fingerprint,
  }) : baseUrl = normalizeBaseUrl(baseUrl),
       _fingerprint = normalizeFingerprint(fingerprint),
       _http =
           httpClient ??
           transport.createHttpClient(normalizeFingerprint(fingerprint));

  final String baseUrl;
  String? get fingerprint => _fingerprint;
  final http.Client _http;
  final String? _fingerprint;
  String? sessionId;

  static String normalizeBaseUrl(String value) {
    var url = value.trim();
    if (!url.startsWith('http://') && !url.startsWith('https://')) {
      url = 'http://$url';
    }
    while (url.endsWith('/')) {
      url = url.substring(0, url.length - 1);
    }
    return url;
  }

  Map<String, String> get _headers => {
    'content-type': 'application/json',
    if (sessionId != null) 'authorization': 'Bearer $sessionId',
  };

  Future<AuthSession> authenticate({required String token}) async {
    final response = await _http.post(
      Uri.parse('$baseUrl/v1/auth'),
      headers: {'content-type': 'application/json'},
      body: jsonEncode({'token': token.trim()}),
    );
    final body = _decode(response);
    final session = AuthSession(
      sessionId: body['session_id'] as String,
      userId: body['user_id'] as String,
      expiresAtMs: body['expires_at_ms'] as int,
      admin: body['admin'] as bool? ?? false,
    );
    sessionId = session.sessionId;
    return session;
  }

  Future<List<LobbyRoom>> lobby() async {
    final body = await _get('/v1/lobby');
    final rooms = body['rooms'] as List<dynamic>? ?? const [];
    return rooms
        .map((room) => LobbyRoom.fromJson(room as Map<String, dynamic>))
        .toList();
  }

  Future<RoomView> room(String address) async {
    final body = await _get('/v1/room', query: {'address': address});
    return RoomView.fromJson(body);
  }

  Future<void> join(String address) async {
    await _post('/v1/room/join', query: {'address': address});
  }

  /// One round trip for join + room snapshot + WebSocket ticket.
  Future<RoomEntry> enter(String address) async {
    final body = await _post('/v1/room/enter', query: {'address': address});
    return RoomEntry(
      room: RoomView.fromJson(body['room'] as Map<String, dynamic>),
      ticket: body['ticket'] as String,
    );
  }

  /// One round trip for create/light/join + snapshot + WebSocket ticket.
  Future<RoomEntry> openRoom(String address, {String? pin}) async {
    final body = await _post(
      '/v1/rooms/open',
      body: {'address': address, 'pin': ?pin},
    );
    return RoomEntry(
      room: RoomView.fromJson(body['room'] as Map<String, dynamic>),
      ticket: body['ticket'] as String,
    );
  }

  /// Host-only: mints a fresh member identity for this room. The guest is a
  /// real server account, but its token is only shared in the invite.
  Future<GuestAccount> createGuest(String address) async {
    final body = await _post('/v1/room/guest', query: {'address': address});
    return GuestAccount(
      userId: body['user_id'] as String,
      token: body['token'] as String,
    );
  }

  Future<void> leave(String address) async {
    await _post('/v1/room/leave', query: {'address': address});
  }

  Future<void> createRoom(String address, {String? pin}) async {
    await _post('/v1/rooms', body: {'address': address, 'pin': ?pin});
  }

  Future<void> setPin(String address, String pin) async {
    await _post('/v1/rooms/pin', body: {'address': address, 'pin': pin});
  }

  Future<String> resolvePin(String pin) async {
    final body = await _get('/v1/room/by-pin', query: {'pin': pin});
    return body['address'] as String;
  }

  Future<({String address, bool created})> startByPin(String pin) async {
    final body = await _post('/v1/rooms/start-by-pin', body: {'pin': pin});
    return (
      address: body['address'] as String,
      created: body['created'] as bool? ?? false,
    );
  }

  Future<void> lightRoom(String address) async {
    await _post('/v1/room/light', query: {'address': address});
  }

  Future<void> promote(String address, String target) async {
    await _post(
      '/v1/room/promote',
      query: {'address': address},
      body: {'target': target},
    );
  }

  Future<void> demote(String address, String target) async {
    await _post(
      '/v1/room/demote',
      query: {'address': address},
      body: {'target': target},
    );
  }

  Future<Relay> publishRelay(
    String address, {
    required List<int> payload,
    String kind = 'text',
  }) async {
    final body = await _post(
      '/v1/room/relay',
      query: {'address': address},
      body: {'kind': kind, 'payload_b64': base64Encode(payload)},
    );
    return Relay.fromJson(body);
  }

  Future<void> publishHeartbeat(String address, String hash) async {
    await _post(
      '/v1/room/relay/heartbeat',
      query: {'address': address},
      body: {'hash': hash},
    );
  }

  /// Gets a single-use ticket, then opens the room event socket.
  Future<WebSocketChannel> openEvents(String address) async {
    final body = await _post('/v1/ws-ticket');
    return openEventsWithTicket(address, body['ticket'] as String);
  }

  Future<WebSocketChannel> openEventsWithTicket(
    String address,
    String ticket,
  ) async {
    final wsBase = baseUrl.replaceFirst(RegExp('^http'), 'ws');
    final uri = Uri.parse('$wsBase/v1/room/events')
        .replace(queryParameters: {'address': address, 'ticket': ticket});
    return transport.openWebSocket(uri, _fingerprint);
  }

  Future<Map<String, dynamic>> _get(
    String path, {
    Map<String, String>? query,
  }) async {
    final uri = Uri.parse('$baseUrl$path').replace(queryParameters: query);
    final response = await _http.get(uri, headers: _headers);
    return _decode(response);
  }

  Future<Map<String, dynamic>> _post(
    String path, {
    Map<String, String>? query,
    Map<String, dynamic>? body,
  }) async {
    final uri = Uri.parse('$baseUrl$path').replace(queryParameters: query);
    final response = await _http.post(
      uri,
      headers: _headers,
      body: body == null ? null : jsonEncode(body),
    );
    return _decode(response);
  }

  Map<String, dynamic> _decode(http.Response response) {
    final text = utf8.decode(response.bodyBytes, allowMalformed: true);
    Map<String, dynamic>? json;
    if (text.isNotEmpty) {
      try {
        final decoded = jsonDecode(text);
        if (decoded is Map<String, dynamic>) {
          json = decoded;
        }
      } catch (_) {
        // Fall through to a status-based error.
      }
    }
    if (response.statusCode >= 400) {
      final message = json?['error'] as String? ?? text;
      throw ApiException(response.statusCode, message);
    }
    return json ?? <String, dynamic>{};
  }

  void close() {
    _http.close();
  }
}

class ApiException implements Exception {
  ApiException(this.status, this.message);

  final int status;
  final String message;

  @override
  String toString() => message.isEmpty ? 'HTTP $status' : message;
}

class RoomEntry {
  const RoomEntry({required this.room, required this.ticket});

  final RoomView room;
  final String ticket;
}

class GuestAccount {
  const GuestAccount({required this.userId, required this.token});

  final String userId;
  final String token;
}

class AuthSession {
  const AuthSession({
    required this.sessionId,
    required this.userId,
    required this.expiresAtMs,
    this.admin = false,
  });

  final String sessionId;
  final String userId;
  final int expiresAtMs;
  final bool admin;
}

class LobbyRoom {
  const LobbyRoom({
    required this.label,
    required this.host,
    required this.memberCount,
  });

  factory LobbyRoom.fromJson(Map<String, dynamic> json) => LobbyRoom(
    label: json['label'] as String,
    host: json['host'] as String,
    memberCount: json['member_count'] as int,
  );

  final String label;
  final String host;
  final int memberCount;
}

class RoomView {
  const RoomView({
    required this.label,
    required this.lit,
    required this.host,
    required this.members,
    required this.relays,
  });

  factory RoomView.fromJson(Map<String, dynamic> json) => RoomView(
    label: json['label'] as String,
    lit: json['lit'] as bool,
    host: json['host'] as String,
    members: (json['members'] as List<dynamic>? ?? const [])
        .map((member) => RoomMember.fromJson(member as Map<String, dynamic>))
        .toList(),
    relays: (json['relays'] as List<dynamic>? ?? const [])
        .map((relay) => Relay.fromJson(relay as Map<String, dynamic>))
        .toList(),
  );

  final String label;
  final bool lit;
  final String host;
  final List<RoomMember> members;
  final List<Relay> relays;

  RoomView copyWith({List<Relay>? relays}) => RoomView(
    label: label,
    lit: lit,
    host: host,
    members: members,
    relays: relays ?? this.relays,
  );
}

class RoomMember {
  const RoomMember({
    required this.user,
    required this.role,
    required this.lastRelay,
  });

  factory RoomMember.fromJson(Map<String, dynamic> json) => RoomMember(
    user: json['user'] as String,
    role: json['role'] as String,
    lastRelay: json['last_relay'] as String?,
  );

  final String user;
  final String role;
  final String? lastRelay;

  bool get canPublish => role == 'host' || role == 'co_host';
  bool get hasRelayed => lastRelay != null;

  String get roleLabel => switch (role) {
    'host' => '房主',
    'co_host' => '副房主',
    _ => '成员',
  };
}

class Relay {
  const Relay({
    required this.id,
    required this.publisher,
    required this.kind,
    required this.payloadB64,
    required this.createdAtMs,
    this.hash,
  });

  factory Relay.fromJson(Map<String, dynamic> json) => Relay(
    id: json['id'] as String,
    publisher: json['publisher'] as String,
    kind: json['kind'] as String? ?? 'text',
    payloadB64: json['payload_b64'] as String,
    createdAtMs: json['created_at_ms'] as int,
    hash: json['hash'] as String?,
  );

  final String id;
  final String publisher;
  final String kind;
  final String payloadB64;
  final int createdAtMs;
  final String? hash;

  List<int> get payloadBytes => base64Decode(payloadB64);

  Relay copyWith({int? createdAtMs}) => Relay(
    id: id,
    publisher: publisher,
    kind: kind,
    payloadB64: payloadB64,
    createdAtMs: createdAtMs ?? this.createdAtMs,
    hash: hash,
  );

  /// Best-effort display text; the bytes are the authoritative payload.
  String get text => utf8.decode(payloadBytes, allowMalformed: true);

  String get timeLabel {
    final time = DateTime.fromMillisecondsSinceEpoch(createdAtMs).toLocal();
    final hour = time.hour.toString().padLeft(2, '0');
    final minute = time.minute.toString().padLeft(2, '0');
    return '$hour:$minute';
  }
}
