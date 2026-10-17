/**
 * Geographic helpers: Web Mercator tile maths and the local metric frame.
 */

export const EARTH_RADIUS = 6378137; // metres, WGS84 semi-major axis
const DEG = Math.PI / 180;

/** Metres per degree of latitude and longitude at a given latitude. */
export function metresPerDegree(lat: number): { east: number; north: number } {
  return { east: DEG * EARTH_RADIUS * Math.cos(lat * DEG), north: DEG * EARTH_RADIUS };
}

/**
 * Local tangent-plane frame centred on a point. Within ~10 km the
 * equirectangular approximation is accurate to well under a metre, which is
 * far below the data's own accuracy.
 */
export class LocalFrame {
  readonly lon0: number;
  readonly lat0: number;
  private readonly mEast: number;
  private readonly mNorth: number;

  constructor(lon0: number, lat0: number) {
    this.lon0 = lon0;
    this.lat0 = lat0;
    const m = metresPerDegree(lat0);
    this.mEast = m.east;
    this.mNorth = m.north;
  }

  /** [east, north] metres of a lon/lat. */
  toLocal(lon: number, lat: number): [number, number] {
    return [(lon - this.lon0) * this.mEast, (lat - this.lat0) * this.mNorth];
  }

  /** lon/lat of a local [east, north]. */
  toLonLat(east: number, north: number): [number, number] {
    return [this.lon0 + east / this.mEast, this.lat0 + north / this.mNorth];
  }
}

/** Bounding box [minLon, minLat, maxLon, maxLat] of a circle of `radius` metres. */
export function bboxForRadius(lon: number, lat: number, radius: number): [number, number, number, number] {
  const m = metresPerDegree(lat);
  const dLon = radius / m.east;
  const dLat = radius / m.north;
  return [lon - dLon, lat - dLat, lon + dLon, lat + dLat];
}

/** Great-circle distance in metres. */
export function haversine(lon1: number, lat1: number, lon2: number, lat2: number): number {
  const dLat = (lat2 - lat1) * DEG;
  const dLon = (lon2 - lon1) * DEG;
  const a =
    Math.sin(dLat / 2) ** 2 + Math.cos(lat1 * DEG) * Math.cos(lat2 * DEG) * Math.sin(dLon / 2) ** 2;
  return 2 * EARTH_RADIUS * Math.asin(Math.sqrt(a));
}

/** Fractional Web Mercator tile coordinates of a lon/lat at zoom z. */
export function lonLatToTile(lon: number, lat: number, z: number): { x: number; y: number } {
  const n = 2 ** z;
  const x = ((lon + 180) / 360) * n;
  const latR = lat * DEG;
  const y = ((1 - Math.log(Math.tan(latR) + 1 / Math.cos(latR)) / Math.PI) / 2) * n;
  return { x, y };
}

/** lon/lat of the north-west corner of tile x/y at zoom z (fractional tiles allowed). */
export function tileToLonLat(x: number, y: number, z: number): { lon: number; lat: number } {
  const n = 2 ** z;
  const lon = (x / n) * 360 - 180;
  const latR = Math.atan(Math.sinh(Math.PI * (1 - (2 * y) / n)));
  return { lon, lat: latR / DEG };
}

export interface TileRange {
  z: number;
  xMin: number;
  xMax: number;
  yMin: number;
  yMax: number;
}

/** Integer tile range covering a bbox at zoom z. */
export function tileRangeForBbox(bbox: [number, number, number, number], z: number): TileRange {
  const a = lonLatToTile(bbox[0], bbox[3], z); // north-west
  const b = lonLatToTile(bbox[2], bbox[1], z); // south-east
  const max = 2 ** z - 1;
  return {
    z,
    xMin: Math.max(0, Math.floor(a.x)),
    xMax: Math.min(max, Math.floor(b.x)),
    yMin: Math.max(0, Math.floor(a.y)),
    yMax: Math.min(max, Math.floor(b.y)),
  };
}

/** Ground resolution of one tile pixel at latitude `lat`, metres, for `tilePx` pixel tiles. */
export function metresPerPixel(lat: number, z: number, tilePx: number): number {
  return (2 * Math.PI * EARTH_RADIUS * Math.cos(lat * DEG)) / (tilePx * 2 ** z);
}

/** Pick the terrain zoom whose pixels are close to `targetMetres` at this latitude. */
export function terrainZoomFor(lat: number, targetMetres: number, tilePx: number, maxZoom: number): number {
  for (let z = 0; z <= maxZoom; z++) {
    if (metresPerPixel(lat, z, tilePx) <= targetMetres) return z;
  }
  return maxZoom;
}

/** Signed area of a ring in local metres (positive = counter-clockwise). */
export function ringArea(ring: [number, number][]): number {
  let a = 0;
  for (let i = 0, j = ring.length - 1; i < ring.length; j = i++) {
    a += ring[j][0] * ring[i][1] - ring[i][0] * ring[j][1];
  }
  return a / 2;
}
