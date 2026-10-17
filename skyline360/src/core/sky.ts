/**
 * Daylight sky model: Preetham, Shirley & Smits, "A Practical Analytic Model
 * for Daylight" (1999). Analytic, no lookup tables, so the same ~60 lines
 * run on the CPU (ambient lighting, tests) and in GLSL (sky dome, HDRI).
 *
 * Units: the model yields luminance in kcd/m². We divide by SCENE_UNIT so a
 * clear-sky zenith at midday is ≈ 1.0, which is how most published HDRIs
 * are scaled. The solar disc radiance is `sunIntensity` in the same unit.
 */
import type { SkyParams } from './types';

export const DEG = Math.PI / 180;
/** kcd/m² that maps to radiance 1.0 (clear sky zenith, sun 60° high, T = 2.5). */
export const SCENE_UNIT = 8.0;
/** Angular radius of the solar disc, radians (0.2665°). */
export const SUN_RADIUS = 0.2665 * DEG;
/** Solid angle of the solar disc, steradians. */
export const SUN_SOLID_ANGLE = 2 * Math.PI * (1 - Math.cos(SUN_RADIUS));

export type Vec3 = [number, number, number];

/** Unit vector towards the sun in the three.js frame (x east, y up, z south). */
export function sunDirection(azimuthDeg: number, elevationDeg: number): Vec3 {
  const az = azimuthDeg * DEG;
  const el = elevationDeg * DEG;
  return [Math.cos(el) * Math.sin(az), Math.sin(el), -Math.cos(el) * Math.cos(az)];
}

export interface PerezCoefficients {
  /** A..E for luminance Y, chromaticity x and chromaticity y. */
  Y: [number, number, number, number, number];
  x: [number, number, number, number, number];
  y: [number, number, number, number, number];
  /** Zenith luminance (kcd/m²) and chromaticity. */
  zenithY: number;
  zenithX: number;
  zenithYc: number;
  /** Sun zenith angle used, radians. */
  thetaS: number;
}

/** Perez distribution coefficients and zenith values for turbidity T and sun zenith angle θs. */
export function perezCoefficients(turbidity: number, sunElevationDeg: number): PerezCoefficients {
  const T = Math.min(Math.max(turbidity, 1.2), 12);
  // Keep the sun at least slightly above the horizon; the model is undefined below it.
  const el = Math.max(sunElevationDeg, 1.5) * DEG;
  const thetaS = Math.PI / 2 - el;
  const t2 = thetaS * thetaS;
  const t3 = t2 * thetaS;

  const chi = (4 / 9 - T / 120) * (Math.PI - 2 * thetaS);
  let zenithY = (4.0453 * T - 4.971) * Math.tan(chi) - 0.2155 * T + 2.4192;
  zenithY = Math.max(zenithY, 0.02);

  const zenithX =
    (0.00166 * t3 - 0.00375 * t2 + 0.00209 * thetaS) * T * T +
    (-0.02903 * t3 + 0.06377 * t2 - 0.03202 * thetaS + 0.00394) * T +
    (0.11693 * t3 - 0.21196 * t2 + 0.06052 * thetaS + 0.25886);
  const zenithYc =
    (0.00275 * t3 - 0.0061 * t2 + 0.00317 * thetaS) * T * T +
    (-0.04214 * t3 + 0.0897 * t2 - 0.04153 * thetaS + 0.00516) * T +
    (0.15346 * t3 - 0.26756 * t2 + 0.0667 * thetaS + 0.26688);

  return {
    Y: [0.1787 * T - 1.463, -0.3554 * T + 0.4275, -0.0227 * T + 5.3251, 0.1206 * T - 2.5771, -0.067 * T + 0.3703],
    x: [-0.0193 * T - 0.2592, -0.0665 * T + 0.0008, -0.0004 * T + 0.2125, -0.0641 * T - 0.8989, -0.0033 * T + 0.0452],
    y: [-0.0167 * T - 0.2608, -0.095 * T + 0.0092, -0.0079 * T + 0.2102, -0.0441 * T - 1.6537, -0.0109 * T + 0.0529],
    zenithY,
    zenithX,
    zenithYc,
    thetaS,
  };
}

function perez(c: [number, number, number, number, number], cosTheta: number, gamma: number, cosGamma: number): number {
  return (1 + c[0] * Math.exp(c[1] / Math.max(cosTheta, 0.01))) * (1 + c[2] * Math.exp(c[3] * gamma) + c[4] * cosGamma * cosGamma);
}

/** CIE xyY → linear sRGB (D65). */
export function xyYToLinearRGB(x: number, y: number, Y: number): Vec3 {
  if (y <= 1e-6) return [0, 0, 0];
  const X = (x * Y) / y;
  const Z = ((1 - x - y) * Y) / y;
  return [
    3.2404542 * X - 1.5371385 * Y - 0.4985314 * Z,
    -0.969266 * X + 1.8760108 * Y + 0.041556 * Z,
    0.0556434 * X - 0.2040259 * Y + 1.0572252 * Z,
  ];
}

/**
 * Sky radiance (linear RGB, scene units) in direction `dir` (unit, three.js
 * frame). Directions below the horizon return the horizon value.
 */
export function skyRadiance(dir: Vec3, sun: Vec3, pc: PerezCoefficients): Vec3 {
  const cosTheta = Math.max(dir[1], 0.0);
  let cosGamma = dir[0] * sun[0] + dir[1] * sun[1] + dir[2] * sun[2];
  cosGamma = Math.min(Math.max(cosGamma, -1), 1);
  const gamma = Math.acos(cosGamma);
  const cosThetaS = Math.cos(pc.thetaS);
  const Y = (pc.zenithY * perez(pc.Y, cosTheta, gamma, cosGamma)) / perez(pc.Y, cosThetaS, pc.thetaS, cosThetaS);
  const x = (pc.zenithX * perez(pc.x, cosTheta, gamma, cosGamma)) / perez(pc.x, cosThetaS, pc.thetaS, cosThetaS);
  const y = (pc.zenithYc * perez(pc.y, cosTheta, gamma, cosGamma)) / perez(pc.y, cosThetaS, pc.thetaS, cosThetaS);
  const rgb = xyYToLinearRGB(x, y, Math.max(Y, 0) / SCENE_UNIT);
  return [Math.max(rgb[0], 0), Math.max(rgb[1], 0), Math.max(rgb[2], 0)];
}

/**
 * Transmittance of direct sunlight through the atmosphere (linear RGB),
 * Rayleigh + Mie with the constants used by Preetham-style sky shaders.
 */
export function sunTransmittance(sunElevationDeg: number, turbidity: number): Vec3 {
  const el = Math.max(sunElevationDeg, 0.5);
  const zenith = 90 - el;
  // Kasten & Young relative air mass.
  const m = 1 / (Math.cos(zenith * DEG) + 0.15 * Math.pow(93.885 - zenith, -1.253));
  // Rayleigh scattering coefficient per metre (R, G, B) and the Mie constant
  // that, times the turbidity-dependent concentration c, gives the Mie
  // coefficient per metre. Zenith path lengths: 8.4 km Rayleigh, 1.25 km Mie.
  const rayleigh: Vec3 = [5.804542996261093e-6, 1.3562911419845635e-5, 3.0265902468824876e-5];
  const mieConst: Vec3 = [1.8399918514433978e14, 2.7798023919660528e14, 4.0790479543861094e14];
  const c = 0.2 * turbidity * 1e-18;
  const sR = 8.4e3 * m;
  const sM = 1.25e3 * m;
  const out: Vec3 = [0, 0, 0];
  for (let i = 0; i < 3; i++) {
    out[i] = Math.exp(-(rayleigh[i] * sR + mieConst[i] * c * sM));
  }
  return out;
}

export interface SkyLighting {
  /** Irradiance on an up-facing surface from the sky dome (scene units). */
  skyIrradiance: Vec3;
  /** Mean sky radiance, used as the hemisphere light's sky colour. */
  skyAverage: Vec3;
  /** Radiance at the horizon away from the sun (fog colour). */
  horizon: Vec3;
  /** Irradiance from the solar disc on a surface facing the sun. */
  sunIrradiance: Vec3;
  /** Radiance of the solar disc. */
  sunRadiance: Vec3;
  sunDir: Vec3;
  perez: PerezCoefficients;
}

/** Integrate the sky model for scene lighting. Cheap: 24×48 samples. */
export function computeSkyLighting(p: SkyParams): SkyLighting {
  const pc = perezCoefficients(p.turbidity, p.sunElevation);
  const sun = sunDirection(p.sunAzimuth, p.sunElevation);
  const irr: Vec3 = [0, 0, 0];
  const avg: Vec3 = [0, 0, 0];
  const nTheta = 24, nPhi = 48;
  let wsum = 0;
  for (let i = 0; i < nTheta; i++) {
    const theta = ((i + 0.5) / nTheta) * (Math.PI / 2);
    const dOmega = Math.sin(theta) * (Math.PI / 2 / nTheta) * ((2 * Math.PI) / nPhi);
    for (let j = 0; j < nPhi; j++) {
      const phi = ((j + 0.5) / nPhi) * 2 * Math.PI;
      const d: Vec3 = [Math.sin(theta) * Math.sin(phi), Math.cos(theta), -Math.sin(theta) * Math.cos(phi)];
      const L = skyRadiance(d, sun, pc);
      for (let k = 0; k < 3; k++) {
        irr[k] += L[k] * Math.cos(theta) * dOmega;
        avg[k] += L[k] * dOmega;
      }
      wsum += dOmega;
    }
  }
  for (let k = 0; k < 3; k++) avg[k] /= wsum;
  // Horizon colour opposite the sun, slightly above the horizon.
  const away: Vec3 = [-sun[0], 0.02, -sun[2]];
  const len = Math.hypot(away[0], away[1], away[2]);
  const horizon = skyRadiance([away[0] / len, away[1] / len, away[2] / len], sun, pc);
  const tr = sunTransmittance(p.sunElevation, p.turbidity);
  const sunRadiance: Vec3 = [tr[0] * p.sunIntensity, tr[1] * p.sunIntensity, tr[2] * p.sunIntensity];
  const sunIrradiance: Vec3 = [sunRadiance[0] * SUN_SOLID_ANGLE, sunRadiance[1] * SUN_SOLID_ANGLE, sunRadiance[2] * SUN_SOLID_ANGLE];
  return { skyIrradiance: irr, skyAverage: avg, horizon, sunIrradiance, sunRadiance, sunDir: sun, perez: pc };
}

/** GLSL for the sky dome and the HDRI pass. Uniforms match PerezCoefficients. */
export const SKY_GLSL = /* glsl */ `
uniform vec3 uSunDir;
uniform vec3 uPerezY; // A, B, C
uniform vec2 uPerezY2; // D, E
uniform vec3 uPerezX;
uniform vec2 uPerezX2;
uniform vec3 uPerezYc;
uniform vec2 uPerezYc2;
uniform vec3 uZenith; // Y (kcd/m²), x, y
uniform float uThetaS;
uniform float uSceneUnit;
uniform vec3 uSunRadiance;
uniform float uSunRadius; // radians
uniform float uShowSun;

float perezF(vec3 abc, vec2 de, float cosTheta, float gamma, float cosGamma) {
  return (1.0 + abc.x * exp(abc.y / max(cosTheta, 0.01))) * (1.0 + abc.z * exp(de.x * gamma) + de.y * cosGamma * cosGamma);
}

vec3 xyYToRGB(float x, float y, float Y) {
  if (y <= 1e-6) return vec3(0.0);
  float X = x * Y / y;
  float Z = (1.0 - x - y) * Y / y;
  return vec3(
    3.2404542 * X - 1.5371385 * Y - 0.4985314 * Z,
    -0.969266 * X + 1.8760108 * Y + 0.041556 * Z,
    0.0556434 * X - 0.2040259 * Y + 1.0572252 * Z);
}

vec3 skyRadiance(vec3 dir) {
  float cosTheta = max(dir.y, 0.0);
  float cosGamma = clamp(dot(dir, uSunDir), -1.0, 1.0);
  float gamma = acos(cosGamma);
  float cosThetaS = cos(uThetaS);
  float Y = uZenith.x * perezF(uPerezY, uPerezY2, cosTheta, gamma, cosGamma) / perezF(uPerezY, uPerezY2, cosThetaS, uThetaS, cosThetaS);
  float x = uZenith.y * perezF(uPerezX, uPerezX2, cosTheta, gamma, cosGamma) / perezF(uPerezX, uPerezX2, cosThetaS, uThetaS, cosThetaS);
  float y = uZenith.z * perezF(uPerezYc, uPerezYc2, cosTheta, gamma, cosGamma) / perezF(uPerezYc, uPerezYc2, cosThetaS, uThetaS, cosThetaS);
  return max(xyYToRGB(x, y, max(Y, 0.0) / uSceneUnit), vec3(0.0));
}

// Solar disc with limb darkening; 0 outside the disc.
vec3 sunDisc(vec3 dir) {
  float cosGamma = clamp(dot(dir, uSunDir), -1.0, 1.0);
  float gamma = acos(cosGamma);
  if (gamma >= uSunRadius) return vec3(0.0);
  float r = gamma / uSunRadius;
  float limb = 1.0 - 0.6 * (1.0 - sqrt(max(1.0 - r * r, 0.0)));
  return uSunRadiance * limb;
}
`;

/** Values for the GLSL uniforms above. */
export function skyUniformValues(p: SkyParams, lighting: SkyLighting): Record<string, number | number[]> {
  const pc = lighting.perez;
  return {
    uSunDir: lighting.sunDir,
    uPerezY: [pc.Y[0], pc.Y[1], pc.Y[2]],
    uPerezY2: [pc.Y[3], pc.Y[4]],
    uPerezX: [pc.x[0], pc.x[1], pc.x[2]],
    uPerezX2: [pc.x[3], pc.x[4]],
    uPerezYc: [pc.y[0], pc.y[1], pc.y[2]],
    uPerezYc2: [pc.y[3], pc.y[4]],
    uZenith: [pc.zenithY, pc.zenithX, pc.zenithYc],
    uThetaS: pc.thetaS,
    uSceneUnit: SCENE_UNIT,
    uSunRadiance: lighting.sunRadiance,
    uSunRadius: SUN_RADIUS,
    uShowSun: 1,
  };
}
