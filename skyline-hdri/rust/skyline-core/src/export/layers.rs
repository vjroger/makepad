//! Raster layers derived from a plate. Twin of `src/export/layers.js`.

use crate::sweep::Plate;

const INF: f64 = 1e20;

fn edt1d(f: &[f64], n: usize, d: &mut [f64], v: &mut [usize], z: &mut [f64]) {
    let mut k = 0usize;
    v[0] = 0; z[0] = -INF; z[1] = INF;
    for q in 1..n {
        let mut s = ((f[q] + (q * q) as f64) - (f[v[k]] + (v[k] * v[k]) as f64)) / (2.0 * q as f64 - 2.0 * v[k] as f64);
        while s <= z[k] {
            k -= 1;
            s = ((f[q] + (q * q) as f64) - (f[v[k]] + (v[k] * v[k]) as f64)) / (2.0 * q as f64 - 2.0 * v[k] as f64);
        }
        k += 1;
        v[k] = q; z[k] = s; z[k + 1] = INF;
    }
    k = 0;
    for q in 0..n {
        while z[k + 1] < q as f64 { k += 1; }
        let dq = q as f64 - v[k] as f64;
        d[q] = dq * dq + f[v[k]];
    }
}

/// Squared distance from every pixel to the nearest pixel where `is_feature` holds.
pub fn distance_transform(width: usize, height: usize, is_feature: &dyn Fn(usize) -> bool) -> Vec<f64> {
    let m = width.max(height);
    let (mut f, mut d, mut v, mut z) = (vec![0.0; m], vec![0.0; m], vec![0usize; m], vec![0.0; m + 1]);
    let mut g = vec![0.0; width * height];
    for x in 0..width {
        for y in 0..height { f[y] = if is_feature(y * width + x) { 0.0 } else { INF }; }
        edt1d(&f, height, &mut d, &mut v, &mut z);
        for y in 0..height { g[y * width + x] = d[y]; }
    }
    let mut out = vec![0.0; width * height];
    for y in 0..height {
        f[..width].copy_from_slice(&g[y * width..(y + 1) * width]);
        edt1d(&f, width, &mut d, &mut v, &mut z);
        out[y * width..(y + 1) * width].copy_from_slice(&d[..width]);
    }
    out
}

/// Signed distance to the silhouette in pixels: positive in the sky, negative inside occluders.
pub fn signed_distance_field(plate: &Plate) -> Vec<f32> {
    let to_occ = distance_transform(plate.width, plate.height, &|i| plate.coverage[i] != 0);
    let to_sky = distance_transform(plate.width, plate.height, &|i| plate.coverage[i] == 0);
    (0..plate.width * plate.height).map(|i| if plate.coverage[i] != 0 { -(to_sky[i].sqrt()) as f32 } else { to_occ[i].sqrt() as f32 }).collect()
}

pub fn sdf_to_gray8(sdf: &[f32], spread: f32) -> Vec<u8> {
    sdf.iter().map(|&s| (128.0 + s / spread * 127.0).round().clamp(0.0, 255.0) as u8).collect()
}

/// 16-bit log depth: 65535 * ln(1 + d) / ln(1 + max_distance); sky = 65535.
pub fn depth_to_gray16(plate: &Plate, max_distance: f64) -> Vec<u16> {
    let k = 65535.0 / (1.0 + max_distance).ln();
    (0..plate.width * plate.height).map(|i| {
        if plate.coverage[i] == 0 { 65535 } else { ((1.0 + plate.depth[i].max(0.0) as f64).ln() * k).round().min(65535.0) as u16 }
    }).collect()
}

pub fn normal_to_rgb8(plate: &Plate) -> Vec<u8> {
    let n = plate.width * plate.height;
    let mut out = vec![0u8; n * 3];
    for i in 0..n {
        if plate.coverage[i] == 0 { out[i * 3] = 128; out[i * 3 + 1] = 128; out[i * 3 + 2] = 255; continue; }
        for c in 0..3 { out[i * 3 + c] = ((plate.normal[i * 3 + c] * 0.5 + 0.5) * 255.0).round().clamp(0.0, 255.0) as u8; }
    }
    out
}

#[derive(Clone, Copy, PartialEq)]
pub enum ShadeMode { Shaded, Flat, Class }

/// Preview RGBA (sRGB, straight alpha). Sun azimuth clockwise from north, elevation, both in degrees.
pub fn shade_plate(plate: &Plate, mode: ShadeMode, sun_az_deg: f64, sun_el_deg: f64, haze_distance: f64) -> Vec<u8> {
    let n = plate.width * plate.height;
    let mut out = vec![0u8; n * 4];
    let (az, el) = (sun_az_deg.to_radians(), sun_el_deg.to_radians());
    let (sx, sy, sz) = (az.sin() * el.cos(), az.cos() * el.cos(), el.sin());
    let tint = [[0.0, 0.0, 0.0], [0.50, 0.49, 0.45], [0.66, 0.63, 0.60]];
    let class_color = [[0.0, 0.0, 0.0], [0.35, 0.55, 0.30], [0.80, 0.45, 0.35]];
    let haze = [0.72, 0.78, 0.86];
    let gamma = |v: f64| (v.clamp(0.0, 1.0).powf(1.0 / 2.2) * 255.0).round() as u8;
    for i in 0..n {
        let o = i * 4;
        if plate.coverage[i] == 0 { continue; }
        out[o + 3] = 255;
        let c = plate.class_id[i] as usize;
        match mode {
            ShadeMode::Flat => { out[o] = 255; out[o + 1] = 255; out[o + 2] = 255; }
            ShadeMode::Class => { for k in 0..3 { out[o + k] = gamma(class_color[c][k]); } }
            ShadeMode::Shaded => {
                let (nx, ny, nz) = (plate.normal[i * 3] as f64, plate.normal[i * 3 + 1] as f64, plate.normal[i * 3 + 2] as f64);
                let diff = (nx * sx + ny * sy + nz * sz).max(0.0);
                let l = 0.28 + 0.72 * diff;
                let f = 1.0 - (-(plate.depth[i] as f64) / haze_distance).exp();
                for k in 0..3 { out[o + k] = gamma(tint[c][k] * l * (1.0 - f) + haze[k] * f); }
            }
        }
    }
    out
}
