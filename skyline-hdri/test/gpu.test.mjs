// Headless-browser test: the GPU march must match the CPU sweep on the synthetic world.
// Run: node --test test/gpu.test.mjs   (uses the bundled Playwright Chromium with SwiftShader)
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { chromium } from 'playwright';
import { startServer } from './fixtures/server.mjs';

test('GPU march matches the CPU sweep', { timeout: 600000 }, async () => {
  const srv = await startServer();
  const browser = await chromium.launch({
    args: ['--use-angle=swiftshader', '--enable-unsafe-swiftshader', '--ignore-gpu-blocklist', '--enable-webgl', '--no-sandbox'],
  });
  try {
    const page = await browser.newPage();
    page.on('console', (m) => { if (process.env.VERBOSE) console.log('[page]', m.text()); });
    page.on('pageerror', (e) => console.log('[pageerror]', e.message));
    await page.goto(`${srv.base}/test/gpu-test.html?width=512&maxDistance=60000`);
    await page.waitForFunction(() => window.result !== null, null, { timeout: 580000 });
    const r = await page.evaluate(() => window.result);
    console.log(JSON.stringify(r, null, 1));
    assert.ok(!r.error, r.error);
    assert.ok(r.both > r.n * 0.4, 'roughly half the sphere is covered');
    assert.ok(r.covMismatch <= r.n * 0.002, `coverage mismatches ${r.covMismatch} of ${r.n}`);
    assert.ok(r.clsMismatch <= r.both * 0.002, `class mismatches ${r.clsMismatch}`);
    assert.ok(r.depthRelMedian < 1e-3, `median depth error ${r.depthRelMedian}`);
    assert.ok(r.depthRel99 < 0.02, `99th pct depth error ${r.depthRel99}`);
    assert.ok(r.normalDeg99 < 5, `99th pct normal error ${r.normalDeg99} deg`);
    assert.ok(r.bandErrDeg < 0.02, `band angle error ${r.bandErrDeg} deg`);
  } finally {
    await browser.close();
    await srv.close();
  }
});
