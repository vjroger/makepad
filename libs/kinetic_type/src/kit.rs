//! Reading a kit: one `Kinetic{...}` Splash document, evaluated whole in
//! the VM ([`load`]).
//!
//! ```text
//! fn lift(g) { ... }                 // top-level fns: kernel helpers (CPU)
//! Kinetic{
//!     name: "Wave Line"  text: "KINETIC TEXT"  case: @upper  font: @bold
//!     size: 1.0  depth: 0.35  bevel: 0.03  bevel_type: @round  material: @metal
//!     layout: @line | @cloud  wrap: 12  align: @center  line_gap: 1.2  tracking: 0.0
//!     copies: 1  alphabet: "#%&"  cells: {res: 12 layers: 2 fill: @block}
//!     colors: {bg: #05060d a: #ffc84a b: #2a1450 c: #49e6ff}
//!     dials: {swing: 0.0 drive: 0.0 split: 0.5}  // p1..p10 in order, 0..1; past the tenth: held (below)
//!     camera: {fov: 50 dist: 9 height: 0}   // dist, height in cap heights
//!     ground: {y: -0.7 size: 14}         // a floor plane (its look: `floor: fn() -> vec4`)
//!     picture: {width: 1024 height: 256 view: 2}   // glyphs into a picture; backdrop = the screen
//!     grid: {u: 96 v: 32 copies: 1}      // a grid shaped by the look's `surface: fn(uv)` hook
//!     post: [Glow{threshold: 0.62 strength: 0.9}]
//!     glyph: fn(g, o) { ... }            // the animator: a kernel (CPU)
//!     camera_fn: fn(c) { ... }           // optional camera kernel (CPU); c.share -> self.k_share
//!     curve: {points: 256 closed: true up: [0, 1, 0]}  curve_fn: fn(c) { c.pos = ... }   // a path at c.u, even by arc length
//!     dying: 0.8                          // a shorter text's surplus stays 0.8 s (g.dying = 1)
//!     screen: true                        // a picture kit's glyphs also draw on the screen (below)
//!     forms: [{name: "WALL"}, {name: "DISC" text: "SPIN" font: @bold wrap: 12 post: []}]   // per-form settings (below)
//!     form_dial: @form                    // the dial that picks the form (default: the first)
//!     look: fn() -> vec4 { ... }         // every other fn: the glyph shader
//! }
//! ```
//!
//! FORMS. A kit that folds several kits into one (a family, its first dial
//! FORM) lists them in `forms: [{...}, {...}]`, each form an object of the
//! kit's own settings that this form sets differently: `text`, `case`,
//! `font`, `weight`, `axes`, `size`, `wrap`, `line_gap`, `tracking`,
//! `align`, `layout`, `copies`, `picture`, `colors`, `camera`, `screen`,
//! `post` and every other value setting above (not `dials` or fns: they
//! are the kit's). The form dial (`form_dial: @name`, else the first
//! dial) split into equal parts picks the form: with three forms 0..1/3 is
//! the first. A form starts from the kit's settings and replaces what it
//! names, as if that kit had been written with them, so the form looks
//! exactly as the kit it came from:
//! - `font`, `weight` and `axes` go together: a form naming any of them
//!   names its whole font (`font: @inter weight: 900` takes no `axes`
//!   from the kit);
//! - `colors` change key by key (`colors: {b: #x3c3c3c}` keeps bg, a, c);
//! - any other `{...}` setting (`picture`, `camera`, `cells`, `cycle`, ...)
//!   replaces the kit's whole (`camera: {fov: 30}` takes no `dist` from
//!   the kit), and `nil` removes one (`wrap: nil`, `picture: nil`: this
//!   form draws its glyphs on the screen like a kit without a picture);
//! - `post` replaces the kit's whole list: `post: [Glow{threshold: 0.6
//!   strength: 0.25}]` its own glow, `post: []` none; a form without
//!   `post` keeps the kit's;
//! - `name` labels the form; the kit keeps its own.
//! Turning the form dial lays the text out again with the form's settings
//! (fresh records, as if the kit had just loaded with this text; the
//! text's arrival time stays). What a host sets still wins over a form's
//! defaults: the host's words over the form's `text`, its font, weight and
//! axes over the form's, its palette over the form's colours. The kernels
//! and the shaders are the kit's, shared by every form: they branch on
//! the form dial (`if form < 0.333 { ... }`). Each form costs what its
//! kit did: the records (its text times its `copies`), the shapes, the
//! grid and the picture target are the shown form's own (the target takes
//! the form's `picture` size, and shrinks to 16 x 16 while a form with
//! `picture: nil` shows).
//!
//! SCREEN. A `picture` kit draws its glyphs flat into the picture, and the
//! backdrop is the screen. With `screen: true` (per form too) the glyphs
//! also draw on the screen, after the backdrop, in the frame's own camera
//! (the default framing or `camera_fn`). The glyph shaders tell the two
//! passes apart by `self.on_screen` (a uniform, readable in `deform` and
//! in `look`): 0 while drawing into the picture, 1 on the screen (always
//! 1 in a kit without a picture). Each does what its pass needs: `look`
//! discards in the pass a form does not use (`if self.on_screen < 0.5 {
//! discard() }`); `deform` is best left whole (an early return there
//! compiles the vertex stage differently, which a strong lens shows).
//! Without `screen` a picture kit's glyphs draw into the picture only, as
//! before. A form that draws on the screen only is cheaper as `picture:
//! nil`: no picture pass at all, its glyphs on the screen as in a kit
//! without a picture (`self.on_screen` 1); `screen: true` is for a form
//! that draws both.
//!
//! DIALS. The first ten a kit declares are its dials, p1..p10: a host
//! turns them ([`crate::view::KineticFrame::dials`]), the kernels read
//! them as `p1..p10` and by name, the shaders as `self.p` (p1..p4),
//! `self.p_b` (p5..p8), `self.p_c.xy` (p9, p10) and `self.<dial>()`. Any
//! past the tenth are held ([`KitValues::held`]): no host turns them, each
//! keeps its default as a constant, by name in the kernels and the
//! shaders, the eleventh and twelfth also in `self.p_c.zw`.
//!
//! Stock shader helpers besides the look's lighting: `self.fwidth(v)` (both
//! draws), and on the backdrop `self.eye()`, `self.ray(uv)`,
//! `self.plane_hit(n, d)` (this pixel's ray on the plane dot(n, p) = d) and
//! `self.text_plane()` (the z = 0 point under the pixel).
//!
//! `glyph`, `camera_fn` and `curve_fn` are kernels: crate::kernel compiles
//! them from their fn objects (makepad-script-compute's vm_kernel), with
//! whatever top-level fns they reach. Every other `name: fn` field is a
//! member of the kit's draw, derived from `DrawKineticGlyph` (`look`,
//! `deform`, `floor`, helpers) or, for `backdrop`, `DrawKineticBackdrop`
//! (view.rs). The rest is read as values ([`KitValues`]).

/// What every kit is evaluated with, on its first line (so its own lines
/// keep their numbers).
const KIT_USES: &str = "use mod.std.* use mod.pod.* use mod.math.* use mod.shader.* use mod.draw use mod.shared.* use mod.kin.* ";

/// The host's side of every kit: the kernel entries, `band`, the dial
/// accessors (kit.splash).
const KIT_GLUE: &str = include_str!("kit.splash");

/// The fields of a kit that are kernels, not draw members.
pub const KERNEL_FIELDS: &[&str] = &["glyph", "camera_fn", "curve_fn"];

/// The dials a kit has: `p1..p10`, the first ten it declares, in the order
/// written (a host carries every one, [`crate::view::KineticFrame::dials`]).
/// Those past the tenth are held at their defaults ([`KitValues::held`]).
pub const MAX_DIALS: usize = 10;

/// A kit evaluated: its object (kept alive while the host builds from it)
/// and its values.
pub struct Kit {
    pub object: ScriptObjectRef,
    pub values: KitValues,
    /// The shader accessors of the held dials: `<dial>: fn() -> float`
    /// returning its default (None: the kit holds none).
    held: Option<ScriptObjectRef>,
}

impl Kit {
    /// The kit's fn field `name` (a script fn), if it has one.
    pub fn fn_field(&self, vm: &ScriptVm, name: &str) -> Option<ScriptObject> {
        field(vm, self.object.as_object(), name).as_object().filter(|f| vm.bx.heap.as_fn(*f).is_some())
    }

    /// The draw members the kit writes (`name: fn` fields other than the
    /// kernels), in the order written.
    pub fn shader_fns(&self, vm: &ScriptVm) -> Vec<(LiveId, ScriptValue)> {
        fields(vm, self.object.as_object())
            .into_iter()
            .filter(|(k, v)| {
                let name = k.to_string();
                !KERNEL_FIELDS.contains(&name.as_str()) && v.as_object().is_some_and(|f| vm.bx.heap.as_fn(f).is_some())
            })
            .collect()
    }

    /// The shader accessor `self.<dial>()` of each held dial: a fn
    /// returning its default.
    pub fn held_fns(&self, vm: &ScriptVm) -> Vec<(LiveId, ScriptValue)> {
        self.held.as_ref().map_or(Vec::new(), |h| fields(vm, h.as_object()))
    }
}

/// The shader accessors of `held` (a kit's dials past the tenth): one
/// object of `<dial>: fn() -> float { return <default> }`, written as a
/// kit writes its own fns.
fn held_accessors(vm: &mut ScriptVm, held: &[(String, f32)], file: &str) -> Result<Option<ScriptObjectRef>, String> {
    if held.is_empty() {
        return Ok(None);
    }
    let fns: Vec<String> = held
        .iter()
        .map(|(name, d)| {
            let v = if *d < 0.0 { format!("0.0 - {:.9}", -d) } else { format!("{d:.9}") };
            format!("{name}: fn() -> float {{ return {v} }}")
        })
        .collect();
    vm.bx.captured_errors = Some(Vec::new());
    let v = vm.eval_transient(ScriptMod { file: format!("{file} (held dials)"), code: format!("{KIT_USES}Kinetic{{ {} }}", fns.join(" ")), ..Default::default() });
    let errors = vm.take_errors();
    if !errors.is_empty() {
        return Err(errors.join("; "));
    }
    let o = v.as_object().ok_or_else(|| format!("{file}: the held dials' accessors did not evaluate"))?;
    Ok(Some(vm.bx.heap.new_object_ref(o)))
}

/// Evaluate a kit's Splash text (`file` names it in diagnostics) and read
/// its values.
pub fn load(vm: &mut ScriptVm, source: &str, file: &str) -> Result<Kit, String> {
    script_mod(vm);
    vm.bx.captured_errors = Some(Vec::new());
    // A body of its own the VM reclaims once nothing holds the kit.
    let v = vm.eval_transient(ScriptMod { file: file.to_string(), code: format!("{KIT_USES}{source}"), ..Default::default() });
    let errors = vm.take_errors();
    if !errors.is_empty() {
        return Err(errors.join("; "));
    }
    let module = vm.bx.heap.value(vm.bx.heap.modules, LiveId::from_str(KIT_MODULE).into(), NoTrap).as_object();
    let kinetic = module.map(|m| vm.bx.heap.value(m, LiveId::from_str("Kinetic").into(), NoTrap));
    let o = v.as_object().filter(|o| Some(vm.bx.heap.proto(*o)) == kinetic).ok_or_else(|| format!("{file}: a kit is one `Kinetic{{ ... }}` object"))?;
    let object = vm.bx.heap.new_object_ref(o);
    let values = read_values(vm, o)?;
    let held = held_accessors(vm, &values.held, file)?;
    Ok(Kit { object, values, held })
}

/// An object's own fields, in the order written.
fn fields(vm: &ScriptVm, o: ScriptObject) -> Vec<(LiveId, ScriptValue)> {
    let mut out = Vec::new();
    vm.bx.heap.object_data(o).map_iter_ordered(|k, v| {
        if let Some(k) = k.as_id() {
            out.push((k, v));
        }
    });
    out
}

// ---------------------------------------------------------------------------
// The values (read through the document VM)
// ---------------------------------------------------------------------------

use crate::shapes::{CellSpec, ShapeSpec};
use makepad_draw::*;
use makepad_text_mesh::letters::{BevelProfile, FontSource, TextAlign};

/// The kit's type settings, palette, camera, floor and passes.
#[derive(Clone, Debug)]
pub struct KitValues {
    pub name: String,
    pub text: String,
    pub upper: bool,
    pub lower: bool,
    /// The shape settings (its `text` is set per text).
    pub shape: ShapeSpec,
    pub copies: usize,
    /// bg, a, b, c.
    pub colors: [Vec4f; 4],
    /// 0 matte, 1 metal, 2 neon, 3 plastic, 4 glass, 5 holo.
    pub material: f32,
    /// The dials, p1..p10 (the first [`MAX_DIALS`] the kit declares): name
    /// and default.
    pub dials: Vec<(String, f32)>,
    /// The kit's dials past the tenth: no host turns them, each keeps its
    /// default as a constant (by name in the kernels and the shaders, the
    /// eleventh and twelfth also in `self.p_c.zw`).
    pub held: Vec<(String, f32)>,
    pub fov: f32,
    /// Camera distance in cap heights; None = frame the text.
    pub dist: Option<f32>,
    /// Camera height in cap heights; None = level (raised over a floor).
    pub height: Option<f32>,
    pub floor: Option<(Option<f32>, Option<f32>)>,
    /// `picture: {width height view}`: the glyphs draw flat into a picture
    /// of width x height pixels showing `view` cap heights vertically (an
    /// orthographic view), and the backdrop is the frame (a screen).
    pub picture: Option<(u32, u32, f32)>,
    /// `grid: {u v copies}`: a u x v grid the look's `surface(uv)` hook
    /// shapes (a globe, a knot, a ribbon printed with the picture), drawn
    /// `copies` times (`self.attr.x` = the copy).
    pub surface: Option<(u32, u32, u32)>,
    /// `curve: {points: 256}`: the curve_fn sampled at this many points and
    /// resampled evenly by arc length (see crate::curve).
    pub curve_points: Option<u32>,
    /// `curve: {closed: true up: [0, 1, 0]}`: how the curve is framed.
    pub curve_frames: crate::curve::Frames,
    /// `grow` runs 0..1 over this many beats (0: stays 1).
    pub cycle_beats: f32,
    pub pingpong: bool,
    /// `dying: 0.8`: when a new text needs fewer elements than the last,
    /// the surplus stays this many seconds as dying records (`g.dying` 1,
    /// `g.changed_at` the change, `g.from` where it was) so a kit can fly
    /// or fade them out; None: they vanish with the old text.
    pub dying: Option<f32>,
    /// `screen: true`: a picture kit's glyphs also draw on the screen
    /// (`self.on_screen` 1 there, 0 in the picture).
    pub screen: bool,
    pub passes: Vec<makepad_render_graph::PassDecl>,
    pub pass_values: Vec<makepad_render_graph::PassValues>,
    /// `forms: [...]`: each form's settings (the kit's with the form's
    /// over them; their own `forms` empty). Empty: a kit without forms.
    pub forms: Vec<Form>,
    /// The dial that picks the form (`form_dial: @name`; the first).
    pub form_dial: usize,
}

/// One of a kit's `forms` (see the module docs): its label and its
/// settings, whole.
#[derive(Clone, Debug)]
pub struct Form {
    pub name: String,
    pub values: KitValues,
}

/// The settings a form may set (the kit's value settings and post passes,
/// not its dials or fns).
const FORM_KEYS: &[&str] = &[
    "name", "text", "case", "font", "weight", "axes", "size", "depth", "bevel", "bevel_type", "bevel_rings", "detail", "tracking", "line_gap", "wrap", "align", "layout",
    "alphabet", "cells", "colors", "material", "camera", "ground", "picture", "grid", "curve", "cycle", "copies", "dying", "screen", "post",
];

/// Where a setting is read: a form's own field when it has one (even
/// `nil`), else the kit's. A form naming any of `font`, `weight`, `axes`
/// names its whole font.
#[derive(Clone, Copy)]
struct Src {
    kit: ScriptObject,
    form: Option<ScriptObject>,
    form_font: bool,
}

impl Src {
    fn new(vm: &ScriptVm, kit: ScriptObject, form: Option<ScriptObject>) -> Self {
        let form_font = form.is_some_and(|f| ["font", "weight", "axes"].iter().any(|n| has_own(vm, f, n)));
        Self { kit, form, form_font }
    }

    fn get(&self, vm: &ScriptVm, name: &str) -> ScriptValue {
        if let Some(f) = self.form {
            let font_key = matches!(name, "font" | "weight" | "axes");
            if (font_key && self.form_font) || has_own(vm, f, name) {
                return field(vm, f, name);
            }
        }
        field(vm, self.kit, name)
    }
}

/// Whether `o` has the field `name` itself (set to anything, `nil` too).
fn has_own(vm: &ScriptVm, o: ScriptObject, name: &str) -> bool {
    let id = LiveId::from_str(name);
    fields(vm, o).iter().any(|(k, _)| *k == id)
}

/// A Splash list's items (`[a, b]`).
fn list(vm: &ScriptVm, v: ScriptValue) -> Vec<ScriptValue> {
    let h = &vm.bx.heap;
    if let Some(a) = v.as_array() {
        return (0..h.array_len(a)).map(|i| h.array_index(a, i, NoTrap)).collect();
    }
    if let Some(o) = v.as_object() {
        return (0..h.vec_len(o)).map(|i| h.vec_value(o, i, NoTrap)).collect();
    }
    Vec::new()
}

fn field(vm: &ScriptVm, o: ScriptObject, name: &str) -> ScriptValue {
    let v = vm.bx.heap.value(o, LiveId::from_str(name).into(), NoTrap);
    if v.is_err() {
        NIL
    } else {
        v
    }
}

fn text_of(vm: &mut ScriptVm, v: ScriptValue) -> Option<String> {
    if v.is_nil() {
        return None;
    }
    if let Some(id) = v.as_id() {
        return Some(id.to_string());
    }
    if v.is_string_like() {
        return vm.bx.heap.cast_to_owned_string(v, "kinetic kit");
    }
    None
}

fn num(v: ScriptValue) -> Option<f32> {
    v.as_number().map(|n| n as f32).or_else(|| v.as_bool().map(|b| if b { 1.0 } else { 0.0 }))
}

fn color(v: ScriptValue) -> Option<Vec4f> {
    v.as_color().map(Vec4f::from_u32)
}

/// The module kits are read in: `Kinetic` plus the graph's post kits.
pub const KIT_MODULE: &str = "kin";

/// Registers the kit module (once per VM).
pub fn script_mod(vm: &mut ScriptVm) {
    let have = vm.bx.heap.value(vm.bx.heap.modules, LiveId::from_str(KIT_MODULE).into(), NoTrap).as_object().is_some();
    if have {
        return;
    }
    let m = vm.new_module(LiveId::from_str(KIT_MODULE));
    let proto = vm.bx.heap.new_object();
    vm.bx.heap.set_value_def(m, LiveId::from_str("Kinetic").into(), proto.into());
    let types: Vec<(&str, &str)> = makepad_render_graph::kits::KITS.iter().map(|k| (k.name, k.kind)).chain([("Pass", "pass")]).collect();
    makepad_render_graph::script::register_post_types(vm, m, &types);
    for e in makepad_render_graph::script::install_kits(vm, &format!("mod.{KIT_MODULE}"), None) {
        log!("kinetic: graph kits: {e}");
    }
    vm.bx.captured_errors = Some(Vec::new());
    vm.eval(ScriptMod { file: "kinetic_type/kit.splash".into(), code: KIT_GLUE.into(), ..Default::default() });
    for e in vm.take_errors() {
        log!("kinetic: kit.splash: {e}");
    }
}

/// Reads the values of the evaluated kit `o`: its own, then each of its
/// `forms` (the kit's settings with the form's over them).
fn read_values(vm: &mut ScriptVm, o: ScriptObject) -> Result<KitValues, String> {
    let src = Src::new(vm, o, None);
    let mut values = read_layer(vm, src)?;
    let fv = field(vm, o, "forms");
    if fv.is_nil() {
        return Ok(values);
    }
    let items = list(vm, fv);
    if items.is_empty() {
        return Err("forms: a list of the forms' settings, `forms: [{name: \"A\"}, {name: \"B\" text: \"...\"}]`".into());
    }
    values.form_dial = match field(vm, o, "form_dial") {
        v if v.is_nil() => 0,
        v => {
            let name = text_of(vm, v).unwrap_or_default();
            let held = values.held.iter().any(|(d, _)| *d == name);
            values.dials.iter().position(|(d, _)| *d == name).ok_or_else(|| match held {
                true => format!("form_dial: @{name} is past the tenth dial, held at its default; the form dial is one of the first {MAX_DIALS}"),
                false => format!("form_dial: @{name} is not one of the kit's dials"),
            })?
        }
    };
    if values.dials.is_empty() {
        return Err("forms: the kit has no dial to pick its forms (`dials: {form: 0.0 ...}`)".into());
    }
    let mut forms = Vec::with_capacity(items.len());
    for (k, item) in items.into_iter().enumerate() {
        let fo = item.as_object().ok_or_else(|| format!("forms[{k}] is not a {{...}} of settings"))?;
        for (key, v) in fields(vm, fo) {
            let key = key.to_string();
            if v.as_object().is_some_and(|f| vm.bx.heap.as_fn(f).is_some()) {
                return Err(format!("forms[{k}]: `{key}` is a fn; a form sets values, the kit's fns serve every form"));
            }
            if !FORM_KEYS.contains(&key.as_str()) {
                return Err(format!("forms[{k}]: `{key}` is not a setting a form can change ({})", FORM_KEYS.join(", ")));
            }
        }
        let src = Src::new(vm, o, Some(fo));
        let mut v = read_layer(vm, src).map_err(|e| format!("forms[{k}]: {e}"))?;
        let name = {
            let n = field(vm, fo, "name");
            text_of(vm, n).unwrap_or_default()
        };
        // The kit's own: its name and dials, and its post passes unless
        // the form has its own `post`.
        v.name = values.name.clone();
        v.dials = values.dials.clone();
        v.held = values.held.clone();
        if !has_own(vm, fo, "post") {
            v.passes = values.passes.clone();
            v.pass_values = values.pass_values.clone();
        }
        v.form_dial = values.form_dial;
        forms.push(Form { name, values: v });
    }
    values.forms = forms;
    Ok(values)
}

/// Reads one layer of settings: the kit's (`src.form` None) or a form's
/// over the kit's. The dials are always the kit's; the post passes are
/// read here for the kit and for a form with its own `post`.
fn read_layer(vm: &mut ScriptVm, src: Src) -> Result<KitValues, String> {
    let o = src.kit;
    let mut shape = ShapeSpec::default();
    let s = |vm: &mut ScriptVm, n: &str| {
        let v = src.get(vm, n);
        text_of(vm, v)
    };
    let f = |vm: &ScriptVm, n: &str| num(src.get(vm, n));
    let mut bold = true;
    if let Some(font) = s(vm, "font") {
        bold = font == "bold";
        shape.font = if font.contains('/') || font.contains('.') { FontSource::Path(font.into()) } else { FontSource::Bundled(font) };
    }
    shape.weight = f(vm, "weight");
    // `@bold` (the default) is Inter at 800, the display weight kinetic
    // type has always been set in.
    if bold {
        shape.font = FontSource::Bundled("inter".into());
        shape.weight = shape.weight.or(Some(800.0));
    }
    // Variable-font axes by four-letter tag: `axes: {wdth: 125 slnt: -8}`
    // (`wght` is `weight`).
    let axes = src.get(vm, "axes");
    if let Some(a) = axes.as_object() {
        for (tag, v) in fields(vm, a) {
            let Some(v) = num(v) else { continue };
            let tag = tag.to_string();
            let b = tag.as_bytes();
            if b.len() != 4 {
                return Err(format!("axes: `{tag}` is not a four-letter axis tag (wdth, wght, slnt, opsz, GRAD, ...)"));
            }
            if tag == "wght" {
                shape.weight = Some(v);
            } else {
                shape.axes.push((u32::from_be_bytes([b[0], b[1], b[2], b[3]]), v));
            }
        }
    }
    if let Some(v) = f(vm, "size") {
        shape.size = v.max(0.001);
    }
    if let Some(v) = f(vm, "depth") {
        shape.depth = v.max(0.0);
    }
    if let Some(v) = f(vm, "bevel") {
        shape.bevel = v.max(0.0);
    }
    if let Some(b) = s(vm, "bevel_type") {
        shape.bevel_profile = BevelProfile::by_name(&b).ok_or_else(|| format!("bevel_type: @{b} is not one of {}", BevelProfile::NAMES.join(", ")))?;
        if shape.bevel == 0.0 && b != "flat" {
            shape.bevel = (shape.depth * 0.25).min(shape.size * 0.06);
        }
        if matches!(shape.bevel_profile, BevelProfile::Round | BevelProfile::Cove | BevelProfile::Ogee) {
            shape.bevel_segments = 4;
        }
    }
    if let Some(v) = f(vm, "bevel_rings") {
        shape.bevel_segments = v.clamp(1.0, 8.0) as u32;
    }
    if let Some(v) = f(vm, "detail") {
        shape.detail = v;
    }
    if let Some(v) = f(vm, "tracking") {
        shape.tracking = v;
    }
    if let Some(v) = f(vm, "line_gap") {
        shape.line_height = v;
    }
    if let Some(v) = f(vm, "wrap") {
        shape.wrap = Some(v);
    }
    if let Some(a) = s(vm, "align") {
        shape.align = match a.as_str() {
            "left" => TextAlign::Left,
            "right" => TextAlign::Right,
            _ => TextAlign::Center,
        };
    }
    if s(vm, "layout").as_deref() == Some("cloud") {
        shape.cloud = true;
    }
    if let Some(a) = s(vm, "alphabet") {
        shape.alphabet = a;
    }
    let cells = src.get(vm, "cells");
    if let Some(c) = cells.as_object() {
        let fill = {
            let v = field(vm, c, "fill");
            text_of(vm, v)
        };
        shape.cells = Some(CellSpec {
            res: num(field(vm, c, "res")).unwrap_or(12.0) as u32,
            layers: num(field(vm, c, "layers")).unwrap_or(2.0) as u32,
            block: fill.as_deref() == Some("block"),
        });
    }
    let case = s(vm, "case");
    let mut colors = [vec4(0.02, 0.02, 0.04, 1.0), vec4(1.0, 1.0, 1.0, 1.0), vec4(0.2, 0.2, 0.3, 1.0), vec4(1.0, 0.45, 0.2, 1.0)];
    // The kit's colours, then a form's key by key.
    let layers = [Some(o), src.form];
    for layer in layers.into_iter().flatten() {
        let cv = field(vm, layer, "colors");
        if let Some(c) = cv.as_object() {
            for (k, n) in ["bg", "a", "b", "c"].iter().enumerate() {
                if let Some(v) = color(field(vm, c, n)) {
                    colors[k] = v;
                }
            }
        }
    }
    let material = match s(vm, "material").as_deref() {
        None | Some("matte") => 0.0,
        Some("metal") | Some("chrome") | Some("gold") => 1.0,
        Some("neon") => 2.0,
        Some("plastic") => 3.0,
        Some("glass") => 4.0,
        Some("holo") => 5.0,
        Some(other) => return Err(format!("material: @{other} is not one of matte, metal, neon, plastic, glass, holo")),
    };
    // The dials are the kit's (read_values gives them to its forms, and
    // the kit's post passes to a form without its own).
    let mut dials = Vec::new();
    let dv = if src.form.is_none() { field(vm, o, "dials") } else { NIL };
    if let Some(d) = dv.as_object() {
        for (name, v) in fields(vm, d) {
            dials.push((name.to_string(), num(v).unwrap_or(0.5)));
        }
    }
    // A dial is a kernel param and a shader function by its name (a held
    // one too): it may not take a name the kernel or the look already has.
    const TAKEN: &[&str] = &[
        "time", "seed", "count", "p1", "p2", "p3", "p4", "p5", "p6", "p7", "p8", "p9", "p10", "pos", "rot", "scale", "shear", "color", "attr", "info", "shape", "face", "nrm", "wpos", "lpos", "luv", "p", "p_b", "p_c", "bands",
        "key", "rim", "cap", "n", "vd", "eye", "content", "screen_uv", "finish", "shade", "env", "spec", "hue", "fog", "look", "floor", "deform", "backdrop", "picture", "ink",
        "qrot", "qturn", "hash1", "phase", "pulse", "beat", "bar", "bpm", "energy", "fwidth", "ray", "plane_hit", "text_plane", "k_share", "dying", "on_screen",
    ];
    for (name, _) in &dials {
        let module_fn = crate::kernel::KINETIC_MODULE.lines().filter_map(|l| l.strip_prefix("fn ")).any(|l| l.split('(').next() == Some(name.as_str()));
        if TAKEN.contains(&name.as_str()) || module_fn || crate::kernel::SIGNALS.iter().any(|(s, _)| s == name) {
            return Err(format!("dial `{name}` clashes with a name kits already have; call it something else (e.g. `{name}_amt`)"));
        }
    }
    // The first ten are the dials; any past them are held at their defaults.
    let held = dials.split_off(dials.len().min(MAX_DIALS));
    let (mut fov, mut dist, mut height) = (50.0, None, None);
    let cam = src.get(vm, "camera");
    if let Some(c) = cam.as_object() {
        fov = num(field(vm, c, "fov")).unwrap_or(fov);
        dist = num(field(vm, c, "dist"));
        height = num(field(vm, c, "height"));
    }
    let fl = src.get(vm, "ground");
    let floor = fl.as_object().map(|c| (num(field(vm, c, "y")), num(field(vm, c, "size"))));
    let pic = src.get(vm, "picture");
    let picture = pic.as_object().map(|c| {
        let w = num(field(vm, c, "width")).unwrap_or(1024.0).clamp(16.0, 4096.0) as u32;
        let h = num(field(vm, c, "height")).unwrap_or(256.0).clamp(16.0, 4096.0) as u32;
        (w, h, num(field(vm, c, "view")).unwrap_or(2.0).max(0.01))
    });
    let sf = src.get(vm, "grid");
    let surface = sf.as_object().map(|c| {
        let u = num(field(vm, c, "u")).unwrap_or(96.0).clamp(2.0, 1024.0) as u32;
        let v = num(field(vm, c, "v")).unwrap_or(32.0).clamp(2.0, 1024.0) as u32;
        (u, v, num(field(vm, c, "copies")).unwrap_or(1.0).clamp(1.0, 256.0) as u32)
    });
    let cv = src.get(vm, "curve");
    let curve_points = cv.as_object().map(|c| num(field(vm, c, "points")).unwrap_or(256.0).clamp(4.0, 4096.0) as u32);
    let curve_frames = cv.as_object().map_or(crate::curve::Frames::default(), |c| crate::curve::Frames {
        closed: num(field(vm, c, "closed")).unwrap_or(0.0) > 0.5,
        up: field(vm, c, "up").as_array().and_then(|a| {
            let at = |i| num(vm.bx.heap.array_index(a, i, NoTrap));
            Some([at(0)?, at(1)?, at(2)?])
        }),
    });
    let (mut cycle_beats, mut pingpong) = (0.0, false);
    let cy = src.get(vm, "cycle");
    if let Some(c) = cy.as_object() {
        cycle_beats = num(field(vm, c, "beats")).unwrap_or(4.0);
        pingpong = num(field(vm, c, "pingpong")).unwrap_or(0.0) > 0.5;
    }
    // Passes: kits call their template, `Pass{}`s are read as they are.
    let mut passes = Vec::new();
    let mut pass_values = Vec::new();
    let post = match src.form {
        None => field(vm, o, "post"),
        Some(f) if has_own(vm, f, "post") => field(vm, f, "post"),
        Some(_) => NIL,
    };
    let items = makepad_render_graph::script::post_items(vm, post);
    let module = vm.bx.heap.value(vm.bx.heap.modules, LiveId::from_str(KIT_MODULE).into(), NoTrap).as_object().ok_or("the kit module is not registered")?;
    for (k, item) in items.into_iter().enumerate() {
        let label = format!("post[{k}]");
        let kind = makepad_render_graph::script::post_kind(vm, item).ok_or_else(|| format!("{label} is a kit (Glow{{..}}) or a Pass{{..}}"))?;
        let reads = match makepad_render_graph::script::read_post_entry(vm, module, &kind, item, &label, &format!("p{k}_"))? {
            makepad_render_graph::script::PostEntry::Passes(reads) => reads,
            makepad_render_graph::script::PostEntry::Host(_) => return Err(format!("{label} is a kit (Glow{{..}}) or a Pass{{..}}")),
        };
        for read in reads {
            let mut decl = read.decl;
            let mut v = Vec::new();
            for (name, val) in read.uniforms {
                let (width, value) = if let Some(n) = num(val) {
                    (1, [n, 0.0, 0.0, 0.0])
                } else if let Some(c) = color(val) {
                    (4, [c.x, c.y, c.z, c.w])
                } else {
                    return Err(format!("{}: uniform `{name}` is a number or a colour", decl.label));
                };
                decl.uniforms.push(makepad_render_graph::UniformDecl { name, width });
                v.push(value.into());
            }
            decl.validate().map_err(|e| format!("{}: {e}", decl.label))?;
            passes.push(decl);
            pass_values.push(v);
        }
    }
    Ok(KitValues {
        name: s(vm, "name").unwrap_or_default(),
        text: s(vm, "text").unwrap_or_default(),
        upper: case.as_deref() == Some("upper"),
        lower: case.as_deref() == Some("lower"),
        shape,
        copies: f(vm, "copies").unwrap_or(1.0).clamp(1.0, 64.0) as usize,
        colors,
        material,
        dials,
        held,
        fov,
        dist,
        height,
        floor,
        picture,
        surface,
        curve_points,
        curve_frames,
        cycle_beats,
        pingpong,
        dying: f(vm, "dying").filter(|d| *d > 0.0).map(|d| d.min(30.0)),
        screen: f(vm, "screen").is_some_and(|v| v > 0.5),
        passes,
        pass_values,
        forms: Vec::new(),
        form_dial: 0,
    })
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// A family kit: the kit's settings, and three forms over them.
    pub const FAMILY: &str = r#"Kinetic{
    name: "FAMILY"  text: "ONE"  case: @upper  font: @roboto  weight: 820  axes: {wdth: 84}
    copies: 2  picture: {width: 512 height: 128 view: 2.0}
    camera: {dist: 10.0 fov: 72}
    colors: {bg: #x000000 a: #xffffff b: #x0d0d0d c: #xff0000}
    dials: {form: 0.1 speed: 0.5}
    post: [Glow{threshold: 0.95 strength: 0.3}]
    forms: [
        {name: "FIRST"},
        {name: "SECOND" text: "two words here" case: @lower font: @inter weight: 900 copies: 3 colors: {b: #x3c3c3c} camera: {fov: 30} wrap: 4 picture: nil screen: true post: [Glow{threshold: 0.95 strength: 0.25}]},
        {name: "THIRD" text: "third" post: []}
    ]
    look: fn() -> vec4 { return vec4(1.0, 1.0, 1.0, 1.0) }
}"#;

    fn with_kit<R>(src: &str, f: impl FnOnce(Result<Kit, String>) -> R) -> R {
        let mut cx = Cx::new(Box::new(|_, _| {}));
        cx.with_vm(|vm| {
            makepad_draw::script_mod(vm);
            crate::view::script_mod(vm);
            f(load(vm, src, "family_test"))
        })
    }

    /// The first value of each pass uniform (a Glow's strength among them).
    fn firsts(v: &KitValues) -> Vec<f32> {
        v.pass_values.iter().flatten().map(|u| u.0[0]).collect()
    }

    #[test]
    fn forms_read_the_kit_with_their_own_settings_over_it() {
        let v = with_kit(FAMILY, |kit| kit.unwrap_or_else(|e| panic!("{e}")).values);
        assert_eq!((v.forms.len(), v.form_dial, v.screen), (3, 0, false));
        let names: Vec<&str> = v.forms.iter().map(|f| f.name.as_str()).collect();
        assert_eq!(names, ["FIRST", "SECOND", "THIRD"]);
        // FIRST names nothing: the kit as written.
        let a = &v.forms[0].values;
        assert_eq!((a.name.as_str(), a.text.as_str(), a.upper, a.copies), ("FAMILY", "ONE", true, 2));
        assert_eq!((a.shape.font.clone(), a.shape.weight, a.shape.axes.clone()), (v.shape.font.clone(), Some(820.0), vec![(u32::from_be_bytes(*b"wdth"), 84.0)]));
        assert_eq!((a.picture, a.fov, a.dist, a.colors, a.shape.wrap), (v.picture, 72.0, Some(10.0), v.colors, None));
        assert_eq!((a.passes.len(), firsts(a)), (v.passes.len(), firsts(&v)));
        assert!(!a.passes.is_empty() && firsts(a).contains(&0.3), "the kit's glow: {:?}", firsts(a));
        // SECOND: its words, case, whole font (no wdth from the kit),
        // copies, one colour, its whole camera, a wrap, no picture, on
        // screen, its own glow; the kit's name and dials.
        let b = &v.forms[1].values;
        assert_eq!((b.name.as_str(), b.text.as_str(), b.upper, b.lower, b.copies), ("FAMILY", "two words here", false, true, 3));
        assert_eq!((b.shape.font.clone(), b.shape.weight, b.shape.axes.clone()), (FontSource::Bundled("inter".into()), Some(900.0), vec![]));
        assert_eq!((b.colors[0], b.colors[1], b.colors[3]), (v.colors[0], v.colors[1], v.colors[3]));
        assert!((b.colors[2].x - 60.0 / 255.0).abs() < 1e-3 && b.colors[2] != v.colors[2], "{:?}", b.colors[2]);
        assert_eq!((b.fov, b.dist, b.shape.wrap, b.picture, b.screen), (30.0, None, Some(4.0), None, true));
        assert_eq!(b.dials, v.dials);
        assert_eq!(b.passes, a.passes, "the same glow, at its own strength");
        assert!(firsts(b).contains(&0.25) && !firsts(b).contains(&0.3), "{:?}", firsts(b));
        // THIRD: its words, no post passes, the kit's font and case.
        let c = &v.forms[2].values;
        assert_eq!((c.text.as_str(), c.upper, c.shape.weight, c.shape.axes.len(), c.passes.len(), c.pass_values.len()), ("third", true, Some(820.0), 1, 0, 0));
        // A kit without forms has none, and reads as before.
        let plain = with_kit("Kinetic{ text: \"A\" dials: {x_amt: 0.2} }", |kit| kit.unwrap().values);
        assert!(plain.forms.is_empty() && plain.text == "A" && !plain.screen);
        // `screen: true` on a kit; `form_dial` names the dial.
        let on = with_kit("Kinetic{ screen: true picture: {} dials: {a_amt: 0.0 pick: 0.9} form_dial: @pick forms: [{}, {text: \"B\"}] }", |kit| kit.unwrap().values);
        assert!(on.screen && on.forms[0].values.screen && on.form_dial == 1);
    }

    #[test]
    fn a_form_sets_values_only() {
        let err = |src: &str| with_kit(src, |kit| kit.err().expect("an error"));
        assert!(err("Kinetic{ dials: {form: 0.0} forms: [{look: fn() -> vec4 { return vec4(1.0, 1.0, 1.0, 1.0) }}] }").contains("forms[0]: `look` is a fn"));
        assert!(err("Kinetic{ dials: {form: 0.0} forms: [{}, {fnt: @inter}] }").contains("forms[1]: `fnt` is not a setting"));
        assert!(err("Kinetic{ dials: {form: 0.0} forms: [{dials: {a_amt: 0.1}}] }").contains("`dials` is not a setting"));
        assert!(err("Kinetic{ dials: {form: 0.0} form_dial: @nope forms: [{}] }").contains("form_dial: @nope"));
        assert!(err("Kinetic{ forms: [{}, {}] }").contains("no dial"));
        assert!(err("Kinetic{ dials: {form: 0.0} forms: [{material: @wood}] }").contains("forms[0]: material"));
    }
}
