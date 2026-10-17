/**
 * Skyline 360 app: map picker → generate → orbit → viewpoint → 360° HDRI → export.
 */
import * as THREE from 'three';
import type { AreaData, HeightGrid, SkyParams } from '../core/types';
import { LocalFrame, metresPerDegree } from '../core/geo';
import { sampleHeight } from '../core/terrarium';
import { buildSceneMeshes, type SceneMeshes } from '../core/pipeline';
import { DEMO_CENTER, DEMO_RADIUS, DEMO_VIEWPOINT, demoScene } from '../core/demo';
import { SkylineScene, DEFAULT_SKY, DEFAULT_STYLE, type StyleParams } from '../render/scene';
import { captureEquirect, type HdrCapture } from '../render/capture';
import { SphereViewer, drawPreview, type PreviewImage } from '../render/preview';
import { fetchArea, fetchTerrain, geocode, GEOCODER_CREDIT } from './fetchers';
import { buildPreview, download, encodeCaptureExr, encodeCaptureHdr, exportGlb, sourcesText, type RenderMetadata } from './export';

declare const maplibregl: any;

const APP = 'Skyline 360';
const VERSION = '0.1.0';
/** Radius of the coarse horizon terrain, metres. */
const HORIZON_RADIUS = 60000;

// ---------------------------------------------------------------------------
// DOM helpers

const $ = <T extends HTMLElement = HTMLElement>(id: string): T => {
  const el = document.getElementById(id);
  if (!el) throw new Error(`missing #${id}`);
  return el as T;
};
const logEl = $('log');
const statusEl = $('status');
function log(msg: string): void {
  logEl.textContent += (logEl.textContent ? '\n' : '') + msg;
  logEl.scrollTop = logEl.scrollHeight;
  statusEl.textContent = msg;
  console.log('[skyline360]', msg);
}
function fmt(n: number): string {
  return n.toLocaleString('en-US');
}

// ---------------------------------------------------------------------------
// State

interface AppState {
  center: { lon: number; lat: number };
  radius: number;
  viewpoint: { lon: number; lat: number };
  eyeHeight: number;
  heading: number;
  sky: SkyParams;
  styleKey: string;
  demo: boolean;
  area: AreaData | null;
  terrain: HeightGrid | null;
  horizon: HeightGrid | null;
  frame: LocalFrame | null;
  wantHorizon: boolean;
  wantTrees: boolean;
  meshes: SceneMeshes | null;
  sources: string[];
  capture: HdrCapture | null;
  preview: PreviewImage | null;
  meta: RenderMetadata | null;
  busy: boolean;
}

const params = new URLSearchParams(location.search);
const state: AppState = {
  center: { lon: DEMO_CENTER.lon, lat: DEMO_CENTER.lat },
  radius: 1500,
  viewpoint: { lon: DEMO_CENTER.lon + 0.002, lat: DEMO_CENTER.lat + 0.001 },
  eyeHeight: 2,
  heading: 0,
  sky: { ...DEFAULT_SKY },
  styleKey: 'concrete',
  demo: false,
  area: null,
  terrain: null,
  horizon: null,
  frame: null,
  wantHorizon: true,
  wantTrees: true,
  meshes: null,
  sources: [],
  capture: null,
  preview: null,
  meta: null,
  busy: false,
};

const STYLES: Record<string, StyleParams> = {
  concrete: DEFAULT_STYLE,
  white: { ...DEFAULT_STYLE, facade: [0.78, 0.76, 0.72], roof: [0.55, 0.53, 0.5], road: [0.18, 0.18, 0.18] },
  dark: { ...DEFAULT_STYLE, facade: [0.06, 0.07, 0.08], roof: [0.04, 0.04, 0.05], terrain: [0.1, 0.1, 0.09], road: [0.05, 0.05, 0.05] },
};

// ---------------------------------------------------------------------------
// 3D scene and viewers

const view3dCanvas = $<HTMLCanvasElement>('view3d');
const scene = new SkylineScene(view3dCanvas);
const sphereCanvas = $<HTMLCanvasElement>('panoSphere');
let sphere: SphereViewer | null = null;

function viewpointLocal(): { x: number; groundY: number; z: number } {
  if (!state.frame) return { x: 0, groundY: 0, z: 0 };
  const [e, n] = state.frame.toLocal(state.viewpoint.lon, state.viewpoint.lat);
  const groundY = state.terrain ? sampleHeight(state.terrain, e, n) : scene.groundAtOrigin;
  return { x: e, groundY, z: -n };
}

function updateViewpointMarker(): void {
  const v = viewpointLocal();
  scene.setViewpoint(v.x, v.groundY, v.z, state.eyeHeight);
  $('viewInfo').textContent = `eye at ${(v.groundY + state.eyeHeight).toFixed(1)} m ASL · ${Math.hypot(v.x, v.z).toFixed(0)} m from centre`;
}

function setStats(m: SceneMeshes | null): void {
  $('statBuildings').textContent = m ? fmt(m.stats.count) : '–';
  $('statHeights').textContent = m && m.stats.count ? `${Math.round(((m.stats.withHeight + m.stats.withLevels) / m.stats.count) * 100)}%` : '–';
  $('statTrees').textContent = m ? fmt(m.stats.trees) : '–';
  $('statTriangles').textContent = m ? fmt(m.stats.triangles) : '–';
  $('statTallest').textContent = m ? `${Math.round(m.stats.maxHeight)} m` : '–';
  let peak = 0;
  if (m?.horizon) for (let i = 1; i < m.horizon.positions.length; i += 3) peak = Math.max(peak, m.horizon.positions[i]);
  $('statPeak').textContent = m ? (m.horizon ? `${Math.round(peak)} m` : 'off') : '–';
}

function applyMeshes(area: AreaData, terrain: HeightGrid | null, horizon: HeightGrid | null, frame: LocalFrame, radius: number, sources: string[], demo: boolean): void {
  const t0 = performance.now();
  const meshes = buildSceneMeshes(area, terrain, frame, { radius, horizon: state.wantHorizon ? horizon : null, trees: state.wantTrees ? {} : false });
  state.area = area;
  state.terrain = terrain;
  state.horizon = horizon;
  state.frame = frame;
  state.meshes = meshes;
  state.sources = sources;
  state.demo = demo;
  scene.setMeshes(meshes, radius);
  scene.setSky(state.sky); // shadow frustum follows the radius
  setStats(meshes);
  updateViewpointMarker();
  scene.frameArea();
  const lc = meshes.landCover.fractions;
  log(`Built ${fmt(meshes.stats.count)} buildings (${meshes.stats.withHeight} with height, ${meshes.stats.withLevels} with levels, ${meshes.stats.defaulted} estimated), ${fmt(meshes.stats.trees)} trees, ${fmt(meshes.stats.triangles)} triangles in ${((performance.now() - t0) / 1000).toFixed(1)} s`);
  log(`Ground: ${(lc.green * 100).toFixed(0)}% green, ${(lc.water * 100).toFixed(0)}% water, ${(lc.road * 100).toFixed(0)}% roads; horizon terrain ${meshes.horizon ? 'on' : 'off'}`);
  invalidateCapture();
}

function invalidateCapture(): void {
  state.capture?.dispose();
  state.capture = null;
  state.preview = null;
  state.meta = null;
  for (const id of ['dlHdr', 'dlExrHalf', 'dlExrFloat', 'dlPng', 'dlJson']) ($(id) as HTMLButtonElement).disabled = true;
}

// ---------------------------------------------------------------------------
// Map (MapLibre GL JS, OpenFreeMap tiles — display only)

let map: any = null;
let viewMarker: any = null;
let centerMarker: any = null;

interface CircleFeature {
  type: 'Feature';
  properties: Record<string, never>;
  geometry: { type: 'Polygon'; coordinates: [number, number][][] };
}

function circlePolygon(lon: number, lat: number, radius: number): CircleFeature {
  const m = metresPerDegree(lat);
  const coords: [number, number][] = [];
  for (let i = 0; i <= 72; i++) {
    const a = (i / 72) * Math.PI * 2;
    coords.push([lon + (Math.cos(a) * radius) / m.east, lat + (Math.sin(a) * radius) / m.north]);
  }
  return { type: 'Feature', properties: {}, geometry: { type: 'Polygon', coordinates: [coords] } };
}

function updateMapOverlay(): void {
  if (!map) return;
  const src = map.getSource('area');
  if (src) src.setData(circlePolygon(state.center.lon, state.center.lat, state.radius));
  centerMarker?.setLngLat([state.center.lon, state.center.lat]);
  viewMarker?.setLngLat([state.viewpoint.lon, state.viewpoint.lat]);
  $('centerHint').textContent = `Centre ${state.center.lat.toFixed(5)}, ${state.center.lon.toFixed(5)} · click the map to move it`;
}

function initMap(): void {
  if (typeof maplibregl === 'undefined') {
    $('mapEmpty').hidden = false;
    log('MapLibre not available (offline?). Sample city only.');
    return;
  }
  map = new maplibregl.Map({
    container: 'map',
    style: 'https://tiles.openfreemap.org/styles/liberty',
    center: [state.center.lon, state.center.lat],
    zoom: 12.5,
    attributionControl: { compact: false },
  });
  map.addControl(new maplibregl.NavigationControl({ showCompass: true }), 'top-right');
  map.on('load', () => {
    map.addSource('area', { type: 'geojson', data: circlePolygon(state.center.lon, state.center.lat, state.radius) });
    map.addLayer({ id: 'area-fill', type: 'fill', source: 'area', paint: { 'fill-color': '#f2a93b', 'fill-opacity': 0.12 } });
    map.addLayer({ id: 'area-line', type: 'line', source: 'area', paint: { 'line-color': '#f2a93b', 'line-width': 2 } });
    centerMarker = new maplibregl.Marker({ color: '#3b8ff2', draggable: true }).setLngLat([state.center.lon, state.center.lat]).addTo(map);
    centerMarker.on('dragend', () => {
      const p = centerMarker.getLngLat();
      state.center = { lon: p.lng, lat: p.lat };
      updateMapOverlay();
    });
    viewMarker = new maplibregl.Marker({ color: '#f2a93b', draggable: true }).setLngLat([state.viewpoint.lon, state.viewpoint.lat]).addTo(map);
    viewMarker.on('dragend', () => {
      const p = viewMarker.getLngLat();
      state.viewpoint = { lon: p.lng, lat: p.lat };
      updateViewpointMarker();
      invalidateCapture();
    });
    map.on('click', (e: any) => {
      if (e.originalEvent.shiftKey) {
        state.viewpoint = { lon: e.lngLat.lng, lat: e.lngLat.lat };
        updateViewpointMarker();
        invalidateCapture();
      } else {
        state.center = { lon: e.lngLat.lng, lat: e.lngLat.lat };
      }
      updateMapOverlay();
    });
    updateMapOverlay();
  });
  map.on('error', (e: any) => log(`Map: ${e?.error?.message ?? 'error'}`));
}

// ---------------------------------------------------------------------------
// Generate

async function generateFromMap(): Promise<void> {
  if (state.busy) return;
  state.busy = true;
  const btn = $<HTMLButtonElement>('generateBtn');
  btn.disabled = true;
  const prog = $('genProgress');
  prog.style.width = '5%';
  let ticker: ReturnType<typeof setInterval> | null = null;
  try {
    const { lon, lat } = state.center;
    const radius = state.radius;
    const frame = new LocalFrame(lon, lat);
    // Keep the viewpoint inside the area.
    const [ve, vn] = frame.toLocal(state.viewpoint.lon, state.viewpoint.lat);
    if (Math.hypot(ve, vn) > radius) {
      state.viewpoint = { lon: lon + 0.0005, lat: lat + 0.0003 };
      updateMapOverlay();
    }
    log(`Generating ${(radius / 1000).toFixed(1)} km around ${lat.toFixed(5)}, ${lon.toFixed(5)}`);
    const tStart = performance.now();
    ticker = setInterval(() => (statusEl.textContent = `Fetching … ${Math.round((performance.now() - tStart) / 1000)} s (Overpass answers dense cities in 10–60 s)`), 1000);
    const [area, terrain, horizon] = await Promise.all([
      fetchArea(lon, lat, radius, log).finally(() => (prog.style.width = '40%')),
      fetchTerrain(lon, lat, radius, frame, log).finally(() => (prog.style.width = '60%')),
      state.wantHorizon ? fetchTerrain(lon, lat, HORIZON_RADIUS, frame, log, { targetMetres: 110, label: 'Horizon' }).finally(() => (prog.style.width = '75%')) : Promise.resolve(null),
    ]);
    const sources = [...area.sources, terrain ? terrain.source.credit : 'Elevation: none (flat ground)', GEOCODER_CREDIT, 'Basemap display: OpenFreeMap / OpenMapTiles (not used for geometry)'];
    if (horizon && terrain && horizon.source !== terrain.source) sources.push(horizon.source.credit);
    if (ticker) clearInterval(ticker);
    ticker = null;
    applyMeshes(area, terrain?.grid ?? null, horizon?.grid ?? null, frame, radius, sources, false);
    prog.style.width = '100%';
    showTab('view3d');
  } catch (err) {
    log(`Generate failed: ${(err as Error).message}`);
    prog.style.width = '0';
  } finally {
    if (ticker) clearInterval(ticker);
    state.busy = false;
    btn.disabled = false;
    setTimeout(() => (prog.style.width = '0'), 1500);
  }
}

function loadDemo(): void {
  const d = demoScene();
  state.center = { ...DEMO_CENTER };
  state.radius = DEMO_RADIUS;
  ($('radius') as HTMLInputElement).value = String(DEMO_RADIUS);
  $('radiusVal').textContent = `${(DEMO_RADIUS / 1000).toFixed(1)} km`;
  const [vlon, vlat] = d.frame.toLonLat(DEMO_VIEWPOINT[0], DEMO_VIEWPOINT[1]);
  state.viewpoint = { lon: vlon, lat: vlat };
  log('Sample city: synthetic data, no network needed');
  applyMeshes(d.area, d.terrain, d.horizon, d.frame, DEMO_RADIUS, d.area.sources, true);
  updateMapOverlay();
}

// ---------------------------------------------------------------------------
// Render 360°

async function render360(sizeOverride?: number): Promise<void> {
  if (state.busy || !state.meshes || !state.frame) return;
  state.busy = true;
  const btn = $<HTMLButtonElement>('renderBtn');
  btn.disabled = true;
  const prog = $('renderProgress');
  prog.style.width = '10%';
  try {
    const width = sizeOverride ?? parseInt(($('size') as HTMLSelectElement).value, 10);
    const ss = parseInt(($('ss') as HTMLSelectElement).value, 10);
    const clamp = ($('clamp') as HTMLInputElement).checked ? 65504 : 0;
    const v = viewpointLocal();
    const eye: [number, number, number] = [v.x, v.groundY + state.eyeHeight, v.z];
    const t0 = performance.now();
    log(`Rendering ${width}×${width / 2} from eye ${eye.map((n) => n.toFixed(1)).join(', ')} heading ${state.heading}°`);
    await new Promise((r) => requestAnimationFrame(r));
    invalidateCapture();
    const capture = captureEquirect(scene, { width, eye, headingDeg: state.heading, supersample: ss, clamp });
    prog.style.width = '60%';
    await new Promise((r) => setTimeout(r, 0));
    const preview = buildPreview(capture, 2048);
    state.capture = capture;
    state.preview = preview;
    state.meta = {
      app: APP,
      version: VERSION,
      renderedAt: new Date().toISOString(),
      latitude: state.viewpoint.lat,
      longitude: state.viewpoint.lon,
      eyeHeight: state.eyeHeight,
      heading: state.heading,
      sunAzimuth: state.sky.sunAzimuth,
      sunElevation: state.sky.sunElevation,
      turbidity: state.sky.turbidity,
      sunIntensity: state.sky.sunIntensity,
      radius: state.radius,
      width,
      height: width / 2,
      dataSources: state.sources,
      demo: state.demo,
    };
    showPreview();
    const secs = ((performance.now() - t0) / 1000).toFixed(1);
    $('renderInfo').textContent = `${width}×${width / 2} rendered in ${secs} s · peak radiance ${capture.stats.max.toExponential(2)}`;
    log(`Render done in ${secs} s, peak ${capture.stats.max.toExponential(2)}`);
    for (const id of ['dlHdr', 'dlExrHalf', 'dlExrFloat', 'dlPng', 'dlJson']) ($(id) as HTMLButtonElement).disabled = false;
    prog.style.width = '100%';
    showTab('pano');
  } catch (err) {
    log(`Render failed: ${(err as Error).message}`);
    console.error(err);
  } finally {
    state.busy = false;
    btn.disabled = false;
    setTimeout(() => (prog.style.width = '0'), 1500);
  }
}

let panoExposure = 1;
function showPreview(): void {
  if (!state.preview) return;
  $('panoEmpty').hidden = true;
  drawPreview($<HTMLCanvasElement>('panoFlat'), state.preview, panoExposure);
  if (!sphere) sphere = new SphereViewer(sphereCanvas);
  sphere.setImage(state.preview);
  sphere.setExposure(panoExposure);
  $('panoStats').textContent = `${state.meta?.width}×${state.meta?.height} · sun az ${state.sky.sunAzimuth}° el ${state.sky.sunElevation}°`;
}

// ---------------------------------------------------------------------------
// Export

function baseName(): string {
  const m = state.meta;
  if (!m) return 'skyline360';
  const place = state.demo ? 'sample-city' : `${m.latitude.toFixed(4)}_${m.longitude.toFixed(4)}`;
  return `skyline360_${place}_${m.width}_sun${Math.round(m.sunElevation)}`;
}

function withProgress<T>(label: string, fn: (p: (f: number, l: string) => void) => T): T {
  const prog = $('renderProgress');
  statusEl.textContent = label;
  const result = fn((f, l) => {
    prog.style.width = `${Math.round(f * 100)}%`;
    statusEl.textContent = `${l} ${Math.round(f * 100)}%`;
  });
  setTimeout(() => (prog.style.width = '0'), 1000);
  return result;
}

function exportHdr(): void {
  if (!state.capture || !state.meta) return;
  const parts = withProgress('Encoding .hdr', (p) => encodeCaptureHdr(state.capture!, state.meta!, p));
  download(parts as BlobPart[], `${baseName()}.hdr`, 'image/vnd.radiance');
  log(`Saved ${baseName()}.hdr (${(parts.reduce((s, x) => s + x.length, 0) / 1e6).toFixed(1)} MB)`);
}

function exportExr(type: 'half' | 'float'): void {
  if (!state.capture || !state.meta) return;
  const parts = withProgress(`Encoding .exr ${type}`, (p) => encodeCaptureExr(state.capture!, state.meta!, type, p));
  download(parts as BlobPart[], `${baseName()}_${type}.exr`, 'image/x-exr');
  log(`Saved ${baseName()}_${type}.exr (${(parts.reduce((s, x) => s + x.length, 0) / 1e6).toFixed(1)} MB)`);
}

function exportPng(): void {
  const canvas = $<HTMLCanvasElement>('panoFlat');
  canvas.toBlob((blob) => {
    if (blob) download([blob], `${baseName()}_preview.png`, 'image/png');
  }, 'image/png');
}

async function exportGlbFile(): Promise<void> {
  if (!state.meshes) return;
  statusEl.textContent = 'Exporting .glb …';
  scene.viewpointMarker.visible = false;
  try {
    const buf = await exportGlb(scene.meshGroup);
    download([buf], `${baseName()}_model.glb`, 'model/gltf-binary');
    log(`Saved model .glb (${(buf.byteLength / 1e6).toFixed(1)} MB)`);
  } finally {
    scene.viewpointMarker.visible = true;
  }
}

function exportSidecar(): void {
  if (!state.meta) return;
  const json = JSON.stringify({ ...state.meta, center: state.center, style: state.styleKey, viewpoint: state.viewpoint }, null, 2);
  download([json], `${baseName()}.skyline360.json`, 'application/json');
  download([sourcesText(state.meta)], `${baseName()}_sources.txt`, 'text/plain');
}

// ---------------------------------------------------------------------------
// UI wiring

type TabId = 'map' | 'view3d' | 'pano';
function showTab(id: TabId): void {
  for (const b of document.querySelectorAll<HTMLButtonElement>('.tabs button')) b.setAttribute('aria-selected', String(b.dataset.tab === id));
  $('mapView').hidden = id !== 'map';
  $('view3dView').hidden = id !== 'view3d';
  $('panoView').hidden = id !== 'pano';
  if (id === 'view3d') scene.resize();
  if (id === 'pano') sphere?.resize();
  if (id === 'map') map?.resize();
}

function bindRange(id: string, valId: string, format: (v: number) => string, onChange: (v: number) => void): void {
  const input = $<HTMLInputElement>(id);
  const show = (): void => {
    $(valId).textContent = format(parseFloat(input.value));
  };
  show();
  input.addEventListener('input', () => {
    show();
    onChange(parseFloat(input.value));
  });
}

function applySky(): void {
  scene.setSky(state.sky);
  invalidateCapture();
}

function setupUI(): void {
  for (const b of document.querySelectorAll<HTMLButtonElement>('.tabs button')) b.addEventListener('click', () => showTab(b.dataset.tab as TabId));

  bindRange('radius', 'radiusVal', (v) => `${(v / 1000).toFixed(1)} km`, (v) => {
    state.radius = v;
    updateMapOverlay();
  });
  bindRange('eye', 'eyeVal', (v) => `${v.toFixed(1)} m`, (v) => {
    state.eyeHeight = v;
    updateViewpointMarker();
    invalidateCapture();
  });
  bindRange('heading', 'headingVal', (v) => `${v}°`, (v) => {
    state.heading = v;
    invalidateCapture();
  });
  bindRange('az', 'azVal', (v) => `${v}°`, (v) => {
    state.sky.sunAzimuth = v;
    applySky();
  });
  bindRange('el', 'elVal', (v) => `${v}°`, (v) => {
    state.sky.sunElevation = v;
    applySky();
  });
  bindRange('turb', 'turbVal', (v) => v.toFixed(1), (v) => {
    state.sky.turbidity = v;
    applySky();
  });
  bindRange('sun', 'sunVal', (v) => `${fmt(Math.round(10 ** v))}×`, (v) => {
    state.sky.sunIntensity = Math.round(10 ** v);
    applySky();
  });
  bindRange('exposure', 'expVal', (v) => (2 ** v).toFixed(2), (v) => scene.setExposure(2 ** v));
  scene.setExposure(2 ** parseFloat(($('exposure') as HTMLInputElement).value));
  bindRange('panoExposure', 'panoExpVal', (v) => (2 ** v).toFixed(2), (v) => {
    panoExposure = 2 ** v;
    if (state.preview) drawPreview($<HTMLCanvasElement>('panoFlat'), state.preview, panoExposure);
    sphere?.setExposure(panoExposure);
  });

  const presets: Record<string, Partial<SkyParams>> = {
    noon: { sunAzimuth: 180, sunElevation: 60, turbidity: 2.2 },
    golden: { sunAzimuth: 262, sunElevation: 7, turbidity: 3.5 },
    hazy: { sunAzimuth: 150, sunElevation: 40, turbidity: 7 },
  };
  for (const b of document.querySelectorAll<HTMLButtonElement>('[data-preset]')) {
    b.addEventListener('click', () => {
      Object.assign(state.sky, presets[b.dataset.preset!]);
      ($('az') as HTMLInputElement).value = String(state.sky.sunAzimuth);
      ($('el') as HTMLInputElement).value = String(state.sky.sunElevation);
      ($('turb') as HTMLInputElement).value = String(state.sky.turbidity);
      $('azVal').textContent = `${state.sky.sunAzimuth}°`;
      $('elVal').textContent = `${state.sky.sunElevation}°`;
      $('turbVal').textContent = state.sky.turbidity.toFixed(1);
      applySky();
    });
  }

  const rebuild = (): void => {
    if (state.area && state.frame) applyMeshes(state.area, state.terrain, state.horizon, state.frame, state.radius, state.sources, state.demo);
  };
  $<HTMLInputElement>('horizonToggle').addEventListener('change', (e) => {
    state.wantHorizon = (e.target as HTMLInputElement).checked;
    if (state.demo || state.horizon) rebuild();
    else log('Horizon terrain will be fetched with the next Generate');
  });
  $<HTMLInputElement>('treesToggle').addEventListener('change', (e) => {
    state.wantTrees = (e.target as HTMLInputElement).checked;
    rebuild();
  });
  $('style').addEventListener('change', (e) => {
    state.styleKey = (e.target as HTMLSelectElement).value;
    scene.applyStyle(STYLES[state.styleKey]);
    invalidateCapture();
  });

  $('generateBtn').addEventListener('click', () => void generateFromMap());
  $('demoBtn').addEventListener('click', () => loadDemo());
  $('renderBtn').addEventListener('click', () => void render360());
  $('lookBtn').addEventListener('click', () => {
    const v = viewpointLocal();
    scene.lookFromViewpoint(v.x, v.groundY + state.eyeHeight, v.z, state.heading);
    showTab('view3d');
  });
  $('frameBtn').addEventListener('click', () => {
    scene.frameArea();
    showTab('view3d');
  });
  $('dlHdr').addEventListener('click', exportHdr);
  $('dlExrHalf').addEventListener('click', () => exportExr('half'));
  $('dlExrFloat').addEventListener('click', () => exportExr('float'));
  $('dlPng').addEventListener('click', exportPng);
  $('dlGlb').addEventListener('click', () => void exportGlbFile());
  $('dlJson').addEventListener('click', exportSidecar);

  const lookAround = $<HTMLInputElement>('lookAround');
  lookAround.addEventListener('change', () => {
    const flat = $<HTMLCanvasElement>('panoFlat');
    flat.hidden = lookAround.checked;
    sphereCanvas.hidden = !lookAround.checked;
    if (lookAround.checked) sphere?.resize();
  });

  // Search.
  const searchInput = $<HTMLInputElement>('searchInput');
  const results = $('searchResults');
  const runSearch = async (): Promise<void> => {
    const q = searchInput.value.trim();
    if (!q) return;
    try {
      const hits = await geocode(q);
      results.innerHTML = '';
      results.hidden = hits.length === 0;
      for (const h of hits) {
        const b = document.createElement('button');
        b.textContent = h.name;
        b.addEventListener('click', () => {
          results.hidden = true;
          state.center = { lon: h.lon, lat: h.lat };
          state.viewpoint = { lon: h.lon + 0.0005, lat: h.lat + 0.0003 };
          map?.flyTo({ center: [h.lon, h.lat], zoom: 13 });
          updateMapOverlay();
          showTab('map');
        });
        results.appendChild(b);
      }
      if (hits.length === 0) log('Search: no results');
    } catch (err) {
      log(`Search failed: ${(err as Error).message}`);
    }
  };
  $('searchBtn').addEventListener('click', () => void runSearch());
  searchInput.addEventListener('keydown', (e) => {
    if (e.key === 'Enter') void runSearch();
    if (e.key === 'Escape') results.hidden = true;
  });

  window.addEventListener('resize', () => {
    scene.resize();
    sphere?.resize();
  });
}

// ---------------------------------------------------------------------------
// Render loop

let frameCounter = 0;
function loop(): void {
  if (!$('view3dView').hidden && scene.render() && ++frameCounter % 10 === 0) {
    $('frameInfo').textContent = `${scene.lastFrameMs.toFixed(1)} ms/frame`;
  }
  if (!$('panoView').hidden && sphere && !sphereCanvas.hidden) sphere.render();
  requestAnimationFrame(loop);
}

// ---------------------------------------------------------------------------
// Boot

setupUI();
scene.resize();
initMap();
if (params.get('nodemo') === null) loadDemo();
if (params.get('demo') !== null) showTab('view3d');
loop();

/** Hooks for the headless render test. */
(window as unknown as { __skyline: unknown }).__skyline = {
  state,
  scene,
  THREE,
  loadDemo,
  render: async (size: number): Promise<{ max: number; width: number; height: number }> => {
    await render360(size);
    if (!state.capture) throw new Error('no capture');
    return { max: state.capture.stats.max, width: state.capture.width, height: state.capture.height };
  },
  hdrBytes: (): Uint8Array => {
    if (!state.capture || !state.meta) throw new Error('no capture');
    const parts = encodeCaptureHdr(state.capture, state.meta);
    const out = new Uint8Array(parts.reduce((s, p) => s + p.length, 0));
    let o = 0;
    for (const p of parts) {
      out.set(p, o);
      o += p.length;
    }
    return out;
  },
  setSky: (p: Partial<SkyParams>): void => {
    Object.assign(state.sky, p);
    applySky();
  },
  setViewpoint: (eyeHeight: number, heading: number): void => {
    state.eyeHeight = eyeHeight;
    state.heading = heading;
    updateViewpointMarker();
  },
  showTab,
  samplePixel: (u: number, v: number): number[] => {
    if (!state.capture) throw new Error('no capture');
    const x = Math.min(state.capture.width - 1, Math.floor(u * state.capture.width));
    const y = Math.min(state.capture.height - 1, Math.floor(v * state.capture.height));
    const row = state.capture.readStrip(y, 1);
    return Array.from(row.subarray(x * 4, x * 4 + 4));
  },
};
