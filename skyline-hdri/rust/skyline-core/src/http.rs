//! Dependency-free HTTP: plain `http://` through std TcpStream, `https://` through the system `curl`.
//! Enough for tiles and Overpass; swap in ureq/reqwest (or Makepad's HttpRequest) in a real app.

use std::io::{Read, Write};
use std::net::TcpStream;
use std::process::Command;
use std::time::Duration;

pub struct Response { pub status: u16, pub body: Vec<u8> }

fn parse_url(url: &str) -> Option<(bool, String, u16, String)> {
    let (tls, rest) = if let Some(r) = url.strip_prefix("https://") { (true, r) } else if let Some(r) = url.strip_prefix("http://") { (false, r) } else { return None };
    let (hostport, path) = match rest.find('/') { Some(i) => (&rest[..i], &rest[i..]), None => (rest, "/") };
    let (host, port) = match hostport.rfind(':') { Some(i) => (&hostport[..i], hostport[i + 1..].parse().ok()?), None => (hostport, if tls { 443 } else { 80 }) };
    Some((tls, host.to_string(), port, path.to_string()))
}

fn plain_request(method: &str, host: &str, port: u16, path: &str, body: Option<&[u8]>, content_type: Option<&str>, timeout: Duration) -> Result<Response, String> {
    let mut stream = TcpStream::connect((host, port)).map_err(|e| format!("connect {}:{}: {}", host, port, e))?;
    stream.set_read_timeout(Some(timeout)).ok();
    stream.set_write_timeout(Some(timeout)).ok();
    let mut req = format!("{} {} HTTP/1.1\r\nHost: {}\r\nUser-Agent: skyline-hdri/0.1\r\nConnection: close\r\nAccept: */*\r\n", method, path, host);
    if let Some(b) = body {
        req.push_str(&format!("Content-Length: {}\r\n", b.len()));
        if let Some(ct) = content_type { req.push_str(&format!("Content-Type: {}\r\n", ct)); }
    }
    req.push_str("\r\n");
    stream.write_all(req.as_bytes()).map_err(|e| e.to_string())?;
    if let Some(b) = body { stream.write_all(b).map_err(|e| e.to_string())?; }
    let mut raw = Vec::new();
    stream.read_to_end(&mut raw).map_err(|e| e.to_string())?;
    let sep = raw.windows(4).position(|w| w == b"\r\n\r\n").ok_or("http: no header end")?;
    let head = String::from_utf8_lossy(&raw[..sep]).to_string();
    let status: u16 = head.split_whitespace().nth(1).and_then(|s| s.parse().ok()).ok_or("http: bad status line")?;
    let mut body = raw[sep + 4..].to_vec();
    let chunked = head.lines().any(|l| l.to_ascii_lowercase().starts_with("transfer-encoding:") && l.to_ascii_lowercase().contains("chunked"));
    if chunked {
        let mut out = Vec::new();
        let mut i = 0;
        loop {
            let line_end = body[i..].windows(2).position(|w| w == b"\r\n").ok_or("http: bad chunk")? + i;
            let size = usize::from_str_radix(String::from_utf8_lossy(&body[i..line_end]).trim().split(';').next().unwrap_or("0"), 16).map_err(|_| "http: chunk size")?;
            i = line_end + 2;
            if size == 0 { break; }
            out.extend_from_slice(&body[i..i + size]);
            i += size + 2;
        }
        body = out;
    }
    Ok(Response { status, body })
}

fn curl_request(method: &str, url: &str, body: Option<&[u8]>, content_type: Option<&str>, timeout: Duration) -> Result<Response, String> {
    let mut cmd = Command::new("curl");
    cmd.arg("-sS").arg("-L").arg("--max-time").arg(timeout.as_secs().to_string()).arg("-w").arg("\n%{http_code}").arg("-X").arg(method).arg("-A").arg("skyline-hdri/0.1");
    if let Some(b) = body {
        cmd.arg("--data-binary").arg("@-");
        if let Some(ct) = content_type { cmd.arg("-H").arg(format!("Content-Type: {}", ct)); }
        cmd.stdin(std::process::Stdio::piped());
        let _ = b;
    }
    cmd.arg(url);
    cmd.stdout(std::process::Stdio::piped()).stderr(std::process::Stdio::piped());
    let mut child = cmd.spawn().map_err(|e| format!("curl not available: {}", e))?;
    if let Some(b) = body {
        let mut stdin = child.stdin.take().ok_or("curl stdin")?;
        stdin.write_all(b).map_err(|e| e.to_string())?;
    }
    let out = child.wait_with_output().map_err(|e| e.to_string())?;
    let mut data = out.stdout;
    let cut = data.iter().rposition(|&c| c == b'\n').ok_or_else(|| format!("curl failed: {}", String::from_utf8_lossy(&out.stderr)))?;
    let status: u16 = String::from_utf8_lossy(&data[cut + 1..]).trim().parse().map_err(|_| "curl: no status")?;
    data.truncate(cut);
    Ok(Response { status, body: data })
}

/// GET a URL (http via sockets, https via curl).
pub fn get(url: &str, timeout: Duration) -> Result<Response, String> {
    let (tls, host, port, path) = parse_url(url).ok_or("bad url")?;
    if tls { curl_request("GET", url, None, None, timeout) } else { plain_request("GET", &host, port, &path, None, None, timeout) }
}

/// POST a body to a URL.
pub fn post(url: &str, body: &[u8], content_type: &str, timeout: Duration) -> Result<Response, String> {
    let (tls, host, port, path) = parse_url(url).ok_or("bad url")?;
    if tls { curl_request("POST", url, Some(body), Some(content_type), timeout) } else { plain_request("POST", &host, port, &path, Some(body), Some(content_type), timeout) }
}

/// Tile source over HTTP with an optional on-disk cache directory.
pub struct HttpTileSource { pub url_template: String, pub cache_dir: Option<std::path::PathBuf>, pub timeout: Duration }

impl HttpTileSource {
    pub fn new(url_template: &str, cache_dir: Option<std::path::PathBuf>) -> Self {
        Self { url_template: url_template.to_string(), cache_dir, timeout: Duration::from_secs(60) }
    }
}

impl crate::tiles::TileSource for HttpTileSource {
    fn fetch(&self, z: u32, x: u32, y: u32) -> Option<Vec<u8>> {
        let path = self.cache_dir.as_ref().map(|d| d.join(format!("{}/{}/{}.png", z, x, y)));
        if let Some(p) = &path { if let Ok(b) = std::fs::read(p) { return Some(b); } }
        let url = self.url_template.replace("{z}", &z.to_string()).replace("{x}", &x.to_string()).replace("{y}", &y.to_string());
        let r = get(&url, self.timeout).ok()?;
        if r.status != 200 { return None; }
        if let Some(p) = &path { let _ = std::fs::create_dir_all(p.parent().unwrap()); let _ = std::fs::write(p, &r.body); }
        Some(r.body)
    }
}

/// Overpass source trying several mirrors in turn.
pub struct HttpBuildingSource { pub urls: Vec<String>, pub timeout: Duration }

impl crate::buildings::BuildingSource for HttpBuildingSource {
    fn fetch(&self, query: &str) -> Option<String> {
        let body = format!("data={}", urlencode(query));
        for url in &self.urls {
            match post(url, body.as_bytes(), "application/x-www-form-urlencoded", self.timeout) {
                Ok(r) if r.status == 200 => return String::from_utf8(r.body).ok(),
                Ok(r) => eprintln!("overpass {}: HTTP {}", url, r.status),
                Err(e) => eprintln!("overpass {}: {}", url, e),
            }
        }
        None
    }
}

pub fn urlencode(s: &str) -> String {
    let mut out = String::with_capacity(s.len() * 3);
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => out.push(b as char),
            b' ' => out.push('+'),
            _ => out.push_str(&format!("%{:02X}", b)),
        }
    }
    out
}
