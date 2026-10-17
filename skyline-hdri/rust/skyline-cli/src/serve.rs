//! Tiny static file server so the browser app can run without Node or Python:
//!   skyline --serve 8080 [--root <project folder>] [--overpass <mirror url> ...]
//! Serves the folder over HTTP/1.1 (one thread per connection), maps `/` to index.html, refuses
//! paths outside the root, sends the content types the browser needs for ES modules and workers,
//! and proxies `POST /overpass` to the Overpass mirrors (server side, via curl) so the page is not
//! at the mercy of a mirror's CORS headers.

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

fn content_type(path: &Path) -> &'static str {
    match path.extension().and_then(|e| e.to_str()).unwrap_or("").to_ascii_lowercase().as_str() {
        "html" => "text/html; charset=utf-8",
        "js" | "mjs" => "text/javascript; charset=utf-8",
        "css" => "text/css; charset=utf-8",
        "json" => "application/json",
        "png" => "image/png",
        "svg" => "image/svg+xml",
        "exr" => "image/x-exr",
        "wasm" => "application/wasm",
        "md" | "txt" => "text/plain; charset=utf-8",
        _ => "application/octet-stream",
    }
}

fn respond(stream: &mut TcpStream, status: &str, ctype: &str, body: &[u8]) {
    let head = format!("HTTP/1.1 {}\r\nContent-Type: {}\r\nContent-Length: {}\r\nCache-Control: no-cache\r\nAccess-Control-Allow-Origin: *\r\nAccess-Control-Allow-Methods: GET, POST, OPTIONS\r\nAccess-Control-Allow-Headers: Content-Type\r\nConnection: close\r\n\r\n", status, ctype, body.len());
    let _ = stream.write_all(head.as_bytes());
    let _ = stream.write_all(body);
    let _ = stream.flush();
}

/// Read a full HTTP/1.1 request (headers + Content-Length body).
fn read_request(stream: &mut TcpStream) -> Option<(String, Vec<u8>)> {
    let mut raw = Vec::with_capacity(8192);
    let mut buf = [0u8; 8192];
    let header_end = loop {
        let n = match stream.read(&mut buf) { Ok(0) => return None, Ok(n) => n, Err(_) => return None };
        raw.extend_from_slice(&buf[..n]);
        if let Some(p) = raw.windows(4).position(|w| w == b"\r\n\r\n") { break p + 4; }
        if raw.len() > 1 << 20 { return None; }
    };
    let head = String::from_utf8_lossy(&raw[..header_end]).to_string();
    let len: usize = head.lines().find_map(|l| { let (k, v) = l.split_once(':')?; if k.trim().eq_ignore_ascii_case("content-length") { v.trim().parse().ok() } else { None } }).unwrap_or(0);
    let mut body = raw[header_end..].to_vec();
    while body.len() < len {
        let n = match stream.read(&mut buf) { Ok(0) => break, Ok(n) => n, Err(_) => break };
        body.extend_from_slice(&buf[..n]);
    }
    body.truncate(len.max(body.len().min(len)));
    Some((head, body))
}

fn proxy_overpass(stream: &mut TcpStream, body: &[u8], mirrors: &[String]) {
    let timeout = Duration::from_secs(240);
    let mut errors = Vec::new();
    for url in mirrors {
        match skyline_core::http::post(url, body, "application/x-www-form-urlencoded", timeout) {
            Ok(r) if r.status == 200 => { eprintln!("overpass proxy: {} ok ({} bytes)", url, r.body.len()); respond(stream, "200 OK", "application/json", &r.body); return; }
            Ok(r) => { eprintln!("overpass proxy: {} HTTP {}", url, r.status); errors.push(format!("{}: HTTP {}", url, r.status)); }
            Err(e) => { eprintln!("overpass proxy: {} {}", url, e); errors.push(format!("{}: {}", url, e)); }
        }
    }
    respond(stream, "502 Bad Gateway", "text/plain", format!("no Overpass mirror answered:\n{}", errors.join("\n")).as_bytes());
}

fn handle(mut stream: TcpStream, root: &Path, mirrors: &[String]) {
    let (head, body) = match read_request(&mut stream) { Some(r) => r, None => return };
    let mut parts = head.lines().next().unwrap_or("").split_whitespace();
    let method = parts.next().unwrap_or("");
    let target = parts.next().unwrap_or("/");
    let path = target.split('?').next().unwrap_or("/");
    if path == "/overpass" {
        if method == "OPTIONS" { respond(&mut stream, "204 No Content", "text/plain", b""); return; }
        if method != "POST" { respond(&mut stream, "405 Method Not Allowed", "text/plain", b"POST the query as data=..."); return; }
        proxy_overpass(&mut stream, &body, mirrors);
        return;
    }
    if method != "GET" && method != "HEAD" { respond(&mut stream, "405 Method Not Allowed", "text/plain", b"GET only"); return; }
    let decoded = percent_decode(path);
    let rel = decoded.trim_start_matches('/');
    let rel = if rel.is_empty() { "index.html" } else { rel };
    // refuse traversal: every component must be a plain name
    if rel.split(['/', '\\']).any(|c| c == ".." || c.is_empty() && rel.contains("//")) {
        respond(&mut stream, "403 Forbidden", "text/plain", b"forbidden"); return;
    }
    let mut file = root.to_path_buf();
    for c in rel.split('/') { file.push(c); }
    if file.is_dir() { file.push("index.html"); }
    match std::fs::read(&file) {
        Ok(data) => { let ct = content_type(&file); if method == "HEAD" { respond(&mut stream, "200 OK", ct, b""); } else { respond(&mut stream, "200 OK", ct, &data); } }
        Err(_) => respond(&mut stream, "404 Not Found", "text/plain", format!("not found: {}", rel).as_bytes()),
    }
}

fn percent_decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' && i + 2 < b.len() {
            if let Ok(v) = u8::from_str_radix(&s[i + 1..i + 3], 16) { out.push(v); i += 3; continue; }
        }
        out.push(b[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).to_string()
}

/// Serve `root` on 127.0.0.1:port until killed; `mirrors` are the Overpass upstreams for /overpass.
pub fn serve(root: PathBuf, port: u16, mirrors: Vec<String>) -> ! {
    let root = root.canonicalize().unwrap_or(root);
    if !root.join("index.html").exists() {
        eprintln!("warning: no index.html in {} — pass --root <folder with index.html>", root.display());
    }
    let listener = match TcpListener::bind(("127.0.0.1", port)) {
        Ok(l) => l,
        Err(e) => { eprintln!("cannot listen on port {}: {}", port, e); std::process::exit(1); }
    };
    eprintln!("serving {}\n  open http://localhost:{}/  (Ctrl+C to stop)\n  /overpass proxies to: {}", root.display(), port, mirrors.join(", "));
    let mirrors = Arc::new(mirrors);
    for conn in listener.incoming() {
        if let Ok(stream) = conn {
            let root = root.clone();
            let mirrors = Arc::clone(&mirrors);
            std::thread::spawn(move || handle(stream, &root, &mirrors));
        }
    }
    std::process::exit(0)
}
