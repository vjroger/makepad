// CPU reference march: one sweep per azimuth column over the tiers.
//
// Shares every rule with the GPU per-pixel march (src/gpu/shaders.js): the step sequence, the
// curvature-corrected ray height, the tier selection, bilinear sampling and the 6-step bisection.
// Output rows are top-down: row 0 is +90 deg (zenith), the last row -90 deg (nadir).

import { EFFECTIVE_RADIUS } from './geo.js';
import { sampleTier, sampleTierClass, tierNormal, tierIndexFor } from './tiers.js';

export const BISECT_STEPS = 6;
export const START_DISTANCE = 1.0; // m

/** Step length at distance d in a tier of the given texel size, for a plate with pixel angle delta. */
export function stepSize(d, texel, delta) {
  const a = 0.5 * texel, b = 0.5 * d * delta;
  return a > b ? a : b;
}

/** Allocate the output layers for a plate of width W (height = W / 2). */
export function allocatePlate(width, bandCount) {
  const height = width / 2, n = width * height;
  return {
    width, height,
    coverage: new Uint8Array(n),
    depth: new Float32Array(n).fill(Infinity),
    classId: new Uint8Array(n),
    normal: new Float32Array(n * 3),
    altitude: new Float32Array(n),
    bands: new Float32Array(bandCount * width).fill(-Math.PI / 2),
    horizon: new Float32Array(width).fill(-Math.PI / 2),          // final max elevation per column (rad)
    horizonDistance: new Float32Array(width),                      // ground distance where it was reached (m)
  };
}

/**
 * Sweep columns [x0, x1) of the plate.
 *
 * params: { eyeZ, zGround, verticalExaggeration, headingOffset (rad), maxDistance, bands: [m, ...] }
 */
export function sweepColumns(tiers, params, plate, x0 = 0, x1 = plate.width, onProgress) {
  const W = plate.width, H = plate.height;
  const delta = 2 * Math.PI / W;
  const { eyeZ, maxDistance } = params;
  const V = params.verticalExaggeration ?? 1, zG = params.zGround ?? 0;
  const heading = params.headingOffset ?? 0;
  const bandRadii = params.bands || [];
  const nb = bandRadii.length;
  const invR2 = 1 / (2 * EFFECTIVE_RADIUS);
  const nrm = [0, 0, 1];

  // height model at (east, north): tier sample with vertical exaggeration about the eye's ground
  const heightAt = (k, e, n) => {
    const h = sampleTier(tiers[k], e, n);
    return V === 1 ? h : zG + (h - zG) * V;
  };

  for (let x = x0; x < x1; x++) {
    const theta = ((x + 0.5) / W) * 2 * Math.PI - Math.PI + heading;
    const sinT = Math.sin(theta), cosT = Math.cos(theta);
    let eMax = -Math.PI / 2, dAtMax = 0;
    let yPrevTop = H;            // rows [yTop, yPrevTop) get filled by a rising sample
    let d = START_DISTANCE, dPrev = 0;
    let bandIdx = 0;
    let steps = 0;
    while (true) {
      const east = d * sinT, north = d * cosT;
      const k = tierIndexFor(tiers, east, north);
      if (k < 0) break;
      const h = heightAt(k, east, north);
      const e = Math.atan2(h - eyeZ - d * d * invR2, d);
      if (e > eMax) {
        let yTop = Math.ceil(H * (0.5 - e / Math.PI) - 0.5);
        if (yTop < 0) yTop = 0;
        for (let y = yTop; y < yPrevTop; y++) {
          const eRow = (0.5 - (y + 0.5) / H) * Math.PI;
          const tanE = Math.tan(eRow);
          // bisection on f(s) = h(s) - rayHeight(s) between dPrev (miss) and d (hit)
          let lo = dPrev, hi = d;
          for (let it = 0; it < BISECT_STEPS; it++) {
            const mid = 0.5 * (lo + hi);
            if (mid <= 0) { lo = mid; continue; }
            const me = mid * sinT, mn = mid * cosT;
            const mk = tierIndexFor(tiers, me, mn);
            const hm = mk < 0 ? -1e9 : heightAt(mk, me, mn);
            const ray = eyeZ + mid * tanE + mid * mid * invR2;
            if (hm >= ray) hi = mid; else lo = mid;
          }
          const dh = hi;
          const he = dh * sinT, hn = dh * cosT;
          const hk = tierIndexFor(tiers, he, hn);
          const kk = hk < 0 ? k : hk;
          const hHit = heightAt(kk, he, hn);
          const idx = y * W + x;
          plate.coverage[idx] = 255;
          plate.depth[idx] = Math.hypot(dh, hHit - eyeZ);
          plate.classId[idx] = sampleTierClass(tiers[kk], he, hn) ? 2 : 1;
          plate.altitude[idx] = hHit;
          tierNormal(tiers[kk], he, hn, nrm);
          if (V !== 1) { // exaggeration scales the slope
            const len = Math.hypot(nrm[0] * V, nrm[1] * V, nrm[2]);
            nrm[0] = nrm[0] * V / len; nrm[1] = nrm[1] * V / len; nrm[2] = nrm[2] / len;
          }
          plate.normal[idx * 3] = nrm[0]; plate.normal[idx * 3 + 1] = nrm[1]; plate.normal[idx * 3 + 2] = nrm[2];
        }
        yPrevTop = yTop;
        eMax = e; dAtMax = d;
      }
      // band snapshots: the horizon angle reached before crossing each band radius
      while (bandIdx < nb && d >= bandRadii[bandIdx]) { plate.bands[bandIdx * W + x] = eMax; bandIdx++; }
      if (d >= maxDistance) break;
      dPrev = d;
      d += stepSize(d, tiers[k].texel, delta);
      steps++;
    }
    while (bandIdx < nb) { plate.bands[bandIdx * W + x] = eMax; bandIdx++; }
    plate.horizon[x] = eMax; plate.horizonDistance[x] = dAtMax;
    if (onProgress && (x & 63) === 0) onProgress({ stage: 'sweep', fraction: (x - x0 + 1) / (x1 - x0) });
  }
  return plate;
}
