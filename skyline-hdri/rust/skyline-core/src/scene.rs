//! Scene preparation and rendering. Twin of `src/engine/index.js`.

use crate::buildings::{build_query, parse_buildings, point_in_polygon, rasterize_buildings, BuildingSource};
use crate::geo::DEG;
use crate::sweep::{sweep, Plate, SweepParams};
use crate::tiers::{build_tier, load_mosaics_for_tiers, max_height, plan_tiers, Tier};
use crate::tiles::TileSource;

#[derive(Clone, Debug)]
pub struct BuildingParams {
    pub enabled: bool,
    pub all_radius: f64,
    pub tall_radius: f64,
    pub tall_threshold: f64,
    pub height_scale: f64,
}

#[derive(Clone, Debug)]
pub struct Params {
    pub lat: f64,
    pub lon: f64,
    pub eye_height: f64,
    pub eye_altitude: Option<f64>,
    pub eye_on_roof: bool,
    pub width: usize,
    pub max_distance: f64,
    pub high_quality: bool,
    pub heading_offset_deg: f64,
    pub vertical_exaggeration: f64,
    pub bands: Vec<f64>,
    pub buildings: BuildingParams,
    pub threads: usize,
}

impl Default for Params {
    fn default() -> Self {
        Self {
            lat: 52.0907, lon: 5.1214, eye_height: 1.7, eye_altitude: None, eye_on_roof: false,
            width: 4096, max_distance: 250_000.0, high_quality: false, heading_offset_deg: 0.0, vertical_exaggeration: 1.0,
            bands: vec![2000.0, 8000.0, 30000.0, 120000.0],
            buildings: BuildingParams { enabled: true, all_radius: 2500.0, tall_radius: 30000.0, tall_threshold: 35.0, height_scale: 1.0 },
            threads: std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4),
        }
    }
}

#[derive(Debug, Default, Clone)]
pub struct Stats { pub tiles: usize, pub tiles_missing: usize, pub buildings_fetched: usize, pub buildings_rasterised: usize, pub buildings_skipped: usize, pub prepare_ms: u128 }

pub struct Scene { pub params: Params, pub tiers: Vec<Tier>, pub eye_z: f64, pub z_ground: f64, pub h_max: f64, pub stats: Stats }

pub trait Progress: Sync { fn report(&self, stage: &str, done: usize, total: usize); }
pub struct NoProgress;
impl Progress for NoProgress { fn report(&self, _: &str, _: usize, _: usize) {} }

/// Fetch terrain and buildings and build the tiers around the eye.
pub fn prepare_scene(params: &Params, tiles: &dyn TileSource, buildings: Option<&dyn BuildingSource>, progress: &dyn Progress) -> Scene {
    let t0 = std::time::Instant::now();
    let specs = plan_tiers(params.max_distance, params.high_quality);
    let (mosaics, tile_total) = load_mosaics_for_tiers(&specs, params.lat, params.lon, tiles, params.threads.max(4), &|d, t, _z| progress.report("tiles", d, t));
    let missing: usize = mosaics.values().map(|m| m.missing).sum();
    let mut tiers: Vec<Tier> = Vec::with_capacity(specs.len());
    for (k, spec) in specs.iter().enumerate() {
        progress.report("tiers", k, specs.len());
        tiers.push(build_tier(spec, &mosaics[&spec.zoom], params.lat, params.lon, true));
    }
    progress.report("tiers", specs.len(), specs.len());
    let z_ground = tiers[0].sample(0.0, 0.0);
    let mut eye_z = params.eye_altitude.unwrap_or(z_ground + params.eye_height);
    let mut stats = Stats { tiles: tile_total, tiles_missing: missing, ..Default::default() };
    if params.buildings.enabled {
        if let Some(src) = buildings {
            progress.report("buildings", 0, 1);
            let query = build_query(params.lat, params.lon, params.buildings.all_radius, params.buildings.tall_radius);
            let parsed = match src.fetch(&query) {
                Some(text) => parse_buildings(&text, params.lat, params.lon).unwrap_or_else(|e| { eprintln!("overpass parse: {}", e); vec![] }),
                None => { eprintln!("no building data"); vec![] }
            };
            stats.buildings_fetched = parsed.len();
            if params.eye_on_roof && params.eye_altitude.is_none() {
                if let Some(under) = parsed.iter().find(|b| b.polys.iter().any(|p| point_in_polygon(p, 0.0, 0.0))) {
                    eye_z = z_ground + under.height * params.buildings.height_scale + params.eye_height;
                }
            }
            let r = rasterize_buildings(&parsed, &mut tiers, params.buildings.tall_threshold, 3, 3, params.buildings.height_scale, Some((0.0, 0.0)));
            stats.buildings_rasterised = r.count;
            stats.buildings_skipped = r.skipped;
            progress.report("buildings", 1, 1);
        }
    }
    let h_max = max_height(&tiers);
    stats.prepare_ms = t0.elapsed().as_millis();
    Scene { params: params.clone(), tiers, eye_z, z_ground, h_max, stats }
}

/// Render the plate with the CPU sweep.
pub fn render(scene: &Scene, progress: &dyn Progress) -> Plate {
    let p = &scene.params;
    let sp = SweepParams {
        eye_z: scene.eye_z, z_ground: scene.z_ground, vertical_exaggeration: p.vertical_exaggeration,
        heading_offset: p.heading_offset_deg * DEG, max_distance: p.max_distance, bands: p.bands.clone(),
    };
    sweep(&scene.tiers, &sp, p.width, p.threads, &|d, t| progress.report("sweep", d, t))
}
