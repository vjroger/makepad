//! The screen view pages under Containers: a view that is a display window,
//! with a bezel, a recessed face and glass over what it holds; and glass
//! that ripples like water where the pointer moves across it.
use crate::makepad_widgets::*;
use crate::registry::{Control, ControlKind, Story};

script_mod! {
    use mod.prelude.widgets.*
    use mod.widgets.*
    use mod.storybook.*

    let ScreenCaption = Label{
        draw_text +: {color: theme.color_screen_ink text_style: theme.font_regular{font_size: theme.font_size_p * 0.8}}
    }

    mod.stories.ScreenViewOverview = StoryPage{
        StoryNote{text: "A container that is a display: a slim bezel, a face set into it with the surround's shadow under its top edge, and a sheet of glass over whatever it holds. The children draw between the face and the glass, so the sheen lies over the digits, not only on the empty face around them."}
        StoryHeading{text: "One screen, under the controls"}
        StoryRow{
            subject := ScreenView{
                ScreenCaption{text: "OUTPUT"}
                Readout{text: "-6.0" cells: 5 digit_height: 30.}
                ScreenCaption{text: "dB"}
            }
        }

        StoryHeading{text: "What it holds"}
        StoryNote{text: "Anything a view holds: a readout with its caption, a level ladder, a line of text. Captions on a screen take the screen's own ink, a pale one on the dark glass of the dark theme and a dark one on the pale glass of the light theme."}
        StoryRow{
            ScreenView{
                flow: Right
                spacing: theme.space_2
                align: Align{y: 1.}
                Readout{text: "12:45" digit_height: 24.}
                ScreenCaption{text: "PM"}
            }
            ScreenView{
                width: 180.
                ScreenCaption{text: "INPUT L / R"}
                LevelMeter{width: Fill height: 6. lamp: false level: 0.72 draw_bg.segment: 4.}
                LevelMeter{width: Fill height: 6. lamp: false level: 0.58 draw_bg.segment: 4.}
            }
            ScreenView{
                width: 180.
                ScreenCaption{text: "READY"}
                ScreenCaption{text: "Take 3 of 12, 00:41 recorded"}
            }
        }

        StoryHeading{text: "Bezel, recess, sheen and scan lines"}
        StoryNote{text: "No bezel is a face cut straight into the panel; a wider one frames it. The recess is how far the surround's shadow reaches down the face. The sheen is quiet on purpose: a band of a few percent from the top left and a lighter line under the top edge. Scan lines are off unless a page or a sheet asks for them."}
        StoryRow{
            ScreenView{bezel: 0.0 Readout{text: "1.0" digit_height: 22.}}
            ScreenView{bezel: 5.0 Readout{text: "2.0" digit_height: 22.}}
            ScreenView{recess: 0.0 Readout{text: "3.0" digit_height: 22.}}
            ScreenView{recess: 9.0 Readout{text: "4.0" digit_height: 22.}}
            ScreenView{sheen: 0.0 Readout{text: "5.0" digit_height: 22.}}
            ScreenView{scan: 0.3 Readout{text: "6.0" digit_height: 22.}}
        }
    }

    // WATER UNDER THE POINTER: the page's own glass, so the ripples show
    // whatever sheet is on. Each drop of the pointer's trail sends a ring
    // out at 60 points a second, two crests deep and 10 points from crest
    // to crest; it lands over its first tenth of a second, weakens as it
    // grows and is gone when the drop's life is.
    // `ripple_drop` is one drop's water at `p` (the drop's place and age,
    // and how long a drop lives): its height and its slope.
    let ripple_drop = fn(p: vec2, d: vec3, life: float) -> vec3 {
        if d.z < 0.0 {
            return vec3(0.0, 0.0, 0.0)
        }
        let left = clamp(1.0 - d.z / max(life, 0.01), 0.0, 1.0)
        let q = p - d.xy
        let r = max(length(q), 0.001)
        let front = d.z * 60.0
        let u = r - front
        let k = 0.628
        let w = 8.0
        let g = exp(0.0 - u * u / (w * w))
        let amp = left * left * smoothstep(0.0, 0.1, d.z) / (1.0 + front / 40.0)
        let h = amp * cos(k * u) * g
        let dh = amp * g * (0.0 - k * sin(k * u) - 2.0 * u / (w * w) * cos(k * u))
        return vec3(h, dh * q.x / r, dh * q.y / r)
    }

    // The water's tilt at a point `face_d` from the face's edge (negative
    // inside): still at the bezel, the ripple dying out over the last 4
    // points, and never steeper than 1, so what the glass reads never
    // comes from past the face's edge.
    let ripple_tilt = fn(slope: vec2, face_d: float) -> vec2 {
        let g = slope * smoothstep(0.0, 4.0, 0.0 - face_d)
        let gl = length(g)
        return g * min(1.0, 1.0 / max(gl, 0.0001))
    }

    // The glass over the water: `under` is what lies under it read from
    // the window's scene along the tilt `g` (when `seen` is 1), the
    // theme's own `sheen` over it, light on the flanks that face the upper
    // left and shade on the others; without the scene, the sheen, light and
    // shade alone, over what is drawn under the glass.
    let ripple_lay = fn(under: vec3, seen: float, g: vec2, sheen: float, cov: float) -> vec4 {
        let lit = dot(g, vec2(-0.6, -0.8))
        let light = clamp(lit * 0.6, 0.0, 0.5)
        let shade = clamp(0.0 - lit * 0.4, 0.0, 0.4)
        if seen > 0.5 {
            var col = under * (1.0 - sheen) + vec3(sheen, sheen, sheen)
            col = mix(col, vec3(1.0, 1.0, 1.0), light)
            col = col * (1.0 - shade)
            return vec4(col * cov, cov)
        }
        var out = vec4(sheen, sheen, sheen, sheen)
        out = Finish.over(out, vec4(light, light, light, light))
        out = Finish.over(out, vec4(0.0, 0.0, 0.0, shade))
        return out * cov
    }

    let RippleScreen = ScreenView{
        field_reach: 2.0
        glass_backdrop: true
        draw_glass +: {
            trail_0: instance(vec3(0.0, 0.0, -1.0))
            trail_1: instance(vec3(0.0, 0.0, -1.0))
            trail_2: instance(vec3(0.0, 0.0, -1.0))
            trail_3: instance(vec3(0.0, 0.0, -1.0))
            trail_4: instance(vec3(0.0, 0.0, -1.0))
            trail_5: instance(vec3(0.0, 0.0, -1.0))
            trail_6: instance(vec3(0.0, 0.0, -1.0))
            trail_7: instance(vec3(0.0, 0.0, -1.0))
            trail_life: uniform(1.3)
            scene_texture: texture_2d(float)
            has_gauss: uniform(0.0)
            source_size: uniform(vec2(1.0, 1.0))
            source_y_flip: uniform(0.0)

            pixel: fn() {
                let p = self.pos * self.rect_size
                let px = 1.0 / max(self.draw_pass.dpi_factor, 0.5)
                let lo = vec2(Finish.snap(self.rect_pos.x, px), Finish.snap(self.rect_pos.y, px)) - self.rect_pos
                let hi = vec2(Finish.snap(self.rect_pos.x + self.rect_size.x, px), Finish.snap(self.rect_pos.y + self.rect_size.y, px) - px) - self.rect_pos
                let c = (lo + hi) * 0.5
                let ho = (hi - lo) * 0.5
                let r_out = min(self.face_radius, min(ho.x, ho.y))
                let b = Finish.snap(max(self.bezel, 0.0), px)
                let hf = max(ho - vec2(b, b), vec2(0.5, 0.5))
                let face_d = Material.sd_box(p, c, hf, max(r_out - b, 0.0))
                // The theme's quiet glass: the diagonal band and the line
                // under the top edge.
                let u = (p - (c - hf)) / max(hf * 2.0, vec2(1.0, 1.0))
                let diag = u.x + u.y * 0.6
                let band = smoothstep(0.0, 0.12, diag) * (1.0 - smoothstep(0.28, 0.5, diag))
                let top = Finish.cover(abs(p.y - (c.y - hf.y) - px * 1.5) - px * 0.5, px) * 0.6
                let sheen = clamp(self.sheen * (band + top), 0.0, 1.0)
                let s = ripple_drop(p, self.trail_0, self.trail_life) + ripple_drop(p, self.trail_1, self.trail_life) + ripple_drop(p, self.trail_2, self.trail_life) + ripple_drop(p, self.trail_3, self.trail_life) + ripple_drop(p, self.trail_4, self.trail_life) + ripple_drop(p, self.trail_5, self.trail_life) + ripple_drop(p, self.trail_6, self.trail_life) + ripple_drop(p, self.trail_7, self.trail_life)
                let g = ripple_tilt(s.yz, face_d)
                var under = vec3(0.0, 0.0, 0.0)
                if self.has_gauss > 0.5 {
                    // The content under the glass, seen through the tilted
                    // water: displaced along the slope, never past the face.
                    let ps = clamp(p + g * 3.0, c - hf, c + hf)
                    let sq = (self.rect_pos + ps) / max(self.source_size, vec2(1.0, 1.0))
                    under = self.scene_texture.sample(vec2(sq.x, mix(sq.y, 1.0 - sq.y, self.source_y_flip))).rgb
                }
                return ripple_lay(under, self.has_gauss, g, sheen, Finish.cover(face_d + px, px))
            }
        }
    }

    let RippleMeter = NeedleMeter{
        field_reach: 2.0
        glass_backdrop: true
        draw_glass +: {
            trail_0: instance(vec3(0.0, 0.0, -1.0))
            trail_1: instance(vec3(0.0, 0.0, -1.0))
            trail_2: instance(vec3(0.0, 0.0, -1.0))
            trail_3: instance(vec3(0.0, 0.0, -1.0))
            trail_4: instance(vec3(0.0, 0.0, -1.0))
            trail_5: instance(vec3(0.0, 0.0, -1.0))
            trail_6: instance(vec3(0.0, 0.0, -1.0))
            trail_7: instance(vec3(0.0, 0.0, -1.0))
            trail_life: uniform(1.3)
            scene_texture: texture_2d(float)
            has_gauss: uniform(0.0)
            source_size: uniform(vec2(1.0, 1.0))
            source_y_flip: uniform(0.0)

            pixel: fn() {
                let p = self.pos * self.rect_size
                let px = 1.0 / max(self.draw_pass.dpi_factor, 0.5)
                let c = self.rect_size * 0.5
                // Two points inside the face, so its rim stays still
                // whatever sheet drew it.
                let hf = max(c - vec2(2.0, 2.0), vec2(0.5, 0.5))
                let face_d = Material.sd_box(p, c, hf, max(min(self.face_radius, min(c.x, c.y)) - 2.0, 0.0))
                let s = ripple_drop(p, self.trail_0, self.trail_life) + ripple_drop(p, self.trail_1, self.trail_life) + ripple_drop(p, self.trail_2, self.trail_life) + ripple_drop(p, self.trail_3, self.trail_life) + ripple_drop(p, self.trail_4, self.trail_life) + ripple_drop(p, self.trail_5, self.trail_life) + ripple_drop(p, self.trail_6, self.trail_life) + ripple_drop(p, self.trail_7, self.trail_life)
                let g = ripple_tilt(s.yz, face_d)
                var under = vec3(0.0, 0.0, 0.0)
                if self.has_gauss > 0.5 {
                    // The content under the glass, seen through the tilted
                    // water: displaced along the slope, never past the face.
                    let ps = clamp(p + g * 3.0, c - hf, c + hf)
                    let sq = (self.rect_pos + ps) / max(self.source_size, vec2(1.0, 1.0))
                    under = self.scene_texture.sample(vec2(sq.x, mix(sq.y, 1.0 - sq.y, self.source_y_flip))).rgb
                }
                return ripple_lay(under, self.has_gauss, g, 0.0, Finish.cover(face_d + px, px))
            }
        }
    }

    mod.stories.ScreenViewRipples = StoryPage{
        StoryNote{text: "Glass that answers the pointer the way water does. Move the pointer across a screen or the meter: wherever it passes it leaves drops, each drop sends a ring out across the glass, and what lies under the glass, the digits, the needle and the scale, bends through the rings and settles again. Stop moving and the water stills; nothing is drawn or asked for until the pointer moves again."}
        StoryHeading{text: "Move the pointer across the glass"}
        StoryRow{
            subject := RippleScreen{
                ScreenCaption{text: "OUTPUT"}
                Readout{text: "-6.0" cells: 5 digit_height: 30.}
                ScreenCaption{text: "dB"}
            }
            RippleMeter{width: 200. height: 120. value: 0.62 label: "VU"}
        }

        StoryHeading{text: "Small screens, side by side"}
        StoryNote{text: "Each screen keeps its own water: a ring stays inside the glass it was dropped in and dies out before the bezel, which stays still."}
        StoryRow{
            RippleScreen{flow: Right spacing: theme.space_2 align: Align{y: 1.} Readout{text: "12:45" digit_height: 24.} ScreenCaption{text: "PM"}}
            RippleScreen{flow: Right spacing: theme.space_2 align: Align{y: 1.} Readout{text: "440.0" cells: 6 digit_height: 24.} ScreenCaption{text: "Hz"}}
            RippleMeter{value: 0.35 label: "LOAD"}
        }

        StoryHeading{text: "Rings without the backdrop"}
        StoryNote{text: "With glass_backdrop off the glass cannot read what lies under it, so the rings are light and shade on the glass alone and the digits stay put. Nothing is captured for it."}
        StoryRow{
            RippleScreen{glass_backdrop: false Readout{text: "-12.0" cells: 5 digit_height: 30.}}
        }
    }
}

pub const STORIES: &[Story] = &[
    Story {
        key: "containers/screenview/overview",
        category: "Containers",
        component: "ScreenView",
        also: &[],
        name: "Overview",
        dsl: "ScreenViewOverview",
        added: "2026-09-28",
        tags: &["instruments", "new"],
        doc: "# ScreenView\n\nA view that is a display window: a slim bezel, the screen face recessed into it with an inner shadow under its top edge, and glass over the children it holds. The children draw between the face and the glass, so the sheen lies over them.\n\n- `bezel` is the ring's width in points; `recess` how far the surround's shadow reaches down the face; `sheen` the glass's strength; `scan` the depth of scan lines, off by default.\n- The face is `color_screen`, the theme's display glass. Put `Readout`s, labels and meters inside; a caption on a screen reads best in `color_screen_ink`.\n- Everything else is a view's: `flow`, `spacing`, `padding`, `align`, `width`, `height`.\n\nThe face is the view's `draw_bg` and the glass `draw_glass`; both read `bezel` (and the face `recess` and `scan`, the glass `sheen`) as instances the widget keeps in step with its properties, so a sheet may replace either pixel function and still draw to the page's geometry. The glass is one quad drawn after the children, inside the widget's own turtle.",
        subject: "subject",
        feature: None,
        controls: &[
            Control { label: "Bezel", target: "subject", kind: ControlKind::Number { prop: "bezel", min: 0., max: 12., step: 0.5, default: 2. } },
            Control { label: "Recess", target: "subject", kind: ControlKind::Number { prop: "recess", min: 0., max: 12., step: 0.5, default: 4. } },
            Control { label: "Sheen", target: "subject", kind: ControlKind::Number { prop: "sheen", min: 0., max: 0.12, step: 0.005, default: 0.03 } },
            Control { label: "Scan lines", target: "subject", kind: ControlKind::Number { prop: "scan", min: 0., max: 0.5, step: 0.01, default: 0. } },
            Control { label: "Corner", target: "subject", kind: ControlKind::Number { prop: "draw_bg.border_radius", min: 0., max: 16., step: 0.5, default: 2. } },
        ],
        on_actions: None,
    },
    Story {
        key: "containers/screenview/ripples",
        category: "Containers",
        component: "ScreenView",
        also: &["NeedleMeter"],
        name: "Ripples",
        dsl: "ScreenViewRipples",
        added: "2026-10-10",
        tags: &["instruments", "pointer", "water", "glass", "new"],
        doc: "# Ripples

Glass that rings like water where the pointer moves across it, and bends what lies under it.

- `field_reach` above zero gives a `ScreenView`'s or a `NeedleMeter`'s glass the pointer: its field and the trail of drops it leaves as it moves, handed to `draw_glass` as instances. `pointer_field` is the field's strength, the pointer's place and a clock; `trail_0` to `trail_7` are the last eight drops, newest first, each its place in points from the glass's top left, its age in seconds and what is left of its life, 1 down to 0.
- `trail_spacing` is how far the pointer moves between drops, `trail_life` how long a drop lives. A pointer that does not move drops nothing.
- `glass_backdrop` lifts the glass over the content while the trail is alive, with the window's scene of the frame bound (`scene_texture`, `source_size`, `source_y_flip`, `has_gauss`), so its shader can read the digits or the needle under it and draw them displaced.
- At rest nothing runs: no frames, no capture, and the glass is drawn where it always is. A meter draws its glass only while `field_reach` is above zero.

The shaders on this page are the page's own, so the ripples show under every sheet: each drop sends a ring outward, the slope of the water displaces what the glass reads, and the flanks facing the light catch it.",
        subject: "subject",
        feature: None,
        controls: &[
            Control { label: "Reach", target: "subject", kind: ControlKind::Number { prop: "field_reach", min: 0., max: 24., step: 0.5, default: 2. } },
            Control { label: "Drop spacing", target: "subject", kind: ControlKind::Number { prop: "trail_spacing", min: 1., max: 40., step: 0.5, default: 6. } },
            Control { label: "Drop life (s)", target: "subject", kind: ControlKind::Number { prop: "trail_life", min: 0.2, max: 4., step: 0.05, default: 1.3 } },
        ],
        on_actions: None,
    },
];

#[cfg(test)]
mod tests {
    use super::*;

    /// The page builds, both of the screen's shaders compile, and the
    /// subject is a screen.
    #[test]
    fn the_page_builds_and_its_subject_is_a_screen() {
        let mut cx = Cx::new(Box::new(|_, _| {}));
        cx.with_vm(|vm| {
            crate::theme::widgets_script_mod(vm);
            crate::shell::script_mod(vm);
            self::script_mod(vm);
            let _ = makepad_platform::shader_error::take();
        });
        let story = &STORIES[0];
        let page = cx.with_vm(|vm| {
            let stories = vm.module(id!(stories));
            let value = vm.bx.heap.value(stories, LiveId::from_str(story.dsl).into(), NoTrap);
            assert!(value.as_object().is_some(), "no template {}", story.dsl);
            WidgetRef::script_from_value(vm, value)
        });
        assert!(!page.is_empty(), "{} built no widget", story.key);
        assert_eq!(makepad_platform::shader_error::take(), None, "a shader on the page failed to compile");
        assert!(page.widget(&cx, ids!(subject)).borrow::<ScreenView>().is_some(), "no screen at subject");
    }
}
