//! The theme builder: a whole theme grown from one favourite colour and a
//! handful of sliders, with the part a panel would otherwise have to remember
//! kept in one place.
//!
//! [`crate::theme_tokens`] already holds the rule. It grows seven families of
//! accent roles from a seed, it can be told the scheme the other two brand
//! families stand to the first in ([`Harmony`]), how much colour they carry
//! and where they sit on the lightness axis ([`RoleTuning`]), and how far the
//! grounds lean toward a colour (`ground_tint`); and it can build a base
//! theme again from its own source with different globals, so that spacing,
//! roundness and the type ladder genuinely move rather than being pinned one
//! rung at a time. What it leaves to its caller is what a person would call
//! the controls: a slider is not a saturation of 0.62 and three lightness
//! targets, it is "accents only" at one end and "saturated" at the other.
//!
//! So this module is two things. [`build`] is the translation, and it is
//! pure: [`BuilderParams`] in, a [`BuiltTheme`] out -- the one script to
//! evaluate, the roles, and how the result reads -- with no VM anywhere, which
//! is what lets a test build four thousand themes and hold every one of them
//! to the readability bar. [`ThemeBuilder`] is the panel's side of it, and it
//! reads as a sentence the way [`crate::theme_lab::ThemeLab`] does: make one,
//! [`ThemeBuilder::enter`], [`ThemeBuilder::set`], [`ThemeBuilder::apply`],
//! [`ThemeBuilder::leave`].
//!
//! # How a built theme is seen at all
//!
//! Through the lab's seam, and for the lab's reason. A widget does not read
//! `mod.theme` when it draws: its colour was baked into its template as a
//! literal when the app's own module block ran, so nothing short of
//! re-running that block moves an app, and a theme evaluated once into the
//! module would be thrown away by the very run that was meant to show it. So
//! the script is written onto the `Cx` with [`crate::set_theme_mix`], where
//! `theme_mod` emits it on every run -- after the themes exist, before a
//! template has read one -- and the run is asked for with
//! `cx.request_style_reload()`. Read the lab's module doc for the long form;
//! none of it is different here.
//!
//! The seam holds ONE script. That makes the builder and the lab mutually
//! exclusive, which is right -- a theme cannot be both a mix of fifteen and a
//! palette from a favourite colour -- and it is made well-defined rather than
//! left to chance:
//!
//! * Entering the builder while a mix is up captures the mix as part of what
//!   was in force. Leaving puts it back, text for text, and the lab, which
//!   still believes it installed that mix, is right again.
//! * Whoever applies last wins the screen. A builder that finds some other
//!   script where it left its own has been stood down -- by the lab, by the
//!   app's own picker, by anything -- and does what the lab does about it:
//!   it opens again on what is now in force, installs nothing, and hands THAT
//!   back when it is left.
//! * The lab, for its part, notices only a seam that has gone EMPTY, so a lab
//!   left open under a builder goes on believing in its mix until its weights
//!   move, at which point its apply takes the screen back and the builder is
//!   the one stood down. Nothing is lost either way; nothing is both.
//!
//! A style sheet comes off while a built theme is up, as it does under a mix
//! and for the same reason: a sheet's own script runs after the seam and
//! points `mod.theme` back at its base over the top of whatever the seam put
//! there. Leaving puts the sheet back.
//!
//! The builder never touches the base theme choice and never touches a
//! person's token edits. It does not need to -- its script names the base it
//! derives from outright -- and `set_base_theme` is a picker's call that
//! clears both the seam and the edits, which is more than a slider move has
//! any business doing.
//!
//! # What the globals cost
//!
//! A theme whose spacing, roundness, type or ground tint has moved is built
//! again from the base theme's SOURCE, because only that re-derives the
//! ladders. That object is new, and `font_policy::install_theme_fonts` ran
//! before the seam, over the objects that existed then -- so the script
//! carries the installed font families across by name (`CARRIED_FONTS`), and
//! a test holds that list to the installer. A theme that only moved its
//! palette skips all of this and derives straight from the base object.

use crate::desktop_style::{self, DesktopStyle, StyleSheet};
use crate::makepad_platform::{LiveId, NoTrap, ScriptMod, ScriptObject, ScriptVm, ScriptVmCx};
use crate::theme_combinations::COMBINATIONS;
use crate::theme_tokens::{
    base_theme_keys, ground_tint, held_pairs, hsl_to_rgb, reads_on, rgb_to_hsl, roles_from_seed_tuned,
    theme_module_script, theme_script_body, theme_source_with_globals, token_spec, Appearance, BlendTheme,
    FamilyTargets, RoleSource, DERIVED_ROLES, KEPT_ERROR, KEPT_WARNING,
};
use crate::BaseTheme;
use std::collections::BTreeMap;

/// Everything the builder's own signatures name, so that a panel has one
/// module to import from. [`Readability`] is the lab's own type and not a
/// copy of it: a panel that shows how a mix reads can show how a built theme
/// reads with the same code.
pub use crate::theme_lab::Readability;
pub use crate::theme_tokens::{ColorRoles, Harmony, RoleTuning, Scheme, SeedColors, TokenValue};

/// The key a built theme is filed under in `mod.themes`. One name, re-used on
/// every apply, so a session leaves one derived object behind rather than one
/// per slider move.
pub const BUILT_NAME: &str = "built";

/// The key the re-derived base is filed under, where a build needs one. Not
/// the base's own name: `mod.themes.dark` stays the theme the library ships,
/// for the font families to be carried from and for a sheet export to be
/// measured against.
const BUILT_SOURCE_NAME: &str = "built_source";

/// The names an export evaluates its scratch copy under, so that asking for a
/// sheet never moves the theme that is on the screen.
const EXPORT_NAME: &str = "built_export";
const EXPORT_SOURCE_NAME: &str = "built_export_source";

/// How much colour the primary family carries with the saturation slider at
/// the far end. The house rule is 0.62; full saturation is a highlighter pen,
/// and the secondary and tertiary keep their shares of whatever this is.
pub const BRAND_SATURATED: f64 = 0.90;

/// Where the brightness slider's two ends put each scheme's families. The
/// middle of the slider is `RoleTuning::HOUSE`, exactly.
const LIGHT_DIM: FamilyTargets = FamilyTargets { base: 0.30, container: 0.84, ink: 0.08 };
const LIGHT_BRIGHT: FamilyTargets = FamilyTargets { base: 0.50, container: 0.95, ink: 0.16 };
const DARK_DIM: FamilyTargets = FamilyTargets { base: 0.64, container: 0.22, ink: 0.12 };
const DARK_BRIGHT: FamilyTargets = FamilyTargets { base: 0.84, container: 0.38, ink: 0.24 };

/// The font styles `font_policy::install_theme_fonts` derives into each base
/// theme. A theme built again from source is a new object that the installer
/// never saw, so its script carries these across from the base by name.
/// `the_carried_fonts_are_the_ones_the_installer_installs` reads the
/// installer's text and fails when the two lists part.
const CARRIED_FONTS: [&str; 11] = [
    "font_label",
    "font_regular",
    "font_bold",
    "font_italic",
    "font_bold_italic",
    "font_regular_i18n",
    "font_bold_i18n",
    "font_italic_i18n",
    "font_bold_italic_i18n",
    "font_code",
    "font_icons",
];

/// The four globals the dimension sliders drive, with the field of
/// [`BuilderParams`] each one is.
const DIMENSIONS: [&str; 4] = ["space_factor", "corner_radius", "font_size_base", "font_size_contrast"];

/// How far a theme file mixes its page from one end toward the other, as
/// `(color_bg_app, color_fg_app)`: the dark file from black toward white, the
/// light file from white toward black. Each is the base of a `pow` whose
/// exponent is `color_contrast`. A second copy of four numbers the theme
/// files own, kept because a reading with no VM in it has to know where the
/// page lands; `the_grounds_are_mixed_the_way_the_theme_files_mix_them` holds
/// the copy to the files' text, and
/// `what_build_predicts_is_what_the_vm_resolves` holds the answer to the VM.
const DARK_GROUNDS: (f64, f64) = (0.3, 0.36);
const LIGHT_GROUNDS: (f64, f64) = (0.15, 0.175);

/// The two rungs of the opaque ladder the inverse page is made of, as the
/// amount `color_fg_app` is mixed toward white and toward black. Held to the
/// files by the same test.
const OPAQUE_U_6: f64 = 0.8;
const OPAQUE_D_5: f64 = 0.75;

const WHITE: u32 = 0xFFFFFFFF;
const BLACK: u32 = 0x000000FF;

/// What a panel's controls say, in the units the controls are in.
///
/// Nothing here is a token. The favourite colour is a colour; the two
/// character sliders run nought to one; the four dimensions are in the units
/// of the globals they drive, because a person dragging "font size" wants to
/// read 11 and not 0.37.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BuilderParams {
    /// The one colour somebody likes, `0xRRGGBBAA`. Read for its hue; the
    /// alpha is ignored, and a grey grows a palette of greys.
    pub favourite: u32,
    /// Where the secondary and tertiary families sit round the hue circle
    /// from the favourite.
    pub harmony: Harmony,
    /// The companion colours a [`Suggestion`] was picked with, naming the
    /// secondary, the tertiary and the page's lean outright rather than
    /// leaving all three to the harmony.
    ///
    /// `None` is the default and is every setting the controls alone can
    /// reach, so a build that names nothing here is the theme the builder
    /// made before there were suggestions, byte for byte.
    ///
    /// A seed named outright beats the harmony -- which is what `SeedColors`
    /// has always said of one -- so choosing a harmony by hand has to let
    /// these go or it would do nothing at all. [`BuilderParams::with_harmony`]
    /// is that move; a panel that writes the `harmony` field itself owes the
    /// same clearing.
    ///
    /// Only the HUE of a named companion is used. The rule brings its own
    /// saturation and lightness, as it does for a hue it worked out from an
    /// offset, which is why a suggestion cannot make a theme that fails to
    /// read. What the mood did to the swatch's saturation and lightness
    /// reaches the theme through the two character sliders instead, which is
    /// where a whole palette's colour and brightness live.
    pub seeds: Option<SuggestionSeeds>,
    /// Accents only at nought, saturated at one. Nought is the house theme:
    /// brand colour at the house strength and a page with no colour in it at
    /// all. Toward one the brand families take on more colour
    /// ([`BRAND_SATURATED`]) and the grounds lean toward the favourite's hue
    /// (`theme_tokens::ground_tint`).
    pub saturation: f64,
    /// Dim at nought, bright at one, the house targets at a half, exactly.
    /// Moves where every family sits on the lightness axis, in the direction
    /// the word says in both appearances.
    pub brightness: f64,
    /// A dark page or a light one. The skeleton is not offered: it has no
    /// brand to build.
    pub dark: bool,
    /// `space_factor`: the unit every space rung and control height is a
    /// multiple of.
    pub spacing: f64,
    /// `corner_radius`.
    pub roundness: f64,
    /// `font_size_base`: the paragraph size.
    pub font_size: f64,
    /// `font_size_contrast`: the step between neighbouring sizes.
    pub font_contrast: f64,
}

impl BuilderParams {
    /// The theme the library ships, in one appearance: the house colour in
    /// the house harmony, both sliders where the house rule is, and the four
    /// dimensions as the base theme's own file has them -- read off the file,
    /// not remembered, so a change to a theme file moves the builder's idea
    /// of "untouched" with it.
    pub fn house(dark: bool) -> Self {
        let scheme = if dark { Scheme::Dark } else { Scheme::Light };
        let of = |key: &str, otherwise: f64| file_number(scheme, key).unwrap_or(otherwise);
        Self {
            favourite: SeedColors::HOUSE.primary,
            harmony: Harmony::House,
            seeds: None,
            saturation: 0.0,
            brightness: 0.5,
            dark,
            spacing: of("space_factor", 6.0),
            roundness: of("corner_radius", 2.5),
            font_size: of("font_size_base", 10.0),
            font_contrast: of("font_size_contrast", 2.5),
        }
    }

    /// The base theme a build derives from.
    pub fn scheme(&self) -> Scheme {
        if self.dark {
            Scheme::Dark
        } else {
            Scheme::Light
        }
    }

    /// Every number brought inside what it can mean: the two sliders into
    /// nought to one, and each dimension into the range its global is
    /// registered with in `THEME_TOKENS`, which is the range the token's own
    /// control offers. [`build`] does this for itself; it is public so that a
    /// panel can show the number that will actually be used.
    pub fn clamped(self) -> Self {
        let within = |key: &str, value: f64| match token_spec(key) {
            Some(spec) if spec.max > spec.min => value.clamp(spec.min, spec.max),
            _ => value,
        };
        let unit = |value: f64| if value.is_finite() { value.clamp(0.0, 1.0) } else { 0.0 };
        Self {
            saturation: unit(self.saturation),
            brightness: if self.brightness.is_finite() { self.brightness.clamp(0.0, 1.0) } else { 0.5 },
            spacing: within("space_factor", self.spacing),
            roundness: within("corner_radius", self.roundness),
            font_size: within("font_size_base", self.font_size),
            font_contrast: within("font_size_contrast", self.font_contrast),
            ..self
        }
    }

    /// The seed the palette grows from: the favourite in its harmony, and --
    /// once the saturation slider has left nought -- a neutral of the
    /// favourite's hue with the slider's value for its saturation, which is
    /// how `SeedColors::neutral` says how far the grounds lean.
    ///
    /// A suggestion's [`seeds`](BuilderParams::seeds) name the two companions
    /// and which way the page leans instead. The slider goes on saying how
    /// FAR it leans even then: the named lean is what the page wears at the
    /// slider's far end, and it is scaled down from there, so the control
    /// still runs from a page with no colour in it to a page with as much as
    /// the appearance can take.
    pub fn seed(&self) -> SeedColors {
        let seed = SeedColors::from_favourite(self.favourite, self.harmony);
        let (hue, colour, _) = rgb_to_hsl(self.favourite | 0xFF);
        let saturation = self.clamped().saturation;
        if let Some(named) = self.seeds {
            let seed = SeedColors {
                secondary: Some(named.secondary | 0xFF),
                tertiary: Some(named.tertiary | 0xFF),
                ..seed
            };
            return seed.with_neutral(leaning(named.neutral, saturation));
        }
        // A grey favourite has no hue for the page to lean toward, by the
        // same bar the rule uses to decide the brand families are greys.
        if saturation > 0.0 && colour >= 0.08 {
            seed.with_neutral(Some(hsl_to_rgb(hue, saturation, 0.5)))
        } else {
            seed
        }
    }

    /// The same settings in another harmony, with any companions a suggestion
    /// named let go.
    ///
    /// Both halves matter. A harmony only decides the hues the rule works out
    /// for itself, so setting one over a suggestion's named seeds would move
    /// a picker and leave the palette exactly where it was -- the one thing a
    /// control must never do.
    pub fn with_harmony(self, harmony: Harmony) -> Self {
        Self { harmony, seeds: None, ..self }
    }

    /// The numbers the rule is run with. Written so that the house settings
    /// come out as `RoleTuning::HOUSE` to the last bit and not merely near
    /// it: each target is the house value plus a distance times how far the
    /// slider is from the middle, and at the middle that is the house value
    /// plus nought.
    pub fn tuning(&self) -> RoleTuning {
        let p = self.clamped();
        let house = RoleTuning::HOUSE;
        let slide = |house: f64, dim: f64, bright: f64| {
            if p.brightness < 0.5 {
                house + (dim - house) * ((0.5 - p.brightness) / 0.5)
            } else {
                house + (bright - house) * ((p.brightness - 0.5) / 0.5)
            }
        };
        let targets = |house: FamilyTargets, dim: FamilyTargets, bright: FamilyTargets| FamilyTargets {
            base: slide(house.base, dim.base, bright.base),
            container: slide(house.container, dim.container, bright.container),
            ink: slide(house.ink, dim.ink, bright.ink),
        };
        RoleTuning {
            brand: house.brand + (BRAND_SATURATED - house.brand) * p.saturation,
            light: targets(house.light, LIGHT_DIM, LIGHT_BRIGHT),
            dark: targets(house.dark, DARK_DIM, DARK_BRIGHT),
        }
    }

    fn dimension(&self, key: &str) -> f64 {
        match key {
            "space_factor" => self.spacing,
            "corner_radius" => self.roundness,
            "font_size_base" => self.font_size,
            _ => self.font_contrast,
        }
    }
}

impl Default for BuilderParams {
    /// The house theme on a dark page, which is the base theme an app gets
    /// when it never chooses one. [`ThemeBuilder::enter`] starts from the
    /// appearance actually in force instead.
    fn default() -> Self {
        Self::house(true)
    }
}

/// A number a base theme's file gives one of its top-level keys, read off the
/// file's text: `space_factor: 6. // Increase for a less dense layout` is 6.
/// `None` for a key the file derives rather than states.
fn file_number(scheme: Scheme, key: &str) -> Option<f64> {
    let opening = format!("        {key}:");
    scheme.source().lines().find_map(|line| {
        let rest = line.strip_prefix(&opening)?;
        rest.split("//").next()?.trim().parse::<f64>().ok()
    })
}

/// A script for a set of pins whose DIMENSIONS moved, or `None` where none
/// of them did and plain pins say everything.
///
/// A pin can carry a colour and a number and nothing else. That is the whole
/// of a palette, and it is most of a layout -- `space_1..6` and every type
/// size are numbers -- but the insets and the text styles are OBJECTS the
/// base file derives from those numbers, and a pin on `space_factor` over
/// the base object leaves every one of them where the library put it. So a
/// theme saved with its spacing at twelve came back with twelve written on
/// it and six in every margin, which is a different theme from the one on
/// screen when the button was pressed.
///
/// The answer is the one [`BuiltTheme::script_as`] already gives: build the
/// base again from its own source with the moved dimensions in place of the
/// file's, carry the installed fonts across by name, and pin over THAT. Only
/// the four dimensions are asked about. The other globals are colour knobs,
/// and a set of pins that came from a snapshot names every colour outright.
///
/// For a theme with no sheet under it only: a sheet assigns into the base
/// object by name, fonts and all, and a base built again under another name
/// is one the sheet never wrote to. The caller checks.
pub(crate) fn rederived_pin_script(
    name: &str,
    scheme: Scheme,
    pins: &[(String, TokenValue)],
) -> Option<String> {
    let moved: Vec<(String, TokenValue)> = DIMENSIONS
        .iter()
        .filter_map(|key| {
            let (_, value) = pins.iter().find(|(pinned, _)| pinned == key)?;
            // A number however it was held: a theme read back from its file
            // has `Num` where the one that was written had `Raw`, and the two
            // are one theme and must be one script.
            let number = match value {
                TokenValue::Num(number) => *number,
                TokenValue::Raw(text) => text.trim().parse::<f64>().ok()?,
                _ => return None,
            };
            match file_number(scheme, key) {
                Some(house) if (house - number).abs() < 1e-9 => None,
                _ => Some((key.to_string(), TokenValue::Num(number))),
            }
        })
        .collect();
    if moved.is_empty() {
        return None;
    }
    let source_name = format!("{name}_source");
    let source = theme_source_with_globals(&source_name, scheme.source(), &moved);
    let mut out = theme_script_body(&source);
    let mut over: Vec<(String, TokenValue)> = CARRIED_FONTS
        .iter()
        .map(|font| {
            let carried = format!("mod.themes.{}.{font}", scheme.theme_name());
            (font.to_string(), TokenValue::Raw(carried))
        })
        .collect();
    over.extend(pins.iter().cloned());
    out.push_str(&theme_module_script(name, &source_name, &over));
    Some(out)
}

/// A colour a base theme's file gives one of its top-level keys as an `#x`
/// literal, which is how every role the library generated is written.
fn file_color(scheme: Scheme, key: &str) -> Option<u32> {
    let opening = format!("        {key}: #x");
    scheme.source().lines().find_map(|line| {
        let rest = line.strip_prefix(&opening)?;
        u32::from_str_radix(rest.get(..8)?, 16).ok()
    })
}

/// The script's `mix()` on two colours: single precision, and each channel
/// cut to a byte rather than rounded, because that is what the VM does every
/// time a colour leaves an expression. `theme_tokens::mix_rgb` rounds, which
/// is the right answer and a different one, and a reading of a built theme
/// has to be a reading of the theme the VM will actually make.
fn vm_mix(a: u32, b: u32, t: f64) -> u32 {
    let t = t as f32;
    let channel = |shift: u32| {
        let x = ((a >> shift) & 0xFF) as f32 / 255.0;
        let y = ((b >> shift) & 0xFF) as f32 / 255.0;
        ((x * (1.0 - t) + y * t) * 255.0) as u8 as u32
    };
    (channel(24) << 24) | (channel(16) << 16) | (channel(8) << 8) | channel(0)
}

/// The script's `*` on two colours, channel by channel, cut the same way.
fn vm_mul(a: u32, b: u32) -> u32 {
    let channel = |shift: u32| {
        let x = ((a >> shift) & 0xFF) as f32 / 255.0;
        let y = ((b >> shift) & 0xFF) as f32 / 255.0;
        (x * y * 255.0) as u8 as u32
    };
    (channel(24) << 24) | (channel(16) << 16) | (channel(8) << 8) | channel(0)
}

/// `color_bg_app` and `color_fg_app` as the base theme's file will derive
/// them under a tint: both ends of the page multiplied by the tint mixed
/// `amount` of the way from white, then mixed by the file's own fraction.
fn grounds(scheme: Scheme, tint: u32, amount: f64) -> (u32, u32) {
    let contrast = file_number(scheme, "color_contrast").unwrap_or(1.0);
    let cast = vm_mix(WHITE, tint, amount);
    let (from, to, (bg, fg)) = match scheme {
        Scheme::Dark => (BLACK, WHITE, DARK_GROUNDS),
        _ => (WHITE, BLACK, LIGHT_GROUNDS),
    };
    let ground = |fraction: f64| vm_mix(vm_mul(from, cast), vm_mul(to, cast), fraction.powf(contrast));
    (ground(bg), ground(fg))
}

/// A theme ready to install, measure or export. Made by [`build`].
#[derive(Clone, Debug, PartialEq)]
pub struct BuiltTheme {
    /// The settings this was built from, clamped.
    pub params: BuilderParams,
    /// The base theme it derives from.
    pub scheme: Scheme,
    /// The seed and the tuning the settings came to, for a caller that wants
    /// to run the rule itself.
    pub seed: SeedColors,
    pub tuning: RoleTuning,
    /// The accent roles, all seven families.
    pub roles: ColorRoles,
    /// The globals that differ from the base theme's own file, in the order a
    /// theme file lists them. Empty for a theme that only moved its palette,
    /// and then the script derives from the base object and re-derives
    /// nothing.
    pub globals: Vec<(String, TokenValue)>,
    /// Every token the script pins over the (re-derived) base: the roles --
    /// all but `color_error` and `color_warning`, which every theme keeps as
    /// its older red and amber -- and either surface ink the tinted page
    /// asked to be changed.
    pub overrides: Vec<(String, TokenValue)>,
    /// The one script to evaluate, ending in the `true` it can afford to
    /// lose. What [`ThemeBuilder::apply`] writes onto the `Cx`.
    pub script: String,
    /// How the theme reads, over every pair the library holds its own themes
    /// to.
    pub readability: Readability,
    /// Every colour the reading was taken over, by token.
    colors: BTreeMap<String, u32>,
}

impl BuiltTheme {
    /// A colour of the built theme, for a swatch or a preview: any accent
    /// role, the page and its ladder, the inks, the inverse page. `None` for
    /// a token the builder neither writes nor measures.
    pub fn color(&self, key: &str) -> Option<u32> {
        self.colors.get(key).copied()
    }

    /// The script under other names, with or without the line that wears the
    /// theme. [`BuiltTheme::script`] is this under [`BUILT_NAME`], worn.
    fn script_as(&self, source_name: &str, name: &str, wear: bool) -> String {
        let mut out = String::new();
        let mut overrides: Vec<(String, TokenValue)> = Vec::new();
        let base = if self.globals.is_empty() {
            self.scheme.theme_name()
        } else {
            let source = theme_source_with_globals(source_name, self.scheme.source(), &self.globals);
            out.push_str(&theme_script_body(&source));
            for font in CARRIED_FONTS {
                let carried = format!("mod.themes.{}.{font}", self.scheme.theme_name());
                overrides.push((font.to_string(), TokenValue::Raw(carried)));
            }
            source_name
        };
        overrides.extend(self.overrides.iter().cloned());
        let script = theme_module_script(name, base, &overrides);
        if wear {
            out.push_str(&script);
        } else {
            out.push_str(&script.replace(&format!("mod.theme = mod.themes.{name}\n"), ""));
        }
        out
    }

    /// A complete theme file for this theme, under `name`: the base theme's
    /// own source with the moved globals and every pinned token written in as
    /// literals and everything else still an expression, so the file
    /// re-derives exactly as the built theme does. `theme_keys` reads it
    /// back, and it has every key the base has.
    ///
    /// This and not `export_theme_source`, which writes a snapshot from a
    /// flat list of values: the builder has the rule and not the values, and
    /// a theme made of only the forty tokens it pinned would be a theme with
    /// no fonts and no spacing in it.
    ///
    /// The Rust around the script comes along as the base file has it, up to
    /// the end of its `script_mod!` and no further -- the base file's own
    /// tests are not part of anybody's theme. `name` has to be an identifier
    /// (`theme_store::normalize_name` makes one); anything else comes back as
    /// `None` rather than as a file that will not parse.
    pub fn theme_source(&self, name: &str) -> Option<String> {
        if !crate::theme_store::is_valid_name(name) {
            return None;
        }
        let mut literals = self.globals.clone();
        literals.extend(self.overrides.iter().cloned());
        let whole = theme_source_with_globals(name, self.scheme.source(), &literals);
        let mut out = String::with_capacity(whole.len());
        let mut inside = false;
        for line in whole.lines() {
            out.push_str(line);
            out.push('\n');
            if line.starts_with("script_mod! {") {
                inside = true;
            } else if inside && line == "}" {
                break;
            }
        }
        Some(out)
    }

    /// A style sheet for this theme, in the flat shape of the ones under
    /// `widgets/themes`.
    ///
    /// A sheet cannot re-derive: it is a list of assignments, and one that
    /// set `space_factor` and stopped would move a single token. So the theme
    /// is evaluated -- under scratch names, without the line that wears it,
    /// so the screen does not move -- and read back, and the sheet assigns
    /// every colour and number that came out different from the base the
    /// sheet's first line names. That is the whole ladder, as literals.
    ///
    /// Two kinds of token are written that a plain diff would miss. The
    /// roles go in whether or not they moved, because a role a sheet does NOT
    /// name is regrown for it (`sheet_roles_script`) from the sheet's accent
    /// token, which in a base theme is a focus blue and would turn this
    /// palette blue. And the tokens that are neither a colour nor a number
    /// but are made of ones that moved -- the `Inset` of `mspace_2`, the
    /// `TextStyle` of `font_title_l` -- are written from the base file's own
    /// expression with the numbers put in, so that a sheet which says
    /// "roomier" also moves the padding.
    ///
    /// The panel calls this inside `cx.with_vm`. It costs one evaluation of a
    /// theme file, a few milliseconds, and leaves two scratch objects in
    /// `mod.themes` until the next module run sweeps them away.
    pub fn sheet_source(&self, vm: &mut ScriptVm) -> String {
        vm.eval(ScriptMod {
            cargo_manifest_path: env!("CARGO_MANIFEST_DIR").into(),
            module_path: "theme_builder".to_string(),
            file: format!("{EXPORT_NAME}.splash"),
            line: 0,
            column: 0,
            code: self.script_as(EXPORT_SOURCE_NAME, EXPORT_NAME, false),
            values: vec![],
        });
        let themes = vm.module(LiveId::from_str("themes"));
        let object = |vm: &mut ScriptVm, name: &str| {
            vm.bx.heap.value(themes, LiveId::from_str(name).into(), NoTrap).as_object()
        };
        let (Some(built), Some(base)) = (object(vm, EXPORT_NAME), object(vm, self.scheme.theme_name())) else {
            return crate::theme_tokens::export_sheet_source(self.scheme, &[]);
        };
        let read = |vm: &mut ScriptVm, theme: ScriptObject, key: &str| {
            let value = vm.bx.heap.value(theme, LiveId::from_str(key).into(), NoTrap);
            match (value.as_color(), value.as_number()) {
                (Some(rgba), _) => Some(TokenValue::Color(rgba)),
                (_, Some(number)) => Some(TokenValue::Num(number)),
                _ => None,
            }
        };
        let mut values: Vec<(String, TokenValue)> = Vec::new();
        let mut moved: Vec<&str> = Vec::new();
        for key in base_theme_keys() {
            let Some(mine) = read(vm, built, key) else {
                continue;
            };
            let theirs = read(vm, base, key);
            if theirs.as_ref() != Some(&mine) {
                moved.push(key);
            }
            if theirs.as_ref() != Some(&mine) || is_role(key) {
                values.push((key.to_string(), mine));
            }
        }
        // The tokens made OF the ones that moved. One line each in the base
        // file, an `Inset{...}` or a font style derived from another; the
        // expression is the file's own, with every `theme.x` that is a number
        // or a colour replaced by what it came to and every other one pointed
        // at the object the sheet is assigning into.
        for line in self.scheme.source().lines() {
            let Some(rest) = line.strip_prefix("        ") else {
                continue;
            };
            let Some((key, value)) = rest.split_once(": ") else {
                continue;
            };
            let sheet_can_say = value.starts_with("Inset{") || value.starts_with("theme.");
            if !sheet_can_say || !value.trim_end().ends_with('}') || !key.chars().all(is_key_char) {
                continue;
            }
            if read(vm, built, key).is_some() || !moved.iter().any(|m| mentions(value, m)) {
                continue;
            }
            let mut text = String::new();
            let mut rest = value.trim_end();
            while let Some(at) = find_theme_ref(rest) {
                text.push_str(&rest[..at]);
                let name_at = at + "theme.".len();
                let end = rest[name_at..].find(|c: char| !is_key_char(c)).map_or(rest.len(), |e| name_at + e);
                let name = &rest[name_at..end];
                match read(vm, built, name) {
                    Some(literal) => text.push_str(&literal.render()),
                    None => text.push_str(&format!("mod.theme.{name}")),
                }
                rest = &rest[end..];
            }
            text.push_str(rest);
            if let Some(inset) = text.strip_prefix("Inset{") {
                text = format!("mod.turtle.Inset{{{inset}");
            }
            values.push((key.to_string(), TokenValue::Raw(text)));
        }
        crate::theme_tokens::export_sheet_source(self.scheme, &values)
    }
}

fn is_key_char(c: char) -> bool {
    c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_'
}

/// Where the next `theme.<key>` starts in an expression, not counting one
/// that is the tail of a longer path.
fn find_theme_ref(text: &str) -> Option<usize> {
    let mut from = 0;
    while let Some(found) = text[from..].find("theme.") {
        let at = from + found;
        let standalone = text[..at].chars().next_back().is_none_or(|c| !(is_key_char(c) || c == '.'));
        if standalone {
            return Some(at);
        }
        from = at + "theme.".len();
    }
    None
}

/// Whether an expression reads `theme.<key>`, and not a longer key that
/// merely starts the same way.
fn mentions(text: &str, key: &str) -> bool {
    let needle = format!("theme.{key}");
    text.match_indices(&needle).any(|(at, _)| !text[at + needle.len()..].starts_with(is_key_char))
}

/// Whether a token is one a sheet must name for itself or have regrown for
/// it: an accent role, a derived surface, or one of the two surface inks.
fn is_role(key: &str) -> bool {
    DERIVED_ROLES.iter().any(|(role, _, _)| *role == key)
        || key == "color_on_surface"
        || key == "color_on_surface_variant"
        || crate::theme_tokens::roles_for(Scheme::Light).entries().iter().any(|(role, _)| *role == key)
}

/// Build a theme from a panel's settings. Pure: the same settings are the
/// same theme, script and all, and no VM is touched.
///
/// The palette is `roles_from_seed_tuned` over [`BuilderParams::seed`] and
/// [`BuilderParams::tuning`], so every ink in it has been through
/// `readable_on` and reads whatever the settings are. The grounds are the
/// base theme's own, leant toward the favourite by the two tint globals; and
/// because the base theme chose its surface inks against a page with no
/// colour in it, both inks are put to every rung of the tinted ladder here
/// and replaced by the plainer of black and white where they fall short,
/// which is the rule a style sheet's inks already go through. The reading
/// that comes back is over the theme AFTER that repair, and
/// `every_built_theme_reads` holds it to the bar for every hue, both ends of
/// both sliders and both appearances.
pub fn build(params: &BuilderParams) -> BuiltTheme {
    let params = params.clamped();
    let scheme = params.scheme();
    let seed = params.seed();
    let tuning = params.tuning();
    let roles = roles_from_seed_tuned(&seed, scheme, &tuning);

    // The globals, in file order, and only the ones that moved: a build with
    // none derives from the base object, keeps its fonts for free, and costs
    // a slider drag nothing but forty literals.
    let mut globals: Vec<(String, TokenValue)> = Vec::new();
    let tint = ground_tint(&seed, scheme);
    if let Some((rgba, amount)) = tint {
        globals.push(("color_tint".to_string(), TokenValue::Color(rgba)));
        globals.push(("color_tint_amount".to_string(), TokenValue::Num(amount)));
    }
    for key in DIMENSIONS {
        let value = params.dimension(key);
        if file_number(scheme, key).is_none_or(|house| (house - value).abs() > 1e-9) {
            globals.push((key.to_string(), TokenValue::Num(value)));
        }
    }

    // Where the page lands, and everything measured that is made of it.
    let mut colors: BTreeMap<String, u32> = BTreeMap::new();
    let (tint_rgba, tint_amount) = tint.unwrap_or((WHITE, 0.0));
    let (bg, fg) = grounds(scheme, tint_rgba, tint_amount);
    colors.insert("color_bg_app".to_string(), bg);
    colors.insert("color_fg_app".to_string(), fg);
    colors.insert("color_opaque_u_6".to_string(), vm_mix(fg, WHITE, OPAQUE_U_6));
    colors.insert("color_opaque_d_5".to_string(), vm_mix(fg, BLACK, OPAQUE_D_5));
    for (role, light, dark) in DERIVED_ROLES {
        let known = |key: &str| colors.get(key).copied();
        let value = match if params.dark { *dark } else { *light } {
            RoleSource::Token(t) => known(t),
            RoleSource::HalfMix(a, b) => known(a).zip(known(b)).map(|(a, b)| vm_mix(a, b, 0.5)),
            RoleSource::MixTo(t, end, amount) => known(t).map(|c| vm_mix(c, end, amount)),
        };
        if let Some(rgba) = value {
            colors.insert(role.to_string(), rgba);
        }
    }
    for (key, rgba) in roles.entries() {
        colors.insert(key.to_string(), rgba);
    }
    colors.insert("color_error".to_string(), KEPT_ERROR);
    colors.insert("color_warning".to_string(), KEPT_WARNING);

    let mut overrides: Vec<(String, TokenValue)> = roles
        .entries()
        .into_iter()
        .filter(|(key, _)| *key != "color_error" && *key != "color_warning")
        .map(|(key, rgba)| (key.to_string(), TokenValue::Color(rgba)))
        .collect();

    // The two surface inks, put to the page as it now is.
    for ink_key in ["color_on_surface", "color_on_surface_variant"] {
        let Some(chosen) = file_color(scheme, ink_key) else {
            continue;
        };
        let settled = settle_ink(&colors, ink_key, chosen);
        colors.insert(ink_key.to_string(), settled);
        if settled != chosen {
            overrides.push((ink_key.to_string(), TokenValue::Color(settled)));
        }
    }

    let readability = read_pairs(&colors);
    let mut built = BuiltTheme {
        params,
        scheme,
        seed,
        tuning,
        roles,
        globals,
        overrides,
        script: String::new(),
        readability,
        colors,
    };
    built.script = built.script_as(BUILT_SOURCE_NAME, BUILT_NAME, true);
    built
}

/// The ink to draw on every ground the library holds this ink to: the base
/// theme's own where that reaches its bar on all of them, and otherwise
/// whichever of white and black stands furthest clear of the ground it does
/// worst on. One ink for the whole ladder, because that is what the token is.
fn settle_ink(colors: &BTreeMap<String, u32>, ink_key: &str, chosen: u32) -> u32 {
    let worst = |ink: u32| {
        held_pairs()
            .iter()
            .filter(|(_, held_ink, _)| *held_ink == ink_key)
            .filter_map(|(ground, _, need)| colors.get(*ground).map(|g| reads_on(*g | 0xFF, ink) - need))
            .fold(f64::INFINITY, f64::min)
    };
    if worst(chosen) >= 0.0 {
        chosen
    } else if worst(WHITE) >= worst(BLACK) {
        WHITE
    } else {
        BLACK
    }
}

/// How a set of colours reads over every pair the library holds a theme to,
/// reported the way `ThemeLab::readability` reports a mix: each ink laid over
/// its ground before it is measured, failures worst first, and the tightest
/// pair named whether or not it fails.
fn read_pairs(colors: &BTreeMap<String, u32>) -> Readability {
    let mut out = Readability::default();
    let mut margin: Option<f64> = None;
    let mut failures: Vec<(f64, String)> = Vec::new();
    for (ground, ink, need) in held_pairs() {
        let (Some(g), Some(i)) = (colors.get(*ground), colors.get(*ink)) else {
            continue;
        };
        let stands = reads_on(*g | 0xFF, *i);
        let line = format!("{ink} on {ground} = {stands:.2}, wants {need}");
        out.measured += 1;
        if margin.is_none_or(|best| stands - need < best) {
            margin = Some(stands - need);
            out.tightest = line.clone();
        }
        if stands < *need {
            failures.push((stands - need, line));
        }
    }
    failures.sort_by(|(a, _), (b, _)| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    out.failures = failures.into_iter().map(|(_, line)| line).collect();
    out.margin = margin.unwrap_or(0.0);
    out
}

/// SplitMix64, as the equalizer's randomiser uses it and for its reason: the
/// same seed is the same sequence on every machine and every run, and the
/// builder owes nothing to a crate or the clock for it.
fn next_unit(state: &mut u64) -> f64 {
    *state = state.wrapping_add(0x9E37_79B9_7F4A_7C15);
    let mut z = *state;
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    ((z ^ (z >> 31)) >> 11) as f64 / (1u64 << 53) as f64
}

/// A theme nobody chose: pure and seeded, so the same seed is the same
/// settings here, in a test, and on the next machine.
///
/// Every field is drawn, the appearance too. The colour is drawn as a hue at
/// a strength and lightness a person might actually pick, because a favourite
/// is read for its hue and a near grey one would spend the draw on a palette
/// of greys. The dimensions are drawn round the house values and not across
/// their whole registered range: a control may be driven to a 20 point corner
/// and a 30 point paragraph, but a random theme that arrives there is a
/// broken-looking app, not a surprise.
pub fn random_params(seed: u64) -> BuilderParams {
    let mut state = seed;
    let mut draw = |low: f64, high: f64| low + (high - low) * next_unit(&mut state);
    let hue = draw(0.0, 360.0);
    let favourite = hsl_to_rgb(hue, draw(0.55, 1.0), draw(0.42, 0.62));
    let harmony = Harmony::ALL[(draw(0.0, Harmony::ALL.len() as f64) as usize).min(Harmony::ALL.len() - 1)];
    let saturation = draw(0.0, 1.0);
    let brightness = draw(0.0, 1.0);
    let dark = draw(0.0, 1.0) < 0.5;
    // Whole and half steps, which is what the sliders themselves land on.
    let mut stepped = |low: f64, high: f64| (draw(low, high) * 2.0).round() / 2.0;
    BuilderParams {
        favourite,
        harmony,
        // A surprise is a palette the rule grew, not one off a list.
        seeds: None,
        saturation,
        brightness,
        dark,
        spacing: stepped(4.0, 9.0),
        roundness: stepped(0.0, 8.0),
        font_size: stepped(9.0, 12.0),
        font_contrast: stepped(1.5, 3.5),
    }
    .clamped()
}

// ---------------------------------------------------------------------------
// Suggestions: several palettes from the one colour
// ---------------------------------------------------------------------------

/// How loudly a suggestion draws the companions of the favourite colour.
///
/// A harmony says WHERE the other two hues sit and nothing else, so the six
/// of them from one colour are six palettes of the same strength and the same
/// brightness: a person choosing among them is choosing one decision six
/// ways. The mood is the other axis, and it is the one somebody points at
/// when they say they like a palette better -- the same three hues drawn
/// quietly, or pale, or deep.
///
/// It moves three things: how much colour the companions keep beside the
/// favourite, where they sit on the lightness axis, and how far the page
/// leans toward the favourite's hue. The last two of those are what the
/// builder's own character sliders are, so a mood also says where it puts
/// them, and picking a suggestion moves them there. Without that the swatch
/// would be drawn at settings the theme was not built at, and the row would
/// be pointing at something it cannot hand over.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Mood {
    /// The companions as strong as the favourite, at its own lightness, and
    /// a page that leans well toward it.
    Vivid,
    /// Half the colour in the companions and the quietest page of the four.
    Muted,
    /// Lighter and softer: the companions lifted off the favourite's
    /// lightness, and a theme built bright.
    Pastel,
    /// Darker and stronger: the companions dropped below the favourite, and
    /// a theme built dim on a page that leans furthest.
    Deep,
}

/// What a mood is, in numbers. Private because these are the kind of numbers
/// that are argued with by looking at them, not by reading them.
struct MoodRule {
    /// The companions' saturation as a share of the favourite's.
    colour: f64,
    /// What is added to the companions' lightness.
    lift: f64,
    /// Where a pick puts [`BuilderParams::saturation`].
    saturation: f64,
    /// Where a pick puts [`BuilderParams::brightness`].
    brightness: f64,
    /// How much colour the page's lean carries at the saturation slider's far
    /// end. The slider scales it down from there.
    lean: f64,
}

impl Mood {
    pub const ALL: [Mood; 4] = [Mood::Vivid, Mood::Muted, Mood::Pastel, Mood::Deep];

    /// The second half of a suggestion's label, in the case it is read in:
    /// "Triadic, muted".
    pub fn label(self) -> &'static str {
        match self {
            Mood::Vivid => "vivid",
            Mood::Muted => "muted",
            Mood::Pastel => "pastel",
            Mood::Deep => "deep",
        }
    }

    fn rule(self) -> MoodRule {
        match self {
            Mood::Vivid => MoodRule { colour: 1.0, lift: 0.0, saturation: 0.85, brightness: 0.5, lean: 0.55 },
            Mood::Muted => MoodRule { colour: 0.5, lift: 0.0, saturation: 0.25, brightness: 0.5, lean: 0.18 },
            Mood::Pastel => MoodRule { colour: 0.65, lift: 0.18, saturation: 0.45, brightness: 0.78, lean: 0.32 },
            Mood::Deep => MoodRule { colour: 0.9, lift: -0.18, saturation: 0.7, brightness: 0.22, lean: 0.62 },
        }
    }
}

/// The shares of the primary's colour that the palette rule gives the other
/// two brand families. Copied from `theme_tokens::family_inputs` so that a
/// swatch previews the palette the rule will actually grow and not three
/// equally strong colours it never makes; a drift here costs a swatch that is
/// a shade off, which is why it is a copy and not a test.
const SECONDARY_SHARE: f64 = 0.55;
const TERTIARY_SHARE: f64 = 0.80;

/// Where a companion colour may sit on a page of each appearance. A dark page
/// cannot show a nearly black accent and a light page cannot show a nearly
/// white one, however much lift or drop the mood asked for, so the mood is
/// spent up to these and no further.
const DARK_COMPANIONS: (f64, f64) = (0.32, 0.88);
const LIGHT_COMPANIONS: (f64, f64) = (0.20, 0.76);

/// How near two colours have to be before a swatch is saying the same thing
/// twice: the largest difference on any channel. Measured in bytes and not in
/// hue, because a hue is a lie about a colour with no colour in it -- two
/// greys 120 degrees apart are one grey, and a favourite with no colour in it
/// grows the same palette in every harmony there is.
const SAME_COLOR: i32 = 8;

/// How much colour a colour needs before it has a hue worth sorting by, and
/// before a page leans toward it. Below this it is a grey said in a roundabout
/// way, and its hue is whatever rounding left behind.
const HAS_HUE: f64 = 0.08;

/// What a suggestion grown from a person's own list is called.
pub const OWN_LABEL: &str = "From your palettes";

/// What a suggestion taken from the built-in table is called, before its
/// number: see [`crate::theme_combinations`] for what the numbers are.
pub const COMBINATION_LABEL: &str = "Combination";

/// The number the first row of the built-in table carries.
pub const FIRST_COMBINATION: usize = 121;

/// How many of the built-in combinations a colour is offered at most. The
/// strip holds eight to a page and a person who has to turn nine pages to see
/// what a colour can do has been given a catalogue rather than a choice, so
/// the nearest three pages' worth are kept and the rest dropped.
pub const MOST_COMBINATIONS: usize = 24;

/// What the row at `at` in the built-in table is called.
pub fn combination_label(at: usize) -> String {
    format!("{COMBINATION_LABEL} {}", FIRST_COMBINATION + at)
}

/// How far a colour in a person's own scheme may stand from the favourite and
/// still count as the one the scheme was found by. Thirty of the sum of hue
/// in degrees and saturation and lightness in percent, which is near enough
/// that the scheme is recognisably about that colour and loose enough that a
/// colour picked by eye off a screen finds it.
pub const OWN_TOLERANCE: f64 = 30.0;

/// The companions a suggestion names, in the form [`BuilderParams`] carries
/// them.
///
/// Two colours and a lean. Only the hues are used -- see
/// [`BuilderParams::seeds`] -- and `neutral` is the lean at the saturation
/// slider's far end, not at the setting the suggestion sets.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SuggestionSeeds {
    pub secondary: u32,
    pub tertiary: u32,
    /// Which way the page leans, or `None` for a page left as the theme file
    /// has it -- which is what a favourite with no colour in it gets.
    pub neutral: Option<u32>,
}

/// One palette offered for a favourite colour: what to call it, what it looks
/// like, and what the builder needs to grow it.
#[derive(Clone, Debug, PartialEq)]
pub struct Suggestion {
    /// Whole words, the way a person would say it: "Triadic, muted", or
    /// [`OWN_LABEL`] for one of their own.
    pub label: String,
    /// The harmony it was grown in, or `None` for one off a person's own
    /// list, which is in no harmony the library names.
    pub harmony: Option<Harmony>,
    /// The mood it was grown in, or `None` for one of a person's own.
    pub mood: Option<Mood>,
    /// The four colours a swatch shows: the favourite, the secondary, the
    /// tertiary, and the PAGE -- not the lean that makes it, but the ground
    /// the theme will actually be drawn on, so that a row of swatches shows
    /// the difference between a tinted theme and an untinted one.
    pub colors: [u32; 4],
    pub seeds: SuggestionSeeds,
    /// Where picking this puts [`BuilderParams::saturation`].
    pub saturation: f64,
    /// Where picking this puts [`BuilderParams::brightness`].
    pub brightness: f64,
}

impl Suggestion {
    /// This suggestion over some settings: the favourite, the companions and
    /// both character sliders come from the suggestion, and the appearance
    /// and the four dimensions stay as they were, because a person who has
    /// set their spacing does not lose it by trying another palette.
    pub fn params(&self, base: BuilderParams) -> BuilderParams {
        BuilderParams {
            favourite: self.colors[0],
            harmony: self.harmony.unwrap_or_default(),
            seeds: Some(self.seeds),
            saturation: self.saturation,
            brightness: self.brightness,
            ..base
        }
    }
}

/// A row of palettes to choose between, all of them grown from the one
/// colour: every harmony in every mood, in an order whose every first handful
/// is varied, with the ones that came out alike dropped.
///
/// It is the same list every time for the same colour and appearance. Nothing
/// is drawn and nothing is read off a clock, because a row of swatches that
/// reshuffles itself under the pointer is a row nobody can point at twice.
///
/// The order is a round robin and not the two loops nested: taking the
/// harmonies in turn and stepping the mood along with them puts all six
/// harmonies in the first six places and all four moods in the first four, so
/// a panel that shows eight of these shows eight different ideas rather than
/// one idea in four brightnesses followed by the next.
///
/// The dropping matters most where it is least expected. A favourite with no
/// colour in it has no hue for a harmony to turn, so all six grow the same
/// four greys and twenty-four suggestions are four; without this a person who
/// picked a grey would be offered twenty-four copies of one swatch.
pub fn suggestions(favourite: u32, dark: bool) -> Vec<Suggestion> {
    let mut out: Vec<Suggestion> = Vec::with_capacity(Harmony::ALL.len() * Mood::ALL.len());
    for round in 0..Mood::ALL.len() {
        for (step, harmony) in Harmony::ALL.into_iter().enumerate() {
            let made = grown(favourite, dark, harmony, Mood::ALL[(round + step) % Mood::ALL.len()]);
            if !out.iter().any(|kept| the_same_palette(kept, &made)) {
                out.push(made);
            }
        }
    }
    out
}

/// Everything a favourite colour is offered, in the order a person meets it:
/// the palettes the rule grew, then the built-in combinations that hold a
/// colour near this one, then the person's own schemes that do.
///
/// The rule's come first because they always exist and they always cover the
/// ground -- every harmony in every mood -- and the two lists after it are
/// what a rule cannot think of. [`crate::theme_combinations`] is a book's
/// worth of combinations put together by eye; a person's own file is the
/// handful they have decided about already, and it comes last because it is
/// the shortest and the easiest to find at the end of a strip.
///
/// Every one of the three is re-anchored on the favourite exactly, given its
/// roles by [`in_role_order`], and put through the same dropping, so a scheme
/// that came out as one the rule had already offered is not offered twice --
/// and neither is a combination that came out as a person's own.
///
/// At most [`MOST_COMBINATIONS`] of the built-in matches are kept, nearest
/// first: the strip is a choice and not a catalogue.
///
/// The library ships no personal list. `own` is read from wherever the caller
/// keeps one; `theme_store::read_palettes` is where a person's own file is.
pub fn all_suggestions(favourite: u32, dark: bool, own: &[Vec<u32>]) -> Vec<Suggestion> {
    let mut out = suggestions(favourite, dark);
    let built_in = matched(favourite, COMBINATIONS.iter().copied(), OWN_TOLERANCE);
    for (at, scheme) in built_in.into_iter().take(MOST_COMBINATIONS) {
        offer(&mut out, from_scheme(favourite, dark, &scheme, combination_label(at)));
    }
    for scheme in matching_schemes(favourite, own, OWN_TOLERANCE) {
        offer(&mut out, from_scheme(favourite, dark, &scheme, OWN_LABEL.to_string()));
    }
    out
}

/// One more palette on the strip, unless it is one that is already on it.
fn offer(out: &mut Vec<Suggestion>, made: Option<Suggestion>) {
    let Some(made) = made else {
        return;
    };
    if !out.iter().any(|kept| the_same_palette(kept, &made)) {
        out.push(made);
    }
}

/// How far one colour stands from another for the purpose of finding a scheme
/// that already holds it: the three parts of a colour added up, the hue in
/// degrees and the saturation and lightness in percent.
///
/// The hue is taken the short way round the circle. A scheme holding a red at
/// 355 degrees is a scheme holding a red at 5 degrees, and a measure that
/// called those 350 apart would never find it.
pub fn color_distance(a: u32, b: u32) -> f64 {
    let (a_hue, a_colour, a_light) = rgb_to_hsl(a | 0xFF);
    let (b_hue, b_colour, b_light) = rgb_to_hsl(b | 0xFF);
    let turn = (a_hue - b_hue).rem_euclid(360.0);
    turn.min(360.0 - turn) + (a_colour - b_colour).abs() * 100.0 + (a_light - b_light).abs() * 100.0
}

/// Every scheme in `schemes` that holds a colour within `tolerance` of the
/// favourite, each one with THAT colour moved to the front and the rest in
/// the order they were written, nearest scheme first.
///
/// The front place is not decoration: it is the colour the scheme is about
/// as far as this favourite is concerned, and [`adjust_scheme`] reads every
/// other colour's offsets from it.
///
/// Any list: the library ships none, a scheme may hold two colours or four,
/// and nothing here knows where the list came from.
pub fn matching_schemes(favourite: u32, schemes: &[Vec<u32>], tolerance: f64) -> Vec<Vec<u32>> {
    matched(favourite, schemes.iter().map(|scheme| scheme.as_slice()), tolerance)
        .into_iter()
        .map(|(_, scheme)| scheme)
        .collect()
}

/// [`matching_schemes`], and where in the list each match came from -- which
/// is how a built-in combination learns the number it is called by.
///
/// Over an iterator rather than a slice because the two lists are not the same
/// shape: a person's own is a `Vec` of `Vec`s read off a file, and the
/// built-in table is rows of a `const`.
fn matched<'a>(
    favourite: u32,
    schemes: impl Iterator<Item = &'a [u32]>,
    tolerance: f64,
) -> Vec<(usize, Vec<u32>)> {
    let mut found: Vec<(f64, usize, Vec<u32>)> = Vec::new();
    for (place, scheme) in schemes.enumerate() {
        let nearest = scheme
            .iter()
            .enumerate()
            .map(|(at, colour)| (at, color_distance(favourite, *colour)))
            .min_by(|(_, a), (_, b)| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        let Some((at, distance)) = nearest else {
            continue;
        };
        if distance > tolerance {
            continue;
        }
        let mut ordered = Vec::with_capacity(scheme.len());
        ordered.push(scheme[at]);
        ordered.extend(scheme.iter().enumerate().filter(|(i, _)| *i != at).map(|(_, c)| *c));
        found.push((distance, place, ordered));
    }
    // A stable sort, so two schemes that stand equally near the favourite
    // stay in the order the person wrote them in.
    found.sort_by(|(a, _, _), (b, _, _)| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    found.into_iter().map(|(_, place, scheme)| (place, scheme)).collect()
}

/// A scheme's colours in the order the theme wants them: the anchor first,
/// then the secondary accent, the tertiary accent, and the page's tint.
///
/// A list of colours has no roles in it. Somebody typed them in some order, or
/// a book printed them in one, and that order says which was written first and
/// nothing else -- so letting it decide which colour becomes the quiet accent
/// and which the ground hands the shape of a theme to an accident of typing.
///
/// What the colours themselves say decides it instead, and it says the same
/// thing every harmony the rule grows already says:
///
/// * The page's tint is the one with LEAST colour in it, the one already
///   behaving like a ground. Only where there is one to spare: three colours
///   are the three accent families exactly, and taking one of them for the
///   page would leave the theme a family short and have it filled by rule
///   anyway, which is a worse palette than the one that was written down.
/// * Of the two accents left, the one NEARER the anchor round the circle is
///   the secondary and the farther one the tertiary -- the quiet accent beside
///   the favourite, the contrast accent across from it. That is what every
///   built-in harmony reads like, and a scheme from a list should read like
///   one rather than like whatever order it was typed in.
///
/// A colour with no colour in it has no hue, so it cannot be near anything:
/// it sorts as far from the anchor as a colour can get, which puts it in the
/// contrast place where it does the least harm -- a grey secondary beside a
/// strong favourite is a family that has quietly stopped being one. Where the
/// ANCHOR is the grey there is nothing at all to be near or far from, and then
/// written order decides, as it does for every other tie. Anything past the
/// fourth colour is left where it fell; nothing reads it.
fn in_role_order(scheme: &[u32]) -> Vec<u32> {
    if scheme.len() < 3 {
        return scheme.to_vec();
    }
    let anchor = scheme[0];
    let mut rest: Vec<u32> = scheme[1..].to_vec();
    let tint = (rest.len() >= 3).then(|| rest.remove(palest(&rest)));
    // A stable sort, so colours standing equally far round the circle keep the
    // order they were written in.
    rest.sort_by(|a, b| {
        hue_gap(anchor, *a).partial_cmp(&hue_gap(anchor, *b)).unwrap_or(std::cmp::Ordering::Equal)
    });
    let accents = rest.len().min(2);
    let mut out = Vec::with_capacity(scheme.len());
    out.push(anchor);
    out.extend(rest.drain(..accents));
    out.extend(tint);
    out.extend(rest);
    out
}

/// Which of these colours has least colour in it. The first of them where
/// several are equally pale, so a tie falls to written order.
fn palest(colors: &[u32]) -> usize {
    colors
        .iter()
        .enumerate()
        .map(|(at, packed)| (at, rgb_to_hsl(*packed | 0xFF).1))
        .min_by(|(_, a), (_, b)| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal))
        .map(|(at, _)| at)
        .unwrap_or(0)
}

/// How far round the circle a colour stands from the anchor, the short way.
///
/// Half a turn is the farthest anything can be, so that is what a colour with
/// no hue is given. An anchor with no hue makes the question meaningless
/// rather than hard, and every answer is nought: nothing is near a grey.
fn hue_gap(anchor: u32, other: u32) -> f64 {
    let (anchor_hue, anchor_colour, _) = rgb_to_hsl(anchor | 0xFF);
    if anchor_colour < HAS_HUE {
        return 0.0;
    }
    let (other_hue, other_colour, _) = rgb_to_hsl(other | 0xFF);
    if other_colour < HAS_HUE {
        return 180.0;
    }
    let turn = (anchor_hue - other_hue).rem_euclid(360.0);
    turn.min(360.0 - turn)
}

/// A scheme re-anchored on the favourite colour: the same scheme, said as
/// offsets from its first colour and then read back out from the favourite.
///
/// What a scheme knows is the SHAPE of a palette -- this much further round
/// the circle, this much paler, this much darker -- and that shape is what
/// survives being moved onto somebody else's colour. So every colour is
/// rewritten as its distance from the scheme's front colour (which
/// [`matching_schemes`] has already put there) and laid out again from the
/// favourite.
///
/// Three things happen at the edges, and they are not the same thing:
///
/// * A hue wraps, because a circle has no edges. A hue that comes out at -30
///   is 330, and that is the only answer: mirroring it to 30 -- which is what
///   subtracting from 360 does -- would turn a scheme the other way round and
///   quietly hand back a palette nobody wrote.
/// * A saturation clamps. There is nothing past nought or past full, and a
///   scheme that asked for more of either is asking for what it already has.
/// * A lightness cannot simply clamp, because two colours clamped to the same
///   end become one colour and the scheme loses a member. So when any
///   lightness leaves the range, the whole scheme's lightnesses are rescaled
///   into it together, lowest and highest mapped onto where they had to be
///   brought: the palette keeps its order and its spacing and moves as one.
///   The front colour moves with it -- it is a member of the scheme like the
///   others -- so it comes back as the favourite EXACTLY only where nothing
///   had to be brought in, which is the ordinary case.
pub fn adjust_scheme(favourite: u32, scheme: &[u32]) -> Vec<u32> {
    if scheme.is_empty() {
        return Vec::new();
    }
    let (anchor_hue, anchor_colour, anchor_light) = rgb_to_hsl(scheme[0] | 0xFF);
    let (hue, colour, light) = rgb_to_hsl(favourite | 0xFF);
    let moved: Vec<(f64, f64, f64)> = scheme
        .iter()
        .map(|member| {
            let (member_hue, member_colour, member_light) = rgb_to_hsl(*member | 0xFF);
            (
                hue + (member_hue - anchor_hue),
                (colour + (member_colour - anchor_colour)).clamp(0.0, 1.0),
                light + (member_light - anchor_light),
            )
        })
        .collect();
    let lowest = moved.iter().fold(f64::INFINITY, |low, (_, _, l)| low.min(*l));
    let highest = moved.iter().fold(f64::NEG_INFINITY, |high, (_, _, l)| high.max(*l));
    let scale = |l: f64| {
        if lowest >= 0.0 && highest <= 1.0 {
            return l;
        }
        let (low, high) = (lowest.clamp(0.0, 1.0), highest.clamp(0.0, 1.0));
        if highest - lowest <= f64::EPSILON {
            return low;
        }
        low + (l - lowest) * (high - low) / (highest - lowest)
    };
    // `hsl_to_rgb` takes the hue round the circle for itself.
    moved.into_iter().map(|(h, s, l)| hsl_to_rgb(h, s, scale(l))).collect()
}

/// One palette in one harmony and one mood, swatch and seeds together so the
/// two cannot say different things.
fn grown(favourite: u32, dark: bool, harmony: Harmony, mood: Mood) -> Suggestion {
    let rule = mood.rule();
    let (hue, colour, light) = rgb_to_hsl(favourite | 0xFF);
    let (low, high) = if dark { DARK_COMPANIONS } else { LIGHT_COMPANIONS };
    let (second, third) = harmony.offsets();
    let companion = |turn: f64, share: f64| {
        let colour = (colour * rule.colour * share).clamp(0.0, 1.0);
        hsl_to_rgb(hue + turn, colour, (light + rule.lift).clamp(low, high))
    };
    // A favourite with no colour in it has no hue for the page to lean
    // toward, by the same bar the palette rule uses to decide the brand
    // families are greys.
    let lean = hsl_to_rgb(hue, if colour >= HAS_HUE { rule.lean } else { 0.0 }, 0.5);
    let seeds = SuggestionSeeds {
        secondary: companion(second, SECONDARY_SHARE),
        tertiary: companion(third, TERTIARY_SHARE),
        neutral: Some(lean),
    };
    Suggestion {
        label: format!("{}, {}", harmony.label(), mood.label()),
        harmony: Some(harmony),
        mood: Some(mood),
        colors: [favourite | 0xFF, seeds.secondary, seeds.tertiary, page_under(dark, seeds.neutral, rule.saturation)],
        seeds,
        saturation: rule.saturation,
        brightness: rule.brightness,
    }
}

/// One scheme off a list -- a person's own or a built-in combination --
/// already matched and re-anchored, dressed as a suggestion under the name it
/// is offered by.
///
/// The roles are handed out by [`in_role_order`] and not by where a colour
/// stood in the line, and they are handed out after the re-anchoring, because
/// it is the colours the theme will actually wear that have to carry them.
///
/// A scheme may be two colours or three, and a suggestion is always four, so
/// the places it does not fill are filled the plain way -- [`Mood::Vivid`],
/// companions as strong as the favourite -- because a palette somebody wrote
/// down by hand is not one to have opinions about. The harmony and the mood
/// stay empty: a scheme off a list is in no harmony the library names, and the
/// label is what says where it came from.
fn from_scheme(favourite: u32, dark: bool, scheme: &[u32], label: String) -> Option<Suggestion> {
    let adjusted = in_role_order(&adjust_scheme(favourite, scheme));
    if adjusted.len() < 2 {
        return None;
    }
    let rule = Mood::Vivid.rule();
    let plain = grown(favourite, dark, Harmony::House, Mood::Vivid);
    let secondary = adjusted[1];
    let tertiary = adjusted.get(2).copied().unwrap_or(plain.seeds.tertiary);
    let neutral = adjusted.get(3).copied().or(plain.seeds.neutral);
    Some(Suggestion {
        label,
        harmony: None,
        mood: None,
        colors: [adjusted[0], secondary, tertiary, page_under(dark, neutral, rule.saturation)],
        seeds: SuggestionSeeds { secondary, tertiary, neutral },
        saturation: rule.saturation,
        brightness: rule.brightness,
    })
}

/// A lean at a saturation slider's setting. The slider is what says how far
/// the page leans, and it goes on saying it after a suggestion has named
/// which way: a lean with no colour left in it is no lean at all, which is
/// what `ground_tint` already makes of one.
fn leaning(neutral: Option<u32>, saturation: f64) -> Option<u32> {
    let (hue, colour, _) = rgb_to_hsl(neutral? | 0xFF);
    Some(hsl_to_rgb(hue, (colour * saturation).clamp(0.0, 1.0), 0.5))
}

/// The page a theme grown from this lean at this slider setting will have --
/// `color_bg_app` -- through the same two functions [`build`] takes it
/// through, so that the fourth swatch and the theme cannot part.
fn page_under(dark: bool, neutral: Option<u32>, saturation: f64) -> u32 {
    let scheme = if dark { Scheme::Dark } else { Scheme::Light };
    let seed = SeedColors::HOUSE.with_neutral(leaning(neutral, saturation));
    let (tint, amount) = ground_tint(&seed, scheme).unwrap_or((WHITE, 0.0));
    grounds(scheme, tint, amount).0
}

/// Do two suggestions show the same four colours? Whole swatch or nothing: a
/// palette that shares three colours with another and differs in the fourth
/// is a different palette, and it is the fourth a person is choosing by.
fn the_same_palette(a: &Suggestion, b: &Suggestion) -> bool {
    a.colors.iter().zip(b.colors.iter()).all(|(x, y)| near(*x, *y))
}

fn near(a: u32, b: u32) -> bool {
    [24, 16, 8].into_iter().all(|shift| {
        let channel = |rgba: u32| ((rgba >> shift) & 0xFF) as i32;
        (channel(a) - channel(b)).abs() <= SAME_COLOR
    })
}

/// What an [`ThemeBuilder::apply`] did, so that a caller can see its own
/// cost: an install is a module rebuild, and everything that is not
/// `Nothing` is one.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Applied {
    /// Nothing was done: the theme on show is the one already in force, or
    /// the builder is not open. Free, and the common case on a drag that has
    /// settled.
    Nothing,
    /// A built theme went in, at the cost of a rebuild.
    Theme,
    /// The built theme came off and what the builder was entered on went
    /// back, at the cost of a rebuild.
    Entry,
}

impl Applied {
    /// Whether the call rebuilt the module, which is the thing a panel
    /// deciding when to apply is really asking about.
    pub fn rebuilt(self) -> bool {
        self != Applied::Nothing
    }
}

/// What was in force when the builder was entered, so that leaving can put it
/// back. Three separate choices: the base theme is only read, to know which
/// appearance to open on; the sheet and the script standing at the seam are
/// both taken off by an install and both go back.
#[derive(Clone, Debug, PartialEq)]
struct Entry {
    sheet: Option<StyleSheet>,
    /// Whatever stood at the seam: nothing, a lab's mix, a saved theme's
    /// pins. The builder does not ask whose it is.
    standing: Option<String>,
    /// The settings the builder opened on. While the controls are still here
    /// nothing is installed, and coming back here takes the built theme off.
    params: BuilderParams,
}

/// A theme built from a favourite colour, and the panel's side of it.
#[derive(Clone, Debug)]
pub struct ThemeBuilder {
    /// What to put back on the way out, and `None` when the builder is not
    /// open.
    entry: Option<Entry>,
    /// Where the controls are.
    params: BuilderParams,
    /// The theme those settings come to. Rebuilt by every [`ThemeBuilder::set`]
    /// while the builder is open, so a reading is never a settle behind the
    /// controls; `None` while it is closed.
    built: Option<BuiltTheme>,
    /// The settings the last apply dealt with, so that a second apply over
    /// untouched controls is nothing at all. `None` before the builder is
    /// entered, and again after [`ThemeBuilder::invalidate`].
    applied: Option<BuilderParams>,
    /// The script this builder put at the seam, while it believes it is still
    /// there. `None` while the entry theme is what is in force.
    installed: Option<String>,
    /// How many module rebuilds this builder has driven. Only ever goes up.
    rebuilds: u32,
}

impl Default for ThemeBuilder {
    /// A builder nobody has entered: no theme to go back to, and the house
    /// settings on a dark page. `enter` decides the appearance for real.
    fn default() -> Self {
        Self {
            entry: None,
            params: BuilderParams::default(),
            built: None,
            applied: None,
            installed: None,
            rebuilds: 0,
        }
    }
}

impl ThemeBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    /// Whether the builder is open: [`ThemeBuilder::enter`] has been called
    /// and [`ThemeBuilder::leave`] has not.
    ///
    /// A closed builder is inert. [`ThemeBuilder::set`] and
    /// [`ThemeBuilder::randomize`] are ignored, [`ThemeBuilder::built`] has
    /// nothing to hand out, [`ThemeBuilder::readability`] measured nothing
    /// and [`ThemeBuilder::apply`] answers [`Applied::Nothing`]. Entering
    /// starts the controls from the appearance in force, which only `enter`
    /// knows, so a setting made before then would be thrown away by it -- and
    /// a control that loses every move is worse than one that is not drawn.
    /// The silence is deliberate, and it means a test that forgets to enter
    /// asserts nothing, so a test asks this first.
    pub fn is_open(&self) -> bool {
        self.entry.is_some()
    }

    /// How many module rebuilds this builder has driven since it was made.
    /// The builder has no frame clock and cannot police the settle
    /// [`ThemeBuilder::apply`] asks of its caller; what it can do is count.
    pub fn rebuilds(&self) -> u32 {
        self.rebuilds
    }

    /// Remember what is in force and open the controls on the house theme in
    /// the same appearance.
    ///
    /// Nothing is installed by entering. The controls open on
    /// [`BuilderParams::house`], and while they are still there the screen
    /// keeps whatever it was wearing -- a style sheet, a saved theme, a lab's
    /// mix -- because an untouched builder has built nothing. The first
    /// setting that differs is the first theme.
    ///
    /// Entering twice without leaving does nothing: the second call would
    /// otherwise capture the built theme as the thing to go back to, and
    /// leaving would never return anywhere.
    ///
    /// Cheap, unlike the lab's: there are no themes to resolve, so this is a
    /// read of three choices off the `Cx`. The panel calls it inside
    /// `cx.with_vm`.
    pub fn enter(&mut self, vm: &mut ScriptVm) {
        if self.entry.is_some() {
            return;
        }
        let sheet = desktop_style::current(vm);
        let dark = match sheet.as_ref().and_then(|sheet| {
            DesktopStyle::parse(&sheet.name).map(|style| (style, sheet.name.ends_with("-dark")))
        }) {
            // A sheet's appearance is its base theme's, which two of the dark
            // ones do not say in their name; the equalizer's table knows.
            Some((style, dark)) => BlendTheme::Sheet(style, dark).appearance() == Appearance::Dark,
            None => crate::base_theme(vm.cx_mut()) == BaseTheme::Dark,
        };
        let params = BuilderParams::house(dark);
        let standing = crate::theme_mix(vm.cx_mut());
        self.entry = Some(Entry { sheet, standing, params });
        self.installed = None;
        self.params = params;
        self.built = Some(build(&params));
        self.applied = Some(params);
    }

    /// Where the controls are. The house settings before the builder is
    /// entered.
    pub fn params(&self) -> BuilderParams {
        self.params
    }

    /// Move the controls. Clamped on the way in, so what
    /// [`ThemeBuilder::params`] hands back is what will be built, and built
    /// at once, so [`ThemeBuilder::readability`] and [`ThemeBuilder::built`]
    /// follow a drag frame by frame. It installs nothing: that is
    /// [`ThemeBuilder::apply`], behind whatever settle the gesture wants.
    ///
    /// A closed builder ignores it. See [`ThemeBuilder::is_open`].
    pub fn set(&mut self, params: BuilderParams) {
        if !self.is_open() {
            return;
        }
        let params = params.clamped();
        if params != self.params || self.built.is_none() {
            self.params = params;
            self.built = Some(build(&params));
        }
    }

    /// [`random_params`], on the page that is showing. The appearance is the
    /// one thing not drawn: a button marked "surprise me" that also flips a
    /// dark room to a white one is a different button.
    pub fn randomize(&mut self, seed: u64) {
        let dark = self.params.dark;
        self.set(BuilderParams { dark, ..random_params(seed) });
    }

    /// Back to the settings the builder opened on, which takes the built
    /// theme off at the next [`ThemeBuilder::apply`].
    pub fn reset(&mut self) {
        if let Some(params) = self.entry.as_ref().map(|entry| entry.params) {
            self.set(params);
        }
    }

    /// The theme the controls come to: the roles for a row of swatches, the
    /// exports, the script. `None` while the builder is closed.
    pub fn built(&self) -> Option<&BuiltTheme> {
        self.built.as_ref()
    }

    /// How the theme on the controls reads, pair by pair. Free: it was taken
    /// when the controls last moved. It installs nothing, and a closed
    /// builder has measured nothing.
    pub fn readability(&self) -> Readability {
        self.built.as_ref().map(|built| built.readability.clone()).unwrap_or_default()
    }

    /// Whether [`ThemeBuilder::apply`] would change anything: the controls
    /// against the settings the last apply dealt with, and nothing else. The
    /// builder holds no handle on the module it installed into, so something
    /// else rebuilding that module leaves this answering `false`, and that is
    /// what [`ThemeBuilder::invalidate`] is for.
    pub fn is_dirty(&self) -> bool {
        self.applied != Some(self.params)
    }

    /// Say that the module was rebuilt underneath the builder -- a style
    /// reload, a live edit, a base theme switched from elsewhere -- so that
    /// the next [`ThemeBuilder::apply`] looks again.
    ///
    /// It costs nothing to be told. The built theme is held on the `Cx` and
    /// came back up with the module, so the apply that follows finds its own
    /// script still at the seam and installs nothing; what it does find, if
    /// the rebuild was somebody picking a theme, is that it has been stood
    /// down. The builder still owes the entry theme back afterwards: only
    /// [`ThemeBuilder::leave`] settles that.
    pub fn invalidate(&mut self) {
        self.applied = None;
    }

    /// Put the theme on the controls in force, and say what that took.
    ///
    /// Safe to call on any frame in the sense that untouched controls are a
    /// return and nothing else. The call itself is cheap -- the theme was
    /// built when the controls moved -- but the reload it asks for is a
    /// module rebuild on the tick that follows, some fifty milliseconds, so a
    /// panel still owes it a settle: apply on the control's own end-of-drag,
    /// or on a timeout re-armed by every edit. See [`Applied`] and
    /// [`ThemeBuilder::rebuilds`].
    ///
    /// Controls back on the settings the builder opened on take the built
    /// theme OFF and put the entry theme back, sheet and all, rather than
    /// installing a house theme over a screen that may have been wearing
    /// something else: untouched means untouched.
    ///
    /// A theme chosen from outside wins, and this is the call that notices.
    /// A builder that believes it installed a script and finds a different
    /// one at the seam, or none, has had the theme chosen out from under it
    /// -- the app's own picker clears the seam, the lab writes its mix there
    /// -- so it does not put its own back: it opens again on what is now in
    /// force, with the controls at the house settings, and answers
    /// [`Applied::Nothing`]. Asked before the dirty check, because a pick
    /// moves no control.
    ///
    /// The panel calls this inside `cx.with_vm`.
    pub fn apply(&mut self, vm: &mut ScriptVm) -> Applied {
        if !self.is_open() {
            return Applied::Nothing;
        }
        if let Some(mine) = &self.installed {
            if crate::theme_mix(vm.cx_mut()).as_deref() != Some(mine.as_str()) {
                self.installed = None;
                self.entry = None;
                self.enter(vm);
                return Applied::Nothing;
            }
        }
        if !self.is_dirty() {
            return Applied::Nothing;
        }
        let mut did = Applied::Nothing;
        if self.entry.as_ref().is_some_and(|entry| entry.params == self.params) {
            if self.installed.is_some() {
                if let Some(entry) = self.entry.clone() {
                    self.install_entry(vm, &entry);
                }
                self.installed = None;
                did = Applied::Entry;
            }
        } else {
            let code = match &self.built {
                Some(built) if built.params == self.params => built.script.clone(),
                _ => build(&self.params).script,
            };
            // A theme that is already the one in force does not go in again.
            // A builder told about a rebuild is owed a second look and
            // nothing else; installing would ask for the reload that told it,
            // and that is a loop with a module rebuild in it.
            if self.installed.as_deref() != Some(code.as_str()) {
                self.rebuilds = self.rebuilds.saturating_add(1);
                desktop_style::uninstall(vm);
                crate::set_theme_mix(vm.cx_mut(), Some(code.clone()));
                vm.cx_mut().request_style_reload();
                self.installed = Some(code);
                did = Applied::Theme;
            }
        }
        self.applied = Some(self.params);
        did
    }

    /// Put back what [`ThemeBuilder::enter`] found, and close the builder.
    ///
    /// A builder that never installed anything restores nothing: there is
    /// nothing to undo, and a rebuild for the sake of it is fifty
    /// milliseconds of nothing. So does one that was stood down -- somebody
    /// else's choice is on the screen, and taking it off on the way out would
    /// be the builder having the last word in a conversation it had left.
    ///
    /// The panel calls this inside `cx.with_vm`.
    pub fn leave(&mut self, vm: &mut ScriptVm) {
        let Some(entry) = self.entry.take() else {
            return;
        };
        if let Some(mine) = self.installed.take() {
            if crate::theme_mix(vm.cx_mut()).as_deref() == Some(mine.as_str()) {
                self.install_entry(vm, &entry);
            }
        }
        self.built = None;
        self.applied = None;
        self.params = BuilderParams::house(self.params.dark);
    }

    /// The sheet and the standing script back where they were found, and the
    /// module run that makes an app wear them asked for. The base theme is
    /// not touched because the builder never moved it, and `set_base_theme`
    /// would clear the very script being handed back.
    fn install_entry(&mut self, vm: &mut ScriptVm, entry: &Entry) {
        self.rebuilds = self.rebuilds.saturating_add(1);
        match &entry.sheet {
            Some(sheet) => desktop_style::install(vm, sheet.clone()),
            None => desktop_style::uninstall(vm),
        }
        crate::set_theme_mix(vm.cx_mut(), entry.standing.clone());
        vm.cx_mut().request_style_reload();
    }
}

#[cfg(test)]
mod theme_builder_tests {
    use super::*;
    use crate::makepad_platform::Cx;
    use crate::theme_lab::ThemeLab;
    use crate::theme_tokens::{
        assigned_keys, roles_for, theme_keys, BlendCache, BlendValue, ThemeValues, GROUND_TINT_DARK,
        GROUND_TINT_LIGHT,
    };

    /// The tick a `cx.request_style_reload()` lands on, driven by hand, and a
    /// failure if nothing asked for one: an install IS the reload it asks
    /// for, so an apply that forgot to ask installed nothing.
    fn the_reload_lands(vm: &mut ScriptVm) {
        assert!(
            std::mem::take(&mut vm.cx_mut().pending_style_reload),
            "nothing asked for the style reload that carries an install to the app"
        );
        vm.cx_mut().pending_live_edit_request = false;
        vm.with_reload(crate::script_mod);
    }

    fn evaluate(vm: &mut ScriptVm, name: &str, code: String) {
        vm.bx.captured_errors = Some(Vec::new());
        vm.eval(ScriptMod {
            cargo_manifest_path: env!("CARGO_MANIFEST_DIR").into(),
            module_path: format!("theme_builder_test_{name}"),
            file: format!("{name}.splash"),
            line: 0,
            column: 0,
            code,
            values: vec![],
        });
        let errors = vm.take_errors();
        assert!(errors.is_empty(), "{name}: {errors:?}");
    }

    fn filed(vm: &mut ScriptVm, name: &str) -> ScriptObject {
        let themes = vm.module(LiveId::from_str("themes"));
        vm.bx
            .heap
            .value(themes, LiveId::from_str(name).into(), NoTrap)
            .as_object()
            .unwrap_or_else(|| panic!("mod.themes.{name} was never built"))
    }

    fn worn(vm: &mut ScriptVm) -> ScriptObject {
        vm.module(LiveId::from_str("theme"))
    }

    fn token(vm: &mut ScriptVm, theme: ScriptObject, key: &str) -> Option<TokenValue> {
        let value = vm.bx.heap.value(theme, LiveId::from_str(key).into(), NoTrap);
        match (value.as_color(), value.as_number()) {
            (Some(rgba), _) => Some(TokenValue::Color(rgba)),
            (_, Some(number)) => Some(TokenValue::Num(number)),
            _ => None,
        }
    }

    fn color(vm: &mut ScriptVm, theme: ScriptObject, key: &str) -> Option<u32> {
        vm.bx.heap.value(theme, LiveId::from_str(key).into(), NoTrap).as_color()
    }

    /// A number inside a token that is an object: the `font_size` of a
    /// `TextStyle`.
    fn inner(vm: &mut ScriptVm, theme: ScriptObject, key: &str, field: &str) -> Option<f64> {
        let object = vm.bx.heap.value(theme, LiveId::from_str(key).into(), NoTrap).as_object()?;
        vm.bx.heap.value(object, LiveId::from_str(field).into(), NoTrap).as_number()
    }

    /// The theme object the widget module was handed, which is what every
    /// `theme.color_x` in a template was baked from.
    fn widget_theme(vm: &mut ScriptVm) -> ScriptObject {
        let prelude = vm.module(LiveId::from_str("prelude"));
        let internal = vm
            .bx
            .heap
            .value(prelude, LiveId::from_str("widgets_internal").into(), NoTrap)
            .as_object()
            .expect("the widget prelude");
        vm.bx.heap.value(internal, LiveId::from_str("theme").into(), NoTrap).as_object().expect("its theme")
    }

    /// A blue nobody would mistake for the house orange.
    const BLUE: u32 = 0x2060E0FF;

    fn blue(dark: bool) -> BuilderParams {
        BuilderParams { favourite: BLUE, ..BuilderParams::house(dark) }
    }

    /// How far apart two hues are, the short way round.
    fn apart(a: f64, b: f64) -> f64 {
        let d = (a - b).rem_euclid(360.0);
        d.min(360.0 - d)
    }

    /// The builder's idea of "untouched" is read off the theme files, so it
    /// has to be able to read them: a file that starts writing `6.0 * 1` or
    /// moves a global under a different indent would otherwise hand every
    /// build a fallback nobody chose, silently.
    #[test]
    fn the_house_numbers_are_read_off_the_theme_files() {
        for scheme in [Scheme::Dark, Scheme::Light] {
            for (key, want) in [
                ("space_factor", 6.0),
                ("corner_radius", 2.5),
                ("font_size_base", 10.0),
                ("font_size_contrast", 2.5),
                ("color_contrast", 1.0),
            ] {
                assert_eq!(file_number(scheme, key), Some(want), "{key} in {}", scheme.theme_name());
            }
            for key in ["color_on_surface", "color_on_surface_variant", "color_primary"] {
                assert!(file_color(scheme, key).is_some(), "{key} in {}", scheme.theme_name());
            }
            // A derived key is not a number the file states.
            assert_eq!(file_number(scheme, "space_2"), None);
        }
        assert_eq!(file_color(Scheme::Dark, "color_on_surface_variant"), Some(0xFFFFFFA8));
        // Every dimension the sliders drive has a registered range to be
        // clamped into, and the house value is inside it.
        let house = BuilderParams::house(true);
        for key in DIMENSIONS {
            let spec = token_spec(key).unwrap_or_else(|| panic!("{key} is not registered"));
            assert!(spec.min < spec.max && (spec.min..=spec.max).contains(&house.dimension(key)), "{key}");
        }
    }

    /// The settings a builder opens on are the theme the library ships: the
    /// same seed, the same tuning to the last bit, the same roles, no global
    /// moved, and a script that pins nothing the base theme does not already
    /// say.
    #[test]
    fn the_house_settings_build_the_house_theme() {
        assert_eq!(BuilderParams::default(), BuilderParams::house(true));
        for dark in [true, false] {
            let built = build(&BuilderParams::house(dark));
            let scheme = built.scheme;
            assert_eq!(scheme, if dark { Scheme::Dark } else { Scheme::Light });
            assert_eq!(built.seed, SeedColors::HOUSE);
            assert_eq!(built.tuning, RoleTuning::HOUSE, "the middle of the sliders is not the house rule");
            assert_eq!(built.roles, roles_for(scheme));
            assert!(built.globals.is_empty(), "{:?}", built.globals);
            assert!(!built.script.contains("use mod."), "an untouched theme was built again from source");
            assert_eq!(built.script, theme_module_script(BUILT_NAME, scheme.theme_name(), &built.overrides));
            assert_eq!(built.overrides.len(), 27, "the roles, less the two every theme keeps");
            for (key, value) in &built.overrides {
                let TokenValue::Color(rgba) = value else { panic!("{key} is not a colour") };
                assert_eq!(file_color(scheme, key), Some(*rgba), "{key} is pinned off the base theme's own value");
            }
            assert!(built.readability.holds(), "{:#?}", built.readability);
            assert_eq!(built.readability.measured, held_pairs().len());
        }
    }

    /// The same, asked of the VM: the house script evaluated over the real
    /// module leaves every colour and every number of the base theme exactly
    /// where it was.
    #[test]
    fn the_house_script_moves_no_token() {
        let mut cx = Cx::new(Box::new(|_, _| {}));
        cx.with_vm(|vm| {
            crate::script_mod(vm);
            for dark in [true, false] {
                let built = build(&BuilderParams::house(dark));
                evaluate(vm, "house", built.script_as("house_source", "house", false));
                let (mine, base) = (filed(vm, "house"), filed(vm, built.scheme.theme_name()));
                let mut compared = 0;
                for key in base_theme_keys() {
                    let theirs = token(vm, base, key);
                    assert_eq!(token(vm, mine, key), theirs, "{key} in {}", built.scheme.theme_name());
                    compared += theirs.is_some() as usize;
                }
                assert!(compared > 400, "only {compared} tokens were compared");
            }
        });
    }

    /// The reading `build` hands back is taken with no VM, off colours it
    /// worked out for itself, so it is only worth anything if those are the
    /// colours the VM makes. Exactly, not nearly: the page's brightest rung
    /// clears its bar by four hundredths in the dark theme, and a channel one
    /// step out is a pair that passes here and fails on the screen.
    #[test]
    fn what_build_predicts_is_what_the_vm_resolves() {
        let mut cx = Cx::new(Box::new(|_, _| {}));
        cx.with_vm(|vm| {
            crate::script_mod(vm);
            let mut evaluated = 0;
            for dark in [true, false] {
                for step in 0..12 {
                    for saturation in [0.0, 0.5, 1.0] {
                        for brightness in [0.0, 1.0] {
                            let params = BuilderParams {
                                favourite: hsl_to_rgb(step as f64 * 30.0 + 7.0, 0.8, 0.5),
                                harmony: Harmony::ALL[step % Harmony::ALL.len()],
                                saturation,
                                brightness,
                                ..BuilderParams::house(dark)
                            };
                            let built = build(&params);
                            evaluate(vm, "predicted", built.script_as("predicted_source", "predicted", false));
                            let theme = filed(vm, "predicted");
                            for (key, want) in &built.colors {
                                assert_eq!(
                                    color(vm, theme, key).map(|c| format!("{c:08X}")),
                                    Some(format!("{want:08X}")),
                                    "{key} under {params:?}"
                                );
                            }
                            evaluated += 1;
                        }
                    }
                }
            }
            assert_eq!(evaluated, 2 * 12 * 3 * 2);
        });
    }

    /// The constants `grounds` is written with are the theme files', and this
    /// is what fails when a file changes one: the two ends of each page in
    /// the order the file mixes them, the fraction each ground sits at, and
    /// the two opaque rungs the inverse page is made of.
    #[test]
    fn the_grounds_are_mixed_the_way_the_theme_files_mix_them() {
        let squeezed = |scheme: Scheme| scheme.source().split_whitespace().collect::<Vec<_>>().join(" ");
        let tinted = |end: &str| format!("theme.color_{end} * mix(#ffffff, theme.color_tint, theme.color_tint_amount)");
        for (scheme, from, to, (bg, fg)) in
            [(Scheme::Dark, "b", "w", DARK_GROUNDS), (Scheme::Light, "w", "b", LIGHT_GROUNDS)]
        {
            let source = squeezed(scheme);
            for (key, fraction) in [("color_bg_app", bg), ("color_fg_app", fg)] {
                let want =
                    format!("{key}: mix( {}, {}, pow({fraction}, theme.color_contrast))", tinted(from), tinted(to));
                assert!(source.contains(&want), "{}: expected `{want}`", scheme.theme_name());
            }
            for want in [
                format!("color_opaque_u_6: mix(theme.color_fg_app, #F, {OPAQUE_U_6})"),
                format!("color_opaque_d_5: mix(theme.color_fg_app, #0, {OPAQUE_D_5})"),
            ] {
                assert!(source.contains(&want), "{}: expected `{want}`", scheme.theme_name());
            }
        }
    }

    /// The point of the whole rule: whatever the favourite colour, wherever
    /// the two sliders are and whichever page it is on, EVERY pair the
    /// library holds its own themes to meets its bar. Every hue at ten degree
    /// steps, both ends and the middle of both sliders, both appearances --
    /// and every harmony, since the harmony decides two of the three hues.
    #[test]
    fn every_built_theme_reads() {
        let mut built_themes = 0;
        for dark in [true, false] {
            for harmony in Harmony::ALL {
                for step in 0..36 {
                    for saturation in [0.0, 0.5, 1.0] {
                        for brightness in [0.0, 0.5, 1.0] {
                            let params = BuilderParams {
                                favourite: hsl_to_rgb(step as f64 * 10.0, 0.85, 0.5),
                                harmony,
                                saturation,
                                brightness,
                                ..BuilderParams::house(dark)
                            };
                            let built = build(&params);
                            assert_eq!(built.readability.measured, held_pairs().len(), "{params:?}");
                            assert!(built.readability.holds(), "{params:?}: {:#?}", built.readability.failures);
                            assert!(built.readability.margin >= 0.0);
                            built_themes += 1;
                        }
                    }
                }
            }
        }
        assert_eq!(built_themes, 2 * 6 * 36 * 3 * 3);
        // A favourite with no colour in it, and the two that are nothing but.
        for favourite in [0x000000FFu32, 0x808080FF, 0xFFFFFFFF, 0xFFFF00FF, 0x0000FFFF] {
            for dark in [true, false] {
                let params = BuilderParams { favourite, saturation: 1.0, ..BuilderParams::house(dark) };
                let built = build(&params);
                assert!(built.readability.holds(), "{params:?}: {:#?}", built.readability.failures);
            }
        }
    }

    /// The sweep above never sees a surface ink replaced: at the amounts the
    /// tint is allowed, the base themes' own inks read on every rung of every
    /// tinted page. So the repair is put to grounds the sliders cannot reach,
    /// because it is there for the day somebody raises those amounts, and a
    /// safety net nobody has fallen into is one nobody knows the state of.
    #[test]
    fn an_ink_that_stops_reading_falls_to_the_plain_end_that_does() {
        let page = |rgba: u32| {
            let mut colors = BTreeMap::new();
            for (ground, ink, _) in held_pairs() {
                if *ink == "color_on_surface" || *ink == "color_on_surface_variant" {
                    colors.insert(ground.to_string(), rgba);
                }
            }
            colors
        };
        // White on a dark page reads, and is kept -- alpha and all.
        assert_eq!(settle_ink(&page(0x202020FF), "color_on_surface", WHITE), WHITE);
        assert_eq!(settle_ink(&page(0x202020FF), "color_on_surface_variant", 0xFFFFFFA8), 0xFFFFFFA8);
        // The same white on a pale page does not, and black does.
        assert_eq!(settle_ink(&page(0xD0D8F0FF), "color_on_surface", WHITE), BLACK);
        // A translucent black that has faded into a mid page is made solid.
        assert_eq!(settle_ink(&page(0x6070A0FF), "color_on_surface_variant", 0x00000060), WHITE);
        // One ink for the whole ladder: a single rung it fails on is enough,
        // and the end chosen is the one that does best on its WORST rung.
        let mut ladder = page(0x202020FF);
        ladder.insert("color_surface_bright".to_string(), 0xC0C0C0FF);
        let grey = 0x909090FF;
        assert!(reads_on(0x202020FF, grey) >= 4.5 && reads_on(0xC0C0C0FF, grey) < 4.5);
        // White is 1.8:1 on the pale rung and black is 1.3:1 on the dark
        // ones, so neither reads everywhere and white is the less bad.
        assert_eq!(settle_ink(&ladder, "color_on_surface", grey), WHITE);
        // And the reading that comes back says so when nothing can be done.
        let reading = read_pairs(&ladder.into_iter().chain([("color_on_surface".to_string(), grey)]).collect());
        assert!(!reading.holds());
        assert!(reading.failures[0].starts_with("color_on_surface on color_surface_bright = "), "{:?}", reading.failures);
        assert_eq!(reading.tightest, reading.failures[0]);
        assert!(reading.margin < 0.0);
    }

    /// A harmony chosen on the panel is the harmony the palette comes out
    /// in: the three brand families of the built theme sit where the offsets
    /// say, read off the colours the script pins.
    #[test]
    fn the_harmony_reaches_the_built_palette() {
        let first = rgb_to_hsl(BLUE).0;
        for harmony in Harmony::ALL {
            let built = build(&BuilderParams { harmony, ..blue(false) });
            let (second, third) = harmony.offsets();
            let hue = |key: &str| rgb_to_hsl(built.color(key).unwrap()).0;
            assert!(apart(hue("color_primary"), first) < 4.0, "{harmony:?}");
            assert!(apart(hue("color_secondary"), first + second) < 4.0, "{harmony:?}: {}", hue("color_secondary"));
            assert!(apart(hue("color_tertiary"), first + third) < 4.0, "{harmony:?}: {}", hue("color_tertiary"));
            let pinned = |key: &str| {
                let want = TokenValue::Color(built.color(key).unwrap());
                built.overrides.iter().any(|(k, v)| k == key && *v == want)
            };
            assert!(pinned("color_secondary") && pinned("color_tertiary_container"));
        }
    }

    /// Accents only at one end: the house strength of colour and a page with
    /// none. Saturated at the other: more colour in the brand families, and
    /// the grounds leaning toward the favourite's hue by as much as the
    /// appearance can take. A favourite that is a grey has no hue for a page
    /// to lean toward, wherever the slider is.
    #[test]
    fn saturation_runs_from_accents_only_to_a_tinted_page() {
        for (dark, most) in [(true, GROUND_TINT_DARK), (false, GROUND_TINT_LIGHT)] {
            let plain = build(&BuilderParams { saturation: 0.0, ..blue(dark) });
            assert!(plain.globals.is_empty(), "{:?}", plain.globals);
            assert_eq!(plain.tuning.brand, RoleTuning::HOUSE.brand);
            assert_eq!(plain.color("color_bg_app"), build(&BuilderParams::house(dark)).color("color_bg_app"));

            let full = build(&BuilderParams { saturation: 1.0, ..blue(dark) });
            assert_eq!(full.tuning.brand, BRAND_SATURATED);
            let global = |built: &BuiltTheme, key: &str| built.globals.iter().find(|(k, _)| k == key).map(|(_, v)| v.clone());
            let Some(TokenValue::Color(tint)) = global(&full, "color_tint") else { panic!("no tint: {:?}", full.globals) };
            let Some(TokenValue::Num(amount)) = global(&full, "color_tint_amount") else { panic!("no amount") };
            assert!(apart(rgb_to_hsl(tint).0, rgb_to_hsl(BLUE).0) < 1.0);
            assert!((amount - most).abs() < 0.005, "{amount} against {most}");
            let page = rgb_to_hsl(full.color("color_bg_app").unwrap());
            assert!(apart(page.0, rgb_to_hsl(BLUE).0) < 3.0 && page.1 > 0.05, "the page did not lean: {page:?}");
            let sat = |built: &BuiltTheme| rgb_to_hsl(built.roles.primary.base).1;
            assert!(sat(&full) > sat(&plain) + 0.2);

            let half = build(&BuilderParams { saturation: 0.5, ..blue(dark) });
            let Some(TokenValue::Num(half_amount)) = global(&half, "color_tint_amount") else { panic!("no amount at a half") };
            assert!((half_amount - most * 0.5).abs() < 0.005, "{half_amount}");

            let grey = build(&BuilderParams { favourite: 0x777777FF, saturation: 1.0, ..BuilderParams::house(dark) });
            assert!(grey.globals.is_empty(), "a grey leant the page: {:?}", grey.globals);
            assert_eq!(rgb_to_hsl(grey.roles.primary.base).1, 0.0);
        }
    }

    /// Dim at one end, bright at the other, the house rule in the middle, and
    /// brighter meaning lighter on both pages.
    #[test]
    fn brightness_moves_the_families_the_way_the_word_says() {
        for dark in [true, false] {
            let at = |brightness: f64| build(&BuilderParams { brightness, ..blue(dark) });
            let l = |built: &BuiltTheme| rgb_to_hsl(built.roles.primary.base).2;
            assert_eq!(at(0.5).tuning, RoleTuning::HOUSE);
            let mut last = -1.0;
            for brightness in [0.0, 0.25, 0.5, 0.75, 1.0] {
                let built = at(brightness);
                assert!(l(&built) > last, "brightness {brightness} on dark={dark}");
                last = l(&built);
            }
            let (dim, bright) = if dark { (DARK_DIM, DARK_BRIGHT) } else { (LIGHT_DIM, LIGHT_BRIGHT) };
            let mine = |tuning: RoleTuning| if dark { tuning.dark } else { tuning.light };
            assert_eq!(mine(at(0.0).tuning), dim);
            assert_eq!(mine(at(1.0).tuning), bright);
        }
    }

    /// Nothing a control can hand over reaches the colour maths or a theme
    /// file raw.
    #[test]
    fn settings_are_brought_inside_what_they_can_mean() {
        let wild = BuilderParams {
            saturation: 9.0,
            brightness: -3.0,
            spacing: 500.0,
            roundness: -4.0,
            font_size: 1000.0,
            font_contrast: -1.0,
            ..blue(true)
        };
        let tame = wild.clamped();
        assert_eq!((tame.saturation, tame.brightness), (1.0, 0.0));
        for (key, value) in [
            ("space_factor", tame.spacing),
            ("corner_radius", tame.roundness),
            ("font_size_base", tame.font_size),
            ("font_size_contrast", tame.font_contrast),
        ] {
            let spec = token_spec(key).unwrap();
            assert!((spec.min..=spec.max).contains(&value), "{key} = {value}");
        }
        assert_eq!(build(&wild), build(&tame));
        assert_eq!(build(&wild).params, tame);
        let lost = BuilderParams { saturation: f64::NAN, brightness: f64::NAN, ..blue(true) }.clamped();
        assert_eq!((lost.saturation, lost.brightness), (0.0, 0.5));
    }

    /// Pure: the same settings are the same theme down to the script, and the
    /// same seed is the same settings.
    #[test]
    fn the_same_settings_are_the_same_theme() {
        for seed in [0u64, 1, 42, u64::MAX] {
            assert_eq!(random_params(seed), random_params(seed));
            assert_eq!(build(&random_params(seed)), build(&random_params(seed)));
            assert_eq!(build(&random_params(seed)).script, build(&random_params(seed)).script);
        }
        assert_ne!(random_params(1), random_params(2));
    }

    /// A random theme is a theme somebody might keep: inside every range,
    /// readable, and -- over enough draws -- every harmony and both pages.
    #[test]
    fn a_random_theme_is_in_range_and_reads() {
        let mut harmonies = Vec::new();
        let mut pages = Vec::new();
        for seed in 0..300u64 {
            let params = random_params(seed);
            assert_eq!(params, params.clamped(), "seed {seed}");
            assert!(rgb_to_hsl(params.favourite).1 > 0.5, "seed {seed} drew a favourite with no colour in it");
            assert!((4.0..=9.0).contains(&params.spacing) && (9.0..=12.0).contains(&params.font_size), "seed {seed}");
            assert_eq!(params.spacing * 2.0, (params.spacing * 2.0).round(), "seed {seed} is off the half step");
            let built = build(&params);
            assert!(built.readability.holds(), "seed {seed}: {:#?}", built.readability.failures);
            if !harmonies.contains(&params.harmony) {
                harmonies.push(params.harmony);
            }
            if !pages.contains(&params.dark) {
                pages.push(params.dark);
            }
        }
        assert_eq!(harmonies.len(), Harmony::ALL.len());
        assert_eq!(pages.len(), 2);
    }

    /// A theme whose globals moved is built again from source, and that is
    /// the only way the ladders follow: every rung is an expression of the
    /// global, and a pin on the global alone left each of them where it was.
    /// It is also a NEW object the font installer never saw, so the font
    /// families are carried across -- checked against the re-derived source
    /// object too, which is what the theme would have worn without the carry.
    #[test]
    fn moved_globals_move_the_ladder_and_keep_the_fonts() {
        let mut cx = Cx::new(Box::new(|_, _| {}));
        cx.with_vm(|vm| {
            crate::script_mod(vm);
            for dark in [true, false] {
                let params =
                    BuilderParams { spacing: 9.0, roundness: 7.0, font_size: 12.0, font_contrast: 3.0, ..blue(dark) };
                let built = build(&params);
                let keys: Vec<&str> = built.globals.iter().map(|(k, _)| k.as_str()).collect();
                assert_eq!(keys, DIMENSIONS, "only what moved, in file order");
                assert!(built.script.starts_with("    use mod.math.*\n"));
                evaluate(vm, "roomy", built.script_as("roomy_source", "roomy", false));
                let (theme, source, base) =
                    (filed(vm, "roomy"), filed(vm, "roomy_source"), filed(vm, built.scheme.theme_name()));
                let number = |vm: &mut ScriptVm, key: &str| match token(vm, theme, key) {
                    Some(TokenValue::Num(n)) => n,
                    other => panic!("{key} is {other:?}"),
                };
                assert_eq!(number(vm, "space_factor"), 9.0);
                assert_eq!(number(vm, "space_1"), 4.5);
                assert_eq!(number(vm, "space_6"), 36.0);
                assert_eq!(number(vm, "corner_radius"), 7.0);
                assert_eq!(number(vm, "font_size_p"), 12.0);
                assert_eq!(number(vm, "font_size_1"), 36.0);
                assert_eq!(number(vm, "type_title_l_size"), 18.0);
                assert_eq!(inner(vm, theme, "font_title_l", "font_size"), Some(18.0));
                // The palette went on over the re-derived base.
                assert_eq!(color(vm, theme, "color_primary"), Some(built.roles.primary.base));
                // And the base theme the library ships is where it was.
                assert_eq!(token(vm, base, "space_factor"), Some(TokenValue::Num(6.0)));
                for font in CARRIED_FONTS {
                    let at = |vm: &mut ScriptVm, object: ScriptObject| {
                        vm.bx.heap.value(object, LiveId::from_str(font).into(), NoTrap)
                    };
                    assert!(at(vm, base).as_object().is_some(), "{font} is not a style in the base theme");
                    assert!(at(vm, theme) == at(vm, base), "{font} was not carried across");
                    assert!(at(vm, source) != at(vm, base), "{font} needed no carrying, so this test proves nothing");
                }
            }
        });
    }

    /// A theme saved with its dimensions moved comes back with its margins
    /// and its text styles moved, and not only the numbers that say so.
    ///
    /// The store keeps colours and numbers. Pinned over the base OBJECT they
    /// left `mspace_2` and `font_title_l` -- objects the base file derives --
    /// at the library's sizes under a `space_factor` that read twelve.
    #[test]
    fn a_saved_theme_whose_dimensions_moved_brings_its_margins_back_with_it() {
        use crate::theme_store::SavedTheme;
        let mut cx = Cx::new(Box::new(|_, _| {}));
        cx.with_vm(|vm| {
            crate::script_mod(vm);
            let mut saved = SavedTheme::new("roomy_saved", Scheme::Dark);
            saved.overrides = vec![
                ("color_primary".to_string(), TokenValue::Color(0x112233FF)),
                ("font_size_base".to_string(), TokenValue::Num(12.0)),
                ("space_factor".to_string(), TokenValue::Num(12.0)),
            ];
            evaluate(vm, "roomy_saved", saved.script());
            let (theme, base) = (filed(vm, "roomy_saved"), filed(vm, "dark"));
            assert_eq!(token(vm, theme, "space_factor"), Some(TokenValue::Num(12.0)));
            // The object a pin cannot carry, and the whole point.
            assert_eq!(inner(vm, theme, "mspace_2", "left"), Some(12.0));
            assert_eq!(inner(vm, base, "mspace_2", "left"), Some(6.0), "the library's own theme moved");
            let title = inner(vm, theme, "font_title_l", "font_size");
            assert_ne!(title, inner(vm, base, "font_title_l", "font_size"), "the title kept the library's size");
            match token(vm, theme, "type_title_l_size") {
                Some(TokenValue::Num(size)) => assert_eq!(title, Some(size), "the style and its size part company"),
                other => panic!("type_title_l_size is {other:?}"),
            }
            // The pins still go on over the top of it.
            assert_eq!(color(vm, theme, "color_primary"), Some(0x112233FF));

            // Nothing moved, nothing re-derived: the script is the plain pins
            // it always was, and a palette-only theme pays for no rebuild.
            let mut plain = SavedTheme::new("plain_saved", Scheme::Dark);
            plain.overrides = vec![
                ("color_primary".to_string(), TokenValue::Color(0x112233FF)),
                ("space_factor".to_string(), TokenValue::Num(6.0)),
            ];
            assert!(!plain.script().contains("plain_saved_source"));

            // Under a sheet the base object is the one the sheet wrote its
            // fonts and its skins into, so it stays the thing derived from.
            let mut sheeted = saved.clone();
            sheeted.sheet = Some((crate::desktop_style::DesktopStyle::Omarchy, false));
            assert!(!sheeted.script().contains("roomy_saved_source"));
        });
    }

    /// `CARRIED_FONTS` is a second copy of a list the font installer owns.
    #[test]
    fn the_carried_fonts_are_the_ones_the_installer_installs() {
        let installer = include_str!("font_policy.rs");
        let opening = "mod.themes.dark = mod.themes.dark{";
        let block = &installer[installer.find(opening).expect("the installer's dark block") + opening.len()..];
        let mut installed: Vec<&str> = Vec::new();
        for line in block.lines().skip(1) {
            let line = line.trim();
            if line == "}" {
                break;
            }
            if let Some((key, _)) = line.split_once(": ") {
                if key.starts_with("font_") {
                    installed.push(key);
                }
            }
        }
        assert_eq!(installed, CARRIED_FONTS);
    }

    /// The panel's sentence, start to finish: an untouched builder is free,
    /// a moved control is one rebuild, the theme reaches the module the
    /// widgets are built from, the same settings do not go in twice -- not
    /// even after being told about a rebuild -- and the controls back where
    /// they started take the theme off again.
    #[test]
    fn enter_set_apply_leave() {
        let mut builder = ThemeBuilder::new();
        let mut cx = Cx::new(Box::new(|_, _| {}));
        cx.with_vm(|vm| {
            crate::script_mod(vm);
            vm.bx.captured_errors = Some(Vec::new());
            desktop_style::uninstall(vm);

            // Closed, it is inert.
            builder.set(blue(true));
            assert_eq!(builder.params(), BuilderParams::default());
            assert_eq!(builder.apply(vm), Applied::Nothing);
            assert!(builder.built().is_none() && builder.readability().measured == 0);

            builder.enter(vm);
            assert!(builder.is_open());
            assert_eq!(builder.params(), BuilderParams::house(true), "the default base theme is the dark one");
            assert!(!builder.is_dirty(), "entering is not a change to anything");
            assert_eq!(builder.apply(vm), Applied::Nothing);
            assert_eq!(builder.rebuilds(), 0);
            assert!(!vm.cx_mut().pending_style_reload, "an untouched builder asked for a rebuild");

            builder.set(BuilderParams { saturation: 1.0, spacing: 8.0, ..blue(true) });
            assert!(builder.is_dirty());
            assert!(builder.readability().holds());
            let built = builder.built().unwrap().clone();
            assert_eq!(builder.apply(vm), Applied::Theme);
            assert!(Applied::Theme.rebuilt() && !Applied::Nothing.rebuilt());
            assert_eq!(builder.rebuilds(), 1);
            assert_eq!(crate::theme_mix(vm.cx_mut()).as_deref(), Some(built.script.as_str()));
            the_reload_lands(vm);
            let want = Some(built.roles.primary.base);
            assert_ne!(want, Some(roles_for(Scheme::Dark).primary.base));
            let theme = worn(vm);
            assert_eq!(color(vm, theme, "color_primary"), want, "the built theme did not become the theme");
            let handed = widget_theme(vm);
            assert_eq!(color(vm, handed, "color_primary"), want, "the widgets were built off the theme it replaced");
            assert_eq!(color(vm, handed, "color_bg_app"), built.color("color_bg_app"));
            assert_eq!(token(vm, handed, "space_2"), Some(TokenValue::Num(8.0)));

            // Settled, and told about the rebuild it asked for: nothing.
            assert_eq!(builder.apply(vm), Applied::Nothing);
            builder.invalidate();
            assert!(builder.is_dirty());
            assert_eq!(builder.apply(vm), Applied::Nothing, "a theme already in force went in a second time");
            assert_eq!(builder.rebuilds(), 1);
            assert!(!vm.cx_mut().pending_style_reload);

            // A second setting is a second theme.
            builder.set(BuilderParams { harmony: Harmony::Triadic, ..builder.params() });
            assert_eq!(builder.apply(vm), Applied::Theme);
            the_reload_lands(vm);
            assert_eq!(builder.rebuilds(), 2);

            // Back where it opened: the theme comes off.
            builder.reset();
            assert_eq!(builder.params(), BuilderParams::house(true));
            assert_eq!(builder.apply(vm), Applied::Entry);
            assert_eq!(crate::theme_mix(vm.cx_mut()), None);
            the_reload_lands(vm);
            let theme = worn(vm);
            assert_eq!(color(vm, theme, "color_primary"), Some(roles_for(Scheme::Dark).primary.base));
            assert_eq!(builder.rebuilds(), 3);

            builder.leave(vm);
            assert!(!builder.is_open() && builder.built().is_none());
            assert!(!vm.cx_mut().pending_style_reload, "leaving an entry theme that was already back rebuilt again");
            let errors = vm.take_errors();
            assert!(errors.is_empty(), "{errors:?}");
        });
    }

    /// Leaving is the inverse of entering: the sheet that came off goes back
    /// on, and so does whatever script stood at the seam -- here a stand-in
    /// for a lab's mix or a saved theme's pins. A builder that was only
    /// looked at puts back nothing, because it took nothing off.
    #[test]
    fn leaving_puts_back_the_sheet_and_the_script_that_stood() {
        let standing =
            theme_module_script("stood", "dark", &[("color_bg_app".to_string(), TokenValue::Color(0x123456FF))]);
        let mut cx = Cx::new(Box::new(|_, _| {}));
        cx.with_vm(|vm| {
            crate::script_mod(vm);
            vm.bx.captured_errors = Some(Vec::new());
            desktop_style::install(vm, StyleSheet::load_with_appearance(DesktopStyle::Omarchy, false));
            crate::set_theme_mix(vm.cx_mut(), Some(standing.clone()));

            let mut looked = ThemeBuilder::new();
            looked.enter(vm);
            assert!(looked.params().dark, "a sheet written against the dark theme opens the dark page");
            looked.leave(vm);
            assert!(!vm.cx_mut().pending_style_reload);
            assert_eq!(desktop_style::current_name(vm).as_deref(), Some("omarchy"));

            let mut builder = ThemeBuilder::new();
            builder.enter(vm);
            builder.set(blue(true));
            assert_eq!(builder.apply(vm), Applied::Theme);
            assert_eq!(desktop_style::current_name(vm), None, "a built theme stands on its base, not under a sheet");
            the_reload_lands(vm);
            builder.leave(vm);
            assert_eq!(desktop_style::current_name(vm).as_deref(), Some("omarchy"), "leaving did not put the sheet back");
            assert_eq!(crate::theme_mix(vm.cx_mut()), Some(standing.clone()), "nor the script that stood at the seam");
            assert_eq!(crate::base_theme(vm.cx_mut()), BaseTheme::Dark);
            the_reload_lands(vm);
            let errors = vm.take_errors();
            assert!(errors.is_empty(), "{errors:?}");
        });

        // On a light base theme with nothing over it, the light page.
        let mut cx = Cx::new(Box::new(|_, _| {}));
        cx.with_vm(|vm| {
            crate::set_base_theme(vm.cx_mut(), BaseTheme::Light);
            desktop_style::uninstall(vm);
            let mut builder = ThemeBuilder::new();
            builder.enter(vm);
            assert_eq!(builder.params(), BuilderParams::house(false));
        });
    }

    /// A theme picked from outside wins. The app's own picker sets the base
    /// theme, which clears the seam; a builder that finds its script gone
    /// does not put it back, opens again on what was picked, and owes nothing
    /// when it is left.
    #[test]
    fn a_pick_from_outside_stands_the_builder_down() {
        let mut builder = ThemeBuilder::new();
        let mut cx = Cx::new(Box::new(|_, _| {}));
        cx.with_vm(|vm| {
            crate::script_mod(vm);
            desktop_style::uninstall(vm);
            builder.enter(vm);
            builder.set(blue(true));
            assert_eq!(builder.apply(vm), Applied::Theme);
            the_reload_lands(vm);

            crate::set_base_theme(vm.cx_mut(), BaseTheme::Light);
            vm.cx_mut().request_style_reload();
            the_reload_lands(vm);

            // Asked with no word about the rebuild: a pick moves no control,
            // so a builder that looked at its controls first would never see it.
            assert!(!builder.is_dirty());
            assert_eq!(builder.apply(vm), Applied::Nothing, "the builder put itself back over a pick");
            assert_eq!(crate::theme_mix(vm.cx_mut()), None);
            assert!(builder.is_open(), "the section stays open");
            assert_eq!(builder.params(), BuilderParams::house(false), "on the theme that was picked");
            assert!(!builder.is_dirty());
            assert_eq!(builder.rebuilds(), 1);
            builder.leave(vm);
            assert!(!vm.cx_mut().pending_style_reload, "a builder that was stood down took the pick off on its way out");
            assert_eq!(crate::base_theme(vm.cx_mut()), BaseTheme::Light);
        });
    }

    /// Every theme the library ships, made up, so that the lab resolves none
    /// of them and this test does not pay for sixteen module reloads.
    fn made_up_cache() -> BlendCache {
        let mut cache = BlendCache::new();
        for (step, theme) in BlendTheme::all().into_iter().enumerate() {
            let shade = (step as u32) * 0x08;
            let (bg, ink) = match theme.appearance() {
                Appearance::Dark => (0x101010FF + (shade << 24) + (shade << 16) + (shade << 8), 0xEEEEEEFFu32),
                Appearance::Light => (0xF0F0F0FF - (shade << 24) - (shade << 16) - (shade << 8), 0x101010FF),
            };
            let mut values = BTreeMap::new();
            for (key, rgba) in [("color_bg_app", bg), ("color_surface", bg), ("color_on_surface", ink)] {
                values.insert(key.to_string(), BlendValue::Color(rgba));
            }
            cache.insert(ThemeValues::new(theme, values));
        }
        cache
    }

    /// The seam holds one script, so the lab and the builder take turns, and
    /// each turn is well-defined: a builder opened over a mix hands the mix
    /// back when it leaves, and a builder whose theme the lab has mixed over
    /// stands down rather than fighting for the screen.
    #[test]
    fn the_lab_and_the_builder_take_turns() {
        let mut cx = Cx::new(Box::new(|_, _| {}));
        cx.with_vm(|vm| {
            crate::script_mod(vm);
            vm.bx.captured_errors = Some(Vec::new());
            desktop_style::uninstall(vm);
            let mut lab = ThemeLab::with_cache(made_up_cache());
            lab.enter(vm);
            let omarchy = lab.index_of(BlendTheme::Sheet(DesktopStyle::Omarchy, false)).unwrap();
            lab.set_weight(omarchy, 100.0);
            assert!(lab.apply(vm).unwrap().rebuilt());
            the_reload_lands(vm);
            let mix = crate::theme_mix(vm.cx_mut()).expect("the lab's mix is at the seam");

            // The builder opens over the mix and takes the screen...
            let mut builder = ThemeBuilder::new();
            builder.enter(vm);
            builder.set(blue(true));
            assert_eq!(builder.apply(vm), Applied::Theme);
            the_reload_lands(vm);
            assert_ne!(crate::theme_mix(vm.cx_mut()).as_deref(), Some(mix.as_str()));
            // ...the lab, whose weights have not moved, does not fight for it...
            assert!(!lab.apply(vm).unwrap().rebuilt());
            // ...and leaving hands the mix back, text for text.
            builder.leave(vm);
            assert_eq!(crate::theme_mix(vm.cx_mut()).as_deref(), Some(mix.as_str()));
            the_reload_lands(vm);
            assert!(!lab.apply(vm).unwrap().rebuilt(), "the lab is right again about what is in force");

            // The other way round: the lab mixes over a built theme.
            builder.enter(vm);
            builder.set(blue(true));
            assert_eq!(builder.apply(vm), Applied::Theme);
            the_reload_lands(vm);
            lab.set_weight(omarchy, 50.0);
            assert!(lab.apply(vm).unwrap().rebuilt());
            the_reload_lands(vm);
            let remixed = crate::theme_mix(vm.cx_mut());
            assert_eq!(builder.apply(vm), Applied::Nothing, "the builder took the screen back from the lab");
            assert_eq!(crate::theme_mix(vm.cx_mut()), remixed);
            assert_eq!(builder.params(), BuilderParams::house(true), "it opened again, on what is in force");
            builder.leave(vm);
            assert_eq!(crate::theme_mix(vm.cx_mut()), remixed, "and owed nothing on its way out");
            assert!(!vm.cx_mut().pending_style_reload);
            let errors = vm.take_errors();
            assert!(errors.is_empty(), "{errors:?}");
        });
    }

    /// The exported theme is a whole theme file: every key the base has, the
    /// moved globals and the palette as literals, everything else still an
    /// expression, and none of the base file's own tests. Evaluated, it is
    /// the built theme token for token.
    #[test]
    fn the_exported_theme_is_the_built_theme_as_a_file() {
        let params = BuilderParams { saturation: 0.7, spacing: 8.0, font_size: 11.0, ..blue(true) };
        let built = build(&params);
        assert_eq!(built.theme_source("My Sunset"), None, "a name that is no identifier made a file");
        let file = built.theme_source("my_sunset").unwrap();
        assert_eq!(theme_keys(&file), theme_keys(Scheme::Dark.source()), "a key was lost or gained");
        assert!(file.starts_with("use crate::makepad_platform::*;"));
        assert!(file.contains("\n    mod.themes.my_sunset = {\n") && !file.contains("mod.themes.dark"));
        assert!(file.ends_with("    }\n}\n") && !file.contains("#[cfg(test)]"), "the base file's tests came along");
        for (key, value) in built.globals.iter().chain(built.overrides.iter()) {
            assert!(file.contains(&format!("\n        {key}: {}\n", value.render())), "{key} is not a literal in the file");
        }
        assert!(file.contains("\n        space_2: 1.0 * theme.space_factor\n"), "the ladder was flattened");

        let mut cx = Cx::new(Box::new(|_, _| {}));
        cx.with_vm(|vm| {
            crate::script_mod(vm);
            let mut code = theme_script_body(&file);
            code.push_str("true\n");
            evaluate(vm, "exported_theme", code);
            evaluate(vm, "exported_theme_built", built.script_as("sunset_source", "sunset_built", false));
            let (exported, theme) = (filed(vm, "my_sunset"), filed(vm, "sunset_built"));
            let mut compared = 0;
            for key in base_theme_keys() {
                let want = token(vm, theme, key);
                assert_eq!(token(vm, exported, key), want, "{key}");
                compared += want.is_some() as usize;
            }
            assert!(compared > 400, "only {compared} tokens were compared");
        });
    }

    /// The exported sheet is in the shape of the shipped ones, `assigned_keys`
    /// reads it back, and -- the part that matters -- installed over the plain
    /// base theme it IS the built theme: every colour, every number, the
    /// padding and the type, with the roles left as the builder made them
    /// rather than regrown from the base theme's focus blue.
    #[test]
    fn the_exported_sheet_installed_is_the_built_theme() {
        // The house colour as well as the blue. A palette that did NOT move
        // is the one a plain diff leaves out of the sheet, and a role a sheet
        // does not name is regrown for it from the base theme's focus blue:
        // the house orange came back as a blue theme with roomier padding.
        for (dark, favourite, saturation) in
            [(true, BLUE, 0.8), (false, BLUE, 0.8), (true, SeedColors::HOUSE.primary, 0.0)]
        {
            let params = BuilderParams {
                favourite,
                saturation,
                spacing: 8.0,
                roundness: 5.0,
                font_size: 11.0,
                ..BuilderParams::house(dark)
            };
            let built = build(&params);
            let mut cx = Cx::new(Box::new(|_, _| {}));
            cx.with_vm(|vm| {
                crate::script_mod(vm);
                vm.bx.captured_errors = Some(Vec::new());
                desktop_style::uninstall(vm);
                let on_screen = worn(vm);
                let before = color(vm, on_screen, "color_primary");
                let sheet = built.sheet_source(vm);
                let on_screen = worn(vm);
                assert_eq!(color(vm, on_screen, "color_primary"), before, "asking for a sheet moved the theme on the screen");

                let first = format!("mod.theme = mod.themes.{}", built.scheme.theme_name());
                assert_eq!(sheet.lines().next(), Some(first.as_str()));
                assert_eq!(sheet.lines().last(), Some("true"));
                let keys = assigned_keys(&sheet);
                assert_eq!(keys.len(), sheet.lines().count() - 2, "a line of the sheet is not an assignment");
                for key in [
                    "color_primary",
                    "color_on_primary",
                    "color_surface",
                    "space_factor",
                    "space_2",
                    "corner_radius",
                    "font_size_p",
                    "mspace_2",
                    "mspace_h_1",
                    "font_title_l",
                    "font_body_m",
                ] {
                    assert!(keys.contains(&key), "{key} is not in the sheet");
                }
                assert_eq!(keys.contains(&"color_bg_app"), saturation > 0.0, "the page is in the sheet when it leant");
                assert!(!keys.contains(&"motion_short_1"), "a token that did not move was written");
                assert!(
                    sheet.contains("mod.theme.mspace_2 = mod.turtle.Inset{top: 8.0, right: 8.0, bottom: 8.0, left: 8.0}\n"),
                    "{sheet}"
                );

                let scratch = filed(vm, EXPORT_NAME);
                let mut want: Vec<(&str, TokenValue)> = Vec::new();
                for key in base_theme_keys() {
                    if let Some(value) = token(vm, scratch, key) {
                        want.push((key, value));
                    }
                }
                let title = inner(vm, scratch, "font_title_l", "font_size");
                assert_eq!(title, Some(11.0 + 2.0 * 2.5));

                desktop_style::install(
                    vm,
                    StyleSheet { name: "built".to_string(), theme: sheet, widgets: String::new(), icons: Vec::new() },
                );
                vm.with_reload(crate::script_mod);
                let theme = widget_theme(vm);
                for (key, value) in want {
                    assert_eq!(token(vm, theme, key), Some(value), "{key} under the installed sheet");
                }
                assert_eq!(inner(vm, theme, "font_title_l", "font_size"), title);
                assert_eq!(inner(vm, theme, "font_body_s", "font_size"), Some(11.0 - 0.4 * 2.5));
                let errors = vm.take_errors();
                assert!(errors.is_empty(), "{errors:?}");
            });
        }
    }

    /// One colour offers a row of palettes and not one answer, the row is the
    /// same row every time it is asked for, and its front is varied: a panel
    /// showing the first eight shows every harmony there is and every mood
    /// there is, rather than one harmony in four brightnesses and then the
    /// next.
    #[test]
    fn a_favourite_offers_a_row_of_palettes_in_a_varied_order() {
        for dark in [true, false] {
            let made = suggestions(BLUE, dark);
            assert_eq!(made.len(), Harmony::ALL.len() * Mood::ALL.len(), "{dark}");
            assert_eq!(made, suggestions(BLUE, dark), "the same colour asked twice");

            let front = &made[..8];
            for harmony in Harmony::ALL {
                assert!(front.iter().any(|s| s.harmony == Some(harmony)), "{harmony:?} is not in the first eight");
            }
            for mood in Mood::ALL {
                assert!(front.iter().any(|s| s.mood == Some(mood)), "{mood:?} is not in the first eight");
            }
            // Every pair the rule can make, once each, and named in words.
            let mut pairs: Vec<(Harmony, Mood)> =
                made.iter().map(|s| (s.harmony.unwrap(), s.mood.unwrap())).collect();
            pairs.sort_by_key(|(h, m)| (format!("{h:?}"), format!("{m:?}")));
            pairs.dedup();
            assert_eq!(pairs.len(), made.len());
            let triadic = made.iter().find(|s| s.harmony == Some(Harmony::Triadic) && s.mood == Some(Mood::Muted));
            assert_eq!(triadic.unwrap().label, "Triadic, muted");
        }
    }

    /// A favourite with no colour in it has no hue for a harmony to turn, so
    /// every harmony grows the same greys and the row would otherwise be two
    /// dozen copies of one swatch. What is left is the moods that genuinely
    /// move something, and a black -- which the companion band pins to one
    /// lightness -- is down to a single palette.
    #[test]
    fn a_grey_favourite_collapses_to_a_handful() {
        for dark in [true, false] {
            let grey = suggestions(0x808080FF, dark);
            assert_eq!(grey.len(), 3, "{:?}", grey.iter().map(|s| s.label.clone()).collect::<Vec<_>>());
            for suggestion in &grey {
                assert_eq!(suggestion.seeds.neutral.map(|n| rgb_to_hsl(n).1), Some(0.0), "a grey leant the page");
            }
            assert_eq!(suggestions(0x000000FF, dark).len(), 1);
            // And a colour still gets the whole row.
            assert_eq!(suggestions(BLUE, dark).len(), 24);
        }
    }

    /// A suggestion's companions are named outright, and the palette follows
    /// them and not the harmony field beside them. Choosing a harmony by hand
    /// lets them go again, which is the only way a harmony picker can mean
    /// anything once a suggestion has been taken.
    #[test]
    fn a_named_seed_beats_the_harmony_and_a_harmony_lets_it_go() {
        let green = hsl_to_rgb(120.0, 0.7, 0.5);
        let purple = hsl_to_rgb(285.0, 0.7, 0.5);
        let named = BuilderParams {
            harmony: Harmony::House,
            seeds: Some(SuggestionSeeds { secondary: green, tertiary: purple, neutral: None }),
            ..blue(true)
        };
        let built = build(&named);
        let hue = |key: &str| rgb_to_hsl(built.color(key).unwrap()).0;
        assert!(apart(hue("color_secondary"), 120.0) < 4.0, "{}", hue("color_secondary"));
        assert!(apart(hue("color_tertiary"), 285.0) < 4.0, "{}", hue("color_tertiary"));
        // The house offsets are +30/-150 off the blue, which is neither.
        let house = rgb_to_hsl(BLUE).0;
        assert!(apart(hue("color_secondary"), house + 30.0) > 20.0);

        let let_go = named.with_harmony(Harmony::Triadic);
        assert_eq!(let_go.seeds, None);
        let plain = build(&let_go);
        let turned = |key: &str| rgb_to_hsl(plain.color(key).unwrap()).0;
        assert!(apart(turned("color_secondary"), house + 120.0) < 4.0, "{}", turned("color_secondary"));
        assert!(apart(turned("color_tertiary"), house - 120.0) < 4.0);
        // And nothing named is what every setting the sliders can reach says.
        assert_eq!(BuilderParams::house(true).seeds, None);
        assert_eq!(random_params(7).seeds, None);
    }

    /// The fourth swatch is the page, not a guess at it: the theme a
    /// suggestion builds is drawn on exactly the ground the row showed. And
    /// what the person had set that the palette has no business moving --
    /// the appearance, the four dimensions -- is still set.
    #[test]
    fn the_fourth_swatch_is_the_page_the_theme_will_have() {
        for dark in [true, false] {
            let base = BuilderParams { spacing: 9.0, font_size: 11.5, ..BuilderParams::house(dark) };
            for suggestion in suggestions(BLUE, dark) {
                let params = suggestion.params(base);
                assert_eq!(params.dark, dark);
                assert_eq!((params.spacing, params.font_size), (9.0, 11.5));
                assert_eq!(params.favourite, BLUE);
                let built = build(&params);
                assert_eq!(
                    built.color("color_bg_app"),
                    Some(suggestion.colors[3]),
                    "{} on dark={dark}",
                    suggestion.label
                );
                assert_eq!(suggestion.colors[0], BLUE);
            }
        }
    }

    /// The sweep the plain builder goes through, over the suggestions
    /// instead: every palette offered for a hue at ten degree steps, on both
    /// pages, and the greys, builds a theme where every pair the library
    /// holds a theme to meets its bar. A companion named outright cannot
    /// break this -- the rule takes its hue and brings its own ink -- and
    /// this is what says so.
    #[test]
    fn every_suggested_theme_reads() {
        let mut checked = 0;
        for dark in [true, false] {
            for step in 0..36 {
                let favourite = hsl_to_rgb(step as f64 * 10.0, 0.85, 0.5);
                for suggestion in suggestions(favourite, dark) {
                    let built = build(&suggestion.params(BuilderParams::house(dark)));
                    assert_eq!(built.readability.measured, held_pairs().len());
                    assert!(
                        built.readability.holds(),
                        "{} for {favourite:08X} on dark={dark}: {:#?}",
                        suggestion.label,
                        built.readability.failures
                    );
                    assert!(built.readability.margin >= 0.0);
                    checked += 1;
                }
            }
        }
        assert_eq!(checked, 2 * 36 * 24);
        for favourite in [0x000000FFu32, 0x808080FF, 0xFFFFFFFF] {
            for dark in [true, false] {
                for suggestion in suggestions(favourite, dark) {
                    let built = build(&suggestion.params(BuilderParams::house(dark)));
                    assert!(built.readability.holds(), "{favourite:08X}: {:#?}", built.readability.failures);
                }
            }
        }
    }

    /// A scheme is found by whichever of its colours stands nearest the
    /// favourite, that colour is moved to the front, and the schemes come
    /// back nearest first. Nothing near enough is no schemes, not a list of
    /// everything sorted badly.
    #[test]
    fn a_scheme_is_found_by_the_colour_it_holds() {
        let favourite = 0x2E8BF0FF;
        let schemes = vec![
            vec![0xFFA500FF, 0x1E90FFFF, 0x2F4F4FFF],
            vec![0x8B0000FF, 0xFFD700FF],
            vec![0x111111FF, 0x777777FF, favourite, 0xEEEEEEFF],
        ];
        let found = matching_schemes(favourite, &schemes, OWN_TOLERANCE);
        assert_eq!(found.len(), 2, "{found:X?}");
        assert_eq!(found[0], vec![favourite, 0x111111FF, 0x777777FF, 0xEEEEEEFF]);
        assert_eq!(found[1], vec![0x1E90FFFF, 0xFFA500FF, 0x2F4F4FFF]);
        assert_eq!(color_distance(favourite, favourite), 0.0);
        // The short way round the circle: two reds either side of nought.
        assert!(color_distance(hsl_to_rgb(355.0, 0.8, 0.5), hsl_to_rgb(5.0, 0.8, 0.5)) < 11.0);
        assert!(matching_schemes(0x00FF00FF, &schemes, 5.0).is_empty());
        assert!(matching_schemes(favourite, &[], OWN_TOLERANCE).is_empty());
    }

    /// A scheme moved onto a favourite keeps its shape: the front colour
    /// becomes the favourite itself and every other keeps the distance it
    /// stood at, in all three parts of a colour.
    #[test]
    fn an_adjusted_scheme_starts_on_the_favourite_and_keeps_its_offsets() {
        let scheme = [0x3366CCFF, 0x66CC33FF, 0xCC3366FF];
        let favourite = 0xE07020FF;
        let adjusted = adjust_scheme(favourite, &scheme);
        assert_eq!(adjusted.len(), 3);
        assert_eq!(adjusted[0], favourite);
        let (anchor_hue, anchor_colour, anchor_light) = rgb_to_hsl(scheme[0]);
        let (hue, colour, light) = rgb_to_hsl(favourite);
        for (at, member) in scheme.iter().enumerate() {
            let (was_hue, was_colour, was_light) = rgb_to_hsl(*member);
            let (now_hue, now_colour, now_light) = rgb_to_hsl(adjusted[at]);
            assert!(apart(now_hue, hue + (was_hue - anchor_hue)) < 1.0, "{at}: {now_hue}");
            assert!((now_colour - (colour + (was_colour - anchor_colour))).abs() < 0.01, "{at}: {now_colour}");
            assert!((now_light - (light + (was_light - anchor_light))).abs() < 0.01, "{at}: {now_light}");
        }
        assert!(adjust_scheme(favourite, &[]).is_empty());
    }

    /// A hue that goes below nought comes back round the way it went. Turning
    /// it into `360 - h` instead mirrors the scheme -- a step of forty
    /// degrees back lands forty degrees ON -- and hands back a palette
    /// nobody wrote, in silence.
    #[test]
    fn a_hue_that_went_below_nought_comes_back_round_the_way_it_went() {
        let at = |degrees: f64| hsl_to_rgb(degrees, 0.8, 0.5);
        let hue = |rgba: u32| rgb_to_hsl(rgba).0;
        let scheme = [at(200.0), at(160.0), at(240.0)];
        let adjusted = adjust_scheme(at(10.0), &scheme);
        assert!(apart(hue(adjusted[1]), 330.0) < 1.0, "forty degrees back from ten is 330, not {}", hue(adjusted[1]));
        assert!(apart(hue(adjusted[2]), 50.0) < 1.0, "{}", hue(adjusted[2]));
        // And the other end of the circle, where a step forward runs past it.
        let over = adjust_scheme(at(350.0), &scheme);
        assert!(apart(hue(over[1]), 310.0) < 1.0, "{}", hue(over[1]));
        assert!(apart(hue(over[2]), 30.0) < 1.0, "{}", hue(over[2]));
    }

    /// A lightness that leaves the range takes the whole scheme with it
    /// rather than piling up at the end: the order holds, the spacing holds,
    /// and everything lands inside nought and one.
    #[test]
    fn lightness_that_leaves_the_range_moves_the_whole_scheme_together() {
        let at = |light: f64| hsl_to_rgb(200.0, 0.6, light);
        let light = |rgba: u32| rgb_to_hsl(rgba).2;
        // Offsets of nought, +0.30 and +0.50 laid on a favourite at 0.70
        // want 0.70, 1.00 and 1.20; the three come back over 0.70..1.00.
        let adjusted = adjust_scheme(at(0.70), &[at(0.20), at(0.50), at(0.70)]);
        assert!(light(adjusted[0]) < light(adjusted[1]) && light(adjusted[1]) < light(adjusted[2]));
        assert!(adjusted.iter().all(|c| (0.0..=1.0).contains(&light(*c))));
        assert!((light(adjusted[0]) - 0.70).abs() < 0.01, "{}", light(adjusted[0]));
        assert!((light(adjusted[1]) - 0.88).abs() < 0.01, "{}", light(adjusted[1]));
        assert!((light(adjusted[2]) - 1.0).abs() < 0.01, "{}", light(adjusted[2]));
        // The other end, and the front colour moves with the rest: it is a
        // member of the scheme, not an anchor the scheme hangs off.
        let below = adjust_scheme(at(0.30), &[at(0.60), at(0.30), at(0.10)]);
        assert!(below.iter().all(|c| (0.0..=1.0).contains(&light(*c))));
        assert!(light(below[0]) > light(below[1]) && light(below[1]) > light(below[2]));
        assert!((light(below[2]) - 0.0).abs() < 0.01, "{}", light(below[2]));
        // Nothing out of range is nothing moved.
        let inside = adjust_scheme(at(0.50), &[at(0.40), at(0.60)]);
        assert_eq!(inside[0], at(0.50));
    }

    /// A person's own schemes come after the ones the rule grew, labelled as
    /// theirs, anchored on the colour they picked, and filled out to four
    /// places however short the line they wrote was -- and they go through
    /// the same bar and the same dropping as the rest.
    #[test]
    fn a_persons_own_palettes_come_after_the_rules() {
        let favourite = 0x2E8BF0FF;
        let own = vec![vec![0xFFA500FF, 0x1E90FFFF, 0x2F4F4FFF], vec![0x8B0000FF, 0xFFD700FF]];
        let grown = suggestions(favourite, true);
        let all = all_suggestions(favourite, true, &own);
        assert_eq!(all[..grown.len()], grown[..]);
        assert_eq!(theirs(&all), 1, "only the scheme holding the colour is offered");
        let mine = all.last().unwrap();
        assert_eq!(mine.label, OWN_LABEL);
        assert_eq!((mine.harmony, mine.mood), (None, None));
        assert_eq!(mine.colors[0], favourite);
        assert!(build(&mine.params(BuilderParams::house(true))).readability.holds());

        // Two colours is a scheme; the other two places are filled for it.
        let short = all_suggestions(favourite, false, &[vec![0x1E90FFFF, 0x20C020FF]]);
        let filled = short.last().unwrap();
        assert_eq!(filled.label, OWN_LABEL);
        assert!(filled.colors.iter().all(|c| c & 0xFF == 0xFF), "{:08X?}", filled.colors);
        // The second swatch is their second colour, turned onto the
        // favourite: the offset they wrote, not a hue the rule chose.
        let turned = rgb_to_hsl(favourite).0 + (rgb_to_hsl(0x20C020FF).0 - rgb_to_hsl(0x1E90FFFF).0);
        assert!(apart(rgb_to_hsl(filled.colors[1]).0, turned) < 1.0, "{}", rgb_to_hsl(filled.colors[1]).0);
        assert_eq!(Some(filled.seeds.tertiary), filled.colors.get(2).copied());
        assert!(build(&filled.params(BuilderParams::house(false))).readability.holds());

        // And a scheme that says what the rule already said is not said twice.
        let doubled = all_suggestions(favourite, true, &[vec![favourite, grown[0].colors[1], grown[0].colors[2]]]);
        assert_eq!(theirs(&doubled), 0, "a person's copy of a grown palette was offered again");
        assert_eq!(theirs(&all_suggestions(favourite, true, &[])), 0);
    }

    /// How many of these palettes came off the person's own file.
    fn theirs(offered: &[Suggestion]) -> usize {
        offered.iter().filter(|offer| offer.label == OWN_LABEL).count()
    }

    /// How many of these palettes came out of the built-in table.
    fn book(offered: &[Suggestion]) -> Vec<&Suggestion> {
        offered.iter().filter(|offer| offer.label.starts_with(COMBINATION_LABEL)).collect()
    }

    /// A colour lifted straight out of a row of the table finds that row, and
    /// finds it before any other: nothing stands nearer to a colour than the
    /// row it was taken from. It comes back under the number the book gives
    /// it, which is the only thing about it a person can look up.
    ///
    /// The rows are ones whose first colour stands in no earlier row, so that
    /// "first" has one answer rather than two equally near ones.
    #[test]
    fn a_colour_out_of_a_row_finds_that_row_first() {
        // Three-colour rows and four-colour rows, by their numbers.
        for number in [121usize, 122, 123, 124, 125, 251, 257, 284, 299] {
            let at = number - FIRST_COMBINATION;
            let favourite = COMBINATIONS[at][0];
            for dark in [true, false] {
                let offered = all_suggestions(favourite, dark, &[]);
                let found = book(&offered);
                assert!(!found.is_empty(), "{number} did not find itself on dark={dark}");
                assert_eq!(found[0].label, combination_label(at), "on dark={dark}");
                assert_eq!(found[0].colors[0], favourite | 0xFF);
                assert_eq!((found[0].harmony, found[0].mood), (None, None));
                assert!(found.len() <= MOST_COMBINATIONS, "{} kept", found.len());
            }
        }
    }

    /// The sweep the rule's own palettes go through, over the built-in
    /// combinations instead: a hue every ten degrees, both pages, and every
    /// combination the colour finds builds a theme where each pair the library
    /// holds a theme to meets its bar.
    ///
    /// The favourite is a middling colour rather than a full one. The book is
    /// printed in inks and most of its colours are muted, so a sweep at the
    /// full saturation the rule's sweep uses would find almost no combinations
    /// and pass by checking nothing -- which is why the count is asserted at
    /// the end as well.
    #[test]
    fn every_combination_theme_reads() {
        let mut checked = 0;
        for dark in [true, false] {
            for step in 0..36 {
                let favourite = hsl_to_rgb(step as f64 * 10.0, 0.55, 0.5);
                for suggestion in book(&all_suggestions(favourite, dark, &[])) {
                    let built = build(&suggestion.params(BuilderParams::house(dark)));
                    assert_eq!(built.readability.measured, held_pairs().len());
                    assert!(
                        built.readability.holds(),
                        "{} for {favourite:08X} on dark={dark}: {:#?}",
                        suggestion.label,
                        built.readability.failures
                    );
                    assert!(built.readability.margin >= 0.0);
                    checked += 1;
                }
            }
        }
        assert!(checked > 300, "the sweep only found {checked} combinations to check");
    }

    /// The six ways three colours can be written down.
    const WRITTEN_WAYS: [[usize; 3]; 6] =
        [[0, 1, 2], [0, 2, 1], [1, 0, 2], [1, 2, 0], [2, 0, 1], [2, 1, 0]];

    /// The one suggestion in `offered` that came off the list, whatever it is
    /// called.
    fn off_the_list<'a>(offered: &'a [Suggestion], label: &str) -> &'a Suggestion {
        offered.iter().find(|offer| offer.label == label).expect("the scheme was not offered at all")
    }

    /// A list has no roles in it, only an order somebody happened to type, so
    /// the same four colours written six ways must come out as one palette:
    /// the palest is the page, and of the two left the one nearer the
    /// favourite round the circle is the secondary and the farther the
    /// tertiary.
    #[test]
    fn a_scheme_takes_its_roles_from_the_colours_and_not_the_written_order() {
        let favourite = hsl_to_rgb(210.0, 0.7, 0.5);
        let near = hsl_to_rgb(250.0, 0.7, 0.5);
        let far = hsl_to_rgb(40.0, 0.75, 0.5);
        let ground = hsl_to_rgb(100.0, 0.12, 0.5);
        let rest = [near, far, ground];
        for way in WRITTEN_WAYS {
            let written: Vec<u32> = std::iter::once(favourite).chain(way.map(|at| rest[at])).collect();
            // And the anchor itself written in every place, because a list
            // does not put the colour somebody will pick first either.
            for turn in 0..written.len() {
                let mut scheme = written.clone();
                scheme.rotate_left(turn);
                let offered = all_suggestions(favourite, true, &[scheme.clone()]);
                let mine = off_the_list(&offered, OWN_LABEL);
                let hue = |packed: u32| rgb_to_hsl(packed).0;
                assert!(apart(hue(mine.seeds.secondary), 250.0) < 1.0, "{scheme:08X?} took {:08X} as the secondary", mine.seeds.secondary);
                assert!(apart(hue(mine.seeds.tertiary), 40.0) < 1.0, "{scheme:08X?} took {:08X} as the tertiary", mine.seeds.tertiary);
                let lean = mine.seeds.neutral.expect("a four colour scheme names the page's lean");
                assert!(apart(hue(lean), 100.0) < 1.0, "{scheme:08X?} leant the page {lean:08X}");
            }
        }
    }

    /// Three colours are the three accent families exactly, so none of them
    /// is taken for the page -- the fill supplies that -- and the two that are
    /// not the favourite go near-then-far like the four's do. A colour with no
    /// colour in it cannot be near anything, so it sorts farthest and lands in
    /// the contrast place.
    #[test]
    fn three_colours_keep_their_accents_and_a_grey_sorts_farthest() {
        let favourite = hsl_to_rgb(210.0, 0.7, 0.5);
        let near = hsl_to_rgb(250.0, 0.7, 0.5);
        let far = hsl_to_rgb(40.0, 0.75, 0.5);
        let fill = suggestions(favourite, true)[0].seeds.neutral;
        for way in [[0, 1], [1, 0]] {
            let rest = [near, far];
            let scheme: Vec<u32> = std::iter::once(favourite).chain(way.map(|at| rest[at])).collect();
            let offered = all_suggestions(favourite, true, &[scheme.clone()]);
            let mine = off_the_list(&offered, OWN_LABEL);
            assert!(apart(rgb_to_hsl(mine.seeds.secondary).0, 250.0) < 1.0, "{scheme:08X?}");
            assert!(apart(rgb_to_hsl(mine.seeds.tertiary).0, 40.0) < 1.0, "{scheme:08X?}");
            assert_eq!(mine.seeds.neutral, fill, "a three colour scheme took a page tint it does not have");
        }
        // The grey stands farther from the favourite than a hue on the far
        // side of the circle does, whichever way round the two are written.
        let grey = 0x808080FF;
        for scheme in [vec![favourite, grey, near], vec![favourite, near, grey]] {
            let offered = all_suggestions(favourite, true, &[scheme.clone()]);
            let mine = off_the_list(&offered, OWN_LABEL);
            assert!(apart(rgb_to_hsl(mine.seeds.secondary).0, 250.0) < 1.0, "{scheme:08X?} made the grey the secondary");
            assert_eq!(rgb_to_hsl(mine.seeds.tertiary).1, 0.0, "{scheme:08X?} did not put the grey across the circle");
        }
    }
}
