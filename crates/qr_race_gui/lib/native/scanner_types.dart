import 'dart:convert';
import 'dart:typed_data';

/// One scanner state poll from the platform session.
class ScannerSnapshot {
  const ScannerSnapshot({
    required this.running,
    required this.error,
    required this.width,
    required this.height,
    required this.rotation,
    required this.frames,
    required this.scanMs,
    required this.hits,
    required this.previewSeq,
    required this.preview,
    required this.source,
    this.wechatRuns = 0,
    this.wechatMs = 0,
    this.lastTier,
  });

  factory ScannerSnapshot.fromJson(
    Map<String, dynamic> json, {
    Uint8List? preview,
  }) {
    final stats = json['stats'] as Map<String, dynamic>? ?? const {};
    final hits = (json['hits'] as List<dynamic>? ?? const [])
        .map((hit) => ScanHit.fromJson(hit as Map<String, dynamic>))
        .toList();
    return ScannerSnapshot(
      running: json['running'] as bool? ?? false,
      error: json['error'] as String?,
      width: json['width'] as int? ?? 0,
      height: json['height'] as int? ?? 0,
      rotation: json['rotation'] as int? ?? 0,
      frames: stats['frames'] as int? ?? 0,
      scanMs: (stats['scan_ms_last'] as num?)?.toDouble() ?? 0,
      hits: hits,
      previewSeq: json['preview_seq'] as int? ?? 0,
      preview: preview,
      source: json['source'] as String?,
      wechatRuns: stats['wechat_runs'] as int? ?? 0,
      wechatMs: (stats['wechat_ms_last'] as num?)?.toDouble() ?? 0,
      lastTier: stats['last_tier'] as String?,
    );
  }

  final bool running;
  final String? error;
  final int width;
  final int height;

  /// Sensor rotation in degrees; the preview is rotated on the GPU.
  final int rotation;
  final int frames;
  final double scanMs;
  final List<ScanHit> hits;
  final int previewSeq;
  final Uint8List? preview;
  final String? source;
  final int wechatRuns;
  final double wechatMs;
  final String? lastTier;
}

class ScanHit {
  const ScanHit({required this.text, required this.raw, required this.atMs});

  factory ScanHit.fromJson(Map<String, dynamic> json) => ScanHit(
        text: json['text'] as String? ?? '',
        raw: base64Decode(json['raw_b64'] as String),
        atMs: json['at_ms'] as int? ?? 0,
      );

  final String text;
  final Uint8List raw;
  final int atMs;

  String get key => base64Encode(raw);
}

/// The platform scanner session, behind an interface so tests never load a
/// native library and the web build has a real (if unsupported) implementation.
abstract class ScannerBackend {
  void start({
    required int camera,
    required int width,
    required int height,
    required int fps,
  });

  void stop();

  ScannerSnapshot poll();

  void dispose();
}
