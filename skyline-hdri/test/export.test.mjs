// Exporter tests on a small synthetic plate (no network needed).
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { encodeRGBA8, encodeGray8, encodeGray16, encodePNG } from '../src/export/png.js';
import { decodePNG } from './png-decode.mjs';
import { encodeEXR, plateToEXR } from '../src/export/exr.js';
import { signedDistanceField, sdfToGray8, depthToGray16, normalToRGB8, shadePlate, distanceTransform } from '../src/export/layers.js';
import { plateToSVG, simplify } from '../src/export/svg.js';
import { allocatePlate } from '../src/engine/sweep.js';

/** A plate with a flat horizon at row H/2 and a 'tower' bump in columns 10..19 up to row H/4. */
function syntheticPlate(W = 64) {
  const H = W / 2;
  const plate = allocatePlate(W, 2);
  for (let x = 0; x < W; x++) {
    const top = x >= 10 && x < 20 ? H / 4 : H / 2;
    for (let y = top; y < H; y++) {
      const i = y * W + x;
      plate.coverage[i] = 255; plate.depth[i] = 100 + y; plate.classId[i] = x >= 10 && x < 20 ? 2 : 1;
      plate.altitude[i] = 10; plate.normal[i * 3 + 2] = 1;
    }
    plate.horizon[x] = (0.5 - top / H) * Math.PI;
    plate.bands[x] = plate.horizon[x]; plate.bands[W + x] = plate.horizon[x];
  }
  return plate;
}

test('PNG: RGBA8 / grey8 / grey16 round trip through a decoder', async () => {
  const w = 5, h = 3;
  const rgba = new Uint8Array(w * h * 4).map((_, i) => (i * 37) & 255);
  const png = await encodeRGBA8(w, h, rgba);
  assert.deepEqual([...png.subarray(0, 8)], [137, 80, 78, 71, 13, 10, 26, 10]);
  const dec = decodePNG(png);
  assert.equal(dec.width, w); assert.equal(dec.height, h); assert.equal(dec.colorType, 6);
  assert.deepEqual([...dec.data], [...rgba]);
  const g8 = new Uint8Array(w * h).map((_, i) => i * 17);
  const d8 = decodePNG(await encodeGray8(w, h, g8));
  assert.equal(d8.data[4 * 7], g8[7]);
  const g16 = new Uint16Array(w * h).map((_, i) => i * 4000);
  const p16 = await encodeGray16(w, h, g16);
  // IHDR: bit depth 16, colour type 0
  assert.equal(p16[24], 16); assert.equal(p16[25], 0);
});

test('EXR: header and pixel layout', () => {
  const w = 3, h = 2;
  const z = new Float32Array([1, 2, 3, 4, 5, 6]);
  const a = new Float32Array([1, 0, 1, 0, 1, 0]);
  const exr = encodeEXR(w, h, [{ name: 'Z', data: z }, { name: 'A', data: a, half: true }]);
  const dv = new DataView(exr.buffer);
  assert.equal(dv.getUint32(0, true), 0x01312f76, 'magic');
  assert.equal(dv.getInt32(4, true), 2, 'version');
  const text = new TextDecoder('latin1').decode(exr);
  assert.ok(text.indexOf('channels\0chlist\0') > 0);
  assert.ok(text.indexOf('A\0') < text.indexOf('Z\0'), 'channels sorted');
  // last scanline: y=1, size = 3*(2+4) = 18, then A halves then Z floats
  const lineBytes = w * 6;
  const end = exr.length;
  const last = end - (8 + lineBytes);
  assert.equal(dv.getInt32(last, true), 1);
  assert.equal(dv.getInt32(last + 4, true), lineBytes);
  assert.equal(dv.getUint16(last + 8, true), 0x0000, 'A[1,0] = 0 as half');
  assert.equal(dv.getUint16(last + 10, true), 0x3c00, 'A[1,1] = 1.0 as half');
  assert.equal(dv.getFloat32(last + 8 + 6, true), 4, 'Z[1,0]');
  assert.equal(dv.getFloat32(last + 8 + 6 + 8, true), 6, 'Z[1,2]');
  // offset table points at the first scanline
  const headerEnd = exr.indexOf(0, text.indexOf('screenWindowWidth\0float\0') + 'screenWindowWidth\0float\0'.length + 8);
  const firstOffset = Number(dv.getBigUint64(headerEnd + 1, true));
  assert.equal(dv.getInt32(firstOffset, true), 0, 'first block is y = 0');
  const plate = syntheticPlate(16);
  const full = plateToEXR(plate, { mode: 'full' });
  assert.ok(full.length > 16 * 8 * (4 + 2 * 6));
});

test('SDF: edge at 128, far sky bright, interior dark', () => {
  const plate = syntheticPlate(64);
  const sdf = signedDistanceField(plate);
  const W = 64, H = 32;
  // the first covered row at x = 40 is y = 16: its sdf is -1 (one pixel inside), the sky pixel above it +1
  assert.equal(sdf[16 * W + 40], -1);
  assert.equal(sdf[15 * W + 40], 1);
  assert.equal(sdf[0 * W + 40], 16);
  assert.equal(sdf[31 * W + 40], -16);
  const g = sdfToGray8(sdf, 16);
  assert.equal(g[0 * W + 40], 255);
  assert.equal(g[31 * W + 40], 1);
  assert.equal(g[16 * W + 40], 120);
  // the tower's side: pixel just right of the tower at its mid height is 1 px from a feature
  assert.equal(sdf[10 * W + 20], 1);
  const dt = distanceTransform(4, 1, (i) => i === 0);
  assert.deepEqual([...dt], [0, 1, 4, 9]);
});

test('depth16, normals and shading encode as specified', () => {
  const plate = syntheticPlate(64);
  const d16 = depthToGray16(plate, 1000);
  assert.equal(d16[0], 65535, 'sky');
  const i = 20 * 64 + 40;
  const expected = Math.round(Math.log(1 + plate.depth[i]) / Math.log(1 + 1000) * 65535);
  assert.equal(d16[i], expected);
  const nrm = normalToRGB8(plate);
  assert.deepEqual([...nrm.subarray(i * 3, i * 3 + 3)], [128, 128, 255]);
  const rgba = shadePlate(plate, { mode: 'flat' });
  assert.deepEqual([...rgba.subarray(i * 4, i * 4 + 4)], [255, 255, 255, 255]);
  assert.equal(rgba[3], 0, 'sky alpha 0');
  const shaded = shadePlate(plate, { mode: 'shaded', sun: { azimuth: 0, elevation: 90 }, hazeDistance: 1e9 });
  assert.ok(shaded[i * 4] > 100 && shaded[i * 4 + 3] === 255);
});

test('SVG: one polygon per layer, horizon rows in pixel space', () => {
  const plate = syntheticPlate(64);
  const svg = plateToSVG(plate, { bands: [2000, 8000], maxDistance: 50000, lat: 52, lon: 5, headingOffset: 0 });
  assert.ok(svg.startsWith('<?xml'));
  assert.equal((svg.match(/<path /g) || []).length, 3);
  assert.ok(svg.includes('viewBox="0 0 64 32"'));
  // the first path starts at the bottom-left corner and reaches row 16 (the flat horizon) at x = 0.5
  assert.ok(/M0,32 L0,16\.00 L0\.50,16\.00/.test(svg), svg.slice(0, 400));
  const pts = [[0, 0], [1, 0.1], [2, 0], [3, 5], [4, 0]];
  assert.deepEqual(simplify(pts, 0.5), [[0, 0], [2, 0], [3, 5], [4, 0]]);
});
