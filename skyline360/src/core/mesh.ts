/**
 * Growable triangle-mesh builder. Positions are in the three.js frame:
 * x = east, y = up, z = south (negative north).
 */
import type { MeshData } from './types';

class GrowableFloat {
  arr = new Float32Array(1024);
  length = 0;
  push3(a: number, b: number, c: number): void {
    if (this.length + 3 > this.arr.length) this.grow();
    this.arr[this.length++] = a;
    this.arr[this.length++] = b;
    this.arr[this.length++] = c;
  }
  private grow(): void {
    const n = new Float32Array(this.arr.length * 2);
    n.set(this.arr);
    this.arr = n;
  }
  trimmed(): Float32Array {
    return this.arr.slice(0, this.length);
  }
}

class GrowableUint {
  arr = new Uint32Array(1024);
  length = 0;
  push3(a: number, b: number, c: number): void {
    if (this.length + 3 > this.arr.length) this.grow();
    this.arr[this.length++] = a;
    this.arr[this.length++] = b;
    this.arr[this.length++] = c;
  }
  private grow(): void {
    const n = new Uint32Array(this.arr.length * 2);
    n.set(this.arr);
    this.arr = n;
  }
  trimmed(): Uint32Array {
    return this.arr.slice(0, this.length);
  }
}

export class MeshBuilder {
  private pos = new GrowableFloat();
  private nrm = new GrowableFloat();
  private idx = new GrowableUint();
  private count = 0;

  get vertexCount(): number {
    return this.count;
  }

  /** Add a vertex given local (east, north, up) metres; returns its index. */
  vertexENU(e: number, n: number, u: number, nx: number, ny: number, nz: number): number {
    this.pos.push3(e, u, -n);
    this.nrm.push3(nx, nz, -ny); // normal given as (east, north, up) too
    return this.count++;
  }

  /** Add a vertex already in the three.js frame. */
  vertexXYZ(x: number, y: number, z: number, nx: number, ny: number, nz: number): number {
    this.pos.push3(x, y, z);
    this.nrm.push3(nx, ny, nz);
    return this.count++;
  }

  triangle(a: number, b: number, c: number): void {
    this.idx.push3(a, b, c);
  }

  quad(a: number, b: number, c: number, d: number): void {
    this.idx.push3(a, b, c);
    this.idx.push3(a, c, d);
  }

  build(): MeshData {
    const indices = this.idx.trimmed();
    return {
      positions: this.pos.trimmed(),
      normals: this.nrm.trimmed(),
      indices,
      vertexCount: this.count,
      triangleCount: indices.length / 3,
    };
  }
}

/** Axis-aligned bounds of a mesh in the three.js frame. */
export function meshBounds(mesh: MeshData): { min: [number, number, number]; max: [number, number, number] } {
  const min: [number, number, number] = [Infinity, Infinity, Infinity];
  const max: [number, number, number] = [-Infinity, -Infinity, -Infinity];
  const p = mesh.positions;
  for (let i = 0; i < p.length; i += 3) {
    for (let k = 0; k < 3; k++) {
      const v = p[i + k];
      if (v < min[k]) min[k] = v;
      if (v > max[k]) max[k] = v;
    }
  }
  return { min, max };
}
