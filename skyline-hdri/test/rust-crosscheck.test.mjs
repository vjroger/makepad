// Cross-check: the Rust CLI's plate must match the JavaScript CPU sweep on the synthetic world.
// Builds the Rust binary (release) if cargo is available; skips otherwise.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { spawnSync, spawn } from 'node:child_process';
import { existsSync, readFileSync, mkdtempSync } from 'node:fs';
import { tmpdir } from 'node:os';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { startServer } from './fixtures/server.mjs';
import { decodePNG } from './png-decode.mjs';
import { EYE } from './fixtures/synthetic-world.mjs';
import { prepareScene, renderCPU } from '../src/engine/index.js';

const ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const W = 1024;

test('Rust sweep matches the JS sweep', { timeout: 900000 }, async (t) => {
  const cargo = spawnSync('cargo', ['--version']);
  if (cargo.status !== 0) { t.skip('cargo not available'); return; }
  const build = spawnSync('cargo', ['build', '--release', '--quiet'], { cwd: path.join(ROOT, 'rust'), stdio: 'inherit' });
  assert.equal(build.status, 0, 'cargo build');
  const bin = path.join(ROOT, 'rust/target/release/skyline');
  assert.ok(existsSync(bin));
  const srv = await startServer();
  try {
    const out = mkdtempSync(path.join(tmpdir(), 'skyline-'));
    const args = ['--lat', String(EYE.lat), '--lon', String(EYE.lon), '--width', String(W), '--max-km', '150',
      '--tile-url', `${srv.base}/terrain/{z}/{x}/{y}.png`, '--overpass', `${srv.base}/overpass`, '--cache', path.join(out, 'cache'),
      '--out', out, '--formats', 'raw,rgba,depth,exr,svg,json,sdf,normal,mask'];
    const t0 = Date.now();
    // spawn asynchronously: the mock server lives in this process and must keep serving
    const status = await new Promise((resolve) => { const c = spawn(bin, args, { stdio: ['ignore', 'inherit', 'pipe'] }); let err = ''; c.stderr.on('data', (d) => { err += d; }); c.on('close', (code) => { console.log(err.split('\n').filter((l) => !l.startsWith('tiles') && l.trim()).join('\n')); resolve(code); }); });
    assert.equal(status, 0, 'skyline exit code');
    console.log(`rust run ${Date.now() - t0} ms`);
    const stem = `skyline_${EYE.lat.toFixed(4)}_${EYE.lon.toFixed(4)}_${W}`;
    const raw = readFileSync(path.join(out, stem + '.raw'));
    const nl = raw.indexOf(10);
    const [tag, w, h, nb] = raw.subarray(0, nl).toString().split(' ');
    assert.equal(tag, 'skyline-raw'); assert.equal(+w, W);
    const n = W * (W / 2);
    let off = nl + 1;
    const coverage = raw.subarray(off, off + n); off += n;
    const classId = raw.subarray(off, off + n); off += n;
    const f32 = (count) => { const a = new Float32Array(count); for (let i = 0; i < count; i++) a[i] = raw.readFloatLE(off + i * 4); off += count * 4; return a; };
    const depth = f32(n), altitude = f32(n), normal = f32(n * 3), bands = f32(+nb * W), horizon = f32(W);

    // the JS reference with identical parameters
    const decode = async (resp) => decodePNG(new Uint8Array(await resp.arrayBuffer())).data;
    const scene = await prepareScene({ lat: EYE.lat, lon: EYE.lon, width: W, maxDistance: 150000, eyeHeight: 1.7,
      buildings: { enabled: true, allRadius: 2500, tallRadius: 30000, tallThreshold: 35 } },
      { tileUrl: srv.base + '/terrain/{z}/{x}/{y}.png', overpassUrls: [srv.base + '/overpass'], decode, useCache: false, concurrency: 16 });
    const js = renderCPU(scene);

    let covMismatch = 0, clsMismatch = 0, both = 0, maxRel = 0, maxNrm = 0, maxAlt = 0;
    for (let i = 0; i < n; i++) {
      const a = coverage[i] > 0, b = js.coverage[i] > 0;
      if (a !== b) { covMismatch++; continue; }
      if (!a) continue;
      both++;
      if (classId[i] !== js.classId[i]) clsMismatch++;
      maxRel = Math.max(maxRel, Math.abs(depth[i] - js.depth[i]) / js.depth[i]);
      maxAlt = Math.max(maxAlt, Math.abs(altitude[i] - js.altitude[i]));
      const dot = normal[i * 3] * js.normal[i * 3] + normal[i * 3 + 1] * js.normal[i * 3 + 1] + normal[i * 3 + 2] * js.normal[i * 3 + 2];
      maxNrm = Math.max(maxNrm, Math.acos(Math.max(-1, Math.min(1, dot))) * 180 / Math.PI);
    }
    let maxBand = 0, maxHor = 0;
    for (let i = 0; i < bands.length; i++) maxBand = Math.max(maxBand, Math.abs(bands[i] - js.bands[i]));
    for (let x = 0; x < W; x++) maxHor = Math.max(maxHor, Math.abs(horizon[x] - js.horizon[x]));
    console.log({ both, covMismatch, clsMismatch, maxRel, maxAlt, maxNrmDeg: maxNrm, maxBandDeg: maxBand * 180 / Math.PI, maxHorizonDeg: maxHor * 180 / Math.PI });
    assert.equal(covMismatch, 0);
    assert.equal(clsMismatch, 0);
    assert.ok(maxRel < 1e-5, `max relative depth error ${maxRel}`);
    assert.ok(maxNrm < 0.05, `max normal error ${maxNrm} deg (acos of a float32 dot product near 1 is ~0.03 deg)`);
    assert.ok(maxBand < 1e-6 && maxHor < 1e-6);
    // the other outputs exist and start with the right magic bytes
    for (const [name, magic] of [[stem + '_rgba.png', [137, 80, 78, 71]], [stem + '.exr', [0x76, 0x2f, 0x31, 0x01]], [stem + '_bands.svg', [0x3c, 0x3f]], [stem + '_depth16.png', [137, 80]]]) {
      const b = readFileSync(path.join(out, name));
      assert.deepEqual([...b.subarray(0, magic.length)], magic, name);
    }
    // the Rust PNG decodes with the test decoder and matches the JS shading
    const img = decodePNG(readFileSync(path.join(out, stem + '_rgba.png')));
    assert.equal(img.width, W);
    assert.equal(img.data[3], 0, 'sky transparent');
    assert.equal(img.data[((W / 2 - 1) * W + 10) * 4 + 3], 255, 'nadir opaque');
  } finally {
    await srv.close();
  }
});
