// Worker: fetches terrain + buildings, builds the tiers, optionally runs the CPU sweep.
// Messages in:  { type: 'prepare', params, hooks: { tileUrl, overpassUrls, useCache }, render: 'gpu' | 'cpu' }
// Messages out: { type: 'progress', ... } | { type: 'scene', scene } | { type: 'plate', plate, sceneInfo } | { type: 'error', message }

import { prepareScene, renderCPU } from '../engine/index.js';

self.onmessage = async (ev) => {
  const msg = ev.data;
  if (msg.type !== 'prepare') return;
  try {
    const warnings = [];
    const scene = await prepareScene(msg.params, {
      ...msg.hooks,
      onProgress: (p) => self.postMessage({ type: 'progress', ...p }),
      onError: (where, err) => { warnings.push(`${where}: ${err.message}`); },
    });
    const info = { params: scene.params, eyeZ: scene.eyeZ, zGround: scene.zGround, hMax: scene.hMax, stats: scene.stats, warnings,
      tiers: scene.tiers.map((t) => ({ size: t.size, texel: t.texel, radius: t.radius, zoom: t.zoom })) };
    if (msg.render === 'cpu') {
      const plate = renderCPU(scene, (p) => self.postMessage({ type: 'progress', ...p }));
      const transfer = [plate.coverage.buffer, plate.depth.buffer, plate.classId.buffer, plate.normal.buffer, plate.altitude.buffer, plate.bands.buffer, plate.horizon.buffer, plate.horizonDistance.buffer];
      self.postMessage({ type: 'plate', plate, sceneInfo: info }, transfer);
    } else {
      const transfer = [];
      for (const t of scene.tiers) { transfer.push(t.height.buffer, t.cls.buffer); }
      self.postMessage({ type: 'scene', scene: { ...info, tierInfo: info.tiers, tiers: scene.tiers } }, transfer);
    }
  } catch (err) {
    self.postMessage({ type: 'error', message: err.message, stack: err.stack });
  }
};
