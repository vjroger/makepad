// WebGL2 renderer for the per-pixel march. Produces the same plate layout as the CPU sweep.

import { VERT, FRAG_HORIZON, FRAG_MARCH, MAX_TIERS } from './shaders.js';
import { EFFECTIVE_RADIUS, DEG } from '../engine/geo.js';
import { allocatePlate } from '../engine/sweep.js';

function compile(gl, type, src) {
  const s = gl.createShader(type);
  gl.shaderSource(s, src);
  gl.compileShader(s);
  if (!gl.getShaderParameter(s, gl.COMPILE_STATUS)) {
    const log = gl.getShaderInfoLog(s);
    gl.deleteShader(s);
    throw new Error('shader compile failed: ' + log);
  }
  return s;
}

function link(gl, vsSrc, fsSrc) {
  const p = gl.createProgram();
  gl.attachShader(p, compile(gl, gl.VERTEX_SHADER, vsSrc));
  gl.attachShader(p, compile(gl, gl.FRAGMENT_SHADER, fsSrc));
  gl.linkProgram(p);
  if (!gl.getProgramParameter(p, gl.LINK_STATUS)) throw new Error('program link failed: ' + gl.getProgramInfoLog(p));
  return p;
}

function createFloatTexture(gl, width, height, internal, format, type, data = null) {
  const tex = gl.createTexture();
  gl.bindTexture(gl.TEXTURE_2D, tex);
  gl.pixelStorei(gl.UNPACK_ALIGNMENT, 1);
  gl.texImage2D(gl.TEXTURE_2D, 0, internal, width, height, 0, format, type, data);
  gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MIN_FILTER, gl.NEAREST);
  gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MAG_FILTER, gl.NEAREST);
  gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_WRAP_S, gl.CLAMP_TO_EDGE);
  gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_WRAP_T, gl.CLAMP_TO_EDGE);
  return tex;
}

export class GpuSkylineRenderer {
  /** canvas: HTMLCanvasElement | OffscreenCanvas (any size; only used for the context). */
  constructor(canvas) {
    const gl = canvas.getContext('webgl2', { antialias: false, depth: false, preserveDrawingBuffer: false, premultipliedAlpha: false });
    if (!gl) throw new Error('WebGL2 is not available');
    if (!gl.getExtension('EXT_color_buffer_float')) throw new Error('EXT_color_buffer_float is not available (needed for float render targets)');
    this.gl = gl;
    this.maxTex = gl.getParameter(gl.MAX_TEXTURE_SIZE);
    this.progHorizon = link(gl, VERT, FRAG_HORIZON);
    this.progMarch = link(gl, VERT, FRAG_MARCH);
    this.vao = gl.createVertexArray();
    this.tierTex = [];
    this.scene = null;
  }

  static isSupported() {
    try {
      const c = typeof OffscreenCanvas !== 'undefined' ? new OffscreenCanvas(1, 1) : document.createElement('canvas');
      const gl = c.getContext('webgl2');
      return !!(gl && gl.getExtension('EXT_color_buffer_float'));
    } catch { return false; }
  }

  /** Upload the scene's tiers as R32F height + R8 class textures. */
  uploadScene(scene) {
    const gl = this.gl;
    this.disposeTiers();
    if (scene.tiers.length > MAX_TIERS) throw new Error(`at most ${MAX_TIERS} tiers`);
    for (const t of scene.tiers) {
      if (t.size > this.maxTex) throw new Error(`tier size ${t.size} exceeds MAX_TEXTURE_SIZE ${this.maxTex}`);
      const h = createFloatTexture(gl, t.size, t.size, gl.R32F, gl.RED, gl.FLOAT, t.height);
      const c = createFloatTexture(gl, t.size, t.size, gl.R8, gl.RED, gl.UNSIGNED_BYTE, t.cls);
      this.tierTex.push({ h, c });
    }
    this.scene = scene;
  }

  disposeTiers() {
    for (const t of this.tierTex) { this.gl.deleteTexture(t.h); this.gl.deleteTexture(t.c); }
    this.tierTex = [];
  }

  dispose() {
    this.disposeTiers();
    const gl = this.gl;
    gl.deleteProgram(this.progHorizon); gl.deleteProgram(this.progMarch); gl.deleteVertexArray(this.vao);
    gl.getExtension('WEBGL_lose_context')?.loseContext();
  }

  _setCommonUniforms(prog, params) {
    const gl = this.gl, scene = this.scene;
    const u = (n) => gl.getUniformLocation(prog, n);
    const tiers = scene.tiers;
    const sizes = new Float32Array(MAX_TIERS), texels = new Float32Array(MAX_TIERS), limits = new Float32Array(MAX_TIERS);
    for (let k = 0; k < tiers.length; k++) { sizes[k] = tiers[k].size; texels[k] = tiers[k].texel; limits[k] = tiers[k].radius - 2 * tiers[k].texel; }
    gl.uniform1i(u('uTierCount'), tiers.length);
    gl.uniform1fv(u('uTierSize'), sizes);
    gl.uniform1fv(u('uTierTexel'), texels);
    gl.uniform1fv(u('uTierLimit'), limits);
    for (let k = 0; k < MAX_TIERS; k++) {
      const t = this.tierTex[Math.min(k, this.tierTex.length - 1)];
      gl.activeTexture(gl.TEXTURE0 + k);
      gl.bindTexture(gl.TEXTURE_2D, t.h);
      gl.uniform1i(u('uH' + k), k);
      gl.activeTexture(gl.TEXTURE0 + MAX_TIERS + k);
      gl.bindTexture(gl.TEXTURE_2D, t.c);
      gl.uniform1i(u('uC' + k), MAX_TIERS + k);
    }
    const V = params.verticalExaggeration ?? 1;
    gl.uniform2f(u('uPlate'), params.width, params.width / 2);
    gl.uniform1f(u('uEyeZ'), scene.eyeZ);
    gl.uniform1f(u('uZGround'), scene.zGround);
    gl.uniform1f(u('uVertExag'), V);
    gl.uniform1f(u('uHeading'), (params.headingOffset || 0) * DEG);
    gl.uniform1f(u('uMaxDist'), params.maxDistance);
    gl.uniform1f(u('uHMax'), scene.zGround + (scene.hMax - scene.zGround) * V);
    gl.uniform1f(u('uInvR2'), 1 / (2 * EFFECTIVE_RADIUS));
    gl.uniform1f(u('uDelta'), 2 * Math.PI / params.width);
  }

  /**
   * Render the plate. params: the merged engine params (width, maxDistance, headingOffset, bands, ...).
   * opts.stripRows: rows per draw call (keeps each draw short for GPU watchdogs).
   * opts.yieldBetweenStrips: await a macrotask between strips so the page can repaint progress.
   * Returns a plate with the same layout as the CPU sweep (rows top-down).
   */
  async render(params, opts = {}) {
    const gl = this.gl, scene = this.scene;
    if (!scene) throw new Error('uploadScene() first');
    const W = params.width, H = W / 2;
    if (W > this.maxTex) throw new Error(`plate width ${W} exceeds MAX_TEXTURE_SIZE ${this.maxTex}`);
    const bands = params.bands || [];
    const NB = bands.length + 1;                 // last row = full range
    const t0 = performance.now();
    const plate = allocatePlate(W, bands.length);
    gl.bindVertexArray(this.vao);
    gl.disable(gl.BLEND); gl.disable(gl.DEPTH_TEST); gl.disable(gl.SCISSOR_TEST);

    // --- pre-pass: horizon per column and band ---------------------------------------------
    const horizonTex = createFloatTexture(gl, W, NB, gl.RGBA32F, gl.RGBA, gl.FLOAT);
    const fboH = gl.createFramebuffer();
    gl.bindFramebuffer(gl.FRAMEBUFFER, fboH);
    gl.framebufferTexture2D(gl.FRAMEBUFFER, gl.COLOR_ATTACHMENT0, gl.TEXTURE_2D, horizonTex, 0);
    gl.drawBuffers([gl.COLOR_ATTACHMENT0]);
    gl.viewport(0, 0, W, NB);
    gl.useProgram(this.progHorizon);
    this._setCommonUniforms(this.progHorizon, params);
    const radii = new Float32Array(8);
    for (let b = 0; b < bands.length; b++) radii[b] = bands[b];
    gl.uniform1i(gl.getUniformLocation(this.progHorizon, 'uBandCount'), NB);
    gl.uniform1fv(gl.getUniformLocation(this.progHorizon, 'uBandRadius'), radii);
    gl.drawArrays(gl.TRIANGLES, 0, 6);
    const hbuf = new Float32Array(W * NB * 4);
    gl.readPixels(0, 0, W, NB, gl.RGBA, gl.FLOAT, hbuf);
    for (let b = 0; b < bands.length; b++) for (let x = 0; x < W; x++) plate.bands[b * W + x] = hbuf[(b * W + x) * 4];
    for (let x = 0; x < W; x++) { plate.horizon[x] = hbuf[((NB - 1) * W + x) * 4]; plate.horizonDistance[x] = hbuf[((NB - 1) * W + x) * 4 + 1]; }
    opts.onProgress?.({ stage: 'gpu', fraction: 0.02, message: 'horizon pass' });

    // --- main pass in strips ----------------------------------------------------------------
    const stripRows = Math.max(16, opts.stripRows || Math.max(32, Math.floor((1 << 20) / W)));
    const tex0 = createFloatTexture(gl, W, stripRows, gl.RGBA32F, gl.RGBA, gl.FLOAT);
    const tex1 = createFloatTexture(gl, W, stripRows, gl.RGBA32F, gl.RGBA, gl.FLOAT);
    const fbo = gl.createFramebuffer();
    gl.bindFramebuffer(gl.FRAMEBUFFER, fbo);
    gl.framebufferTexture2D(gl.FRAMEBUFFER, gl.COLOR_ATTACHMENT0, gl.TEXTURE_2D, tex0, 0);
    gl.framebufferTexture2D(gl.FRAMEBUFFER, gl.COLOR_ATTACHMENT1, gl.TEXTURE_2D, tex1, 0);
    gl.drawBuffers([gl.COLOR_ATTACHMENT0, gl.COLOR_ATTACHMENT1]);
    if (gl.checkFramebufferStatus(gl.FRAMEBUFFER) !== gl.FRAMEBUFFER_COMPLETE) throw new Error('float MRT framebuffer incomplete');
    gl.useProgram(this.progMarch);
    this._setCommonUniforms(this.progMarch, params);
    gl.activeTexture(gl.TEXTURE0 + 2 * MAX_TIERS);
    gl.bindTexture(gl.TEXTURE_2D, horizonTex);
    gl.uniform1i(gl.getUniformLocation(this.progMarch, 'uHorizon'), 2 * MAX_TIERS);
    gl.uniform1i(gl.getUniformLocation(this.progMarch, 'uBandCount'), NB);
    const uStripY0 = gl.getUniformLocation(this.progMarch, 'uStripY0');
    const buf0 = new Float32Array(W * stripRows * 4), buf1 = new Float32Array(W * stripRows * 4);
    for (let y0 = 0; y0 < H; y0 += stripRows) {
      const rows = Math.min(stripRows, H - y0);
      gl.viewport(0, 0, W, rows);
      gl.uniform1f(uStripY0, y0);
      gl.drawArrays(gl.TRIANGLES, 0, 6);
      gl.readBuffer(gl.COLOR_ATTACHMENT0);
      gl.readPixels(0, 0, W, rows, gl.RGBA, gl.FLOAT, buf0);
      gl.readBuffer(gl.COLOR_ATTACHMENT1);
      gl.readPixels(0, 0, W, rows, gl.RGBA, gl.FLOAT, buf1);
      for (let r = 0; r < rows; r++) {
        const yPlate = y0 + r;             // bottom-up
        const yTop = H - 1 - yPlate;       // top-down row in the plate arrays
        const src = r * W, dst = yTop * W;
        for (let x = 0; x < W; x++) {
          const s = (src + x) * 4, d = dst + x;
          if (buf0[s + 3] > 0.5) {
            plate.coverage[d] = 255;
            plate.depth[d] = buf0[s];
            plate.altitude[d] = buf0[s + 1];
            plate.classId[d] = buf0[s + 2] > 1.5 ? 2 : 1;
            plate.normal[d * 3] = buf1[s]; plate.normal[d * 3 + 1] = buf1[s + 1]; plate.normal[d * 3 + 2] = buf1[s + 2];
          } else {
            plate.normal[d * 3 + 2] = 1;
          }
        }
      }
      opts.onProgress?.({ stage: 'gpu', fraction: 0.02 + 0.98 * Math.min(1, (y0 + rows) / H), message: `march ${y0 + rows}/${H} rows` });
      if (opts.yieldBetweenStrips) await new Promise((r) => setTimeout(r, 0));
    }
    gl.bindFramebuffer(gl.FRAMEBUFFER, null);
    gl.deleteFramebuffer(fbo); gl.deleteFramebuffer(fboH);
    gl.deleteTexture(tex0); gl.deleteTexture(tex1); gl.deleteTexture(horizonTex);
    plate.renderMs = performance.now() - t0;
    return plate;
  }
}
