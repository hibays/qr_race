import 'dart:async';

import 'package:flutter/foundation.dart';

import '../api/api_client.dart';
import '../native/scanner_backend.dart';
import 'relay_slot.dart';

/// Owns the native scanner session and turns its polls into UI state.
class ScanController extends ChangeNotifier {
  ScanController({
    required this.backend,
    this.camera = 0,
    this.width = 1280,
    this.height = 720,
    this.fps = 30,
    this.interval = const Duration(milliseconds: 120),
  });

  final ScannerBackend backend;
  final int camera;
  final int width;
  final int height;
  final int fps;
  final Duration interval;

  ScannerSnapshot? snapshot;
  String? fatalError;
  String? publishError;
  final Set<String> _published = {};
  final Set<String> _publishedTexts = {};
  final Map<String, String> _lastBySlot = {};
  final Map<String, DateTime> _retryAt = {};
  final Set<String> _queuedSlots = {};
  final List<_PendingRelay> _queue = [];
  Timer? _timer;
  ApiClient? _api;
  String? _address;
  bool _draining = false;

  bool isPublished(ScanHit hit) => _published.contains(hit.key);

  bool get autoPublishEnabled => _api != null;

  /// Publishes new/changed prefixed payloads automatically. Scanning a code
  /// whose prefix was seen before with different bytes updates that slot.
  void enableAutoPublish(ApiClient api, String address) {
    _api = api;
    _address = address;
  }

  void start() {
    try {
      backend.start(
        camera: camera,
        width: width,
        height: height,
        fps: fps,
      );
      _timer = Timer.periodic(interval, (_) => _poll());
    } catch (error) {
      fatalError = error.toString();
      notifyListeners();
    }
  }

  void _poll() {
    try {
      final next = backend.poll();
      snapshot = next;
      fatalError = null;
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
    for (final hit in snapshot.hits) {
      if (_publishedTexts.contains(hit.text)) continue;
      final slot = relaySlot(hit.text);
      if (_lastBySlot[slot] == hit.text) continue;
      if (_queuedSlots.contains(slot)) continue;
      final retry = _retryAt[slot];
      if (retry != null && DateTime.now().isBefore(retry)) continue;
      _queuedSlots.add(slot);
      _queue.add(_PendingRelay(api, address, slot, hit));
    }
    unawaited(_drain());
  }

  Future<void> _drain() async {
    if (_draining) return;
    _draining = true;
    try {
      while (_queue.isNotEmpty) {
        final pending = _queue.removeAt(0);
        try {
          await pending.api.publishRelay(
            pending.address,
            payload: pending.hit.raw,
          );
          _lastBySlot[pending.slot] = pending.hit.text;
          _retryAt.remove(pending.slot);
          _published.add(pending.hit.key);
          _publishedTexts.add(pending.hit.text);
          publishError = null;
        } catch (error) {
          publishError =
              error is ApiException ? error.message : error.toString();
          // Back off this slot; another code may still publish fine.
          _retryAt[pending.slot] = DateTime.now().add(const Duration(seconds: 2));
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
  _PendingRelay(this.api, this.address, this.slot, this.hit);

  final ApiClient api;
  final String address;
  final String slot;
  final ScanHit hit;
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
  void start({
    required int camera,
    required int width,
    required int height,
    required int fps,
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
    previewSeq: previewSeq,
    preview: preview,
    source: 'fake camera',
  );
}
