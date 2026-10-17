declare module 'earcut' {
  /** Triangulate a flat array of vertex coordinates; returns triangle vertex indices. */
  export default function earcut(vertices: ArrayLike<number>, holeIndices?: ArrayLike<number> | null, dimensions?: number): number[];
  export function deviation(vertices: ArrayLike<number>, holeIndices: ArrayLike<number> | null | undefined, dimensions: number, triangles: ArrayLike<number>): number;
  export function flatten(data: ArrayLike<ArrayLike<ArrayLike<number>>>): { vertices: number[]; holes: number[]; dimensions: number };
}
