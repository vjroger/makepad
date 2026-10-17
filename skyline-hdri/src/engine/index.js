// Engine entry points: build the scene (tiers + buildings) and render it on the CPU.
// The GPU path (src/gpu/renderer.js) consumes the same scene object.

import { planTiers, loadMosaicsForTiers, buildTier, sampleTier, maxHeight } from './tiers.js';
import { fetchBuildings, parseBuildings, rasterizeBuildings, pointInPolygon } from './buildings.js';
import { allocatePlate, sweepColumns } from './sweep.js';
import { DEG } from './geo.js';

export const DEFAULT_PARAMS = {
  lat: 52.0907, lon: 5.1214,          // Utrecht, Dom tower area
  eyeHeight: 1.7,                     // m above ground
  eyeAltitude: null,                  // absolute m, overrides eyeHeight when set
  eyeOnRoof: false,                   // if the eye is inside a footprint, stand on its roof
  width: 4096,                        // plate width; height = width / 2
  maxDistance: 250000,                // m
  quality: 'normal',                  // 'normal' | 'high' (finer far tiers, 4x tiles)
  headingOffset: 0,                   // degrees; rotates the plate (positive turns the view clockwise)
  verticalExaggeration: 1,
  bands: [2000, 8000, 30000, 120000], // m; SVG layers
  buildings: {
    enabled: true,
    allRadius: 2500,                  // m: every building
    tallRadius: 30000,                // m: only tall buildings
    tallThreshold: 35,                // m
    heightScale: 1,
  },
  sun: { azimuth: 225, elevation: 35 },  // preview shading only
  hazeDistance: 60000,                   // m, preview shading only
};

export function mergeParams(p = {}) {
  return { ...DEFAULT_PARAMS, ...p, buildings: { ...DEFAULT_PARAMS.buildings, ...(p.buildings || {}) }, sun: { ...DEFAULT_PARAMS.sun, ...(p.sun || {}) } };
}

/**
 * Fetch terrain and buildings and build the tier stack around the eye.
 * hooks: { onProgress, tileUrl, overpassUrls, useCache, decode, onError }
 */
export async function prepareScene(rawParams, hooks = {}) {
  const params = mergeParams(rawParams);
  const t0 = Date.now();
  const onProgress = hooks.onProgress || (() => {});
  const specs = planTiers(params.maxDistance, params.quality);
  const { mosaics, tileTotal } = await loadMosaicsForTiers(specs, params.lat, params.lon, {
    onProgress, tileUrl: hooks.tileUrl, useCache: hooks.useCache, decode: hooks.decode, onError: hooks.onError, concurrency: hooks.concurrency,
  });
  let missing = 0;
  for (const m of mosaics.values()) missing += m.missing;

  const tiers = [];
  for (let k = 0; k < specs.length; k++) {
    onProgress({ stage: 'tiers', done: k, total: specs.length });
    tiers.push(buildTier(specs[k], mosaics.get(specs[k].zoom), params.lat, params.lon, { onProgress }));
  }
  onProgress({ stage: 'tiers', done: specs.length, total: specs.length });

  const zGround = sampleTier(tiers[0], 0, 0);
  let eyeZ = params.eyeAltitude != null ? params.eyeAltitude : zGround + params.eyeHeight;

  let buildingStats = { count: 0, skipped: 0, fetched: 0, maxRoof: 0 };
  if (params.buildings.enabled) {
    let elements = [];
    try {
      elements = await fetchBuildings(params.lat, params.lon, {
        allRadius: params.buildings.allRadius, tallRadius: params.buildings.tallRadius,
        overpassUrls: hooks.overpassUrls, onProgress, onError: hooks.onError,
      });
    } catch (err) {
      hooks.onError?.('overpass', err);
      onProgress({ stage: 'buildings', message: 'no building data (' + err.message + ')' });
    }
    const parsed = parseBuildings(elements, params.lat, params.lon);
    buildingStats.fetched = parsed.length;
    if (params.eyeOnRoof && params.eyeAltitude == null) {
      const under = parsed.find((b) => b.polys.some((p) => pointInPolygon(p, 0, 0)));
      if (under) eyeZ = zGround + under.height * params.buildings.heightScale + params.eyeHeight;
    }
    onProgress({ stage: 'buildings', message: `rasterising ${parsed.length} footprints` });
    const r = rasterizeBuildings(parsed, tiers, {
      tallThreshold: params.buildings.tallThreshold, heightScale: params.buildings.heightScale,
      excludeContaining: [0, 0],
    });
    buildingStats = { ...buildingStats, ...r };
  }

  const hMax = maxHeight(tiers);
  return {
    params, tiers, eyeZ, zGround, hMax,
    stats: { tiles: tileTotal, tilesMissing: missing, buildings: buildingStats, prepareMs: Date.now() - t0 },
  };
}

/** Render the whole plate with the CPU sweep. */
export function renderCPU(scene, onProgress) {
  const p = scene.params;
  const plate = allocatePlate(p.width, p.bands.length);
  const t0 = Date.now();
  sweepColumns(scene.tiers, {
    eyeZ: scene.eyeZ, zGround: scene.zGround, verticalExaggeration: p.verticalExaggeration,
    headingOffset: p.headingOffset * DEG, maxDistance: p.maxDistance, bands: p.bands,
  }, plate, 0, p.width, onProgress);
  plate.renderMs = Date.now() - t0;
  return plate;
}

/** Convenience: prepare + CPU render in one call (no WebGL needed; works in Node and workers). */
export async function renderSkyline(params, hooks = {}) {
  const scene = await prepareScene(params, hooks);
  const plate = renderCPU(scene, hooks.onProgress);
  plate.scene = scene;
  return plate;
}
