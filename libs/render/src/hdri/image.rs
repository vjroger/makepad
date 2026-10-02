//! Image files for the HDRI generator: the sRGB curves, the quarter-width column roll between
//! the engine's equirect convention and the file convention, and readers for OpenEXR, the 8-bit
//! formats the image cache decodes, and Radiance `.hdr` (through `ibl::load_hdr`). Writers live
//! in `export.rs`.
//!
//! Everything here is linear Rec.709 radiance in an `EnvMap` (alpha 1). sRGB appears only where
//! 8-bit files come in. Nothing here samples, resizes or decodes RGBE itself: `EnvMap` and `ibl`
//! already do that.
//!
//! Two conventions meet here. The engine (`ibl::dir_to_equirect_uv`) puts −Z at the image centre
//! and +X at u = 0.75; files (Blender, Unity, three.js, DrawPbr) put +X at the centre. They differ
//! by exactly a quarter turn, `u_file = u_engine − 0.25`, so a file is a column roll of the
//! engine map. `decode_image` and `read_exr` return the file's columns untouched (the file
//! convention); `load_equirect` rolls them into the engine convention, and the writers in
//! `export.rs` roll the other way.

use makepad_draw::*;
use makepad_draw::makepad_platform::resource_resolver::DecodeBudget;
use makepad_openexr::{read_from_slice, ExrPart, SampleBuffer};
use makepad_render_material::ibl::{self, EnvMap};
use std::path::Path;
use super::vec;

/// The exact sRGB decoding curve (IEC 61966-2-1), for 8-bit colours coming into the linear model.
pub fn srgb_to_linear(c: f32) -> f32 {
    if c <= 0.04045 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}

/// The exact sRGB encoding curve, the inverse of `srgb_to_linear`. It does not clamp; callers
/// clamp to 0..1 first when they want a displayable value.
pub fn linear_to_srgb(c: f32) -> f32 {
    if c <= 0.003_130_8 {
        c * 12.92
    } else {
        1.055 * c.powf(1.0 / 2.4) - 0.055
    }
}

/// Lossless quarter-width column roll between the engine convention (−Z at the centre, +X at
/// u = 0.75) and the file convention (+X at the centre). `to_file` rolls engine → file
/// (`u_file = u − 0.25`); otherwise file → engine. Exact when `width % 4 == 0` (every size the
/// app bakes, 512 up to 8192); other widths are resampled bilinearly at the texel centres, which
/// blurs by up to half a texel and is the only lossy path. Any height works: the roll only moves
/// columns, so it also serves files that are a pixel off 2:1.
pub fn roll_quarter(env: &EnvMap, to_file: bool) -> EnvMap {
    let (width, height) = (env.width, env.height);
    if width == 0 || height == 0 || env.data.len() < width * height {
        return env.clone();
    }
    let mut data = Vec::with_capacity(width * height);
    if width % 4 == 0 {
        // Engine column x holds u = (x + 0.5) / w. That content sits a quarter turn to the left
        // in the file, so file column x reads engine column x + w/4; the way back reads x − w/4
        // (written as x + 3w/4 to stay unsigned). Both wrap at the seam.
        let shift = if to_file { width / 4 } else { width - width / 4 };
        for y in 0..height {
            let row = &env.data[y * width..(y + 1) * width];
            for x in 0..width {
                data.push(row[(x + shift) % width]);
            }
        }
    } else {
        // A fractional shift: read each output texel centre a quarter turn away. `sample_uv`
        // wraps u, so the seam needs no special case.
        let du = if to_file { 0.25 } else { -0.25 };
        for y in 0..height {
            let v = (y as f32 + 0.5) / height as f32;
            for x in 0..width {
                let u = (x as f32 + 0.5) / width as f32 + du;
                data.push(env.sample_uv([u, v]));
            }
        }
    }
    EnvMap { width, height, data }
}

/// Luminance of the map's solid-angle-weighted mean radiance: the auto-exposure meter. An
/// empty map meters 0 rather than NaN.
pub fn mean_luminance(env: &EnvMap) -> f32 {
    if env.width == 0 || env.height == 0 || env.data.len() < env.width * env.height {
        return 0.0;
    }
    let l = crate::sky::luminance(vec(env.mean()));
    if l.is_finite() {
        l
    } else {
        0.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Texel (x, y) holds (x, y, 1, 1), so a texel shows where it came from.
    fn ramp(width: usize, height: usize) -> EnvMap {
        let mut data = Vec::with_capacity(width * height);
        for y in 0..height {
            for x in 0..width {
                data.push([x as f32, y as f32, 1.0, 1.0]);
            }
        }
        EnvMap { width, height, data }
    }

    fn texel(env: &EnvMap, x: usize, y: usize) -> [f32; 4] {
        env.data[y * env.width + x]
    }

    #[test]
    fn srgb_curves_are_exact_and_inverse() {
        assert_eq!(srgb_to_linear(0.0), 0.0);
        assert!((srgb_to_linear(1.0) - 1.0).abs() < 1.0e-6);
        assert!((srgb_to_linear(0.5) - 0.214_041_14).abs() < 1.0e-6);
        assert!((linear_to_srgb(0.18) - 0.461_356).abs() < 1.0e-4);
        // The linear and power pieces meet at the knee, in both directions.
        assert!((srgb_to_linear(0.04045) - srgb_to_linear(0.040_451)).abs() < 1.0e-6);
        assert!((linear_to_srgb(0.003_130_8) - linear_to_srgb(0.003_131)).abs() < 1.0e-5);
        for i in 0..=1000 {
            let x = i as f32 / 1000.0;
            assert!((linear_to_srgb(srgb_to_linear(x)) - x).abs() < 1.0e-5, "{x}");
        }
    }

    #[test]
    fn roll_quarter_is_an_exact_column_roll_for_widths_divisible_by_four() {
        let engine = ramp(8, 4);
        let file = roll_quarter(&engine, true);
        assert_eq!((file.width, file.height), (8, 4));
        // The engine puts +X at u = 0.75: column 6 of 8. In the file it is the centre column, 4.
        let plus_x = ibl::dir_to_equirect_uv([1.0, 0.0, 0.0]);
        assert_eq!((plus_x[0] * 8.0).floor() as usize, 6);
        assert_eq!(texel(&file, 4, 1), texel(&engine, 6, 1));
        // North (−Z), the engine's centre column 4, lands at u = 0.25 in the file: column 2.
        assert_eq!(texel(&file, 2, 3), texel(&engine, 4, 3));
        // Every column moved by exactly a quarter, wrapping at the seam.
        for y in 0..4 {
            for x in 0..8 {
                assert_eq!(texel(&file, x, y), texel(&engine, (x + 2) % 8, y), "({x},{y})");
            }
        }
        // The way back is the inverse, bit for bit, in both orders.
        assert_eq!(roll_quarter(&file, false), engine);
        assert_eq!(roll_quarter(&roll_quarter(&engine, false), true), engine);
        // An empty map rolls to an empty map instead of panicking.
        let empty = EnvMap { width: 0, height: 0, data: Vec::new() };
        assert_eq!(roll_quarter(&empty, true), empty);
    }

    #[test]
    fn roll_quarter_resamples_other_widths_and_keeps_the_energy() {
        // 6 columns: a quarter turn is 1.5 texels, so the roll is a bilinear resample.
        let mut engine = ramp(6, 2);
        // A smooth ring pattern; ramp's x would jump from 5 to 0 at the seam.
        for (i, t) in engine.data.iter_mut().enumerate() {
            let x = (i % 6) as f32;
            *t = [(x * std::f32::consts::TAU / 6.0).sin() + 2.0, 1.0, 0.5, 1.0];
        }
        let file = roll_quarter(&engine, true);
        assert_eq!((file.width, file.height), (6, 2));
        // File column x reads the engine at u + 0.25: halfway between engine columns x+1 and x+2.
        for x in 0..6 {
            let want = engine.sample_uv([(x as f32 + 0.5) / 6.0 + 0.25, 0.25]);
            assert_eq!(texel(&file, x, 0), want, "column {x}");
            let mid = [texel(&engine, (x + 1) % 6, 0)[0], texel(&engine, (x + 2) % 6, 0)[0]];
            assert!((texel(&file, x, 0)[0] - (mid[0] + mid[1]) * 0.5).abs() < 1.0e-6);
        }
        // A wrap-around linear resample moves energy around; it neither creates nor loses any.
        let sum = |e: &EnvMap| e.data.iter().map(|t| t[0] as f64).sum::<f64>();
        assert!((sum(&file) - sum(&engine)).abs() < 1.0e-4);
        // Alpha stays 1 and the other channels stay flat.
        assert!(file.data.iter().all(|t| t[1] == 1.0 && t[2] == 0.5 && t[3] == 1.0));
        // The way back is also a resample; it lands within a texel's blur of the source.
        let back = roll_quarter(&file, false);
        for x in 0..6 {
            assert!((texel(&back, x, 0)[0] - texel(&engine, x, 0)[0]).abs() < 0.5, "column {x}");
        }
    }

    #[test]
    fn mean_luminance_meters_the_solid_angle_mean() {
        let grey = EnvMap::constant(16, [0.5, 0.5, 0.5]);
        assert!((mean_luminance(&grey) - 0.5).abs() < 1.0e-5);
        // Rec.709 weights: pure green weighs 0.7152.
        let green = EnvMap::constant(16, [0.0, 1.0, 0.0]);
        assert!((mean_luminance(&green) - 0.7152).abs() < 1.0e-5);
        // A bright top row counts less than a bright equator row: the meter is solid-angle weighted.
        let mut top = EnvMap::constant(16, [0.0, 0.0, 0.0]);
        let mut equator = top.clone();
        for x in 0..16 {
            top.data[x] = [10.0, 10.0, 10.0, 1.0];
            equator.data[4 * 16 + x] = [10.0, 10.0, 10.0, 1.0];
        }
        assert!(mean_luminance(&top) < mean_luminance(&equator) * 0.5);
        // An empty map meters 0 rather than NaN.
        assert_eq!(mean_luminance(&EnvMap { width: 0, height: 0, data: Vec::new() }), 0.0);
    }
}
