import 'package:flutter/material.dart';

import '../native/camera_permission.dart';
import '../native/scanner_backend.dart';
import '../state/app_state.dart';
import '../state/relay_slot.dart';
import '../state/scan_controller.dart';
import 'open_link_sheet.dart';
import 'theme.dart';

class ScanScreen extends StatefulWidget {
  const ScanScreen({
    super.key,
    required this.state,
    required this.address,
    this.backend,
    this.permission,
  });

  final AppState state;
  final String address;

  /// Tests inject fakes; the app opens the native session and the platform
  /// camera permission gateway.
  final ScannerBackend? backend;
  final CameraPermission? permission;

  @override
  State<ScanScreen> createState() => _ScanScreenState();
}

class _ScanScreenState extends State<ScanScreen> with WidgetsBindingObserver {
  ScanController? _controller;
  late final CameraPermission _permission;
  CameraPermissionStatus? _denied;
  String? _initError;
  String? _permissionError;
  bool _requesting = true;
  String _backendName = 'hybrid';
  bool _listOnly = false;
  bool _paused = false;
  bool _fullFrame = true;

  @override
  void initState() {
    super.initState();
    _permission = widget.permission ?? createCameraPermission();
    WidgetsBinding.instance.addObserver(this);
    _requestPermission();
  }

  Future<void> _requestPermission() async {
    setState(() {
      _requesting = true;
      _permissionError = null;
    });
    CameraPermissionStatus status;
    try {
      status = await _permission.request();
    } catch (error) {
      _permissionError = error.toString();
      status = CameraPermissionStatus.denied;
    }
    if (!mounted) return;
    setState(() {
      _requesting = false;
      _denied = status == CameraPermissionStatus.granted ? null : status;
    });
    if (status == CameraPermissionStatus.granted) {
      _startScanner();
      return;
    }
    await _showPermissionDialog(status);
  }

  void _startScanner() {
    try {
      final controller = ScanController(
        backend: widget.backend ?? createScannerBackend(),
        width: const int.fromEnvironment('SCAN_WIDTH', defaultValue: 1920),
        height: const int.fromEnvironment('SCAN_HEIGHT', defaultValue: 1440),
        backendName: _backendName,
      );
      _controller = controller;
      controller.start();
      final api = widget.state.api;
      if (api != null) {
        controller.enableAutoPublish(api, widget.address);
      }
      setState(() {});
    } catch (error) {
      setState(() => _initError = error.toString());
    }
  }

  Future<void> _showPermissionDialog(CameraPermissionStatus status) async {
    if (!mounted) return;
    final goSettings = await showDialog<bool>(
      context: context,
      barrierDismissible: false,
      builder: (context) => AlertDialog(
        title: const Text('需要相机权限'),
        content: Text(
          status == CameraPermissionStatus.permanentlyDenied
              ? '相机权限已被永久拒绝。请到系统设置 → 应用 → qr_race → 权限 中手动打开相机。'
              : '扫码需要使用相机。请允许相机权限；如果弹窗不再出现，请到系统设置中手动打开。',
        ),
        actions: [
          TextButton(
            onPressed: () => Navigator.pop(context, false),
            child: const Text('取消'),
          ),
          FilledButton(
            onPressed: () => Navigator.pop(context, true),
            child: const Text('去设置'),
          ),
        ],
      ),
    );
    if (goSettings == true) {
      await _permission.openSettings();
    }
  }

  @override
  void dispose() {
    WidgetsBinding.instance.removeObserver(this);
    _controller?.dispose();
    super.dispose();
  }

  @override
  void didChangeAppLifecycleState(AppLifecycleState state) {
    if (state == AppLifecycleState.paused) {
      _paused = true;
    } else if (state == AppLifecycleState.resumed && _paused) {
      _paused = false;
      _controller?.restartCamera();
    }
  }

  void _changeBackend(String backend) {
    if (backend == _backendName) return;
    setState(() => _backendName = backend);
    final controller = _controller;
    if (controller != null) {
      controller.restart(backend);
    } else {
      _startScanner();
    }
  }

  @override
  Widget build(BuildContext context) {
    final controller = _controller;
    return Scaffold(
      extendBodyBehindAppBar: true,
      appBar: AppBar(
        backgroundColor: Colors.black.withValues(alpha: 0.42),
        elevation: 0,
        actions: [
          PopupMenuButton<String>(
            tooltip: '扫码后端',
            initialValue: _backendName,
            onSelected: _changeBackend,
            itemBuilder: (context) => const [
              PopupMenuItem(value: 'hybrid', child: Text('混合：zxing-cpp + CNN')),
              PopupMenuItem(value: 'zxing', child: Text('仅 zxing-cpp')),
              PopupMenuItem(value: 'wechat', child: Text('微信 CNN（自实现）')),
            ],
          ),
          IconButton(
            tooltip: _fullFrame ? '铺满屏幕' : '显示全画幅',
            onPressed: () => setState(() => _fullFrame = !_fullFrame),
            icon: Icon(_fullFrame ? Icons.fullscreen : Icons.fit_screen),
          ),
          IconButton(
            tooltip: _listOnly ? '显示相机' : '只看结果',
            onPressed: () => setState(() => _listOnly = !_listOnly),
            icon: Icon(_listOnly ? Icons.camera_alt_outlined : Icons.view_list_outlined),
          ),
        ],
        title: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            const Text('自动扫码', style: TextStyle(fontSize: 18, fontWeight: FontWeight.w700)),
            Text(widget.address, style: const TextStyle(fontSize: 12, color: Colors.white70)),
          ],
        ),
      ),
      body: _requesting
          ? const Center(
              child: Column(
                mainAxisSize: MainAxisSize.min,
                children: [
                  CircularProgressIndicator(),
                  SizedBox(height: 14),
                  Text('正在申请相机权限…', style: TextStyle(color: muted, fontSize: 12.5)),
                ],
              ),
            )
          : _denied != null
              ? _PermissionBlocked(
                  status: _denied!,
                  error: _permissionError,
                  onRetry: _requestPermission,
                  onSettings: _permission.openSettings,
                )
              : controller == null
                  ? _InitError(message: _initError ?? '原生扫描器不可用')
                  : ListenableBuilder(
                      listenable: controller,
                      builder: (context, _) {
                        if (controller.snapshot == null &&
                            controller.fatalError != null) {
                          return _InitError(message: controller.fatalError!);
                        }
                        return _Body(
                          controller: controller,
                          listOnly: _listOnly,
                          onToggleList: () => setState(() => _listOnly = !_listOnly),
                          fullFrame: _fullFrame,
                          onToggleFullFrame: () => setState(() => _fullFrame = !_fullFrame),
                        );
                      },
                    ),
    );
  }
}

class _PermissionBlocked extends StatelessWidget {
  const _PermissionBlocked({
    required this.status,
    required this.error,
    required this.onRetry,
    required this.onSettings,
  });

  final CameraPermissionStatus status;
  final String? error;
  final Future<void> Function() onRetry;
  final Future<void> Function() onSettings;

  @override
  Widget build(BuildContext context) {
    final message = status == CameraPermissionStatus.permanentlyDenied
        ? '相机权限已被永久拒绝。请在系统设置 → 应用 → qr_race → 权限 中打开相机，然后回到这里重试。'
        : '扫码需要相机权限。你可以再次申请，或到系统设置中手动打开。';
    return Center(
      child: ConstrainedBox(
        constraints: const BoxConstraints(maxWidth: 460),
        child: Padding(
          padding: const EdgeInsets.all(28),
          child: Column(
            mainAxisSize: MainAxisSize.min,
            children: [
              const Icon(Icons.no_photography_outlined, color: warn, size: 42),
              const SizedBox(height: 14),
              const Text(
                '需要相机权限',
                style: TextStyle(fontSize: 18, fontWeight: FontWeight.w600),
              ),
              const SizedBox(height: 8),
              Text(
                message,
                textAlign: TextAlign.center,
                style: const TextStyle(color: muted, height: 1.5, fontSize: 13),
              ),
              if (error != null) ...[
                const SizedBox(height: 8),
                Text(
                  error!,
                  textAlign: TextAlign.center,
                  style: const TextStyle(color: danger, fontSize: 12, height: 1.4),
                ),
              ],
              const SizedBox(height: 18),
              FilledButton(
                onPressed: onRetry,
                child: const Text('重新申请'),
              ),
              const SizedBox(height: 8),
              OutlinedButton(
                onPressed: onSettings,
                child: const Text('打开系统设置'),
              ),
            ],
          ),
        ),
      ),
    );
  }
}

class _Body extends StatelessWidget {
  const _Body({
    required this.controller,
    required this.listOnly,
    required this.onToggleList,
    required this.fullFrame,
    required this.onToggleFullFrame,
  });


  final ScanController controller;
  final bool listOnly;
  final VoidCallback onToggleList;
  final bool fullFrame;
  final VoidCallback onToggleFullFrame;

  @override
  Widget build(BuildContext context) {
    final snapshot = controller.snapshot;
    if (listOnly) {
      return Padding(
        padding: const EdgeInsets.only(top: 92),
        child: _Results(
          controller: controller,
          snapshot: snapshot,
          onToggleList: onToggleList,
        ),
      );
    }
    final media = MediaQuery.sizeOf(context);
    final landscape = media.width > media.height;
    final previewRotation = landscape ? 90 : 90;
    final source = _SourceSize.of(snapshot, previewRotation);
    final previewFit = landscape && !fullFrame ? BoxFit.cover : BoxFit.contain;
    return Stack(
      children: [
        Positioned.fill(
          child: Container(
            color: Colors.black,
            child: controller.previewTextureId != null
                ? ClipRect(
                    child: FittedBox(
                      fit: previewFit,
                      child: RotatedBox(
                        quarterTurns: landscape ? 3 : 0,
                        child: SizedBox(
                          width: landscape ? (snapshot?.width ?? 1920).toDouble() : source.width,
                          height: landscape ? (snapshot?.height ?? 1440).toDouble() : source.height,
                          child: Texture(
                            textureId: controller.previewTextureId!,
                          ),
                        ),
                      ),
                    ),
                  )
                : snapshot?.preview != null
                    ? Image.memory(
                        snapshot!.preview!,
                        fit: previewFit,
                        gaplessPlayback: true,
                      )
                    : const _CameraPlaceholder(),
          ),
        ),
        Positioned.fill(
          child: IgnorePointer(
            child: DecoratedBox(
              decoration: BoxDecoration(
                gradient: LinearGradient(
                  begin: Alignment.topCenter,
                  end: Alignment.bottomCenter,
                  colors: [
                    Colors.black.withValues(alpha: 0.42),
                    Colors.transparent,
                    Colors.black.withValues(alpha: 0.52),
                  ],
                  stops: const [0, 0.45, 1],
                ),
              ),
            ),
          ),
        ),
        Positioned.fill(
          child: _HitOverlay(
            controller: controller,
            snapshot: snapshot,
            rotation: controller.previewTextureId != null
                ? previewRotation
                : (snapshot?.rotation ?? 0),
            contain: controller.previewTextureId != null && previewFit == BoxFit.contain,
          ),
        ),
        Positioned(
          left: 12,
          bottom: 12,
          child: _StatusChip(snapshot: snapshot, backend: controller.backendName),
        ),
        Positioned(
          left: 0,
          right: 0,
          bottom: 0,
          height: MediaQuery.sizeOf(context).height * 0.34,
          child: _Results(
            controller: controller,
            snapshot: snapshot,
            onToggleList: onToggleList,
            floating: true,
          ),
        ),
      ],
    );
  }
}

class _CameraPlaceholder extends StatelessWidget {
  const _CameraPlaceholder();

  @override
  Widget build(BuildContext context) {
    return const Center(
      child: Column(
        mainAxisSize: MainAxisSize.min,
        children: [
          Icon(Icons.videocam_off_outlined, color: muted, size: 34),
          SizedBox(height: 8),
          Text('等待相机预览…', style: TextStyle(color: muted, fontSize: 12.5)),
        ],
      ),
    );
  }
}

class _Results extends StatelessWidget {
  const _Results({
    required this.controller,
    required this.snapshot,
    required this.onToggleList,
    this.floating = false,
  });

  final ScanController controller;
  final ScannerSnapshot? snapshot;
  final VoidCallback onToggleList;
  final bool floating;

  @override
  Widget build(BuildContext context) {
    final hits = snapshot?.hits ?? const <ScanHit>[];
    final column = Column(
      children: [
        if (controller.fatalError != null)
          _Banner(message: controller.fatalError!, color: danger),
        if (snapshot?.error != null)
          _Banner(message: snapshot!.error!, color: danger),
        if (controller.publishError != null)
          _Banner(message: controller.publishError!, color: warn),
        Padding(
          padding: const EdgeInsets.fromLTRB(16, 10, 8, 6),
          child: Row(
            children: [
              const Text(
                '识别结果 · 自动发布',
                style: TextStyle(fontSize: 12, letterSpacing: 1.2, color: muted),
              ),
              const SizedBox(width: 8),
              const Expanded(child: Divider(color: line)),
              IconButton(
                tooltip: '只看结果',
                visualDensity: VisualDensity.compact,
                onPressed: onToggleList,
                icon: const Icon(Icons.fullscreen, size: 18),
              ),
            ],
          ),
        ),
        Expanded(
          child: hits.isEmpty
              ? const _NoHits()
              : ListView.separated(
                  padding: const EdgeInsets.fromLTRB(16, 0, 16, 16),
                  itemCount: hits.length,
                  separatorBuilder: (_, _) => const SizedBox(height: 8),
                  itemBuilder: (context, index) {
                    final hit = hits[index];
                    return _HitTile(
                      hit: hit,
                      published: controller.isPublished(hit),
                      autoPublish: controller.autoPublishEnabled,
                    );
                  },
                ),
        ),
      ],
    );
    if (!floating) return column;
    return Container(
      decoration: BoxDecoration(
        color: Colors.black.withValues(alpha: 0.66),
        borderRadius: const BorderRadius.vertical(top: Radius.circular(18)),
        border: Border(top: BorderSide(color: ink.withValues(alpha: 0.12))),
      ),
      child: column,
    );
  }
}

/// Draws the latest hit next to its QR corners. The Android SurfaceTexture is
/// already upright but its buffer is portrait (height×width of the sensor), so
/// the texture uses swapped dimensions and the points map with a 90° rotation.
class _HitOverlay extends StatelessWidget {
  const _HitOverlay({
    required this.controller,
    required this.snapshot,
    required this.rotation,
    required this.contain,
  });

  final ScanController controller;
  final ScannerSnapshot? snapshot;
  final int rotation;
  final bool contain;

  @override
  Widget build(BuildContext context) {
    final all = snapshot?.hits ?? const <ScanHit>[];
    final seq = snapshot?.latestSeq ?? 0;
    final hits = seq == 0
        ? all
        : all.where((hit) => seq - hit.lastSeq <= 3).toList(growable: false);
    if (hits.isEmpty) return const SizedBox.shrink();
    final latest = hits.first;
    return LayoutBuilder(
      builder: (context, constraints) {
        final size = Size(constraints.maxWidth, constraints.maxHeight);
        final source = _SourceSize.of(snapshot, rotation);
        final point = _mapToView(latest.points, snapshot, source, size, rotation, contain);
        final left = point == null
            ? 16.0
            : (point.dx - 130).clamp(8.0, size.width - 268.0);
        final top = point == null
            ? 96.0
            : (point.dy - 92).clamp(78.0, size.height - 190.0);
        return Stack(
          children: [
            Positioned.fill(
              child: IgnorePointer(
                child: CustomPaint(
                  painter: _DetectionBoxPainter(
                    hits: hits,
                    detections: snapshot?.detections ?? const [],
                    snapshot: snapshot,
                    source: source,
                    view: size,
                    rotation: rotation,
                    contain: contain,
                  ),
                ),
              ),
            ),
            Positioned(
              left: left,
              top: top,
              child: _HitBubble(
                hit: latest,
                published: controller.isPublished(latest),
                autoPublish: controller.autoPublishEnabled,
              ),
            ),
            for (final hit in hits.skip(1).take(2))
              if (_mapToView(hit.points, snapshot, source, size, rotation, contain)
                  case final point?)
                Positioned(
                  left: (point.dx - 85).clamp(8.0, size.width - 178.0),
                  top: (point.dy + 50).clamp(78.0, size.height - 120.0),
                  child: _MiniHitBubble(
                    hit: hit,
                    published: controller.isPublished(hit),
                  ),
                ),
          ],
        );
      },
    );
  }
}

class _SourceSize {
  const _SourceSize(this.width, this.height);
  final double width;
  final double height;

  static _SourceSize of(ScannerSnapshot? snapshot, int rotation) {
    if (snapshot == null || snapshot.width == 0 || snapshot.height == 0) {
      return const _SourceSize(1, 1);
    }
    final degrees = ((rotation % 360) + 360) % 360;
    return degrees == 90 || degrees == 270
        ? _SourceSize(snapshot.height.toDouble(), snapshot.width.toDouble())
        : _SourceSize(snapshot.width.toDouble(), snapshot.height.toDouble());
  }
}

Offset? _mapToView(
  List<(double, double)> points,
  ScannerSnapshot? snapshot,
  _SourceSize source,
  Size view,
  int rotation,
  bool contain,
) {
  final mapped = _mapPoints(points, snapshot, source, view, rotation, contain);
  if (mapped.isEmpty) return null;
  final sum = mapped.fold(Offset.zero, (a, b) => a + b);
  return sum / mapped.length.toDouble();
}

List<Offset> _mapPoints(
  List<(double, double)> points,
  ScannerSnapshot? snapshot,
  _SourceSize source,
  Size view,
  int rotation,
  bool contain,
) {
  if (points.isEmpty || snapshot == null) return const [];
  final degrees = ((rotation % 360) + 360) % 360;
  final w = snapshot.width.toDouble();
  final h = snapshot.height.toDouble();
  final scale = contain
      ? ((view.width / source.width) < (view.height / source.height)
          ? view.width / source.width
          : view.height / source.height)
      : ((view.width / source.width) > (view.height / source.height)
          ? view.width / source.width
          : view.height / source.height);
  final drawn = Size(source.width * scale, source.height * scale);
  final offset = Offset(
    (view.width - drawn.width) / 2,
    (view.height - drawn.height) / 2,
  );
  return [
    for (final raw in points)
      () {
        final rotated = switch (degrees) {
          90 => Offset(h - 1 - raw.$2, raw.$1),
          180 => Offset(w - 1 - raw.$1, h - 1 - raw.$2),
          270 => Offset(raw.$2, w - 1 - raw.$1),
          _ => Offset(raw.$1, raw.$2),
        };
        return offset + Offset(rotated.dx * scale, rotated.dy * scale);
      }(),
  ];
}

class _DetectionBoxPainter extends CustomPainter {
  _DetectionBoxPainter({
    required this.hits,
    required this.detections,
    required this.snapshot,
    required this.source,
    required this.view,
    required this.rotation,
    required this.contain,
  });

  final List<ScanHit> hits;
  final List<List<(double, double)>> detections;
  final ScannerSnapshot? snapshot;
  final _SourceSize source;
  final Size view;
  final int rotation;
  final bool contain;

  @override
  void paint(Canvas canvas, Size size) {
    for (final box in detections) {
      final quad = _mapPoints(box, snapshot, source, size, rotation, contain);
      if (quad.length < 4) continue;
      final path = Path()..moveTo(quad[0].dx, quad[0].dy);
      for (final point in quad.skip(1)) {
        path.lineTo(point.dx, point.dy);
      }
      path.close();
      final cyanBounds = path.getBounds().inflate(5);
      canvas.drawRect(
        cyanBounds,
        Paint()
          ..style = PaintingStyle.stroke
          ..strokeWidth = 2
          ..color = const Color(0xFF6FE7FF),
      );
    }
    for (var i = 0; i < hits.length; i++) {
      final quad = _mapPoints(hits[i].points, snapshot, source, size, rotation, contain);
      if (quad.isEmpty) continue;
      final path = Path();
      if (quad.length >= 4) {
        path.moveTo(quad[0].dx, quad[0].dy);
        for (final point in quad.skip(1)) {
          path.lineTo(point.dx, point.dy);
        }
      } else {
        final xs = quad.map((p) => p.dx);
        final ys = quad.map((p) => p.dy);
        final left = xs.reduce((a, b) => a < b ? a : b);
        final top = ys.reduce((a, b) => a < b ? a : b);
        final right = xs.reduce((a, b) => a > b ? a : b);
        final bottom = ys.reduce((a, b) => a > b ? a : b);
        path.addRect(Rect.fromLTRB(left - 8, top - 8, right + 8, bottom + 8));
      }
      path.close();
      final paint = Paint()
        ..style = PaintingStyle.stroke
        ..strokeWidth = i == 0 ? 3 : 2
        ..color = i == 0 ? accent : Colors.white70;
      canvas.drawPath(path, paint);
    }
  }

  @override
  bool shouldRepaint(covariant _DetectionBoxPainter old) =>
      old.hits != hits ||
      old.detections != detections ||
      old.snapshot != snapshot;
}

class _MiniHitBubble extends StatelessWidget {
  const _MiniHitBubble({required this.hit, required this.published});

  final ScanHit hit;
  final bool published;

  @override
  Widget build(BuildContext context) {
    return Container(
      width: 170,
      padding: const EdgeInsets.symmetric(horizontal: 9, vertical: 7),
      decoration: BoxDecoration(
        color: Colors.black.withValues(alpha: 0.72),
        borderRadius: BorderRadius.circular(10),
        border: Border.all(
          color: published ? accent : Colors.white.withValues(alpha: 0.28),
        ),
      ),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        mainAxisSize: MainAxisSize.min,
        children: [
          Text(
            hit.text,
            maxLines: 2,
            overflow: TextOverflow.ellipsis,
            style: const TextStyle(fontSize: 11, color: Colors.white),
          ),
          const SizedBox(height: 3),
          Text(
            published ? '已广播' : '离线 · 未广播',
            style: TextStyle(
              fontSize: 9,
              color: published ? accent : muted,
            ),
          ),
        ],
      ),
    );
  }
}
class _HitBubble extends StatelessWidget {
  const _HitBubble({
    required this.hit,
    required this.published,
    required this.autoPublish,
  });

  final ScanHit hit;
  final bool published;
  final bool autoPublish;

  @override
  Widget build(BuildContext context) {
    final text = hit.text.isEmpty ? '（二进制内容）' : hit.text;
    final status = !autoPublish
        ? '离线 · 未广播'
        : published
            ? '已广播到房间'
            : '广播中…';
    return ConstrainedBox(
      constraints: const BoxConstraints(maxWidth: 260),
      child: Container(
        padding: const EdgeInsets.fromLTRB(12, 9, 12, 9),
        decoration: BoxDecoration(
          color: Colors.black.withValues(alpha: 0.78),
          borderRadius: BorderRadius.circular(12),
          border: Border.all(
            color: published ? accent : Colors.white.withValues(alpha: 0.35),
          ),
        ),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          mainAxisSize: MainAxisSize.min,
          children: [
            Text(
              text,
              maxLines: 3,
              overflow: TextOverflow.ellipsis,
              style: const TextStyle(fontSize: 13, height: 1.3, color: Colors.white),
            ),
            const SizedBox(height: 4),
            Row(
              mainAxisSize: MainAxisSize.min,
              children: [
                Icon(
                  published ? Icons.cloud_done_outlined : Icons.cloud_off_outlined,
                  size: 13,
                  color: published ? accent : muted,
                ),
                const SizedBox(width: 4),
                Text(
                  status,
                  style: TextStyle(
                    fontSize: 11,
                    color: published ? accent : muted,
                  ),
                ),
              ],
            ),
          ],
        ),
      ),
    );
  }
}

class _StatusChip extends StatelessWidget {
  const _StatusChip({required this.snapshot, required this.backend});

  final ScannerSnapshot? snapshot;
  final String backend;

  @override
  Widget build(BuildContext context) {
    final mode = switch (backend) {
      'zxing' => 'zxing',
      'wechat' => 'CNN',
      _ => '混合',
    };
    final text = snapshot == null
        ? '启动中…'
        : '$mode · ${snapshot!.width}×${snapshot!.height} · 帧 ${snapshot!.frames}'
            ' · ${snapshot!.scanMs.toStringAsFixed(1)}ms'
            ' · p95 ${snapshot!.p95Ms.toStringAsFixed(1)}ms'
            '${snapshot!.wechatRuns > 0 ? ' · CNN ${snapshot!.wechatMs.toStringAsFixed(1)}ms' : ''}'
            '${snapshot!.dropped > 0 ? ' · 丢 ${snapshot!.dropped}' : ''}';
    return Container(
      padding: const EdgeInsets.symmetric(horizontal: 10, vertical: 6),
      decoration: BoxDecoration(
        color: Colors.black.withValues(alpha: 0.62),
        borderRadius: BorderRadius.circular(8),
      ),
      child: Text(
        text,
        style: const TextStyle(color: ink, fontSize: 11.5, height: 1.2),
      ),
    );
  }
}

class _HitTile extends StatelessWidget {
  const _HitTile({
    required this.hit,
    required this.published,
    required this.autoPublish,
  });

  final ScanHit hit;
  final bool published;
  final bool autoPublish;

  @override
  Widget build(BuildContext context) {
    final label = hit.text.isEmpty ? '（二进制内容）' : hit.text;
    final slot = relaySlot(hit.text);
    final hasPrefix = slot != hit.text;
    return Container(
      padding: const EdgeInsets.all(12),
      decoration: BoxDecoration(
        color: surface,
        border: Border.all(color: published ? accent : line),
        borderRadius: BorderRadius.circular(12),
      ),
      child: Row(
        children: [
          Expanded(
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                if (hasPrefix)
                  Container(
                    margin: const EdgeInsets.only(bottom: 4),
                    padding: const EdgeInsets.symmetric(horizontal: 6, vertical: 2),
                    decoration: BoxDecoration(
                      color: accent.withValues(alpha: 0.14),
                      borderRadius: BorderRadius.circular(4),
                    ),
                    child: Text(
                      slot,
                      style: const TextStyle(
                        color: accent,
                        fontSize: 10.5,
                        fontWeight: FontWeight.w600,
                      ),
                    ),
                  ),
                Text(
                  label,
                  maxLines: 2,
                  overflow: TextOverflow.ellipsis,
                  style: const TextStyle(fontSize: 13, height: 1.35),
                ),
                const SizedBox(height: 4),
                Text(
                  '${hit.raw.length} 字节 · ${_timeLabel(hit.atMs)} · '
                  '${autoPublish ? (published ? '已自动发布' : '等待自动发布') : '离线，不发布'}',
                  style: TextStyle(
                    fontSize: 11.5,
                    color: published ? accent : muted,
                  ),
                ),
              ],
            ),
          ),
          const SizedBox(width: 8),
          IconButton(
            tooltip: '打开链接',
            visualDensity: VisualDensity.compact,
            onPressed: () => showOpenLinkSheet(context, hit.text),
            icon: const Icon(Icons.open_in_new, size: 20),
          ),
          Icon(
            published
                ? Icons.cloud_done_outlined
                : autoPublish
                    ? Icons.cloud_upload_outlined
                    : Icons.cloud_off_outlined,
            color: published ? accent : muted,
            size: 22,
          ),
        ],
      ),
    );
  }

  static String _timeLabel(int atMs) {
    if (atMs == 0) return '刚刚';
    final time = DateTime.fromMillisecondsSinceEpoch(atMs).toLocal();
    return '${time.hour.toString().padLeft(2, '0')}:'
        '${time.minute.toString().padLeft(2, '0')}';
  }
}

class _NoHits extends StatelessWidget {
  const _NoHits();

  @override
  Widget build(BuildContext context) {
    return LayoutBuilder(
      builder: (context, constraints) => SingleChildScrollView(
        padding: const EdgeInsets.all(24),
        child: ConstrainedBox(
          constraints: BoxConstraints(
            minHeight: (constraints.maxHeight - 48).clamp(0.0, 1e6),
          ),
          child: const Center(
            child: Column(
              mainAxisSize: MainAxisSize.min,
              children: [
                Icon(Icons.qr_code_2, color: muted, size: 40),
                SizedBox(height: 12),
                Text(
                  '把二维码对准摄像头',
                  style: TextStyle(fontSize: 16, fontWeight: FontWeight.w600),
                ),
                SizedBox(height: 6),
                Text(
                  '把二维码对准摄像头，识别后会自动发布；同一前缀的新内容会更新该槽位。',
                  textAlign: TextAlign.center,
                  style: TextStyle(color: muted, height: 1.5, fontSize: 12.5),
                ),
              ],
            ),
          ),
        ),
      ),
    );
  }
}

class _Banner extends StatelessWidget {
  const _Banner({required this.message, required this.color});

  final String message;
  final Color color;

  @override
  Widget build(BuildContext context) {
    return Container(
      width: double.infinity,
      color: color.withValues(alpha: 0.12),
      padding: const EdgeInsets.symmetric(horizontal: 16, vertical: 10),
      child: Text(
        message,
        style: TextStyle(color: color, fontSize: 12.5, height: 1.4),
      ),
    );
  }
}

class _InitError extends StatelessWidget {
  const _InitError({required this.message});

  final String message;

  @override
  Widget build(BuildContext context) {
    return Center(
      child: ConstrainedBox(
        constraints: const BoxConstraints(maxWidth: 420),
        child: Padding(
          padding: const EdgeInsets.all(28),
          child: Column(
            mainAxisSize: MainAxisSize.min,
            children: [
              const Icon(Icons.memory, color: muted, size: 40),
              const SizedBox(height: 14),
              const Text(
                '原生扫描器没有加载',
                style: TextStyle(fontSize: 18, fontWeight: FontWeight.w600),
              ),
              const SizedBox(height: 8),
              Text(
                message,
                textAlign: TextAlign.center,
                style: const TextStyle(color: muted, height: 1.5),
              ),
            ],
          ),
        ),
      ),
    );
  }
}






















