import 'dart:async';

import 'package:flutter/foundation.dart';

import '../api/api_client.dart';
import '../native/scanner_backend.dart';

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
  Timer? _timer;

  bool isPublished(ScanHit hit) => _published.contains(hit.key);

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
    } catch (error) {
      fatalError = error.toString();
      _timer?.cancel();
      _timer = null;
    }
    notifyListeners();
  }

  /// Publishes one hit's exact bytes as a relay. Returns the relay id.
  Future<String?> publish(
    ApiClient api,
    String address,
    ScanHit hit,
  ) async {
    publishError = null;
    try {
      final relay = await api.publishRelay(address, payload: hit.raw);
      _published.add(hit.key);
      notifyListeners();
      return relay.id;
    } catch (error) {
      publishError = error is ApiException ? error.message : error.toString();
      notifyListeners();
      return null;
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
}) {
  return ScannerSnapshot(
    running: true,
    error: null,
    width: 1280,
    height: 720,
    frames: 42,
    scanMs: 1.8,
    hits: hits,
    previewSeq: previewSeq,
    preview: preview,
    source: 'fake camera',
  );
}
