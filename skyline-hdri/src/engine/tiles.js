// Terrain tile loading: AWS Terrain Tiles (Mapzen/Tilezen "terrarium" PNG encoding).
//
//   elevation (m) = R * 256 + G + B / 256 - 32768
//
// Keyless, CORS-open, 256 px tiles, zoom 0..15. Attribution: https://github.com/tilezen/joerd/blob/master/docs/attribution.md

import { mercatorPixel } from './geo.js';

export const DEFAULT_TILE_URL = 'https://s3.amazonaws.com/elevation-tiles-prod/terrarium/{z}/{x}/{y}.png';
export const TILE_SIZE = 256;

const CACHE_NAME = 'skyline-terrain-tiles-v1';

/** Decode a terrarium RGBA buffer (256*256*4) into metres. */
export function decodeTerrarium(rgba, out = new Float32Array(TILE_SIZE * TILE_SIZE)) {
  const n = TILE_SIZE * TILE_SIZE;
  for (let i = 0, j = 0; i < n; i++, j += 4) {
    out[i] = rgba[j] * 256 + rgba[j + 1] + rgba[j + 2] / 256 - 32768;
  }
  return out;
}

function tileUrl(template, z, x, y) {
  return template.replace('{z}', z).replace('{x}', x).replace('{y}', y);
}

async function fetchWithCache(url, useCache) {
  let cache = null;
  if (useCache && typeof caches !== 'undefined') {
    try { cache = await caches.open(CACHE_NAME); } catch { cache = null; }
  }
  if (cache) {
    try {
      const hit = await cache.match(url);
      if (hit) return hit;
    } catch { /* ignore */ }
  }
  const resp = await fetch(url, { mode: 'cors' });
  if (!resp.ok) throw new Error(`HTTP ${resp.status} for ${url}`);
  if (cache) {
    try { await cache.put(url, resp.clone()); } catch { /* quota or opaque; ignore */ }
  }
  return resp;
}

/** Decode a PNG response to RGBA bytes using the browser's decoder (works in workers via OffscreenCanvas). */
async function responseToRgba(resp) {
  const blob = await resp.blob();
  const bmp = await createImageBitmap(blob);
  const canvas = new OffscreenCanvas(TILE_SIZE, TILE_SIZE);
  const ctx = canvas.getContext('2d', { willReadFrequently: true });
  ctx.drawImage(bmp, 0, 0);
  bmp.close?.();
  return ctx.getImageData(0, 0, TILE_SIZE, TILE_SIZE).data;
}

/**
 * Fetch and decode one tile. Returns Float32Array(65536) of metres, or null if the tile is
 * unavailable (ocean tiles at high zoom are present, so null usually means a network problem).
 */
export async function fetchTile(z, x, y, opts = {}) {
  const template = opts.tileUrl || DEFAULT_TILE_URL;
  const url = tileUrl(template, z, x, y);
  try {
    const resp = await fetchWithCache(url, opts.useCache !== false);
    const rgba = opts.decode ? await opts.decode(resp) : await responseToRgba(resp);
    return decodeTerrarium(rgba);
  } catch (err) {
    opts.onError?.(url, err);
    return null;
  }
}

/** A rectangular block of tiles at one zoom, assembled into one Float32Array. */
export class Mosaic {
  constructor(z, x0, y0, nx, ny) {
    this.z = z; this.x0 = x0; this.y0 = y0; this.nx = nx; this.ny = ny;
    this.width = nx * TILE_SIZE; this.height = ny * TILE_SIZE;
    this.data = new Float32Array(this.width * this.height);
    this.missing = 0;
  }
  /** Mercator pixel (px, py) at this zoom -> mosaic texel coordinates. */
  toLocal(px, py) {
    return [px - this.x0 * TILE_SIZE, py - this.y0 * TILE_SIZE];
  }
  /** Bilinear sample at Mercator pixel coordinates; clamps at the edges. */
  sample(px, py) {
    const fx = px - this.x0 * TILE_SIZE - 0.5;
    const fy = py - this.y0 * TILE_SIZE - 0.5;
    const w = this.width, h = this.height, d = this.data;
    let x0 = Math.floor(fx), y0 = Math.floor(fy);
    const tx = fx - x0, ty = fy - y0;
    let x1 = x0 + 1, y1 = y0 + 1;
    if (x0 < 0) x0 = 0; if (x1 < 0) x1 = 0; if (x0 > w - 1) x0 = w - 1; if (x1 > w - 1) x1 = w - 1;
    if (y0 < 0) y0 = 0; if (y1 < 0) y1 = 0; if (y0 > h - 1) y0 = h - 1; if (y1 > h - 1) y1 = h - 1;
    const a = d[y0 * w + x0], b = d[y0 * w + x1], c = d[y1 * w + x0], e = d[y1 * w + x1];
    return (a * (1 - tx) + b * tx) * (1 - ty) + (c * (1 - tx) + e * tx) * ty;
  }
  put(tx, ty, tile) {
    const ox = (tx - this.x0) * TILE_SIZE, oy = (ty - this.y0) * TILE_SIZE;
    for (let r = 0; r < TILE_SIZE; r++) {
      this.data.set(tile.subarray(r * TILE_SIZE, (r + 1) * TILE_SIZE), (oy + r) * this.width + ox);
    }
  }
}

/** Tile index range covering a lat/lon bounding box at zoom z, with one tile of margin. */
export function tileRange(z, latMin, lonMin, latMax, lonMax, margin = 1) {
  const [pxa, pya] = mercatorPixel(latMax, lonMin, z);
  const [pxb, pyb] = mercatorPixel(latMin, lonMax, z);
  const n = Math.pow(2, z);
  const clampT = (v) => Math.max(0, Math.min(n - 1, v));
  const x0 = clampT(Math.floor(pxa / TILE_SIZE) - margin);
  const y0 = clampT(Math.floor(pya / TILE_SIZE) - margin);
  const x1 = clampT(Math.floor(pxb / TILE_SIZE) + margin);
  const y1 = clampT(Math.floor(pyb / TILE_SIZE) + margin);
  return { z, x0, y0, nx: x1 - x0 + 1, ny: y1 - y0 + 1 };
}

/** Load every tile in a range into a Mosaic with bounded concurrency. */
export async function loadMosaic(range, opts = {}) {
  const { z, x0, y0, nx, ny } = range;
  const mosaic = new Mosaic(z, x0, y0, nx, ny);
  const jobs = [];
  for (let ty = y0; ty < y0 + ny; ty++) for (let tx = x0; tx < x0 + nx; tx++) jobs.push([tx, ty]);
  const concurrency = opts.concurrency || 8;
  let next = 0, done = 0;
  const total = jobs.length;
  const worker = async () => {
    while (next < jobs.length) {
      const [tx, ty] = jobs[next++];
      const tile = await fetchTile(z, tx, ty, opts);
      if (tile) mosaic.put(tx, ty, tile); else mosaic.missing++;
      done++;
      opts.onProgress?.({ stage: 'tiles', zoom: z, done, total });
    }
  };
  await Promise.all(Array.from({ length: Math.min(concurrency, jobs.length) }, worker));
  return mosaic;
}
