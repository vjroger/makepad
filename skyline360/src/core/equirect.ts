/**
 * Equirectangular mapping shared by the HDRI pass and the tests.
 *
 * Image convention: the centre column is `heading` (compass degrees); columns
 * run clockwise (east is right of north); row 0 is the zenith, the middle row
 * is the horizon. Directions are in the three.js frame (x east, y up, z south).
 */
import { DEG } from './sky';

export type Vec3 = [number, number, number];

/** Direction for normalised image coordinates u (0..1 left→right), v (0..1 top→bottom). */
export function dirFromUV(u: number, v: number, headingDeg: number): Vec3 {
  const az = (u - 0.5) * 2 * Math.PI + headingDeg * DEG;
  const lat = (0.5 - v) * Math.PI;
  const c = Math.cos(lat);
  return [c * Math.sin(az), Math.sin(lat), -c * Math.cos(az)];
}

/** Inverse of dirFromUV. */
export function uvFromDir(d: Vec3, headingDeg: number): [number, number] {
  const az = Math.atan2(d[0], -d[2]) - headingDeg * DEG;
  let u = az / (2 * Math.PI) + 0.5;
  u -= Math.floor(u);
  const v = 0.5 - Math.asin(Math.max(-1, Math.min(1, d[1]))) / Math.PI;
  return [u, v];
}

/** GLSL twin of dirFromUV; `uHeading` in radians. */
export const EQUIRECT_GLSL = /* glsl */ `
uniform float uHeading;
vec3 dirFromUV(vec2 uv) {
  float az = (uv.x - 0.5) * 6.28318530718 + uHeading;
  float lat = (0.5 - uv.y) * 3.14159265359;
  float c = cos(lat);
  return vec3(c * sin(az), sin(lat), -c * cos(az));
}
`;
