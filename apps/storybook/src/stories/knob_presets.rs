//! The Material component's second page: the Material Bench's knob presets.
//!
//! Every one of the bench's nineteen knob styles, live, in the material the
//! controls pick, on that material's ground; the picked style large and in
//! 3D beside them. The knobs are the storybook's port of the bench's knob
//! engine (`crate::knob`); this page is its host: it holds the material the
//! controls write, hands it to every knob and to the 3D view, and picks the
//! style a knob in the gallery is tapped on.
use crate::controls::{ControlValue, StoryControlAction};
use crate::knob::look::{color_of, KnobLook, STYLE_INDEX, STYLE_PRESET, VALUE};
use crate::knob::presets::{KnobMaterial, STYLES};
use crate::knob::widgets::{set_material_uniforms, KnobView3dWidgetExt, TurnedKnobAction, TurnedKnobWidgetExt};
use crate::makepad_widgets::*;
use crate::registry::Story;

pub fn script_mod(vm: &mut ScriptVm) -> ScriptValue {
    crate::knob::script_mod(vm);
    page::script_mod(vm)
}

mod page {
    use super::KnobPresets;
    use crate::makepad_widgets::*;

    script_mod! {
        use mod.prelude.widgets.*
        use mod.widgets.*
        use mod.storybook.*

        mod.storybook.KnobPresetsBase = #(KnobPresets::register_widget(vm))
        mod.storybook.KnobPresets = set_type_default() do mod.storybook.KnobPresetsBase{
            width: Fill
            height: Fill
        }

        // One gallery cell: a knob and its style's name under it.
        // A knob's light and shadow reach past its cell; no view between it
        // and the page clips them.
        let KnobCell = View{
            width: 136.
            height: Fit
            flow: Down
            clip_x: false
            clip_y: false
            align: Align{x: 0.5 y: 0.0}
            knob := TurnedKnob{width: 136. height: 128. fill: 0.5}
            name := Label{
                text: ""
                draw_text +: {text_style: theme.font_regular{font_size: 9.5}}
            }
        }

        let PageNote = P{
            width: Fill
            text: ""
        }

        mod.stories.MaterialKnobPresets = mod.storybook.KnobPresets{
            flow: Down
            spacing: 14.
            padding: Inset{left: 24. right: 24. top: 20. bottom: 24.}
            scroll_bars: ScrollBars{
                show_scroll_x: false
                show_scroll_y: true
                scroll_bar_y.drag_scrolling: true
            }
            // The page is the material's ground, through the same exposure
            // and roll-off as the knobs, so their quads meet it seamlessly.
            show_bg: true
            draw_bg +: {..mod.storybook.KnobGroundFill}

            intro := PageNote{text: "The Material Bench's knob engine, ported: nineteen knob styles in twelve materials. Every knob here is live -- drag one to turn them all, tap one to pick its style. The large knob and the 3D view show the picked style; drag the 3D view to orbit it, ctrl-scroll to zoom, double-tap to put the camera back."}
            stage := View{
                width: Fill
                height: Fit
                flow: Flow.Right{wrap: true}
                clip_x: false
                clip_y: false
                spacing: 16.
                align: Align{x: 0.0 y: 0.5}
                view3d := KnobView3d{width: 403. height: 290.}
                side := View{
                    width: 270.
                    height: Fit
                    flow: Down
                    clip_x: false
                    clip_y: false
                    spacing: 4.
                    align: Align{x: 0.5 y: 0.0}
                    big := TurnedKnob{width: 270. height: 270. fill: 0.62}
                    caption := Label{
                        text: ""
                        draw_text +: {text_style: theme.font_bold{font_size: 11}}
                    }
                }
            }
            gallery := View{
                width: Fill
                height: Fit
                flow: Flow.Right{wrap: true}
                clip_x: false
                clip_y: false
                c0 := KnobCell{}
                c1 := KnobCell{}
                c2 := KnobCell{}
                c3 := KnobCell{}
                c4 := KnobCell{}
                c5 := KnobCell{}
                c6 := KnobCell{}
                c7 := KnobCell{}
                c8 := KnobCell{}
                c9 := KnobCell{}
                c10 := KnobCell{}
                c11 := KnobCell{}
                c12 := KnobCell{}
                c13 := KnobCell{}
                c14 := KnobCell{}
                c15 := KnobCell{}
                c16 := KnobCell{}
                c17 := KnobCell{}
                c18 := KnobCell{}
            }
        }
    }
}

/// The page: the look its controls write (`crate::knob::look`), handed to
/// every knob and to the 3D view.
#[derive(Script, Widget)]
pub struct KnobPresets {
    #[deref]
    view: View,
    #[live]
    look: KnobLook,
    /// What the children were last handed, so a draw that changes nothing
    /// hands them nothing.
    #[rust]
    pushed: Option<(KnobMaterial, usize, f64, bool)>,
}

impl KnobPresets {
    fn cell(i: usize) -> LiveId {
        LiveId::from_str(&format!("c{i}"))
    }

    /// Hand the material, the value and the picked style to every knob and
    /// the 3D view, and colour the text to read on the ground.
    fn push(&mut self, cx: &mut Cx) {
        let m = self.look.material();
        let style = self.look.style_index();
        let state = (m, style, self.look.value, self.look.lit);
        if self.pushed == Some(state) {
            return;
        }
        let first = self.pushed.is_none();
        let old = self.pushed.replace(state);
        let ground = crate::knob::bake::ink(m.ground);
        let luma = ground[0] * 0.2126 + ground[1] * 0.7152 + ground[2] * 0.0722;
        let text: Vec4f = if luma > 0.45 { vec4(0.14, 0.15, 0.18, 1.0) } else { vec4(0.86, 0.88, 0.91, 1.0) };
        let meta: Vec4f = if luma > 0.45 { vec4(0.30, 0.32, 0.37, 1.0) } else { vec4(0.62, 0.65, 0.70, 1.0) };
        // The picked style's name in the glow ink, where that reads on the
        // ground; a white glow on porcelain does not, and takes the text's.
        let glow = crate::knob::bake::ink(m.glow_ink);
        let glow_luma = glow[0] * 0.2126 + glow[1] * 0.7152 + glow[2] * 0.0722;
        let accent = if (glow_luma - luma).abs() > 0.3 { color_of(m.glow_ink) } else { text };
        let recolour = first || old.map(|o| o.0.ground != m.ground || o.0.glow_ink != m.glow_ink).unwrap_or(true);
        let restyle = first || old.map(|o| o.1 != style).unwrap_or(true);
        for i in 0..STYLES.len() {
            let knob = self.view.turned_knob(cx, &[Self::cell(i), live_id!(knob)]);
            knob.set_style(cx, i);
            knob.set_material(cx, &m);
            knob.set_value(cx, self.look.value);
            knob.set_lit(cx, self.look.lit);
            if recolour || restyle {
                let mut name = self.view.widget(cx, &[Self::cell(i), live_id!(name)]);
                let label = STYLES[i].label();
                let color = if i == style { accent } else { meta };
                name.set_text(cx, &label);
                script_apply_eval!(cx, name, { draw_text +: {color: #(color)} });
            }
        }
        let big = self.view.turned_knob(cx, &[live_id!(big)]);
        big.set_style(cx, style);
        big.set_material(cx, &m);
        big.set_value(cx, self.look.value);
        big.set_lit(cx, self.look.lit);
        let view3d = self.view.knob_view3d(cx, &[live_id!(view3d)]);
        view3d.set_style(cx, style);
        view3d.set_material(cx, &m);
        view3d.set_value(cx, self.look.value);
        if recolour || restyle {
            let mut caption = self.view.widget(cx, &[live_id!(caption)]);
            caption.set_text(cx, &format!("{} in {}", STYLES[style].label(), self.look.material_name()));
            script_apply_eval!(cx, caption, { draw_text +: {color: #(text)} });
            let mut intro = self.view.widget(cx, &[live_id!(intro)]);
            script_apply_eval!(cx, intro, { draw_text +: {color: #(meta)} });
        }
        self.view.redraw(cx);
    }
}

impl ScriptHook for KnobPresets {
    fn on_after_new(&mut self, _vm: &mut ScriptVm) {
        self.look.reset();
    }

    fn on_after_apply(&mut self, vm: &mut ScriptVm, _apply: &Apply, _scope: &mut Scope, _value: ScriptValue) {
        vm.with_cx_mut(|cx| self.view.redraw(cx));
    }
}

impl Widget for KnobPresets {
    fn draw_walk(&mut self, cx: &mut Cx2d, scope: &mut Scope, walk: Walk) -> DrawStep {
        self.push(cx);
        let m = self.look.material();
        set_material_uniforms(cx, &mut self.view.draw_bg.draw_vars, &m);
        self.view.draw_walk(cx, scope, walk)
    }

    fn handle_event(&mut self, cx: &mut Cx, event: &Event, scope: &mut Scope) {
        let actions = cx.capture_actions(|cx| self.view.handle_event(cx, event, scope));
        let mut picked = None;
        let mut turned = None;
        for action in actions.iter() {
            let Some(wa) = action.as_widget_action() else {
                continue;
            };
            match wa.cast::<TurnedKnobAction>() {
                TurnedKnobAction::Changed(v) => turned = Some(v),
                TurnedKnobAction::Tapped => {
                    for i in 0..STYLES.len() {
                        let knob = self.view.turned_knob(cx, &[Self::cell(i), live_id!(knob)]);
                        if knob.widget_uid() == wa.widget_uid {
                            picked = Some(i);
                        }
                    }
                }
                TurnedKnobAction::None => {}
            }
        }
        // What a knob does on the page, the controls show: the value
        // slider follows a turn and the style's two controls a pick.
        let uid = self.widget_uid();
        if let Some(v) = turned {
            self.look.value = v;
            self.push(cx);
            cx.widget_action(uid, StoryControlAction::Set { label: VALUE, value: ControlValue::Number(v) });
        }
        if let Some(i) = picked {
            self.look.style = i as f64;
            self.push(cx);
            cx.widget_action(uid, KnobPresetsAction::StylePicked(i));
            cx.widget_action(uid, StoryControlAction::Set { label: STYLE_PRESET, value: ControlValue::Choice(i) });
            cx.widget_action(
                uid,
                StoryControlAction::Set { label: STYLE_INDEX, value: ControlValue::Number(i as f64) },
            );
        }
        cx.extend_actions(actions);
    }
}

/// What the page raised.
#[derive(Clone, Debug, Default, PartialEq)]
pub enum KnobPresetsAction {
    /// A knob in the gallery was tapped: its style is the picked one now.
    StylePicked(usize),
    #[default]
    None,
}

pub const STORIES: &[Story] = &[Story {
    key: "containers/material/knobs",
    category: "Containers",
    component: "Material",
    also: &["TurnedKnob", "KnobView3d"],
    name: "Knob presets",
    dsl: "MaterialKnobPresets",
    added: "2026-09-26",
    tags: &["material", "knob", "presets", "3d", "skeuomorph", "bench"],
    doc: "# Knob presets

The Material Bench's knob engine, ported into the storybook: every one of its nineteen knob styles, live, in any of its eleven materials (and a dark neumorphic one), on that material's ground. The picked style is shown large and in 3D beside the gallery.

## The engine

`TurnedKnob` draws one knob in one quad, grown past its layout rect as far as its light and shadow reach and composed over the page (transparent where it paints nothing), so no quad edge ever shows: the ground under it with the knob's cast shadow, ground lip and contact ring, the wells some styles stand in, the knob's face and its marks -- the pointer, the dial ticks and the value arc. `KnobView3d` ray marches the same solid under an orbiting camera. Both live in the storybook (`apps/storybook/src/knob/`); nothing in the widget library depends on them.

A **style** is geometry: a revolve profile drawn as a Bezier curve, a grip (flutes, knurls, lobes), a wing -- a ridge added along the pointer or two cutters taken away -- a cut (a dimple, a slot, scallops, a ring), a flat, a cap, and the marks. A **material** is light and finish: the key light, the tier, the specular and roughness (GGX), metal, a clear coat, the studio it reflects (a softbox studio, a chrome studio, outdoors), exposure and highlight roll-off, the shadow, and seven inks. The two multiply: any style in any material.

What the bench works out in JavaScript is worked out here in Rust, once per style and light: the curves resampled to 256 taps, the revolve's outline by height for the analytic cast-shadow sweep, the wing as a few knots, and the self-shadow over the disc as a 64 x 64 table. The shader reads them from two small textures.

## Reading a knob

- The **cast shadow** is swept analytically along the light from the solid's outline at each height, so a tall wing throws a long, soft shadow and a low skirt a short, crisp one.
- The **face** is lit from the solid's own height field: five taps give the normal, the curvature widens the highlight where the surface turns inside a pixel, and a crease is reflected from both of its sides with the darker kept, so thin bright lines do not sparkle.
- Reflective materials show the studio in the face; metals show it in their own colour.

## Using it

Drag any knob, the large one included, to turn them all; tap a knob in the gallery to pick its style. On the 3D view a drag orbits the camera, ctrl and the wheel zoom, and a double tap puts the camera back.

The Controls tab has the style and value, a material preset that sets every material control at once, and the bench's material controls in folding groups: Light, Surface, Environment, Relief, Wells, Shadow and Colours.",
    subject: "",
    feature: None,
    controls: crate::knob_controls!(),
    on_actions: None,
}];
