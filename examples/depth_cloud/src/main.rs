//! Video -> realtime depthmap -> 3D point cloud.
//!
//! From the front the cloud reads as the flat video; drag to orbit and the
//! pixels float apart by depth. Depth comes from a pluggable source, with
//! the model input size and the depth rate as the speed / accuracy knobs.
//!
//! ```text
//! makepad-example-depth-cloud [VIDEO] [--layout full|sbs|tb]
//!     [--depth flat|ground|packed|anything|da3] [--model WEIGHTS]
//! ```
//!
//! * no VIDEO: a built-in animated RGBD clip (exact depth, no model needed);
//! * `--layout sbs|tb` + `--depth packed`: RGBD videos (picture left/top,
//!   grayscale depth right/bottom, white = near) play with zero model cost;
//! * `--model depth_anything_v2_vits.pth` (`--depth anything`, the default
//!   with a model): native Depth-Anything-V2 (Small = realtime tier; also
//!   Distill-Any-Depth and V2-Base/Large checkpoints). `--depth da3 --model
//!   model.safetensors`: Depth-Anything-3 metric-large. Both need
//!   `--features localai` and run on CUDA (NVIDIA, Windows/Linux).
//!   `DEPTH_CLOUD_MODEL` also works.
//! * `--depth ground`: a free "lower is nearer" prior, `flat`: a plane.
//!
//! Already-rendered content uses its own depth instead of a model:
//! * `--depth-video PASS` (`--depth-near-dark` when near is black): a 3D
//!   render's depth / Z / mist pass exported as a second video, frame-locked;
//! * `--scene`: a live GPU-rendered scene whose colour and linear-depth
//!   targets the cloud samples directly (`DepthCloud::set_rendered_source`,
//!   the hook for any in-app renderer): no readback, no model.
//!
//! Controls: drag = orbit, wheel = dolly, "Front view" = back to the video.

pub use makepad_widgets;
pub use makepad_xr;

mod cloud;
mod depth;
mod pipeline;
mod rendered;

use cloud::{DepthCloud, RenderedDepth};
use rendered::RenderedScene;
use depth::{DepthSource, FrameLayout};
use makepad_widgets::*;
use makepad_xr::scene::XrSceneView;
use pipeline::{Pipeline, PipelineSettings, SourceSpec, VideoInput};

app_main!(App);

script_mod! {
    use mod.prelude.widgets.*
    use mod.widgets.*

    let PanelSlider = Slider{width: Fill}

    load_all_resources() do #(App::script_component(vm)){
        ui: Root{
            main_window := Window{
                window.inner_size: vec2(1440, 860)
                body +: {
                    View{
                        width: Fill
                        height: Fill
                        flow: Right

                        live_scene := RenderedScene{}
                        scene := XrSceneView{
                            width: Fill
                            height: Fill
                            clear_color: #x05070a
                            camera.fov_y: 50.0
                            camera.desktop_target: vec3(0.0, 0.0, -1.6)
                            camera.distance: 1.6
                            camera.distance_min: 0.02
                            camera.distance_max: 40.0
                            camera.near: 0.02
                            camera.wheel_zoom_step: 0.06
                            cloud := DepthCloud{}
                        }

                        SolidView{
                            width: 320
                            height: Fill
                            flow: Down
                            padding: 14
                            spacing: 8
                            draw_bg +: {color: #x11161c}

                            H3{text: "Depth cloud"}
                            status := Label{width: Fill text: "starting..."}
                            stats := Label{width: Fill text: ""}
                            Hr{}

                            Label{text: "Depth (speed vs accuracy)"}
                            depth_res := PanelSlider{text: "Model input (px)" min: 112.0 max: 518.0 step: 14.0 default: 308.0 precision: 0}
                            depth_every := PanelSlider{text: "Depth every N frames" min: 1.0 max: 8.0 step: 1.0 default: 1.0 precision: 0}
                            range_smoothing := PanelSlider{text: "Range stability" min: 0.0 max: 0.98 default: 0.85}
                            pixel_smoothing := PanelSlider{text: "Pixel smoothing" min: 0.0 max: 0.9 default: 0.0}
                            Hr{}

                            Label{text: "Point cloud"}
                            depth_amount := PanelSlider{text: "Depth amount (far/near)" min: 1.0 max: 12.0 default: 4.0}
                            points_per_row := PanelSlider{text: "Points per row" min: 64.0 max: 1280.0 step: 16.0 default: 384.0 precision: 0}
                            point_size := PanelSlider{text: "Point size (cells)" min: 0.3 max: 4.0 default: 1.15}
                            edge_cut := PanelSlider{text: "Edge cut (0 = off)" min: 0.0 max: 0.5 default: 0.08}
                            fov := PanelSlider{text: "Field of view (deg)" min: 20.0 max: 100.0 step: 1.0 default: 50.0 precision: 0}
                            Hr{}

                            View{
                                width: Fill
                                height: Fit
                                flow: Right
                                spacing: 8
                                play_pause := Button{text: "Pause"}
                                front_view := Button{text: "Front view"}
                            }
                            Label{width: Fill text: "Drag to orbit, wheel to dolly."}
                        }
                    }
                }
            }
        }
    }
}

enum Args {
    Pipeline(SourceSpec),
    /// The live GPU-rendered scene.
    Scene,
}

impl Args {
    fn parse() -> Result<Self, String> {
        let mut video = None;
        let mut layout = None;
        let mut depth = None;
        let mut model = std::env::var("DEPTH_CLOUD_MODEL").ok();
        let mut depth_video = None;
        let mut near_dark = false;
        let mut args = std::env::args().skip(1);
        while let Some(arg) = args.next() {
            match arg.as_str() {
                "--layout" => layout = args.next(),
                "--depth" => depth = args.next(),
                "--model" => model = args.next(),
                "--depth-video" => depth_video = args.next(),
                "--depth-near-dark" => near_dark = true,
                "--scene" => return Ok(Self::Scene),
                // Platform / Studio flags (`--remote`, `--stdin-loop`, ...).
                other if other.starts_with("--") => {}
                other => video = Some(other.to_string()),
            }
        }
        let input = match video.or_else(|| std::env::var("DEPTH_CLOUD_VIDEO").ok()) {
            Some(path) => VideoInput::File(path),
            None => VideoInput::Synthetic,
        };
        let layout = match (layout.as_deref(), &input) {
            (Some("sbs"), _) | (None, VideoInput::Synthetic) => FrameLayout::SideBySide,
            (Some("tb"), _) => FrameLayout::TopBottom,
            (Some("full"), _) | (None, _) => FrameLayout::Full,
            (Some(other), _) => return Err(format!("unknown --layout {other} (full|sbs|tb)")),
        };
        let default_depth = if depth_video.is_some() {
            "pass"
        } else if layout != FrameLayout::Full {
            "packed"
        } else if model.is_some() {
            "anything"
        } else {
            "ground"
        };
        let depth = match depth.as_deref().unwrap_or(default_depth) {
            "flat" => DepthSource::Flat,
            "ground" => DepthSource::GroundPrior,
            "packed" => DepthSource::Packed(layout),
            "pass" => DepthSource::PassVideo {
                path: depth_video.ok_or("--depth pass needs --depth-video <file>")?,
                near_dark,
            },
            "anything" => Self::native(model, false)?,
            "da3" => Self::native(model, true)?,
            other => {
                return Err(format!(
                    "unknown --depth {other} (flat|ground|packed|pass|anything|da3)"
                ))
            }
        };
        Ok(Self::Pipeline(SourceSpec {
            input,
            layout,
            depth,
        }))
    }

    #[cfg(feature = "localai")]
    fn native(model: Option<String>, da3: bool) -> Result<DepthSource, String> {
        let model_path = model.ok_or("a native depth model needs --model <weights file>")?;
        Ok(if da3 {
            DepthSource::Da3 { model_path }
        } else {
            DepthSource::Anything { model_path }
        })
    }

    #[cfg(not(feature = "localai"))]
    fn native(_model: Option<String>, _da3: bool) -> Result<DepthSource, String> {
        Err("native depth models need a build with --features localai".into())
    }
}

#[derive(Script, ScriptHook)]
pub struct App {
    #[live]
    ui: WidgetRef,
    #[rust]
    pipeline: Option<Pipeline>,
    #[rust]
    settings: PipelineSettings,
    #[rust]
    pump: NextFrame,
    #[rust]
    paused: bool,
    #[rust(4.0)]
    depth_amount: f32,
    #[rust]
    stats_at: f64,
    #[rust]
    scene_mode: bool,
}

impl App {
    fn set_status(&self, cx: &mut Cx, text: &str) {
        self.ui.label(cx, ids!(status)).set_text(cx, text);
    }

    fn with_cloud(&self, cx: &mut Cx, f: impl FnOnce(&mut Cx, &mut DepthCloud)) {
        let cloud = self.ui.widget(cx, ids!(cloud));
        if let Some(mut cloud) = cloud.borrow_mut::<DepthCloud>() {
            f(cx, &mut cloud);
        };
    }

    fn with_scene(&self, cx: &mut Cx, f: impl FnOnce(&mut XrSceneView)) {
        let scene = self.ui.widget(cx, ids!(scene));
        if let Some(mut scene) = scene.borrow_mut::<XrSceneView>() {
            f(&mut scene);
        };
    }

    /// Orbit camera back to the capture camera: the cloud reads as the video.
    fn front_view(&self, cx: &mut Cx) {
        let pivot = DepthCloud::pivot_distance(self.depth_amount);
        self.with_scene(cx, |scene| {
            let camera = scene.camera_mut();
            camera.orbit_yaw = 0.0;
            camera.orbit_pitch = 0.0;
            camera.desktop_target = vec3f(0.0, 0.0, -pivot);
            camera.distance = pivot;
        });
        cx.redraw_all();
    }

    fn pump_frame(&mut self, cx: &mut Cx) {
        if self.scene_mode {
            // Animate: the scene re-renders and the cloud re-reads it.
            if !self.paused {
                cx.redraw_all();
            }
            self.pump = cx.new_next_frame();
            return;
        }
        let Some(pipeline) = self.pipeline.as_mut() else {
            return;
        };
        pipeline.flush_settings();
        if let Some(status) = pipeline.take_status() {
            self.set_status(cx, &status);
        }
        let Some(pipeline) = self.pipeline.as_mut() else {
            return;
        };
        if let Some(frame) = pipeline.take_frame() {
            let stats = frame.stats;
            self.with_cloud(cx, |cx, cloud| {
                cloud.push_frame(cx, frame.width, frame.height, &frame.nv12, frame.depth)
            });
            let now = Cx::monotonic_now();
            if now - self.stats_at > 0.25 {
                self.stats_at = now;
                let text = format!(
                    "depth {}x{}: {:.1} ms\nshown {}  dropped {}  depth runs {}",
                    stats.depth_width,
                    stats.depth_height,
                    stats.depth_ms,
                    stats.shown,
                    stats.dropped,
                    stats.depth_runs
                );
                self.ui.label(cx, ids!(stats)).set_text(cx, &text);
            }
            cx.redraw_all();
        }
        let Some(pipeline) = self.pipeline.as_mut() else {
            return;
        };
        if pipeline.failed() {
            if let Some(status) = pipeline.take_status() {
                self.set_status(cx, &status);
            }
        } else {
            self.pump = cx.new_next_frame();
        }
    }

    /// Live rendered input: the cloud samples the scene's own colour and
    /// linear-depth targets on the GPU.
    fn start_scene(&mut self, cx: &mut Cx) {
        let live = self.ui.widget(cx, ids!(live_scene));
        let targets = live.borrow_mut::<RenderedScene>().map(|mut scene| {
            scene.active = true;
            scene.targets(cx)
        });
        if let Some((color, depth, size)) = targets {
            // Scene depth 2..8 units -> cloud units around 1..4.
            self.with_cloud(cx, |cx, cloud| {
                cloud.set_rendered_source(cx, &color, &depth, size, RenderedDepth::Linear, 0.5)
            });
        }
        self.set_status(cx, "live rendered scene: GPU depth, no model");
        self.front_view(cx);
        self.scene_mode = true;
        self.pump = cx.new_next_frame();
    }

    fn update_settings(&mut self) {
        if let Some(pipeline) = self.pipeline.as_mut() {
            pipeline.set_settings(self.settings);
        }
    }
}

impl MatchEvent for App {
    fn handle_startup(&mut self, cx: &mut Cx) {
        let spec = match Args::parse() {
            Ok(Args::Pipeline(spec)) => spec,
            Ok(Args::Scene) => {
                self.start_scene(cx);
                return;
            }
            Err(err) => {
                log!("depth-cloud: {err}");
                self.set_status(cx, &err);
                return;
            }
        };
        let picture_rect = spec.layout.picture_rect();
        self.with_cloud(cx, |_, cloud| cloud.set_picture_rect(picture_rect));
        self.front_view(cx);
        match Pipeline::start(cx.task_pool(), spec, self.settings) {
            Ok(pipeline) => {
                self.pipeline = Some(pipeline);
                self.pump = cx.new_next_frame();
            }
            Err(err) => self.set_status(cx, &err),
        }
    }

    fn handle_next_frame(&mut self, cx: &mut Cx, e: &NextFrameEvent) {
        if e.set.contains(&self.pump) {
            self.pump_frame(cx);
        }
    }

    fn handle_actions(&mut self, cx: &mut Cx, actions: &Actions) {
        let ui = self.ui.clone();
        let slided = |cx: &mut Cx, id: &[LiveId]| ui.slider(cx, id).slided(actions);

        if let Some(v) = slided(cx, ids!(depth_res)) {
            self.settings.depth_res = v.round() as usize;
            self.update_settings();
        }
        if let Some(v) = slided(cx, ids!(depth_every)) {
            self.settings.depth_every = v.round().max(1.0) as u32;
            self.update_settings();
        }
        if let Some(v) = slided(cx, ids!(range_smoothing)) {
            self.settings.range_smoothing = v as f32;
            self.update_settings();
        }
        if let Some(v) = slided(cx, ids!(pixel_smoothing)) {
            self.settings.pixel_smoothing = v as f32;
            self.update_settings();
        }

        let mut redraw = false;
        if let Some(v) = slided(cx, ids!(depth_amount)) {
            // Keep the orbit pivot at the scene's middle and any dolly offset.
            let old_pivot = DepthCloud::pivot_distance(self.depth_amount);
            self.depth_amount = v as f32;
            let pivot = DepthCloud::pivot_distance(self.depth_amount);
            self.with_cloud(cx, |_, cloud| cloud.depth_amount = v as f32);
            self.with_scene(cx, |scene| {
                let camera = scene.camera_mut();
                camera.desktop_target = vec3f(0.0, 0.0, -pivot);
                camera.distance += pivot - old_pivot;
            });
            redraw = true;
        }
        if let Some(v) = slided(cx, ids!(points_per_row)) {
            self.with_cloud(cx, |_, cloud| cloud.points_per_row = v as f32);
            redraw = true;
        }
        if let Some(v) = slided(cx, ids!(point_size)) {
            self.with_cloud(cx, |_, cloud| cloud.point_size = v as f32);
            redraw = true;
        }
        if let Some(v) = slided(cx, ids!(edge_cut)) {
            self.with_cloud(cx, |_, cloud| cloud.edge_cut = v as f32);
            redraw = true;
        }
        if let Some(v) = slided(cx, ids!(fov)) {
            self.with_scene(cx, |scene| scene.camera_mut().fov_y = v as f32);
            let live = self.ui.widget(cx, ids!(live_scene));
            if let Some(mut live) = live.borrow_mut::<RenderedScene>() {
                live.fov_y = v as f32;
            };
            redraw = true;
        }

        if self.ui.button(cx, ids!(front_view)).clicked(actions) {
            self.front_view(cx);
        }
        if self.ui.button(cx, ids!(play_pause)).clicked(actions) {
            self.paused = !self.paused;
            if let Some(pipeline) = &self.pipeline {
                pipeline.set_paused(self.paused);
            }
            let text = if self.paused { "Play" } else { "Pause" };
            self.ui.button(cx, ids!(play_pause)).set_text(cx, text);
        }
        if redraw {
            cx.redraw_all();
        }
    }
}

impl AppMain for App {
    fn script_mod(vm: &mut ScriptVm) -> ScriptValue {
        crate::makepad_widgets::script_mod(vm);
        makepad_xr::script_mod(vm);
        cloud::script_mod(vm);
        rendered::script_mod(vm);
        self::script_mod(vm)
    }

    fn handle_event(&mut self, cx: &mut Cx, event: &Event) {
        self.match_event(cx, event);
        self.ui.handle_event(cx, event, &mut Scope::empty());
    }
}
