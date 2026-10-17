/**
 * Headless end-to-end render of the sample city: builds the scene in Chromium
 * (SwiftShader WebGL), renders a 1024×512 HDRI, decodes it in Node and checks
 * the geometry of the panorama: horizon in the middle, sun where the sky
 * parameters put it, the landmark tower at its bearing, sky above it.
 *
 * Run: npm run build && npm run test:render
 * Writes test-output/{view3d,pano}.png, test-output/sample.hdr and sample_tonemapped.png.
 */
import { chromium } from 'playwright';
import http from 'node:http';
import { readFileSync, mkdirSync, writeFileSync, existsSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { decodeHdr } from '../src/core/rgbe';
import { dirFromUV, uvFromDir } from '../src/core/equirect';
import { sunDirection, sunTransmittance } from '../src/core/sky';
import { acesTonemap } from '../src/render/preview';
import { DEMO_TOWER, DEMO_VIEWPOINT } from '../src/core/demo';

const root = path.dirname(path.dirname(fileURLToPath(import.meta.url)));
const dist = path.join(root, 'dist');
const outDir = path.join(root, 'test-output');
mkdirSync(outDir, { recursive: true });

function serve(): Promise<{ url: string; close: () => void }> {
  const types: Record<string, string> = { '.html': 'text/html', '.js': 'text/javascript', '.map': 'application/json' };
  const server = http.createServer((req, res) => {
    const file = path.join(dist, req.url === '/' || req.url?.startsWith('/?') ? 'index.html' : (req.url ?? '').split('?')[0]);
    if (!existsSync(file)) {
      res.writeHead(404);
      res.end();
      return;
    }
    res.writeHead(200, { 'content-type': types[path.extname(file)] ?? 'application/octet-stream' });
    res.end(readFileSync(file));
  });
  return new Promise((resolve) => {
    server.listen(0, '127.0.0.1', () => {
      const addr = server.address() as { port: number };
      resolve({ url: `http://127.0.0.1:${addr.port}/`, close: () => server.close() });
    });
  });
}

function assert(cond: unknown, msg: string): void {
  if (!cond) throw new Error(`FAIL: ${msg}`);
  console.log(`ok   ${msg}`);
}

async function main(): Promise<void> {
  const { url, close } = await serve();
  const browser = await chromium.launch({
    args: ['--use-gl=angle', '--use-angle=swiftshader', '--enable-unsafe-swiftshader', '--ignore-gpu-blocklist', '--enable-webgl'],
  });
  const page = await browser.newPage({ viewport: { width: 1200, height: 760 } });
  page.on('console', (m) => {
    if (m.type() === 'error' || m.text().includes('[skyline360]')) console.log('  browser:', m.text());
  });
  page.on('pageerror', (e) => console.log('  pageerror:', e.message));
  await page.goto(`${url}?demo=1`, { waitUntil: 'load' });
  await page.waitForFunction(() => (window as any).__skyline?.state?.meshes, null, { timeout: 60000 });

  const stats = await page.evaluate(() => {
    const s = (window as any).__skyline.state.meshes.stats;
    return { count: s.count, triangles: s.triangles, maxHeight: s.maxHeight };
  });
  console.log('scene stats', stats);
  assert(stats.count > 300, `sample city has ${stats.count} buildings`);

  await page.evaluate(() => (window as any).__skyline.showTab('view3d'));
  await page.waitForTimeout(800);
  await page.screenshot({ path: path.join(outDir, 'view3d.png'), timeout: 120000 });

  const sky = { sunAzimuth: 135, sunElevation: 38, turbidity: 2.5, sunIntensity: 100000 };
  await page.evaluate((p) => (window as any).__skyline.setSky(p), sky);
  await page.evaluate(() => (window as any).__skyline.setViewpoint(2, 0));
  const t0 = Date.now();
  const r = await page.evaluate(async () => (window as any).__skyline.render(1024));
  console.log(`render ${r.width}×${r.height} in ${((Date.now() - t0) / 1000).toFixed(1)} s, peak ${r.max.toExponential(2)}`);
  await page.waitForTimeout(500);
  await page.screenshot({ path: path.join(outDir, 'pano.png'), timeout: 120000 });
  await page.click('#lookAround');
  await page.waitForTimeout(1500);
  await page.screenshot({ path: path.join(outDir, 'pano_sphere.png'), timeout: 120000 });
  await page.click('#lookAround');

  const b64 = await page.evaluate(() => {
    const bytes = (window as any).__skyline.hdrBytes() as Uint8Array;
    let s = '';
    for (let i = 0; i < bytes.length; i += 0x8000) s += String.fromCharCode.apply(null, Array.from(bytes.subarray(i, i + 0x8000)));
    return btoa(s);
  });
  const hdrBytes = Buffer.from(b64, 'base64');
  writeFileSync(path.join(outDir, 'sample.hdr'), hdrBytes);
  const img = decodeHdr(new Uint8Array(hdrBytes));
  assert(img.width === 1024 && img.height === 512, `decoded ${img.width}×${img.height} .hdr (${(hdrBytes.length / 1e6).toFixed(2)} MB)`);

  const px = (u: number, v: number): [number, number, number] => {
    const x = Math.min(img.width - 1, Math.floor(u * img.width));
    const y = Math.min(img.height - 1, Math.floor(v * img.height));
    const o = (y * img.width + x) * 3;
    return [img.data[o], img.data[o + 1], img.data[o + 2]];
  };
  const lum = (c: [number, number, number]): number => 0.2126 * c[0] + 0.7152 * c[1] + 0.0722 * c[2];

  // Tone-mapped PNG for a visual check.
  const sharp = (await import('sharp')).default;
  const raw = Buffer.alloc(img.width * img.height * 3);
  for (let i = 0; i < img.data.length; i++) {
    const v = acesTonemap(img.data[i] * 1.0);
    raw[i] = Math.round((v <= 0.0031308 ? 12.92 * v : 1.055 * Math.pow(v, 1 / 2.4) - 0.055) * 255);
  }
  await sharp(raw, { raw: { width: img.width, height: img.height, channels: 3 } }).png().toFile(path.join(outDir, 'sample_tonemapped.png'));
  console.log(`wrote ${outDir}/sample_tonemapped.png`);

  // 1. Zenith row is sky: blue and around unit radiance.
  const zenith = px(0.25, 0.01);
  assert(zenith[2] > zenith[0] && lum(zenith) > 0.4 && lum(zenith) < 3, `zenith is blue sky, luminance ${lum(zenith).toFixed(2)}`);

  // 2. Nadir is the street: lit, but dimmer than the sky (a shadowed grey road under a blue sky is bluish too).
  const nadir = px(0.25, 0.99);
  assert(lum(nadir) > 0.005 && lum(nadir) < lum(zenith), `nadir is ground, luminance ${lum(nadir).toFixed(3)}`);

  // 3. Sky mask: at the sun's position the cube alpha must read as sky (0), and the sun disc must be there.
  const [sunU, sunV] = uvFromDir(sunDirection(sky.sunAzimuth, sky.sunElevation), 0);
  const atSun = (await page.evaluate(([u, v]) => (window as any).__skyline.samplePixel(u, v), [sunU, sunV])) as number[];
  console.log('pixel at sun position (r,g,b,alpha):', atSun.map((c) => c.toExponential(2)).join(' '));
  assert(atSun[3] < 0.5, `sky mask alpha ${atSun[3]} marks the sun direction as sky`);

  // 4. Brightest pixel is the sun, at the configured azimuth/elevation, with the model's radiance.
  let maxL = 0, maxI = 0;
  for (let i = 0; i < img.data.length; i += 3) {
    const l = img.data[i] + img.data[i + 1] + img.data[i + 2];
    if (l > maxL) {
      maxL = l;
      maxI = i / 3;
    }
  }
  const su = (maxI % img.width + 0.5) / img.width;
  const sv = (Math.floor(maxI / img.width) + 0.5) / img.height;
  const sdir = dirFromUV(su, sv, 0);
  const sunAz = ((Math.atan2(sdir[0], -sdir[2]) * 180) / Math.PI + 360) % 360;
  const sunEl = (Math.asin(sdir[1]) * 180) / Math.PI;
  assert(Math.abs(sunAz - sky.sunAzimuth) < 1.5 && Math.abs(sunEl - sky.sunElevation) < 1.5, `sun found at az ${sunAz.toFixed(1)}° el ${sunEl.toFixed(1)}° (expected ${sky.sunAzimuth}°, ${sky.sunElevation}°)`);
  const expectedSun = sunTransmittance(sky.sunElevation, sky.turbidity)[1] * sky.sunIntensity;
  const sunG = img.data[maxI * 3 + 1];
  assert(sunG > expectedSun * 0.3 && sunG < expectedSun * 1.5, `sun radiance ${sunG.toExponential(2)} ≈ ${expectedSun.toExponential(2)} (limb darkening allowed)`);

  // 5. Looking south-east from the park: sky 20° up, grass 20° down; the landmark tower to the north-west is
  //    solid 5° up and sky 60° up (checked through the sky mask so facade colour does not matter).
  const [seU] = uvFromDir(sunDirection(135, 0), 0);
  const skySE = px(seU, 0.5 - 20 / 180);
  const grass = px(seU, 0.5 + 45 / 180);
  const [wU] = uvFromDir(sunDirection(270, 0), 0);
  const road = px(wU, 0.5 + 10 / 180); // the north–south street 11 m west of the viewpoint
  assert(skySE[2] > skySE[0] * 1.2, `sky 20° up to the south-east is blue (${skySE.map((c) => c.toFixed(3)).join(', ')})`);
  assert(grass[1] > grass[0] && grass[1] > grass[2], `park grass 45° down is green (${grass.map((c) => c.toFixed(3)).join(', ')})`);
  assert(Math.abs(road[0] - road[1]) < 0.05 && road[0] > grass[0], `street 10° down to the west is grey asphalt (${road.map((c) => c.toFixed(3)).join(', ')})`);
  const de = DEMO_TOWER.east - DEMO_VIEWPOINT[0], dn = DEMO_TOWER.north - DEMO_VIEWPOINT[1];
  const bearing = ((Math.atan2(de, dn) * 180) / Math.PI + 360) % 360;
  const [tu] = uvFromDir(sunDirection(bearing, 0), 0);
  const low = (await page.evaluate(([u, v]) => (window as any).__skyline.samplePixel(u, v), [tu, 0.5 - 5 / 180])) as number[];
  const high = (await page.evaluate(([u, v]) => (window as any).__skyline.samplePixel(u, v), [tu, 0.5 - 60 / 180])) as number[];
  assert(low[3] > 0.5, `buildings fill the view 5° up towards the tower (bearing ${bearing.toFixed(1)}°)`);
  assert(high[3] < 0.5, `sky 60° up towards the tower`);

  // 6. Edge continuity (v = 0.5) away from buildings is much brighter than the nadir and roughly continuous: no seam at u = 0/1.
  const left = px(0.001, 0.02), right = px(0.999, 0.02);
  assert(Math.abs(lum(left) - lum(right)) / Math.max(lum(left), 1e-6) < 0.1, `no seam between the left and right edges (${lum(left).toFixed(3)} vs ${lum(right).toFixed(3)})`);

  // 7. From 80 m up, the 1,400 m ridge 35 km to the north-east shows on the horizon (bearing 65°).
  await page.evaluate(() => (window as any).__skyline.setViewpoint(80, 0));
  await page.evaluate(async () => (window as any).__skyline.render(1024));
  const dirAt = (bearing: number, el: number): [number, number] => uvFromDir(sunDirection(bearing, el), 0);
  const ridge = (await page.evaluate(([u, v]) => (window as any).__skyline.samplePixel(u, v), dirAt(65, 1.0))) as number[];
  const aboveRidge = (await page.evaluate(([u, v]) => (window as any).__skyline.samplePixel(u, v), dirAt(65, 3.5))) as number[];
  const openHorizon = (await page.evaluate(([u, v]) => (window as any).__skyline.samplePixel(u, v), dirAt(200, 1.0))) as number[];
  assert(ridge[3] > 0.5, `ridge is solid 1° above the horizon at bearing 65° (${ridge.slice(0, 3).map((c) => c.toFixed(3)).join(', ')})`);
  assert(aboveRidge[3] < 0.5, 'sky 3.5° above the horizon at bearing 65°');
  assert(openHorizon[3] < 0.5, 'sky 1° above the horizon to the south, where there are no mountains');

  await browser.close();
  close();
}

main().catch((err) => {
  console.error(err);
  process.exit(1);
});
