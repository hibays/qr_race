// Browser camera loop for the scan page. Loaded as a module from index.html;
// Flutter calls the two globals through dart:js_interop.
//
// The wasm module is generated next to this file by scripts/build-wasm.ps1.

import init, { scan_gray } from './qr_race_scan_wasm.js';

let ready = null;
let session = null;

function ensureReady() {
  if (!ready) ready = init();
  return ready;
}

function toGray(rgba, pixels) {
  const gray = new Uint8Array(pixels);
  for (let source = 0, target = 0; target < pixels; source += 4, target++) {
    gray[target] = (rgba[source] * 77 + rgba[source + 1] * 150 + rgba[source + 2] * 29) >> 8;
  }
  return gray;
}

export async function start(onJson, onError) {
  try {
    await ensureReady();
    stop();

    const stream = await navigator.mediaDevices.getUserMedia({
      audio: false,
      video: {
        facingMode: 'environment',
        width: { ideal: 1280 },
        height: { ideal: 720 },
      },
    });
    const video = document.createElement('video');
    video.playsInline = true;
    video.muted = true;
    video.srcObject = stream;
    await video.play();

    const canvas = document.createElement('canvas');
    const context = canvas.getContext('2d', { willReadFrequently: true });
    const state = { stream, video, canvas, context, timer: 0, stopped: false };
    session = state;

    let lastScan = 0;
    const tick = () => {
      if (state.stopped) return;
      const now = performance.now();
      if (video.readyState >= 2 && video.videoWidth > 0 && now - lastScan >= 100) {
        lastScan = now;
        const width = video.videoWidth;
        const height = video.videoHeight;
        canvas.width = width;
        canvas.height = height;
        context.drawImage(video, 0, 0, width, height);
        const rgba = context.getImageData(0, 0, width, height).data;
        const gray = toGray(rgba, width * height);
        const started = performance.now();
        try {
          const result = JSON.parse(scan_gray(width, height, gray));
          result.scan_ms = performance.now() - started;
          onJson(JSON.stringify(result));
        } catch (error) {
          onError(String(error));
        }
      }
      state.timer = requestAnimationFrame(tick);
    };
    tick();
  } catch (error) {
    // getUserMedia rejects without a secure context (HTTPS or localhost).
    onError(error && error.message ? error.message : String(error));
  }
}

export function stop() {
  if (!session) return;
  session.stopped = true;
  cancelAnimationFrame(session.timer);
  session.stream.getTracks().forEach((track) => track.stop());
  session.video.srcObject = null;
  session = null;
}

window.qrRaceStartCameraScan = start;
window.qrRaceStopCameraScan = stop;
window.qrRaceScanReady = ensureReady();
