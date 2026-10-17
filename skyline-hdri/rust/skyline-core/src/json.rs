//! Tiny JSON parser (enough for Overpass responses), no dependencies.

#[derive(Debug, Clone, PartialEq)]
pub enum Json {
    Null,
    Bool(bool),
    Number(f64),
    Str(String),
    Array(Vec<Json>),
    Object(Vec<(String, Json)>),
}

impl Json {
    pub fn get(&self, key: &str) -> Option<&Json> {
        match self { Json::Object(kv) => kv.iter().find(|(k, _)| k == key).map(|(_, v)| v), _ => None }
    }
    pub fn as_str(&self) -> Option<&str> { if let Json::Str(s) = self { Some(s) } else { None } }
    pub fn as_f64(&self) -> Option<f64> { if let Json::Number(n) = self { Some(*n) } else { None } }
    pub fn as_array(&self) -> Option<&Vec<Json>> { if let Json::Array(a) = self { Some(a) } else { None } }
    /// String value of a key, or a number rendered as a string (OSM tags are always strings anyway).
    pub fn str_of(&self, key: &str) -> Option<String> {
        match self.get(key) { Some(Json::Str(s)) => Some(s.clone()), Some(Json::Number(n)) => Some(n.to_string()), _ => None }
    }
}

struct Parser<'a> { s: &'a [u8], i: usize }

impl<'a> Parser<'a> {
    fn ws(&mut self) { while self.i < self.s.len() && matches!(self.s[self.i], b' ' | b'\n' | b'\r' | b'\t') { self.i += 1; } }
    fn err<T>(&self, msg: &str) -> Result<T, String> { Err(format!("json: {} at byte {}", msg, self.i)) }
    fn value(&mut self) -> Result<Json, String> {
        self.ws();
        if self.i >= self.s.len() { return self.err("unexpected end"); }
        match self.s[self.i] {
            b'{' => {
                self.i += 1;
                let mut kv = Vec::new();
                self.ws();
                if self.i < self.s.len() && self.s[self.i] == b'}' { self.i += 1; return Ok(Json::Object(kv)); }
                loop {
                    self.ws();
                    let k = self.string()?;
                    self.ws();
                    if self.i >= self.s.len() || self.s[self.i] != b':' { return self.err("expected ':'"); }
                    self.i += 1;
                    let v = self.value()?;
                    kv.push((k, v));
                    self.ws();
                    match self.s.get(self.i) { Some(b',') => { self.i += 1; } Some(b'}') => { self.i += 1; return Ok(Json::Object(kv)); } _ => return self.err("expected ',' or '}'") }
                }
            }
            b'[' => {
                self.i += 1;
                let mut a = Vec::new();
                self.ws();
                if self.i < self.s.len() && self.s[self.i] == b']' { self.i += 1; return Ok(Json::Array(a)); }
                loop {
                    a.push(self.value()?);
                    self.ws();
                    match self.s.get(self.i) { Some(b',') => { self.i += 1; } Some(b']') => { self.i += 1; return Ok(Json::Array(a)); } _ => return self.err("expected ',' or ']'") }
                }
            }
            b'"' => Ok(Json::Str(self.string()?)),
            b't' => { self.lit(b"true")?; Ok(Json::Bool(true)) }
            b'f' => { self.lit(b"false")?; Ok(Json::Bool(false)) }
            b'n' => { self.lit(b"null")?; Ok(Json::Null) }
            _ => self.number(),
        }
    }
    fn lit(&mut self, w: &[u8]) -> Result<(), String> {
        if self.s[self.i..].starts_with(w) { self.i += w.len(); Ok(()) } else { self.err("bad literal") }
    }
    fn number(&mut self) -> Result<Json, String> {
        let start = self.i;
        while self.i < self.s.len() && matches!(self.s[self.i], b'-' | b'+' | b'.' | b'e' | b'E' | b'0'..=b'9') { self.i += 1; }
        let t = std::str::from_utf8(&self.s[start..self.i]).map_err(|_| "json: utf8".to_string())?;
        t.parse::<f64>().map(Json::Number).map_err(|_| format!("json: bad number '{}'", t))
    }
    fn string(&mut self) -> Result<String, String> {
        if self.s.get(self.i) != Some(&b'"') { return self.err("expected string"); }
        self.i += 1;
        let mut out: Vec<u8> = Vec::new();
        loop {
            let c = *self.s.get(self.i).ok_or("json: unterminated string")?;
            self.i += 1;
            match c {
                b'"' => break,
                b'\\' => {
                    let e = *self.s.get(self.i).ok_or("json: bad escape")?;
                    self.i += 1;
                    match e {
                        b'"' => out.push(b'"'), b'\\' => out.push(b'\\'), b'/' => out.push(b'/'),
                        b'b' => out.push(8), b'f' => out.push(12), b'n' => out.push(b'\n'), b'r' => out.push(b'\r'), b't' => out.push(b'\t'),
                        b'u' => {
                            let mut cp = self.hex4()?;
                            if (0xD800..0xDC00).contains(&cp) && self.s[self.i..].starts_with(b"\\u") {
                                self.i += 2;
                                let lo = self.hex4()?;
                                cp = 0x10000 + ((cp - 0xD800) << 10) + (lo.wrapping_sub(0xDC00));
                            }
                            let ch = char::from_u32(cp).unwrap_or('\u{FFFD}');
                            let mut buf = [0u8; 4];
                            out.extend_from_slice(ch.encode_utf8(&mut buf).as_bytes());
                        }
                        _ => return self.err("bad escape"),
                    }
                }
                _ => out.push(c),
            }
        }
        String::from_utf8(out).map_err(|_| "json: utf8 in string".to_string())
    }
    fn hex4(&mut self) -> Result<u32, String> {
        if self.i + 4 > self.s.len() { return self.err("short \\u"); }
        let t = std::str::from_utf8(&self.s[self.i..self.i + 4]).map_err(|_| "json: utf8".to_string())?;
        self.i += 4;
        u32::from_str_radix(t, 16).map_err(|_| "json: bad \\u".to_string())
    }
}

pub fn parse(text: &str) -> Result<Json, String> {
    let mut p = Parser { s: text.as_bytes(), i: 0 };
    let v = p.value()?;
    p.ws();
    if p.i != p.s.len() { return p.err("trailing data"); }
    Ok(v)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn parses_overpass_like_json() {
        let j = parse(r#"{"version":0.6,"elements":[{"type":"way","id":1,"tags":{"height":"25 m","name":"Café \"x\""},"geometry":[{"lat":52.1,"lon":5.2}]}]}"#).unwrap();
        let els = j.get("elements").unwrap().as_array().unwrap();
        assert_eq!(els[0].get("tags").unwrap().str_of("height").unwrap(), "25 m");
        assert_eq!(els[0].get("tags").unwrap().str_of("name").unwrap(), "Café \"x\"");
        assert_eq!(els[0].get("geometry").unwrap().as_array().unwrap()[0].get("lon").unwrap().as_f64(), Some(5.2));
    }
}
