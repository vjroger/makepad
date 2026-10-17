/**
 * A deterministic synthetic city: street grid, downtown towers, a river, a
 * park and a gentle hill. Used by the offline tests and as the sample scene
 * shown before any real area is fetched.
 */
import type { AreaData, Footprint, GroundPolygon, HeightGrid, RoadLine } from './types';
import { LocalFrame } from './geo';

export const DEMO_CENTER = { lon: 4.9041, lat: 52.3676 };
export const DEMO_RADIUS = 1500;
/** Spot in the park in local metres (east, north) used as the sample viewpoint: open sky to the south-east. */
export const DEMO_VIEWPOINT: [number, number] = [380, -280];
/** Centre of the landmark tower in local metres and its height, for tests. */
export const DEMO_TOWER = { east: 10, north: 260, height: 222 };

/** mulberry32 PRNG: small, fast, deterministic. */
export function prng(seed: number): () => number {
  let a = seed >>> 0;
  return () => {
    a = (a + 0x6d2b79f5) >>> 0;
    let t = a;
    t = Math.imul(t ^ (t >>> 15), t | 1);
    t ^= t + Math.imul(t ^ (t >>> 7), t | 61);
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
}

export interface DemoScene {
  area: AreaData;
  terrain: HeightGrid;
  horizon: HeightGrid;
  frame: LocalFrame;
}

/** Terrain height in metres at local (e, n). */
export function demoHeight(e: number, n: number): number {
  const hill = 14 * Math.exp(-((e - 700) ** 2 + (n + 500) ** 2) / 700 ** 2);
  const ridge = 6 * Math.exp(-((e + 900) ** 2) / 300 ** 2);
  const valley = -3 * Math.exp(-((e + 250) ** 2) / 120 ** 2);
  const ripple = 0.4 * Math.sin(e / 90) * Math.cos(n / 110);
  return 4 + hill + ridge + valley + ripple;
}

/** Distant mountains for the sample: a ridge 25–45 km to the north-east, up to 1,400 m. */
export function demoHorizon(radius = 60000, cell = 250): HeightGrid {
  const nx = Math.ceil((2 * radius) / cell) + 1;
  const data = new Float32Array(nx * nx);
  for (let j = 0; j < nx; j++) {
    for (let i = 0; i < nx; i++) {
      const e = -radius + i * cell, n = -radius + j * cell;
      const d = Math.hypot(e, n);
      const bearing = Math.atan2(e, n);
      // Ridge between bearings 20° and 110°, peaking around 35 km.
      const along = Math.exp(-(((d - 35000) / 9000) ** 2));
      let db = bearing - 1.13;
      db = Math.atan2(Math.sin(db), Math.cos(db)); // wrap to [-π, π]
      const sector = Math.abs(db) * 1.6 < Math.PI / 2 ? Math.cos(db * 1.6) : 0;
      const peaks = 0.6 + 0.4 * Math.sin(e / 2600) * Math.cos(n / 3100);
      const ridge = 1400 * along * sector * peaks;
      const hills = 180 * Math.exp(-(((d - 12000) / 5000) ** 2)) * (0.5 + 0.5 * Math.sin(e / 1500 + n / 1800));
      data[j * nx + i] = 4 + Math.max(0, ridge) + Math.max(0, hills) + (d < 2000 ? demoHeight(e, n) - 4 : 0);
    }
  }
  return { x0: -radius, y0: -radius, dx: cell, dy: cell, nx, ny: nx, data };
}

export function demoTerrain(radius = DEMO_RADIUS, cell = 10): HeightGrid {
  const half = radius + 100;
  const nx = Math.ceil((2 * half) / cell) + 1;
  const ny = nx;
  const data = new Float32Array(nx * ny);
  for (let j = 0; j < ny; j++) {
    for (let i = 0; i < nx; i++) {
      data[j * nx + i] = demoHeight(-half + i * cell, -half + j * cell);
    }
  }
  return { x0: -half, y0: -half, dx: cell, dy: cell, nx, ny, data };
}

export function demoScene(seed = 7): DemoScene {
  const rnd = prng(seed);
  const frame = new LocalFrame(DEMO_CENTER.lon, DEMO_CENTER.lat);
  const toLL = (e: number, n: number): [number, number] => frame.toLonLat(e, n);
  const buildings: Footprint[] = [];
  const ground: GroundPolygon[] = [];
  const roads: RoadLine[] = [];

  // River: a gentle S-curve running north–south around e ≈ -250.
  const riverLeft: [number, number][] = [];
  const riverRight: [number, number][] = [];
  for (let n = -DEMO_RADIUS - 200; n <= DEMO_RADIUS + 200; n += 50) {
    const c = -250 + 60 * Math.sin(n / 400);
    riverLeft.push([c - 45, n]);
    riverRight.push([c + 45, n]);
  }
  ground.push({ id: 'river', kind: 'water', outer: [...riverLeft, ...riverRight.reverse()].map(([e, n]) => toLL(e, n)), holes: [], trees: 'none' });
  // Park with a pond.
  const park: [number, number][] = [];
  for (let a = 0; a < 24; a++) {
    const t = (a / 24) * Math.PI * 2;
    park.push([400 + 220 * Math.cos(t), -300 + 160 * Math.sin(t)]);
  }
  const pond: [number, number][] = [];
  for (let a = 0; a < 16; a++) {
    const t = (a / 16) * Math.PI * 2;
    pond.push([420 + 50 * Math.cos(t), -320 + 35 * Math.sin(t)]);
  }
  ground.push({ id: 'park', kind: 'green', outer: park.map(([e, n]) => toLL(e, n)), holes: [pond.map(([e, n]) => toLL(e, n))], trees: 'sparse' });
  ground.push({ id: 'pond', kind: 'water', outer: pond.map(([e, n]) => toLL(e, n)), holes: [], trees: 'none' });
  // A wood on the hill to the south-east.
  const wood: [number, number][] = [];
  for (let a = 0; a < 20; a++) {
    const t = (a / 20) * Math.PI * 2;
    wood.push([760 + 240 * Math.cos(t) + 40 * Math.sin(3 * t), -620 + 200 * Math.sin(t)]);
  }
  ground.push({ id: 'wood', kind: 'green', outer: wood.map(([e, n]) => toLL(e, n)), holes: [], trees: 'dense' });

  const inRiver = (e: number, n: number): boolean => Math.abs(e - (-250 + 60 * Math.sin(n / 400))) < 70;
  const inPark = (e: number, n: number): boolean => ((e - 400) / 240) ** 2 + ((n + 300) / 180) ** 2 < 1;
  const inWood = (e: number, n: number): boolean => ((e - 760) / 300) ** 2 + ((n + 620) / 250) ** 2 < 1;

  // Street grid.
  const block = 96, street = 14, pitch = block + street;
  const extent = DEMO_RADIUS;
  for (let x = -extent; x <= extent; x += pitch) {
    roads.push({ id: `rx${x}`, points: [toLL(x, -extent), toLL(x, extent)], width: Math.abs(x) < 20 ? 18 : 9 });
  }
  for (let y = -extent; y <= extent; y += pitch) {
    roads.push({ id: `ry${y}`, points: [toLL(-extent, y), toLL(extent, y)], width: Math.abs(y) < 20 ? 18 : 9 });
  }

  // Buildings inside each block.
  let id = 0;
  for (let bx = -extent; bx < extent; bx += pitch) {
    for (let by = -extent; by < extent; by += pitch) {
      const x0 = bx + street / 2, y0 = by + street / 2;
      const cx = x0 + block / 2, cy = y0 + block / 2;
      if (Math.hypot(cx, cy) > extent - 60) continue;
      if (inRiver(cx, cy) || inPark(cx, cy) || inWood(cx, cy)) continue;
      const downtown = Math.hypot(cx - 150, cy - 100) < 420;
      const lots = downtown ? 1 + Math.floor(rnd() * 2) : 2 + Math.floor(rnd() * 4);
      const lotW = block / lots;
      for (let l = 0; l < lots; l++) {
        const inset = 4 + rnd() * 6;
        const e0 = x0 + l * lotW + inset, e1 = x0 + (l + 1) * lotW - inset;
        const depth = block * (0.45 + rnd() * 0.5);
        const n0 = y0 + inset, n1 = y0 + inset + depth;
        if (e1 - e0 < 8 || n1 - n0 < 8) continue;
        let outer: [number, number][] = [
          [e0, n0],
          [e1, n0],
          [e1, n1],
          [e0, n1],
        ];
        // L-shaped footprints now and then.
        if (rnd() < 0.3) {
          const mx = (e0 + e1) / 2, my = (n0 + n1) / 2;
          outer = [
            [e0, n0],
            [e1, n0],
            [e1, my],
            [mx, my],
            [mx, n1],
            [e0, n1],
          ];
        }
        let height: number | null = null;
        let levels: number | null = null;
        let cls = 'house';
        const r = rnd();
        if (downtown) {
          cls = 'office';
          height = 30 + rnd() * 130;
          if (r < 0.15) height = 150 + rnd() * 90;
        } else if (r < 0.5) {
          height = 6 + rnd() * 8;
        } else if (r < 0.8) {
          levels = 2 + Math.floor(rnd() * 4);
          cls = 'apartments';
        } else {
          cls = r < 0.9 ? 'house' : 'industrial';
        }
        buildings.push({ id: `b${id++}`, outer: outer.map(([e, n]) => toLL(e, n)), holes: [], height, levels, minHeight: 0, cls });
      }
    }
  }

  // One landmark tower with a floating part on a podium.
  buildings.push({ id: 'podium', outer: [toLL(-40, 220), toLL(60, 220), toLL(60, 300), toLL(-40, 300)], holes: [], height: 12, levels: null, minHeight: 0, cls: 'commercial' });
  buildings.push({ id: 'tower', outer: [toLL(-20, 240), toLL(40, 240), toLL(40, 280), toLL(-20, 280)], holes: [], height: 210, levels: null, minHeight: 12, cls: 'office' });

  return {
    area: { buildings, ground, roads, sources: ['Synthetic sample city generated by Skyline 360 (no real-world data)'] },
    terrain: demoTerrain(),
    horizon: demoHorizon(),
    frame,
  };
}
