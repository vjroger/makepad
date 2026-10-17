// UI: parameters, map picker, worker orchestration, GPU/CPU render, preview, exports.

import { GpuSkylineRenderer } from './gpu/renderer.js';
import { mergeParams } from './engine/index.js';
import { DEFAULT_OVERPASS_URLS } from './engine/buildings.js';
import { encodeRGBA8, encodeRGB8, encodeGray8, encodeGray16 } from './export/png.js';
import { plateToEXR } from './export/exr.js';
import { shadePlate, depthToGray16, normalToRGB8, signedDistanceField, sdfToGray8 } from './export/layers.js';
import { plateToSVG } from './export/svg.js';
import { saveBlob } from './export/download.js';

const $ = (id) => document.getElementById(id);
const DEG = Math.PI / 180;
// URL overrides for self-hosted data: ?tileUrl=https://host/{z}/{x}/{y}.png&overpass=https://host/api/interpreter&nocache=1
const QUERY = new URLSearchParams(location.search);

let plate = null, sceneInfo = null, rgba = null, worker = null, gpu = null, busy = false;

// --- map -----------------------------------------------------------------------------------
let map = null, marker = null;
function setLatLon(lat, lon, pan = false) {
  $('lat').value = lat.toFixed(6); $('lon').value = lon.toFixed(6);
  if (map) {
    if (!marker) marker = L.marker([lat, lon], { draggable: true }).addTo(map).on('dragend', () => { const p = marker.getLatLng(); setLatLon(p.lat, p.lng); });
    else marker.setLatLng([lat, lon]);
    if (pan) map.setView([lat, lon], Math.max(map.getZoom(), 12));
  }
}
try {
  if (typeof L !== 'undefined') {
    map = L.map('map', { zoomControl: true }).setView([+$('lat').value, +$('lon').value], 11);
    L.tileLayer('https://tile.openstreetmap.org/{z}/{x}/{y}.png', { maxZoom: 19, attribution: '© OpenStreetMap' }).addTo(map);
    map.on('click', (e) => setLatLon(e.latlng.lat, e.latlng.lng));
    setLatLon(+$('lat').value, +$('lon').value);
  } else {
    $('map').textContent = 'Map library not loaded (offline?). Type coordinates below.';
    $('map').style.padding = '12px';
  }
} catch (err) { $('map').textContent = 'Map unavailable: ' + err.message; }
for (const id of ['lat', 'lon']) $(id).addEventListener('change', () => setLatLon(+$('lat').value, +$('lon').value, true));
$('go').addEventListener('click', geocode);
$('q').addEventListener('keydown', (e) => { if (e.key === 'Enter') geocode(); });
async function geocode() {
  const q = $('q').value.trim();
  if (!q) return;
  setStatus('searching…');
  try {
    const r = await fetch('https://nominatim.openstreetmap.org/search?format=json&limit=1&q=' + encodeURIComponent(q));
    const j = await r.json();
    if (!j.length) { setStatus('no result for "' + q + '"'); return; }
    setLatLon(+j[0].lat, +j[0].lon, true);
    setStatus(j[0].display_name);
  } catch (err) { setStatus('search failed: ' + err.message); }
}

// --- params ----------------------------------------------------------------------------------
function readParams() {
  const alt = $('eyeAltitude').value;
  return mergeParams({
    lat: +$('lat').value, lon: +$('lon').value,
    eyeHeight: +$('eyeHeight').value, eyeAltitude: alt === '' ? null : +alt, eyeOnRoof: $('eyeOnRoof').checked,
    width: +$('width').value, headingOffset: +$('heading').value, verticalExaggeration: +$('vexag').value,
    maxDistance: +$('maxDistance').value, quality: $('quality').value,
    buildings: { enabled: $('bEnabled').checked, allRadius: +$('bAll').value, tallRadius: +$('bTall').value, tallThreshold: +$('bThr').value, heightScale: +$('bScale').value },
    sun: { azimuth: +$('sunAz').value, elevation: +$('sunEl').value }, hazeDistance: +$('haze').value,
  });
}
function shadeOpts() { return { mode: $('mode').value, sun: { azimuth: +$('sunAz').value, elevation: +$('sunEl').value }, hazeDistance: +$('haze').value }; }

function setStatus(msg, cls = '') { $('status').textContent = msg; $('status').className = cls; }
function setProgress(f) { $('progress').firstElementChild.style.width = (Math.max(0, Math.min(1, f)) * 100).toFixed(1) + '%'; }

// --- render ----------------------------------------------------------------------------------
$('render').addEventListener('click', render);

function onProgress(p) {
  if (p.stage === 'tiles') { setProgress(0.4 * p.done / p.total); setStatus(`terrain tiles ${p.done}/${p.total} (zoom ${p.zoom})`); }
  else if (p.stage === 'tiers') { setProgress(0.4 + 0.1 * p.done / p.total); setStatus(`building height tiers ${p.done}/${p.total}`); }
  else if (p.stage === 'tier') { setStatus(`resampling ${p.texel} m tier ${(p.fraction * 100).toFixed(0)}%`); }
  else if (p.stage === 'buildings') { setProgress(0.55); setStatus('buildings: ' + (p.message || '')); }
  else if (p.stage === 'sweep') { setProgress(0.6 + 0.4 * p.fraction); setStatus(`CPU sweep ${(p.fraction * 100).toFixed(0)}%`); }
  else if (p.stage === 'gpu') { setProgress(0.6 + 0.4 * p.fraction); setStatus('GPU ' + (p.message || '')); }
}

async function render() {
  if (busy) return;
  busy = true; $('render').disabled = true; setProgress(0);
  for (const b of document.querySelectorAll('.toolbar button')) b.disabled = true;
  const params = readParams();
  const engine = $('engine').value;
  const ov = $('overpass').value;
  // 'auto': the same-origin proxy of `skyline --serve` first (falls through at once on other servers), then the public mirrors
  const hooks = { overpassUrls: ov === 'auto' ? [location.origin + '/overpass', ...DEFAULT_OVERPASS_URLS] : [ov], useCache: !QUERY.has('nocache') };
  if (QUERY.get('tileUrl')) hooks.tileUrl = QUERY.get('tileUrl');
  if (QUERY.get('overpass')) hooks.overpassUrls = [QUERY.get('overpass')];
  const t0 = performance.now();
  try {
    if (worker) worker.terminate();
    worker = new Worker(new URL('./worker/data-worker.js', import.meta.url), { type: 'module' });
    const result = await new Promise((resolve, reject) => {
      worker.onmessage = (ev) => {
        const m = ev.data;
        if (m.type === 'progress') onProgress(m);
        else if (m.type === 'error') reject(new Error(m.message));
        else resolve(m);
      };
      worker.onerror = (e) => reject(new Error(e.message || 'worker error'));
      worker.postMessage({ type: 'prepare', params, hooks, render: engine });
    });
    if (result.type === 'plate') {
      plate = result.plate; sceneInfo = result.sceneInfo;
    } else {
      sceneInfo = { ...result.scene, tiers: result.scene.tierInfo };
      if (!gpu) gpu = new GpuSkylineRenderer(document.createElement('canvas'));
      gpu.uploadScene({ tiers: result.scene.tiers, eyeZ: result.scene.eyeZ, zGround: result.scene.zGround, hMax: result.scene.hMax });
      setStatus('GPU march…');
      plate = await gpu.render(params, { onProgress, yieldBetweenStrips: true });
      gpu.disposeTiers();
    }
    plate.params = { ...params, eyeZ: sceneInfo.eyeZ };
    const total = ((performance.now() - t0) / 1000).toFixed(1);
    const s = sceneInfo.stats;
    $('stats').textContent =
      `eye ${sceneInfo.eyeZ.toFixed(1)} m ASL (ground ${sceneInfo.zGround.toFixed(1)} m) · tiles ${s.tiles}${s.tilesMissing ? ` (${s.tilesMissing} missing)` : ''} · buildings ${s.buildings.count} rasterised` +
      `${s.buildings.skipped ? `, ${s.buildings.skipped} skipped (contain the eye)` : ''} · tiers ${sceneInfo.tiers.map((t) => t.texel + 'm').join('/')}\n` +
      `render ${(plate.renderMs / 1000).toFixed(1)} s (${engine}), total ${total} s` + (sceneInfo.warnings?.length ? `\n⚠ ${sceneInfo.warnings.join('\n⚠ ')}` : '');
    setProgress(1); setStatus('done — hover the plate, export below.');
    updatePreview();
    for (const b of document.querySelectorAll('.toolbar button')) b.disabled = false;
  } catch (err) {
    console.error(err);
    setStatus('failed: ' + err.message, 'warn');
  } finally {
    busy = false; $('render').disabled = false;
  }
}

// --- preview ---------------------------------------------------------------------------------
const canvas = $('preview');
let previewScale = 1;
function updatePreview() {
  if (!plate) return;
  rgba = shadePlate(plate, shadeOpts());
  const W = plate.width, H = plate.height;
  const maxW = 4096;
  previewScale = W > maxW ? maxW / W : 1;
  canvas.width = Math.round(W * previewScale); canvas.height = Math.round(H * previewScale);
  const ctx = canvas.getContext('2d');
  const img = new ImageData(new Uint8ClampedArray(rgba.buffer, rgba.byteOffset, rgba.length), W, H);
  if (previewScale === 1) ctx.putImageData(img, 0, 0);
  else createImageBitmap(img).then((bmp) => { ctx.clearRect(0, 0, canvas.width, canvas.height); ctx.drawImage(bmp, 0, 0, canvas.width, canvas.height); bmp.close(); });
}
$('compositeToggle').addEventListener('change', () => { $('view').classList.toggle('sky', $('compositeToggle').checked); });
for (const id of ['mode', 'sunAz', 'sunEl', 'haze']) $(id).addEventListener('change', updatePreview);

canvas.addEventListener('mousemove', (e) => {
  if (!plate) return;
  const r = canvas.getBoundingClientRect();
  const x = Math.floor((e.clientX - r.left) / r.width * plate.width), y = Math.floor((e.clientY - r.top) / r.height * plate.height);
  if (x < 0 || y < 0 || x >= plate.width || y >= plate.height) return;
  const az = ((x + 0.5) / plate.width) * 360 - 180 + (plate.params.headingOffset || 0);
  const el = (0.5 - (y + 0.5) / plate.height) * 180;
  const i = y * plate.width + x;
  const cls = ['sky', 'terrain', 'building'][plate.classId[i]];
  const d = plate.coverage[i] ? `${plate.depth[i] >= 1000 ? (plate.depth[i] / 1000).toFixed(2) + ' km' : plate.depth[i].toFixed(1) + ' m'} · alt ${plate.altitude[i].toFixed(0)} m` : '—';
  $('readout').textContent = `az ${az.toFixed(2)}°  el ${el.toFixed(2)}°  ${cls}  ${d}\nhorizon here: ${(plate.horizon[x] / DEG).toFixed(3)}° at ${(plate.horizonDistance[x] / 1000).toFixed(1)} km`;
});

// --- exports ---------------------------------------------------------------------------------
function stem() { const p = plate.params; return `skyline_${p.lat.toFixed(4)}_${p.lon.toFixed(4)}_${plate.width}`; }
async function withStatus(label, fn) {
  if (!plate || busy) return;
  busy = true; setStatus(`encoding ${label}…`);
  try { await new Promise((r) => setTimeout(r, 10)); await fn(); setStatus(`${label} saved.`); }
  catch (err) { console.error(err); setStatus('export failed: ' + err.message, 'warn'); }
  finally { busy = false; }
}
$('dl-rgba').addEventListener('click', () => withStatus('RGBA PNG', async () => {
  const data = shadePlate(plate, shadeOpts());
  saveBlob(await encodeRGBA8(plate.width, plate.height, data), `${stem()}_rgba.png`, 'image/png');
}));
$('dl-mask').addEventListener('click', () => withStatus('mask PNG', async () => {
  const data = shadePlate(plate, { mode: 'flat' });
  saveBlob(await encodeRGBA8(plate.width, plate.height, data), `${stem()}_mask.png`, 'image/png');
}));
$('dl-depth').addEventListener('click', () => withStatus('depth PNG', async () => {
  saveBlob(await encodeGray16(plate.width, plate.height, depthToGray16(plate, plate.params.maxDistance)), `${stem()}_depth16.png`, 'image/png');
}));
$('dl-normal').addEventListener('click', () => withStatus('normal PNG', async () => {
  saveBlob(await encodeRGB8(plate.width, plate.height, normalToRGB8(plate)), `${stem()}_normal.png`, 'image/png');
}));
$('dl-sdf').addEventListener('click', () => withStatus('SDF PNG', async () => {
  const spread = +$('sdfSpread').value || 64;
  saveBlob(await encodeGray8(plate.width, plate.height, sdfToGray8(signedDistanceField(plate), spread)), `${stem()}_sdf${spread}.png`, 'image/png');
}));
$('dl-exr').addEventListener('click', () => withStatus('EXR', async () => {
  saveBlob(plateToEXR(plate, { mode: 'full', skyDepth: plate.params.maxDistance * 10 }), `${stem()}.exr`, 'image/x-exr');
}));
$('dl-exr-compact').addEventListener('click', () => withStatus('EXR (A+Z)', async () => {
  saveBlob(plateToEXR(plate, { mode: 'compact', skyDepth: plate.params.maxDistance * 10 }), `${stem()}_az.exr`, 'image/x-exr');
}));
$('dl-svg').addEventListener('click', () => withStatus('SVG', async () => {
  saveBlob(plateToSVG(plate, plate.params), `${stem()}_bands.svg`, 'image/svg+xml');
}));
$('dl-json').addEventListener('click', () => withStatus('JSON', async () => {
  const p = plate.params;
  const meta = {
    generator: 'skyline-hdri 0.1', generated: new Date().toISOString(),
    viewpoint: { lat: p.lat, lon: p.lon, eyeAltitude: p.eyeZ, eyeHeight: p.eyeHeight }, plate: { width: plate.width, height: plate.height, headingOffset: p.headingOffset, verticalExaggeration: p.verticalExaggeration },
    layout: 'equirectangular; x: azimuth -180..180 deg (centre = north, east right); y: elevation +90 (top) .. -90 (bottom)',
    depth: 'metres from the eye to the first hit (3-D distance); 16-bit PNG: v = 65535*ln(1+d)/ln(1+maxDistance), sky = 65535', maxDistance: p.maxDistance, bands: p.bands,
    curvature: { earthRadius: 6371000, refractionK: 0.13 }, buildings: p.buildings,
    stats: sceneInfo?.stats, tiers: sceneInfo?.tiers,
    attribution: ['Terrain: Mapzen / AWS Terrain Tiles (https://github.com/tilezen/joerd/blob/master/docs/attribution.md)', 'Buildings: © OpenStreetMap contributors, ODbL'],
  };
  saveBlob(JSON.stringify(meta, null, 2), `${stem()}.json`, 'application/json');
}));

if (!GpuSkylineRenderer.isSupported()) { $('engine').value = 'cpu'; setStatus('WebGL2 float render targets unavailable: using the CPU engine.', 'warn'); }
