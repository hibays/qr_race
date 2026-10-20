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
    this.latestHits = const [],
    this.latestSeq = 0,
    required this.previewSeq,
    required this.preview,
    required this.source,
    this.detections = const [],
    this.wechatRuns = 0,
    this.wechatMs = 0,
    this.lastTier,
    this.p95Ms = 0,
    this.maxMs = 0,
    this.dropped = 0,
  });

  factory ScannerSnapshot.fromJson(
    Map<String, dynamic> json, {
    Uint8List? preview,
  }) {
    final stats = json['stats'] as Map<String, dynamic>? ?? const {};
    final hits = (json['hits'] as List<dynamic>? ?? const [])
        .map((hit) => ScanHit.fromJson(hit as Map<String, dynamic>))
        .toList();
    final latestHits = (json['latest'] as List<dynamic>? ?? const [])
        .map((hit) => ScanHit.fromJson(hit as Map<String, dynamic>))
        .toList(growable: false);
    return ScannerSnapshot(
      running: json['running'] as bool? ?? false,
      error: json['error'] as String?,
      width: json['width'] as int? ?? 0,
      height: json['height'] as int? ?? 0,
      rotation: json['rotation'] as int? ?? 0,
      frames: stats['frames'] as int? ?? 0,
      scanMs: (stats['scan_ms_last'] as num?)?.toDouble() ?? 0,
      hits: hits,
      latestHits: latestHits,
      latestSeq: json['latest_seq'] as int? ?? 0,
      previewSeq: json['preview_seq'] as int? ?? 0,
      preview: preview,
      source: json['source'] as String?,
      detections: (json['detections'] as List<dynamic>? ?? const [])
          .map((box) => _decodePoints(box))
          .where((box) => box.length >= 4)
          .toList(growable: false),
      wechatRuns: stats['wechat_runs'] as int? ?? 0,
      wechatMs: (stats['wechat_ms_last'] as num?)?.toDouble() ?? 0,
      lastTier: stats['last_tier'] as String?,
      p95Ms: (stats['p95_ms'] as num?)?.toDouble() ?? 0,
      maxMs: (stats['max_ms'] as num?)?.toDouble() ?? 0,
      dropped: stats['dropped'] as int? ?? 0,
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

  /// Hits from the newest frame, including repeated content whose corners
  /// moved. The overlay uses these; [hits] remains the deduplicated list.
  final List<ScanHit> latestHits;
  final int latestSeq;
  final int previewSeq;
  final Uint8List? preview;
  final String? source;
  /// Latest WeChat detector candidate boxes (four corners).
  final List<List<(double, double)>> detections;
  final int wechatRuns;
  final double wechatMs;
  final String? lastTier;
  final double p95Ms;
  final double maxMs;
  final int dropped;
}

class ScanHit {
  const ScanHit({
    required this.text,
    required this.raw,
    required this.atMs,
    this.points = const [],
    this.lastSeq = 0,
  });

  factory ScanHit.fromJson(Map<String, dynamic> json) => ScanHit(
        text: json['text'] as String? ?? '',
        raw: _decodeBase64(json['raw_b64']),
        atMs: json['at_ms'] as int? ?? 0,
        points: _decodePoints(json['points']),
        lastSeq: json['last_seq'] as int? ?? 0,
      );

  final String text;
  final Uint8List raw;
  final int atMs;

  /// Frame sequence that last contained this hit.
  final int lastSeq;
  /// Four source-frame corners, [top-left, top-right, bottom-right, bottom-left].
  final List<(double, double)> points;

  String get key => base64Encode(raw);
}

/// The platform scanner session, behind an interface so tests never load a
/// native library and the web build has a real implementation.
abstract class ScannerBackend {
  void start({
    required int camera,
    required int width,
    required int height,
    required int fps,
    String backend = 'hybrid',
  });

  void stop();

  /// Android camera preview texture id; null when the platform uses JPEG
  /// previews or has no texture preview.
  int? get previewTextureId => null;

  ScannerSnapshot poll();

  void dispose();
}


Uint8List _decodeBase64(Object? value) {
  if (value is! String || value.isEmpty) return Uint8List(0);
  try {
    return base64Decode(value);
  } on FormatException {
    return Uint8List(0);
  }
}

List<(double, double)> _decodePoints(Object? value) {
  if (value is! List) return const [];
  final points = <(double, double)>[];
  for (final item in value) {
    if (item is! List || item.length < 2) continue;
    final x = item[0];
    final y = item[1];
    if (x is num && y is num) points.add((x.toDouble(), y.toDouble()));
  }
  return List.unmodifiable(points);
}

