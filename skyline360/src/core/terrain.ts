/**
 * Terrain mesh from a height grid, plus the flat far ring that closes the
 * horizon beyond the fetched area.
 */
import type { HeightGrid, MeshData } from './types';
import { MeshBuilder } from './mesh';

export interface TerrainMeshOptions {
  /** Depth of the skirt dropped from the grid edge, metres. */
  skirtDepth?: number;
}

/**
 * Build the terrain surface over the whole (rectangular) grid. Vertices sit
 * on grid cell centres; normals come from central differences so lighting
 * follows the slope. A vertical skirt hangs from the four edges so the far
 * ring below never shows through a gap.
 */
export function terrainMesh(grid: HeightGrid, opts: TerrainMeshOptions = {}): MeshData {
  const mb = new MeshBuilder();
  const { nx, ny, dx, dy, data } = grid;
  const skirt = opts.skirtDepth ?? 60;

  for (let j = 0; j < ny; j++) {
    const n = grid.y0 + j * dy;
    for (let i = 0; i < nx; i++) {
      const e = grid.x0 + i * dx;
      const hl = data[j * nx + Math.max(i - 1, 0)];
      const hr = data[j * nx + Math.min(i + 1, nx - 1)];
      const hd = data[Math.max(j - 1, 0) * nx + i];
      const hu = data[Math.min(j + 1, ny - 1) * nx + i];
      const sx = i === 0 || i === nx - 1 ? dx : 2 * dx;
      const sy = j === 0 || j === ny - 1 ? dy : 2 * dy;
      let nE = -(hr - hl) / sx;
      let nN = -(hu - hd) / sy;
      const len = Math.hypot(nE, nN, 1);
      nE /= len;
      nN /= len;
      mb.vertexENU(e, n, data[j * nx + i], nE, nN, 1 / len);
    }
  }

  for (let j = 0; j < ny - 1; j++) {
    for (let i = 0; i < nx - 1; i++) {
      const a = j * nx + i;
      const b = a + 1;
      const c = a + nx + 1;
      const d = a + nx;
      // a (SW) → b (SE) → c (NE) is counter-clockwise seen from above.
      mb.triangle(a, b, c);
      mb.triangle(a, c, d);
    }
  }

  if (skirt > 0) {
    const addSkirt = (idxA: number, idxB: number, eA: number, nA: number, eB: number, nB: number, outE: number, outN: number): void => {
      const a2 = mb.vertexENU(eA, nA, data[idxA] - skirt, outE, outN, 0);
      const b2 = mb.vertexENU(eB, nB, data[idxB] - skirt, outE, outN, 0);
      // Two-sided so the drop is visible from inside the area and from outside.
      mb.quad(idxA, a2, b2, idxB);
      mb.quad(idxB, b2, a2, idxA);
    };
    const e0 = grid.x0, e1 = grid.x0 + (nx - 1) * dx;
    const n0 = grid.y0, n1 = grid.y0 + (ny - 1) * dy;
    for (let i = 0; i < nx - 1; i++) {
      addSkirt(i, i + 1, e0 + i * dx, n0, e0 + (i + 1) * dx, n0, 0, -1); // south edge
      const t = (ny - 1) * nx + i;
      addSkirt(t, t + 1, e0 + i * dx, n1, e0 + (i + 1) * dx, n1, 0, 1); // north edge
    }
    for (let j = 0; j < ny - 1; j++) {
      addSkirt(j * nx, (j + 1) * nx, e0, n0 + j * dy, e0, n0 + (j + 1) * dy, -1, 0); // west edge
      const r = j * nx + nx - 1;
      addSkirt(r, r + nx, e1, n0 + j * dy, e1, n0 + (j + 1) * dy, 1, 0); // east edge
    }
  }

  const mesh = mb.build();
  // Planar uv over the grid extent (cell edges), matching the land-cover raster.
  const ox = grid.x0 - dx / 2, oy = grid.y0 - dy / 2;
  const ex = nx * dx, ey = ny * dy;
  const uvs = new Float32Array(mesh.vertexCount * 2);
  for (let i = 0; i < mesh.vertexCount; i++) {
    uvs[i * 2] = (mesh.positions[i * 3] - ox) / ex;
    uvs[i * 2 + 1] = (-mesh.positions[i * 3 + 2] - oy) / ey;
  }
  mesh.uvs = uvs;
  return mesh;
}

/**
 * Coarse surroundings for distant hills and mountains: a copy of the coarse
 * grid with every cell under the detailed grid pushed below it, so the two
 * never fight and the detailed skirt hides the seam.
 */
export function horizonGrid(coarse: HeightGrid, fine: HeightGrid): HeightGrid {
  let fineMin = Infinity;
  for (let i = 0; i < fine.data.length; i++) if (fine.data[i] < fineMin) fineMin = fine.data[i];
  const clampTo = fineMin - 30;
  const fx0 = fine.x0 - fine.dx, fx1 = fine.x0 + fine.nx * fine.dx;
  const fy0 = fine.y0 - fine.dy, fy1 = fine.y0 + fine.ny * fine.dy;
  const data = new Float32Array(coarse.data.length);
  for (let j = 0; j < coarse.ny; j++) {
    const n = coarse.y0 + j * coarse.dy;
    for (let i = 0; i < coarse.nx; i++) {
      const e = coarse.x0 + i * coarse.dx;
      const h = coarse.data[j * coarse.nx + i];
      data[j * coarse.nx + i] = e > fx0 && e < fx1 && n > fy0 && n < fy1 ? Math.min(h, clampTo) : h;
    }
  }
  return { ...coarse, data };
}

/** A flat annulus at height `h` from innerRadius to outerRadius, normal up. */
export function farRing(innerRadius: number, outerRadius: number, h: number, segments = 96): MeshData {
  const mb = new MeshBuilder();
  for (let s = 0; s < segments; s++) {
    const a = (s / segments) * Math.PI * 2;
    mb.vertexENU(Math.cos(a) * innerRadius, Math.sin(a) * innerRadius, h, 0, 0, 1);
    mb.vertexENU(Math.cos(a) * outerRadius, Math.sin(a) * outerRadius, h, 0, 0, 1);
  }
  for (let s = 0; s < segments; s++) {
    const i0 = s * 2;
    const i1 = ((s + 1) % segments) * 2;
    mb.triangle(i0, i1, i1 + 1);
    mb.triangle(i0, i1 + 1, i0 + 1);
  }
  return mb.build();
}

/** A flat height grid at `h` metres covering ±half metres, used when no terrain source answered. */
export function flatGrid(half: number, h: number, cells = 64): HeightGrid {
  const cell = (2 * half) / cells;
  const n = cells + 1;
  return { x0: -half, y0: -half, dx: cell, dy: cell, nx: n, ny: n, data: new Float32Array(n * n).fill(h) };
}

/** Lowest height along the four grid edges; the far ring sits just below it. */
export function edgeMinHeight(grid: HeightGrid): number {
  let m = Infinity;
  const { nx, ny, data } = grid;
  for (let i = 0; i < nx; i++) {
    m = Math.min(m, data[i], data[(ny - 1) * nx + i]);
  }
  for (let j = 0; j < ny; j++) {
    m = Math.min(m, data[j * nx], data[j * nx + nx - 1]);
  }
  return m;
}
