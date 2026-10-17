/**
 * Building extrusion: footprints → walls and flat roofs in local metres.
 */
import earcut from 'earcut';
import type { Footprint, HeightGrid, MeshData } from './types';
import { LocalFrame, ringArea } from './geo';
import { MeshBuilder } from './mesh';
import { sampleHeight } from './terrarium';

export const METRES_PER_LEVEL = 3.2;

/** Default heights (metres) by OSM building value when no height or levels are tagged. */
const DEFAULT_HEIGHTS: Record<string, number> = {
  house: 7,
  detached: 7,
  semidetached_house: 7,
  terrace: 7,
  bungalow: 4,
  residential: 11,
  apartments: 14,
  commercial: 12,
  retail: 8,
  office: 15,
  industrial: 9,
  warehouse: 9,
  church: 18,
  cathedral: 30,
  chapel: 8,
  school: 9,
  university: 12,
  hospital: 15,
  hotel: 18,
  garage: 3,
  garages: 3,
  shed: 3,
  hut: 3,
  roof: 4,
  carport: 3,
  greenhouse: 4,
  farm_auxiliary: 6,
  barn: 7,
  parking: 9,
  stadium: 25,
  train_station: 12,
};

export interface HeightResolution {
  height: number;
  source: 'height' | 'levels' | 'default';
}

/** Decide a building's height from its tags, with a size-aware fallback. */
export function resolveHeight(fp: Footprint, areaM2: number): HeightResolution {
  if (fp.height !== null && fp.height > 0) return { height: Math.min(fp.height, 500), source: 'height' };
  if (fp.levels !== null && fp.levels > 0) return { height: Math.min(fp.levels * METRES_PER_LEVEL + 0.5, 500), source: 'levels' };
  let h = DEFAULT_HEIGHTS[fp.cls];
  if (h === undefined) h = areaM2 > 1500 ? 10 : areaM2 > 400 ? 8 : 6;
  return { height: h, source: 'default' };
}

export interface ExtrudeOptions {
  /** How far walls extend below ground so slopes never show a gap. */
  buryDepth?: number;
  /** Footprints smaller than this (m²) are skipped. */
  minArea?: number;
}

export interface BuildingStats {
  count: number;
  skipped: number;
  withHeight: number;
  withLevels: number;
  defaulted: number;
  maxHeight: number;
}

export interface CityMeshes {
  walls: MeshData;
  roofs: MeshData;
  stats: BuildingStats;
}

/** Ensure a local ring is counter-clockwise (positive area). */
function ccw(ring: [number, number][]): [number, number][] {
  return ringArea(ring) < 0 ? ring.slice().reverse() : ring;
}

function cw(ring: [number, number][]): [number, number][] {
  return ringArea(ring) > 0 ? ring.slice().reverse() : ring;
}

/** Drop consecutive duplicate points and points closer than `eps` metres. */
function cleanRing(ring: [number, number][], eps = 0.05): [number, number][] {
  const out: [number, number][] = [];
  for (const p of ring) {
    const last = out[out.length - 1];
    if (last && Math.abs(last[0] - p[0]) < eps && Math.abs(last[1] - p[1]) < eps) continue;
    out.push(p);
  }
  if (out.length > 1) {
    const a = out[0];
    const b = out[out.length - 1];
    if (Math.abs(a[0] - b[0]) < eps && Math.abs(a[1] - b[1]) < eps) out.pop();
  }
  return out;
}

/**
 * Append one extruded footprint to the wall and roof builders.
 * `base` is the ground height at the footprint; returns false if skipped.
 */
export function extrudeFootprint(
  walls: MeshBuilder,
  roofs: MeshBuilder,
  outer: [number, number][],
  holes: [number, number][][],
  base: number,
  height: number,
  minHeight: number,
  bury: number,
): boolean {
  const o = ccw(cleanRing(outer));
  if (o.length < 3) return false;
  const hs = holes.map((h) => cw(cleanRing(h))).filter((h) => h.length >= 3);
  const top = base + height;
  const bottom = minHeight > 0 ? base + minHeight : base - bury;

  // Walls: one quad per edge, flat normals, outward facing.
  for (const ring of [o, ...hs]) {
    for (let i = 0; i < ring.length; i++) {
      const p = ring[i];
      const q = ring[(i + 1) % ring.length];
      const dx = q[0] - p[0];
      const dy = q[1] - p[1];
      const len = Math.hypot(dx, dy);
      if (len < 1e-6) continue;
      const nx = dy / len;
      const ny = -dx / len;
      const a = walls.vertexENU(p[0], p[1], bottom, nx, ny, 0);
      const b = walls.vertexENU(q[0], q[1], bottom, nx, ny, 0);
      const c = walls.vertexENU(q[0], q[1], top, nx, ny, 0);
      const d = walls.vertexENU(p[0], p[1], top, nx, ny, 0);
      walls.quad(a, b, c, d);
    }
  }

  // Roof (and a floor for floating parts): earcut over the flattened rings.
  const flat: number[] = [];
  const holeIndices: number[] = [];
  for (const p of o) flat.push(p[0], p[1]);
  for (const h of hs) {
    holeIndices.push(flat.length / 2);
    for (const p of h) flat.push(p[0], p[1]);
  }
  const tri = earcut(flat, holeIndices.length ? holeIndices : undefined, 2);
  if (tri.length === 0) return false;

  const roofBase = roofs.vertexCount;
  for (let i = 0; i < flat.length; i += 2) roofs.vertexENU(flat[i], flat[i + 1], top, 0, 0, 1);
  for (let i = 0; i < tri.length; i += 3) {
    const a = tri[i], b = tri[i + 1], c = tri[i + 2];
    // Keep triangles counter-clockwise in (east, north) so they face up.
    const ax = flat[a * 2], ay = flat[a * 2 + 1];
    const bx = flat[b * 2], by = flat[b * 2 + 1];
    const cx = flat[c * 2], cy = flat[c * 2 + 1];
    const area = (bx - ax) * (cy - ay) - (cx - ax) * (by - ay);
    if (area >= 0) roofs.triangle(roofBase + a, roofBase + b, roofBase + c);
    else roofs.triangle(roofBase + a, roofBase + c, roofBase + b);
  }

  if (minHeight > 0) {
    const floorBase = roofs.vertexCount;
    for (let i = 0; i < flat.length; i += 2) roofs.vertexENU(flat[i], flat[i + 1], bottom, 0, 0, -1);
    for (let i = 0; i < tri.length; i += 3) {
      const a = tri[i], b = tri[i + 1], c = tri[i + 2];
      const ax = flat[a * 2], ay = flat[a * 2 + 1];
      const bx = flat[b * 2], by = flat[b * 2 + 1];
      const cx = flat[c * 2], cy = flat[c * 2 + 1];
      const area = (bx - ax) * (cy - ay) - (cx - ax) * (by - ay);
      if (area >= 0) roofs.triangle(floorBase + a, floorBase + c, floorBase + b);
      else roofs.triangle(floorBase + a, floorBase + b, floorBase + c);
    }
  }
  return true;
}

/** Centroid of a local ring (area-weighted). */
export function ringCentroid(ring: [number, number][]): [number, number] {
  let cx = 0, cy = 0, a = 0;
  for (let i = 0, j = ring.length - 1; i < ring.length; j = i++) {
    const f = ring[j][0] * ring[i][1] - ring[i][0] * ring[j][1];
    cx += (ring[j][0] + ring[i][0]) * f;
    cy += (ring[j][1] + ring[i][1]) * f;
    a += f;
  }
  if (Math.abs(a) < 1e-9) return ring[0];
  a *= 3;
  return [cx / a, cy / a];
}

/** Build merged wall and roof meshes for all footprints inside `radius` metres of the frame origin. */
export function buildCity(
  footprints: Footprint[],
  frame: LocalFrame,
  terrain: HeightGrid | null,
  radius: number,
  opts: ExtrudeOptions = {},
): CityMeshes {
  const bury = opts.buryDepth ?? 2;
  const minArea = opts.minArea ?? 4;
  const walls = new MeshBuilder();
  const roofs = new MeshBuilder();
  const stats: BuildingStats = { count: 0, skipped: 0, withHeight: 0, withLevels: 0, defaulted: 0, maxHeight: 0 };

  for (const fp of footprints) {
    const outer = fp.outer.map(([lon, lat]) => frame.toLocal(lon, lat));
    const area = Math.abs(ringArea(outer));
    if (area < minArea) {
      stats.skipped++;
      continue;
    }
    const c = ringCentroid(outer);
    if (Math.hypot(c[0], c[1]) > radius) {
      stats.skipped++;
      continue;
    }
    const holes = fp.holes.map((h) => h.map(([lon, lat]) => frame.toLocal(lon, lat)));
    const res = resolveHeight(fp, area);
    const minH = fp.minHeight < res.height ? fp.minHeight : 0;
    const base = terrain ? sampleHeight(terrain, c[0], c[1]) : 0;
    if (!extrudeFootprint(walls, roofs, outer, holes, base, res.height, minH, bury)) {
      stats.skipped++;
      continue;
    }
    stats.count++;
    if (res.source === 'height') stats.withHeight++;
    else if (res.source === 'levels') stats.withLevels++;
    else stats.defaulted++;
    if (res.height > stats.maxHeight) stats.maxHeight = res.height;
  }

  return { walls: walls.build(), roofs: roofs.build(), stats };
}
