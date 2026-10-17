/**
 * Tree placement from the land-cover tree-density raster. Trees are simple
 * instances (position, height, rotation); the renderer draws one low-poly
 * tree per instance. "Trees yes/no, how high" is what the HDRI needs.
 */
import type { HeightGrid } from './types';
import type { LandCover } from './landcover';
import { sampleHeight } from './terrarium';
import { prng } from './demo';

export interface TreeInstances {
  count: number;
  /** x, y (ground), z per tree in the three.js frame. */
  positions: Float32Array;
  /** Total height in metres per tree. */
  heights: Float32Array;
  /** Rotation about the vertical axis, radians. */
  rotations: Float32Array;
}

export interface TreeOptions {
  /** Mean spacing in metres for dense cover (forest). */
  denseSpacing?: number;
  /** Mean spacing in metres for sparse cover (parks). */
  sparseSpacing?: number;
  /** Height range for dense cover. */
  denseHeight?: [number, number];
  /** Height range for sparse cover. */
  sparseHeight?: [number, number];
  /** Hard cap on instances; spacing grows to respect it. */
  maxCount?: number;
  /** Only place trees within this radius of the origin. */
  radius?: number;
  seed?: number;
}

export function placeTrees(cover: LandCover, terrain: HeightGrid, opts: TreeOptions = {}): TreeInstances {
  const denseSpacing = opts.denseSpacing ?? 7;
  const sparseSpacing = opts.sparseSpacing ?? 22;
  const denseH = opts.denseHeight ?? [12, 24];
  const sparseH = opts.sparseHeight ?? [7, 16];
  const maxCount = opts.maxCount ?? 80000;
  const radius = opts.radius ?? Infinity;
  const rnd = prng(opts.seed ?? 11);
  const ts = cover.texelSize;
  const texelArea = ts * ts;

  // Expected count from the density raster; scale spacing up if over the cap.
  let dense = 0, sparse = 0;
  for (let i = 0; i < cover.trees.length; i++) {
    if (cover.trees[i] === 2) dense++;
    else if (cover.trees[i] === 1) sparse++;
  }
  let expected = (dense * texelArea) / (denseSpacing * denseSpacing) + (sparse * texelArea) / (sparseSpacing * sparseSpacing);
  const scale = expected > maxCount ? Math.sqrt(expected / maxCount) : 1;
  const pDense = texelArea / (denseSpacing * scale) ** 2;
  const pSparse = texelArea / (sparseSpacing * scale) ** 2;
  expected = Math.min(expected, maxCount);

  const cap = Math.ceil(expected * 1.1) + 16;
  const positions = new Float32Array(cap * 3);
  const heights = new Float32Array(cap);
  const rotations = new Float32Array(cap);
  let n = 0;
  for (let j = 0; j < cover.height && n < cap; j++) {
    for (let i = 0; i < cover.width && n < cap; i++) {
      const d = cover.trees[j * cover.width + i];
      if (d === 0) continue;
      const p = d === 2 ? pDense : pSparse;
      // Expected trees per texel may exceed one when texels are larger than the spacing.
      const tries = Math.floor(p) + (rnd() < p - Math.floor(p) ? 1 : 0);
      for (let t = 0; t < tries && n < cap; t++) {
        const e = cover.originX + (i + rnd()) * ts;
        const north = cover.originY + (j + rnd()) * ts;
        if (Math.hypot(e, north) > radius) continue;
        const range = d === 2 ? denseH : sparseH;
        const h = range[0] + rnd() * (range[1] - range[0]);
        positions[n * 3] = e;
        positions[n * 3 + 1] = sampleHeight(terrain, e, north);
        positions[n * 3 + 2] = -north;
        heights[n] = h;
        rotations[n] = rnd() * Math.PI * 2;
        n++;
      }
    }
  }
  return { count: n, positions: positions.slice(0, n * 3), heights: heights.slice(0, n), rotations: rotations.slice(0, n) };
}
