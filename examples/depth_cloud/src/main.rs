//! Video -> realtime depthmap -> 3D point cloud, on the reusable
//! `makepad-depth-cloud` widget.
//!
//! From the front the cloud reads as the flat video; drag to orbit and the
//! pixels float apart by depth. "Open video..." / "Open model..." (or a drop
//! on the window) pick the inputs; the sliders trade depth speed against
//! accuracy, crop a depth band, and shape the cloud with mouse effects.
//!
//! ```text
//! makepad-example-depth-cloud [VIDEO] [--layout full|sbs|tb]
//!     [--depth flat|ground|packed|pass|anything|da3] [--model WEIGHTS]
//!     [--depth-video PASS [--depth-near-dark]] [--scene]
//! ```
//!
//! * no VIDEO: a built-in animated RGBD clip (exact depth, no model needed);
//! * `--model depth_anything_v2_vits.pth` or `video_depth_anything_vits.pth`:
//!   native Depth-Anything-V2 / Video-Depth-Anything (temporal, no flicker);
//!   `--depth da3 --model model.safetensors`: DA3 metric-large. Models need
//!   `--features localai` and CUDA (NVIDIA, Windows/Linux);
//! * `--layout sbs|tb`: RGBD videos with the depth packed in the frame;
//! * `--depth-video PASS`: a render's depth pass as a second video;
//! * `--scene`: a live GPU-rendered scene sampled directly, no model.

pub use makepad_widgets;
pub use makepad_xr;

mod picker;
mod rendered;

use makepad_depth_cloud::{
    CloudEffect, DepthCloud, DepthCloudAction, DepthSource, FrameLayout, PipelineSettings,
    RenderedDepth, SourceSpec, VideoInput,
};
use makepad_widgets::*;
use makepad_xr::scene::XrSceneView;
use rendered::RenderedScene;

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
                            draw_bg +: {color: #x11161c}

                            // Always visible: title, status and the buttons.
                            View{
                                width: Fill
                                height: Fit
                                flow: Down
                                padding: Inset{left: 14 right: 14 top: 14 bottom: 6}
                                spacing: 6
                                H3{text: "Depth cloud"}
                                status := Label{width: Fill text: "starting..."}
                                stats := Label{width: Fill text: ""}
                                View{
                                    width: Fill
                                    height: Fit
                                    flow: Right
                                    spacing: 8
                                    open_video := Button{text: "Open video..."}
                                    open_model := Button{text: "Open model..."}
                                }
                                View{
                                    width: Fill
                                    height: Fit
                                    flow: Right
                                    spacing: 8
                                    play_pause := Button{text: "Pause"}
                                    front_view := Button{text: "Front view"}
                                }
                                Label{width: Fill text: "Or drop a video / model file on the window. Drag to orbit, wheel to dolly."}
                                Hr{}
                            }

                            // The sliders scroll when the window is short.
                            ScrollYView{
                                width: Fill
                                height: Fill
                                flow: Down
                                padding: Inset{left: 14 right: 14 bottom: 14}
                                spacing: 8

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
                                edge_cut := PanelSlider{text: "Edge cut (0 = off)" min: 0.0 max: 0.5 default: 0.0}
                                fov := PanelSlider{text: "Field of view (deg)" min: 20.0 max: 100.0 step: 1.0 default: 50.0 precision: 0}
                                Hr{}

                                Label{text: "Depth crop (0 = nearest, 1 = farthest)"}
                                crop_near := PanelSlider{text: "Crop near" min: 0.0 max: 1.0 default: 0.0}
                                crop_far := PanelSlider{text: "Crop far" min: 0.0 max: 1.0 default: 1.0}
                                Hr{}

                                Label{text: "Mouse effect (a ray from the camera through the cursor)"}
                                effect := DropDown{labels: ["Off" "Attract" "Repel" "Swirl" "Ripple"]}
                                effect_strength := PanelSlider{text: "Strength" min: 0.0 max: 2.0 default: 0.6}
                                effect_radius := PanelSlider{text: "Radius" min: 0.05 max: 2.0 default: 0.35}
                                momentum := CheckBox{text: "Momentum (fly, spring back, settle)" active: true}
                                spring := PanelSlider{text: "Spring (pull home)" min: 0.0 max: 200.0 default: 40.0}
                                damping := PanelSlider{text: "Damping" min: 0.0 max: 30.0 default: 5.0}
                            }
                        }
                    }
                }
            }
        }
    }
}

enum Args {
    Play(SourceSpec),
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
            "anything" => native(model, false)?,
            "da3" => native(model, true)?,
            other => {
                return Err(format!(
                    "unknown --depth {other} (flat|ground|packed|pass|anything|da3)"
                ))
            }
        };
        Ok(Self::Play(SourceSpec {
            input,
            layout,
            depth,
        }))
    }
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

#[derive(Script, ScriptHook)]
pub struct App {
    #[live]
    ui: WidgetRef,
    #[rust]
    settings: PipelineSettings,
    #[rust]
    pump: NextFrame,
    #[rust]
    paused: bool,
    #[rust(4.0)]
    depth_amount: f32,
    /// Crop band as fractions of the near..far range.
    #[rust((0.0, 1.0))]
    crop: (f32, f32),
    #[rust]
    scene_mode: bool,
    /// Chosen through the buttons or a drop; `None` = built-in test clip.
    #[rust]
    video_path: Option<String>,
    #[rust]
    model_path: Option<String>,
}

impl App {
    fn set_status(&self, cx: &mut Cx, text: &str) {
        self.ui.label(cx, ids!(status)).set_text(cx, text);
    }

    fn with_cloud<R>(&self, cx: &mut Cx, f: impl FnOnce(&mut Cx, &mut DepthCloud) -> R) -> Option<R> {
        let cloud = self.ui.widget(cx, ids!(cloud));
        let mut cloud = cloud.borrow_mut::<DepthCloud>()?;
        Some(f(cx, &mut cloud))
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

    /// The crop band of the source's own depth: 0 = nearest, 1 = farthest.
    fn apply_crop(&self, cx: &mut Cx) {
        let crop = self.crop;
        self.with_cloud(cx, |_, cloud| cloud.crop = crop);
        cx.redraw_all();
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
                cloud.set_rendered_source(
                    cx,
                    &color,
                    &depth,
                    size,
                    RenderedDepth::Linear,
                    0.5,
                    (2.0, 9.0),
                )
            });
        }
        self.set_status(cx, "live rendered scene: GPU depth, no model");
        self.front_view(cx);
        self.scene_mode = true;
        self.pump = cx.new_next_frame();
    }

    fn play(&mut self, cx: &mut Cx, spec: SourceSpec) {
        if self.scene_mode {
            self.scene_mode = false;
            let live = self.ui.widget(cx, ids!(live_scene));
            if let Some(mut live) = live.borrow_mut::<RenderedScene>() {
                live.active = false;
            };
        }
        if self.paused {
            self.paused = false;
            self.ui.button(cx, ids!(play_pause)).set_text(cx, "Pause");
        }
        let settings = self.settings;
        let result = self.with_cloud(cx, |cx, cloud| {
            cloud.set_settings(settings);
            cloud.open(cx, spec)
        });
        if let Some(Err(err)) = result {
            self.set_status(cx, &err);
        }
        self.front_view(cx);
    }

    /// (Re)start playback from the chosen video and model.
    fn open_chosen(&mut self, cx: &mut Cx) {
        let input = match &self.video_path {
            Some(path) => VideoInput::File(path.clone()),
            None => VideoInput::Synthetic,
        };
        let layout = match input {
            VideoInput::Synthetic => FrameLayout::SideBySide,
            VideoInput::File(_) => FrameLayout::Full,
        };
        let depth = if layout != FrameLayout::Full {
            DepthSource::Packed(layout)
        } else {
            native(self.model_path.clone(), false).unwrap_or(DepthSource::GroundPrior)
        };
        let no_model = matches!(depth, DepthSource::GroundPrior);
        self.play(cx, SourceSpec { input, layout, depth });
        if no_model {
            self.set_status(
                cx,
                "No depth model loaded: using a rough guess. Click \"Open model...\" and pick depth_anything_v2_vits.pth.",
            );
        }
    }

    /// A video or model file from a dialog or a drop.
    fn open_file(&mut self, cx: &mut Cx, path: String) {
        if picker::is_model_file(&path) {
            if !cfg!(feature = "localai") {
                self.set_status(cx, "This build has no model support: rebuild with --features localai.");
                return;
            }
            self.model_path = Some(path);
        } else {
            self.video_path = Some(path);
        }
        self.open_chosen(cx);
    }

    fn update_settings(&mut self, cx: &mut Cx) {
        let settings = self.settings;
        self.with_cloud(cx, |_, cloud| cloud.set_settings(settings));
    }
}

impl MatchEvent for App {
    fn handle_startup(&mut self, cx: &mut Cx) {
        match Args::parse() {
            Ok(Args::Play(spec)) => {
                if let VideoInput::File(path) = &spec.input {
                    self.video_path = Some(path.clone());
                }
                #[cfg(feature = "localai")]
                if let DepthSource::Anything { model_path } = &spec.depth {
                    self.model_path = Some(model_path.clone());
                }
                self.play(cx, spec);
            }
            Ok(Args::Scene) => self.start_scene(cx),
            Err(err) => {
                log!("depth-cloud: {err}");
                self.set_status(cx, &err);
            }
        }
    }

    fn handle_next_frame(&mut self, cx: &mut Cx, e: &NextFrameEvent) {
        // Only the live scene needs the app to drive frames: it re-renders
        // and the cloud re-reads it. Video playback pumps inside the widget.
        if e.set.contains(&self.pump) && self.scene_mode {
            if !self.paused {
                cx.redraw_all();
            }
            self.pump = cx.new_next_frame();
        }
    }

    fn handle_actions(&mut self, cx: &mut Cx, actions: &Actions) {
        let cloud_uid = self.ui.widget(cx, ids!(cloud)).widget_uid();
        for action in actions.filter_widget_actions_cast::<DepthCloudAction>(cloud_uid) {
            match action {
                DepthCloudAction::Status(status) => self.set_status(cx, &status),
                DepthCloudAction::Stats(stats) => {
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
                DepthCloudAction::None => {}
            }
        }
        for action in actions {
            if let Some(path) = picker::picked(action, picker::PICK_VIDEO)
                .or_else(|| picker::picked(action, picker::PICK_MODEL))
            {
                self.open_file(cx, path);
            }
        }
        if self.ui.button(cx, ids!(open_video)).clicked(actions) {
            picker::pick_video(cx);
        }
        if self.ui.button(cx, ids!(open_model)).clicked(actions) {
            picker::pick_model(cx);
        }

        let ui = self.ui.clone();
        let slided = |cx: &mut Cx, id: &[LiveId]| ui.slider(cx, id).slided(actions);

        if let Some(v) = slided(cx, ids!(depth_res)) {
            self.settings.depth_res = v.round() as usize;
            self.update_settings(cx);
        }
        if let Some(v) = slided(cx, ids!(depth_every)) {
            self.settings.depth_every = v.round().max(1.0) as u32;
            self.update_settings(cx);
        }
        if let Some(v) = slided(cx, ids!(range_smoothing)) {
            self.settings.range_smoothing = v as f32;
            self.update_settings(cx);
        }
        if let Some(v) = slided(cx, ids!(pixel_smoothing)) {
            self.settings.pixel_smoothing = v as f32;
            self.update_settings(cx);
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
        if let Some(v) = slided(cx, ids!(crop_near)) {
            self.crop.0 = v as f32;
            self.apply_crop(cx);
        }
        if let Some(v) = slided(cx, ids!(crop_far)) {
            self.crop.1 = v as f32;
            self.apply_crop(cx);
        }
        if let Some(index) = self.ui.drop_down(cx, ids!(effect)).changed(actions) {
            let effect = CloudEffect::ALL.get(index).copied().unwrap_or_default();
            self.with_cloud(cx, |_, cloud| cloud.effect = effect);
            redraw = true;
        }
        if let Some(v) = slided(cx, ids!(effect_strength)) {
            self.with_cloud(cx, |_, cloud| cloud.effect_strength = v as f32);
            redraw = true;
        }
        if let Some(v) = slided(cx, ids!(effect_radius)) {
            self.with_cloud(cx, |_, cloud| cloud.effect_radius = v as f32);
            redraw = true;
        }
        if let Some(on) = self.ui.check_box(cx, ids!(momentum)).changed(actions) {
            self.with_cloud(cx, |_, cloud| cloud.momentum = on);
            redraw = true;
        }
        if let Some(v) = slided(cx, ids!(spring)) {
            self.with_cloud(cx, |_, cloud| cloud.spring = v as f32);
        }
        if let Some(v) = slided(cx, ids!(damping)) {
            self.with_cloud(cx, |_, cloud| cloud.damping = v as f32);
        }

        if self.ui.button(cx, ids!(front_view)).clicked(actions) {
            self.front_view(cx);
        }
        if self.ui.button(cx, ids!(play_pause)).clicked(actions) {
            self.paused = !self.paused;
            let paused = self.paused;
            self.with_cloud(cx, |_, cloud| cloud.set_paused(paused));
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
        makepad_depth_cloud::script_mod(vm);
        rendered::script_mod(vm);
        self::script_mod(vm)
    }

    fn handle_event(&mut self, cx: &mut Cx, event: &Event) {
        if let Event::Drop(drop) = event {
            for item in drop.items.iter() {
                if let DragItem::FilePath { path, .. } = item {
                    self.open_file(cx, path.clone());
                }
            }
        }
        self.match_event(cx, event);
        self.ui.handle_event(cx, event, &mut Scope::empty());
    }
}
