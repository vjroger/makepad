//! Buildings from OpenStreetMap via Overpass. Twin of `src/engine/buildings.js`.

use crate::geo::to_local;
use crate::json::{parse, Json};
use crate::tiers::{tier_index_for, Tier};

pub const DEFAULT_OVERPASS_URLS: [&str; 5] = [
    "https://overpass-api.de/api/interpreter",
    "https://overpass.private.coffee/api/interpreter",
    "https://overpass.kumi.systems/api/interpreter",
    "https://lz4.overpass-api.de/api/interpreter",
    "https://maps.mail.ru/osm/tools/overpass/api/interpreter",
];

const MAN_MADE: &str = "^(tower|mast|chimney|communications_tower|lighthouse|water_tower|windmill)$";

/// Two-tier Overpass query (everything within all_radius, tall things within tall_radius).
pub fn build_query(lat: f64, lon: f64, all_radius: f64, tall_radius: f64) -> String {
    let a = format!("(around:{},{},{})", all_radius.round(), lat, lon);
    let t = format!("(around:{},{},{})", tall_radius.round(), lat, lon);
    let tall_h = r#"["height"~"^([3-9][0-9]|[1-9][0-9]{2,})"]"#;
    let tall_l = r#"["building:levels"~"^([1-9][0-9]+)$"]"#;
    format!(
        "[out:json][timeout:180];\n(\n  way[\"building\"]{a};\n  way[\"building:part\"]{a};\n  relation[\"building\"]{a};\n  way[\"building\"]{th}{t};\n  way[\"building:part\"]{th}{t};\n  relation[\"building\"]{th}{t};\n  way[\"building\"]{tl}{t};\n  way[\"building:part\"]{tl}{t};\n  relation[\"building\"]{tl}{t};\n  node[\"man_made\"~\"{mm}\"]{t};\n  way[\"man_made\"~\"{mm}\"]{t};\n);\nout geom qt;",
        a = a, t = t, th = tall_h, tl = tall_l, mm = MAN_MADE
    )
}

/// Where building JSON comes from (HTTP POST of the query, a cached file, a fixture...).
pub trait BuildingSource {
    /// Overpass JSON text for the query, or None.
    fn fetch(&self, query: &str) -> Option<String>;
}

#[derive(Debug, Clone)]
pub struct Building {
    pub polys: Vec<Vec<(f64, f64)>>,   // local metres (east, north), rings without the closing point
    pub height: f64,
    pub kind: &'static str,            // "building" | "part" | "tower"
    pub area: f64,
    pub centroid: (f64, f64),
}

/// Parse an OSM length ("25", "25 m", "82 ft", "82'", "25,5") into metres.
pub fn parse_length(v: &str) -> Option<f64> {
    let t = v.trim().replace(',', ".");
    let mut end = 0;
    let b = t.as_bytes();
    if end < b.len() && b[end] == b'-' { end += 1; }
    while end < b.len() && (b[end].is_ascii_digit() || b[end] == b'.') { end += 1; }
    let num: f64 = t[..end].parse().ok()?;
    if !num.is_finite() { return None; }
    let unit = t[end..].trim().to_ascii_lowercase();
    Some(if unit.starts_with("ft") || unit.starts_with("feet") || unit.starts_with('\'') { num * 0.3048 } else { num })
}

fn default_by_type(t: &str) -> Option<f64> {
    Some(match t {
        "house" | "detached" | "semidetached_house" | "farm" | "barn" => 7.0,
        "terrace" | "transportation" => 8.0,
        "bungalow" | "roof" | "greenhouse" | "service" | "ruins" => 4.0,
        "residential" | "warehouse" | "factory" | "manufacture" | "school" | "parking" | "hangar" | "construction" => 10.0,
        "apartments" | "dormitory" | "university" | "government" => 14.0,
        "hotel" | "church" | "silo" => 20.0,
        "commercial" | "mosque" => 15.0,
        "office" => 16.0,
        "retail" | "supermarket" | "farm_auxiliary" => 6.0,
        "kiosk" | "garage" | "garages" | "shed" | "hut" | "carport" | "bunker" => 3.0,
        "industrial" => 9.0,
        "cathedral" => 35.0,
        "chapel" => 8.0,
        "synagogue" | "temple" | "public" | "civic" | "college" | "sports_hall" | "train_station" => 12.0,
        "kindergarten" | "stable" => 5.0,
        "hospital" => 18.0,
        "stadium" => 25.0,
        "tower" => 30.0,
        _ => return None,
    })
}

fn man_made_height(t: &str) -> Option<f64> {
    Some(match t {
        "tower" | "water_tower" => 30.0, "mast" => 60.0, "chimney" => 50.0, "communications_tower" => 100.0,
        "lighthouse" => 25.0, "windmill" => 20.0, _ => return None,
    })
}

/// Estimate a height in metres from tags; `area` (m²) refines generic buildings.
pub fn estimate_height(tags: &Json, area: f64) -> f64 {
    for k in ["height", "building:height", "est_height"] {
        if let Some(v) = tags.str_of(k).and_then(|s| parse_length(&s)) { if v > 0.0 { return v; } }
    }
    if let Some(levels) = tags.str_of("building:levels").and_then(|s| s.trim().parse::<f64>().ok()) {
        if levels > 0.0 {
            let roof = tags.str_of("roof:levels").and_then(|s| s.trim().parse::<f64>().ok()).unwrap_or(0.0);
            return levels * 3.0 + roof * 2.5;
        }
    }
    if let Some(h) = tags.str_of("man_made").and_then(|m| man_made_height(&m)) { return h; }
    let t = tags.str_of("building").or_else(|| tags.str_of("building:part")).unwrap_or_else(|| "yes".into());
    if let Some(h) = default_by_type(&t) { return h; }
    if area > 2000.0 { 12.0 } else if area > 500.0 { 9.0 } else { 8.0 }
}

fn polygon_area(p: &[(f64, f64)]) -> f64 {
    let n = p.len();
    let mut a = 0.0;
    for i in 0..n { let (x0, y0) = p[i]; let (x1, y1) = p[(i + 1) % n]; a += x0 * y1 - x1 * y0; }
    a.abs() / 2.0
}

fn centroid(p: &[(f64, f64)]) -> (f64, f64) {
    let n = p.len() as f64;
    (p.iter().map(|q| q.0).sum::<f64>() / n, p.iter().map(|q| q.1).sum::<f64>() / n)
}

fn ring_to_local(geometry: &Json, lat0: f64, lon0: f64) -> Option<Vec<(f64, f64)>> {
    let mut poly: Vec<(f64, f64)> = geometry.as_array()?.iter().filter_map(|p| {
        let lat = p.get("lat")?.as_f64()?;
        let lon = p.get("lon")?.as_f64()?;
        Some(to_local(lat0, lon0, lat, lon))
    }).collect();
    if poly.len() > 1 && poly[0] == poly[poly.len() - 1] { poly.pop(); }
    if poly.len() >= 3 { Some(poly) } else { None }
}

/// Turn Overpass JSON into building records in local metres around (lat0, lon0).
pub fn parse_buildings(json_text: &str, lat0: f64, lon0: f64) -> Result<Vec<Building>, String> {
    let root = parse(json_text)?;
    let empty = Json::Object(vec![]);
    let elements = root.get("elements").and_then(|e| e.as_array()).ok_or("overpass: no elements")?;
    let mut out = Vec::new();
    for el in elements {
        let tags = el.get("tags").unwrap_or(&empty);
        let typ = el.str_of("type").unwrap_or_default();
        if typ == "node" {
            if tags.str_of("man_made").is_none() { continue; }
            let (lat, lon) = match (el.get("lat").and_then(|v| v.as_f64()), el.get("lon").and_then(|v| v.as_f64())) { (Some(a), Some(b)) => (a, b), _ => continue };
            let (e, n) = to_local(lat0, lon0, lat, lon);
            let s = tags.str_of("width").and_then(|v| parse_length(&v)).or_else(|| tags.str_of("diameter").and_then(|v| parse_length(&v))).unwrap_or(8.0) / 2.0;
            let poly = vec![(e - s, n - s), (e + s, n - s), (e + s, n + s), (e - s, n + s)];
            out.push(Building { polys: vec![poly], height: estimate_height(tags, 4.0 * s * s), kind: "tower", area: 4.0 * s * s, centroid: (e, n) });
            continue;
        }
        let mut polys = Vec::new();
        if typ == "way" {
            if let Some(g) = el.get("geometry") { if let Some(p) = ring_to_local(g, lat0, lon0) { polys.push(p); } }
        } else if typ == "relation" {
            if let Some(members) = el.get("members").and_then(|m| m.as_array()) {
                for m in members {
                    if m.str_of("type").as_deref() != Some("way") { continue; }
                    if let Some(role) = m.str_of("role") { if !role.is_empty() && role != "outer" { continue; } }
                    if let Some(g) = m.get("geometry") { if let Some(p) = ring_to_local(g, lat0, lon0) { polys.push(p); } }
                }
            }
        }
        if polys.is_empty() { continue; }
        let area: f64 = polys.iter().map(|p| polygon_area(p)).sum();
        let kind = if tags.str_of("building:part").is_some() { "part" } else if tags.str_of("building").is_some() { "building" } else { "tower" };
        let height = estimate_height(tags, area);
        if !(height > 0.0) { continue; }
        let c = centroid(&polys[0]);
        out.push(Building { polys, height, kind, area, centroid: c });
    }
    Ok(out)
}

pub fn point_in_polygon(poly: &[(f64, f64)], x: f64, y: f64) -> bool {
    let mut inside = false;
    let n = poly.len();
    let mut j = n - 1;
    for i in 0..n {
        let (xi, yi) = poly[i];
        let (xj, yj) = poly[j];
        if (yi > y) != (yj > y) && x < (xj - xi) * (y - yi) / (yj - yi) + xi { inside = !inside; }
        j = i;
    }
    inside
}

/// Scan-convert one polygon into a tier with max(); marks the centroid texel when nothing else is covered.
pub fn rasterize_polygon(tier: &mut Tier, poly: &[(f64, f64)], roof_abs: f64) -> usize {
    let size = tier.size;
    let half = size as f64 / 2.0 - 0.5;
    let n = poly.len();
    let xs: Vec<f64> = poly.iter().map(|p| p.0 / tier.texel + half).collect();
    let ys: Vec<f64> = poly.iter().map(|p| p.1 / tier.texel + half).collect();
    let min_j = ys.iter().cloned().fold(f64::INFINITY, f64::min);
    let max_j = ys.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
    let j0 = (min_j.ceil().max(0.0)) as i64;
    let j1 = (max_j.floor().min(size as f64 - 1.0)) as i64;
    let roof = roof_abs as f32;
    let mut written = 0usize;
    let mut cuts: Vec<f64> = Vec::new();
    let mut j = j0;
    while j <= j1 {
        cuts.clear();
        let jf = j as f64;
        let mut m = n - 1;
        for k in 0..n {
            let (y0, y1) = (ys[m], ys[k]);
            if (y0 <= jf) != (y1 <= jf) { cuts.push(xs[m] + (jf - y0) / (y1 - y0) * (xs[k] - xs[m])); }
            m = k;
        }
        if cuts.len() >= 2 {
            cuts.sort_by(|a, b| a.partial_cmp(b).unwrap());
            let mut c = 0;
            while c + 1 < cuts.len() {
                let ia = cuts[c].ceil().max(0.0) as i64;
                let ib = cuts[c + 1].floor().min(size as f64 - 1.0) as i64;
                let row = j as usize * size;
                let mut i = ia;
                while i <= ib {
                    let idx = row + i as usize;
                    if roof > tier.height[idx] { tier.height[idx] = roof; tier.cls[idx] = 1; }
                    written += 1;
                    i += 1;
                }
                c += 2;
            }
        }
        j += 1;
    }
    if written == 0 {
        let cx = xs.iter().sum::<f64>() / n as f64;
        let cy = ys.iter().sum::<f64>() / n as f64;
        let (i, j) = (cx.round() as i64, cy.round() as i64);
        if i >= 0 && j >= 0 && (i as usize) < size && (j as usize) < size {
            let idx = j as usize * size + i as usize;
            if roof > tier.height[idx] { tier.height[idx] = roof; tier.cls[idx] = 1; }
            written = 1;
        }
    }
    written
}

pub struct RasterStats { pub count: usize, pub skipped: usize, pub max_roof: f64 }

/// Rasterise all buildings into the tiers (tiers >= tall_from_tier take only buildings >= tall_threshold;
/// tiers > max_tier take none; footprints containing `exclude` are skipped).
pub fn rasterize_buildings(buildings: &[Building], tiers: &mut [Tier], tall_threshold: f64, tall_from_tier: usize,
                           max_tier: usize, height_scale: f64, exclude: Option<(f64, f64)>) -> RasterStats {
    let max_tier = max_tier.min(tiers.len().saturating_sub(1));
    let grounds: Vec<f64> = buildings.iter().map(|b| {
        match tier_index_for(tiers, b.centroid.0, b.centroid.1) { Some(k) => tiers[k].sample(b.centroid.0, b.centroid.1), None => 0.0 }
    }).collect();
    let (mut count, mut skipped, mut max_roof) = (0usize, 0usize, f64::NEG_INFINITY);
    for (b, bld) in buildings.iter().enumerate() {
        if let Some((ex, ey)) = exclude { if bld.polys.iter().any(|p| point_in_polygon(p, ex, ey)) { skipped += 1; continue; } }
        let roof_abs = grounds[b] + bld.height * height_scale;
        if roof_abs > max_roof { max_roof = roof_abs; }
        let dist = (bld.centroid.0.powi(2) + bld.centroid.1.powi(2)).sqrt();
        let mut any = false;
        for k in 0..=max_tier {
            if dist > tiers[k].radius { continue; }
            if k >= tall_from_tier && bld.height < tall_threshold { continue; }
            for poly in &bld.polys { rasterize_polygon(&mut tiers[k], poly, roof_abs); }
            any = true;
        }
        if any { count += 1; }
    }
    RasterStats { count, skipped, max_roof: if max_roof.is_finite() { max_roof } else { 0.0 } }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn lengths_and_heights() {
        assert_eq!(parse_length("25"), Some(25.0));
        assert_eq!(parse_length("25 m"), Some(25.0));
        assert_eq!(parse_length("25,5"), Some(25.5));
        assert!((parse_length("82 ft").unwrap() - 24.9936).abs() < 1e-3);
        assert!((parse_length("82'").unwrap() - 24.9936).abs() < 1e-3);
        assert_eq!(parse_length("tall"), None);
        let t = parse(r#"{"building:levels":"10","roof:levels":"1"}"#).unwrap();
        assert_eq!(estimate_height(&t, 0.0), 32.5);
        let t = parse(r#"{"building":"church"}"#).unwrap();
        assert_eq!(estimate_height(&t, 0.0), 20.0);
        let t = parse(r#"{"building":"yes"}"#).unwrap();
        assert_eq!(estimate_height(&t, 3000.0), 12.0);
    }
}
