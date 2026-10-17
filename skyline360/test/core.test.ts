import { test } from 'node:test';
import assert from 'node:assert/strict';
import { LocalFrame, bboxForRadius, lonLatToTile, tileRangeForBbox, tileToLonLat, ringArea } from '../src/core/geo';
import { assembleRings, buildOverpassQuery, parseLength, parseOverpass } from '../src/core/overpass';
import { decodeTerrarium, gridFromTiles, sampleHeight } from '../src/core/terrarium';
import { extrudeFootprint, resolveHeight } from '../src/core/extrude';
import { MeshBuilder } from '../src/core/mesh';
import { terrainMesh, farRing } from '../src/core/terrain';
import { computeSkyLighting, perezCoefficients, skyRadiance, sunDirection, sunTransmittance } from '../src/core/sky';
import { decodeHdr, encodeHdr } from '../src/core/rgbe';
import { encodeExr, floatToHalf } from '../src/core/exr';
import { dirFromUV, uvFromDir } from '../src/core/equirect';
import { demoScene } from '../src/core/demo';
import { buildSceneMeshes } from '../src/core/pipeline';
import { COVER_GREEN, COVER_ROAD, COVER_TERRAIN, COVER_WATER, buildLandCover, coverAt } from '../src/core/landcover';

test('local frame round-trips and scales with latitude', () => {
  const f = new LocalFrame(4.9, 52.37);
  const [e, n] = f.toLocal(4.91, 52.38);
  assert.ok(e > 600 && e < 700, `east ${e}`); // 0.01° lon at 52° ≈ 680 m
  assert.ok(n > 1100 && n < 1120, `north ${n}`); // 0.01° lat ≈ 1113 m
  const [lon, lat] = f.toLonLat(e, n);
  assert.ok(Math.abs(lon - 4.91) < 1e-9 && Math.abs(lat - 52.38) < 1e-9);
});

test('tile maths agrees with the slippy map convention', () => {
  const t = lonLatToTile(4.9, 52.37, 14);
  assert.equal(Math.floor(t.x), 8415); // (4.9 + 180) / 360 * 2^14
  assert.equal(Math.floor(t.y), 5384);
  const back = tileToLonLat(Math.floor(t.x), Math.floor(t.y), 14);
  assert.ok(back.lon <= 4.9 && back.lat >= 52.37);
  const r = tileRangeForBbox(bboxForRadius(4.9, 52.37, 1000), 14);
  assert.ok(r.xMax >= r.xMin && r.yMax >= r.yMin);
  assert.ok((r.xMax - r.xMin + 1) * (r.yMax - r.yMin + 1) <= 12);
});

test('overpass tags and relations parse', () => {
  assert.equal(parseLength('12'), 12);
  assert.equal(parseLength('12.5 m'), 12.5);
  assert.ok(Math.abs(parseLength('10 ft')! - 3.048) < 1e-9);
  assert.equal(parseLength('tall'), null);
  const q = buildOverpassQuery([4.8, 52.3, 4.9, 52.4]);
  assert.ok(q.includes('(52.3,4.8,52.4,4.9)') && q.includes('out geom'));

  const rings = assembleRings([
    [[0, 0], [1, 0]],
    [[1, 1], [0, 1], [0, 0]],
    [[1, 0], [1, 1]],
  ]);
  assert.equal(rings.length, 1);
  assert.equal(rings[0].length, 4);

  const data = parseOverpass({
    elements: [
      { type: 'way', id: 1, tags: { building: 'yes', height: '20', 'building:levels': '5' }, geometry: [{ lon: 0, lat: 0 }, { lon: 0.001, lat: 0 }, { lon: 0.001, lat: 0.001 }, { lon: 0, lat: 0.001 }, { lon: 0, lat: 0 }] },
      { type: 'way', id: 2, tags: { highway: 'primary' }, geometry: [{ lon: 0, lat: 0 }, { lon: 0.01, lat: 0 }] },
      { type: 'way', id: 3, tags: { natural: 'water' }, geometry: [{ lon: 0, lat: 0 }, { lon: 0.001, lat: 0 }, { lon: 0.001, lat: 0.001 }] },
      {
        type: 'relation',
        id: 4,
        tags: { type: 'multipolygon', building: 'apartments', 'building:levels': '6' },
        members: [
          { type: 'way', ref: 10, role: 'outer', geometry: [{ lon: 0, lat: 0 }, { lon: 0.002, lat: 0 }, { lon: 0.002, lat: 0.002 }, { lon: 0, lat: 0.002 }, { lon: 0, lat: 0 }] },
          { type: 'way', ref: 11, role: 'inner', geometry: [{ lon: 0.0005, lat: 0.0005 }, { lon: 0.001, lat: 0.0005 }, { lon: 0.001, lat: 0.001 }, { lon: 0.0005, lat: 0.001 }, { lon: 0.0005, lat: 0.0005 }] },
        ],
      },
    ],
  });
  assert.equal(data.buildings.length, 2);
  assert.equal(data.buildings[0].height, 20);
  assert.equal(data.buildings[0].outer.length, 4);
  assert.equal(data.buildings[1].holes.length, 1);
  assert.equal(data.buildings[1].levels, 6);
  assert.equal(data.roads.length, 1);
  assert.equal(data.roads[0].width, 14);
  assert.equal(data.ground.length, 1);
});

test('terrarium decodes and the grid samples bilinearly', () => {
  // Height 100 m: R*256 + G + B/256 - 32768 = 100 → R=128, G=100, B=0.
  const px = new Uint8ClampedArray([128, 100, 0, 255, 128, 100, 0, 255, 128, 100, 0, 255, 128, 100, 0, 255]);
  const h = decodeTerrarium(px, 2, 2);
  assert.equal(h[0], 100);
  const frame = new LocalFrame(4.9, 52.37);
  const range = tileRangeForBbox(bboxForRadius(4.9, 52.37, 300), 14);
  const tiles = [];
  for (let x = range.xMin; x <= range.xMax; x++) {
    for (let y = range.yMin; y <= range.yMax; y++) {
      const heights = new Float32Array(256 * 256);
      for (let i = 0; i < heights.length; i++) heights[i] = (i % 256) / 25.6; // 0..10 m east ramp
      tiles.push({ x, y, z: 14, size: 256, heights });
    }
  }
  const grid = gridFromTiles(tiles, range, frame);
  assert.equal(grid.nx, (range.xMax - range.xMin + 1) * 256);
  const hA = sampleHeight(grid, grid.x0 + 10 * grid.dx, grid.y0 + 3 * grid.dy);
  const hB = sampleHeight(grid, grid.x0 + 20 * grid.dx, grid.y0 + 3 * grid.dy);
  assert.ok(hB > hA, 'ramp rises eastward');
  assert.ok(grid.dx > 5 && grid.dx < 7, `dx ${grid.dx}`); // ≈ 5.9 m at 52°N, z14/256
});

test('a box footprint extrudes to 8 wall triangles and 2 roof triangles, facing out and up', () => {
  const walls = new MeshBuilder();
  const roofs = new MeshBuilder();
  const ok = extrudeFootprint(walls, roofs, [[0, 0], [10, 0], [10, 6], [0, 6]], [], 5, 12, 0, 2);
  assert.ok(ok);
  const w = walls.build();
  const r = roofs.build();
  assert.equal(w.triangleCount, 8);
  assert.equal(r.triangleCount, 2);
  // Roof vertices at base + height, normal up.
  for (let i = 0; i < r.vertexCount; i++) {
    assert.equal(r.positions[i * 3 + 1], 17);
    assert.equal(r.normals[i * 3 + 1], 1);
  }
  // First wall (south edge, y = 0 in east/north) faces south = +z in three.js.
  assert.equal(w.normals[2], 1);
  // Every wall normal points away from the footprint centre (5, 3) → three.js (5, *, -3).
  for (let i = 0; i < w.vertexCount; i++) {
    const dx = w.positions[i * 3] - 5;
    const dz = w.positions[i * 3 + 2] + 3;
    assert.ok(dx * w.normals[i * 3] + dz * w.normals[i * 3 + 2] > 0, `wall vertex ${i} faces inward`);
  }
  // Wall triangles wind counter-clockwise when seen from outside (front faces out).
  for (let t = 0; t < w.triangleCount; t++) {
    const [a, b, c] = [w.indices[t * 3], w.indices[t * 3 + 1], w.indices[t * 3 + 2]];
    const p = (k: number) => [w.positions[k * 3], w.positions[k * 3 + 1], w.positions[k * 3 + 2]];
    const [ax, ay, az] = p(a), [bx, by, bz] = p(b), [cx, cy, cz] = p(c);
    const ux = bx - ax, uy = by - ay, uz = bz - az;
    const vx = cx - ax, vy = cy - ay, vz = cz - az;
    const nx = uy * vz - uz * vy, ny = uz * vx - ux * vz, nz = ux * vy - uy * vx;
    assert.ok(nx * w.normals[a * 3] + ny * w.normals[a * 3 + 1] + nz * w.normals[a * 3 + 2] > 0, `wall triangle ${t} winds inward`);
  }
  // Triangle winding: roof triangles are counter-clockwise seen from above.
  for (let t = 0; t < r.triangleCount; t++) {
    const [a, b, c] = [r.indices[t * 3], r.indices[t * 3 + 1], r.indices[t * 3 + 2]];
    const ax = r.positions[a * 3], az = -r.positions[a * 3 + 2];
    const bx = r.positions[b * 3], bz = -r.positions[b * 3 + 2];
    const cx = r.positions[c * 3], cz = -r.positions[c * 3 + 2];
    assert.ok((bx - ax) * (cz - az) - (cx - ax) * (bz - az) > 0);
  }
  assert.equal(resolveHeight({ id: 'x', outer: [], holes: [], height: null, levels: 3, minHeight: 0, cls: 'yes' }, 100).height, 3 * 3.2 + 0.5);
  assert.equal(resolveHeight({ id: 'x', outer: [], holes: [], height: null, levels: null, minHeight: 0, cls: 'church' }, 100).height, 18);
});

test('terrain mesh and far ring have the expected sizes', () => {
  const grid = { x0: -10, y0: -10, dx: 10, dy: 10, nx: 3, ny: 3, data: new Float32Array([0, 0, 0, 0, 5, 0, 0, 0, 0]) };
  const m = terrainMesh(grid, { skirtDepth: 0 });
  assert.equal(m.vertexCount, 9);
  assert.equal(m.triangleCount, 8);
  // Centre vertex is highest and its normal points straight up.
  assert.equal(m.positions[4 * 3 + 1], 5);
  assert.ok(Math.abs(m.normals[4 * 3 + 1] - 1) < 1e-6);
  const withSkirt = terrainMesh(grid, { skirtDepth: 20 });
  assert.ok(withSkirt.triangleCount > m.triangleCount);
  const ring = farRing(100, 1000, -1, 16);
  assert.equal(ring.triangleCount, 32);
  assert.ok(ringArea([[0, 0], [1, 0], [1, 1]]) > 0);
});

test('sky model: zenith ≈ 1 at noon, darker at sunset, sun transmittance reddens', () => {
  const pc = perezCoefficients(2.5, 60);
  const zenith = skyRadiance([0, 1, 0], sunDirection(180, 60), pc);
  const lum = 0.2126 * zenith[0] + 0.7152 * zenith[1] + 0.0722 * zenith[2];
  assert.ok(lum > 0.8 && lum < 1.3, `zenith luminance ${lum}`);
  assert.ok(zenith[2] > zenith[0], 'zenith is blue');
  const low = perezCoefficients(2.5, 5);
  const z2 = skyRadiance([0, 1, 0], sunDirection(270, 5), low);
  assert.ok(z2[1] < zenith[1], 'sunset zenith is darker');
  const tHigh = sunTransmittance(60, 2.5);
  const tLow = sunTransmittance(3, 2.5);
  assert.ok(tHigh[0] > 0.7 && tHigh[2] > 0.5, `high sun transmittance ${tHigh}`);
  assert.ok(tLow[0] / tLow[2] > tHigh[0] / tHigh[2], 'low sun is redder');
  const L = computeSkyLighting({ sunAzimuth: 180, sunElevation: 45, turbidity: 2.5, sunIntensity: 100000 });
  assert.ok(L.skyIrradiance[1] > 1.5 && L.skyIrradiance[1] < 6, `sky irradiance ${L.skyIrradiance}`);
  assert.ok(L.sunIrradiance[1] > L.skyIrradiance[1], 'direct sun beats the sky');
});

test('RGBE round-trips within 1% and EXR headers are well formed', () => {
  const w = 16, h = 4;
  const data = new Float32Array(w * h * 3);
  for (let i = 0; i < data.length; i++) data[i] = i % 7 === 0 ? 0 : 10 ** ((i % 13) - 6) * (1 + (i % 5) / 10);
  const hdr = encodeHdr(w, h, data, 3, ['skyline360 test']);
  const back = decodeHdr(hdr);
  assert.equal(back.width, w);
  assert.equal(back.height, h);
  assert.deepEqual(back.comments, ['skyline360 test']);
  // RGBE shares one exponent per pixel: each channel is exact to 1/256 of the pixel's brightest channel.
  for (let p = 0; p < w * h; p++) {
    const m = Math.max(data[p * 3], data[p * 3 + 1], data[p * 3 + 2]);
    for (let c = 0; c < 3; c++) {
      const a = data[p * 3 + c], b = back.data[p * 3 + c];
      if (m === 0) assert.ok(b < 1e-30);
      else assert.ok(Math.abs(a - b) <= m / 128, `pixel ${p} channel ${c}: ${a} vs ${b}`);
    }
  }
  assert.equal(floatToHalf(1), 0x3c00);
  assert.equal(floatToHalf(-2), 0xc000);
  assert.equal(floatToHalf(65504), 0x7bff);
  assert.equal(floatToHalf(1e6), 0x7c00);
  const exr = encodeExr({ width: w, height: h, type: 'half', attributes: { comment: 'x' } }, data, 3);
  assert.equal(new DataView(exr.buffer).getInt32(0, true), 20000630);
  assert.equal(exr.length > w * h * 3 * 2 + h * 8, true);
  const exrF = encodeExr({ width: w, height: h, type: 'float' }, data, 3);
  // Last chunk's first float after the 8-byte prefix must equal the B channel of the last row's first pixel.
  const chunk = 8 + w * 3 * 4;
  const lastChunk = exrF.length - chunk;
  const dv = new DataView(exrF.buffer);
  assert.equal(dv.getInt32(lastChunk, true), h - 1);
  assert.ok(Math.abs(dv.getFloat32(lastChunk + 8, true) - data[(h - 1) * w * 3 + 2]) < 1e-6);
});

test('equirect mapping: centre is the heading, top is up, round-trip holds', () => {
  const north = dirFromUV(0.5, 0.5, 0);
  assert.ok(Math.abs(north[2] + 1) < 1e-9 && Math.abs(north[1]) < 1e-9);
  const east = dirFromUV(0.75, 0.5, 0);
  assert.ok(Math.abs(east[0] - 1) < 1e-9);
  const up = dirFromUV(0.3, 0, 0);
  assert.ok(Math.abs(up[1] - 1) < 1e-9);
  const withHeading = dirFromUV(0.5, 0.5, 90);
  assert.ok(Math.abs(withHeading[0] - 1) < 1e-9, 'heading 90 puts east in the centre');
  for (const [u, v] of [[0.1, 0.2], [0.6, 0.7], [0.99, 0.5]]) {
    const [u2, v2] = uvFromDir(dirFromUV(u, v, 30), 30);
    assert.ok(Math.abs(u2 - u) < 1e-9 && Math.abs(v2 - v) < 1e-9, `${u},${v} → ${u2},${v2}`);
  }
});

test('land cover rasterizes polygons with holes and road ribbons', () => {
  const frame = new LocalFrame(4.9, 52.37);
  const grid = { x0: -100, y0: -100, dx: 10, dy: 10, nx: 21, ny: 21, data: new Float32Array(441) };
  const sq = (cx: number, cy: number, r: number): [number, number][] => [frame.toLonLat(cx - r, cy - r), frame.toLonLat(cx + r, cy - r), frame.toLonLat(cx + r, cy + r), frame.toLonLat(cx - r, cy + r)];
  const cover = buildLandCover(
    [{ id: 'p', kind: 'green', outer: sq(0, 0, 60), holes: [sq(0, 0, 20)], trees: 'sparse' }, { id: 'w', kind: 'water', outer: sq(0, 0, 20), holes: [], trees: 'none' }],
    [{ id: 'r', points: [frame.toLonLat(-100, 80), frame.toLonLat(100, 80)], width: 10 }],
    frame,
    grid,
    256,
    1,
  );
  assert.equal(cover.texelSize, 1);
  assert.equal(coverAt(cover, 40, 40), COVER_GREEN);
  assert.equal(coverAt(cover, 0, 0), COVER_WATER);
  assert.equal(coverAt(cover, 90, 90), COVER_TERRAIN);
  assert.equal(coverAt(cover, 0, 80), COVER_ROAD);
  assert.equal(coverAt(cover, 0, 70), COVER_TERRAIN);
  // Tree density follows the park but not the pond or the road.
  const at = (x: number, y: number): number => cover.trees[Math.floor((y - cover.originY) / cover.texelSize) * cover.width + Math.floor((x - cover.originX) / cover.texelSize)];
  assert.equal(at(40, 40), 1);
  assert.equal(at(0, 0), 0);
  assert.equal(at(0, 80), 0);
});

test('demo scene builds through the whole pipeline', () => {
  const scene = demoScene();
  assert.ok(scene.area.buildings.length > 300, `buildings ${scene.area.buildings.length}`);
  const meshes = buildSceneMeshes(scene.area, scene.terrain, scene.frame, { radius: 1500, maxTerrainSide: 256, horizon: scene.horizon });
  assert.ok(meshes.stats.count > 300);
  assert.ok(meshes.stats.maxHeight > 150);
  assert.ok(meshes.walls.triangleCount > 2000);
  assert.ok(meshes.terrain.vertexCount <= 257 * 257 + 4 * 2 * 257);
  const lc = meshes.landCover;
  assert.ok(lc.fractions.green > 0.005 && lc.fractions.water > 0.002 && lc.fractions.road > 0.02, `cover fractions ${JSON.stringify(lc.fractions)}`);
  assert.equal(coverAt(lc, 380, -280), COVER_GREEN); // in the park
  assert.equal(coverAt(lc, 420, -320), COVER_WATER); // the pond
  assert.equal(coverAt(lc, -262, -80), COVER_WATER); // the river
  assert.equal(coverAt(lc, 370, -500), COVER_ROAD); // a north–south street
  assert.equal(coverAt(lc, 1000, 1000), COVER_TERRAIN);
  assert.ok(meshes.terrain.uvs && meshes.terrain.uvs.length === meshes.terrain.vertexCount * 2);
  assert.ok(meshes.trees.count > 500 && meshes.trees.count < 80000, `trees ${meshes.trees.count}`);
  assert.ok(meshes.horizon && meshes.horizon.triangleCount > 1000, 'horizon terrain present');
  let maxHorizon = -Infinity;
  for (let i = 1; i < meshes.horizon!.positions.length; i += 3) maxHorizon = Math.max(maxHorizon, meshes.horizon!.positions[i]);
  assert.ok(maxHorizon > 1000, `mountains reach ${maxHorizon.toFixed(0)} m`);
  assert.ok(meshes.groundAtOrigin > 0);
  // Every index points at a vertex.
  for (const m of [meshes.walls, meshes.roofs, meshes.terrain, meshes.ring, meshes.horizon!]) {
    for (let i = 0; i < m.indices.length; i++) assert.ok(m.indices[i] < m.vertexCount);
    assert.equal(m.positions.length, m.vertexCount * 3);
  }
});
