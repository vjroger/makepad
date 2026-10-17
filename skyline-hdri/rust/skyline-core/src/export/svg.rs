//! SVG horizon bands. Twin of `src/export/svg.js`.

use crate::sweep::Plate;
use std::f64::consts::PI;

/// Douglas-Peucker simplification; tolerance in pixels.
pub fn simplify(points: &[(f64, f64)], tolerance: f64) -> Vec<(f64, f64)> {
    if points.len() < 3 { return points.to_vec(); }
    let mut keep = vec![false; points.len()];
    keep[0] = true; keep[points.len() - 1] = true;
    let mut stack = vec![(0usize, points.len() - 1)];
    while let Some((a, b)) = stack.pop() {
        let (ax, ay) = points[a];
        let (bx, by) = points[b];
        let (dx, dy) = (bx - ax, by - ay);
        let len = (dx * dx + dy * dy).sqrt().max(1e-12);
        let (mut max_d, mut idx) = (0.0, None);
        for i in a + 1..b {
            let (px, py) = points[i];
            let d = (dy * px - dx * py + bx * ay - by * ax).abs() / len;
            if d > max_d { max_d = d; idx = Some(i); }
        }
        if let Some(i) = idx { if max_d > tolerance { keep[i] = true; stack.push((a, i)); stack.push((i, b)); } }
    }
    points.iter().zip(keep).filter(|(_, k)| *k).map(|(p, _)| *p).collect()
}

pub fn row_of_angle(e: f64, height: usize) -> f64 { (0.5 - e / PI) * height as f64 }

/// Build the SVG: full horizon first, then each band from the largest radius down (far → near, light → dark).
pub fn plate_to_svg(plate: &Plate, bands_m: &[f64], max_distance: f64, lat: f64, lon: f64, heading_deg: f64, tolerance: f64) -> String {
    let (w, h) = (plate.width, plate.height);
    let mut layers: Vec<(String, Vec<f32>)> = vec![(format!("all (to {} km)", (max_distance / 1000.0).round()), plate.horizon.clone())];
    for b in (0..bands_m.len()).rev() {
        layers.push((format!("within {} km", bands_m[b] / 1000.0), plate.bands[b * w..(b + 1) * w].to_vec()));
    }
    let n = layers.len();
    let mut body = String::new();
    for (i, (label, angles)) in layers.iter().enumerate() {
        let pts: Vec<(f64, f64)> = (0..w).map(|x| (x as f64 + 0.5, row_of_angle(angles[x] as f64, h))).collect();
        let simp = simplify(&pts, tolerance);
        let mut d = format!("M0,{} L0,{:.2} ", h, simp[0].1);
        for (x, y) in &simp { d.push_str(&format!("L{:.2},{:.2} ", x, y)); }
        d.push_str(&format!("L{},{:.2} L{},{} Z", w, simp[simp.len() - 1].1, w, h));
        let g = (225.0 - (i as f64 / (n.max(2) - 1) as f64) * 150.0).round() as i32;
        body.push_str(&format!("  <path id=\"band-{}\" data-label=\"{}\" fill=\"rgb({g},{g},{g})\" d=\"{}\"/>\n", i, label, d, g = g));
    }
    format!("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 {w} {h}\" width=\"{w}\" height=\"{h}\">\n<!-- skyline-hdri: lat {lat} lon {lon} heading {heading} deg; x = azimuth (-180..180 deg, centre = north), y = elevation (+90 top .. -90 bottom) -->\n{body}</svg>\n",
        w = w, h = h, lat = lat, lon = lon, heading = heading_deg, body = body)
}
