/**
 * Minimal OpenEXR writer: scanline image, RGB, half or float, no compression,
 * with optional string attributes for metadata. Streams row by row.
 *
 * Layout (OpenEXR 2 file format): magic, version, header attributes, null,
 * offset table (one uint64 per scanline), then chunks of
 * [int32 y][int32 size][B row][G row][R row].
 */

export type ExrPixelType = 'half' | 'float';

export interface ExrOptions {
  width: number;
  height: number;
  type: ExrPixelType;
  /** Extra string attributes (name → value). */
  attributes?: Record<string, string>;
}

const enc = new TextEncoder();

class ByteWriter {
  private chunks: Uint8Array[] = [];
  length = 0;
  bytes(b: Uint8Array): void {
    this.chunks.push(b);
    this.length += b.length;
  }
  str0(s: string): void {
    this.bytes(enc.encode(s + '\0'));
  }
  i32(v: number): void {
    const b = new Uint8Array(4);
    new DataView(b.buffer).setInt32(0, v, true);
    this.bytes(b);
  }
  u8(v: number): void {
    this.bytes(Uint8Array.of(v & 0xff));
  }
  f32(v: number): void {
    const b = new Uint8Array(4);
    new DataView(b.buffer).setFloat32(0, v, true);
    this.bytes(b);
  }
  u64(v: number): void {
    const b = new Uint8Array(8);
    new DataView(b.buffer).setBigUint64(0, BigInt(v), true);
    this.bytes(b);
  }
  concat(): Uint8Array {
    const out = new Uint8Array(this.length);
    let o = 0;
    for (const c of this.chunks) {
      out.set(c, o);
      o += c.length;
    }
    return out;
  }
}

function attribute(w: ByteWriter, name: string, type: string, value: Uint8Array): void {
  w.str0(name);
  w.str0(type);
  w.i32(value.length);
  w.bytes(value);
}

/** Header bytes plus the offset table; returns the header and the byte size of each chunk. */
export function exrHeader(o: ExrOptions): { header: Uint8Array; chunkSize: number; dataStart: number } {
  const bytesPerSample = o.type === 'half' ? 2 : 4;
  const pixelType = o.type === 'half' ? 1 : 2;
  const w = new ByteWriter();
  w.i32(20000630); // magic
  w.i32(2); // version 2, scanline, no flags

  // channels: alphabetical order is required by the format.
  const ch = new ByteWriter();
  for (const name of ['B', 'G', 'R']) {
    ch.str0(name);
    ch.i32(pixelType);
    ch.u8(0); // pLinear
    ch.u8(0);
    ch.u8(0);
    ch.u8(0);
    ch.i32(1); // xSampling
    ch.i32(1); // ySampling
  }
  ch.u8(0);
  attribute(w, 'channels', 'chlist', ch.concat());
  attribute(w, 'compression', 'compression', Uint8Array.of(0));
  const box = new ByteWriter();
  box.i32(0);
  box.i32(0);
  box.i32(o.width - 1);
  box.i32(o.height - 1);
  const boxBytes = box.concat();
  attribute(w, 'dataWindow', 'box2i', boxBytes);
  attribute(w, 'displayWindow', 'box2i', boxBytes);
  attribute(w, 'lineOrder', 'lineOrder', Uint8Array.of(0));
  const f1 = new ByteWriter();
  f1.f32(1);
  attribute(w, 'pixelAspectRatio', 'float', f1.concat());
  const v2 = new ByteWriter();
  v2.f32(0);
  v2.f32(0);
  attribute(w, 'screenWindowCenter', 'v2f', v2.concat());
  attribute(w, 'screenWindowWidth', 'float', f1.concat());
  for (const [k, v] of Object.entries(o.attributes ?? {})) {
    attribute(w, k, 'string', enc.encode(v));
  }
  w.u8(0); // end of header

  const chunkSize = 8 + o.width * 3 * bytesPerSample;
  const dataStart = w.length + o.height * 8;
  for (let y = 0; y < o.height; y++) w.u64(dataStart + y * chunkSize);
  return { header: w.concat(), chunkSize, dataStart };
}

const f32 = new Float32Array(1);
const u32 = new Uint32Array(f32.buffer);

/** IEEE float → half with round-to-nearest-even; overflow becomes infinity. */
export function floatToHalf(value: number): number {
  f32[0] = value;
  const x = u32[0];
  const sign = (x >>> 16) & 0x8000;
  let exp = (x >>> 23) & 0xff;
  let mant = x & 0x7fffff;
  if (exp === 0xff) return sign | 0x7c00 | (mant ? 0x200 : 0); // inf / nan
  const e = exp - 127 + 15;
  if (e >= 0x1f) return sign | 0x7c00; // overflow → inf
  if (e <= 0) {
    if (e < -10) return sign; // underflow → zero
    mant |= 0x800000;
    const shift = 14 - e;
    let half = mant >> shift;
    const rem = mant & ((1 << shift) - 1);
    const halfway = 1 << (shift - 1);
    if (rem > halfway || (rem === halfway && (half & 1))) half++;
    return sign | half;
  }
  let half = (e << 10) | (mant >> 13);
  const rem = mant & 0x1fff;
  if (rem > 0x1000 || (rem === 0x1000 && (half & 1))) half++;
  return sign | half;
}

/** Encode one scanline chunk from linear RGB floats (stride 3 or 4). */
export function exrScanline(y: number, src: Float32Array, srcOffset: number, width: number, stride: 3 | 4, type: ExrPixelType): Uint8Array {
  const bps = type === 'half' ? 2 : 4;
  const out = new Uint8Array(8 + width * 3 * bps);
  const dv = new DataView(out.buffer);
  dv.setInt32(0, y, true);
  dv.setInt32(4, width * 3 * bps, true);
  let o = 8;
  // Channel order B, G, R.
  for (const c of [2, 1, 0]) {
    for (let i = 0; i < width; i++) {
      const v = src[srcOffset + i * stride + c];
      if (type === 'half') {
        dv.setUint16(o, floatToHalf(v), true);
        o += 2;
      } else {
        dv.setFloat32(o, v, true);
        o += 4;
      }
    }
  }
  return out;
}

/** Convenience: encode a whole image (top row first). */
export function encodeExr(o: ExrOptions, data: Float32Array, stride: 3 | 4): Uint8Array {
  const { header, chunkSize } = exrHeader(o);
  const out = new Uint8Array(header.length + o.height * chunkSize);
  out.set(header, 0);
  for (let y = 0; y < o.height; y++) {
    out.set(exrScanline(y, data, y * o.width * stride, o.width, stride, o.type), header.length + y * chunkSize);
  }
  return out;
}
