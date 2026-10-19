// Verifies the wasm scan core end to end outside a browser.
//
// Build the glue first (see docs/WASM.md):
//   cargo build -p qr_race_scan_wasm --target wasm32-unknown-unknown --release
//   wasm-bindgen target/wasm32-unknown-unknown/release/qr_race_scan_wasm.wasm \
//     --target nodejs --out-dir .refs/wasm-node
//   cargo run --release -p qr_race_bench -- --input .refs/scene.png \
//     --dump-gray .refs/scene.gray
//
// Then: node tools/wasm_scan_probe.cjs

const fs = require('node:fs');
const path = require('node:path');

const glue = path.resolve('.refs/wasm-node/qr_race_scan_wasm.js');
const framePath = path.resolve('.refs/scene.gray');
for (const file of [glue, framePath]) {
  if (!fs.existsSync(file)) {
    console.error(`missing ${file}; see docs/WASM.md for the build steps`);
    process.exit(2);
  }
}

const wasm = require(glue);
const gray = fs.readFileSync(framePath);
const width = gray.readUInt32LE(0);
const height = gray.readUInt32LE(4);
const pixels = new Uint8Array(gray.subarray(8));
if (pixels.length !== width * height) {
  console.error(`frame size mismatch: ${pixels.length} vs ${width}x${height}`);
  process.exit(2);
}

const started = performance.now();
const result = JSON.parse(wasm.scan_gray(width, height, pixels));
const wallMs = performance.now() - started;

console.log(
  `${width}x${height} wall=${wallMs.toFixed(2)}ms hits=${result.hits.length} ` +
    `error=${result.error}`,
);
for (const hit of result.hits) {
  console.log('  raw:', Buffer.from(hit.raw_b64, 'base64').toString());
}

const expected = 'https://example.test/relay';
if (result.error) process.exit(1);
if (!result.hits.some((hit) => Buffer.from(hit.raw_b64, 'base64').toString() === expected)) {
  console.error(`missing expected payload: ${expected}`);
  process.exit(1);
}
console.log('wasm scan OK');
