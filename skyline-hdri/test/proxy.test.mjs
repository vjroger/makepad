// The Rust server's /overpass proxy: the browser app, served by `skyline --serve`, gets its buildings
// through the proxy (upstream = the mock Overpass here) with no CORS dependency on the mirror.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { spawn, spawnSync } from 'node:child_process';
import { existsSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { chromium } from 'playwright';
import { startServer } from './fixtures/server.mjs';

const ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');

test('skyline --serve proxies Overpass and serves the app', { timeout: 600000 }, async (t) => {
  if (spawnSync('cargo', ['--version']).status !== 0) { t.skip('cargo not available'); return; }
  assert.equal(spawnSync('cargo', ['build', '--release', '--quiet'], { cwd: path.join(ROOT, 'rust'), stdio: 'inherit' }).status, 0);
  const bin = path.join(ROOT, 'rust/target/release/skyline');
  assert.ok(existsSync(bin));
  const mock = await startServer();
  const port = 8090 + Math.floor(Math.random() * 500);
  const srv = spawn(bin, ['--serve', String(port), '--root', ROOT, '--overpass', 'http://127.0.0.1:1/dead', '--overpass', `${mock.base}/overpass`], { stdio: ['ignore', 'ignore', 'pipe'] });
  let log = '';
  srv.stderr.on('data', (d) => { log += d; });
  await new Promise((r) => setTimeout(r, 800));
  try {
    // 1. direct proxy call: the dead mirror is skipped, the mock answers
    const r = await fetch(`http://127.0.0.1:${port}/overpass`, { method: 'POST', headers: { 'Content-Type': 'application/x-www-form-urlencoded' }, body: 'data=' + encodeURIComponent('[out:json];node(1);out;') });
    assert.equal(r.status, 200);
    assert.equal(r.headers.get('access-control-allow-origin'), '*');
    const j = await r.json();
    assert.equal(j.elements.length, 5);
    // 2. the app through the Rust server, 'auto' mirror mode -> same-origin /overpass
    const browser = await chromium.launch({ args: ['--use-angle=swiftshader', '--enable-unsafe-swiftshader', '--ignore-gpu-blocklist', '--no-sandbox'] });
    try {
      const page = await browser.newPage();
      await page.goto(`http://127.0.0.1:${port}/index.html?tileUrl=${encodeURIComponent(mock.base + '/terrain/{z}/{x}/{y}.png')}&nocache=1`);
      await page.fill('#lat', '52'); await page.fill('#lon', '5');
      await page.selectOption('#width', '1024'); await page.selectOption('#maxDistance', '50000');
      await page.click('#render');
      await page.waitForFunction(() => document.getElementById('status').textContent.startsWith('done') || document.getElementById('status').textContent.startsWith('failed'), null, { timeout: 500000 });
      const stats = await page.textContent('#stats');
      console.log(stats);
      assert.match(stats, /buildings 4 rasterised/);
      assert.ok(!/⚠/.test(stats), 'no warnings');
    } finally { await browser.close(); }
    assert.match(log, /overpass proxy: .* ok/);
  } finally {
    srv.kill();
    await mock.close();
  }
});
