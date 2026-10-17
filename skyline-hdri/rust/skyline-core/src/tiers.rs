//! Height-model tiers in the azimuthal-equidistant frame. Twin of `src/engine/tiers.js`.

use crate::geo::{destination, flat_offset, mercator_pixel, DEG, EARTH_RADIUS};
use crate::tiles::{load_mosaic, tile_range, Mosaic, TileSource};
use std::collections::HashMap;

#[derive(Clone, Copy, Debug)]
pub struct TierSpec { pub size: usize, pub texel: f64, pub zoom: u32, pub zoom_high: u32 }

/// Default ladder; radius = size / 2 * texel.
pub const DEFAULT_TIERS: [TierSpec; 6] = [
    TierSpec { size: 1024, texel: 0.5, zoom: 15, zoom_high: 15 },
    TierSpec { size: 2048, texel: 2.0, zoom: 15, zoom_high: 15 },
    TierSpec { size: 2048, texel: 10.0, zoom: 13, zoom_high: 14 },
    TierSpec { size: 2048, texel: 50.0, zoom: 10, zoom_high: 11 },
    TierSpec { size: 2048, texel: 250.0, zoom: 8, zoom_high: 9 },
    TierSpec { size: 2048, texel: 1000.0, zoom: 6, zoom_high: 7 },
];

pub const FLAT_LIMIT: f64 = 3000.0;

pub fn tier_radius(t: &TierSpec) -> f64 { t.size as f64 / 2.0 * t.texel }

#[derive(Clone, Copy, Debug)]
pub struct TierPlan { pub size: usize, pub texel: f64, pub zoom: u32, pub radius: f64 }

/// Pick the tiers needed to reach `max_distance` metres.
pub fn plan_tiers(max_distance: f64, high_quality: bool) -> Vec<TierPlan> {
    let mut out = Vec::new();
    for t in DEFAULT_TIERS.iter() {
        out.push(TierPlan { size: t.size, texel: t.texel, zoom: if high_quality { t.zoom_high } else { t.zoom }, radius: tier_radius(t) });
        if tier_radius(t) >= max_distance { break; }
    }
    out
}

pub struct Tier {
    pub size: usize,
    pub texel: f64,
    pub radius: f64,
    pub zoom: u32,
    pub height: Vec<f32>,
    pub cls: Vec<u8>,
}

pub fn bbox_around(lat0: f64, lon0: f64, r: f64) -> (f64, f64, f64, f64) {
    let d_lat = (r / EARTH_RADIUS) / DEG;
    let d_lon = (r / (EARTH_RADIUS * (lat0 * DEG).cos())) / DEG;
    (lat0 - d_lat, lon0 - d_lon, lat0 + d_lat, lon0 + d_lon)
}

/// One Mercator mosaic per distinct zoom, sized for the largest tier using it. Returns (mosaics, tile count).
pub fn load_mosaics_for_tiers(tiers: &[TierPlan], lat0: f64, lon0: f64, source: &dyn TileSource, threads: usize,
                              progress: &(dyn Fn(usize, usize, u32) + Sync)) -> (HashMap<u32, Mosaic>, usize) {
    let mut by_zoom: HashMap<u32, f64> = HashMap::new();
    for t in tiers {
        let r = t.radius * 1.02;
        let e = by_zoom.entry(t.zoom).or_insert(r);
        if *e < r { *e = r; }
    }
    let mut zooms: Vec<u32> = by_zoom.keys().copied().collect();
    zooms.sort_unstable();
    let ranges: Vec<_> = zooms.iter().map(|&z| {
        let (lat_min, lon_min, lat_max, lon_max) = bbox_around(lat0, lon0, by_zoom[&z]);
        tile_range(z, lat_min, lon_min, lat_max, lon_max, 1)
    }).collect();
    let total: usize = ranges.iter().map(|r| (r.nx * r.ny) as usize).sum();
    let mut done_before = 0usize;
    let mut mosaics = HashMap::new();
    for range in &ranges {
        let z = range.z;
        let m = load_mosaic(range, source, threads, &|d, _t| progress(done_before + d, total, z));
        done_before += (range.nx * range.ny) as usize;
        mosaics.insert(z, m);
    }
    (mosaics, total)
}

/// Resample a mosaic into a tier (separable flat frame below FLAT_LIMIT, spherical beyond).
pub fn build_tier(spec: &TierPlan, mosaic: &Mosaic, lat0: f64, lon0: f64, clamp_sea: bool) -> Tier {
    let size = spec.size;
    let texel = spec.texel;
    let half = size as f64 / 2.0;
    let mut height = vec![0f32; size * size];
    let cls = vec![0u8; size * size];
    if spec.radius < FLAT_LIMIT {
        let px: Vec<f64> = (0..size).map(|i| { let (lat, lon) = flat_offset(lat0, lon0, (i as f64 - half + 0.5) * texel, 0.0); mercator_pixel(lat, lon, spec.zoom).0 }).collect();
        let py: Vec<f64> = (0..size).map(|j| { let (lat, lon) = flat_offset(lat0, lon0, 0.0, (j as f64 - half + 0.5) * texel); mercator_pixel(lat, lon, spec.zoom).1 }).collect();
        for j in 0..size {
            for i in 0..size {
                let mut h = mosaic.sample(px[i], py[j]);
                if clamp_sea && h < 0.0 { h = 0.0; }
                height[j * size + i] = h as f32;
            }
        }
    } else {
        let rows: Vec<Vec<f32>> = {
            let threads = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(2).max(1);
            let chunk = (size + threads - 1) / threads;
            let mut parts: Vec<Vec<f32>> = Vec::new();
            std::thread::scope(|s| {
                let handles: Vec<_> = (0..threads).map(|t| {
                    let j0 = t * chunk;
                    let j1 = ((t + 1) * chunk).min(size);
                    s.spawn(move || {
                        let mut v = Vec::with_capacity((j1.saturating_sub(j0)) * size);
                        for j in j0..j1 {
                            let north = (j as f64 - half + 0.5) * texel;
                            for i in 0..size {
                                let east = (i as f64 - half + 0.5) * texel;
                                let d = (east * east + north * north).sqrt();
                                let (lat, lon) = if d < 1.0 { (lat0, lon0) } else { destination(lat0, lon0, east.atan2(north), d) };
                                let (mx, my) = mercator_pixel(lat, lon, spec.zoom);
                                let mut h = mosaic.sample(mx, my);
                                if clamp_sea && h < 0.0 { h = 0.0; }
                                v.push(h as f32);
                            }
                        }
                        v
                    })
                }).collect();
                for h in handles { parts.push(h.join().unwrap()); }
            });
            parts
        };
        let mut off = 0;
        for part in rows { height[off..off + part.len()].copy_from_slice(&part); off += part.len(); }
    }
    Tier { size, texel, radius: spec.radius, zoom: spec.zoom, height, cls }
}

impl Tier {
    /// Bilinear height at (east, north) metres. Mirrors sampleTier() in the shader.
    #[inline]
    pub fn sample(&self, east: f64, north: f64) -> f64 {
        let size = self.size;
        let fi = east / self.texel + size as f64 / 2.0 - 0.5;
        let fj = north / self.texel + size as f64 / 2.0 - 0.5;
        let i0f = fi.floor();
        let j0f = fj.floor();
        let (tx, ty) = (fi - i0f, fj - j0f);
        let m = size as i64 - 1;
        let cl = |v: i64| v.clamp(0, m) as usize;
        let (i0, i1) = (cl(i0f as i64), cl(i0f as i64 + 1));
        let (j0, j1) = (cl(j0f as i64), cl(j0f as i64 + 1));
        let h = &self.height;
        let a = h[j0 * size + i0] as f64;
        let b = h[j0 * size + i1] as f64;
        let c = h[j1 * size + i0] as f64;
        let d = h[j1 * size + i1] as f64;
        let top = a + (b - a) * tx;
        top + ((c + (d - c) * tx) - top) * ty
    }
    /// Nearest-texel class at (east, north).
    #[inline]
    pub fn class_at(&self, east: f64, north: f64) -> u8 {
        let m = self.size as i64 - 1;
        let i = ((east / self.texel + self.size as f64 / 2.0 - 0.5).round() as i64).clamp(0, m) as usize;
        let j = ((north / self.texel + self.size as f64 / 2.0 - 0.5).round() as i64).clamp(0, m) as usize;
        self.cls[j * self.size + i]
    }
    /// Unit normal (east, north, up) from central differences of the bilinear height.
    pub fn normal(&self, east: f64, north: f64) -> [f64; 3] {
        let t = self.texel;
        let dx = (self.sample(east + t, north) - self.sample(east - t, north)) / (2.0 * t);
        let dy = (self.sample(east, north + t) - self.sample(east, north - t)) / (2.0 * t);
        let len = (dx * dx + dy * dy + 1.0).sqrt();
        [-dx / len, -dy / len, 1.0 / len]
    }
}

/// Index of the finest tier containing (east, north) with a 2-texel margin, or None.
#[inline]
pub fn tier_index_for(tiers: &[Tier], east: f64, north: f64) -> Option<usize> {
    let m = east.abs().max(north.abs());
    tiers.iter().position(|t| m < t.radius - 2.0 * t.texel)
}

pub fn max_height(tiers: &[Tier]) -> f64 {
    tiers.iter().flat_map(|t| t.height.iter()).fold(f64::NEG_INFINITY, |m, &v| m.max(v as f64))
}
