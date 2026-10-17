import { chromium } from 'playwright';
import { startServer } from '../test/fixtures/server.mjs';
import { copyFile } from 'node:fs/promises';
const out = process.argv[2] || '/tmp/claude-0/-home-claude/cb2aad83-e8e0-5dcf-9991-00f7b8920ff2/scratchpad';
const srv = await startServer();
const browser = await chromium.launch({ args: ['--use-angle=swiftshader', '--enable-unsafe-swiftshader', '--ignore-gpu-blocklist', '--no-sandbox'] });
const page = await browser.newPage({ viewport: { width: 1500, height: 900 }, acceptDownloads: true });
await page.goto(`${srv.base}/index.html?tileUrl=${encodeURIComponent(srv.base + '/terrain/{z}/{x}/{y}.png')}&overpass=${encodeURIComponent(srv.base + '/overpass')}&nocache=1`);
await page.fill('#lat', '52'); await page.fill('#lon', '5');
await page.selectOption('#width', '2048'); await page.selectOption('#maxDistance', '50000');
await page.click('#render');
await page.waitForFunction(() => document.getElementById('status').textContent.startsWith('done'), null, { timeout: 500000 });
await page.hover('#preview', { position: { x: 900, y: 240 } });
await page.screenshot({ path: out + '/ui.png' });
await page.check('#compositeToggle');
await page.screenshot({ path: out + '/ui-composite.png' });
for (const [sel, name] of [['#dl-rgba', 'rgba.png'], ['#dl-sdf', 'sdf.png'], ['#dl-normal', 'normal.png'], ['#dl-svg', 'bands.svg']]) {
  const [dl] = await Promise.all([page.waitForEvent('download'), page.click(sel)]);
  await copyFile(await dl.path(), out + '/' + name);
}
console.log(await page.textContent('#stats'));
await browser.close(); await srv.close();
