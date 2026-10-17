/**
 * three.js scene: terrain, buildings, ground classes, analytic sky, sun light
 * and sky image-based lighting. Linear radiance everywhere; tone mapping is
 * applied only when drawing to the screen.
 */
import * as THREE from 'three';
import { OrbitControls } from 'three/addons/controls/OrbitControls.js';
import type { MeshData, SkyParams } from '../core/types';
import type { SceneMeshes } from '../core/pipeline';
import type { TreeInstances } from '../core/trees';
import { SKY_GLSL, computeSkyLighting, skyUniformValues, type SkyLighting } from '../core/sky';
import { landCoverTextures, type LandCover } from '../core/landcover';

export interface StyleParams {
  facade: [number, number, number];
  roof: [number, number, number];
  terrain: [number, number, number];
  water: [number, number, number];
  green: [number, number, number];
  road: [number, number, number];
  crown: [number, number, number];
  trunk: [number, number, number];
}

/** Linear albedos; the facades read as light concrete, roofs as dark gravel. */
export const DEFAULT_STYLE: StyleParams = {
  facade: [0.42, 0.4, 0.37],
  roof: [0.2, 0.19, 0.18],
  terrain: [0.22, 0.2, 0.16],
  water: [0.02, 0.05, 0.07],
  green: [0.07, 0.14, 0.05],
  road: [0.12, 0.12, 0.12],
  crown: [0.045, 0.1, 0.03],
  trunk: [0.11, 0.08, 0.05],
};

export const DEFAULT_SKY: SkyParams = { sunAzimuth: 135, sunElevation: 38, turbidity: 2.5, sunIntensity: 100000 };

const SKY_RADIUS = 50000;

function geometryFrom(md: MeshData): THREE.BufferGeometry {
  const g = new THREE.BufferGeometry();
  g.setAttribute('position', new THREE.BufferAttribute(md.positions, 3));
  g.setAttribute('normal', new THREE.BufferAttribute(md.normals, 3));
  if (md.uvs) g.setAttribute('uv', new THREE.BufferAttribute(md.uvs, 2));
  g.setIndex(new THREE.BufferAttribute(md.indices, 1));
  g.computeBoundingSphere();
  return g;
}

function linearColor(rgb: [number, number, number]): THREE.Color {
  return new THREE.Color().setRGB(rgb[0], rgb[1], rgb[2], THREE.LinearSRGBColorSpace);
}

export class SkylineScene {
  readonly renderer: THREE.WebGLRenderer;
  readonly scene = new THREE.Scene();
  readonly camera: THREE.PerspectiveCamera;
  readonly controls: OrbitControls;
  readonly skyMaterial: THREE.ShaderMaterial;
  readonly sunLight: THREE.DirectionalLight;
  readonly viewpointMarker: THREE.Group;
  readonly meshGroup = new THREE.Group();
  private skyMesh: THREE.Mesh;
  private skyScene = new THREE.Scene();
  private pmrem: THREE.PMREMGenerator;
  private envTarget: THREE.WebGLRenderTarget | null = null;
  private materials: Record<'facade' | 'roof' | 'terrain' | 'crown' | 'trunk', THREE.MeshStandardMaterial>;
  private ringMaterial: THREE.MeshStandardMaterial;
  private landCover: LandCover | null = null;
  private coverAlbedo: THREE.DataTexture | null = null;
  private coverRoughness: THREE.DataTexture | null = null;
  skyParams: SkyParams = { ...DEFAULT_SKY };
  lighting: SkyLighting;
  style: StyleParams = { ...DEFAULT_STYLE };
  radius = 1500;
  groundAtOrigin = 0;
  exposure = 1;

  constructor(canvas: HTMLCanvasElement) {
    this.renderer = new THREE.WebGLRenderer({ canvas, antialias: true, powerPreference: 'high-performance', alpha: false });
    this.renderer.setPixelRatio(Math.min(window.devicePixelRatio || 1, 2));
    this.renderer.shadowMap.enabled = true;
    this.renderer.shadowMap.type = THREE.PCFShadowMap;
    // The scene is static between edits: render the shadow map only when something changed.
    this.renderer.shadowMap.autoUpdate = false;
    this.renderer.toneMapping = THREE.ACESFilmicToneMapping;
    this.renderer.toneMappingExposure = this.exposure;
    this.renderer.outputColorSpace = THREE.SRGBColorSpace;

    this.camera = new THREE.PerspectiveCamera(55, 1, 1, 160000);
    this.camera.position.set(1200, 900, 1800);
    this.controls = new OrbitControls(this.camera, canvas);
    this.controls.enableDamping = true;
    this.controls.maxPolarAngle = Math.PI * 0.49;
    this.controls.maxDistance = 40000;

    this.pmrem = new THREE.PMREMGenerator(this.renderer);

    // Sky dome shared by the main scene and the environment-map scene.
    const initial = skyUniformValues(this.skyParams, computeSkyLighting(this.skyParams));
    const uniforms: Record<string, THREE.IUniform> = {};
    for (const [k, v] of Object.entries(initial)) {
      uniforms[k] = { value: Array.isArray(v) ? (v.length === 2 ? new THREE.Vector2(v[0], v[1]) : new THREE.Vector3(v[0], v[1], v[2])) : v };
    }
    this.skyMaterial = new THREE.ShaderMaterial({
      uniforms,
      vertexShader: /* glsl */ `
        varying vec3 vDir;
        void main() {
          vDir = normalize((modelMatrix * vec4(position, 1.0)).xyz - cameraPosition);
          gl_Position = projectionMatrix * modelViewMatrix * vec4(position, 1.0);
        }`,
      fragmentShader: /* glsl */ `
        ${SKY_GLSL}
        varying vec3 vDir;
        void main() {
          vec3 d = normalize(vDir);
          vec3 c = skyRadiance(d) + uShowSun * sunDisc(d);
          // Alpha 0 marks "sky" for the HDRI pass; geometry writes alpha 1.
          gl_FragColor = vec4(c, 0.0);
        }`,
      side: THREE.BackSide,
      depthWrite: false,
      fog: false,
    });
    this.skyMesh = new THREE.Mesh(new THREE.SphereGeometry(SKY_RADIUS, 96, 48), this.skyMaterial);
    this.skyMesh.frustumCulled = false;
    this.scene.add(this.skyMesh);
    // The environment scene renders the same material on its own dome.
    const envDome = new THREE.Mesh(this.skyMesh.geometry, this.skyMaterial);
    envDome.frustumCulled = false;
    this.skyScene.add(envDome);

    this.sunLight = new THREE.DirectionalLight(0xffffff, 1);
    this.sunLight.castShadow = true;
    this.sunLight.shadow.mapSize.set(4096, 4096);
    this.sunLight.shadow.bias = -0.0004;
    this.sunLight.shadow.normalBias = 2;
    this.scene.add(this.sunLight);
    this.scene.add(this.sunLight.target);

    this.materials = {
      facade: new THREE.MeshStandardMaterial({ roughness: 0.92, metalness: 0 }),
      roof: new THREE.MeshStandardMaterial({ roughness: 0.95, metalness: 0 }),
      terrain: new THREE.MeshStandardMaterial({ roughness: 1, metalness: 0 }),
      crown: new THREE.MeshStandardMaterial({ roughness: 1, metalness: 0 }),
      trunk: new THREE.MeshStandardMaterial({ roughness: 1, metalness: 0 }),
    };
    this.ringMaterial = new THREE.MeshStandardMaterial({ roughness: 1, metalness: 0 });
    this.applyStyle(this.style);
    this.scene.add(this.meshGroup);

    // Viewpoint marker: a thin pole with a small sphere at eye height.
    this.viewpointMarker = new THREE.Group();
    const pole = new THREE.Mesh(new THREE.CylinderGeometry(0.6, 0.6, 1, 12), new THREE.MeshBasicMaterial({ color: 0xffaa33 }));
    pole.name = 'pole';
    const head = new THREE.Mesh(new THREE.SphereGeometry(3, 16, 12), new THREE.MeshBasicMaterial({ color: 0xffaa33 }));
    head.name = 'head';
    this.viewpointMarker.add(pole, head);
    this.scene.add(this.viewpointMarker);

    this.lighting = computeSkyLighting(this.skyParams);
    this.setSky(this.skyParams);
  }

  /** Resize the drawing buffer to the canvas's CSS size. */
  resize(): void {
    const c = this.renderer.domElement;
    const w = c.clientWidth || 1;
    const h = c.clientHeight || 1;
    this.renderer.setSize(w, h, false);
    this.camera.aspect = w / h;
    this.camera.updateProjectionMatrix();
    this.invalidate();
  }

  applyStyle(style: StyleParams): void {
    this.style = { ...style };
    this.materials.facade.color = linearColor(style.facade);
    this.materials.roof.color = linearColor(style.roof);
    this.materials.crown.color = linearColor(style.crown);
    this.materials.trunk.color = linearColor(style.trunk);
    this.ringMaterial.color = linearColor(style.terrain);
    // The terrain's colour lives in the land-cover texture; keep the material white.
    this.materials.terrain.color = new THREE.Color(1, 1, 1);
    this.rebuildCoverTextures();
    this.invalidate();
  }

  /** Paint the land-cover classes into the terrain's albedo and roughness maps. */
  private rebuildCoverTextures(): void {
    if (!this.landCover) return;
    const cover = this.landCover;
    const tex = landCoverTextures(cover, { terrain: this.style.terrain, green: this.style.green, water: this.style.water, road: this.style.road });
    this.coverAlbedo?.dispose();
    this.coverRoughness?.dispose();
    const albedo = new THREE.DataTexture(tex.albedo, cover.width, cover.height, THREE.RGBAFormat, THREE.UnsignedByteType);
    albedo.colorSpace = THREE.LinearSRGBColorSpace;
    albedo.minFilter = THREE.LinearMipmapLinearFilter;
    albedo.magFilter = THREE.LinearFilter;
    albedo.generateMipmaps = true;
    albedo.anisotropy = Math.min(8, this.renderer.capabilities.getMaxAnisotropy());
    albedo.needsUpdate = true;
    const rough = new THREE.DataTexture(tex.roughness, cover.width, cover.height, THREE.RGBAFormat, THREE.UnsignedByteType);
    rough.minFilter = THREE.LinearMipmapLinearFilter;
    rough.magFilter = THREE.LinearFilter;
    rough.generateMipmaps = true;
    rough.needsUpdate = true;
    this.coverAlbedo = albedo;
    this.coverRoughness = rough;
    this.materials.terrain.map = albedo;
    this.materials.terrain.roughnessMap = rough;
    this.materials.terrain.roughness = 1;
    this.materials.terrain.needsUpdate = true;
  }

  /** Replace all scene meshes. */
  setMeshes(sm: SceneMeshes, radius: number): void {
    for (const child of [...this.meshGroup.children]) {
      this.meshGroup.remove(child);
      (child as THREE.Mesh).geometry.dispose();
      if ((child as THREE.InstancedMesh).isInstancedMesh) (child as THREE.InstancedMesh).dispose();
    }
    this.radius = radius;
    this.groundAtOrigin = sm.groundAtOrigin;
    const add = (md: MeshData, material: THREE.Material, name: string, shadows: boolean): void => {
      if (md.triangleCount === 0) return;
      const mesh = new THREE.Mesh(geometryFrom(md), material);
      mesh.name = name;
      mesh.castShadow = shadows;
      mesh.receiveShadow = true;
      mesh.frustumCulled = false;
      this.meshGroup.add(mesh);
    };
    this.landCover = sm.landCover;
    this.rebuildCoverTextures();
    add(sm.terrain, this.materials.terrain, 'terrain', true);
    add(sm.ring, this.ringMaterial, 'ring', false);
    if (sm.horizon) add(sm.horizon, this.ringMaterial, 'horizon', false);
    add(sm.walls, this.materials.facade, 'walls', true);
    add(sm.roofs, this.materials.roof, 'roofs', true);
    this.addTrees(sm.trees);
    this.updateShadowFrustum();
    this.invalidateShadows();
    this.controls.target.set(0, this.groundAtOrigin, 0);
    this.controls.update();
    this.invalidate();
  }

  /** One low-poly tree per instance: an ellipsoid crown on a short trunk, scaled by height. */
  private addTrees(trees: TreeInstances): void {
    if (trees.count === 0) return;
    // 20-triangle crowns and 4-sided trunks: a forest of 40,000 trees stays near a million triangles.
    const crownGeo = new THREE.IcosahedronGeometry(1, 0);
    crownGeo.scale(0.32, 0.36, 0.32);
    crownGeo.translate(0, 0.64, 0);
    const trunkGeo = new THREE.CylinderGeometry(0.025, 0.035, 0.42, 4, 1, true);
    trunkGeo.translate(0, 0.21, 0);
    const crowns = new THREE.InstancedMesh(crownGeo, this.materials.crown, trees.count);
    const trunks = new THREE.InstancedMesh(trunkGeo, this.materials.trunk, trees.count);
    const m = new THREE.Matrix4();
    const q = new THREE.Quaternion();
    const up = new THREE.Vector3(0, 1, 0);
    const pos = new THREE.Vector3();
    const scl = new THREE.Vector3();
    for (let i = 0; i < trees.count; i++) {
      pos.set(trees.positions[i * 3], trees.positions[i * 3 + 1], trees.positions[i * 3 + 2]);
      const h = trees.heights[i];
      scl.set(h * (0.85 + 0.3 * ((i * 7919) % 13) / 13), h, h * (0.85 + 0.3 * ((i * 104729) % 11) / 11));
      q.setFromAxisAngle(up, trees.rotations[i]);
      m.compose(pos, q, scl);
      crowns.setMatrixAt(i, m);
      trunks.setMatrixAt(i, m);
    }
    for (const mesh of [crowns, trunks]) {
      mesh.instanceMatrix.needsUpdate = true;
      mesh.castShadow = true;
      mesh.receiveShadow = true;
      mesh.frustumCulled = false;
      mesh.name = mesh === crowns ? 'tree-crowns' : 'tree-trunks';
      this.meshGroup.add(mesh);
    }
  }

  /** Update sun direction, sky uniforms, fog and the sky environment map. */
  setSky(p: SkyParams): void {
    this.skyParams = { ...p };
    this.lighting = computeSkyLighting(this.skyParams);
    const vals = skyUniformValues(this.skyParams, this.lighting);
    for (const [k, v] of Object.entries(vals)) {
      const u = this.skyMaterial.uniforms[k];
      if (!u) continue;
      if (Array.isArray(v)) {
        if (u.value instanceof THREE.Vector2) u.value.set(v[0], v[1]);
        else (u.value as THREE.Vector3).set(v[0], v[1], v[2]);
      } else u.value = v;
    }
    const L = this.lighting;
    this.sunLight.color.setRGB(L.sunIrradiance[0], L.sunIrradiance[1], L.sunIrradiance[2], THREE.LinearSRGBColorSpace);
    this.sunLight.intensity = 1;
    this.updateShadowFrustum();
    // Aerial perspective: extinction grows with turbidity; T = 2.5 leaves a 35 km ridge at ~40% contrast.
    const fogColor = linearColor(L.horizon);
    this.scene.fog = new THREE.FogExp2(fogColor, 1e-5 * this.skyParams.turbidity);
    this.scheduleEnvironment();
    this.invalidateShadows();
  }

  private updateShadowFrustum(): void {
    const d = this.lighting.sunDir;
    const dist = Math.max(this.radius * 2.5, 2000);
    this.sunLight.position.set(d[0] * dist, d[1] * dist, d[2] * dist);
    this.sunLight.target.position.set(0, this.groundAtOrigin, 0);
    const cam = this.sunLight.shadow.camera;
    const ext = this.radius * 1.15 + 100;
    cam.left = -ext;
    cam.right = ext;
    cam.top = ext;
    cam.bottom = -ext;
    cam.near = 10;
    cam.far = dist * 2 + this.radius;
    cam.updateProjectionMatrix();
    this.sunLight.shadow.needsUpdate = true;
  }

  private envTimer: ReturnType<typeof setTimeout> | null = null;

  /** Rebuild the sky environment at most every 120 ms while sliders move. */
  private scheduleEnvironment(): void {
    if (this.envTimer) return;
    this.envTimer = setTimeout(() => {
      this.envTimer = null;
      this.rebuildEnvironment();
      this.invalidate();
    }, 120);
  }

  /** Prefilter the sky dome (without the sun disc) into the scene's environment map. */
  private rebuildEnvironment(): void {
    const show = this.skyMaterial.uniforms.uShowSun.value as number;
    this.skyMaterial.uniforms.uShowSun.value = 0;
    const old = this.envTarget;
    this.envTarget = this.pmrem.fromScene(this.skyScene, 0, 1, SKY_RADIUS * 2);
    this.scene.environment = this.envTarget.texture;
    this.scene.environmentIntensity = 1;
    if (old) old.dispose();
    this.skyMaterial.uniforms.uShowSun.value = show;
  }

  setExposure(e: number): void {
    this.exposure = e;
    this.renderer.toneMappingExposure = e;
    this.invalidate();
  }

  /** Place the viewpoint marker at a local position (three.js frame) with eye height. */
  setViewpoint(x: number, groundY: number, z: number, eyeHeight: number): void {
    const pole = this.viewpointMarker.getObjectByName('pole') as THREE.Mesh;
    const head = this.viewpointMarker.getObjectByName('head') as THREE.Mesh;
    const h = Math.max(eyeHeight, 0.5);
    pole.scale.set(1, h, 1);
    pole.position.set(0, h / 2, 0);
    head.position.set(0, h, 0);
    head.scale.setScalar(Math.max(1, h / 40));
    this.viewpointMarker.position.set(x, groundY, z);
    this.invalidate();
  }

  /** Switch between the editing view and the raw capture state. */
  beginCapture(): void {
    if (this.envTimer) {
      clearTimeout(this.envTimer);
      this.envTimer = null;
      this.rebuildEnvironment();
    }
    this.skyMaterial.uniforms.uShowSun.value = 0; // the HDRI pass draws the sun disc analytically
    this.viewpointMarker.visible = false;
    this.renderer.toneMapping = THREE.NoToneMapping;
  }

  endCapture(): void {
    this.skyMaterial.uniforms.uShowSun.value = 1;
    this.viewpointMarker.visible = true;
    this.renderer.toneMapping = THREE.ACESFilmicToneMapping;
    this.invalidate();
  }

  /** Orbit camera to a pleasant overview of the loaded area. */
  frameArea(): void {
    const r = this.radius;
    this.camera.position.set(r * 0.9, r * 0.7 + this.groundAtOrigin, r * 1.3);
    this.controls.target.set(0, this.groundAtOrigin, 0);
    this.controls.update();
    this.invalidate();
  }

  /** Put the orbit camera at the viewpoint, looking along the heading. */
  lookFromViewpoint(x: number, y: number, z: number, headingDeg: number): void {
    const h = (headingDeg * Math.PI) / 180;
    this.camera.position.set(x, y, z);
    this.controls.target.set(x + Math.sin(h) * 50, y, z - Math.cos(h) * 50);
    this.controls.update();
    this.invalidate();
  }

  private dirty = true;

  /** Ask for a redraw on the next frame (state changed outside the camera). */
  invalidate(): void {
    this.dirty = true;
  }

  /** Re-render the shadow map on the next frame (sun, meshes or style changed). */
  invalidateShadows(): void {
    this.renderer.shadowMap.needsUpdate = true;
    this.dirty = true;
  }

  /** Milliseconds the last drawn frame took on the CPU side (GPU time is not observable). */
  lastFrameMs = 0;

  /** Draw if the camera moved or something changed; returns whether a frame was drawn. */
  render(force = false): boolean {
    const moved = this.controls.update();
    if (!moved && !this.dirty && !force) return false;
    this.dirty = false;
    const t0 = performance.now();
    this.renderer.render(this.scene, this.camera);
    this.lastFrameMs = performance.now() - t0;
    return true;
  }

  dispose(): void {
    this.pmrem.dispose();
    this.renderer.dispose();
  }
}
