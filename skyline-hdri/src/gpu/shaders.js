// GLSL ES 3.00 shaders for the per-pixel heightfield march.
//
// The functions sampleTier / heightAt / tierFor / stepSize mirror src/engine/tiers.js and
// src/engine/sweep.js exactly; keep them in sync (the Node test compares both paths).
// Porting note for Makepad: everything here is plain GLSL arithmetic plus texelFetch; the only
// WebGL-specific parts are the uniform declarations and the MRT outputs.

export const MAX_TIERS = 6;

export const VERT = `#version 300 es
precision highp float;
const vec2 QUAD[6] = vec2[6](vec2(-1.,-1.), vec2(1.,-1.), vec2(-1.,1.), vec2(-1.,1.), vec2(1.,-1.), vec2(1.,1.));
void main() { gl_Position = vec4(QUAD[gl_VertexID], 0.0, 1.0); }
`;

const COMMON = `#version 300 es
precision highp float;
precision highp int;
precision highp sampler2D;

const float PI = 3.141592653589793;
const float TWO_PI = 6.283185307179586;
const int MAX_STEPS = 24000;

uniform int   uTierCount;
uniform float uTierSize[${MAX_TIERS}];
uniform float uTierTexel[${MAX_TIERS}];
uniform float uTierLimit[${MAX_TIERS}];   // radius - 2 texels
uniform sampler2D uH0, uH1, uH2, uH3, uH4, uH5;   // R32F heights
uniform sampler2D uC0, uC1, uC2, uC3, uC4, uC5;   // R8 class (0 terrain, 1 building)

uniform vec2  uPlate;        // W, H
uniform float uEyeZ;
uniform float uZGround;
uniform float uVertExag;
uniform float uHeading;      // rad
uniform float uMaxDist;      // m
uniform float uHMax;         // highest exaggerated height in the tiers
uniform float uInvR2;        // 1 / (2 * R_eff)
uniform float uDelta;        // 2 pi / W

float fetchH(int k, ivec2 p) {
  if (k == 0) return texelFetch(uH0, p, 0).r;
  if (k == 1) return texelFetch(uH1, p, 0).r;
  if (k == 2) return texelFetch(uH2, p, 0).r;
  if (k == 3) return texelFetch(uH3, p, 0).r;
  if (k == 4) return texelFetch(uH4, p, 0).r;
  return texelFetch(uH5, p, 0).r;
}
float fetchC(int k, ivec2 p) {
  if (k == 0) return texelFetch(uC0, p, 0).r;
  if (k == 1) return texelFetch(uC1, p, 0).r;
  if (k == 2) return texelFetch(uC2, p, 0).r;
  if (k == 3) return texelFetch(uC3, p, 0).r;
  if (k == 4) return texelFetch(uC4, p, 0).r;
  return texelFetch(uC5, p, 0).r;
}

// Bilinear height at (east, north) metres in tier k. Mirrors sampleTier() in tiers.js.
float sampleTier(int k, vec2 en) {
  float size = uTierSize[k], texel = uTierTexel[k];
  float fi = en.x / texel + size * 0.5 - 0.5;
  float fj = en.y / texel + size * 0.5 - 0.5;
  float i0f = floor(fi), j0f = floor(fj);
  float tx = fi - i0f, ty = fj - j0f;
  int m = int(size) - 1;
  int i0 = clamp(int(i0f), 0, m), i1 = clamp(int(i0f) + 1, 0, m);
  int j0 = clamp(int(j0f), 0, m), j1 = clamp(int(j0f) + 1, 0, m);
  float a = fetchH(k, ivec2(i0, j0)), b = fetchH(k, ivec2(i1, j0));
  float c = fetchH(k, ivec2(i0, j1)), d = fetchH(k, ivec2(i1, j1));
  float top = a + (b - a) * tx;
  return top + ((c + (d - c) * tx) - top) * ty;
}
float classAt(int k, vec2 en) {
  float size = uTierSize[k], texel = uTierTexel[k];
  int m = int(size) - 1;
  int i = clamp(int(floor(en.x / texel + size * 0.5)), 0, m);
  int j = clamp(int(floor(en.y / texel + size * 0.5)), 0, m);
  return fetchC(k, ivec2(i, j));
}
float heightAt(int k, vec2 en) {
  float h = sampleTier(k, en);
  return uZGround + (h - uZGround) * uVertExag;
}
int tierFor(vec2 en) {
  float m = max(abs(en.x), abs(en.y));
  for (int k = 0; k < ${MAX_TIERS}; k++) {
    if (k >= uTierCount) break;
    if (m < uTierLimit[k]) return k;
  }
  return -1;
}
float stepSize(float d, float texel) {
  return max(0.5 * texel, 0.5 * d * uDelta);
}
vec3 normalAt(int k, vec2 en) {
  float t = uTierTexel[k];
  float dx = (heightAt(k, en + vec2(t, 0.0)) - heightAt(k, en - vec2(t, 0.0))) / (2.0 * t);
  float dy = (heightAt(k, en + vec2(0.0, t)) - heightAt(k, en - vec2(0.0, t))) / (2.0 * t);
  return normalize(vec3(-dx, -dy, 1.0));
}
`;

/**
 * Pre-pass: one fragment per (column, band). Marches the column to the band's radius (the last
 * band row marches to uMaxDist) and records the maximum apparent elevation and where it was found.
 */
export const FRAG_HORIZON = COMMON + `
uniform int   uBandCount;
uniform float uBandRadius[8];
out vec4 outHorizon;   // (eMax, dAtMax, 0, 1)

void main() {
  int b = int(gl_FragCoord.y);
  float limit = (b >= uBandCount - 1) ? uMaxDist : min(uBandRadius[b], uMaxDist);
  float theta = (gl_FragCoord.x / uPlate.x) * TWO_PI - PI + uHeading;
  vec2 dir = vec2(sin(theta), cos(theta));
  float eMax = -0.5 * PI, dMax = 0.0;
  float d = 1.0;
  for (int i = 0; i < MAX_STEPS; i++) {
    vec2 en = d * dir;
    int k = tierFor(en);
    if (k < 0) break;
    float h = heightAt(k, en);
    float e = atan(h - uEyeZ - d * d * uInvR2, d);
    if (e > eMax) { eMax = e; dMax = d; }
    if (d >= limit) break;
    d += stepSize(d, uTierTexel[k]);
  }
  outHorizon = vec4(eMax, dMax, 0.0, 1.0);
}
`;

/**
 * Main pass: one fragment per plate pixel of the current strip.
 *   out0 = (depth m, altitude m, class 1|2, coverage 0|1)   sky: (-1, 0, 0, 0)
 *   out1 = (normal.xyz, 1)                                   sky: (0, 0, 1, 0)
 */
export const FRAG_MARCH = COMMON + `
uniform sampler2D uHorizon;  // W x bands, R = eMax of the full range on the last row
uniform int   uBandCount;
uniform float uStripY0;      // first plate row (bottom-up) of this strip
layout(location = 0) out vec4 out0;
layout(location = 1) out vec4 out1;

void main() {
  int px = int(gl_FragCoord.x);
  float yPlate = uStripY0 + gl_FragCoord.y;
  float theta = (gl_FragCoord.x / uPlate.x) * TWO_PI - PI + uHeading;
  float e = (yPlate / uPlate.y - 0.5) * PI;
  float eH = texelFetch(uHorizon, ivec2(px, uBandCount - 1), 0).r;
  if (e > eH + 2.0 * uDelta) { out0 = vec4(-1.0, 0.0, 0.0, 0.0); out1 = vec4(0.0, 0.0, 1.0, 0.0); return; }
  vec2 dir = vec2(sin(theta), cos(theta));
  float tanE = tan(e);
  float d = 1.0, dPrev = 0.0;
  int k = 0;
  bool hit = false;
  for (int i = 0; i < MAX_STEPS; i++) {
    vec2 en = d * dir;
    k = tierFor(en);
    if (k < 0) break;
    float h = heightAt(k, en);
    float zRay = uEyeZ + d * tanE + d * d * uInvR2;
    if (h >= zRay) { hit = true; break; }
    // once the ray climbs it never comes back: above everything means sky
    if (zRay > uHMax && tanE + 2.0 * d * uInvR2 >= 0.0) break;
    if (d >= uMaxDist) break;
    dPrev = d;
    d += stepSize(d, uTierTexel[k]);
  }
  if (!hit) { out0 = vec4(-1.0, 0.0, 0.0, 0.0); out1 = vec4(0.0, 0.0, 1.0, 0.0); return; }
  float lo = dPrev, hi = d;
  for (int it = 0; it < 6; it++) {
    float mid = 0.5 * (lo + hi);
    vec2 en = mid * dir;
    int mk = tierFor(en);
    float hm = mk < 0 ? -1.0e9 : heightAt(mk, en);
    float zr = uEyeZ + mid * tanE + mid * mid * uInvR2;
    if (hm >= zr) hi = mid; else lo = mid;
  }
  float dh = hi;
  vec2 en = dh * dir;
  int hk = tierFor(en);
  if (hk < 0) hk = k;
  float hHit = heightAt(hk, en);
  float cls = classAt(hk, en);
  vec3 n = normalAt(hk, en);
  float depth = length(vec2(dh, hHit - uEyeZ));
  out0 = vec4(depth, hHit, cls > 0.5 ? 2.0 : 1.0, 1.0);
  out1 = vec4(n, 1.0);
}
`;
