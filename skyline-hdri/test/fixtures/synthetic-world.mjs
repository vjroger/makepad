// A synthetic landscape with analytic elevation, served as terrarium tiles + an Overpass fixture.
// Used by the Node tests and the headless-browser test so every number can be checked by hand.

import { destination, flatOffset, pixelToLatLon, toLocal, DEG, EARTH_RADIUS } from '../../src/engine/geo.js';
import { encodeRGB8 } from '../../src/export/png.js';

export const EYE = { lat: 52.0, lon: 5.0 };
export const PLAIN = 10;                  // m, the plain everywhere

// Features. Bearings in degrees clockwise from north, distances in metres from the eye.
export const CONE = { bearing: 45, distance: 20000, height: 1500, radius: 5000 };   // visible peak NE
export const HILL = { bearing: 180, distance: 100000, height: 100, radius: 10000 }; // hidden by curvature, S
export const SEA = { bearing: 270, from: 3000 };                                     // sea west of 3 km

const conePos = destination(EYE.lat, EYE.lon, CONE.bearing * DEG, CONE.distance);
const hillPos = destination(EYE.lat, EYE.lon, HILL.bearing * DEG, HILL.distance);

function localOf(lat, lon) { return toLocal(EYE.lat, EYE.lon, lat, lon); }

/** Analytic elevation in metres at a lat/lon. */
export function elevationAt(lat, lon) {
  const [e, n] = localOf(lat, lon);
  let h = PLAIN;
  // sea: west of the eye beyond 3 km (east < -3000), 20 m below sea level
  if (e < -SEA.from) h = -20;
  const [ce, cn] = localOf(conePos[0], conePos[1]);
  const rc = Math.hypot(e - ce, n - cn);
  if (rc < CONE.radius) h = Math.max(h, CONE.height * (1 - rc / CONE.radius));
  const [he, hn] = localOf(hillPos[0], hillPos[1]);
  const rh = Math.hypot(e - he, n - hn);
  if (rh < HILL.radius) h = Math.max(h, HILL.height * (1 - rh / HILL.radius));
  return h;
}

export function encodeTerrarium(h) {
  const v = Math.round((h + 32768) * 256);
  const r = (v >> 16) & 255, g = (v >> 8) & 255, b = v & 255;
  return [r, g, b];
}

/** Render one 256x256 terrarium tile as PNG bytes. */
export async function renderTile(z, x, y) {
  const rgb = new Uint8Array(256 * 256 * 3);
  for (let j = 0; j < 256; j++) {
    for (let i = 0; i < 256; i++) {
      const [lat, lon] = pixelToLatLon(x * 256 + i + 0.5, y * 256 + j + 0.5, z);
      const [r, g, b] = encodeTerrarium(elevationAt(lat, lon));
      const k = (j * 256 + i) * 3;
      rgb[k] = r; rgb[k + 1] = g; rgb[k + 2] = b;
    }
  }
  return encodeRGB8(256, 256, rgb);
}

// Buildings (local metres from the eye). Each is converted to lat/lon geometry for the fixture.
export const BUILDINGS = [
  { name: 'tower', center: [500, 0], size: [20, 20], tags: { building: 'yes', height: '100' } },          // E, 100 m
  { name: 'block', center: [0, 300], size: [40, 60], tags: { building: 'apartments', 'building:levels': '10' } }, // N, 30 m
  { name: 'relation', center: [0, -1000], size: [50, 50], tags: { building: 'yes', height: '40 m' }, relation: true }, // S, 40 m
  { name: 'feet', center: [0, 0], size: [30, 30], tags: { building: 'yes', height: '300' } },             // contains the eye: must be skipped
];
export const MAST = { local: [1414.2, 1414.2], tags: { man_made: 'mast', height: '150' } };                // NE 2 km

function rect(center, size) {
  const [cx, cy] = center, [w, h] = size;
  return [[cx - w / 2, cy - h / 2], [cx + w / 2, cy - h / 2], [cx + w / 2, cy + h / 2], [cx - w / 2, cy + h / 2], [cx - w / 2, cy - h / 2]];
}

function geom(points) {
  return points.map(([e, n]) => { const [lat, lon] = flatOffset(EYE.lat, EYE.lon, e, n); return { lat, lon }; });
}

export function overpassFixture() {
  const elements = [];
  let id = 1;
  for (const b of BUILDINGS) {
    const g = geom(rect(b.center, b.size));
    if (b.relation) {
      elements.push({ type: 'relation', id: id++, tags: { ...b.tags, type: 'multipolygon' }, members: [{ type: 'way', ref: id++, role: 'outer', geometry: g }] });
    } else {
      elements.push({ type: 'way', id: id++, tags: b.tags, geometry: g });
    }
  }
  const [mlat, mlon] = flatOffset(EYE.lat, EYE.lon, MAST.local[0], MAST.local[1]);
  elements.push({ type: 'node', id: id++, lat: mlat, lon: mlon, tags: MAST.tags });
  return { version: 0.6, generator: 'fixture', elements };
}

export { EARTH_RADIUS };
