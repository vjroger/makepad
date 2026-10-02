//! Procedural HDRI generator: outdoor skies (sun, atmosphere, clouds, moon,
//! stars) and studio lighting (a backdrop plus shaped lights) as linear
//! Rec.709 HDR equirect maps, in the engine's own [`EnvMap`].
//!
//! [`Env::new`] does all the work that depends only on the parameters, and
//! [`Env::radiance`] then answers "what arrives from this direction" for
//! any direction. The app preview, file export and the engine's dome all
//! sample that one function, so they cannot disagree.
//!
//! Conventions (pinned by the tests at the bottom of this file):
//! - Y is up, +X is east and -Z is north.
//! - Azimuths are degrees clockwise from north (0 = -Z, 90 = +X), as in NOAA
//!   and `ibl::dir_deg`.
//! - Equirect: `ibl`'s, and only `ibl`'s: `u = 0.5 + atan2(x, -z) / 2pi`,
//!   `v = acos(y) / pi`. -Z (north) sits at the image centre, +X at
//!   u = 0.75, row 0 is the zenith and texels are evaluated at their
//!   centres. `EnvMap::from_fn`, `prefilter`, `sh9`, `DrawEnvBackground`
//!   and the `mat_ibl_*` shader functions all assume this, so a baked map
//!   plugs into every one of them unchanged. Files keep the DCC convention
//!   (+X at the centre); `image::roll_quarter` converts at the file
//!   boundary and nowhere else.
//! - `rotation_deg` turns the content about +Y, positive counter-clockwise
//!   seen from above, exactly as `EnvMap::procedural` and `Ibl.rotation_deg`
//!   do: +90 moves a light at -Z to -X ([`rotate_y`]).
//! - `ibl` works in `[f32; 3]`; this module works in `Vec3f` and converts at
//!   the boundary with [`arr`] and [`vec`].
//!
//! Design: docs/superpowers/specs/2026-09-29-procedural-hdri-generator-design.md

// `!(x > 0.0)` and its kin are written negated on purpose throughout this module and its
// children: a NaN fails the comparison, so it takes the guard's branch. Clippy would have
// them as `partial_cmp`, which says the same in more words.
#![allow(clippy::neg_cmp_op_on_partial_ord)]

use makepad_draw::*;
use makepad_render_material::ibl::{self, EnvMap};

pub mod params;
pub mod presets;
pub mod noise;
pub mod studio;
pub mod atmosphere;
pub mod clouds;
pub mod night;
pub mod image;
pub mod export;

pub use params::*;

use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};

/// The strongest radiance a bake writes: far above any real sun, but finite,
/// so a layer bug cannot put infinity into an export or the SH projection.
const MAX_RADIANCE: f32 = 1.0e30;

/// Sub-samples per axis for a texel that touches a hot spot.
const HOT_SAMPLES: usize = 8;
/// A feature is refined while its radius is under this many texels; a wider one is resolved by the grid itself.
const HOT_MAX_TEXELS: f32 = 6.0;

/// `ibl` works in `[f32; 3]`; the layers work in `Vec3f`. These two
/// functions are the whole boundary between them.
pub fn arr(v: Vec3f) -> [f32; 3] {
    [v.x, v.y, v.z]
}

pub fn vec(a: [f32; 3]) -> Vec3f {
    vec3f(a[0], a[1], a[2])
}

/// The environment's key light, for engines that light with one directional
/// light next to the map. Phase 2 (task C1) moves this struct to makepad-scene
/// and re-exports it here.
/// - `dir` points toward the light (unit, world space).
/// - `cos_radius` is the cosine of the angular radius of the key's cone.
/// - `radiance` is the AVERAGE radiance over that cone, with one meaning for
///   every key: the light's whole emission (∫ L dΩ as the map draws it) divided
///   by the cone's solid angle 2π(1 − cos_radius). So `radiance × 2π(1 −
///   cos_radius)` is the key's irradiance on a surface facing it (for a small
///   light; a wide one is below).
///
/// Per key:
/// - The sun: its irradiance at the ground spread over the nominal disc's cone,
///   the soft limb past it included (`Atmosphere::sun_cone_radiance`).
/// - The moon: the mean over its disc.
/// - A studio key: the light integrated once per map over its own tangent
///   plane, with its shape, corner, ring, soft edge, hotspot and roll and the
///   Multiply flags after it, so a thin strip or a ring carries only what it
///   draws (`studio::key_emission`).
///
/// The cosine across a wide light is not in it. A surface facing the Overcast
/// dome preset's 110 degree disc gets 0.78 of `radiance × solid angle` and one
/// facing the Top softbox 0.93; the other built-in keys get 0.95 or more, and
/// the sun and moon 1.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct EnvSun {
    pub dir: Vec3f,
    pub radiance: Vec3f,
    pub cos_radius: f32,
}

/// One environment, ready to sample: every layer is built once in `new`, so
/// `radiance` is cheap enough to call per texel from many threads (the struct
/// stays `Send + Sync`; A1's test pins that).
pub struct Env {
    /// Clamped copy of the parameters the layers were built from.
    params: HdriParams,
    /// 2^intensity_ev, applied last so every layer keeps its own units.
    scale: f32,
    /// The studio backdrop (Studio mode) and the light overlay (both modes).
    studio: studio::Studio,
    /// The clear sky; Some exactly in Sky mode. The three Sky layers (atmosphere,
    /// clouds, night) are all built in the map's own, unrotated frame.
    atmo: Option<atmosphere::Atmosphere>,
    /// Cloud deck and cirrus; Some in Sky mode when there is any cover.
    clouds: Option<clouds::CloudLayer>,
    /// Stars, moon and glow; Some exactly in Sky mode.
    night: Option<night::NightSky>,
    /// Light on the clouds, gathered once per map: the direct sun, and the ambient
    /// of the day sky plus the night (glow and moonlit air).
    cloud_sun: Vec3f,
    cloud_ambient: Vec3f,
}

impl Env {
    /// Clamps a copy of `params` and builds every layer once.
    pub fn new(params: &HdriParams) -> Env {
        let mut params = params.clone();
        params.clamp();
        let scale = 2.0f32.powf(params.intensity_ev);
        let studio = studio::Studio::new(&params.studio, &params.lights);
        let (atmo, clouds, night, cloud_sun, cloud_ambient) = match params.mode() {
            Mode::Sky => {
                let sky = &params.sky;
                let sun_dir = atmosphere::sun_direction(&sky.sun);
                let atmo = atmosphere::Atmosphere::new(sun_dir, &sky.atmosphere, &sky.sun_disc);
                let clouds = clouds::CloudLayer::new(&sky.clouds, params.seed, atmo.sun_dir());
                let night = night::NightSky::new(&sky.night, params.seed, atmo.sun_dir(), night::star_hours(&sky.sun), sky.sun.latitude);
                let cloud_sun = atmo.sun_irradiance();
                let cloud_ambient = atmo.ambient() + night.ambient();
                (Some(atmo), clouds, Some(night), cloud_sun, cloud_ambient)
            }
            Mode::Studio => (None, None, None, Vec3f::default(), Vec3f::default()),
        };
        Env { params, scale, studio, atmo, clouds, night, cloud_sun, cloud_ambient }
    }

    pub fn params(&self) -> &HdriParams {
        &self.params
    }

    /// Linear radiance arriving from `dir` (unit, world space, as displayed:
    /// rotation already applied). Order: undo the rotation, base layer (Sky:
    /// atmosphere + disc + stars + moon + glow + night ground, then the clouds
    /// over it; Studio: backdrop), lights overlay, x 2^intensity_ev.
    pub fn radiance(&self, dir: Vec3f) -> Vec3f {
        // The layers live in the map's own frame; turning the map by
        // rotation_deg (ibl's sign) is turning the lookup the other way.
        let d = rotate_y(dir, -self.params.rotation_deg);
        let base = match &self.atmo {
            Some(atmo) => {
                let mut c = atmo.sky(d);
                // The sun's and the moon's discs are blocked by the unfaded
                // cover, like their keys in `sun`; the rest of the sky by the
                // visible deck, which thins into the haze at the horizon.
                let mut discs = atmo.sun_disc(d);
                if let Some(night) = &self.night {
                    let g = self.params.sky.atmosphere.ground_color;
                    c += night.stars(d) + night.glow(d) + night.ground(d, vec3f(g[0], g[1], g[2]));
                    discs += night.moon(d);
                }
                // Clouds are in front of everything in the sky: the stars, the
                // moon and the sun's disc.
                if let Some(layer) = &self.clouds {
                    let s = layer.sample(d);
                    if s.alpha > 0.0 {
                        let lit = layer.shade(&s, self.cloud_sun, self.cloud_ambient);
                        c = c * (1.0 - s.alpha) + lit * s.alpha;
                    }
                    // Only a texel on a disc pays for the second cloud lookup.
                    if discs.max_elem() > 0.0 {
                        discs *= 1.0 - layer.cover_toward(d);
                    }
                }
                c + discs
            }
            None => self.studio.backdrop(d),
        };
        self.studio.apply_lights(d, base) * self.scale
    }

    /// Key light for the engine, in world space, already x 2^intensity_ev
    /// (every layer's key goes through `key_to_world`, which turns it with the
    /// map and applies the intensity). In order: the key light (both modes);
    /// Sky mode only: the sun while it is above the horizon, dimmed by the cloud
    /// cover along it (unfaded: a full overcast hides a 2 degree sun too); else
    /// the risen moon once the sun is 6 degrees down, dimmed the same way; else
    /// None.
    pub fn sun(&self) -> Option<EnvSun> {
        if let Some(key) = self.studio.key() {
            return Some(self.key_to_world(key));
        }
        // Studio mode has no sky and no sun.
        let atmo = self.atmo.as_ref()?;
        // The same cover that hides the disc in the map dims the key.
        let cover = |dir: Vec3f| self.clouds.as_ref().map_or(1.0, |layer| 1.0 - layer.cover_toward(dir));
        let sun = atmo.sun_dir();
        if sun.y > 0.0 {
            // The cone's mean radiance, so radiance x cone solid angle is the
            // sun's irradiance at the ground whatever the disc's size and limb.
            return Some(self.key_to_world(EnvSun {
                dir: sun,
                radiance: atmo.sun_cone_radiance() * cover(sun),
                cos_radius: atmo.sun_cos_radius(),
            }));
        }
        // After dark the risen moon takes over, once the sun is 6 degrees down.
        let moon = self.night.as_ref()?.moon_key()?;
        Some(self.key_to_world(EnvSun {
            dir: moon.dir,
            radiance: moon.radiance * cover(moon.dir),
            cos_radius: moon.cos_radius,
        }))
    }

    /// World-space sun direction in Sky mode (also below the horizon), None
    /// in Studio mode.
    pub fn sun_dir(&self) -> Option<Vec3f> {
        self.atmo
            .as_ref()
            .map(|atmo| rotate_y(atmo.sun_dir(), self.params.rotation_deg))
    }

    /// Map frame to world frame, with the map's intensity applied.
    fn key_to_world(&self, key: EnvSun) -> EnvSun {
        EnvSun {
            dir: rotate_y(key.dir, self.params.rotation_deg),
            radiance: key.radiance * self.scale,
            cos_radius: key.cos_radius,
        }
    }

    /// The sun's and the moon's discs, in world space, for the bake's
    /// refinement: below 4K both are smaller than or comparable to a texel, so
    /// their texels are area-averaged (A1's `refine_hot_spots`).
    fn hot_spots(&self) -> Vec<(Vec3f, f32)> {
        let mut hot = Vec::new();
        if let Some(atmo) = &self.atmo {
            if self.params.sky.sun_disc.visible && atmo.sun_dir().y > -0.1 {
                hot.push((rotate_y(atmo.sun_dir(), self.params.rotation_deg), atmo.sun_outer_radius()));
            }
        }
        if let Some((dir, radius)) = self.night.as_ref().and_then(|night| night.moon_disc()) {
            if dir.y > -0.1 {
                hot.push((rotate_y(dir, self.params.rotation_deg), radius));
            }
        }
        hot
    }

    /// Serial bake of the whole map (tests, small previews): `EnvMap::from_fn`
    /// over `radiance`, so the map is in ibl's convention by construction;
    /// then the texels on the hot spots (the sun's and moon's discs) are
    /// replaced by their area average.
    pub fn bake(&self, width: usize) -> EnvMap {
        let radiance = |d: Vec3f| self.radiance(d);
        let mut map = bake_with(width, &radiance);
        refine_hot_spots(&mut map, &radiance, &self.hot_spots());
        map
    }

    /// Row-parallel bake. `run(n, f)` must call f(i) for every i in 0..n
    /// (`pool.fan_out(Lane::Heavy, n, f)` inside a submitted Heavy job; on the
    /// UI thread run it serially, as `render_kernels::task_pool_executor` does).
    /// A skipped row (cancelled) stays black. Equals `bake` bit for bit.
    pub fn bake_par(&self, width: usize, run: impl FnOnce(usize, &(dyn Fn(usize) + Sync))) -> EnvMap {
        let radiance = |d: Vec3f| self.radiance(d);
        // Count the rows the runner really baked: a cancelled bake keeps its
        // skipped rows black, so it gets no refinement either.
        let baked = AtomicUsize::new(0);
        let mut map = bake_par_with(width, &radiance, |rows, row| {
            run(rows, &|y: usize| {
                row(y);
                baked.fetch_add(1, Ordering::Relaxed);
            })
        });
        if baked.load(Ordering::Relaxed) >= map.height {
            refine_hot_spots(&mut map, &radiance, &self.hot_spots());
        }
        map
    }
}

/// Unit direction for an azimuth (degrees clockwise from north) and an
/// elevation (degrees above the horizon): (sin az cos el, sin el, -cos az cos el).
pub fn dir_from_az_el(azimuth_deg: f32, elevation_deg: f32) -> Vec3f {
    let (az, el) = (azimuth_deg.to_radians(), elevation_deg.to_radians());
    let horizontal = el.cos();
    vec3f(az.sin() * horizontal, el.sin(), -az.cos() * horizontal)
}

/// Azimuth in [0, 360) and elevation in [-90, 90], in degrees. The input need
/// not be normalised. Straight up or down (or no direction at all) has no
/// azimuth and reports 0.
pub fn az_el_from_dir(dir: Vec3f) -> (f32, f32) {
    let horizontal = (dir.x * dir.x + dir.z * dir.z).sqrt();
    // The negated test also catches NaN.
    if !(horizontal > 1.0e-6 * dir.y.abs()) {
        let elevation = if dir.y > 0.0 {
            90.0
        } else if dir.y < 0.0 {
            -90.0
        } else {
            0.0
        };
        return (0.0, elevation);
    }
    // atan2 for both angles: no normalisation is needed, and it stays
    // accurate near the poles where asin(y) does not.
    let elevation = dir.y.atan2(horizontal).to_degrees();
    let mut azimuth = dir.x.atan2(-dir.z).to_degrees();
    if azimuth < 0.0 {
        azimuth += 360.0;
    }
    // A tiny negative angle plus 360 rounds to 360, which is outside [0, 360).
    if azimuth >= 360.0 {
        azimuth = 0.0;
    }
    (azimuth, elevation)
}

/// Turns `dir` about +Y by `deg`, positive counter-clockwise seen from above
/// (ibl's sign: +90 takes -Z to -X, the turn `EnvMap::procedural` and the
/// shader's `mat_ibl_dir` apply to a `rotation_deg`); the elevation does not
/// change. In compass terms the azimuth falls by `deg`.
pub fn rotate_y(dir: Vec3f, deg: f32) -> Vec3f {
    let (s, c) = deg.to_radians().sin_cos();
    vec3f(dir.x * c + dir.z * s, dir.y, dir.z * c - dir.x * s)
}

/// One baked channel made safe for every consumer: NaN becomes 0, a negative
/// becomes 0 (radiance is never negative) and the rest stays finite. Layer
/// tests that need the raw values call `Env::radiance` directly.
fn clean_radiance(v: f32) -> f32 {
    if v.is_nan() {
        0.0
    } else {
        v.clamp(0.0, MAX_RADIANCE)
    }
}

/// A texel's RGB from a radiance: the array crossing plus the clean-up. Both
/// bakes go through this one function, which is what keeps them bit-identical.
fn texel(radiance: Vec3f) -> [f32; 3] {
    [clean_radiance(radiance.x), clean_radiance(radiance.y), clean_radiance(radiance.z)]
}

/// The serial bake behind `Env::bake`, over any radiance function.
fn bake_with(width: usize, radiance: &dyn Fn(Vec3f) -> Vec3f) -> EnvMap {
    EnvMap::from_fn(width, |d| texel(radiance(vec(d))))
}

/// The parallel bake behind `Env::bake_par`, over any radiance function.
fn bake_par_with(
    width: usize,
    radiance: &(dyn Fn(Vec3f) -> Vec3f + Sync),
    run: impl FnOnce(usize, &(dyn Fn(usize) + Sync)),
) -> EnvMap {
    // from_fn's size rounding, so the two bakes agree on every request.
    let width = width.max(2);
    let height = (width / 2).max(1);
    // Rows a cancelled run never reaches stay black, with the alpha every
    // texel carries.
    let mut data = vec![[0.0, 0.0, 0.0, 1.0]; width * height];
    {
        // One lock per row. Each row is written by one call, so the locks
        // never contend; they only let disjoint rows cross threads safely.
        let rows: Vec<Mutex<&mut [[f32; 4]]>> = data.chunks_exact_mut(width).map(Mutex::new).collect();
        let bake_row = |y: usize| {
            if let Some(Ok(mut row)) = rows.get(y).map(|row| row.lock()) {
                // The same texel-centre arithmetic as EnvMap::from_fn, through
                // the same function, so the bits match.
                let v = (y as f32 + 0.5) / height as f32;
                for (x, out) in row.iter_mut().enumerate() {
                    let d = ibl::equirect_uv_to_dir([(x as f32 + 0.5) / width as f32, v]);
                    let c = texel(radiance(vec(d)));
                    *out = [c[0], c[1], c[2], 1.0];
                }
            }
        };
        run(height, &bake_row);
    }
    EnvMap { width, height, data }
}

/// Re-evaluates the texels on and around each hot spot as the mean of
/// HOT_SAMPLES x HOT_SAMPLES sub-samples spread over the texel. A 0.5 degree
/// sun is smaller than a texel below 1K: at texel centres alone it lands on
/// one texel or none, and the map's sun energy swings from 0 to 2.5 times.
/// Both bakes run this same serial pass, so they still agree bit for bit.
fn refine_hot_spots(map: &mut EnvMap, radiance: &dyn Fn(Vec3f) -> Vec3f, hot: &[(Vec3f, f32)]) {
    let (w, h) = (map.width, map.height);
    if w == 0 || h == 0 || map.data.len() != w * h {
        return;
    }
    // One texel of latitude, in radians.
    let step = std::f32::consts::PI / h as f32;
    for &(centre, radius) in hot {
        // The negated comparison also skips a NaN radius.
        if !centre.is_finite() || !(radius > 0.0) || radius >= HOT_MAX_TEXELS * step {
            continue;
        }
        let centre = centre.normalize();
        // The feature plus a texel's diagonal: every texel it can touch.
        let reach = radius + 1.5 * step;
        let cos_reach = reach.min(std::f32::consts::PI).cos();
        let row = ibl::dir_to_equirect_uv(arr(centre))[1] * h as f32;
        let rows = reach / step;
        let y0 = (row - rows).floor().max(0.0) as usize;
        let y1 = ((row + rows).ceil().max(0.0) as usize).min(h);
        for y in y0..y1 {
            let v = (y as f32 + 0.5) / h as f32;
            for x in 0..w {
                let u = (x as f32 + 0.5) / w as f32;
                if vec(ibl::equirect_uv_to_dir([u, v])).dot(centre) < cos_reach {
                    continue;
                }
                let mut sum = Vec3f::default();
                for j in 0..HOT_SAMPLES {
                    let sv = v + ((j as f32 + 0.5) / HOT_SAMPLES as f32 - 0.5) / h as f32;
                    for i in 0..HOT_SAMPLES {
                        let su = u + ((i as f32 + 0.5) / HOT_SAMPLES as f32 - 0.5) / w as f32;
                        let s = radiance(vec(ibl::equirect_uv_to_dir([su, sv])));
                        if s.is_finite() {
                            sum += s;
                        }
                    }
                }
                let c = texel(sum * (1.0 / (HOT_SAMPLES * HOT_SAMPLES) as f32));
                map.data[y * w + x] = [c[0], c[1], c[2], 1.0];
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: f32, b: f32, eps: f32) -> bool {
        (a - b).abs() <= eps
    }

    fn close3(a: Vec3f, b: Vec3f, eps: f32) -> bool {
        (a - b).length() <= eps
    }

    /// Direction-coded radiance: every channel in 0..1, so the bake's
    /// clean-up leaves it exactly as it is and a texel decodes back to the
    /// direction it was evaluated at.
    fn coded(d: Vec3f) -> Vec3f {
        (d + Vec3f::all(1.0)) * 0.5
    }

    /// The direction a `coded` texel was evaluated at.
    fn decoded(texel: [f32; 4]) -> Vec3f {
        vec3f(texel[0], texel[1], texel[2]) * 2.0 - Vec3f::all(1.0)
    }

    /// A deterministic spread of unit vectors (LCG, rejection-sampled from the unit ball).
    fn pseudo_random_dir(state: &mut u32) -> Vec3f {
        loop {
            let mut next = || {
                *state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                (*state >> 8) as f32 / 16_777_216.0 * 2.0 - 1.0
            };
            let d = vec3f(next(), next(), next());
            let length = d.length();
            if length > 0.1 && length <= 1.0 {
                return d / length;
            }
        }
    }

    /// Runs every index on four scoped threads, interleaved, as fan_out would.
    fn four_threads(n: usize, f: &(dyn Fn(usize) + Sync)) {
        std::thread::scope(|scope| {
            for t in 0..4 {
                scope.spawn(move || {
                    let mut i = t;
                    while i < n {
                        f(i);
                        i += 4;
                    }
                });
            }
        });
    }

    #[test]
    fn arr_and_vec_cross_the_boundary_losslessly() {
        let v = vec3f(0.25, -3.0, 1.0e-7);
        assert_eq!(arr(v), [0.25, -3.0, 1.0e-7]);
        assert_eq!(vec(arr(v)), v);
        assert_eq!(vec([1.0, 2.0, 3.0]), vec3f(1.0, 2.0, 3.0));
        // ibl's helpers take and return arrays; a round trip through them is
        // the whole boundary this module crosses.
        let d = vec(ibl::equirect_uv_to_dir(ibl::dir_to_equirect_uv(arr(vec3f(0.6, 0.0, -0.8)))));
        assert!(close3(d, vec3f(0.6, 0.0, -0.8), 1e-5), "{d:?}");
    }

    #[test]
    fn azimuth_and_elevation_follow_the_compass() {
        assert!(close3(dir_from_az_el(0.0, 0.0), vec3f(0.0, 0.0, -1.0), 1e-6), "azimuth 0 is north (-Z)");
        assert!(close3(dir_from_az_el(90.0, 0.0), vec3f(1.0, 0.0, 0.0), 1e-6), "azimuth 90 is east (+X)");
        assert!(close3(dir_from_az_el(180.0, 0.0), vec3f(0.0, 0.0, 1.0), 1e-6), "azimuth 180 is south (+Z)");
        assert!(close3(dir_from_az_el(270.0, 0.0), vec3f(-1.0, 0.0, 0.0), 1e-6), "azimuth 270 is west (-X)");
        assert!(dir_from_az_el(123.0, 90.0).y > 0.999_999);
        for az in (0..24).map(|i| i as f32 * 15.0) {
            for el in [-80.0f32, -45.0, -10.0, 0.0, 10.0, 45.0, 80.0] {
                let (a, e) = az_el_from_dir(dir_from_az_el(az, el));
                let da = ((a - az + 540.0).rem_euclid(360.0) - 180.0).abs();
                assert!(da < 1e-3 && close(e, el, 1e-3), "({az}, {el}) came back as ({a}, {e})");
                assert!((0.0..360.0).contains(&a));
            }
            // On the horizon, azimuth maps linearly onto ibl's u, from north
            // at the centre: u = 0.5 + az / 360 (mod 1). This is the one
            // place the compass meets the equirect convention.
            let u = ibl::dir_to_equirect_uv(arr(dir_from_az_el(az, 0.0)))[0];
            let want = (0.5 + az / 360.0).rem_euclid(1.0);
            let du = (u - want).abs().min(1.0 - (u - want).abs());
            assert!(du < 1e-5, "az {az}: u {u}, want {want}");
        }
        // The anchors of ibl's convention, through this module's helpers.
        assert!(close(ibl::dir_to_equirect_uv(arr(dir_from_az_el(0.0, 0.0)))[0], 0.5, 1e-6), "north at the centre");
        assert!(close(ibl::dir_to_equirect_uv(arr(dir_from_az_el(90.0, 0.0)))[0], 0.75, 1e-6), "+X at u = 0.75");
        assert!(close(ibl::dir_to_equirect_uv(arr(dir_from_az_el(270.0, 0.0)))[0], 0.25, 1e-6), "-X at u = 0.25");
        assert!(close(ibl::dir_to_equirect_uv(arr(dir_from_az_el(45.0, 60.0)))[1], 30.0 / 180.0, 1e-6), "v is the angle from the zenith");
        assert_eq!(az_el_from_dir(vec3f(0.0, 1.0, 0.0)), (0.0, 90.0));
        assert_eq!(az_el_from_dir(vec3f(0.0, -2.0, 0.0)), (0.0, -90.0));
        assert_eq!(az_el_from_dir(Vec3f::default()), (0.0, 0.0));
    }

    #[test]
    fn rotate_y_turns_counter_clockwise_seen_from_above() {
        let north = vec3f(0.0, 0.0, -1.0);
        let west = vec3f(-1.0, 0.0, 0.0);
        // ibl's sign (EnvMap::procedural, mat_ibl_dir): +90 takes -Z to -X.
        assert!(close3(rotate_y(north, 90.0), west, 1e-6), "north + 90 = west: {:?}", rotate_y(north, 90.0));
        assert!(close3(rotate_y(west, 90.0), vec3f(0.0, 0.0, 1.0), 1e-6), "west + 90 = south");
        assert!(close3(rotate_y(north, -90.0), vec3f(1.0, 0.0, 0.0), 1e-6), "north - 90 = east");
        assert_eq!(rotate_y(north, 0.0), north, "no turn is exact");
        let mut state = 7u32;
        for _ in 0..50 {
            let d = pseudo_random_dir(&mut state);
            let r = rotate_y(d, 37.0);
            assert!(close(r.y, d.y, 1e-6) && close(r.length(), 1.0, 1e-5), "elevation and length are kept");
            assert!(close3(rotate_y(r, -37.0), d, 1e-5), "the opposite turn undoes it");
            if d.y.abs() < 0.99 {
                // Counter-clockwise from above is a falling compass azimuth.
                let (a0, e0) = az_el_from_dir(d);
                let (a1, e1) = az_el_from_dir(r);
                let da = ((a1 - a0 + 37.0 + 540.0).rem_euclid(360.0) - 180.0).abs();
                assert!(da < 1e-2 && close(e0, e1, 1e-3), "{a0} - 37 came out as {a1}");
            }
        }
        // The same turn EnvMap::procedural applies: the preset is evaluated at
        // rotate_y(d, -rotation), so a feature at -Z lands at -X for +90.
        let sunset = EnvMap::procedural(&ibl::EnvPreset::Sunset, 64, 1.0, 90.0);
        let brightest = vec(sunset.brightest_direction());
        assert!(brightest.x < -0.9, "the sunset's sun moved from -Z to -X: {brightest:?}");
    }

    #[test]
    fn env_keeps_a_clamped_copy_and_bakes_a_clean_map() {
        let mut params = HdriParams { intensity_ev: 50.0, ..Default::default() };
        params.sky.clouds.coverage = f32::NAN;
        let env = Env::new(&params);
        assert_eq!(env.params().intensity_ev, 10.0);
        assert_eq!(env.params().sky.clouds.coverage, CloudParams::default().coverage);
        assert_eq!(params.intensity_ev, 50.0, "the caller's params are untouched");
        let map = env.bake(32);
        assert_eq!((map.width, map.height, map.data.len()), (32, 16, 32 * 16));
        assert!(map.data.iter().all(|t| t[3] == 1.0), "alpha is 1 everywhere");
        assert!(map.data.iter().all(|t| t[..3].iter().all(|v| v.is_finite() && *v >= 0.0)));
        // from_fn's size rounding is kept, so tiny requests still make a map.
        let tiny = env.bake(0);
        assert_eq!((tiny.width, tiny.height), (2, 1));
    }

    #[test]
    fn a_bake_cleans_every_channel() {
        // The one layer-free test of the clean-up: every other bake here is
        // already inside 0..1, so only this one fails if clean_radiance or
        // texel stops mapping NaN to 0, a negative to 0 and +inf to
        // MAX_RADIANCE (each channel on its own branch).
        let dirty = |_: Vec3f| vec3f(f32::NAN, -1.0, f32::INFINITY);
        let want = [0.0, 0.0, MAX_RADIANCE, 1.0];
        let serial = bake_with(16, &dirty);
        assert_eq!((serial.width, serial.height), (16, 8));
        assert!(serial.data.iter().all(|t| *t == want), "serial bake: {:?}", serial.data[0]);
        let parallel = bake_par_with(16, &dirty, four_threads);
        assert!(parallel.data.iter().all(|t| *t == want), "parallel bake: {:?}", parallel.data[0]);
        // And the minus-infinity and large-negative ends of the same branches.
        let low = |_: Vec3f| vec3f(f32::NEG_INFINITY, f32::MIN, f32::MAX);
        assert!(bake_with(8, &low).data.iter().all(|t| *t == [0.0, 0.0, MAX_RADIANCE, 1.0]));
    }

    #[test]
    fn env_can_be_shared_across_threads() {
        // bake_par hands &Env to worker threads; later layers must keep it Send + Sync.
        fn shareable<T: Send + Sync>() {}
        shareable::<Env>();
        shareable::<EnvSun>();
    }

    #[test]
    fn bake_evaluates_texel_centres_with_row_zero_at_the_zenith() {
        let map = bake_with(64, &coded);
        let (w, h) = (map.width, map.height);
        assert_eq!((w, h), (64, 32));
        for y in 0..h {
            for x in 0..w {
                // The very arithmetic EnvMap::from_fn uses, so this is exact.
                let d = vec(ibl::equirect_uv_to_dir([(x as f32 + 0.5) / w as f32, (y as f32 + 0.5) / h as f32]));
                let want = coded(d);
                assert_eq!(map.data[y * w + x], [want.x, want.y, want.z, 1.0], "texel ({x}, {y})");
            }
        }
        let at = |x: usize, y: usize| decoded(map.data[y * w + x]);
        assert!(at(20, 0).y > 0.99, "row 0 looks up");
        assert!(at(20, h - 1).y < -0.99, "the last row looks down");
        assert!(at(w / 2, h / 2).z < -0.99, "-Z (north) at the centre column");
        assert!(at(3 * w / 4, h / 2).x > 0.99, "+X at three quarters of the width");
        assert!(at(w / 4, h / 2).x < -0.99, "-X at a quarter of the width");
        assert!(at(0, h / 2).z > 0.99, "+Z at the seam");
    }

    #[test]
    fn the_centre_column_looks_north() {
        // The task's pin: bake(64) puts azimuth 0 (north) in the centre column.
        let uv = ibl::dir_to_equirect_uv(arr(dir_from_az_el(0.0, 0.0)));
        assert!(close(uv[0], 0.5, 1e-6) && close(uv[1], 0.5, 1e-6), "{uv:?}");
        let map = Env::new(&HdriParams::default()).bake(64);
        assert_eq!((map.width, map.height), (64, 32));
        let (x, y) = ((uv[0] * map.width as f32).floor() as usize, (uv[1] * map.height as f32).floor() as usize);
        assert_eq!((x, y), (32, 16), "the centre column of a 64-wide map");
        // With a direction-coded map the texel there faces north, and a
        // bilinear lookup at exactly u = 0.5 splits the two centre columns
        // symmetrically about -Z.
        let coded_map = bake_with(64, &coded);
        let texel = decoded(coded_map.data[y * 64 + x]);
        assert!(texel.z < -0.99 && texel.x.abs() < 0.06 && texel.y.abs() < 0.06, "{texel:?}");
        let north = decoded(coded_map.sample(arr(dir_from_az_el(0.0, 0.0))));
        assert!(close3(north, vec3f(0.0, 0.0, -1.0), 0.01), "{north:?}");
        let east = decoded(coded_map.sample(arr(dir_from_az_el(90.0, 0.0))));
        assert!(close3(east, vec3f(1.0, 0.0, 0.0), 0.01), "{east:?}");
        assert_eq!((uv[0] * 64.0).floor() as usize, 32);
        assert_eq!((ibl::dir_to_equirect_uv(arr(dir_from_az_el(90.0, 0.0)))[0] * 64.0).floor() as usize, 48);
    }

    #[test]
    fn parallel_bake_matches_the_serial_bake() {
        assert_eq!(bake_par_with(48, &coded, four_threads), bake_with(48, &coded));
        let env = Env::new(&HdriParams::default());
        assert_eq!(env.bake_par(32, four_threads), env.bake(32));
        assert_eq!(env.bake_par(0, four_threads), env.bake(0), "the same size rounding");
        // A serial runner (what the UI thread does) gives the same bits too.
        assert_eq!(bake_par_with(16, &coded, |n, f| (0..n).for_each(f)), bake_with(16, &coded));
    }

    #[test]
    fn rows_a_cancelled_run_skipped_stay_black() {
        let full = bake_par_with(16, &coded, |n, f| (0..n).for_each(f));
        let half = bake_par_with(16, &coded, |n, f| (0..n).filter(|i| i % 2 == 0).for_each(f));
        let w = full.width;
        for y in 0..full.height {
            let row = y * w..(y + 1) * w;
            if y % 2 == 0 {
                assert_eq!(half.data[row.clone()], full.data[row]);
            } else {
                assert!(half.data[row].iter().all(|t| *t == [0.0, 0.0, 0.0, 1.0]), "row {y} was skipped");
            }
        }
        // A runner that calls out of range is ignored, not a panic.
        let wild = bake_par_with(8, &coded, |n, f| (0..n + 5).for_each(f));
        assert_eq!(wild, bake_with(8, &coded));
    }

    #[test]
    fn hot_spots_keep_the_energy_of_a_disc_smaller_than_a_texel() {
        // A 4 degree disc of radiance 1 in a 64 x 32 map (5.6 degree texels):
        // sampled at texel centres alone it holds 0 to 2 times its energy.
        let radius = 2.0f32.to_radians();
        for az in [100.0f32, 101.4, 102.8, 104.2] {
            let centre = dir_from_az_el(az, 35.0);
            let disc = move |d: Vec3f| if d.dot(centre) >= radius.cos() { Vec3f::all(1.0) } else { Vec3f::default() };
            let mut map = bake_with(64, &disc);
            refine_hot_spots(&mut map, &disc, &[(centre, radius)]);
            let energy: f32 = map.data.iter().enumerate().map(|(i, t)| {
                let theta = ((i / map.width) as f32 + 0.5) / map.height as f32 * std::f32::consts::PI;
                t[0] * (std::f32::consts::TAU / map.width as f32) * (std::f32::consts::PI / map.height as f32) * theta.sin()
            }).sum();
            let want = std::f32::consts::TAU * (1.0 - radius.cos());
            assert!((energy / want - 1.0).abs() < 0.1, "az {az}: {energy} vs {want}");
        }
        // No hot spot, or a feature the grid resolves, leaves the map as it is.
        let plain = bake_with(64, &coded);
        let mut same = plain.clone();
        refine_hot_spots(&mut same, &coded, &[]);
        refine_hot_spots(&mut same, &coded, &[(vec3f(0.0, 1.0, 0.0), 1.0)]);
        assert_eq!(same, plain);
    }
}
