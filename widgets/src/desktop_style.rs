//! Hotloadable application styles. The WM owns framebuffer transitions; this
//! module only installs Splash definitions and reapplies the existing widget tree.
use crate::*;
use makepad_micro_serde::*;
use std::collections::HashMap;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum DesktopStyle {
    #[default]
    Omarchy,
    Macos,
    Windows,
    Windows2000,
    NextStep,
    Ios,
    Android,
    /// A dark style in near-black and orange, the only one here not modelled
    /// on somebody else's desktop. Declared last: the window manager's style
    /// tween reads its weights by discriminant (1 is macOS, 3 Windows 2000,
    /// 4 NeXTSTEP), so a new style takes the next number and `ALL` below
    /// keeps the order they are shown in.
    BlackOrange,
    /// Soft moulded surfaces on one near-white ground: no borders, every
    /// visible edge is light on a shoulder. The first sheet built on the
    /// surface material.
    Neumorphic,
    /// Moulded grey plastic: caps standing off a warm grey housing under a
    /// hard light, wells cut into it. A press deepens.
    Molded,
    /// Black glossy plastic with a cyan indicator: a gloss sweep on every
    /// cap, and a press that lights up rather than moves.
    Glossy,
    /// Milled near-black metal lit from inside in orange: flat machined
    /// faces, a hard hairline on every edge, everything that is on glows.
    Milled,
    /// Turned and brushed aluminium on a pale housing, chrome where a
    /// control is held.
    Aluminium,
    /// Frosted glass cards over a dark ground.
    Frosted,
    /// Clear glass on a light ground: thin bright rims, the ground seen
    /// through every face.
    Liquid,
    /// Porcelain faces on a dark ground, lit from underneath.
    Luminous,
    /// Minimal instrument hardware: a pale case, hairlines, few colours.
    FieldKit,
    /// A text interface: one face, one ink, drawn in character cells.
    Terminal,
    /// A segment display on a pale glass, unlit segments still faintly there.
    Lcd,
    /// Tubes of light on a near-black ground.
    Neon,
    /// Line frames of a head-up display over a dark ground.
    Hud,
}

impl DesktopStyle {
    /// How many styles there are, and so how many weights a table indexed by
    /// discriminant needs: every variant is in `ALL`.
    pub const COUNT: usize = 21;
    pub const ALL: [Self; Self::COUNT] = [
        Self::Omarchy, Self::BlackOrange, Self::Neumorphic, Self::Molded, Self::Glossy, Self::Milled,
        Self::Aluminium, Self::Frosted, Self::Liquid, Self::Luminous, Self::FieldKit,
        Self::Terminal, Self::Lcd, Self::Neon, Self::Hud,
        Self::Macos, Self::Windows, Self::Windows2000, Self::NextStep, Self::Ios, Self::Android,
    ];
    /// The styles laid out as tiles rather than as floating windows, with no
    /// shelf and no title bar of their own: the tilers and every style built
    /// on the library's own surfaces rather than on somebody's desktop.
    const TILING: [Self; 15] = [
        Self::Omarchy, Self::BlackOrange, Self::Neumorphic, Self::Molded, Self::Glossy, Self::Milled,
        Self::Aluminium, Self::Frosted, Self::Liquid, Self::Luminous, Self::FieldKit,
        Self::Terminal, Self::Lcd, Self::Neon, Self::Hud,
    ];
    fn tiling(self) -> bool {
        Self::TILING.contains(&self)
    }
    pub fn id(self) -> &'static str {
        match self {
            Self::Omarchy => "omarchy",
            Self::BlackOrange => "black-orange",
            Self::Neumorphic => "neumorphic",
            Self::Molded => "molded",
            Self::Glossy => "glossy",
            Self::Milled => "milled",
            Self::Aluminium => "aluminium",
            Self::Frosted => "frosted",
            Self::Liquid => "liquid",
            Self::Luminous => "luminous",
            Self::FieldKit => "field-kit",
            Self::Terminal => "terminal",
            Self::Lcd => "lcd",
            Self::Neon => "neon",
            Self::Hud => "hud",
            Self::Macos => "macos",
            Self::Windows => "windows",
            Self::Windows2000 => "windows-2000",
            Self::NextStep => "nextstep",
            Self::Ios => "ios",
            Self::Android => "android",
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            Self::Omarchy => "Omarchy",
            Self::BlackOrange => "Black orange",
            Self::Neumorphic => "Neumorphic",
            Self::Molded => "Molded",
            Self::Glossy => "Glossy",
            Self::Milled => "Milled",
            Self::Aluminium => "Aluminium",
            Self::Frosted => "Frosted glass",
            Self::Liquid => "Liquid glass",
            Self::Luminous => "Luminous",
            Self::FieldKit => "Field kit",
            Self::Terminal => "Terminal",
            Self::Lcd => "Segment display",
            Self::Neon => "Neon",
            Self::Hud => "Head-up display",
            Self::Macos => "macOS",
            Self::Windows => "Windows",
            Self::Windows2000 => "Windows 2000",
            Self::NextStep => "NeXTSTEP",
            Self::Ios => "iOS",
            Self::Android => "Android",
        }
    }
    pub fn parse(s: &str) -> Option<Self> {
        let s = s.strip_suffix("-dark").unwrap_or(s);
        Self::ALL.into_iter().find(|v| v.id() == s)
    }
    /// Neumorphic's dark appearance is a soft dark ground of the same
    /// moulding, so it is that style's other appearance, `neumorphic-dark`,
    /// and not a style of its own: `parse` reads a `-dark` suffix as the
    /// appearance, which a style named so could never get past.
    pub fn supports_dark(self) -> bool { matches!(self, Self::Neumorphic | Self::Macos | Self::Windows | Self::Ios | Self::Android) }
    pub fn mobile(self) -> bool { matches!(self, Self::Ios | Self::Android) }
    /// Which set of app artwork this style draws, as an index into the icon
    /// table. A style is free to borrow another's drawings rather than have
    /// every icon redrawn for it -- the table is one entry per SET, not one
    /// per style, so the enum's own order must not be read as an index into
    /// it.
    pub fn icon_set(self) -> usize {
        match self {
            Self::Omarchy | Self::BlackOrange | Self::Neumorphic | Self::Glossy | Self::Milled => 0,
            Self::Aluminium | Self::Frosted | Self::Liquid | Self::Luminous | Self::FieldKit => 0,
            Self::Terminal | Self::Lcd | Self::Neon | Self::Hud => 0,
            Self::Macos => 1,
            Self::Windows | Self::Molded => 2,
            Self::Windows2000 => 3,
            Self::NextStep => 4,
            Self::Ios => 5,
            Self::Android => 6,
        }
    }
    pub fn next(self) -> Self {
        // By place in `ALL`, not by discriminant: the two orders differ.
        let at = Self::ALL.iter().position(|style| *style == self).unwrap_or(0);
        Self::ALL[(at + 1) % Self::ALL.len()]
    }
    pub fn floating(self) -> bool {
        !self.tiling() && !self.mobile()
    }
    pub fn shelf_height(self) -> f64 {
        match self {
            Self::Omarchy | Self::BlackOrange | Self::Neumorphic | Self::Molded | Self::Glossy | Self::Milled => 0.0,
            Self::Aluminium | Self::Frosted | Self::Liquid | Self::Luminous | Self::FieldKit => 0.0,
            Self::Terminal | Self::Lcd | Self::Neon | Self::Hud => 0.0,
            Self::Macos => 86.0,
            Self::Windows => 54.0,
            Self::Windows2000 => 34.0,
            Self::NextStep | Self::Ios | Self::Android => 0.0,
        }
    }
    pub fn title_height(self) -> f64 {
        match self {
            Self::Omarchy | Self::BlackOrange | Self::Neumorphic | Self::Molded | Self::Glossy | Self::Milled => 0.0,
            Self::Aluminium | Self::Frosted | Self::Liquid | Self::Luminous | Self::FieldKit => 0.0,
            Self::Terminal | Self::Lcd | Self::Neon | Self::Hud => 0.0,
            Self::Macos => 32.0,
            Self::Windows => 34.0,
            Self::Windows2000 => 20.0,
            Self::NextStep => 22.0,
            Self::Ios | Self::Android => 0.0,
        }
    }
}

/// Two phases: theme tokens before widget registration, component overrides
/// after it. Applications then evaluate their own Splash against those defaults.
#[derive(Clone, Debug, PartialEq, SerJson, DeJson)]
pub struct StyleSheet {
    pub name: String,
    pub theme: String,
    pub widgets: String,
    pub icons: Vec<crate::app_icon::IconAsset>,
}
#[derive(SerJson, DeJson)]
struct Envelope {
    makepad_style: StyleSheet,
}
#[derive(Default)]
struct Styles {
    heaps: HashMap<usize, StyleSheet>,
}

impl StyleSheet {
    pub fn load(style: DesktopStyle) -> Self {
        Self::load_with_appearance(style, false)
    }
    pub fn load_with_appearance(style: DesktopStyle, dark: bool) -> Self {
        let name = match (style, dark) {
            (DesktopStyle::Neumorphic, true) => "neumorphic-dark",
            (DesktopStyle::Macos, true) => "macos-dark",
            (DesktopStyle::Windows, true) => "windows-dark",
            (DesktopStyle::Ios, true) => "ios-dark",
            (DesktopStyle::Android, true) => "android-dark",
            _ => style.id(),
        };
        let (theme, widgets) = match style {
            DesktopStyle::Omarchy => (
                include_str!("../themes/omarchy/theme.splash"),
                include_str!("../themes/omarchy/widgets.splash"),
            ),
            DesktopStyle::BlackOrange => (
                include_str!("../themes/black-orange/theme.splash"),
                include_str!("../themes/black-orange/widgets.splash"),
            ),
            DesktopStyle::Neumorphic if dark => (
                include_str!("../themes/neumorphic-dark/theme.splash"),
                include_str!("../themes/neumorphic-dark/widgets.splash"),
            ),
            DesktopStyle::Neumorphic => (
                include_str!("../themes/neumorphic/theme.splash"),
                include_str!("../themes/neumorphic/widgets.splash"),
            ),
            DesktopStyle::Molded => (
                include_str!("../themes/molded/theme.splash"),
                include_str!("../themes/molded/widgets.splash"),
            ),
            DesktopStyle::Glossy => (
                include_str!("../themes/glossy/theme.splash"),
                include_str!("../themes/glossy/widgets.splash"),
            ),
            DesktopStyle::Milled => (
                include_str!("../themes/milled/theme.splash"),
                include_str!("../themes/milled/widgets.splash"),
            ),
            DesktopStyle::Aluminium => (
                include_str!("../themes/aluminium/theme.splash"),
                include_str!("../themes/aluminium/widgets.splash"),
            ),
            DesktopStyle::Frosted => (
                include_str!("../themes/frosted/theme.splash"),
                include_str!("../themes/frosted/widgets.splash"),
            ),
            DesktopStyle::Liquid => (
                include_str!("../themes/liquid/theme.splash"),
                include_str!("../themes/liquid/widgets.splash"),
            ),
            DesktopStyle::Luminous => (
                include_str!("../themes/luminous/theme.splash"),
                include_str!("../themes/luminous/widgets.splash"),
            ),
            DesktopStyle::FieldKit => (
                include_str!("../themes/field-kit/theme.splash"),
                include_str!("../themes/field-kit/widgets.splash"),
            ),
            DesktopStyle::Terminal => (
                include_str!("../themes/terminal/theme.splash"),
                include_str!("../themes/terminal/widgets.splash"),
            ),
            DesktopStyle::Lcd => (
                include_str!("../themes/lcd/theme.splash"),
                include_str!("../themes/lcd/widgets.splash"),
            ),
            DesktopStyle::Neon => (
                include_str!("../themes/neon/theme.splash"),
                include_str!("../themes/neon/widgets.splash"),
            ),
            DesktopStyle::Hud => (
                include_str!("../themes/hud/theme.splash"),
                include_str!("../themes/hud/widgets.splash"),
            ),
            DesktopStyle::Macos if dark => (
                include_str!("../themes/macos-dark/theme.splash"),
                include_str!("../themes/macos-dark/widgets.splash"),
            ),
            DesktopStyle::Macos => (
                include_str!("../themes/macos/theme.splash"),
                include_str!("../themes/macos/widgets.splash"),
            ),
            DesktopStyle::Windows if dark => (
                include_str!("../themes/windows-dark/theme.splash"),
                include_str!("../themes/windows-dark/widgets.splash"),
            ),
            DesktopStyle::Windows => (
                include_str!("../themes/windows/theme.splash"),
                include_str!("../themes/windows/widgets.splash"),
            ),
            DesktopStyle::Windows2000 => (
                include_str!("../themes/windows-2000/theme.splash"),
                include_str!("../themes/windows-2000/widgets.splash"),
            ),
            DesktopStyle::NextStep => (
                include_str!("../themes/nextstep/theme.splash"),
                include_str!("../themes/nextstep/widgets.splash"),
            ),
            DesktopStyle::Ios if dark => (
                include_str!("../themes/ios-dark/theme.splash"),
                include_str!("../themes/ios-dark/widgets.splash"),
            ),
            DesktopStyle::Ios => (
                include_str!("../themes/ios/theme.splash"),
                include_str!("../themes/ios/widgets.splash"),
            ),
            DesktopStyle::Android if dark => (
                include_str!("../themes/android-dark/theme.splash"),
                include_str!("../themes/android-dark/widgets.splash"),
            ),
            DesktopStyle::Android => (
                include_str!("../themes/android/theme.splash"),
                include_str!("../themes/android/widgets.splash"),
            ),
        };
        let read = |file: &str, bundled: &str| {
            // Source checkouts hotload on every selection; installed/wasm builds
            // carry the identical embedded stylesheet, with no external dependency.
            #[cfg(not(target_arch = "wasm32"))]
            if let Ok(text) = std::fs::read_to_string(
                std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join("themes")
                    .join(name)
                    .join(file),
            ) {
                return text;
            }
            let _ = file;
            bundled.to_string()
        };
        Self {
            name: name.into(),
            theme: read("theme.splash", theme),
            widgets: read("widgets.splash", widgets),
            icons: crate::app_icon::load_assets(style),
        }
    }
    pub fn to_json(&self) -> String {
        Envelope {
            makepad_style: self.clone(),
        }
        .serialize_json()
    }
    pub fn parse(json: &str) -> Option<Self> {
        if !json.contains("\"makepad_style\"") {
            return None;
        }
        Envelope::deserialize_json(json)
            .ok()
            .map(|e| e.makepad_style)
    }
}

pub(crate) fn gc_heaps(cx: &mut Cx, heaps: &[usize]) {
    cx.global::<Styles>()
        .heaps
        .retain(|key, _| !heaps.contains(key));
}
pub fn install(vm: &mut ScriptVm, sheet: StyleSheet) {
    let style = DesktopStyle::parse(&sheet.name).unwrap_or(DesktopStyle::Macos);
    crate::app_icon::install(vm.cx_mut(), style, &sheet.icons);
    let key = vm.bx.heap.heap_key();
    vm.cx_mut().global::<Styles>().heaps.insert(key, sheet);
}
/// Take the sheet off again, so the next evaluation runs under the plain
/// theme. `install` had no way back: an app that lets somebody try a sheet
/// could put one on and never return to what it started with. A sheet named
/// by `MAKEPAD_WIDGET_STYLE` comes back on the next read, as it would have
/// arrived in the first place.
pub fn uninstall(vm: &mut ScriptVm) {
    let key = vm.bx.heap.heap_key();
    vm.cx_mut().global::<Styles>().heaps.remove(&key);
}
pub fn current(vm: &mut ScriptVm) -> Option<StyleSheet> {
    let key = vm.bx.heap.heap_key();
    if let Some(sheet) = vm.cx_mut().global::<Styles>().heaps.get(&key).cloned() {
        return Some(sheet);
    }
    // Opt-in only. Picking a sheet from OsType restyled every app that had
    // never asked for one, and an app that calls `theme_mod` + `widgets_mod`
    // without `script_mod` got the theme half of it and not the widget half.
    let name = std::env::var("MAKEPAD_WIDGET_STYLE").ok()?;
    let style = DesktopStyle::parse(&name)?;
    let sheet = StyleSheet::load_with_appearance(style, name.ends_with("-dark"));
    install(vm, sheet.clone());
    Some(sheet)
}
/// Read just the active appearance without cloning the Splash and SVG payloads.
pub fn current_name(vm: &mut ScriptVm) -> Option<String> {
    let key = vm.bx.heap.heap_key();
    if let Some(sheet) = vm.cx_mut().global::<Styles>().heaps.get(&key) {
        return Some(sheet.name.clone());
    }
    current(vm).map(|sheet| sheet.name)
}
/// The active family, without copying a stylesheet or its assets.
pub fn current_style(vm: &mut ScriptVm) -> DesktopStyle {
    let key=vm.bx.heap.heap_key();
    if let Some(sheet)=vm.cx_mut().global::<Styles>().heaps.get(&key) {
        return DesktopStyle::parse(&sheet.name).unwrap_or_default();
    }
    current(vm).and_then(|sheet|DesktopStyle::parse(&sheet.name)).unwrap_or_default()
}
fn evaluate(vm: &mut ScriptVm, sheet: &StyleSheet, phase: &str, code: String) {
    vm.eval(ScriptMod {
        cargo_manifest_path: env!("CARGO_MANIFEST_DIR").into(),
        module_path: format!("desktop_style_{}_{}", sheet.name, phase),
        file: format!("themes/{}/{phase}.splash", sheet.name),
        line: 0,
        column: 0,
        code,
        values: vec![],
    });
}
pub fn apply_theme(vm: &mut ScriptVm) {
    if let Some(sheet) = current(vm) {
        evaluate(vm, &sheet, "theme", sheet.theme.clone());
        // The library's own roles are younger than the sheets and no sheet
        // names them, so they are brought into line with what this one set.
        let roles = {
            let theme = vm.module(id!(theme));
            let mut read = |key: &str| vm.bx.heap.value(theme, LiveId::from_str(key).into(), NoTrap).as_color();
            crate::theme_tokens::sheet_roles_script(&sheet.theme, &mut read)
        };
        evaluate(vm, &sheet, "roles", roles);
        if DesktopStyle::parse(&sheet.name).is_some_and(|style| style.mobile()) {
            crate::font_policy::append_style_fallbacks(vm);
        }
    }
}
pub fn apply_widgets(vm: &mut ScriptVm) {
    if let Some(sheet) = current(vm) {
        evaluate(vm, &sheet, "widgets", sheet.widgets.clone());
    }
}
/// The stock templates whose face a sheet may replace -- `draw_bg.vertex`
/// and `draw_bg.pixel` -- and whose own face the library's chrome keeps.
///
/// A sheet's second half runs after every template registered and writes
/// straight onto these objects, so everything derived from one of them
/// that does not declare the two functions itself takes the sheet's. The
/// developer panel, the fab controls and a host's own tool panels are built
/// from these same templates and must keep their look under every sheet, so
/// before the sheet runs each face is kept under `mod.stock_faces.<name>`
/// as `{vertex pixel}`, and chrome spreads it into its own draw object:
/// `draw_bg +: {..mod.stock_faces.CheckBox}`.
pub const STOCK_FACES: &[&str] = &[
    "Button", "ButtonFlat", "ButtonFlatter", "ButtonPrimary", "ButtonSecondary", "ButtonTertiary",
    "ButtonOutline", "ButtonDashed", "ButtonDanger", "ButtonIcon", "ButtonFlatIcon", "ButtonFlatterIcon",
    "CheckBox", "CheckBoxFlat", "Toggle", "ToggleFlat",
    "RadioButton", "RadioButtonFlat", "RadioButtonTab", "RadioButtonTabFlat",
    "Slider", "SliderFlat", "SliderMinimal", "SliderRound", "Rotary", "RotaryKnob",
    "TextInput", "TextInputFlat", "ComboBox", "FieldWell", "TagField", "NumberField",
    "DropDown", "DropDownFlat", "PopupMenu", "PopupMenuItem",
    "Tab", "TabBar", "ProgressBar", "ScrollBar", "RoundedView", "PanelView",
];

/// Keep every face in [`STOCK_FACES`] under `mod.stock_faces`, before a
/// sheet's widget half can replace it. Called at the end of the module run's
/// widget registration, so the kept face is the library's own even while a
/// sheet is installed.
pub(crate) fn keep_stock_faces(vm: &mut ScriptVm) {
    let mut code = String::from("mod.stock_faces = {\n");
    for name in STOCK_FACES {
        code.push_str(&format!(
            "    {name}: {{vertex: mod.widgets.{name}.draw_bg.vertex pixel: mod.widgets.{name}.draw_bg.pixel}}\n"
        ));
    }
    // The last statement of an evaluated script is swallowed.
    code.push_str("}\ntrue\n");
    vm.eval(ScriptMod {
        cargo_manifest_path: env!("CARGO_MANIFEST_DIR").into(),
        module_path: "desktop_style_stock_faces".into(),
        file: "desktop_style/stock_faces".into(),
        line: 0,
        column: 0,
        code,
        values: vec![],
    });
}

/// A sheet that gives every stock face a vertex and a pixel function of its
/// own and changes nothing else: what a host's tests install to prove its
/// chrome keeps its own face under any sheet. See [`faces_reaching`].
#[doc(hidden)]
pub fn marker_sheet() -> StyleSheet {
    let mut widgets = String::from("use mod.prelude.widgets_internal.*\n");
    for name in STOCK_FACES {
        widgets.push_str(&format!("mod.widgets.{name}.draw_bg.pixel = fn() {{ return #ff00ffff }}\n"));
        widgets.push_str(&format!(
            "mod.widgets.{name}.draw_bg.vertex = fn() {{ self.vertex_pos = self.clip_and_transform_vertex(self.rect_pos, self.rect_size) }}\n"
        ));
    }
    widgets.push_str("true\n");
    StyleSheet {
        name: "marker".into(),
        theme: "mod.theme = mod.themes.dark\ntrue\n".into(),
        widgets,
        icons: Vec::new(),
    }
}

/// Every place under `root` where a face the installed sheet gave a stock
/// template is what would be drawn, as a readable path from `what`.
///
/// Walks the object and everything it holds or inherits -- its own keys,
/// the keys of its prototypes, and its children -- because a draw object a
/// template never wrote out is its prototype's, and that is the object a
/// sheet wrote into. Meant to be run with [`marker_sheet`] installed and the
/// module reloaded, so that every face a sheet can reach is a marker.
#[doc(hidden)]
pub fn faces_reaching(vm: &mut ScriptVm, root: ScriptValue, what: &str) -> Vec<String> {
    use std::collections::HashMap;
    let widgets = vm.module(id!(widgets));
    let mut marks = Vec::new();
    for name in STOCK_FACES {
        for face in [id!(pixel), id!(vertex)] {
            marks.push(vm.bx.heap.value_path(widgets, &[LiveId::from_str(name), id!(draw_bg), face], NoTrap));
        }
    }
    let heap = &vm.bx.heap;
    // What an object holds or inherits, as (name, value): its own keys and
    // its prototypes', resolved on the object, and its children -- its own,
    // or, where it was made without a copy of them, the nearest prototype's.
    let entries = |obj: ScriptObject| -> Vec<(String, ScriptValue)> {
        let mut keys: Vec<ScriptValue> = Vec::new();
        let mut at = Some(obj);
        while let Some(o) = at {
            for (key, _) in heap.map_ref(o).iter() {
                if !keys.contains(key) {
                    keys.push(*key);
                }
            }
            at = heap.proto(o).as_object();
        }
        let name = |key: ScriptValue| key.as_id().map(|id| id.to_string()).unwrap_or_else(|| "_".into());
        let mut out: Vec<(String, ScriptValue)> = keys.into_iter().map(|key| (name(key), heap.value(obj, key, NoTrap))).collect();
        let mut at = Some(obj);
        while let Some(o) = at {
            let vec = heap.vec_ref(o);
            if !vec.is_empty() {
                out.extend(vec.iter().map(|entry| (name(entry.key), entry.value)));
                break;
            }
            at = heap.proto(o).as_object();
        }
        out
    };
    let is_mark = |name: &str, value: ScriptValue| (name == "pixel" || name == "vertex") && marks.contains(&value);
    // First which objects lead to a marker at all, each object once; then
    // every path to one, so a part shared by many controls is named at each
    // of them rather than at whichever the walk happened to reach first.
    fn leads(
        obj: ScriptObject,
        depth: usize,
        entries: &dyn Fn(ScriptObject) -> Vec<(String, ScriptValue)>,
        is_mark: &dyn Fn(&str, ScriptValue) -> bool,
        is_fn: &dyn Fn(ScriptObject) -> bool,
        memo: &mut HashMap<ScriptObject, bool>,
    ) -> bool {
        if let Some(known) = memo.get(&obj) {
            return *known;
        }
        memo.insert(obj, false);
        if depth > 48 {
            return false;
        }
        let mut found = false;
        for (name, value) in entries(obj) {
            let Some(child) = value.as_object() else { continue };
            if is_fn(child) {
                found |= is_mark(&name, value);
            } else {
                found |= leads(child, depth + 1, entries, is_mark, is_fn, memo);
            }
        }
        memo.insert(obj, found);
        found
    }
    let is_fn = |obj: ScriptObject| heap.is_fn(obj);
    let mut memo = HashMap::new();
    let mut out = Vec::new();
    let Some(root) = root.as_object() else { return out };
    if !leads(root, 0, &entries, &is_mark, &is_fn, &mut memo) {
        return out;
    }
    let mut stack = vec![(root, what.to_string(), vec![root])];
    while let Some((obj, path, trail)) = stack.pop() {
        if out.len() >= 400 {
            break;
        }
        for (name, value) in entries(obj) {
            let Some(child) = value.as_object() else { continue };
            if heap.is_fn(child) {
                if is_mark(&name, value) {
                    out.push(format!("{path}.{name}"));
                }
            } else if memo.get(&child) == Some(&true) && !trail.contains(&child) {
                let mut trail = trail.clone();
                trail.push(child);
                stack.push((child, format!("{path}.{name}"), trail));
            }
        }
    }
    out.sort();
    out.dedup();
    out
}

/// Window provides the common receive path, including apps with no WM API dependency.
pub fn handle_event(cx: &mut Cx, event: &Event) {
    let Event::Custom(json) = event else {
        return;
    };
    let Some(sheet) = StyleSheet::parse(json) else {
        return;
    };
    let changed = cx.with_vm(|vm| {
        if current(vm).as_ref() == Some(&sheet) {
            return false;
        }
        install(vm, sheet);
        true
    });
    if changed {
        cx.request_style_reload();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The fields a person types or picks a value in that are laid out by a
    /// TURTLE: they are Fit, so the vertical padding IS their height, and two
    /// of them padded differently part company again as soon as the type
    /// grows. `TextInput` heads the list because every sheet already reshaped
    /// it, and it is the one the rest of the row has to match.
    ///
    /// Left out, with reasons, so the next reader does not take the gaps for
    /// oversights: `WellInput` is the inside of a field, not a field, and is
    /// the one thing that must impose no height at all; `Select` wears a
    /// `Button` for a face and follows the button rule; `TreeSelect` names its
    /// corner `radius` and packs chips into whatever box its own Rust is
    /// handed.
    const FIELD_FIT: &[&str] = &[
        "TextInput",
        "ComboBox",
        "DropDown",
        "DropDown2",
        "FieldWell",
        "TagField",
    ];
    /// The fields that state a FRAME and lay their own parts out inside it: a
    /// number with a stepper, a number you drag, a date, a time, and the two
    /// pickers that wrap a date. The row's height is all these can take from a
    /// sheet. Each measures its parts against the turtle's whole rect and
    /// draws them from the content origin, so a vertical padding displaces the
    /// line instead of holding it off the box -- padded, the number sat on the
    /// bottom edge of its box with its two arrows split around it. They must
    /// therefore carry NO vertical padding, and that is asserted below rather
    /// than skipped, because it is the thing the next sheet would get wrong.
    const FIELD_FRAME: &[&str] = &[
        "NumberField",
        "ValueInput",
        "DateField",
        "TimeField",
        "DatePicker",
        "DateRangePicker",
    ];

    /// A field's box metrics as the sheet leaves them: the minimum height,
    /// and the padding above and below the line.
    fn field_metrics(vm: &mut ScriptVm, name: &str) -> (Option<f64>, Option<f64>, Option<f64>) {
        let widgets = vm.module(id!(widgets));
        let widget = vm
            .bx
            .heap
            .value(widgets, LiveId::from_str(name).into(), NoTrap)
            .as_object()
            .unwrap_or_else(|| panic!("the library has no widget named {name}"));
        let min = vm.bx.heap.value(widget, id!(min_height).into(), NoTrap).as_f64();
        let pad = vm.bx.heap.value(widget, id!(padding).into(), NoTrap).as_object();
        let side = |vm: &mut ScriptVm, key: LiveId| {
            pad.and_then(|p| vm.bx.heap.value(p, key.into(), NoTrap).as_f64())
        };
        (min, side(vm, id!(top)), side(vm, id!(bottom)))
    }

    /// A row of fields is one height, under every sheet the library ships.
    ///
    /// The sheets used to reshape the text box alone: with the android sheet
    /// on, the catalogue's search box became a 48 point pill and the number
    /// field beside it stayed 24 tall, the drop down after it 26. Each sheet
    /// now states its field metrics once and hands them to the whole family,
    /// and this is what holds that: a field added to the library, or a sheet
    /// added to the folder, cannot quietly stand at a height of its own.
    #[test]
    fn every_field_stands_at_the_height_its_sheet_gives_the_text_box() {
        for (style, dark) in DesktopStyle::ALL
            .into_iter()
            .flat_map(|style| if style.supports_dark() { vec![(style, false), (style, true)] } else { vec![(style, false)] })
        {
            // A sheet of its own per appearance: an assignment a sheet makes
            // stays made, so sheets read one after another on one VM would
            // measure the last one that named a number, not this one.
            let mut cx = Cx::new(Box::new(|_, _| {}));
            cx.init_cx_os();
            cx.with_vm(|vm| {
                crate::script_mod(vm);
                install(vm, StyleSheet::load_with_appearance(style, dark));
                vm.bx.captured_errors = Some(Vec::new());
                vm.with_reload(crate::script_mod);
                let sheet = if dark { format!("{}-dark", style.id()) } else { style.id().to_string() };
                assert!(vm.take_errors().is_empty(), "{sheet} does not evaluate");
                let row = field_metrics(vm, "TextInput");
                assert!(row.0.is_some(), "{sheet} states no field height for the row to stand at");
                for name in FIELD_FIT {
                    assert_eq!(
                        field_metrics(vm, name),
                        row,
                        "{sheet}: {name} does not stand in the row its TextInput sets"
                    );
                }
                for name in FIELD_FRAME {
                    let field = field_metrics(vm, name);
                    assert_eq!(field.0, row.0, "{sheet}: {name} does not stand at the height of the row");
                    assert!(
                        field.1.unwrap_or(0.0) == 0.0 && field.2.unwrap_or(0.0) == 0.0,
                        "{sheet}: {name} lays its own parts out, so a vertical padding on it moves the line off the box"
                    );
                }
                // The chrome-less input inside a well inherits TextInput, and
                // a minimum as tall as the whole field, applied inside a well
                // already that tall, pushes the line out through the bottom.
                assert_eq!(
                    field_metrics(vm, "WellInput").0,
                    Some(0.0),
                    "{sheet}: the input inside a well must impose no height"
                );
            });
        }
    }

    #[test]
    fn mobile_typefaces_keep_symbol_fallbacks_across_appearances() {
        let mut cx=Cx::new(Box::new(|_,_|{}));
        cx.init_cx_os();
        cx.with_vm(|vm| {
            crate::script_mod(vm);
            for style in [DesktopStyle::Ios,DesktopStyle::Android] {
                for dark in [false,true] {
                    install(vm,StyleSheet::load_with_appearance(style,dark));
                    vm.with_reload(crate::script_mod);
                    let value=script_eval!(vm,{mod.theme.font_regular});
                    let text=TextStyle::script_from_value(vm,value);
                    let members=text.font_family.member_ids().collect::<Vec<_>>();
                    assert_eq!(members.first(),Some(&"latin"));
                    // The mobile policy's fallback chain (font_policy.rs) ends
                    // in the emoji face; it must survive every appearance.
                    assert!(members.contains(&"noto_color_emoji"),"{members:?}");
                    assert!(vm.take_errors().is_empty());
                }
            }
        });
    }
    #[test]
    fn styles_re_evaluate_splash_without_replacing_user_text() {
        let mut cx = Cx::new(Box::new(|_, _| {}));
        cx.with_vm(|vm| {
            crate::script_mod(vm);
            let value = script_eval!(vm,{use mod.widgets.* Label{text:"original"}});
            let mut label = Label::script_from_value(vm, value);
            label.set_text(vm.cx_mut(), "edited document");
            for style in DesktopStyle::ALL {
                install(vm, StyleSheet::load(style));
                vm.with_reload(crate::script_mod);
                let errors = vm.take_errors();
                assert!(errors.is_empty(), "{}: {:?}", style.id(), errors);
                let theme = vm.module(id!(theme));
                let radius = vm
                    .bx
                    .heap
                    .value(theme, id!(corner_radius).into(), NoTrap)
                    .as_f64()
                    .unwrap();
                assert_eq!(
                    radius,
                    match style {
                        DesktopStyle::Macos => 6.0,
                        DesktopStyle::Windows => 4.0,
                        DesktopStyle::Ios => 14.0,
                        DesktopStyle::Android => 20.0,
                        DesktopStyle::BlackOrange => 2.5,
                        DesktopStyle::Neumorphic => 8.0,
                        DesktopStyle::Molded => 5.0,
                        DesktopStyle::Glossy => 6.0,
                        DesktopStyle::Milled => 3.0,
                        DesktopStyle::Aluminium => 2.0,
                        DesktopStyle::Frosted | DesktopStyle::Liquid | DesktopStyle::Luminous => 3.0,
                        DesktopStyle::FieldKit => 1.5,
                        DesktopStyle::Neon => 2.0,
                        _ => 0.0,
                    }
                );
                let value = script_eval!(vm,{use mod.widgets.* Label{text:"original"}});
                label.script_apply(vm, &Apply::ScriptReapply, &mut Scope::empty(), value);
                assert_eq!(label.text(), "edited document");
            }
        });
    }
    #[test]
    fn modern_dark_modes_hotload_component_geometry_and_tokens() {
        let mut cx = Cx::new(Box::new(|_, _| {}));
        cx.with_vm(|vm| {
            crate::script_mod(vm);
            for (style, dark) in [DesktopStyle::Macos, DesktopStyle::Windows, DesktopStyle::Ios, DesktopStyle::Android].into_iter().flat_map(|s| [false, true, false].map(|d|(s,d))) {
                install(
                    vm,
                    StyleSheet::load_with_appearance(style, dark),
                );
                vm.bx.captured_errors = Some(Vec::new());
                vm.with_reload(crate::script_mod);
                assert!(vm.take_errors().is_empty());
                let theme = vm.module(id!(theme));
                assert_eq!(
                    vm.bx
                        .heap
                        .value(theme, id!(color_bg_app).into(), NoTrap)
                        .as_color(),
                    Some(match (style, dark) {
                        (DesktopStyle::Macos, true) => 0x28282aff, (DesktopStyle::Macos, false) => 0xecececff,
                        (DesktopStyle::Ios, true) => 0x000000ff, (DesktopStyle::Ios, false) => 0xf2f2f7ff,
                        (DesktopStyle::Android, true) => 0x141218ff, (DesktopStyle::Android, false) => 0xfef7ffff,
                        (_, true) => 0x202020ff, (_, false) => 0xf3f3f3ff
                    })
                );
                let widgets = vm.module(id!(widgets));
                let button = vm
                    .bx
                    .heap
                    .value(widgets, id!(Button).into(), NoTrap)
                    .as_object()
                    .unwrap();
                let draw = vm
                    .bx
                    .heap
                    .value(button, id!(draw_bg).into(), NoTrap)
                    .as_object()
                    .unwrap();
                assert_eq!(
                    vm.bx
                        .heap
                        .value(draw, id!(border_radius).into(), NoTrap)
                        .as_f64(),
                    Some(match style {DesktopStyle::Macos=>3.0,DesktopStyle::Ios=>22.0,DesktopStyle::Android=>24.0,_=>4.0})
                );
            }
        });
    }
    #[test]
    fn style_reapply_preserves_panel_visibility() {
        let mut cx = Cx::new(Box::new(|_, _| {}));
        cx.with_vm(|vm| {
            crate::script_mod(vm);
            let original = script_eval!(vm, {use mod.widgets.* View{visible: false}});
            let mut panel = View::script_from_value(vm, original);
            panel.visible = true;
            install(vm, StyleSheet::load(DesktopStyle::Windows));
            vm.with_reload(crate::script_mod);
            panel.script_apply(vm, &Apply::ScriptReapply, &mut Scope::empty(), original);
            assert!(panel.visible, "An open panel must survive a style change");
            panel.script_apply(vm, &Apply::Eval, &mut Scope::empty(), original);
            assert!(!panel.visible, "Explicit visibility edits must still apply");
        });
    }
    /// A table indexed by discriminant is `COUNT` long, and every variant
    /// is in `ALL` exactly once, so no style falls off the end of one.
    #[test]
    fn every_style_is_listed_once_and_fits_the_count() {
        for (at, style) in DesktopStyle::ALL.into_iter().enumerate() {
            assert!((style as usize) < DesktopStyle::COUNT, "{style:?}");
            assert_eq!(DesktopStyle::ALL.iter().position(|s| *s == style), Some(at), "{style:?} twice");
            assert_eq!(DesktopStyle::parse(style.id()), Some(style));
        }
        // The styles built on the library's own surfaces tile, as the
        // tilers do; only the desktops modelled on somebody else's float.
        for style in DesktopStyle::TILING {
            assert!(!style.floating(), "{style:?} floats");
            assert_eq!(style.shelf_height(), 0.0);
            assert_eq!(style.title_height(), 0.0);
        }
    }

    /// The faces the library keeps for its own chrome are the stock ones
    /// under any sheet, and a template that spreads one in is out of a
    /// sheet's reach while one that does not is not.
    #[test]
    fn the_kept_faces_are_the_librarys_own_under_any_sheet() {
        let mut cx = Cx::new(Box::new(|_, _| {}));
        cx.with_vm(|vm| {
            crate::script_mod(vm);
            install(vm, marker_sheet());
            vm.bx.captured_errors = Some(Vec::new());
            vm.with_reload(crate::script_mod);
            let errors = vm.take_errors();
            assert!(errors.is_empty(), "the marker sheet does not evaluate: {errors:?}");
            let widgets = vm.module(id!(widgets));
            let kept = vm.module(id!(stock_faces));
            for name in STOCK_FACES {
                for face in [id!(pixel), id!(vertex)] {
                    let sheet = vm.bx.heap.value_path(widgets, &[LiveId::from_str(name), id!(draw_bg), face], NoTrap);
                    let own = vm.bx.heap.value_path(kept, &[LiveId::from_str(name), face], NoTrap);
                    assert!(sheet.as_object().is_some() && own.as_object().is_some(), "{name}.{face} did not resolve");
                    assert_ne!(sheet, own, "the face kept for {name}.{face} is the sheet's");
                }
            }
            // The walk itself sees a face the sheet reached, or its silence
            // below would mean nothing.
            let plain = script_eval!(vm, {mod.widgets.CheckBox{}});
            assert!(!faces_reaching(vm, plain, "CheckBox").is_empty(), "the walk is blind");
            let own = script_eval!(vm, {mod.widgets.CheckBox{draw_bg +: {..mod.stock_faces.CheckBox}}});
            let leaks = faces_reaching(vm, own, "CheckBox");
            assert!(leaks.is_empty(), "{leaks:?}");
            uninstall(vm);
        });
    }

    #[test]
    fn stylesheet_wire_preserves_both_splash_phases() {
        let sheet = StyleSheet::load(DesktopStyle::Windows2000);
        assert_eq!(StyleSheet::parse(&sheet.to_json()), Some(sheet));
        assert!(StyleSheet::parse("{\"wm\":\"Adopted\"}").is_none());
    }
}
