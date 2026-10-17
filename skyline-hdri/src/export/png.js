// Minimal PNG writer: 8-bit RGBA / RGB / grey and 16-bit grey, zlib via the native CompressionStream.
// Works in browsers, workers and Node 20+.

const CRC_TABLE = (() => {
  const t = new Uint32Array(256);
  for (let n = 0; n < 256; n++) {
    let c = n;
    for (let k = 0; k < 8; k++) c = c & 1 ? 0xedb88320 ^ (c >>> 1) : c >>> 1;
    t[n] = c >>> 0;
  }
  return t;
})();

function crc32(bytes) {
  let c = 0xffffffff;
  for (let i = 0; i < bytes.length; i++) c = CRC_TABLE[(c ^ bytes[i]) & 0xff] ^ (c >>> 8);
  return (c ^ 0xffffffff) >>> 0;
}

function chunk(type, data) {
  const out = new Uint8Array(12 + data.length);
  const dv = new DataView(out.buffer);
  dv.setUint32(0, data.length);
  out[4] = type.charCodeAt(0); out[5] = type.charCodeAt(1); out[6] = type.charCodeAt(2); out[7] = type.charCodeAt(3);
  out.set(data, 8);
  dv.setUint32(8 + data.length, crc32(out.subarray(4, 8 + data.length)));
  return out;
}

export async function deflate(bytes) {
  const stream = new Blob([bytes]).stream().pipeThrough(new CompressionStream('deflate'));
  return new Uint8Array(await new Response(stream).arrayBuffer());
}

/**
 * Encode raw scanlines (no filter bytes) into a PNG.
 * colorType: 0 grey, 2 RGB, 6 RGBA. bitDepth: 8 or 16 (16-bit samples big-endian in `data`).
 */
export async function encodePNG({ width, height, colorType, bitDepth, data }) {
  const channels = colorType === 6 ? 4 : colorType === 2 ? 3 : 1;
  const rowBytes = width * channels * (bitDepth / 8);
  if (data.length !== rowBytes * height) throw new Error(`PNG data length ${data.length} != ${rowBytes * height}`);
  const filtered = new Uint8Array((rowBytes + 1) * height);
  for (let y = 0; y < height; y++) {
    filtered[y * (rowBytes + 1)] = 0; // filter type None
    filtered.set(data.subarray(y * rowBytes, (y + 1) * rowBytes), y * (rowBytes + 1) + 1);
  }
  const idat = await deflate(filtered);
  const ihdr = new Uint8Array(13);
  const dv = new DataView(ihdr.buffer);
  dv.setUint32(0, width); dv.setUint32(4, height);
  ihdr[8] = bitDepth; ihdr[9] = colorType; ihdr[10] = 0; ihdr[11] = 0; ihdr[12] = 0;
  const sig = new Uint8Array([137, 80, 78, 71, 13, 10, 26, 10]);
  const parts = [sig, chunk('IHDR', ihdr), chunk('IDAT', idat), chunk('IEND', new Uint8Array(0))];
  const total = parts.reduce((s, p) => s + p.length, 0);
  const out = new Uint8Array(total);
  let off = 0;
  for (const p of parts) { out.set(p, off); off += p.length; }
  return out;
}

export function encodeRGBA8(width, height, rgba) {
  return encodePNG({ width, height, colorType: 6, bitDepth: 8, data: rgba });
}

export function encodeRGB8(width, height, rgb) {
  return encodePNG({ width, height, colorType: 2, bitDepth: 8, data: rgb });
}

export function encodeGray8(width, height, gray) {
  return encodePNG({ width, height, colorType: 0, bitDepth: 8, data: gray });
}

/** 16-bit grey from a Uint16Array (host order); written big-endian as PNG requires. */
export function encodeGray16(width, height, gray16) {
  const data = new Uint8Array(width * height * 2);
  for (let i = 0, j = 0; i < gray16.length; i++, j += 2) {
    data[j] = gray16[i] >> 8; data[j + 1] = gray16[i] & 255;
  }
  return encodePNG({ width, height, colorType: 0, bitDepth: 16, data });
}
