package app.qrrace.qr_race_gui

import android.Manifest
import android.app.Activity
import android.content.ActivityNotFoundException
import android.content.Intent
import android.content.pm.PackageManager
import android.net.Uri
import android.os.Build
import android.os.Environment
import android.provider.OpenableColumns
import android.provider.Settings
import android.view.Surface
import io.flutter.embedding.android.FlutterActivity
import io.flutter.embedding.engine.FlutterEngine
import io.flutter.plugin.common.MethodChannel
import io.flutter.view.TextureRegistry
import java.io.File

class MainActivity : FlutterActivity() {
    private val channelName = "app.qrrace.qr_race_gui/camera"
    private val filesChannelName = "app.qrrace.qr_race_gui/files"
    private val linksChannelName = "app.qrrace.qr_race_gui/links"
    private val requestCode = 4107
    private val fileRequestCode = 4108
    private var pending: MethodChannel.Result? = null
    private var pendingFile: MethodChannel.Result? = null
    private var pendingLink: String? = null
    private var linkChannel: MethodChannel? = null
    private var capture: CameraCapture? = null
    private var captureWidth = 1280
    private var captureHeight = 720
    private var captureActive = false
    private var previewTexture: TextureRegistry.SurfaceTextureEntry? = null

    override fun configureFlutterEngine(flutterEngine: FlutterEngine) {
        super.configureFlutterEngine(flutterEngine)
        pendingLink = intent?.dataString
        linkChannel = MethodChannel(
            flutterEngine.dartExecutor.binaryMessenger,
            linksChannelName,
        ).also { channel ->
            channel.setMethodCallHandler { call, result ->
                when (call.method) {
                    "getInitialLink" -> {
                        val link = pendingLink
                        pendingLink = null
                        result.success(link)
                    }
                    "openExternal" -> {
                        val url = call.argument<String>("url")
                        result.success(url != null && openExternal(url))
                    }
                    else -> result.notImplemented()
                }
            }
        }
        MethodChannel(
            flutterEngine.dartExecutor.binaryMessenger,
            "app.qrrace.qr_race_gui/storage",
        ).setMethodCallHandler { call, result ->
            val prefs = getSharedPreferences("qr_race_settings", MODE_PRIVATE)
            when (call.method) {
                "load" -> result.success(
                    mapOf(
                        "server" to (prefs.getString("server_url", null) ?: ""),
                        "fingerprint" to (prefs.getString("server_fingerprint", null) ?: ""),
                        "token" to (prefs.getString("server_token", null) ?: ""),
                    ),
                )
                "save" -> {
                    prefs.edit()
                        .putString("server_url", call.argument<String>("server") ?: "")
                        .putString(
                            "server_fingerprint",
                            call.argument<String>("fingerprint") ?: "",
                        )
                        .putString("server_token", call.argument<String>("token") ?: "")
                        .apply()
                    result.success(null)
                }
                else -> result.notImplemented()
            }
        }
        MethodChannel(flutterEngine.dartExecutor.binaryMessenger, channelName)
            .setMethodCallHandler { call, result ->
                when (call.method) {
                    "request" -> requestCamera(result)
                    "openSettings" -> {
                        openAppSettings()
                        result.success(null)
                    }
                    "createPreviewTexture" -> {
                        result.success(ensurePreviewTexture().id())
                    }
                    "previewTextureId" -> {
                        result.success(previewTexture?.id() ?: -1)
                    }
                    "startCapture" -> {
                        val width = call.argument<Int>("width") ?: 1280
                        val height = call.argument<Int>("height") ?: 720
                        captureWidth = width
                        captureHeight = height
                        captureActive = true
                        capture?.stop()
                        previewTexture?.release()
                        previewTexture = null
                        val texture = ensurePreviewTexture()
                        texture.surfaceTexture().setDefaultBufferSize(width, height)
                        capture = CameraCapture(
                            this,
                            texture.surfaceTexture(),
                        ).also { it.start(width, height) }
                        result.success(texture.id())
                    }
                    "previewTransform" -> {
                        val capture = capture
                        val matrix = capture?.previewTransform()
                        if (matrix == null) {
                            result.success(null)
                        } else {
                            result.success(
                                mapOf(
                                    "matrix" to matrix.toList(),
                                    "width" to capture.previewWidth(),
                                    "height" to capture.previewHeight(),
                                ),
                            )
                        }
                    }
                    "trackDetections" -> {
                        capture?.trackDetections(
                            (call.argument<Double>("cx") ?: 0.5).toFloat(),
                            (call.argument<Double>("cy") ?: 0.5).toFloat(),
                            (call.argument<Double>("w") ?: 1.0).toFloat(),
                            (call.argument<Double>("h") ?: 1.0).toFloat(),
                        )
                        result.success(null)
                    }
                    "clearTracking" -> {
                        capture?.clearTracking()
                        result.success(null)
                    }
                    "stopCapture" -> {
                        captureActive = false
                        capture?.stop()
                        capture = null
                        previewTexture?.release()
                        previewTexture = null
                        result.success(null)
                    }
                    else -> result.notImplemented()
                }
            }
        MethodChannel(flutterEngine.dartExecutor.binaryMessenger, filesChannelName)
            .setMethodCallHandler { call, result ->
                when (call.method) {
                    "hasAllFilesAccess" -> result.success(hasAllFilesAccess())
                    "requestAllFilesAccess" -> {
                        openAllFilesSettings()
                        result.success(hasAllFilesAccess())
                    }
                    "pickFile" -> {
                        if (pendingFile != null) {
                            result.error("busy", "a file picker is already open", null)
                        } else {
                            pendingFile = result
                            val intent = Intent(Intent.ACTION_OPEN_DOCUMENT).apply {
                                addCategory(Intent.CATEGORY_OPENABLE)
                                type = "*/*"
                            }
                            startActivityForResult(intent, fileRequestCode)
                        }
                    }
                    else -> result.notImplemented()
                }
            }
    }

    override fun onNewIntent(intent: Intent) {
        super.onNewIntent(intent)
        setIntent(intent)
        val link = intent.dataString ?: return
        pendingLink = link
        linkChannel?.invokeMethod("link", link)
    }

    override fun onPause() {
        // Camera2 sessions are invalidated when another app (WeChat/browser)
        // comes forward. Stop here and reopen in onResume so returning to the
        // scanner does not leave a dead session.
        if (captureActive) {
            capture?.stop()
            capture = null
        }
        super.onPause()
    }

    override fun onResume() {
        super.onResume()
    }

    private fun ensurePreviewTexture(): TextureRegistry.SurfaceTextureEntry {
        previewTexture?.let { return it }
        return flutterEngine!!.renderer.createSurfaceTexture().also { previewTexture = it }
    }

    override fun onDestroy() {
        captureActive = false
        capture?.stop()
        capture = null
        previewTexture?.release()
        previewTexture = null
        super.onDestroy()
    }

    override fun onActivityResult(requestCode: Int, resultCode: Int, data: Intent?) {
        super.onActivityResult(requestCode, resultCode, data)
        if (requestCode != fileRequestCode) return
        val result = pendingFile ?: return
        pendingFile = null
        val uri = data?.data
        if (resultCode != Activity.RESULT_OK || uri == null) {
            result.success(null)
            return
        }
        try {
            result.success(copyToCache(uri))
        } catch (error: Exception) {
            result.error("pick", error.message ?: "could not read file", null)
        }
    }

    private fun openExternal(url: String): Boolean {
        return try {
            val intent = Intent(Intent.ACTION_VIEW, Uri.parse(url))
            if (url.startsWith("http://") || url.startsWith("https://")) {
                startActivity(Intent.createChooser(intent, "打开链接"))
            } else {
                startActivity(intent)
            }
            true
        } catch (_: ActivityNotFoundException) {
            false
        } catch (_: Exception) {
            false
        }
    }

    private fun hasAllFilesAccess(): Boolean =
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.R) {
            Environment.isExternalStorageManager()
        } else {
            checkSelfPermission(Manifest.permission.WRITE_EXTERNAL_STORAGE) ==
                PackageManager.PERMISSION_GRANTED
        }

    private fun openAllFilesSettings() {
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.R) {
            val intent = Intent(
                Settings.ACTION_MANAGE_APP_ALL_FILES_ACCESS_PERMISSION,
                Uri.parse("package:$packageName"),
            )
            startActivity(intent)
        }
    }

    private fun copyToCache(uri: Uri): String {
        val name = queryDisplayName(uri) ?: "picked"
        val target = File(cacheDir, "qrrace-${System.currentTimeMillis()}-$name")
        contentResolver.openInputStream(uri)?.use { input ->
            target.outputStream().use { output -> input.copyTo(output) }
        } ?: throw IllegalStateException("cannot open $uri")
        return target.absolutePath
    }

    private fun queryDisplayName(uri: Uri): String? {
        contentResolver.query(uri, arrayOf(OpenableColumns.DISPLAY_NAME), null, null, null)
            ?.use { cursor ->
                if (cursor.moveToFirst()) {
                    val index = cursor.getColumnIndex(OpenableColumns.DISPLAY_NAME)
                    if (index >= 0) return cursor.getString(index)
                }
            }
        return uri.lastPathSegment
    }

    private fun isGranted(): Boolean =
        checkSelfPermission(Manifest.permission.CAMERA) == PackageManager.PERMISSION_GRANTED

    private fun requestCamera(result: MethodChannel.Result) {
        if (isGranted()) {
            result.success("granted")
            return
        }
        if (pending != null) {
            result.error("busy", "a permission request is already running", null)
            return
        }
        pending = result
        requestPermissions(arrayOf(Manifest.permission.CAMERA), requestCode)
    }

    override fun onRequestPermissionsResult(
        requestCode: Int,
        permissions: Array<out String>,
        grantResults: IntArray,
    ) {
        super.onRequestPermissionsResult(requestCode, permissions, grantResults)
        if (requestCode != this.requestCode) return
        val result = pending ?: return
        pending = null
        val allowed = grantResults.isNotEmpty() &&
            grantResults[0] == PackageManager.PERMISSION_GRANTED
        if (allowed) {
            result.success("granted")
            return
        }
        // False after "don't ask again" (or policy denial): the app cannot show
        // the system dialog anymore, so the UI sends the user to Settings.
        val permanentlyDenied = !shouldShowRequestPermissionRationale(Manifest.permission.CAMERA)
        result.success(if (permanentlyDenied) "permanentlyDenied" else "denied")
    }

    private fun openAppSettings() {
        val intent = Intent(
            Settings.ACTION_APPLICATION_DETAILS_SETTINGS,
            Uri.parse("package:$packageName"),
        )
        intent.addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
        startActivity(intent)
    }
}





