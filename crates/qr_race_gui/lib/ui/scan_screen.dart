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

class _ScanScreenState extends State<ScanScreen> {
  ScanController? _controller;
  late final CameraPermission _permission;
  CameraPermissionStatus? _denied;
  String? _initError;
  String? _permissionError;
  bool _requesting = true;

  @override
  void initState() {
    super.initState();
    _permission = widget.permission ?? createCameraPermission();
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
        width: const int.fromEnvironment('SCAN_WIDTH', defaultValue: 1280),
        height: const int.fromEnvironment('SCAN_HEIGHT', defaultValue: 720),
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
    _controller?.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final controller = _controller;
    return Scaffold(
      appBar: AppBar(
        title: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            const Text('自动扫码', style: TextStyle(fontSize: 18, fontWeight: FontWeight.w700)),
            Text(widget.address, style: const TextStyle(fontSize: 12, color: muted)),
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
                        return _Body(controller: controller);
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
  const _Body({required this.controller});

  final ScanController controller;

  @override
  Widget build(BuildContext context) {
    final snapshot = controller.snapshot;
    final preview = snapshot?.preview;

    return LayoutBuilder(
      builder: (context, constraints) {
        // Keep the preview large but never let it starve the result list.
        final byWidth = constraints.maxWidth * 9 / 16;
        final previewHeight =
            (constraints.maxHeight * 0.45).clamp(140.0, 420.0);
        final height = previewHeight < byWidth ? previewHeight : byWidth;

        return Column(
          children: [
            SizedBox(
              height: height,
              width: double.infinity,
              child: Container(
                color: Colors.black,
                child: Stack(
                  fit: StackFit.expand,
                  children: [
                    if (preview != null)
                      Image.memory(
                        preview,
                        fit: BoxFit.contain,
                        gaplessPlayback: true,
                      )
                    else
                      const Center(
                        child: Column(
                          mainAxisSize: MainAxisSize.min,
                          children: [
                            Icon(Icons.videocam_off_outlined,
                                color: muted, size: 34),
                            SizedBox(height: 8),
                            Text(
                              '等待相机预览…',
                              style: TextStyle(color: muted, fontSize: 12.5),
                            ),
                          ],
                        ),
                      ),
                    Positioned(
                      left: 12,
                      bottom: 12,
                      child: _StatusChip(snapshot: snapshot),
                    ),
                  ],
                ),
              ),
            ),
            if (controller.fatalError != null)
              _Banner(message: controller.fatalError!, color: danger),
            if (snapshot?.error != null)
              _Banner(message: snapshot!.error!, color: danger),
            if (controller.publishError != null)
              _Banner(message: controller.publishError!, color: warn),
            const Padding(
              padding: EdgeInsets.fromLTRB(16, 16, 16, 8),
              child: Row(
                children: [
                  Text(
                    '识别结果 · 自动发布',
                    style: TextStyle(
                      fontSize: 12,
                      letterSpacing: 1.2,
                      color: muted,
                    ),
                  ),
                  SizedBox(width: 8),
                  Expanded(child: Divider(color: line)),
                ],
              ),
            ),
            Expanded(
              child: snapshot == null || snapshot.hits.isEmpty
                  ? const _NoHits()
                  : ListView.separated(
                      padding: const EdgeInsets.fromLTRB(16, 0, 16, 16),
                      itemCount: snapshot.hits.length,
                      separatorBuilder: (_, _) => const SizedBox(height: 8),
                      itemBuilder: (context, index) {
                        final hit = snapshot.hits[index];
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
      },
    );
  }
}

class _StatusChip extends StatelessWidget {
  const _StatusChip({required this.snapshot});

  final ScannerSnapshot? snapshot;

  @override
  Widget build(BuildContext context) {
    final text = snapshot == null
        ? '启动中…'
        : '${snapshot!.width}×${snapshot!.height} · 帧 ${snapshot!.frames}'
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
