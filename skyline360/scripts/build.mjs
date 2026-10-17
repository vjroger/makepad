// Bundles src/app/main.ts (+ three.js, earcut) into dist/app.js and copies index.html.
// Usage: node scripts/build.mjs [--watch]
import { build, context } from 'esbuild';
import { copyFileSync, mkdirSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import path from 'node:path';

const root = path.dirname(path.dirname(fileURLToPath(import.meta.url)));
const watch = process.argv.includes('--watch');
mkdirSync(path.join(root, 'dist'), { recursive: true });
copyFileSync(path.join(root, 'src/index.html'), path.join(root, 'dist/index.html'));

const options = {
  entryPoints: [path.join(root, 'src/app/main.ts')],
  bundle: true,
  format: 'iife',
  target: ['es2022'],
  outfile: path.join(root, 'dist/app.js'),
  sourcemap: true,
  minify: !watch,
  legalComments: 'none',
  logLevel: 'info',
};

if (watch) {
  const ctx = await context(options);
  await ctx.watch();
  console.log('watching… serve dist/ with: npx serve dist');
} else {
  await build(options);
}
