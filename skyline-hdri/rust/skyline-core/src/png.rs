//! Minimal PNG support with no dependencies: a DEFLATE decoder (RFC 1951, after Mark Adler's puff),
//! a PNG decoder for 8-bit grey/RGB/RGBA (filters 0-4), and a PNG encoder that writes stored
//! (uncompressed) deflate blocks — valid everywhere, just not small.

// ------------------------------------------------------------------------------------------
// inflate

struct BitReader<'a> {
    data: &'a [u8],
    pos: usize,
    bitbuf: u32,
    bitcnt: u32,
}

impl<'a> BitReader<'a> {
    fn new(data: &'a [u8]) -> Self { Self { data, pos: 0, bitbuf: 0, bitcnt: 0 } }
    fn bits(&mut self, need: u32) -> Result<u32, String> {
        let mut val = self.bitbuf;
        while self.bitcnt < need {
            if self.pos >= self.data.len() { return Err("inflate: out of input".into()); }
            val |= (self.data[self.pos] as u32) << self.bitcnt;
            self.pos += 1;
            self.bitcnt += 8;
        }
        self.bitbuf = val >> need;
        self.bitcnt -= need;
        Ok(val & ((1u32 << need) - 1))
    }
}

struct Huffman { count: [u16; 16], symbol: Vec<u16> }

impl Huffman {
    fn new(lengths: &[u16]) -> Self {
        let mut count = [0u16; 16];
        for &l in lengths { count[l as usize] += 1; }
        count[0] = 0;
        let mut offs = [0u16; 16];
        for len in 1..16 { offs[len] = offs[len - 1] + count[len - 1]; }
        let mut symbol = vec![0u16; lengths.len()];
        for (sym, &l) in lengths.iter().enumerate() {
            if l != 0 { symbol[offs[l as usize] as usize] = sym as u16; offs[l as usize] += 1; }
        }
        Self { count, symbol }
    }
    fn decode(&self, br: &mut BitReader) -> Result<u16, String> {
        let (mut code, mut first, mut index) = (0i32, 0i32, 0i32);
        for len in 1..16 {
            code |= br.bits(1)? as i32;
            let count = self.count[len] as i32;
            if code - count < first { return Ok(self.symbol[(index + (code - first)) as usize]); }
            index += count;
            first += count;
            first <<= 1;
            code <<= 1;
        }
        Err("inflate: bad code".into())
    }
}

const LBASE: [u16; 29] = [3, 4, 5, 6, 7, 8, 9, 10, 11, 13, 15, 17, 19, 23, 27, 31, 35, 43, 51, 59, 67, 83, 99, 115, 131, 163, 195, 227, 258];
const LEXT: [u16; 29] = [0, 0, 0, 0, 0, 0, 0, 0, 1, 1, 1, 1, 2, 2, 2, 2, 3, 3, 3, 3, 4, 4, 4, 4, 5, 5, 5, 5, 0];
const DBASE: [u16; 30] = [1, 2, 3, 4, 5, 7, 9, 13, 17, 25, 33, 49, 65, 97, 129, 193, 257, 385, 513, 769, 1025, 1537, 2049, 3073, 4097, 6145, 8193, 12289, 16385, 24577];
const DEXT: [u16; 30] = [0, 0, 0, 0, 1, 1, 2, 2, 3, 3, 4, 4, 5, 5, 6, 6, 7, 7, 8, 8, 9, 9, 10, 10, 11, 11, 12, 12, 13, 13];

fn codes(br: &mut BitReader, out: &mut Vec<u8>, lencode: &Huffman, distcode: &Huffman) -> Result<(), String> {
    loop {
        let sym = lencode.decode(br)? as usize;
        if sym < 256 { out.push(sym as u8); }
        else if sym == 256 { return Ok(()); }
        else {
            let i = sym - 257;
            if i >= 29 { return Err("inflate: bad length symbol".into()); }
            let len = LBASE[i] as usize + br.bits(LEXT[i] as u32)? as usize;
            let ds = distcode.decode(br)? as usize;
            if ds >= 30 { return Err("inflate: bad distance symbol".into()); }
            let dist = DBASE[ds] as usize + br.bits(DEXT[ds] as u32)? as usize;
            if dist > out.len() { return Err("inflate: distance too far".into()); }
            let start = out.len() - dist;
            for k in 0..len { let b = out[start + k]; out.push(b); }
        }
    }
}

/// Inflate a raw DEFLATE stream.
pub fn inflate(data: &[u8]) -> Result<Vec<u8>, String> {
    let mut br = BitReader::new(data);
    let mut out = Vec::with_capacity(data.len() * 4);
    loop {
        let last = br.bits(1)?;
        let btype = br.bits(2)?;
        match btype {
            0 => {
                br.bitbuf = 0; br.bitcnt = 0;
                if br.pos + 4 > data.len() { return Err("inflate: stored header".into()); }
                let len = data[br.pos] as usize | ((data[br.pos + 1] as usize) << 8);
                br.pos += 4;
                if br.pos + len > data.len() { return Err("inflate: stored data".into()); }
                out.extend_from_slice(&data[br.pos..br.pos + len]);
                br.pos += len;
            }
            1 => {
                let mut lengths = [0u16; 288];
                for (i, l) in lengths.iter_mut().enumerate() {
                    *l = if i < 144 { 8 } else if i < 256 { 9 } else if i < 280 { 7 } else { 8 };
                }
                let lencode = Huffman::new(&lengths);
                let distcode = Huffman::new(&[5u16; 30]);
                codes(&mut br, &mut out, &lencode, &distcode)?;
            }
            2 => {
                let nlen = br.bits(5)? as usize + 257;
                let ndist = br.bits(5)? as usize + 1;
                let ncode = br.bits(4)? as usize + 4;
                const ORDER: [usize; 19] = [16, 17, 18, 0, 8, 7, 9, 6, 10, 5, 11, 4, 12, 3, 13, 2, 14, 1, 15];
                let mut lengths = [0u16; 19];
                for &o in ORDER.iter().take(ncode) { lengths[o] = br.bits(3)? as u16; }
                let codecode = Huffman::new(&lengths);
                let mut lens = vec![0u16; nlen + ndist];
                let mut idx = 0;
                while idx < nlen + ndist {
                    let sym = codecode.decode(&mut br)?;
                    if sym < 16 { lens[idx] = sym; idx += 1; }
                    else {
                        let (rep, val) = match sym {
                            16 => { if idx == 0 { return Err("inflate: repeat with no previous".into()); } (3 + br.bits(2)? as usize, lens[idx - 1]) }
                            17 => (3 + br.bits(3)? as usize, 0),
                            _ => (11 + br.bits(7)? as usize, 0),
                        };
                        if idx + rep > nlen + ndist { return Err("inflate: too many lengths".into()); }
                        for _ in 0..rep { lens[idx] = val; idx += 1; }
                    }
                }
                let lencode = Huffman::new(&lens[..nlen]);
                let distcode = Huffman::new(&lens[nlen..]);
                codes(&mut br, &mut out, &lencode, &distcode)?;
            }
            _ => return Err("inflate: bad block type".into()),
        }
        if last == 1 { break; }
    }
    Ok(out)
}

/// Inflate a zlib-wrapped stream (2-byte header, adler32 trailer ignored).
pub fn zlib_decompress(data: &[u8]) -> Result<Vec<u8>, String> {
    if data.len() < 6 { return Err("zlib: too short".into()); }
    inflate(&data[2..data.len() - 4])
}

// ------------------------------------------------------------------------------------------
// PNG decode

pub struct Image { pub width: usize, pub height: usize, pub channels: usize, pub data: Vec<u8> }

fn be32(b: &[u8]) -> u32 { ((b[0] as u32) << 24) | ((b[1] as u32) << 16) | ((b[2] as u32) << 8) | b[3] as u32 }

/// Decode an 8-bit, non-interlaced PNG. Output channels: 1 (grey), 2 (grey+alpha), 3 (RGB) or 4 (RGBA).
pub fn decode_png(bytes: &[u8]) -> Result<Image, String> {
    if bytes.len() < 8 || bytes[..8] != [137, 80, 78, 71, 13, 10, 26, 10] { return Err("png: bad signature".into()); }
    let (mut off, mut width, mut height, mut color_type, mut bit_depth) = (8usize, 0usize, 0usize, 0u8, 0u8);
    let mut idat = Vec::new();
    while off + 8 <= bytes.len() {
        let len = be32(&bytes[off..]) as usize;
        let typ = &bytes[off + 4..off + 8];
        let data = &bytes[off + 8..(off + 8 + len).min(bytes.len())];
        match typ {
            b"IHDR" => {
                width = be32(data) as usize; height = be32(&data[4..]) as usize;
                bit_depth = data[8]; color_type = data[9];
                if data[12] != 0 { return Err("png: interlaced".into()); }
            }
            b"IDAT" => idat.extend_from_slice(data),
            b"IEND" => break,
            _ => {}
        }
        off += 12 + len;
    }
    if bit_depth != 8 { return Err(format!("png: bit depth {} not supported", bit_depth)); }
    let channels = match color_type { 0 => 1, 2 => 3, 4 => 2, 6 => 4, _ => return Err("png: colour type".into()) };
    let raw = zlib_decompress(&idat)?;
    let stride = width * channels;
    if raw.len() < (stride + 1) * height { return Err("png: truncated".into()); }
    let mut out = vec![0u8; stride * height];
    for y in 0..height {
        let f = raw[y * (stride + 1)];
        let row = &raw[y * (stride + 1) + 1..(y + 1) * (stride + 1)];
        for i in 0..stride {
            let a = if i >= channels { out[y * stride + i - channels] as i32 } else { 0 };
            let b = if y > 0 { out[(y - 1) * stride + i] as i32 } else { 0 };
            let c = if y > 0 && i >= channels { out[(y - 1) * stride + i - channels] as i32 } else { 0 };
            let x = row[i] as i32;
            let v = match f {
                0 => x,
                1 => x + a,
                2 => x + b,
                3 => x + ((a + b) >> 1),
                4 => {
                    let p = a + b - c;
                    let (pa, pb, pc) = ((p - a).abs(), (p - b).abs(), (p - c).abs());
                    x + if pa <= pb && pa <= pc { a } else if pb <= pc { b } else { c }
                }
                _ => return Err("png: bad filter".into()),
            };
            out[y * stride + i] = (v & 255) as u8;
        }
    }
    Ok(Image { width, height, channels, data: out })
}

// ------------------------------------------------------------------------------------------
// PNG encode (stored deflate blocks)

fn crc32(bytes: &[u8]) -> u32 {
    static mut TABLE: [u32; 256] = [0; 256];
    static INIT: std::sync::Once = std::sync::Once::new();
    INIT.call_once(|| unsafe {
        for n in 0..256u32 {
            let mut c = n;
            for _ in 0..8 { c = if c & 1 != 0 { 0xedb88320 ^ (c >> 1) } else { c >> 1 }; }
            TABLE[n as usize] = c;
        }
    });
    let table = unsafe { &*std::ptr::addr_of!(TABLE) };
    let mut c = 0xffffffffu32;
    for &b in bytes { c = table[((c ^ b as u32) & 0xff) as usize] ^ (c >> 8); }
    c ^ 0xffffffff
}

fn adler32(bytes: &[u8]) -> u32 {
    let (mut a, mut b) = (1u32, 0u32);
    for chunk in bytes.chunks(5552) {
        for &x in chunk { a += x as u32; b += a; }
        a %= 65521; b %= 65521;
    }
    (b << 16) | a
}

fn chunk(out: &mut Vec<u8>, typ: &[u8; 4], data: &[u8]) {
    out.extend_from_slice(&(data.len() as u32).to_be_bytes());
    let start = out.len();
    out.extend_from_slice(typ);
    out.extend_from_slice(data);
    let crc = crc32(&out[start..]);
    out.extend_from_slice(&crc.to_be_bytes());
}

/// zlib stream made of stored blocks (no compression).
pub fn zlib_stored(raw: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(raw.len() + raw.len() / 65535 * 5 + 16);
    out.extend_from_slice(&[0x78, 0x01]);
    let mut chunks = raw.chunks(65535).peekable();
    if raw.is_empty() { out.extend_from_slice(&[1, 0, 0, 0xff, 0xff]); }
    while let Some(c) = chunks.next() {
        let last = chunks.peek().is_none();
        out.push(if last { 1 } else { 0 });
        let len = c.len() as u16;
        out.extend_from_slice(&len.to_le_bytes());
        out.extend_from_slice(&(!len).to_le_bytes());
        out.extend_from_slice(c);
    }
    out.extend_from_slice(&adler32(raw).to_be_bytes());
    out
}

/// Encode raw scanlines (no filter bytes). color_type: 0 grey, 2 RGB, 6 RGBA; bit_depth 8 or 16 (big-endian).
pub fn encode_png(width: usize, height: usize, color_type: u8, bit_depth: u8, data: &[u8]) -> Result<Vec<u8>, String> {
    let channels = match color_type { 0 => 1, 2 => 3, 6 => 4, _ => return Err("png: colour type".into()) };
    let row_bytes = width * channels * (bit_depth as usize / 8);
    if data.len() != row_bytes * height { return Err(format!("png: data length {} != {}", data.len(), row_bytes * height)); }
    let mut filtered = Vec::with_capacity((row_bytes + 1) * height);
    for y in 0..height {
        filtered.push(0);
        filtered.extend_from_slice(&data[y * row_bytes..(y + 1) * row_bytes]);
    }
    let idat = zlib_stored(&filtered);
    let mut out = Vec::with_capacity(idat.len() + 64);
    out.extend_from_slice(&[137, 80, 78, 71, 13, 10, 26, 10]);
    let mut ihdr = Vec::with_capacity(13);
    ihdr.extend_from_slice(&(width as u32).to_be_bytes());
    ihdr.extend_from_slice(&(height as u32).to_be_bytes());
    ihdr.extend_from_slice(&[bit_depth, color_type, 0, 0, 0]);
    chunk(&mut out, b"IHDR", &ihdr);
    chunk(&mut out, b"IDAT", &idat);
    chunk(&mut out, b"IEND", &[]);
    Ok(out)
}

pub fn encode_gray16(width: usize, height: usize, gray: &[u16]) -> Result<Vec<u8>, String> {
    let mut data = Vec::with_capacity(gray.len() * 2);
    for &v in gray { data.extend_from_slice(&v.to_be_bytes()); }
    encode_png(width, height, 0, 16, &data)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stored_png_round_trips_through_our_decoder() {
        let (w, h) = (7usize, 3usize);
        let rgba: Vec<u8> = (0..w * h * 4).map(|i| (i * 37 & 255) as u8).collect();
        let png = encode_png(w, h, 6, 8, &rgba).unwrap();
        let img = decode_png(&png).unwrap();
        assert_eq!((img.width, img.height, img.channels), (w, h, 4));
        assert_eq!(img.data, rgba);
    }

    #[test]
    fn inflate_fixed_huffman_known_vector() {
        // "hello hello hello" compressed by zlib (fixed Huffman, with a back-reference)
        let z: [u8; 16] = [0x78, 0x9c, 0xcb, 0x48, 0xcd, 0xc9, 0xc9, 0x57, 0xc8, 0x40, 0x90, 0x00, 0x3a, 0x2e, 0x06, 0x7d];
        let out = zlib_decompress(&z).unwrap();
        assert_eq!(String::from_utf8(out).unwrap(), "hello hello hello");
    }
}
