/**
 * Overpass API query builder and response parser (OpenStreetMap data, ODbL).
 *
 * Used by the prototype because one bbox query returns footprints with
 * height tags and the ground classes in a single JSON document. Production
 * builds should read Overture PMTiles instead (see the plan); the parser's
 * output type is the same.
 */
import type { AreaData, Footprint, GroundPolygon, LonLatRing, RoadLine, TreeDensity } from './types';

export const OVERPASS_URL = 'https://overpass-api.de/api/interpreter';

const ROAD_WIDTHS: Record<string, number> = {
  motorway: 25,
  trunk: 20,
  primary: 14,
  secondary: 11,
  tertiary: 9,
  residential: 7,
  unclassified: 6,
  living_street: 5,
  pedestrian: 4,
  motorway_link: 8,
  trunk_link: 8,
  primary_link: 8,
  secondary_link: 7,
  tertiary_link: 6,
  rail: 5,
};

const GREEN_LANDUSE = /^(grass|forest|meadow|recreation_ground|cemetery|orchard|allotments|farmland|greenfield|village_green|vineyard)$/;
const GREEN_LEISURE = /^(park|garden|golf_course|pitch|nature_reserve)$/;
const GREEN_NATURAL = /^(wood|scrub|heath|grassland|wetland)$/;

/** Build the Overpass QL query for a bbox [minLon, minLat, maxLon, maxLat]. */
export function buildOverpassQuery(bbox: [number, number, number, number], timeoutSeconds = 90): string {
  const b = `(${bbox[1]},${bbox[0]},${bbox[3]},${bbox[2]})`;
  return [
    `[out:json][timeout:${timeoutSeconds}];`,
    '(',
    `  way["building"]["building"!="no"]${b};`,
    `  relation["building"]["type"="multipolygon"]${b};`,
    `  way["natural"="water"]${b};`,
    `  relation["natural"="water"]["type"="multipolygon"]${b};`,
    `  way["waterway"="riverbank"]${b};`,
    `  way["landuse"~"^(reservoir|basin)$"]${b};`,
    `  way["leisure"~"^(park|garden|golf_course|pitch|nature_reserve)$"]${b};`,
    `  relation["leisure"~"^(park|garden|golf_course|nature_reserve)$"]["type"="multipolygon"]${b};`,
    `  way["landuse"~"^(grass|forest|meadow|recreation_ground|cemetery|orchard|allotments|farmland|greenfield|village_green|vineyard)$"]${b};`,
    `  relation["landuse"~"^(forest|grass|meadow|farmland)$"]["type"="multipolygon"]${b};`,
    `  way["natural"~"^(wood|scrub|heath|grassland|wetland)$"]${b};`,
    `  way["highway"~"^(motorway|trunk|primary|secondary|tertiary|residential|unclassified|living_street|pedestrian|motorway_link|trunk_link|primary_link|secondary_link|tertiary_link)$"]${b};`,
    `  way["railway"="rail"]${b};`,
    ');',
    `out geom${b};`,
  ].join('\n');
}

// ---------------------------------------------------------------------------
// Response types (the subset we read)

interface OsmPoint {
  lat: number;
  lon: number;
}
interface OsmWay {
  type: 'way';
  id: number;
  tags?: Record<string, string>;
  geometry?: OsmPoint[];
}
interface OsmRelationMember {
  type: string;
  ref: number;
  role: string;
  geometry?: OsmPoint[];
}
interface OsmRelation {
  type: 'relation';
  id: number;
  tags?: Record<string, string>;
  members?: OsmRelationMember[];
}
type OsmElement = OsmWay | OsmRelation | { type: 'node'; id: number };

export interface OverpassResponse {
  elements: OsmElement[];
}

// ---------------------------------------------------------------------------
// Tag helpers

/** Parse an OSM length tag ("12", "12.5 m", "40 ft", "40'") into metres. */
export function parseLength(value: string | undefined): number | null {
  if (!value) return null;
  const v = value.trim().toLowerCase().replace(',', '.');
  const m = /^(-?\d+(?:\.\d+)?)\s*(m|ft|feet|'|")?/.exec(v);
  if (!m) return null;
  const n = parseFloat(m[1]);
  if (!Number.isFinite(n)) return null;
  const unit = m[2];
  if (unit === 'ft' || unit === 'feet' || unit === "'") return n * 0.3048;
  if (unit === '"') return n * 0.0254;
  return n;
}

function parseNumber(value: string | undefined): number | null {
  if (!value) return null;
  const n = parseFloat(value.replace(',', '.'));
  return Number.isFinite(n) ? n : null;
}

/** Tree cover implied by the tags: forests dense, parks and gardens sparse, fields none. */
export function treeDensityFromTags(tags: Record<string, string>): TreeDensity {
  if (tags.natural === 'wood' || tags.landuse === 'forest' || tags.natural === 'scrub') return 'dense';
  if (tags.landuse === 'orchard' || tags.landuse === 'vineyard') return 'dense';
  if (tags.leisure === 'park' || tags.leisure === 'garden' || tags.leisure === 'nature_reserve') return 'sparse';
  if (tags.landuse === 'cemetery' || tags.landuse === 'allotments' || tags.landuse === 'village_green' || tags.landuse === 'recreation_ground') return 'sparse';
  return 'none';
}

function groundKindFromTags(tags: Record<string, string>): 'water' | 'green' | null {
  if (tags.natural === 'water' || tags.waterway === 'riverbank') return 'water';
  if (tags.landuse === 'reservoir' || tags.landuse === 'basin') return 'water';
  if (tags.leisure && GREEN_LEISURE.test(tags.leisure)) return 'green';
  if (tags.landuse && GREEN_LANDUSE.test(tags.landuse)) return 'green';
  if (tags.natural && GREEN_NATURAL.test(tags.natural)) return 'green';
  return null;
}

// ---------------------------------------------------------------------------
// Ring assembly

function toRing(points: OsmPoint[]): LonLatRing {
  const ring: LonLatRing = points.map((p) => [p.lon, p.lat]);
  if (ring.length > 1) {
    const a = ring[0];
    const b = ring[ring.length - 1];
    if (a[0] === b[0] && a[1] === b[1]) ring.pop();
  }
  return ring;
}

function samePoint(a: [number, number], b: [number, number]): boolean {
  return Math.abs(a[0] - b[0]) < 1e-9 && Math.abs(a[1] - b[1]) < 1e-9;
}

/**
 * Join open way geometries into closed rings by matching endpoints
 * (relation members arrive as unordered, possibly reversed, fragments).
 */
export function assembleRings(ways: [number, number][][]): LonLatRing[] {
  const pending = ways.filter((w) => w.length >= 2).map((w) => w.slice());
  const rings: LonLatRing[] = [];
  while (pending.length) {
    let ring = pending.shift()!;
    let progress = true;
    while (progress && !samePoint(ring[0], ring[ring.length - 1])) {
      progress = false;
      const tail = ring[ring.length - 1];
      for (let i = 0; i < pending.length; i++) {
        const w = pending[i];
        if (samePoint(w[0], tail)) {
          ring = ring.concat(w.slice(1));
        } else if (samePoint(w[w.length - 1], tail)) {
          ring = ring.concat(w.slice(0, -1).reverse());
        } else continue;
        pending.splice(i, 1);
        progress = true;
        break;
      }
    }
    if (samePoint(ring[0], ring[ring.length - 1])) ring.pop();
    if (ring.length >= 3) rings.push(ring);
  }
  return rings;
}

function pointInRing(p: [number, number], ring: LonLatRing): boolean {
  let inside = false;
  for (let i = 0, j = ring.length - 1; i < ring.length; j = i++) {
    const xi = ring[i][0], yi = ring[i][1];
    const xj = ring[j][0], yj = ring[j][1];
    const intersect = yi > p[1] !== yj > p[1] && p[0] < ((xj - xi) * (p[1] - yi)) / (yj - yi) + xi;
    if (intersect) inside = !inside;
  }
  return inside;
}

/** Split a relation's outer and inner rings into polygons (outer + its holes). */
export function multipolygonToPolygons(outers: LonLatRing[], inners: LonLatRing[]): { outer: LonLatRing; holes: LonLatRing[] }[] {
  const polys = outers.map((outer) => ({ outer, holes: [] as LonLatRing[] }));
  for (const inner of inners) {
    const target = polys.find((p) => pointInRing(inner[0], p.outer)) ?? polys[0];
    if (target) target.holes.push(inner);
  }
  return polys;
}

// ---------------------------------------------------------------------------
// Parser

function footprintFromTags(id: string, tags: Record<string, string>, outer: LonLatRing, holes: LonLatRing[]): Footprint {
  const levels = parseNumber(tags['building:levels']);
  const minLevel = parseNumber(tags['building:min_level']);
  const minHeight = parseLength(tags['min_height']) ?? (minLevel ? minLevel * 3.2 : 0);
  return {
    id,
    outer,
    holes,
    height: parseLength(tags.height) ?? parseLength(tags['building:height']),
    levels,
    minHeight: Math.max(0, minHeight),
    cls: tags.building || 'yes',
  };
}

/** Parse an Overpass JSON response into AreaData. */
export function parseOverpass(json: OverpassResponse): AreaData {
  const buildings: Footprint[] = [];
  const ground: GroundPolygon[] = [];
  const roads: RoadLine[] = [];

  for (const el of json.elements) {
    if (el.type === 'way') {
      const tags = el.tags ?? {};
      if (!el.geometry || el.geometry.length < 2) continue;
      if (tags.highway && ROAD_WIDTHS[tags.highway]) {
        roads.push({ id: `w${el.id}`, points: el.geometry.map((p) => [p.lon, p.lat]), width: ROAD_WIDTHS[tags.highway] });
        continue;
      }
      if (tags.railway === 'rail') {
        roads.push({ id: `w${el.id}`, points: el.geometry.map((p) => [p.lon, p.lat]), width: ROAD_WIDTHS.rail });
        continue;
      }
      const ring = toRing(el.geometry);
      if (ring.length < 3) continue;
      if (tags.building && tags.building !== 'no') {
        buildings.push(footprintFromTags(`w${el.id}`, tags, ring, []));
        continue;
      }
      const gk = groundKindFromTags(tags);
      if (gk) ground.push({ id: `w${el.id}`, kind: gk, outer: ring, holes: [], trees: gk === 'green' ? treeDensityFromTags(tags) : 'none' });
    } else if (el.type === 'relation') {
      const tags = el.tags ?? {};
      const members = el.members ?? [];
      const outerWays = members.filter((m) => m.type === 'way' && m.role !== 'inner' && m.geometry).map((m) => toRingOpen(m.geometry!));
      const innerWays = members.filter((m) => m.type === 'way' && m.role === 'inner' && m.geometry).map((m) => toRingOpen(m.geometry!));
      const polys = multipolygonToPolygons(assembleRings(outerWays), assembleRings(innerWays));
      polys.forEach((poly, i) => {
        const id = `r${el.id}${polys.length > 1 ? `_${i}` : ''}`;
        if (tags.building && tags.building !== 'no') {
          buildings.push(footprintFromTags(id, tags, poly.outer, poly.holes));
          return;
        }
        const gk = groundKindFromTags(tags);
        if (gk) ground.push({ id, kind: gk, outer: poly.outer, holes: poly.holes, trees: gk === 'green' ? treeDensityFromTags(tags) : 'none' });
      });
    }
  }

  return {
    buildings,
    ground,
    roads,
    sources: ['Map data © OpenStreetMap contributors, ODbL 1.0 (https://www.openstreetmap.org/copyright), fetched via the Overpass API'],
  };
}

/** Member geometry as an open polyline (keeps a duplicate closing point if present; assembleRings handles it). */
function toRingOpen(points: OsmPoint[]): [number, number][] {
  return points.map((p) => [p.lon, p.lat]);
}
