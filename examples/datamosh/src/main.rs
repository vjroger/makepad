//! Datamosh demo: the motion of one source moving the picture of another,
//! and the deleted-keyframe transition between them.
//!
//! The stage renders two procedural sources (a raymarched flight that
//! supplies exact motion vectors, and a 2D animation whose motion is
//! estimated) and runs `makepad-datamosh` on them. The panel picks the
//! picture, the motion and the decoder's knobs; "Datamosh transition" cuts
//! to the other source the way a missing I-frame would.

pub use makepad_widgets;

mod stage;

use makepad_datamosh::{MoshMode, MoshView, TransitionMotion};
use makepad_widgets::*;
use stage::{MoshStage, MotionChoice, Source};

app_main!(App);

script_mod! {
    use mod.prelude.widgets.*

    let Knob = Slider{
        width: Fill
    }

    startup() do #(App::script_component(vm)){
        ui: Root{
            main_window := Window{
                window.inner_size: vec2(1440, 800)
                pass.clear_color: #x0b0b10
                body +: {
                    flow: Right
                    stage := mod.widgets.MoshStage{}
                    panel := ScrollYView{
                        width: 320
                        height: Fill
                        flow: Down
                        spacing: 4
                        padding: Inset{left: 12 right: 12 top: 12 bottom: 12}

                        H4{text: "Demo"}
                        demo := DropDown{
                            labels: ["Effect (continuous mosh)" "Transition (clean A, mosh, clean B)"]
                            selected_item: 1
                        }

                        Hr{}
                        H4{text: "Mixer (the few sliders a VJ mixer has)"}
                        mix_drift := Knob{text: "Drift: 0 none .. patterns +/- .. top random" min: 0.0 max: 1.0 default: 0.636 precision: 3}
                        drift_label := Label{text: "drift: Zoom"}
                        mix_block := Knob{text: "Block size" min: 0.0 max: 1.0 default: 0.0 precision: 3}
                        mix_dirty := Knob{text: "Dirty: damage, incoming residual, 1 - hold" min: 0.0 max: 1.0 default: 0.1 precision: 3}

                        Hr{}
                        H4{text: "Sources"}
                        Label{text: "Picture (what gets moshed)"}
                        picture := DropDown{
                            labels: ["Grid flight" "Shapes"]
                            selected_item: 1
                        }
                        Label{text: "Motion (whose vectors move it)"}
                        motion := DropDown{
                            labels: ["Grid flight: render vectors" "Grid flight: estimated" "Shapes: estimated"]
                            selected_item: 0
                        }
                        source_fps := Knob{text: "Source fps" min: 6.0 max: 60.0 step: 1.0 default: 30.0 precision: 0}

                        Hr{}
                        H4{text: "Decoder"}
                        mosh_on := CheckBox{
                            text: "Mosh (off = a keyframe every frame)"
                            animator +: {active: {default: @on}}
                        }
                        freeze := CheckBox{text: "Freeze motion (repeat the last P-frame)"}
                        iframe := Button{text: "I-frame now"}
                        auto_iframe := Knob{text: "Auto I-frame every (s, 0 = never)" min: 0.0 max: 20.0 step: 0.5 default: 4.0 precision: 1}
                        mode := DropDown{
                            labels: ["Decode" "Remap" "Remap live"]
                            selected_item: 0
                        }
                        view := DropDown{
                            labels: ["Output" "Vectors" "Damage"]
                            selected_item: 0
                        }
                        gain := Knob{text: "Motion gain" min: -3.0 max: 3.0 default: 1.0 precision: 2}
                        pel := Knob{text: "Pel (0 cont., 1 full, 4 quarter)" min: 0.0 max: 4.0 step: 1.0 default: 4.0 precision: 0}
                        diffusion := Knob{text: "Diffusion (px)" min: 0.0 max: 8.0 default: 0.0 precision: 1}
                        blur_motion := Knob{text: "Motion blur: vectors (steps)" min: 0.0 max: 4.0 default: 0.0 precision: 2}
                        blur_drift := Knob{text: "Motion blur: drift (steps)" min: 0.0 max: 8.0 default: 0.0 precision: 2}
                        drift := Knob{text: "Drift strength (px per step)" min: -8.0 max: 8.0 default: 0.0 precision: 1}
                        refresh := Knob{text: "Intra refresh" min: 0.0 max: 0.25 default: 0.0 precision: 3}
                        heal := Knob{text: "Heal" min: 0.0 max: 0.25 default: 0.0 precision: 3}
                        residual := Knob{text: "Residual" min: 0.0 max: 1.0 default: 0.0 precision: 2}
                        wet := Knob{text: "Wet" min: 0.0 max: 1.0 default: 1.0 precision: 2}

                        Hr{}
                        H4{text: "Transition"}
                        transition := Button{text: "Datamosh transition to the other source"}
                        transition_motion := DropDown{
                            labels: ["Incoming motion" "Outgoing motion"]
                            selected_item: 0
                        }
                        transition_secs := Knob{text: "Duration (s)" min: 0.5 max: 8.0 default: 1.2 precision: 1}
                        refresh_peak := Knob{text: "Refresh at the end" min: 0.02 max: 1.0 default: 0.3 precision: 2}
                        fade_out := Knob{text: "Fade out (end of transition)" min: 0.0 max: 1.0 default: 0.58 precision: 2}
                    }
                }
            }
        }
    }
}

#[derive(Script, ScriptHook)]
pub struct App {
    #[live]
    ui: WidgetRef,
}

impl App {
    fn stage<R>(&self, cx: &mut Cx, f: impl FnOnce(&mut MoshStage) -> R) -> Option<R> {
        let widget = self.ui.widget(cx, ids!(stage));
        let mut stage = widget.borrow_mut::<MoshStage>()?;
        Some(f(&mut stage))
    }
}

impl MatchEvent for App {
    fn handle_actions(&mut self, cx: &mut Cx, actions: &Actions) {
        let Some(mut s) = self.stage(cx, |stage| stage.settings()) else {
            return;
        };
        let ui = self.ui.clone();
        let slided = |cx: &mut Cx, path: &[LiveId], into: &mut f32| {
            if let Some(v) = ui.slider(cx, path).slided(actions) {
                *into = v as f32;
            }
        };
        if let Some(i) = ui.drop_down(cx, ids!(demo)).selected(actions) {
            s.transition_mode = i == 1;
        }
        if let Some(i) = ui.drop_down(cx, ids!(picture)).selected(actions) {
            s.picture = Source::ALL[i.min(Source::ALL.len() - 1)];
        }
        if let Some(i) = ui.drop_down(cx, ids!(motion)).selected(actions) {
            s.motion = MotionChoice::ALL[i.min(MotionChoice::ALL.len() - 1)];
        }
        if let Some(i) = ui.drop_down(cx, ids!(mode)).selected(actions) {
            s.params.mode = MoshMode::ALL[i.min(MoshMode::ALL.len() - 1)];
        }
        if let Some(i) = ui.drop_down(cx, ids!(view)).selected(actions) {
            s.params.view = MoshView::ALL[i.min(MoshView::ALL.len() - 1)];
        }
        if let Some(i) = ui.drop_down(cx, ids!(transition_motion)).selected(actions) {
            s.transition.motion = TransitionMotion::ALL[i.min(TransitionMotion::ALL.len() - 1)];
        }
        if let Some(on) = ui.check_box(cx, ids!(mosh_on)).changed(actions) {
            s.mosh_on = on;
        }
        if let Some(on) = ui.check_box(cx, ids!(freeze)).changed(actions) {
            s.freeze = on;
        }
        if let Some(v) = ui.slider(cx, ids!(source_fps)).slided(actions) {
            s.source_fps = v;
        }
        if let Some(v) = ui.slider(cx, ids!(auto_iframe)).slided(actions) {
            s.auto_iframe = v;
        }
        if let Some(v) = ui.slider(cx, ids!(transition_secs)).slided(actions) {
            s.transition_secs = v;
        }
        slided(cx, ids!(gain), &mut s.params.gain);
        slided(cx, ids!(pel), &mut s.params.pel);
        slided(cx, ids!(diffusion), &mut s.params.diffusion);
        slided(cx, ids!(drift), &mut s.params.drift);
        slided(cx, ids!(blur_motion), &mut s.params.blur_motion);
        slided(cx, ids!(blur_drift), &mut s.params.blur_drift);
        slided(cx, ids!(mix_drift), &mut s.mixer.drift);
        slided(cx, ids!(mix_block), &mut s.mixer.block);
        slided(cx, ids!(mix_dirty), &mut s.mixer.dirty);
        ui.label(cx, ids!(drift_label))
            .set_text(cx, &format!("drift: {}", s.mixer.drift_label()));
        slided(cx, ids!(refresh), &mut s.params.refresh);
        slided(cx, ids!(heal), &mut s.params.heal);
        slided(cx, ids!(residual), &mut s.params.residual);
        slided(cx, ids!(wet), &mut s.params.wet);
        slided(cx, ids!(refresh_peak), &mut s.transition.refresh_peak);
        slided(cx, ids!(fade_out), &mut s.transition.fade_out);

        let iframe = ui.button(cx, ids!(iframe)).clicked(actions);
        let transition = ui.button(cx, ids!(transition)).clicked(actions);
        // The transition button puts the demo in transition mode.
        if transition && !s.transition_mode {
            s.transition_mode = true;
            ui.drop_down(cx, ids!(demo)).set_selected_item(cx, 1);
        }
        self.stage(cx, |stage| {
            stage.set_settings(s);
            if iframe {
                stage.request_iframe();
            }
            if transition {
                stage.start_transition();
            }
        });
    }
}

impl AppMain for App {
    fn script_mod(vm: &mut ScriptVm) -> ScriptValue {
        crate::makepad_widgets::script_mod(vm);
        makepad_datamosh::script_mod(vm);
        crate::stage::script_mod(vm);
        self::script_mod(vm)
    }

    fn handle_event(&mut self, cx: &mut Cx, event: &Event) {
        self.match_event(cx, event);
        self.ui.handle_event(cx, event, &mut Scope::empty());
        // A finished transition switched the picture: show it in the panel.
        if let Some(Some(source)) = self.stage(cx, |stage| stage.take_picture_changed()) {
            self.ui
                .drop_down(cx, ids!(picture))
                .set_selected_item(cx, source.index());
        }
    }
}
