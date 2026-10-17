//! Minimal OpenEXR writer (scanline, NO_COMPRESSION, FLOAT/HALF). Twin of `src/export/exr.js`.

use crate::sweep::Plate;

pub fn float_to_half(f: f32) -> u16 {
    let x = f.to_bits();
    let sign = ((x >> 16) & 0x8000) as u16;
    let exp = ((x >> 23) & 0xff) as i32;
    let mut mant = x & 0x7fffff;
    if exp == 0xff { return sign | 0x7c00 | if mant != 0 { 0x200 } else { 0 }; }
    let e = exp - 127 + 15;
    if e >= 0x1f { return sign | 0x7c00; }
    if e <= 0 {
        if e < -10 { return sign; }
        mant |= 0x800000;
        let shift = (14 - e) as u32;
        let mut h = (mant >> shift) as u16;
        let rem = mant & ((1 << shift) - 1);
        let halfway = 1 << (shift - 1);
        if rem > halfway || (rem == halfway && (h & 1) == 1) { h += 1; }
        return sign | h;
    }
    let mut h = sign | ((e as u16) << 10) | (mant >> 13) as u16;
    let rem = mant & 0x1fff;
    if rem > 0x1000 || (rem == 0x1000 && (h & 1) == 1) { h += 1; }
    h
}

pub struct Channel<'a> { pub name: &'a str, pub data: &'a [f32], pub half: bool }

fn cstr(out: &mut Vec<u8>, s: &str) { out.extend_from_slice(s.as_bytes()); out.push(0); }
fn attr(out: &mut Vec<u8>, name: &str, typ: &str, body: &[u8]) { cstr(out, name); cstr(out, typ); out.extend_from_slice(&(body.len() as i32).to_le_bytes()); out.extend_from_slice(body); }

/// Encode channels (any order; sorted by name as the format requires) into an EXR file.
pub fn encode_exr(width: usize, height: usize, channels: &[Channel]) -> Vec<u8> {
    let mut chans: Vec<&Channel> = channels.iter().collect();
    chans.sort_by(|a, b| a.name.as_bytes().cmp(b.name.as_bytes()));
    let mut out = Vec::new();
    out.extend_from_slice(&[0x76, 0x2f, 0x31, 0x01]);
    out.extend_from_slice(&2i32.to_le_bytes());
    let mut chl = Vec::new();
    for c in &chans {
        cstr(&mut chl, c.name);
        chl.extend_from_slice(&(if c.half { 1i32 } else { 2i32 }).to_le_bytes());
        chl.extend_from_slice(&[0, 0, 0, 0]);
        chl.extend_from_slice(&1i32.to_le_bytes());
        chl.extend_from_slice(&1i32.to_le_bytes());
    }
    chl.push(0);
    attr(&mut out, "channels", "chlist", &chl);
    attr(&mut out, "compression", "compression", &[0]);
    let mut bx = Vec::new();
    for v in [0i32, 0, width as i32 - 1, height as i32 - 1] { bx.extend_from_slice(&v.to_le_bytes()); }
    attr(&mut out, "dataWindow", "box2i", &bx);
    attr(&mut out, "displayWindow", "box2i", &bx);
    attr(&mut out, "lineOrder", "lineOrder", &[0]);
    attr(&mut out, "pixelAspectRatio", "float", &1f32.to_le_bytes());
    attr(&mut out, "screenWindowCenter", "v2f", &[0u8; 8]);
    attr(&mut out, "screenWindowWidth", "float", &1f32.to_le_bytes());
    out.push(0);
    let bytes_per_pixel: usize = chans.iter().map(|c| if c.half { 2 } else { 4 }).sum();
    let line_bytes = width * bytes_per_pixel;
    let first_line = out.len() + height * 8;
    for y in 0..height { out.extend_from_slice(&((first_line + y * (8 + line_bytes)) as u64).to_le_bytes()); }
    for y in 0..height {
        out.extend_from_slice(&(y as i32).to_le_bytes());
        out.extend_from_slice(&(line_bytes as i32).to_le_bytes());
        for c in &chans {
            let row = &c.data[y * width..(y + 1) * width];
            if c.half { for &v in row { out.extend_from_slice(&float_to_half(v).to_le_bytes()); } }
            else { for &v in row { out.extend_from_slice(&v.to_le_bytes()); } }
        }
    }
    out
}

/// Pack a plate into EXR channels (full: A Z N.x N.y N.z height id; compact: A Z).
pub fn plate_to_exr(plate: &Plate, full: bool, sky_depth: f32) -> Vec<u8> {
    let n = plate.width * plate.height;
    let a: Vec<f32> = plate.coverage.iter().map(|&c| if c != 0 { 1.0 } else { 0.0 }).collect();
    let z: Vec<f32> = (0..n).map(|i| if plate.coverage[i] != 0 { plate.depth[i] } else { sky_depth }).collect();
    let mut channels = vec![Channel { name: "A", data: &a, half: true }, Channel { name: "Z", data: &z, half: false }];
    let (nx, ny, nz, id, h);
    if full {
        nx = (0..n).map(|i| plate.normal[i * 3]).collect::<Vec<f32>>();
        ny = (0..n).map(|i| plate.normal[i * 3 + 1]).collect::<Vec<f32>>();
        nz = (0..n).map(|i| plate.normal[i * 3 + 2]).collect::<Vec<f32>>();
        id = plate.class_id.iter().map(|&c| c as f32).collect::<Vec<f32>>();
        h = (0..n).map(|i| if plate.coverage[i] != 0 { plate.altitude[i] } else { 0.0 }).collect::<Vec<f32>>();
        channels.push(Channel { name: "N.x", data: &nx, half: true });
        channels.push(Channel { name: "N.y", data: &ny, half: true });
        channels.push(Channel { name: "N.z", data: &nz, half: true });
        channels.push(Channel { name: "height", data: &h, half: true });
        channels.push(Channel { name: "id", data: &id, half: true });
        return encode_exr(plate.width, plate.height, &channels);
    }
    encode_exr(plate.width, plate.height, &channels)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn half_conversion() {
        assert_eq!(float_to_half(1.0), 0x3c00);
        assert_eq!(float_to_half(0.0), 0);
        assert_eq!(float_to_half(-2.0), 0xc000);
        assert_eq!(float_to_half(65504.0), 0x7bff);
    }
    #[test]
    fn header_layout() {
        let z = [1f32, 2.0, 3.0, 4.0, 5.0, 6.0];
        let a = [1f32, 0.0, 1.0, 0.0, 1.0, 0.0];
        let exr = encode_exr(3, 2, &[Channel { name: "Z", data: &z, half: false }, Channel { name: "A", data: &a, half: true }]);
        assert_eq!(&exr[..4], &[0x76, 0x2f, 0x31, 0x01]);
        let line_bytes = 3 * 6;
        let last = exr.len() - (8 + line_bytes);
        assert_eq!(i32::from_le_bytes(exr[last..last + 4].try_into().unwrap()), 1);
        assert_eq!(u16::from_le_bytes(exr[last + 10..last + 12].try_into().unwrap()), 0x3c00);
        assert_eq!(f32::from_le_bytes(exr[last + 14..last + 18].try_into().unwrap()), 4.0);
    }
}
