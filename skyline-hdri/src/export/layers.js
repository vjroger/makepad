// Raster layers derived from a plate: preview RGBA, 16-bit depth, normals, SDF.

const INF = 1e20;

/** 1-D squared Euclidean distance transform (Felzenszwalb & Huttenlocher). f: INF for non-features. */
function edt1d(f, n, d, v, z) {
  let k = 0;
  v[0] = 0; z[0] = -INF; z[1] = INF;
  for (let q = 1; q < n; q++) {
    let s = ((f[q] + q * q) - (f[v[k]] + v[k] * v[k])) / (2 * q - 2 * v[k]);
    while (s <= z[k]) {
      k--;
      s = ((f[q] + q * q) - (f[v[k]] + v[k] * v[k])) / (2 * q - 2 * v[k]);
    }
    k++;
    v[k] = q; z[k] = s; z[k + 1] = INF;
  }
  k = 0;
  for (let q = 0; q < n; q++) {
    while (z[k + 1] < q) k++;
    const dq = q - v[k];
    d[q] = dq * dq + f[v[k]];
  }
}

/** Squared distance from every pixel to the nearest pixel where mask(i) is true. */
export function distanceTransform(width, height, isFeature) {
  const n = width * height;
  const g = new Float32Array(n);
  const m = Math.max(width, height);
  const f = new Float32Array(m), d = new Float32Array(m), v = new Int32Array(m), z = new Float32Array(m + 1);
  for (let x = 0; x < width; x++) {
    for (let y = 0; y < height; y++) f[y] = isFeature(y * width + x) ? 0 : INF;
    edt1d(f, height, d, v, z);
    for (let y = 0; y < height; y++) g[y * width + x] = d[y];
  }
  const out = new Float32Array(n);
  for (let y = 0; y < height; y++) {
    for (let x = 0; x < width; x++) f[x] = g[y * width + x];
    edt1d(f, width, d, v, z);
    for (let x = 0; x < width; x++) out[y * width + x] = d[x];
  }
  return out;
}

/**
 * Signed distance to the silhouette in pixels: positive in the sky, negative inside occluders.
 * Returns Float32Array (pixels). Columns wrap horizontally only approximately (no wrap handling).
 */
export function signedDistanceField(plate) {
  const { width, height, coverage } = plate;
  const toOccluder = distanceTransform(width, height, (i) => coverage[i] > 0);
  const toSky = distanceTransform(width, height, (i) => coverage[i] === 0);
  const sdf = new Float32Array(width * height);
  for (let i = 0; i < sdf.length; i++) sdf[i] = coverage[i] > 0 ? -Math.sqrt(toSky[i]) : Math.sqrt(toOccluder[i]);
  return sdf;
}

/** 8-bit SDF: 128 on the edge, +spread px -> 255, -spread px -> 0. */
export function sdfToGray8(sdf, spread = 64) {
  const out = new Uint8Array(sdf.length);
  for (let i = 0; i < sdf.length; i++) {
    const v = 128 + (sdf[i] / spread) * 127;
    out[i] = v < 0 ? 0 : v > 255 ? 255 : Math.round(v);
  }
  return out;
}

/** 16-bit log depth: 65535 * ln(1 + d) / ln(1 + maxDistance); sky = 65535. */
export function depthToGray16(plate, maxDistance) {
  const out = new Uint16Array(plate.width * plate.height);
  const k = 65535 / Math.log(1 + maxDistance);
  for (let i = 0; i < out.length; i++) {
    if (!plate.coverage[i]) { out[i] = 65535; continue; }
    const v = Math.log(1 + Math.max(0, plate.depth[i])) * k;
    out[i] = v > 65535 ? 65535 : Math.round(v);
  }
  return out;
}

/** Normal map RGB8: n * 0.5 + 0.5; sky = (128, 128, 255). */
export function normalToRGB8(plate) {
  const n = plate.width * plate.height;
  const out = new Uint8Array(n * 3);
  for (let i = 0; i < n; i++) {
    if (!plate.coverage[i]) { out[i * 3] = 128; out[i * 3 + 1] = 128; out[i * 3 + 2] = 255; continue; }
    for (let c = 0; c < 3; c++) {
      const v = Math.round((plate.normal[i * 3 + c] * 0.5 + 0.5) * 255);
      out[i * 3 + c] = v < 0 ? 0 : v > 255 ? 255 : v;
    }
  }
  return out;
}

const DEG = Math.PI / 180;

/**
 * Preview RGBA (sRGB, straight alpha). mode: 'shaded' | 'flat' | 'class'.
 * sun: { azimuth (deg clockwise from north), elevation (deg) }, hazeDistance in metres.
 */
export function shadePlate(plate, { mode = 'shaded', sun = { azimuth: 225, elevation: 35 }, hazeDistance = 60000 } = {}) {
  const n = plate.width * plate.height;
  const out = new Uint8Array(n * 4);
  const az = sun.azimuth * DEG, el = sun.elevation * DEG;
  const sx = Math.sin(az) * Math.cos(el), sy = Math.cos(az) * Math.cos(el), sz = Math.sin(el);
  const tint = [[0, 0, 0], [0.50, 0.49, 0.45], [0.66, 0.63, 0.60]];
  const haze = [0.72, 0.78, 0.86];
  const classColor = [[0, 0, 0], [0.35, 0.55, 0.30], [0.80, 0.45, 0.35]];
  const gamma = (v) => Math.round(Math.pow(Math.max(0, Math.min(1, v)), 1 / 2.2) * 255);
  for (let i = 0; i < n; i++) {
    const o = i * 4;
    if (!plate.coverage[i]) { out[o] = out[o + 1] = out[o + 2] = 0; out[o + 3] = 0; continue; }
    out[o + 3] = 255;
    if (mode === 'flat') { out[o] = out[o + 1] = out[o + 2] = 255; continue; }
    const c = plate.classId[i];
    if (mode === 'class') { const cc = classColor[c]; out[o] = gamma(cc[0]); out[o + 1] = gamma(cc[1]); out[o + 2] = gamma(cc[2]); continue; }
    const nx = plate.normal[i * 3], ny = plate.normal[i * 3 + 1], nz = plate.normal[i * 3 + 2];
    const diff = Math.max(0, nx * sx + ny * sy + nz * sz);
    const l = 0.28 + 0.72 * diff;
    const f = 1 - Math.exp(-plate.depth[i] / hazeDistance);
    const t = tint[c];
    out[o] = gamma(t[0] * l * (1 - f) + haze[0] * f);
    out[o + 1] = gamma(t[1] * l * (1 - f) + haze[1] * f);
    out[o + 2] = gamma(t[2] * l * (1 - f) + haze[2] * f);
  }
  return out;
}
