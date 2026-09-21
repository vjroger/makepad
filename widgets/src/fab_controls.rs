//! The "fab" control set — the fab app's control styling
//! (libs/fab/src/ui: the drag-numeric field, the color picker, the row /
//! panel / search visual language) ported into the widget library as a
//! named, reusable set. Nothing here depends on libs/fab: the code and the
//! token table are carried over and adapted; the fab app itself migrates to
//! these later.
//!
//! Registered names:
//! * `mod.fab` — the token table (surfaces, text, accents, density, type,
//!   motion), same token names the fab app uses.
//! * `mod.widgets.FabValueInput` — the drag-numeric field. Press arms, 3 px
//!   engages a drag (one step per pixel, Shift fine, Ctrl snaps, clamping
//!   shifts the anchor), a plain click opens text entry, the end zones step.
//!   `enabled: false` dims it and makes it inert.
//! * `mod.widgets.FabSlider` — a horizontal track with the name on its
//!   left and the number on its right. The press lands the thumb where the
//!   pointer is and the drag keeps it there; a click on the name takes the
//!   row back to zero.
//! * `mod.widgets.FabKnob` — the slider's number on a dial, for a cell of a
//!   matrix: 44 by 64 by default and sized by whatever box it is given, down
//!   to 28 across. Press and pull up for more (the whole range in 150 points,
//!   Shift a tenth of the speed), the wheel and the arrows step it once it
//!   has the keyboard, a double click takes it back to zero. Same fields,
//!   same actions and same ref helpers as the slider.
//! * `mod.widgets.FabDiagonalLabel` — the name over a column too narrow to
//!   hold it: a matrix header, turned 45 degrees and let out over its
//!   neighbours. `lean` picks which side it hangs over. It draws OUTSIDE its
//!   own box on purpose, so the row it stands in wants `clip_x: false`.
//! * `mod.widgets.FabColorWheel` — hue ring around a saturation/value
//!   square, pointer-captured drags, arrow-key nudges.
//! * `mod.widgets.FabColorPick` — a swatch that opens a self-managed
//!   popover (wheel + RGB rows + hex entry) anchored at the swatch;
//!   outside-click commits, Escape reverts. Publishes `Changed` live and
//!   `Ended` on commit, plus `Opened`/`Closed` for hosts that need to know.
//! * `mod.widgets.FabLabel` / `FabLabelDim` / `FabLabelSmall` /
//!   `FabHeaderLabel`, `mod.widgets.FabSearch` (input well),
//!   `mod.widgets.FabPropRow` (label-left / value-right row),
//!   `mod.widgets.FabSection` (clickable section header) — the DSL shapes
//!   panels are assembled from (the tweaker's sidebar is the first tenant).

use crate::button::ButtonAction;
use crate::widget_tree::CxWidgetExt;
use crate::{
    animator::*, makepad_derive_widget::*, makepad_draw::ime::TextInputConfig,
    makepad_draw::text::geom::Point, makepad_draw::*, text_input::*, view::View, widget::*,
};
use crate::makepad_script::script;

pub fn script_mod(vm: &mut ScriptVm) {
    // Phase 1: the token table and a prelude carrying the `fab` alias, so
    // the ported DSL below reads exactly like it does in the fab app.
    let block = script! {
        use mod.prelude.widgets_internal.*

        mod.fab = {
            // ---- surfaces (fab default-dark grade) ----
            color_area: #x303030
            color_editor: #x232323
            color_editor_alt: #x282828
            color_header: #x3d3d3d
            color_panel: #x3d3d3d
            color_panel_sub: #x353535
            color_popover: #x1a1a1a
            color_popover_border: #x545454
            color_border: #x161616
            color_border_light: #x4a4a4a
            color_row_hover: #x3a3a3a
            color_input: #x1d1d1d
            color_input_hover: #x232323
            color_input_active: #x161616
            color_button: #x545454
            color_button_hover: #x656565
            color_button_down: #x4a4a4a
            color_button_active: #x5680c2

            // ---- text ----
            color_text: #xe6e6e6
            color_text_dim: #x9a9a9a
            color_text_muted: #x707070
            color_text_active: #xffffff
            color_text_header: #xd0d0d0
            color_text_on_accent: #xffffff

            // ---- accents ----
            color_accent: #x5680c2
            color_accent_hover: #x6b93d4
            color_accent_dim: #x3c5a8a
            color_selection_bg: #x334d80
            color_focus_ring: #x7aa2e8
            color_warning: #xe0a020
            color_error: #xe04040
            color_ok: #x5cb85c

            // ---- the drag-numeric field's inset well ----
            color_num: #x1d1d1d
            color_num_hover: #x2a2a2a
            color_num_fill: #x3c5a8a
            color_num_arrow: #xb0b0b0

            // ---- density ----
            row_height: 24.0
            row_height_sm: 20.0
            header_height: 26.0
            prop_label_width: 92.0
            pad_1: 4.0
            pad_2: 6.0
            pad_3: 10.0
            // Sdf2d.box arguments — the drawn corner reads as twice these.
            radius: 2.0
            radius_lg: 3.0
            border: 1.0
            swatch_width: 46.0

            // ---- type ----
            // The kit's own face, here for the reason the palette above is
            // here. A sheet MOVES `theme.font_regular` -- `android` takes
            // Roboto, `ios` Inter -- and installing a blend takes the sheet
            // off and puts it back on, so a kit whose words came from the
            // theme changed typeface on the applied frame and changed back
            // on leave. The row heights are fab and held; the text metrics
            // did not, and every label and field on the panel reflowed under
            // the hand of whoever was dragging a weight. The Theme tab's
            // picker already carries a face of its own against exactly this
            // (`PanelFont`, tweaker.rs); the kit carries the same one, so
            // the panel and the controls in it read as one surface.
            //
            // The LATIN member only. What follows it in the family -- the
            // symbol face, and the CJK and emoji members the policy keeps
            // lazy -- is whatever was resolved for the app, because the
            // search well and the hex field take TYPED text: a panel that
            // reflows is a nuisance, a panel that cannot spell what was
            // typed into it is a dead end.
            font: theme.font_regular{
                font_family: theme.font_regular.font_family{
                    latin := FontMember{
                        res: crate_resource("self:resources/IBMPlexSans-Text.ttf")
                        asc: -0.1
                        desc: 0.0
                    }
                }
                line_spacing: 1.2
            }

            // ---- type sizes (points) ----
            font_size_ui: 8.5
            font_size_small: 7.5
            font_size_header: 9.0

            // ---- motion ----
            anim_fast: 0.10
            anim_normal: 0.15
        }
        true
    };
    vm.eval(block);

    // Phase 1b: the prelude carrying the `fab` alias, built from the table
    // above once it is final. The alias holds the table it was built from, so
    // re-pointing `mod.fab` after this would leave every control reading the
    // palette that is no longer there.
    let block = script! {
        use mod.prelude.widgets_internal.*

        mod.prelude.fab_internal = {
            ..mod.prelude.widgets_internal,
            fab: mod.fab
        }
    };
    vm.eval(block);

    // Phase 2: the controls, in the fab visual language.
    let block = script! {
        use mod.prelude.fab_internal.*
        use mod.widgets.*

        set_type_default() do #(DrawDragNum::script_shader(vm)){
            ..mod.draw.DrawQuad

            // These are `#[live]` fields on DrawDragNum, so they are already
            // instances; `instance(..)` here would hand the f32 an object.
            hover: 0.0
            down: 0.0
            focus: 0.0
            disabled: 0.0
            fill: -1.0
            flat: 0.0

            pixel: fn() {
                let sdf = Sdf2d.viewport(self.pos * self.rect_size)
                let w = self.rect_size.x
                let h = self.rect_size.y
                sdf.box(0.5, 0.5, w - 1.0, h - 1.0, fab.radius)
                let reveal = mix(1.0, max(self.hover, max(self.down, self.focus)), self.flat)
                // Off: the well sinks most of the way into the row, so a
                // field that answers nothing does not look like one that will.
                let dim = 1.0 - 0.6 * self.disabled
                let mut base = fab.color_num.mix(fab.color_num_hover, self.hover).mix(fab.color_input_active, self.down)
                base = vec4(base.xyz, base.w * reveal * dim)
                sdf.fill_keep(base)
                let mut border = fab.color_border.mix(fab.color_focus_ring, self.focus)
                border = vec4(border.xyz, border.w * reveal * dim)
                sdf.stroke(border, 1.0)
                if self.fill >= 0.0 {
                    sdf.box(1.0, 1.0, max(2.0, (w - 2.0) * self.fill), h - 2.0, fab.radius)
                    sdf.fill(vec4(fab.color_num_fill.xyz, 0.85))
                }
                // Hover arrows in the end zones; they retire while the field
                // is a text editor (focus carries the editing state) and
                // while it is off.
                if self.hover * (1.0 - self.disabled) > 0.01 {
                    if self.focus < 0.5 {
                        let cy = h * 0.5
                        let a = vec4(fab.color_num_arrow.xyz, self.hover)
                        sdf.move_to(9.0, cy - 3.5)
                        sdf.line_to(5.5, cy)
                        sdf.line_to(9.0, cy + 3.5)
                        sdf.stroke(a, 1.25)
                        sdf.move_to(w - 9.0, cy - 3.5)
                        sdf.line_to(w - 5.5, cy)
                        sdf.line_to(w - 9.0, cy + 3.5)
                        sdf.stroke(a, 1.25)
                    }
                }
                return sdf.result
            }
        }

        mod.widgets.FabValueInputBase = #(FabValueInput::register_widget(vm))
        /** The drag-numeric field: press arms, 3 px of travel starts the
         * scrub, release without travel opens keyboard editing. */
        mod.widgets.FabValueInput = set_type_default() do mod.widgets.FabValueInputBase{
            width: Fill
            height: fab.row_height
            flow: Right
            align: Align{x: 0.0 y: 0.5}
            padding: Inset{left: 8 right: 8 top: 0 bottom: 0}
            margin: Inset{top: 0 bottom: 0 left: 0 right: 0}

            label: ""
            min: 0.0
            max: 0.0
            /** scrub granularity per pixel of travel 0.001..1 step 0.001 */
            step: 0.01
            snap: 0.0
            precision: 2
            suffix: ""
            value: 0.0
            wrap: false
            show_fill: false
            quantize: false

            draw_text +: {
                ink_centered: true
                color: fab.color_text_dim
                text_overflow: TextOverflow.Ellipsis
                text_style: fab.font{
                    font_size: fab.font_size_ui
                }
            }
            text_input: TextInput{
                width: Fill
                height: Fill
                // The same trap FabSearch's `input` documents: `android`
                // and `ios` set `mod.widgets.TextInput.min_height`, a walk
                // applies it whatever height was asked for, and the value
                // would be drawn below the 18px field it belongs to. Zero
                // is what the default theme resolves to, so nothing moves.
                min_height: 0
                // Read-only display may carry a unit suffix. Editing
                // switches this back to numeric-only in Rust.
                is_numeric_only: false
                padding: Inset{left: 0 right: 0 top: 0 bottom: 0}
                margin: Inset{top: 0 bottom: 0 left: 0 right: 0}
                label_align: Align{x: 1.0 y: 0.5}
                draw_bg +: {
                    color: vec4(0.0, 0.0, 0.0, 0.0)
                    border_radius: 0.0
                }
                draw_text +: {
                    ink_centered: true
                    color: fab.color_text
                    text_style: fab.font{
                        font_size: fab.font_size_ui
                    }
                }
            }
            animator: Animator{
                hover: {
                    default: @off
                    off: AnimatorState{
                        from: {all: Forward {duration: fab.anim_fast}}
                        apply: { draw_bg: {hover: 0.0, down: 0.0} }
                    }
                    on: AnimatorState{
                        from: {all: Snap}
                        apply: { draw_bg: {hover: 1.0, down: 0.0} }
                    }
                    down: AnimatorState{
                        from: {all: Snap}
                        apply: { draw_bg: {hover: 1.0, down: 1.0} }
                    }
                }
                focus: {
                    default: @off
                    off: AnimatorState{
                        from: {all: Forward {duration: fab.anim_fast}}
                        apply: { draw_bg: {focus: 0.0} }
                    }
                    on: AnimatorState{
                        from: {all: Snap}
                        apply: { draw_bg: {focus: 1.0} }
                    }
                }
            }
        }

        set_type_default() do #(DrawFabSlider::script_shader(vm)){
            ..mod.draw.DrawQuad

            // `#[live]` fields on DrawFabSlider, so they are already
            // instances — see DrawDragNum above.
            hover: 0.0
            down: 0.0
            focus: 0.0
            disabled: 0.0
            travel: 0.0
            label_px: 0.0
            readout_px: 0.0
            thumb_px: 12.0
            inset_px: 2.0

            pixel: fn() {
                let sdf = Sdf2d.viewport(self.pos * self.rect_size)
                let h = self.rect_size.y
                // The track is whatever the name and the number leave of the
                // row. Rust hands over the same two column widths its hit
                // test measures from, so the thumb is drawn where it can be
                // taken hold of.
                let x0 = self.label_px
                let x1 = max(x0 + 2.0, self.rect_size.x - self.readout_px)
                let w = x1 - x0
                let dim = 1.0 - 0.6 * self.disabled
                let cy = h * 0.5
                let well = max(4.0, h - 10.0)

                sdf.box(x0 + 0.5, cy - well * 0.5, w - 1.0, well, fab.radius)
                let mut base = fab.color_num.mix(fab.color_num_hover, self.hover)
                base = vec4(base.xyz, base.w * dim)
                sdf.fill_keep(base)
                let mut border = fab.color_border.mix(fab.color_focus_ring, self.focus)
                border = vec4(border.xyz, border.w * dim)
                sdf.stroke(border, 1.0)

                // The thumb's centre never leaves the well, so what it runs
                // over is the well less the inset at both ends and its own
                // width — the three numbers `SliderTravel` measures with.
                let span = max(1.0, w - self.inset_px * 2.0 - self.thumb_px)
                let tc = x0 + self.inset_px + self.thumb_px * 0.5 + self.travel * span
                sdf.box(x0 + 1.0, cy - well * 0.5 + 1.0, max(1.0, tc - x0 - 1.0), well - 2.0, fab.radius)
                sdf.fill(vec4(fab.color_num_fill.xyz, 0.85 * dim))

                sdf.box(tc - self.thumb_px * 0.5, 2.0, self.thumb_px, h - 4.0, fab.radius)
                let mut face = fab.color_button.mix(fab.color_button_hover, self.hover).mix(fab.color_button_down, self.down)
                face = vec4(face.xyz, face.w * dim)
                sdf.fill_keep(face)
                sdf.stroke(vec4(fab.color_border.xyz, fab.color_border.w * dim), 1.0)
                return sdf.result
            }
        }

        mod.widgets.FabSliderBase = #(FabSlider::register_widget(vm))
        /** The track: a press anywhere on it lands the thumb under the
         * pointer and the drag keeps it there, a click on the name takes the
         * row back to zero. */
        mod.widgets.FabSlider = set_type_default() do mod.widgets.FabSliderBase{
            width: Fill
            height: fab.row_height
            flow: Right
            align: Align{x: 0.0 y: 0.5}
            padding: Inset{left: 8 right: 6 top: 0 bottom: 0}
            margin: Inset{top: 0 bottom: 0 left: 0 right: 0}
            // No spacing: the three columns are measured rather than spaced.
            // The hit test derives the track from the two column widths, and
            // a gap the turtle put in is a gap it cannot see.
            spacing: 0

            label: ""
            label_width: fab.prop_label_width
            readout_width: 34.0
            min: 0.0
            max: 100.0
            /** the arrow-key increment, and the detent a drag lands on 0..25 step 0.5 */
            step: 1.0
            /** the shift+arrow increment 0..50 step 0.5 */
            big_step: 10.0
            precision: 0
            unit: "%"
            value: 0.0
            thumb_size: 12.0
            track_inset: 2.0
            enabled: true

            draw_label +: {
                ink_centered: true
                color: fab.color_text_dim
                text_overflow: TextOverflow.Ellipsis
                text_style: fab.font{
                    font_size: fab.font_size_ui
                }
            }
            draw_value +: {
                ink_centered: true
                color: fab.color_text
                text_style: fab.font{
                    font_size: fab.font_size_ui
                }
            }
        }

        set_type_default() do #(DrawFabKnob::script_shader(vm)){
            ..mod.draw.DrawQuad

            // `#[live]` fields on DrawFabKnob, so they are already
            // instances — see DrawDragNum above.
            hover: 0.0
            down: 0.0
            focus: 0.0
            disabled: 0.0
            travel: 0.0
            label_px: 0.0
            readout_px: 0.0

            pixel: fn() {
                let sdf = Sdf2d.viewport(self.pos * self.rect_size)
                // Where the face stands in the box: the three lines of
                // `knob_face`, to the letter. Rust lays the name and the
                // number out round the same arithmetic, so the words land
                // above and below the dial and never on it.
                let rows = self.label_px + self.readout_px
                let d = max(min(self.rect_size.x, self.rect_size.y - rows), 0.0)
                let top = max((self.rect_size.y - rows - d) * 0.5, 0.0)
                let c = vec2(self.rect_size.x * 0.5, top + self.label_px + d * 0.5)
                let r = d * 0.5
                let dim = 1.0 - 0.6 * self.disabled

                // Every thickness is a share of the radius over a floor in
                // pixels. The shares are what a 44 face is drawn with; the
                // floors are what a 28 face is drawn with, which is the size
                // a matrix cell comes down to, and under them a groove stops
                // being a groove and becomes a grey smear.
                let groove = max(r * 0.18, 2.5)
                let ring_r = max(r - 1.0 - groove * 0.5, 1.0)
                let cap_r = max(ring_r - groove * 0.5 - max(r * 0.1, 1.5), 1.0)

                // Sdf2d measures an arc from straight DOWN and turns
                // clockwise, so an eighth of a turn in is half past seven
                // and three quarters of a turn on from there is half past
                // four: the opening sits at the bottom, where a hand expects
                // the two stops of a dial to be.
                let start = PI * 0.25
                let sweep = PI * 1.5
                let at = start + sweep * self.travel

                // NOUGHT IS OFF, and it has to read as off from across a
                // matrix in which most of the cells are at nought. So the
                // lit arc is not drawn at all there -- an arc of no length
                // still draws its round cap, which is a lamp on the stop and
                // says "a little" rather than "nothing" -- and the tick goes
                // down to the muted ink. Anything above nought is on, however
                // little of it there is: the lamp on the stop is its first
                // sign.
                let lit = step(0.0005, self.travel)
                let lifted = max(self.hover, self.down)

                // The groove does NOT lift under the pointer, where the
                // slider's well does. A well is a slab with a border round
                // it; a groove is a line on the panel's own ground, and the
                // well's hover tone is within a shade of that ground -- the
                // unlit half of the dial went out at the moment a hand
                // arrived to turn it. The cap and the arc say hover instead.
                sdf.arc_round_caps(c.x, c.y, ring_r, start, start + sweep, groove)
                sdf.fill(vec4(fab.color_num.xyz, fab.color_num.w * dim))

                if lit > 0.5 {
                    sdf.arc_round_caps(c.x, c.y, ring_r, start, at, groove)
                    let accent = fab.color_accent.mix(fab.color_accent_hover, lifted)
                    sdf.fill(vec4(accent.xyz, accent.w * dim))
                }

                // The cap is the button face of the rest of the kit, and it
                // answers the hand the way the slider's thumb does. Its edge
                // is where the keyboard shows: the same ring the wells wear.
                sdf.circle(c.x, c.y, cap_r)
                let mut face = fab.color_button.mix(fab.color_button_hover, self.hover).mix(fab.color_button_down, self.down)
                face = vec4(face.xyz, face.w * dim)
                sdf.fill_keep(face)
                let mut edge = fab.color_border.mix(fab.color_focus_ring, self.focus)
                edge = vec4(edge.xyz, edge.w * dim)
                sdf.stroke(edge, 1.0 + 0.5 * self.focus)

                // The tick points at the value's own angle, from a third of
                // the way out to just short of the cap's edge. The arc's
                // round cap un-rotates to centre + radius * (-sin, cos), so
                // the tick and the head of the lit arc share one bearing.
                let dir = vec2(0.0 - sin(at), cos(at))
                let heel = cap_r * 0.3
                let tip = max(cap_r - 2.0, heel + 1.0)
                sdf.move_to(c.x + dir.x * heel, c.y + dir.y * heel)
                sdf.line_to(c.x + dir.x * tip, c.y + dir.y * tip)
                let mut ink = fab.color_text_muted.mix(fab.color_text, lit)
                ink = ink.mix(fab.color_text_active, lifted * lit)
                sdf.stroke(vec4(ink.xyz, ink.w * dim), max(r * 0.07, 1.0))
                return sdf.result
            }
        }

        mod.widgets.FabKnobBase = #(FabKnob::register_widget(vm))
        /** The dial: press it and pull up for more or down for less, the
         * whole range in a hand's width of travel; a double click takes it
         * back to nought. Sized by the cell it is put in, down to 28 wide. */
        mod.widgets.FabKnob = set_type_default() do mod.widgets.FabKnobBase{
            // 44 across and 64 down: a 44 face, the number under it, and the
            // slack shared above and below. Both are only a default. The face
            // is the biggest circle the box holds once the two text rows are
            // taken off its height, so a cell hands over whatever it has --
            // fixed or Fill, either way -- and the dial fits itself to it.
            width: 44
            height: 64
            flow: Down
            // Written out, and nought: the face is measured off the WHOLE
            // box, by the shader and by the layout alike, so padding here
            // would move the words and leave the dial where it was. A cell
            // that wants air round its knob asks for it with a margin.
            padding: Inset{left: 0 right: 0 top: 0 bottom: 0}
            margin: Inset{top: 0 bottom: 0 left: 0 right: 0}
            spacing: 0

            // No name by default, and then no row for one either: in a
            // matrix the column and the row headers carry the names, and
            // twelve pixels of nothing over every face is a row of knobs
            // fewer on the panel.
            label: ""
            label_height: 12.0
            show_readout: true
            readout_height: 12.0
            min: 0.0
            max: 100.0
            /** the arrow-key and wheel increment, and the detent a drag lands on 0..25 step 0.5 */
            step: 1.0
            /** the shift+arrow and shift+wheel increment 0..50 step 0.5 */
            big_step: 10.0
            precision: 0
            unit: "%"
            value: 0.0
            /** how far the pointer travels for the whole range, in points 40..400 step 10 */
            drag_travel: 150.0
            wheel_on_hover: false
            enabled: true

            draw_label +: {
                ink_centered: true
                color: fab.color_text_dim
                text_overflow: TextOverflow.Ellipsis
                text_style: fab.font{
                    font_size: fab.font_size_small
                }
            }
            draw_value +: {
                ink_centered: true
                color: fab.color_text
                text_style: fab.font{
                    font_size: fab.font_size_small
                }
            }
            // The number under a knob standing at nought. A second ink and
            // not a second size, so a column of cells keeps one baseline and
            // only the cells that count for something are lit.
            draw_value_off +: {
                ink_centered: true
                color: fab.color_text_muted
                text_style: fab.font{
                    font_size: fab.font_size_small
                }
            }
        }

        set_type_default() do #(DrawColorWheel::script_shader(vm)){
            ..mod.draw.DrawQuad

            hue: 0.0
            sat: 0.0
            val: 0.0

            pixel: fn() {
                let size = min(self.rect_size.x, self.rect_size.y)
                let sdf = Sdf2d.viewport(self.pos * self.rect_size)
                let c = self.rect_size * 0.5
                let dx = self.pos.x * self.rect_size.x - c.x
                let dy = self.pos.y * self.rect_size.y - c.y

                let outer = size * 0.48
                let inner = size * 0.385
                let half = size * 0.255

                // Hue ring: 0 at twelve o'clock, clockwise, red at the top.
                sdf.circle(c.x, c.y, outer)
                sdf.circle(c.x, c.y, inner)
                sdf.subtract()
                let ang = atan2(dx, 0.0 - dy)
                let hue_at = fract(ang / 6.2831853 + 1.0)
                sdf.fill(Pal.hsv2rgb(vec4(hue_at, 1.0, 1.0, 1.0)))

                // Saturation/value square at the current hue.
                let sq_s = clamp((dx + half) / (2.0 * half), 0.0, 1.0)
                let sq_v = 1.0 - clamp((dy + half) / (2.0 * half), 0.0, 1.0)
                sdf.rect(c.x - half, c.y - half, half * 2.0, half * 2.0)
                sdf.fill(Pal.hsv2rgb(vec4(self.hue, sq_s, sq_v, 1.0)))

                // Pucks: a dark outline with a light ring inside stays
                // visible over any colour underneath.
                let mid = (outer + inner) * 0.5
                let pa = self.hue * 6.2831853
                let rp = vec2(c.x + sin(pa) * mid, c.y - cos(pa) * mid)
                sdf.circle(rp.x, rp.y, 6.5)
                sdf.stroke(vec4(0.04, 0.04, 0.04, 0.9), 1.4)
                sdf.circle(rp.x, rp.y, 5.0)
                sdf.stroke(vec4(1.0, 1.0, 1.0, 0.95), 1.6)

                let sp = vec2(
                    c.x - half + self.sat * 2.0 * half,
                    c.y - half + (1.0 - self.val) * 2.0 * half
                )
                sdf.circle(sp.x, sp.y, 6.0)
                sdf.stroke(vec4(0.04, 0.04, 0.04, 0.9), 1.4)
                sdf.circle(sp.x, sp.y, 4.5)
                sdf.stroke(vec4(1.0, 1.0, 1.0, 0.95), 1.6)

                return sdf.result
            }
        }

        mod.widgets.FabColorWheelBase = #(FabColorWheel::register_widget(vm))
        mod.widgets.FabColorWheel = set_type_default() do mod.widgets.FabColorWheelBase{
            width: 220
            height: 220
        }

        set_type_default() do #(DrawFabSwatch::script_shader(vm)){
            ..mod.draw.DrawQuad
            hover: 0.0
            open: 0.0
            pixel: fn() {
                let sdf = Sdf2d.viewport(self.pos * self.rect_size)
                sdf.box(0.5, 0.5, self.rect_size.x - 1.0, self.rect_size.y - 1.0, fab.radius)
                sdf.fill_keep(vec4(self.swatch.xyz, 1.0))
                let ring = fab.color_border.mix(fab.color_focus_ring, max(self.hover, self.open))
                sdf.stroke(ring, 1.0)
                return sdf.result
            }
        }

        // ---- type ----
        // The stock `Label` carries padding that overflows a 20 px fab row;
        // zero padding and centred ink keep every label on the row's line.
        mod.widgets.FabLabel = Label{
            width: Fit
            height: Fit
            padding: Inset{left: 0 right: 0 top: 0 bottom: 0}
            draw_text +: {
                ink_centered: true
                color: fab.color_text
                text_style: fab.font{
                    font_size: fab.font_size_ui
                }
            }
        }
        mod.widgets.FabLabelDim = mod.widgets.FabLabel{
            draw_text +: {
                color: fab.color_text_dim
            }
        }
        mod.widgets.FabLabelSmall = mod.widgets.FabLabel{
            draw_text +: {
                color: fab.color_text_dim
                text_style: fab.font{
                    font_size: fab.font_size_small
                }
            }
        }
        mod.widgets.FabHeaderLabel = mod.widgets.FabLabel{
            draw_text +: {
                color: fab.color_text_header
                text_style: fab.font{
                    font_size: fab.font_size_header
                }
            }
        }

        // The name over a column that cannot hold it flat. Kept here among
        // the labels because that is what it is; what makes it its own
        // control is that the ink is MEANT to leave the box.
        let DiagonalLean = set_type_default() do #(DiagonalLean::script_api(vm))
        mod.widgets.DiagonalLean = DiagonalLean
        mod.widgets.FabDiagonalLabelBase = #(FabDiagonalLabel::register_widget(vm))
        /** A name written across the corner of the box it names, for a
         * column too narrow to hold it flat: a matrix header. The ink
         * overflows its own box on purpose, so the row these stand in wants
         * `clip_x: false` and room at the end they lean over. */
        mod.widgets.FabDiagonalLabel = set_type_default() do mod.widgets.FabDiagonalLabelBase{
            // Fill so a header row divides itself between its columns the
            // way the grid under it does. 60 down is measured and not
            // guessed: the longest theme name the library ships takes 56.5
            // points of height at 45 degrees in the panel's small face, and
            // a host that knows its own longest name should fix this for
            // itself with `diagonal_row_height`.
            width: Fill
            height: 60
            text: ""
            /** how far from the horizontal the name is turned, in degrees 0..90 step 5 */
            angle: 45.0
            /** Fall hangs the name over the LEFT, Rise over the right */
            lean: DiagonalLean.Fall

            draw_text +: {
                color: fab.color_text_dim
                text_style: fab.font{
                    font_size: fab.font_size_small
                }
            }
        }

        // ---- the search well ----
        mod.widgets.FabSearch = View{
            width: Fill
            height: fab.row_height
            flow: Right
            align: Align{x: 0.0 y: 0.5}
            padding: Inset{left: 6 right: 4 top: 0 bottom: 0}
            show_bg: true
            draw_bg +: {
                pixel: fn() {
                    let sdf = Sdf2d.viewport(self.pos * self.rect_size)
                    sdf.box(0.5, 0.5, self.rect_size.x - 1.0, self.rect_size.y - 1.0, fab.radius)
                    sdf.fill_keep(fab.color_input)
                    sdf.stroke(fab.color_border, 1.0)
                    return sdf.result
                }
            }
            input := TextInput{
                width: Fill
                height: Fill
                // The field fills a row of a FIXED height, so nothing here
                // may impose a height of its own. `android` and `ios` set
                // `mod.widgets.TextInput.min_height` to 48 and 44, and a
                // walk applies a min height UNCONDITIONALLY -- the height
                // asked for cannot escape it (draw/src/turtle.rs, where
                // `walk.min_height` is resolved). The field's content box
                // then stood 48 tall inside a 24 tall well, and a
                // single-line input CENTRES its line box in that box
                // (`TextInput::scroll_to_cursor`), so the word "Filter" was
                // drawn 12px below where the well ends: sunk to the bottom
                // of the box, straddling the lower border. Zero is what the
                // default theme resolves to anyway, so this moves nothing
                // that is on screen today.
                min_height: 0
                padding: Inset{left: 0 right: 0 top: 0 bottom: 0}
                margin: Inset{top: 0 bottom: 0 left: 0 right: 0}
                empty_text: "Filter"
                draw_bg +: {
                    color: vec4(0.0, 0.0, 0.0, 0.0)
                    color_hover: vec4(0.0, 0.0, 0.0, 0.0)
                    color_focus: vec4(0.0, 0.0, 0.0, 0.0)
                    color_down: vec4(0.0, 0.0, 0.0, 0.0)
                    color_empty: vec4(0.0, 0.0, 0.0, 0.0)
                    border_size: 0.0
                    border_radius: 0.0
                    // The field has no ground of its own: the well around it
                    // is this FabSearch View's `draw_bg`, drawn in the fab
                    // palette. Written out because `windows-2000` and
                    // `nextstep` REPLACE `mod.widgets.TextInput.draw_bg.pixel`
                    // outright with a hard-coded opaque white Win95 field,
                    // which ignores every colour declared above and would
                    // paint a white slab over the well -- leaving the
                    // panel's own light grey text on white. With the colours
                    // above all transparent and no border, this is exactly
                    // what the stock face already resolves to today.
                    pixel: fn() {
                        return vec4(0.0, 0.0, 0.0, 0.0)
                    }
                }
                draw_text +: {
                    ink_centered: true
                    color: fab.color_text
                    // Every state, not just the resting one: the stock field takes the
                    // app theme's ink for hover, focus and down, and under a light theme
                    // that ink is dark -- so the word being typed went black on this
                    // panel's dark well the moment the box took focus.
                    color_hover: fab.color_text_active
                    color_focus: fab.color_text_active
                    color_down: fab.color_text_active
                    color_disabled: fab.color_text_muted
                    color_empty: fab.color_text_muted
                    color_empty_hover: fab.color_text_dim
                    color_empty_focus: fab.color_text_dim
                    text_style: fab.font{
                        font_size: fab.font_size_ui
                    }
                }
            }
        }

        // ---- label-left / value-right row ----
        mod.widgets.FabPropRow = View{
            width: Fill
            height: fab.row_height
            flow: Right
            align: Align{x: 0.0 y: 0.5}
            padding: Inset{left: 8 right: 6 top: 0 bottom: 0}
            spacing: 6
            name := mod.widgets.FabLabelDim{
                width: fab.prop_label_width
                text: "Name"
                max_lines: 1
                text_overflow: TextOverflow.Ellipsis
            }
        }

        // ---- clickable section header (text chevron; icons stay SVG-only
        // elsewhere, a fold glyph is text) ----
        mod.widgets.FabSection = View{
            width: Fill
            height: 22
            flow: Right
            align: Align{x: 0.0 y: 0.5}
            padding: Inset{left: 6 right: 6 top: 0 bottom: 0}
            spacing: 4
            cursor: MouseCursor.Hand
            show_bg: true
            draw_bg +: {
                hover: instance(0.0)
                pixel: fn() {
                    let sdf = Sdf2d.viewport(self.pos * self.rect_size)
                    sdf.box(0.5, 0.5, self.rect_size.x - 1.0, self.rect_size.y - 1.0, fab.radius)
                    sdf.fill(fab.color_panel.mix(fab.color_button_hover, self.hover * 0.5))
                    return sdf.result
                }
            }
            title := mod.widgets.FabHeaderLabel{ text: "Section" }
        }

        // ---- theme palette strip (in the colour popover) ----
        set_type_default() do #(DrawFabPaletteCell::script_shader(vm)){
            ..mod.draw.DrawQuad
            // `#[live]` fields on DrawFabPaletteCell, so they are already
            // instances — see DrawDragNum above: `instance(..)` here hands a
            // Vec4f (and two f32s) an object, and every app that loads these
            // widgets says so in three lines at startup.
            cell: vec4(0.0, 0.0, 0.0, 1.0)
            hot: 0.0
            cur: 0.0
            pixel: fn() {
                let sdf = Sdf2d.viewport(self.pos * self.rect_size)
                sdf.box(0.5, 0.5, self.rect_size.x - 1.0, self.rect_size.y - 1.0, 2.0)
                // A checker under the colour so translucent entries read as such.
                let cx = floor(self.pos.x * self.rect_size.x / 4.0)
                let cy = floor(self.pos.y * self.rect_size.y / 4.0)
                let ch = modf(cx + cy, 2.0)
                let back = vec3(0.22, 0.22, 0.22).mix(vec3(0.34, 0.34, 0.34), ch)
                let rgb = back.mix(self.cell.xyz, self.cell.w)
                sdf.fill_keep(vec4(rgb, 1.0))
                let ring = fab.color_border.mix(fab.color_focus_ring, max(self.hot, self.cur))
                sdf.stroke(ring, 1.0)
                return sdf.result
            }
        }
        mod.widgets.FabPaletteStripBase = #(FabPaletteStrip::register_widget(vm))
        mod.widgets.FabPaletteStrip = set_type_default() do mod.widgets.FabPaletteStripBase{
            width: Fill
            height: Fit
            cell_size: 12.0
            gap: 2.0
        }

        // ---- one whole palette, pressable ----
        set_type_default() do #(DrawFabPaletteChip::script_shader(vm)){
            ..mod.draw.DrawQuad
            band_0: vec4(0.0, 0.0, 0.0, 1.0)
            band_1: vec4(0.0, 0.0, 0.0, 1.0)
            band_2: vec4(0.0, 0.0, 0.0, 1.0)
            band_3: vec4(0.0, 0.0, 0.0, 1.0)
            hover: 0.0
            cur: 0.0
            pixel: fn() {
                let sdf = Sdf2d.viewport(self.pos * self.rect_size)
                sdf.box(0.5, 0.5, self.rect_size.x - 1.0, self.rect_size.y - 1.0, 2.0)
                // The band under this fragment, chosen by three steps rather
                // than by branching: a chip is small and every fragment of it
                // takes this path.
                let y = self.pos.y
                let mut band = self.band_0
                band = band.mix(self.band_1, step(0.25, y))
                band = band.mix(self.band_2, step(0.5, y))
                band = band.mix(self.band_3, step(0.75, y))
                sdf.fill_keep(vec4(band.xyz, 1.0))
                // The ring is how the chip in force is told from the rest,
                // so it thickens as well as lights: a row of chips at this
                // size is mostly colour, and a hue alone does not carry a
                // one-pixel difference in an edge.
                let ring = fab.color_border.mix(fab.color_focus_ring, max(self.hover, self.cur))
                sdf.stroke(ring, 1.0 + self.cur)
                return sdf.result
            }
        }
        mod.widgets.FabPaletteChipBase = #(FabPaletteChip::register_widget(vm))
        /** A whole palette in one pressable block: four colours stacked, the
         * first on top, outlined while it is the one in force. */
        mod.widgets.FabPaletteChip = set_type_default() do mod.widgets.FabPaletteChipBase{
            width: Fill
            height: 36
        }

        mod.widgets.FabColorPickBase = #(FabColorPick::register_widget(vm))
        mod.widgets.FabColorPick = set_type_default() do mod.widgets.FabColorPickBase{
            width: fab.swatch_width
            height: 16
            with_alpha: true
            popover: View{
                width: 244
                height: Fit
                flow: Down
                padding: 8
                spacing: 6
                show_bg: true
                draw_bg +: {
                    pixel: fn() {
                        let sdf = Sdf2d.viewport(self.pos * self.rect_size)
                        sdf.box(0.5, 0.5, self.rect_size.x - 1.0, self.rect_size.y - 1.0, fab.radius_lg)
                        sdf.fill_keep(fab.color_popover)
                        sdf.stroke(fab.color_popover_border, 1.0)
                        return sdf.result
                    }
                }
                wheel := mod.widgets.FabColorWheel{
                    width: 228
                    height: 228
                }
                num_r := mod.widgets.FabValueInput{ label: "R" min: 0.0 max: 255.0 step: 1.0 precision: 0 show_fill: true quantize: true }
                num_g := mod.widgets.FabValueInput{ label: "G" min: 0.0 max: 255.0 step: 1.0 precision: 0 show_fill: true quantize: true }
                num_b := mod.widgets.FabValueInput{ label: "B" min: 0.0 max: 255.0 step: 1.0 precision: 0 show_fill: true quantize: true }
                num_a := mod.widgets.FabValueInput{ label: "A" min: 0.0 max: 255.0 step: 1.0 precision: 0 show_fill: true quantize: true }
                hex_row := View{
                    width: Fill
                    height: fab.row_height
                    flow: Right
                    align: Align{x: 0.0 y: 0.5}
                    spacing: 6
                    mod.widgets.FabLabelDim{ width: 30 text: "Hex" }
                    pick := mod.widgets.Button{
                        width: Fit
                        height: Fill
                        // `android` and `ios` set `mod.widgets.Button.min_height`
                        // to 48 and 44; a walk applies it whatever height the
                        // instance asked for, so this row would stand twice
                        // its height inside a popover sized for one.
                        min_height: 0
                        padding: Inset{left: 6 right: 6 top: 2 bottom: 2}
                        text: "pick"
                    }
                    hex := TextInput{
                        width: Fill
                        height: Fill
                        min_height: 0
                        empty_text: ""
                        draw_bg +: {
                            color: fab.color_input
                            border_radius: fab.radius
                        }
                        draw_text +: {
                            color: fab.color_text
                            ink_centered: true
                            text_style: fab.font{ font_size: fab.font_size_ui }
                        }
                    }
                }
                // The host's theme palette: hover names (and pulses) a
                // colour, a click binds the property to it by reference.
                palette_name := mod.widgets.FabLabelDim{ width: Fill text: "" }
                palette := mod.widgets.FabPaletteStrip{}
            }
        }
    };
    vm.eval(block);
}

// ===========================================================================
// Shared pure helpers (color space, hex, wheel geometry)
// ===========================================================================

/// HSV → RGB, all channels 0..1. `h` wraps.
pub fn hsv_to_rgb(h: f32, s: f32, v: f32) -> [f32; 3] {
    let h = (h.rem_euclid(1.0)) * 6.0;
    let i = h.floor();
    let f = h - i;
    let p = v * (1.0 - s);
    let q = v * (1.0 - s * f);
    let t = v * (1.0 - s * (1.0 - f));
    match i as i32 % 6 {
        0 => [v, t, p],
        1 => [q, v, p],
        2 => [p, v, t],
        3 => [p, q, v],
        4 => [t, p, v],
        _ => [v, p, q],
    }
}

/// RGB → HSV, all channels 0..1. A grey keeps hue 0 and sat 0.
pub fn rgb_to_hsv(r: f32, g: f32, b: f32) -> [f32; 3] {
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let d = max - min;
    let v = max;
    let s = if max > 0.0 { d / max } else { 0.0 };
    let h = if d <= 0.0 {
        0.0
    } else if (max - r).abs() < f32::EPSILON {
        ((g - b) / d).rem_euclid(6.0) / 6.0
    } else if (max - g).abs() < f32::EPSILON {
        ((b - r) / d + 2.0) / 6.0
    } else {
        ((r - g) / d + 4.0) / 6.0
    };
    [h, s, v]
}

/// Accepts `#rgb`, `#rrggbb`, `#rrggbbaa`, each with or without the hash.
/// Returns the colour and whether the string carried alpha.
pub fn parse_hex(text: &str) -> Option<([f32; 4], bool)> {
    let t = text.trim().trim_start_matches('#');
    if !t.chars().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    let nib = |c: u8| -> f32 {
        let d = (c as char).to_digit(16).unwrap_or(0) as f32;
        d / 15.0
    };
    let byte = |hi: u8, lo: u8| -> f32 {
        let h = (hi as char).to_digit(16).unwrap_or(0);
        let l = (lo as char).to_digit(16).unwrap_or(0);
        ((h * 16 + l) as f32) / 255.0
    };
    let b = t.as_bytes();
    match b.len() {
        3 => Some(([nib(b[0]), nib(b[1]), nib(b[2]), 1.0], false)),
        6 => Some((
            [byte(b[0], b[1]), byte(b[2], b[3]), byte(b[4], b[5]), 1.0],
            false,
        )),
        8 => Some((
            [
                byte(b[0], b[1]),
                byte(b[2], b[3]),
                byte(b[4], b[5]),
                byte(b[6], b[7]),
            ],
            true,
        )),
        _ => None,
    }
}

/// `#rrggbb`, or `#rrggbbaa` when `with_alpha`.
pub fn format_hex(rgba: [f32; 4], with_alpha: bool) -> String {
    let b = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() as u32;
    if with_alpha {
        format!(
            "#{:02x}{:02x}{:02x}{:02x}",
            b(rgba[0]),
            b(rgba[1]),
            b(rgba[2]),
            b(rgba[3])
        )
    } else {
        format!("#{:02x}{:02x}{:02x}", b(rgba[0]), b(rgba[1]), b(rgba[2]))
    }
}

/// Ring outer radius as a fraction of the widget size (the shader uses the
/// same constants, so hit testing and pixels never disagree).
pub const RING_OUTER: f64 = 0.48;
/// Ring inner radius as a fraction of the widget size.
pub const RING_INNER: f64 = 0.385;
/// Half-side of the SV square as a fraction of the widget size.
pub const SQUARE_HALF: f64 = 0.255;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WheelZone {
    Ring,
    Square,
    None,
}

/// Which zone a pointer at `rel` (widget-local, origin top-left) lands in,
/// for a wheel drawn at `size` (its smaller dimension).
pub fn wheel_zone(rel: DVec2, size: f64) -> WheelZone {
    let dx = rel.x - size * 0.5;
    let dy = rel.y - size * 0.5;
    let half = SQUARE_HALF * size;
    if dx.abs() <= half && dy.abs() <= half {
        return WheelZone::Square;
    }
    let r = (dx * dx + dy * dy).sqrt();
    if r <= RING_OUTER * size + 4.0 && r >= RING_INNER * size - 4.0 {
        return WheelZone::Ring;
    }
    WheelZone::None
}

/// Hue (0..1) for a pointer on the ring: 0 at twelve o'clock, increasing
/// clockwise, red at the top.
pub fn ring_hue(rel: DVec2, size: f64) -> f32 {
    let dx = rel.x - size * 0.5;
    let dy = rel.y - size * 0.5;
    let ang = dx.atan2(-dy);
    ((ang / std::f64::consts::TAU).rem_euclid(1.0)) as f32
}

/// (saturation, value) for a pointer over the SV square, clamped so a drag
/// that leaves the square keeps tracking the nearest edge.
pub fn square_sv(rel: DVec2, size: f64) -> (f32, f32) {
    let half = SQUARE_HALF * size;
    let cx = size * 0.5;
    let s = ((rel.x - (cx - half)) / (half * 2.0)).clamp(0.0, 1.0);
    let v = 1.0 - ((rel.y - (cx - half)) / (half * 2.0)).clamp(0.0, 1.0);
    (s as f32, v as f32)
}

// ===========================================================================
// FabValueInput — the drag-numeric field. The pure drag core carries every
// mapping decision, no Cx anywhere.
// ===========================================================================

#[derive(Script, ScriptHook)]
#[repr(C)]
pub struct DrawDragNum {
    #[deref]
    draw_super: DrawQuad,
    #[live]
    hover: f32,
    #[live]
    down: f32,
    #[live]
    focus: f32,
    #[live]
    disabled: f32,
    #[live]
    fill: f32,
    /// Hide the idle chip; hover/down/focus still reveal the editor surface.
    #[live]
    flat: f32,
}

#[derive(Clone, Debug, Default)]
pub enum FabValueInputAction {
    /// Live while dragging or after a typed entry.
    Changed(f64),
    /// The gesture finished (mouse up / Enter) — commit points.
    Ended(f64),
    /// Double-click: the host should reset this field's prop to its
    /// baseline and drop it from any change ledger.
    Reset,
    #[default]
    None,
}

/// The numeric contract one field carries into a drag.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DragParams {
    pub min: f64,
    pub max: f64,
    /// One arrow-click / wheel-step increment.
    pub step: f64,
    /// Cyclic: the value comes round at the ends instead of clamping.
    pub wrap: bool,
    /// Bounded mapping: the field's width sweeps the whole range.
    pub bounded: bool,
    /// Explicit Ctrl-snap increment; `0` picks a rung from the range.
    pub snap_override: f64,
}

impl DragParams {
    pub fn range(&self) -> f64 {
        self.max - self.min
    }
    fn has_range(&self) -> bool {
        self.max > self.min
    }
}

/// Where a drag measures from. Clamping and modifier changes move the
/// anchor rather than the value.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DragAnchor {
    pub x: f64,
    pub value: f64,
}

/// A press engages into a drag only past this much horizontal travel;
/// below it, the release is a click.
pub const DRAG_THRESHOLD: f64 = 3.0;

/// Value change per pixel for the current mapping and modifiers.
/// Bounded: the range across `width`, ×0.05 fine. Unbounded: one step per
/// pixel — the drag is the coarse gesture, Shift (×0.1) the fine one.
/// How many field-widths of travel a bounded scrub takes to cross its whole
/// range.
///
/// One was the obvious mapping and the wrong one: the pointer moving with
/// the value 1:1 across a 48-point field means the entire range passes under
/// a thumb's width of movement, and nothing in between can be landed on.
/// Four gives the hand somewhere to go.
const DRAG_RANGE_TRAVEL: f64 = 4.0;

pub fn drag_rate(p: &DragParams, width: f64, shift: bool) -> f64 {
    if p.bounded && p.has_range() {
        let rate = p.range() / (width.max(1.0) * DRAG_RANGE_TRAVEL);
        if shift {
            rate * 0.05
        } else {
            rate
        }
    } else {
        let rate = p.step;
        if shift {
            rate * 0.1
        } else {
            rate
        }
    }
}

/// The Ctrl-snap increment: an explicit override wins, otherwise a rung
/// sized to the range, and Ctrl+Shift takes the next finer rung.
pub fn snap_increment(p: &DragParams, fine: bool) -> f64 {
    let base = if p.snap_override > 0.0 {
        p.snap_override
    } else {
        let range = if p.has_range() { p.range() } else { 21.0 };
        if range < 2.1 {
            0.1
        } else if range < 21.0 {
            1.0
        } else {
            10.0
        }
    };
    if fine {
        base * 0.1
    } else {
        base
    }
}

/// One step of the drag mapping: pointer at `x`, modifiers as held right
/// now. Returns the value to publish and the anchor to carry forward
/// (shifted when a limit was hit). Both ends stay reachable under snap.
pub fn drag_map(
    p: &DragParams,
    anchor: DragAnchor,
    x: f64,
    width: f64,
    shift: bool,
    ctrl: bool,
) -> (f64, DragAnchor) {
    let rate = drag_rate(p, width, shift);
    let raw = anchor.value + (x - anchor.x) * rate;

    let (ranged, anchor) = if p.has_range() {
        if p.wrap {
            let wrapped = p.min + (raw - p.min).rem_euclid(p.range());
            if (wrapped - raw).abs() > f64::EPSILON {
                (wrapped, DragAnchor { x, value: wrapped })
            } else {
                (raw, anchor)
            }
        } else {
            let clamped = raw.clamp(p.min, p.max);
            if (clamped - raw).abs() > f64::EPSILON {
                // Anchor shift: measure the rest of the drag from the limit.
                (clamped, DragAnchor { x, value: clamped })
            } else {
                (raw, anchor)
            }
        }
    } else {
        (raw, anchor)
    };

    // Snap the published value only; the anchor stays on the unsnapped
    // track so releasing Ctrl lands back on the pointer's own value.
    let mut publish = ranged;
    if ctrl {
        let inc = snap_increment(p, shift);
        if inc > 0.0 {
            publish = (ranged / inc).round() * inc;
            if p.has_range() && !p.wrap {
                publish = publish.clamp(p.min, p.max);
                if ranged <= p.min {
                    publish = p.min;
                } else if ranged >= p.max {
                    publish = p.max;
                }
            }
        }
    }
    (publish, anchor)
}

/// Re-anchor for a modifier change: the value stays put at the current
/// pointer position, only the rate changes from here on.
pub fn reanchor(current_value: f64, x: f64) -> DragAnchor {
    DragAnchor {
        x,
        value: current_value,
    }
}

/// The three zones of the row: the stepping arrows at the ends and the
/// drag/edit surface between them.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum FieldZone {
    Decrement,
    Middle,
    Increment,
}

/// Zone for a pointer at `x` within a row of `width`×`height`.
pub fn field_zone(x: f64, width: f64, height: f64) -> FieldZone {
    let zone = (width / 3.0).min(height * 0.7);
    if x < zone {
        FieldZone::Decrement
    } else if x > width - zone {
        FieldZone::Increment
    } else {
        FieldZone::Middle
    }
}

#[derive(Clone, Copy, Debug)]
struct DragState {
    press_x: f64,
    press_value: f64,
    width: f64,
    engaged: bool,
    anchor: DragAnchor,
    shift: bool,
    raw_value: f64,
}

#[derive(Script, Widget, Animator)]
pub struct FabValueInput {
    #[uid]
    uid: WidgetUid,
    #[source]
    source: ScriptObjectRef,
    #[apply_default]
    animator: Animator,
    #[redraw]
    #[live]
    draw_bg: DrawDragNum,
    #[live]
    draw_text: DrawText,
    #[live]
    text_input: TextInput,
    #[walk]
    walk: Walk,
    #[layout]
    layout: Layout,
    /// A host can swap the field for another control in the same slot.
    #[live(true)]
    #[visible]
    visible: bool,

    /// Held during either a drag or text editing, so cancel never also
    /// dismisses the surrounding popup or modal.
    #[rust]
    cancel_scope: Option<CancelScope>,

    #[live]
    label: String,
    #[live]
    min: f64,
    #[live]
    max: f64,
    #[live(0.01)]
    step: f64,
    /// Explicit Ctrl-snap increment; `0` derives one from the range.
    #[live]
    snap: f64,
    #[live(2)]
    precision: usize,
    #[live]
    suffix: String,
    #[live]
    value: f64,
    #[live]
    wrap: bool,
    /// The range came from a `/**name min..max step s*/` doc-channel hint:
    /// a hint, never a clamp — a typed value outside it EXPANDS the range.
    #[live]
    hint_bounds: bool,
    /// Bounded: the fill bar shows the value's place in the range and a
    /// drag sweeps the range across the row's width.
    #[live]
    show_fill: bool,
    #[live]
    quantize: bool,
    /// Off: the value shows dimmed and nothing answers — no press, scrub,
    /// wheel step or click into text entry. A host switches a field off
    /// when what it drives is not there to be driven.
    #[live(true)]
    enabled: bool,

    #[rust]
    drag: Option<DragState>,
    /// Pointer over the field: the ‹ › stepper chevrons reveal.
    #[rust]
    hovered: bool,
    /// Time of the last primary press inside the field: two presses within
    /// the double-click window make a RESET gesture.
    #[rust]
    last_press_time: f64,
    /// Live-path measurement: FingerMoves delivered to this owner and
    /// publishes made during the current drag. Logged at drag end so a
    /// physical pass measures against the platform's PIN stats line.
    #[rust]
    drag_moves: u64,
    #[rust]
    drag_publishes: u64,
    #[rust]
    editing: bool,
}

impl ScriptHook for FabValueInput {
    fn on_after_new(&mut self, vm: &mut ScriptVm) {
        let text = self.format();
        vm.with_cx_mut(|cx| {
            self.text_input.set_is_numeric_only(cx, false);
            self.text_input.set_text(cx, &text);
            self.text_input.set_is_read_only(cx, true);
        });
    }
}

impl FabValueInput {
    fn params(&self) -> DragParams {
        DragParams {
            min: self.min,
            max: self.max,
            step: self.step,
            wrap: self.wrap,
            bounded: self.show_fill,
            snap_override: self.snap,
        }
    }

    fn format(&self) -> String {
        let v = match self.precision {
            0 => format!("{:.0}", self.value),
            1 => format!("{:.1}", self.value),
            2 => format!("{:.2}", self.value),
            3 => format!("{:.3}", self.value),
            _ => format!("{}", self.value),
        };
        if self.suffix.is_empty() {
            v
        } else {
            format!("{v}{}", self.suffix)
        }
    }

    /// The string offered for editing: full precision, trailing zeros
    /// trimmed, so opening and committing an edit can never silently round
    /// the stored value.
    fn format_full(&self) -> String {
        let mut v = format!("{:.6}", self.value);
        if v.contains('.') {
            while v.ends_with('0') {
                v.pop();
            }
            if v.ends_with('.') {
                v.pop();
            }
        }
        v
    }

    fn normalize(&self, mut value: f64) -> f64 {
        if self.quantize && self.step > 0.0 {
            value = self.min + ((value - self.min) / self.step).round() * self.step;
        }
        if self.max <= self.min {
            return value;
        }
        if self.wrap {
            self.min + (value - self.min).rem_euclid(self.max - self.min)
        } else {
            value.clamp(self.min, self.max)
        }
    }

    fn parse(&self, text: &str) -> Option<f64> {
        let cleaned: String = text
            .chars()
            .filter(|c| c.is_ascii_digit() || *c == '.' || *c == '-')
            .collect();
        cleaned.parse::<f64>().ok()
    }

    fn sync_text(&mut self, cx: &mut Cx) {
        let t = self.format();
        self.text_input.set_text(cx, &t);
    }

    pub fn set_value(&mut self, cx: &mut Cx, v: f64) {
        if self.editing || self.drag.is_some() {
            return;
        }
        let v = self.normalize(v);
        if (v - self.value).abs() > f64::EPSILON {
            self.value = v;
            self.sync_text(cx);
            self.draw_bg.redraw(cx);
        }
    }

    pub fn value(&self) -> f64 {
        self.value
    }

    pub fn enabled(&self) -> bool {
        self.enabled
    }

    /// Switching off mid-gesture ends the gesture first: an open editor
    /// closes without committing, an engaged scrub lets the pointer go.
    pub fn set_enabled(&mut self, cx: &mut Cx, enabled: bool) {
        if self.enabled == enabled {
            return;
        }
        self.enabled = enabled;
        if !enabled {
            let uid = self.widget_uid();
            if self.editing {
                self.end_edit(cx);
                cx.revert_key_focus();
            }
            self.cancel_drag(cx, uid);
            self.hovered = false;
            self.animator_play(cx, ids!(hover.off));
        }
        self.draw_bg.redraw(cx);
    }

    /// Focus/IME state of the private text editor used while a scrub field is
    /// being typed. Canvas hosts cannot discover this child through the
    /// public widget tree because it is embedded directly, not a WidgetRef.
    pub fn text_ime_anchor(&self, cx: &Cx) -> Option<(Area, Rect, TextInputConfig)> {
        let area = self.text_input.area();
        if !self.editing || area.is_empty() || !cx.has_key_focus(area) {
            return None;
        }
        Some((
            area,
            self.text_input.cursor_rect_in_absolute(cx)?,
            self.text_input.ime_config(),
        ))
    }

    fn publish(&mut self, cx: &mut Cx, uid: WidgetUid, v: f64, ended: bool) {
        if (v - self.value).abs() > f64::EPSILON {
            self.value = v;
            self.sync_text(cx);
            self.draw_bg.redraw(cx);
            cx.widget_action(uid, FabValueInputAction::Changed(self.value));
        }
        if ended {
            cx.widget_action(uid, FabValueInputAction::Ended(self.value));
        }
    }

    fn step_once(&mut self, cx: &mut Cx, uid: WidgetUid, direction: f64, shift: bool) {
        let step = if shift { self.step * 0.1 } else { self.step };
        let v = self.normalize(self.value + direction * step.max(f64::EPSILON));
        if (v - self.value).abs() > f64::EPSILON {
            self.publish(cx, uid, v, true);
        }
    }

    pub fn begin_edit(&mut self, cx: &mut Cx) {
        self.drag = None;
        if self.cancel_scope.is_none() {
            self.cancel_scope = Some(self.begin_cancel_scope(cx));
        }
        self.editing = true;
        let full = self.format_full();
        self.text_input.set_is_numeric_only(cx, true);
        self.text_input.set_text(cx, &full);
        self.text_input.set_is_read_only(cx, false);
        self.text_input.set_key_focus(cx);
        self.text_input.select_all(cx);
        self.animator_play(cx, ids!(focus.on));
        self.draw_bg.redraw(cx);
    }

    fn end_edit(&mut self, cx: &mut Cx) {
        self.editing = false;
        self.cancel_scope = None;
        self.text_input.set_is_read_only(cx, true);
        self.text_input.set_is_numeric_only(cx, false);
        self.sync_text(cx);
        self.animator_play(cx, ids!(focus.off));
        self.draw_bg.redraw(cx);
    }

    fn commit_edit_text(&mut self, cx: &mut Cx, uid: WidgetUid, text: &str) {
        if let Some(parsed) = self.parse(text) {
            if self.hint_bounds && self.max > self.min {
                // Hint semantics: typing past a bound expands the range.
                self.min = self.min.min(parsed);
                self.max = self.max.max(parsed);
            }
            let v = self.normalize(parsed);
            self.publish(cx, uid, v, true);
        }
        self.end_edit(cx);
    }

    /// Apply a `name min..max step s` doc-channel hint to the scrubber:
    /// bounds show the fill bar and set the drag sweep, step sets the
    /// granularity. A hint, not a schema — typing past a bound expands it.
    pub fn set_hint(&mut self, min: Option<f64>, max: Option<f64>, step: Option<f64>) {
        if let (Some(a), Some(b)) = (min, max) {
            if b > a {
                self.min = a;
                self.max = b;
                self.show_fill = true;
                self.hint_bounds = true;
            }
        }
        if let Some(step) = step {
            if step > 0.0 {
                self.step = step;
            }
        }
    }

    fn cancel_drag(&mut self, cx: &mut Cx, uid: WidgetUid) {
        self.cancel_scope = None;
        if let Some(drag) = self.drag.take() {
            if drag.engaged {
                // Early cancel (Escape / right-click): the button is still
                // held, so the pin must be lifted explicitly.
                cx.unpin_pointer_capture();
                self.publish(cx, uid, drag.press_value, false);
            }
            self.animator_play(cx, ids!(hover.off));
        }
    }
}

impl Widget for FabValueInput {
    // The generic switch and the bridge's /snap `enabled` column both go
    // through these, so what they say is what the field does.
    fn set_disabled(&mut self, cx: &mut Cx, disabled: bool) {
        self.set_enabled(cx, !disabled);
    }

    fn disabled(&self, _cx: &Cx) -> bool {
        !self.enabled
    }

    fn draw_walk(&mut self, cx: &mut Cx2d, _scope: &mut Scope, walk: Walk) -> DrawStep {
        if !self.visible {
            return DrawStep::done();
        }
        // The fill claims "this range means something": only bounded fields
        // paint one.
        self.draw_bg.fill = if self.show_fill && self.max > self.min {
            (((self.value - self.min) / (self.max - self.min)) as f32).clamp(0.0, 1.0)
        } else {
            -1.0
        };
        self.draw_bg.disabled = if self.enabled { 0.0 } else { 1.0 };
        self.draw_bg.begin(cx, walk, self.layout);
        if !self.label.is_empty() {
            // The label spans exactly the space the value does not need:
            // the value lands right-anchored; a tight row elides the label,
            // never the number.
            let row = cx.turtle().rect().size.x;
            let pad = self.layout.padding.left + self.layout.padding.right;
            let fs = self.draw_text.text_style.font_size as f64;
            let value_reserve = (self.format().chars().count() as f64 + 0.5) * fs * 0.72 + 6.0;
            let label_w = (row - pad - value_reserve).max(0.0);
            // A label that cannot fit is not drawn at all: a crushed "w"
            // renders as a stray dot beside the number.
            let needed = self.label.chars().count() as f64 * fs * 0.62 + 2.0;
            if label_w >= needed {
                let mut label_walk = Walk::fit();
                label_walk.width = Size::Fixed(label_w);
                self.draw_text
                    .draw_walk(cx, label_walk, Align::default(), &self.label);
            }
        }
        let iw = self.text_input.walk(cx);
        if self.enabled {
            let _ = self.text_input.draw_walk(cx, &mut Scope::empty(), iw);
        } else {
            // Off: the value is still there to read, in the label's ink at
            // half strength, where the editor would have put it.
            let text = self.format();
            let old = self.draw_text.color;
            self.draw_text.color = vec4(old.x, old.y, old.z, old.w * 0.5);
            self.draw_text.draw_walk(cx, iw, Align { x: 1.0, y: 0.5 }, &text);
            self.draw_text.color = old;
        }
        // The 3D-suite convention: stepper chevrons reveal on hover at the
        // field's edges — their zones (field_zone) exist regardless; the
        // glyphs only while the pointer is here and nothing is in flight.
        if self.enabled && self.hovered && !self.editing && self.drag.is_none() {
            let rect = cx.turtle().rect();
            let fs = self.draw_text.text_style.font_size as f64;
            let y = rect.pos.y + (rect.size.y - fs * 1.5).max(0.0) * 0.5;
            let old = self.draw_text.color;
            self.draw_text.color = vec4(0.69, 0.69, 0.69, 0.9);
            self.draw_text
                .draw_abs(cx, dvec2(rect.pos.x + 3.0, y), "\u{2039}");
            self.draw_text.draw_abs(
                cx,
                dvec2(rect.pos.x + rect.size.x - 9.0, y),
                "\u{203a}",
            );
            self.draw_text.color = old;
        }
        self.draw_bg.end(cx);
        DrawStep::done()
    }

    fn handle_event(&mut self, cx: &mut Cx, event: &Event, scope: &mut Scope) {
        let uid = self.widget_uid();
        self.animator_handle_event(cx, event);
        // Off: nothing below answers. The animator still settles whatever
        // was in flight when the field went off.
        if !self.enabled {
            return;
        }
        if (self.editing || self.drag.is_some())
            && crate::modal::ModalAction::is_dismissal(event)
        {
            self.cancel_drag(cx, uid);
            if self.editing {
                self.end_edit(cx);
            }
            return;
        }

        // Double-click = RESET, detected on the raw press so it works in
        // every state (the second press of a double-click lands while the
        // first click's text editor is already open — the editor claims
        // hits, so hit-testing can't see it). Coexistence: a single click
        // still opens the editor instantly (snappy); the second click
        // within the window converts that into end-edit + reset.
        if let Event::MouseDown(me) = event {
            // ...but only in the MIDDLE. The stepper arrows exist to be
            // clicked repeatedly, and two of those inside the double-click
            // window were being read as the reset gesture — nudge a value up
            // three times and it snapped back to its default on the way.
            let face = self.draw_bg.area().rect(cx);
            let on_middle = face.size.x > 0.0
                && matches!(
                    field_zone(me.abs.x - face.pos.x, face.size.x, face.size.y),
                    FieldZone::Middle
                );
            if me.button.is_primary()
                && on_middle
                && self.draw_bg.area().clipped_rect(cx).contains(me.abs)
            {
                if me.time - self.last_press_time < 0.4 {
                    self.last_press_time = 0.0;
                    if self.editing {
                        self.end_edit(cx);
                        cx.revert_key_focus();
                    }
                    self.drag = None;
                    self.cancel_scope = None;
                    cx.widget_action(uid, FabValueInputAction::Reset);
                    return;
                }
                self.last_press_time = me.time;
            }
        }

        // Ctrl+Wheel nudges by one step; a plain wheel keeps scrolling the
        // panel underneath.
        if let Event::Scroll(e) = event {
            if e.modifiers.control || e.modifiers.logo {
                if !e.handled_y.get()
                    && e.scroll.y.abs() > f64::EPSILON
                    && self.draw_bg.area().rect(cx).contains(e.abs)
                {
                    let direction = if e.scroll.y < 0.0 { 1.0 } else { -1.0 };
                    self.step_once(cx, uid, direction, e.modifiers.shift);
                    e.handled_y.set(true);
                }
            }
        }

        if self.editing
            && self.cancel_scope.as_ref().is_some_and(|s| cx.owns_cancel(s))
            && (matches!(event, Event::KeyDown(ke) if ke.key_code == KeyCode::Escape)
                || event.back_pressed())
        {
            self.end_edit(cx);
            cx.revert_key_focus();
            return;
        }

        // Escape, Back or a right-button press cancels an in-flight drag and
        // restores the pressed value.
        if self.drag.is_some() {
            match event {
                Event::KeyDown(ke)
                    if ke.key_code == KeyCode::Escape
                        && self.cancel_scope.as_ref().is_some_and(|s| cx.owns_cancel(s)) =>
                {
                    self.cancel_drag(cx, uid);
                    return;
                }
                Event::BackPressed { .. }
                    if self.cancel_scope.as_ref().is_some_and(|s| cx.owns_cancel(s))
                        && event.back_pressed() =>
                {
                    self.cancel_drag(cx, uid);
                    return;
                }
                Event::MouseDown(me) if me.button.is_secondary() => {
                    self.cancel_drag(cx, uid);
                    return;
                }
                Event::WindowLostFocus(_) => {
                    self.cancel_drag(cx, uid);
                    return;
                }
                _ => {}
            }
        }

        // The embedded input is a display until a click opens it: while it
        // is not editing it receives no events at all — otherwise it claims
        // the press for text selection and the drag never sees a move.
        if self.editing {
            // Focus ownership is the state boundary: if focus moved away
            // while an action was consumed elsewhere, commit and return to
            // the read-only display.
            let input_area = self.text_input.area();
            if input_area != Area::Empty && !cx.has_key_focus(input_area) {
                let text = self.text_input.text().to_string();
                self.commit_edit_text(cx, uid, &text);
                return;
            }
            for action in cx.capture_actions(|cx| self.text_input.handle_event(cx, event, scope)) {
                match action.as_widget_action().cast() {
                    TextInputAction::KeyFocus => {
                        self.animator_play(cx, ids!(focus.on));
                    }
                    TextInputAction::KeyFocusLost => {
                        if self.editing {
                            let text = self.text_input.text().to_string();
                            self.commit_edit_text(cx, uid, &text);
                        }
                    }
                    TextInputAction::Returned(v, _) => {
                        if self.editing {
                            self.commit_edit_text(cx, uid, &v);
                            cx.revert_key_focus();
                        }
                    }
                    TextInputAction::Escaped => {
                        if self.editing
                            && self.cancel_scope.as_ref().is_some_and(|s| cx.owns_cancel(s))
                        {
                            self.end_edit(cx);
                            cx.revert_key_focus();
                        }
                    }
                    _ => {}
                }
            }
        }

        match event.hits(cx, self.draw_bg.area()) {
            Hit::FingerHoverIn(fe) => {
                let rect = self.draw_bg.area().rect(cx);
                let zone = field_zone(fe.abs.x - rect.pos.x, rect.size.x, rect.size.y);
                cx.set_cursor(match zone {
                    FieldZone::Middle => MouseCursor::EwResize,
                    _ => MouseCursor::Default,
                });
                self.hovered = true;
                self.draw_bg.redraw(cx);
                self.animator_play(cx, ids!(hover.on));
            }
            Hit::FingerHoverOver(fe) => {
                if self.drag.is_none() && !self.editing {
                    let rect = self.draw_bg.area().rect(cx);
                    let zone = field_zone(fe.abs.x - rect.pos.x, rect.size.x, rect.size.y);
                    cx.set_cursor(match zone {
                        FieldZone::Middle => MouseCursor::EwResize,
                        _ => MouseCursor::Default,
                    });
                }
            }
            Hit::FingerHoverOut(_) => {
                self.hovered = false;
                self.draw_bg.redraw(cx);
                self.animator_play(cx, ids!(hover.off));
            }
            Hit::FingerDown(fe) if fe.device.is_primary_hit() && !self.editing => {
                let rect = self.draw_bg.area().rect(cx);
                // Press changes nothing: it only arms.
                self.drag = Some(DragState {
                    press_x: fe.abs.x,
                    press_value: self.value,
                    width: rect.size.x,
                    engaged: false,
                    anchor: DragAnchor {
                        x: fe.abs.x,
                        value: self.value,
                    },
                    shift: fe.modifiers.shift,
                    raw_value: self.value,
                });
                // The next event may already be Escape; ownership is captured
                // before dispatch, so the scope must exist before this returns.
                self.cancel_scope = Some(self.begin_cancel_scope(cx));
                self.animator_play(cx, ids!(hover.down));
            }
            Hit::FingerMove(fe) => {
                let Some(mut drag) = self.drag else {
                    return;
                };
                if !drag.engaged {
                    if (fe.abs.x - drag.press_x).abs() < DRAG_THRESHOLD {
                        return;
                    }
                    // Engage at the pointer, discarding the threshold
                    // distance: the first dragged pixel is a small change.
                    drag.engaged = true;
                    drag.anchor = reanchor(self.value, fe.abs.x);
                    drag.raw_value = self.value;
                    // The pointer pins where the press happened: hidden,
                    // infinite drag range, restored in place on release.
                    // Engaged only now, at the threshold — a plain click
                    // never touches the cursor. The pin rides on this
                    // widget's finger capture; the hardware button-up
                    // releases both automatically.
                    cx.pin_pointer_capture();
                    self.drag_moves = 0;
                    self.drag_publishes = 0;
                }
                self.drag_moves += 1;
                let mods = cx.keyboard.modifiers();
                if mods.shift != drag.shift {
                    // A modifier change re-anchors: the value holds still,
                    // only the rate changes from here.
                    drag.shift = mods.shift;
                    drag.anchor = reanchor(drag.raw_value, fe.abs.x);
                }
                let params = self.params();
                let (publish, anchor) = drag_map(
                    &params,
                    drag.anchor,
                    fe.abs.x,
                    drag.width,
                    mods.shift,
                    mods.control | mods.logo,
                );
                drag.raw_value = anchor.value
                    + (fe.abs.x - anchor.x) * drag_rate(&params, drag.width, mods.shift);
                if params.has_range() && !params.wrap {
                    drag.raw_value = drag.raw_value.clamp(params.min, params.max);
                }
                drag.anchor = anchor;
                self.drag = Some(drag);
                let v = self.normalize(publish);
                self.drag_publishes += 1;
                self.publish(cx, uid, v, false);
                // Hold the pin against quiet OS re-association drops.
                cx.repin_mouse_pointer();
            }
            Hit::FingerUp(fe) => {
                self.cancel_scope = None;
                let Some(drag) = self.drag.take() else {
                    return;
                };
                if drag.engaged {
                    // The pin released with the capture on the way in; the
                    // action is all that is left to send.
                    log!(
                        "SCRUB stats: finger_moves={} publishes={}",
                        self.drag_moves,
                        self.drag_publishes
                    );
                    cx.widget_action(uid, FabValueInputAction::Ended(self.value));
                } else {
                    // A click. The zone at release decides: arrows step,
                    // the middle opens text entry with the value selected.
                    let rect = self.draw_bg.area().rect(cx);
                    let zone = field_zone(fe.abs.x - rect.pos.x, rect.size.x, rect.size.y);
                    match zone {
                        FieldZone::Decrement => self.step_once(cx, uid, -1.0, fe.modifiers.shift),
                        FieldZone::Increment => self.step_once(cx, uid, 1.0, fe.modifiers.shift),
                        FieldZone::Middle => self.begin_edit(cx),
                    }
                }
                if fe.is_over && fe.device.has_hovers() {
                    self.animator_play(cx, ids!(hover.on));
                } else {
                    self.animator_play(cx, ids!(hover.off));
                }
            }
            _ => {}
        }
    }
}

impl FabValueInputRef {
    pub fn changed(&self, actions: &Actions) -> Option<f64> {
        if let Some(item) = actions.find_widget_action(self.widget_uid()) {
            if let FabValueInputAction::Changed(v) = item.cast() {
                return Some(v);
            }
        }
        None
    }

    pub fn ended(&self, actions: &Actions) -> Option<f64> {
        ended_value(actions, self.widget_uid())
    }

    pub fn set_value(&self, cx: &mut Cx, v: f64) {
        if let Some(mut inner) = self.borrow_mut() {
            inner.set_value(cx, v);
        }
    }

    pub fn value(&self) -> f64 {
        self.borrow().map_or(0.0, |i| i.value())
    }

    pub fn set_enabled(&self, cx: &mut Cx, enabled: bool) {
        if let Some(mut inner) = self.borrow_mut() {
            inner.set_enabled(cx, enabled);
        }
    }

    pub fn enabled(&self) -> bool {
        self.borrow().map_or(true, |i| i.enabled())
    }
}

fn ended_value(actions: &Actions, uid: WidgetUid) -> Option<f64> {
    for action in actions.filter_widget_actions_cast::<FabValueInputAction>(uid) {
        if let FabValueInputAction::Ended(v) = action {
            return Some(v);
        }
    }
    None
}

// ===========================================================================
// FabSlider — a horizontal track whose thumb goes where the pointer is. The
// travel law is pure and the shader is handed the same numbers the hit test
// measures with, so what is drawn is what is grabbable.
// ===========================================================================

#[derive(Script, ScriptHook)]
#[repr(C)]
pub struct DrawFabSlider {
    #[deref]
    draw_super: DrawQuad,
    #[live]
    hover: f32,
    #[live]
    down: f32,
    #[live]
    focus: f32,
    #[live]
    disabled: f32,
    /// Where the value sits along the travel, 0..1.
    #[live]
    travel: f32,
    /// The name's column and the number's column, in pixels. Rust measures
    /// the track between them and the shader draws it between the same two.
    #[live]
    label_px: f32,
    #[live]
    readout_px: f32,
    #[live]
    thumb_px: f32,
    #[live]
    inset_px: f32,
}

#[derive(Clone, Debug, Default)]
pub enum FabSliderAction {
    /// The value moved. Live under a drag, and under every arrow key
    /// including the repeats the keyboard sends while one is held down.
    Changed(f64),
    /// The gesture that was moving the value is over, and this is the value
    /// it came to rest on. A commit: a host that rate-limits `Changed` is
    /// meant to spend this one at once.
    ///
    /// At most ONE per gesture, and never none. A mouse release ends a drag
    /// or a tap on the name; a deliberate key press ends itself, so a single
    /// arrow and a jump to a stop both land without waiting; and the release
    /// of a HELD key ends the run of repeats it sent, which is what keeps a
    /// second of held arrow down to two of these instead of thirty. See
    /// `key_step`.
    ///
    /// Where a run ends without its release -- the keyboard moving on, the
    /// row being switched off, the window losing the focus mid-key -- what
    /// the run owes is paid there instead. A host that rate-limits `Changed`
    /// and spends this one can hold it to that.
    Ended(f64),
    /// A click on the name: the row is back at zero and the host should take
    /// it out of whatever it feeds.
    Reset,
    #[default]
    None,
}

/// The travel one slider carries into a press: the range it spans, the
/// detent it lands on, and the two pixel sizes the shader is handed.
///
/// The mapping is ABSOLUTE — the value is where the pointer is, not how far
/// it has come — which is the whole difference between this and the number
/// field's scrub above. Kept here, entire and with no `Cx` anywhere, so the
/// law the hit test uses is the law the tests read.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SliderTravel {
    pub min: f64,
    pub max: f64,
    /// The detent a value lands on; `0` is continuous.
    pub step: f64,
    /// Thumb width and the track's inset, in pixels.
    pub thumb: f64,
    pub inset: f64,
}

impl SliderTravel {
    /// The two stops in order, whichever way round they were written.
    pub fn stops(&self) -> (f64, f64) {
        (self.min.min(self.max), self.min.max(self.max))
    }

    /// A value onto the detent and inside the stops. Quantising walks a
    /// value off the end of a range that is not a whole number of steps, so
    /// the clamp comes second and not first.
    pub fn settle(&self, v: f64) -> f64 {
        let (lo, hi) = self.stops();
        let v = if self.step > 0.0 {
            lo + ((v - lo) / self.step).round() * self.step
        } else {
            v
        };
        v.clamp(lo, hi)
    }

    /// Inside the stops, and nowhere near the detent.
    ///
    /// The detent belongs to the hand: it is where a drag lands and how far
    /// an arrow key carries. A number that arrives from a caller is somebody
    /// else's arithmetic and is none of its business -- a row of weights
    /// sharing a hundred parts stops adding up the moment three of them are
    /// rounded on the way in.
    pub fn contain(&self, v: f64) -> f64 {
        let (lo, hi) = self.stops();
        v.clamp(lo, hi)
    }

    /// Where a value sits along the travel, 0..1.
    pub fn travel(&self, v: f64) -> f64 {
        let (lo, hi) = self.stops();
        let span = hi - lo;
        if span.abs() < f64::EPSILON {
            0.0
        } else {
            ((v - lo) / span).clamp(0.0, 1.0)
        }
    }

    /// What the thumb's CENTRE runs over: the track less its inset at both
    /// ends and the thumb's own width, so neither stop hangs off the end.
    pub fn travel_px(&self, width: f64) -> f64 {
        (width - self.inset * 2.0 - self.thumb).max(1.0)
    }

    /// The value at an x offset from the left edge of the TRACK.
    pub fn value_at(&self, x: f64, width: f64) -> f64 {
        let t = ((x - self.inset - self.thumb * 0.5) / self.travel_px(width)).clamp(0.0, 1.0);
        let (lo, hi) = self.stops();
        self.settle(lo + t * (hi - lo))
    }

    /// The x of the thumb's centre, in the same frame as `value_at`.
    pub fn thumb_x(&self, v: f64, width: f64) -> f64 {
        self.inset + self.thumb * 0.5 + self.travel(v) * self.travel_px(width)
    }
}

/// The three columns of the row. Only the track answers a press with a
/// value: the number is there to be read, and the name is the reset.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SliderZone {
    Label,
    Track,
    Readout,
}

/// Which column a pointer at `x` (from the row's left edge) is over, for a
/// row of `width` whose outer columns are `label_px` and `readout_px` wide.
pub fn slider_zone(x: f64, width: f64, label_px: f64, readout_px: f64) -> SliderZone {
    if x < label_px {
        SliderZone::Label
    } else if x > width - readout_px {
        SliderZone::Readout
    } else {
        SliderZone::Track
    }
}

#[derive(Script, ScriptHook, Widget)]
pub struct FabSlider {
    #[uid]
    uid: WidgetUid,
    #[source]
    source: ScriptObjectRef,
    #[redraw]
    #[live]
    draw_bg: DrawFabSlider,
    #[live]
    draw_label: DrawText,
    #[live]
    draw_value: DrawText,
    #[walk]
    walk: Walk,
    #[layout]
    layout: Layout,

    #[live]
    label: String,
    /// The name's column and the number's, in points. Fixed rather than
    /// fitted: an equalizer is a stack of these, and the tracks have to
    /// begin in the same place all the way down the column.
    #[live(92.0)]
    label_width: f64,
    #[live(34.0)]
    readout_width: f64,
    #[live]
    min: f64,
    #[live(100.0)]
    max: f64,
    /// The arrow-key increment, and the detent a drag lands on.
    #[live(1.0)]
    step: f64,
    /// Shift+arrow. Coarse, where Shift on the number field above is fine:
    /// one arrow on a 0..100 track is already the small gesture, so Shift
    /// has nowhere to go but up.
    #[live(10.0)]
    big_step: f64,
    #[live(0)]
    precision: usize,
    #[live]
    unit: String,
    #[live]
    value: f64,
    #[live(12.0)]
    thumb_size: f64,
    #[live(2.0)]
    track_inset: f64,
    /// Off: the row shows dimmed and nothing answers.
    #[live(true)]
    enabled: bool,

    /// Held for the length of a gesture, so Escape and a modal's dismissal
    /// reach this control rather than whatever it is sitting in.
    #[rust]
    cancel_scope: Option<CancelScope>,
    #[rust]
    dragging: bool,
    /// What the value was when the press landed, for a cancel to put back.
    #[rust]
    press_value: f64,
    /// The number this row PRINTS, where that is not the number it holds.
    ///
    /// A column that is read as a whole -- shares of a total, rounded over
    /// the column rather than a row at a time -- has a number for the
    /// readout that is nobody's own value, and can be a part or two from it.
    /// Kept apart so that it stops at the text: the thumb stands on `value`,
    /// an arrow steps from `value`, and what a gesture reports is `value`.
    /// Cleared by anything that moves the row, because the number a hand has
    /// just set is the row's own, and a share worked out for the value
    /// before it would print as a lie under a moving thumb.
    #[rust]
    readout: Option<f64>,
    #[rust]
    hovered: bool,
    /// A keyboard run owes a commit: an arrow has moved the row since the
    /// last one went out, and the release that will end the run has not
    /// arrived yet.
    #[rust]
    key_commit_due: bool,
    /// The name's own box. The face is one area and it takes the press; this
    /// is only ever asked whether a tap FINISHED on the word, and never
    /// asked for a hit of its own — a second area over the same press is
    /// what once left the stock Slider unable to be dragged from its legend.
    #[rust]
    label_area: Area,
}

impl FabSlider {
    fn travel(&self) -> SliderTravel {
        SliderTravel {
            min: self.min,
            max: self.max,
            step: self.step,
            thumb: self.thumb_size,
            inset: self.track_inset,
        }
    }

    /// The row's outer two columns, padding included, in the face's frame.
    fn columns(&self) -> (f64, f64) {
        (
            self.layout.padding.left + self.label_width,
            self.layout.padding.right + self.readout_width,
        )
    }

    /// The value the pointer is naming right now.
    fn value_at_pointer(&self, cx: &Cx, abs_x: f64) -> f64 {
        let face = self.draw_bg.area().rect(cx);
        let (label_px, readout_px) = self.columns();
        let width = (face.size.x - label_px - readout_px).max(1.0);
        self.travel().value_at(abs_x - face.pos.x - label_px, width)
    }

    /// Which column a press at `abs_x` landed in.
    fn zone_at(&self, cx: &Cx, abs_x: f64) -> SliderZone {
        let face = self.draw_bg.area().rect(cx);
        let (label_px, readout_px) = self.columns();
        slider_zone(abs_x - face.pos.x, face.size.x, label_px, readout_px)
    }

    pub fn value(&self) -> f64 {
        self.value
    }

    /// A value pushed in from outside, held EXACTLY as it was handed over.
    ///
    /// What is guaranteed: the number a host sets is the number `value()`
    /// reads back, the number the next arrow key steps from, and the number
    /// the thumb stands on -- clamped to the stops and to nothing else. The
    /// detent is the HAND's grid, the thing a drag lands on and the thing an
    /// arrow steps by; it is never a filter on the host's own arithmetic. A
    /// `step` of 1 is what a weight row wants under a finger, and it used to
    /// round what the host had worked out as well: four rows splitting a
    /// hundred parts come down to 0 / 37.5 / 37.5 / 25, and rows that stored
    /// 38 read back 101% under a legend promising a hundred -- and then
    /// handed the next arrow key an origin nobody had set.
    ///
    /// What is NOT guaranteed: that the number on screen is the number held.
    /// The readout is `precision` places wide and rounds to fit, so a row
    /// holding 37.5 at `precision: 0` prints 38. A host that needs the column
    /// to READ as a hundred as well as sum to one is not to round before it
    /// sets -- that stores the rounded number, with every consequence in the
    /// paragraph above -- but to say both numbers at once. See
    /// [`FabSlider::set_value_and_readout`].
    ///
    /// Refused mid-drag: a host answering late must not argue with the hand
    /// that is on the thumb.
    pub fn set_value(&mut self, cx: &mut Cx, v: f64) {
        if self.dragging {
            return;
        }
        self.hold(cx, v, None);
    }

    /// The number the row HOLDS and the number it PRINTS, handed over
    /// together.
    ///
    /// For the host whose readout is not its own arithmetic. A column of
    /// shares of a total is rounded over the whole column, so what one row
    /// shows is a part or two off what it carries; a host with nowhere to
    /// put that but the value ended up storing it, and then the row stepped
    /// from it -- an arrow on the largest share of a mix walked the weight
    /// DOWN, because the largest share is the one the column takes its
    /// rounding out of.
    ///
    /// So they arrive together and part company here: `v` is the whole of
    /// what the row holds, steps from and reports, and `readout` reaches
    /// nothing but the text. They are one call because they are one fact --
    /// a row left printing the share of a mix it no longer holds is the same
    /// fault the other way round.
    ///
    /// Refused mid-drag, for the reason above it.
    pub fn set_value_and_readout(&mut self, cx: &mut Cx, v: f64, readout: f64) {
        if self.dragging {
            return;
        }
        self.hold(cx, v, Some(readout));
    }

    /// What the row prints: what a host said to print, or what the row holds.
    pub fn readout(&self) -> f64 {
        self.readout.unwrap_or(self.value)
    }

    /// The one door a host's number comes in by. The redraw hangs off the
    /// PAIR, because a column can be re-rounded by a move on another row
    /// without this one's value changing by anything at all.
    fn hold(&mut self, cx: &mut Cx, v: f64, readout: Option<f64>) {
        let v = self.travel().contain(v);
        if (v - self.value).abs() > f64::EPSILON || readout != self.readout {
            self.value = v;
            self.readout = readout;
            self.draw_bg.redraw(cx);
        }
    }

    pub fn label(&self) -> &str {
        &self.label
    }

    /// The row's name, for a host filling a list of them.
    pub fn set_label(&mut self, cx: &mut Cx, text: &str) {
        if self.label != text {
            self.label = text.to_string();
            self.draw_bg.redraw(cx);
        }
    }

    pub fn enabled(&self) -> bool {
        self.enabled
    }

    /// Switching off mid-gesture ends the gesture first: the pointer is let
    /// go, the value the press found is put back, and a keyboard run that has
    /// not committed yet pays up. A row nothing can reach will never see the
    /// release that would otherwise have ended it.
    pub fn set_enabled(&mut self, cx: &mut Cx, enabled: bool) {
        if self.enabled == enabled {
            return;
        }
        self.enabled = enabled;
        if !enabled {
            let uid = self.widget_uid();
            self.cancel_drag(cx, uid);
            self.end_key_run(cx, uid);
            self.hovered = false;
        }
        self.draw_bg.redraw(cx);
    }

    /// Answers whether the value actually moved, which is what tells a key
    /// press whether it has anything left to commit.
    fn publish(&mut self, cx: &mut Cx, uid: WidgetUid, v: f64, ended: bool) -> bool {
        let moved = (v - self.value).abs() > f64::EPSILON;
        if moved {
            self.value = v;
            // The hand's number is the row's own, so whatever a host had it
            // printing instead goes here: a share worked out for the weight
            // before this one would sit under a thumb that has left it.
            self.readout = None;
            self.draw_bg.redraw(cx);
            cx.widget_action(uid, FabSliderAction::Changed(self.value));
        }
        if ended {
            cx.widget_action(uid, FabSliderAction::Ended(self.value));
        }
        moved
    }

    /// One key's worth of movement, and whether it ends anything.
    ///
    /// A fresh press is a deliberate act and commits where it lands, so one
    /// arrow and a jump to a stop both land at once. What the keyboard sends
    /// AFTER it is not a second gesture, it is the first one still running:
    /// a repeat says only that the value moved, and the single commit the run
    /// owes is paid at the release. An arrow held for a second is therefore
    /// two commits rather than thirty -- which is the difference that matters
    /// on the other side, where a commit is a host dropping everything to
    /// install what it was handed. A drag is bounded there already, by the
    /// interval behind its moves; a held key had been going round it.
    fn key_step(&mut self, cx: &mut Cx, uid: WidgetUid, v: f64, repeat: bool) {
        let moved = self.publish(cx, uid, v, !repeat);
        if repeat {
            // A repeat that landed nowhere new -- an arrow held against a
            // stop -- owes nothing of its own, and cancels nothing already
            // owed by the repeats before it.
            self.key_commit_due |= moved;
        } else {
            self.key_commit_due = false;
        }
    }

    /// The commit a keyboard run still owes, paid at the release -- or at
    /// whatever ends the run before one arrives. Unpaid, the last value a
    /// hand nudged sits on the row and reaches nobody.
    fn end_key_run(&mut self, cx: &mut Cx, uid: WidgetUid) {
        if self.key_commit_due {
            self.key_commit_due = false;
            cx.widget_action(uid, FabSliderAction::Ended(self.value));
        }
    }

    fn nudge(&mut self, cx: &mut Cx, uid: WidgetUid, direction: f64, big: bool, repeat: bool) {
        let step = if big { self.big_step } else { self.step };
        // A continuous track still has to move by something; a hundredth of
        // the range is the arrow-key equivalent of one percent.
        let step = if step > 0.0 {
            step
        } else {
            (self.max - self.min).abs() * 0.01
        };
        // One step from where the row actually stands. Settling the sum onto
        // the detent's grid reads the origin off the grid first, so an arrow
        // pressed on a row set to 37.5 published 39 -- a step and a half the
        // hand never asked for.
        let v = self.travel().contain(self.value + direction * step);
        self.key_step(cx, uid, v, repeat);
    }

    /// ZERO, and not the range's floor nor a `default:` the way the stock
    /// Slider's title click goes. "This one counts for nothing" is the
    /// gesture a row of these needs most, and it is the same number whichever
    /// row it is asked of; a range that never reaches zero takes its nearest
    /// stop instead.
    fn reset(&mut self, cx: &mut Cx, uid: WidgetUid) {
        let v = self.travel().settle(0.0);
        self.publish(cx, uid, v, false);
        cx.widget_action(uid, FabSliderAction::Reset);
    }

    fn cancel_drag(&mut self, cx: &mut Cx, uid: WidgetUid) {
        self.cancel_scope = None;
        if self.dragging {
            self.dragging = false;
            let back = self.press_value;
            self.publish(cx, uid, back, false);
            self.draw_bg.redraw(cx);
        }
    }
}

impl Widget for FabSlider {
    // The generic switch and the bridge's `enabled` column both come through
    // here, so what they say is what the row does.
    fn set_disabled(&mut self, cx: &mut Cx, disabled: bool) {
        self.set_enabled(cx, !disabled);
    }

    fn disabled(&self, _cx: &Cx) -> bool {
        !self.enabled
    }

    fn draw_walk(&mut self, cx: &mut Cx2d, _scope: &mut Scope, walk: Walk) -> DrawStep {
        let (label_px, readout_px) = self.columns();
        self.draw_bg.label_px = label_px as f32;
        self.draw_bg.readout_px = readout_px as f32;
        self.draw_bg.thumb_px = self.thumb_size as f32;
        self.draw_bg.inset_px = self.track_inset as f32;
        self.draw_bg.travel = self.travel().travel(self.value) as f32;
        self.draw_bg.hover = if self.hovered && self.enabled { 1.0 } else { 0.0 };
        self.draw_bg.down = if self.dragging { 1.0 } else { 0.0 };
        self.draw_bg.focus = if cx.cx.cx.has_key_focus(self.draw_bg.area()) {
            1.0
        } else {
            0.0
        };
        self.draw_bg.disabled = if self.enabled { 0.0 } else { 1.0 };
        self.draw_bg.begin(cx, walk, self.layout);

        // The name gets a turtle of its own, because the reset gesture needs
        // a box to ask about at the release. It is a measurement and not a
        // hit target: the face above is the only thing that takes a press.
        let label_walk = Walk::new(Size::Fixed(self.label_width), Size::fill());
        cx.begin_turtle(label_walk, Layout::default());
        if !self.label.is_empty() {
            self.draw_label
                .draw_walk(cx, label_walk, Align { x: 0.0, y: 0.5 }, &self.label);
        }
        cx.end_turtle_with_area(&mut self.label_area);

        // The track itself is painted by the face underneath; this only
        // claims the width, and claims exactly the width the hit test and
        // the shader both measure, so the number lands where it belongs.
        let row = cx.turtle().rect().size.x;
        let track_w = (row - label_px - readout_px).max(1.0);
        let _ = cx.walk_turtle(Walk::new(Size::Fixed(track_w), Size::fill()));

        let text = crate::slider::format_readout(self.readout(), self.precision, &self.unit);
        let value_walk = Walk::new(Size::Fixed(self.readout_width), Size::fill());
        self.draw_value
            .draw_walk(cx, value_walk, Align { x: 1.0, y: 0.5 }, &text);

        self.draw_bg.end(cx);
        DrawStep::done()
    }

    fn handle_event(&mut self, cx: &mut Cx, event: &Event, _scope: &mut Scope) {
        let uid = self.widget_uid();
        // Off: nothing below answers.
        if !self.enabled {
            return;
        }
        if self.dragging && crate::modal::ModalAction::is_dismissal(event) {
            self.cancel_drag(cx, uid);
            return;
        }
        // The window going away ends whatever this row was in the middle of,
        // by either hand: the press is let go and the value it found put
        // back, and a keyboard run pays the commit it owes. Whatever would
        // have ended either one -- the release, the key coming up -- is
        // going to the app that took the focus. `Hit::KeyFocusLost` does not
        // stand in for it: the focus INSIDE this app has not moved, so a held
        // arrow and an alt-tab left the last value a hand nudged sitting on
        // the row with nothing having been told about it. Ordered as
        // `set_enabled` orders the same pair, so that what is committed is
        // the value the row is left standing on.
        if let Event::WindowLostFocus(_) = event {
            self.cancel_drag(cx, uid);
            self.end_key_run(cx, uid);
            return;
        }
        // Escape, Back or the right button puts back the value the press
        // found.
        if self.dragging {
            match event {
                Event::KeyDown(ke)
                    if ke.key_code == KeyCode::Escape
                        && self.cancel_scope.as_ref().is_some_and(|s| cx.owns_cancel(s)) =>
                {
                    self.cancel_drag(cx, uid);
                    return;
                }
                Event::BackPressed { .. }
                    if self.cancel_scope.as_ref().is_some_and(|s| cx.owns_cancel(s))
                        && event.back_pressed() =>
                {
                    self.cancel_drag(cx, uid);
                    return;
                }
                Event::MouseDown(me) if me.button.is_secondary() => {
                    self.cancel_drag(cx, uid);
                    return;
                }
                _ => {}
            }
        }

        // One area, asked plainly: no sweep area and no capture overload,
        // so the press this takes is the press nothing else is holding. And
        // no wheel arm below, deliberately — the panel these sit in scrolls,
        // and a row that ate the wheel would be a row you could not get past.
        match event.hits(cx, self.draw_bg.area()) {
            Hit::FingerHoverIn(fe) | Hit::FingerHoverOver(fe) => {
                cx.set_cursor(match self.zone_at(cx, fe.abs.x) {
                    SliderZone::Track => MouseCursor::Grab,
                    SliderZone::Label => MouseCursor::Hand,
                    SliderZone::Readout => MouseCursor::Default,
                });
                if !self.hovered {
                    self.hovered = true;
                    self.draw_bg.redraw(cx);
                }
            }
            Hit::FingerHoverOut(_) => {
                self.hovered = false;
                self.draw_bg.redraw(cx);
            }
            Hit::FingerDown(fe) if fe.device.is_primary_hit() => {
                cx.set_key_focus(self.draw_bg.area());
                self.press_value = self.value;
                // `hits` took the mouse on the way in and holds it until the
                // release, which is what a scroller around this control asks
                // the capture list about before it drags its own content.
                self.cancel_scope = Some(self.begin_cancel_scope(cx));
                if self.zone_at(cx, fe.abs.x) == SliderZone::Track {
                    // A track is not a scrub: the thumb goes where the
                    // finger is, on the press itself, and stays under it.
                    self.dragging = true;
                    let v = self.value_at_pointer(cx, fe.abs.x);
                    self.publish(cx, uid, v, false);
                }
                self.draw_bg.redraw(cx);
            }
            Hit::FingerMove(fe) => {
                if self.dragging {
                    let v = self.value_at_pointer(cx, fe.abs.x);
                    self.publish(cx, uid, v, false);
                }
            }
            Hit::FingerUp(fe) => {
                self.cancel_scope = None;
                // A tap that began AND stayed on the name. `was_tap` rather
                // than `is_over`, so a drag that merely started there ends as
                // the drag it was; and it is the press that is asked about,
                // so the box is tested against `abs_start`.
                let tapped_label = fe.was_tap()
                    && !self.label.is_empty()
                    && self.label_area.rect(cx).contains(fe.abs_start);
                if tapped_label {
                    self.reset(cx, uid);
                }
                if self.dragging || tapped_label {
                    // After the reset and never before it: the commit is what
                    // a host writes down, and it has to carry the zero.
                    cx.widget_action(uid, FabSliderAction::Ended(self.value));
                }
                self.dragging = false;
                self.draw_bg.redraw(cx);
            }
            // Ctrl and Cmd are the accelerator space, and that space belongs
            // to whatever this row is sitting in: Ctrl+Home is the panel going
            // to its top, Cmd+Arrow is the window manager's. A focused row
            // that nudged on either would break those shortcuts silently, and
            // only while the keyboard happened to be resting on it. Shift is
            // this control's own -- the coarse step -- and Alt is left
            // unclaimed, which is where a fine step would go.
            Hit::KeyDown(ke) if !ke.modifiers.control && !ke.modifiers.logo => {
                match ke.key_code {
                    KeyCode::ArrowLeft | KeyCode::ArrowDown => {
                        self.nudge(cx, uid, -1.0, ke.modifiers.shift, ke.is_repeat)
                    }
                    KeyCode::ArrowRight | KeyCode::ArrowUp => {
                        self.nudge(cx, uid, 1.0, ke.modifiers.shift, ke.is_repeat)
                    }
                    // Absolute, both of them: the first press names the stop
                    // and every repeat after it names the same stop, so a key
                    // held against the end of its own travel goes quiet.
                    KeyCode::Home => {
                        let v = self.travel().stops().0;
                        self.key_step(cx, uid, v, ke.is_repeat);
                    }
                    KeyCode::End => {
                        let v = self.travel().stops().1;
                        self.key_step(cx, uid, v, ke.is_repeat);
                    }
                    _ => {}
                }
            }
            // Letting go of the key that was driving the value ends the
            // gesture, the way letting go of the mouse button does.
            Hit::KeyUp(ke)
                if matches!(
                    ke.key_code,
                    KeyCode::ArrowLeft
                        | KeyCode::ArrowRight
                        | KeyCode::ArrowUp
                        | KeyCode::ArrowDown
                        | KeyCode::Home
                        | KeyCode::End
                ) =>
            {
                self.end_key_run(cx, uid);
            }
            Hit::KeyFocus(_) => {
                self.draw_bg.redraw(cx);
            }
            Hit::KeyFocusLost(_) => {
                // The keyboard has gone elsewhere and the release will go with
                // it, so what the run owes is paid here or never.
                self.end_key_run(cx, uid);
                self.draw_bg.redraw(cx);
            }
            _ => {}
        }
    }
}

impl FabSliderRef {
    pub fn changed(&self, actions: &Actions) -> Option<f64> {
        if let Some(item) = actions.find_widget_action(self.widget_uid()) {
            if let FabSliderAction::Changed(v) = item.cast() {
                return Some(v);
            }
        }
        None
    }

    pub fn ended(&self, actions: &Actions) -> Option<f64> {
        slider_ended_value(actions, self.widget_uid())
    }

    /// Was the name clicked? The value is already back at zero; this is the
    /// host's cue to drop the row from whatever ledger it keeps.
    pub fn was_reset(&self, actions: &Actions) -> bool {
        actions
            .filter_widget_actions_cast::<FabSliderAction>(self.widget_uid())
            .any(|action| matches!(action, FabSliderAction::Reset))
    }

    pub fn set_value(&self, cx: &mut Cx, v: f64) {
        if let Some(mut inner) = self.borrow_mut() {
            inner.set_value(cx, v);
        }
    }

    /// See [`FabSlider::set_value_and_readout`]: what the row holds and what
    /// it prints, for a host whose column is rounded over the column.
    pub fn set_value_and_readout(&self, cx: &mut Cx, v: f64, readout: f64) {
        if let Some(mut inner) = self.borrow_mut() {
            inner.set_value_and_readout(cx, v, readout);
        }
    }

    pub fn value(&self) -> f64 {
        self.borrow().map_or(0.0, |i| i.value())
    }

    /// What the row prints, which is what it holds unless a host said
    /// otherwise.
    pub fn readout(&self) -> f64 {
        self.borrow().map_or(0.0, |i| i.readout())
    }

    pub fn set_label(&self, cx: &mut Cx, text: &str) {
        if let Some(mut inner) = self.borrow_mut() {
            inner.set_label(cx, text);
        }
    }

    pub fn set_enabled(&self, cx: &mut Cx, enabled: bool) {
        if let Some(mut inner) = self.borrow_mut() {
            inner.set_enabled(cx, enabled);
        }
    }

    pub fn enabled(&self) -> bool {
        self.borrow().map_or(true, |i| i.enabled())
    }
}

/// `Changed` comes before `Ended` in the same buffer and
/// `find_widget_action` answers with the first of them, so the commit has to
/// be looked for rather than found.
fn slider_ended_value(actions: &Actions, uid: WidgetUid) -> Option<f64> {
    for action in actions.filter_widget_actions_cast::<FabSliderAction>(uid) {
        if let FabSliderAction::Ended(v) = action {
            return Some(v);
        }
    }
    None
}

// ===========================================================================
// FabKnob — the slider's number on a dial, for where a row is too much room:
// a cell of a matrix. It holds, steps and reports exactly as the slider does,
// so a panel treats the two alike; what differs is the gesture. A dial has no
// track to press on, so the value is how far the pointer has COME and not
// where it is, and the law of that is pure for the same reason the slider's
// is.
// ===========================================================================

#[derive(Script, ScriptHook)]
#[repr(C)]
pub struct DrawFabKnob {
    #[deref]
    draw_super: DrawQuad,
    #[live]
    hover: f32,
    #[live]
    down: f32,
    #[live]
    focus: f32,
    #[live]
    disabled: f32,
    /// Where the value sits along the sweep, 0..1. Nought is drawn as OFF.
    #[live]
    travel: f32,
    /// The name's row and the number's, in pixels, either of them nought
    /// where there is no such row. The shader finds the face from these two
    /// and its own box, by the arithmetic of [`knob_face`].
    #[live]
    label_px: f32,
    #[live]
    readout_px: f32,
}

#[derive(Clone, Debug, Default)]
pub enum FabKnobAction {
    /// The value moved. Live under a drag, under the wheel, and under every
    /// arrow key including the repeats the keyboard sends while one is held.
    Changed(f64),
    /// The gesture that was moving the value is over, and this is the value
    /// it came to rest on. A commit, as [`FabSliderAction::Ended`] is one: a
    /// host that rate-limits `Changed` is meant to spend this one at once.
    ///
    /// At most ONE per gesture. A release ends a drag; a deliberate key press
    /// ends itself and the release of a HELD key ends the run of repeats it
    /// sent; a spin of the wheel ends when the wheel has been still for
    /// [`KNOB_WHEEL_SETTLE`]; a double click ends itself. Where a run ends
    /// without its ending -- the keyboard moving on, the knob being switched
    /// off, the window losing the focus -- what it owes is paid there.
    ///
    /// And NONE for a press that moved nothing. A knob is pressed to be given
    /// the keyboard far more often than a track is, there are a hundred of
    /// them on the panel this was built for, and a commit is a host dropping
    /// everything to install what it was handed: a click that changed nothing
    /// has nothing to install.
    Ended(f64),
    /// A double click: the knob is back at nought and the host should take it
    /// out of whatever it feeds. The slider's reset is a click on its name,
    /// and a knob in a matrix has no name to click.
    Reset,
    #[default]
    None,
}

/// How long the wheel has to have been still before a spin of it counts as
/// over, in seconds. One notch is not a gesture: a hand spins a wheel through
/// five or ten of them, and a commit for each is the held arrow's thirty
/// commits by another door.
pub const KNOB_WHEEL_SETTLE: f64 = 0.35;

/// How far a press has to travel before it is a drag, in points. The number
/// field's threshold and for the number field's reason: a knob is clicked to
/// be given the keyboard, and a careless click must not nudge the value it
/// was only meant to select.
pub const KNOB_DRAG_SLOP: f64 = 3.0;

/// The turn one knob carries into a gesture: the range it spans, the detent
/// it lands on, and how far a pointer travels to cross the whole of it.
///
/// The mapping is RELATIVE -- the value is how far the pointer has come since
/// the press, not where it is -- which is the whole difference between this
/// and [`SliderTravel`]. A dial drawn 28 across has no room for an absolute
/// law: a quarter of a turn would be eleven pixels. So the travel is a number
/// of its own and the same for every size of knob, and a hand that has
/// learned the rate on one has it on all of them.
///
/// Kept here, entire and with no `Cx` anywhere, so the law the drag uses is
/// the law the tests read.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct KnobTurn {
    pub min: f64,
    pub max: f64,
    /// The detent a value lands on; `0` is continuous.
    pub step: f64,
    /// The pointer travel that covers the whole range, in points.
    pub travel: f64,
}

impl KnobTurn {
    /// The stops, the detent and the containment are the SLIDER's, borrowed
    /// whole rather than written again: a number means the same thing in a
    /// cell of a matrix as in a row of sliders, and a host that offers both
    /// must not have the two disagree about what a hand asked for.
    fn law(&self) -> SliderTravel {
        SliderTravel {
            min: self.min,
            max: self.max,
            step: self.step,
            thumb: 0.0,
            inset: 0.0,
        }
    }

    /// The two stops in order, whichever way round they were written.
    pub fn stops(&self) -> (f64, f64) {
        self.law().stops()
    }

    /// A value onto the detent and inside the stops: where a HAND lands.
    pub fn settle(&self, v: f64) -> f64 {
        self.law().settle(v)
    }

    /// Inside the stops and nowhere near the detent: where a HOST's number
    /// is held. See [`SliderTravel::contain`].
    pub fn contain(&self, v: f64) -> f64 {
        self.law().contain(v)
    }

    /// Where a value sits along the sweep, 0..1.
    pub fn travel(&self, v: f64) -> f64 {
        self.law().travel(v)
    }

    /// The value a drag is carrying, after the pointer has risen `up` points
    /// (down is negative) from where it last was.
    ///
    /// What goes in and comes out is the RAW value -- what the hand has asked
    /// for, before the detent -- because a drag that settled on every move
    /// would round each pixel's worth back to where it started, and a fine
    /// drag over a coarse detent would never leave it. The caller settles
    /// what it publishes and keeps this.
    ///
    /// Held to the stops, though, and at once: a pointer that has gone a
    /// hand's width past the top must not have to come all the way back down
    /// before the value starts to fall. Clamping here moves the anchor with
    /// the hand, as the number field's scrub does.
    ///
    /// `fine` is Shift: a tenth of the speed, so the same travel covers a
    /// tenth of the range.
    pub fn carry(&self, raw: f64, up: f64, fine: bool) -> f64 {
        let (lo, hi) = self.stops();
        // A travel of nought or less is not a travel. It would divide into
        // infinity and pin the value to a stop on the first pixel, or run
        // the drag backwards.
        let travel = if self.travel > 0.0 { self.travel } else { 150.0 };
        let rate = if fine { 0.1 } else { 1.0 };
        (raw + up / travel * (hi - lo) * rate).clamp(lo, hi)
    }
}

/// Where the dial stands in its box.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct KnobFace {
    /// The face's diameter: the biggest circle the box holds once the text
    /// rows are taken off its height.
    pub diameter: f64,
    /// The top of the stack -- name, face, number -- from the top of the
    /// box. The slack is shared above and below, so a short knob in a tall
    /// cell sits in the middle of it.
    pub top: f64,
}

/// The face a box of `width` by `height` has room for, with `label_px` of
/// name over it and `readout_px` of number under it (nought for a row that
/// is not there).
///
/// Three lines, and the shader has the same three: this is what lays the
/// words out and that is what paints the dial, and they agree because they
/// are one piece of arithmetic written down twice and tested once.
pub fn knob_face(width: f64, height: f64, label_px: f64, readout_px: f64) -> KnobFace {
    let rows = label_px + readout_px;
    let diameter = width.min(height - rows).max(0.0);
    let top = ((height - rows - diameter) * 0.5).max(0.0);
    KnobFace { diameter, top }
}

/// The number under a knob: the slider's readout with the unit hard against
/// the number. A row has room for the space between them and a cell does not
/// -- `100 %` in the panel's small face is a point wider than a 28 point
/// cell, and it is the unit that falls off the end.
pub fn knob_readout(value: f64, precision: usize, unit: &str) -> String {
    let mut text = crate::slider::format_readout(value, precision, "");
    text.push_str(unit);
    text
}

/// Is a press at `time` and `abs` the second half of a double click whose
/// first half was the press in `first`?
///
/// Measured press to press, against the platform's own two numbers, and by
/// the knob itself rather than read off the event's tap count: that count is
/// kept by the platform's event loop, which a headless run does not have, so
/// a reset hung off it could be shipped and never once tested.
pub fn is_double_press(first: Option<(f64, Vec2d)>, time: f64, abs: Vec2d) -> bool {
    use crate::event::{TAP_COUNT_DISTANCE, TAP_COUNT_TIME};
    first.is_some_and(|(t, p)| {
        time - t < TAP_COUNT_TIME && (abs - p).length() < TAP_COUNT_DISTANCE
    })
}

#[derive(Script, ScriptHook, Widget)]
pub struct FabKnob {
    #[uid]
    uid: WidgetUid,
    #[source]
    source: ScriptObjectRef,
    #[redraw]
    #[live]
    draw_bg: DrawFabKnob,
    #[live]
    draw_label: DrawText,
    #[live]
    draw_value: DrawText,
    /// The number's ink while the knob stands at nought.
    #[live]
    draw_value_off: DrawText,
    #[walk]
    walk: Walk,
    #[layout]
    layout: Layout,

    /// The name over the face. Empty is no name AND no row for one.
    #[live]
    label: String,
    /// The name's row and the number's, in points. Fixed rather than fitted,
    /// for the slider's reason: a matrix is a grid of these, and the faces
    /// have to stand on one line all the way along a row.
    #[live(12.0)]
    label_height: f64,
    /// Whether the number is printed under the face. Off, its row goes too
    /// and the face takes the room.
    #[live(true)]
    show_readout: bool,
    #[live(12.0)]
    readout_height: f64,
    #[live]
    min: f64,
    #[live(100.0)]
    max: f64,
    /// The arrow-key and wheel increment, and the detent a drag lands on.
    #[live(1.0)]
    step: f64,
    /// Shift+arrow and Shift+wheel. Coarse, as it is on the slider, and for
    /// the slider's reason: one step is already the small gesture. Under a
    /// DRAG Shift is fine instead, because a drag is continuous and has
    /// somewhere smaller to go.
    #[live(10.0)]
    big_step: f64,
    #[live(0)]
    precision: usize,
    #[live]
    unit: String,
    #[live]
    value: f64,
    /// The pointer travel that covers the whole range, in points.
    #[live(150.0)]
    drag_travel: f64,
    /// Whether the wheel turns a knob the pointer is merely OVER.
    ///
    /// Off, the wheel turns only the knob that has the keyboard -- the one
    /// that was last pressed. The panel these sit in scrolls, and it is a
    /// wall of them: a knob that took every wheel that crossed it would be a
    /// panel that cannot be scrolled and a column of weights moved by
    /// accident on the way past. On is for a host whose knobs stand somewhere
    /// that does not scroll.
    #[live(false)]
    wheel_on_hover: bool,
    /// Off: the knob shows dimmed and nothing answers.
    #[live(true)]
    enabled: bool,

    /// Held for the length of a gesture, so Escape and a modal's dismissal
    /// reach this control rather than whatever it is sitting in.
    #[rust]
    cancel_scope: Option<CancelScope>,
    /// A press is down and is this knob's to turn.
    #[rust]
    dragging: bool,
    /// The press has travelled [`KNOB_DRAG_SLOP`] and is turning the knob.
    #[rust]
    engaged: bool,
    /// The drag has said `Changed` at least once, so its release owes a
    /// commit.
    #[rust]
    drag_said: bool,
    /// What the drag has asked for, before the detent. See
    /// [`KnobTurn::carry`].
    #[rust]
    drag_raw: f64,
    /// Where the pointer was at the last move. The drag is summed move by
    /// move rather than measured from the press, so that Shift can come and
    /// go in the middle of one without the value jumping to where the other
    /// rate would have had it.
    #[rust]
    drag_last_y: f64,
    /// What the value was when the press landed, for a cancel to put back.
    #[rust]
    press_value: f64,
    /// When and where the last press landed, while it could still be the
    /// first half of a double click.
    #[rust]
    first_press: Option<(f64, Vec2d)>,
    /// The number this knob PRINTS, where that is not the number it holds.
    /// See `FabSlider::readout`: a column rounded over the column.
    #[rust]
    readout: Option<f64>,
    #[rust]
    hovered: bool,
    /// A keyboard run owes a commit. See `FabSlider::key_commit_due`.
    #[rust]
    key_commit_due: bool,
    /// Wheel travel that has not yet made a whole notch. A trackpad sends a
    /// notch as a dozen small deltas.
    #[rust]
    wheel_carry: f64,
    /// A spin of the wheel owes a commit, and this is the clock it is paid
    /// on.
    #[rust]
    wheel_commit_due: bool,
    #[rust]
    wheel_timer: Timer,
}

impl FabKnob {
    fn turn(&self) -> KnobTurn {
        KnobTurn {
            min: self.min,
            max: self.max,
            step: self.step,
            travel: self.drag_travel,
        }
    }

    /// The two text rows as they stand: nought for the one that is not there.
    fn text_rows(&self) -> (f64, f64) {
        (
            if self.label.is_empty() { 0.0 } else { self.label_height.max(0.0) },
            if self.show_readout { self.readout_height.max(0.0) } else { 0.0 },
        )
    }

    pub fn value(&self) -> f64 {
        self.value
    }

    /// A value pushed in from outside, held EXACTLY as it was handed over and
    /// clamped to the stops and to nothing else. Emits nothing. Everything
    /// [`FabSlider::set_value`] says about the detent being the hand's grid
    /// and never a filter on the host's arithmetic is true here, and for the
    /// same rows of weights.
    ///
    /// Refused mid-drag: a host answering late must not argue with the hand
    /// that is on the knob.
    pub fn set_value(&mut self, cx: &mut Cx, v: f64) {
        if self.dragging {
            return;
        }
        self.hold(cx, v, None);
    }

    /// The number the knob HOLDS and the number it PRINTS, handed over
    /// together, for the host whose readout is rounded over a whole column.
    /// See [`FabSlider::set_value_and_readout`]. Emits nothing; refused
    /// mid-drag.
    pub fn set_value_and_readout(&mut self, cx: &mut Cx, v: f64, readout: f64) {
        if self.dragging {
            return;
        }
        self.hold(cx, v, Some(readout));
    }

    /// What the knob prints: what a host said to print, or what it holds.
    pub fn readout(&self) -> f64 {
        self.readout.unwrap_or(self.value)
    }

    /// The one door a host's number comes in by. The redraw hangs off the
    /// PAIR, as the slider's does.
    fn hold(&mut self, cx: &mut Cx, v: f64, readout: Option<f64>) {
        let v = self.turn().contain(v);
        if (v - self.value).abs() > f64::EPSILON || readout != self.readout {
            self.value = v;
            self.readout = readout;
            self.draw_bg.redraw(cx);
        }
    }

    pub fn label(&self) -> &str {
        &self.label
    }

    /// The knob's name. Empty takes the name's row away with it.
    pub fn set_label(&mut self, cx: &mut Cx, text: &str) {
        if self.label != text {
            self.label = text.to_string();
            self.draw_bg.redraw(cx);
        }
    }

    pub fn enabled(&self) -> bool {
        self.enabled
    }

    /// Switching off mid-gesture ends the gesture first, by every hand: the
    /// pointer is let go and the value the press found put back, and a run
    /// of the keyboard or of the wheel that has not committed yet pays up. A
    /// knob nothing can reach will never see what would have ended them.
    pub fn set_enabled(&mut self, cx: &mut Cx, enabled: bool) {
        if self.enabled == enabled {
            return;
        }
        self.enabled = enabled;
        if !enabled {
            let uid = self.widget_uid();
            self.cancel_drag(cx, uid);
            self.end_runs(cx, uid);
            self.hovered = false;
        }
        self.draw_bg.redraw(cx);
    }

    /// Answers whether the value actually moved.
    fn publish(&mut self, cx: &mut Cx, uid: WidgetUid, v: f64, ended: bool) -> bool {
        let moved = (v - self.value).abs() > f64::EPSILON;
        if moved {
            self.value = v;
            // The hand's number is the knob's own; see `FabSlider::publish`.
            self.readout = None;
            self.draw_bg.redraw(cx);
            cx.widget_action(uid, FabKnobAction::Changed(self.value));
        }
        if ended {
            cx.widget_action(uid, FabKnobAction::Ended(self.value));
        }
        moved
    }

    /// One key's worth of movement, and whether it ends anything: a fresh
    /// press commits where it lands, a repeat says only that the value moved
    /// and leaves the commit to the release. `FabSlider::key_step`, whole.
    fn key_step(&mut self, cx: &mut Cx, uid: WidgetUid, v: f64, repeat: bool) {
        let moved = self.publish(cx, uid, v, !repeat);
        if repeat {
            self.key_commit_due |= moved;
        } else {
            self.key_commit_due = false;
        }
    }

    /// The commit a keyboard run still owes.
    fn end_key_run(&mut self, cx: &mut Cx, uid: WidgetUid) {
        if self.key_commit_due {
            self.key_commit_due = false;
            cx.widget_action(uid, FabKnobAction::Ended(self.value));
        }
    }

    /// The commit a spin of the wheel still owes, paid when the wheel has
    /// been still long enough -- or at whatever ends the spin before that.
    fn end_wheel_run(&mut self, cx: &mut Cx, uid: WidgetUid) {
        cx.stop_timer(self.wheel_timer);
        self.wheel_timer = Timer::default();
        self.wheel_carry = 0.0;
        if self.wheel_commit_due {
            self.wheel_commit_due = false;
            cx.widget_action(uid, FabKnobAction::Ended(self.value));
        }
    }

    /// Both of the runs that end on something other than a release.
    fn end_runs(&mut self, cx: &mut Cx, uid: WidgetUid) {
        self.end_key_run(cx, uid);
        self.end_wheel_run(cx, uid);
    }

    /// One step's worth, or `big_step`'s; a hundredth of the range where the
    /// knob is continuous, which is the arrow-key equivalent of one percent.
    fn increment(&self, big: bool) -> f64 {
        let step = if big { self.big_step } else { self.step };
        if step > 0.0 {
            step
        } else {
            (self.max - self.min).abs() * 0.01
        }
    }

    /// One step from where the knob actually STANDS, which may be off the
    /// detent because a host put it there. See `FabSlider::nudge`.
    fn nudge(&mut self, cx: &mut Cx, uid: WidgetUid, direction: f64, big: bool, repeat: bool) {
        let v = self.turn().contain(self.value + direction * self.increment(big));
        self.key_step(cx, uid, v, repeat);
    }

    /// The wheel, a notch at a time. Up is more. What is left of a notch is
    /// kept for the next event, so a trackpad's dozen small deltas add up to
    /// the step a wheel's one click is.
    fn wheel(&mut self, cx: &mut Cx, uid: WidgetUid, scroll: Vec2d, big: bool) {
        // Shift turns a wheel sideways on some platforms; it is the same
        // wheel.
        let axis = if scroll.y != 0.0 { -scroll.y } else { -scroll.x };
        self.wheel_carry += axis / 120.0;
        let notches = self.wheel_carry.trunc();
        if notches == 0.0 {
            return;
        }
        self.wheel_carry -= notches;
        let v = self.turn().contain(self.value + notches * self.increment(big));
        if self.publish(cx, uid, v, false) {
            self.wheel_commit_due = true;
        }
        if self.wheel_commit_due {
            cx.stop_timer(self.wheel_timer);
            self.wheel_timer = cx.start_timeout(KNOB_WHEEL_SETTLE);
        }
    }

    /// NOUGHT, as the slider's reset is and for its reason: "this one counts
    /// for nothing" is the same number whichever cell it is asked of. A range
    /// that never reaches nought takes its nearest stop instead. The commit
    /// goes out with it, because a double click is over when it lands.
    fn reset(&mut self, cx: &mut Cx, uid: WidgetUid) {
        let v = self.turn().settle(0.0);
        self.publish(cx, uid, v, false);
        cx.widget_action(uid, FabKnobAction::Reset);
        cx.widget_action(uid, FabKnobAction::Ended(self.value));
    }

    fn cancel_drag(&mut self, cx: &mut Cx, uid: WidgetUid) {
        self.cancel_scope = None;
        if self.dragging {
            self.dragging = false;
            self.engaged = false;
            self.drag_said = false;
            let back = self.press_value;
            self.publish(cx, uid, back, false);
            self.draw_bg.redraw(cx);
        }
    }
}

impl Widget for FabKnob {
    // The generic switch and the bridge's `enabled` column both come through
    // here, so what they say is what the knob does.
    fn set_disabled(&mut self, cx: &mut Cx, disabled: bool) {
        self.set_enabled(cx, !disabled);
    }

    fn disabled(&self, _cx: &Cx) -> bool {
        !self.enabled
    }

    fn draw_walk(&mut self, cx: &mut Cx2d, _scope: &mut Scope, walk: Walk) -> DrawStep {
        let (label_px, readout_px) = self.text_rows();
        self.draw_bg.label_px = label_px as f32;
        self.draw_bg.readout_px = readout_px as f32;
        self.draw_bg.travel = self.turn().travel(self.value) as f32;
        self.draw_bg.hover = if self.hovered && self.enabled { 1.0 } else { 0.0 };
        self.draw_bg.down = if self.dragging { 1.0 } else { 0.0 };
        self.draw_bg.focus = if cx.cx.cx.has_key_focus(self.draw_bg.area()) {
            1.0
        } else {
            0.0
        };
        self.draw_bg.disabled = if self.enabled { 0.0 } else { 1.0 };
        // The face is measured off the whole box, so the box is laid out
        // with no padding whatever the caller wrote: see the template.
        let layout = Layout {
            padding: Inset::default(),
            spacing: 0.0,
            ..self.layout
        };
        self.draw_bg.begin(cx, walk, layout);

        // A box that is Fit on an axis has no size yet. A Fit height comes
        // out as a square face with its rows round it, which is what the
        // arithmetic gives for a box exactly as tall as its stack; a Fit
        // width has nothing to be measured against and takes the default.
        let size = cx.turtle().rect().size;
        let width = if size.x.is_finite() { size.x } else { 44.0 };
        let height = if size.y.is_finite() {
            size.y
        } else {
            width + label_px + readout_px
        };
        let face = knob_face(width, height, label_px, readout_px);
        let row = |h: f64| Walk::new(Size::Fixed(width), Size::Fixed(h));

        if face.top > 0.0 {
            let _ = cx.walk_turtle(row(face.top));
        }
        if label_px > 0.0 {
            self.draw_label
                .draw_walk(cx, row(label_px), Align { x: 0.5, y: 0.5 }, &self.label);
        }
        // The dial itself is painted by the face underneath; this only
        // claims the height the shader gives it, so the number lands under
        // the dial and not on it.
        let _ = cx.walk_turtle(row(face.diameter));
        if readout_px > 0.0 {
            let text = knob_readout(self.readout(), self.precision, &self.unit);
            let lit = self.turn().travel(self.value) > 0.0005;
            let ink = if lit {
                &mut self.draw_value
            } else {
                &mut self.draw_value_off
            };
            ink.draw_walk(cx, row(readout_px), Align { x: 0.5, y: 0.5 }, &text);
        }

        self.draw_bg.end(cx);
        DrawStep::done()
    }

    fn handle_event(&mut self, cx: &mut Cx, event: &Event, _scope: &mut Scope) {
        let uid = self.widget_uid();
        // Off: nothing below answers.
        if !self.enabled {
            return;
        }
        if self.dragging && crate::modal::ModalAction::is_dismissal(event) {
            self.cancel_drag(cx, uid);
            return;
        }
        // The window going away ends whatever this knob was in the middle
        // of, by any hand. Ordered as `set_enabled` orders the same things,
        // so that what is committed is the value the knob is left on. See
        // the same arm of `FabSlider::handle_event` for why `KeyFocusLost`
        // does not stand in for this.
        if let Event::WindowLostFocus(_) = event {
            self.cancel_drag(cx, uid);
            self.end_runs(cx, uid);
            return;
        }
        // The wheel has been still long enough: the spin is over.
        if self.wheel_timer.is_event(event).is_some() {
            self.end_wheel_run(cx, uid);
            return;
        }
        // Escape, Back or the right button puts back the value the press
        // found.
        if self.dragging {
            match event {
                Event::KeyDown(ke)
                    if ke.key_code == KeyCode::Escape
                        && self.cancel_scope.as_ref().is_some_and(|s| cx.owns_cancel(s)) =>
                {
                    self.cancel_drag(cx, uid);
                    return;
                }
                Event::BackPressed { .. }
                    if self.cancel_scope.as_ref().is_some_and(|s| cx.owns_cancel(s))
                        && event.back_pressed() =>
                {
                    self.cancel_drag(cx, uid);
                    return;
                }
                Event::MouseDown(me) if me.button.is_secondary() => {
                    self.cancel_drag(cx, uid);
                    return;
                }
                _ => {}
            }
        }

        // THE POINTER-CAPTURE RULE. One area, asked plainly: no sweep area
        // and no capture overload, so the press this takes is the press
        // nothing else is holding, and `hits` holds it until the release. A
        // drag that leaves the cell -- and every drag on a 28 point knob
        // leaves the cell -- goes on turning this knob and lights up nothing
        // it passes over.
        match event.hits(cx, self.draw_bg.area()) {
            Hit::FingerHoverIn(_) | Hit::FingerHoverOver(_) => {
                cx.set_cursor(MouseCursor::NsResize);
                if !self.hovered {
                    self.hovered = true;
                    self.draw_bg.redraw(cx);
                }
            }
            Hit::FingerHoverOut(_) => {
                self.hovered = false;
                self.draw_bg.redraw(cx);
            }
            Hit::FingerDown(fe) if fe.device.is_primary_hit() => {
                cx.set_key_focus(self.draw_bg.area());
                // A press is the end of whatever the wheel was doing.
                self.end_wheel_run(cx, uid);
                self.cancel_scope = Some(self.begin_cancel_scope(cx));
                if is_double_press(self.first_press, fe.time, fe.abs) {
                    // The second half of a double click is a command and
                    // not a grip: it turns nothing, and a third press starts
                    // the count again.
                    self.first_press = None;
                    self.reset(cx, uid);
                } else {
                    self.first_press = Some((fe.time, fe.abs));
                    self.dragging = true;
                    self.engaged = false;
                    self.drag_said = false;
                    self.press_value = self.value;
                    self.drag_raw = self.value;
                    self.drag_last_y = fe.abs.y;
                }
                self.draw_bg.redraw(cx);
            }
            Hit::FingerMove(fe) => {
                if !self.dragging {
                    return;
                }
                // Up and down only. A hand pulling a knob up wanders
                // sideways, and a sideways inch that counted for anything
                // would make every drag a slightly different one.
                if !self.engaged {
                    let come = fe.abs.y - self.drag_last_y;
                    if come.abs() < KNOB_DRAG_SLOP {
                        return;
                    }
                    // Measured from the EDGE of the slop, so the slop is
                    // travel the value never sees rather than a jump it takes
                    // on engaging -- and so whatever the pointer has come
                    // beyond it counts, however few moves it came in.
                    self.engaged = true;
                    self.drag_last_y += come.signum() * KNOB_DRAG_SLOP;
                    // A press that has travelled is a drag, and a drag is
                    // not the first half of a double click.
                    self.first_press = None;
                }
                let up = self.drag_last_y - fe.abs.y;
                self.drag_last_y = fe.abs.y;
                if up == 0.0 {
                    return;
                }
                let turn = self.turn();
                self.drag_raw = turn.carry(self.drag_raw, up, fe.modifiers.shift);
                let v = turn.settle(self.drag_raw);
                if self.publish(cx, uid, v, false) {
                    self.drag_said = true;
                }
            }
            Hit::FingerUp(fe) => {
                self.cancel_scope = None;
                if self.dragging && self.drag_said {
                    cx.widget_action(uid, FabKnobAction::Ended(self.value));
                }
                self.dragging = false;
                self.engaged = false;
                self.drag_said = false;
                // A drag on a knob ends somewhere else more often than not,
                // and a release off the knob is the last this area hears of
                // that pointer: no hover-out follows it, because the hover
                // was never handed back. Unanswered, the knob stays lit
                // until the pointer happens to cross it again.
                self.hovered = fe.is_over && fe.device.has_hovers();
                self.draw_bg.redraw(cx);
            }
            Hit::FingerScroll(fe)
                if self.wheel_on_hover || cx.has_key_focus(self.draw_bg.area()) =>
            {
                // A hand that is on the knob already has it.
                if !self.dragging {
                    self.wheel(cx, uid, fe.scroll, fe.modifiers.shift);
                }
                // The wheel a knob took is spent, whether or not the notch
                // moved it: a knob at its stop still holds the panel, or the
                // last notch of a spin would turn into a scroll.
                event.set_scroll_handled(Vec2Index::X);
                event.set_scroll_handled(Vec2Index::Y);
            }
            // Ctrl and Cmd are the accelerator space and belong to whatever
            // this knob is sitting in; see the same arm of the slider.
            Hit::KeyDown(ke) if !ke.modifiers.control && !ke.modifiers.logo => {
                match ke.key_code {
                    KeyCode::ArrowLeft | KeyCode::ArrowDown => {
                        self.nudge(cx, uid, -1.0, ke.modifiers.shift, ke.is_repeat)
                    }
                    KeyCode::ArrowRight | KeyCode::ArrowUp => {
                        self.nudge(cx, uid, 1.0, ke.modifiers.shift, ke.is_repeat)
                    }
                    // Absolute, both of them, so a key held against the end
                    // of its own travel goes quiet.
                    KeyCode::Home => {
                        let v = self.turn().stops().0;
                        self.key_step(cx, uid, v, ke.is_repeat);
                    }
                    KeyCode::End => {
                        let v = self.turn().stops().1;
                        self.key_step(cx, uid, v, ke.is_repeat);
                    }
                    _ => {}
                }
            }
            // Letting go of the key that was driving the value ends the
            // gesture, the way letting go of the mouse button does.
            Hit::KeyUp(ke)
                if matches!(
                    ke.key_code,
                    KeyCode::ArrowLeft
                        | KeyCode::ArrowRight
                        | KeyCode::ArrowUp
                        | KeyCode::ArrowDown
                        | KeyCode::Home
                        | KeyCode::End
                ) =>
            {
                self.end_key_run(cx, uid);
            }
            Hit::KeyFocus(_) => {
                self.draw_bg.redraw(cx);
            }
            Hit::KeyFocusLost(_) => {
                // The keyboard has gone elsewhere, and with it both the
                // release that would have ended a run of arrows and the
                // wheel's claim on this knob: what either owes is paid here
                // or never.
                self.end_runs(cx, uid);
                self.draw_bg.redraw(cx);
            }
            _ => {}
        }
    }
}

impl FabKnobRef {
    pub fn changed(&self, actions: &Actions) -> Option<f64> {
        if let Some(item) = actions.find_widget_action(self.widget_uid()) {
            if let FabKnobAction::Changed(v) = item.cast() {
                return Some(v);
            }
        }
        None
    }

    pub fn ended(&self, actions: &Actions) -> Option<f64> {
        knob_ended_value(actions, self.widget_uid())
    }

    /// Was the knob double clicked? The value is already back at nought and
    /// the commit for it is in the same buffer; this is the host's cue to
    /// drop the cell from whatever ledger it keeps.
    pub fn was_reset(&self, actions: &Actions) -> bool {
        actions
            .filter_widget_actions_cast::<FabKnobAction>(self.widget_uid())
            .any(|action| matches!(action, FabKnobAction::Reset))
    }

    /// Emits nothing. See [`FabKnob::set_value`].
    pub fn set_value(&self, cx: &mut Cx, v: f64) {
        if let Some(mut inner) = self.borrow_mut() {
            inner.set_value(cx, v);
        }
    }

    /// Emits nothing. See [`FabKnob::set_value_and_readout`].
    pub fn set_value_and_readout(&self, cx: &mut Cx, v: f64, readout: f64) {
        if let Some(mut inner) = self.borrow_mut() {
            inner.set_value_and_readout(cx, v, readout);
        }
    }

    pub fn value(&self) -> f64 {
        self.borrow().map_or(0.0, |i| i.value())
    }

    /// What the knob prints, which is what it holds unless a host said
    /// otherwise.
    pub fn readout(&self) -> f64 {
        self.borrow().map_or(0.0, |i| i.readout())
    }

    pub fn set_label(&self, cx: &mut Cx, text: &str) {
        if let Some(mut inner) = self.borrow_mut() {
            inner.set_label(cx, text);
        }
    }

    pub fn set_enabled(&self, cx: &mut Cx, enabled: bool) {
        if let Some(mut inner) = self.borrow_mut() {
            inner.set_enabled(cx, enabled);
        }
    }

    pub fn enabled(&self) -> bool {
        self.borrow().map_or(true, |i| i.enabled())
    }
}

/// `Changed` comes before `Ended` in the same buffer, so the commit has to
/// be looked for rather than found. See `slider_ended_value`.
fn knob_ended_value(actions: &Actions, uid: WidgetUid) -> Option<f64> {
    for action in actions.filter_widget_actions_cast::<FabKnobAction>(uid) {
        if let FabKnobAction::Ended(v) = action {
            return Some(v);
        }
    }
    None
}

// ===========================================================================
// FabDiagonalLabel — the name over a column too narrow to hold it.
//
// A matrix of knobs ten rows by eight columns, in a sidebar 280 wide, comes
// out around 26 points a column, and the names a panel has to write over
// such columns -- a theme's, a family's -- run to two and a half times that.
// So the name is turned on its side and let out over its neighbours,
// which is safe
// for the reason parallel lines are safe: at 45 degrees a pitch of 26 puts
// 26 * sin(45) = 18 points between one name and the next ACROSS the line,
// and a line of the panel's small face is ten, so however long the names get
// they never touch — only the empty ground beside them is crossed.
//
// The widget therefore draws OUTSIDE its own box on purpose. The box is an
// anchor and not a frame, and nothing here opens a turtle, because a turtle
// is exactly what would cut the name off: a begun turtle pushes its rect
// onto the clip stack, the align pass stamps that rect onto every instance
// drawn inside it, and `DrawText` discards the pixels outside it
// (`draw/src/turtle.rs`, `clip_and_shift_align_list`; the clip note at the
// top of `draw/src/shader/draw_text.rs`). The host owes the other half of
// that bargain: the row these stand in carries `clip_x: false`, and the
// ground the ink spills onto has to be inside whatever DOES clip.
//
// Where the baseline starts is the one thing that has to be exact — a name
// standing over the wrong column names the wrong column — so that is pure
// arithmetic, settled and tested without a window.
// ===========================================================================

/// Which way a diagonal name runs across the column it names.
///
/// Both are wanted, and what the choice really decides is which edge of the
/// panel the ink hangs over.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Script, ScriptHook)]
#[repr(u32)]
pub enum DiagonalLean {
    /// The name ENDS on its column's bottom centre, having begun up and to
    /// the left, and reads downward to the right. The overflow is to the
    /// LEFT, and over a matrix that is the empty corner above the row-name
    /// column — so the LAST column's name is never cut in half by the
    /// panel's right edge. That is why it is the default.
    #[pick]
    Fall = 0,
    /// The name STARTS on its column's bottom centre and rises to the upper
    /// right: the spreadsheet convention. The overflow is to the RIGHT, so a
    /// host that picks this owes its last column that much room.
    Rise = 1,
}

/// Where a diagonal name's baseline runs, and the box its ink takes.
///
/// Every coordinate is absolute, in the same space as the box handed in.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DiagonalRun {
    /// The first glyph's pen point.
    pub start: DVec2,
    /// One advance past the last glyph's: where the baseline stops.
    pub end: DVec2,
    /// The rotation every glyph is turned by, in radians and in SCREEN
    /// terms — y points down, so a `Fall` is positive and a `Rise`
    /// negative.
    pub angle: f64,
    /// Everything the ink covers. Normally wider than the box and usually
    /// taller: it is what must NOT be clipped, and what a host measures a
    /// header row against.
    pub bounds: Rect,
}

/// How tall a header row has to be to hold a name of `text_width` turned by
/// `angle_deg`, in points.
///
/// The name lies along the hypotenuse and its line stands across it, so the
/// row needs both: `text_width * sin(angle) + line_height * cos(angle)`. It
/// is the same for either lean — they differ in which way the ink leans, not
/// in how much room it takes. A host that cannot ask the widget (a height
/// written in the DSL) can do this sum with a ruler: at 45 degrees it is
/// about 0.71 of the longest name plus 0.71 of a line.
pub fn diagonal_row_height(text_width: f64, line_height: f64, angle_deg: f64) -> f64 {
    let rad = angle_deg.clamp(0.0, 90.0).to_radians();
    text_width.max(0.0) * rad.sin() + line_height.max(0.0) * rad.cos()
}

/// Where the name goes over the box it names.
///
/// The anchor is the box's BOTTOM CENTRE in both leans, and that is the
/// whole point of the control: whatever the name is, one end of it stands on
/// the middle of its own column, right above whatever the column holds. What
/// changes with the lean is WHICH end, and so which side the rest hangs over.
///
/// At nought degrees this degenerates to a plain horizontal label sitting on
/// the box's bottom edge — a `Fall` ending on the centre, a `Rise` starting
/// there — which is the honest answer rather than a special case: a column
/// wide enough not to need the trick does not need a different control.
pub fn diagonal_run(
    box_: Rect,
    text_width: f64,
    line_height: f64,
    angle_deg: f64,
    lean: DiagonalLean,
) -> DiagonalRun {
    let rad = angle_deg.clamp(0.0, 90.0).to_radians();
    let (sin, cos) = (rad.sin(), rad.cos());
    let width = text_width.max(0.0);
    let line = line_height.max(0.0);
    let anchor = dvec2(box_.pos.x + box_.size.x * 0.5, box_.pos.y + box_.size.y);
    // The glyph band lies from the baseline UP by one line, so the rotated
    // band reaches `width * sin + line * cos` above the anchor either way.
    let top = anchor.y - width * sin - line * cos;
    let (angle, start, end, left, right) = match lean {
        DiagonalLean::Fall => {
            let start = dvec2(anchor.x - width * cos, anchor.y - width * sin);
            // Leaning down to the right, the band's far corner is the one
            // that reaches furthest right; the pen point is the left edge.
            (rad, start, anchor, start.x, start.x + width * cos + line * sin)
        }
        DiagonalLean::Rise => {
            let end = dvec2(anchor.x + width * cos, anchor.y - width * sin);
            // Leaning up to the right, the band hangs back over the pen.
            (-rad, anchor, end, anchor.x - line * sin, anchor.x + width * cos)
        }
    };
    DiagonalRun {
        start,
        end,
        angle,
        bounds: Rect {
            pos: dvec2(left, top),
            size: dvec2(right - left, anchor.y - top),
        },
    }
}

/// A name written across the corner of the box it names, for a column too
/// narrow to hold it flat. No gesture, no focus, no actions: it is a label.
#[derive(Script, ScriptHook, Widget)]
pub struct FabDiagonalLabel {
    #[uid]
    uid: WidgetUid,
    #[source]
    source: ScriptObjectRef,
    #[live]
    draw_text: DrawRotatedText,
    #[walk]
    walk: Walk,

    /// The name.
    #[live]
    text: String,
    /// How far from the horizontal the name is turned, in degrees, 0 to 90.
    #[live(45.0)]
    angle: f64,
    /// Which way it leans, and so which side it hangs over.
    #[live]
    lean: DiagonalLean,

    /// The claimed box, kept so a set of the name can ask for the frame that
    /// shows it. The glyphs are not it: a label with nothing in it draws no
    /// glyphs and would then have no area to redraw from, which is exactly
    /// the moment a host fills the header in.
    #[redraw]
    #[rust]
    area: Area,
    /// Scratch for one name's glyphs, kept so a wall of these does not
    /// allocate once a frame each.
    #[rust]
    glyphs: Vec<PathGlyphInstance>,
    /// Where the last draw actually put the ink. A host that wants to know
    /// whether its row is tall enough can read it back rather than guess.
    #[rust]
    last_run: Option<DiagonalRun>,
}

impl FabDiagonalLabel {
    /// Where the last draw put the name, or `None` before it has drawn one.
    pub fn last_run(&self) -> Option<DiagonalRun> {
        self.last_run
    }

    /// How tall a header row has to be for `longest` at this label's angle
    /// and face, in points.
    ///
    /// The host asks once, with the longest name it will ever write, and
    /// fixes the row at the answer; every shorter name then hangs from the
    /// same bottom line. See [`diagonal_row_height`] for the sum itself,
    /// which a DSL can do without a widget.
    pub fn row_height_for(&self, cx: &mut Cx2d, longest: &str) -> f64 {
        match self.draw_text.prepare_single_line_run(cx, longest) {
            Some(run) => diagonal_row_height(
                run.width_in_lpxs as f64,
                (run.ascender_in_lpxs - run.descender_in_lpxs) as f64,
                self.angle,
            ),
            None => 0.0,
        }
    }
}

impl Widget for FabDiagonalLabel {
    fn text(&self) -> String {
        self.text.clone()
    }

    /// Emits nothing, and says nothing when the name has not changed: the
    /// host writes every header into a fixed slot on every draw, and a
    /// setter that dirtied the draw list each time would redraw the panel
    /// forever.
    fn set_text(&mut self, cx: &mut Cx, v: &str) {
        if self.text == v {
            return;
        }
        self.text.clear();
        self.text.push_str(v);
        self.redraw(cx);
    }

    fn draw_walk(&mut self, cx: &mut Cx2d, _scope: &mut Scope, walk: Walk) -> DrawStep {
        // The box is CLAIMED and then drawn over, never drawn in: no turtle
        // is begun here, so this widget pushes no clip of its own and the
        // name is free to cross its neighbours. See the note above.
        let rect = cx.walk_turtle(walk);
        cx.add_aligned_rect_area(&mut self.area, rect);
        self.last_run = None;
        // A Fill on an axis nothing has sized is a NaN box; there is no
        // centre to stand a name on, so nothing is drawn rather than a name
        // at nowhere.
        if self.text.is_empty() || !rect.size.x.is_finite() || !rect.size.y.is_finite() {
            return DrawStep::done();
        }
        let Some(run) = self.draw_text.prepare_single_line_run(cx, &self.text) else {
            return DrawStep::done();
        };
        let placed = diagonal_run(
            rect,
            run.width_in_lpxs as f64,
            (run.ascender_in_lpxs - run.descender_in_lpxs) as f64,
            self.angle,
            self.lean,
        );
        // A straight baseline is one direction for every glyph, so the walk
        // along it is the run's own pen advances and nothing else: the
        // rotation origin is the pen point, the ink sits one bearing along
        // from it, and every glyph turns by the same angle.
        let dir = dvec2(placed.angle.cos(), placed.angle.sin());
        let mut glyphs = std::mem::take(&mut self.glyphs);
        glyphs.clear();
        for glyph in &run.glyphs {
            if glyph.advance_in_lpxs <= 0.0 {
                continue;
            }
            let pen = placed.start + dir * glyph.pen_x_in_lpxs as f64;
            let ink = pen + dir * glyph.offset_x_in_lpxs as f64;
            glyphs.push(PathGlyphInstance {
                glyph_origin: Point::new(ink.x as f32, ink.y as f32),
                rotation_origin: Point::new(pen.x as f32, pen.y as f32),
                font_size_in_lpxs: glyph.font_size_in_lpxs,
                rasterized: glyph.rasterized,
                angle: placed.angle as f32,
            });
        }
        // One draw call for the whole name rather than one per letter. The
        // camera, warp and fade uniforms this shader carries for the map are
        // left where their defaults are — identity, no fold, no fade — so
        // what is drawn is the placement above and nothing on top of it.
        self.draw_text.begin_glyph_batch(cx);
        self.draw_text.draw_path_glyphs(cx, &glyphs);
        self.draw_text.end_glyph_batch(cx);
        self.glyphs = glyphs;
        self.last_run = Some(placed);
        DrawStep::done()
    }
}

impl FabDiagonalLabelRef {
    pub fn text(&self) -> String {
        self.borrow().map_or_else(String::new, |inner| inner.text())
    }

    /// Emits nothing. See [`FabDiagonalLabel::set_text`].
    pub fn set_text(&self, cx: &mut Cx, text: &str) {
        if let Some(mut inner) = self.borrow_mut() {
            inner.set_text(cx, text);
        }
    }

    /// Where the last draw put the name. See
    /// [`FabDiagonalLabel::last_run`].
    pub fn last_run(&self) -> Option<DiagonalRun> {
        self.borrow().and_then(|inner| inner.last_run())
    }
}

// ===========================================================================
// FabColorWheel
// ===========================================================================

#[derive(Script, ScriptHook)]
#[repr(C)]
pub struct DrawColorWheel {
    #[deref]
    draw_super: DrawQuad,
    #[live]
    hue: f32,
    #[live]
    sat: f32,
    #[live]
    val: f32,
}

#[derive(Clone, Debug, Default)]
pub enum ColorWheelAction {
    /// Live while dragging or nudging: (hue, sat, val), all 0..1.
    Changed([f32; 3]),
    /// The gesture finished (mouse up).
    Ended([f32; 3]),
    #[default]
    None,
}

#[derive(Script, ScriptHook, Widget)]
pub struct FabColorWheel {
    #[uid]
    uid: WidgetUid,
    #[source]
    source: ScriptObjectRef,
    #[redraw]
    #[live]
    draw_wheel: DrawColorWheel,
    #[walk]
    walk: Walk,
    #[rust]
    drag: Option<WheelZone>,
}

impl FabColorWheel {
    pub fn set_hsv(&mut self, cx: &mut Cx, h: f32, s: f32, v: f32) {
        if (h - self.draw_wheel.hue).abs() > f32::EPSILON
            || (s - self.draw_wheel.sat).abs() > f32::EPSILON
            || (v - self.draw_wheel.val).abs() > f32::EPSILON
        {
            self.draw_wheel.hue = h;
            self.draw_wheel.sat = s;
            self.draw_wheel.val = v;
            self.draw_wheel.redraw(cx);
        }
    }

    pub fn hsv(&self) -> [f32; 3] {
        [
            self.draw_wheel.hue,
            self.draw_wheel.sat,
            self.draw_wheel.val,
        ]
    }

    fn apply_pointer(&mut self, cx: &mut Cx, uid: WidgetUid, abs: DVec2, ended: bool) {
        let rect = self.draw_wheel.area().rect(cx);
        let size = rect.size.x.min(rect.size.y);
        let rel = abs - rect.pos;
        match self.drag {
            Some(WheelZone::Ring) => {
                self.draw_wheel.hue = ring_hue(rel, size);
            }
            Some(WheelZone::Square) => {
                let (s, v) = square_sv(rel, size);
                self.draw_wheel.sat = s;
                self.draw_wheel.val = v;
            }
            _ => return,
        }
        self.draw_wheel.redraw(cx);
        let hsv = self.hsv();
        cx.widget_action(uid, ColorWheelAction::Changed(hsv));
        if ended {
            cx.widget_action(uid, ColorWheelAction::Ended(hsv));
        }
    }

    fn nudge(&mut self, cx: &mut Cx, uid: WidgetUid, dh: f32, dv: f32) {
        self.draw_wheel.hue = (self.draw_wheel.hue + dh).rem_euclid(1.0);
        self.draw_wheel.val = (self.draw_wheel.val + dv).clamp(0.0, 1.0);
        self.draw_wheel.redraw(cx);
        let hsv = self.hsv();
        cx.widget_action(uid, ColorWheelAction::Changed(hsv));
        cx.widget_action(uid, ColorWheelAction::Ended(hsv));
    }
}

impl Widget for FabColorWheel {
    fn draw_walk(&mut self, cx: &mut Cx2d, _scope: &mut Scope, walk: Walk) -> DrawStep {
        let _ = self.draw_wheel.draw_walk(cx, walk);
        DrawStep::done()
    }

    fn handle_event(&mut self, cx: &mut Cx, event: &Event, _scope: &mut Scope) {
        let uid = self.widget_uid();
        match event.hits(cx, self.draw_wheel.area()) {
            Hit::FingerHoverIn(_) => {
                cx.set_cursor(MouseCursor::Crosshair);
            }
            Hit::FingerDown(fe) if fe.device.is_primary_hit() => {
                cx.set_key_focus(self.draw_wheel.area());
                let rect = self.draw_wheel.area().rect(cx);
                let size = rect.size.x.min(rect.size.y);
                let zone = wheel_zone(fe.abs - rect.pos, size);
                if zone != WheelZone::None {
                    self.drag = Some(zone);
                    self.apply_pointer(cx, uid, fe.abs, false);
                }
            }
            Hit::FingerMove(fe) => {
                if self.drag.is_some() {
                    self.apply_pointer(cx, uid, fe.abs, false);
                }
            }
            Hit::FingerUp(fe) => {
                if self.drag.is_some() {
                    self.apply_pointer(cx, uid, fe.abs, true);
                    self.drag = None;
                }
            }
            Hit::KeyDown(ke) => {
                let fine = if ke.modifiers.shift { 0.1 } else { 1.0 };
                match ke.key_code {
                    KeyCode::ArrowLeft => self.nudge(cx, uid, -fine / 360.0, 0.0),
                    KeyCode::ArrowRight => self.nudge(cx, uid, fine / 360.0, 0.0),
                    KeyCode::ArrowUp => self.nudge(cx, uid, 0.0, fine / 100.0),
                    KeyCode::ArrowDown => self.nudge(cx, uid, 0.0, -fine / 100.0),
                    _ => {}
                }
            }
            _ => {}
        }
    }
}

impl FabColorWheelRef {
    pub fn changed(&self, actions: &Actions) -> Option<[f32; 3]> {
        if let Some(item) = actions.find_widget_action(self.widget_uid()) {
            if let ColorWheelAction::Changed(hsv) = item.cast() {
                return Some(hsv);
            }
        }
        None
    }

    pub fn set_hsv(&self, cx: &mut Cx, h: f32, s: f32, v: f32) {
        if let Some(mut inner) = self.borrow_mut() {
            inner.set_hsv(cx, h, s, v);
        }
    }
}

// ===========================================================================
// FabPaletteStrip — a wrapped grid of small colour cells (one draw call),
// hit-tested by rect math. The host fills it (a theme palette); hover and
// click come back as indices.
// ===========================================================================

#[derive(Script, ScriptHook)]
#[repr(C)]
pub struct DrawFabPaletteCell {
    #[deref]
    draw_super: DrawQuad,
    #[live]
    pub cell: Vec4f,
    #[live]
    pub hot: f32,
    #[live]
    pub cur: f32,
}

#[derive(Clone, Debug, Default)]
pub enum FabPaletteAction {
    /// The pointer rests on a cell (None: it left the strip).
    Hover(Option<usize>),
    Pick(usize),
    #[default]
    None,
}

#[derive(Script, ScriptHook, Widget)]
pub struct FabPaletteStrip {
    #[uid]
    uid: WidgetUid,
    #[source]
    source: ScriptObjectRef,
    #[redraw]
    #[live]
    draw_cell: DrawFabPaletteCell,
    #[walk]
    walk: Walk,
    #[live]
    cell_size: f64,
    #[live]
    gap: f64,
    #[rust]
    colors: Vec<[f32; 4]>,
    #[rust]
    current: Option<usize>,
    #[rust]
    hot: Option<usize>,
    #[rust]
    cols: usize,
    #[rust]
    area: Area,
}

impl FabPaletteStrip {
    pub fn set_colors(&mut self, cx: &mut Cx, colors: Vec<[f32; 4]>) {
        self.colors = colors;
        self.hot = None;
        self.draw_cell.redraw(cx);
    }

    /// Mark the cell equal to the host's current colour.
    pub fn set_current(&mut self, cx: &mut Cx, current: Option<usize>) {
        if self.current != current {
            self.current = current;
            self.draw_cell.redraw(cx);
        }
    }

    fn pitch(&self) -> f64 {
        self.cell_size + self.gap
    }

    fn cols_for(&self, width: f64) -> usize {
        (((width + self.gap) / self.pitch()).floor() as usize).max(1)
    }

    /// The strip's height at a width (the popover sizes itself with it).
    pub fn height_for(&self, width: f64) -> f64 {
        if self.colors.is_empty() {
            return 0.0;
        }
        let rows = self.colors.len().div_ceil(self.cols_for(width));
        rows as f64 * self.pitch() - self.gap
    }

    fn cell_at(&self, rect: Rect, abs: DVec2) -> Option<usize> {
        if !rect.contains(abs) || self.cols == 0 {
            return None;
        }
        let rel = abs - rect.pos;
        let col = (rel.x / self.pitch()).floor() as usize;
        let row = (rel.y / self.pitch()).floor() as usize;
        if col >= self.cols {
            return None;
        }
        // The gap between cells belongs to nobody.
        if rel.x - col as f64 * self.pitch() > self.cell_size || rel.y - row as f64 * self.pitch() > self.cell_size {
            return None;
        }
        let index = row * self.cols + col;
        (index < self.colors.len()).then_some(index)
    }
}

impl Widget for FabPaletteStrip {
    fn draw_walk(&mut self, cx: &mut Cx2d, _scope: &mut Scope, walk: Walk) -> DrawStep {
        cx.begin_turtle(walk, Layout::flow_down());
        let width = cx.turtle().rect().size.x;
        self.cols = self.cols_for(width);
        let height = self.height_for(width);
        // Claim the grid's height, then paint the cells over it.
        let rect = cx.walk_turtle(Walk::new(Size::fill(), Size::Fixed(height)));
        let (pitch, cell_size) = (self.pitch(), self.cell_size);
        for (i, c) in self.colors.iter().enumerate() {
            let col = (i % self.cols) as f64;
            let row = (i / self.cols) as f64;
            self.draw_cell.cell = vec4(c[0], c[1], c[2], c[3]);
            self.draw_cell.hot = if self.hot == Some(i) { 1.0 } else { 0.0 };
            self.draw_cell.cur = if self.current == Some(i) { 1.0 } else { 0.0 };
            self.draw_cell.draw_abs(
                cx,
                Rect {
                    pos: dvec2(rect.pos.x + col * pitch, rect.pos.y + row * pitch),
                    size: dvec2(cell_size, cell_size),
                },
            );
        }
        cx.end_turtle_with_area(&mut self.area);
        DrawStep::done()
    }

    fn handle_event(&mut self, cx: &mut Cx, event: &Event, _scope: &mut Scope) {
        let uid = self.widget_uid();
        let rect = self.area.rect(cx);
        match event.hits(cx, self.area) {
            Hit::FingerHoverIn(fe) | Hit::FingerHoverOver(fe) => {
                let hot = self.cell_at(rect, fe.abs);
                cx.set_cursor(if hot.is_some() { MouseCursor::Hand } else { MouseCursor::Default });
                if hot != self.hot {
                    self.hot = hot;
                    self.draw_cell.redraw(cx);
                    cx.widget_action(uid, FabPaletteAction::Hover(hot));
                }
            }
            Hit::FingerHoverOut(_) => {
                if self.hot.is_some() {
                    self.hot = None;
                    self.draw_cell.redraw(cx);
                    cx.widget_action(uid, FabPaletteAction::Hover(None));
                }
            }
            Hit::FingerDown(fe) if fe.device.is_primary_hit() => {
                if let Some(i) = self.cell_at(rect, fe.abs) {
                    cx.widget_action(uid, FabPaletteAction::Pick(i));
                }
            }
            _ => {}
        }
    }
}

// ===========================================================================
// FabPaletteChip — a whole palette as one pressable stack of colour bands.
// Four colours in a single quad, hit-tested as one thing; the host writes the
// bands and says which chip is the one in force, and a press comes back.
// ===========================================================================

/// Four colours stacked in one quad, rounded as a whole.
///
/// One quad and not four: a chip is one thing to a hand -- it is pressed, it
/// is outlined, it is the palette -- and four boxes with a corner each would
/// have to be rounded outside-only and kept in step. The bands are chosen in
/// the shader off the fragment's own height.
#[derive(Script, ScriptHook)]
#[repr(C)]
pub struct DrawFabPaletteChip {
    #[deref]
    draw_super: DrawQuad,
    /// Top band first, the way the colours are read down the chip.
    #[live]
    pub band_0: Vec4f,
    #[live]
    pub band_1: Vec4f,
    #[live]
    pub band_2: Vec4f,
    #[live]
    pub band_3: Vec4f,
    #[live]
    pub hover: f32,
    /// The chip the host is wearing: a ring that stays on without a pointer.
    #[live]
    pub cur: f32,
}

#[derive(Clone, Debug, Default)]
pub enum FabPaletteChipAction {
    /// Pressed. There is nothing else a chip does.
    Pick,
    #[default]
    None,
}

/// One palette, pressable.
///
/// Its own control rather than a button with a colour written into it: what a
/// chip has to show is four colours at once, and what it has to do is take a
/// press. A button shows one colour and a row of coloured boxes takes none.
#[derive(Script, ScriptHook, Widget)]
pub struct FabPaletteChip {
    #[uid]
    uid: WidgetUid,
    #[source]
    source: ScriptObjectRef,
    #[redraw]
    #[live]
    draw_chip: DrawFabPaletteChip,
    #[walk]
    walk: Walk,
    /// Whether it is showing a palette at all.
    ///
    /// A hidden chip claims NOTHING, and that is the point of it: a strip of
    /// these holds a fixed row of slots dividing the row's width between
    /// them, and a slot whose chip has gone still holds its share. Hiding the
    /// slot instead would hand its width to its neighbours and a page with
    /// one palette on it would be one chip a whole row wide.
    #[live(true)]
    #[visible]
    pub visible: bool,
    #[rust]
    hover: bool,
    #[rust]
    current: bool,
}

impl FabPaletteChip {
    /// The four colours, top band first.
    ///
    /// Silent when nothing moved, for [`FabDiagonalLabel::set_text`]'s
    /// reason: a strip of these is written whole on every draw, and a setter
    /// that dirtied the draw list each time would redraw the panel forever.
    pub fn set_colors(&mut self, cx: &mut Cx, colors: [Vec4f; 4]) {
        let bands = [
            self.draw_chip.band_0,
            self.draw_chip.band_1,
            self.draw_chip.band_2,
            self.draw_chip.band_3,
        ];
        if bands == colors {
            return;
        }
        self.draw_chip.band_0 = colors[0];
        self.draw_chip.band_1 = colors[1];
        self.draw_chip.band_2 = colors[2];
        self.draw_chip.band_3 = colors[3];
        self.redraw(cx);
    }

    /// Whether this is the chip the host is wearing.
    pub fn set_current(&mut self, cx: &mut Cx, current: bool) {
        if self.current == current {
            return;
        }
        self.current = current;
        self.redraw(cx);
    }

    /// What it is showing, for a test that has to read the bands back off the
    /// widget rather than off the state that wrote them.
    pub fn colors(&self) -> [Vec4f; 4] {
        [
            self.draw_chip.band_0,
            self.draw_chip.band_1,
            self.draw_chip.band_2,
            self.draw_chip.band_3,
        ]
    }

    pub fn is_current(&self) -> bool {
        self.current
    }
}

impl Widget for FabPaletteChip {
    fn draw_walk(&mut self, cx: &mut Cx2d, _scope: &mut Scope, walk: Walk) -> DrawStep {
        if !self.visible {
            // Nothing drawn and no box claimed. The slot round it keeps its
            // share of the row all the same, which is the whole point.
            self.hover = false;
            return DrawStep::done();
        }
        self.draw_chip.hover = if self.hover { 1.0 } else { 0.0 };
        self.draw_chip.cur = if self.current { 1.0 } else { 0.0 };
        self.draw_chip.draw_walk(cx, walk);
        DrawStep::done()
    }

    /// Through `hits`, so that a press anywhere else in the host holds the
    /// pointer and this chip neither lights nor answers while it does; and
    /// the press it does take holds the pointer against everything else.
    ///
    /// The choice is made on the release and over the chip, the way every
    /// button is: a hand that came down on a palette and slid off it before
    /// letting go has chosen nothing, and a strip of these is a row of small
    /// targets side by side where that happens.
    fn handle_event(&mut self, cx: &mut Cx, event: &Event, _scope: &mut Scope) {
        let uid = self.widget_uid();
        match event.hits(cx, self.draw_chip.area()) {
            Hit::FingerHoverIn(_) => {
                cx.set_cursor(MouseCursor::Hand);
                if !self.hover {
                    self.hover = true;
                    self.redraw(cx);
                }
            }
            Hit::FingerHoverOut(_) => {
                if self.hover {
                    self.hover = false;
                    self.redraw(cx);
                }
            }
            // Taken, and nothing said: what it buys is the pointer, which is
            // what stops the chip beside it answering the release.
            Hit::FingerDown(fe) if fe.is_primary_hit() => {
                self.hover = true;
                self.redraw(cx);
            }
            Hit::FingerUp(fe) if fe.is_primary_hit() => {
                if fe.is_over {
                    cx.widget_action(uid, FabPaletteChipAction::Pick);
                } else if self.hover {
                    self.hover = false;
                    self.redraw(cx);
                }
            }
            _ => {}
        }
    }
}

// ===========================================================================
// FabColorPick — a swatch that opens a self-managed popover (wheel + RGBA
// rows + hex). No shell bus: the popover draws in an overlay draw list
// anchored at the swatch, outside-click commits, Escape reverts.
// ===========================================================================

#[derive(Script, ScriptHook)]
#[repr(C)]
pub struct DrawFabSwatch {
    #[deref]
    draw_super: DrawQuad,
    #[live]
    pub hover: f32,
    #[live]
    pub open: f32,
    #[live]
    pub swatch: Vec4f,
}

#[derive(Clone, Debug, Default)]
pub enum FabColorPickAction {
    /// Live: the bound value should follow immediately (rgba 0..1).
    Changed(Vec4f),
    /// Commit (release / Enter / outside-click close). Escape publishes
    /// `Changed(original)` then `Ended(original)`.
    Ended(Vec4f),
    Opened,
    Closed,
    /// The popover's `pick` button: sample a colour from the app — the
    /// host owns the eyedropper (it knows the window), the popover closes.
    Eyedropper,
    /// The pointer rests on a palette cell (its name) or left the strip.
    PaletteHover(Option<String>),
    /// A palette cell was clicked: the host binds the property to the
    /// named colour; the popover has closed without publishing a value.
    PalettePick(String),
    #[default]
    None,
}

// Hand-written `WidgetNode` (the `Widget` derive owns that impl): the open
// popover's controls surface as children so the remote bridge and the
// design tweaker's walks can reach the `pick` button and the fields.
#[derive(Script, WidgetRegister, WidgetRef, WidgetSet)]
pub struct FabColorPick {
    #[uid]
    uid: WidgetUid,
    #[source]
    source: ScriptObjectRef,
    #[live]
    draw_swatch: DrawFabSwatch,
    #[walk]
    walk: Walk,
    #[layout]
    layout: Layout,
    #[live]
    with_alpha: bool,
    /// The popover panel (wheel + rows + hex), from the type default.
    #[live]
    popover: View,
    #[rust]
    overlay_list: Option<DrawList2d>,
    #[rust]
    open: bool,
    /// Held while the popover is open, so `Escape` reverts this picker rather
    /// than dismissing whatever it was opened in front of.
    #[rust]
    cancel_scope: Option<CancelScope>,
    #[rust]
    hsv: [f32; 3],
    #[rust(1.0)]
    alpha: f32,
    /// The colour when the popover opened — restored by Escape.
    #[rust]
    opened_value: [f32; 4],
    #[rust]
    panel_rect: Rect,
    #[rust]
    sync_pending: bool,
    /// Names of the palette entries, in strip order.
    #[rust]
    palette_names: Vec<String>,
}

impl ScriptHook for FabColorPick {
    fn on_after_new(&mut self, vm: &mut ScriptVm) {
        self.overlay_list = Some(DrawList2d::script_new(vm));
    }
}

impl FabColorPick {
    pub fn rgba(&self) -> [f32; 4] {
        let [h, s, v] = self.hsv;
        let [r, g, b] = hsv_to_rgb(h, s, v);
        [r, g, b, self.alpha]
    }

    pub fn set_rgba(&mut self, cx: &mut Cx, rgba: [f32; 4]) {
        self.hsv = rgb_to_hsv(rgba[0], rgba[1], rgba[2]);
        self.alpha = rgba[3];
        self.draw_swatch.swatch = vec4(rgba[0], rgba[1], rgba[2], rgba[3]);
        self.draw_swatch.redraw(cx);
        if self.open {
            self.sync_pending = true;
            self.sync_widgets(cx);
        }
    }

    pub fn is_open(&self) -> bool {
        self.open
    }

    /// The popover's palette strip: named colours in display order.
    pub fn set_palette(&mut self, cx: &mut Cx, entries: Vec<(String, [f32; 4])>) {
        let colors = entries.iter().map(|(_, c)| *c).collect();
        self.palette_names = entries.into_iter().map(|(n, _)| n).collect();
        if let Some(mut strip) = self.popover.child(live_id!(palette)).borrow_mut::<FabPaletteStrip>() {
            strip.set_colors(cx, colors);
        }
        self.sync_pending = true;
    }

    fn palette_label(&self, cx: &mut Cx, hot: Option<usize>) {
        let text = match hot.and_then(|i| self.palette_names.get(i)) {
            Some(name) => format!("theme.{name}"),
            None if self.palette_names.is_empty() => String::new(),
            None => format!("theme colours ({})", self.palette_names.len()),
        };
        self.popover.child(live_id!(palette_name)).set_text(cx, &text);
    }

    /// Close without publishing: the host is about to bind the property
    /// to a palette reference, and a `Changed` would ledger a hex first.
    fn close_quiet(&mut self, cx: &mut Cx) {
        if !self.open {
            return;
        }
        let uid = self.widget_uid();
        self.popover.handle_event(cx,
            &Event::Actions(vec![Box::new(crate::modal::ModalAction::Dismissed)]),
            &mut Scope::empty());
        self.open = false;
        self.cancel_scope = None;
        self.draw_swatch.open = 0.0;
        cx.widget_action(uid, FabColorPickAction::Closed);
        if let Some(list) = &self.overlay_list {
            list.redraw(cx);
        }
        self.draw_swatch.redraw(cx);
        cx.redraw_all();
    }

    fn publish(&mut self, cx: &mut Cx, uid: WidgetUid, ended: bool) {
        let rgba = self.rgba();
        self.draw_swatch.swatch = vec4(rgba[0], rgba[1], rgba[2], rgba[3]);
        self.draw_swatch.redraw(cx);
        let value = vec4(rgba[0], rgba[1], rgba[2], rgba[3]);
        cx.widget_action(uid, FabColorPickAction::Changed(value));
        if ended {
            cx.widget_action(uid, FabColorPickAction::Ended(value));
        }
    }

    /// Push the state into every control (wheel, rows, hex).
    fn sync_widgets(&mut self, cx: &mut Cx) {
        let [h, s, v] = self.hsv;
        let rgba = self.rgba();
        if let Some(mut wheel) = self.popover.child(live_id!(wheel)).borrow_mut::<FabColorWheel>()
        {
            wheel.set_hsv(cx, h, s, v);
        }
        let nums = [
            (live_id!(num_r), rgba[0]),
            (live_id!(num_g), rgba[1]),
            (live_id!(num_b), rgba[2]),
            (live_id!(num_a), rgba[3]),
        ];
        for (id, channel) in nums {
            if let Some(mut num) = self.popover.child(id).borrow_mut::<FabValueInput>() {
                num.set_value(cx, (channel * 255.0).round() as f64);
            }
        }
        let hex = self.popover.child(live_id!(hex_row)).child(live_id!(hex));
        if !hex.is_empty() {
            // Don't stomp the hex text while the person is typing in it.
            if hex.area() == Area::Empty || !cx.has_key_focus(hex.area()) {
                hex.set_text(cx, &format_hex(rgba, self.with_alpha));
            }
        }
        if let Some(mut strip) = self.popover.child(live_id!(palette)).borrow_mut::<FabPaletteStrip>() {
            let byte = |v: f32| (v * 255.0).round() as i32;
            let current = strip
                .colors
                .iter()
                .position(|c| (0..4).all(|k| byte(c[k]) == byte(rgba[k])));
            strip.set_current(cx, current);
            let hot = strip.hot;
            drop(strip);
            self.palette_label(cx, hot);
        }
    }

    pub fn open_popover(&mut self, cx: &mut Cx) {
        if self.open {
            return;
        }
        self.open = true;
        self.cancel_scope = Some(self.begin_cancel_scope(cx));
        self.opened_value = self.rgba();
        self.draw_swatch.open = 1.0;
        self.sync_pending = true;
        let uid = self.widget_uid();
        cx.widget_action(uid, FabColorPickAction::Opened);
        if let Some(list) = &self.overlay_list {
            list.redraw(cx);
        }
        self.draw_swatch.redraw(cx);
        // The popover's controls join the widget tree under this swatch
        // while it is open, so the remote bridge (/snap) and the tweaker's
        // tree walks can reach the `pick` button and the fields.
        let uid = self.uid;
        let mut kids = Vec::new();
        self.popover.children(&mut |id, w| kids.push((id, w)));
        for (id, w) in kids {
            cx.widget_tree_insert_child_deep(uid, id, w);
        }
    }

    pub fn close_popover(&mut self, cx: &mut Cx, revert: bool) {
        if !self.open {
            return;
        }
        self.popover.handle_event(cx,
            &Event::Actions(vec![Box::new(crate::modal::ModalAction::Dismissed)]),
            &mut Scope::empty());
        let uid = self.widget_uid();
        if revert {
            let original = self.opened_value;
            self.hsv = rgb_to_hsv(original[0], original[1], original[2]);
            self.alpha = original[3];
            self.publish(cx, uid, true);
        } else {
            self.publish(cx, uid, true);
        }
        self.open = false;
        self.cancel_scope = None;
        self.draw_swatch.open = 0.0;
        cx.widget_action(uid, FabColorPickAction::Closed);
        if let Some(list) = &self.overlay_list {
            list.redraw(cx);
        }
        self.draw_swatch.redraw(cx);
        cx.redraw_all();
    }

    /// The open popover's window-local rect (zero when closed). The panel
    /// host uses it to give the popup input priority over its scroll list.
    pub fn popover_rect(&self) -> Rect {
        if self.open {
            self.panel_rect
        } else {
            Rect::default()
        }
    }
}

impl WidgetNode for FabColorPick {
    fn widget_uid(&self) -> WidgetUid {
        self.uid
    }

    fn set_action_data(&mut self, _action_data: std::sync::Arc<dyn ActionTrait>) {}

    fn action_data(&self) -> Option<std::sync::Arc<dyn ActionTrait>> {
        None
    }

    fn area(&self) -> Area {
        self.draw_swatch.area()
    }

    fn walk(&mut self, _cx: &mut Cx) -> Walk {
        self.walk
    }

    fn redraw(&mut self, cx: &mut Cx) {
        self.draw_swatch.redraw(cx);
    }

    fn children(&self, visit: &mut dyn FnMut(LiveId, WidgetRef)) {
        if self.open {
            self.popover.children(visit);
        }
    }
}

impl Widget for FabColorPick {
    fn draw_walk(&mut self, cx: &mut Cx2d, scope: &mut Scope, walk: Walk) -> DrawStep {
        let rgba = self.rgba();
        self.draw_swatch.swatch = vec4(rgba[0], rgba[1], rgba[2], rgba[3]);
        self.draw_swatch.draw_walk(cx, walk);
        if self.open {
            let anchor = self.draw_swatch.area().rect(cx);
            let overlay_list = self.overlay_list.as_mut().unwrap();
            overlay_list.begin_overlay_reuse(cx);
            let pass_size = cx.current_pass_size();
            cx.begin_root_turtle(pass_size, Layout::flow_down());
            // Anchor under the swatch; clamp into the pass, flip above when
            // the bottom would overflow.
            let width = 244.0_f64;
            let strip_height = self
                .popover
                .child(live_id!(palette))
                .borrow::<FabPaletteStrip>()
                .map_or(0.0, |s| s.height_for(width - 16.0));
            let est_height = 360.0_f64 + if strip_height > 0.0 { strip_height + 26.0 } else { 0.0 };
            let mut pos = dvec2(anchor.pos.x + anchor.size.x - width, anchor.pos.y + anchor.size.y + 2.0);
            if pos.y + est_height > pass_size.y {
                pos.y = (anchor.pos.y - est_height - 2.0).max(0.0);
            }
            pos.x = pos.x.clamp(0.0, (pass_size.x - width).max(0.0));
            let mut panel_walk = Walk::fit();
            panel_walk.abs_pos = Some(pos);
            panel_walk.width = Size::Fixed(width);
            // Push state into the controls BEFORE they draw: a redraw
            // requested during the draw event is dropped, so a sync after
            // the draw only showed on the next unrelated redraw.
            if self.sync_pending {
                self.sync_pending = false;
                self.sync_widgets(cx);
            }
            let _ = self.popover.draw_walk(cx, scope, panel_walk);
            // The UNCLIPPED rect: the popover draws in an overlay above
            // every clip, but `clipped_rect` intersects the host row's
            // clip stack and came back zero inside a scroll list — which
            // made the first outside-press logic close the popover on ANY
            // press ("the popup cannot be manipulated").
            self.panel_rect = self.popover.area().rect(cx);
            cx.end_pass_sized_turtle();
            self.overlay_list.as_mut().unwrap().end(cx);
        }
        DrawStep::done()
    }

    fn handle_event(&mut self, cx: &mut Cx, event: &Event, scope: &mut Scope) {
        let uid = self.widget_uid();
        if self.open && crate::modal::ModalAction::is_dismissal(event) {
            self.close_popover(cx, true);
            return;
        }

        if self.open {
            // Only the owner can consume Back or act on Escape.
            if self.cancel_scope.as_ref().is_some_and(|s| cx.owns_cancel(s))
                && (matches!(event, Event::KeyDown(ke) if ke.key_code == KeyCode::Escape)
                    || event.back_pressed())
            {
                self.close_popover(cx, true);
                return;
            }
            // A press outside the panel and the swatch commits and closes.
            if let Event::MouseDown(me) = event {
                let swatch_rect = self.draw_swatch.area().rect(cx);
                if !self.panel_rect.contains(me.abs) && !swatch_rect.contains(me.abs) {
                    self.close_popover(cx, false);
                    // Do not return: the press still belongs to whatever is
                    // underneath.
                }
            }
            let mut changed = false;
            let mut ended = false;
            for action in cx.capture_actions(|cx| self.popover.handle_event(cx, event, scope)) {
                let Some(widget_action) = action.as_widget_action() else {
                    continue;
                };
                let wheel_uid = self.popover.child(live_id!(wheel)).widget_uid();
                let r_uid = self.popover.child(live_id!(num_r)).widget_uid();
                let g_uid = self.popover.child(live_id!(num_g)).widget_uid();
                let b_uid = self.popover.child(live_id!(num_b)).widget_uid();
                let a_uid = self.popover.child(live_id!(num_a)).widget_uid();
                let hex_uid = self
                    .popover
                    .child(live_id!(hex_row))
                    .child(live_id!(hex))
                    .widget_uid();
                let pick_uid = self
                    .popover
                    .child(live_id!(hex_row))
                    .child(live_id!(pick))
                    .widget_uid();
                if widget_action.widget_uid == pick_uid {
                    if let ButtonAction::Clicked(_) = widget_action.cast::<ButtonAction>() {
                        let uid = self.widget_uid();
                        self.close_popover(cx, false);
                        cx.widget_action(uid, FabColorPickAction::Eyedropper);
                    }
                    continue;
                }
                let strip_uid = self.popover.child(live_id!(palette)).widget_uid();
                if widget_action.widget_uid == strip_uid {
                    match widget_action.cast::<FabPaletteAction>() {
                        FabPaletteAction::Hover(hot) => {
                            self.palette_label(cx, hot);
                            let name = hot.and_then(|i| self.palette_names.get(i).cloned());
                            cx.widget_action(uid, FabColorPickAction::PaletteHover(name));
                        }
                        FabPaletteAction::Pick(i) => {
                            if let Some(name) = self.palette_names.get(i).cloned() {
                                let color = self
                                    .popover
                                    .child(live_id!(palette))
                                    .borrow::<FabPaletteStrip>()
                                    .and_then(|s| s.colors.get(i).copied());
                                if let Some(c) = color {
                                    self.hsv = rgb_to_hsv(c[0], c[1], c[2]);
                                    self.alpha = c[3];
                                    self.draw_swatch.swatch = vec4(c[0], c[1], c[2], c[3]);
                                }
                                self.close_quiet(cx);
                                cx.widget_action(uid, FabColorPickAction::PaletteHover(None));
                                cx.widget_action(uid, FabColorPickAction::PalettePick(name));
                            }
                        }
                        _ => {}
                    }
                    continue;
                }
                if widget_action.widget_uid == wheel_uid {
                    match widget_action.cast::<ColorWheelAction>() {
                        ColorWheelAction::Changed(hsv) => {
                            self.hsv = hsv;
                            changed = true;
                        }
                        ColorWheelAction::Ended(hsv) => {
                            self.hsv = hsv;
                            changed = true;
                            ended = true;
                        }
                        _ => {}
                    }
                } else if widget_action.widget_uid == r_uid
                    || widget_action.widget_uid == g_uid
                    || widget_action.widget_uid == b_uid
                    || widget_action.widget_uid == a_uid
                {
                    let (value, is_ended) = match widget_action.cast::<FabValueInputAction>() {
                        FabValueInputAction::Changed(v) => (Some(v), false),
                        FabValueInputAction::Ended(v) => (Some(v), true),
                        _ => (None, false),
                    };
                    if let Some(v) = value {
                        let channel = (v / 255.0).clamp(0.0, 1.0) as f32;
                        let mut rgba = self.rgba();
                        if widget_action.widget_uid == r_uid {
                            rgba[0] = channel;
                        } else if widget_action.widget_uid == g_uid {
                            rgba[1] = channel;
                        } else if widget_action.widget_uid == b_uid {
                            rgba[2] = channel;
                        } else {
                            rgba[3] = channel;
                        }
                        self.hsv = rgb_to_hsv(rgba[0], rgba[1], rgba[2]);
                        self.alpha = rgba[3];
                        changed = true;
                        ended |= is_ended;
                    }
                } else if widget_action.widget_uid == hex_uid {
                    if let TextInputAction::Returned(text, _) =
                        widget_action.cast::<TextInputAction>()
                    {
                        if let Some((rgba, had_alpha)) = parse_hex(&text) {
                            self.hsv = rgb_to_hsv(rgba[0], rgba[1], rgba[2]);
                            if had_alpha {
                                self.alpha = rgba[3];
                            }
                            changed = true;
                            ended = true;
                        }
                        self.sync_pending = true;
                    }
                }
            }
            if changed {
                self.publish(cx, uid, ended);
                self.sync_widgets(cx);
            }
        }

        match event.hits(cx, self.draw_swatch.area()) {
            Hit::FingerHoverIn(_) => {
                cx.set_cursor(MouseCursor::Hand);
                self.draw_swatch.hover = 1.0;
                self.draw_swatch.redraw(cx);
            }
            Hit::FingerHoverOut(_) => {
                self.draw_swatch.hover = 0.0;
                self.draw_swatch.redraw(cx);
            }
            Hit::FingerDown(fe) if fe.device.is_primary_hit() => {
                if self.open {
                    self.close_popover(cx, false);
                } else {
                    self.open_popover(cx);
                }
            }
            _ => {}
        }
    }
}

impl FabColorPickRef {
    pub fn changed(&self, actions: &Actions) -> Option<Vec4f> {
        if let Some(item) = actions.find_widget_action(self.widget_uid()) {
            if let FabColorPickAction::Changed(v) = item.cast() {
                return Some(v);
            }
        }
        None
    }

    pub fn set_rgba(&self, cx: &mut Cx, rgba: [f32; 4]) {
        if let Some(mut inner) = self.borrow_mut() {
            inner.set_rgba(cx, rgba);
        }
    }

    pub fn is_open(&self) -> bool {
        self.borrow().map_or(false, |i| i.is_open())
    }
}

// ===========================================================================
// Tests — the pure core, ported with the control
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;

    fn bounded(min: f64, max: f64, wrap: bool) -> DragParams {
        DragParams {
            min,
            max,
            step: 0.25,
            wrap,
            bounded: true,
            snap_override: 0.0,
        }
    }

    fn unbounded(step: f64) -> DragParams {
        DragParams {
            min: 0.0,
            max: 0.0,
            step,
            wrap: false,
            bounded: false,
            snap_override: 0.0,
        }
    }

    /// The range takes FOUR widths of travel, not one — see
    /// [`DRAG_RANGE_TRAVEL`]. A field's own width is a thumb's movement, and
    /// spending the whole range across it leaves nothing landable in
    /// between.
    #[test]
    fn a_bounded_field_takes_four_widths_to_sweep_its_range() {
        let p = bounded(0.0, 24.0, false);
        let a = DragAnchor { x: 0.0, value: 12.0 };
        // 200 wide, so 800 of travel is the full 24; 100 is an eighth of it.
        let (v, _) = drag_map(&p, a, 100.0, 200.0, false, false);
        assert!((v - 15.0).abs() < 1e-9, "{v}");
        let (v, _) = drag_map(&p, a, 800.0, 200.0, false, false);
        assert!((v - 24.0).abs() < 1e-9, "{v}");
        let (v, _) = drag_map(&p, a, 50.0, 200.0, false, false);
        assert!((v - 13.5).abs() < 1e-9, "{v}");
        let (v, _) = drag_map(&p, a, 50.0, 200.0, true, false);
        assert!((v - 12.075).abs() < 1e-9, "{v}");
    }

    #[test]
    fn an_unbounded_field_moves_by_pixels_times_step() {
        let p = unbounded(1.0);
        let a = DragAnchor { x: 0.0, value: 2000.0 };
        let (v, _) = drag_map(&p, a, 100.0, 400.0, false, false);
        assert!((v - 2100.0).abs() < 1e-9, "{v}");
        let (v, _) = drag_map(&p, a, 100.0, 400.0, true, false);
        assert!((v - 2010.0).abs() < 1e-9, "{v}");
    }

    #[test]
    fn clamping_shifts_the_anchor_so_reversal_moves_immediately() {
        let p = bounded(0.0, 1.0, false);
        let a = DragAnchor { x: 0.0, value: 0.5 };
        let (v, a2) = drag_map(&p, a, 300.0, 100.0, false, false);
        assert!((v - 1.0).abs() < 1e-9);
        assert_eq!(a2.x, 300.0);
        assert_eq!(a2.value, 1.0);
        // One pixel back off the limit moves by one pixel's worth: the whole
        // range is 400 of travel here, so that is 1/400.
        let (v, _) = drag_map(&p, a2, 299.0, 100.0, false, false);
        assert!((v - 0.9975).abs() < 1e-9, "{v}");
    }

    #[test]
    fn cyclic_fields_wrap_at_their_ends() {
        let p = bounded(0.0, 24.0, true);
        let a = DragAnchor { x: 0.0, value: 23.0 };
        // 1200 wide is 4800 of travel for 24, so 400 pixels is 2.
        let (v, _) = drag_map(&p, a, 400.0, 1200.0, false, false);
        assert!((v - 1.0).abs() < 1e-9, "{v}");
    }

    #[test]
    fn hex_parses_and_formats_round_trip() {
        let (rgba, had_alpha) = parse_hex("#ff8000").unwrap();
        assert!(!had_alpha);
        assert!((rgba[0] - 1.0).abs() < 1e-6);
        assert!((rgba[1] - 128.0 / 255.0).abs() < 1e-6);
        assert_eq!(format_hex(rgba, false), "#ff8000");
        let (rgba, had_alpha) = parse_hex("40E0D080").unwrap();
        assert!(had_alpha);
        assert_eq!(format_hex(rgba, true), "#40e0d080");
        assert!(parse_hex("#12345").is_none());
        assert!(parse_hex("nope").is_none());
    }

    #[test]
    fn hsv_rgb_round_trips() {
        for rgb in [[1.0f32, 0.0, 0.0], [0.2, 0.7, 0.4], [0.5, 0.5, 0.5]] {
            let [h, s, v] = rgb_to_hsv(rgb[0], rgb[1], rgb[2]);
            let back = hsv_to_rgb(h, s, v);
            for i in 0..3 {
                assert!((back[i] - rgb[i]).abs() < 1e-5, "{rgb:?} -> {back:?}");
            }
        }
    }

    #[test]
    fn the_zones_split_arrows_from_the_drag_surface() {
        assert_eq!(field_zone(5.0, 200.0, 20.0), FieldZone::Decrement);
        assert_eq!(field_zone(100.0, 200.0, 20.0), FieldZone::Middle);
        assert_eq!(field_zone(195.0, 200.0, 20.0), FieldZone::Increment);
    }

    /// The box every stock widget nested in a fab template resolves to,
    /// under whatever sheet is installed. One string per control, compared
    /// against the same reading with no sheet at all.
    fn fab_geometry(cx: &mut Cx) -> Vec<(&'static str, String)> {
        fn built(cx: &mut Cx, name: &str) -> WidgetRef {
            let widget = cx.with_vm(|vm| {
                let widgets = vm.module(id!(widgets));
                let value = vm
                    .bx
                    .heap
                    .value(widgets, LiveId::from_str(name).into(), NoTrap);
                WidgetRef::script_from_value(vm, value)
            });
            assert!(!widget.is_empty(), "{name} built no widget");
            widget
        }
        // Everything a walk can impose a size with. `margin` is in here too:
        // the stock field takes it from a theme token every sheet moves.
        fn shape(w: Walk) -> String {
            format!(
                "w={:?} h={:?} min={:?}/{:?} max={:?}/{:?} aspect={:?} margin={:?}",
                w.width,
                w.height,
                w.min_width,
                w.min_height,
                w.max_width,
                w.max_height,
                w.aspect,
                w.margin,
            )
        }
        let mut out = Vec::new();

        // The panel's filter field: the well, and the field inside it.
        let search = built(cx, "FabSearch");
        let input = search.widget(&*cx, &[live_id!(input)]);
        assert!(!input.is_empty(), "FabSearch no longer has an `input`");
        out.push(("FabSearch", shape(search.walk(cx))));
        out.push(("FabSearch/input", shape(input.walk(cx))));

        // The drag-numeric field's editor -- every property row on the panel.
        let value_input = built(cx, "FabValueInput");
        out.push(("FabValueInput", shape(value_input.walk(cx))));
        {
            let mut field = value_input
                .borrow_mut::<FabValueInput>()
                .expect("FabValueInput is a FabValueInput");
            out.push(("FabValueInput/text_input", shape(field.text_input.walk(cx))));
        }

        // The colour popover's hex row: a stock Button beside a stock field.
        let color_pick = built(cx, "FabColorPick");
        let (hex, pick) = {
            let popover = color_pick
                .borrow::<FabColorPick>()
                .expect("FabColorPick is a FabColorPick");
            (
                popover.popover.widget(&*cx, &[live_id!(hex)]),
                popover.popover.widget(&*cx, &[live_id!(pick)]),
            )
        };
        assert!(!hex.is_empty(), "the colour popover no longer has a `hex`");
        assert!(!pick.is_empty(), "the colour popover no longer has a `pick`");
        out.push(("FabColorPick/hex", shape(hex.walk(cx))));
        out.push(("FabColorPick/pick", shape(pick.walk(cx))));

        // The knob nests nothing a sheet can reach, and that is the claim:
        // its box is its own two numbers, under every sheet.
        let knob = built(cx, "FabKnob");
        out.push(("FabKnob", shape(knob.walk(cx))));

        // The matrix's column header, which is a box like any other even
        // though its ink is not.
        let header = built(cx, "FabDiagonalLabel");
        out.push(("FabDiagonalLabel", shape(header.walk(cx))));
        out
    }

    /// Every control the dev panel is built from resolves to the SAME box
    /// under every sheet the library ships as it does under none.
    ///
    /// The panel paints its own palette on purpose and is meant to be immune
    /// to whatever the app is wearing -- but immunity is not automatic. A fab
    /// template NESTS stock widgets, and a sheet reaches those through
    /// `mod.widgets.TextInput` and `mod.widgets.Button`, taking everything
    /// the template did not write out for itself.
    ///
    /// That is how the filter field came to sink. `android` and `ios` set
    /// `mod.widgets.TextInput.min_height` to 48 and 44; a walk applies a min
    /// height UNCONDITIONALLY (`draw/src/turtle.rs`, where `walk.min_height`
    /// is resolved), so FabSearch's `height: Fill` field stood 48 tall inside
    /// a 24 tall well -- and a single-line input CENTRES its line box in its
    /// own content box (`TextInput::scroll_to_cursor`), which put the word
    /// "Filter" a dozen pixels below the well's floor, straddling its border.
    ///
    /// Read off `DesktopStyle::ALL` rather than written out, so a sheet added
    /// later -- or an existing one that starts overriding `max_height`, the
    /// margin or the padding -- fails HERE and not on somebody's screen.
    #[test]
    fn the_fab_controls_resolve_the_same_box_under_every_sheet() {
        use crate::desktop_style::{install, uninstall, DesktopStyle, StyleSheet};
        let mut cx = Cx::new(Box::new(|_, _| {}));
        cx.init_cx_os();
        cx.with_vm(crate::script_mod);
        let plain = fab_geometry(&mut cx);
        assert_eq!(plain.len(), 8, "a control was dropped from the reading");
        for style in DesktopStyle::ALL {
            for dark in [false, true] {
                if dark && !style.supports_dark() {
                    continue;
                }
                cx.with_vm(|vm| {
                    install(vm, StyleSheet::load_with_appearance(style, dark));
                    vm.with_reload(crate::script_mod);
                });
                let under = fab_geometry(&mut cx);
                for ((name, want), (_, got)) in plain.iter().zip(under.iter()) {
                    assert_eq!(
                        want,
                        got,
                        "`{name}` resolves to a different box under `{}`{}",
                        style.id(),
                        if dark { " dark" } else { "" }
                    );
                }
            }
        }
        // ...and taking the sheet off puts the panel back where it started.
        cx.with_vm(|vm| {
            uninstall(vm);
            vm.with_reload(crate::script_mod);
        });
        assert_eq!(fab_geometry(&mut cx), plain);
    }

    /// The same question about the PAINT rather than the box, for the field
    /// the panel's filter is.
    ///
    /// `windows-2000` and `nextstep` REPLACE
    /// `mod.widgets.TextInput.draw_bg.pixel` outright with a hard-coded
    /// opaque white Win95 field. That ignores every colour the filter field
    /// declares, paints over the well FabSearch draws around it and leaves
    /// the panel's own light grey placeholder on white. Answering an
    /// override means declaring the same leaf in this template, so the
    /// sheet's value lands on something the panel does not use.
    ///
    /// Read off the sheets in the tree, so a sheet that starts overriding
    /// something else is caught here rather than by somebody finding the
    /// filter unreadable.
    ///
    /// Every property a sheet sets on `target`, whether it says so in a line
    /// of its own or through one of the sheet's own helpers. A sheet states
    /// its field metrics ONCE and hands them to each field in turn -- `let
    /// field_room = fn(w) { w.min_height = .. }`, then
    /// `field_room(mod.widgets.TextInput)` -- so a scan that reads only
    /// `mod.widgets.TextInput.x = y` lines goes blind the moment a sheet
    /// stops repeating itself, and says the sheets did not load. The helper
    /// bodies are read here too, so both spellings count.
    fn sheet_overrides(text: &str, target: &str) -> Vec<String> {
        let mut helpers: Vec<(String, String, Vec<String>)> = Vec::new();
        let mut open: Option<(String, String, Vec<String>)> = None;
        for line in text.lines() {
            let line = line.trim();
            if let Some((name, arg)) = line
                .strip_prefix("let ")
                .and_then(|rest| rest.split_once(" = fn("))
                .and_then(|(name, rest)| rest.split_once(')').map(|(arg, _)| (name, arg)))
            {
                open = Some((name.trim().to_string(), arg.trim().to_string(), Vec::new()));
                continue;
            }
            let Some((_, arg, props)) = open.as_mut() else { continue };
            if line == "}" {
                helpers.push(open.take().expect("the block is open"));
                continue;
            }
            if let Some(prop) = line
                .strip_prefix(&format!("{arg}."))
                .and_then(|rest| rest.split_once('='))
                .map(|(prop, _)| prop.trim().to_string())
            {
                props.push(prop);
            }
        }
        let mut found = Vec::new();
        for line in text.lines() {
            let line = line.trim();
            if let Some(rest) = line.strip_prefix(&format!("{target}.")) {
                if let Some(prop) = rest.split([' ', '=']).next() {
                    found.push(prop.to_string());
                }
                continue;
            }
            for (name, _, props) in &helpers {
                // `field_face(mod.widgets.TextInput.draw_bg)` reaches the
                // same place as `mod.widgets.TextInput.draw_bg.border_radius
                // = ..`, so whatever the call named is put back in front.
                let Some(arg) = line
                    .strip_prefix(&format!("{name}("))
                    .and_then(|rest| rest.strip_suffix(')'))
                else {
                    continue;
                };
                let under = match arg.strip_prefix(target) {
                    Some("") => String::new(),
                    Some(rest) => format!("{}.", rest.trim_start_matches('.')),
                    None => continue,
                };
                found.extend(props.iter().map(|prop| format!("{under}{prop}")));
            }
        }
        found
    }

    #[test]
    fn the_filter_field_answers_what_the_sheets_override_on_a_text_input() {
        // Everything before `#[cfg(test)]`: the controls, without the tests
        // that talk about them -- a test looking for a spelling in the whole
        // file finds its own words and passes on them.
        let src = include_str!("fab_controls.rs")
            .split("#[cfg(test)]")
            .next()
            .expect("the file has a first half");
        let search = src
            .split("mod.widgets.FabSearch = View{")
            .nth(1)
            .expect("the file declares `mod.widgets.FabSearch`");
        // The FIELD's own text, not the well's: the View around it declares a
        // `pixel` of its own, and that must not answer for the field.
        let field = search
            .split("input := TextInput{")
            .nth(1)
            .expect("FabSearch declares `input := TextInput`");
        let kit = &field[..field
            .find("mod.widgets.FabPropRow")
            .expect("FabSearch is followed by FabPropRow")];
        let themes = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("themes");
        let mut seen = 0usize;
        for entry in std::fs::read_dir(&themes).expect("the themes folder is in the tree") {
            let sheet = entry.expect("a readable entry").path().join("widgets.splash");
            let Ok(text) = std::fs::read_to_string(&sheet) else {
                continue;
            };
            for prop in sheet_overrides(&text, "mod.widgets.TextInput") {
                // `draw_bg.pixel` is answered by declaring `pixel:` inside
                // this field's own `draw_bg`, and so on down.
                let leaf = prop.rsplit('.').next().unwrap_or(&prop).to_string();
                assert!(
                    kit.contains(&format!("{leaf}:")),
                    "{} overrides `{prop}` on a TextInput and the filter field does not declare `{leaf}`",
                    sheet.display()
                );
                seen += 1;
            }
        }
        assert!(
            seen > 8,
            "only {seen} overrides were read -- the sheets did not load"
        );
        // The two a sheet reaches through a THEME token rather than through
        // `widgets.splash`: the stock field takes its padding from
        // `theme.mspace_1` and its margin from `theme.mspace_v_1`, and every
        // sheet moves the space factor those are built from.
        assert!(kit.contains("padding: Inset{"), "the padding is not written out");
        assert!(kit.contains("margin: Inset{"), "the margin is not written out");
        assert!(kit.contains("min_height: 0"), "the min height is not written out");
    }

    /// One entry of a runtime table, as the VM has it right now.
    fn table_color(cx: &mut Cx, table: LiveId, key: &str) -> u32 {
        cx.with_vm(|vm| {
            let module = vm.module(table);
            vm.bx
                .heap
                .value(module, LiveId::from_str(key).into(), NoTrap)
                .as_color()
                .unwrap_or_else(|| panic!("`{key}` is not a colour in this table"))
        })
    }

    /// Every colour the fab palette declares, with the literal it is written
    /// as. Read off the source, so an entry added to the table joins the
    /// tests below without anybody remembering to come back for it.
    fn declared_fab_colors() -> Vec<(String, u32)> {
        let src = include_str!("fab_controls.rs")
            .split("#[cfg(test)]")
            .next()
            .expect("the file has a first half");
        let table = src
            .split("mod.fab = {")
            .nth(1)
            .expect("the file declares `mod.fab`");
        // The colours only: the density, type and motion entries after them
        // are numbers, and are deliberately NOT part of any palette swap.
        let table = &table[..table
            .find("// ---- density ----")
            .expect("the table still has a density block after the colours")];
        let mut out = Vec::new();
        for line in table.lines() {
            let line = line.trim();
            let Some((name, value)) = line.split_once(':') else {
                continue;
            };
            let Some(hex) = value.trim().strip_prefix("#x") else {
                continue;
            };
            if !name.chars().all(|c| c.is_ascii_lowercase() || c == '_') {
                continue;
            }
            let rgba = u32::from_str_radix(hex, 16).expect("a hex colour");
            // The table writes six digits and means opaque.
            out.push((name.to_string(), if hex.len() == 6 { rgba << 8 | 0xFF } else { rgba }));
        }
        assert!(out.len() > 30, "only {} colours were read off the table", out.len());
        out
    }

    /// The panel's own palette, under every sheet the library ships.
    ///
    /// This is the immunity the panel exists for, stated as a value rather
    /// than as an intention: every colour the panel's chrome is drawn in is
    /// the literal written in the table above, whatever the app is wearing.
    /// Not a default with something on the other side of it -- there is no
    /// other side, and this is the reading that keeps it that way.
    #[test]
    fn the_panels_palette_is_untouched_under_every_sheet() {
        use crate::desktop_style::{install, uninstall, DesktopStyle, StyleSheet};
        let declared = declared_fab_colors();
        let mut cx = Cx::new(Box::new(|_, _| {}));
        cx.init_cx_os();
        cx.with_vm(crate::script_mod);
        for style in [None].into_iter().chain(DesktopStyle::ALL.map(Some)) {
            cx.with_vm(|vm| {
                match style {
                    Some(style) => install(vm, StyleSheet::load(style)),
                    None => uninstall(vm),
                }
                vm.with_reload(crate::script_mod);
            });
            for (name, want) in &declared {
                assert_eq!(
                    table_color(&mut cx, id!(fab), name),
                    *want,
                    "`fab.{name}` moved under `{}`",
                    style.map(|s| s.id()).unwrap_or("no sheet")
                );
            }
        }
    }

    /// The panel's palette can be READ, under every sheet the library ships.
    ///
    /// Immunity says the colours do not move. It does not say they were ever
    /// legible, and the two are worth holding apart: the pairs below are the
    /// panel's load-bearing ones -- the ground it draws its words on, the
    /// well, the button face, the popover, and the ink on the accent.
    ///
    /// Under every sheet rather than under none, because `mod.tweak_panel`'s
    /// three ink grades belong to the tweaker's table and so are not in the
    /// reading the test above takes.
    #[test]
    fn the_panels_palette_reads_against_itself_under_every_sheet() {
        use crate::desktop_style::{install, uninstall, DesktopStyle, StyleSheet};
        use crate::theme_tokens::{reads_on, LEGIBLE, READABLE};
        // (ground, ink, how far apart they have to stand)
        const PAIRS: &[(&str, &str, f64)] = &[
            ("color_area", "color_text", READABLE),
            ("color_area", "color_text_dim", LEGIBLE),
            ("color_panel", "color_text_header", READABLE),
            ("color_header", "color_text", READABLE),
            ("color_button", "color_text", READABLE),
            ("color_button_hover", "color_text_active", READABLE),
            // LEGIBLE, not READABLE: the panel's accent (#x5680c2 under
            // white) stands 3.99 apart and always has. This records where it
            // is rather than claiming it was ever a 4.5.
            ("color_button_active", "color_text_on_accent", LEGIBLE),
            ("color_input", "color_text", READABLE),
            ("color_popover", "color_text", READABLE),
            ("color_row_hover", "color_text", READABLE),
            // The two faces a panel SWITCH takes (`set_button_fill`): the
            // accent's container while it is on, the well tone while it is
            // off. Both carry the button's own word, which is `color_text`.
            ("color_accent_dim", "color_text", READABLE),
            ("color_input_hover", "color_text", READABLE),
        ];
        let mut cx = Cx::new(Box::new(|_, _| {}));
        cx.init_cx_os();
        cx.with_vm(crate::script_mod);
        for style in [None].into_iter().chain(DesktopStyle::ALL.map(Some)) {
            cx.with_vm(|vm| {
                match style {
                    Some(style) => install(vm, StyleSheet::load(style)),
                    None => uninstall(vm),
                }
                vm.with_reload(crate::script_mod);
            });
            let where_ = style.map(|s| s.id()).unwrap_or("no sheet");
            for (ground, ink, need) in PAIRS {
                let g = table_color(&mut cx, id!(fab), ground);
                let i = table_color(&mut cx, id!(fab), ink);
                let apart = reads_on(g, i);
                assert!(
                    apart >= *need,
                    "{where_}: `{ink}` on `{ground}` stands {apart:.2} apart, under the {need} it needs"
                );
            }
            // The panel's own ink grades sit on the same ground.
            for (ground, ink) in [("color_area", "text_dim"), ("color_input", "text_muted")] {
                let g = table_color(&mut cx, id!(fab), ground);
                let i = table_color(&mut cx, id!(tweak_panel), ink);
                let apart = reads_on(g, i);
                assert!(apart >= LEGIBLE, "{where_}: panel `{ink}` on `{ground}` is {apart:.2}");
            }
        }
    }

    /// Every text style the kit declares, in template order, read back as
    /// what it RESOLVES to rather than as what the source says it is.
    fn kit_text_styles(cx: &mut Cx) -> Vec<TextStyle> {
        cx.with_vm(|vm| {
            let values = vec![
                crate::script_eval!(vm, {mod.widgets.FabValueInput.draw_text.text_style}),
                crate::script_eval!(vm, {mod.widgets.FabValueInput.text_input.draw_text.text_style}),
                crate::script_eval!(vm, {mod.widgets.FabSlider.draw_label.text_style}),
                crate::script_eval!(vm, {mod.widgets.FabSlider.draw_value.text_style}),
                crate::script_eval!(vm, {mod.widgets.FabKnob.draw_label.text_style}),
                crate::script_eval!(vm, {mod.widgets.FabKnob.draw_value.text_style}),
                crate::script_eval!(vm, {mod.widgets.FabKnob.draw_value_off.text_style}),
                crate::script_eval!(vm, {mod.widgets.FabLabel.draw_text.text_style}),
                crate::script_eval!(vm, {mod.widgets.FabLabelSmall.draw_text.text_style}),
                crate::script_eval!(vm, {mod.widgets.FabHeaderLabel.draw_text.text_style}),
                crate::script_eval!(vm, {mod.widgets.FabDiagonalLabel.draw_text.text_style}),
                crate::script_eval!(vm, {mod.widgets.FabSearch.input.draw_text.text_style}),
                crate::script_eval!(vm, {mod.widgets.FabColorPick.popover.hex_row.hex.draw_text.text_style}),
            ];
            values
                .into_iter()
                .map(|value| TextStyle::script_from_value(vm, value))
                .collect()
        })
    }

    /// The kit's words keep ONE face, under every sheet the library ships.
    ///
    /// `mod.fab` is there so the panel does not restyle itself while the
    /// theme underneath it is being changed, and the text was the half of
    /// that which leaked: each `text_style` below named `theme.font_regular`
    /// and took only its SIZE from the table. Installing a blend takes the
    /// sheet off on every apply, so on the applied frame `android`'s Roboto
    /// came through every label and field on the panel at once and went back
    /// on leave -- the row heights are fab and held, the text metrics moved,
    /// and the labels reflowed under the hand that was dragging a weight.
    ///
    /// Two things make this hard to fake. The face is the one the family
    /// RESOLVES to, so a site re-pointed at the theme by hand fails here
    /// however it is spelled; and each entry is checked against the size its
    /// own template declares, which is what proves the path found a real fab
    /// text style rather than an empty object wearing the 10pt default.
    /// The filter box, focused with a word in it, under either base theme:
    /// the ink it types with has to be the panel's, never the theme's. Only
    /// the resting ink was; hover, focus and down fell through to the stock
    /// field's `theme.color_text_focus` and friends, which under a light
    /// theme are dark, on a well that is not.
    #[test]
    fn the_filter_box_keeps_its_own_ink_in_every_state() {
        for base in [crate::BaseTheme::Light, crate::BaseTheme::Dark] {
            let mut cx = Cx::new(Box::new(|_, _| {}));
            crate::set_base_theme(&mut cx, base);
            let (inks, own) = cx.with_vm(|vm| {
                crate::script_mod(vm);
                let widgets = vm.module(id!(widgets));
                let search = vm.bx.heap.value(widgets, LiveId::from_str("FabSearch").into(), NoTrap);
                let input = vm.bx.heap.value(search.as_object().expect("FabSearch"), LiveId::from_str("input").into(), NoTrap);
                let draw_text = vm.bx.heap.value(input.as_object().expect("input"), LiveId::from_str("draw_text").into(), NoTrap);
                let dt = draw_text.as_object().expect("draw_text");
                let keys = ["color", "color_hover", "color_focus", "color_down", "color_disabled", "color_empty", "color_empty_hover", "color_empty_focus"];
                let inks: Vec<(&str, Option<u32>)> = keys
                    .into_iter()
                    .map(|k| (k, vm.bx.heap.value(dt, LiveId::from_str(k).into(), NoTrap).as_color()))
                    .collect();
                let fab = vm.module(id!(fab));
                let own: Vec<u32> = ["color_text", "color_text_active", "color_text_dim", "color_text_muted"]
                    .into_iter()
                    .filter_map(|k| vm.bx.heap.value(fab, LiveId::from_str(k).into(), NoTrap).as_color())
                    .collect();
                (inks, own)
            });
            assert_eq!(own.len(), 4, "the panel's palette is missing an ink this test relies on");
            for (key, ink) in inks {
                let ink = ink.unwrap_or_else(|| panic!("{key} is not a colour on the filter box"));
                assert!(
                    own.contains(&ink),
                    "under {base:?}, {key} is #{ink:08X}, not one of the panel's own inks: it fell through to the app theme"
                );
            }
        }
    }

    #[test]
    fn the_kits_words_keep_one_face_under_every_sheet() {
        use crate::desktop_style::{install, uninstall, DesktopStyle, StyleSheet};
        // (where it is written, the size that template asks for)
        const SITES: &[(&str, f32)] = &[
            ("FabValueInput.draw_text", 8.5),
            ("FabValueInput.text_input.draw_text", 8.5),
            ("FabSlider.draw_label", 8.5),
            ("FabSlider.draw_value", 8.5),
            ("FabKnob.draw_label", 7.5),
            ("FabKnob.draw_value", 7.5),
            ("FabKnob.draw_value_off", 7.5),
            ("FabLabel.draw_text", 8.5),
            ("FabLabelSmall.draw_text", 7.5),
            ("FabHeaderLabel.draw_text", 9.0),
            ("FabDiagonalLabel.draw_text", 7.5),
            ("FabSearch.input.draw_text", 8.5),
            ("FabColorPick.popover.hex_row.hex.draw_text", 8.5),
        ];
        let mut cx = Cx::new(Box::new(|_, _| {}));
        cx.init_cx_os();
        cx.with_vm(crate::script_mod);
        for style in [None].into_iter().chain(DesktopStyle::ALL.map(Some)) {
            cx.with_vm(|vm| {
                match style {
                    Some(style) => install(vm, StyleSheet::load(style)),
                    None => uninstall(vm),
                }
                vm.with_reload(crate::script_mod);
            });
            let where_ = style.map(|s| s.id()).unwrap_or("no sheet");
            let read = kit_text_styles(&mut cx);
            assert_eq!(read.len(), SITES.len());
            for (style, (site, size)) in read.into_iter().zip(SITES) {
                assert_eq!(
                    style.font_size, *size,
                    "{where_}: `{site}` did not resolve to a fab text style"
                );
                let family = format!("{:?}", style.font_family);
                // The FIRST member is the one the latin metrics come from.
                let first = family
                    .split("resource_path: \"")
                    .nth(1)
                    .and_then(|rest| rest.split('"').next())
                    .unwrap_or("nothing at all");
                assert!(
                    first.ends_with("IBMPlexSans-Text.ttf"),
                    "{where_}: `{site}` leads with `{first}`"
                );
                assert!(
                    !family.contains("RobotoFlex.ttf") && !family.contains("Inter.ttf"),
                    "{where_}: `{site}` took the sheet's typeface: {family}"
                );
                // ...and what follows it is still the app's fallback chain,
                // so a filter field can spell what was typed into it.
                assert!(
                    family.contains("NotoColorEmoji.ttf"),
                    "{where_}: `{site}` lost the fallbacks: {family}"
                );
            }
        }
    }

    /// The panel's density and type are not a sheet's to move.
    ///
    /// The density and type entries are what make the panel an inspector: a
    /// 24px row, a 20px small row, 8.5pt words. A sheet reaching these would
    /// put `android`'s 48px controls back into the panel through the front
    /// door -- the very thing the sunken filter field was.
    #[test]
    fn the_panels_density_and_type_never_move_under_a_sheet() {
        use crate::desktop_style::{install, DesktopStyle, StyleSheet};
        const NUMBERS: &[(&str, f64)] = &[
            ("row_height", 24.0),
            ("row_height_sm", 20.0),
            ("header_height", 26.0),
            ("prop_label_width", 92.0),
            ("font_size_ui", 8.5),
            ("font_size_small", 7.5),
            ("font_size_header", 9.0),
        ];
        let mut cx = Cx::new(Box::new(|_, _| {}));
        cx.init_cx_os();
        cx.with_vm(crate::script_mod);
        for style in DesktopStyle::ALL {
            cx.with_vm(|vm| {
                install(vm, StyleSheet::load(style));
                vm.with_reload(crate::script_mod);
            });
            for (name, want) in NUMBERS {
                let got = cx.with_vm(|vm| {
                    let fab = vm.module(id!(fab));
                    vm.bx
                        .heap
                        .value(fab, LiveId::from_str(name).into(), NoTrap)
                        .as_f64()
                });
                assert_eq!(got, Some(*want), "`fab.{name}` moved under `{}`", style.id());
            }
        }
    }

    #[test]
    fn ended_finds_commit_after_changed_action() {
        let uid = WidgetUid(17);
        let actions: ActionsBuf = vec![
            Box::new(WidgetAction {
                data: None,
                action: Box::new(FabValueInputAction::Changed(72.0)),
                widget_uid: uid,
                group: None,
            }),
            Box::new(WidgetAction {
                data: None,
                action: Box::new(FabValueInputAction::Ended(73.0)),
                widget_uid: uid,
                group: None,
            }),
        ];
        assert_eq!(ended_value(&actions, uid), Some(73.0));
    }

    fn equalizer_row() -> SliderTravel {
        SliderTravel {
            min: 0.0,
            max: 100.0,
            step: 0.0,
            thumb: 12.0,
            inset: 2.0,
        }
    }

    /// The whole point of a track: the value is where the pointer IS, not
    /// how far it has travelled since the press. Both stops and the middle,
    /// measured from the track's left edge — 160 of track, 12 of thumb and 2
    /// of inset at each end leave 144 of travel, starting 8 in.
    #[test]
    fn the_track_reads_the_value_under_the_pointer() {
        let t = equalizer_row();
        let v = t.value_at(8.0, 160.0);
        assert!(v.abs() < 1e-9, "{v}");
        let v = t.value_at(80.0, 160.0);
        assert!((v - 50.0).abs() < 1e-9, "{v}");
        let v = t.value_at(152.0, 160.0);
        assert!((v - 100.0).abs() < 1e-9, "{v}");
    }

    /// ...and the thumb is drawn where a press will read it back. The shader
    /// is handed these same three numbers every draw, so this is also what
    /// keeps the pixels and the hit test from disagreeing.
    #[test]
    fn the_thumb_stands_where_a_press_reads_it_back() {
        let t = equalizer_row();
        for want in [0.0, 12.5, 33.0, 50.0, 99.0, 100.0] {
            let x = t.thumb_x(want, 160.0);
            let got = t.value_at(x, 160.0);
            assert!((got - want).abs() < 1e-9, "{want} came back as {got}");
        }
    }

    /// Past either stop is the stop. A track has nowhere else to go, and a
    /// detent that does not divide the range must not walk off the end of
    /// it either.
    #[test]
    fn the_value_can_never_leave_the_track() {
        let t = equalizer_row();
        assert!(t.value_at(-400.0, 160.0).abs() < 1e-9);
        assert!((t.value_at(4000.0, 160.0) - 100.0).abs() < 1e-9);
        assert!(t.settle(-1.0).abs() < 1e-9);
        assert!((t.settle(1e9) - 100.0).abs() < 1e-9);
        let detented = SliderTravel {
            step: 7.0,
            ..equalizer_row()
        };
        let v = detented.value_at(4000.0, 160.0);
        assert!((v - 98.0).abs() < 1e-9, "{v}");
        assert!(detented.settle(1e9) <= 100.0);
        assert!(detented.settle(-1e9) >= 0.0);
    }

    #[test]
    fn the_columns_split_the_name_and_the_number_off_the_track() {
        assert_eq!(slider_zone(4.0, 300.0, 100.0, 40.0), SliderZone::Label);
        assert_eq!(slider_zone(150.0, 300.0, 100.0, 40.0), SliderZone::Track);
        assert_eq!(slider_zone(280.0, 300.0, 100.0, 40.0), SliderZone::Readout);
    }

    #[test]
    fn a_sliders_commit_is_found_after_the_change_it_follows() {
        let uid = WidgetUid(19);
        let actions: ActionsBuf = vec![
            Box::new(WidgetAction {
                data: None,
                action: Box::new(FabSliderAction::Changed(41.0)),
                widget_uid: uid,
                group: None,
            }),
            Box::new(WidgetAction {
                data: None,
                action: Box::new(FabSliderAction::Ended(42.0)),
                widget_uid: uid,
                group: None,
            }),
        ];
        assert_eq!(slider_ended_value(&actions, uid), Some(42.0));
    }

    /// The new control is painted out of the panel's own table and nothing
    /// else. A face that writes a colour of its own is a face that stays
    /// dark under a light theme; a face that reads the app's theme is the
    /// white slab the whole palette was written to avoid.
    #[test]
    fn the_sliders_face_names_only_the_panels_own_palette() {
        let src = include_str!("fab_controls.rs")
            .split("#[cfg(test)]")
            .next()
            .expect("the file has a first half");
        let face = src
            .split("do #(DrawFabSlider::script_shader(vm)){")
            .nth(1)
            .expect("the file declares the slider's shader");
        let face = &face[..face
            .find("mod.widgets.FabSliderBase")
            .expect("the shader is followed by the registration")];
        assert!(
            !face.contains("#x"),
            "the slider's face writes a colour of its own"
        );
        assert!(
            !face.contains("theme."),
            "the slider's face reads the app's theme"
        );
        assert!(
            face.contains("fab.color_num"),
            "the slider's face is drawn from the fab table"
        );
    }

    /// The knob is held to the same table, and ALL of it is: the shader, and
    /// the template after it with its three inks. The slider's reading stops
    /// at the shader; this one runs on to the next control's, because a knob
    /// is put on the panel a hundred at a time and a word of it that took the
    /// app's ink would be a hundred words changing colour under the hand that
    /// is blending the app's theme.
    #[test]
    fn the_knob_names_only_the_panels_own_palette() {
        let src = include_str!("fab_controls.rs")
            .split("#[cfg(test)]")
            .next()
            .expect("the file has a first half");
        let knob = src
            .split("do #(DrawFabKnob::script_shader(vm)){")
            .nth(1)
            .expect("the file declares the knob's shader");
        let knob = &knob[..knob
            .find("do #(DrawColorWheel::script_shader(vm)){")
            .expect("the knob is followed by the colour wheel")];
        assert!(
            knob.contains("mod.widgets.FabKnob = "),
            "the reading stops short of the knob's template"
        );
        assert!(!knob.contains("#x"), "the knob writes a colour of its own");
        assert!(!knob.contains("vec4(0.") && !knob.contains("vec4(1."), "the knob writes a colour of its own");
        assert!(
            !knob.contains("theme.") && !knob.contains("mod.theme"),
            "the knob reads the app's theme"
        );
        for ink in ["fab.color_accent", "fab.color_num", "fab.color_button", "fab.color_focus_ring", "fab.color_text_muted"] {
            assert!(knob.contains(ink), "the knob's face no longer draws from `{ink}`");
        }
        // Every text style in it is the kit's own face.
        assert_eq!(
            knob.matches("text_style:").count(),
            knob.matches("text_style: fab.font{").count(),
            "a text style on the knob is not built from `fab.font`"
        );
    }

    /// A 26 point column of a knob matrix, 48 down.
    fn column() -> Rect {
        Rect {
            pos: dvec2(100.0, 40.0),
            size: dvec2(26.0, 48.0),
        }
    }

    /// The anchor, which is the whole control: one end of the name stands on
    /// the middle of its own column's bottom edge, so a name is always over
    /// the thing it names however long it is. A `Fall` ENDS there, a `Rise`
    /// STARTS there.
    #[test]
    fn a_name_stands_on_the_bottom_centre_of_its_own_column() {
        let b = column();
        let centre = dvec2(b.pos.x + 13.0, b.pos.y + 48.0);
        for name_width in [10.0, 50.0, 120.0] {
            let fall = diagonal_run(b, name_width, 10.0, 45.0, DiagonalLean::Fall);
            assert!(
                (fall.end - centre).length() < 1e-9,
                "a fall of {name_width} ends at {:?}, not {centre:?}",
                fall.end
            );
            let rise = diagonal_run(b, name_width, 10.0, 45.0, DiagonalLean::Rise);
            assert!(
                (rise.start - centre).length() < 1e-9,
                "a rise of {name_width} starts at {:?}, not {centre:?}",
                rise.start
            );
            // ...and the other end is one name away along the slope.
            assert!(((fall.end - fall.start).length() - name_width).abs() < 1e-9);
            assert!(((rise.end - rise.start).length() - name_width).abs() < 1e-9);
        }
    }

    /// The two leans hang over OPPOSITE sides, which is the reason both
    /// exist: over a matrix, `Fall` spills into the empty corner above the
    /// row names and `Rise` spills past the last column into the panel's
    /// edge.
    #[test]
    fn the_two_leans_hang_over_opposite_sides_of_the_column() {
        let b = column();
        let fall = diagonal_run(b, 50.0, 10.0, 45.0, DiagonalLean::Fall);
        let rise = diagonal_run(b, 50.0, 10.0, 45.0, DiagonalLean::Rise);
        assert!(fall.bounds.pos.x < b.pos.x, "a fall hangs over the left");
        assert!(
            fall.bounds.pos.x + fall.bounds.size.x <= b.pos.x + b.size.x + 1e-9,
            "a fall stays off the right"
        );
        assert!(
            rise.bounds.pos.x + rise.bounds.size.x > b.pos.x + b.size.x,
            "a rise hangs over the right"
        );
        assert!(rise.bounds.pos.x >= b.pos.x - 10.0, "a rise barely hangs left");
        // Both sit ON the column's bottom edge, whichever way they lean.
        for run in [fall, rise] {
            let bottom = run.bounds.pos.y + run.bounds.size.y;
            assert!((bottom - (b.pos.y + b.size.y)).abs() < 1e-9);
        }
        // ...and a name too long for the row it is in leaves the top of it
        // rather than shrinking or being cut: the row's height is the host's
        // to get right, and `diagonal_row_height` is how.
        let over = diagonal_run(b, 90.0, 10.0, 45.0, DiagonalLean::Fall);
        assert!(over.bounds.size.y > b.size.y);
        assert!(over.bounds.pos.y < b.pos.y, "the name stayed inside a box too short for it");
    }

    /// Nought degrees is a plain horizontal label on the box's bottom edge,
    /// not a special case: a column wide enough not to need the trick does
    /// not need a different control.
    #[test]
    fn a_turn_of_nothing_is_a_plain_horizontal_label() {
        let b = column();
        for lean in [DiagonalLean::Fall, DiagonalLean::Rise] {
            let run = diagonal_run(b, 50.0, 10.0, 0.0, lean);
            assert_eq!(run.angle, 0.0);
            assert!((run.start.y - run.end.y).abs() < 1e-9, "the baseline is level");
            assert!((run.start.y - (b.pos.y + b.size.y)).abs() < 1e-9, "on the bottom edge");
            assert!((run.bounds.size.y - 10.0).abs() < 1e-9, "one line tall");
        }
        assert!(
            (diagonal_row_height(50.0, 10.0, 0.0) - 10.0).abs() < 1e-9,
            "a level row is a line tall whatever the name is"
        );
    }

    /// A longer name needs a taller row, and the widget's bounds and the
    /// number a host fixes its row with are the same number.
    #[test]
    fn a_longer_name_needs_a_taller_header_row() {
        let short = diagonal_row_height(20.0, 10.0, 45.0);
        let long = diagonal_row_height(60.0, 10.0, 45.0);
        assert!(long > short + 20.0, "{short} -> {long}");
        // The sum itself: the name along the hypotenuse, the line across it.
        let want = 60.0 * std::f64::consts::FRAC_1_SQRT_2 + 10.0 * std::f64::consts::FRAC_1_SQRT_2;
        assert!((long - want).abs() < 1e-9, "{long} is not {want}");
        // ...and a steeper turn needs more room still.
        assert!(diagonal_row_height(60.0, 10.0, 60.0) > long);
        // What the helper says and what the run takes are one number.
        for lean in [DiagonalLean::Fall, DiagonalLean::Rise] {
            let run = diagonal_run(column(), 60.0, 10.0, 45.0, lean);
            assert!((run.bounds.size.y - long).abs() < 1e-9);
        }
    }

    /// The panel's own palette and the panel's own face, on the header as on
    /// everything else the kit draws. Read off the SOURCE, so a colour token
    /// borrowed from the app's theme fails here rather than on the day
    /// somebody installs a sheet. The same reading the knob gets, for the
    /// same reason.
    #[test]
    fn the_diagonal_header_names_only_the_panels_own_palette() {
        let src = include_str!("fab_controls.rs")
            .split("#[cfg(test)]")
            .next()
            .expect("the file has a first half");
        let label = src
            .split("mod.widgets.FabDiagonalLabelBase = ")
            .nth(1)
            .expect("the file declares the diagonal header");
        let label = &label[..label
            .find("// ---- the search well ----")
            .expect("the diagonal header is followed by the search well")];
        assert!(
            label.contains("mod.widgets.FabDiagonalLabel = "),
            "the reading stops short of the header's template"
        );
        assert!(!label.contains("#x"), "the header writes a colour of its own");
        assert!(
            !label.contains("theme.") && !label.contains("mod.theme"),
            "the header reads the app's theme"
        );
        for token in ["fab.color_text_dim", "fab.font_size_small"] {
            assert!(label.contains(token), "the header no longer draws from `{token}`");
        }
        assert_eq!(
            label.matches("text_style:").count(),
            label.matches("text_style: fab.font{").count(),
            "a text style on the header is not built from `fab.font`"
        );
    }

    /// The chip is the one control in the kit that is MEANT to be a colour
    /// the panel knows nothing about: its four bands are the theme being
    /// offered, and a chip drawn from the panel's table would show the panel
    /// instead of the palette. So the reading is the other way round -- the
    /// bands come off the host, and everything the panel owns, which is the
    /// ring that says which chip is hovered and which is in force, comes off
    /// the fab table like everything else.
    #[test]
    fn the_palette_chip_shows_the_hosts_colours_and_wears_the_panels_ring() {
        let src = include_str!("fab_controls.rs")
            .split("#[cfg(test)]")
            .next()
            .expect("the file has a first half");
        let chip = src
            .split("do #(DrawFabPaletteChip::script_shader(vm)){")
            .nth(1)
            .expect("the file declares the chip's shader");
        let chip = &chip[..chip
            .find("mod.widgets.FabColorPickBase")
            .expect("the chip is followed by the colour picker")];
        assert!(
            chip.contains("mod.widgets.FabPaletteChip = "),
            "the reading stops short of the chip's template"
        );
        assert!(!chip.contains("#x"), "the chip writes a colour of its own");
        assert!(
            !chip.contains("theme.") && !chip.contains("mod.theme"),
            "the chip reads the app's theme"
        );
        for band in ["self.band_0", "self.band_1", "self.band_2", "self.band_3"] {
            assert!(chip.contains(band), "the chip no longer paints `{band}`");
        }
        for ink in ["fab.color_border", "fab.color_focus_ring"] {
            assert!(chip.contains(ink), "the chip's ring no longer comes from `{ink}`");
        }
        // The one colour written out is the band's own opacity: a palette
        // holding a translucent colour must still show as a solid block, or
        // a chip becomes a reading of the panel's ground through it.
        assert_eq!(
            chip.matches("vec4(").count(),
            chip.matches("vec4(0.0, 0.0, 0.0, 1.0)").count() + chip.matches("vec4(band.xyz, 1.0)").count(),
            "the chip's face writes a colour that is neither a band nor a band's opacity"
        );
    }

    fn cell() -> KnobTurn {
        KnobTurn {
            min: 0.0,
            max: 100.0,
            step: 1.0,
            travel: 150.0,
        }
    }

    /// The whole range in the travel the knob names, up for more -- and the
    /// same travel whatever size the knob is drawn at, which is the point of
    /// the number being its own.
    #[test]
    fn a_knob_crosses_its_range_in_the_travel_it_names() {
        let t = cell();
        assert!((t.carry(0.0, 150.0, false) - 100.0).abs() < 1e-9);
        assert!((t.carry(0.0, 75.0, false) - 50.0).abs() < 1e-9);
        assert!((t.carry(50.0, -75.0, false) - 0.0).abs() < 1e-9, "down is less");
        // Shift is a tenth of the speed: the same travel, a tenth of the way.
        assert!((t.carry(0.0, 150.0, true) - 10.0).abs() < 1e-9);
        // A travel that is not one falls back rather than dividing by it.
        let broken = KnobTurn { travel: 0.0, ..cell() };
        assert!((broken.carry(0.0, 75.0, false) - 50.0).abs() < 1e-9);
    }

    /// Past either stop is the stop, AND the anchor goes with the hand: a
    /// pointer that overshot the top by a mile starts bringing the value down
    /// on the first pixel of its way back.
    #[test]
    fn a_knob_can_never_leave_its_range_and_does_not_wind_up_past_it() {
        let t = cell();
        let top = t.carry(90.0, 4000.0, false);
        assert_eq!(top, 100.0);
        let back = t.carry(top, -15.0, false);
        assert!((back - 90.0).abs() < 1e-9, "the overshoot had to be unwound first: {back}");
        assert_eq!(t.carry(5.0, -4000.0, false), 0.0);
        assert_eq!(t.settle(1e9), 100.0);
        assert_eq!(t.contain(-3.0), 0.0);
        // The host's number keeps its fraction; the hand's lands on the
        // detent.
        assert_eq!(t.contain(37.5), 37.5);
        assert_eq!(t.settle(37.4), 37.0);
    }

    /// A fine drag over a coarse detent still gets somewhere, because what is
    /// carried from move to move is what the hand asked for and not what the
    /// detent made of it.
    #[test]
    fn a_fine_drag_is_not_rounded_back_to_where_it_started() {
        let t = KnobTurn { step: 5.0, ..cell() };
        let mut raw = 50.0;
        let mut said = Vec::new();
        for _ in 0..60 {
            raw = t.carry(raw, 1.0, true);
            said.push(t.settle(raw));
        }
        // Sixty points at a tenth of the speed is four parts of the range.
        assert!((raw - 54.0).abs() < 1e-9, "{raw}");
        assert_eq!(*said.last().unwrap(), 55.0, "the detent swallowed the drag");
        assert!(said.iter().all(|v| *v == 50.0 || *v == 55.0));
    }

    /// The face is the biggest circle the box holds once the words have had
    /// their rows, and the slack is shared above and below.
    #[test]
    fn the_face_is_what_the_words_leave_of_the_box() {
        // The default: 44 by 64 with the number and no name.
        assert_eq!(knob_face(44.0, 64.0, 0.0, 12.0), KnobFace { diameter: 44.0, top: 4.0 });
        // A name as well takes the face down to what is left of the height.
        assert_eq!(knob_face(44.0, 64.0, 12.0, 12.0), KnobFace { diameter: 40.0, top: 0.0 });
        // The cell the matrix comes down to: the width is what binds.
        assert_eq!(knob_face(28.0, 64.0, 0.0, 12.0), KnobFace { diameter: 28.0, top: 12.0 });
        // No words at all, in a square: all face.
        assert_eq!(knob_face(28.0, 28.0, 0.0, 0.0), KnobFace { diameter: 28.0, top: 0.0 });
        // A box with no room is a face of nothing, never a negative one.
        assert_eq!(knob_face(28.0, 10.0, 12.0, 12.0), KnobFace { diameter: 0.0, top: 0.0 });
    }

    /// ...and the shader finds the face by the same three lines. It cannot be
    /// run from here, so it is READ: the day somebody changes one copy of the
    /// arithmetic, this is what says there is another.
    #[test]
    fn the_shader_finds_the_face_by_the_same_arithmetic() {
        let src = include_str!("fab_controls.rs")
            .split("#[cfg(test)]")
            .next()
            .expect("the file has a first half");
        for line in [
            "let rows = self.label_px + self.readout_px",
            "let d = max(min(self.rect_size.x, self.rect_size.y - rows), 0.0)",
            "let top = max((self.rect_size.y - rows - d) * 0.5, 0.0)",
            "let rows = label_px + readout_px;",
            "let diameter = width.min(height - rows).max(0.0);",
            "let top = ((height - rows - diameter) * 0.5).max(0.0);",
        ] {
            assert!(src.contains(line), "`{line}` is gone: the face is measured two ways now");
        }
    }

    /// The unit stands hard against the number, because the cell is 28 wide.
    #[test]
    fn a_knobs_number_carries_its_unit_without_a_space() {
        assert_eq!(knob_readout(100.0, 0, "%"), "100%");
        assert_eq!(knob_readout(37.5, 0, "%"), "38%");
        assert_eq!(knob_readout(0.25, 2, ""), "0.25");
    }

    #[test]
    fn a_double_press_is_two_presses_close_in_time_and_place() {
        let first = Some((10.0, dvec2(100.0, 100.0)));
        assert!(is_double_press(first, 10.2, dvec2(101.0, 99.0)));
        assert!(!is_double_press(first, 10.7, dvec2(100.0, 100.0)), "too late");
        assert!(!is_double_press(first, 10.2, dvec2(100.0, 120.0)), "too far");
        assert!(!is_double_press(None, 10.2, dvec2(100.0, 100.0)), "there was no first");
    }

    #[test]
    fn a_knobs_commit_is_found_after_the_change_it_follows() {
        let uid = WidgetUid(23);
        let actions: ActionsBuf = vec![
            Box::new(WidgetAction {
                data: None,
                action: Box::new(FabKnobAction::Changed(41.0)),
                widget_uid: uid,
                group: None,
            }),
            Box::new(WidgetAction {
                data: None,
                action: Box::new(FabKnobAction::Ended(42.0)),
                widget_uid: uid,
                group: None,
            }),
        ];
        assert_eq!(knob_ended_value(&actions, uid), Some(42.0));
    }
}

/// THE POINTER-CAPTURE RULE, as it applies to the track.
///
/// One press, one owner: the face takes the mouse on the way in through
/// `hits` and holds it until the release, which is what lets a scroller
/// around this control stand its own drag down while a thumb is being
/// moved. The name is a box the release is MEASURED against, never a second
/// area asking for a hit of its own — that second ask is what once left the
/// stock Slider unable to be dragged from its own legend.
#[cfg(test)]
mod fab_slider_gestures {
    #![allow(dead_code)]
    use super::*;
    use crate::makepad_draw::cx_draw::CxDraw;
    use std::cell::Cell;

    const SIZE: Vec2d = Vec2d { x: 800.0, y: 600.0 };
    const WINDOW: WindowId = WindowId(1, 1);

    pub(super) struct Target {
        pass: DrawPass,
        draw_list: DrawList2d,
    }

    impl Target {
        pub(super) fn new(cx: &mut Cx) -> Self {
            Target {
                pass: DrawPass::new(cx),
                draw_list: DrawList2d::new(cx),
            }
        }

        pub(super) fn draw(&mut self, cx: &mut Cx, root: &WidgetRef) {
            self.pass.set_size(cx, SIZE);
            let event = DrawEvent::default();
            let mut draw = CxDraw::new(cx, &event);
            let mut cx2d = Cx2d::new(&mut draw);
            cx2d.begin_pass(&self.pass, None);
            self.draw_list.begin_always(&mut cx2d);
            cx2d.begin_root_turtle(SIZE, Layout::flow_down());
            root.draw_all(&mut cx2d, &mut Scope::empty());
            cx2d.end_pass_sized_turtle();
            self.draw_list.end(&mut cx2d);
            cx2d.end_pass(&self.pass);
        }
    }

    pub(super) fn press(abs: Vec2d, time: f64) -> Event {
        Event::MouseDown(MouseDownEvent {
            abs,
            button: MouseButton::PRIMARY,
            window_id: WINDOW,
            modifiers: KeyModifiers::default(),
            handled: Cell::new(Area::Empty),
            time,
        })
    }

    pub(super) fn moved(abs: Vec2d, time: f64) -> Event {
        Event::MouseMove(MouseMoveEvent {
            abs,
            lock_delta: Vec2d::default(),
            window_id: WINDOW,
            modifiers: KeyModifiers::default(),
            handled: Cell::new(Area::Empty),
            time,
        })
    }

    pub(super) fn release(abs: Vec2d, time: f64) -> Event {
        Event::MouseUp(MouseUpEvent {
            abs,
            button: MouseButton::PRIMARY,
            window_id: WINDOW,
            modifiers: KeyModifiers::default(),
            time,
        })
    }

    pub(super) fn send(cx: &mut Cx, root: &WidgetRef, event: &Event) -> ActionsBuf {
        cx.capture_actions(|cx| root.handle_event(cx, event, &mut Scope::empty()))
    }

    fn scene(cx: &mut Cx) -> WidgetRef {
        cx.with_vm(|vm| {
            let value = crate::script_eval!(vm, {
                use mod.prelude.widgets.*
                use mod.widgets.*
                View{
                    width: Fill
                    height: Fill
                    flow: Down
                    weight := FabSlider{
                        width: 300.
                        label: "one"
                        value: 50.0
                    }
                }
            });
            WidgetRef::script_from_value(vm, value)
        })
    }

    fn start(cx: &mut Cx) -> (WidgetRef, WidgetRef) {
        cx.init_cx_os();
        cx.with_vm(crate::script_mod);
        let root = scene(cx);
        let mut target = Target::new(cx);
        target.draw(cx, &root);
        let weight = root.widget(cx, ids!(weight));
        assert!(!weight.is_empty(), "the scene has a slider in it");
        (root, weight)
    }

    /// The window x of a point `travel` (0..1) along the track, taken the
    /// way the control takes it.
    fn on_the_track(cx: &Cx, weight: &WidgetRef, travel: f64) -> Vec2d {
        let inner = weight.borrow::<FabSlider>().unwrap();
        let face = inner.draw_bg.area().rect(cx);
        assert!(face.size.x > 0.0, "the slider was drawn");
        let (label_px, readout_px) = inner.columns();
        let width = face.size.x - label_px - readout_px;
        let t = inner.travel();
        let (lo, hi) = t.stops();
        dvec2(
            face.pos.x + label_px + t.thumb_x(lo + travel * (hi - lo), width),
            face.pos.y + face.size.y * 0.5,
        )
    }

    fn on_the_word(cx: &Cx, weight: &WidgetRef) -> Vec2d {
        let rect = weight.borrow::<FabSlider>().unwrap().label_area.rect(cx);
        assert!(
            rect.size.x > 0.0 && rect.size.y > 0.0,
            "the name was drawn, or this test is pressing nothing"
        );
        rect.pos + rect.size * 0.5
    }

    fn changed(actions: &ActionsBuf, weight: &WidgetRef) -> Option<f64> {
        weight.as_fab_slider().changed(actions)
    }

    fn ended(actions: &ActionsBuf, weight: &WidgetRef) -> Option<f64> {
        weight.as_fab_slider().ended(actions)
    }

    fn was_reset(actions: &ActionsBuf, weight: &WidgetRef) -> bool {
        weight.as_fab_slider().was_reset(actions)
    }

    /// A press on the track lands the value under the pointer at once — no
    /// threshold, no anchor, nothing to travel through first — and the move
    /// keeps it there.
    #[test]
    fn a_press_on_the_track_jumps_the_thumb_and_then_tracks_it() {
        let mut cx = Cx::new(Box::new(|_, _| {}));
        let (root, weight) = start(&mut cx);
        let at = on_the_track(&cx, &weight, 0.25);
        cx.fingers.first_mouse_button = Some((MouseButton::PRIMARY, WINDOW));
        let actions = send(&mut cx, &root, &press(at, 0.0));
        assert_eq!(
            changed(&actions, &weight),
            Some(25.0),
            "the press landed the thumb"
        );
        let to = on_the_track(&cx, &weight, 0.75);
        let actions = send(&mut cx, &root, &moved(to, 0.1));
        assert_eq!(
            changed(&actions, &weight),
            Some(75.0),
            "the drag followed the pointer"
        );
        let actions = send(&mut cx, &root, &release(to, 0.2));
        assert_eq!(ended(&actions, &weight), Some(75.0));
        cx.fingers.first_mouse_button = None;
    }

    /// The name is the reset, and the commit that follows it carries the
    /// zero rather than the value the press found.
    #[test]
    fn a_tap_on_the_name_resets_the_row_before_it_commits() {
        let mut cx = Cx::new(Box::new(|_, _| {}));
        let (root, weight) = start(&mut cx);
        let at = on_the_word(&cx, &weight);
        cx.fingers.first_mouse_button = Some((MouseButton::PRIMARY, WINDOW));
        let actions = send(&mut cx, &root, &press(at, 0.0));
        assert_eq!(
            changed(&actions, &weight),
            None,
            "a press on the name moves nothing"
        );
        let actions = send(&mut cx, &root, &release(at, 0.0));
        assert!(was_reset(&actions, &weight), "the tap read as a reset");
        assert_eq!(
            ended(&actions, &weight),
            Some(0.0),
            "and the commit carries the zero"
        );
        cx.fingers.first_mouse_button = None;
    }

    pub(super) fn key(key_code: KeyCode, shift: bool) -> Event {
        Event::KeyDown(KeyEvent {
            key_code,
            is_repeat: false,
            modifiers: KeyModifiers {
                shift,
                ..KeyModifiers::default()
            },
            time: 0.0,
        })
    }

    /// What a caller SETS is what the row holds.
    ///
    /// `step: 1.0` and `precision: 0` are what a weight row wants under a
    /// finger. Neither is a licence to round the host's own arithmetic on the
    /// way in: four rows splitting a hundred parts come down to 0 / 37.5 /
    /// 37.5 / 25, and rows that stored 38 put 101% on screen under a legend
    /// promising a hundred.
    #[test]
    fn a_value_set_from_outside_is_held_exactly() {
        let mut cx = Cx::new(Box::new(|_, _| {}));
        let (_root, weight) = start(&mut cx);
        let row = weight.as_fab_slider();
        row.set_value(&mut cx, 37.5);
        assert_eq!(
            row.value(),
            37.5,
            "the detent rounded a number nobody dragged"
        );
        // The stops are not the detent, and they still hold.
        row.set_value(&mut cx, 120.0);
        assert_eq!(row.value(), 100.0);
        row.set_value(&mut cx, -3.0);
        assert_eq!(row.value(), 0.0);
    }

    /// An arrow moves one step from where the row STANDS.
    ///
    /// Focused the way a hand focuses it -- a press on the track and a
    /// release -- and only then is the off-detent value pushed in, which is
    /// the panel's own order: the host writes the weights, the hand nudges
    /// one of them. Settling the sum onto the grid read the origin off the
    /// grid first and published 39 from a row standing at 37.5, a jump of a
    /// step and a half that the ledger behind the panel then had to absorb.
    #[test]
    fn an_arrow_moves_one_step_from_where_the_row_stands() {
        let mut cx = Cx::new(Box::new(|_, _| {}));
        let (root, weight) = start(&mut cx);
        let at = on_the_track(&cx, &weight, 0.5);
        cx.fingers.first_mouse_button = Some((MouseButton::PRIMARY, WINDOW));
        // Dispatched rather than captured: `set_key_focus` only records the
        // request, and the focus moves on the cycle that runs once the
        // press's actions have gone out.
        root.handle_event(&mut cx, &press(at, 0.0), &mut Scope::empty());
        cx.handle_actions();
        root.handle_event(&mut cx, &release(at, 0.1), &mut Scope::empty());
        cx.handle_actions();
        cx.fingers.first_mouse_button = None;
        let face = weight.borrow::<FabSlider>().unwrap().draw_bg.area();
        assert!(
            cx.has_key_focus(face),
            "the press left the keyboard elsewhere, so the arrows below reach nothing"
        );
        weight.as_fab_slider().set_value(&mut cx, 37.5);

        let actions = send(&mut cx, &root, &key(KeyCode::ArrowRight, false));
        assert_eq!(
            changed(&actions, &weight),
            Some(38.5),
            "the arrow started from the rounded number"
        );
        let actions = send(&mut cx, &root, &key(KeyCode::ArrowLeft, false));
        assert_eq!(changed(&actions, &weight), Some(37.5), "and back again");
        // Shift is the coarse step, taken from the same true origin.
        let actions = send(&mut cx, &root, &key(KeyCode::ArrowRight, true));
        assert_eq!(changed(&actions, &weight), Some(47.5));
    }

    /// The detent is still the detent for the hand that is on the thumb: a
    /// drag lands on whole parts however the row was set.
    #[test]
    fn a_drag_still_lands_on_the_detent() {
        let mut cx = Cx::new(Box::new(|_, _| {}));
        let (root, weight) = start(&mut cx);
        weight.as_fab_slider().set_value(&mut cx, 37.5);
        let at = on_the_track(&cx, &weight, 0.617);
        cx.fingers.first_mouse_button = Some((MouseButton::PRIMARY, WINDOW));
        let actions = send(&mut cx, &root, &press(at, 0.0));
        let landed = changed(&actions, &weight).expect("the press moved the thumb");
        assert_eq!(landed, landed.round(), "the drag came to rest off the detent");
        send(&mut cx, &root, &release(at, 0.1));
        cx.fingers.first_mouse_button = None;
    }

    pub(super) fn key_repeat(key_code: KeyCode) -> Event {
        Event::KeyDown(KeyEvent {
            key_code,
            is_repeat: true,
            modifiers: KeyModifiers::default(),
            time: 0.0,
        })
    }

    pub(super) fn key_up(key_code: KeyCode) -> Event {
        Event::KeyUp(KeyEvent {
            key_code,
            is_repeat: false,
            modifiers: KeyModifiers::default(),
            time: 0.0,
        })
    }

    pub(super) fn key_with(key_code: KeyCode, modifiers: KeyModifiers) -> Event {
        Event::KeyDown(KeyEvent {
            key_code,
            is_repeat: false,
            modifiers,
            time: 0.0,
        })
    }

    /// How many commits one event produced. A commit is the expensive word in
    /// this control's vocabulary -- the panel rebuilds a module on each one --
    /// so the COUNT is the thing under test, not whether one arrived.
    fn commits(actions: &ActionsBuf, weight: &WidgetRef) -> usize {
        actions
            .filter_widget_actions_cast::<FabSliderAction>(weight.widget_uid())
            .filter(|action| matches!(action, FabSliderAction::Ended(_)))
            .count()
    }

    /// The keyboard, taken the way a hand has to take it: there is no tab
    /// order into this row, so a press on the track and a release is the only
    /// door in. Dispatched rather than captured -- `set_key_focus` only
    /// records the request, and the focus moves on the cycle that runs once
    /// the press's actions have gone out.
    fn give_it_the_keyboard(cx: &mut Cx, root: &WidgetRef, weight: &WidgetRef) {
        let at = on_the_track(cx, weight, 0.5);
        cx.fingers.first_mouse_button = Some((MouseButton::PRIMARY, WINDOW));
        root.handle_event(cx, &press(at, 0.0), &mut Scope::empty());
        cx.handle_actions();
        root.handle_event(cx, &release(at, 0.1), &mut Scope::empty());
        cx.handle_actions();
        cx.fingers.first_mouse_button = None;
        let face = weight.borrow::<FabSlider>().unwrap().draw_bg.area();
        assert!(
            cx.has_key_focus(face),
            "the press left the keyboard elsewhere, so the keys below reach nothing"
        );
    }

    /// A HELD arrow is ONE gesture, not thirty.
    ///
    /// The keyboard sends a repeat every thirty-odd milliseconds. A host that
    /// reads each of them as the end of a gesture does its end-of-gesture work
    /// thirty times for one second of a held key, and the panel these rows sit
    /// in spends a module rebuild on every one of them -- a second of held
    /// arrow for a second of main thread nobody can draw on. The same second
    /// spent dragging costs a handful, because a drag reports moves and the
    /// settle behind them bounds it. This is the test that says the arrows are
    /// bounded the same way.
    #[test]
    fn a_held_arrow_commits_for_the_run_and_not_once_per_repeat() {
        let mut cx = Cx::new(Box::new(|_, _| {}));
        let (root, weight) = start(&mut cx);
        give_it_the_keyboard(&mut cx, &root, &weight);
        weight.as_fab_slider().set_value(&mut cx, 10.0);

        // The deliberate press commits where it lands: one tap of an arrow is
        // a whole gesture, and it must not wait for a release to be seen.
        let actions = send(&mut cx, &root, &key(KeyCode::ArrowRight, false));
        assert_eq!(changed(&actions, &weight), Some(11.0));
        assert_eq!(
            commits(&actions, &weight),
            1,
            "a single deliberate press has to land at once"
        );

        // ...and then the key is HELD. Every repeat moves the row; not one of
        // them ends a gesture the hand has not let go of.
        let mut moved = 0;
        for beat in 0..29 {
            let actions = send(&mut cx, &root, &key_repeat(KeyCode::ArrowRight));
            moved += changed(&actions, &weight).is_some() as usize;
            assert_eq!(
                commits(&actions, &weight),
                0,
                "repeat {beat} ended a gesture the hand has not let go of"
            );
        }
        assert_eq!(moved, 29, "the repeats stopped moving the row");

        // The release is the end, and it carries what the run stopped on.
        let actions = send(&mut cx, &root, &key_up(KeyCode::ArrowRight));
        assert_eq!(
            commits(&actions, &weight),
            1,
            "the run ended without a commit, so its last value never left the row"
        );
        assert_eq!(ended(&actions, &weight), Some(40.0));
        assert_eq!(weight.as_fab_slider().value(), 40.0);
    }

    /// The value a run stops on is the value the host has to end up holding.
    ///
    /// The repeats say only "moved", so the commit rides on the release -- and
    /// a release that never arrives, because the keyboard went somewhere else
    /// mid-run, must still pay what the run owes. Otherwise the last thing the
    /// hand did sits on the row and reaches nothing.
    #[test]
    fn a_run_the_keyboard_walks_out_on_still_commits() {
        let mut cx = Cx::new(Box::new(|_, _| {}));
        let (root, weight) = start(&mut cx);
        give_it_the_keyboard(&mut cx, &root, &weight);
        weight.as_fab_slider().set_value(&mut cx, 10.0);
        send(&mut cx, &root, &key(KeyCode::ArrowRight, false));
        let actions = send(&mut cx, &root, &key_repeat(KeyCode::ArrowRight));
        assert_eq!(changed(&actions, &weight), Some(12.0));
        assert_eq!(commits(&actions, &weight), 0, "the repeat is mid-run");

        let face = weight.borrow::<FabSlider>().unwrap().draw_bg.area();
        let actions = send(
            &mut cx,
            &root,
            &Event::KeyFocus(KeyFocusEvent {
                prev: face,
                focus: Area::Empty,
            }),
        );
        assert_eq!(
            ended(&actions, &weight),
            Some(12.0),
            "the run was abandoned with a value the host had never been told to keep"
        );
        // ...and once only. What is owed is owed once.
        let actions = send(
            &mut cx,
            &root,
            &Event::KeyFocus(KeyFocusEvent {
                prev: face,
                focus: Area::Empty,
            }),
        );
        assert_eq!(commits(&actions, &weight), 0);
    }

    /// An arrow steps from what the row HOLDS, not from what it PRINTS.
    ///
    /// A host whose column is read as a whole rounds over the whole column,
    /// and the leftover parts come off the largest share -- so the biggest
    /// row of a mix prints several parts below the weight it carries.
    /// Written into the value, which was once the only way to say it, that
    /// printed number became the one the next arrow started from: the key
    /// that means MORE asked for less than the row already had, and walked
    /// the biggest theme in the mix down four parts a press. Where the gap
    /// was narrower than one step the row could not move at all, and every
    /// press still cost the host its end-of-gesture work.
    #[test]
    fn an_arrow_steps_from_what_the_row_holds_and_not_what_it_prints() {
        let mut cx = Cx::new(Box::new(|_, _| {}));
        let (root, weight) = start(&mut cx);
        give_it_the_keyboard(&mut cx, &root, &weight);
        let row = weight.as_fab_slider();
        // The shape a mix with a tail leaves: a dominant row, and four parts
        // of the column's rounding taken off it.
        row.set_value_and_readout(&mut cx, 85.5, 81.0);
        assert_eq!(row.value(), 85.5, "the weight the row stands for");
        assert_eq!(row.readout(), 81.0, "the share the column prints");

        let actions = send(&mut cx, &root, &key(KeyCode::ArrowRight, false));
        assert_eq!(
            changed(&actions, &weight),
            Some(86.5),
            "the arrow stepped from the printed share, so the key that means more asked for \
             less than the row already held"
        );
        assert_eq!(commits(&actions, &weight), 1, "a deliberate press lands at once");
        assert_eq!(ended(&actions, &weight), Some(86.5));
        // ...and the row prints its own number again. A share worked out for
        // the weight before this one is a share this row no longer has.
        assert_eq!(
            row.readout(),
            86.5,
            "the row went on printing a share the hand has moved it off"
        );

        // A plain value takes the readout back with it, so a host cannot
        // leave one standing over a number it was never worked out for.
        row.set_value(&mut cx, 40.0);
        assert_eq!(row.readout(), 40.0);
    }

    /// A run the WINDOW walks out on still commits.
    ///
    /// `Hit::KeyFocusLost` does not fire when the whole app is deactivated --
    /// the focus INSIDE it has not moved -- so an arrow held while the hand
    /// alt-tabs away ended the run with nothing told about where it stopped.
    /// The repeats say only that the value moved, so a host that rate-limits
    /// those and acts on the commit was left holding the value from before
    /// the run.
    #[test]
    fn a_run_the_window_walks_out_on_still_commits() {
        let mut cx = Cx::new(Box::new(|_, _| {}));
        let (root, weight) = start(&mut cx);
        give_it_the_keyboard(&mut cx, &root, &weight);
        weight.as_fab_slider().set_value(&mut cx, 10.0);
        send(&mut cx, &root, &key(KeyCode::ArrowRight, false));
        let actions = send(&mut cx, &root, &key_repeat(KeyCode::ArrowRight));
        assert_eq!(changed(&actions, &weight), Some(12.0));
        assert_eq!(commits(&actions, &weight), 0, "the repeat is mid-run");

        let actions = send(&mut cx, &root, &Event::WindowLostFocus(WINDOW));
        assert_eq!(
            ended(&actions, &weight),
            Some(12.0),
            "the window took the keyboard away and the run went uncommitted"
        );
        // ...and once only. What is owed is owed once.
        let actions = send(&mut cx, &root, &Event::WindowLostFocus(WINDOW));
        assert_eq!(commits(&actions, &weight), 0);
        assert_eq!(
            weight.as_fab_slider().value(),
            12.0,
            "the deactivation moved the row the hand had stopped on"
        );
    }

    /// End is ABSOLUTE: the first press names the stop, and every repeat after
    /// it names the same stop. A key held against the end of its own travel is
    /// a key this row has already answered, and answering it again is an
    /// end-of-gesture for a value that did not move.
    #[test]
    fn a_held_stop_key_commits_once_and_then_has_nothing_to_say() {
        let mut cx = Cx::new(Box::new(|_, _| {}));
        let (root, weight) = start(&mut cx);
        give_it_the_keyboard(&mut cx, &root, &weight);
        weight.as_fab_slider().set_value(&mut cx, 10.0);

        let actions = send(&mut cx, &root, &key(KeyCode::End, false));
        assert_eq!(
            changed(&actions, &weight),
            Some(100.0),
            "End is the far stop, and it goes there on the press"
        );
        assert_eq!(
            commits(&actions, &weight),
            1,
            "a stop key has to feel immediate"
        );
        for beat in 0..10 {
            let actions = send(&mut cx, &root, &key_repeat(KeyCode::End));
            assert_eq!(changed(&actions, &weight), None);
            assert_eq!(
                commits(&actions, &weight),
                0,
                "repeat {beat} commits the stop the row is already standing on"
            );
        }
        let actions = send(&mut cx, &root, &key_up(KeyCode::End));
        assert_eq!(
            commits(&actions, &weight),
            0,
            "the press paid for itself; the release owes nothing"
        );
    }

    /// Ctrl and Cmd belong to whatever the row is sitting IN.
    ///
    /// Ctrl+Home is the panel going to its top and Cmd+Arrow is the window
    /// manager's; a focused row that nudges on either is a row that broke a
    /// shortcut the rest of the app still honours -- silently, and only while
    /// the keyboard happens to be on it. Shift is this control's own.
    #[test]
    fn the_accelerator_modifiers_are_not_this_rows_to_take() {
        let mut cx = Cx::new(Box::new(|_, _| {}));
        let (root, weight) = start(&mut cx);
        give_it_the_keyboard(&mut cx, &root, &weight);
        let row = weight.as_fab_slider();
        row.set_value(&mut cx, 10.0);

        let held = [
            (
                "ctrl",
                KeyModifiers {
                    control: true,
                    ..KeyModifiers::default()
                },
            ),
            (
                "cmd",
                KeyModifiers {
                    logo: true,
                    ..KeyModifiers::default()
                },
            ),
        ];
        for (name, modifiers) in held {
            for key_code in [KeyCode::ArrowRight, KeyCode::ArrowDown, KeyCode::End] {
                let actions = send(&mut cx, &root, &key_with(key_code, modifiers));
                assert_eq!(
                    changed(&actions, &weight),
                    None,
                    "{name}+{key_code:?} moved the row"
                );
                assert_eq!(commits(&actions, &weight), 0);
            }
        }
        assert_eq!(row.value(), 10.0, "the row answered an accelerator");

        // ...and Shift, which IS the row's own, still takes the coarse step.
        let actions = send(&mut cx, &root, &key(KeyCode::ArrowRight, true));
        assert_eq!(changed(&actions, &weight), Some(20.0));
    }

    /// One press, one owner. This is the reading a scroller takes before it
    /// starts dragging its content, so a second hold anywhere on the row
    /// would be the panel scrolling out from under a thumb.
    #[test]
    fn the_face_is_the_only_thing_holding_that_press() {
        let mut cx = Cx::new(Box::new(|_, _| {}));
        let (root, weight) = start(&mut cx);
        let at = on_the_track(&cx, &weight, 0.5);
        let face = weight.borrow::<FabSlider>().unwrap().draw_bg.area();
        cx.fingers.first_mouse_button = Some((MouseButton::PRIMARY, WINDOW));
        send(&mut cx, &root, &press(at, 0.0));
        assert!(
            cx.fingers.is_area_captured(face),
            "the track took the pointer"
        );
        assert!(
            !cx.fingers.is_mouse_held_outside(&[face]),
            "and nothing else on the row took a second hold of it"
        );
        cx.fingers.first_mouse_button = None;
    }
}

/// THE POINTER-CAPTURE RULE, as it applies to the dial, and the rest of what
/// a hand can do to one.
///
/// The slider's harness, borrowed: a real knob, drawn once so it has an area
/// a press can land on, taking real events. What a headless run cannot do is
/// let a digit go -- the release is the platform loop's -- so the capture
/// outlives each press here, which costs these tests nothing: the knob keeps
/// its own account of where a press landed and when.
#[cfg(test)]
mod fab_knob_gestures {
    #![allow(dead_code)]
    use super::fab_slider_gestures::{key, key_repeat, key_up, press, release, send, Target};
    use super::*;
    use crate::event::{ScrollEvent, ScrollPhase};
    use std::cell::Cell;

    const WINDOW: WindowId = WindowId(1, 1);

    /// Four knobs in a row, the way a row of the matrix is: the default cell,
    /// its neighbour, the narrowest cell there is, and one with a name.
    fn scene(cx: &mut Cx) -> WidgetRef {
        cx.with_vm(|vm| {
            let value = crate::script_eval!(vm, {
                use mod.prelude.widgets.*
                use mod.widgets.*
                View{
                    width: Fill
                    height: Fill
                    flow: Right
                    cell := FabKnob{value: 50.0}
                    other := FabKnob{value: 20.0}
                    small := FabKnob{width: 28. height: 28. show_readout: false}
                    named := FabKnob{label: "mix"}
                }
            });
            WidgetRef::script_from_value(vm, value)
        })
    }

    fn start(cx: &mut Cx) -> (WidgetRef, WidgetRef) {
        cx.init_cx_os();
        cx.with_vm(crate::script_mod);
        let root = scene(cx);
        let mut target = Target::new(cx);
        target.draw(cx, &root);
        let cell = root.widget(cx, ids!(cell));
        assert!(!cell.is_empty(), "the scene has a knob in it");
        (root, cell)
    }

    fn face_of(knob: &WidgetRef) -> Area {
        knob.borrow::<FabKnob>().unwrap().draw_bg.area()
    }

    /// The middle of a knob's box, which is where a hand takes hold of one.
    fn middle(cx: &Cx, knob: &WidgetRef) -> Vec2d {
        let rect = face_of(knob).rect(cx);
        assert!(rect.size.x > 0.0 && rect.size.y > 0.0, "the knob was drawn");
        rect.pos + rect.size * 0.5
    }

    fn moved_with(abs: Vec2d, time: f64, shift: bool) -> Event {
        Event::MouseMove(MouseMoveEvent {
            abs,
            lock_delta: Vec2d::default(),
            window_id: WINDOW,
            modifiers: KeyModifiers {
                shift,
                ..KeyModifiers::default()
            },
            handled: Cell::new(Area::Empty),
            time,
        })
    }

    /// `notches` of a real wheel over `abs`, up for positive.
    fn wheel(abs: Vec2d, notches: f64, shift: bool) -> Event {
        Event::Scroll(ScrollEvent {
            window_id: WINDOW,
            scroll: dvec2(0.0, -120.0 * notches),
            abs,
            modifiers: KeyModifiers {
                shift,
                ..KeyModifiers::default()
            },
            handled_x: Cell::new(false),
            handled_y: Cell::new(false),
            is_mouse: true,
            time: 0.0,
            phase: ScrollPhase::Changed,
        })
    }

    fn changed(actions: &ActionsBuf, knob: &WidgetRef) -> Option<f64> {
        knob.as_fab_knob().changed(actions)
    }

    fn ended(actions: &ActionsBuf, knob: &WidgetRef) -> Option<f64> {
        knob.as_fab_knob().ended(actions)
    }

    /// Everything one knob said in one buffer, in the order it said it.
    fn said(actions: &ActionsBuf, knob: &WidgetRef) -> Vec<String> {
        actions
            .filter_widget_actions_cast::<FabKnobAction>(knob.widget_uid())
            .map(|action| format!("{action:?}"))
            .collect()
    }

    fn commits(actions: &ActionsBuf, knob: &WidgetRef) -> usize {
        actions
            .filter_widget_actions_cast::<FabKnobAction>(knob.widget_uid())
            .filter(|action| matches!(action, FabKnobAction::Ended(_)))
            .count()
    }

    /// Something for `handle_actions` to carry, and nothing else.
    #[derive(Debug)]
    struct Tick;

    /// The keyboard, taken the only way there is to take it: a press on the
    /// knob and a release that moved nothing. Dispatched rather than
    /// captured, because `set_key_focus` only records the request.
    ///
    /// The focus moves on the cycle behind an event, and the only such cycle
    /// a headless run can reach is the one `handle_actions` runs when it has
    /// an action to carry. The slider's press always has one, because a
    /// press on a track moves the thumb. A press on a knob turns nothing and
    /// says nothing, so the cycle is given a tick of its own to go round on.
    fn give_it_the_keyboard(cx: &mut Cx, root: &WidgetRef, knob: &WidgetRef) {
        let at = middle(cx, knob);
        cx.fingers.first_mouse_button = Some((MouseButton::PRIMARY, WINDOW));
        root.handle_event(cx, &press(at, 0.0), &mut Scope::empty());
        cx.action(Tick);
        cx.handle_actions();
        root.handle_event(cx, &release(at, 0.1), &mut Scope::empty());
        cx.handle_actions();
        cx.fingers.first_mouse_button = None;
        assert!(
            cx.has_key_focus(face_of(knob)),
            "the press left the keyboard elsewhere, so the keys below reach nothing"
        );
    }

    /// Up is more, the whole range in 150 points, and the gesture says
    /// `Changed` while it runs and `Ended` once when it is let go. The three
    /// points of slop are travel the value never sees.
    #[test]
    fn a_press_and_a_pull_up_raises_the_value_and_then_commits() {
        let mut cx = Cx::new(Box::new(|_, _| {}));
        let (root, cell) = start(&mut cx);
        let at = middle(&cx, &cell);
        cx.fingers.first_mouse_button = Some((MouseButton::PRIMARY, WINDOW));
        let actions = send(&mut cx, &root, &press(at, 0.0));
        assert!(said(&actions, &cell).is_empty(), "a press turns nothing: {:?}", said(&actions, &cell));

        let up = dvec2(at.x, at.y - KNOB_DRAG_SLOP - 30.0);
        let actions = send(&mut cx, &root, &moved_with(up, 0.1, false));
        assert_eq!(changed(&actions, &cell), Some(70.0), "thirty points up is a fifth of the range");
        assert_eq!(commits(&actions, &cell), 0, "the hand is still on it");

        let down = dvec2(at.x, up.y + 15.0);
        let actions = send(&mut cx, &root, &moved_with(down, 0.2, false));
        assert_eq!(changed(&actions, &cell), Some(60.0), "and down is less");

        let actions = send(&mut cx, &root, &release(down, 0.3));
        assert_eq!(said(&actions, &cell), vec!["Ended(60.0)".to_string()]);
        assert_eq!(cell.as_fab_knob().value(), 60.0);
        cx.fingers.first_mouse_button = None;
    }

    /// Shift is a tenth of the speed -- and it can come and go in the middle
    /// of a drag without the value jumping to where the other rate would have
    /// had it, because the drag is summed move by move.
    #[test]
    fn shift_is_a_finer_drag_and_can_change_its_mind() {
        let mut cx = Cx::new(Box::new(|_, _| {}));
        let (root, cell) = start(&mut cx);
        let at = middle(&cx, &cell);
        cx.fingers.first_mouse_button = Some((MouseButton::PRIMARY, WINDOW));
        send(&mut cx, &root, &press(at, 0.0));
        let a = dvec2(at.x, at.y - KNOB_DRAG_SLOP - 30.0);
        let actions = send(&mut cx, &root, &moved_with(a, 0.1, true));
        assert_eq!(
            changed(&actions, &cell),
            Some(52.0),
            "thirty points with Shift down is a tenth of thirty points without"
        );
        // Shift let go: the next thirty points are worth the full twenty,
        // from where the fine drag left off.
        let b = dvec2(at.x, a.y - 30.0);
        let actions = send(&mut cx, &root, &moved_with(b, 0.2, false));
        assert_eq!(changed(&actions, &cell), Some(72.0));
        // A fine drag arrives a point at a time, each worth a fifteenth of
        // the detent. Summed before the detent they add up; rounded on every
        // move they would each round back to where they started.
        for point in 1..=45 {
            send(&mut cx, &root, &moved_with(dvec2(at.x, b.y - point as f64), 0.2, true));
        }
        assert_eq!(cell.as_fab_knob().value(), 75.0, "the detent swallowed a fine drag");
        let b = dvec2(at.x, b.y - 45.0);
        send(&mut cx, &root, &release(b, 0.3));
        cx.fingers.first_mouse_button = None;
    }

    /// Sideways is nothing. A hand pulling up wanders, and a drag that
    /// counted the wander would be a slightly different drag every time.
    #[test]
    fn sideways_travel_turns_nothing() {
        let mut cx = Cx::new(Box::new(|_, _| {}));
        let (root, cell) = start(&mut cx);
        let at = middle(&cx, &cell);
        cx.fingers.first_mouse_button = Some((MouseButton::PRIMARY, WINDOW));
        send(&mut cx, &root, &press(at, 0.0));
        let actions = send(&mut cx, &root, &moved_with(dvec2(at.x + 200.0, at.y), 0.1, false));
        assert!(said(&actions, &cell).is_empty(), "{:?}", said(&actions, &cell));
        let actions = send(&mut cx, &root, &moved_with(dvec2(at.x - 200.0, at.y + 1.0), 0.2, false));
        assert!(said(&actions, &cell).is_empty(), "{:?}", said(&actions, &cell));
        // ...and a press that moved nothing has nothing to commit.
        let actions = send(&mut cx, &root, &release(dvec2(at.x - 200.0, at.y + 1.0), 0.3));
        assert!(said(&actions, &cell).is_empty(), "{:?}", said(&actions, &cell));
        assert_eq!(cell.as_fab_knob().value(), 50.0);
        cx.fingers.first_mouse_button = None;
    }

    /// A careless click does not nudge the value it was only meant to select:
    /// under the slop nothing moves, and nothing is committed.
    #[test]
    fn a_wobble_under_the_slop_moves_nothing() {
        let mut cx = Cx::new(Box::new(|_, _| {}));
        let (root, cell) = start(&mut cx);
        let at = middle(&cx, &cell);
        cx.fingers.first_mouse_button = Some((MouseButton::PRIMARY, WINDOW));
        send(&mut cx, &root, &press(at, 0.0));
        // Two and a half points, written out: measured against the constant
        // this would follow it down to nothing and go on passing. Without a
        // slop they are a part and two thirds, which the detent calls two.
        let wobble = dvec2(at.x + 1.0, at.y - 2.5);
        let actions = send(&mut cx, &root, &moved_with(wobble, 0.05, false));
        assert!(said(&actions, &cell).is_empty(), "{:?}", said(&actions, &cell));
        let actions = send(&mut cx, &root, &release(wobble, 0.1));
        assert!(said(&actions, &cell).is_empty(), "{:?}", said(&actions, &cell));
        assert_eq!(cell.as_fab_knob().value(), 50.0);
        cx.fingers.first_mouse_button = None;
    }

    /// Past the stop is the stop, under the hand as under the host.
    #[test]
    fn the_value_clamps_at_both_ends() {
        let mut cx = Cx::new(Box::new(|_, _| {}));
        let (root, cell) = start(&mut cx);
        let at = middle(&cx, &cell);
        cx.fingers.first_mouse_button = Some((MouseButton::PRIMARY, WINDOW));
        send(&mut cx, &root, &press(at, 0.0));
        let actions = send(&mut cx, &root, &moved_with(dvec2(at.x, at.y - 4000.0), 0.1, false));
        assert_eq!(changed(&actions, &cell), Some(100.0));
        // The overshoot is not wound up: the first points of the way back
        // already count.
        let actions = send(&mut cx, &root, &moved_with(dvec2(at.x, at.y - 3985.0), 0.15, false));
        assert_eq!(changed(&actions, &cell), Some(90.0), "the pointer had a mile to unwind first");
        let actions = send(&mut cx, &root, &moved_with(dvec2(at.x, at.y + 4000.0), 0.2, false));
        assert_eq!(changed(&actions, &cell), Some(0.0));
        let actions = send(&mut cx, &root, &release(dvec2(at.x, at.y + 4000.0), 0.3));
        assert_eq!(ended(&actions, &cell), Some(0.0));
        cx.fingers.first_mouse_button = None;

        let knob = cell.as_fab_knob();
        knob.set_value(&mut cx, 120.0);
        assert_eq!(knob.value(), 100.0);
        knob.set_value(&mut cx, -3.0);
        assert_eq!(knob.value(), 0.0);
    }

    /// What a host SETS is what the knob holds -- the detent is the hand's --
    /// and setting it says nothing to anybody. A panel that pushed a hundred
    /// weights into a matrix and heard a hundred `Changed` back would install
    /// the mix it had just installed.
    #[test]
    fn a_value_set_from_outside_is_held_exactly_and_emits_nothing() {
        let mut cx = Cx::new(Box::new(|_, _| {}));
        let (_root, cell) = start(&mut cx);
        let knob = cell.as_fab_knob();
        let actions = cx.capture_actions(|cx| {
            knob.set_value(cx, 37.5);
        });
        assert!(actions.is_empty(), "set_value spoke");
        assert_eq!(knob.value(), 37.5, "the detent rounded a number nobody dragged");
        assert_eq!(knob.readout(), 37.5);

        let actions = cx.capture_actions(|cx| {
            knob.set_value_and_readout(cx, 85.5, 81.0);
        });
        assert!(actions.is_empty(), "set_value_and_readout spoke");
        assert_eq!(knob.value(), 85.5, "the weight the knob stands for");
        assert_eq!(knob.readout(), 81.0, "the share the column prints");
        // A plain value takes the readout back with it.
        knob.set_value(&mut cx, 40.0);
        assert_eq!(knob.readout(), 40.0);
    }

    /// A double click is the reset: nought, said as a change, a reset and a
    /// commit, in that order and on the second PRESS. The first press of the
    /// pair turned nothing and so committed nothing.
    #[test]
    fn a_double_click_resets_the_knob_and_commits_the_nought() {
        let mut cx = Cx::new(Box::new(|_, _| {}));
        let (root, cell) = start(&mut cx);
        let at = middle(&cx, &cell);
        cx.fingers.first_mouse_button = Some((MouseButton::PRIMARY, WINDOW));
        let actions = send(&mut cx, &root, &press(at, 1.0));
        assert!(said(&actions, &cell).is_empty());
        let actions = send(&mut cx, &root, &release(at, 1.05));
        assert!(said(&actions, &cell).is_empty(), "the first click of the pair spoke: {:?}", said(&actions, &cell));

        let actions = send(&mut cx, &root, &press(at, 1.2));
        assert_eq!(
            said(&actions, &cell),
            vec!["Changed(0.0)".to_string(), "Reset".to_string(), "Ended(0.0)".to_string()]
        );
        assert!(cell.as_fab_knob().was_reset(&actions));
        assert_eq!(cell.as_fab_knob().value(), 0.0);
        // The second press is a command, not a grip: it turns nothing.
        let actions = send(&mut cx, &root, &moved_with(dvec2(at.x, at.y - 60.0), 1.25, false));
        assert!(said(&actions, &cell).is_empty(), "{:?}", said(&actions, &cell));
        let actions = send(&mut cx, &root, &release(at, 1.3));
        assert!(said(&actions, &cell).is_empty(), "{:?}", said(&actions, &cell));
        cx.fingers.first_mouse_button = None;
    }

    /// Two clicks that are NOT a double click: too far apart in time, and a
    /// drag followed at once by a press.
    #[test]
    fn two_separate_presses_are_not_a_reset() {
        let mut cx = Cx::new(Box::new(|_, _| {}));
        let (root, cell) = start(&mut cx);
        let at = middle(&cx, &cell);
        cx.fingers.first_mouse_button = Some((MouseButton::PRIMARY, WINDOW));
        send(&mut cx, &root, &press(at, 1.0));
        send(&mut cx, &root, &release(at, 1.05));
        let actions = send(&mut cx, &root, &press(at, 2.0));
        assert!(!cell.as_fab_knob().was_reset(&actions), "a slow second click reset the knob");
        // That press becomes a drag that comes back to where it began...
        send(&mut cx, &root, &moved_with(dvec2(at.x, at.y - 40.0), 2.05, false));
        send(&mut cx, &root, &moved_with(at, 2.1, false));
        send(&mut cx, &root, &release(at, 2.15));
        // ...and a press straight after it, on the same spot, is a new grip.
        let actions = send(&mut cx, &root, &press(at, 2.2));
        assert!(!cell.as_fab_knob().was_reset(&actions), "a drag counted as half a double click");
        send(&mut cx, &root, &release(at, 2.25));
        cx.fingers.first_mouse_button = None;
    }

    /// One press, one owner, and nothing else reacts meanwhile: the drag
    /// leaves the cell on its way up -- on a 28 point knob every drag does --
    /// and the knob it passes over neither lights up nor moves.
    #[test]
    fn the_knob_holds_its_press_and_the_neighbour_stays_out_of_it() {
        let mut cx = Cx::new(Box::new(|_, _| {}));
        let (root, cell) = start(&mut cx);
        let other = root.widget(&cx, ids!(other));
        let at = middle(&cx, &cell);
        let over_there = middle(&cx, &other);
        let face = face_of(&cell);
        cx.fingers.first_mouse_button = Some((MouseButton::PRIMARY, WINDOW));
        send(&mut cx, &root, &press(at, 0.0));
        assert!(cx.fingers.is_area_captured(face), "the knob took the pointer");
        assert!(
            !cx.fingers.is_mouse_held_outside(&[face]),
            "and nothing else took a second hold of it"
        );
        let actions = send(&mut cx, &root, &moved_with(dvec2(over_there.x, over_there.y - 30.0), 0.1, false));
        assert!(changed(&actions, &cell).is_some(), "the drag went on turning the knob it began on");
        assert!(said(&actions, &other).is_empty(), "the neighbour answered a drag that is not its own");
        assert!(!other.borrow::<FabKnob>().unwrap().hovered, "the neighbour lit up under a held pointer");
        assert_eq!(other.as_fab_knob().value(), 20.0);
        cx.fingers.first_mouse_button = None;
    }

    /// The arrows, once the knob has the keyboard: one step from where it
    /// stands, Shift the coarse step, and the commit rules of the slider --
    /// a press commits where it lands, a held key commits at its release.
    #[test]
    fn the_arrows_step_a_knob_that_has_the_keyboard() {
        let mut cx = Cx::new(Box::new(|_, _| {}));
        let (root, cell) = start(&mut cx);
        // Before it has the keyboard, the keys are somebody else's.
        let actions = send(&mut cx, &root, &key(KeyCode::ArrowUp, false));
        assert!(said(&actions, &cell).is_empty());

        give_it_the_keyboard(&mut cx, &root, &cell);
        cell.as_fab_knob().set_value(&mut cx, 37.5);
        let actions = send(&mut cx, &root, &key(KeyCode::ArrowUp, false));
        assert_eq!(said(&actions, &cell), vec!["Changed(38.5)".to_string(), "Ended(38.5)".to_string()]);
        let actions = send(&mut cx, &root, &key(KeyCode::ArrowDown, false));
        assert_eq!(changed(&actions, &cell), Some(37.5));
        let actions = send(&mut cx, &root, &key(KeyCode::ArrowRight, true));
        assert_eq!(changed(&actions, &cell), Some(47.5), "Shift is the coarse step");

        let mut run = 0;
        for _ in 0..9 {
            let actions = send(&mut cx, &root, &key_repeat(KeyCode::ArrowUp));
            run += commits(&actions, &cell);
        }
        assert_eq!(run, 0, "a held arrow committed on its repeats");
        let actions = send(&mut cx, &root, &key_up(KeyCode::ArrowUp));
        assert_eq!(said(&actions, &cell), vec!["Ended(56.5)".to_string()]);

        let actions = send(&mut cx, &root, &key(KeyCode::Home, false));
        assert_eq!(ended(&actions, &cell), Some(0.0));
        let actions = send(&mut cx, &root, &key(KeyCode::End, false));
        assert_eq!(ended(&actions, &cell), Some(100.0));
    }

    /// The wheel steps the knob that has the keyboard and leaves every other
    /// knob alone, so a wheel crossing the matrix scrolls the panel instead
    /// of walking a column of weights. A spin is one gesture: every notch
    /// says `Changed`, and the one commit waits for the wheel to go still.
    #[test]
    fn the_wheel_steps_the_knob_that_has_the_keyboard_and_commits_once() {
        let mut cx = Cx::new(Box::new(|_, _| {}));
        let (root, cell) = start(&mut cx);
        let at = middle(&cx, &cell);

        // Merely under the pointer: not this knob's wheel, and said so by
        // leaving the event unspent for the scroller around it.
        let event = wheel(at, 1.0, false);
        let actions = send(&mut cx, &root, &event);
        assert!(said(&actions, &cell).is_empty(), "{:?}", said(&actions, &cell));
        assert!(!event.scroll_handled(Vec2Index::Y), "an idle knob ate the panel's wheel");

        give_it_the_keyboard(&mut cx, &root, &cell);
        let mut heard = Vec::new();
        for _ in 0..3 {
            let event = wheel(at, 1.0, false);
            let actions = send(&mut cx, &root, &event);
            heard.extend(said(&actions, &cell));
            assert!(event.scroll_handled(Vec2Index::Y), "the knob's wheel also scrolled the panel");
        }
        assert_eq!(heard, vec!["Changed(51.0)", "Changed(52.0)", "Changed(53.0)"]);
        let actions = send(&mut cx, &root, &wheel(at, -1.0, true));
        assert_eq!(said(&actions, &cell), vec!["Changed(43.0)".to_string()], "Shift is the coarse step");

        // A trackpad's notch arrives in pieces, and the pieces add up.
        let mut heard = Vec::new();
        for _ in 0..4 {
            let actions = send(&mut cx, &root, &wheel(at, 0.25, false));
            heard.extend(said(&actions, &cell));
        }
        assert_eq!(heard, vec!["Changed(44.0)"]);

        // The wheel goes still, and the spin is over.
        let timer = cell.borrow::<FabKnob>().unwrap().wheel_timer;
        assert_ne!(timer.0, 0, "nothing is waiting to commit the spin");
        let settle = Event::Timer(TimerEvent {
            time: None,
            timer_id: timer.0,
        });
        let actions = send(&mut cx, &root, &settle);
        assert_eq!(said(&actions, &cell), vec!["Ended(44.0)".to_string()]);
        // ...once. What is owed is owed once.
        let actions = send(&mut cx, &root, &settle);
        assert!(said(&actions, &cell).is_empty());

        // At the stop the wheel still belongs to the knob, and says nothing.
        cell.as_fab_knob().set_value(&mut cx, 100.0);
        let event = wheel(at, 1.0, false);
        let actions = send(&mut cx, &root, &event);
        assert!(said(&actions, &cell).is_empty());
        assert!(event.scroll_handled(Vec2Index::Y));
    }

    /// A spin the keyboard walks out on still commits, as a run of arrows
    /// does: the timer it was waiting on belongs to a knob that no longer
    /// has the wheel.
    #[test]
    fn a_spin_the_keyboard_walks_out_on_still_commits() {
        let mut cx = Cx::new(Box::new(|_, _| {}));
        let (root, cell) = start(&mut cx);
        let at = middle(&cx, &cell);
        give_it_the_keyboard(&mut cx, &root, &cell);
        send(&mut cx, &root, &wheel(at, 2.0, false));
        let actions = send(
            &mut cx,
            &root,
            &Event::KeyFocus(KeyFocusEvent {
                prev: face_of(&cell),
                focus: Area::Empty,
            }),
        );
        assert_eq!(said(&actions, &cell), vec!["Ended(52.0)".to_string()]);
        let actions = send(&mut cx, &root, &Event::WindowLostFocus(WINDOW));
        assert!(said(&actions, &cell).is_empty(), "the spin was paid for twice");
    }

    /// `wheel_on_hover` is the host saying its knobs stand somewhere that
    /// does not scroll.
    #[test]
    fn a_knob_told_to_takes_the_wheel_it_is_merely_under() {
        let mut cx = Cx::new(Box::new(|_, _| {}));
        let (root, cell) = start(&mut cx);
        cell.borrow_mut::<FabKnob>().unwrap().wheel_on_hover = true;
        let event = wheel(middle(&cx, &cell), 1.0, false);
        let actions = send(&mut cx, &root, &event);
        assert_eq!(changed(&actions, &cell), Some(51.0));
        assert!(event.scroll_handled(Vec2Index::Y));
    }

    /// Switched off, a knob answers nothing -- and switching it off in the
    /// middle of a drag puts back what the press found.
    #[test]
    fn a_knob_switched_off_lets_go_and_goes_quiet() {
        let mut cx = Cx::new(Box::new(|_, _| {}));
        let (root, cell) = start(&mut cx);
        let at = middle(&cx, &cell);
        cx.fingers.first_mouse_button = Some((MouseButton::PRIMARY, WINDOW));
        send(&mut cx, &root, &press(at, 0.0));
        send(&mut cx, &root, &moved_with(dvec2(at.x, at.y - 63.0), 0.1, false));
        assert_eq!(cell.as_fab_knob().value(), 90.0);
        let actions = cx.capture_actions(|cx| cell.as_fab_knob().set_enabled(cx, false));
        assert_eq!(
            cell.as_fab_knob().changed(&actions),
            Some(50.0),
            "the drag that was cut short left its value behind"
        );
        let actions = send(&mut cx, &root, &moved_with(dvec2(at.x, at.y - 120.0), 0.2, false));
        assert!(said(&actions, &cell).is_empty());
        assert_eq!(cell.as_fab_knob().value(), 50.0);
        cx.fingers.first_mouse_button = None;
    }

    /// A drag that is let go somewhere else leaves the knob unlit. No
    /// hover-out follows a release off the area, so a knob that waited for
    /// one stayed lit until the pointer happened to cross it again -- seen in
    /// a running window, on the first drag that was ever tried on one.
    #[test]
    fn a_drag_let_go_off_the_knob_does_not_leave_it_lit() {
        let mut cx = Cx::new(Box::new(|_, _| {}));
        let (root, cell) = start(&mut cx);
        let at = middle(&cx, &cell);
        send(&mut cx, &root, &moved_with(at, 0.0, false));
        assert!(cell.borrow::<FabKnob>().unwrap().hovered, "the pointer arrived and nothing lit");
        cx.fingers.first_mouse_button = Some((MouseButton::PRIMARY, WINDOW));
        send(&mut cx, &root, &press(at, 0.1));
        let away = dvec2(at.x, at.y - 200.0);
        send(&mut cx, &root, &moved_with(away, 0.2, false));
        send(&mut cx, &root, &release(away, 0.3));
        cx.fingers.first_mouse_button = None;
        assert!(!cell.borrow::<FabKnob>().unwrap().hovered, "the knob is still lit with the pointer gone");

        // ...and one let go ON the knob keeps the light the pointer earns it.
        send(&mut cx, &root, &moved_with(at, 0.4, false));
        cx.fingers.first_mouse_button = Some((MouseButton::PRIMARY, WINDOW));
        send(&mut cx, &root, &press(at, 1.5));
        send(&mut cx, &root, &release(at, 1.6));
        cx.fingers.first_mouse_button = None;
        assert!(cell.borrow::<FabKnob>().unwrap().hovered);
    }

    /// The box is whatever the cell says, the face is what the words leave of
    /// it, and a knob with no name has no row for one.
    #[test]
    fn the_knob_takes_the_size_of_its_cell_and_drops_the_rows_it_has_no_words_for() {
        let mut cx = Cx::new(Box::new(|_, _| {}));
        let (root, cell) = start(&mut cx);
        let small = root.widget(&cx, ids!(small));
        let named = root.widget(&cx, ids!(named));
        assert_eq!(face_of(&cell).rect(&cx).size, dvec2(44.0, 64.0), "the default box");
        assert_eq!(face_of(&small).rect(&cx).size, dvec2(28.0, 28.0), "the cell's box");
        assert_eq!(cell.borrow::<FabKnob>().unwrap().text_rows(), (0.0, 12.0));
        assert_eq!(small.borrow::<FabKnob>().unwrap().text_rows(), (0.0, 0.0));
        assert_eq!(named.borrow::<FabKnob>().unwrap().text_rows(), (12.0, 12.0));
        // What the shader was handed is what the layout measured with.
        let inner = named.borrow::<FabKnob>().unwrap();
        assert_eq!((inner.draw_bg.label_px, inner.draw_bg.readout_px), (12.0, 12.0));
        // Nought is drawn as off, and the travel is what says so.
        assert_eq!(inner.draw_bg.travel, 0.0);
        assert_eq!(cell.borrow::<FabKnob>().unwrap().draw_bg.travel, 0.5);
    }
}

/// The header, drawn. What the arithmetic says about where a name goes is
/// only worth something if that is where the widget actually put it, and a
/// name written in by a host has to reach the screen.
#[cfg(test)]
mod fab_diagonal_label_draw {
    #![allow(dead_code)]
    use super::fab_slider_gestures::Target;
    use super::*;
    use crate::makepad_draw::cx_draw::CxDraw;

    const SIZE: Vec2d = Vec2d { x: 800.0, y: 600.0 };

    /// A header row the way a matrix wants one: columns at the pitch a 280
    /// wide sidebar comes down to, leaning both ways, and one with no name
    /// behind it.
    fn scene(cx: &mut Cx) -> WidgetRef {
        cx.with_vm(|vm| {
            let value = crate::script_eval!(vm, {
                use mod.prelude.widgets.*
                use mod.widgets.*
                View{
                    width: Fill
                    height: Fill
                    flow: Down
                    View{
                        width: Fit
                        height: Fit
                        flow: Right
                        clip_x: false
                        clip_y: false
                        first := FabDiagonalLabel{width: 26. text: "Windows 2000"}
                        second := FabDiagonalLabel{width: 26. text: "Dark"}
                        rising := FabDiagonalLabel{width: 26. text: "Windows 2000" lean: DiagonalLean.Rise}
                        blank := FabDiagonalLabel{width: 26.}
                    }
                }
            });
            WidgetRef::script_from_value(vm, value)
        })
    }

    fn start(cx: &mut Cx) -> (WidgetRef, Target) {
        cx.init_cx_os();
        cx.with_vm(crate::script_mod);
        let root = scene(cx);
        let mut target = Target::new(cx);
        target.draw(cx, &root);
        (root, target)
    }

    fn run_of(widget: &WidgetRef) -> DiagonalRun {
        widget
            .borrow::<FabDiagonalLabel>()
            .expect("it is a FabDiagonalLabel")
            .last_run()
            .expect("it drew a name")
    }

    fn box_of(widget: &WidgetRef, cx: &Cx) -> Rect {
        let area = widget
            .borrow::<FabDiagonalLabel>()
            .expect("it is a FabDiagonalLabel")
            .area;
        area.rect(cx)
    }

    /// `row_height_for` shapes the name, and shaping wants a live `Cx2d`
    /// even though nothing is drawn with it.
    fn measure(cx: &mut Cx, widget: &WidgetRef, text: &str) -> f64 {
        let pass = DrawPass::new(cx);
        let mut draw_list = DrawList2d::new(cx);
        pass.set_size(cx, SIZE);
        let event = DrawEvent::default();
        let mut draw = CxDraw::new(cx, &event);
        let mut cx2d = Cx2d::new(&mut draw);
        cx2d.begin_pass(&pass, None);
        draw_list.begin_always(&mut cx2d);
        cx2d.begin_root_turtle(SIZE, Layout::flow_down());
        let height = widget
            .borrow::<FabDiagonalLabel>()
            .expect("it is a FabDiagonalLabel")
            .row_height_for(&mut cx2d, text);
        cx2d.end_pass_sized_turtle();
        draw_list.end(&mut cx2d);
        cx2d.end_pass(&pass);
        height
    }

    /// The drawn name lands where the pure placement says it does, its ink
    /// leaves its own 26 point box, and the box it claimed is still only 26
    /// wide — which together is the whole claim: the widget takes a column
    /// and paints across the panel.
    #[test]
    fn the_drawn_name_stands_on_its_own_column_and_leaves_its_box() {
        let mut cx = Cx::new(Box::new(|_, _| {}));
        let (root, _target) = start(&mut cx);
        let first = root.widget(&cx, ids!(first));
        let box_ = box_of(&first, &cx);
        assert_eq!(box_.size.x, 26.0, "the claimed box is one column");
        let run = run_of(&first);
        // The baseline ends on the middle of the claimed box's bottom edge.
        let centre = dvec2(box_.pos.x + box_.size.x * 0.5, box_.pos.y + box_.size.y);
        assert!((run.end - centre).length() < 1e-6, "{:?} is not {centre:?}", run.end);
        assert!(run.angle > 0.0, "a fall turns clockwise on screen");
        // The ink is wider than the column, which is the reason to have it.
        assert!(run.bounds.size.x > 26.0, "the name stayed inside its box");
        assert!(run.bounds.pos.x < box_.pos.x, "the name did not hang left");
        // ...and no taller than the row the template ships: the default
        // height holds the longest theme name the library carries, which is
        // the name in this scene.
        assert!(
            run.bounds.size.y <= box_.size.y,
            "the template's {} is too short for {}",
            box_.size.y,
            run.bounds.size.y
        );
    }

    /// The rise leans the other way: the same anchor, the opposite side.
    #[test]
    fn a_rising_name_starts_where_a_falling_one_ends() {
        let mut cx = Cx::new(Box::new(|_, _| {}));
        let (root, _target) = start(&mut cx);
        let rising = root.widget(&cx, ids!(rising));
        let box_ = box_of(&rising, &cx);
        let run = run_of(&rising);
        let centre = dvec2(box_.pos.x + box_.size.x * 0.5, box_.pos.y + box_.size.y);
        assert!((run.start - centre).length() < 1e-6);
        assert!(run.angle < 0.0, "a rise turns anticlockwise on screen");
        assert!(
            run.bounds.pos.x + run.bounds.size.x > box_.pos.x + box_.size.x,
            "the name did not hang right"
        );
    }

    /// A shorter name takes less room, and one with nothing in it draws
    /// nothing — a matrix leaves columns it has nothing to name blank, and
    /// they still have to keep their place in the row.
    #[test]
    fn a_shorter_name_takes_less_room_and_an_empty_one_takes_none() {
        let mut cx = Cx::new(Box::new(|_, _| {}));
        let (root, _target) = start(&mut cx);
        let long = run_of(&root.widget(&cx, ids!(first)));
        let short = run_of(&root.widget(&cx, ids!(second)));
        assert!(
            short.bounds.size.y < long.bounds.size.y,
            "a short name wants less height: {} vs {}",
            short.bounds.size.y,
            long.bounds.size.y
        );
        let blank = root.widget(&cx, ids!(blank));
        assert!(
            blank
                .borrow::<FabDiagonalLabel>()
                .unwrap()
                .last_run()
                .is_none(),
            "an empty header drew something"
        );
        assert_eq!(box_of(&blank, &cx).size.x, 26.0, "it still claimed its column");
    }

    /// The name a host writes in is the name that is drawn, and asking for
    /// it asks for the frame that shows it. Writing the same name again is
    /// silent: a matrix fills every header on every draw, and a setter
    /// that dirtied the list each time would redraw the panel forever.
    #[test]
    fn a_written_name_is_held_and_asks_for_the_frame_that_shows_it() {
        let mut cx = Cx::new(Box::new(|_, _| {}));
        let (root, mut target) = start(&mut cx);
        let first = root.widget(&cx, ids!(first));
        assert_eq!(
            first.borrow::<FabDiagonalLabel>().unwrap().text(),
            "Windows 2000"
        );
        let was = run_of(&first).bounds.size.y;

        cx.new_draw_event = Default::default();
        first
            .borrow_mut::<FabDiagonalLabel>()
            .unwrap()
            .set_text(&mut cx, "Windows 2000");
        assert!(
            !cx.new_draw_event.will_redraw(),
            "the same name asked for a frame"
        );

        first
            .borrow_mut::<FabDiagonalLabel>()
            .unwrap()
            .set_text(&mut cx, "NeXTSTEP");
        assert!(cx.new_draw_event.will_redraw(), "a new name asked for no frame");
        assert_eq!(first.borrow::<FabDiagonalLabel>().unwrap().text(), "NeXTSTEP");

        // ...and the next frame draws it, shorter than what it replaced.
        target.draw(&mut cx, &root);
        assert!(run_of(&first).bounds.size.y < was);
    }

    /// The row height a host fixes its header at is the height the longest
    /// name it will write actually needs.
    #[test]
    fn the_widget_can_say_how_tall_the_header_row_has_to_be() {
        let mut cx = Cx::new(Box::new(|_, _| {}));
        let (root, _target) = start(&mut cx);
        let first = root.widget(&cx, ids!(first));
        let drawn = run_of(&first).bounds.size.y;
        let asked = measure(&mut cx, &first, "Windows 2000");
        assert!((asked - drawn).abs() < 1e-6, "asked {asked}, drew {drawn}");
        // A longer name than any theme carries wants a taller row.
        assert!(measure(&mut cx, &first, "Windows 2000 dark") > asked);
    }
}
