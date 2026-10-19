import 'dart:convert';
import 'dart:js_interop';

import 'scanner_types.dart';

@JS('qrRaceStartCameraScan')
external JSPromise<JSAny?> _startCamera(JSFunction onJson, JSFunction onError);

@JS('qrRaceStopCameraScan')
external void _stopCamera();

/// Web scanner: camera frames never enter Dart. `camera_scan.js` grabs a frame,
/// converts it to grayscale, scans it with the wasm core and sends back only the
/// hits JSON. Dart polls that latest result like any other backend.
class WebScannerBackend implements ScannerBackend {
  List<ScanHit> _hits = const [];
  int _width = 0;
  int _height = 0;
  int _frames = 0;
  double _scanMs = 0;
  String? _error;
  bool _running = false;

  @override
  void start({
    required int camera,
    required int width,
    required int height,
    required int fps,
  }) {
    _running = true;
    _error = null;
    final onJson = ((JSString payload) => _accept(payload.toDart)).toJS;
    final onError = ((JSString message) {
      _error = message.toDart;
      _running = false;
    }).toJS;

    _startCamera(onJson, onError).toDart.then(
      (_) {},
      onError: (Object error) {
        _error = '无法启动摄像头：$error';
        _running = false;
      },
    );
  }

  void _accept(String payload) {
    try {
      final json = jsonDecode(payload) as Map<String, dynamic>;
      final error = json['error'] as String?;
      if (error != null) _error = error;
      _hits = (json['hits'] as List<dynamic>? ?? const [])
          .map((hit) => ScanHit.fromJson(hit as Map<String, dynamic>))
          .toList(growable: false);
      _width = json['width'] as int? ?? _width;
      _height = json['height'] as int? ?? _height;
      _scanMs = (json['scan_ms'] as num?)?.toDouble() ?? 0;
      _frames++;
      _running = true;
    } catch (error) {
      _error = '解析扫码结果失败：$error';
    }
  }

  @override
  void stop() {
    _stopCamera();
    _running = false;
  }

  @override
  ScannerSnapshot poll() => ScannerSnapshot(
        running: _running && _error == null,
        error: _error,
        width: _width,
        height: _height,
        frames: _frames,
        scanMs: _scanMs,
        hits: _hits,
        previewSeq: 0,
        preview: null,
        source: 'web camera',
      );

  @override
  void dispose() => stop();
}

ScannerBackend createScannerBackend() => WebScannerBackend();
