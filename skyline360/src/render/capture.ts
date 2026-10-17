/**
 * 360° HDRI capture: six cube faces from the viewpoint, folded into an
 * equirectangular float target, read back in strips.
 */
import * as THREE from 'three';
import type { SkylineScene } from './scene';
import { EQUIRECT_GLSL } from '../core/equirect';
import { SKY_GLSL } from '../core/sky';

export interface CaptureOptions {
  /** Output width; height is width / 2. */
  width: number;
  /** Eye position in the three.js frame. */
  eye: [number, number, number];
  headingDeg: number;
  /** 1 or 2: faces rendered at 1× or 2× the equirect face size. */
  supersample?: number;
  /** Clamp radiance to this value (0 = no clamp). Use 65504 for half-float EXR with a bright sun. */
  clamp?: number;
  /** Draw the solar disc (off for a sunless sky). */
  sun?: boolean;
}

/** A captured panorama living in GPU memory; read it in strips and dispose it. */
export interface HdrCapture {
  width: number;
  height: number;
  /** Read `rows` image rows starting at image row `y0` (0 = zenith). RGBA floats, top row first. */
  readStrip(y0: number, rows: number): Float32Array;
  /** Maximum radiance seen while reading (updated per strip). */
  stats: { max: number; sunPixels: number };
  dispose(): void;
}

const EQUIRECT_FRAGMENT = /* glsl */ `
precision highp float;
uniform samplerCube tCube;
uniform float uSunEnabled;
uniform float uClamp;
${SKY_GLSL}
${EQUIRECT_GLSL}
varying vec2 vUv;
void main() {
  // vUv.y = 0 is the bottom GL row, which we define as the image's top row (zenith),
  // so read-back rows arrive in image order without a flip.
  vec3 dir = dirFromUV(vUv);
  vec4 c = textureCube(tCube, dir);
  vec3 col = c.rgb;
  if (c.a < 0.5 && uSunEnabled > 0.5) col += sunDisc(dir);
  if (uClamp > 0.0) col = min(col, vec3(uClamp));
  // Alpha carries the sky mask (0 = sky) so callers can inspect it; encoders ignore it.
  gl_FragColor = vec4(col, c.a);
}
`;

const EQUIRECT_VERTEX = /* glsl */ `
varying vec2 vUv;
void main() {
  vUv = uv;
  gl_Position = vec4(position.xy, 0.0, 1.0);
}
`;

export function captureEquirect(api: SkylineScene, opts: CaptureOptions): HdrCapture {
  const width = opts.width;
  const height = width / 2;
  const ss = opts.supersample ?? 1;
  const faceSize = Math.min(Math.round((width / 4) * ss), api.renderer.capabilities.maxCubemapSize);
  const renderer = api.renderer;

  const cubeTarget = new THREE.WebGLCubeRenderTarget(faceSize, {
    type: THREE.HalfFloatType,
    format: THREE.RGBAFormat,
    colorSpace: THREE.LinearSRGBColorSpace,
    generateMipmaps: false,
    minFilter: THREE.LinearFilter,
    magFilter: THREE.LinearFilter,
    depthBuffer: true,
  });
  const cubeCamera = new THREE.CubeCamera(0.3, 160000, cubeTarget);
  cubeCamera.position.set(opts.eye[0], opts.eye[1], opts.eye[2]);

  api.beginCapture();
  const prevClear = renderer.getClearColor(new THREE.Color());
  const prevAlpha = renderer.getClearAlpha();
  renderer.setClearColor(0x000000, 0);
  cubeCamera.update(renderer, api.scene);
  renderer.setClearColor(prevClear, prevAlpha);

  // Equirect pass.
  const target = new THREE.WebGLRenderTarget(width, height, {
    type: THREE.FloatType,
    format: THREE.RGBAFormat,
    colorSpace: THREE.LinearSRGBColorSpace,
    depthBuffer: false,
    stencilBuffer: false,
    generateMipmaps: false,
    minFilter: THREE.NearestFilter,
    magFilter: THREE.NearestFilter,
  });
  const skyU = api.skyMaterial.uniforms;
  const material = new THREE.ShaderMaterial({
    uniforms: {
      tCube: { value: cubeTarget.texture },
      uSunEnabled: { value: opts.sun === false ? 0 : 1 },
      uClamp: { value: opts.clamp ?? 0 },
      uHeading: { value: (opts.headingDeg * Math.PI) / 180 },
      uSunDir: skyU.uSunDir,
      uPerezY: skyU.uPerezY,
      uPerezY2: skyU.uPerezY2,
      uPerezX: skyU.uPerezX,
      uPerezX2: skyU.uPerezX2,
      uPerezYc: skyU.uPerezYc,
      uPerezYc2: skyU.uPerezYc2,
      uZenith: skyU.uZenith,
      uThetaS: skyU.uThetaS,
      uSceneUnit: skyU.uSceneUnit,
      uSunRadiance: skyU.uSunRadiance,
      uSunRadius: skyU.uSunRadius,
      uShowSun: { value: 1 },
    },
    vertexShader: EQUIRECT_VERTEX,
    fragmentShader: EQUIRECT_FRAGMENT,
    depthTest: false,
    depthWrite: false,
  });
  const quadScene = new THREE.Scene();
  const quad = new THREE.Mesh(new THREE.PlaneGeometry(2, 2), material);
  quad.frustumCulled = false;
  quadScene.add(quad);
  const orthoCamera = new THREE.OrthographicCamera(-1, 1, 1, -1, 0, 1);

  const prevTarget = renderer.getRenderTarget();
  renderer.setRenderTarget(target);
  renderer.render(quadScene, orthoCamera);
  renderer.setRenderTarget(prevTarget);
  api.endCapture();

  const stats = { max: 0, sunPixels: 0 };
  let disposed = false;
  return {
    width,
    height,
    stats,
    readStrip(y0: number, rows: number): Float32Array {
      if (disposed) throw new Error('capture disposed');
      const n = Math.min(rows, height - y0);
      const buf = new Float32Array(width * n * 4);
      renderer.readRenderTargetPixels(target, 0, y0, width, n, buf);
      for (let i = 0; i < buf.length; i += 4) {
        const m = Math.max(buf[i], buf[i + 1], buf[i + 2]);
        if (m > stats.max) stats.max = m;
      }
      return buf;
    },
    dispose(): void {
      if (disposed) return;
      disposed = true;
      target.dispose();
      cubeTarget.dispose();
      material.dispose();
      quad.geometry.dispose();
    },
  };
}
