use makepad_math::*;

/// Sky + atmosphere, set from script with game.sky({...}). Off by default so
/// existing indoor/abstract games keep their dark backdrop.
#[derive(Clone, Copy)]
pub struct SkyConfig {
    pub top: Vec4f,
    pub horizon: Vec4f,
    pub ground: Vec4f,
    pub ground_bottom: Vec4f,
    /// Exponential distance-fog density toward the horizon color.
    pub fog: f32,
    /// Inputs for the shared analytic daylight/twilight model.
    pub turbidity: f32,
    pub sky_strength: f32,
    pub sun_strength: f32,
    /// User compensation on top of mean-luminance auto exposure.
    pub exposure_ev: f32,
}

impl Default for SkyConfig {
    fn default() -> Self {
        // The sky every app gets. The warm, slightly deeper horizon and the
        // thin haze were tuned on the sandbox's village demo and looked
        // right there — a sunset that reads as air rather than a grey veil —
        // so they belong to the engine, not to one scene: the viewer and
        // every game now open on the same sky.
        Self {
            top: vec4(0.32, 0.58, 0.9, 1.0),
            horizon: vec4(0.66, 0.76, 0.80, 1.0),
            ground: vec4(0.68, 0.75, 0.66, 1.0),
            ground_bottom: vec4(0.3, 0.4, 0.3, 1.0),
            fog: 0.0015,
            turbidity: 2.5,
            sky_strength: 1.0,
            sun_strength: 4.0,
            exposure_ev: 0.0,
        }
    }
}

/// A game's colour grade (`game.grade({...})`), applied where the HDR
/// composite tone maps. Every default reproduces the stock look.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ColorGrade {
    /// Exposure bias in EV on top of the metered (and adapted) exposure.
    pub exposure_ev: f32,
    /// Contrast about mid-grey, in the tone map's log domain (1 = stock).
    pub contrast: f32,
    /// Saturation multiplier (1 = stock).
    pub saturation: f32,
    /// Auto-exposure on; off holds the metered exposure (plus the bias).
    pub auto: bool,
    /// How far auto-exposure may move from the metered exposure, in EV.
    pub auto_min_ev: f32,
    pub auto_max_ev: f32,
    /// Screen-space ambient occlusion strength multiplier (1 = stock): how
    /// dark corners, wall feet and the undersides of things read.
    pub ao: f32,
    /// Tilt-shift (the diorama look): how far the frame above and below a
    /// sharp horizontal band softens into the blurred image, 0 = off.
    pub tilt: f32,
    /// The sharp band's centre (0 = top of the frame, 1 = bottom) and its
    /// half-height, in frame heights.
    pub tilt_center: f32,
    pub tilt_width: f32,
}

impl Default for ColorGrade {
    fn default() -> Self {
        Self {
            exposure_ev: 0.0,
            contrast: 1.0,
            saturation: 1.0,
            auto: true,
            // x0.75 .. x1.6 of the metered exposure.
            auto_min_ev: -0.415,
            auto_max_ev: 0.678,
            ao: 1.0,
            tilt: 0.0,
            tilt_center: 0.6,
            tilt_width: 0.2,
        }
    }
}

/// What `game.sun({...})` asked for. The sim stores only the request — it
/// cannot depend on `makepad_draw`, so the renderer resolves this against
/// the shared `SceneSun` model (see game_render's `resolve_sun`). Lighting
/// is presentation: nothing here is ever read by the step.
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct SunConfig {
    /// Local solar hour 0..24. `None` keeps the default rig.
    pub time_of_day: Option<f32>,
    /// Latitude for the solar model, degrees.
    pub latitude: f32,
    /// Explicit direction toward the sun (y-up), overriding `time_of_day`.
    pub dir: Option<Vec3f>,
    /// Direct-term multiplier.
    pub color: Option<Vec3f>,
    /// Flat ambient, applied to both hemisphere terms.
    pub ambient: Option<Vec3f>,
    /// How much brighter the DISC is than the DOME at full daylight, as a
    /// ratio of luminances. `None` keeps the stock split (roughly 2.6:1),
    /// which is a soft, forgiving key for a stylised world. A viewer that
    /// wants a CLEAR sky asks for around 9: measured clear daylight puts
    /// only about a tenth of the light in the dome, and that is the
    /// difference between shadows that read and shadows that fill in.
    ///
    /// Applied to the DAYLIGHT rig only — the twilight and night ramps run
    /// on top of it untouched, so an evening keeps its own floor.
    pub daylight_balance: Option<f32>,
    /// How dark cast shadows draw, 0..1.
    pub shadow_alpha: Option<f32>,
}


/// The key light an environment carries (the HDRI generator's phase 2):
/// the sun of a sky, the moon, or a studio's key light, as a renderer's one
/// directional light should see it. `dir` points TOWARD the light (unit,
/// y-up) in the environment map's own frame: a generator's own
/// `rotation_deg` is baked in, `Ibl.rotation_deg` is NOT; the renderer
/// turns the sun together with the map.
/// - `cos_radius` is the cosine of the angular radius of the key's cone, the
///   cone `radiance` is averaged over.
/// - `radiance` is the AVERAGE radiance over that cone (linear Rec.709, the
///   environment's own units), with one meaning for every key: the light's
///   whole emission (∫ L dΩ as the map draws it) divided by the cone's solid
///   angle 2π(1 − cos_radius). So `radiance × 2π(1 − cos_radius)`
///   ([`EnvSun::irradiance`]) is the key's whole emission, which a surface
///   facing a small light receives in full and a wide one in part (`facing`).
/// - `facing` is the share of that emission a surface facing the key's centre
///   receives, ∫ L cosθ dΩ / ∫ L dΩ over the key's reach (θ measured from
///   `dir`), in 0..=1. The sun and the moon, discs under a degree wide, have
///   1.0. A studio key's comes from the same integral as its `radiance`, a
///   detected sun's from the texels it gathered; of the built-in studio
///   presets the Overcast dome (a 110 degree disc) has about 0.78, the Top
///   softbox 0.93, the Rim pair 0.95, the Three-point and Ring lights 0.98.
///   The renderer multiplies its directional light's colour by it, so the
///   one light delivers what a surface facing the key receives from the map.
/// - `cos_cover` is the cosine of the half-angle of the smallest cone around
///   `dir` that holds the key's whole reach: a sun's or moon's outer limb
///   (soft edge included), a studio key's reach box with its corners and soft
///   edge, a detected sun's grown cone. It is at most `cos_radius` (the cone
///   `radiance` is averaged over lies inside it). The renderer takes the
///   cone out of the map's own lighting (SH9, specular) because the
///   directional light carries the key's energy, and takes all of it, so no
///   part of the key is lit twice.
///
/// What each producer puts in `radiance`:
/// - a generated sun: its irradiance at the ground spread over the nominal
///   disc's cone, the soft limb past it included;
/// - the moon: the mean over its disc;
/// - a studio key: the light integrated once per map over its own tangent
///   plane (shape, corner, ring, soft edge, hotspot, roll, and the Multiply
///   flags after it), so a thin strip or a ring carries only what it draws;
///   part of a wide rect's or a soft light's emission lies outside the cone
///   (up to about 8 % for the built-in studio presets) and is counted in it,
///   and `cos_cover` is what holds it;
/// - a detected sun (`hdri::detect_sun`): the energy it gathered over the
///   cone's solid angle.
///
/// `cos_radius` is an f32: for the 0.53 degree sun `1 − cos_radius` keeps
/// about three significant digits, so `irradiance()` is good to about 0.3 %
/// there.
///
/// The renderer fills the covering cone in the lighting it derives from the
/// map, because the directional light carries the energy; the drawn dome
/// keeps the disc.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct EnvSun {
    pub dir: Vec3f,
    pub radiance: Vec3f,
    pub cos_radius: f32,
    pub facing: f32,
    pub cos_cover: f32,
}

impl EnvSun {
    /// Every field finite, a direction with length, light that is not
    /// negative, a cosine that names a cone, a share in 0..=1 and a covering
    /// cone at least as wide as the key's own.
    pub fn validate(&self) -> Result<(), &'static str> {
        if !self.dir.is_finite() || self.dir.length() < 1.0e-3 {
            return Err("sun direction must be finite and non-zero");
        }
        if !self.radiance.is_finite() || self.radiance.x < 0.0 || self.radiance.y < 0.0 || self.radiance.z < 0.0 {
            return Err("sun radiance must be finite and non-negative");
        }
        if !self.cos_radius.is_finite() || !(-1.0..=1.0).contains(&self.cos_radius) {
            return Err("sun cos_radius must lie in -1..=1");
        }
        if !self.facing.is_finite() || !(0.0..=1.0).contains(&self.facing) {
            return Err("sun facing must lie in 0..=1");
        }
        if !self.cos_cover.is_finite() || !(-1.0..=self.cos_radius).contains(&self.cos_cover) {
            return Err("sun cos_cover must lie in -1..=cos_radius");
        }
        Ok(())
    }

    /// The key's whole emission: radiance x the cone's solid angle
    /// 2π(1 - cos r) (the irradiance on a surface facing a small light;
    /// a wide one delivers `facing` of it, which is not in this).
    pub fn irradiance(&self) -> Vec3f {
        self.radiance * (std::f32::consts::TAU * (1.0 - self.cos_radius.clamp(-1.0, 1.0)))
    }
}

/// What surrounds a world: its background, image-based lighting and fog.
/// `Environment::default()` asks for nothing, and a host that leaves it so
/// keeps its own sky (`World::sky`) and analytic reflections.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Environment {
    pub background: Background,
    /// `Some` switches on image-based lighting (prefiltered specular and
    /// SH9 diffuse). `None` keeps the analytic sky reflection.
    pub ibl: Option<Ibl>,
    pub fog: Fog,
    /// The environment's key light, when its producer knows one (a baked
    /// sky's sun, a studio's key, a detected sun in a loaded HDRI). The
    /// renderer's sun rig takes it unless the host's sun config or a
    /// `Light::Sun` say otherwise; `None` keeps today's rig. Its `dir` is
    /// in the map's own frame: a generator's own `rotation_deg` is baked
    /// in, `Ibl.rotation_deg` is NOT; the renderer turns the sun together
    /// with the map.
    pub sun: Option<EnvSun>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum Background {
    /// The host's sky (`World::sky`, or none).
    #[default]
    Host,
    Color(Vec4f),
    /// Vertical gradient, zenith to nadir.
    Gradient { top: Vec4f, bottom: Vec4f },
    /// The IBL source image itself, blurred by `blur` (0..1, the
    /// prefilter's roughness) and scaled by `intensity`.
    Environment { blur: f32, intensity: f32 },
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Ibl {
    pub source: IblSource,
    pub intensity: f32,
    /// Rotation about +Y, degrees.
    pub rotation_deg: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum IblSource {
    /// An equirectangular HDR image (a resident texture handle).
    Hdri(crate::item::TextureRef),
    /// One of the built-in procedural environments, by index.
    Procedural(u32),
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum Fog {
    /// The host's fog (`SkyConfig::fog`), or none.
    #[default]
    Host,
    None,
    Linear { color: Vec3f, start: f32, end: f32 },
    Exp2 { color: Vec3f, density: f32 },
    /// Exponential in distance, falling off with height above `base`.
    Height { color: Vec3f, density: f32, base: f32, falloff: f32 },
}

impl Environment {
    pub fn validate(&self) -> Result<(), &'static str> {
        let f = |v: f32| v.is_finite();
        if let Some(ibl) = &self.ibl {
            if !f(ibl.intensity) || ibl.intensity < 0.0 || !f(ibl.rotation_deg) {
                return Err("ibl intensity must be non-negative and its rotation finite");
            }
        }
        if let Some(sun) = &self.sun {
            sun.validate()?;
        }
        match self.fog {
            Fog::Linear { start, end, .. } if !(f(start) && f(end) && start < end) => Err("linear fog needs start < end"),
            Fog::Exp2 { density, .. } if !(f(density) && density >= 0.0) => Err("fog density must be non-negative"),
            Fog::Height { density, falloff, base, .. } if !(f(density) && density >= 0.0 && f(falloff) && falloff >= 0.0 && f(base)) => {
                Err("height fog needs a non-negative density and falloff")
            }
            _ => Ok(()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sun() -> EnvSun {
        EnvSun { dir: vec3f(0.0, 0.8, -0.6), radiance: vec3f(5.0e4, 4.8e4, 4.5e4), cos_radius: 0.99996, facing: 1.0, cos_cover: 0.99995 }
    }

    /// The field is additive: an environment that asks for nothing still
    /// has no sun, validates, and stays a plain `Copy` value (hosts pass
    /// it by value every frame).
    #[test]
    fn an_environment_asks_for_no_sun_by_default_and_stays_copy() {
        let e = Environment::default();
        assert_eq!(e.sun, None);
        assert!(e.validate().is_ok());
        let copy: Environment = e;
        assert_eq!(copy, e);
        let lit = Environment { sun: Some(sun()), ..Environment::default() };
        assert!(lit.validate().is_ok());
        assert_ne!(lit, e);
    }

    /// A sun that cannot light anything is refused at the frame check, the
    /// way a negative IBL intensity is: NaN, a zero direction, negative
    /// light, a cosine outside [-1, 1], a share outside 0..=1, or a covering
    /// cone narrower than the cone the light is averaged over.
    #[test]
    fn a_sun_must_be_finite_with_a_cone_and_non_negative_light() {
        assert!(sun().validate().is_ok());
        let bad = [
            EnvSun { dir: vec3f(f32::NAN, 1.0, 0.0), ..sun() },
            EnvSun { dir: vec3f(0.0, 0.0, 0.0), ..sun() },
            EnvSun { radiance: vec3f(-1.0, 1.0, 1.0), ..sun() },
            EnvSun { radiance: vec3f(1.0, f32::INFINITY, 1.0), ..sun() },
            EnvSun { cos_radius: 1.5, cos_cover: -1.0, ..sun() },
            EnvSun { cos_radius: f32::NAN, ..sun() },
            EnvSun { facing: -0.1, ..sun() },
            EnvSun { facing: 1.5, ..sun() },
            EnvSun { facing: f32::NAN, ..sun() },
            EnvSun { facing: f32::INFINITY, ..sun() },
            // The covering cone holds the cone `radiance` is averaged over,
            // so its cosine is the smaller one, and a cosine names a cone.
            EnvSun { cos_cover: 0.99997, ..sun() },
            EnvSun { cos_cover: -1.5, ..sun() },
            EnvSun { cos_cover: f32::NAN, ..sun() },
            EnvSun { cos_cover: f32::NEG_INFINITY, ..sun() },
        ];
        for s in bad {
            assert!(s.validate().is_err(), "{s:?}");
            let e = Environment { sun: Some(s), ..Environment::default() };
            assert!(e.validate().is_err(), "the environment check sees the sun: {s:?}");
        }
    }

    /// The ends of every range are valid: a key that is a point, one that
    /// lights a surface edge-on, a cone that covers the whole sphere.
    #[test]
    fn the_ends_of_a_suns_ranges_are_valid() {
        let ok = [
            EnvSun { facing: 0.0, ..sun() },
            EnvSun { facing: 1.0, ..sun() },
            EnvSun { cos_cover: 0.99996, ..sun() },
            EnvSun { cos_cover: -1.0, ..sun() },
            EnvSun { cos_radius: 1.0, cos_cover: 1.0, ..sun() },
            EnvSun { cos_radius: -1.0, cos_cover: -1.0, ..sun() },
            EnvSun { radiance: vec3f(0.0, 0.0, 0.0), ..sun() },
        ];
        for s in ok {
            assert!(s.validate().is_ok(), "{s:?}");
        }
    }

    /// radiance x the cone's solid angle: a hemisphere-wide "sun" of
    /// radiance 1 delivers 2π, a zero-width one nothing. The cosine across
    /// a wide light is `facing`'s, not the irradiance's.
    #[test]
    fn a_sun_irradiance_is_its_radiance_over_its_cone() {
        let wide = EnvSun { dir: vec3f(0.0, 1.0, 0.0), radiance: vec3f(1.0, 2.0, 3.0), cos_radius: 0.0, facing: 1.0, cos_cover: 0.0 };
        let e = wide.irradiance();
        let tau = std::f32::consts::TAU;
        assert!((e.x - tau).abs() < 1.0e-4 && (e.y - 2.0 * tau).abs() < 1.0e-4 && (e.z - 3.0 * tau).abs() < 1.0e-4, "{e:?}");
        let point = EnvSun { cos_radius: 1.0, cos_cover: 1.0, ..wide };
        assert_eq!(point.irradiance(), vec3f(0.0, 0.0, 0.0));
        let tilted = EnvSun { facing: 0.78, cos_cover: -0.5, ..wide };
        assert_eq!(tilted.irradiance(), wide.irradiance(), "facing and the covering cone are not in it");
    }
}
