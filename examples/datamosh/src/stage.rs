//! The demo stage: two procedural sources rendered offscreen every source
//! frame, a datamosh engine fed from them, and the result on screen with
//! the two sources as picture-in-picture.
//!
//! - **Grid flight**: a raymarched flight through a beam lattice (after the
//!   classic reprojection shadertoy). It knows its own camera, so besides
//!   its picture it renders EXACT motion vectors: each hit point projected
//!   into the previous frame's camera.
//! - **Shapes**: a 2D animation with pan, zoom, rotation and orbits. It has
//!   no vectors; the engine estimates them.

use makepad_datamosh::{
    Datamosh, DriftMode, MoshParams, TransitionParams, TransitionPhase, VectorFormat, VectorKind,
};
use makepad_widgets::*;

/// Source render size (16:9). The mosh runs at the same size.
pub const SRC_W: usize = 960;
pub const SRC_H: usize = 540;

script_mod! {
    use mod.prelude.widgets_internal.*
    use mod.widgets.*

    set_type_default() do #(DrawGridScene::script_shader(vm)){
        ..mod.draw.DrawQuad
        color_format: @Rgba16F
        vertex: fn() {
            let clipped = self.geom.pos * self.rect_size + self.rect_pos
            self.pos = self.geom.pos
            self.world = vec4(clipped.x, clipped.y, self.draw_depth, 1.0)
            return self.draw_pass.camera_projection * (self.draw_pass.camera_view * self.world)
        }
        // Beams along all three axes through every cell centre.
        map: fn(p: vec3) -> float {
            let q0 = abs(fract(p) - vec3(0.5, 0.5, 0.5))
            let q = min(q0, vec3(q0.z, q0.x, q0.y))
            return max(max(q.x, q.y), q.z) - 0.1
        }
        trace: fn(ro: vec3, rd: vec3) -> float {
            let mut t = 0.1
            let mut i = 0.0
            loop {
                if i > 160.0 { break }
                let m = self.map(ro + rd * t)
                t = t + m
                if m < 0.002 { break }
                if t > 16.0 { break }
                i = i + 1.0
            }
            return t
        }
        // The camera flies down the gap between beams, swaying and
        // looking around.
        cam_pos: fn(t: float) -> vec3 {
            return vec3(0.1 * sin(t * 0.9), 0.1 * cos(1.1 * t), 0.7 * t + 0.25 * sin(t * 0.6))
        }
        cam_dir: fn(t: float) -> vec3 {
            return normalize(vec3(0.35 * sin(t * 0.31), 0.25 * sin(t * 0.23 + 1.0), 1.0))
        }
        cam_up: fn(t: float) -> vec3 {
            return normalize(vec3(0.45 * sin(t * 0.17), 1.0, 0.0))
        }
        palette: fn(h: float) -> vec3 {
            let k = vec3(h, h, h) + vec3(0.0, 0.33, 0.67)
            return vec3(0.5, 0.5, 0.5) + vec3(0.5, 0.5, 0.5) * cos(k * 6.2831853)
        }
        cell_hash: fn(c: vec3) -> float {
            return fract(sin(dot(c, vec3(12.9898, 78.233, 37.719))) * 43758.5453)
        }
        pixel: fn() {
            let aspect = self.aspect
            let s = vec2((self.pos.x * 2.0 - 1.0) * aspect, 1.0 - self.pos.y * 2.0)
            let t = self.time
            let ro = self.cam_pos(t)
            let cw = self.cam_dir(t)
            let cu = normalize(cross(self.cam_up(t), cw))
            let cv = cross(cw, cu)
            let rd = normalize(cu * s.x + cv * s.y + cw * 1.2)
            let dist = self.trace(ro, rd)
            let p = ro + rd * min(dist, 16.0)
            // VECTORS: where this surface point was on screen one source
            // frame ago, as forward motion in uv.
            if self.out_vectors > 0.5 {
                let pt = self.prev_time
                let pro = self.cam_pos(pt)
                let pw = self.cam_dir(pt)
                let pu = normalize(cross(self.cam_up(pt), pw))
                let pv = cross(pw, pu)
                let od = p - pro
                let z = dot(od, pw)
                if z < 0.01 {
                    return vec4(0.0, 0.0, 0.0, 1.0)
                }
                let ps = vec2(dot(od, pu), dot(od, pv)) * (1.2 / z)
                let prev_uv = vec2((ps.x / aspect + 1.0) * 0.5, (1.0 - ps.y) * 0.5)
                let d = self.pos - prev_uv
                return vec4(d.x, d.y, 0.0, 1.0)
            }
            let e = 0.002
            let n = normalize(vec3(
                self.map(p + vec3(e, 0.0, 0.0)) - self.map(p - vec3(e, 0.0, 0.0)),
                self.map(p + vec3(0.0, e, 0.0)) - self.map(p - vec3(0.0, e, 0.0)),
                self.map(p + vec3(0.0, 0.0, e)) - self.map(p - vec3(0.0, 0.0, e))
            ))
            let h = self.cell_hash(floor(p * 2.0))
            let base = self.palette(h * 0.7 + t * 0.03)
            let checker = abs(modf(floor(p.x * 8.0) + floor(p.y * 8.0) + floor(p.z * 8.0), 2.0))
            let light = normalize(vec3(0.4, 0.7, -0.5))
            let diffuse = max(dot(n, light), 0.0) * 0.7 + 0.3
            let surf = base * diffuse * (0.7 + 0.3 * checker)
            let sky = mix(vec3(0.02, 0.02, 0.05), vec3(0.10, 0.06, 0.18), self.pos.y)
            let hit = 1.0 - step(16.0, dist)
            let col = mix(sky, surf, exp(0.0 - dist * 0.16) * hit)
            return vec4(col.x, col.y, col.z, 1.0)
        }
    }

    set_type_default() do #(DrawShapesScene::script_shader(vm)){
        ..mod.draw.DrawQuad
        color_format: @Rgba16F
        vertex: fn() {
            let clipped = self.geom.pos * self.rect_size + self.rect_pos
            self.pos = self.geom.pos
            self.world = vec4(clipped.x, clipped.y, self.draw_depth, 1.0)
            return self.draw_pass.camera_projection * (self.draw_pass.camera_view * self.world)
        }
        pixel: fn() {
            let t = self.time
            let p = vec2((self.pos.x - 0.5) * self.aspect, self.pos.y - 0.5)
            // A panning, breathing floor with texture everywhere, so block
            // matching has something to lock onto.
            let zoom = 1.0 + 0.25 * sin(t * 0.5)
            let bg = p * zoom + vec2(t * 0.12, 0.04 * sin(t * 0.7))
            let checker = abs(modf(floor(bg.x * 10.0) + floor(bg.y * 10.0), 2.0))
            let stripes = 0.5 + 0.5 * sin((bg.x - bg.y) * 60.0)
            let mut col = mix(vec3(0.05, 0.12, 0.20), vec3(0.85, 0.80, 0.70), checker * 0.35 + stripes * 0.08)
            // A spinning pinwheel.
            let r = length(p)
            let ang = atan2(p.y, p.x) - t * 1.3
            let blades = step(0.0, sin(ang * 7.0))
            let wheel = (1.0 - smoothstep(0.26, 0.27, r)) * smoothstep(0.05, 0.06, r)
            let wheel_col = mix(vec3(0.95, 0.30, 0.15), vec3(1.0, 0.85, 0.20), blades) * (0.75 + 0.25 * sin(r * 80.0))
            col = mix(col, wheel_col, wheel)
            // Three balls on Lissajous orbits.
            let mut i = 0.0
            loop {
                if i > 2.5 { break }
                let ph = t * (0.6 + i * 0.25) + i * 2.0944
                let bc = vec2(cos(ph) * 0.55, sin(ph * 1.3) * 0.28)
                let d = length(p - bc)
                let ball = 1.0 - smoothstep(0.075, 0.08, d)
                let k = vec3(i * 0.31, i * 0.31, i * 0.31) + vec3(0.55, 0.85, 0.15)
                let hue = vec3(0.5, 0.5, 0.5) + vec3(0.5, 0.5, 0.5) * cos(k * 6.2831853)
                let rings = 0.6 + 0.4 * sin(d * 120.0 - t * 4.0)
                col = mix(col, hue * rings, ball)
                i = i + 1.0
            }
            return vec4(col.x, col.y, col.z, 1.0)
        }
    }

    set_type_default() do #(DrawTexView::script_shader(vm)){
        ..mod.draw.DrawQuad
        tex: texture_2d(float)
        pixel: fn() {
            let c = self.tex.sample(self.pos)
            let px = self.pos * self.rect_size
            let rim = min(min(px.x, px.y), min(self.rect_size.x - px.x, self.rect_size.y - px.y))
            let frame = (1.0 - step(2.0, rim)) * self.border
            let rgb = mix(vec3(c.x, c.y, c.z), vec3(1.0, 1.0, 1.0), frame)
            return vec4(rgb.x, rgb.y, rgb.z, 1.0)
        }
    }

    mod.widgets.MoshStageBase = #(MoshStage::register_widget(vm))
    mod.widgets.MoshStage = set_type_default() do mod.widgets.MoshStageBase{
        width: Fill
        height: Fill
    }
}

#[derive(Script, ScriptHook)]
#[repr(C)]
pub struct DrawGridScene {
    #[deref]
    draw_super: DrawQuad,
    #[live]
    time: f32,
    #[live]
    prev_time: f32,
    #[live]
    aspect: f32,
    #[live]
    out_vectors: f32,
}

#[derive(Script, ScriptHook)]
#[repr(C)]
pub struct DrawShapesScene {
    #[deref]
    draw_super: DrawQuad,
    #[live]
    time: f32,
    #[live]
    aspect: f32,
}

#[derive(Script, ScriptHook)]
#[repr(C)]
pub struct DrawTexView {
    #[deref]
    draw_super: DrawQuad,
    #[live]
    border: f32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Source {
    Grid,
    Shapes,
}

impl Source {
    pub const ALL: [Source; 2] = [Source::Grid, Source::Shapes];

    pub fn other(self) -> Source {
        match self {
            Source::Grid => Source::Shapes,
            Source::Shapes => Source::Grid,
        }
    }

    pub fn index(self) -> usize {
        Self::ALL.iter().position(|s| *s == self).unwrap_or(0)
    }
}

/// Whose motion moves the picture, and how it is obtained.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MotionChoice {
    /// The flight's own exact vectors (and its residual).
    GridVectors,
    /// The flight's motion estimated from its pictures.
    GridEstimated,
    /// The shapes' motion estimated from their pictures.
    ShapesEstimated,
}

impl MotionChoice {
    pub const ALL: [MotionChoice; 3] = [
        MotionChoice::GridVectors,
        MotionChoice::GridEstimated,
        MotionChoice::ShapesEstimated,
    ];
}

/// Everything the control panel sets.
#[derive(Clone, Copy, Debug)]
pub struct StageSettings {
    /// Transition mode: the picture stays clean and only the transition
    /// moshes. Effect mode: the effect runs continuously.
    pub transition_mode: bool,
    /// Ignore `params.drift_mode` and pick a pattern at random for every
    /// transition (and every I-frame in effect mode).
    pub random_drift: bool,
    pub picture: Source,
    pub motion: MotionChoice,
    /// Off: a keyframe every frame, the clean picture.
    pub mosh_on: bool,
    /// Duplicate the last P-frame instead of taking new motion (bloom).
    pub freeze: bool,
    /// The sources' frame rate: the decoder steps only on a new frame.
    pub source_fps: f64,
    /// Seconds between automatic keyframes; 0 never.
    pub auto_iframe: f64,
    pub params: MoshParams,
    pub transition: TransitionParams,
    pub transition_secs: f64,
}

impl Default for StageSettings {
    fn default() -> Self {
        Self {
            transition_mode: false,
            random_drift: false,
            picture: Source::Shapes,
            motion: MotionChoice::GridVectors,
            mosh_on: true,
            freeze: false,
            source_fps: 30.0,
            auto_iframe: 8.0,
            params: MoshParams {
                entropy: 0.15,
                ..MoshParams::default()
            },
            transition: TransitionParams::default(),
            transition_secs: 3.0,
        }
    }
}

struct SourcePass {
    pass: DrawPass,
    draw_list: DrawList,
    tex: Texture,
}

impl SourcePass {
    fn new(cx: &mut Cx) -> Self {
        Self {
            pass: DrawPass::new(cx),
            draw_list: DrawList::new(cx),
            tex: Texture::new_with_format(
                cx,
                TextureFormat::RenderRGBAf16 {
                    size: TextureSize::Fixed {
                        width: SRC_W,
                        height: SRC_H,
                    },
                    initial: true,
                },
            ),
        }
    }

    fn begin(&mut self, cx: &mut Cx2d) -> Rect {
        let size = dvec2(SRC_W as f64, SRC_H as f64);
        self.pass.set_size(cx, size);
        self.pass.clear_color_textures(cx.cx);
        self.pass.set_color_texture(
            cx,
            &self.tex,
            DrawPassClearColor::ClearWith(vec4(0.0, 0.0, 0.0, 1.0)),
        );
        cx.begin_pass(&self.pass, Some(1.0));
        self.pass.set_size(cx, size);
        self.pass.set_dpi_factor(cx, 1.0);
        self.draw_list.begin_always(cx);
        let pass_size = cx.current_pass_size();
        cx.begin_root_turtle(pass_size, Layout::flow_overlay());
        Rect {
            pos: dvec2(0.0, 0.0),
            size,
        }
    }

    fn end(&mut self, cx: &mut Cx2d) {
        cx.end_pass_sized_turtle();
        self.draw_list.end(cx);
        cx.end_pass(&self.pass);
    }
}

struct Sources {
    grid: SourcePass,
    grid_vectors: SourcePass,
    shapes: SourcePass,
}

struct RunningTransition {
    from: Source,
    to: Source,
    start: f64,
    /// The engine has shown the incoming clip's keyframe.
    landed: bool,
}

#[derive(Script, ScriptHook, Widget)]
pub struct MoshStage {
    #[uid]
    uid: WidgetUid,
    #[walk]
    walk: Walk,
    #[layout]
    layout: Layout,
    #[redraw]
    #[live]
    draw_view: DrawTexView,
    #[live]
    draw_grid: DrawGridScene,
    #[live]
    draw_shapes: DrawShapesScene,
    #[live]
    mosh: Datamosh,
    #[rust]
    sources: Option<Sources>,
    #[rust]
    next_frame: NextFrame,
    #[rust]
    area: Area,
    #[rust]
    settings: StageSettings,
    /// Wall clock of the animation, seconds.
    #[rust]
    time: f64,
    #[rust]
    last_tick: Option<f64>,
    /// The time of the source frame currently rendered, and of the one
    /// before it (the vectors point back to that one).
    #[rust]
    src_time: Option<f64>,
    #[rust]
    prev_src_time: f64,
    #[rust]
    last_iframe: f64,
    #[rust]
    iframe_requested: bool,
    #[rust]
    transition: Option<RunningTransition>,
    /// Set when a transition changed the picture source, for the panel.
    #[rust]
    picture_changed: Option<Source>,
    /// The demo's Random drift: the pattern currently standing in.
    #[rust]
    drift_pick: Option<DriftMode>,
    #[rust]
    rng: u32,
}

impl MoshStage {
    pub fn settings(&self) -> StageSettings {
        self.settings
    }

    pub fn set_settings(&mut self, settings: StageSettings) {
        self.settings = settings;
        self.push_params();
    }

    /// The engine's parameters: the panel's, with the demo's own random
    /// drift pick standing in for the pattern when Random is on.
    fn push_params(&mut self) {
        if self.settings.random_drift && self.drift_pick.is_none() {
            // Just switched to Random: start with a pick (roll comes back
            // here with it set).
            return self.roll_drift();
        }
        let mut params = self.settings.params;
        if let (true, Some(pick)) = (self.settings.random_drift, self.drift_pick) {
            params.drift_mode = pick;
        }
        self.mosh.set_params(params);
    }

    /// A new random drift pattern, different from the last one.
    fn roll_drift(&mut self) {
        if !self.settings.random_drift {
            return;
        }
        let all = DriftMode::ALL;
        let mut x = self.rng ^ (self.time * 1000.0) as u32 ^ 0x2545_f491;
        loop {
            x ^= x << 13;
            x ^= x >> 17;
            x ^= x << 5;
            let pick = all[x as usize % all.len()];
            if Some(pick) != self.drift_pick {
                self.drift_pick = Some(pick);
                break;
            }
        }
        self.rng = x;
        self.push_params();
    }

    pub fn request_iframe(&mut self) {
        self.iframe_requested = true;
    }

    /// Cut to the other source with a datamosh transition.
    pub fn start_transition(&mut self) {
        if self.transition.is_some() {
            return;
        }
        self.roll_drift();
        let from = self.settings.picture;
        self.transition = Some(RunningTransition {
            from,
            to: from.other(),
            start: self.time,
            landed: false,
        });
    }

    /// The picture source a finished transition switched to, once.
    pub fn take_picture_changed(&mut self) -> Option<Source> {
        self.picture_changed.take()
    }

    fn source_tex(sources: &Sources, source: Source) -> Texture {
        match source {
            Source::Grid => sources.grid.tex.clone(),
            Source::Shapes => sources.shapes.tex.clone(),
        }
    }

    /// Render the sources for source time `t` (and the flight's vectors
    /// back to `prev`), declaring them upstream of the engine.
    fn render_sources(&mut self, cx: &mut Cx2d, t: f64, prev: f64) {
        let Some(sources) = self.sources.as_mut() else {
            return;
        };
        let aspect = SRC_W as f32 / SRC_H as f32;
        self.draw_grid.time = t as f32;
        self.draw_grid.prev_time = prev as f32;
        self.draw_grid.aspect = aspect;
        for (pass, vectors) in [(&mut sources.grid, 0.0), (&mut sources.grid_vectors, 1.0)] {
            let r = pass.begin(cx);
            self.draw_grid.out_vectors = vectors;
            self.draw_grid.draw_abs(cx, r);
            pass.end(cx);
            self.mosh.depends_on(&pass.pass);
        }
        self.draw_shapes.time = t as f32;
        self.draw_shapes.aspect = aspect;
        let r = sources.shapes.begin(cx);
        self.draw_shapes.draw_abs(cx, r);
        sources.shapes.end(cx);
        self.mosh.depends_on(&sources.shapes.pass);
    }

    /// Tell the engine what this display frame is: a transition frame, a
    /// clean frame, or a mosh step (only when the sources advanced).
    fn feed(&mut self, advanced: bool) {
        let Some(sources) = self.sources.as_ref() else {
            return;
        };
        let settings = self.settings;
        if let Some(tr) = self.transition.as_mut() {
            let progress = ((self.time - tr.start) / settings.transition_secs.max(0.1)) as f32;
            let from = Self::source_tex(sources, tr.from);
            let to = Self::source_tex(sources, tr.to);
            let phase =
                self.mosh
                    .drive_transition(&from, &to, progress, &settings.transition, advanced);
            if phase == TransitionPhase::After {
                tr.landed = true;
            }
            return;
        }
        if settings.transition_mode {
            // Waiting for the cut: the current clip clean, the motion
            // history already following the clip that will come in.
            let from = Self::source_tex(sources, settings.picture);
            let to = Self::source_tex(sources, settings.picture.other());
            self.mosh
                .drive_transition(&from, &to, 0.0, &settings.transition, advanced);
            return;
        }
        self.mosh.end_transition();
        let picture = Self::source_tex(sources, settings.picture);
        self.mosh.set_picture(Some(&picture));
        if !advanced {
            if self.iframe_requested {
                self.iframe_requested = false;
                self.mosh.keyframe();
                self.roll_drift();
            }
            return;
        }
        match settings.motion {
            // Freezing keeps the motion history out of it: the last field
            // repeats as it is.
            _ if settings.freeze && settings.mosh_on => self.mosh.repeat_step(),
            MotionChoice::GridVectors => self.mosh.push_motion_vectors(
                &sources.grid_vectors.tex,
                VectorFormat::uv(VectorKind::Forward),
                Some(&sources.grid.tex),
            ),
            MotionChoice::GridEstimated => self.mosh.push_motion_frame(&sources.grid.tex),
            MotionChoice::ShapesEstimated => self.mosh.push_motion_frame(&sources.shapes.tex),
        }
        let auto =
            settings.auto_iframe > 0.0 && self.time - self.last_iframe >= settings.auto_iframe;
        if !settings.mosh_on || self.iframe_requested || auto {
            if settings.mosh_on {
                self.roll_drift();
            }
            self.iframe_requested = false;
            self.last_iframe = self.time;
            self.mosh.keyframe();
        }
    }

    /// Fit `aspect` inside `outer`, centred.
    fn fit(outer: Rect, aspect: f64) -> Rect {
        let mut size = outer.size;
        if size.x / size.y.max(1.0) > aspect {
            size.x = size.y * aspect;
        } else {
            size.y = size.x / aspect;
        }
        Rect {
            pos: outer.pos + (outer.size - size) * 0.5,
            size,
        }
    }
}

impl Widget for MoshStage {
    fn handle_event(&mut self, cx: &mut Cx, event: &Event, _scope: &mut Scope) {
        if let Some(ne) = self.next_frame.is_event(event) {
            if let Some(last) = self.last_tick {
                // Clamp the step so a stall does not jump the animation.
                self.time += (ne.time - last).clamp(0.0, 0.1);
            }
            self.last_tick = Some(ne.time);
            let landed = self.transition.as_ref().is_some_and(|tr| tr.landed);
            if landed {
                let to = self
                    .transition
                    .take()
                    .map(|tr| tr.to)
                    .unwrap_or(self.settings.picture);
                self.settings.picture = to;
                self.picture_changed = Some(to);
                self.last_iframe = self.time;
                self.mosh.end_transition();
            }
            self.area.redraw(cx);
        }
    }

    fn draw_walk(&mut self, cx: &mut Cx2d, _scope: &mut Scope, walk: Walk) -> DrawStep {
        cx.begin_turtle(walk, self.layout);
        let rect = cx.turtle().rect();
        if self.sources.is_none() {
            self.sources = Some(Sources {
                grid: SourcePass::new(cx.cx),
                grid_vectors: SourcePass::new(cx.cx),
                shapes: SourcePass::new(cx.cx),
            });
            self.mosh.set_frame_size(SRC_W, SRC_H);
            self.mosh.set_params(self.settings.params);
        }
        // The sources run at their own frame rate; the decoder steps only
        // when they have a new frame.
        let fps = self.settings.source_fps.max(1.0);
        let t = (self.time * fps).floor() / fps;
        let advanced = self.src_time != Some(t);
        if advanced {
            self.prev_src_time = self.src_time.unwrap_or(t);
            self.src_time = Some(t);
            let prev = self.prev_src_time;
            self.render_sources(cx, t, prev);
        }
        self.feed(advanced);
        self.mosh.render(cx);

        let main = Self::fit(rect, SRC_W as f64 / SRC_H as f64);
        if let Some(out) = self.mosh.output_texture() {
            self.draw_view.border = 0.0;
            self.draw_view.draw_vars.set_texture(0, &out);
            self.draw_view.draw_abs(cx, main);
        }
        if let Some(sources) = self.sources.as_ref() {
            // Picture bottom-left, motion bottom-right.
            let thumb = dvec2(main.size.x * 0.22, main.size.y * 0.22);
            let margin = 10.0;
            let y = main.pos.y + main.size.y - thumb.y - margin;
            let picture = match &self.transition {
                Some(tr) => Self::source_tex(sources, tr.to),
                None => Self::source_tex(sources, self.settings.picture),
            };
            let motion = match self.settings.motion {
                MotionChoice::GridVectors | MotionChoice::GridEstimated => sources.grid.tex.clone(),
                MotionChoice::ShapesEstimated => sources.shapes.tex.clone(),
            };
            self.draw_view.border = 1.0;
            for (tex, x) in [
                (picture, main.pos.x + margin),
                (motion, main.pos.x + main.size.x - thumb.x - margin),
            ] {
                self.draw_view.draw_vars.set_texture(0, &tex);
                self.draw_view.draw_abs(
                    cx,
                    Rect {
                        pos: dvec2(x, y),
                        size: thumb,
                    },
                );
            }
        }
        cx.end_turtle_with_area(&mut self.area);
        self.next_frame = cx.new_next_frame();
        DrawStep::done()
    }
}
