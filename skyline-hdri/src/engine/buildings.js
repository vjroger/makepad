// Buildings from OpenStreetMap through the Overpass API: query, parse, estimate heights,
// and scan-convert footprints into the height tiers with a max().

import { toLocal } from './geo.js';
import { sampleTier, tierIndexFor } from './tiers.js';

// Public mirrors. Note: a browser can only use mirrors that send CORS headers, and an error page
// (502/504) from a mirror usually has none, so a down mirror shows up as "Failed to fetch".
// The Rust server's /overpass proxy (skyline --serve) sidesteps both problems; the app tries it first.
export const DEFAULT_OVERPASS_URLS = [
  'https://overpass-api.de/api/interpreter',
  'https://overpass.kumi.systems/api/interpreter',
  'https://lz4.overpass-api.de/api/interpreter',
  'https://maps.mail.ru/osm/tools/overpass/api/interpreter',
  'https://overpass.private.coffee/api/interpreter',
];

const MAN_MADE = '^(tower|mast|chimney|communications_tower|lighthouse|water_tower|windmill)$';

/**
 * Two-tier query: everything within allRadius, only tall things (height tag >= 30 m or >= 10 levels,
 * refined later in JS) within tallRadius, plus free-standing towers and masts.
 */
export function buildQuery(lat, lon, allRadius, tallRadius) {
  const a = `(around:${Math.round(allRadius)},${lat},${lon})`;
  const t = `(around:${Math.round(tallRadius)},${lat},${lon})`;
  const tallH = '["height"~"^([3-9][0-9]|[1-9][0-9]{2,})"]';
  const tallL = '["building:levels"~"^([1-9][0-9]+)$"]';
  return `[out:json][timeout:180];
(
  way["building"]${a};
  way["building:part"]${a};
  relation["building"]${a};
  way["building"]${tallH}${t};
  way["building:part"]${tallH}${t};
  relation["building"]${tallH}${t};
  way["building"]${tallL}${t};
  way["building:part"]${tallL}${t};
  relation["building"]${tallL}${t};
  node["man_made"~"${MAN_MADE}"]${t};
  way["man_made"~"${MAN_MADE}"]${t};
);
out geom qt;`;
}

/** POST the query to the first Overpass mirror that answers. Returns the raw `elements` array. */
export async function fetchBuildings(lat, lon, opts = {}) {
  const urls = opts.overpassUrls || DEFAULT_OVERPASS_URLS;
  const query = buildQuery(lat, lon, opts.allRadius ?? 2500, opts.tallRadius ?? 30000);
  let lastErr = null;
  for (const url of urls) {
    try {
      opts.onProgress?.({ stage: 'buildings', message: `querying ${new URL(url).host}` });
      const resp = await fetch(url, {
        method: 'POST',
        headers: { 'Content-Type': 'application/x-www-form-urlencoded' },
        body: 'data=' + encodeURIComponent(query),
      });
      if (!resp.ok) throw new Error(`HTTP ${resp.status} from ${url}` + (resp.status === 502 || resp.status === 504 || resp.status === 429 ? ' (mirror overloaded)' : ''));
      const json = await resp.json();
      return json.elements || [];
    } catch (err) {
      lastErr = err;
      opts.onError?.(url, err.message === 'Failed to fetch' ? new Error('Failed to fetch (mirror down, or it sends no CORS headers; run the page through `skyline --serve` to proxy the query)') : err);
    }
  }
  throw lastErr || new Error('No Overpass mirror answered');
}

// ---------------------------------------------------------------------------------------------
// Height estimation

const DEFAULT_BY_TYPE = {
  house: 7, detached: 7, semidetached_house: 7, terrace: 8, bungalow: 4, residential: 10,
  apartments: 14, dormitory: 14, hotel: 20, commercial: 15, office: 16, retail: 6, supermarket: 6,
  kiosk: 3, industrial: 9, warehouse: 10, factory: 10, manufacture: 10, church: 20, cathedral: 35,
  chapel: 8, mosque: 15, synagogue: 12, temple: 12, school: 10, kindergarten: 5, university: 14,
  college: 12, hospital: 18, public: 12, civic: 12, government: 14, garage: 3, garages: 3, shed: 3,
  hut: 3, roof: 4, carport: 3, greenhouse: 4, barn: 7, farm: 7, farm_auxiliary: 6, stable: 5,
  stadium: 25, sports_hall: 12, train_station: 12, transportation: 8, parking: 10, hangar: 10,
  silo: 20, bunker: 3, service: 4, construction: 10, ruins: 4, tower: 30,
};

const MAN_MADE_HEIGHT = {
  tower: 30, mast: 60, chimney: 50, communications_tower: 100, lighthouse: 25, water_tower: 30, windmill: 20,
};

/** Parse an OSM height-like value ("25", "25 m", "82 ft", "82'", "25,5") into metres. */
export function parseLength(value) {
  if (value == null) return null;
  const m = String(value).trim().replace(',', '.').match(/^(-?\d+(?:\.\d+)?)\s*(m|ft|feet|')?/i);
  if (!m) return null;
  const v = parseFloat(m[1]);
  if (!isFinite(v)) return null;
  const unit = (m[2] || 'm').toLowerCase();
  return unit === 'm' ? v : v * 0.3048;
}

/** Estimate a height in metres from tags; `area` (m^2) refines generic buildings. */
export function estimateHeight(tags, area = 0) {
  const h = parseLength(tags.height) ?? parseLength(tags['building:height']) ?? parseLength(tags.est_height);
  if (h != null && h > 0) return h;
  const levels = parseFloat(tags['building:levels']);
  if (isFinite(levels) && levels > 0) {
    const roof = parseFloat(tags['roof:levels']);
    return levels * 3.0 + (isFinite(roof) ? roof * 2.5 : 0);
  }
  if (tags.man_made && MAN_MADE_HEIGHT[tags.man_made]) return MAN_MADE_HEIGHT[tags.man_made];
  const type = tags.building || tags['building:part'] || 'yes';
  if (DEFAULT_BY_TYPE[type] != null) return DEFAULT_BY_TYPE[type];
  if (area > 2000) return 12;
  if (area > 500) return 9;
  return 8;
}

// ---------------------------------------------------------------------------------------------
// Parsing Overpass geometry into local polygons

function polygonArea(poly) {
  let a = 0;
  for (let i = 0, n = poly.length; i < n; i++) {
    const [x0, y0] = poly[i], [x1, y1] = poly[(i + 1) % n];
    a += x0 * y1 - x1 * y0;
  }
  return Math.abs(a) / 2;
}

function centroid(poly) {
  let x = 0, y = 0;
  for (const p of poly) { x += p[0]; y += p[1]; }
  return [x / poly.length, y / poly.length];
}

function ringToLocal(geometry, lat0, lon0) {
  const poly = geometry.map((p) => toLocal(lat0, lon0, p.lat, p.lon));
  if (poly.length > 1) {
    const f = poly[0], l = poly[poly.length - 1];
    if (f[0] === l[0] && f[1] === l[1]) poly.pop();
  }
  return poly.length >= 3 ? poly : null;
}

/**
 * Turn Overpass elements into { polys, height, kind, area, centroid } records in local metres.
 * kind: 'building' | 'part' | 'tower'
 */
export function parseBuildings(elements, lat0, lon0, opts = {}) {
  const towerSize = opts.towerSize ?? 8; // m, footprint of a point tower
  const out = [];
  for (const el of elements) {
    const tags = el.tags || {};
    if (el.type === 'node') {
      if (!tags.man_made) continue;
      const [e, n] = toLocal(lat0, lon0, el.lat, el.lon);
      const s = (parseLength(tags.width) ?? parseLength(tags.diameter) ?? towerSize) / 2;
      const poly = [[e - s, n - s], [e + s, n - s], [e + s, n + s], [e - s, n + s]];
      out.push({ polys: [poly], height: estimateHeight(tags, 4 * s * s), kind: 'tower', area: 4 * s * s, centroid: [e, n], tags });
      continue;
    }
    let polys = [];
    if (el.type === 'way' && el.geometry) {
      const poly = ringToLocal(el.geometry, lat0, lon0);
      if (poly) polys.push(poly);
    } else if (el.type === 'relation' && el.members) {
      for (const m of el.members) {
        if (m.type !== 'way' || !m.geometry || (m.role && m.role !== 'outer')) continue;
        const poly = ringToLocal(m.geometry, lat0, lon0);
        if (poly) polys.push(poly);
      }
    }
    if (!polys.length) continue;
    const area = polys.reduce((s, p) => s + polygonArea(p), 0);
    const kind = tags['building:part'] ? 'part' : tags.building ? 'building' : 'tower';
    const height = estimateHeight(tags, area);
    if (!(height > 0)) continue;
    out.push({ polys, height, kind, area, centroid: centroid(polys[0]), tags });
  }
  return out;
}

export function pointInPolygon(poly, x, y) {
  let inside = false;
  for (let i = 0, j = poly.length - 1; i < poly.length; j = i++) {
    const [xi, yi] = poly[i], [xj, yj] = poly[j];
    if ((yi > y) !== (yj > y) && x < ((xj - xi) * (y - yi)) / (yj - yi) + xi) inside = !inside;
  }
  return inside;
}

// ---------------------------------------------------------------------------------------------
// Rasterisation into tiers

/**
 * Scan-convert one polygon (local metres) into a tier with max(). Texel (i, j) is inside when its
 * centre is inside the polygon; a polygon that covers no centre marks the texel under its centroid
 * so thin towers survive coarse tiers. Returns the number of texels written.
 */
export function rasterizePolygon(tier, poly, roofAbs) {
  const { size, texel, height, cls } = tier;
  const half = size / 2 - 0.5;
  const n = poly.length;
  const xs = new Float64Array(n), ys = new Float64Array(n);
  let minJ = Infinity, maxJ = -Infinity;
  for (let k = 0; k < n; k++) {
    xs[k] = poly[k][0] / texel + half;
    ys[k] = poly[k][1] / texel + half;
    if (ys[k] < minJ) minJ = ys[k];
    if (ys[k] > maxJ) maxJ = ys[k];
  }
  const j0 = Math.max(0, Math.ceil(minJ)), j1 = Math.min(size - 1, Math.floor(maxJ));
  const cuts = [];
  let written = 0;
  for (let j = j0; j <= j1; j++) {
    cuts.length = 0;
    for (let k = 0, m = n - 1; k < n; m = k++) {
      const y0 = ys[m], y1 = ys[k];
      if ((y0 <= j) !== (y1 <= j)) {
        cuts.push(xs[m] + (j - y0) / (y1 - y0) * (xs[k] - xs[m]));
      }
    }
    if (cuts.length < 2) continue;
    cuts.sort((a, b) => a - b);
    for (let c = 0; c + 1 < cuts.length; c += 2) {
      const ia = Math.max(0, Math.ceil(cuts[c])), ib = Math.min(size - 1, Math.floor(cuts[c + 1]));
      const row = j * size;
      for (let i = ia; i <= ib; i++) {
        if (roofAbs > height[row + i]) { height[row + i] = roofAbs; cls[row + i] = 1; }
        written++;
      }
    }
  }
  if (written === 0) {
    let cx = 0, cy = 0;
    for (let k = 0; k < n; k++) { cx += xs[k]; cy += ys[k]; }
    const i = Math.round(cx / n), j = Math.round(cy / n);
    if (i >= 0 && i < size && j >= 0 && j < size) {
      const idx = j * size + i;
      if (roofAbs > height[idx]) { height[idx] = roofAbs; cls[idx] = 1; }
      written = 1;
    }
  }
  return written;
}

/**
 * Rasterise all buildings into the tiers.
 *   tiers: built terrain tiers (modified in place)
 *   opts.tallThreshold: metres; tiers with index >= opts.tallFromTier only take buildings this tall
 *   opts.maxTier: last tier index that takes buildings at all
 *   opts.excludeContaining: [east, north] — skip footprints containing this point (the eye)
 * Returns { count, skipped, maxRoof }.
 */
export function rasterizeBuildings(buildings, tiers, opts = {}) {
  const tallThreshold = opts.tallThreshold ?? 35;
  const tallFromTier = opts.tallFromTier ?? 3;
  const maxTier = Math.min(opts.maxTier ?? 3, tiers.length - 1);
  const ex = opts.excludeContaining;
  // Ground elevations first, before any roof changes the tiers.
  const grounds = new Float64Array(buildings.length);
  for (let b = 0; b < buildings.length; b++) {
    const [cx, cy] = buildings[b].centroid;
    const k = tierIndexFor(tiers, cx, cy);
    grounds[b] = k >= 0 ? sampleTier(tiers[k], cx, cy) : 0;
  }
  let count = 0, skipped = 0, maxRoof = -Infinity;
  for (let b = 0; b < buildings.length; b++) {
    const bld = buildings[b];
    if (ex && bld.polys.some((p) => pointInPolygon(p, ex[0], ex[1]))) { skipped++; continue; }
    const roofAbs = grounds[b] + bld.height * (opts.heightScale ?? 1);
    if (roofAbs > maxRoof) maxRoof = roofAbs;
    const dist = Math.hypot(bld.centroid[0], bld.centroid[1]);
    let any = false;
    for (let k = 0; k <= maxTier; k++) {
      const tier = tiers[k];
      if (dist > tier.radius) continue;
      if (k >= tallFromTier && bld.height < tallThreshold) continue;
      for (const poly of bld.polys) rasterizePolygon(tier, poly, roofAbs);
      any = true;
    }
    if (any) count++;
  }
  return { count, skipped, maxRoof: isFinite(maxRoof) ? maxRoof : 0 };
}
