/**
 * Radiance RGBE (.hdr) encoder and decoder, new-style run-length encoding.
 * Streams scanline by scanline so an 8k panorama never needs one buffer.
 */

export interface HdrHeaderOptions {
  width: number;
  height: number;
  /** Free-text comment lines written into the header (no newlines inside). */
  comments?: string[];
}

const encoder = new TextEncoder();

/** Header bytes; the caller appends encoded scanlines, top row first. */
export function hdrHeader(o: HdrHeaderOptions): Uint8Array {
  const lines = ['#?RADIANCE', ...(o.comments ?? []).map((c) => `# ${c.replace(/[\r\n]/g, ' ')}`), 'FORMAT=32-bit_rle_rgbe', 'EXPOSURE=1.0', '', `-Y ${o.height} +X ${o.width}`, ''];
  return encoder.encode(lines.join('\n'));
}

/** Convert one scanline of linear RGB floats (stride 3 or 4) to RGBE bytes (4 per pixel). */
export function rgbeFromFloats(src: Float32Array, srcOffset: number, width: number, stride: 3 | 4, out: Uint8Array): void {
  for (let i = 0; i < width; i++) {
    const p = srcOffset + i * stride;
    let r = src[p], g = src[p + 1], b = src[p + 2];
    if (!(r > 0)) r = 0;
    if (!(g > 0)) g = 0;
    if (!(b > 0)) b = 0;
    const m = Math.max(r, g, b);
    const o = i * 4;
    if (m < 1e-32) {
      out[o] = out[o + 1] = out[o + 2] = out[o + 3] = 0;
      continue;
    }
    const e = Math.floor(Math.log2(m)) + 1; // m = f * 2^e with f in [0.5, 1)
    const v = 256 / 2 ** e;
    out[o] = Math.min(255, Math.floor(r * v));
    out[o + 1] = Math.min(255, Math.floor(g * v));
    out[o + 2] = Math.min(255, Math.floor(b * v));
    out[o + 3] = e + 128;
  }
}

/** RLE-encode one scanline of RGBE bytes (new-style, per channel). */
export function encodeScanlineRLE(rgbe: Uint8Array, width: number): Uint8Array {
  if (width < 8 || width > 0x7fff) {
    // Flat scanline outside the RLE-able range.
    return rgbe.slice(0, width * 4);
  }
  const out: number[] = [2, 2, (width >> 8) & 0xff, width & 0xff];
  const plane = new Uint8Array(width);
  for (let ch = 0; ch < 4; ch++) {
    for (let i = 0; i < width; i++) plane[i] = rgbe[i * 4 + ch];
    let i = 0;
    while (i < width) {
      // Find the next run of at least 4 equal bytes.
      let runStart = i;
      let runLen = 0;
      while (runStart < width) {
        runLen = 1;
        while (runStart + runLen < width && runLen < 127 && plane[runStart + runLen] === plane[runStart]) runLen++;
        if (runLen >= 4) break;
        runStart += runLen;
      }
      // Emit the non-run bytes before it in chunks of up to 128.
      let j = i;
      while (j < Math.min(runStart, width)) {
        const n = Math.min(128, runStart - j);
        out.push(n);
        for (let k = 0; k < n; k++) out.push(plane[j + k]);
        j += n;
      }
      if (runStart < width && runLen >= 4) {
        out.push(128 + runLen, plane[runStart]);
        i = runStart + runLen;
      } else {
        i = width;
      }
    }
  }
  return Uint8Array.from(out);
}

/** Decode a .hdr file into linear RGB floats (stride 3), top row first. */
export function decodeHdr(bytes: Uint8Array): { width: number; height: number; data: Float32Array; comments: string[] } {
  let pos = 0;
  const readLine = (): string => {
    let s = '';
    while (pos < bytes.length && bytes[pos] !== 0x0a) s += String.fromCharCode(bytes[pos++]);
    pos++;
    return s;
  };
  const magic = readLine();
  if (!magic.startsWith('#?')) throw new Error('not a Radiance file');
  const comments: string[] = [];
  let line = readLine();
  while (line !== '') {
    if (line.startsWith('#')) comments.push(line.slice(1).trim());
    line = readLine();
  }
  const res = /^-Y (\d+) \+X (\d+)$/.exec(readLine());
  if (!res) throw new Error('unsupported resolution line');
  const height = parseInt(res[1], 10);
  const width = parseInt(res[2], 10);
  const data = new Float32Array(width * height * 3);
  const scan = new Uint8Array(width * 4);
  for (let y = 0; y < height; y++) {
    if (width >= 8 && width <= 0x7fff && bytes[pos] === 2 && bytes[pos + 1] === 2 && ((bytes[pos + 2] << 8) | bytes[pos + 3]) === width) {
      pos += 4;
      for (let ch = 0; ch < 4; ch++) {
        let i = 0;
        while (i < width) {
          let count = bytes[pos++];
          if (count > 128) {
            count -= 128;
            const v = bytes[pos++];
            for (let k = 0; k < count; k++) scan[(i + k) * 4 + ch] = v;
            i += count;
          } else {
            for (let k = 0; k < count; k++) scan[(i + k) * 4 + ch] = bytes[pos++];
            i += count;
          }
        }
      }
    } else {
      scan.set(bytes.subarray(pos, pos + width * 4));
      pos += width * 4;
    }
    for (let i = 0; i < width; i++) {
      const e = scan[i * 4 + 3];
      const o = (y * width + i) * 3;
      if (e === 0) {
        data[o] = data[o + 1] = data[o + 2] = 0;
      } else {
        const f = 2 ** (e - 136);
        data[o] = (scan[i * 4] + 0.5) * f;
        data[o + 1] = (scan[i * 4 + 1] + 0.5) * f;
        data[o + 2] = (scan[i * 4 + 2] + 0.5) * f;
      }
    }
  }
  return { width, height, data, comments };
}

/** Convenience: encode a whole float image (stride 3 or 4, top row first) into a .hdr file. */
export function encodeHdr(width: number, height: number, data: Float32Array, stride: 3 | 4, comments?: string[]): Uint8Array {
  const parts: Uint8Array[] = [hdrHeader({ width, height, comments })];
  const scan = new Uint8Array(width * 4);
  let total = parts[0].length;
  for (let y = 0; y < height; y++) {
    rgbeFromFloats(data, y * width * stride, width, stride, scan);
    const enc = encodeScanlineRLE(scan, width);
    parts.push(enc);
    total += enc.length;
  }
  const out = new Uint8Array(total);
  let off = 0;
  for (const p of parts) {
    out.set(p, off);
    off += p.length;
  }
  return out;
}
