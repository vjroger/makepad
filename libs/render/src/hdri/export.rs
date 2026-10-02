//! Export of baked maps: OpenEXR (ZIP, float or half), Radiance `.hdr` (new-style RLE),
//! tonemapped PNG, the six cube faces, and `export_all`, which writes a set of them next to one
//! base path. Every file goes through `write_atomic`. A failed or cancelled export removes what
//! it wrote, so it never leaves half a set behind.
//!
//! Every equirect encoder takes the ENGINE convention (−Z at the centre, what `Env::bake`
//! produces) and rolls it to the file convention (+X at the centre) itself through
//! `image::roll_quarter`, so a file opens the right way round in Blender, three.js and DrawPbr,
//! and `image::load_equirect` brings it back bit for bit. Cube faces are sampled by direction
//! and need no roll.

use makepad_draw::*;
use makepad_half::f16;
use makepad_openexr::{write_to_vec, Compression, ExrChannel, ExrImage, ExrPart};
use makepad_render_material::ibl::{self, EnvMap};
use super::image::{roll_quarter, sanitize_radiance};
use super::vec;

/// The largest finite half float. Anything brighter clips when written as half.
const HALF_MAX: f32 = 65504.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExrPrecision {
    Float,
    Half,
}

impl Default for ExrPrecision {
    /// Float, because a clear sun disc is far brighter than half can hold.
    fn default() -> Self {
        ExrPrecision::Float
    }
}

/// What an EXR write had to give up. When writing half, `clipped` counts the pixels that had a
/// channel above 65504; those channels are clamped to 65504. The UI shows the count as a warning.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ExrReport {
    pub clipped: usize,
}

/// One cube face: `size` × `size` linear RGB triples, row-major, top row first.
#[derive(Clone, Debug, PartialEq)]
pub struct FaceImage {
    pub size: usize,
    pub rgb: Vec<f32>,
}

/// Returns an error unless the map has a size and a buffer of exactly width × height texels.
fn check_map(env: &EnvMap, what: &str) -> Result<(), String> {
    if env.width == 0 || env.height == 0 || env.data.len() != env.width * env.height {
        return Err(format!(
            "{what}: the map is empty or its buffer does not match {}x{}",
            env.width, env.height
        ));
    }
    Ok(())
}

/// Returns an error unless the face is square and complete.
fn check_face(face: &FaceImage, what: &str) -> Result<(), String> {
    if face.size == 0 || face.rgb.len() != face.size * face.size * 3 {
        return Err(format!("{what}: the face is empty or its buffer does not match {}x{}", face.size, face.size));
    }
    Ok(())
}

/// Encodes a single-part scanline OpenEXR with R, G and B channels and ZIP compression (16 rows
/// per block) from `count = width × height` texels of `texel(i)`. Float keeps every value. Half
/// clamps to 65504 and reports how many pixels it clipped. NaN and negatives are written as 0.
fn encode_rgb_exr(width: usize, height: usize, texel: impl Fn(usize) -> [f32; 3], precision: ExrPrecision) -> Result<(Vec<u8>, ExrReport), String> {
    let count = width * height;
    let mut planes = [Vec::with_capacity(count), Vec::with_capacity(count), Vec::with_capacity(count)];
    for i in 0..count {
        let t = texel(i);
        for (plane, &value) in planes.iter_mut().zip(&t) {
            plane.push(sanitize_radiance(value));
        }
    }
    let [r, g, b] = planes;
    let mut report = ExrReport::default();
    let channels = match precision {
        ExrPrecision::Float => vec![ExrChannel::float("R", r), ExrChannel::float("G", g), ExrChannel::float("B", b)],
        ExrPrecision::Half => {
            // Half tops out at 65504, and a clear sun disc is far brighter. Clamp there rather than
            // overflow to infinity, and count the pixels that lost energy for the UI's warning.
            report.clipped = (0..count)
                .filter(|&i| r[i] > HALF_MAX || g[i] > HALF_MAX || b[i] > HALF_MAX)
                .count();
            let half = |plane: Vec<f32>| plane.into_iter().map(|v| f16::from_f32(v.min(HALF_MAX))).collect::<Vec<f16>>();
            vec![ExrChannel::half("R", half(r)), ExrChannel::half("G", half(g)), ExrChannel::half("B", half(b))]
        }
    };
    let part = ExrPart::new(None, width, height, Compression::Zip, channels);
    let bytes = write_to_vec(&ExrImage::single(part)).map_err(|e| format!("EXR: {e}"))?;
    Ok((bytes, report))
}

/// Encodes the map as an OpenEXR in the file convention (+X at the centre). See `encode_rgb_exr`
/// for the precision rules.
pub fn encode_exr(env: &EnvMap, precision: ExrPrecision) -> Result<(Vec<u8>, ExrReport), String> {
    check_map(env, "EXR")?;
    let file = roll_quarter(env, true);
    encode_rgb_exr(file.width, file.height, |i| [file.data[i][0], file.data[i][1], file.data[i][2]], precision)
}

/// One face as an OpenEXR, with the clip report kept for `export_all`.
fn face_exr(face: &FaceImage, precision: ExrPrecision) -> Result<(Vec<u8>, ExrReport), String> {
    check_face(face, "EXR")?;
    encode_rgb_exr(face.size, face.size, |i| [face.rgb[i * 3], face.rgb[i * 3 + 1], face.rgb[i * 3 + 2]], precision)
}

/// One cube face as an OpenEXR (same precision rules as `encode_exr`; clipped pixels are clamped
/// but not reported here, `export_all` folds their count into its report).
pub fn encode_face_exr(face: &FaceImage, precision: ExrPrecision) -> Result<Vec<u8>, String> {
    face_exr(face, precision).map(|(bytes, _)| bytes)
}

/// The brightest channel value RGBE can hold without the exponent byte overflowing. Beyond 2^127
/// `ibl::float_to_rgbe` clamps the exponent and the texel decodes at half its value or, for an
/// infinity, panics in `floor() as i32 + 1`; saturating a stop below keeps every texel exact.
fn rgbe_storable(x: f32) -> f32 {
    if x.is_nan() {
        0.0
    } else {
        x.clamp(0.0, 8.5e37)
    }
}

/// Writes a Radiance `.hdr` in the file convention: the standard header, `-Y H +X W` (top row
/// first), then one new-style RLE scanline per row. Rows narrower than 8 or wider than 32767
/// texels cannot be run-length coded, so they are written flat, as the format requires (and as
/// `ibl::load_hdr` expects). An empty or short map writes a header with no pixel data.
pub fn encode_hdr(env: &EnvMap) -> Vec<u8> {
    let file = roll_quarter(env, true);
    let (width, height) = (file.width, file.height);
    let mut out = Vec::with_capacity(width * height * 4 + 96);
    out.extend_from_slice(b"#?RADIANCE\n# Made with Makepad HDRI\nFORMAT=32-bit_rle_rgbe\n\n");
    out.extend_from_slice(format!("-Y {height} +X {width}\n").as_bytes());
    if file.data.len() < width * height {
        return out;
    }
    let rle = (8..=0x7fff).contains(&width);
    let mut scan = vec![[0u8; 4]; width];
    let mut channel = vec![0u8; width];
    for y in 0..height {
        for (x, texel) in scan.iter_mut().enumerate() {
            let t = file.data[y * width + x];
            *texel = ibl::float_to_rgbe([rgbe_storable(t[0]), rgbe_storable(t[1]), rgbe_storable(t[2])]);
        }
        if !rle {
            for texel in &scan {
                out.extend_from_slice(texel);
            }
            continue;
        }
        out.extend_from_slice(&[2, 2, (width >> 8) as u8, (width & 0xff) as u8]);
        for c in 0..4 {
            for (value, texel) in channel.iter_mut().zip(&scan) {
                *value = texel[c];
            }
            rle_component(&channel, &mut out);
        }
    }
    out
}

/// Run-length codes one byte stream the way Radiance's writer does:
/// - a run of at least 4 equal bytes becomes (128 + n, value), with n ≤ 127;
/// - everything between runs becomes literals (n, bytes…), with n ≤ 128;
/// - a run of 2 or 3 just before a long run is also written as a run, since that is cheaper than
///   a literal.
/// `ibl::load_hdr` reads exactly this: count > 128 is a run of count − 128, else a literal.
fn rle_component(data: &[u8], out: &mut Vec<u8>) {
    const MIN_RUN: usize = 4;
    let n = data.len();
    let mut cur = 0;
    while cur < n {
        // Find where the next run of at least MIN_RUN starts, remembering the short run (if any)
        // just before it.
        let mut beg_run = cur;
        let mut run_count = 0usize;
        let mut old_run_count = 0usize;
        while run_count < MIN_RUN && beg_run < n {
            beg_run += run_count;
            old_run_count = run_count;
            run_count = 1;
            while beg_run + run_count < n && run_count < 127 && data[beg_run] == data[beg_run + run_count] {
                run_count += 1;
            }
        }
        if old_run_count > 1 && old_run_count == beg_run - cur {
            out.push(128 + old_run_count as u8);
            out.push(data[cur]);
            cur = beg_run;
        }
        while cur < beg_run {
            let count = (beg_run - cur).min(128);
            out.push(count as u8);
            out.extend_from_slice(&data[cur..cur + count]);
            cur += count;
        }
        if run_count >= MIN_RUN {
            out.push(128 + run_count as u8);
            out.push(data[beg_run]);
            cur += run_count;
        }
    }
}

/// Tonemaps linear texels to opaque 8-bit RGBA through `sky::display_rgb` (ACES fit, then gamma
/// 2.2) at an exposure of 2^exposure_ev, the same transform the preview shows. EV is clamped to
/// ±20. A NaN texel comes out black, not garbage.
fn tonemap_rgba(texels: impl Iterator<Item = [f32; 3]>, exposure_ev: f32) -> Vec<u8> {
    let ev = if exposure_ev.is_finite() { exposure_ev.clamp(-20.0, 20.0) } else { 0.0 };
    let exposure = 2.0f32.powf(ev);
    // NaN clamps to NaN and then casts to 0.
    let to_byte = |v: f32| (v.clamp(0.0, 1.0) * 255.0 + 0.5) as u8;
    let mut rgba = Vec::new();
    for p in texels {
        let d = crate::sky::display_rgb(vec(p), exposure);
        rgba.extend_from_slice(&[to_byte(d.x), to_byte(d.y), to_byte(d.z), 255]);
    }
    rgba
}

/// Tonemaps the map to an 8-bit PNG in the file convention, for LDR skyboxes.
pub fn encode_png(env: &EnvMap, exposure_ev: f32) -> Result<Vec<u8>, String> {
    check_map(env, "PNG")?;
    let file = roll_quarter(env, true);
    let rgba = tonemap_rgba(file.data.iter().map(|t| [t[0], t[1], t[2]]), exposure_ev);
    Cx::encode_rgba_as_png(file.width as u32, file.height as u32, &rgba)
}

/// Tonemaps one cube face to an 8-bit PNG, `size` × `size`.
pub fn encode_face_png(face: &FaceImage, exposure_ev: f32) -> Result<Vec<u8>, String> {
    check_face(face, "PNG")?;
    let rgba = tonemap_rgba(face.rgb.chunks_exact(3).map(|p| [p[0], p[1], p[2]]), exposure_ev);
    Cx::encode_rgba_as_png(face.size as u32, face.size as u32, &rgba)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hdri::image::read_exr;
    use makepad_draw::makepad_platform::resource_resolver::DecodeBudget;

    /// A deterministic test map. The left half is a flat stretch, so the .hdr writer emits runs,
    /// including runs longer than 127 on wide rows. The right half varies, and the rows span
    /// several powers of ten. Any height is allowed: the writers never assume 2:1.
    fn test_map(width: usize, height: usize) -> EnvMap {
        let mut data = Vec::with_capacity(width * height);
        for y in 0..height {
            for x in 0..width {
                let c = if x < width / 2 {
                    [0.25, 0.5, 1.0]
                } else {
                    let t = ((x * 31 + y * 17) % 23) as f32;
                    [t * 0.37 + 0.01, (t * 1.7).sin().abs() * 3.0, 1.0e-3 * (x + 1) as f32]
                };
                let s = 1.0 + y as f32 * 100.0;
                data.push([c[0] * s, c[1] * s, c[2] * s, 1.0]);
            }
        }
        EnvMap { width, height, data }
    }

    fn texel(env: &EnvMap, x: usize, y: usize) -> [f32; 4] {
        env.data[y * env.width + x]
    }

    fn load_hdr(bytes: &[u8]) -> EnvMap {
        ibl::load_hdr(bytes, &mut DecodeBudget::default()).unwrap()
    }

    #[test]
    fn hdr_encode_then_load_hdr_round_trips_within_one_percent() {
        // 16 wide takes the RLE path, 4 wide the flat one, and 300 wide needs both width bytes
        // plus runs split at 127. All are divisible by 4, so the roll there and back is exact.
        for (width, height) in [(16, 5), (4, 3), (300, 2)] {
            let map = test_map(width, height);
            let back = roll_quarter(&load_hdr(&encode_hdr(&map)), false);
            assert_eq!((back.width, back.height), (width, height));
            for y in 0..height {
                for x in 0..width {
                    let (want, got) = (texel(&map, x, y), texel(&back, x, y));
                    // RGBE shares one exponent, so the error is measured against the brightest channel.
                    let max = want[0].max(want[1]).max(want[2]);
                    for k in 0..3 {
                        assert!((got[k] - want[k]).abs() <= 0.01 * max + 1.0e-6, "({x},{y}) of {width}x{height}: {got:?} vs {want:?}");
                    }
                    assert_eq!(got[3], 1.0);
                }
            }
        }
    }

    #[test]
    fn hdr_rle_compresses_flat_rows() {
        let env = EnvMap::constant(64, [0.3, 0.6, 1.2]);
        let bytes = encode_hdr(&env);
        // Each row is 4 header bytes plus one (count, value) run per channel: 12 bytes, not 256.
        let header = b"#?RADIANCE\n# Made with Makepad HDRI\nFORMAT=32-bit_rle_rgbe\n\n-Y 32 +X 64\n".len();
        assert_eq!(bytes.len(), header + 32 * 12);
        let want = ibl::rgbe_to_float(ibl::float_to_rgbe([0.3, 0.6, 1.2]));
        let t = load_hdr(&bytes).data[31 * 64 + 63];
        assert_eq!([t[0], t[1], t[2]], want);
        // NaN, negatives and infinities cannot be stored: they write as black or saturate.
        let mut odd = EnvMap::constant(8, [1.0, 1.0, 1.0]);
        odd.data[0] = [f32::NAN, -1.0, 0.0, 1.0];
        odd.data[1] = [f32::INFINITY, 0.0, 0.0, 1.0];
        let back = load_hdr(&encode_hdr(&odd));
        // The roll moves engine columns 0 and 1 to file columns 6 and 7.
        assert_eq!(back.data[6], [0.0, 0.0, 0.0, 1.0]);
        assert!(back.data[7][0] > 1.0e37 && back.data[7][0].is_finite());
    }

    #[test]
    fn writers_put_plus_x_at_the_file_centre_column() {
        let (width, height) = (16, 8);
        let mut env = EnvMap::constant(width, [0.1, 0.1, 0.1]);
        assert_eq!((env.width, env.height), (width, height));
        // +X is the engine's u = 0.75: column 12 of 16.
        let x_engine = (ibl::dir_to_equirect_uv([1.0, 0.0, 0.0])[0] * width as f32).floor() as usize;
        assert_eq!(x_engine, 12);
        env.data[3 * width + x_engine] = [50.0, 40.0, 30.0, 1.0];
        let bright = |m: &EnvMap, x: usize| m.data[3 * m.width + x][0] > 10.0;
        // Radiance: the file (not rolled back) holds it at the centre column, 8.
        let hdr = load_hdr(&encode_hdr(&env));
        assert!(bright(&hdr, 8) && !bright(&hdr, 12));
        // EXR the same, and the roll back restores the engine map exactly.
        let (bytes, _) = encode_exr(&env, ExrPrecision::Float).unwrap();
        let exr = read_exr(&bytes).unwrap();
        assert!(bright(&exr, 8) && !bright(&exr, 12));
        assert_eq!(roll_quarter(&exr, false), env);
        // PNG too: the decoded image is brightest at column 8 of row 3.
        let png = decode_image_from_data(&encode_png(&env, 0.0).unwrap()).unwrap();
        let red = |x: usize| (png.data[3 * 16 + x] >> 16) & 0xff;
        assert!(red(8) > red(12) + 50, "{} vs {}", red(8), red(12));
    }

    #[test]
    fn exr_float_round_trip_is_exact() {
        let mut map = test_map(12, 6);
        // A sun far past half's range survives in float.
        map.data[0] = [1.0e6, 3.0e5, 70000.0, 1.0];
        let (bytes, report) = encode_exr(&map, ExrPrecision::Float).unwrap();
        assert_eq!(report.clipped, 0);
        assert_eq!(roll_quarter(&read_exr(&bytes).unwrap(), false), map);
        assert!(encode_exr(&EnvMap { width: 0, height: 0, data: Vec::new() }, ExrPrecision::Float).is_err());
    }

    #[test]
    fn exr_half_round_trips_and_counts_clipped_pixels() {
        let mut map = test_map(8, 4);
        map.data[1 * 8 + 3] = [70000.0, 1.0, 0.5, 1.0];
        map.data[2 * 8 + 5] = [1.0e6, 2.0e6, 65504.0, 1.0];
        let (bytes, report) = encode_exr(&map, ExrPrecision::Half).unwrap();
        assert_eq!(report.clipped, 2);
        let back = roll_quarter(&read_exr(&bytes).unwrap(), false);
        assert_eq!((back.width, back.height), (8, 4));
        assert_eq!(texel(&back, 3, 1), [65504.0, 1.0, 0.5, 1.0]);
        assert_eq!(texel(&back, 5, 2), [65504.0, 65504.0, 65504.0, 1.0]);
        for y in 0..4 {
            for x in 0..8 {
                if (x, y) == (3, 1) || (x, y) == (5, 2) {
                    continue;
                }
                let (a, b) = (texel(&map, x, y), texel(&back, x, y));
                for k in 0..3 {
                    // Half keeps 11 significant bits: a relative error of at most 2^-11.
                    assert!((a[k] - b[k]).abs() <= a[k] * 4.9e-4 + 6.0e-8, "({x},{y}): {a:?} vs {b:?}");
                }
            }
        }
    }

    #[test]
    fn png_encodes_and_decodes_to_the_expected_size() {
        let map = test_map(16, 8);
        let png = encode_png(&map, 1.0).unwrap();
        assert!(png.starts_with(&[0x89, b'P', b'N', b'G']));
        let decoded = decode_image_from_data(&png).unwrap();
        assert_eq!((decoded.width, decoded.height), (16, 8));
        // The PNG holds the file convention: compare against the rolled map.
        let file = roll_quarter(&map, true);
        let byte = |v: f32| (v.clamp(0.0, 1.0) * 255.0 + 0.5) as u32;
        for (x, y) in [(0, 0), (5, 3), (15, 7)] {
            // EV 1 is an exposure of 2, through the same display transform the preview uses.
            let want = crate::sky::display_rgb(vec([texel(&file, x, y)[0], texel(&file, x, y)[1], texel(&file, x, y)[2]]), 2.0);
            let packed = decoded.data[y * 16 + x];
            assert_eq!((packed >> 16) & 0xff, byte(want.x), "({x},{y})");
            assert_eq!((packed >> 8) & 0xff, byte(want.y), "({x},{y})");
            assert_eq!(packed & 0xff, byte(want.z), "({x},{y})");
            assert_eq!(packed >> 24, 255);
        }
        assert!(encode_png(&EnvMap { width: 0, height: 0, data: Vec::new() }, 0.0).is_err());
        // A face writes as a square PNG the same way.
        let face = FaceImage { size: 2, rgb: vec![0.5; 12] };
        let decoded = decode_image_from_data(&encode_face_png(&face, 0.0).unwrap()).unwrap();
        assert_eq!((decoded.width, decoded.height), (2, 2));
        assert_eq!((decoded.data[3] >> 16) & 0xff, byte(crate::sky::display_rgb(vec([0.5; 3]), 1.0).x));
    }
}
