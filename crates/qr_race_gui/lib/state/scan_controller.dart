import 'dart:async';

import 'package:crypto/crypto.dart';
import 'package:flutter/foundation.dart';

import '../api/api_client.dart';
import '../native/scanner_backend.dart';
import 'relay_slot.dart';

/// Owns the native scanner session and turns its polls into UI state.
class ScanController extends ChangeNotifier {
  ScanController({
    required this.backend,
    this.camera = 0,
    this.width = 2560,
    this.height = 1440,
    this.fps = 30,
    this.backendName = 'hybrid',
    this.interval = const Duration(milliseconds: 80),
  });

  final ScannerBackend backend;
  final int camera;
  final int width;
  final int height;
  final int fps;
  String backendName;
  final Duration interval;

  ScannerSnapshot? snapshot;
  String? fatalError;
  String? publishError;
  final Set<String> _published = {};
  final Set<String> _publishedTexts = {};
  final Map<String, String> _lastBySlot = {};
  final Map<String, String> _hashBySlot = {};
  final Map<String, String> _keyBySlot = {};
  final Map<String, DateTime> _lastRefresh = {};
  final Map<String, DateTime> _retryAt = {};
  final Set<String> _queuedSlots = {};
  final List<_PendingRelay> _queue = [];
  Timer? _timer;
  ApiClient? _api;
  String? _address;
  bool _draining = false;

  bool isPublished(ScanHit hit) => _published.contains(hit.key);

  /// True while a hot relay is waiting for, or inside, the publish HTTP call.
  bool get isBroadcasting => _draining || _queue.isNotEmpty;

  bool get autoPublishEnabled => _api != null;

  int? get previewTextureId => backend.previewTextureId;

  /// Publishes new/changed prefixed payloads automatically. Scanning a code
  /// whose prefix was seen before with different bytes updates that slot.
  void enableAutoPublish(ApiClient api, String address) {
    _api = api;
    _address = address;
  }

  void start() {
    _startBackend();
    _timer = Timer.periodic(interval, (_) => _poll());
  }

  /// Switches decoder backend without tearing down the widget. The native
  /// `qr_scanner_start_backend` stops only the Rust worker; the Android
  /// Camera2 session stays open, so switching backends does not race a new
  /// camera open against the old stopCapture.
  void restartCamera() {
    snapshot = null;
    _startBackend();
    notifyListeners();
  }

  void restart(String name) {
    if (name == backendName) return;
    backendName = name;
    snapshot = null;
    _startBackend();
    notifyListeners();
  }

  void _startBackend() {
    try {
      backend.start(
        camera: camera,
        width: width,
        height: height,
        fps: fps,
        backend: backendName,
      );
    } catch (error) {
      fatalError = error.toString();
      notifyListeners();
    }
  }

  void _poll() {
    try {
      final next = backend.poll();
      snapshot = next;
      _collect(next);
    } catch (error) {
      fatalError = error.toString();
      _timer?.cancel();
      _timer = null;
    }
    notifyListeners();
  }

  void _collect(ScannerSnapshot snapshot) {
    final api = _api;
    final address = _address;
    if (api == null || address == null) return;
    final now = DateTime.now();
    _pruneLocal(now);
    for (final hit in snapshot.latestHits) {
      final slot = relaySlot(hit.text);
      if (_publishedTexts.contains(hit.text)) {
        if (_queuedSlots.contains(slot)) continue;
        final last = _lastRefresh[slot];
        final hash = _hashBySlot[slot];
        if (last != null &&
            hash != null &&
            now.difference(last) > const Duration(seconds: 4)) {
          _queuedSlots.add(slot);
          _queue.add(_PendingRelay(api, address, slot, hit, hash: hash));
        }
        continue;
      }
      if (_lastBySlot[slot] == hit.text) continue;
      if (_queuedSlots.contains(slot)) continue;
      final retry = _retryAt[slot];
      if (retry != null && now.isBefore(retry)) continue;
      _queuedSlots.add(slot);
      // A newly seen hot code jumps ahead of refresh heartbeats.
      _queue.insert(0, _PendingRelay(api, address, slot, hit));
    }
    unawaited(_drain());
  }

  void _pruneLocal(DateTime now) {
    final stale = _lastRefresh.entries
        .where(
          (entry) => now.difference(entry.value) > const Duration(seconds: 40),
        )
        .map((entry) => entry.key)
        .toList();
    for (final slot in stale) {
      _forgetSlot(slot);
    }
    if (_lastRefresh.length <= 64) return;
    final oldest = _lastRefresh.entries.toList()
      ..sort((a, b) => a.value.compareTo(b.value));
    for (final entry in oldest.take(_lastRefresh.length - 64)) {
      _forgetSlot(entry.key);
    }
  }

  void _forgetSlot(String slot) {
    final text = _lastBySlot.remove(slot);
    final key = _keyBySlot.remove(slot);
    _hashBySlot.remove(slot);
    _lastRefresh.remove(slot);
    _retryAt.remove(slot);
    _queue.removeWhere((pending) => pending.slot == slot);
    _queuedSlots.remove(slot);
    if (text != null) _publishedTexts.remove(text);
    if (key != null) _published.remove(key);
  }

  Future<void> _drain() async {
    if (_draining) return;
    _draining = true;
    try {
      while (_queue.isNotEmpty) {
        final pending = _queue.removeAt(0);
        try {
          if (pending.hash == null) {
            await pending.api.publishRelay(
              pending.address,
              payload: pending.hit.raw,
            );
            final hash = sha256.convert(pending.hit.raw).toString();
            _hashBySlot[pending.slot] = hash;
            _keyBySlot[pending.slot] = pending.hit.key;
            _published.add(pending.hit.key);
            _publishedTexts.add(pending.hit.text);
          } else {
            await pending.api.publishHeartbeat(pending.address, pending.hash!);
          }
          _lastBySlot[pending.slot] = pending.hit.text;
          _lastRefresh[pending.slot] = DateTime.now();
          _retryAt.remove(pending.slot);
          publishError = null;
        } catch (error) {
          publishError = error is ApiException
              ? error.message
              : error.toString();
          // A server restart drops memory-only relays. Fall back to a full
          // upsert once instead of leaving the code dead until it is rescanned.
          if (pending.hash != null &&
              error is ApiException &&
              error.status == 404) {
            _hashBySlot.remove(pending.slot);
            _publishedTexts.remove(pending.hit.text);
            _keyBySlot.remove(pending.slot);
            _published.remove(pending.hit.key);
            _queue.insert(
              0,
              _PendingRelay(
                pending.api,
                pending.address,
                pending.slot,
                pending.hit,
              ),
            );
          } else {
            _retryAt[pending.slot] = DateTime.now().add(
              const Duration(seconds: 2),
            );
          }
        } finally {
          _queuedSlots.remove(pending.slot);
        }
        notifyListeners();
      }
    } finally {
      _draining = false;
    }
  }

  @override
  void dispose() {
    _timer?.cancel();
    _timer = null;
    backend.stop();
    backend.dispose();
    super.dispose();
  }
}

class _PendingRelay {
  _PendingRelay(this.api, this.address, this.slot, this.hit, {this.hash});

  final ApiClient api;
  final String address;
  final String slot;
  final ScanHit hit;

  /// Non-null means a hash-only heartbeat; null means a full upsert.
  final String? hash;
}

/// Test/demo backend: hand it snapshots, no native library involved.
class FakeScannerBackend implements ScannerBackend {
  FakeScannerBackend({List<ScannerSnapshot>? snapshots})
    : _snapshots = snapshots ?? const [];

  final List<ScannerSnapshot> _snapshots;
  int _index = 0;
  bool started = false;
  bool disposed = false;

  @override
  int? get previewTextureId => null;

  @override
  void start({
    required int camera,
    required int width,
    required int height,
    required int fps,
    String backend = 'hybrid',
  }) {
    started = true;
  }

  @override
  void stop() => started = false;

  @override
  ScannerSnapshot poll() {
    if (_snapshots.isEmpty) {
      return ScannerSnapshot(
        running: started,
        error: null,
        width: 1280,
        height: 720,
        rotation: 0,
        frames: 0,
        scanMs: 0,
        hits: const [],
        previewSeq: 0,
        preview: null,
        source: 'fake',
      );
    }
    final snapshot = _snapshots[_index.clamp(0, _snapshots.length - 1)];
    _index++;
    return snapshot;
  }

  @override
  void dispose() {
    disposed = true;
    started = false;
  }
}

/// A ready-made snapshot for tests and the demo build.
ScannerSnapshot fakeSnapshot({
  List<ScanHit> hits = const [],
  List<ScanHit>? latestHits,
  Uint8List? preview,
  int previewSeq = 1,
  int rotation = 0,
}) {
  return ScannerSnapshot(
    running: true,
    error: null,
    width: 1280,
    height: 720,
    rotation: rotation,
    frames: 42,
    scanMs: 1.8,
    hits: hits,
    latestHits: latestHits ?? hits,
    previewSeq: previewSeq,
    preview: preview,
    source: 'fake camera',
  );
}
