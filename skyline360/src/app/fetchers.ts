/**
 * Network side of the pipeline: Overpass, terrain tiles, geocoding. Browser
 * only (fetch, createImageBitmap). Everything is cached per URL so repeated
 * generates of the same area cost nothing.
 */
import type { AreaData, HeightGrid } from '../core/types';
import { LocalFrame, bboxForRadius, terrainZoomFor, tileRangeForBbox } from '../core/geo';
import { OVERPASS_URL, buildOverpassQuery, parseOverpass, type OverpassResponse } from '../core/overpass';
import { decodeTerrarium, gridFromTiles, type DecodedTile } from '../core/terrarium';

export const USER_AGENT_NOTE = 'Skyline360 prototype (HDRI generator; contact via the project repository)';

export interface TerrainSource {
  id: string;
  name: string;
  url: (z: number, x: number, y: number) => string;
  tilePx: number;
  maxZoom: number;
  credit: string;
}

export const TERRAIN_SOURCES: TerrainSource[] = [
  {
    id: 'mapterhorn',
    name: 'Mapterhorn',
    url: (z, x, y) => `https://tiles.mapterhorn.com/${z}/${x}/${y}.webp`,
    tilePx: 512,
    maxZoom: 15,
    credit: 'Elevation: Mapterhorn (https://mapterhorn.com), built from open national and global DEMs; see https://download.mapterhorn.com/attribution.json',
  },
  {
    id: 'aws',
    name: 'AWS Terrain Tiles',
    url: (z, x, y) => `https://s3.amazonaws.com/elevation-tiles-prod/terrarium/${z}/${x}/${y}.png`,
    tilePx: 256,
    maxZoom: 15,
    credit: 'Elevation: Mapzen Terrain Tiles via the AWS Open Data registry; sources per https://github.com/tilezen/joerd/blob/master/docs/attribution.md',
  },
];

// ---------------------------------------------------------------------------
// Cache: Cache Storage when available (http/https), memory otherwise.

const memoryCache = new Map<string, ArrayBuffer>();

async function cachedFetch(url: string, init?: RequestInit): Promise<ArrayBuffer> {
  const key = init?.body ? `${url}#${typeof init.body === 'string' ? hashString(init.body) : ''}` : url;
  const mem = memoryCache.get(key);
  if (mem) return mem;
  let cache: Cache | null = null;
  try {
    if (typeof caches !== 'undefined') cache = await caches.open('skyline360-v1');
  } catch {
    cache = null;
  }
  if (cache) {
    const hit = await cache.match(key);
    if (hit) {
      const buf = await hit.arrayBuffer();
      memoryCache.set(key, buf);
      return buf;
    }
  }
  const res = await fetch(url, init);
  if (!res.ok) throw new Error(`${res.status} ${res.statusText} for ${url}`);
  const buf = await res.arrayBuffer();
  memoryCache.set(key, buf);
  if (cache) {
    try {
      await cache.put(key, new Response(buf.slice(0), { headers: { 'content-type': res.headers.get('content-type') ?? 'application/octet-stream' } }));
    } catch {
      /* quota or opaque response: ignore */
    }
  }
  return buf;
}

function hashString(s: string): string {
  let h = 2166136261;
  for (let i = 0; i < s.length; i++) {
    h ^= s.charCodeAt(i);
    h = Math.imul(h, 16777619);
  }
  return (h >>> 0).toString(16);
}

export async function clearCache(): Promise<void> {
  memoryCache.clear();
  try {
    if (typeof caches !== 'undefined') await caches.delete('skyline360-v1');
  } catch {
    /* ignore */
  }
}

// ---------------------------------------------------------------------------
// Overpass

export async function fetchArea(lon: number, lat: number, radius: number, log: (s: string) => void): Promise<AreaData> {
  const bbox = bboxForRadius(lon, lat, radius);
  const query = buildOverpassQuery(bbox);
  log(`Overpass: querying ${(radius / 1000).toFixed(1)} km radius …`);
  const t0 = performance.now();
  const buf = await cachedFetch(OVERPASS_URL, {
    method: 'POST',
    body: 'data=' + encodeURIComponent(query),
    headers: { 'Content-Type': 'application/x-www-form-urlencoded' },
  });
  const json = JSON.parse(new TextDecoder().decode(buf)) as OverpassResponse & { remark?: string };
  if (json.remark) log(`Overpass remark: ${json.remark}`);
  const data = parseOverpass(json);
  log(`Overpass: ${data.buildings.length} buildings, ${data.ground.length} ground polygons, ${data.roads.length} roads in ${((performance.now() - t0) / 1000).toFixed(1)} s (${(buf.byteLength / 1e6).toFixed(1)} MB)`);
  return data;
}

// ---------------------------------------------------------------------------
// Terrain

async function decodeTile(buf: ArrayBuffer, size: number): Promise<Float32Array> {
  const bmp = await createImageBitmap(new Blob([buf]), { premultiplyAlpha: 'none', colorSpaceConversion: 'none' });
  const canvas = typeof OffscreenCanvas !== 'undefined' ? new OffscreenCanvas(size, size) : Object.assign(document.createElement('canvas'), { width: size, height: size });
  const ctx = canvas.getContext('2d', { willReadFrequently: true }) as OffscreenCanvasRenderingContext2D | CanvasRenderingContext2D;
  ctx.drawImage(bmp, 0, 0);
  const img = ctx.getImageData(0, 0, size, size);
  bmp.close();
  return decodeTerrarium(img.data, size, size);
}

/** Bilinear resample of a square height tile. */
function resampleTile(src: Float32Array, from: number, to: number): Float32Array {
  if (from === to) return src;
  const out = new Float32Array(to * to);
  const scale = from / to;
  for (let y = 0; y < to; y++) {
    const fy = Math.min((y + 0.5) * scale - 0.5, from - 1);
    const y0 = Math.max(0, Math.floor(fy));
    const y1 = Math.min(y0 + 1, from - 1);
    const ty = Math.max(0, fy - y0);
    for (let x = 0; x < to; x++) {
      const fx = Math.min((x + 0.5) * scale - 0.5, from - 1);
      const x0 = Math.max(0, Math.floor(fx));
      const x1 = Math.min(x0 + 1, from - 1);
      const tx = Math.max(0, fx - x0);
      const a = src[y0 * from + x0] * (1 - tx) + src[y0 * from + x1] * tx;
      const b = src[y1 * from + x0] * (1 - tx) + src[y1 * from + x1] * tx;
      out[y * to + x] = a * (1 - ty) + b * ty;
    }
  }
  return out;
}

export interface TerrainResult {
  grid: HeightGrid;
  source: TerrainSource;
  zoom: number;
  tiles: number;
  missing: number;
}

/**
 * Fetch terrain for the area from the first source that answers; tiles that
 * are missing in the primary source are filled from the fallback.
 */
export interface TerrainFetchOptions {
  /** Desired metres per height sample; defaults to the area's extent over 1,600 samples. */
  targetMetres?: number;
  label?: string;
}

export async function fetchTerrain(lon: number, lat: number, radius: number, frame: LocalFrame, log: (s: string) => void, fetchOpts: TerrainFetchOptions = {}): Promise<TerrainResult | null> {
  const bbox = bboxForRadius(lon, lat, radius + 50);
  const targetMetres = fetchOpts.targetMetres ?? Math.max(3, (2 * radius) / 1600);
  const label = fetchOpts.label ?? 'Terrain';
  let lastError: unknown = null;
  for (const source of TERRAIN_SOURCES) {
    const zoom = terrainZoomFor(lat, targetMetres, source.tilePx, source.maxZoom);
    const range = tileRangeForBbox(bbox, zoom);
    const count = (range.xMax - range.xMin + 1) * (range.yMax - range.yMin + 1);
    log(`${label}: ${source.name} z${zoom}, ${count} tiles …`);
    const t0 = performance.now();
    const tiles: DecodedTile[] = [];
    let missing = 0;
    let failedHard = false;
    const jobs: Promise<void>[] = [];
    let active = 0;
    const queue: [number, number][] = [];
    for (let x = range.xMin; x <= range.xMax; x++) for (let y = range.yMin; y <= range.yMax; y++) queue.push([x, y]);
    await new Promise<void>((resolve) => {
      const next = (): void => {
        if (failedHard) return resolve();
        if (queue.length === 0 && active === 0) return resolve();
        while (active < 6 && queue.length) {
          const [x, y] = queue.shift()!;
          active++;
          const job = cachedFetch(source.url(zoom, x, y))
            .then((buf) => decodeTile(buf, source.tilePx))
            .then((heights) => {
              tiles.push({ x, y, z: zoom, size: source.tilePx, heights });
            })
            .catch((err: unknown) => {
              const msg = String(err);
              if (msg.includes('404')) missing++;
              else {
                lastError = err;
                failedHard = true;
              }
            })
            .finally(() => {
              active--;
              next();
            });
          jobs.push(job);
        }
      };
      next();
    });
    await Promise.allSettled(jobs);
    if (failedHard) {
      log(`${label}: ${source.name} unavailable (${String(lastError).slice(0, 80)}), trying next source`);
      continue;
    }
    if (tiles.length === 0) {
      log(`${label}: ${source.name} has no tiles here, trying next source`);
      continue;
    }
    // Fill tiles the primary source lacks from the next source (resampled to this tile size).
    if (missing > 0) {
      const fallback = TERRAIN_SOURCES.find((s) => s !== source && zoom <= s.maxZoom);
      if (fallback) {
        const have = new Set(tiles.map((t) => `${t.x}/${t.y}`));
        let filled = 0;
        for (let x = range.xMin; x <= range.xMax; x++) {
          for (let y = range.yMin; y <= range.yMax; y++) {
            if (have.has(`${x}/${y}`)) continue;
            try {
              const heights = await decodeTile(await cachedFetch(fallback.url(zoom, x, y)), fallback.tilePx);
              tiles.push({ x, y, z: zoom, size: source.tilePx, heights: resampleTile(heights, fallback.tilePx, source.tilePx) });
              filled++;
            } catch {
              /* stays at sea level */
            }
          }
        }
        if (filled) log(`${label}: ${filled} missing tiles filled from ${fallback.name}`);
        missing -= filled;
      }
    }
    const grid = gridFromTiles(tiles, range, frame, 0);
    log(`${label}: ${tiles.length} tiles (${missing} missing → sea level), ${grid.nx}×${grid.ny} cells of ${grid.dx.toFixed(1)} m in ${((performance.now() - t0) / 1000).toFixed(1)} s`);
    return { grid, source, zoom, tiles: tiles.length, missing };
  }
  log(`${label}: no source reachable (${String(lastError).slice(0, 120)}); using flat ground`);
  return null;
}

// ---------------------------------------------------------------------------
// Geocoding (Photon, OSM data)

export interface PlaceHit {
  name: string;
  lon: number;
  lat: number;
}

export async function geocode(query: string): Promise<PlaceHit[]> {
  const url = `https://photon.komoot.io/api/?q=${encodeURIComponent(query)}&limit=6`;
  const res = await fetch(url);
  if (!res.ok) throw new Error(`Photon ${res.status}`);
  const json = (await res.json()) as { features: { geometry: { coordinates: [number, number] }; properties: Record<string, string> }[] };
  return json.features.map((f) => {
    const p = f.properties;
    const name = [p.name, p.city ?? p.county, p.country].filter(Boolean).join(', ');
    return { name, lon: f.geometry.coordinates[0], lat: f.geometry.coordinates[1] };
  });
}

export const GEOCODER_CREDIT = 'Search: Photon (komoot) on OpenStreetMap data';
