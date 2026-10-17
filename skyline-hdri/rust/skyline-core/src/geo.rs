//! Geodesy helpers — the Rust twin of `src/engine/geo.js`.
//!
//! Conventions: lat/lon in degrees, bearings in radians clockwise from north, "east/north" in metres
//! in the azimuthal-equidistant frame around the eye, Web Mercator pixels at 256 px per tile.

pub const EARTH_RADIUS: f64 = 6_371_000.0;
pub const REFRACTION_K: f64 = 0.13;
pub const EFFECTIVE_RADIUS: f64 = EARTH_RADIUS / (1.0 - REFRACTION_K);
pub const DEG: f64 = std::f64::consts::PI / 180.0;

/// Great-circle destination point from (lat, lon) along `bearing` (rad) for `distance` metres.
pub fn destination(lat: f64, lon: f64, bearing: f64, distance: f64) -> (f64, f64) {
    let phi1 = lat * DEG;
    let lam1 = lon * DEG;
    let delta = distance / EARTH_RADIUS;
    let (sin_phi1, cos_phi1) = (phi1.sin(), phi1.cos());
    let (sin_delta, cos_delta) = (delta.sin(), delta.cos());
    let sin_phi2 = sin_phi1 * cos_delta + cos_phi1 * sin_delta * bearing.cos();
    let phi2 = sin_phi2.clamp(-1.0, 1.0).asin();
    let y = bearing.sin() * sin_delta * cos_phi1;
    let x = cos_delta - sin_phi1 * sin_phi2;
    let lam2 = lam1 + y.atan2(x);
    (phi2 / DEG, ((lam2 / DEG + 540.0) % 360.0) - 180.0)
}

/// Local flat-frame offset (east, north metres) -> (lat, lon). Good to ~1 m within 3 km.
pub fn flat_offset(lat0: f64, lon0: f64, east: f64, north: f64) -> (f64, f64) {
    let lat = lat0 + (north / EARTH_RADIUS) / DEG;
    let lon = lon0 + (east / (EARTH_RADIUS * (lat0 * DEG).cos())) / DEG;
    (lat, lon)
}

/// (lat, lon) -> azimuthal-equidistant metres (east, north) around (lat0, lon0). Inverse of `destination`.
pub fn to_local(lat0: f64, lon0: f64, lat: f64, lon: f64) -> (f64, f64) {
    let phi1 = lat0 * DEG;
    let phi2 = lat * DEG;
    let d_lam = (lon - lon0) * DEG;
    let sin_dphi = ((phi2 - phi1) / 2.0).sin();
    let sin_dlam = (d_lam / 2.0).sin();
    let a = sin_dphi * sin_dphi + phi1.cos() * phi2.cos() * sin_dlam * sin_dlam;
    let d = 2.0 * EARTH_RADIUS * a.sqrt().atan2((1.0 - a).max(0.0).sqrt());
    let y = d_lam.sin() * phi2.cos();
    let x = phi1.cos() * phi2.sin() - phi1.sin() * phi2.cos() * d_lam.cos();
    let bearing = y.atan2(x);
    (d * bearing.sin(), d * bearing.cos())
}

/// Web Mercator: (lat, lon) -> fractional pixel coordinates at zoom z (256 px tiles).
pub fn mercator_pixel(lat: f64, lon: f64, z: u32) -> (f64, f64) {
    let n = 256.0 * (1u64 << z) as f64;
    let x = (lon + 180.0) / 360.0 * n;
    let phi = lat.clamp(-85.05112878, 85.05112878) * DEG;
    let y = (1.0 - (phi.tan() + 1.0 / phi.cos()).ln() / std::f64::consts::PI) / 2.0 * n;
    (x, y)
}

/// Inverse Web Mercator: pixel coordinates at zoom z -> (lat, lon).
pub fn pixel_to_lat_lon(px: f64, py: f64, z: u32) -> (f64, f64) {
    let n = 256.0 * (1u64 << z) as f64;
    let lon = px / n * 360.0 - 180.0;
    let yv = std::f64::consts::PI * (1.0 - 2.0 * py / n);
    let lat = yv.sinh().atan() / DEG;
    (lat, lon)
}

/// Apparent elevation angle (rad) of a point at height `h`, ground distance `d`, from eye altitude `ze`.
pub fn apparent_elevation(h: f64, d: f64, ze: f64) -> f64 {
    (h - ze - d * d / (2.0 * EFFECTIVE_RADIUS)).atan2(d)
}

/// Height of the ray with elevation angle `e` at ground distance `d`.
pub fn ray_height(e: f64, d: f64, ze: f64) -> f64 {
    ze + d * e.tan() + d * d / (2.0 * EFFECTIVE_RADIUS)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn destination_and_to_local_are_inverse() {
        let (lat, lon) = destination(52.0, 5.0, 45.0 * DEG, 5000.0);
        let (e, n) = to_local(52.0, 5.0, lat, lon);
        let s = 5000.0 * std::f64::consts::FRAC_1_SQRT_2;
        assert!(((e - s).powi(2) + (n - s).powi(2)).sqrt() < 0.01);
    }

    #[test]
    fn mercator_round_trip() {
        let (px, py) = mercator_pixel(52.37, 4.89, 14);
        let (lat, lon) = pixel_to_lat_lon(px, py, 14);
        assert!((lat - 52.37).abs() < 1e-9 && (lon - 4.89).abs() < 1e-9);
    }
}
