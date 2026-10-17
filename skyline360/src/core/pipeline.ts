/**
 * Area data + terrain → the full set of meshes. Pure; no DOM, no three.js.
 */
import type { AreaData, HeightGrid, MeshData } from './types';
import { LocalFrame } from './geo';
import { buildCity, type BuildingStats } from './extrude';
import { buildLandCover, type LandCover } from './landcover';
import { downsampleGrid, sampleHeight } from './terrarium';
import { edgeMinHeight, farRing, flatGrid, horizonGrid, terrainMesh } from './terrain';
import { placeTrees, type TreeInstances, type TreeOptions } from './trees';

export interface SceneMeshes {
  walls: MeshData;
  roofs: MeshData;
  terrain: MeshData;
  ring: MeshData;
  /** Ground classes painted over the terrain. */
  landCover: LandCover;
  /** Coarse terrain out to the horizon radius, when fetched. */
  horizon: MeshData | null;
  trees: TreeInstances;
  stats: BuildingStats & { terrainVertices: number; triangles: number; trees: number };
  /** Ground height (m above sea level) at the frame origin. */
  groundAtOrigin: number;
  /** Height of the far ring. */
  ringHeight: number;
}

export interface PipelineOptions {
  radius: number;
  /** Cap on terrain grid vertices per side; larger grids are box-filtered. */
  maxTerrainSide?: number;
  farRadius?: number;
  /** Cap on the land-cover raster side, texels. */
  maxCoverSide?: number;
  /** Coarse height grid covering the horizon radius (distant hills and mountains). */
  horizon?: HeightGrid | null;
  /** Cap on the horizon grid side; 512 keeps it near 500k triangles. */
  maxHorizonSide?: number;
  trees?: TreeOptions | false;
}

export function buildSceneMeshes(area: AreaData, terrain: HeightGrid | null, frame: LocalFrame, opts: PipelineOptions): SceneMeshes {
  const maxSide = opts.maxTerrainSide ?? 1024;
  const farRadius = opts.farRadius ?? 60000;
  let grid = terrain ?? flatGrid(opts.radius + 100, 0);
  const factor = Math.ceil(Math.max(grid.nx, grid.ny) / maxSide);
  if (factor > 1) grid = downsampleGrid(grid, factor);

  const city = buildCity(area.buildings, frame, grid, opts.radius);
  const landCover = buildLandCover(area.ground, area.roads, frame, grid, opts.maxCoverSide ?? 2048);
  const terrainMd = terrainMesh(grid, { skirtDepth: 80 });
  const groundAtOrigin = sampleHeight(grid, 0, 0);

  // Distant terrain: a coarse grid pushed below the detailed one where they overlap.
  let horizon: MeshData | null = null;
  let ringGrid = grid;
  if (opts.horizon) {
    let coarse = opts.horizon;
    const f = Math.ceil(Math.max(coarse.nx, coarse.ny) / (opts.maxHorizonSide ?? 512));
    if (f > 1) coarse = downsampleGrid(coarse, f);
    const hg = horizonGrid(coarse, grid);
    horizon = terrainMesh(hg, { skirtDepth: 0 });
    ringGrid = hg;
  }
  const ringHeight = edgeMinHeight(ringGrid) - 1;
  // The ring starts inside the outermost grid's half-width so corners never show a gap.
  const inner = Math.min(Math.abs(ringGrid.x0), Math.abs(ringGrid.y0)) * 0.98;
  const ring = farRing(inner, Math.max(farRadius, inner * 1.5), ringHeight);

  const trees = opts.trees === false
    ? { count: 0, positions: new Float32Array(0), heights: new Float32Array(0), rotations: new Float32Array(0) }
    : placeTrees(landCover, grid, { radius: opts.radius + 100, maxCount: 40000, ...(opts.trees ?? {}) });

  const triangles = [city.walls, city.roofs, terrainMd, ring, ...(horizon ? [horizon] : [])].reduce((s, m) => s + m.triangleCount, 0);
  return {
    walls: city.walls,
    roofs: city.roofs,
    terrain: terrainMd,
    ring,
    landCover,
    horizon,
    trees,
    stats: { ...city.stats, terrainVertices: terrainMd.vertexCount, triangles, trees: trees.count },
    groundAtOrigin,
    ringHeight,
  };
}
