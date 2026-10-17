// Small PNG decoder for Node tests (8-bit grey/RGB/RGBA, filters 0-4, no interlace).
import { inflateSync } from 'node:zlib';

export function decodePNG(bytes) {
  const dv = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);
  let off = 8, width = 0, height = 0, colorType = 0, bitDepth = 0;
  const idat = [];
  while (off < bytes.length) {
    const len = dv.getUint32(off);
    const type = String.fromCharCode(bytes[off + 4], bytes[off + 5], bytes[off + 6], bytes[off + 7]);
    const data = bytes.subarray(off + 8, off + 8 + len);
    if (type === 'IHDR') {
      width = dv.getUint32(off + 8); height = dv.getUint32(off + 12);
      bitDepth = bytes[off + 16]; colorType = bytes[off + 17];
      if (bytes[off + 20] !== 0) throw new Error('interlaced PNG not supported');
    } else if (type === 'IDAT') idat.push(data);
    else if (type === 'IEND') break;
    off += 12 + len;
  }
  if (bitDepth !== 8) throw new Error('only 8-bit PNG supported in test decoder');
  const channels = { 0: 1, 2: 3, 4: 2, 6: 4 }[colorType];
  const total = idat.reduce((s, d) => s + d.length, 0);
  const zdata = new Uint8Array(total);
  let p = 0;
  for (const d of idat) { zdata.set(d, p); p += d.length; }
  const raw = inflateSync(zdata);
  const stride = width * channels;
  const out = new Uint8Array(stride * height);
  let prev = new Uint8Array(stride);
  for (let y = 0; y < height; y++) {
    const f = raw[y * (stride + 1)];
    const row = raw.subarray(y * (stride + 1) + 1, (y + 1) * (stride + 1));
    const cur = new Uint8Array(stride);
    for (let i = 0; i < stride; i++) {
      const a = i >= channels ? cur[i - channels] : 0;
      const b = prev[i];
      const c = i >= channels ? prev[i - channels] : 0;
      let v = row[i];
      if (f === 1) v += a;
      else if (f === 2) v += b;
      else if (f === 3) v += (a + b) >> 1;
      else if (f === 4) {
        const pp = a + b - c, pa = Math.abs(pp - a), pb = Math.abs(pp - b), pc = Math.abs(pp - c);
        v += pa <= pb && pa <= pc ? a : pb <= pc ? b : c;
      }
      cur[i] = v & 255;
    }
    out.set(cur, y * stride);
    prev = cur;
  }
  // expand to RGBA
  const rgba = new Uint8Array(width * height * 4);
  for (let i = 0, j = 0; i < width * height; i++, j += channels) {
    if (channels === 1) { rgba[i * 4] = rgba[i * 4 + 1] = rgba[i * 4 + 2] = out[j]; rgba[i * 4 + 3] = 255; }
    else if (channels === 2) { rgba[i * 4] = rgba[i * 4 + 1] = rgba[i * 4 + 2] = out[j]; rgba[i * 4 + 3] = out[j + 1]; }
    else if (channels === 3) { rgba[i * 4] = out[j]; rgba[i * 4 + 1] = out[j + 1]; rgba[i * 4 + 2] = out[j + 2]; rgba[i * 4 + 3] = 255; }
    else { rgba[i * 4] = out[j]; rgba[i * 4 + 1] = out[j + 1]; rgba[i * 4 + 2] = out[j + 2]; rgba[i * 4 + 3] = out[j + 3]; }
  }
  return { width, height, colorType, data: rgba };
}
