//! Terrain tiles (Mapzen terrarium PNGs) → Mercator mosaics. Twin of `src/engine/tiles.js`.

use crate::geo::mercator_pixel;
use crate::png::decode_png;

pub const TILE_SIZE: usize = 256;
pub const DEFAULT_TILE_URL: &str = "https://s3.amazonaws.com/elevation-tiles-prod/terrarium/{z}/{x}/{y}.png";

/// Where tile bytes come from (HTTP, disk cache, test fixture...). Must be thread-safe.
pub trait TileSource: Sync {
    /// PNG bytes of tile z/x/y, or None when unavailable.
    fn fetch(&self, z: u32, x: u32, y: u32) -> Option<Vec<u8>>;
}

/// Decode terrarium RGB(A) bytes into metres.
pub fn decode_terrarium(img: &crate::png::Image) -> Vec<f32> {
    let n = img.width * img.height;
    let c = img.channels;
    let mut out = vec![0f32; n];
    for i in 0..n {
        let r = img.data[i * c] as f32;
        let g = if c >= 3 { img.data[i * c + 1] as f32 } else { r };
        let b = if c >= 3 { img.data[i * c + 2] as f32 } else { r };
        out[i] = r * 256.0 + g + b / 256.0 - 32768.0;
    }
    out
}

pub struct TileRange { pub z: u32, pub x0: u32, pub y0: u32, pub nx: u32, pub ny: u32 }

/// Tile index range covering a lat/lon bounding box at zoom z, with `margin` tiles around it.
pub fn tile_range(z: u32, lat_min: f64, lon_min: f64, lat_max: f64, lon_max: f64, margin: i64) -> TileRange {
    let (pxa, pya) = mercator_pixel(lat_max, lon_min, z);
    let (pxb, pyb) = mercator_pixel(lat_min, lon_max, z);
    let n = (1i64 << z) - 1;
    let clamp = |v: i64| v.clamp(0, n) as u32;
    let x0 = clamp((pxa / 256.0).floor() as i64 - margin);
    let y0 = clamp((pya / 256.0).floor() as i64 - margin);
    let x1 = clamp((pxb / 256.0).floor() as i64 + margin);
    let y1 = clamp((pyb / 256.0).floor() as i64 + margin);
    TileRange { z, x0, y0, nx: x1 - x0 + 1, ny: y1 - y0 + 1 }
}

/// A rectangular block of tiles at one zoom, assembled into one array.
pub struct Mosaic {
    pub z: u32, pub x0: u32, pub y0: u32, pub nx: u32, pub ny: u32,
    pub width: usize, pub height: usize,
    pub data: Vec<f32>,
    pub missing: usize,
}

impl Mosaic {
    pub fn new(r: &TileRange) -> Self {
        let width = r.nx as usize * TILE_SIZE;
        let height = r.ny as usize * TILE_SIZE;
        Self { z: r.z, x0: r.x0, y0: r.y0, nx: r.nx, ny: r.ny, width, height, data: vec![0.0; width * height], missing: 0 }
    }
    /// Bilinear sample at Mercator pixel coordinates; clamps at the edges.
    pub fn sample(&self, px: f64, py: f64) -> f64 {
        let fx = px - self.x0 as f64 * 256.0 - 0.5;
        let fy = py - self.y0 as f64 * 256.0 - 0.5;
        let (w, h) = (self.width as i64, self.height as i64);
        let x0f = fx.floor();
        let y0f = fy.floor();
        let (tx, ty) = (fx - x0f, fy - y0f);
        let cl = |v: i64, m: i64| v.clamp(0, m - 1) as usize;
        let (x0, x1) = (cl(x0f as i64, w), cl(x0f as i64 + 1, w));
        let (y0, y1) = (cl(y0f as i64, h), cl(y0f as i64 + 1, h));
        let d = &self.data;
        let wu = self.width;
        let a = d[y0 * wu + x0] as f64;
        let b = d[y0 * wu + x1] as f64;
        let c = d[y1 * wu + x0] as f64;
        let e = d[y1 * wu + x1] as f64;
        (a * (1.0 - tx) + b * tx) * (1.0 - ty) + (c * (1.0 - tx) + e * tx) * ty
    }
    fn put(&mut self, tx: u32, ty: u32, tile: &[f32]) {
        let ox = (tx - self.x0) as usize * TILE_SIZE;
        let oy = (ty - self.y0) as usize * TILE_SIZE;
        for r in 0..TILE_SIZE {
            let dst = (oy + r) * self.width + ox;
            self.data[dst..dst + TILE_SIZE].copy_from_slice(&tile[r * TILE_SIZE..(r + 1) * TILE_SIZE]);
        }
    }
}

/// Load every tile of a range using `threads` worker threads.
pub fn load_mosaic(range: &TileRange, source: &dyn TileSource, threads: usize, progress: &(dyn Fn(usize, usize) + Sync)) -> Mosaic {
    let mut mosaic = Mosaic::new(range);
    let jobs: Vec<(u32, u32)> = (range.y0..range.y0 + range.ny)
        .flat_map(|ty| (range.x0..range.x0 + range.nx).map(move |tx| (tx, ty)))
        .collect();
    let total = jobs.len();
    let next = std::sync::atomic::AtomicUsize::new(0);
    let done = std::sync::atomic::AtomicUsize::new(0);
    let results: std::sync::Mutex<Vec<(u32, u32, Option<Vec<f32>>)>> = std::sync::Mutex::new(Vec::with_capacity(total));
    let z = range.z;
    std::thread::scope(|s| {
        for _ in 0..threads.max(1) {
            s.spawn(|| loop {
                let i = next.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                if i >= jobs.len() { break; }
                let (tx, ty) = jobs[i];
                let tile = source.fetch(z, tx, ty).and_then(|bytes| decode_png(&bytes).ok()).map(|img| decode_terrarium(&img));
                let tile = tile.filter(|t| t.len() == TILE_SIZE * TILE_SIZE);
                results.lock().unwrap().push((tx, ty, tile));
                let d = done.fetch_add(1, std::sync::atomic::Ordering::Relaxed) + 1;
                progress(d, total);
            });
        }
    });
    for (tx, ty, tile) in results.into_inner().unwrap() {
        match tile { Some(t) => mosaic.put(tx, ty, &t), None => mosaic.missing += 1 }
    }
    mosaic
}
