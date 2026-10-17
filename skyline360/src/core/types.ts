/**
 * Shared data types for the Skyline 360 core.
 *
 * Everything in `core/` is plain TypeScript with no DOM or three.js
 * dependency, so it ports to Rust (the `skyline-core` crate) function by
 * function. Coordinates are [lon, lat] in degrees until they enter the
 * LocalFrame, where they become metres east/north/up around the viewpoint.
 */

/** A ring of [lon, lat] pairs. First point is not repeated at the end. */
export type LonLatRing = [number, number][];

/** A polygon in geographic coordinates: one outer ring plus holes. */
export interface GeoPolygon {
  outer: LonLatRing;
  holes: LonLatRing[];
}

export type FootprintKind = 'building';
export type GroundKind = 'water' | 'green' | 'road';

/** A building footprint with whatever height information the source had. */
export interface Footprint extends GeoPolygon {
  id: string;
  /** Height in metres from ground to the highest point, if known. */
  height: number | null;
  /** Number of above-ground floors, if known. */
  levels: number | null;
  /** Height above ground where the part starts (overhangs, towers on podiums). */
  minHeight: number;
  /** OSM building value or Overture class, used for default heights. */
  cls: string;
}

export type TreeDensity = 'none' | 'sparse' | 'dense';

/** A flat ground polygon: water or green area, with the tree cover it implies. */
export interface GroundPolygon extends GeoPolygon {
  id: string;
  kind: 'water' | 'green';
  trees: TreeDensity;
}

/** A road centreline with a nominal width in metres. */
export interface RoadLine {
  id: string;
  points: LonLatRing;
  width: number;
}

/** Everything one area fetch returns. */
export interface AreaData {
  buildings: Footprint[];
  ground: GroundPolygon[];
  roads: RoadLine[];
  /** Human-readable source credit lines, written into the attribution file. */
  sources: string[];
}

/**
 * A regular height grid in local metres. Row 0 is the southern edge
 * (smallest north value); column 0 the western edge.
 */
export interface HeightGrid {
  /** Local x (east) of column 0, metres. */
  x0: number;
  /** Local y (north) of row 0, metres. */
  y0: number;
  /** Cell size in metres along east and north. */
  dx: number;
  dy: number;
  nx: number;
  ny: number;
  /** Heights in metres above sea level, row-major, ny * nx values. */
  data: Float32Array;
}

/** Triangle mesh buffers ready for the GPU. */
export interface MeshData {
  positions: Float32Array; // xyz per vertex, three.js frame (x east, y up, z south)
  normals: Float32Array;
  /** Optional texture coordinates (terrain uses them for the land-cover map). */
  uvs?: Float32Array;
  indices: Uint32Array;
  vertexCount: number;
  triangleCount: number;
}

/** Sun and sky parameters shared by the CPU model and the shaders. */
export interface SkyParams {
  /** Compass bearing of the sun, degrees clockwise from north. */
  sunAzimuth: number;
  /** Sun elevation above the horizon, degrees. */
  sunElevation: number;
  /** Atmospheric turbidity, 1.5 (very clear) to 10 (hazy). */
  turbidity: number;
  /** Radiance of the solar disc in multiples of the scene unit (clear-sky zenith ≈ 1). */
  sunIntensity: number;
}

export interface Viewpoint {
  lon: number;
  lat: number;
  /** Eye height above local ground, metres. */
  eyeHeight: number;
  /** Compass heading that lands at the centre column of the panorama. */
  heading: number;
}
