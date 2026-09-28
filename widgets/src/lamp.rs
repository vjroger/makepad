//! Lamp -- an indicator lamp, round or a bar, lit by an amount that eases.
//!
//! A lamp says that something is on: power, a latched mode, a record armed.
//! Two things decide whether one reads as a lamp or as a coloured dot, and
//! both were measured on the reference collection before a line of this was
//! written.
//!
//! # Off is a lens, not a hole
//!
//! A lamp that is out still shows its lens: dark, faintly the colour it
//! would light, with its rim. An empty socket reads as a missing part, and
//! a lamp that simply disappears when it goes out takes the layout's
//! information with it.
//!
//! # The halo holds a ceiling by construction
//!
//! On the references a lit lamp's light one device pixel outside it is 20
//! to 45 percent of the lamp's own excess brightness, halves within a third
//! to a half of the lamp's thickness, and is gone within one thickness.
//! `halo` and `halo_reach` are the two properties that move it, and their
//! ranges END at that ceiling: the widget clamps them, and the shader clamps
//! them again, so neither a page nor a sheet writing the instance directly
//! can make a lamp glow louder than a lamp does. `halo_at` is the profile,
//! the same arithmetic the shader runs, and the tests hold it to the numbers.
//!
//! The lit amount eases (`Glide`), so a lamp comes on and goes out rather
//! than blinking, and it stops asking for frames the moment it arrives.
use crate::{makepad_derive_widget::*, makepad_draw::*, widget::*};

/// The largest halo a lamp draws one device pixel outside its edge, as a
/// share of its own light.
pub const HALO_CEILING: f32 = 0.45;
/// The furthest a halo reaches, in lamp thicknesses.
pub const REACH_CEILING: f32 = 1.0;

script_mod! {
    use mod.prelude.widgets_internal.*
    use mod.widgets.*

    /** A lamp's outline: a round lens or a bar. */
    mod.widgets.LampShape = #(LampShape::script_api(vm))
    /** What a lamp's colour means, taken from the theme's roles. */
    mod.widgets.LampIntent = #(LampIntent::script_api(vm))

    mod.widgets.DrawLampBase = #(DrawLamp::script_component(vm))
    set_type_default() do #(DrawLamp::script_shader(vm)){
        ..mod.draw.DrawQuad
    }

    mod.widgets.LampBase = #(Lamp::register_widget(vm))
    /** An indicator lamp, round or a bar, lit by an amount from 0 to 1 that
     * eases. Its colour comes from its intent; off it shows a dark lens, lit
     * a pale core, and its halo can never pass the measured ceiling. */
    mod.widgets.Lamp = set_type_default() do mod.widgets.LampBase{
        width: Fit
        height: Fit
        /** a round lens or a bar */
        shape: mod.widgets.LampShape.Round
        /** what the colour means */
        intent: mod.widgets.LampIntent.Accent
        /** how far lit, 0 out and 1 fully lit; the lamp eases to it 0..1 step 0.01 */
        lit: 0.0
        /** the lens: a round lamp's diameter, a bar's thickness, in points 2..32 step 0.5 */
        size: theme.size_icon_s * 0.67
        /** a bar's length in points; 0 makes it three times its thickness 0..96 step 1 */
        length: 0.0
        /** the halo one device pixel out, as a share of the lamp's light; ends at the measured 0.45 0..0.45 step 0.01 */
        halo: theme.lamp_halo
        /** how far the halo reaches, in lamp thicknesses; ends at the measured one 0..1 step 0.05 */
        halo_reach: 1.0
        /** seconds a change of the lit amount takes 0..1 step 0.01 */
        ease_secs: theme.motion_short_3

        // `lit`..`opacity` are the instances: they ride in the draw
        // struct, so every other property here is a uniform or the
        // instance slots stop lining up with the struct.
        draw_bg +: {
            lit: 0.0
            intent: 0.0
            bar: 0.0
            halo: 0.25
            reach: 1.0
            opacity: 1.0
            /** the accent lamp, lit */
            color_accent: uniform(theme.color_primary)
            /** the success lamp, lit */
            color_success: uniform(theme.color_success)
            /** the warning lamp, lit */
            color_warning: uniform(theme.color_warning)
            /** the error lamp, lit */
            color_error: uniform(theme.color_error)
            /** the lamp with no intent, lit */
            color_plain: uniform(theme.color_lamp_plain)
            /** the dark lens of a lamp that is out */
            color_off: uniform(theme.color_lamp_off)
            /** how much of the lit colour the dark lens carries 0..0.4 step 0.01 */
            lens_tint: uniform(0.14)
            /** how far the lit core pales toward white 0..1 step 0.05 */
            core: uniform(0.55)
            /** the bar's corner, in points 0..8 step 0.25 */
            bar_radius: uniform(1.5)

            // The lit colour of this lamp's intent.
            intent_color: fn() -> vec4 {
                let i = self.intent
                if i < 0.5 { return self.color_accent }
                if i < 1.5 { return self.color_success }
                if i < 2.5 { return self.color_warning }
                if i < 3.5 { return self.color_error }
                return self.color_plain
            }

            // The halo at `d` points outside the lens: the share of the
            // lamp's light left there. `strength` at the edge, so never more
            // than that one device pixel out, half of it 0.4 of the reach
            // out, gone at the reach. Both inputs clamped to the ceiling,
            // here as well as in Rust.
            halo_at: fn(d: float, t: float, px: float, strength: float, reach: float) -> float {
                let s = clamp(strength, 0.0, 0.45)
                let r = clamp(reach, 0.0, 1.0) * t
                if d <= 0.0 || r <= px {
                    return 0.0
                }
                let half = max(0.4 * r, 0.001)
                let fall = pow(2.0, -d / half)
                return s * fall * (1.0 - smoothstep(0.55 * r, r, d))
            }

            pixel: fn() {
                let p = self.pos * self.rect_size
                let px = 1.0 / max(self.draw_pass.dpi_factor, 0.5)
                let reach = clamp(self.reach, 0.0, 1.0)
                // The quad is the lens with the halo's room around it.
                let t = self.rect_size.y / (1.0 + 2.0 * reach)
                let room = t * reach
                let c = self.rect_size * 0.5
                let half_len = max((self.rect_size.x - 2.0 * room) * 0.5, t * 0.5)
                let hb = vec2(mix(t * 0.5, half_len, self.bar), t * 0.5)
                let corner = mix(t * 0.5, min(self.bar_radius, t * 0.5), self.bar)
                let d = Material.sd_box(p, c, hb, corner)
                let lit = clamp(self.lit, 0.0, 1.0)
                let ink = self.intent_color()

                // Off: a dark lens faintly the colour it lights, a shade
                // lighter at the top as a dome is, with a small glint.
                let v = clamp((p.y - (c.y - t * 0.5)) / max(t, 0.001), 0.0, 1.0)
                let lens = mix(self.color_off.rgb, ink.rgb, self.lens_tint) * (1.1 - 0.2 * v)
                // Lit: the plain colour, paling toward the middle.
                let along = max(abs(p.x - c.x) - (hb.x - t * 0.5), 0.0)
                let core_d = length(vec2(along, p.y - c.y)) / max(t * 0.5, 0.001)
                let pale = mix(ink.rgb, vec3(1.0, 1.0, 1.0), 0.65)
                let glow = mix(ink.rgb, pale, self.core * (1.0 - smoothstep(0.0, 0.8, core_d)))
                var body = mix(lens, glow, lit)
                let glint = (1.0 - smoothstep(0.0, t * 0.16, length(p - vec2(c.x - hb.x + t * 0.32, c.y - t * 0.2)))) * (1.0 - lit)
                body = mix(body, vec3(1.0, 1.0, 1.0), glint * 0.22)
                // The rim: one device pixel just inside the edge, darker.
                body = mix(body, body * 0.5, Finish.band(d, 0.0, px, px) * (0.8 - 0.4 * lit))
                let a = Finish.cover(d, px)
                // A lens sits in the housing: a device pixel of light just
                // under its lower edge, so an unlit lens on a dark ground
                // still reads as a part and not as a hole.
                let lip = Finish.ring_out(d, 0.0, px, px) * step(c.y, p.y) * 0.14 * (1.0 - lit)
                let h = max(self.halo_at(d, t, px, self.halo * lit, reach), 0.0)
                let under = vec4(ink.rgb * h, h) + vec4(lip, lip, lip, lip) * (1.0 - h)
                let out = Finish.over(under, vec4(body * a, a))
                return out * self.opacity
            }
        }
    }

    /** A bar lamp: the same lamp three times as long as it is thick. */
    mod.widgets.LampBar = mod.widgets.Lamp{
        shape: mod.widgets.LampShape.Bar
    }
}

/// A lamp's outline.
#[derive(Copy, Clone, Debug, PartialEq, Default, Script, ScriptHook)]
pub enum LampShape {
    #[pick]
    #[default]
    Round,
    Bar,
}

/// What a lamp's colour means. Each takes its colour from the theme role of
/// the same name; `Accent` is the theme's primary colour and `Plain` a
/// neutral light with no meaning of its own.
#[derive(Copy, Clone, Debug, PartialEq, Default, Script, ScriptHook)]
pub enum LampIntent {
    #[pick]
    #[default]
    Accent,
    Success,
    Warning,
    Error,
    Plain,
}

impl LampIntent {
    fn index(self) -> f32 {
        match self {
            LampIntent::Accent => 0.0,
            LampIntent::Success => 1.0,
            LampIntent::Warning => 2.0,
            LampIntent::Error => 3.0,
            LampIntent::Plain => 4.0,
        }
    }
}

/// The halo at `d` points outside a lamp `t` points thick, as a share of
/// the lamp's own light: what the shader draws, for a test to hold to the
/// measured ceiling. `px` is one device pixel in points.
pub fn halo_at(d: f32, t: f32, px: f32, strength: f32, reach: f32) -> f32 {
    let s = strength.clamp(0.0, HALO_CEILING);
    let r = reach.clamp(0.0, REACH_CEILING) * t;
    if d <= 0.0 || r <= px {
        return 0.0;
    }
    let half = (0.4 * r).max(0.001);
    let fall = 2f32.powf(-d / half);
    let x = ((d - 0.55 * r) / (0.45 * r)).clamp(0.0, 1.0);
    let window = 1.0 - x * x * (3.0 - 2.0 * x);
    s * fall * window
}

/// A value that eases toward a target: a fixed share of what is left per
/// unit of time, so it arrives in about `secs` however unevenly it is
/// ticked, and snaps the last little way so it can stop.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Glide {
    value: f32,
    target: f32,
}

impl Glide {
    pub fn new(value: f32) -> Self {
        Self { value, target: value }
    }

    pub fn value(&self) -> f32 {
        self.value
    }

    pub fn target(&self) -> f32 {
        self.target
    }

    /// Head for `target` from wherever it is now.
    pub fn set(&mut self, target: f32) {
        self.target = target;
    }

    /// Be at `value` now, with nothing left to ease.
    pub fn jump(&mut self, value: f32) {
        self.value = value;
        self.target = value;
    }

    pub fn is_settled(&self) -> bool {
        self.value == self.target
    }

    /// One tick of `dt` seconds toward the target over `secs`. A gap longer
    /// than a quarter second is taken as a quarter second: coming back to a
    /// window that was not painted should finish the ease, not skip it.
    pub fn tick(&mut self, dt: f32, secs: f32) {
        let dt = if dt.is_finite() { dt.clamp(0.0, 0.25) } else { 0.0 };
        if secs <= 0.0 {
            self.value = self.target;
            return;
        }
        // Three time constants in `secs`: 95 percent of the way there.
        let k = 1.0 - (-dt * 3.0 / secs).exp();
        self.value += (self.target - self.value) * k;
        if (self.target - self.value).abs() < 0.002 {
            self.value = self.target;
        }
    }
}

/// The lamp's shader. The instances carry everything that differs from one
/// lamp to the next, so a row of lamps of every intent is one draw call.
#[derive(Script, ScriptHook)]
#[repr(C)]
pub struct DrawLamp {
    #[deref]
    draw_super: DrawQuad,
    #[live]
    lit: f32,
    #[live]
    intent: f32,
    #[live]
    bar: f32,
    #[live]
    halo: f32,
    #[live]
    reach: f32,
    #[live]
    opacity: f32,
}

/// An indicator lamp.
#[derive(Script, ScriptHook, Widget)]
pub struct Lamp {
    #[uid]
    uid: WidgetUid,
    #[source]
    source: ScriptObjectRef,
    #[walk]
    walk: Walk,
    #[layout]
    layout: Layout,
    #[redraw]
    #[live]
    draw_bg: DrawLamp,
    #[live]
    pub shape: LampShape,
    #[live]
    pub intent: LampIntent,
    /// How far lit, 0 to 1; the lamp eases to it.
    #[live]
    pub lit: f64,
    /// The lens diameter, or a bar's thickness, in points.
    #[live(8.0)]
    pub size: f64,
    /// A bar's length; zero makes it three thicknesses long.
    #[live]
    pub length: f64,
    /// The halo one device pixel out, as a share of the lamp's light.
    #[live(0.25)]
    pub halo: f64,
    /// How far the halo reaches, in lamp thicknesses.
    #[live(1.0)]
    pub halo_reach: f64,
    #[live(0.15)]
    pub ease_secs: f64,
    #[live]
    pub disabled: bool,
    #[rust]
    glide: Glide,
    /// The `lit` the lamp last headed for, so a script apply that moves it is
    /// seen at draw time; `None` before the first draw, which jumps.
    #[rust]
    seeded: Option<f64>,
    #[rust]
    last_tick: f64,
    #[rust]
    running: bool,
    #[rust]
    next_frame: NextFrame,
    #[rust]
    #[area]
    area: Area,
}

impl Lamp {
    /// Light the lamp to `amount`, 0 out to 1 fully lit, easing there.
    pub fn set_lit(&mut self, cx: &mut Cx, amount: f64) {
        self.lit = amount.clamp(0.0, 1.0);
        self.head_for(cx);
    }

    /// Where the lamp is heading, 0 to 1.
    pub fn lit(&self) -> f64 {
        self.lit.clamp(0.0, 1.0)
    }

    /// The halo strength and reach the lamp will draw: its properties held
    /// to the ceiling.
    pub fn halo_drawn(&self) -> (f32, f32) {
        ((self.halo as f32).clamp(0.0, HALO_CEILING), (self.halo_reach as f32).clamp(0.0, REACH_CEILING))
    }

    fn head_for(&mut self, cx: &mut Cx) {
        let target = self.lit.clamp(0.0, 1.0);
        self.seeded = Some(self.lit);
        self.glide.set(target as f32);
        if !self.glide.is_settled() && !self.running {
            self.running = true;
            self.last_tick = cx.seconds_since_app_start();
            self.next_frame = cx.new_next_frame();
        }
        self.draw_bg.redraw(cx);
    }

    /// The lens and the room its halo needs around it, in points.
    fn extent(&self) -> DVec2 {
        let t = self.size.max(1.0);
        let room = t * self.halo_drawn().1 as f64;
        let len = match self.shape {
            LampShape::Round => t,
            LampShape::Bar => {
                if self.length > 0.0 {
                    self.length.max(t)
                } else {
                    t * 3.0
                }
            }
        };
        dvec2(len + room * 2.0, t + room * 2.0)
    }
}

impl Widget for Lamp {
    fn draw_walk(&mut self, cx: &mut Cx2d, _scope: &mut Scope, walk: Walk) -> DrawStep {
        match self.seeded {
            None => {
                self.seeded = Some(self.lit);
                self.glide.jump(self.lit.clamp(0.0, 1.0) as f32);
            }
            Some(seen) if seen != self.lit => self.head_for(cx),
            _ => {}
        }
        let size = self.extent();
        let pad = self.layout.padding;
        let mut walk = walk;
        if walk.width.is_fit() {
            walk.width = Size::Fixed(size.x + pad.left + pad.right);
        }
        if walk.height.is_fit() {
            walk.height = Size::Fixed(size.y + pad.top + pad.bottom);
        }
        cx.begin_turtle(walk, Layout::default());
        let rect = cx.turtle().rect();
        let (halo, reach) = self.halo_drawn();
        self.draw_bg.lit = self.glide.value();
        self.draw_bg.intent = self.intent.index();
        self.draw_bg.bar = if self.shape == LampShape::Bar { 1.0 } else { 0.0 };
        self.draw_bg.halo = halo;
        self.draw_bg.reach = reach;
        self.draw_bg.opacity = if self.disabled { 0.5 } else { 1.0 };
        // The lens keeps its own proportions in the middle of whatever box
        // the layout handed over.
        let at = dvec2(
            rect.pos.x + pad.left + ((rect.size.x - pad.left - pad.right - size.x) * 0.5).max(0.0),
            rect.pos.y + pad.top + ((rect.size.y - pad.top - pad.bottom - size.y) * 0.5).max(0.0),
        );
        self.draw_bg.draw_abs(cx, Rect { pos: at, size });
        cx.end_turtle_with_area(&mut self.area);
        DrawStep::done()
    }

    fn handle_event(&mut self, cx: &mut Cx, event: &Event, _scope: &mut Scope) {
        if let Some(ne) = self.next_frame.is_event(event) {
            let dt = (ne.time - self.last_tick).max(0.0) as f32;
            self.last_tick = ne.time;
            self.glide.tick(dt, self.ease_secs as f32);
            self.draw_bg.redraw(cx);
            if self.glide.is_settled() {
                self.running = false;
            } else {
                self.next_frame = cx.new_next_frame();
            }
        }
    }

    fn set_disabled(&mut self, cx: &mut Cx, disabled: bool) {
        if self.disabled != disabled {
            self.disabled = disabled;
            self.draw_bg.redraw(cx);
        }
    }

    fn disabled(&self, _cx: &Cx) -> bool {
        self.disabled
    }

    fn snapshot_value(&self, _cx: &Cx) -> Option<String> {
        Some(format!("{:.2}", self.lit()))
    }
}

impl LampRef {
    pub fn set_lit(&self, cx: &mut Cx, amount: f64) {
        if let Some(mut inner) = self.borrow_mut() {
            inner.set_lit(cx, amount);
        }
    }

    pub fn lit(&self) -> f64 {
        self.borrow().map(|inner| inner.lit()).unwrap_or(0.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// One device pixel at the catalogue's density, in points.
    const PX: f32 = 1.0 / 1.5;

    /// The halo holds the measured ceiling whatever it is asked for: at one
    /// device pixel it is the strength asked for, never above 0.45; it
    /// halves within a third to a half of the lamp's thickness; and it is
    /// gone by one thickness, whatever reach was asked for.
    #[test]
    fn the_halo_holds_the_measured_ceiling() {
        for t in [6.0f32, 8.0, 12.0, 20.0] {
            for (strength, reach) in [(0.25, 0.8), (0.45, 1.0), (0.9, 3.0), (2.0, 1.0)] {
                let at_px = halo_at(PX, t, PX, strength, reach);
                assert!(at_px <= HALO_CEILING, "{at_px} at one pixel for strength {strength}");
                assert!(at_px <= strength.min(HALO_CEILING), "never more than asked for one pixel out: {at_px}");
                assert!(at_px >= 0.75 * strength.min(HALO_CEILING), "and most of it: {at_px}");
                // Where it has halved, measured from the pixel it starts at.
                let mut half = None;
                let mut d = PX;
                while d < 2.0 * t {
                    if halo_at(d, t, PX, strength, reach) <= at_px * 0.5 {
                        half = Some(d - PX);
                        break;
                    }
                    d += 0.01;
                }
                let half = half.expect("the halo halves");
                assert!(half <= t * 0.5 + 0.02, "halves by half the thickness: {half} of {t}");
                assert_eq!(halo_at(t + 0.01, t, PX, strength, reach), 0.0, "gone by one thickness");
            }
        }
        assert_eq!(halo_at(-1.0, 8.0, PX, 0.3, 1.0), 0.0, "no halo inside the lens");
        assert_eq!(halo_at(2.0, 8.0, PX, 0.3, 0.0), 0.0, "no reach, no halo");
    }

    /// The lit amount is an amount: out of range it is clamped, and a
    /// negative one is out, not a lamp lit backwards.
    #[test]
    fn the_lit_amount_is_clamped() {
        crate::on_test_cx(|| {
            let mut cx = crate::checkout_test_cx();
            let lamp = cx.with_vm(|vm| {
                let value = crate::script_eval!(vm, { mod.widgets.Lamp{lit: 1.7 halo: 0.9 halo_reach: 4.0} });
                WidgetRef::script_from_value(vm, value)
            });
            let mut inner = lamp.borrow_mut::<Lamp>().expect("a lamp");
            assert_eq!(inner.lit(), 1.0, "past full is full");
            assert_eq!(inner.halo_drawn(), (HALO_CEILING, REACH_CEILING), "the halo is held at the ceiling");
            inner.set_lit(&mut cx, -0.5);
            assert_eq!(inner.lit(), 0.0, "below out is out");
            inner.set_lit(&mut cx, 0.5);
            assert_eq!(inner.lit(), 0.5);
        });
    }

    /// The ease arrives in about its time however it is ticked, and stops.
    #[test]
    fn the_ease_arrives_and_stops() {
        let mut often = Glide::new(0.0);
        let mut seldom = Glide::new(0.0);
        often.set(1.0);
        seldom.set(1.0);
        for _ in 0..30 {
            often.tick(0.005, 0.15);
        }
        for _ in 0..3 {
            seldom.tick(0.05, 0.15);
        }
        assert!((often.value() - seldom.value()).abs() < 0.01, "{} against {}", often.value(), seldom.value());
        assert!(often.value() > 0.9, "most of the way in the ease time: {}", often.value());
        for _ in 0..20 {
            often.tick(0.05, 0.15);
        }
        assert!(often.is_settled(), "and it lands, so the frames can stop");
        let mut instant = Glide::new(0.0);
        instant.set(1.0);
        instant.tick(0.016, 0.0);
        assert_eq!(instant.value(), 1.0, "no ease time is a cut");
    }
}
