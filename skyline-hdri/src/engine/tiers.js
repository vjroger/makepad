// Height-model tiers: square grids in the azimuthal-equidistant frame around the eye.
//
// Tier texel (i, j) is centred at east = (i - size/2 + 0.5) * texel, north = (j - size/2 + 0.5) * texel.
// Row j grows northward. Each tier has a height array (metres) and a class array (0 terrain, 1 building).
// The sampling functions here are the CPU twins of the GLSL ones in src/gpu/shaders.js.

import { EARTH_RADIUS, DEG, flatOffset, destination, mercatorPixel } from './geo.js';
import { tileRange, loadMosaic } from './tiles.js';

/**
 * Default tier ladder. radius = size / 2 * texel.
 * `zoomHigh` is used when quality === 'high' (4x the tiles for the two far tiers).
 */
export const DEFAULT_TIERS = [
  { size: 1024, texel: 0.5,  zoom: 15, zoomHigh: 15 },   // r = 256 m   (near buildings)
  { size: 2048, texel: 2,    zoom: 15, zoomHigh: 15 },   // r = 2.0 km
  { size: 2048, texel: 10,   zoom: 13, zoomHigh: 14 },   // r = 10.2 km
  { size: 2048, texel: 50,   zoom: 10, zoomHigh: 11 },   // r = 51 km
  { size: 2048, texel: 250,  zoom: 8,  zoomHigh: 9 },    // r = 256 km
  { size: 2048, texel: 1000, zoom: 6,  zoomHigh: 7 },    // r = 1024 km
];

export const FLAT_LIMIT = 3000; // m: tiers whose radius is below this use the separable flat frame (error < 1 m)

export function tierRadius(t) { return t.size / 2 * t.texel; }

/** Pick the tiers needed to reach maxDistance (the first tier whose radius covers it is the last). */
export function planTiers(maxDistance, quality = 'normal', ladder = DEFAULT_TIERS) {
  const out = [];
  for (const t of ladder) {
    out.push({ size: t.size, texel: t.texel, zoom: quality === 'high' ? t.zoomHigh : t.zoom, radius: tierRadius(t) });
    if (tierRadius(t) >= maxDistance) break;
  }
  return out;
}

/** Lat/lon bounding box of a square of half-size r metres around (lat0, lon0). */
export function bboxAround(lat0, lon0, r) {
  const dLat = (r / EARTH_RADIUS) / DEG;
  const dLon = (r / (EARTH_RADIUS * Math.cos(lat0 * DEG))) / DEG;
  return { latMin: lat0 - dLat, latMax: lat0 + dLat, lonMin: lon0 - dLon, lonMax: lon0 + dLon };
}

/** Load one Mercator mosaic per distinct zoom, sized for the largest tier that uses it. */
export async function loadMosaicsForTiers(tiers, lat0, lon0, opts = {}) {
  const byZoom = new Map();
  for (const t of tiers) {
    const r = t.radius * 1.02;
    const prev = byZoom.get(t.zoom);
    if (!prev || prev < r) byZoom.set(t.zoom, r);
  }
  const mosaics = new Map();
  let tileTotal = 0;
  const ranges = [];
  for (const [z, r] of byZoom) {
    const b = bboxAround(lat0, lon0, r);
    const range = tileRange(z, b.latMin, b.lonMin, b.latMax, b.lonMax, 1);
    ranges.push(range);
    tileTotal += range.nx * range.ny;
  }
  let tileDone = 0;
  for (const range of ranges) {
    const mosaic = await loadMosaic(range, {
      ...opts,
      onProgress: (p) => {
        opts.onProgress?.({ stage: 'tiles', done: tileDone + p.done, total: tileTotal, zoom: range.z });
      },
    });
    tileDone += range.nx * range.ny;
    mosaics.set(range.z, mosaic);
  }
  return { mosaics, tileTotal };
}

/**
 * Resample a mosaic into a tier. Near tiers use the separable flat frame (two 1-D tables of
 * Mercator coordinates), far tiers evaluate the spherical direct formula per texel.
 */
export function buildTier(spec, mosaic, lat0, lon0, opts = {}) {
  const { size, texel, zoom } = spec;
  const clampSea = opts.clampSea !== false;
  const height = new Float32Array(size * size);
  const cls = new Uint8Array(size * size);
  const half = size / 2;
  const radius = tierRadius(spec);

  if (radius < FLAT_LIMIT) {
    const px = new Float64Array(size), py = new Float64Array(size);
    for (let i = 0; i < size; i++) {
      const east = (i - half + 0.5) * texel;
      const [lat, lon] = flatOffset(lat0, lon0, east, 0);
      px[i] = mercatorPixel(lat, lon, zoom)[0];
    }
    for (let j = 0; j < size; j++) {
      const north = (j - half + 0.5) * texel;
      const [lat, lon] = flatOffset(lat0, lon0, 0, north);
      py[j] = mercatorPixel(lat, lon, zoom)[1];
    }
    for (let j = 0; j < size; j++) {
      const row = j * size;
      const yy = py[j];
      for (let i = 0; i < size; i++) {
        let h = mosaic.sample(px[i], yy);
        if (clampSea && h < 0) h = 0;
        height[row + i] = h;
      }
    }
  } else {
    for (let j = 0; j < size; j++) {
      const north = (j - half + 0.5) * texel;
      const row = j * size;
      for (let i = 0; i < size; i++) {
        const east = (i - half + 0.5) * texel;
        const d = Math.hypot(east, north);
        const [lat, lon] = d < 1 ? [lat0, lon0] : destination(lat0, lon0, Math.atan2(east, north), d);
        const [mx, my] = mercatorPixel(lat, lon, zoom);
        let h = mosaic.sample(mx, my);
        if (clampSea && h < 0) h = 0;
        height[row + i] = h;
      }
      if (opts.onProgress && (j & 255) === 0) opts.onProgress({ stage: 'tier', texel, fraction: j / size });
    }
  }
  return { size, texel, radius, zoom, height, cls };
}

/** Bilinear height at (east, north) metres. Mirrors sampleTier() in the shader. */
export function sampleTier(tier, east, north) {
  const { size, texel, height } = tier;
  const fi = east / texel + size / 2 - 0.5;
  const fj = north / texel + size / 2 - 0.5;
  let i0 = Math.floor(fi), j0 = Math.floor(fj);
  const tx = fi - i0, ty = fj - j0;
  let i1 = i0 + 1, j1 = j0 + 1;
  const m = size - 1;
  if (i0 < 0) i0 = 0; if (i0 > m) i0 = m; if (i1 < 0) i1 = 0; if (i1 > m) i1 = m;
  if (j0 < 0) j0 = 0; if (j0 > m) j0 = m; if (j1 < 0) j1 = 0; if (j1 > m) j1 = m;
  const a = height[j0 * size + i0], b = height[j0 * size + i1];
  const c = height[j1 * size + i0], d = height[j1 * size + i1];
  return (a + (b - a) * tx) + ((c + (d - c) * tx) - (a + (b - a) * tx)) * ty;
}

/** Nearest-texel class at (east, north). */
export function sampleTierClass(tier, east, north) {
  const { size, texel, cls } = tier;
  const i = Math.max(0, Math.min(size - 1, Math.round(east / texel + size / 2 - 0.5)));
  const j = Math.max(0, Math.min(size - 1, Math.round(north / texel + size / 2 - 0.5)));
  return cls[j * size + i];
}

/** Unit normal (east, north, up) from central differences of the bilinear height. */
export function tierNormal(tier, east, north, out = [0, 0, 1]) {
  const t = tier.texel;
  const dx = (sampleTier(tier, east + t, north) - sampleTier(tier, east - t, north)) / (2 * t);
  const dy = (sampleTier(tier, east, north + t) - sampleTier(tier, east, north - t)) / (2 * t);
  const len = Math.hypot(dx, dy, 1);
  out[0] = -dx / len; out[1] = -dy / len; out[2] = 1 / len;
  return out;
}

/** Index of the finest tier that contains (east, north) with a safety margin, or -1 when outside all. */
export function tierIndexFor(tiers, east, north) {
  const m = Math.max(Math.abs(east), Math.abs(north));
  for (let k = 0; k < tiers.length; k++) {
    if (m < tiers[k].radius - 2 * tiers[k].texel) return k;
  }
  return -1;
}

/** Highest value in all tiers, used as the sky early-out bound. */
export function maxHeight(tiers) {
  let m = -Infinity;
  for (const t of tiers) for (let i = 0; i < t.height.length; i++) if (t.height[i] > m) m = t.height[i];
  return m;
}
