/**
 * Previews of a captured HDRI: a tone-mapped flat image on a 2D canvas and
 * a look-around sphere drawn with the same equirect convention as the capture.
 */
import * as THREE from 'three';
import { OrbitControls } from 'three/addons/controls/OrbitControls.js';
import { floatToHalf } from '../core/exr';

/** A reduced-resolution float copy of a panorama kept on the CPU for previews. */
export interface PreviewImage {
  width: number;
  height: number;
  /** RGB floats, top row first. */
  data: Float32Array;
}

/** ACES filmic approximation (Narkowicz), applied per channel on linear input. */
export function acesTonemap(x: number): number {
  const a = 2.51, b = 0.03, c = 2.43, d = 0.59, e = 0.14;
  const y = (x * (a * x + b)) / (x * (c * x + d) + e);
  return y < 0 ? 0 : y > 1 ? 1 : y;
}

function linearToSrgb(v: number): number {
  return v <= 0.0031308 ? 12.92 * v : 1.055 * Math.pow(v, 1 / 2.4) - 0.055;
}

/** Draw the preview with an exposure multiplier; returns the ImageData for PNG export. */
export function drawPreview(canvas: HTMLCanvasElement, img: PreviewImage, exposure: number): ImageData {
  canvas.width = img.width;
  canvas.height = img.height;
  const ctx = canvas.getContext('2d')!;
  const out = ctx.createImageData(img.width, img.height);
  const px = out.data;
  const lut = new Uint8ClampedArray(4096);
  for (let i = 0; i < lut.length; i++) lut[i] = Math.round(linearToSrgb(i / 4095) * 255);
  for (let i = 0, o = 0; i < img.data.length; i += 3, o += 4) {
    px[o] = lut[Math.round(acesTonemap(img.data[i] * exposure) * 4095)];
    px[o + 1] = lut[Math.round(acesTonemap(img.data[i + 1] * exposure) * 4095)];
    px[o + 2] = lut[Math.round(acesTonemap(img.data[i + 2] * exposure) * 4095)];
    px[o + 3] = 255;
  }
  ctx.putImageData(out, 0, 0);
  return out;
}

/** Box-filter an RGBA strip (top row first) into an RGB preview buffer at 1/factor. */
export function accumulatePreview(preview: PreviewImage, strip: Float32Array, srcWidth: number, y0: number, rows: number, factor: number): void {
  const pw = preview.width;
  const inv = 1 / (factor * factor);
  for (let r = 0; r < rows; r++) {
    const py = Math.floor((y0 + r) / factor);
    if (py >= preview.height) break;
    for (let x = 0; x < srcWidth; x++) {
      const px = Math.floor(x / factor);
      if (px >= pw) break;
      const s = (r * srcWidth + x) * 4;
      const d = (py * pw + px) * 3;
      preview.data[d] += strip[s] * inv;
      preview.data[d + 1] += strip[s + 1] * inv;
      preview.data[d + 2] += strip[s + 2] * inv;
    }
  }
}

/** Interactive sphere viewer for the captured panorama. */
export class SphereViewer {
  private renderer: THREE.WebGLRenderer;
  private scene = new THREE.Scene();
  private camera: THREE.PerspectiveCamera;
  private controls: OrbitControls;
  private material: THREE.ShaderMaterial;
  private texture: THREE.DataTexture | null = null;
  exposure = 1;

  constructor(canvas: HTMLCanvasElement) {
    this.renderer = new THREE.WebGLRenderer({ canvas, antialias: true });
    this.renderer.setPixelRatio(Math.min(window.devicePixelRatio || 1, 2));
    this.renderer.toneMapping = THREE.ACESFilmicToneMapping;
    this.renderer.outputColorSpace = THREE.SRGBColorSpace;
    this.camera = new THREE.PerspectiveCamera(75, 1, 0.1, 100);
    this.camera.position.set(0, 0, 0.01);
    this.controls = new OrbitControls(this.camera, canvas);
    this.controls.enableZoom = false;
    this.controls.enablePan = false;
    this.controls.rotateSpeed = -0.35;
    this.controls.enableDamping = true;
    this.material = new THREE.ShaderMaterial({
      uniforms: { tPano: { value: null }, uExposure: { value: 1 } },
      vertexShader: /* glsl */ `
        varying vec3 vDir;
        void main() {
          vDir = position;
          gl_Position = projectionMatrix * modelViewMatrix * vec4(position, 1.0);
        }`,
      fragmentShader: /* glsl */ `
        uniform sampler2D tPano;
        uniform float uExposure;
        varying vec3 vDir;
        void main() {
          vec3 d = normalize(vDir);
          float az = atan(d.x, -d.z);
          float u = fract(az / 6.28318530718 + 0.5);
          float v = 0.5 - asin(clamp(d.y, -1.0, 1.0)) / 3.14159265359;
          vec3 c = texture2D(tPano, vec2(u, v)).rgb * uExposure;
          gl_FragColor = vec4(c, 1.0);
          #include <tonemapping_fragment>
          #include <colorspace_fragment>
        }`,
      side: THREE.BackSide,
    });
    const sphere = new THREE.Mesh(new THREE.SphereGeometry(10, 64, 32), this.material);
    this.scene.add(sphere);
  }

  setImage(img: PreviewImage): void {
    // Half floats are filterable in WebGL2 without extensions. Data row 0 (the zenith) becomes
    // texture v = 0, which is what the sphere shader samples for straight up.
    const rgba = new Uint16Array(img.width * img.height * 4);
    const one = floatToHalf(1);
    for (let y = 0; y < img.height; y++) {
      const srcRow = y * img.width * 3;
      const dstRow = y * img.width * 4;
      for (let x = 0; x < img.width; x++) {
        rgba[dstRow + x * 4] = floatToHalf(Math.min(img.data[srcRow + x * 3], 60000));
        rgba[dstRow + x * 4 + 1] = floatToHalf(Math.min(img.data[srcRow + x * 3 + 1], 60000));
        rgba[dstRow + x * 4 + 2] = floatToHalf(Math.min(img.data[srcRow + x * 3 + 2], 60000));
        rgba[dstRow + x * 4 + 3] = one;
      }
    }
    this.texture?.dispose();
    this.texture = new THREE.DataTexture(rgba, img.width, img.height, THREE.RGBAFormat, THREE.HalfFloatType);
    this.texture.colorSpace = THREE.LinearSRGBColorSpace;
    this.texture.minFilter = THREE.LinearFilter;
    this.texture.magFilter = THREE.LinearFilter;
    this.texture.wrapS = THREE.RepeatWrapping;
    this.texture.needsUpdate = true;
    this.material.uniforms.tPano.value = this.texture;
  }

  setExposure(e: number): void {
    this.exposure = e;
    this.material.uniforms.uExposure.value = e;
  }

  resize(): void {
    const c = this.renderer.domElement;
    const w = c.clientWidth || 1, h = c.clientHeight || 1;
    this.renderer.setSize(w, h, false);
    this.camera.aspect = w / h;
    this.camera.updateProjectionMatrix();
  }

  render(): void {
    this.controls.update();
    this.renderer.render(this.scene, this.camera);
  }
}
