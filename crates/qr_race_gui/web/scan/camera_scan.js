// Browser camera -> zxing-wasm (ZXing-C++ WebAssembly). This replaces the old
// Rust wasm module, so the browser no longer ships rxing. All frames stay in
// JavaScript; Dart only receives the hit JSON.
import {
  prepareZXingModule as prepareReader,
  readBarcodes,
} from './zxing-wasm/reader/index.js';
import {
  prepareZXingModule as prepareWriter,
  writeBarcode,
} from './zxing-wasm/writer/index.js';

const readerWasm = new URL('./zxing_reader.wasm', import.meta.url).href;
const writerWasm = new URL('./zxing_writer.wasm', import.meta.url).href;

const locate = (wasm) => (path, prefix) =>
  path.endsWith('.wasm') ? wasm : prefix + path;

window.qrRaceScanReady = Promise.all([
  prepareReader({
    overrides: { locateFile: locate(readerWasm) },
    fireImmediately: true,
  }),
  prepareWriter({
    overrides: { locateFile: locate(writerWasm) },
    fireImmediately: true,
  }),
]).then(() => true);

let stream = null;
let video = null;
let canvas = null;
let running = false;
let lastScan = 0;

function ensureCanvas() {
  if (!canvas) {
    canvas = document.createElement('canvas');
    canvas.width = 640;
    canvas.height = 480;
  }
  return canvas;
}

function base64(bytes) {
  let binary = '';
  for (let i = 0; i < bytes.length; i++) binary += String.fromCharCode(bytes[i]);
  return btoa(binary);
}

async function scanFrame(onJson) {
  const width = video.videoWidth;
  const height = video.videoHeight;
  if (!width || !height) return;
  const c = ensureCanvas();
  if (c.width !== width || c.height !== height) {
    c.width = width;
    c.height = height;
  }
  const ctx = c.getContext('2d', { willReadFrequently: true });
  ctx.drawImage(video, 0, 0, width, height);
  const image = ctx.getImageData(0, 0, width, height);
  const started = performance.now();
  let results = [];
  let error = null;
  try {
    results = await readBarcodes(image, {
      formats: ['QRCode'],
      tryHarder: true,
      maxNumberOfSymbols: 8,
    });
  } catch (e) {
    error = String(e && e.message ? e.message : e);
  }
  const scanMs = performance.now() - started;
  const hits = results.map((result) => ({
    text: result.text,
    raw_b64: base64(result.bytes || new TextEncoder().encode(result.text || '')),
    format: result.format,
    points: result.position
      ? [
          [result.position.topLeft.x, result.position.topLeft.y],
          [result.position.topRight.x, result.position.topRight.y],
          [result.position.bottomRight.x, result.position.bottomRight.y],
          [result.position.bottomLeft.x, result.position.bottomLeft.y],
        ]
      : [],
  }));
  onJson(
    JSON.stringify({
      width,
      height,
      scan_ms: scanMs,
      error,
      hits,
    }),
  );
}

window.qrRaceStartCameraScan = async function (onJson, onError) {
  if (running) return;
  running = true;
  try {
    await window.qrRaceScanReady;
    stream = await navigator.mediaDevices.getUserMedia({
      video: {
        facingMode: { ideal: 'environment' },
        width: { ideal: 1280 },
        height: { ideal: 720 },
      },
      audio: false,
    });
    video = document.createElement('video');
    video.playsInline = true;
    video.muted = true;
    video.srcObject = stream;
    await video.play();
    const tick = async (now) => {
      if (!running) return;
      if (now - lastScan >= 100) {
        lastScan = now;
        try {
          await scanFrame(onJson);
        } catch (e) {
          onError(String(e && e.message ? e.message : e));
        }
      }
      requestAnimationFrame(tick);
    };
    requestAnimationFrame(tick);
    return true;
  } catch (e) {
    running = false;
    onError(String(e && e.message ? e.message : e));
    return false;
  }
};

window.qrRaceStopCameraScan = function () {
  running = false;
  if (stream) {
    for (const track of stream.getTracks()) track.stop();
  }
  stream = null;
  if (video) {
    video.srcObject = null;
    video = null;
  }
};

window.qrRaceQrWebp = async function (text) {
  await window.qrRaceScanReady;
  const output = await writeBarcode(text, {
    format: 'QRCode',
    scale: 8,
    addQuietZones: true,
  });
  const bitmap = await createImageBitmap(output.image);
  const qr = document.createElement('canvas');
  qr.width = 2048;
  qr.height = 2048;
  const ctx = qr.getContext('2d');
  ctx.fillStyle = '#fff';
  ctx.fillRect(0, 0, 2048, 2048);
  ctx.imageSmoothingEnabled = false;
  ctx.drawImage(bitmap, 0, 0, 2048, 2048);
  const blob = await new Promise((resolve) =>
    qr.toBlob(resolve, 'image/webp', 0.9),
  );
  return new Uint8Array(await blob.arrayBuffer());
};
