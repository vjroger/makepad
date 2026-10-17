// Minimal OpenEXR writer: scanline, NO_COMPRESSION, FLOAT or HALF channels.
// Readable by Blender, Nuke, Photoshop, Affinity, three.js EXRLoader, OpenImageIO.

const FLOAT = 2, HALF = 1;

function floatToHalf(f) {
  // round-to-nearest-even IEEE half
  const fb = new Float32Array([f]), ib = new Uint32Array(fb.buffer);
  const x = ib[0];
  const sign = (x >>> 16) & 0x8000;
  let exp = (x >>> 23) & 0xff;
  let mant = x & 0x7fffff;
  if (exp === 0xff) return sign | 0x7c00 | (mant ? 0x200 : 0); // inf / nan
  let e = exp - 127 + 15;
  if (e >= 0x1f) return sign | 0x7c00;                           // overflow -> inf
  if (e <= 0) {
    if (e < -10) return sign;                                     // underflow -> 0
    mant |= 0x800000;
    const shift = 14 - e;
    let h = mant >> shift;
    const rem = mant & ((1 << shift) - 1), halfway = 1 << (shift - 1);
    if (rem > halfway || (rem === halfway && (h & 1))) h++;
    return sign | h;
  }
  let h = sign | (e << 10) | (mant >> 13);
  const rem = mant & 0x1fff;
  if (rem > 0x1000 || (rem === 0x1000 && (h & 1))) h++;
  return h;
}

function cstr(s) { return [...new TextEncoder().encode(s), 0]; }

/**
 * channels: [{ name, data: Float32Array(width*height) (top-down), half?: boolean }]
 * Returns a Uint8Array with the EXR file.
 */
export function encodeEXR(width, height, channels) {
  const chans = [...channels].sort((a, b) => (a.name < b.name ? -1 : a.name > b.name ? 1 : 0));
  const bytes = [];
  const push = (...a) => { for (const v of a) bytes.push(v); };
  const pushI32 = (v) => { push(v & 255, (v >> 8) & 255, (v >> 16) & 255, (v >>> 24) & 255); };
  const pushF32 = (v) => { const b = new Uint8Array(new Float32Array([v]).buffer); push(b[0], b[1], b[2], b[3]); };
  const attr = (name, type, body) => { push(...cstr(name), ...cstr(type)); pushI32(body.length); push(...body); };

  push(0x76, 0x2f, 0x31, 0x01); pushI32(2);           // magic, version 2, no flags
  const chl = [];
  for (const c of chans) {
    chl.push(...cstr(c.name));
    const t = c.half ? HALF : FLOAT;
    chl.push(t & 255, (t >> 8) & 255, (t >> 16) & 255, (t >> 24) & 255);
    chl.push(0, 0, 0, 0);                                // pLinear + reserved
    chl.push(1, 0, 0, 0, 1, 0, 0, 0);                    // xSampling, ySampling
  }
  chl.push(0);
  attr('channels', 'chlist', chl);
  attr('compression', 'compression', [0]);
  const box = []; const bi = (v) => box.push(v & 255, (v >> 8) & 255, (v >> 16) & 255, (v >>> 24) & 255);
  bi(0); bi(0); bi(width - 1); bi(height - 1);
  attr('dataWindow', 'box2i', box);
  attr('displayWindow', 'box2i', box);
  attr('lineOrder', 'lineOrder', [0]);
  const f1 = Array.from(new Uint8Array(new Float32Array([1]).buffer));
  attr('pixelAspectRatio', 'float', f1);
  attr('screenWindowCenter', 'v2f', [0, 0, 0, 0, 0, 0, 0, 0]);
  attr('screenWindowWidth', 'float', f1);
  push(0);                                               // end of header

  const header = new Uint8Array(bytes);
  const bytesPerPixel = chans.reduce((s, c) => s + (c.half ? 2 : 4), 0);
  const lineBytes = width * bytesPerPixel;
  const tableBytes = height * 8;
  const total = header.length + tableBytes + height * (8 + lineBytes);
  const out = new Uint8Array(total);
  out.set(header, 0);
  const dv = new DataView(out.buffer);
  let off = header.length;
  const firstLine = header.length + tableBytes;
  for (let y = 0; y < height; y++) {
    const o = firstLine + y * (8 + lineBytes);
    dv.setUint32(off, o >>> 0, true); dv.setUint32(off + 4, 0, true);   // uint64 LE offset
    off += 8;
  }
  for (let y = 0; y < height; y++) {
    let o = firstLine + y * (8 + lineBytes);
    dv.setInt32(o, y, true); dv.setInt32(o + 4, lineBytes, true);
    o += 8;
    for (const c of chans) {
      const row = y * width;
      if (c.half) {
        for (let x = 0; x < width; x++, o += 2) dv.setUint16(o, floatToHalf(c.data[row + x]), true);
      } else {
        for (let x = 0; x < width; x++, o += 4) dv.setFloat32(o, c.data[row + x], true);
      }
    }
  }
  return out;
}

/**
 * Pack a plate into EXR channels.
 *   mode 'full':    A (half), Z (float), N.x N.y N.z (half), height (half), id (half)
 *   mode 'compact': A (half), Z (float)
 * Sky pixels get Z = skyDepth (default 10 x maxDistance).
 */
export function plateToEXR(plate, { mode = 'full', skyDepth } = {}) {
  const { width, height } = plate, n = width * height;
  const A = new Float32Array(n), Z = new Float32Array(n);
  const sky = skyDepth ?? 1e9;
  for (let i = 0; i < n; i++) { A[i] = plate.coverage[i] ? 1 : 0; Z[i] = plate.coverage[i] ? plate.depth[i] : sky; }
  const channels = [{ name: 'A', data: A, half: true }, { name: 'Z', data: Z }];
  if (mode === 'full') {
    const nx = new Float32Array(n), ny = new Float32Array(n), nz = new Float32Array(n), id = new Float32Array(n), h = new Float32Array(n);
    for (let i = 0; i < n; i++) {
      nx[i] = plate.normal[i * 3]; ny[i] = plate.normal[i * 3 + 1]; nz[i] = plate.normal[i * 3 + 2];
      id[i] = plate.classId[i]; h[i] = plate.coverage[i] ? plate.altitude[i] : 0;
    }
    channels.push({ name: 'N.x', data: nx, half: true }, { name: 'N.y', data: ny, half: true }, { name: 'N.z', data: nz, half: true },
      { name: 'height', data: h, half: true }, { name: 'id', data: id, half: true });
  }
  return encodeEXR(width, height, channels);
}
