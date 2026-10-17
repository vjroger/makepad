/**
 * Land-cover raster: ground classes (green, water, road) painted into a
 * texel grid that covers the terrain. The terrain mesh samples it as its
 * albedo and roughness map, so ground classes follow the terrain exactly.
 *
 * Texel (i, j) covers x ∈ [ox + i·ts, ox + (i+1)·ts), y likewise with j
 * counting northward; row 0 is the southern edge, matching the terrain uv.
 */
import type { GroundPolygon, HeightGrid, RoadLine } from './types';
import { LocalFrame } from './geo';

export const COVER_TERRAIN = 0;
export const COVER_GREEN = 1;
export const COVER_WATER = 2;
export const COVER_ROAD = 3;

export interface LandCover {
  width: number;
  height: number;
  /** Class id per texel, row-major from the south-west corner. */
  classes: Uint8Array;
  /** Tree density per texel: 0 none, 1 sparse, 2 dense. */
  trees: Uint8Array;
  originX: number;
  originY: number;
  texelSize: number;
  /** Fraction of texels per class, for the stats panel. */
  fractions: { green: number; water: number; road: number };
}

/** Even-odd scanline fill of a set of rings (local metres) into the class raster. */
function fillRings(cover: LandCover, rings: [number, number][][], cls: number, target: Uint8Array = cover.classes): void {
  const { width, height, originX, originY, texelSize: ts } = cover;
  const classes = target;
  let yMin = Infinity, yMax = -Infinity;
  for (const r of rings) for (const p of r) {
    if (p[1] < yMin) yMin = p[1];
    if (p[1] > yMax) yMax = p[1];
  }
  const jStart = Math.max(0, Math.floor((yMin - originY) / ts));
  const jEnd = Math.min(height - 1, Math.ceil((yMax - originY) / ts));
  const xs: number[] = [];
  for (let j = jStart; j <= jEnd; j++) {
    const yc = originY + (j + 0.5) * ts;
    xs.length = 0;
    for (const r of rings) {
      for (let i = 0, k = r.length - 1; i < r.length; k = i++) {
        const a = r[k], b = r[i];
        if (a[1] === b[1]) continue;
        const lo = a[1] < b[1] ? a : b;
        const hi = a[1] < b[1] ? b : a;
        if (yc < lo[1] || yc >= hi[1]) continue;
        xs.push(lo[0] + ((yc - lo[1]) * (hi[0] - lo[0])) / (hi[1] - lo[1]));
      }
    }
    if (xs.length < 2) continue;
    xs.sort((p, q) => p - q);
    for (let s = 0; s + 1 < xs.length; s += 2) {
      const i0 = Math.max(0, Math.ceil((xs[s] - originX) / ts - 0.5));
      const i1 = Math.min(width - 1, Math.floor((xs[s + 1] - originX) / ts - 0.5));
      for (let i = i0; i <= i1; i++) classes[j * width + i] = cls;
    }
  }
}

/** Rasterize a road centreline as a ribbon of `width` metres. */
function fillRoad(cover: LandCover, pts: [number, number][], width: number): void {
  const hw = Math.max(width / 2, cover.texelSize * 0.6);
  for (let i = 0; i < pts.length - 1; i++) {
    const p = pts[i], q = pts[i + 1];
    const dx = q[0] - p[0], dy = q[1] - p[1];
    const len = Math.hypot(dx, dy);
    if (len < 1e-3) continue;
    const ox = (-dy / len) * hw, oy = (dx / len) * hw;
    // Extend the quad by hw along the segment so joints have no gaps.
    const ex = (dx / len) * hw, ey = (dy / len) * hw;
    const quad: [number, number][] = [
      [p[0] + ox - ex, p[1] + oy - ey],
      [q[0] + ox + ex, q[1] + oy + ey],
      [q[0] - ox + ex, q[1] - oy + ey],
      [p[0] - ox - ex, p[1] - oy - ey],
    ];
    fillRings(cover, [quad], COVER_ROAD);
  }
}

/**
 * Build the land-cover raster for the terrain grid. `maxSide` caps the
 * raster size; the texel size is at least `minTexel` metres.
 */
export function buildLandCover(polys: GroundPolygon[], roads: RoadLine[], frame: LocalFrame, grid: HeightGrid, maxSide = 2048, minTexel = 1.5): LandCover {
  const x0 = grid.x0 - grid.dx / 2;
  const y0 = grid.y0 - grid.dy / 2;
  const extentX = grid.nx * grid.dx;
  const extentY = grid.ny * grid.dy;
  const ts = Math.max(minTexel, Math.max(extentX, extentY) / maxSide);
  const width = Math.max(1, Math.round(extentX / ts));
  const height = Math.max(1, Math.round(extentY / ts));
  const cover: LandCover = {
    width,
    height,
    classes: new Uint8Array(width * height),
    trees: new Uint8Array(width * height),
    originX: x0,
    originY: y0,
    texelSize: ts,
    fractions: { green: 0, water: 0, road: 0 },
  };
  const local = (ring: [number, number][]): [number, number][] => ring.map(([lon, lat]) => frame.toLocal(lon, lat));
  // Paint order: green, then water (ponds inside parks), then roads on top.
  for (const kind of ['green', 'water'] as const) {
    for (const poly of polys) {
      if (poly.kind !== kind) continue;
      fillRings(cover, [local(poly.outer), ...poly.holes.map(local)], kind === 'green' ? COVER_GREEN : COVER_WATER);
    }
  }
  for (const road of roads) fillRoad(cover, local(road.points), road.width);
  // Tree density, then cleared wherever water or a road ended up on top.
  for (const poly of polys) {
    if (poly.kind !== 'green' || poly.trees === 'none') continue;
    fillRings(cover, [local(poly.outer), ...poly.holes.map(local)], poly.trees === 'dense' ? 2 : 1, cover.trees);
  }
  for (let i = 0; i < cover.classes.length; i++) {
    if (cover.classes[i] === COVER_WATER || cover.classes[i] === COVER_ROAD) cover.trees[i] = 0;
  }
  let g = 0, w = 0, r = 0;
  for (let i = 0; i < cover.classes.length; i++) {
    const c = cover.classes[i];
    if (c === COVER_GREEN) g++;
    else if (c === COVER_WATER) w++;
    else if (c === COVER_ROAD) r++;
  }
  const n = cover.classes.length;
  cover.fractions = { green: g / n, water: w / n, road: r / n };
  return cover;
}

export interface CoverColours {
  terrain: [number, number, number];
  green: [number, number, number];
  water: [number, number, number];
  road: [number, number, number];
}

/** RGBA8 albedo (linear) and roughness textures from the class raster. */
export function landCoverTextures(cover: LandCover, colours: CoverColours): { albedo: Uint8Array; roughness: Uint8Array } {
  const n = cover.width * cover.height;
  const albedo = new Uint8Array(n * 4);
  const roughness = new Uint8Array(n * 4);
  const table: [number, number, number][] = [colours.terrain, colours.green, colours.water, colours.road];
  const rough = [255, 255, 30, 215]; // terrain, green, water, road (roughness × 255)
  for (let i = 0; i < n; i++) {
    const c = cover.classes[i];
    const col = table[c];
    albedo[i * 4] = Math.round(Math.min(1, col[0]) * 255);
    albedo[i * 4 + 1] = Math.round(Math.min(1, col[1]) * 255);
    albedo[i * 4 + 2] = Math.round(Math.min(1, col[2]) * 255);
    albedo[i * 4 + 3] = 255;
    // three.js reads roughness from the green channel.
    roughness[i * 4 + 1] = rough[c];
    roughness[i * 4 + 3] = 255;
  }
  return { albedo, roughness };
}

/** Class at a local position (for tests). */
export function coverAt(cover: LandCover, x: number, y: number): number {
  const i = Math.floor((x - cover.originX) / cover.texelSize);
  const j = Math.floor((y - cover.originY) / cover.texelSize);
  if (i < 0 || j < 0 || i >= cover.width || j >= cover.height) return COVER_TERRAIN;
  return cover.classes[j * cover.width + i];
}
