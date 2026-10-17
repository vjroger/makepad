// Produces dist/skyline360.html: index.html with app.js inlined, so the app runs from a double-click.
import { readFileSync, writeFileSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
const root = path.dirname(path.dirname(fileURLToPath(import.meta.url)));
const html = readFileSync(path.join(root, 'dist/index.html'), 'utf8');
const js = readFileSync(path.join(root, 'dist/app.js'), 'utf8').replace(/<\/script>/g, '<\\/script>');
const out = html.replace('<script src="./app.js"></script>', `<script>\n${js}\n</script>`);
writeFileSync(path.join(root, 'dist/skyline360.html'), out);
console.log(`dist/skyline360.html ${(out.length / 1024).toFixed(0)} kB`);
