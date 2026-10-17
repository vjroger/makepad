//! CPU column sweep, parallel over columns with std threads. Twin of `src/engine/sweep.js`
//! and of the per-pixel march in `src/gpu/shaders.js`.

use crate::geo::EFFECTIVE_RADIUS;
use crate::tiers::{tier_index_for, Tier};
use std::f64::consts::PI;

pub const BISECT_STEPS: usize = 6;
pub const START_DISTANCE: f64 = 1.0;

#[inline]
pub fn step_size(d: f64, texel: f64, delta: f64) -> f64 { (0.5 * texel).max(0.5 * d * delta) }

/// Output layers, rows top-down (row 0 = +90 deg).
pub struct Plate {
    pub width: usize,
    pub height: usize,
    pub coverage: Vec<u8>,
    pub depth: Vec<f32>,
    pub class_id: Vec<u8>,
    pub normal: Vec<f32>,     // 3 per pixel (east, north, up)
    pub altitude: Vec<f32>,
    pub bands: Vec<f32>,      // band_count x width, radians
    pub horizon: Vec<f32>,    // per column, radians
    pub horizon_distance: Vec<f32>,
}

impl Plate {
    pub fn new(width: usize, band_count: usize) -> Self {
        let height = width / 2;
        let n = width * height;
        Self {
            width, height,
            coverage: vec![0; n], depth: vec![f32::INFINITY; n], class_id: vec![0; n], normal: vec![0.0; n * 3],
            altitude: vec![0.0; n], bands: vec![-(PI as f32) / 2.0; band_count * width],
            horizon: vec![-(PI as f32) / 2.0; width], horizon_distance: vec![0.0; width],
        }
    }
}

#[derive(Clone, Debug)]
pub struct SweepParams {
    pub eye_z: f64,
    pub z_ground: f64,
    pub vertical_exaggeration: f64,
    pub heading_offset: f64,   // radians
    pub max_distance: f64,
    pub bands: Vec<f64>,
}

struct ColumnOut { x: usize, rows: Vec<(usize, f32, u8, [f32; 3], f32)>, bands: Vec<f32>, horizon: f32, horizon_distance: f32 }

fn sweep_column(tiers: &[Tier], p: &SweepParams, w: usize, h: usize, x: usize) -> ColumnOut {
    let delta = 2.0 * PI / w as f64;
    let v = p.vertical_exaggeration;
    let zg = p.z_ground;
    let inv_r2 = 1.0 / (2.0 * EFFECTIVE_RADIUS);
    let height_at = |k: usize, e: f64, n: f64| -> f64 { let hh = tiers[k].sample(e, n); if v == 1.0 { hh } else { zg + (hh - zg) * v } };
    let theta = ((x as f64 + 0.5) / w as f64) * 2.0 * PI - PI + p.heading_offset;
    let (sin_t, cos_t) = (theta.sin(), theta.cos());
    let mut e_max = -PI / 2.0;
    let mut d_at_max = 0.0;
    let mut y_prev_top = h;
    let (mut d, mut d_prev) = (START_DISTANCE, 0.0);
    let mut band_idx = 0usize;
    let nb = p.bands.len();
    let mut bands = vec![-(PI as f32) / 2.0; nb];
    let mut rows = Vec::new();
    loop {
        let (east, north) = (d * sin_t, d * cos_t);
        let k = match tier_index_for(tiers, east, north) { Some(k) => k, None => break };
        let hh = height_at(k, east, north);
        let e = (hh - p.eye_z - d * d * inv_r2).atan2(d);
        if e > e_max {
            let y_top = ((h as f64 * (0.5 - e / PI) - 0.5).ceil().max(0.0)) as usize;
            for y in y_top..y_prev_top {
                let e_row = (0.5 - (y as f64 + 0.5) / h as f64) * PI;
                let tan_e = e_row.tan();
                let (mut lo, mut hi) = (d_prev, d);
                for _ in 0..BISECT_STEPS {
                    let mid = 0.5 * (lo + hi);
                    let (me, mn) = (mid * sin_t, mid * cos_t);
                    let hm = match tier_index_for(tiers, me, mn) { Some(mk) => height_at(mk, me, mn), None => -1e9 };
                    let ray = p.eye_z + mid * tan_e + mid * mid * inv_r2;
                    if hm >= ray { hi = mid; } else { lo = mid; }
                }
                let dh = hi;
                let (he, hn) = (dh * sin_t, dh * cos_t);
                let kk = tier_index_for(tiers, he, hn).unwrap_or(k);
                let h_hit = height_at(kk, he, hn);
                let mut nrm = tiers[kk].normal(he, hn);
                if v != 1.0 {
                    let len = ((nrm[0] * v).powi(2) + (nrm[1] * v).powi(2) + nrm[2].powi(2)).sqrt();
                    nrm = [nrm[0] * v / len, nrm[1] * v / len, nrm[2] / len];
                }
                let depth = (dh * dh + (h_hit - p.eye_z).powi(2)).sqrt();
                let cls = if tiers[kk].class_at(he, hn) != 0 { 2 } else { 1 };
                rows.push((y, depth as f32, cls, [nrm[0] as f32, nrm[1] as f32, nrm[2] as f32], h_hit as f32));
            }
            y_prev_top = y_top;
            e_max = e;
            d_at_max = d;
        }
        while band_idx < nb && d >= p.bands[band_idx] { bands[band_idx] = e_max as f32; band_idx += 1; }
        if d >= p.max_distance { break; }
        d_prev = d;
        d += step_size(d, tiers[k].texel, delta);
    }
    while band_idx < nb { bands[band_idx] = e_max as f32; band_idx += 1; }
    ColumnOut { x, rows, bands, horizon: e_max as f32, horizon_distance: d_at_max as f32 }
}

/// Sweep the whole plate using `threads` threads.
pub fn sweep(tiers: &[Tier], p: &SweepParams, width: usize, threads: usize, progress: &(dyn Fn(usize, usize) + Sync)) -> Plate {
    let mut plate = Plate::new(width, p.bands.len());
    let (w, h) = (plate.width, plate.height);
    let next = std::sync::atomic::AtomicUsize::new(0);
    let done = std::sync::atomic::AtomicUsize::new(0);
    let results = std::sync::Mutex::new(Vec::<ColumnOut>::with_capacity(w));
    std::thread::scope(|s| {
        for _ in 0..threads.max(1) {
            s.spawn(|| loop {
                let x = next.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                if x >= w { break; }
                let col = sweep_column(tiers, p, w, h, x);
                results.lock().unwrap().push(col);
                let d = done.fetch_add(1, std::sync::atomic::Ordering::Relaxed) + 1;
                if d % 64 == 0 || d == w { progress(d, w); }
            });
        }
    });
    for col in results.into_inner().unwrap() {
        let x = col.x;
        for (y, depth, cls, n, alt) in col.rows {
            let i = y * w + x;
            plate.coverage[i] = 255; plate.depth[i] = depth; plate.class_id[i] = cls; plate.altitude[i] = alt;
            plate.normal[i * 3..i * 3 + 3].copy_from_slice(&n);
        }
        for (b, v) in col.bands.iter().enumerate() { plate.bands[b * w + x] = *v; }
        plate.horizon[x] = col.horizon;
        plate.horizon_distance[x] = col.horizon_distance;
    }
    plate
}
