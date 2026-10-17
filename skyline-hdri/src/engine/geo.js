// Geodesy helpers. Everything here is pure arithmetic so it ports 1:1 to Rust.
//
// Conventions:
//   - lat/lon in degrees, bearings in radians clockwise from north
//   - "east/north" are metres in the azimuthal-equidistant frame around the eye
//   - Web Mercator pixel coordinates are at 256 px per tile

export const EARTH_RADIUS = 6371000;          // m, mean sphere
export const REFRACTION_K = 0.13;             // standard terrestrial refraction coefficient
export const EFFECTIVE_RADIUS = EARTH_RADIUS / (1 - REFRACTION_K);
export const DEG = Math.PI / 180;

/** Great-circle destination point: start (lat, lon) deg, bearing rad, distance m. Returns [lat, lon] deg. */
export function destination(lat, lon, bearing, distance) {
  const phi1 = lat * DEG, lam1 = lon * DEG;
  const delta = distance / EARTH_RADIUS;
  const sinPhi1 = Math.sin(phi1), cosPhi1 = Math.cos(phi1);
  const sinDelta = Math.sin(delta), cosDelta = Math.cos(delta);
  const sinPhi2 = sinPhi1 * cosDelta + cosPhi1 * sinDelta * Math.cos(bearing);
  const phi2 = Math.asin(Math.max(-1, Math.min(1, sinPhi2)));
  const y = Math.sin(bearing) * sinDelta * cosPhi1;
  const x = cosDelta - sinPhi1 * sinPhi2;
  const lam2 = lam1 + Math.atan2(y, x);
  return [phi2 / DEG, ((lam2 / DEG + 540) % 360) - 180];
}

/** Local flat-frame offset (east, north metres) -> [lat, lon] deg. Good to ~0.1% within 20 km. */
export function flatOffset(lat0, lon0, east, north) {
  const lat = lat0 + (north / EARTH_RADIUS) / DEG;
  const lon = lon0 + (east / (EARTH_RADIUS * Math.cos(lat0 * DEG))) / DEG;
  return [lat, lon];
}

/** (lat, lon) -> local flat-frame metres relative to (lat0, lon0). Inverse of flatOffset. */
export function toLocalFlat(lat0, lon0, lat, lon) {
  const north = (lat - lat0) * DEG * EARTH_RADIUS;
  const east = (lon - lon0) * DEG * EARTH_RADIUS * Math.cos(lat0 * DEG);
  return [east, north];
}

/**
 * (lat, lon) -> azimuthal-equidistant metres (east, north) around (lat0, lon0): great-circle
 * distance along the initial bearing. Exact inverse of destination(); used for building footprints.
 */
export function toLocal(lat0, lon0, lat, lon) {
  const phi1 = lat0 * DEG, phi2 = lat * DEG, dLam = (lon - lon0) * DEG;
  const sinDPhi = Math.sin((phi2 - phi1) / 2), sinDLam = Math.sin(dLam / 2);
  const a = sinDPhi * sinDPhi + Math.cos(phi1) * Math.cos(phi2) * sinDLam * sinDLam;
  const d = 2 * EARTH_RADIUS * Math.atan2(Math.sqrt(a), Math.sqrt(Math.max(0, 1 - a)));
  const y = Math.sin(dLam) * Math.cos(phi2);
  const x = Math.cos(phi1) * Math.sin(phi2) - Math.sin(phi1) * Math.cos(phi2) * Math.cos(dLam);
  const bearing = Math.atan2(y, x);
  return [d * Math.sin(bearing), d * Math.cos(bearing)];
}

/**
 * Azimuthal-equidistant frame point (east, north) metres -> [lat, lon] deg.
 * Uses the flat frame below `flatLimit` metres and the spherical direct formula beyond.
 */
export function aeqdToLatLon(lat0, lon0, east, north, flatLimit = 20000) {
  const d = Math.hypot(east, north);
  if (d < flatLimit) return flatOffset(lat0, lon0, east, north);
  return destination(lat0, lon0, Math.atan2(east, north), d);
}

/** Web Mercator: (lat, lon) -> fractional pixel coordinates at zoom z (256 px tiles). */
export function mercatorPixel(lat, lon, z) {
  const n = 256 * Math.pow(2, z);
  const x = (lon + 180) / 360 * n;
  const phi = Math.max(-85.05112878, Math.min(85.05112878, lat)) * DEG;
  const y = (1 - Math.log(Math.tan(phi) + 1 / Math.cos(phi)) / Math.PI) / 2 * n;
  return [x, y];
}

/** Inverse Web Mercator: pixel coordinates at zoom z -> [lat, lon]. */
export function pixelToLatLon(px, py, z) {
  const n = 256 * Math.pow(2, z);
  const lon = px / n * 360 - 180;
  const yv = Math.PI * (1 - 2 * py / n);
  const lat = Math.atan(Math.sinh(yv)) / DEG;
  return [lat, lon];
}

/** Metres per Mercator pixel at a latitude and zoom. */
export function metersPerPixel(lat, z) {
  return 156543.03392804097 * Math.cos(lat * DEG) / Math.pow(2, z);
}

/** Apparent elevation angle (rad) of a point at height h, ground distance d, seen from eye altitude ze. */
export function apparentElevation(h, d, ze) {
  return Math.atan2(h - ze - d * d / (2 * EFFECTIVE_RADIUS), d);
}

/** Height of the ray with elevation angle e at ground distance d, relative to the eye frame. */
export function rayHeight(e, d, ze) {
  return ze + d * Math.tan(e) + d * d / (2 * EFFECTIVE_RADIUS);
}
