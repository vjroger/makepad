//! Image-based lighting for Splash materials (KERNELS.md §3.3.3). Opt-in:
//! `World::environment.ibl` names an environment; the renderer prepares it
//! off the UI thread (hdri/prepare.rs: render-material's lane texture, a
//! full-resolution dome, and the numbers the sun rig, the exposure and the
//! fog take from the map) and binds the lane texture on the detail slot of
//! the materials compiled with IBL once it lands. Until then, and without
//! an environment, nothing is bound and every stock lane keeps its
//! analytic sky reflection. `Background::Environment` draws the dome as the
//! sky (`DrawEnvBackground`); once it is up the frame skips its analytic
//! dome.
use super::*;
use crate::hdri::prepare::{prepare_ibl_sized, PreparedIbl, ATLAS_WIDTH, DOME_MAX_WIDTH};
use makepad_draw::makepad_platform::thread::{Lane, SubmitError, TaskHandle, TaskPool};
use makepad_render_material::ibl::{EnvMap, EnvPreset};
use makepad_scene::{Background, EnvSun, Environment, IblSource, TextureRef};
use std::collections::HashMap;
use std::sync::Arc;

script_mod! {
    use mod.prelude.widgets_internal.*
    use mod.geom

    // The environment as the background (`Background::Environment`): a
    // fullscreen quad drawn first in the scene pass. Sharp (blur 0, dome
    // up): the full-resolution map in `env_tex`, turned like the IBL and at
    // its intensity. Blurred: the prefiltered atlas in `detail_map` through
    // the IBL material's own lookups (render-material builtin.rs). Both
    // show the map at its own scale x Ibl.intensity (x the background's
    // intensity), exactly what the IBL reflections see: HDR_SKY_GAIN is
    // the analytic sky's and never applies here. HDR lane: that linear
    // radiance, the composite tone maps. Legacy lane: the analytic dome's
    // tone map (sky_dome.rs), metered on the luminance the dome shows. A
    // hash dither against banding, as the analytic dome has.
    mod.draw.DrawEnvBackground = mod.std.set_type_default() do #(DrawEnvBackground::script_shader(vm)){
        alpha_blend: false
        backface_culling: false
        vertex_pos: vertex_position(vec4f)
        fb0: fragment_output(0, vec4f)
        draw_call: uniform_buffer(draw.DrawCallUniforms)
        draw_pass: uniform_buffer(draw.DrawPassUniforms)
        draw_list: uniform_buffer(draw.DrawListUniforms)
        geom: vertex_buffer(geom.QuadVertex, geom.QuadGeom)
        detail_map: texture_2d(float)
        env_tex: texture_2d(float)
        v_ndc: varying(vec2f)
        // render-material's lookups, as the base assigns them. Since plan 1a
        // task A7 the meta row is row 0 and the atlas rows start at 1, both
        // addressed absolutely (normalised by size() only at the sample), so
        // a padded RGBA32F allocation (D3D11, desktop GL: spare rows below
        // the data, size() reports the allocation) cannot hide them; nothing
        // here overrides them.
        mat_ibl_meta: mod.draw.mat_ibl_meta
        mat_ibl_dir: mod.draw.mat_ibl_dir
        mat_ibl_level: mod.draw.mat_ibl_level
        mat_ibl_sky_env: mod.draw.mat_ibl_sky_env
        // The dome, bilinear by hand (float textures are unfiltered): u
        // wraps, v clamps, texel centres. The dome is RGBA32F too, so the
        // same padding applies: the texel coordinates are in the LOGICAL
        // size (bg2.zw) and size() only converts a texel index to the
        // sampler's coordinates (never wraps, clamps or scales by it). The
        // test `the_dome_lookup_never_reads_the_padding_of_a_float_allocation`
        // runs this function's transcription over a NaN-padded allocation.
        env_texel: fn(uv: vec2) -> vec3 {
            let size = self.env_tex.size()
            let w = self.bg2.z
            let h = self.bg2.w
            let px = uv.x * w - 0.5
            let py = clamp(uv.y * h - 0.5, 0.0, h - 1.0)
            let x0 = floor(px)
            let y0 = floor(py)
            let fx = px - x0
            let fy = py - y0
            let xa = (x0 - floor(x0 / w) * w + 0.5) / size.x
            let xb = (x0 + 1.0 - floor((x0 + 1.0) / w) * w + 0.5) / size.x
            let ya = (y0 + 0.5) / size.y
            let yb = (min(y0 + 1.0, h - 1.0) + 0.5) / size.y
            let a = mix(self.env_tex.sample_nearest(vec2(xa, ya)).xyz, self.env_tex.sample_nearest(vec2(xb, ya)).xyz, fx)
            let b = mix(self.env_tex.sample_nearest(vec2(xa, yb)).xyz, self.env_tex.sample_nearest(vec2(xb, yb)).xyz, fx)
            return mix(a, b, fy)
        }
        vertex: fn() {
            let p = self.geom.pos * 2.0 - vec2(1.0, 1.0)
            self.v_ndc = p
            self.vertex_pos = vec4(p.x, p.y, 0.9999, 1.0)
        }
        pixel: fn() {
            // The view ray through this pixel, in world space.
            let v = vec4(self.v_ndc.x * self.bg.z, self.v_ndc.y * self.bg.w, -1.0, 0.0)
            let d = normalize((self.draw_pass.camera_inv * v).xyz)
            // Sharp with the dome up: the map itself, turned by the IBL's
            // rotation (meta texel 9) and at its intensity; else the atlas
            // at the blur (which carries the intensity already).
            let sharp = step(self.bg.x, 0.0) * step(0.5, self.bg2.z)
            var c = vec3(0.0, 0.0, 0.0)
            if sharp > 0.5 {
                let r = self.mat_ibl_dir(d)
                let uv = vec2(fract(0.5 + atan2(r.x, 0.0 - r.z) / 6.2831853), acos(clamp(r.y, -1.0, 1.0)) / 3.14159265)
                c = self.env_texel(uv) * self.mat_ibl_meta(9.0).z
            } else {
                c = self.mat_ibl_sky_env(d, self.bg.x)
            }
            c = max(c * self.bg.y, vec3(0.0, 0.0, 0.0))
            // Legacy lane (bg2.y = 0): Reinhard on the luminance at the
            // metered exposure, normalised, gamma 1/2.2 — the analytic
            // dome's own steps. HDR lane (bg2.y = 1): the linear radiance
            // as it is (no gain: the environment's own scale).
            let hdr = step(0.5, self.bg2.y)
            let lum = dot(c, vec3(0.2126, 0.7152, 0.0722))
            let yt = max(lum * self.bg2.x, 0.0)
            let ldr0 = c * ((yt / (1.0 + yt)) / max(lum, 0.000001))
            let m = max(max(ldr0.x, ldr0.y), max(ldr0.z, 1.0))
            let ldr = pow(ldr0 * (1.0 / m), vec3(0.4545454, 0.4545454, 0.4545454))
            let out = mix(ldr, c, hdr)
            let hash = fract(sin(dot(d.xy + d.zz, vec2(12.9898, 78.233))) * 43758.5453)
            return vec4(out + vec3(1.0, 1.0, 1.0) * ((hash - 0.5) * 0.008), 1.0)
        }
        fragment: fn() {
            self.fb0 = self.pixel()
        }
    }
}

/// The environment background's draw.
#[derive(Script, ScriptHook)]
#[repr(C)]
pub struct DrawEnvBackground {
    #[deref]
    pub draw_vars: DrawVars,
    /// x = blur (0..1), y = intensity over the IBL's, zw = the projection's
    /// inverse x and y scales (view ray from the NDC).
    #[live(vec4(0.0, 1.0, 1.0, 1.0))]
    pub bg: Vec4f,
    /// x = the legacy lane's exposure (metered on the luminance the dome
    /// shows, the map's mean x Ibl.intensity, x 2^exposure_ev; 0 in the
    /// HDR lane), y = the lane (1 = HDR: linear out at the environment's
    /// own scale; 0 = legacy: the shader tone maps), zw = the dome
    /// texture's LOGICAL size (0 = no dome yet: the atlas draws at every
    /// blur).
    #[live(vec4(0.0, 0.0, 0.0, 0.0))]
    pub bg2: Vec4f,
}

/// How a draw takes part in the renderer's environment (`draw_scene_full`,
/// `draw_scene_aux`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum EnvScope {
    /// The scene's own draw: its world's environment is what the renderer
    /// prepares, binds and drops.
    Scene,
    /// A draw of the same frame that is not the scene's (a contact or id
    /// map): its world's environment is not looked at, so a world that
    /// names none cannot drop what the scene's draws prepared.
    Aux,
}

/// What a preparation is for: the source and the sun filled in its
/// lighting copy. Intensity and rotation are not here: they are meta texels
/// the shader reads, rewritten in place when they change.
#[derive(Clone, Copy, Debug, PartialEq)]
struct PrepareKey {
    source: IblSource,
    sun: Option<EnvSun>,
}

/// What the Heavy job hands back.
struct IblJobResult {
    generation: u64,
    key: PrepareKey,
    /// The intensity and rotation the meta row was packed with.
    intensity: f32,
    rotation_deg: f32,
    prepared: PreparedIbl,
    /// A procedural hdri preset's own sun (a registered map has none here;
    /// its sun is the world's `Environment.sun`).
    baked_sun: Option<EnvSun>,
}

#[derive(Default)]
pub(super) struct IblState {
    /// Host-registered HDR environments, by the handle documents name.
    maps: HashMap<TextureRef, Arc<EnvMap>>,
    /// What `texture`'s meta row holds: (source, intensity bits, rotation bits).
    key: Option<(IblSource, u32, u32)>,
    /// The lane texture (meta row 0, then the atlas: plan 1a task A7) and
    /// the full-resolution dome, with the dome's LOGICAL size (the shader
    /// addresses texels in it; a padded allocation's `size()` is larger).
    texture: Option<Texture>,
    dome: Option<Texture>,
    dome_size: (usize, usize),
    /// What the world asked for on the last `resolve_ibl` (None: no
    /// environment, or one that cannot be built). What `environment_pending`
    /// compares the prepared key and the failed key with.
    wanted: Option<PrepareKey>,
    /// What the textures and numbers below were prepared for.
    prepared_for: Option<PrepareKey>,
    /// The bound lighting copy's numbers. `sh` is `Some` exactly while a
    /// preparation is bound (set in `adopt_ibl`, cleared in `drop_ibl`); it
    /// outlives `prepared_for`, which a re-registration clears.
    sh: Option<[[f32; 3]; 9]>,
    mean_luminance: f32,
    horizon_rgb: Vec3f,
    /// The sun filled in the bound lighting copy: the declared one, else a
    /// procedural hdri preset's baked one.
    filled_sun: Option<EnvSun>,
    /// The job in flight, what it prepares, and the generation it carries
    /// (a result from an older generation is stale and dropped).
    job: Option<TaskHandle<Option<IblJobResult>>>,
    job_for: Option<PrepareKey>,
    generation: u64,
    /// A key whose job failed (a worker panic) is not retried every frame.
    failed_for: Option<PrepareKey>,
    queue_full_logged: bool,
    background: Option<Box<DrawEnvBackground>>,
}

/// The built-in environments `IblSource::Procedural` indexes, in order:
/// render-material's seven looks, then the HDRI generator's presets
/// (`hdri::presets::PRESET_NAMES`) as `hdri_<name>`, lowercase with `_` for
/// spaces and dashes (see `hdri_procedural_name`). Indices are stable:
/// Scene3D's `@names` and `material_lab` keep working.
pub const PROCEDURAL_ENVIRONMENTS: &[&str] = &[
    "studio", "softbox", "sunset", "overcast", "night", "neon", "gradient",
    "hdri_clear_noon", "hdri_golden_hour", "hdri_sunset", "hdri_overcast", "hdri_blue_hour", "hdri_moonlit_night", "hdri_starry_night",
    "hdri_three_point", "hdri_top_softbox", "hdri_rim_pair", "hdri_overcast_dome", "hdri_ring_light",
];
/// The prefix of the hdri presets in `PROCEDURAL_ENVIRONMENTS`.
pub const HDRI_PREFIX: &str = "hdri_";
/// A procedural hdri preset bakes at this width (its dome and atlas source).
pub const PROCEDURAL_HDRI_WIDTH: usize = 1024;
/// The synchronous fallback (a closed task pool: wasm without atomics)
/// prepares small so it cannot stall a frame for long.
const SYNC_DOME_WIDTH: usize = 512;
const SYNC_ATLAS_WIDTH: usize = 64;
const SYNC_HDRI_WIDTH: usize = 256;

/// Runs `f(i)` for every `i in 0..n`, however it likes: `fan_out` on the
/// pool inside a job, a plain loop in the synchronous fallback. A bake takes
/// it to spread its rows.
type RowRunner<'a> = &'a dyn Fn(usize, &(dyn Fn(usize) + Sync));

/// The name `IblSource::Procedural(i)` stands for.
pub fn procedural_environment_name(i: u32) -> Option<&'static str> {
    PROCEDURAL_ENVIRONMENTS.get(i as usize).copied()
}

/// An hdri preset name ("Three-point") as it appears in
/// `PROCEDURAL_ENVIRONMENTS` ("hdri_three_point"). `hdri::presets::preset`
/// ignores case and treats '-', '_' and ' ' alike, so the suffix resolves.
/// The table above is written out by hand so its indices read at a glance;
/// this is the rule it follows, and a test holds the two together.
#[cfg(test)]
pub fn hdri_procedural_name(preset: &str) -> String {
    let mut out = String::from(HDRI_PREFIX);
    for ch in preset.chars() {
        out.push(match ch {
            ' ' | '-' => '_',
            c => c.to_ascii_lowercase(),
        });
    }
    out
}

/// The environment a procedural index names, baked or painted, with the
/// sun a bake knows. `run` spreads a bake's rows (the pool inside a job).
fn procedural_env_map(
    i: u32,
    run: RowRunner,
    bake_width: usize,
) -> Option<(EnvMap, Option<EnvSun>)> {
    let name = procedural_environment_name(i)?;
    if let Some(hdri_name) = name.strip_prefix(HDRI_PREFIX) {
        let params = crate::hdri::presets::preset(hdri_name)?;
        Some(crate::hdri::bake_env_map(&params, bake_width, run))
    } else {
        let preset = EnvPreset::by_name(name)?;
        Some((EnvMap::procedural(&preset, 256, 1.0, 0.0), None))
    }
}

/// Build everything for one key: the map (registered, or baked/painted for
/// a procedural index), then `prepare_ibl_sized` with the declared sun
/// filled. Pure apart from `run`, so the job and the synchronous fallback
/// share it.
#[allow(clippy::too_many_arguments)]
fn build_ibl(
    generation: u64,
    key: PrepareKey,
    map: Option<Arc<EnvMap>>,
    intensity: f32,
    rotation_deg: f32,
    run: RowRunner,
    dome_cap: usize,
    atlas_width: usize,
    bake_width: usize,
) -> Option<IblJobResult> {
    let (map, baked_sun) = match key.source {
        IblSource::Hdri(_) => (map?, None),
        IblSource::Procedural(i) => {
            let (m, s) = procedural_env_map(i, run, bake_width)?;
            (Arc::new(m), s)
        }
    };
    // The sun filled in the lighting copy: the one the world declares, else
    // the one a procedural hdri preset baked (a host that names a preset by
    // index has no map to declare a sun from). Either way the directional
    // light carries it (`ibl_sun`, renderer/env_sun.rs) and the SH, the
    // atlas and the meter do not.
    let fill = key.sun.or(baked_sun);
    let prepared = prepare_ibl_sized(&map, fill.as_ref(), intensity, rotation_deg, dome_cap, atlas_width);
    Some(IblJobResult { generation, key, intensity, rotation_deg, prepared, baked_sun })
}

/// The quad's two instance vectors for this frame. Pure, so the lane
/// switch, the meter and the dome gate are testable without a device.
/// `metered_luminance` is the luminance the dome shows: the bound map's
/// whole-sphere mean x `Ibl.intensity` (the caller multiplies).
pub(super) fn env_dome_controls(
    blur: f32,
    intensity: f32,
    projection: &Mat4f,
    hdr_output: bool,
    metered_luminance: f32,
    exposure_ev: f32,
    dome_size: Option<(usize, usize)>,
) -> (Vec4f, Vec4f) {
    let clean = |v: f32, fallback: f32| if v.is_finite() { v } else { fallback };
    let (px, py) = (projection.v[0], projection.v[5]);
    let bg = vec4(
        clean(blur, 0.0).clamp(0.0, 1.0),
        clean(intensity, 0.0).max(0.0),
        1.0 / if px.abs() > 1e-6 { px } else { 1.0 },
        1.0 / if py.abs() > 1e-6 { py } else { 1.0 },
    );
    // Legacy lane: the same reflected-light meter as sky::exposure, on the
    // dome's whole-sphere mean at the environment's scale; the floor keeps
    // a black map finite. The HDR lane has no exposure of its own (the
    // composite's) and no gain: the dome is the map x Ibl.intensity,
    // exactly what the IBL lights with (HDR_SKY_GAIN is the analytic
    // sky's).
    let (exposure, lane) = if hdr_output {
        (0.0, 1.0)
    } else {
        let metered = clean(metered_luminance, 0.0).max(crate::sky::EXPOSURE_LUMINANCE_FLOOR);
        ((crate::sky::EXPOSURE_KEY / metered) * 2.0f32.powf(clean(exposure_ev, 0.0).clamp(-12.0, 12.0)), 0.0)
    };
    let (dw, dh) = dome_size.map(|(w, h)| (w as f32, h as f32)).unwrap_or((0.0, 0.0));
    (bg, vec4(exposure, lane, dw, dh))
}

impl Renderer {
    /// Make an HDR environment available to `IblSource::Hdri(texture)`.
    /// Decoding (and its budget) is the host's: render-material's
    /// `ibl::load_hdr` confines it. Re-registering the handle a world
    /// currently shows prepares the new map in the background; the old
    /// textures stay bound until it lands.
    pub fn register_environment(&mut self, texture: TextureRef, env: Arc<EnvMap>) {
        let names = |k: Option<IblSource>| k == Some(IblSource::Hdri(texture));
        if names(self.ibl.key.map(|k| k.0)) {
            self.ibl.key = None;
        }
        if names(self.ibl.prepared_for.map(|p| p.source)) {
            self.ibl.prepared_for = None;
        }
        if names(self.ibl.failed_for.map(|p| p.source)) {
            self.ibl.failed_for = None;
        }
        if names(self.ibl.job_for.map(|p| p.source)) {
            self.cancel_ibl_job();
        }
        self.ibl.maps.insert(texture, env);
    }

    /// The IBL lane texture for this frame, when the environment asks for one.
    pub fn ibl_texture(&self) -> Option<&Texture> {
        self.ibl.texture.as_ref()
    }

    /// The full-resolution dome (RGBA f32, at most 2048 wide) the
    /// environment background draws at blur 0.
    pub fn ibl_dome_texture(&self) -> Option<&Texture> {
        self.ibl.dome.as_ref()
    }

    /// A preparation for the world's current source has landed.
    pub fn environment_ready(&self) -> bool {
        self.ibl.prepared_for.is_some() && self.ibl.texture.is_some()
    }

    /// ... and its dome texture with it.
    pub fn environment_dome_ready(&self) -> bool {
        self.environment_ready() && self.ibl.dome.is_some()
    }

    /// The world's environment is not prepared yet: a job is in flight (or
    /// finished, and waits for the next draw to adopt it), or the world
    /// wants one that no job was submitted for (the queue was full; the
    /// next draw retries). A host keeps drawing while this holds, since a
    /// draw is what adopts a result and what retries; `items_ready` is
    /// false meanwhile, so a locked-time host does not take the frame
    /// before the environment is in it. A source that cannot be built (an
    /// index past the table, a handle nobody registered) and a failed job
    /// are not pending: nothing will come.
    pub fn environment_pending(&self) -> bool {
        let s = &self.ibl;
        s.job.is_some() || s.wanted.is_some_and(|w| s.prepared_for != Some(w) && s.failed_for != Some(w))
    }

    /// How many preparations were submitted so far (tests: a key change
    /// prepares exactly once).
    pub fn environment_preparations(&self) -> u64 {
        self.ibl.generation
    }

    /// The SH9 irradiance coefficients of the BOUND lighting copy. The
    /// three numeric accessors are gated on the bound numbers (`sh` is set
    /// in `adopt_ibl` and cleared in `drop_ibl`), not on `prepared_for`:
    /// re-registering the bound handle clears `prepared_for`, and the rig,
    /// the exposure and the fog must keep the old map's values until the
    /// new ones land (a day cycle re-registers every quarter hour).
    pub fn ibl_sh9(&self) -> Option<&[[f32; 3]; 9]> {
        self.ibl.sh.as_ref()
    }

    /// Solid-angle mean luminance of the bound lighting copy.
    pub fn ibl_mean_luminance(&self) -> Option<f32> {
        self.ibl.sh.map(|_| self.ibl.mean_luminance)
    }

    /// Linear mean colour of the horizon band of the bound lighting copy.
    pub fn ibl_horizon_rgb(&self) -> Option<Vec3f> {
        self.ibl.sh.map(|_| self.ibl.horizon_rgb)
    }

    /// The sun the bound lighting copy was prepared without: the world's
    /// `Environment.sun` as the job saw it, else the sun a procedural hdri
    /// preset baked. `None` when the map kept all its light (no declared
    /// sun, an engine preset, an overcast bake).
    pub fn ibl_sun(&self) -> Option<EnvSun> {
        self.ibl.sh.and(self.ibl.filled_sun)
    }

    /// `resolve_ibl` for a draw of this scope: the scene's draw resolves its
    /// world's environment, an aux draw leaves the environment as it is.
    pub(super) fn resolve_ibl_for(&mut self, cx: &mut Cx, env: &Environment, scope: EnvScope) {
        match scope {
            EnvScope::Scene => self.resolve_ibl(cx, env),
            EnvScope::Aux => {}
        }
    }

    /// Prepare (in the background, on change) or drop the environment's
    /// textures. Called once per frame before the items pick materials.
    pub(super) fn resolve_ibl(&mut self, cx: &mut Cx, env: &Environment) {
        let Some(ibl) = env.ibl.filter(|i| i.intensity.is_finite() && i.rotation_deg.is_finite()) else {
            self.drop_ibl();
            return;
        };
        let sun = env.sun.filter(|s| s.validate().is_ok());
        let wanted = PrepareKey { source: ibl.source, sun };
        // Set before the submit: a source that cannot be built drops it again.
        self.ibl.wanted = Some(wanted);
        // A job for another key than the world now names is stale: drop it
        // before it can land (else a world that returns to the bound key
        // while that job runs would see the abandoned source's textures
        // flip in, and need a second job to get its own back).
        if self.ibl.job_for.is_some() && self.ibl.job_for != Some(wanted) {
            self.cancel_ibl_job();
        }
        self.adopt_finished_ibl(cx);
        if self.ibl.prepared_for != Some(wanted) && self.ibl.failed_for != Some(wanted) && self.ibl.job_for != Some(wanted) {
            self.submit_ibl(cx, wanted, ibl.intensity, ibl.rotation_deg);
        }
        // Intensity and rotation ride the meta row: rewrite texel 9 when
        // they moved and the lane texture is this source's.
        let key = (ibl.source, ibl.intensity.to_bits(), ibl.rotation_deg.to_bits());
        if self.ibl.texture.is_some() && self.ibl.key != Some(key) && self.ibl.prepared_for.map(|p| p.source) == Some(ibl.source) {
            self.rewrite_ibl_meta(cx, ibl.intensity, ibl.rotation_deg);
            self.ibl.key = Some(key);
        }
    }

    fn cancel_ibl_job(&mut self) {
        if let Some(job) = self.ibl.job.take() {
            // Only a job that has not started stops; a running one finishes
            // and its result is dropped (the handle is not polled again).
            job.cancel();
            job.detach();
        }
        self.ibl.job_for = None;
    }

    /// No environment: nothing bound, nothing kept (the maps and the
    /// background draw stay; they are cheap and the world may ask again).
    fn drop_ibl(&mut self) {
        self.cancel_ibl_job();
        let s = &mut self.ibl;
        s.wanted = None;
        s.key = None;
        s.texture = None;
        s.dome = None;
        s.dome_size = (0, 0);
        s.prepared_for = None;
        s.failed_for = None;
        s.sh = None;
        s.mean_luminance = 0.0;
        s.horizon_rgb = Vec3f::default();
        s.filled_sun = None;
    }

    /// Submit the Heavy job for `wanted`. A full queue retries next frame;
    /// a closed pool (wasm without atomics) prepares now, small.
    fn submit_ibl(&mut self, cx: &mut Cx, wanted: PrepareKey, intensity: f32, rotation_deg: f32) {
        let map = match wanted.source {
            IblSource::Hdri(t) => self.ibl.maps.get(&t).cloned(),
            IblSource::Procedural(_) => None,
        };
        let resolvable = match wanted.source {
            IblSource::Hdri(_) => map.is_some(),
            IblSource::Procedural(i) => procedural_environment_name(i).is_some(),
        };
        if !resolvable {
            // An unregistered handle or an index past the table: nothing to
            // build, nothing bound (as before).
            self.drop_ibl();
            return;
        }
        self.cancel_ibl_job();
        let pool = cx.task_pool();
        match pool.reserve(Lane::Heavy) {
            Ok(slot) => {
                self.ibl.generation = self.ibl.generation.wrapping_add(1);
                self.ibl.queue_full_logged = false;
                let generation = self.ibl.generation;
                let rows: TaskPool = pool.clone();
                self.ibl.job = Some(slot.submit(move || {
                    // Inside a Heavy job: a bake's rows fan out over the pool.
                    let run = move |n: usize, f: &(dyn Fn(usize) + Sync)| rows.fan_out(Lane::Heavy, n, f);
                    build_ibl(generation, wanted, map, intensity, rotation_deg, &run, DOME_MAX_WIDTH, ATLAS_WIDTH, PROCEDURAL_HDRI_WIDTH)
                }));
                self.ibl.job_for = Some(wanted);
            }
            Err(SubmitError::QueueFull) => {
                if !self.ibl.queue_full_logged {
                    log!("ibl: worker queue busy; the environment prepares next frame");
                    self.ibl.queue_full_logged = true;
                }
            }
            Err(SubmitError::Closed) => {
                self.ibl.generation = self.ibl.generation.wrapping_add(1);
                let serial = |n: usize, f: &(dyn Fn(usize) + Sync)| {
                    for i in 0..n {
                        f(i);
                    }
                };
                let result = build_ibl(self.ibl.generation, wanted, map, intensity, rotation_deg, &serial, SYNC_DOME_WIDTH, SYNC_ATLAS_WIDTH, SYNC_HDRI_WIDTH);
                match result {
                    Some(r) => self.adopt_ibl(cx, r),
                    None => self.ibl.failed_for = Some(wanted),
                }
            }
        }
    }

    /// Take a finished job: adopt its result when it is the newest, drop a
    /// stale one, remember a failure so it is not retried every frame.
    fn adopt_finished_ibl(&mut self, cx: &mut Cx) {
        let Some(result) = self.ibl.job.as_mut().and_then(|j| j.try_take()) else { return };
        self.ibl.job = None;
        let job_for = self.ibl.job_for.take();
        match result {
            Ok(Some(r)) if r.generation == self.ibl.generation => self.adopt_ibl(cx, r),
            Ok(Some(_)) => {}
            Ok(None) => self.ibl.failed_for = job_for,
            Err(e) => {
                log!("ibl: environment preparation failed: {e:?}");
                self.ibl.failed_for = job_for;
            }
        }
    }

    /// Upload a preparation: both textures replaced at once, the numbers
    /// beside them, the meta key set to what the job packed (resolve_ibl
    /// rewrites it if the world moved on meanwhile).
    fn adopt_ibl(&mut self, cx: &mut Cx, r: IblJobResult) {
        let p = r.prepared;
        let lane = Texture::new_with_format(cx, TextureFormat::VecRGBAf32 {
            width: p.texture.width,
            height: p.texture.height,
            data: Some(p.texture.data),
            updated: TextureUpdated::Full,
        });
        let dome = Texture::new_with_format(cx, TextureFormat::VecRGBAf32 {
            width: p.dome_width,
            height: p.dome_height,
            data: Some(p.dome),
            updated: TextureUpdated::Full,
        });
        let s = &mut self.ibl;
        s.texture = Some(lane);
        s.dome = Some(dome);
        s.dome_size = (p.dome_width, p.dome_height);
        s.key = Some((r.key.source, r.intensity.to_bits(), r.rotation_deg.to_bits()));
        s.prepared_for = Some(r.key);
        s.failed_for = None;
        s.sh = Some(p.sh);
        s.mean_luminance = p.mean_luminance;
        s.horizon_rgb = p.horizon_rgb;
        s.filled_sun = r.key.sun.or(r.baked_sun);
    }

    /// Rewrite meta texel 9's intensity and rotation in the resident lane
    /// texture, one texel dirty. The meta row is row 0 (render-material's
    /// `pack_ibl` layout since plan 1a task A7), so texel 9 is data[36..40]
    /// whatever the texture's width or height.
    fn rewrite_ibl_meta(&mut self, cx: &mut Cx, intensity: f32, rotation_deg: f32) {
        let Some(texture) = self.ibl.texture.clone() else { return };
        let mut data = texture.take_vec_f32(cx);
        let at = 9 * 4;
        if at + 4 <= data.len() {
            data[at + 2] = intensity.max(0.0);
            data[at + 3] = rotation_deg.to_radians();
        }
        texture.put_back_vec_f32(cx, data, Some(RectUsize::new(PointUsize::new(9, 0), SizeUsize::new(1, 1))));
    }

    /// Draw the environment as the background when the world asks for it
    /// (first in the scene pass: everything after draws over it). Returns
    /// true when the full-resolution dome was drawn: the frame then skips
    /// its analytic or gradient dome. Until the dome lands, the atlas
    /// background draws under the dome exactly as before; an MR stage
    /// never shows an environment (the room supplies the horizon).
    pub(super) fn draw_environment_background(&mut self, cx: &mut Cx3d, env: &Environment, projection: &Mat4f, exposure_ev: f32) -> bool {
        let Background::Environment { blur, intensity } = env.background else { return false };
        if !self.stage.shows_environment() {
            return false;
        }
        let Some(texture) = self.ibl.texture.clone() else { return false };
        if self.ibl.background.is_none() {
            // Held VM (a script-driven draw is mid-apply): try again next
            // frame rather than drawing a sky with no shader.
            self.ibl.background = cx.cx.try_with_vm(|vm| {
                makepad_render_material::builtin::register(vm);
                let draw = vm.bx.heap.value(vm.bx.heap.modules, LiveId::from_str("draw").into(), NoTrap).as_object();
                let have = draw.is_some_and(|d| {
                    let v = vm.bx.heap.value(d, LiveId::from_str("DrawEnvBackground").into(), NoTrap);
                    !v.is_nil() && !v.is_err()
                });
                if !have {
                    script_mod(vm);
                }
                Box::new(DrawEnvBackground::script_new_with_default(vm))
            });
        }
        let dome = self.ibl.dome.clone();
        // The dome's LOGICAL size: the shader addresses texels in it, never
        // in a (possibly padded) allocation's size().
        let dome_size = dome.as_ref().map(|_| self.ibl.dome_size);
        // The legacy meter reads the luminance the dome shows: the map's
        // mean at the environment's scale x Ibl.intensity (C5's env_exposure
        // meters the rig on the same product).
        let ibl_scale = env.ibl.map_or(0.0, |i| if i.intensity.is_finite() { i.intensity.max(0.0) } else { 0.0 });
        let (bg, bg2) = env_dome_controls(blur, intensity, projection, self.hdr_output, self.ibl.mean_luminance * ibl_scale, exposure_ev, dome_size);
        let Some(d) = self.ibl.background.as_mut() else { return false };
        d.bg = bg;
        d.bg2 = bg2;
        d.draw_vars.options.depth_write = false;
        d.draw_vars.set_texture(0, &texture);
        // Slot 1 is env_tex; the lane texture stands in until the dome is
        // up (bg2.z = 0 keeps the shader on the atlas path).
        d.draw_vars.set_texture(1, dome.as_ref().unwrap_or(&texture));
        let drawn = d.draw_vars.can_instance();
        if drawn {
            cx.add_instance(&d.draw_vars);
        }
        drawn && dome.is_some()
    }
}

#[cfg(test)]
impl Renderer {
    /// Sibling test modules (`sun_tests`, C6) hand-feed a landed
    /// preparation's numbers: no pool, no device, no textures. The numeric
    /// accessors are gated on `sh`, so this is all they need.
    pub(super) fn feed_environment_numbers_for_tests(&mut self, sh: [[f32; 3]; 9], mean_luminance: f32, horizon_rgb: Vec3f) {
        self.ibl.sh = Some(sh);
        self.ibl.mean_luminance = mean_luminance;
        self.ibl.horizon_rgb = horizon_rgb;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use makepad_scene::Ibl;

    fn env_of(source: IblSource, intensity: f32, rotation_deg: f32) -> Environment {
        Environment {
            ibl: Some(Ibl { source, intensity, rotation_deg }),
            background: Background::Environment { blur: 0.0, intensity: 1.0 },
            ..Default::default()
        }
    }
    fn grey(width: usize, v: f32) -> Arc<EnvMap> {
        Arc::new(EnvMap::constant(width, [v, v, v]))
    }
    /// Resolve every 2 ms until no job is in flight (a real pool job; a
    /// prefilter at 256 x 128 x 6 takes seconds in debug). The clock and the
    /// sleep are the test's: it runs natively, with a real pool.
    #[allow(clippy::disallowed_types, clippy::disallowed_methods)]
    fn settle(renderer: &mut Renderer, cx: &mut Cx, env: &Environment) {
        let start = std::time::Instant::now();
        loop {
            renderer.resolve_ibl(cx, env);
            if renderer.ibl.job.is_none() {
                return;
            }
            assert!(start.elapsed().as_secs() < 180, "the preparation did not finish within 180 s");
            std::thread::sleep(std::time::Duration::from_millis(2));
        }
    }
    fn size_of(texture: &Texture, cx: &mut Cx) -> (usize, usize) {
        match texture.get_format(cx) {
            TextureFormat::VecRGBAf32 { width, height, .. } => (*width, *height),
            other => panic!("expected an RGBA f32 texture, got {other:?}"),
        }
    }
    /// Texel `i` of the lane texture's meta row, which is row 0 (plan 1a
    /// task A7: `pack_ibl` puts it first, so a padded float allocation on
    /// D3D11 or desktop GL cannot hide it).
    fn meta_texel_at(texture: &Texture, cx: &mut Cx, i: usize) -> [f32; 4] {
        match texture.get_format(cx) {
            TextureFormat::VecRGBAf32 { data: Some(data), .. } => {
                let at = i * 4;
                [data[at], data[at + 1], data[at + 2], data[at + 3]]
            }
            other => panic!("expected resident RGBA f32 data, got {other:?}"),
        }
    }
    fn meta_texel(texture: &Texture, cx: &mut Cx) -> [f32; 4] {
        meta_texel_at(texture, cx, 9)
    }

    /// The whole life of one environment: nothing until a world asks; a
    /// job while it prepares (the lane texture stays None: items keep the
    /// analytic sky, exactly as today with no environment); both textures
    /// once it lands; the same source on later frames means no new work;
    /// no environment drops everything.
    #[test]
    fn a_registered_environment_prepares_in_the_background_and_uploads_both_textures() {
        let mut cx = Cx::new(Box::new(|_, _| {}));
        let mut renderer = Renderer::default();
        renderer.resolve_ibl(&mut cx, &Environment::default());
        assert_eq!(renderer.environment_preparations(), 0);
        assert!(!renderer.environment_pending() && !renderer.environment_ready());

        renderer.register_environment(TextureRef(1), grey(32, 0.25));
        let env = env_of(IblSource::Hdri(TextureRef(1)), 1.0, 0.0);
        renderer.resolve_ibl(&mut cx, &env);
        assert!(renderer.environment_pending(), "a Heavy job was submitted");
        assert!(renderer.ibl_texture().is_none() && !renderer.environment_ready());
        assert_eq!(renderer.environment_preparations(), 1);

        settle(&mut renderer, &mut cx, &env);
        assert!(renderer.environment_ready() && renderer.environment_dome_ready());
        let lane = renderer.ibl_texture().cloned().expect("lane texture");
        let dome = renderer.ibl_dome_texture().cloned().expect("dome texture");
        assert_eq!(size_of(&lane, &mut cx), (256, 1 + 6 * 128), "the meta row (row 0) plus prefilter 256 x 6");
        assert_eq!(size_of(&dome, &mut cx), (32, 16), "the dome is the map's own size");
        assert_eq!(renderer.ibl.dome_size, (32, 16), "the logical size the shader addresses");
        assert!(
            matches!(dome.get_format(&mut cx), TextureFormat::VecRGBAf32 { .. }),
            "the dome never carries mips: a chain would turn the atan2 cut at +Z into a one-pixel line"
        );
        assert_eq!(meta_texel(&lane, &mut cx), [6.0, 128.0, 1.0, 0.0]);
        let sh = renderer.ibl_sh9().expect("sh9");
        let dc = meta_texel_at(&lane, &mut cx, 0);
        assert_eq!([dc[0], dc[1], dc[2]], sh[0], "row 0 holds the SH9 (plan 1a task A7: the meta row first)");
        let e = makepad_render_material::ibl::sh9_irradiance(sh, [0.0, 1.0, 0.0]);
        assert!((e[1] - std::f32::consts::PI * 0.25).abs() < 0.02, "a constant 0.25 map: E = pi L, got {e:?}");
        assert!((renderer.ibl_mean_luminance().unwrap() - 0.25).abs() < 1.0e-3);
        assert!((renderer.ibl_horizon_rgb().unwrap() - vec3f(0.25, 0.25, 0.25)).length() < 1.0e-3);
        assert_eq!(renderer.ibl_sun(), None);

        for _ in 0..3 {
            renderer.resolve_ibl(&mut cx, &env);
        }
        assert_eq!(renderer.environment_preparations(), 1, "the same source on later frames is no new work");
        assert!(!renderer.environment_pending());

        renderer.resolve_ibl(&mut cx, &Environment::default());
        assert!(renderer.ibl_texture().is_none() && renderer.ibl_dome_texture().is_none());
        assert!(!renderer.environment_ready() && renderer.ibl_sh9().is_none());
    }

    /// Intensity and rotation are meta texels the shader reads per pixel:
    /// changing them rewrites texel 9 in place and never re-prefilters.
    #[test]
    fn an_intensity_or_rotation_change_rewrites_the_meta_texel_without_a_new_preparation() {
        let mut cx = Cx::new(Box::new(|_, _| {}));
        let mut renderer = Renderer::default();
        renderer.register_environment(TextureRef(4), grey(16, 0.5));
        let env = env_of(IblSource::Hdri(TextureRef(4)), 1.0, 0.0);
        settle(&mut renderer, &mut cx, &env);
        assert_eq!(renderer.environment_preparations(), 1);
        let lane = renderer.ibl_texture().cloned().unwrap();

        let turned = env_of(IblSource::Hdri(TextureRef(4)), 2.0, 45.0);
        renderer.resolve_ibl(&mut cx, &turned);
        assert!(!renderer.environment_pending());
        assert_eq!(renderer.environment_preparations(), 1, "no job for a meta change");
        let same = renderer.ibl_texture().cloned().unwrap();
        assert_eq!(same.texture_id(), lane.texture_id(), "the texture object is kept");
        let meta = meta_texel(&same, &mut cx);
        assert_eq!((meta[0], meta[1], meta[2]), (6.0, 128.0, 2.0));
        assert!((meta[3] - 45.0f32.to_radians()).abs() < 1.0e-6);
        // A non-finite intensity drops the environment, as before.
        renderer.resolve_ibl(&mut cx, &env_of(IblSource::Hdri(TextureRef(4)), f32::NAN, 0.0));
        assert!(renderer.ibl_texture().is_none());
    }

    /// A source that changes while a job runs: the old job's result is
    /// stale and dropped; the textures that land are the new source's. The
    /// previous environment's textures stay bound until then.
    #[test]
    fn a_source_change_while_preparing_drops_the_stale_result_and_keeps_the_old_textures() {
        let mut cx = Cx::new(Box::new(|_, _| {}));
        let mut renderer = Renderer::default();
        renderer.register_environment(TextureRef(1), grey(32, 0.25));
        renderer.register_environment(TextureRef(2), grey(16, 0.75));
        let a = env_of(IblSource::Hdri(TextureRef(1)), 1.0, 0.0);
        let b = env_of(IblSource::Hdri(TextureRef(2)), 1.0, 0.0);
        settle(&mut renderer, &mut cx, &a);
        let dome_a = renderer.ibl_dome_texture().cloned().unwrap();
        assert_eq!(size_of(&dome_a, &mut cx), (32, 16));

        // Switch to B and, before it lands, back to A and to B again: two
        // more jobs: B, then B again after the world went back to A (the
        // first B job is dropped as stale). One result adopted.
        renderer.resolve_ibl(&mut cx, &b);
        assert!(renderer.environment_pending());
        assert_eq!(renderer.ibl_dome_texture().map(|t| t.texture_id()), Some(dome_a.texture_id()), "A stays up while B prepares");
        assert!(renderer.environment_ready() && renderer.environment_dome_ready(), "A's textures stay bound");
        renderer.resolve_ibl(&mut cx, &a);
        assert!(!renderer.environment_pending(), "back on the bound source: the B job is stale and dropped, nothing new is needed");
        assert_eq!(renderer.ibl_dome_texture().map(|t| t.texture_id()), Some(dome_a.texture_id()));
        renderer.resolve_ibl(&mut cx, &b);
        assert_eq!(renderer.environment_preparations(), 3);
        settle(&mut renderer, &mut cx, &b);
        assert_eq!(renderer.environment_preparations(), 3, "settling submits nothing more");
        let dome_b = renderer.ibl_dome_texture().cloned().unwrap();
        assert_eq!(size_of(&dome_b, &mut cx), (16, 8), "B's dome");
        assert!((renderer.ibl_mean_luminance().unwrap() - 0.75).abs() < 1.0e-3, "B's meter");
        assert_eq!(renderer.ibl.prepared_for.map(|p| p.source), Some(IblSource::Hdri(TextureRef(2))));
    }

    /// The declared sun is part of what was prepared: it is filled in the
    /// lighting copy, so a new sun re-prepares once, and `ibl_sun` reports it.
    #[test]
    fn a_declared_sun_is_removed_from_the_lighting_and_reported() {
        let mut cx = Cx::new(Box::new(|_, _| {}));
        let mut renderer = Renderer::default();
        let sun_dir = crate::hdri::dir_from_az_el(90.0, 45.0);
        let cos_radius = 3.0f32.to_radians().cos();
        let sun = EnvSun { dir: sun_dir, radiance: vec3f(1.0e4, 1.0e4, 1.0e4), cos_radius, facing: 1.0, cos_cover: cos_radius };
        let map = EnvMap::from_fn(128, |d| if crate::hdri::vec(d).dot(sun_dir) >= sun.cos_radius { [1.0e4; 3] } else { [0.5; 3] });
        renderer.register_environment(TextureRef(9), Arc::new(map));
        let mut env = env_of(IblSource::Hdri(TextureRef(9)), 1.0, 0.0);
        settle(&mut renderer, &mut cx, &env);
        assert!(renderer.ibl_mean_luminance().unwrap() > 2.0, "no declared sun: the disc is metered");
        env.sun = Some(sun);
        settle(&mut renderer, &mut cx, &env);
        assert_eq!(renderer.environment_preparations(), 2, "a new sun prepares once more");
        assert!((renderer.ibl_mean_luminance().unwrap() - 0.5).abs() < 0.01, "the cone is filled in the lighting copy");
        assert_eq!(renderer.ibl_sun(), Some(sun));
        settle(&mut renderer, &mut cx, &env);
        assert_eq!(renderer.environment_preparations(), 2);
    }

    /// Re-registering a handle re-prepares it; a handle nobody registered
    /// builds nothing (today's behaviour).
    #[test]
    fn re_registering_a_handle_prepares_again_and_an_unknown_handle_builds_nothing() {
        let mut cx = Cx::new(Box::new(|_, _| {}));
        let mut renderer = Renderer::default();
        let env = env_of(IblSource::Hdri(TextureRef(7)), 1.0, 0.0);
        for _ in 0..3 {
            renderer.resolve_ibl(&mut cx, &env);
        }
        assert_eq!(renderer.environment_preparations(), 0);
        assert!(renderer.ibl_texture().is_none());
        renderer.register_environment(TextureRef(7), grey(16, 0.1));
        settle(&mut renderer, &mut cx, &env);
        assert_eq!(renderer.environment_preparations(), 1);
        renderer.register_environment(TextureRef(7), grey(16, 0.9));
        renderer.resolve_ibl(&mut cx, &env);
        assert!(renderer.environment_pending() && renderer.ibl_texture().is_some(), "the old texture stays while the new map prepares");
        assert!(
            (renderer.ibl_mean_luminance().unwrap() - 0.1).abs() < 1.0e-3 && renderer.ibl_sh9().is_some() && renderer.ibl_horizon_rgb().is_some(),
            "the old numbers stay while the new map prepares"
        );
        settle(&mut renderer, &mut cx, &env);
        assert_eq!(renderer.environment_preparations(), 2);
        assert!((renderer.ibl_mean_luminance().unwrap() - 0.9).abs() < 1.0e-3);
    }

    /// A closed pool (wasm without atomics) prepares on the spot and small:
    /// a 64 wide atlas and a dome capped at 512, adopted by the same call.
    #[test]
    fn a_closed_pool_prepares_synchronously_and_small() {
        use makepad_draw::makepad_platform::thread::ShutdownMode;
        let mut cx = Cx::new(Box::new(|_, _| {}));
        cx.task_pool().close(ShutdownMode::CancelPending);
        let mut renderer = Renderer::default();
        renderer.register_environment(TextureRef(3), grey(1024, 0.5));
        let env = env_of(IblSource::Hdri(TextureRef(3)), 1.5, 0.0);
        renderer.resolve_ibl(&mut cx, &env);
        assert!(!renderer.environment_pending(), "no job: the pool is closed");
        assert_eq!(renderer.environment_preparations(), 1);
        assert!(renderer.environment_dome_ready());
        let lane = renderer.ibl_texture().cloned().unwrap();
        let dome = renderer.ibl_dome_texture().cloned().unwrap();
        assert_eq!(size_of(&lane, &mut cx), (64, 1 + 6 * 32), "the meta row plus prefilter 64 x 6");
        assert_eq!(size_of(&dome, &mut cx), (SYNC_DOME_WIDTH, SYNC_DOME_WIDTH / 2), "a 1024 wide map is resized to the cap");
        assert_eq!(meta_texel(&lane, &mut cx), [6.0, 32.0, 1.5, 0.0], "the shader reads the level height from the meta row");
        assert!((renderer.ibl_mean_luminance().unwrap() - 0.5).abs() < 1.0e-3);
        renderer.resolve_ibl(&mut cx, &env);
        assert_eq!(renderer.environment_preparations(), 1, "and it is not repeated");
    }

    /// A locked-time host takes a frame once `items_ready` says so, and it
    /// records again until then: with the environment preparing off the UI
    /// thread, the first frame has no lane texture, so the host has to wait
    /// for it (a scene lit by the studio environment alone renders black
    /// until it lands). The wait covers the job in flight, a result that
    /// finished but is not adopted yet (adopting is the next draw's work),
    /// and an environment the world wants that no job was submitted for.
    #[test]
    fn items_are_not_ready_while_the_environment_prepares() {
        let mut cx = Cx::new(Box::new(|_, _| {}));
        let mut renderer = Renderer::default();
        renderer.resolve_ibl(&mut cx, &Environment::default());
        assert!(renderer.items_ready(&cx), "no environment, nothing to wait for");

        renderer.register_environment(TextureRef(1), grey(32, 0.25));
        let env = env_of(IblSource::Hdri(TextureRef(1)), 1.0, 0.0);
        renderer.resolve_ibl(&mut cx, &env);
        assert!(renderer.environment_pending());
        assert!(!renderer.items_ready(&cx), "the job is in flight: the frame has no lane texture yet");

        settle(&mut renderer, &mut cx, &env);
        assert!(renderer.environment_ready());
        assert!(!renderer.environment_pending());
        assert!(renderer.items_ready(&cx), "the lane texture is bound");
        // An intensity change is a meta rewrite, not a preparation.
        renderer.resolve_ibl(&mut cx, &env_of(IblSource::Hdri(TextureRef(1)), 2.0, 0.0));
        assert!(renderer.items_ready(&cx));

        // Another source: the old textures stay bound, the new ones are awaited.
        renderer.register_environment(TextureRef(2), grey(16, 0.75));
        let b = env_of(IblSource::Hdri(TextureRef(2)), 1.0, 0.0);
        renderer.resolve_ibl(&mut cx, &b);
        assert!(renderer.environment_ready() && !renderer.items_ready(&cx), "A is bound, B is awaited");
        settle(&mut renderer, &mut cx, &b);
        assert!(renderer.items_ready(&cx));

        // Back to A: B stays bound while A is awaited. Then no environment
        // at all: nothing to wait for, whatever was in flight.
        renderer.resolve_ibl(&mut cx, &env);
        assert!(!renderer.items_ready(&cx));
        renderer.resolve_ibl(&mut cx, &Environment::default());
        assert!(renderer.items_ready(&cx) && !renderer.environment_pending());
    }

    /// A job that finished is still awaited until a draw adopts it: the
    /// host's re-record (reattach) is what picks it up, so the wait must
    /// not end before that draw.
    #[test]
    #[allow(clippy::disallowed_types, clippy::disallowed_methods)]
    fn a_finished_job_is_awaited_until_a_draw_adopts_it() {
        let mut cx = Cx::new(Box::new(|_, _| {}));
        let mut renderer = Renderer::default();
        renderer.register_environment(TextureRef(1), grey(16, 0.25));
        let env = env_of(IblSource::Hdri(TextureRef(1)), 1.0, 0.0);
        renderer.resolve_ibl(&mut cx, &env);
        let start = std::time::Instant::now();
        while !renderer.ibl.job.as_ref().is_some_and(|j| j.is_finished()) {
            assert!(start.elapsed().as_secs() < 180, "the preparation did not finish within 180 s");
            std::thread::sleep(std::time::Duration::from_millis(2));
        }
        assert!(renderer.environment_pending() && !renderer.items_ready(&cx), "finished, not adopted: nothing is bound yet");
        assert!(renderer.ibl_texture().is_none());
        renderer.resolve_ibl(&mut cx, &env);
        assert!(renderer.environment_ready() && renderer.items_ready(&cx));
    }

    /// A full Heavy queue means no job was submitted, but the world still
    /// wants the environment: the host keeps waiting (and drawing, which is
    /// the retry), and the next draw with room submits it.
    #[test]
    fn a_full_queue_is_awaited_and_retried() {
        let mut cx = Cx::new(Box::new(|_, _| {}));
        let mut renderer = Renderer::default();
        renderer.register_environment(TextureRef(1), grey(16, 0.25));
        let env = env_of(IblSource::Hdri(TextureRef(1)), 1.0, 0.0);
        // Hold every Heavy slot (reserved, never submitted).
        let pool = cx.task_pool();
        let mut held = Vec::new();
        while let Ok(slot) = pool.reserve(Lane::Heavy) {
            held.push(slot);
        }
        renderer.resolve_ibl(&mut cx, &env);
        renderer.resolve_ibl(&mut cx, &env);
        assert!(renderer.ibl.job.is_none() && renderer.environment_preparations() == 0, "nothing could be submitted");
        assert!(renderer.environment_pending(), "wanted, not prepared: the host keeps drawing");
        assert!(!renderer.items_ready(&cx));

        drop(held);
        renderer.resolve_ibl(&mut cx, &env);
        assert_eq!(renderer.environment_preparations(), 1, "the retry submitted it");
        settle(&mut renderer, &mut cx, &env);
        assert!(renderer.environment_ready() && !renderer.environment_pending() && renderer.items_ready(&cx));
    }

    /// Nothing to wait for when nothing will come: an index past the table,
    /// a handle nobody registered, a closed pool (prepared on the spot).
    #[test]
    fn items_do_not_wait_for_an_environment_that_cannot_prepare() {
        let mut cx = Cx::new(Box::new(|_, _| {}));
        let mut renderer = Renderer::default();
        renderer.resolve_ibl(&mut cx, &env_of(IblSource::Procedural(999), 1.0, 0.0));
        assert!(!renderer.environment_pending() && renderer.items_ready(&cx));
        renderer.resolve_ibl(&mut cx, &env_of(IblSource::Hdri(TextureRef(77)), 1.0, 0.0));
        assert!(!renderer.environment_pending() && renderer.items_ready(&cx));

        use makepad_draw::makepad_platform::thread::ShutdownMode;
        let mut closed = Cx::new(Box::new(|_, _| {}));
        closed.task_pool().close(ShutdownMode::CancelPending);
        renderer.register_environment(TextureRef(3), grey(16, 0.5));
        renderer.resolve_ibl(&mut closed, &env_of(IblSource::Hdri(TextureRef(3)), 1.0, 0.0));
        assert!(renderer.environment_ready() && !renderer.environment_pending() && renderer.items_ready(&closed));
    }

    /// A frame that records maps the scene reads (motion3d's contact and id
    /// maps, each with a world of its own and so the default environment)
    /// before the scene's own draw: those draws must not drop what the
    /// scene's draw prepares. Resolved like the scene's, the first map of
    /// every recording cancelled the job and cleared the textures, so
    /// nothing was ever adopted and a locked-time host waited for ever.
    #[test]
    #[allow(clippy::disallowed_types, clippy::disallowed_methods)]
    fn aux_draws_do_not_drop_the_scenes_environment() {
        let mut cx = Cx::new(Box::new(|_, _| {}));
        let mut renderer = Renderer::default();
        renderer.register_environment(TextureRef(1), grey(16, 0.25));
        let env = env_of(IblSource::Hdri(TextureRef(1)), 1.0, 0.0);
        let none = Environment::default();
        // One recording: the two maps, then the scene.
        let record = |renderer: &mut Renderer, cx: &mut Cx| {
            renderer.resolve_ibl_for(cx, &none, EnvScope::Aux);
            renderer.resolve_ibl_for(cx, &none, EnvScope::Aux);
            renderer.resolve_ibl_for(cx, &env, EnvScope::Scene);
        };
        record(&mut renderer, &mut cx);
        assert!(renderer.environment_pending() && renderer.ibl.job.is_some(), "the scene's draw submitted the job");
        let start = std::time::Instant::now();
        while renderer.environment_pending() {
            assert!(start.elapsed().as_secs() < 60, "the environment never landed: the maps' draws discard it");
            std::thread::sleep(std::time::Duration::from_millis(2));
            record(&mut renderer, &mut cx);
        }
        assert_eq!(renderer.environment_preparations(), 1, "every recording named the same environment: one preparation");
        assert!(renderer.environment_ready() && renderer.items_ready(&cx));

        // Landed: a map's draw leaves it, and shows what is bound.
        renderer.resolve_ibl_for(&mut cx, &none, EnvScope::Aux);
        assert!(renderer.environment_ready() && renderer.ibl_texture().is_some() && renderer.ibl_sh9().is_some());
        record(&mut renderer, &mut cx);
        assert_eq!(renderer.environment_preparations(), 1);
        assert!(!renderer.environment_pending() && renderer.items_ready(&cx));

        // The scene's own draw with no environment still drops it.
        renderer.resolve_ibl_for(&mut cx, &none, EnvScope::Scene);
        assert!(renderer.ibl_texture().is_none() && !renderer.environment_ready() && !renderer.environment_pending());
    }

    /// A finished preparation that waits for the scene's next draw to adopt
    /// it is not thrown away by a map's draw in between.
    #[test]
    #[allow(clippy::disallowed_types, clippy::disallowed_methods)]
    fn an_aux_draw_keeps_a_finished_preparation_for_the_scenes_next_draw() {
        let mut cx = Cx::new(Box::new(|_, _| {}));
        let mut renderer = Renderer::default();
        renderer.register_environment(TextureRef(1), grey(16, 0.25));
        let env = env_of(IblSource::Hdri(TextureRef(1)), 1.0, 0.0);
        renderer.resolve_ibl_for(&mut cx, &env, EnvScope::Scene);
        let start = std::time::Instant::now();
        while !renderer.ibl.job.as_ref().is_some_and(|j| j.is_finished()) {
            assert!(start.elapsed().as_secs() < 180, "the preparation did not finish within 180 s");
            std::thread::sleep(std::time::Duration::from_millis(2));
        }
        renderer.resolve_ibl_for(&mut cx, &Environment::default(), EnvScope::Aux);
        assert!(renderer.ibl.job.is_some() && renderer.environment_pending(), "the finished job is still there to adopt");
        renderer.resolve_ibl_for(&mut cx, &env, EnvScope::Scene);
        assert!(renderer.environment_ready() && !renderer.environment_pending());
        assert_eq!(renderer.environment_preparations(), 1);
    }

    /// What a sibling test module (C6's fog, C5's rig) feeds in answers the
    /// numeric accessors with no pool, no device and no texture.
    #[test]
    fn hand_fed_numbers_answer_the_accessors_with_nothing_bound() {
        let mut renderer = Renderer::default();
        assert!(renderer.ibl_sh9().is_none() && renderer.ibl_mean_luminance().is_none() && renderer.ibl_horizon_rgb().is_none());
        let mut sh = [[0.0f32; 3]; 9];
        sh[0] = [0.5, 0.6, 0.7];
        renderer.feed_environment_numbers_for_tests(sh, 0.4, vec3f(0.1, 0.2, 0.3));
        assert_eq!(renderer.ibl_sh9(), Some(&sh));
        assert_eq!(renderer.ibl_mean_luminance(), Some(0.4));
        assert_eq!(renderer.ibl_horizon_rgb(), Some(vec3f(0.1, 0.2, 0.3)));
        assert!(renderer.ibl_texture().is_none() && !renderer.environment_ready() && renderer.ibl_sun().is_none());
    }

    /// The dome is sampled through atan2, whose cut sits at +Z (u jumps by
    /// a whole turn there). Only a mip-less texture with a manually wrapped
    /// lookup hides that cut (the pattern of
    /// `sky_lane_tests::the_longitude_cut_jumps_by_whole_periods`); the
    /// format assert in the first test pins "no mips", this pins the cut.
    #[test]
    fn the_equirect_cut_jumps_by_a_whole_turn() {
        use makepad_render_material::ibl::dir_to_equirect_uv;
        let l = dir_to_equirect_uv([1.0e-4, 0.0, 1.0]);
        let r = dir_to_equirect_uv([-1.0e-4, 0.0, 1.0]);
        assert!(((l[0] - r[0]).abs() - 1.0).abs() < 1.0e-3, "u jumps by one period across +Z: only a mip-less, manually wrapped lookup hides it");
        // Away from the cut the mapping is smooth.
        let a = dir_to_equirect_uv([0.0, 0.0, -1.0]);
        let b = dir_to_equirect_uv([0.017, 0.0, -1.0]);
        assert!((a[0] - b[0]).abs() < 0.01);
    }

    /// The engine's seven presets keep their indices; the hdri presets
    /// follow them by name, and every name resolves.
    #[test]
    fn procedural_indices_past_the_engine_presets_name_the_hdri_presets() {
        let hdri = crate::hdri::presets::PRESET_NAMES;
        assert_eq!(PROCEDURAL_ENVIRONMENTS.len(), 7 + hdri.len());
        assert_eq!(&PROCEDURAL_ENVIRONMENTS[..7], &["studio", "softbox", "sunset", "overcast", "night", "neon", "gradient"]);
        for (i, name) in PROCEDURAL_ENVIRONMENTS.iter().enumerate() {
            assert_eq!(procedural_environment_name(i as u32), Some(*name));
            if i < 7 {
                assert!(EnvPreset::by_name(name).is_some(), "{name}");
            } else {
                let hdri_name = name.strip_prefix(HDRI_PREFIX).expect("hdri_ prefix");
                assert!(crate::hdri::presets::preset(hdri_name).is_some(), "{name} -> {hdri_name}");
                assert_eq!(*name, hdri_procedural_name(hdri[i - 7]), "{name} is PRESET_NAMES[{}] lowered", i - 7);
            }
        }
        assert_eq!(procedural_environment_name(PROCEDURAL_ENVIRONMENTS.len() as u32), None);
        assert_eq!(hdri_procedural_name("Three-point"), "hdri_three_point");
        assert_eq!(hdri_procedural_name("Clear noon"), "hdri_clear_noon");
        let unique: std::collections::HashSet<&str> = PROCEDURAL_ENVIRONMENTS.iter().copied().collect();
        assert_eq!(unique.len(), PROCEDURAL_ENVIRONMENTS.len(), "no duplicate names");
    }

    /// A procedural hdri preset bakes on the job (1024 x 512, its own sun)
    /// and prepares like a registered map. Release only: a debug bake of
    /// half a million texels through the atmosphere takes too long.
    #[test]
    #[ignore]
    fn a_procedural_hdri_preset_bakes_on_the_job_and_reports_its_sun() {
        let mut cx = Cx::new(Box::new(|_, _| {}));
        let mut renderer = Renderer::default();
        let env = env_of(IblSource::Procedural(7), 1.0, 0.0);
        settle(&mut renderer, &mut cx, &env);
        assert_eq!(renderer.environment_preparations(), 1);
        let dome = renderer.ibl_dome_texture().cloned().unwrap();
        assert_eq!(size_of(&dome, &mut cx), (PROCEDURAL_HDRI_WIDTH, PROCEDURAL_HDRI_WIDTH / 2));
        let sun = renderer.ibl_sun().expect("clear noon has a sun");
        assert!(sun.dir.y > 0.5, "{sun:?}");
        assert!(renderer.ibl_mean_luminance().unwrap() > 0.0);
        // Nobody declared that sun, so the job filled the preset's own: the
        // SH toward it holds the sky, not the disc (the directional light
        // carries the disc, renderer/env_sun.rs).
        let e = makepad_render_material::ibl::sh9_irradiance(renderer.ibl_sh9().unwrap(), crate::hdri::arr(sun.dir));
        let disc = crate::sky::luminance(sun.irradiance());
        assert!(crate::sky::luminance(vec3f(e[0], e[1], e[2])) < 0.5 * disc, "the baked sun is not in the SH: E {e:?} vs the disc's {disc}");
        // An index past the table builds nothing.
        renderer.resolve_ibl(&mut cx, &env_of(IblSource::Procedural(999), 1.0, 0.0));
        assert!(renderer.ibl_texture().is_none() && !renderer.environment_pending());
    }

    /// The quad's two instance vectors: the lane switch and the meter,
    /// decided on the CPU so the shader is one branch per pixel.
    #[test]
    fn the_background_controls_pick_the_lane_the_meter_and_the_dome() {
        let mut proj = Mat4f::identity();
        proj.v[0] = 2.0;
        proj.v[5] = 4.0;
        // HDR lane: the lane flag 1 (linear out at the map's own scale x
        // Ibl.intensity, which the shader reads from meta texel 9; never
        // HDR_SKY_GAIN, the analytic sky's), no exposure of its own (the
        // composite's), the dome's logical size.
        let (bg, bg2) = env_dome_controls(0.3, 1.5, &proj, true, 0.25, 1.0, Some((2048, 1024)));
        assert_eq!(bg, vec4(0.3, 1.5, 0.5, 0.25));
        assert_eq!(bg2, vec4(0.0, 1.0, 2048.0, 1024.0));
        // Legacy lane: exposure metered on the luminance the dome shows (the
        // caller passes the map's mean x Ibl.intensity), times
        // 2^exposure_ev; lane flag 0 (the shader tone maps).
        let (_, bg2) = env_dome_controls(0.0, 1.0, &proj, false, 0.25, 1.0, None);
        assert!((bg2.x - crate::sky::EXPOSURE_KEY / 0.25 * 2.0).abs() < 1.0e-5, "{bg2:?}");
        assert_eq!((bg2.y, bg2.z, bg2.w), (0.0, 0.0, 0.0), "no dome: the atlas path");
        // A black map meters at the floor, not at infinity; NaN clamps.
        let (_, floor) = env_dome_controls(0.0, 1.0, &proj, false, 0.0, 0.0, None);
        assert!((floor.x - crate::sky::EXPOSURE_KEY / crate::sky::EXPOSURE_LUMINANCE_FLOOR).abs() < 1.0e-4);
        let (bg, bg2) = env_dome_controls(f32::NAN, -2.0, &proj, false, f32::NAN, f32::NAN, Some((4, 2)));
        assert_eq!((bg.x, bg.y), (0.0, 0.0), "blur and intensity are clamped");
        assert!(bg2.x.is_finite() && bg2.x > 0.0);
        // A degenerate projection still gives a finite ray scale.
        let (bg, _) = env_dome_controls(0.0, 1.0, &Mat4f::default(), true, 1.0, 0.0, None);
        assert_eq!((bg.z, bg.w), (1.0, 1.0));
    }

    /// The background is not the sky until the full-resolution dome is up;
    /// the accessor the frame gates on says so, and an MR stage never
    /// shows an environment.
    #[test]
    fn the_dome_replaces_the_sky_only_once_it_is_ready_and_never_in_mr() {
        let mut cx = Cx::new(Box::new(|_, _| {}));
        let mut renderer = Renderer::default();
        assert!(!renderer.environment_dome_ready());
        renderer.register_environment(TextureRef(3), grey(16, 0.4));
        let env = env_of(IblSource::Hdri(TextureRef(3)), 1.0, 0.0);
        renderer.resolve_ibl(&mut cx, &env);
        assert!(!renderer.environment_dome_ready(), "preparing: the analytic dome still draws");
        settle(&mut renderer, &mut cx, &env);
        assert!(renderer.environment_dome_ready());
        assert!(renderer.stage.shows_environment(), "a flat stage shows it");
        renderer.stage.mode = crate::stage::StageMode::MrDiorama;
        assert!(!renderer.stage.shows_environment(), "MR: the room is the horizon");
        // The quad's dome size is the preparation's LOGICAL size (what the
        // shader addresses texels in), not a texture allocation's.
        assert_eq!(renderer.ibl.dome_size, (16, 8));
    }

    /// The shader's `env_texel`, transcribed line for line (Step 3): texel
    /// coordinates in the LOGICAL size (`bg2.zw`), u wrapped, v clamped,
    /// texel centres; only the final sample is normalised by the
    /// allocation (`env_tex.size()`), then read back the way
    /// `sample_nearest` reads a texel.
    fn env_texel_cpu(alloc: &[[f32; 4]], alloc_w: usize, alloc_h: usize, w: f32, h: f32, uv: [f32; 2]) -> [f32; 3] {
        let (sx, sy) = (alloc_w as f32, alloc_h as f32);
        let nearest = |u: f32, v: f32| {
            let x = ((u * sx).floor() as usize).min(alloc_w - 1);
            let y = ((v * sy).floor() as usize).min(alloc_h - 1);
            alloc[y * alloc_w + x]
        };
        let mix = |a: [f32; 4], b: [f32; 4], t: f32| [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t, a[2] + (b[2] - a[2]) * t, 1.0];
        let px = uv[0] * w - 0.5;
        let py = (uv[1] * h - 0.5).clamp(0.0, h - 1.0);
        let (x0, y0) = (px.floor(), py.floor());
        let (fx, fy) = (px - x0, py - y0);
        let xa = (x0 - (x0 / w).floor() * w + 0.5) / sx;
        let xb = (x0 + 1.0 - ((x0 + 1.0) / w).floor() * w + 0.5) / sx;
        let ya = (y0 + 0.5) / sy;
        let yb = ((y0 + 1.0).min(h - 1.0) + 0.5) / sy;
        let a = mix(nearest(xa, ya), nearest(xb, ya), fx);
        let b = mix(nearest(xa, yb), nearest(xb, yb), fx);
        let c = mix(a, b, fy);
        [c[0], c[1], c[2]]
    }

    /// The dome is a VecRGBAf32 texture too, so D3D11 and desktop GL may
    /// allocate it with spare rows (`ceil128(max(3 h, 512))`,
    /// `platform/src/os/windows/d3d11.rs:3345-3352`,
    /// `platform/src/os/linux/opengl.rs:3221-3224`) and `size()` reports the
    /// allocation. The lookup addresses texels in the logical size and
    /// normalises only the sample, so over an allocation whose spare rows
    /// are NaN it reads exactly the map: `EnvMap::sample_uv`, finite, at the
    /// seam, the poles and in between.
    #[test]
    fn the_dome_lookup_never_reads_the_padding_of_a_float_allocation() {
        let map = EnvMap::from_fn(32, |d| [0.5 + 0.5 * d[0], 0.5 + 0.5 * d[1], 0.5 + 0.5 * d[2]]);
        let (w, h) = (map.width, map.height);
        let alloc_h = (3 * h).max(512).div_ceil(128) * 128;
        let mut alloc = map.data.clone();
        alloc.resize(w * alloc_h, [f32::NAN; 4]);
        for uv in [[0.5f32, 0.5f32], [0.001, 0.02], [0.999, 0.98], [0.0, 0.0], [0.75, 1.0], [0.37, 0.61], [0.016, 0.5]] {
            let got = env_texel_cpu(&alloc, w, alloc_h, w as f32, h as f32, uv);
            let want = map.sample_uv(uv);
            for k in 0..3 {
                assert!(got[k].is_finite() && (got[k] - want[k]).abs() < 1.0e-5, "uv {uv:?}: {got:?} vs {want:?}");
            }
        }
        // The same lookup with size() taken as the map's size (no padding:
        // Metal, WebGPU) is the same texels, so one shader serves both.
        let unpadded = env_texel_cpu(&map.data, w, h, w as f32, h as f32, [0.37, 0.61]);
        let want = map.sample_uv([0.37, 0.61]);
        assert!((0..3).all(|k| (unpadded[k] - want[k]).abs() < 1.0e-5));
        // The controls carry the logical size the lookup needs.
        let (_, bg2) = env_dome_controls(0.0, 1.0, &Mat4f::identity(), true, 1.0, 0.0, Some((w, h)));
        assert_eq!((bg2.z, bg2.w), (w as f32, h as f32));
    }
}
