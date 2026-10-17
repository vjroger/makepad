// Engine tests against the synthetic world. Run: node --test test/
import { test, before, after } from 'node:test';
import assert from 'node:assert/strict';
import { startServer } from './fixtures/server.mjs';
import { decodePNG } from './png-decode.mjs';
import { EYE, PLAIN, CONE, HILL, BUILDINGS, MAST, elevationAt, encodeTerrarium } from './fixtures/synthetic-world.mjs';
import { prepareScene, renderCPU } from '../src/engine/index.js';
import { decodeTerrarium } from '../src/engine/tiles.js';
import { destination, flatOffset, toLocal, apparentElevation, DEG, EFFECTIVE_RADIUS } from '../src/engine/geo.js';
import { sampleTier, tierIndexFor } from '../src/engine/tiers.js';
import { parseLength, estimateHeight, parseBuildings } from '../src/engine/buildings.js';
import { overpassFixture } from './fixtures/synthetic-world.mjs';

let srv, scene, plate;
const W = 2048, H = 1024;
const EYE_Z = PLAIN + 1.7;

const colOf = (bearingDeg) => Math.round(((((bearingDeg + 180) % 360) + 360) % 360) / 360 * W - 0.5);
const rowAngle = (y) => (0.5 - (y + 0.5) / H) * Math.PI;
const topCoveredRow = (x) => { for (let y = 0; y < H; y++) if (plate.coverage[y * W + x]) return y; return -1; };
const horizonDeg = (x) => rowAngle(topCoveredRow(x)) / DEG;
const expectedAngle = (h, d) => apparentElevation(h, d, EYE_Z) / DEG;
const PX = 360 / W; // degrees per pixel

before(async () => {
  srv = await startServer();
  const decode = async (resp) => decodePNG(new Uint8Array(await resp.arrayBuffer())).data;
  scene = await prepareScene({
    lat: EYE.lat, lon: EYE.lon, width: W, maxDistance: 150000, eyeHeight: 1.7,
    buildings: { enabled: true, allRadius: 2500, tallRadius: 30000, tallThreshold: 35 },
  }, { tileUrl: srv.base + '/terrain/{z}/{x}/{y}.png', overpassUrls: [srv.base + '/overpass'], decode, useCache: false, concurrency: 16 });
  plate = renderCPU(scene);
});

after(async () => { await srv.close(); });

test('terrarium encode/decode round trip', () => {
  const rgba = new Uint8Array(256 * 256 * 4);
  for (let i = 0; i < 256 * 256; i++) {
    const h = (i % 1000) * 7.25 - 500;
    const [r, g, b] = encodeTerrarium(h);
    rgba[i * 4] = r; rgba[i * 4 + 1] = g; rgba[i * 4 + 2] = b; rgba[i * 4 + 3] = 255;
  }
  const out = decodeTerrarium(rgba);
  for (let i = 0; i < 256 * 256; i += 97) assert.ok(Math.abs(out[i] - ((i % 1000) * 7.25 - 500)) < 1 / 256 + 1e-6);
});

test('geodesy: destination vs flat offset agree in the near field', () => {
  const [lat, lon] = destination(52, 5, 45 * DEG, 5000);
  const [lat2, lon2] = flatOffset(52, 5, 5000 * Math.SQRT1_2, 5000 * Math.SQRT1_2);
  const [e, n] = toLocal(52, 5, lat, lon);
  assert.ok(Math.hypot(e - 5000 * Math.SQRT1_2, n - 5000 * Math.SQRT1_2) < 0.01, 'toLocal inverts destination');
  assert.ok(Math.abs(lat - lat2) < 5e-5 && Math.abs(lon - lon2) < 5e-5, 'flat offset within ~3 m at 5 km');
});

test('scene: tiers, eye and buildings', () => {
  assert.equal(scene.tiers.length, 5, 'tiers 0..4 for 150 km');
  assert.ok(Math.abs(scene.zGround - PLAIN) < 0.05, `ground at eye ${scene.zGround}`);
  assert.ok(Math.abs(scene.eyeZ - EYE_Z) < 0.05);
  assert.equal(scene.stats.tilesMissing, 0);
  assert.equal(scene.stats.buildings.fetched, 5);
  assert.equal(scene.stats.buildings.skipped, 1, 'the footprint containing the eye is skipped');
  assert.equal(scene.stats.buildings.count, 4);
  // cone peak sampled from the tier that covers 20 km
  const [plat, plon] = destination(EYE.lat, EYE.lon, CONE.bearing * DEG, CONE.distance);
  const [pe, pn] = toLocal(EYE.lat, EYE.lon, plat, plon);
  const k = tierIndexFor(scene.tiers, pe, pn);
  const h = sampleTier(scene.tiers[k], pe, pn);
  assert.ok(Math.abs(h - CONE.height) < CONE.height * 0.03, `cone peak sampled ${h.toFixed(1)} m in tier ${k}`);
  // sea west of 3 km is clamped to 0
  assert.equal(sampleTier(scene.tiers[2], -5000, 0), 0);
  assert.ok(Math.abs(sampleTier(scene.tiers[2], 5000, 0) - PLAIN) < 0.05);
});

test('height estimation and parsing', () => {
  assert.equal(parseLength('25'), 25);
  assert.equal(parseLength('25 m'), 25);
  assert.equal(parseLength('25,5'), 25.5);
  assert.ok(Math.abs(parseLength("82 ft") - 24.99) < 0.01);
  assert.ok(Math.abs(parseLength("82'") - 24.99) < 0.01);
  assert.equal(parseLength('tall'), null);
  assert.equal(estimateHeight({ 'building:levels': '10' }), 30);
  assert.equal(estimateHeight({ 'building:levels': '10', 'roof:levels': '1' }), 32.5);
  assert.equal(estimateHeight({ building: 'church' }), 20);
  assert.equal(estimateHeight({ building: 'yes' }, 3000), 12);
  assert.equal(estimateHeight({ man_made: 'mast' }), 60);
  const parsed = parseBuildings(overpassFixture().elements, EYE.lat, EYE.lon);
  const byH = parsed.map((b) => b.height).sort((a, b) => a - b);
  assert.deepEqual(byH, [30, 40, 100, 150, 300]);
  const rel = parsed.find((b) => b.height === 40);
  assert.equal(rel.polys.length, 1);
  assert.ok(Math.abs(rel.centroid[1] + 1000) < 1);
});

test('horizon: the cone peak at 20 km NE sits at the right elevation and distance', () => {
  const x = colOf(CONE.bearing);
  const exp = expectedAngle(CONE.height, CONE.distance);
  const got = horizonDeg(x);
  assert.ok(Math.abs(got - exp) < 1.5 * PX, `cone horizon ${got.toFixed(3)} vs expected ${exp.toFixed(3)} deg`);
  const y = topCoveredRow(x);
  const depth = plate.depth[y * W + x];
  assert.ok(Math.abs(depth - Math.hypot(CONE.distance, CONE.height - EYE_Z)) < 400, `depth ${depth.toFixed(0)}`);
  assert.equal(plate.classId[y * W + x], 1, 'terrain');
  assert.ok(Math.abs(plate.altitude[y * W + x] - CONE.height) < 150, `hit altitude ${plate.altitude[y * W + x].toFixed(0)}`);
  // rows above the horizon are sky
  assert.equal(plate.coverage[(y - 2) * W + x], 0);
  assert.equal(plate.depth[(y - 2) * W + x], Infinity);
});

test('buildings: tower, block, relation and mast are hit at the right angle, distance and class', () => {
  const checks = [
    { name: 'tower', bearing: 90, near: 490, h: 100 },
    { name: 'block', bearing: 0, near: 270, h: 30 },
    { name: 'relation', bearing: 180, near: 975, h: 40 },
  ];
  for (const c of checks) {
    const x = colOf(c.bearing);
    const exp = expectedAngle(PLAIN + c.h, c.near);
    const got = horizonDeg(x);
    assert.ok(Math.abs(got - exp) < 1.5 * PX, `${c.name}: horizon ${got.toFixed(3)} vs ${exp.toFixed(3)} deg`);
    const y = topCoveredRow(x);
    assert.equal(plate.classId[y * W + x], 2, `${c.name}: class building`);
    const depth = plate.depth[y * W + x];
    assert.ok(Math.abs(depth - Math.hypot(c.near, PLAIN + c.h - EYE_Z)) < 6, `${c.name}: depth ${depth.toFixed(1)}`);
    // the facade normal is nearly horizontal and points back at the eye
    const nx = plate.normal[(y * W + x) * 3], ny = plate.normal[(y * W + x) * 3 + 1], nz = plate.normal[(y * W + x) * 3 + 2];
    const b = c.bearing * DEG;
    const toward = -(nx * Math.sin(b) + ny * Math.cos(b));
    assert.ok(nz < 0.5 && toward > 0.7, `${c.name}: facade normal (${nx.toFixed(2)}, ${ny.toFixed(2)}, ${nz.toFixed(2)})`);
  }
  // mast at 2 km bearing 30 deg: check the best of the three columns around it
  const bearing = Math.atan2(MAST.local[0], MAST.local[1]) / DEG;
  const d = Math.hypot(MAST.local[0], MAST.local[1]);
  const exp = expectedAngle(PLAIN + 150, d - 4);
  let best = -90;
  for (let dx = -1; dx <= 1; dx++) best = Math.max(best, horizonDeg(colOf(bearing) + dx));
  assert.ok(Math.abs(best - exp) < 2 * PX, `mast horizon ${best.toFixed(3)} vs ${exp.toFixed(3)}`);
});

test('curvature: the 100 m hill at 100 km south is hidden, the plain horizon dips below zero', () => {
  const x = colOf(180);
  // above the relation building (40 m at 1 km) everything must be sky up to the zenith
  const y = topCoveredRow(x);
  for (let yy = 0; yy < y; yy++) assert.equal(plate.coverage[yy * W + x], 0);
  // bands: the horizon reached within 2 km is the building top; by 30 km the hill is still invisible
  const b0 = plate.bands[0 * W + x] / DEG;
  assert.ok(Math.abs(b0 - expectedAngle(PLAIN + 40, 975)) < 0.05, `band 0 = ${b0}`);
  const hillAngle = expectedAngle(HILL.height, HILL.distance);
  assert.ok(hillAngle < 0, 'hill below the true horizontal because of curvature');
  // a column with nothing but plain: the horizon is the curvature dip, about -0.039 deg
  const xPlain = colOf(135);
  const dip = -Math.sqrt(2 * 1.7 / EFFECTIVE_RADIUS) / DEG;
  const bLast = plate.bands[3 * W + xPlain] / DEG;
  assert.ok(Math.abs(bLast - dip) < 0.01, `plain horizon ${bLast.toFixed(4)} vs dip ${dip.toFixed(4)}`);
  // sea to the west: the plain ends at 3 km, horizon slightly lower than the dip
  const xWest = colOf(270);
  const expWest = expectedAngle(PLAIN, 3000);
  const gotWest = plate.bands[3 * W + xWest] / DEG;
  assert.ok(Math.abs(gotWest - expWest) < 0.01, `west horizon ${gotWest.toFixed(4)} vs ${expWest.toFixed(4)}`);
});

test('plate layout: full sphere, ground covered below the horizon, sky above', () => {
  const x = colOf(135);
  assert.equal(plate.coverage[(H - 1) * W + x], 255, 'nadir is ground');
  assert.equal(plate.coverage[0 * W + x], 0, 'zenith is sky');
  const yh = topCoveredRow(x);
  assert.ok(Math.abs(yh - H / 2) <= 1, `horizon row ${yh} near the middle row`);
  // nadir depth is the eye height (looking straight down at the plain)
  assert.ok(Math.abs(plate.depth[(H - 1) * W + x] - 1.7) < 0.05, `nadir depth ${plate.depth[(H - 1) * W + x]}`);
  const n = (H - 1) * W + x;
  assert.ok(plate.normal[n * 3 + 2] > 0.99, 'flat ground normal is up');
});
