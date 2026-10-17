// Local mock of the two data sources + static file server for the browser test.
//   GET  /terrain/{z}/{x}/{y}.png   -> synthetic terrarium tile
//   POST /overpass                  -> fixture buildings JSON
//   GET  /...                       -> project files
import http from 'node:http';
import { readFile } from 'node:fs/promises';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { renderTile, overpassFixture } from './synthetic-world.mjs';

const ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../..');
const MIME = { '.html': 'text/html', '.js': 'text/javascript', '.mjs': 'text/javascript', '.css': 'text/css', '.json': 'application/json', '.png': 'image/png', '.svg': 'image/svg+xml' };

export function startServer(port = 0) {
  const tileCache = new Map();
  let tileRequests = 0, overpassRequests = 0;
  const server = http.createServer(async (req, res) => {
    const url = new URL(req.url, 'http://localhost');
    const cors = { 'Access-Control-Allow-Origin': '*', 'Cross-Origin-Opener-Policy': 'same-origin', 'Cross-Origin-Embedder-Policy': 'require-corp' };
    try {
      let m = url.pathname.match(/^\/terrain\/(\d+)\/(\d+)\/(\d+)\.png$/);
      if (m) {
        tileRequests++;
        const key = m.slice(1).join('/');
        let png = tileCache.get(key);
        if (!png) { png = await renderTile(+m[1], +m[2], +m[3]); tileCache.set(key, png); }
        res.writeHead(200, { 'Content-Type': 'image/png', ...cors });
        res.end(Buffer.from(png));
        return;
      }
      if (url.pathname === '/overpass') {
        overpassRequests++;
        let body = '';
        for await (const c of req) body += c;
        res.writeHead(200, { 'Content-Type': 'application/json', ...cors });
        res.end(JSON.stringify(overpassFixture()));
        return;
      }
      const file = path.join(ROOT, url.pathname === '/' ? 'index.html' : url.pathname);
      if (!file.startsWith(ROOT)) { res.writeHead(403); res.end(); return; }
      const data = await readFile(file);
      res.writeHead(200, { 'Content-Type': MIME[path.extname(file)] || 'application/octet-stream', ...cors });
      res.end(data);
    } catch (err) {
      res.writeHead(404, cors);
      res.end(String(err.message));
    }
  });
  return new Promise((resolve) => {
    server.listen(port, '127.0.0.1', () => {
      const base = `http://127.0.0.1:${server.address().port}`;
      resolve({ server, base, stats: () => ({ tileRequests, overpassRequests }), close: () => new Promise((r) => server.close(r)) });
    });
  });
}

if (process.argv[1] && fileURLToPath(import.meta.url) === path.resolve(process.argv[1])) {
  startServer(+process.argv[2] || 8765).then(({ base }) => console.log('mock server at', base));
}
