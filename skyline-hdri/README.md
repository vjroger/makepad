# skyline-hdri

Generate a 360° equirectangular **skyline plate** — terrain and buildings opaque, sky transparent — for any
viewpoint on Earth, to composite over a skybox / HDRI. Worldwide, keyless, fully client-side.

* Terrain: [AWS Terrain Tiles](https://registry.opendata.aws/terrain-tiles) (Mapzen terrarium PNGs, zoom 0–15).
* Buildings: OpenStreetMap footprints + heights through the Overpass API (`height`, `building:levels`,
  towers/masts/chimneys), with per-type defaults where no height is tagged.
* Engine: a per-pixel heightfield ray march in a WebGL2 fragment shader (production shape, ports to Makepad),
  plus a CPU column-sweep reference that produces the identical plate (used by the tests and as the fallback).
* Exports: RGBA PNG (alpha = coverage), flat mask, 16-bit depth, normals, SDF, float EXR (all layers), SVG
  horizon bands (also CNC/laser ready), JSON metadata.

## Run

The page must come over HTTP (ES modules + a Worker are blocked on `file://`). Any static server works;
the Rust CLI has one built in, so with only Rust installed:

```sh
cd rust && cargo build --release
./target/release/skyline --serve 8080        # Windows: .\target\release\skyline.exe --serve 8080
# open http://localhost:8080/
```

(`--root <folder>` if you run it from somewhere else.) The server also proxies the building query at
`/overpass` (server side, through `curl`), and the page tries that first: public Overpass mirrors go down
regularly and an error page or a mirror without CORS headers shows up in a browser as a bare
"Failed to fetch". Add `--overpass <url>` (repeatable) to choose the upstream mirrors.

Alternatives: `npx serve .` or `python3 -m http.server 8080` in the project folder — the page then talks to
the public mirrors directly, which works whenever `overpass-api.de` is healthy. Chrome, Firefox and Safari 17+ work; the GPU path
needs WebGL2 with `EXT_color_buffer_float` (every desktop GPU of the last decade); otherwise the CPU engine
is selected automatically.

Pick a spot on the map (or type coordinates / search a place), set the plate width, press **Render plate**.
A 4096 × 2048 plate with 250 km reach downloads ~180 terrain tiles (12–15 MB, cached by the browser) and
one Overpass query, then renders in a few seconds on a discrete GPU.

URL overrides for self-hosted data: `?tileUrl=https://host/{z}/{x}/{y}.png&overpass=https://host/api/interpreter&nocache=1`.

## Test

```sh
npm test                     # Node tests: engine on a synthetic world, exporters
node --test test/gpu.test.mjs   # headless Chromium: GPU march vs CPU sweep (needs playwright)
node --test test/ui.test.mjs    # headless Chromium: UI render + all exports
```

The synthetic world (`test/fixtures/synthetic-world.mjs`) is a flat plain with a 1500 m cone 20 km NE, a
100 m hill 100 km S (hidden by Earth curvature), sea to the W, and four buildings + a mast with known
heights; every horizon angle, depth, class and normal is checked against hand-computable values.

## Plate conventions

| | |
| --- | --- |
| Projection | equirectangular, width = 2 × height |
| x | azimuth: x = 0 → −180° (south), centre → 0° (north), ¾ W → +90° (east). Clockwise turns move right. `headingOffset` rotates. |
| y | elevation: top = +90°, middle = 0° (true horizontal), bottom = −90° |
| coverage | 0 sky, 255 occluder (alpha of the RGBA/mask PNG, `A` in EXR) |
| depth | 3-D distance eye → first hit, metres. 16-bit PNG: `v = 65535·ln(1+d)/ln(1+maxDistance)`, sky = 65535. EXR `Z` float, sky = 10 × maxDistance |
| classId | 0 sky, 1 terrain, 2 building (`id` in EXR) |
| normal | world-space (east, north, up); PNG = n·0.5+0.5; EXR `N.x N.y N.z` |
| altitude | metres above sea level of the hit (`height` in EXR) |
| SDF | signed distance to the silhouette in plate pixels, positive in the sky, 128 on the edge, ±spread → 0/255 |
| SVG | one closed polygon per distance band in pixel coordinates, far → near, light → dark |

Earth curvature and standard refraction (k = 0.13) are applied: `e = atan2(h − z_eye − d²/(2R_eff), d)` with
`R_eff = R / (1 − k)`.

## Use the engine from your own code

```js
import { prepareScene, renderCPU } from './src/engine/index.js';
import { GpuSkylineRenderer } from './src/gpu/renderer.js';

const scene = await prepareScene({ lat: 52.0907, lon: 5.1214, width: 4096, maxDistance: 250000, eyeHeight: 1.7 },
                                 { onProgress: console.log });
// GPU
const gpu = new GpuSkylineRenderer(document.createElement('canvas'));
gpu.uploadScene(scene);
const plate = await gpu.render(scene.params, { yieldBetweenStrips: true });
// or CPU (also works in Node / a Worker)
const plate2 = renderCPU(scene);

plate.width, plate.height          // 4096, 2048
plate.coverage                     // Uint8Array
plate.depth, plate.altitude        // Float32Array (metres)
plate.classId                      // Uint8Array
plate.normal                       // Float32Array, 3 per pixel
plate.horizon, plate.bands         // Float32Array per column (radians)
```

`src/engine/` has no DOM dependencies; `src/gpu/` needs WebGL2; `src/export/` are pure functions.

## Layout

```
index.html, src/app.js          UI
src/engine/geo.js               geodesy, Mercator, curvature
src/engine/tiles.js             terrarium tiles → mosaics
src/engine/tiers.js             azimuthal-equidistant height tiers, bilinear sampling, normals
src/engine/buildings.js         Overpass query/parse, height estimation, footprint rasterisation
src/engine/sweep.js             CPU column sweep (reference)
src/engine/index.js             prepareScene / renderCPU / renderSkyline
src/gpu/shaders.js              GLSL: horizon pre-pass + per-pixel march
src/gpu/renderer.js             WebGL2 driver (float MRT, strips, readback)
src/export/*.js                 PNG, EXR, layers (depth/normal/SDF/shading), SVG
src/worker/data-worker.js       data preparation off the main thread
rust/                           Rust core (same algorithm, CLI)
```

## Rust core and CLI

`rust/` is a Cargo workspace with **no external dependencies** (own inflate/PNG, JSON, HTTP via sockets +
`curl` for https), so it builds anywhere:

```sh
cd rust && cargo build --release
./target/release/skyline --lat 51.909 --lon 4.488 --width 4096 --max-km 250 --out ./out \
    --formats rgba,depth,normal,sdf,exr,svg,json
```

`skyline --help` lists every option (eye height, absolute altitude, heading, exaggeration, building radii,
tile URL, Overpass mirrors, cache directory, threads). `test/rust-crosscheck.test.mjs` renders the same
synthetic scene with the CLI and the JavaScript sweep and requires them to match bit for bit.

## Porting to Makepad (Rust + shaders)

See `rust/MAKEPAD-PORT.md` for the piece-by-piece mapping and a shader sketch.


The GLSL in `src/gpu/shaders.js` is plain arithmetic + `texelFetch`; `sampleTier`, `heightAt`, `tierFor`,
`stepSize` and the march loop translate line by line into Makepad's shader language. The host side
(`src/engine/*`) is written with typed arrays and no DOM so it transliterates to Rust; the `rust/` crate is
that port, sharing the test fixture so both can be compared.

## Data attribution

Terrain tiles: Mapzen / Tilezen, built from USGS 3DEP, SRTM, Copernicus EU-DEM, ArcticDEM, ETOPO1 and
national datasets — see the [attribution list](https://github.com/tilezen/joerd/blob/master/docs/attribution.md).
Buildings: © OpenStreetMap contributors, ODbL. Overpass: please respect the mirrors' usage policies.

## Known limits (v0.1)

* Terrain tiles in cities are partly SRTM-derived and can carry a few metres of building bias; the eye's
  "ground" and building bases follow the tile, so absolute altitudes can be off by that much.
* Building facades are rasterised into the tiers (0.5 m near the eye, 2 m to 2 km, 10 m to 10 km), so
  very close walls show texel steps; analytic edges for near buildings are on the roadmap.
* No trees, bridges, cranes or terrain detail below ~3 m/px; no building height data outside OSM.
* Overpass can take tens of seconds in dense cities and may rate-limit; the three mirrors are tried in turn.
