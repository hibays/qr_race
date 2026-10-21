package app.qrrace.qr_race_gui

import android.annotation.SuppressLint
import android.content.Context
import android.graphics.ImageFormat
import android.hardware.camera2.CameraCaptureSession
import android.hardware.camera2.CameraCharacteristics
import android.hardware.camera2.CameraDevice
import android.hardware.camera2.CameraManager
import android.hardware.camera2.CaptureRequest
import android.media.ImageReader
import android.os.Handler
import android.os.HandlerThread
import android.util.Log
import android.util.Size
import java.util.concurrent.atomic.AtomicBoolean
import kotlin.math.abs

/**
 * Camera2 YUV capture. The Y plane is handed to Rust through JNI; no frame ever
 * reaches Dart. The UI only polls decoded hits and the JPEG preview.
 */
class CameraCapture(
    private val context: Context,
    private val previewSurface: android.view.Surface? = null,
) {
    private var thread: HandlerThread? = null
    private var handler: Handler? = null
    private var camera: CameraDevice? = null
    private var session: CameraCaptureSession? = null
    private var reader: ImageReader? = null
    private val busy = AtomicBoolean(false)
    private var sensorOrientation = 90

    @SuppressLint("MissingPermission")
    fun start(requestedWidth: Int, requestedHeight: Int) {
        if (camera != null) return
        val worker = HandlerThread("qr-camera").also { it.start() }
        thread = worker
        val callbackHandler = Handler(worker.looper)
        handler = callbackHandler

        val manager = context.getSystemService(Context.CAMERA_SERVICE) as CameraManager
        val cameraId = chooseBackCamera(manager) ?: run {
            Log.e(TAG, "no camera available")
            return
        }
        val characteristics = manager.getCameraCharacteristics(cameraId)
        sensorOrientation = characteristics.get(CameraCharacteristics.SENSOR_ORIENTATION) ?: 90
        val map = characteristics.get(CameraCharacteristics.SCALER_STREAM_CONFIGURATION_MAP)
        val size = chooseSize(map, requestedWidth, requestedHeight)

        reader = ImageReader.newInstance(size.width, size.height, ImageFormat.YUV_420_888, 2).also {
            it.setOnImageAvailableListener({ source -> onImage(source) }, callbackHandler)
        }
        val surface = reader!!.surface
        try {
            manager.openCamera(cameraId, object : CameraDevice.StateCallback() {
                override fun onOpened(device: CameraDevice) {
                    camera = device
                    val targets = buildList {
                        add(surface)
                        previewSurface?.let { add(it) }
                    }
                    device.createCaptureSession(
                        targets,
                        object : CameraCaptureSession.StateCallback() {
                            override fun onConfigured(configured: CameraCaptureSession) {
                                session = configured
                                val request = device.createCaptureRequest(
                                    CameraDevice.TEMPLATE_PREVIEW,
                                ).apply {
                                    addTarget(surface)
                                    previewSurface?.let { addTarget(it) }
                                    set(
                                        CaptureRequest.CONTROL_AF_MODE,
                                        CaptureRequest.CONTROL_AF_MODE_CONTINUOUS_PICTURE,
                                    )
                                    set(
                                        CaptureRequest.CONTROL_AE_MODE,
                                        CaptureRequest.CONTROL_AE_MODE_ON,
                                    )
                                }
                                try {
                                    configured.setRepeatingRequest(request.build(), null, callbackHandler)
                                } catch (error: Exception) {
                                    Log.e(TAG, "setRepeatingRequest failed", error)
                                }
                            }

                            override fun onConfigureFailed(failed: CameraCaptureSession) {
                                Log.e(TAG, "capture session configure failed")
                            }
                        },
                        callbackHandler,
                    )
                }

                override fun onDisconnected(device: CameraDevice) {
                    device.close()
                    camera = null
                }

                override fun onError(device: CameraDevice, error: Int) {
                    Log.e(TAG, "camera error $error")
                    device.close()
                    camera = null
                }
            }, callbackHandler)
        } catch (error: SecurityException) {
            Log.e(TAG, "camera permission missing", error)
        } catch (error: Exception) {
            Log.e(TAG, "openCamera failed", error)
        }
    }

    fun stop() {
        try {
            session?.stopRepeating()
            session?.close()
        } catch (_: Exception) {
        }
        session = null
        try {
            camera?.close()
        } catch (_: Exception) {
        }
        camera = null
        try {
            reader?.close()
        } catch (_: Exception) {
        }
        reader = null
        thread?.quitSafely()
        try {
            thread?.join(500)
        } catch (_: InterruptedException) {
        }
        thread = null
        handler = null
        busy.set(false)
    }

    private fun onImage(source: ImageReader) {
        val image = source.acquireLatestImage() ?: return
        if (!busy.compareAndSet(false, true)) {
            nativeFrameDropped()
            image.close()
            return
        }
        try {
            val width = image.width
            val height = image.height
            val y = copyPlane(image.planes[0], width, height)
            val chromaWidth = width / 2
            val chromaHeight = height / 2
            val u = copyPlane(image.planes[1], chromaWidth, chromaHeight)
            val v = copyPlane(image.planes[2], chromaWidth, chromaHeight)
            // Scanning is rotation-independent; send rotation metadata and let
            // Flutter rotate the low-rate preview on the GPU instead of copying
            // ~1 MB of pixels per frame here.
            nativePushFrame(width, height, sensorOrientation, y, u, v)
        } catch (error: Exception) {
            Log.e(TAG, "frame copy failed", error)
        } finally {
            busy.set(false)
            image.close()
        }
    }

    /** Copies one plane into a tightly packed array, honoring its strides. */
    private fun copyPlane(
        plane: android.media.Image.Plane,
        width: Int,
        height: Int,
    ): ByteArray {
        val buffer = plane.buffer
        val rowStride = plane.rowStride
        val pixelStride = plane.pixelStride
        val out = ByteArray(width * height)
        val row = ByteArray(rowStride)
        val limit = buffer.limit()
        for (r in 0 until height) {
            val start = r * rowStride
            if (start >= limit) break
            val available = minOf(rowStride, limit - start)
            buffer.position(start)
            buffer.get(row, 0, available)
            if (pixelStride == 1 && available >= width) {
                System.arraycopy(row, 0, out, r * width, width)
            } else {
                for (col in 0 until width) {
                    val index = col * pixelStride
                    if (index < available) {
                        out[r * width + col] = row[index]
                    }
                }
            }
        }
        return out
    }

    private fun chooseBackCamera(manager: CameraManager): String? {
        for (id in manager.cameraIdList) {
            val facing = manager.getCameraCharacteristics(id)
                .get(CameraCharacteristics.LENS_FACING)
            if (facing == CameraCharacteristics.LENS_FACING_BACK) return id
        }
        return manager.cameraIdList.firstOrNull()
    }

    private fun chooseSize(
        map: android.hardware.camera2.params.StreamConfigurationMap?,
        width: Int,
        height: Int,
    ): Size {
        val sizes = map?.getOutputSizes(ImageFormat.YUV_420_888)
        if (sizes.isNullOrEmpty()) return Size(width, height)
        sizes.firstOrNull { it.width == width && it.height == height }?.let { return it }
        // Prefer the largest supported size that does not exceed the requested
        // 2K target. This avoids picking 4K on devices that also expose 1080p.
        sizes.filter { it.width <= width && it.height <= height }
            .maxByOrNull { it.width.toLong() * it.height }
            ?.let { return it }
        return sizes.minByOrNull { abs(it.width - width) + abs(it.height - height) }
            ?: Size(width, height)
    }

    private external fun nativePushFrame(
        width: Int,
        height: Int,
        rotation: Int,
        y: ByteArray,
        u: ByteArray,
        v: ByteArray,
    )

    private external fun nativeFrameDropped()

    companion object {
        private const val TAG = "qr_race_camera"

        init {
            try {
                // Load ORT into the app namespace before dynamic loading by soname.
                System.loadLibrary("onnxruntime")
            } catch (error: UnsatisfiedLinkError) {
                Log.e(TAG, "onnxruntime not packaged", error)
            }
            try {
                System.loadLibrary("qr_race_ffi")
            } catch (error: UnsatisfiedLinkError) {
                Log.e(TAG, "qr_race_ffi not packaged", error)
            }
        }
    }
}

