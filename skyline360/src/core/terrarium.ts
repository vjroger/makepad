/**
 * Terrarium-encoded elevation tiles (Mapterhorn, AWS Terrain Tiles) and the
 * height grid built from them.
 *
 * Terrarium: height = (R * 256 + G + B / 256) - 32768 metres.
 */
import type { HeightGrid } from './types';
import { LocalFrame, metresPerPixel, tileToLonLat, type TileRange } from './geo';

/** Decode one tile's RGBA pixels into metres. */
export function decodeTerrarium(rgba: Uint8ClampedArray | Uint8Array, width: number, height: number): Float32Array {
  const out = new Float32Array(width * height);
  for (let i = 0, p = 0; i < out.length; i++, p += 4) {
    out[i] = rgba[p] * 256 + rgba[p + 1] + rgba[p + 2] / 256 - 32768;
  }
  return out;
}

export interface DecodedTile {
  x: number;
  y: number;
  z: number;
  size: number; // pixels per side
  heights: Float32Array; // row 0 = north edge of the tile (image order)
}

/**
 * Stitch tiles into one HeightGrid in the local frame. Tiles are in image
 * order (row 0 = north); the grid is south-up (row 0 = south) so that
 * increasing row index means increasing north, matching the mesh builder.
 *
 * Missing tiles are filled with `fill` metres (0 = sea level).
 */
export function gridFromTiles(tiles: DecodedTile[], range: TileRange, frame: LocalFrame, fill = 0): HeightGrid {
  const size = tiles[0]?.size ?? 256;
  const cols = range.xMax - range.xMin + 1;
  const rows = range.yMax - range.yMin + 1;
  const nx = cols * size;
  const ny = rows * size;
  const data = new Float32Array(nx * ny).fill(fill);

  for (const t of tiles) {
    const cx = t.x - range.xMin;
    const cy = t.y - range.yMin;
    for (let r = 0; r < size; r++) {
      // image row r (north to south) → grid row counted from the south
      const gridRow = ny - 1 - (cy * size + r);
      const src = r * size;
      const dst = gridRow * nx + cx * size;
      data.set(t.heights.subarray(src, src + size), dst);
    }
  }

  // Pixel centres: the north-west corner of the tile range is at pixel (0,0)'s corner.
  const nw = tileToLonLat(range.xMin, range.yMin, range.z);
  const se = tileToLonLat(range.xMax + 1, range.yMax + 1, range.z);
  const [xw, yn] = frame.toLocal(nw.lon, nw.lat);
  const [xe, ys] = frame.toLocal(se.lon, se.lat);
  const dx = (xe - xw) / nx;
  const dy = (yn - ys) / ny;
  return { x0: xw + dx / 2, y0: ys + dy / 2, dx, dy, nx, ny, data };
}

/** Bilinear height lookup in local metres; clamps to the grid edge. */
export function sampleHeight(grid: HeightGrid, x: number, y: number): number {
  const fx = (x - grid.x0) / grid.dx;
  const fy = (y - grid.y0) / grid.dy;
  const cx = Math.min(Math.max(fx, 0), grid.nx - 1);
  const cy = Math.min(Math.max(fy, 0), grid.ny - 1);
  const x0 = Math.floor(cx);
  const y0 = Math.floor(cy);
  const x1 = Math.min(x0 + 1, grid.nx - 1);
  const y1 = Math.min(y0 + 1, grid.ny - 1);
  const tx = cx - x0;
  const ty = cy - y0;
  const d = grid.data;
  const h00 = d[y0 * grid.nx + x0];
  const h10 = d[y0 * grid.nx + x1];
  const h01 = d[y1 * grid.nx + x0];
  const h11 = d[y1 * grid.nx + x1];
  return (h00 * (1 - tx) + h10 * tx) * (1 - ty) + (h01 * (1 - tx) + h11 * tx) * ty;
}

/** Mean height of the grid, metres. */
export function meanHeight(grid: HeightGrid): number {
  let s = 0;
  for (let i = 0; i < grid.data.length; i++) s += grid.data[i];
  return s / grid.data.length;
}

/** Nominal metres per terrain pixel for a tile set at this latitude. */
export function terrainResolution(lat: number, z: number, tilePx: number): number {
  return metresPerPixel(lat, z, tilePx);
}

/**
 * Downsample a grid by an integer factor (box filter). Used to cap the
 * terrain triangle count when a large radius meets a fine tile set.
 */
export function downsampleGrid(grid: HeightGrid, factor: number): HeightGrid {
  if (factor <= 1) return grid;
  const nx = Math.floor(grid.nx / factor);
  const ny = Math.floor(grid.ny / factor);
  const data = new Float32Array(nx * ny);
  for (let y = 0; y < ny; y++) {
    for (let x = 0; x < nx; x++) {
      let s = 0;
      for (let j = 0; j < factor; j++) {
        const row = (y * factor + j) * grid.nx + x * factor;
        for (let i = 0; i < factor; i++) s += grid.data[row + i];
      }
      data[y * nx + x] = s / (factor * factor);
    }
  }
  return {
    x0: grid.x0 + (grid.dx * (factor - 1)) / 2,
    y0: grid.y0 + (grid.dy * (factor - 1)) / 2,
    dx: grid.dx * factor,
    dy: grid.dy * factor,
    nx,
    ny,
    data,
  };
}
