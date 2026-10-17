// UI smoke test in headless Chromium against the mock server: render, preview, every export.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { chromium } from 'playwright';
import { startServer } from './fixtures/server.mjs';
import { readFile } from 'node:fs/promises';

test('UI renders a plate and all exports download', { timeout: 600000 }, async () => {
  const srv = await startServer();
  const browser = await chromium.launch({ args: ['--use-angle=swiftshader', '--enable-unsafe-swiftshader', '--ignore-gpu-blocklist', '--no-sandbox'] });
  try {
    const page = await browser.newPage({ acceptDownloads: true });
    const errors = [];
    page.on('pageerror', (e) => errors.push(e.message));
    page.on('console', (m) => { if (m.type() === 'error' && process.env.VERBOSE) console.log('[console]', m.text()); });
    await page.goto(`${srv.base}/index.html?tileUrl=${encodeURIComponent(srv.base + '/terrain/{z}/{x}/{y}.png')}&overpass=${encodeURIComponent(srv.base + '/overpass')}&nocache=1`);
    await page.fill('#lat', '52'); await page.fill('#lon', '5');
    await page.selectOption('#width', '1024');
    await page.selectOption('#maxDistance', '50000');
    await page.click('#render');
    await page.waitForFunction(() => document.getElementById('status').textContent.startsWith('done'), null, { timeout: 500000 });
    const stats = await page.textContent('#stats');
    console.log(stats);
    assert.match(stats, /buildings 4 rasterised/);
    assert.match(stats, /1 skipped/);
    // the preview canvas has sky (alpha 0) at the top and ground at the bottom
    const px = await page.evaluate(() => {
      const c = document.getElementById('preview'); const ctx = c.getContext('2d');
      const top = ctx.getImageData(10, 2, 1, 1).data, bottom = ctx.getImageData(10, c.height - 2, 1, 1).data;
      return { top: [...top], bottom: [...bottom], w: c.width, h: c.height };
    });
    assert.equal(px.w, 1024); assert.equal(px.h, 512);
    assert.equal(px.top[3], 0, 'sky transparent'); assert.equal(px.bottom[3], 255, 'ground opaque');
    // hover readout
    await page.hover('#preview', { position: { x: 300, y: 100 } });
    const readout = await page.textContent('#readout');
    assert.match(readout, /az .*el .*/);
    // exports
    const expect = [
      ['#dl-rgba', /_rgba\.png$/, [137, 80, 78, 71]],
      ['#dl-mask', /_mask\.png$/, [137, 80, 78, 71]],
      ['#dl-depth', /_depth16\.png$/, [137, 80, 78, 71]],
      ['#dl-normal', /_normal\.png$/, [137, 80, 78, 71]],
      ['#dl-sdf', /_sdf64\.png$/, [137, 80, 78, 71]],
      ['#dl-exr', /\.exr$/, [0x76, 0x2f, 0x31, 0x01]],
      ['#dl-exr-compact', /_az\.exr$/, [0x76, 0x2f, 0x31, 0x01]],
      ['#dl-svg', /_bands\.svg$/, [0x3c, 0x3f, 0x78, 0x6d]],
      ['#dl-json', /\.json$/, [0x7b]],
    ];
    for (const [sel, namePattern, magic] of expect) {
      const [dl] = await Promise.all([page.waitForEvent('download', { timeout: 120000 }), page.click(sel)]);
      const name = dl.suggestedFilename();
      assert.match(name, namePattern);
      const path = await dl.path();
      const bytes = await readFile(path);
      assert.ok(bytes.length > 100, `${name} has ${bytes.length} bytes`);
      assert.deepEqual([...bytes.subarray(0, magic.length)], magic, `${name} magic`);
      console.log(name, bytes.length, 'bytes');
    }
    assert.deepEqual(errors, []);
  } finally {
    await browser.close(); await srv.close();
  }
});
