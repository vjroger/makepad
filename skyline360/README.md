# Skyline 360

Pick a real place, build what is visible there as a 3D scene (buildings, trees, water, roads, hills and distant mountains), put a camera inside it and render a **360° HDRI** (equirectangular, linear radiance) for image-based lighting in Makepad, Blender, Notch, TouchDesigner or three.js.

This is the web prototype described in the *Skyline 360 Build Plan*. Its `src/core/` is plain TypeScript with no DOM or three.js dependency, written to port function by function to the `skyline-core` Rust crate of the Makepad app.

## Run it

```bash
npm install
npm run build          # bundles src/ → dist/app.js + dist/index.html, and the single-file dist/skyline360.html
npx serve dist         # or: python3 -m http.server -d dist 5173
```

Open the page, keep the sample city or pick a real area:

1. **Area** – search a place, click the map to move the centre, set the radius (0.5–5 km), press *Generate from map*. Buildings and ground classes come from OpenStreetMap through the Overpass API; terrain from Mapterhorn with AWS Terrain Tiles as fallback; distant terrain (to 60 km) from the same sources at low zoom.
2. **Viewpoint** – shift-click the map or drag the orange pin, set eye height and the heading that lands in the centre of the image.
3. **Sun and sky** – azimuth, elevation, turbidity, sun intensity (multiples of the clear-sky zenith, which is ≈ 1.0).
4. **Render 360°** – 2k, 4k or 8k. Six half-float cube faces are rendered from the eye and folded into an equirectangular float target; the sun disc is added analytically where the sky mask says sky.
5. **Export** – `.hdr` (Radiance RGBE), `.exr` (half or float, uncompressed), PNG preview, the model as `.glb`, and a JSON sidecar plus a sources file with the exact data credits.

`dist/skyline360.html` is a single-file build: double-click it in Chrome and it runs without a server (the map library loads from a CDN and the data hosts allow cross-origin requests).

## Conventions

* Scene frame: x east, y up, z south (three.js). Core functions take local metres (east, north, up) around the area centre.
* HDRI: 2:1 equirectangular, row 0 = zenith, centre column = the chosen heading, columns run clockwise (east is right of north). Linear Rec.709 primaries, no tone mapping. Peak values reach the sun intensity (default 100 000); tick *Clamp to half-float range* for half-float EXR consumers.
* Radiance scale: clear-sky zenith at midday ≈ 1.0 (Preetham model divided by 8 kcd/m²).

## Tests

```bash
npm test               # core maths: projection, parsing, extrusion, sky, RGBE/EXR, equirect, land cover, pipeline
npm run build && npm run test:render   # headless Chromium render of the sample city with geometric checks
```

The render test needs Playwright's Chromium (`npx playwright install chromium`). It writes `test-output/sample.hdr` and tone-mapped PNGs.

## Layout

```
src/core/      pure TypeScript (ports to Rust)
  geo.ts       Web Mercator tiles, local tangent frame
  overpass.ts  Overpass query + parser → footprints, ground polygons, roads
  terrarium.ts terrain tile decoding and height grids
  extrude.ts   footprint extrusion (earcut), height rules
  terrain.ts   terrain mesh, skirt, horizon grid, far ring
  landcover.ts ground-class raster (green / water / road) painted onto the terrain
  trees.ts     tree instances from the land-cover density
  sky.ts       Preetham daylight model (CPU + GLSL), sun transmittance, scene lighting
  equirect.ts  equirect ↔ direction maths (CPU + GLSL)
  rgbe.ts      Radiance .hdr encoder/decoder (RLE)
  exr.ts       minimal OpenEXR writer (half/float, no compression, string attributes)
  demo.ts      deterministic sample city, hill, wood, river and distant ridge
  pipeline.ts  area data + terrain → meshes
src/render/    three.js scene, capture pass, previews
src/app/       UI, fetchers (Overpass, terrain tiles, Photon), export
test/          unit tests and the headless render test
```

## Known limits

* Overpass is a shared community service: generate sparingly, cache is on. Production should read Overture PMTiles or a self-hosted extract.
* Building heights are real where OSM has `height` or `building:levels`; otherwise estimated by building type. Coastal sea is not a polygon in OSM, so it renders as land.
* Trees are placed statistically from land-use polygons (forest dense, parks sparse), not from canopy data.
* The map picker and live data path need internet access and were written against the MapLibre GL JS 5 API; the geometry, lighting and HDRI pipeline are verified by the headless test on the sample city.
