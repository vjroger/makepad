//! skyline — render a 360° skyline plate from the command line.
//!
//!   skyline --lat 52.0907 --lon 5.1214 --width 4096 --max-km 250 --out ./out
//!
//! Options (defaults in brackets):
//!   --lat, --lon            viewpoint (required)
//!   --eye 1.7               eye height above ground, m
//!   --alt <m>               absolute eye altitude, overrides --eye
//!   --roof                  if the eye is inside a footprint, stand on its roof
//!   --width 4096            plate width (height = width / 2)
//!   --max-km 250            reach
//!   --high                  finer far tiers (4x tiles)
//!   --heading 0             heading offset, degrees
//!   --vexag 1               vertical exaggeration
//!   --no-buildings          terrain only
//!   --all-radius 2500 --tall-radius 30000 --tall 35 --height-scale 1
//!   --threads <n>
//!   --tile-url <template>   default AWS terrain tiles
//!   --overpass <url>        one mirror (repeatable); default: the three public mirrors
//!   --cache <dir>           tile cache directory [~/.cache/skyline-hdri/tiles]
//!   --out <dir>             output directory [.]
//!   --formats a,b,c         rgba,mask,depth,normal,sdf,exr,exr-az,svg,json,raw [rgba,depth,exr,svg,json]
//!   --sun-az 225 --sun-el 35 --haze 60000 --sdf-spread 64
//!
//! Static server for the browser app (no Node/Python needed), with an Overpass proxy at /overpass:
//!   skyline --serve 8080 [--root <folder containing index.html>] [--overpass <mirror> ...]

use skyline_core::export::exr::plate_to_exr;
use skyline_core::export::layers::{depth_to_gray16, normal_to_rgb8, sdf_to_gray8, shade_plate, signed_distance_field, ShadeMode};
use skyline_core::export::svg::plate_to_svg;
use skyline_core::http::{HttpBuildingSource, HttpTileSource};
use skyline_core::png::{encode_gray16, encode_png};
use skyline_core::tiles::DEFAULT_TILE_URL;
use skyline_core::{prepare_scene, render, Params, Progress};
use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Duration;

mod serve;

struct StderrProgress { last: std::sync::Mutex<String> }
impl Progress for StderrProgress {
    fn report(&self, stage: &str, done: usize, total: usize) {
        let line = format!("{} {}/{}", stage, done, total);
        let mut last = self.last.lock().unwrap();
        if *last != line { eprint!("\r{:<40}", line); *last = line; }
        if done == total { eprintln!(); }
    }
}

fn parse_args() -> (HashMap<String, Vec<String>>, Vec<String>) {
    let mut flags: HashMap<String, Vec<String>> = HashMap::new();
    let mut positional = Vec::new();
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut i = 0;
    while i < args.len() {
        if let Some(name) = args[i].strip_prefix("--") {
            let bool_flags = ["high", "roof", "no-buildings", "help"];
            if bool_flags.contains(&name) || i + 1 >= args.len() || args[i + 1].starts_with("--") {
                flags.entry(name.to_string()).or_default().push("true".into());
                i += 1;
            } else {
                flags.entry(name.to_string()).or_default().push(args[i + 1].clone());
                i += 2;
            }
        } else { positional.push(args[i].clone()); i += 1; }
    }
    (flags, positional)
}

fn main() {
    let (flags, _) = parse_args();
    let get = |k: &str| flags.get(k).and_then(|v| v.last()).cloned();
    let num = |k: &str, d: f64| get(k).and_then(|v| v.parse::<f64>().ok()).unwrap_or(d);
    if let Some(port) = flags.get("serve") {
        let port: u16 = port.last().and_then(|v| v.parse().ok()).unwrap_or(8080);
        let root = get("root").map(PathBuf::from).unwrap_or_else(|| {
            // default: the project folder = the parent of the folder holding the executable's workspace, if it has index.html
            let exe = std::env::current_exe().ok();
            let candidates: Vec<PathBuf> = exe.iter().flat_map(|e| e.ancestors().map(|a| a.to_path_buf()).collect::<Vec<_>>()).collect();
            candidates.into_iter().find(|c| c.join("index.html").exists() && c.join("src").join("app.js").exists()).unwrap_or_else(|| PathBuf::from("."))
        });
        let mirrors: Vec<String> = flags.get("overpass").cloned().unwrap_or_else(|| skyline_core::buildings::DEFAULT_OVERPASS_URLS.iter().map(|s| s.to_string()).collect());
        serve::serve(root, port, mirrors);
    }
    if flags.contains_key("help") || !flags.contains_key("lat") || !flags.contains_key("lon") {
        eprintln!("usage: skyline --lat <deg> --lon <deg> [--width 4096] [--max-km 250] [--out dir] [--formats rgba,depth,exr,svg,json] ...\n       skyline --serve 8080 [--root <project folder>]   # serve the browser app\nsee the source header for all options");
        std::process::exit(if flags.contains_key("help") { 0 } else { 2 });
    }
    let mut p = Params::default();
    p.lat = num("lat", p.lat);
    p.lon = num("lon", p.lon);
    p.eye_height = num("eye", p.eye_height);
    p.eye_altitude = get("alt").and_then(|v| v.parse().ok());
    p.eye_on_roof = flags.contains_key("roof");
    p.width = num("width", p.width as f64) as usize;
    p.max_distance = num("max-km", p.max_distance / 1000.0) * 1000.0;
    p.high_quality = flags.contains_key("high");
    p.heading_offset_deg = num("heading", 0.0);
    p.vertical_exaggeration = num("vexag", 1.0);
    p.buildings.enabled = !flags.contains_key("no-buildings");
    p.buildings.all_radius = num("all-radius", p.buildings.all_radius);
    p.buildings.tall_radius = num("tall-radius", p.buildings.tall_radius);
    p.buildings.tall_threshold = num("tall", p.buildings.tall_threshold);
    p.buildings.height_scale = num("height-scale", 1.0);
    if let Some(t) = get("threads").and_then(|v| v.parse().ok()) { p.threads = t; }
    if p.width % 2 != 0 || p.width < 64 { eprintln!("width must be even and >= 64"); std::process::exit(2); }

    let cache = get("cache").map(PathBuf::from).or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".cache/skyline-hdri/tiles")));
    let tiles = HttpTileSource::new(&get("tile-url").unwrap_or_else(|| DEFAULT_TILE_URL.into()), cache);
    let overpass_urls: Vec<String> = flags.get("overpass").cloned().unwrap_or_else(|| skyline_core::buildings::DEFAULT_OVERPASS_URLS.iter().map(|s| s.to_string()).collect());
    let buildings = HttpBuildingSource { urls: overpass_urls, timeout: Duration::from_secs(240) };
    let progress = StderrProgress { last: std::sync::Mutex::new(String::new()) };

    let t0 = std::time::Instant::now();
    let scene = prepare_scene(&p, &tiles, if p.buildings.enabled { Some(&buildings) } else { None }, &progress);
    eprintln!("scene: eye {:.1} m ASL (ground {:.1}), tiles {} ({} missing), buildings {} rasterised / {} fetched / {} skipped, {} ms",
        scene.eye_z, scene.z_ground, scene.stats.tiles, scene.stats.tiles_missing, scene.stats.buildings_rasterised, scene.stats.buildings_fetched, scene.stats.buildings_skipped, scene.stats.prepare_ms);
    let t1 = std::time::Instant::now();
    let plate = render(&scene, &progress);
    eprintln!("sweep: {} x {} in {} ms on {} threads", plate.width, plate.height, t1.elapsed().as_millis(), p.threads);

    let out_dir = PathBuf::from(get("out").unwrap_or_else(|| ".".into()));
    std::fs::create_dir_all(&out_dir).expect("output directory");
    let stem = format!("skyline_{:.4}_{:.4}_{}", p.lat, p.lon, p.width);
    let formats: Vec<String> = get("formats").unwrap_or_else(|| "rgba,depth,exr,svg,json".into()).split(',').map(|s| s.trim().to_string()).collect();
    let (sun_az, sun_el, haze) = (num("sun-az", 225.0), num("sun-el", 35.0), num("haze", 60000.0));
    let write = |name: &str, bytes: &[u8]| {
        let path = out_dir.join(name);
        std::fs::write(&path, bytes).expect("write");
        eprintln!("wrote {} ({} bytes)", path.display(), bytes.len());
    };
    for f in &formats {
        match f.as_str() {
            "rgba" => write(&format!("{}_rgba.png", stem), &encode_png(plate.width, plate.height, 6, 8, &shade_plate(&plate, ShadeMode::Shaded, sun_az, sun_el, haze)).unwrap()),
            "mask" => write(&format!("{}_mask.png", stem), &encode_png(plate.width, plate.height, 6, 8, &shade_plate(&plate, ShadeMode::Flat, sun_az, sun_el, haze)).unwrap()),
            "class" => write(&format!("{}_class.png", stem), &encode_png(plate.width, plate.height, 6, 8, &shade_plate(&plate, ShadeMode::Class, sun_az, sun_el, haze)).unwrap()),
            "depth" => write(&format!("{}_depth16.png", stem), &encode_gray16(plate.width, plate.height, &depth_to_gray16(&plate, p.max_distance)).unwrap()),
            "normal" => write(&format!("{}_normal.png", stem), &encode_png(plate.width, plate.height, 2, 8, &normal_to_rgb8(&plate)).unwrap()),
            "sdf" => { let spread = num("sdf-spread", 64.0) as f32; write(&format!("{}_sdf{}.png", stem, spread as i32), &encode_png(plate.width, plate.height, 0, 8, &sdf_to_gray8(&signed_distance_field(&plate), spread)).unwrap()) }
            "exr" => write(&format!("{}.exr", stem), &plate_to_exr(&plate, true, (p.max_distance * 10.0) as f32)),
            "exr-az" => write(&format!("{}_az.exr", stem), &plate_to_exr(&plate, false, (p.max_distance * 10.0) as f32)),
            "svg" => write(&format!("{}_bands.svg", stem), plate_to_svg(&plate, &p.bands, p.max_distance, p.lat, p.lon, p.heading_offset_deg, 0.3).as_bytes()),
            "json" => {
                let json = format!("{{\n  \"generator\": \"skyline-hdri rust 0.1\",\n  \"viewpoint\": {{\"lat\": {}, \"lon\": {}, \"eyeAltitude\": {:.3}, \"eyeHeight\": {}}},\n  \"plate\": {{\"width\": {}, \"height\": {}, \"headingOffset\": {}, \"verticalExaggeration\": {}}},\n  \"layout\": \"equirectangular; x: azimuth -180..180 deg (centre = north, east right); y: elevation +90 (top) .. -90 (bottom)\",\n  \"depth\": \"metres from the eye to the first hit (3-D distance); 16-bit PNG: v = 65535*ln(1+d)/ln(1+maxDistance), sky = 65535\",\n  \"maxDistance\": {},\n  \"bands\": {:?},\n  \"curvature\": {{\"earthRadius\": 6371000, \"refractionK\": 0.13}},\n  \"stats\": {{\"tiles\": {}, \"tilesMissing\": {}, \"buildingsFetched\": {}, \"buildingsRasterised\": {}, \"buildingsSkipped\": {}}},\n  \"attribution\": [\"Terrain: Mapzen / AWS Terrain Tiles (https://github.com/tilezen/joerd/blob/master/docs/attribution.md)\", \"Buildings: (c) OpenStreetMap contributors, ODbL\"]\n}}\n",
                    p.lat, p.lon, scene.eye_z, p.eye_height, plate.width, plate.height, p.heading_offset_deg, p.vertical_exaggeration, p.max_distance, p.bands,
                    scene.stats.tiles, scene.stats.tiles_missing, scene.stats.buildings_fetched, scene.stats.buildings_rasterised, scene.stats.buildings_skipped);
                write(&format!("{}.json", stem), json.as_bytes())
            }
            "raw" => {
                // Cross-check dump: header line, then coverage (u8), class (u8), depth (f32 LE), altitude (f32 LE), normal (3 x f32 LE), bands (f32 LE), horizon (f32 LE)
                let mut raw = Vec::new();
                raw.extend_from_slice(format!("skyline-raw {} {} {}\n", plate.width, plate.height, p.bands.len()).as_bytes());
                raw.extend_from_slice(&plate.coverage);
                raw.extend_from_slice(&plate.class_id);
                for v in &plate.depth { raw.extend_from_slice(&v.to_le_bytes()); }
                for v in &plate.altitude { raw.extend_from_slice(&v.to_le_bytes()); }
                for v in &plate.normal { raw.extend_from_slice(&v.to_le_bytes()); }
                for v in &plate.bands { raw.extend_from_slice(&v.to_le_bytes()); }
                for v in &plate.horizon { raw.extend_from_slice(&v.to_le_bytes()); }
                write(&format!("{}.raw", stem), &raw)
            }
            other => eprintln!("unknown format '{}'", other),
        }
    }
    eprintln!("done in {:.1} s", t0.elapsed().as_secs_f64());
}
