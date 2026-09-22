//! The theme builder: a whole theme grown from one favourite colour and a
//! handful of sliders, with the part a panel would otherwise have to remember
//! kept in one place.
//!
//! [`crate::theme_tokens`] already holds the rule. It grows seven families of
//! accent roles, from a seed or from colours named outright
//! (`roles_from_colors`), and it can build a base theme again from its own
//! source with different globals, so that spacing, roundness, the type ladder
//! and the page genuinely move rather than being pinned one rung at a time.
//! What it leaves to its caller is what a person would call the controls.
//!
//! # What the controls mean
//!
//! A palette is FOUR colours: the primary, the secondary, the tertiary, and
//! the background colour ([`BuilderParams::palette`]). The first three are the
//! accents, and they are the colours themselves: the primary IS the colour
//! the person picked -- hue, saturation and lightness -- moved along the
//! lightness axis only where it could not otherwise be told from the page,
//! and the companions are the colours the chip showed. No slider moves them.
//!
//! The house theme is the one exception, and by construction: with the
//! palette untouched nothing is installed at all, so the roles the rule grew
//! for the theme files stand, and they are not the house colour the colour
//! control shows. The moment the palette moves, what the swatch shows is what
//! the primary is.
//!
//! The fourth colour is the page's, and the two background sliders are about
//! it alone. Saturation is how much of that colour's own saturation the page
//! and every ground stepped off it take -- all of it at the top, a grey of the
//! same lightness at nought -- and Lightness is how light the page is, over
//! the whole range, which makes it the control of whether the theme is a dark
//! one or a light one. Text contrast is the contrast ratio the body text
//! holds against that page. See [`BuilderParams`] for each.
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
//! # The older tokens
//!
//! A palette that only moved the roles moved almost nothing a person could
//! see. Two thirds of the widget files read no role at all, and the classic
//! controls -- the check box, the radio, the text field, the tab, the drop
//! down, the scroll bar -- read none between them, so a theme grown from an
//! orange favourite came out with an orange page, grey controls and the one
//! focus blue both base theme files have always had. [`ACCENTED`] is the
//! mapping that fixes it: a table from the built roles to the older tokens
//! those controls do read, with all three brand families spent deliberately
//! -- the primary on the value and the main action, the secondary on what is
//! selected or on, the tertiary on what is being pointed out -- so that an
//! ordinary screen of ordinary controls shows the palette rather than one
//! colour of it. Read that table's own doc for what is in it and what is
//! not. None of it touches a widget file: which token a widget reads is not
//! this panel's to change.
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
    base_theme_keys, held_pairs, hsl_to_rgb, over, reads_on, rgb_to_hsl, roles_for, roles_from_colors,
    theme_module_script, theme_script_body, theme_source_with_globals, token_spec, Appearance, BlendTheme,
    RoleSource, DERIVED_ROLES, KEPT_ERROR, KEPT_WARNING, LEGIBLE, READABLE,
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

/// Where the lightness slider's travel ends, as CIE lightness (`L*`, nought
/// for black to a hundred for white): the darkest page a dark theme is
/// offered and the two ends of a light one.
///
/// The LIGHTEST dark page is not here because it is not a choice: it is the
/// house dark page itself (`L*` 32.3), and [`dark_lightest`] reads it off the
/// file. The dark theme's inks are white and its brightest rung,
/// `color_surface_bright`, is the page carried a sixth of the way to white,
/// so pure white body text already stands only 4.54 off that rung on the
/// house page -- a hair over the 4.5 it is held to. One step lighter and no
/// ink the dark base theme can draw reads there.
///
/// The darkest LIGHT page is where the same thing happens from the other
/// side: pure black on the light theme's darkest rung, `color_surface_dim`,
/// which is the page carried a quarter of the way to black. A grey page
/// clears 4.5 from about `L*` 66.4, and a fully saturated page of the same
/// luminance needs up to about 67.6 depending on its hue, so the light end
/// starts at 68. Between 32.3 and 68 there is no page either base theme can
/// write readable text on, and the slider does not offer one: see
/// [`BuilderParams::lightness`].
///
/// The outer ends stop short of black and white because a page at either has
/// no room left in it for colour, and the saturation slider would do nothing
/// there at all.
const DARK_DARKEST: f64 = 8.0;
const LIGHT_DARKEST: f64 = 68.0;
const LIGHT_LIGHTEST: f64 = 98.0;

/// How much colour the rule gives the background of a palette it grows, as
/// the bounds on an HSL saturation taken from the favourite. The background
/// is the colour the saturation slider brings the page up to at its top, so
/// a rule that handed out a nearly grey one would leave the slider with
/// nothing to do; and a favourite that is already a highlighter pen does not
/// make its page one.
const RULE_GROUND_COLOUR: (f64, f64) = (0.45, 0.85);

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
/// background sliders run nought to one; the text contrast is a contrast
/// ratio; the four dimensions are in the units of the globals they drive,
/// because a person dragging "font size" wants to read 11 and not 0.37.
///
/// # Four colours, and which slider moves which
///
/// A palette is four colours: the primary, the secondary, the tertiary and
/// the BACKGROUND colour ([`BuilderParams::palette`]). The first three are
/// the accents, and no slider touches them. The fourth is what the page and
/// every ground stepped off it are coloured with, and the two background
/// sliders are about it and about nothing else:
///
/// * [`saturation`](BuilderParams::saturation) is how much of the
///   background colour's own saturation the grounds take -- all of it at the
///   top, which is the default, and none at nought, where the page is the
///   GREY of the same lightness. It never goes past the palette's own
///   colour, and it moves chroma alone.
/// * [`lightness`](BuilderParams::lightness) is how light or dark the page
///   is, and it alone decides that -- which makes it the control that decides
///   whether the theme is a dark one or a light one at all.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BuilderParams {
    /// The one colour somebody picked, `0xRRGGBBAA`, alpha ignored. Every
    /// suggestion is grown from it and a colour control shows it. It is the
    /// primary itself unless a mood suggestion has named a calmer primary in
    /// [`seeds`](BuilderParams::seeds) -- and choosing that suggestion does
    /// not move this, so the colour the person picked is never lost to a
    /// chip they were only trying out.
    pub favourite: u32,
    /// Where the secondary and tertiary sit round the hue circle from the
    /// favourite, for a palette the rule grows. A suggestion carries one; the
    /// panel no longer offers it as a control of its own, since the six
    /// harmonies are the first six suggestions.
    pub harmony: Harmony,
    /// The four colours a [`Suggestion`] named, or [`BuilderParams::with_palette`]
    /// named by hand, outright: the primary as it will be, the two
    /// companions, and the background colour.
    ///
    /// `None` is the default and means the palette is the plain one the
    /// harmony grows from the favourite. A seed named outright beats the
    /// harmony, so choosing a harmony by hand has to let these go or it
    /// would do nothing at all; [`BuilderParams::with_harmony`] is that move.
    pub seeds: Option<SuggestionSeeds>,
    /// How much of the background colour's own saturation the page and the
    /// controls' grounds carry, nought to one. One is the default: the first
    /// palette a person presses puts all four of its colours on the app at
    /// once, and this is how they take the colour back out of the grounds.
    /// Nought is a page with no colour in it at exactly the lightness it had.
    pub saturation: f64,
    /// How light the page is, nought (the darkest dark page) to one (the
    /// lightest light one), even to the eye along its whole travel.
    ///
    /// The lower half is the dark theme and the upper half the light one:
    /// nought to a half runs the page from `L*` 8 up to the house dark page,
    /// which is as light as a page carrying the dark theme's white text can
    /// be; just past a half it is `L*` 68, the darkest page the light
    /// theme's black text reads on, and one is `L*` 98. The band between
    /// (`L*` 32.3 to 68) is a page neither theme can write on, so the slider
    /// steps across it rather than offering a theme that does not read -- and
    /// the appearance, [`BuilderParams::dark`], flips there and nowhere else.
    /// A half is the house dark page exactly; the house light page is where
    /// `BuilderParams::house(false)` says.
    pub lightness: f64,
    /// The contrast ratio the body text holds against the page it is written
    /// on, from `READABLE` (4.5) up to what pure white or pure black would
    /// give on that page. The house value is where the house theme's body
    /// text already sits, so leaving this alone leaves the text alone.
    ///
    /// It moves the text inks and nothing else, and the quieter voices --
    /// the placeholder, the meta line, the disabled label, the variant ink --
    /// keep their distance from the body ink rather than all landing on one
    /// number. It is not `font_contrast`, which is the step between type
    /// sizes, and it is not the theme's `color_contrast`, which moves every
    /// rung of the grey ladder, fills and bevels included.
    pub text_contrast: f64,
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
    /// the house harmony with nothing named, the page at the house lightness
    /// and the text at the house contrast -- both read off the file -- and
    /// the four dimensions as the base theme's own file has them. Read, not
    /// remembered, so a change to a theme file moves the builder's idea of
    /// "untouched" with it.
    ///
    /// The saturation is at its top, where a palette puts all four of its
    /// colours on. The house palette's background colour has no colour in
    /// it, so at the house settings that colours nothing.
    pub fn house(dark: bool) -> Self {
        let scheme = if dark { Scheme::Dark } else { Scheme::Light };
        let of = |key: &str, otherwise: f64| file_number(scheme, key).unwrap_or(otherwise);
        Self {
            favourite: SeedColors::HOUSE.primary,
            harmony: Harmony::House,
            seeds: None,
            saturation: 1.0,
            lightness: house_lightness(dark),
            text_contrast: house_text_contrast(scheme),
            spacing: of("space_factor", 6.0),
            roundness: of("corner_radius", 2.5),
            font_size: of("font_size_base", 10.0),
            font_contrast: of("font_size_contrast", 2.5),
        }
    }

    /// Whether this is a dark theme. Not a setting: the lightness slider is
    /// the only control of it, and this is which half of the slider it is in.
    pub fn dark(&self) -> bool {
        !(self.lightness > 0.5)
    }

    /// The base theme a build derives from.
    pub fn scheme(&self) -> Scheme {
        if self.dark() {
            Scheme::Dark
        } else {
            Scheme::Light
        }
    }

    /// Every number brought inside what it can mean: the two background
    /// sliders into nought to one, the text contrast into what the page
    /// allows, and each dimension into the range its global is registered
    /// with in `THEME_TOKENS`, which is the range the token's own control
    /// offers. [`build`] does this for itself; it is public so that a panel
    /// can show the number that will actually be used.
    pub fn clamped(self) -> Self {
        let within = |key: &str, value: f64| match token_spec(key) {
            Some(spec) if spec.max > spec.min => value.clamp(spec.min, spec.max),
            _ => value,
        };
        let unit = |value: f64, lost: f64| if value.is_finite() { value.clamp(0.0, 1.0) } else { lost };
        let mut out = Self {
            saturation: unit(self.saturation, 1.0),
            lightness: unit(self.lightness, house_lightness(true)),
            spacing: within("space_factor", self.spacing),
            roundness: within("corner_radius", self.roundness),
            font_size: within("font_size_base", self.font_size),
            font_contrast: within("font_size_contrast", self.font_contrast),
            ..self
        };
        let (least, most) = out.text_contrast_range();
        out.text_contrast = if self.text_contrast.is_finite() {
            self.text_contrast.clamp(least, most)
        } else {
            house_text_contrast(out.scheme()).clamp(least, most)
        };
        out
    }

    /// The range the text contrast can be set in on the page these settings
    /// make: `READABLE` at the bottom, and at the top what the plain end of
    /// the appearance -- white on a dark page, black on a light one -- stands
    /// off it. A panel draws its track over this.
    pub fn text_contrast_range(&self) -> (f64, f64) {
        let page = page_of(self);
        let most = reads_on(page, plain_ink(self.scheme()));
        (READABLE, most.max(READABLE))
    }

    /// The seed the palette is grown from. The house settings are the house
    /// seed, field for field; any other palette names all four of its colours
    /// in it, the background colour as the neutral.
    pub fn seed(&self) -> SeedColors {
        if !palette_moved(self) {
            return SeedColors::HOUSE;
        }
        let [primary, secondary, tertiary, background] = self.palette();
        SeedColors {
            primary,
            secondary: Some(secondary),
            tertiary: Some(tertiary),
            neutral: Some(background),
            error: None,
            harmony: self.harmony,
        }
    }

    /// The same settings in another harmony, with any colours a suggestion
    /// named let go.
    ///
    /// Both halves matter. A harmony only decides the hues the rule works out
    /// for itself, so setting one over a suggestion's named seeds would move
    /// a picker and leave the palette exactly where it was -- the one thing a
    /// control must never do.
    pub fn with_harmony(self, harmony: Harmony) -> Self {
        Self { harmony, seeds: None, ..self }
    }

    /// The four colours in force: the primary as it will be, the secondary,
    /// the tertiary, and the background colour -- which is the fourth colour
    /// at its own saturation and at the appearance's house page lightness,
    /// the square a palette chip shows.
    ///
    /// For a suggestion's settings this is that suggestion's
    /// [`colors`](Suggestion::colors), exactly. The built theme's
    /// `color_primary`, `color_secondary` and `color_tertiary` are the first
    /// three to within what reading forced, and its page wears the fourth's
    /// hue.
    ///
    /// The house theme is the one exception, and by construction: with the
    /// palette untouched nothing is installed at all, so the roles the rule
    /// grew for the theme files stand, and those are what this hands back,
    /// with the house page -- a grey -- as the fourth. The moment the palette
    /// moves, the primary IS the colour picked.
    pub fn palette(&self) -> [u32; 4] {
        if let Some(seeds) = self.seeds {
            return [seeds.primary, seeds.secondary, seeds.tertiary, seeds.background];
        }
        if !palette_moved(self) {
            let roles = roles_for(self.scheme());
            return [roles.primary.base, roles.secondary.base, roles.tertiary.base, house_page(self.scheme())];
        }
        grown(self.favourite, self.dark(), self.harmony, None).colors
    }

    /// These settings with all four colours named outright, as a person
    /// editing the palette's squares by hand names them: the first becomes
    /// the favourite and the primary, plain and with no mood; the other three
    /// become the named seeds, the fourth as the background colour. Whatever
    /// a suggestion had named goes. The sliders and the dimensions stay.
    ///
    /// Literal colours are used as they are, moved only as far as reading
    /// demands, so `with_palette(p).palette() == p`, and a suggestion's
    /// settings and `with_palette(suggestion.colors)` build the same theme.
    pub fn with_palette(self, palette: [u32; 4]) -> Self {
        let [primary, secondary, tertiary, background] = palette.map(|color| color | 0xFF);
        Self {
            favourite: primary,
            harmony: Harmony::House,
            seeds: Some(SuggestionSeeds { primary, secondary, tertiary, background }),
            ..self
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

// ---------------------------------------------------------------------------
// The page: a colour at a lightness
// ---------------------------------------------------------------------------

/// Relative luminance, the quantity a contrast ratio is made of: each channel
/// taken out of its display curve and weighted by how bright the eye finds
/// it. `theme_tokens::contrast` is built on the same sum.
fn luminance(rgba: u32) -> f64 {
    let channel = |shift: u32| {
        let v = ((rgba >> shift) & 0xFF) as f64 / 255.0;
        if v <= 0.03928 { v / 12.92 } else { ((v + 0.055) / 1.055).powf(2.4) }
    };
    0.2126 * channel(24) + 0.7152 * channel(16) + 0.0722 * channel(8)
}

/// CIE lightness from relative luminance: the scale on which equal steps look
/// equal, which is what a slider's travel has to be spent on.
fn lstar_of(y: f64) -> f64 {
    if y > 216.0 / 24389.0 {
        116.0 * y.cbrt() - 16.0
    } else {
        y * 24389.0 / 27.0
    }
}

/// Relative luminance from CIE lightness.
fn luminance_at(lstar: f64) -> f64 {
    if lstar > 8.0 {
        ((lstar + 16.0) / 116.0).powi(3)
    } else {
        lstar * 27.0 / 24389.0
    }
}

/// The page the base theme's own file makes, with no tint: `color_bg_app` at
/// the house settings.
fn house_page(scheme: Scheme) -> u32 {
    grounds(scheme, WHITE, 0.0).0
}

/// How light the lightest dark page is: the house dark page's own, read off
/// the file. See [`DARK_DARKEST`] for why it is also the edge.
fn dark_lightest() -> f64 {
    lstar_of(luminance(house_page(Scheme::Dark)))
}

/// The page's CIE lightness at a place on the slider. Two even runs, one per
/// appearance, meeting at a half with the unreadable band between them left
/// out.
fn lightness_lstar(lightness: f64) -> f64 {
    let v = lightness.clamp(0.0, 1.0);
    if v <= 0.5 {
        DARK_DARKEST + (dark_lightest() - DARK_DARKEST) * (v / 0.5)
    } else {
        LIGHT_DARKEST + (LIGHT_LIGHTEST - LIGHT_DARKEST) * ((v - 0.5) / 0.5)
    }
}

/// Where on the slider the house page of an appearance is. A half for the
/// dark one, exactly, because the dark run ends on it.
fn house_lightness(dark: bool) -> f64 {
    if dark {
        return 0.5;
    }
    let lstar = lstar_of(luminance(house_page(Scheme::Light)));
    0.5 + 0.5 * (lstar - LIGHT_DARKEST) / (LIGHT_LIGHTEST - LIGHT_DARKEST)
}

/// The colour of `hue` at HSL saturation `sat` whose luminance is `y`, or the
/// nearest the eight bits allow.
///
/// The lightness is solved for rather than set, and that is the whole point.
/// HSL's own lightness is a lie about brightness: a pure blue and a pure
/// yellow are both "fifty per cent" and one is five times as bright as the
/// other, so a page that kept its HSL lightness while its colour came up or
/// went out would brighten and darken under the saturation slider -- which is
/// exactly what the slider must not do. Holding the luminance holds what the
/// eye calls lightness, and it holds every contrast ratio against the page
/// with it, since those are made of nothing else.
///
/// No saturation is the grey of that luminance. So the grey at the bottom of
/// the saturation slider is the colour at its top with the colour taken out,
/// and not black, white, or the appearance's stock page.
fn at_luminance(hue: f64, sat: f64, y: f64) -> u32 {
    if sat <= 0.0 {
        let grey = |g: u32| (g << 24) | (g << 16) | (g << 8) | 0xFF;
        return (0..=255u32)
            .map(grey)
            .min_by(|a, b| (luminance(*a) - y).abs().partial_cmp(&(luminance(*b) - y).abs()).unwrap())
            .unwrap_or(BLACK);
    }
    // Luminance only rises with HSL lightness at a fixed hue and saturation,
    // so halving the interval finds it.
    let (mut low, mut high) = (0.0f64, 1.0f64);
    for _ in 0..40 {
        let middle = (low + high) / 2.0;
        if luminance(hsl_to_rgb(hue, sat, middle)) < y {
            low = middle;
        } else {
            high = middle;
        }
    }
    let (under, over) = (hsl_to_rgb(hue, sat, low), hsl_to_rgb(hue, sat, high));
    if (luminance(under) - y).abs() <= (luminance(over) - y).abs() {
        under
    } else {
        over
    }
}

/// The colour a palette chip shows for a background colour: its own hue and
/// saturation at the house page lightness of the appearance. What the page
/// is with both sliders where they open.
fn ground_swatch(background: u32, dark: bool) -> u32 {
    let (hue, sat, _) = rgb_to_hsl(background | 0xFF);
    let scheme = if dark { Scheme::Dark } else { Scheme::Light };
    at_luminance(hue, sat, luminance(house_page(scheme)))
}

/// The plain end of an appearance: the ink its base theme writes in.
fn plain_ink(scheme: Scheme) -> u32 {
    match scheme {
        Scheme::Light => BLACK,
        _ => WHITE,
    }
}

/// `color_fg_app` for a page: the page scaled by the file's own ratio between
/// its two fractions, which is what the file's multiply makes of a tinted
/// page -- 1.2 of it in the dark theme, 0.97 in the light one. Scaling keeps
/// the hue and the saturation, so the panel ground is the page a step off
/// and not a second colour.
fn fg_of(scheme: Scheme, page: u32) -> u32 {
    let ratio = match scheme {
        Scheme::Light => (1.0 - LIGHT_GROUNDS.1) / (1.0 - LIGHT_GROUNDS.0),
        _ => DARK_GROUNDS.1 / DARK_GROUNDS.0,
    };
    let channel = |shift: u32| ((((page >> shift) & 0xFF) as f64 * ratio).round().min(255.0)) as u32;
    (channel(24) << 24) | (channel(16) << 16) | (channel(8) << 8) | 0xFF
}

/// Everything a page is the ground of, as the file derives it from
/// `color_bg_app` and `color_fg_app`: the two opaque rungs the inverse page
/// is made of, and the surface ladder.
fn page_colors(scheme: Scheme, bg: u32, fg: u32) -> BTreeMap<String, u32> {
    let mut colors: BTreeMap<String, u32> = BTreeMap::new();
    colors.insert("color_bg_app".to_string(), bg);
    colors.insert("color_fg_app".to_string(), fg);
    colors.insert("color_opaque_u_6".to_string(), vm_mix(fg, WHITE, OPAQUE_U_6));
    colors.insert("color_opaque_d_5".to_string(), vm_mix(fg, BLACK, OPAQUE_D_5));
    for (role, light, dark) in DERIVED_ROLES {
        let known = |key: &str| colors.get(key).copied();
        let value = match if scheme == Scheme::Dark { *dark } else { *light } {
            RoleSource::Token(t) => known(t),
            RoleSource::HalfMix(a, b) => known(a).zip(known(b)).map(|(a, b)| vm_mix(a, b, 0.5)),
            RoleSource::MixTo(t, end, amount) => known(t).map(|c| vm_mix(c, end, amount)),
        };
        if let Some(rgba) = value {
            colors.insert(role.to_string(), rgba);
        }
    }
    colors
}

/// The page and the panel ground for these settings. The house page, exactly
/// and as the file derives it, wherever the settings come to it.
fn grounds_of(params: &BuilderParams) -> (u32, u32) {
    let scheme = params.scheme();
    let page = page_of(params);
    if page == house_page(scheme) {
        return grounds(scheme, WHITE, 0.0);
    }
    (page, fg_of(scheme, page))
}

/// `color_bg_app` for these settings: the background colour's hue, the
/// saturation slider's share of the background colour's own saturation, and
/// the luminance the lightness slider names.
///
/// Held to its appearance's plain ink as well. The lightness ends are drawn
/// from a grey page, and a saturated page of the same luminance makes its
/// rungs a little differently -- a mix toward white in display space is not
/// a mix of luminances -- so a page whose plain ink would fall short on one
/// of its rungs is carried the last step away from the ink until it does
/// not. It is the ground that gives way here and never the words.
fn page_of(params: &BuilderParams) -> u32 {
    let scheme = params.scheme();
    let [_, _, _, background] = params.palette();
    let (hue, own, _) = rgb_to_hsl(background | 0xFF);
    let sat = if own < 0.01 { 0.0 } else { own * params.saturation.clamp(0.0, 1.0) };
    let mut y = luminance_at(lightness_lstar(params.lightness));
    let mut page = at_luminance(hue, sat, y);
    for _ in 0..40 {
        if plain_ink_reads(scheme, page) {
            break;
        }
        y = match scheme {
            Scheme::Light => y + (1.0 - y) * 0.02,
            _ => y * 0.97,
        };
        page = at_luminance(hue, sat, y);
    }
    page
}

/// Whether the appearance's plain ink reads on every rung this page makes, at
/// the bar the library holds each rung to.
fn plain_ink_reads(scheme: Scheme, page: u32) -> bool {
    let colors = page_colors(scheme, page, fg_of(scheme, page));
    let ink = plain_ink(scheme);
    held_pairs().iter().all(|(ground, held, need)| match (*held, colors.get(*ground)) {
        ("color_on_surface" | "color_on_surface_variant", Some(g)) => reads_on(*g | 0xFF, ink) >= *need,
        _ => true,
    })
}

/// What the house theme's body text stands off the house page: the text
/// contrast that leaves the text alone.
fn house_text_contrast(scheme: Scheme) -> f64 {
    let body = file_value(scheme, "color_text", &BTreeMap::new()).unwrap_or(plain_ink(scheme));
    reads_on(house_page(scheme), body)
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
/// literal, which is how every role the library generated is written. Only
/// the tests read it now: the build takes its inks through [`file_value`],
/// which reads this form and the other two.
#[cfg(test)]
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

/// What a base theme's file says one of its colour keys is, worked out from
/// the file's own text.
///
/// The builder never moves these tokens -- until this mapping nothing in a
/// built theme touched one at all -- so they are not values that have to be
/// PREDICTED. They are values that have to be KNOWN, and for two reasons.
/// They are the grounds a check mark, a value fill or a caret lands on, and
/// an accent chosen without looking at the ground under it can come out
/// invisible. And they are the bar a new colour has to clear: a control that
/// reads WORSE than the grey it replaced is not an improvement, and the grey
/// it replaced is exactly what this function says.
///
/// Three forms cover the colour half of both files -- a literal, an alias to
/// another key, and `mix`, whose amount is either a plain number or the
/// `pow(fraction, theme.color_contrast)` the translucent ladder is built
/// with. That is enough for every name [`ACCENTED`] uses, and a gate test
/// says so in both files. Anything else -- `theme.color_d_3 * 0.8`, or the
/// two page colours, which run over several lines -- comes back as `None`,
/// and a row that needs a value and gets none simply leaves its token alone.
///
/// `known` is the build's own answers so far, and it is consulted before the
/// file is: the page and the roles have already been worked out here, under
/// a tint the file knows nothing about, and reading them off the file again
/// would answer with the untinted theme.
fn file_value(scheme: Scheme, key: &str, known: &BTreeMap<String, u32>) -> Option<u32> {
    file_key(scheme, key, known, 0)
}

/// [`file_value`] on a key, with the hop count that stops a file whose
/// aliases somehow come round in a circle from hanging the builder.
fn file_key(scheme: Scheme, key: &str, known: &BTreeMap<String, u32>, depth: u32) -> Option<u32> {
    if depth > 8 {
        return None;
    }
    if let Some(rgba) = known.get(key) {
        return Some(*rgba);
    }
    file_expr(scheme, &file_stated(scheme, key)?, known, depth + 1)
}

/// What a file writes after one of its top-level keys, comment and
/// surrounding space taken off: `color_inset: theme.color_d_1` is
/// `theme.color_d_1`.
fn file_stated(scheme: Scheme, key: &str) -> Option<String> {
    let opening = format!("        {key}: ");
    scheme
        .source()
        .lines()
        .find_map(|line| line.strip_prefix(&opening))
        .map(|rest| rest.split("//").next().unwrap_or(rest).trim().to_string())
}

/// One of the three forms, worked out the way the VM will work it out --
/// `vm_mix` and not `mix_rgb`, because a reading of a built theme has to be
/// a reading of the theme the VM will actually make.
fn file_expr(scheme: Scheme, text: &str, known: &BTreeMap<String, u32>, depth: u32) -> Option<u32> {
    if depth > 8 {
        return None;
    }
    let text = text.trim();
    if let Some(alias) = text.strip_prefix("theme.") {
        return alias.chars().all(is_key_char).then(|| file_key(scheme, alias, known, depth))?;
    }
    if text.starts_with('#') {
        return hash_color(text);
    }
    let inside = text.strip_prefix("mix(")?.strip_suffix(')')?;
    let parts = commas(inside)?;
    let [a, b, t] = parts[..] else { return None };
    let a = file_expr(scheme, a, known, depth + 1)?;
    let b = file_expr(scheme, b, known, depth + 1)?;
    Some(vm_mix(a, b, file_amount(scheme, t)?))
}

/// A `mix`'s third argument: a number, or the power of the contrast global
/// that every rung of the translucent ladder is spaced by.
fn file_amount(scheme: Scheme, text: &str) -> Option<f64> {
    let text = text.trim();
    let Some(inside) = text.strip_prefix("pow(").and_then(|rest| rest.strip_suffix(')')) else {
        return text.parse::<f64>().ok();
    };
    let (base, exponent) = inside.split_once(',')?;
    if exponent.trim() != "theme.color_contrast" {
        return None;
    }
    let contrast = file_number(scheme, "color_contrast").unwrap_or(1.0);
    Some(base.trim().parse::<f64>().ok()?.powf(contrast))
}

/// An argument list split on its own commas, leaving any inside a nested
/// call where they are. `None` for anything that is not three arguments,
/// which is the only shape `mix` has.
fn commas(text: &str) -> Option<Vec<&str>> {
    let mut out = Vec::new();
    let (mut depth, mut from) = (0i32, 0usize);
    for (at, c) in text.char_indices() {
        match c {
            '(' => depth += 1,
            ')' => depth -= 1,
            ',' if depth == 0 => {
                out.push(&text[from..at]);
                from = at + 1;
            }
            _ => {}
        }
    }
    out.push(&text[from..]);
    (out.len() == 3).then_some(out)
}

/// A colour literal as the script's own parser reads one: `#x` or `#`, then
/// one, three, four, six or eight hex digits, the short forms doubling each
/// digit. `#F` is white and `#0` is black, which is how both files write the
/// ends of the opaque ladder.
fn hash_color(text: &str) -> Option<u32> {
    let digits = text.trim().strip_prefix('#')?.trim_start_matches('x');
    if !digits.chars().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    let widened: String = match digits.len() {
        1 | 3 | 4 => digits.chars().flat_map(|c| [c, c]).collect(),
        6 | 8 => digits.to_string(),
        _ => return None,
    };
    let widened = match widened.len() {
        2 => format!("{widened}{widened}{widened}FF"),
        6 => format!("{widened}FF"),
        8 => widened,
        _ => return None,
    };
    u32::from_str_radix(&widened, 16).ok()
}

/// Which member of which family a row of [`ACCENTED`] draws on.
///
/// All three brand families are spent here, and that is the point of the
/// table. A library that gave every coloured thing the primary would answer
/// "pick a colour, get a theme" with a screen in one colour, which is what
/// the panel used to do and what a person looking at it said out loud: the
/// palette had three colours in it and the interface showed one.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Source {
    /// The accent itself, the colour a person picked.
    Primary,
    /// The accent as something to stand ON.
    PrimaryContainer,
    /// What the palette already decided reads on that container, and so the
    /// second thing to try where the accent itself is too close to a ground.
    OnPrimaryContainer,
    Secondary,
    SecondaryContainer,
    OnSecondaryContainer,
    Tertiary,
    TertiaryContainer,
    OnTertiaryContainer,
    /// The palette's fourth colour, the one the page is made of, at its own
    /// saturation and at the lightness of the token it is leant into: pale
    /// where the token is a wash of white, deep where it is a wash of black.
    /// A control ground is a background, so it wears the background's hue
    /// and not an accent's.
    Background,
}

impl Source {
    /// The colour this member is, for a token whose own value is `token`.
    /// `None` for [`Source::Background`] where the fourth colour has no
    /// colour in it: a lean toward a grey would only move a wash of white
    /// toward a wash of grey, and the house palette's grounds are not the
    /// builder's to repaint.
    fn toward(self, roles: &ColorRoles, background: u32, token: u32) -> Option<u32> {
        if self != Source::Background {
            return Some(self.of(roles));
        }
        let (hue, sat, _) = rgb_to_hsl(background | 0xFF);
        if sat < 0.01 {
            return None;
        }
        let light = if luminance(token | 0xFF) > 0.5 { 0.8 } else { 0.2 };
        Some(hsl_to_rgb(hue, sat, light))
    }

    fn of(self, roles: &ColorRoles) -> u32 {
        match self {
            Source::Primary => roles.primary.base,
            Source::PrimaryContainer => roles.primary.container,
            Source::OnPrimaryContainer => roles.primary.on_container,
            Source::Secondary => roles.secondary.base,
            Source::SecondaryContainer => roles.secondary.container,
            Source::OnSecondaryContainer => roles.secondary.on_container,
            Source::Tertiary => roles.tertiary.base,
            Source::TertiaryContainer => roles.tertiary.container,
            Source::OnTertiaryContainer => roles.tertiary.on_container,
            // Never asked: the fourth colour is only ever leant toward, and
            // only through `toward`, which knows the token it leans.
            Source::Background => roles.secondary.container,
        }
    }
}

/// How a row's source reaches its tokens.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Reaches {
    /// Straight, because nothing draws the token: it is one of the names the
    /// library keeps the accent itself under, for a sheet to be regrown from.
    Named,
    /// As something drawn ON the grounds named -- a mark, a ring, a caret, a
    /// value fill. The source where it stands clear of every one of them,
    /// `alt` where only that does, and failing both the plainer of black and
    /// white. `need` is the bar: a graphic answers to `LEGIBLE`, words to
    /// `READABLE`.
    Ink { alt: Source, on: &'static [&'static str], need: f64 },
    /// As a ground something else is drawn on. The source where `ink` still
    /// reads on it, `alt` where only that does, and the base theme's own
    /// value where neither does -- a selection a person cannot read the text
    /// in is worse than a selection with no colour in it.
    ///
    /// Where `ink` is itself a token this table pins, the question does not
    /// arise and the source is taken outright: the ink is chosen against
    /// this ground afterwards, in the second pass, so a ground that gave way
    /// to an ink that was about to move anyway would have given way for
    /// nothing.
    Ground { alt: Source, ink: &'static str, need: f64 },
    /// As the source at the token's own alpha: a wash over whatever is under
    /// it rather than a colour in front of it. What a drop target is -- it
    /// has to be seen through, so it cannot be a colour that covers.
    Veil,
    /// As a lean: the token's own value carried `most` of the way toward the
    /// source, at the token's OWN alpha, and given back as far as it has to
    /// be for `ink` to go on reading on the result, so a fill can never
    /// swallow its own label.
    ///
    /// `waits` is whether the lean is a matter of degree. A control's ground
    /// is: it waits on [`control_ground_lean`], the saturation slider, and is
    /// nothing at all at nought. A selection is not -- it lands at once, like every other
    /// accent -- and leans rather than taking the family's container outright
    /// only where the base theme's own ground is DARKER than the container,
    /// so that taking it would cost the words on it the contrast the base
    /// theme gave them.
    ///
    /// The alpha is what makes this safe, and it is not a detail. Nearly
    /// every control ground in both files is plain white or plain black at
    /// some percentage -- the ladder is one colour at nine weights -- so a
    /// mix that carried the alpha across would swap a fifteen-per-cent wash
    /// for a half-opaque fill and rewrite the whole depth of the interface
    /// on the way to tinting it. Kept, the wash stays a wash and only its
    /// colour moves.
    Lean { most: f64, ink: &'static str, need: f64, waits: bool },
}

/// One row of the mapping.
struct Accented {
    /// The older tokens this row pins. More than one where a family of
    /// states is the same thing drawn in the same place: a slider's value
    /// fill is `color_val` plain, hovered, focused and dragged, and a fill
    /// that took the accent only while the pointer was elsewhere would be a
    /// control that changes colour under the hand.
    ///
    /// A state is also listed where the FILE aliases it to one that moves.
    /// `color_inset_focus` is `color_inset_hover` is `color_inset`; a pin
    /// lands on the object after it has derived, so pinning the hover and
    /// leaving the focus would give one control two grounds.
    tokens: &'static [&'static str],
    from: Source,
    reaches: Reaches,
}

/// FROM the built roles TO the older tokens the classic controls actually
/// draw from. The other direction of `theme_tokens::derived_roles` with
/// `RoleGrowth::FromAccent`, which grows the roles from a style sheet's older
/// accent; here the roles exist and it is the older tokens that are still the
/// base theme's greys.
///
/// # Why a table and not a pin per control
///
/// Only 35 of the 109 widget files that read a theme colour read a ROLE at
/// all, and the classic ones read none. `check_box`, `radio_button`,
/// `text_input`, `tab`, `tab_bar`, `drop_down`, `scroll_bar` and
/// `link_label` between them read zero; `slider` reads one. They draw from
/// `theme_desktop_dark` / `theme_desktop_light`, where every one of these
/// tokens is a grey or the one fixed focus blue (`color_focus: #x7aa2f7`).
/// So a theme grown from an orange favourite had an orange-leaning page and
/// grey controls with a blue focus ring, and "pick a colour, get a theme"
/// was not true. It is fixed here rather than in those files because several
/// of them are the upstream library's, and which token a widget reads is not
/// this panel's to change.
///
/// # Which family a thing wears
///
/// Three voices, and a control kind always wears the same one, so that the
/// colour says what a thing IS and not merely that somebody chose a palette:
///
/// * The primary is the main action and the VALUE: value fills, check and
///   radio marks, and the focus ring, which is the app saying where you are.
/// * The secondary is SELECTION and the on-state: the ground under a ticked
///   box, a chosen radio, a selected row in a menu, a drop down or a file
///   tree, the label on the tab you are on.
/// * The tertiary POINTS THINGS OUT: selected text, the caret, and the
///   preview of where a drag would land.
///
/// And the grounds of the controls at rest, under the pointer and pressed
/// wear none of the three. They are BACKGROUNDS -- a row of buttons is part
/// of the page it sits on -- so they step off the page in the palette's
/// fourth colour, carrying the same saturation the saturation slider gives
/// the page. A control ground in an accent was a second accent spread over
/// every button, and it put the colour of selection on things that were
/// not selected.
///
/// # What a style sheet colours, and what it leaves
///
/// Reading the twelve sheets under `widgets/themes` for the tokens they
/// assign their own accent to gives `color_focus`, `color_ctrl_selected`,
/// `color_ctrl_active`, the four numbered focus bevels, `color_outset_active`
/// and the two highlight grounds -- and that list is not a guess about what
/// CAN carry an accent, it is twelve independent answers to the question.
/// The marks, the value fills, the text selection and the caret are added to
/// it, because a sheet leaves those at the base theme's greys and the
/// complaint was exactly that the controls stayed grey.
///
/// What is deliberately NOT in it:
///
/// * Links. `link_label` draws from `color_label_inner`, the same token every
///   button's label draws from, so there is no colour to give a link that is
///   not also given to every label in the app. It needs a token of its own
///   before it can have a colour of its own, and that is a change to a widget
///   file and to both theme files, not a change to this table.
/// * The slider's handle. It sits ON the value fill; both in the primary
///   would be a thumb that disappears into the colour it is meant to mark
///   the end of.
/// * The two tracks, `color_inset_1` and `color_inset_2`. A track is the part
///   of a value that is not there yet; tinting it eats the fill that is.
/// * The rungs themselves -- `color_u_3`, `color_opaque_u_2` and the rest.
///   A rung is shared by the things that must STAY neutral, so the table pins
///   the TOKEN and never the rung it happens to alias.
/// * `color_bg_highlight`, although every sheet colours it. In this library
///   it is the ground behind a block quote, a code span and a table header in
///   `markdown`, `html`, `text_flow` and `code_block` -- three block grounds
///   sharing one name, none of them a highlight and none of them a control.
/// * The word beside a control. `color_label_outer_active` is the text next
///   to a ticked check box; that is body text that happens to sit by
///   something ticked, and colouring it would say the label is the accent.
/// * The disabled states, and the hover INKS. Nothing is selected, focused
///   or on in any of them.
/// * The categorical palettes, `color_map_*` and `color_syntax_*`. Those are
///   told apart BY their colours; a palette that leant on them would make two
///   directories or two token kinds the same thing.
const ACCENTED: &[Accented] = {
    use Reaches::{Ground, Ink, Lean, Named, Veil};
    use Source::{
        Background, OnPrimaryContainer, OnSecondaryContainer, OnTertiaryContainer, Primary, Secondary,
        SecondaryContainer, Tertiary,
    };
    /// Where a focus ring is seen: against the page, and against the panel
    /// ground a control more often sits on.
    const PAGES: &[&str] = &["color_bg_app", "color_fg_app"];
    /// The box behind a check mark or a radio dot, in the two states a mark
    /// is visible in.
    const BOXES: &[&str] = &["color_inset_active", "color_inset_focus"];
    /// The three grounds a value fill is drawn along.
    const TRACKS: &[&str] = &["color_inset", "color_inset_1", "color_inset_2"];
    /// Everything the label of a selected thing lands on: the four selected
    /// grounds below, and the page itself, because a tab draws its active
    /// label straight onto the page while a menu row draws it on the fill.
    const SELECTED: &[&str] = &[
        "color_outset_active",
        "color_outset_1_active",
        "color_outset_2_active",
        "color_highlight",
        "color_bg_app",
    ];
    &[
        // ------------------------------------------------ THE PRIMARY: the
        // main action, and the value.
        //
        // The accent under the two names the base themes keep it under.
        // Nothing draws these. `color_ctrl_selected` is `SHEET_ACCENT`, the
        // token a style sheet's roles are regrown from, so a sheet exported
        // from an orange theme that left this at the base theme's blue would
        // come back blue. (`color_ctrl_active` is the third name and is NOT
        // here: only the sheets declare it, and a token no base theme has is
        // a token this mapping cannot pin -- the gate test says so.)
        Accented { tokens: &["color_focus", "color_ctrl_selected"], from: Primary, reaches: Named },
        // The focus ring, on every control that draws one: the plain bevel
        // and the four numbered ones, which is what the sheets set and what
        // `button`, `check_box`, `radio_button`, `slider`, `text_input` and
        // `drop_down` read between them. A ring is a graphic.
        Accented {
            tokens: &[
                "color_bevel_focus",
                "color_bevel_inset_1_focus",
                "color_bevel_inset_2_focus",
                "color_bevel_outset_1_focus",
                "color_bevel_outset_2_focus",
            ],
            from: Primary,
            reaches: Ink { alt: OnPrimaryContainer, on: PAGES, need: LEGIBLE },
        },
        // The mark: a check box's tick, a radio's dot, a menu row's tick.
        Accented {
            tokens: &["color_mark_active", "color_mark_active_hover", "color_mark_focus", "color_mark_down"],
            from: Primary,
            reaches: Ink { alt: OnPrimaryContainer, on: BOXES, need: LEGIBLE },
        },
        // The value fill: a slider's filled part, a progress bar, a wave.
        // Nothing is written on it (see `WRITTEN`), which is why it may be
        // chosen against its track alone.
        Accented {
            tokens: &[
                "color_val", "color_val_hover", "color_val_focus", "color_val_drag",
                "color_val_1", "color_val_1_hover", "color_val_1_focus", "color_val_1_drag",
                "color_val_2", "color_val_2_hover", "color_val_2_focus", "color_val_2_drag",
            ],
            from: Primary,
            reaches: Ink { alt: OnPrimaryContainer, on: TRACKS, need: LEGIBLE },
        },
        // ---------------------------------------------- THE SECONDARY: what
        // is selected, and what is on.
        //
        // The ground of a ticked box and a chosen radio. Its ink is the mark
        // above, which this table also chooses, so the ground is taken
        // outright and the mark follows it.
        Accented {
            tokens: &["color_inset_active"],
            from: SecondaryContainer,
            reaches: Ground { alt: Secondary, ink: "color_mark_active", need: LEGIBLE },
        },
        // The ground under a selected row: a drop down's chosen item, a menu
        // row, a combo box, a file tree's selected file, and the on-state of
        // anything built out of a radio, the wheel picker's band and the
        // time picker's chosen plate. `color_highlight` is the file tree's
        // own name for the same thing.
        Accented {
            tokens: &["color_outset_active", "color_outset_1_active", "color_outset_2_active", "color_highlight"],
            from: SecondaryContainer,
            reaches: Ground { alt: Secondary, ink: "color_label_inner_active", need: READABLE },
        },
        // The ink on those grounds, and the one thing a tab has to say it is
        // the one you are on -- `tab` draws this label straight onto the
        // page, so it is held to the page as well as to the fills.
        Accented {
            tokens: &["color_label_inner_active"],
            from: Secondary,
            reaches: Ink { alt: OnSecondaryContainer, on: SELECTED, need: READABLE },
        },
        // -------------------------------------------- THE BACKGROUND: the
        // grounds of the controls, which are the page's and not an accent's.
        //
        // The fills a pointer or a press puts up, and the controls at rest.
        // Each is a wash of white or of black over the page, so over a
        // coloured page it already carries the page's hue, thinned; the lean
        // puts back the colour the wash thinned out, in the background's hue
        // and in step with the saturation slider -- none of it at nought,
        // where the page is a grey and so is every ground on it.
        Accented {
            tokens: &[
                "color_outset_hover", "color_outset_down", "color_outset_drag",
                "color_outset_1_hover", "color_outset_1_down", "color_outset_1_drag",
                "color_outset_2_hover", "color_outset_2_down", "color_outset_2_drag",
            ],
            from: Background,
            reaches: Lean { most: 0.45, ink: "color_label_inner_hover", need: READABLE, waits: true },
        },
        Accented {
            tokens: &["color_inset_hover", "color_inset_down", "color_inset_drag"],
            from: Background,
            reaches: Lean { most: 0.45, ink: "color_text", need: READABLE, waits: true },
        },
        Accented {
            tokens: &[
                "color_outset", "color_outset_focus",
                "color_outset_1", "color_outset_1_focus",
                "color_outset_2", "color_outset_2_focus",
            ],
            from: Background,
            reaches: Lean { most: 0.30, ink: "color_label_inner", need: READABLE, waits: true },
        },
        // `color_icon_inactive` and `color_mark_empty` are here because both
        // files say they ARE the inset -- an icon with nothing behind it and
        // a mark with nothing in it are the field they sit in. A theme that
        // leant the inset and left those two would be a theme whose exported
        // FILE re-derived them off the lean while the pins over the base
        // object did not, which is one theme with two answers.
        Accented {
            tokens: &[
                "color_inset", "color_inset_focus", "color_inset_empty",
                "color_icon_inactive", "color_mark_empty",
            ],
            from: Background,
            reaches: Lean { most: 0.30, ink: "color_text", need: READABLE, waits: true },
        },
        // ----------------------------------------------- THE TERTIARY: what
        // is being pointed out.
        //
        // The ground behind selected TEXT: a text field's selection, and the
        // one inside a slider's editable value. The words on it are
        // `color_text`, one token for the whole app, so the ground is what
        // has to give way here -- and it gives way as far as the base
        // theme's own selection, never further.
        // The ground behind selected TEXT: a text field's selection, and the
        // one inside a slider's editable value.
        //
        // A wash and not a fill, which is the whole of why this is a lean.
        // A text input draws its selection OVER the glyphs, so the base
        // theme's white-at-a-quarter lets the words through; an opaque
        // colour in the same place is a coloured block with the text gone
        // under it. Every readability sweep passed that block -- the words
        // were still a bar clear of the ground they were no longer on -- and
        // the screen showed it in one glance, which is what the screen is
        // for. So the alpha is the base theme's and only the colour moves.
        //
        // The family's BASE and not its container, because the base is the
        // member that follows the page: light in a dark theme, dark in a
        // light one, which is the same way round as the wash it replaces.
        // And no waiting on the slider -- a selection is a selection as soon
        // as there is a palette.
        Accented {
            tokens: &[
                "color_selection_hover",
                "color_selection_focus",
                "color_selection_down",
                "color_bg_highlight_inline",
            ],
            from: Tertiary,
            reaches: Lean { most: 0.85, ink: "color_text", need: READABLE, waits: false },
        },
        // The caret, in a text field and in a slider's editable value.
        Accented {
            tokens: &["color_text_cursor"],
            from: Tertiary,
            reaches: Ink { alt: OnTertiaryContainer, on: &["color_inset", "color_bg_app"], need: LEGIBLE },
        },
        // Where a drag would land, in a dock and on a board. Drawn over the
        // content it would replace, so it keeps the base theme's alpha and
        // takes only the hue.
        Accented { tokens: &["color_drag_target_preview"], from: Tertiary, reaches: Veil },
    ]
};

/// Words a widget writes on a ground: which widget files, the grounds -- one
/// per state the words are drawn in, the ones a pointer, a focus or a press
/// puts up as well as the one at rest -- the ink, and the bar the words
/// answer to.
struct Written {
    widgets: &'static [&'static str],
    grounds: &'static [&'static str],
    ink: &'static str,
    need: f64,
}

/// The bar disabled words answer to. Lower than the one for words, because
/// a disabled control says it cannot be used partly by being quieter, and
/// the library holds none of its own themes to more; not nothing, because
/// the words still say what the control is.
const DISABLED_WORDS: f64 = LEGIBLE;

/// Every place a widget in this crate writes words on a ground the mapping
/// colours, state by state.
///
/// [`ACCENTED`] was written from the grounds' side: each row that makes a
/// ground names the ONE ink it gives way to. That is enough only where one
/// ink is ever written on it, and it was not. The WheelPicker wrote the body
/// ink on a slider's value fill, which the mapping pushes dark on a light
/// page to stand off its track and holds to nothing else, because nothing is
/// ever written on a fill -- and the digits in the band went dark on dark,
/// and darker again under the pointer. So this is the other side: what the
/// widgets actually draw, read off their templates, one row per ink with
/// every state's ground in it. A build measures every pair here, and every
/// ground the mapping leans or chooses gives way to every ink written on it
/// and not only to its row's own (see [`accent_pins`]).
///
/// `written_words_are_all_measured` holds the list to the widget files: a
/// file that draws text and reads an accented ground has to have that ground
/// either here or in [`UNWRITTEN`], with the reason, so a widget that starts
/// writing on a coloured ground cannot do it unmeasured.
///
/// The disabled grounds are not accented -- the mapping leaves them at the
/// base theme's washes -- but they are in the list all the same, because the
/// page under them is the builder's, and a disabled label that reads on the
/// house page need not read on a page the lightness slider moved.
const WRITTEN: &[Written] = {
    /// The faces a label is written in the middle of.
    const FACES: &[&str] = &["button.rs", "drop_down.rs", "combo_box.rs", "drop_down2.rs"];
    /// The fields a value is typed or shown in.
    const FIELDS: &[&str] =
        &["text_input.rs", "number_field.rs", "tag_field.rs", "tree_select.rs", "value_input.rs", "dropzone.rs"];
    /// A chosen row, in every widget that marks one: the menus and lists, the
    /// file tree, a radio drawn as a tab, the time picker's plate and the
    /// wheel picker's band.
    const CHOSEN: &[&str] = &[
        "combo_box.rs", "drop_down2.rs", "popup_menu.rs", "file_tree.rs", "radio_button.rs",
        "time_picker.rs", "wheel_picker.rs",
    ];
    &[
        // ------------------------------------------ the faces of the controls
        Written {
            widgets: FACES,
            grounds: &["color_outset", "color_outset_1", "color_outset_2"],
            ink: "color_label_inner",
            need: READABLE,
        },
        Written {
            widgets: FACES,
            grounds: &["color_outset_focus", "color_outset_1_focus", "color_outset_2_focus"],
            ink: "color_label_inner_focus",
            need: READABLE,
        },
        Written {
            widgets: &["button.rs", "drop_down.rs", "combo_box.rs", "drop_down2.rs", "popup_menu.rs"],
            grounds: &["color_outset_hover", "color_outset_1_hover", "color_outset_2_hover"],
            ink: "color_label_inner_hover",
            need: READABLE,
        },
        Written {
            widgets: FACES,
            grounds: &["color_outset_down", "color_outset_1_down", "color_outset_2_down"],
            ink: "color_label_inner_down",
            need: READABLE,
        },
        // A radio drawn as a tab writes its label on the field at rest.
        Written { widgets: &["radio_button.rs"], grounds: &["color_inset"], ink: "color_label_inner", need: READABLE },
        // ----------------------------------------------------- a chosen row
        Written {
            widgets: CHOSEN,
            grounds: &["color_outset_active", "color_outset_1_active", "color_outset_2_active", "color_highlight"],
            ink: "color_label_inner_active",
            need: READABLE,
        },
        // ------------------------------------------------------- the fields
        Written { widgets: FIELDS, grounds: &["color_inset"], ink: "color_text", need: READABLE },
        Written { widgets: FIELDS, grounds: &["color_inset_hover"], ink: "color_text_hover", need: READABLE },
        Written { widgets: FIELDS, grounds: &["color_inset_focus"], ink: "color_text_focus", need: READABLE },
        Written { widgets: &["text_input.rs"], grounds: &["color_inset_down"], ink: "color_text_down", need: READABLE },
        Written {
            widgets: &["text_input.rs"],
            grounds: &["color_inset_empty"],
            ink: "color_text_placeholder",
            need: LEGIBLE,
        },
        // The quieter line a drop zone, a tree select and a waveform write
        // under or beside their main words.
        Written {
            widgets: &["dropzone.rs", "tree_select.rs", "waveform.rs"],
            grounds: &["color_inset", "color_inset_hover", "color_inset_focus", "color_inset_drag"],
            ink: "color_text_meta",
            need: LEGIBLE,
        },
        // A slider's value, where the slider draws its field round it.
        Written { widgets: &["slider.rs"], grounds: &["color_inset"], ink: "color_text_val", need: READABLE },
        Written { widgets: &["slider.rs"], grounds: &["color_inset_hover"], ink: "color_text_hover", need: READABLE },
        Written { widgets: &["slider.rs"], grounds: &["color_inset_focus"], ink: "color_text_focus", need: READABLE },
        Written { widgets: &["slider.rs"], grounds: &["color_inset_drag"], ink: "color_text_down", need: READABLE },
        // The time picker's values that were not chosen, at rest and under
        // the pointer, and the wheel picker's rows away from its band.
        Written {
            widgets: &["time_picker.rs"],
            grounds: &["color_inset", "color_inset_hover"],
            ink: "color_label_inner_inactive",
            need: READABLE,
        },
        Written {
            widgets: &["wheel_picker.rs"],
            grounds: &["color_inset", "color_inset_hover", "color_inset_focus", "color_inset_drag"],
            ink: "color_label_outer_off",
            need: LEGIBLE,
        },
        // ------------------------------------------------- selected words
        Written {
            widgets: &["text_input.rs", "rich_text.rs", "text_flow.rs"],
            grounds: &["color_selection_hover", "color_selection_focus", "color_selection_down"],
            ink: "color_text",
            need: READABLE,
        },
        Written {
            widgets: &["html.rs", "markdown.rs"],
            grounds: &["color_selection_focus"],
            ink: "color_label_inner",
            need: READABLE,
        },
        // The selection inside a slider's value, which is only ever up while
        // the value has the focus.
        Written {
            widgets: &["slider.rs"],
            grounds: &["color_bg_highlight_inline"],
            ink: "color_text_focus",
            need: READABLE,
        },
        // ------------------------------------------------------ disabled
        Written {
            widgets: &["popup_menu.rs"],
            grounds: &["color_outset_disabled"],
            ink: "color_label_inner_disabled",
            need: DISABLED_WORDS,
        },
        Written {
            widgets: &["wheel_picker.rs"],
            grounds: &["color_outset_disabled"],
            ink: "color_label_inner",
            need: DISABLED_WORDS,
        },
        Written {
            widgets: &["time_picker.rs"],
            grounds: &["color_outset_disabled"],
            ink: "color_label_inner_inactive",
            need: DISABLED_WORDS,
        },
    ]
};

/// The accented grounds a widget that draws words reads and writes none of
/// them on, each with why: the gate test's other half. A ground belongs here
/// only where the words really are somewhere else.
const UNWRITTEN: &[(&str, &[&str], &str)] = &[
    (
        "check_box.rs",
        &["color_inset", "color_inset_active", "color_inset_down", "color_inset_focus", "color_inset_hover"],
        "the box; its label is written beside it, on the page",
    ),
    (
        "radio_button.rs",
        &["color_inset_active", "color_inset_down", "color_inset_focus", "color_inset_hover"],
        "the round radio's box; its label is beside it, on the page",
    ),
    (
        "carousel.rs",
        &["color_inset", "color_inset_drag", "color_inset_focus", "color_inset_hover"],
        "the frame round the cards; the words are on the cards",
    ),
    ("kanban.rs", &["color_drag_target_preview"], "a veil laid over the cards and seen through, not a ground"),
    (
        "range_slider.rs",
        &[
            "color_inset", "color_inset_drag", "color_inset_focus", "color_inset_hover",
            "color_val", "color_val_drag", "color_val_focus", "color_val_hover",
        ],
        "the track and its fill; the label is outside the track, on the page",
    ),
    ("radio_group.rs", &["color_val_focus"], "a ring round the group, not a ground"),
    (
        "slider.rs",
        &[
            "color_val", "color_val_hover", "color_val_focus", "color_val_drag",
            "color_val_1", "color_val_1_hover", "color_val_1_focus", "color_val_1_drag",
            "color_val_2", "color_val_2_hover", "color_val_2_focus", "color_val_2_drag",
        ],
        "the value fill; the round face's readout runs over it near the top of the travel and is NOT held: slider.rs is upstream's, and a plate under the readout would change every theme's slider, so it waits on the operator",
    ),
    (
        "waveform.rs",
        &["color_val", "color_val_hover", "color_val_focus", "color_val_drag"],
        "the wave; the region names along the lane's foot sit on a plate of the page",
    ),
];

/// Every ground the mapping reads and every ink it protects, so that a build
/// can resolve them once and the gate test can hold every one of them to the
/// theme files.
fn accent_grounds() -> Vec<&'static str> {
    let mut out: Vec<&'static str> = Vec::new();
    let mut want = |key: &'static str| {
        if !out.contains(&key) {
            out.push(key);
        }
    };
    for row in ACCENTED {
        match row.reaches {
            Reaches::Ink { on, .. } => on.iter().for_each(|key| want(key)),
            Reaches::Ground { ink, .. } | Reaches::Lean { ink, .. } => want(ink),
            Reaches::Named | Reaches::Veil => {}
        }
    }
    for written in WRITTEN {
        written.grounds.iter().for_each(|key| want(key));
        want(written.ink);
    }
    out
}

/// A ground carried along its own lightness, keeping its hue, saturation
/// and alpha, just far enough for every ink given to read on it at its bar,
/// laid over the page: toward whichever end gets there in the shorter move.
/// `None` where neither end does.
fn stand_off(fill: u32, inks: &[(u32, f64)], page: u32) -> Option<u32> {
    let (h, s, l) = rgb_to_hsl(fill);
    let alpha = fill & 0xFF;
    let at = |l: f64| (hsl_to_rgb(h, s, l) & 0xFFFF_FF00) | alpha;
    let reads = |rgba: u32| inks.iter().all(|(ink, need)| reads_on(over(page, rgba), *ink) >= *need);
    (1..=100)
        .map(|step| step as f64 * 0.01)
        .flat_map(|by| [l - by, l + by])
        .filter(|l| (0.0..=1.0).contains(l))
        .map(at)
        .find(|rgba| reads(*rgba))
}

/// The inks [`WRITTEN`] says are written on a token, each with its bar.
fn written_on(token: &str) -> impl Iterator<Item = (&'static str, f64)> + Clone + '_ {
    WRITTEN.iter().filter(move |w| w.grounds.contains(&token)).map(|w| (w.ink, w.need))
}

/// Whether the settings name a palette that is not the house one: a
/// favourite, a harmony, or colours a suggestion or a hand named.
///
/// The whole of the mapping hangs off this. Somebody who opened the panel and
/// dragged the spacing did not ask for coloured controls and does not get
/// them; somebody who moved nothing at all still installs nothing at all,
/// which is what [`ThemeBuilder::apply`] promises and what the three house
/// tests hold it to. The two background sliders are not in it: they move
/// the grounds, and the grounds are not the accents.
fn palette_moved(params: &BuilderParams) -> bool {
    let house = SeedColors::HOUSE.primary;
    (params.favourite | 0xFF) != (house | 0xFF) || params.harmony != Harmony::House || params.seeds.is_some()
}

/// How far the GROUNDS of the controls lean toward the palette's background
/// colour: none of the way at nought, all of a row's `most` at one.
///
/// The accents themselves do not ask this. A mark, a value fill, a focus
/// ring, a selection, a caret -- those are the palette showing up at all,
/// and they land in full the moment the palette moves. What is a matter of
/// degree is the colour in the BACKGROUNDS, and the saturation slider is the
/// control of exactly that: the page takes this share of the background
/// colour's saturation, and so do the grounds stepped off it.
fn control_ground_lean(params: &BuilderParams) -> f64 {
    params.clamped().saturation
}

/// How an ink reads on the ground it does worst on. One token, several
/// grounds, and the worst of them decides -- the same shape as
/// [`settle_ink`] and for the same reason.
fn worst_reading(grounds: &[u32], ink: u32) -> f64 {
    grounds.iter().map(|g| reads_on(*g, ink)).fold(f64::INFINITY, f64::min)
}

/// The bar a colour this mapping chooses has to clear: the row's own, or
/// what the base theme's value for that token manages on the same grounds,
/// whichever is LOWER.
///
/// Without the second half the mapping would mostly refuse to do anything.
/// The base themes' own controls do not meet the library's bars: the dark
/// theme's check mark is white at 35% on a box that is black at 15% over the
/// page, which reads 2.7 against a graphic bar of 3, and its text selection
/// reads 1.9 against a text bar of 4.5. Held to the stated bar, every accent
/// would be rejected in favour of a fallback and the controls would stay
/// grey -- which is the defect. Held to this bar, a colour is taken whenever
/// it is no worse to read than leaving the token alone, and the stated bar
/// still applies wherever the base theme meets it.
///
/// The twentieth is there because "no worse" cannot be decided in the third
/// decimal. At the far end of the saturation slider the base theme's own
/// text selection reads 3.23 and the palette's reads 3.22, a difference no
/// eye has ever seen, and without the allowance the selection would lose its
/// colour at exactly that point on the slider and nowhere else.
fn accent_bar(need: f64, base_reading: Option<f64>) -> f64 {
    match base_reading {
        Some(reading) if reading * 0.95 < need => reading * 0.95,
        _ => need,
    }
}

/// What the mapping came to.
#[derive(Default)]
struct Accents {
    /// The tokens it pinned, as the script will write them.
    pins: Vec<(String, u32)>,
    /// Every ground a pin was chosen against that the theme does not already
    /// carry, as the VM will resolve it and not as it was composited --
    /// `color_inset_1` is a translucent rung, and it goes into the built
    /// theme's colours so that `what_build_predicts_is_what_the_vm_resolves`
    /// holds the file reading to the VM along with everything else.
    grounds: Vec<(String, u32)>,
    /// Every pair the mapping created, as `(ground, ink, bar)` in the units
    /// [`read_pairs`] measures in: the mark on the box it is drawn in, the
    /// value fill along its track, the focus ring against the page, the text
    /// selection under the words it holds, and each leaning fill under its
    /// own label. The bar is the one the choice was actually made against,
    /// [`accent_bar`] and not the row's stated `need`, or the reading would
    /// report as a failure the very case the bar was lowered for.
    ///
    /// Kept here and not folded into `theme_tokens::held_pairs`, which is the
    /// list the library holds EVERY theme and sheet to: these pairs only
    /// exist where a palette put an accent on a control, and a base theme
    /// whose check mark is grey on grey is not a theme that fails -- it is a
    /// theme with no accent in it, which is what a base theme is.
    pairs: Vec<(String, String, f64)>,
}

/// One page an accent is chosen on: the colours the build knows under it,
/// and the page itself for laying the translucent rungs over.
struct Under<'a> {
    colors: &'a BTreeMap<String, u32>,
    page: u32,
    /// The text inks at the quietest the text contrast can make them, for
    /// the rows that give way to the words on them. Chosen against these,
    /// a fill reads under every text contrast -- a louder ink only reads
    /// better on the same ground -- and so the text contrast, which is the
    /// words' own setting, never moves a fill.
    quiet: BTreeMap<String, u32>,
}

/// The pages [`accent_pins`] asks about, in the order it asks.
struct Pages<'a> {
    /// The page these settings make, with the colour taken out: the grey of
    /// the same lightness.
    grey: Under<'a>,
    /// The appearance's house page.
    house: Under<'a>,
    /// The page these settings make.
    real: Under<'a>,
}

/// The colours the mapping pins. Empty where the palette has not moved.
///
/// Two passes, because the grounds have to exist before what stands on them
/// can be chosen: the rows that make a ground go first, and the rows that put
/// an ink on one read the answers.
///
/// # Three pages
///
/// A choice is made on two pages that do not depend on the background
/// colour -- the page these settings make with its colour taken out, and the
/// appearance's house page -- and only then checked against the real page,
/// which is the one it is measured on and the one that has the last word.
///
/// That is what keeps the accents where they are while the background
/// sliders move. The palette's roles already stand off the house page and
/// not the real one (see [`build`]); if the choices made on top of them were
/// made on the real page alone, a check mark would change member of its
/// family somewhere along a slider, because a coloured page and the grey of
/// the same lightness make their translucent rungs a hair apart, and a
/// person dragging the saturation would watch an accent change under them --
/// the one thing that slider is promised not to do. Made this way, the
/// choice is the same at every saturation, and it is the house page's
/// choice wherever the page lets it read. It changes only where the real
/// page genuinely cannot hold it: the accent moves then because reading
/// forced it, and not otherwise. In practice that is the ends of the
/// lightness slider -- the darkest light page and the blackest dark one --
/// and never the saturation.
fn accent_pins<'a>(
    params: &BuilderParams,
    roles: &ColorRoles,
    pages: &Pages<'a>,
) -> Accents {
    let mut out = Accents::default();
    if !palette_moved(params) {
        return out;
    }
    let scheme = params.scheme();
    let lean = control_ground_lean(params);
    let [_, _, _, background] = params.palette();
    let chosen_on = [&pages.grey, &pages.house];
    let real = &pages.real;
    let mut pinned: BTreeMap<&'static str, u32> = BTreeMap::new();
    // The same pins less the grounds that lean with the saturation, which
    // is what the two steady pages are asked with: a box whose colour came
    // up with the slider is a box the steady pages have not got, and a mark
    // chosen on it would move with the slider without anything forcing it.
    let mut steady: BTreeMap<&'static str, u32> = BTreeMap::new();
    // What a token is worth right now: a pin if this build has made one yet,
    // otherwise the theme's own value under that page.
    let raw = |pinned: &BTreeMap<&'static str, u32>, under: &Under, key: &str| -> Option<u32> {
        pinned.get(key).copied().or_else(|| file_value(scheme, key, under.colors))
    };
    // The same, as a GROUND: laid over the page, because half of these are
    // translucent and a colour chosen against the black that `color_inset`
    // really is, rather than against the grey it makes on the page, is
    // chosen against something nobody ever sees. An ink is never composited
    // this way -- its alpha is what lets it read on whatever it lands on,
    // and `reads_on` spends it against the right ground.
    let ground = |pinned: &BTreeMap<&'static str, u32>, under: &Under, key: &str| {
        raw(pinned, under, key).map(|rgba| over(under.page, rgba))
    };
    // Whether the mapping also chooses what goes ON a token, in which case a
    // ground does not have to protect its ink: the ink is chosen against the
    // ground in the second pass.
    let repaired = |key: &str| ACCENTED.iter().any(|row| row.tokens.contains(&key));
    for inks_pass in [false, true] {
        for row in ACCENTED {
            if matches!(row.reaches, Reaches::Ink { .. }) != inks_pass {
                continue;
            }
            for token in row.tokens {
                let base = file_value(scheme, token, real.colors);
                let Some(source) = row.from.toward(roles, background, base.unwrap_or(WHITE)) else {
                    continue;
                };
                let value = match row.reaches {
                    Reaches::Named => source,
                    Reaches::Veil => {
                        let Some(base) = base else { continue };
                        (source & 0xFFFF_FF00) | (base & 0xFF)
                    }
                    Reaches::Ink { alt, on, need } => {
                        // The grounds under one page and the bar there.
                        let bar_on = |under: &Under| {
                            let pins = if std::ptr::eq(under, real) { &pinned } else { &steady };
                            let grounds: Vec<u32> = on.iter().filter_map(|key| ground(pins, under, key)).collect();
                            let base = file_value(scheme, token, under.colors);
                            // Words are held to the bar for words, however
                            // far short of it the base theme's own label
                            // falls. The allowance is for marks and rings,
                            // which a base theme draws grey on grey and a
                            // palette only has to draw no worse; a chosen
                            // row's label let down to the dark theme's three
                            // to one was a secondary ink at three to one on
                            // the secondary's own ground, which is the band
                            // of a WheelPicker nobody could read.
                            let bar = if need >= READABLE {
                                need
                            } else {
                                accent_bar(need, base.map(|base| worst_reading(&grounds, base)))
                            };
                            (grounds, bar)
                        };
                        let clears = |ink: u32, under: &Under| {
                            let (grounds, bar) = bar_on(under);
                            grounds.is_empty() || worst_reading(&grounds, ink) >= bar
                        };
                        let everywhere = |ink: u32| chosen_on.iter().all(|under| clears(ink, under));
                        let candidates = [source, alt.of(roles)];
                        let (real_grounds, real_bar) = bar_on(real);
                        let chosen = match candidates.into_iter().find(|ink| everywhere(*ink)) {
                            Some(ink) if clears(ink, real) => ink,
                            _ => match candidates.into_iter().find(|ink| everywhere(*ink) && clears(*ink, real)) {
                                Some(ink) => ink,
                                None if clears(source, real) => source,
                                None if clears(alt.of(roles), real) => alt.of(roles),
                                None if worst_reading(&real_grounds, WHITE) >= worst_reading(&real_grounds, BLACK) => WHITE,
                                None => BLACK,
                            },
                        };
                        for key in on {
                            if ground(&pinned, real, key).is_some() {
                                out.pairs.push((key.to_string(), token.to_string(), real_bar));
                            }
                        }
                        chosen
                    }
                    Reaches::Ground { alt, ink, need } => {
                        let bar_on = |under: &Under| {
                            let pins = if std::ptr::eq(under, real) { &pinned } else { &steady };
                            let ink_rgba = raw(pins, under, ink)?;
                            let base = file_value(scheme, token, under.colors);
                            Some((ink_rgba, accent_bar(need, base.map(|base| reads_on(over(under.page, base), ink_rgba)))))
                        };
                        let Some((_, real_bar)) = bar_on(real) else { continue };
                        let clears = |fill: u32, under: &Under| {
                            bar_on(under).is_none_or(|(ink_rgba, bar)| reads_on(over(under.page, fill), ink_rgba) >= bar)
                        };
                        let everywhere = |fill: u32| chosen_on.iter().all(|under| clears(fill, under)) && clears(fill, real);
                        let chosen = if repaired(ink) || everywhere(source) {
                            source
                        } else if everywhere(alt.of(roles)) {
                            alt.of(roles)
                        } else if clears(source, real) {
                            source
                        } else if clears(alt.of(roles), real) {
                            alt.of(roles)
                        } else {
                            continue;
                        };
                        if !repaired(ink) {
                            out.pairs.push((token.to_string(), ink.to_string(), real_bar));
                        }
                        chosen
                    }
                    Reaches::Lean { most, ink, need, waits } => {
                        let Some(base) = base else { continue };
                        let leant = |amount: f64| (vm_mix(base, source, amount) & 0xFFFF_FF00) | (base & 0xFF);
                        // The row's own ink first, then every other ink a
                        // widget writes on this token: a fill that gave way
                        // to its label and not to the hover label or the
                        // placeholder drawn on it too still swallowed words.
                        let inks = std::iter::once((ink, need)).chain(written_on(token));
                        let bar_on = |under: &Under, ink: &str, need: f64| {
                            let pins = if std::ptr::eq(under, real) { &pinned } else { &steady };
                            let ink_rgba = under.quiet.get(ink).copied().or_else(|| raw(pins, under, ink))?;
                            let bar = accent_bar(need, Some(reads_on(over(under.page, base), ink_rgba)));
                            Some((ink_rgba, bar))
                        };
                        let Some((_, real_bar)) = bar_on(real, ink, need) else { continue };
                        let reads = |amount: f64, under: &Under| {
                            inks.clone().all(|(ink, need)| {
                                bar_on(under, ink, need)
                                    .is_none_or(|(ink_rgba, bar)| reads_on(over(under.page, leant(amount)), ink_rgba) >= bar)
                            })
                        };
                        // As far as the lean asks, and then back off a
                        // twentieth at a time until the label on the fill
                        // reads again: first on the two pages that do not
                        // move with the colour, then, if it has to, on the
                        // real one. A lean that ended at nothing is not
                        // pinned at all: the token is already its base value.
                        let mut amount = if waits { most * lean } else { most };
                        while amount > 0.0 && !chosen_on.iter().all(|under| reads(amount, under)) {
                            amount -= 0.05;
                        }
                        while amount > 0.0 && !reads(amount, real) {
                            amount -= 0.05;
                        }
                        if amount <= 0.0 {
                            continue;
                        }
                        if !repaired(ink) {
                            out.pairs.push((token.to_string(), ink.to_string(), real_bar));
                        }
                        // And every other ink written on it, at the bar the
                        // lean gave way to, so the reading asks what the
                        // choice was made against.
                        for (written, need) in written_on(token) {
                            if written == ink || repaired(written) {
                                continue;
                            }
                            if let Some((_, bar)) = bar_on(real, written, need) {
                                out.pairs.push((token.to_string(), written.to_string(), bar));
                            }
                        }
                        leant(amount)
                    }
                };
                pinned.insert(token, value);
                if row.from != Source::Background {
                    steady.insert(token, value);
                }
            }
        }
    }
    // A chosen ground the words on it still cannot be read on. The ink row
    // above has already done what an ink can -- the palette's own member,
    // its other one, then the plainer of black and white -- and on some
    // palettes even that stands a quarter short on a container of middling
    // lightness. So the ground gives way instead: the same hue and
    // saturation, carried lighter or darker, whichever is the shorter way,
    // until the words on it read. A selection a person cannot read the
    // chosen row in is worse than a selection a shade off the palette.
    for row in ACCENTED {
        let Reaches::Ground { .. } = row.reaches else { continue };
        for token in row.tokens {
            let Some(fill) = pinned.get(token).copied() else { continue };
            let inks: Vec<(u32, f64)> = written_on(token)
                .filter_map(|(ink, need)| raw(&pinned, real, ink).map(|rgba| (rgba, need)))
                .collect();
            if inks.iter().all(|(ink, need)| reads_on(over(real.page, fill), *ink) >= *need) {
                continue;
            }
            if let Some(moved) = stand_off(fill, &inks, real.page) {
                pinned.insert(token, moved);
            }
        }
    }
    // Every pair a widget writes, measured whether or not a row above chose
    // either half of it: the reading is what says a built theme can be read,
    // and a pair it never looks at is a pair nothing stops from failing. The
    // bar is the row's own or what the ground the base theme gives it would
    // manage under the same ink, whichever is lower, as for every other pair
    // the mapping answers for.
    for written in WRITTEN {
        for key in written.grounds {
            if out.pairs.iter().any(|(g, i, _)| g == key && i == written.ink) {
                continue;
            }
            let (Some(_), Some(ink), Some(base)) = (
                ground(&pinned, real, key),
                real.quiet.get(written.ink).copied().or_else(|| raw(&pinned, real, written.ink)),
                file_value(scheme, key, real.colors),
            ) else {
                continue;
            };
            // Asked with the quietest ink the text contrast allows, as the
            // choices above are: a louder one only reads better on the same
            // ground, so what holds here holds under every text contrast.
            let chosen = ACCENTED
                .iter()
                .any(|row| matches!(row.reaches, Reaches::Ground { .. }) && row.tokens.contains(key));
            let bar = if chosen {
                written.need
            } else {
                accent_bar(written.need, Some(reads_on(over(real.page, base), ink)))
            };
            out.pairs.push((key.to_string(), written.ink.to_string(), bar));
        }
    }
    // The grounds and the inks, where the theme does not already carry one:
    // a token this mapping measured on is a token the VM will be held to.
    let wanted = accent_grounds().into_iter().chain(ACCENTED.iter().flat_map(|row| row.tokens.iter().copied()));
    for key in wanted {
        if real.colors.contains_key(key) || pinned.contains_key(key) || out.grounds.iter().any(|(k, _)| k == key) {
            continue;
        }
        if let Some(rgba) = file_value(scheme, key, real.colors) {
            out.grounds.push((key.to_string(), rgba));
        }
    }
    out.pins = pinned.into_iter().map(|(key, rgba)| (key.to_string(), rgba)).collect();
    out
}

/// A theme ready to install, measure or export. Made by [`build`].
#[derive(Clone, Debug, PartialEq)]
pub struct BuiltTheme {
    /// The settings this was built from, clamped.
    pub params: BuilderParams,
    /// The base theme it derives from.
    pub scheme: Scheme,
    /// The seed the settings came to, for a caller that wants to run the rule
    /// itself, and the numbers the rule grew the house roles, the intents and
    /// every container with. Always `RoleTuning::HOUSE`: no setting moves the
    /// rule any more, since the accents are the colours named and the
    /// background sliders move the grounds.
    pub seed: SeedColors,
    pub tuning: RoleTuning,
    /// The accent roles, all seven families.
    pub roles: ColorRoles,
    /// The globals that differ from the base theme's own file, in the order a
    /// theme file lists them: the four dimensions, and the page and panel
    /// ground as literals wherever the page is not the house one. Empty for
    /// a theme that only moved its accents, and then the script derives from
    /// the base object and re-derives nothing.
    pub globals: Vec<(String, TokenValue)>,
    /// Every token the script pins over the (re-derived) base: the roles --
    /// all but `color_error` and `color_warning`, which every theme keeps as
    /// its older red and amber -- then the text inks wherever the page or the
    /// text contrast moved, then the older tokens the palette reaches.
    pub overrides: Vec<(String, TokenValue)>,
    /// The one script to evaluate, ending in the `true` it can afford to
    /// lose. What [`ThemeBuilder::apply`] writes onto the `Cx`.
    pub script: String,
    /// How the theme reads, over every pair the library holds its own themes
    /// to.
    pub readability: Readability,
    /// Every colour the reading was taken over, by token.
    colors: BTreeMap<String, u32>,
    /// The pairs the palette's mapping created, with the bar each was chosen
    /// against, which the reading measured on top of the held ones.
    accent_pairs: Vec<(String, String, f64)>,
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
/// Four things, in the order they depend on one another:
///
/// * The accents. With the palette untouched these are the house roles,
///   exactly. Otherwise the three brand families are the palette's first
///   three colours through `roles_from_colors` -- the colours themselves,
///   moved only where one could not be told from the page -- and they are
///   made to stand off the appearance's HOUSE page and not the page these
///   settings make, so that no background slider can move an accent.
/// * The page, from the background colour and the two background sliders
///   ([`page_of`]). It goes in as `color_bg_app` and `color_fg_app` written
///   into a copy of the base theme's source, so the surface ladder, the
///   opaque ladder and every wash laid over them re-derive from it.
/// * The text, put to the page at the text contrast asked for
///   ([`text_inks`]), which is also what keeps words readable on a page the
///   lightness slider has moved.
/// * The older tokens the classic controls draw from ([`ACCENTED`]).
///
/// The reading that comes back is over the theme after all of that, and
/// `every_built_theme_reads` holds it to the bar for every hue, both ends of
/// both background sliders and both appearances.
pub fn build(params: &BuilderParams) -> BuiltTheme {
    let params = params.clamped();
    let scheme = params.scheme();
    let seed = params.seed();
    let tuning = RoleTuning::HOUSE;
    let reference = house_page(scheme);
    let roles = if palette_moved(&params) {
        let [primary, secondary, tertiary, _] = params.palette();
        roles_from_colors([primary, secondary, tertiary], scheme, reference)
    } else {
        roles_for(scheme)
    };

    // The globals, in file order, and only the ones that moved: a build with
    // none derives from the base object, keeps its fonts for free, and costs
    // a slider drag nothing but forty literals.
    let mut globals: Vec<(String, TokenValue)> = Vec::new();
    for key in DIMENSIONS {
        let value = params.dimension(key);
        if file_number(scheme, key).is_none_or(|house| (house - value).abs() > 1e-9) {
            globals.push((key.to_string(), TokenValue::Num(value)));
        }
    }
    let (bg, fg) = grounds_of(&params);
    let page_moved = bg != reference;
    if page_moved {
        globals.push(("color_bg_app".to_string(), TokenValue::Color(bg)));
        globals.push(("color_fg_app".to_string(), TokenValue::Color(fg)));
    }

    // Everything measured, on the real page and on the house one: the second
    // is what the accents are chosen against as well, see `accent_pins`.
    let text_moved = page_moved || params.text_contrast != house_text_contrast(scheme);
    let known = |bg: u32, fg: u32| {
        let mut colors = page_colors(scheme, bg, fg);
        for (key, rgba) in roles.entries() {
            colors.insert(key.to_string(), rgba);
        }
        colors.insert("color_error".to_string(), KEPT_ERROR);
        colors.insert("color_warning".to_string(), KEPT_WARNING);
        let inks = text_inks(scheme, &colors, params.text_contrast, text_moved);
        for (key, rgba, _) in &inks {
            colors.insert(key.to_string(), *rgba);
        }
        (colors, inks)
    };
    let (mut colors, inks) = known(bg, fg);
    let (house_bg, house_fg) = grounds(scheme, WHITE, 0.0);
    let (house_colors, _) = known(house_bg, house_fg);
    let (grey_bg, grey_fg) = grounds_of(&BuilderParams { saturation: 0.0, ..params });
    let (grey_colors, _) = known(grey_bg, grey_fg);

    let mut overrides: Vec<(String, TokenValue)> = roles
        .entries()
        .into_iter()
        .filter(|(key, _)| *key != "color_error" && *key != "color_warning")
        .map(|(key, rgba)| (key.to_string(), TokenValue::Color(rgba)))
        .collect();

    // And the older tokens the classic controls actually draw from, where the
    // palette moved at all. See `ACCENTED`: without this a theme grown from
    // an orange favourite had an orange page and grey controls.
    let under = |colors: &BTreeMap<String, u32>| -> BTreeMap<String, u32> {
        let quiet = text_inks(scheme, colors, READABLE, true);
        quiet.into_iter().map(|(key, rgba, _)| (key.to_string(), rgba)).collect()
    };
    let pages = Pages {
        grey: Under { colors: &grey_colors, page: grey_bg, quiet: under(&grey_colors) },
        house: Under { colors: &house_colors, page: house_bg, quiet: under(&house_colors) },
        real: Under { colors: &colors, page: bg, quiet: under(&colors) },
    };
    let accents = accent_pins(&params, &roles, &pages);
    drop(pages);
    for (key, rgba) in accents.grounds.iter().chain(accents.pins.iter()) {
        colors.insert(key.clone(), *rgba);
    }
    // The text inks, less any the mapping has its own answer for: the label
    // of a selected thing is the secondary's, and a token pinned twice is a
    // token whose value depends on which line of the script came last.
    for (key, rgba, _) in inks.iter().filter(|(_, _, pin)| *pin) {
        if !accents.pins.iter().any(|(pinned, _)| pinned == key) {
            overrides.push((key.to_string(), TokenValue::Color(*rgba)));
        }
    }
    for (key, rgba) in &accents.pins {
        overrides.push((key.clone(), TokenValue::Color(*rgba)));
    }

    let readability = read_pairs(&colors, &accents.pairs, bg);
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
        accent_pairs: accents.pairs,
    };
    built.script = built.script_as(BUILT_SOURCE_NAME, BUILT_NAME, true);
    built
}

/// Every token that is words: the body ink and the tokens the files alias to
/// it, the quieter voices -- the value, the placeholder, the meta line, the
/// disabled text -- the labels inside and beside the controls in all their
/// states, and the two surface inks the newer widgets read.
///
/// All the states are named, the ones that are only an alias of another as
/// well: a pin lands on the object after it has derived, so a theme that
/// pinned `color_text` and left `color_text_hover` would be a theme whose
/// text changes contrast under the pointer. `color_text_on_accent` is not
/// here -- it is written on an accent, not on the page.
const TEXT_INKS: &[&str] = &[
    "color_text",
    "color_text_hl",
    "color_text_hover",
    "color_text_active",
    "color_text_focus",
    "color_text_down",
    "color_text_val",
    "color_text_disabled",
    "color_text_placeholder",
    "color_text_placeholder_hover",
    "color_text_meta",
    "color_label_inner",
    "color_label_inner_down",
    "color_label_inner_drag",
    "color_label_inner_hover",
    "color_label_inner_focus",
    "color_label_inner_active",
    "color_label_inner_inactive",
    "color_label_inner_disabled",
    "color_label_outer",
    "color_label_outer_off",
    "color_label_outer_down",
    "color_label_outer_drag",
    "color_label_outer_hover",
    "color_label_outer_focus",
    "color_label_outer_active",
    "color_label_outer_active_focus",
    "color_label_outer_disabled",
    "color_on_surface",
    "color_on_surface_variant",
];

/// The text inks put to the page at the contrast asked for, each with
/// whether it has to be pinned: all of them where the page or the contrast
/// moved, and otherwise only a surface ink the file's own page somehow
/// failed -- which the house page never does -- with the two surface inks
/// handed back regardless, because the reading is taken over them.
///
/// Every ink in both files is the appearance's plain end -- white in the dark
/// theme, black in the light one -- at some alpha, so an ink's contrast is
/// one number, its alpha, and that is all this moves: the body ink gets the
/// least alpha that stands `target` off the page, which is the ratio the
/// setting names.
///
/// The other voices keep the distance they had from the body ink on the
/// house page, measured as a ratio of contrasts: a placeholder that stood at
/// four fifths of the body's contrast stands at four fifths of the new one.
/// So a quieter theme is quieter throughout and a louder one louder, and the
/// voices do not all collapse onto one number at either end. And the two
/// surface inks the library HOLDS to a bar, on every rung of the surface
/// ladder, are then raised as far as it takes to clear it there, and failing
/// that to the plain end, which is `settle_ink`'s rule.
fn text_inks(scheme: Scheme, colors: &BTreeMap<String, u32>, target: f64, moved: bool) -> Vec<(&'static str, u32, bool)> {
    let page = colors.get("color_bg_app").copied().unwrap_or(BLACK);
    let house = house_page(scheme);
    let nothing = BTreeMap::new();
    let Some(body) = file_value(scheme, "color_text", &nothing) else {
        return Vec::new();
    };
    let body_house = reads_on(house, body);
    let mut out = Vec::new();
    for key in TEXT_INKS {
        let Some(base) = file_value(scheme, key, &nothing) else {
            continue;
        };
        if !moved {
            // Nothing to do but put the file's own surface inks to the
            // file's own page, which they already read on.
            if *key == "color_on_surface" || *key == "color_on_surface_variant" {
                let settled = settle_ink(colors, key, base);
                out.push((*key, settled, settled != base));
            }
            continue;
        }
        let want = target * reads_on(house, base) / body_house;
        let mut ink = alpha_for(page, base, want);
        if *key == "color_on_surface" || *key == "color_on_surface_variant" {
            while ink & 0xFF < 0xFF && !holds_its_bar(colors, key, ink) {
                ink += 1;
            }
            ink = settle_ink(colors, key, ink);
        }
        out.push((*key, ink, true));
    }
    out
}

/// The least alpha at which an ink of this colour stands `want` off `page`,
/// or the whole of it where nothing less does.
fn alpha_for(page: u32, ink: u32, want: f64) -> u32 {
    let rgb = ink & 0xFFFF_FF00;
    if reads_on(page, rgb | 0xFF) < want {
        return rgb | 0xFF;
    }
    let (mut low, mut high) = (0u32, 0xFFu32);
    while low < high {
        let middle = (low + high) / 2;
        if reads_on(page, rgb | middle) >= want {
            high = middle;
        } else {
            low = middle + 1;
        }
    }
    rgb | low
}

/// Whether an ink clears the bar on every ground the library holds it to.
fn holds_its_bar(colors: &BTreeMap<String, u32>, ink_key: &str, ink: u32) -> bool {
    held_pairs()
        .iter()
        .filter(|(_, held_ink, _)| *held_ink == ink_key)
        .all(|(ground, _, need)| colors.get(*ground).is_none_or(|g| reads_on(*g | 0xFF, ink) >= *need))
}

/// The ink to draw on every ground the library holds this ink to: the one
/// given where that reaches its bar on all of them, and otherwise
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

/// How a set of colours reads over every pair the library holds a theme to
/// AND every pair the accent mapping created, reported the way
/// `ThemeLab::readability` reports a mix: each ink laid over its ground
/// before it is measured, failures worst first, and the tightest pair named
/// whether or not it fails.
///
/// The two kinds of pair differ in one thing. A held pair names roles, which
/// are opaque; an accent pair can name a rung of the translucent ladder --
/// the box a check mark sits in is black at fifteen per cent -- so its ground
/// is laid over the page before anything is measured on it. A mark measured
/// against that black rather than against the grey it actually makes would be
/// held to a ground nobody ever sees.
fn read_pairs(colors: &BTreeMap<String, u32>, accents: &[(String, String, f64)], page: u32) -> Readability {
    let mut out = Readability::default();
    let mut margin: Option<f64> = None;
    let mut failures: Vec<(f64, String)> = Vec::new();
    let held = held_pairs().iter().map(|(g, i, need)| (g.to_string(), i.to_string(), *need, false));
    let accents = accents.iter().map(|(g, i, need)| (g.clone(), i.clone(), *need, true));
    for (ground, ink, need, composite) in held.chain(accents) {
        let (Some(g), Some(i)) = (colors.get(&ground), colors.get(&ink)) else {
            continue;
        };
        let g = if composite { over(page, *g) } else { *g | 0xFF };
        let stands = reads_on(g, *i);
        let line = format!("{ink} on {ground} = {stands:.2}, wants {need:.1}");
        out.measured += 1;
        if margin.is_none_or(|best| stands - need < best) {
            margin = Some(stands - need);
            out.tightest = line.clone();
        }
        if stands < need {
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
/// Every field is drawn, the lightness too, and with it the appearance. The
/// colour is drawn at a strength and lightness a person might actually pick:
/// a near grey one would spend the draw on a palette of greys. The text
/// contrast is drawn across what the page allows, and the dimensions round
/// the house values and not across their whole registered range: a control
/// may be driven to a 20 point corner and a 30 point paragraph, but a random
/// theme that arrives there is a broken-looking app, not a surprise.
pub fn random_params(seed: u64) -> BuilderParams {
    random_params_on(seed, None)
}

/// [`random_params`], with the lightness drawn inside one appearance's half
/// of the slider where one is named: a surprise button does not turn a dark
/// room white.
fn random_params_on(seed: u64, dark: Option<bool>) -> BuilderParams {
    let mut state = seed;
    let mut draw = |low: f64, high: f64| low + (high - low) * next_unit(&mut state);
    let hue = draw(0.0, 360.0);
    let favourite = hsl_to_rgb(hue, draw(0.55, 1.0), draw(0.42, 0.62));
    let harmony = Harmony::ALL[(draw(0.0, Harmony::ALL.len() as f64) as usize).min(Harmony::ALL.len() - 1)];
    let saturation = draw(0.0, 1.0);
    let lightness = match dark {
        Some(true) => draw(0.0, 0.5),
        Some(false) => draw(0.5, 1.0).max(0.5 + 1e-6),
        None => draw(0.0, 1.0),
    };
    let text_share = draw(0.0, 1.0);
    // Whole and half steps, which is what the sliders themselves land on.
    let mut stepped = |low: f64, high: f64| (draw(low, high) * 2.0).round() / 2.0;
    let mut params = BuilderParams {
        favourite,
        harmony,
        // A surprise is a palette the rule grew, not one off a list.
        seeds: None,
        saturation,
        lightness,
        text_contrast: READABLE,
        spacing: stepped(4.0, 9.0),
        roundness: stepped(0.0, 8.0),
        font_size: stepped(9.0, 12.0),
        font_contrast: stepped(1.5, 3.5),
    };
    let (least, most) = params.text_contrast_range();
    params.text_contrast = least + (most - least) * text_share;
    params.clamped()
}

// ---------------------------------------------------------------------------
// Suggestions: several palettes from the one colour
// ---------------------------------------------------------------------------

/// A look for a whole palette, from the top of it to the bottom.
///
/// A harmony says WHERE the other hues sit and nothing else, so the six of
/// them from one colour are six palettes of the same strength and the same
/// brightness. The mood is the other axis, and it is the one somebody points
/// at when they say they like a palette better: the same hues drawn quietly,
/// or pale, or deep.
///
/// It moves ALL FOUR colours, the primary included. A person who does not
/// like a colour that loud is not asked to go and pick a quieter one: they
/// press "muted", and the palette -- the primary, both companions and the
/// background colour -- is calm from top to bottom, while the colour they
/// picked stays picked ([`BuilderParams::favourite`]) for the next chip. The
/// background colour takes the mood's saturation only; its lightness is the
/// lightness slider's, whatever the mood.
///
/// There is no "vivid". The plain palette of a harmony IS the vivid one --
/// the companions at the primary's own saturation and lightness, and the
/// primary exactly the colour picked -- and the six plain palettes are
/// already the first six on the strip, so a vivid mood would offer each of
/// them twice.
///
/// A mood does not touch the background sliders. Those are the person's, and
/// a chip that moved them would undo a choice made on purpose every time a
/// palette was tried.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Mood {
    /// Half the colour, at the same lightness.
    Muted,
    /// Lighter and softer.
    Pastel,
    /// Darker and nearly as strong.
    Deep,
}

/// What a mood is, in numbers. Private because these are the kind of numbers
/// that are argued with by looking at them, not by reading them.
struct MoodRule {
    /// The saturation every colour keeps, as a share of its own.
    colour: f64,
    /// What is added to every accent's lightness.
    lift: f64,
}

impl Mood {
    pub const ALL: [Mood; 3] = [Mood::Muted, Mood::Pastel, Mood::Deep];

    /// The second half of a suggestion's label, in the case it is read in:
    /// "Triadic, muted".
    pub fn label(self) -> &'static str {
        match self {
            Mood::Muted => "muted",
            Mood::Pastel => "pastel",
            Mood::Deep => "deep",
        }
    }

    fn rule(self) -> MoodRule {
        match self {
            Mood::Muted => MoodRule { colour: 0.5, lift: 0.0 },
            Mood::Pastel => MoodRule { colour: 0.65, lift: 0.18 },
            Mood::Deep => MoodRule { colour: 0.9, lift: -0.18 },
        }
    }
}

/// A colour in a mood: its hue exactly, its saturation shared down and its
/// lightness lifted or dropped. No mood is the colour, byte for byte.
fn in_mood(color: u32, mood: Option<Mood>) -> u32 {
    let Some(mood) = mood else {
        return color | 0xFF;
    };
    let rule = mood.rule();
    let (hue, sat, light) = rgb_to_hsl(color | 0xFF);
    hsl_to_rgb(hue, (sat * rule.colour).clamp(0.0, 1.0), (light + rule.lift).clamp(0.08, 0.92))
}

/// The shares of the primary's saturation a companion keeps where the
/// harmony puts it on the primary's OWN hue -- the secondary of a single-hue
/// or a complementary palette, the tertiary of a single-hue one. The rule's
/// own shares, and for its reason: a companion in the same hue at the same
/// saturation and lightness is the primary a second time, and a palette of
/// three colours that shows one has lost two of them.
const SECONDARY_SHARE: f64 = 0.55;
const TERTIARY_SHARE: f64 = 0.80;

/// How near two colours have to be before a swatch is saying the same thing
/// twice: the largest difference on any channel. Measured in bytes and not in
/// hue, because a hue is a lie about a colour with no colour in it -- two
/// greys 120 degrees apart are one grey, and a favourite with no colour in it
/// grows the same palette in every harmony there is.
const SAME_COLOR: i32 = 8;

/// How much colour a colour needs before it has a hue worth sorting by, and
/// before a background colour carries any. Below this it is a grey said in a
/// roundabout way, and its hue is whatever rounding left behind.
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

/// The four colours a palette names outright, in the form [`BuilderParams`]
/// carries them. Each is used as it is, moved only as far as reading demands.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SuggestionSeeds {
    /// The primary as it will be: the favourite exactly for a plain palette,
    /// the favourite in the mood for a mood one.
    pub primary: u32,
    pub secondary: u32,
    pub tertiary: u32,
    /// The background colour, the palette's fourth. Its hue is the page's
    /// hue and its saturation is the most the page takes, at the top of the
    /// saturation slider; its lightness is only what a chip shows it at,
    /// since how light the page is belongs to the lightness slider. A grey
    /// here is a palette whose page stays grey wherever the slider is.
    pub background: u32,
}

/// One palette offered for a favourite colour: what to call it, what it looks
/// like, and what the builder needs to grow it.
#[derive(Clone, Debug, PartialEq)]
pub struct Suggestion {
    /// Whole words, the way a person would say it: "Triadic" for a plain
    /// harmony, "Triadic, muted" for a mood, a combination's number, or
    /// [`OWN_LABEL`] for one of their own.
    pub label: String,
    /// The harmony it was grown in, or `None` for one off a list, which is in
    /// no harmony the library names.
    pub harmony: Option<Harmony>,
    /// The mood it was grown in: `None` for a plain harmony and for one off a
    /// list.
    pub mood: Option<Mood>,
    /// The four colours a swatch shows, which are the four the theme is
    /// built from: the primary as it will be, the secondary, the tertiary,
    /// and the background colour at the appearance's house page lightness.
    /// None of them depends on where the background sliders are, so a strip
    /// of these does not have to be grown again when a slider moves.
    pub colors: [u32; 4],
    pub seeds: SuggestionSeeds,
}

impl Suggestion {
    /// This suggestion over some settings: its four colours and its harmony
    /// go on, and everything else stays as it was -- the favourite the
    /// person picked, both background sliders, the text contrast and the four
    /// dimensions. A chip is a palette, and trying one on does not undo the
    /// settings a person made on purpose.
    pub fn params(&self, base: BuilderParams) -> BuilderParams {
        BuilderParams { harmony: self.harmony.unwrap_or_default(), seeds: Some(self.seeds), ..base }
    }
}

/// A row of palettes to choose between, all of them grown from the one
/// colour, first the six harmonies as they are and then each of them in each
/// mood.
///
/// The first six are the six harmonies in their plain form, in
/// [`Harmony::ALL`] order and called by the harmony alone -- "Default",
/// "Single hue", "Analogous", "Complementary", "Split", "Triadic" -- with the
/// primary exactly the favourite and the companions at its own saturation and
/// lightness. They stand first and they always stand: they are what a
/// harmony picker used to offer, and a picker's worth of choices that a
/// later look-alike could push off the strip would be a picker with holes in
/// it. Where two of them are the SAME palette -- a grey favourite has no hue
/// for a harmony to turn -- the later one goes, and the strip says so by
/// being shorter.
///
/// The moods come after, as a round robin and not as two loops nested:
/// taking the harmonies in turn and stepping the mood along with them puts
/// every mood in the first three places of the run, so a strip showing eight
/// shows eight different ideas rather than one idea in three moods.
///
/// It is the same list every time for the same colour and appearance, and it
/// does not depend on the background sliders at all.
pub fn suggestions(favourite: u32, dark: bool) -> Vec<Suggestion> {
    let mut out: Vec<Suggestion> = Vec::with_capacity(Harmony::ALL.len() * (Mood::ALL.len() + 1));
    for harmony in Harmony::ALL {
        offer(&mut out, Some(grown(favourite, dark, harmony, None)));
    }
    for round in 0..Mood::ALL.len() {
        for (step, harmony) in Harmony::ALL.into_iter().enumerate() {
            let mood = Mood::ALL[(round + step) % Mood::ALL.len()];
            offer(&mut out, Some(grown(favourite, dark, harmony, Some(mood))));
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
/// then the secondary accent, the tertiary accent, and the background colour.
///
/// A list of colours has no roles in it. Somebody typed them in some order, or
/// a book printed them in one, and that order says which was written first and
/// nothing else -- so letting it decide which colour becomes the quiet accent
/// and which the ground hands the shape of a theme to an accident of typing.
///
/// What the colours themselves say decides it instead:
///
/// * The background is the dark END of the palette in a dark theme and the
///   light end in a light one: of the colours that are not the anchor, the
///   one with the least luminance on a dark page and the most on a light one.
///   A page is what a palette is laid on, and a palette laid on its own
///   darkest colour in the dark -- its lightest in the light -- is the one a
///   designer would have drawn. Least saturated, which is what this used to
///   say, picked a grey for nineteen of the book's hundred and eight
///   four-colour rows, and a page that could never carry any colour. Ties go
///   to the less saturated, and then to written order. Only where there is
///   one to spare: three colours are the three accent families exactly, and
///   taking one of them for the page would leave the theme a family short.
///   So the order depends on the appearance, and a panel that flips it grows
///   the list again.
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
fn in_role_order(scheme: &[u32], dark: bool) -> Vec<u32> {
    if scheme.len() < 3 {
        return scheme.to_vec();
    }
    let anchor = scheme[0];
    let mut rest: Vec<u32> = scheme[1..].to_vec();
    let ground = (rest.len() >= 3).then(|| rest.remove(page_end(&rest, dark)));
    // A stable sort, so colours standing equally far round the circle keep the
    // order they were written in.
    rest.sort_by(|a, b| {
        hue_gap(anchor, *a).partial_cmp(&hue_gap(anchor, *b)).unwrap_or(std::cmp::Ordering::Equal)
    });
    let accents = rest.len().min(2);
    let mut out = Vec::with_capacity(scheme.len());
    out.push(anchor);
    out.extend(rest.drain(..accents));
    out.extend(ground);
    out.extend(rest);
    out
}

/// Which of these colours is the palette's end on this side: the darkest for
/// a dark theme, the lightest for a light one, by luminance. The less
/// saturated of two that are equally dark, and then the first written.
fn page_end(colors: &[u32], dark: bool) -> usize {
    let key = |packed: u32| {
        let y = luminance(packed | 0xFF);
        (if dark { y } else { -y }, rgb_to_hsl(packed | 0xFF).1)
    };
    let mut best = 0;
    for at in 1..colors.len() {
        let (y, s) = key(colors[at]);
        let (best_y, best_s) = key(colors[best]);
        if y < best_y - 1e-12 || ((y - best_y).abs() <= 1e-12 && s < best_s) {
            best = at;
        }
    }
    best
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
        let (mut low, mut high) = (lowest.clamp(0.0, 1.0), highest.clamp(0.0, 1.0));
        let span = highest - lowest;
        if span <= f64::EPSILON {
            return low;
        }
        // A scheme that went WHOLLY past one end clamps to a single point,
        // and a rescale onto a point is every member on the one lightness:
        // two or three colours somebody wrote down handed back as one. So
        // where the clamped range is empty the scheme is slid back inside
        // instead, keeping as much of its own spacing as the range has room
        // for, hard against the end it went over.
        if high - low <= f64::EPSILON {
            if high <= 0.0 {
                high = span.min(1.0);
            } else {
                low = 1.0 - span.min(1.0);
            }
        }
        low + (l - lowest) * (high - low) / span
    };
    // `hsl_to_rgb` takes the hue round the circle for itself.
    moved.into_iter().map(|(h, s, l)| hsl_to_rgb(h, s, scale(l))).collect()
}

/// One palette in one harmony and, if any, one mood, swatch and seeds
/// together so the two cannot say different things.
///
/// The primary is the favourite, in the mood. The companions take their hue
/// from the harmony and their saturation and lightness from the primary, so
/// a plain harmony's three colours are as strong and as light as the one
/// picked; where the harmony puts a companion on the primary's own hue it
/// keeps the rule's share of the saturation instead, and the single-hue
/// tertiary steps a fifth along the lightness axis toward the middle, or the
/// three would be one colour three times. The background is
/// [`rule_background`].
fn grown(favourite: u32, dark: bool, harmony: Harmony, mood: Option<Mood>) -> Suggestion {
    let primary = in_mood(favourite, mood);
    let (hue, sat, light) = rgb_to_hsl(primary);
    let (second, third) = harmony.offsets();
    let companion = |turn: f64, share: f64, step: f64| {
        if turn.rem_euclid(360.0).abs() < 0.5 {
            hsl_to_rgb(hue, sat * share, (light + step).clamp(0.0, 1.0))
        } else {
            hsl_to_rgb(hue + turn, sat, light)
        }
    };
    let secondary = companion(second, SECONDARY_SHARE, 0.0);
    let toward_middle = if light < 0.5 { 0.2 } else { -0.2 };
    let tertiary = companion(third, TERTIARY_SHARE, if harmony == Harmony::Single { toward_middle } else { 0.0 });
    let background = rule_background(harmony, favourite, mood, [primary, secondary, tertiary], dark);
    let seeds = SuggestionSeeds { primary, secondary, tertiary, background };
    Suggestion {
        label: match mood {
            None => harmony.label().to_string(),
            Some(mood) => format!("{}, {}", harmony.label(), mood.label()),
        },
        harmony: Some(harmony),
        mood,
        colors: [primary, secondary, tertiary, background],
        seeds,
    }
}

/// The background colour the rule gives a palette that did not come with
/// one: a hue a designer would put behind those three accents, properly
/// saturated so that the top of the saturation slider is unmistakably a
/// colour, and shown at the appearance's house page lightness.
///
/// The hue, harmony by harmony:
///
/// * Single hue: the primary's own. A single-hue palette is one colour, and
///   its page is that colour too.
/// * Analogous: the next neighbour along, sixty degrees on from the primary,
///   so the run of neighbours carries on into the page.
/// * The others -- the house harmony, complementary, split and triadic --
///   the middle of the widest stretch of the circle the three accents leave
///   empty, the first such stretch where two are equally wide. A page in a
///   hue none of the accents is in lets every one of them stand out on it,
///   and it is the one hue the palette has room for.
///
/// The same rule for a scheme off a list that brought only three colours or
/// two ([`from_scheme`]), which is the widest-gap case on its own hues.
///
/// The saturation is the favourite's, kept between the two bounds of
/// [`RULE_GROUND_COLOUR`], in the mood. A favourite that is a grey has no
/// colour for a page to take, and its background is a grey.
fn rule_background(harmony: Harmony, favourite: u32, mood: Option<Mood>, accents: [u32; 3], dark: bool) -> u32 {
    let (hue, own, _) = rgb_to_hsl(favourite | 0xFF);
    let ground_hue = match harmony {
        Harmony::Single => hue,
        Harmony::Analogous => hue + 60.0,
        _ => widest_gap(&accents).unwrap_or(hue),
    };
    let share = mood.map_or(1.0, |mood| mood.rule().colour);
    let sat = if own < HAS_HUE { 0.0 } else { own.clamp(RULE_GROUND_COLOUR.0, RULE_GROUND_COLOUR.1) * share };
    ground_swatch(hsl_to_rgb(ground_hue, sat, 0.5), dark)
}

/// The middle of the widest stretch of the hue circle none of these colours
/// is in, or `None` where none of them has a hue. Colours with no colour in
/// them are passed over: a grey is not anywhere on the circle.
fn widest_gap(colors: &[u32]) -> Option<f64> {
    let mut hues: Vec<f64> = colors
        .iter()
        .map(|c| rgb_to_hsl(*c | 0xFF))
        .filter(|(_, sat, _)| *sat >= HAS_HUE)
        .map(|(hue, _, _)| hue.rem_euclid(360.0))
        .collect();
    if hues.is_empty() {
        return None;
    }
    hues.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let mut best: Option<(f64, f64)> = None;
    for (at, from) in hues.iter().enumerate() {
        let to = if at + 1 < hues.len() { hues[at + 1] } else { hues[0] + 360.0 };
        let wide = to - from;
        if best.is_none_or(|(widest, _)| wide > widest + 1e-9) {
            best = Some((wide, from + wide / 2.0));
        }
    }
    best.map(|(_, middle)| middle.rem_euclid(360.0))
}

/// One scheme off a list -- a person's own or a built-in combination --
/// already matched and re-anchored, dressed as a suggestion under the name it
/// is offered by.
///
/// The roles are handed out by [`in_role_order`] and not by where a colour
/// stood in the line, and they are handed out after the re-anchoring, because
/// it is the colours the theme will actually wear that have to carry them.
/// The colours are the scheme's own: nothing is shared down or lifted, and
/// the primary is the anchor, which is the favourite wherever the scheme did
/// not have to be brought back inside the lightness range.
///
/// A four-colour scheme brings its own background colour, and it is taken as
/// it is: its hue for the page's, and its own saturation for the most the
/// page carries, with no floor -- a combination whose fourth colour is a pale
/// beige tops out at that beige. A scheme of three colours or two has none,
/// and gets one by [`rule_background`]; a scheme of two borrows its tertiary
/// from the plain house harmony first.
fn from_scheme(favourite: u32, dark: bool, scheme: &[u32], label: String) -> Option<Suggestion> {
    let adjusted = in_role_order(&adjust_scheme(favourite, scheme), dark);
    if adjusted.len() < 2 {
        return None;
    }
    let primary = adjusted[0] | 0xFF;
    let secondary = adjusted[1] | 0xFF;
    let tertiary = adjusted.get(2).map_or_else(|| grown(favourite, dark, Harmony::House, None).seeds.tertiary, |c| c | 0xFF);
    let background = match adjusted.get(3) {
        Some(ground) => ground_swatch(*ground, dark),
        None => rule_background(Harmony::House, favourite, None, [primary, secondary, tertiary], dark),
    };
    let seeds = SuggestionSeeds { primary, secondary, tertiary, background };
    Some(Suggestion { label, harmony: None, mood: None, colors: [primary, secondary, tertiary, background], seeds })
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
    /// dark room to a white one is a different button, so the lightness is
    /// drawn inside the half of the slider the page is in.
    pub fn randomize(&mut self, seed: u64) {
        let dark = self.params.dark();
        self.set(random_params_on(seed, Some(dark)));
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
        self.params = BuilderParams::house(self.params.dark());
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
    use crate::theme_tokens::{assigned_keys, theme_keys, BlendCache, BlendValue, ThemeValues};

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
    ///
    /// Over both background sliders at the places that matter -- both ends
    /// of each appearance's half of the lightness slider, the house pages,
    /// no colour and all of it -- both ends of the text contrast, and a
    /// palette off the strip as well as one the rule grew.
    #[test]
    fn what_build_predicts_is_what_the_vm_resolves() {
        let mut cx = Cx::new(Box::new(|_, _| {}));
        cx.with_vm(|vm| {
            crate::script_mod(vm);
            let mut evaluated = 0;
            let mut check = |vm: &mut ScriptVm, params: BuilderParams| {
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
            };
            for step in 0..12 {
                for saturation in [0.0, 1.0] {
                    for lightness in [0.0, 0.5, 0.500001, house_lightness(false), 1.0] {
                        let params = BuilderParams {
                            favourite: hsl_to_rgb(step as f64 * 30.0 + 7.0, 0.8, 0.5),
                            harmony: Harmony::ALL[step % Harmony::ALL.len()],
                            saturation,
                            lightness,
                            ..BuilderParams::house(true)
                        };
                        check(vm, params);
                    }
                }
            }
            for dark in [true, false] {
                let chip = suggestions(BLUE, dark)[8].params(BuilderParams::house(dark));
                check(vm, BuilderParams { saturation: 0.5, ..chip });
                let (least, most) = chip.text_contrast_range();
                check(vm, BuilderParams { text_contrast: least, ..chip });
                check(vm, BuilderParams { text_contrast: most, ..chip });
                // The house palette on a page the lightness slider moved.
                check(vm, BuilderParams { lightness: if dark { 0.1 } else { 0.95 }, ..BuilderParams::house(dark) });
            }
            assert_eq!(evaluated, 12 * 2 * 5 + 2 * 4);
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

    /// Places on the lightness slider every sweep visits: both ends of the
    /// dark half, both ends of the light half, and each appearance's house
    /// page.
    fn lightness_ends() -> [f64; 5] {
        [0.0, 0.5, 0.500001, house_lightness(false), 1.0]
    }

    /// The point of the whole rule: whatever the favourite colour, wherever
    /// the sliders are and whichever page it is on, EVERY pair the library
    /// holds its own themes to meets its bar. Every hue at ten degree steps,
    /// every harmony, no colour in the grounds, half and all of it, at both
    /// ends of both appearances' halves of the lightness slider -- and at
    /// both ends of the text contrast.
    #[test]
    fn every_built_theme_reads() {
        let mut built_themes = 0;
        for harmony in Harmony::ALL {
            for step in 0..36 {
                for saturation in [0.0, 0.5, 1.0] {
                    for lightness in lightness_ends() {
                        let params = BuilderParams {
                            favourite: hsl_to_rgb(step as f64 * 10.0, 0.85, 0.5),
                            harmony,
                            saturation,
                            lightness,
                            ..BuilderParams::house(true)
                        };
                        let (least, most) = params.text_contrast_range();
                        for text_contrast in [params.clamped().text_contrast, least, most] {
                            let params = BuilderParams { text_contrast, ..params };
                            let built = build(&params);
                            // Every pair the library holds a theme to, and
                            // the ones this palette's own mapping created on
                            // top of them: a favourite that is not the house
                            // one moves the palette, so the controls are
                            // coloured here and measured where they are drawn.
                            assert!(built.readability.measured > held_pairs().len(), "{params:?}");
                            assert!(built.readability.holds(), "{params:?}: {:#?}", built.readability.failures);
                            assert!(built.readability.margin >= 0.0);
                            built_themes += 1;
                        }
                    }
                }
            }
        }
        assert_eq!(built_themes, 6 * 36 * 3 * 5 * 3);
        // A favourite with no colour in it, and the two that are nothing but.
        for favourite in [0x000000FFu32, 0x808080FF, 0xFFFFFFFF, 0xFFFF00FF, 0x0000FFFF] {
            for lightness in lightness_ends() {
                let params = BuilderParams { favourite, lightness, ..BuilderParams::house(true) };
                let built = build(&params);
                assert!(built.readability.holds(), "{params:?}: {:#?}", built.readability.failures);
            }
        }
        // And the house palette itself, which installs nothing where it is
        // untouched but whose page the lightness slider still moves.
        for lightness in lightness_ends() {
            let params = BuilderParams { lightness, ..BuilderParams::house(true) };
            let built = build(&params);
            assert!(built.readability.holds(), "{params:?}: {:#?}", built.readability.failures);
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
        let reading =
            read_pairs(&ladder.into_iter().chain([("color_on_surface".to_string(), grey)]).collect(), &[], BLACK);
        assert!(!reading.holds());
        assert!(reading.failures[0].starts_with("color_on_surface on color_surface_bright = "), "{:?}", reading.failures);
        assert_eq!(reading.tightest, reading.failures[0]);
        assert!(reading.margin < 0.0);
    }

    /// The defect the mapping is for, in the three places it showed worst.
    ///
    /// A theme grown from an orange favourite used to come out with an orange
    /// page and grey controls: the slider's value fill was `color_opaque_u_2`
    /// -- the page mixed toward white -- the check mark was `color_u_5`, a
    /// translucent white, and the focus ring was `#x7aa2f7`, the one fixed
    /// blue in both base theme files. Every one of them is written here as
    /// the base theme's own value, so this test fails on the theme the
    /// builder made before the mapping and cannot pass by accident.
    #[test]
    fn an_orange_theme_does_not_leave_the_controls_grey_and_the_focus_blue() {
        const ORANGE: u32 = 0xE8730CFF;
        for dark in [true, false] {
            let params = BuilderParams { favourite: ORANGE, ..BuilderParams::house(dark) };
            let built = build(&params);
            let scheme = built.scheme;
            let at = |key: &str| built.color(key).unwrap_or_else(|| panic!("{key} is not a colour the build knows"));
            let page: BTreeMap<String, u32> = ["color_bg_app", "color_fg_app"]
                .iter()
                .map(|key| (key.to_string(), at(key)))
                .collect();
            let stated = |key: &str| file_value(scheme, key, &page);
            // The one fixed blue, gone in both appearances.
            assert_eq!(stated("color_focus"), Some(if dark { 0x7AA2F7FF } else { 0x0067C0FF }));
            for key in ["color_focus", "color_ctrl_selected", "color_bevel_inset_1_focus", "color_bevel_outset_1_focus"] {
                assert_ne!(Some(at(key)), stated("color_focus"), "{key} is still the base theme's blue");
            }
            // The mark, the value fill, the selected row and the selection,
            // off the greys the files derive.
            for key in [
                "color_mark_active",
                "color_val",
                "color_val_1",
                "color_val_2",
                "color_text_cursor",
                "color_inset_active",
                "color_outset_active",
                "color_selection_focus",
            ] {
                assert_ne!(Some(at(key)), stated(key), "{key} is still the base theme's own grey");
            }
            // And what they are instead is the colour that was picked. The
            // two the library keeps the accent under are the accent itself;
            // the mark and the fill are whichever member of the primary
            // family reads where they are drawn, and in the light theme that
            // is the deeper one, because an accent at the lightness a light
            // theme gives it cannot be told from a field that is nearly the
            // page. Both are the favourite's hue, which is the part a person
            // sees.
            for key in ["color_focus", "color_ctrl_selected"] {
                assert_eq!(at(key), at("color_primary"), "{key} is not the accent");
            }
            let hue = rgb_to_hsl(ORANGE).0;
            for key in ["color_mark_active", "color_val", "color_val_2", "color_bevel_focus"] {
                assert!(apart(rgb_to_hsl(at(key)).0, hue) < 12.0, "{key} is not the favourite's hue");
            }
            // Pinned, not merely predicted: the script carries them.
            for (key, _) in &built.overrides {
                assert!(base_theme_keys().contains(&key.as_str()), "{key} is not a token");
            }
            for key in ["color_focus", "color_mark_active", "color_val"] {
                let want = TokenValue::Color(at(key));
                assert!(built.overrides.iter().any(|(k, v)| k == key && *v == want), "{key} is not pinned");
            }
            assert!(built.readability.holds(), "{:#?}", built.readability.failures);
        }
    }

    /// What the operator said, after paging through every widget page with a
    /// palette chosen: "it looks like everything just uses the first color, I
    /// don't see any of the nice palettes in the interface".
    ///
    /// So: all three brand families reach the older tokens, and each kind of
    /// thing wears the one it is supposed to. A triadic harmony puts the
    /// three families 120 degrees apart, which is what makes this measurable
    /// -- the value fill has to be in the favourite's hue, the selected row
    /// in the second, the text selection in the third, and a mapping that
    /// drove everything from the primary fails on the second assertion.
    #[test]
    fn all_three_families_reach_the_older_tokens() {
        for dark in [true, false] {
            let params = BuilderParams {
                favourite: 0xE8730CFF,
                harmony: Harmony::Triadic,
                saturation: 1.0,
                ..BuilderParams::house(dark)
            };
            let built = build(&params);
            let at = |key: &str| built.color(key).unwrap_or_else(|| panic!("{key} is not a colour the build knows"));
            let family = |key: &str| {
                let hue = rgb_to_hsl(at(key)).0;
                let near = |of: &str| apart(rgb_to_hsl(at(of)).0, hue);
                match (near("color_primary"), near("color_secondary"), near("color_tertiary")) {
                    (p, s, t) if p <= s && p <= t => "primary",
                    (_, s, t) if s <= t => "secondary",
                    _ => "tertiary",
                }
            };
            // The main action and the value.
            for key in ["color_focus", "color_bevel_focus", "color_mark_active", "color_val", "color_val_2"] {
                assert_eq!(family(key), "primary", "{key} on dark={dark}");
            }
            // Selection and the on-state.
            for key in [
                "color_inset_active",
                "color_outset_active",
                "color_outset_1_active",
                "color_highlight",
                "color_label_inner_active",
            ] {
                assert_eq!(family(key), "secondary", "{key} on dark={dark}");
            }
            // And the grounds of the controls wear none of the three: they
            // are the page's, in the background colour's hue.
            let ground = rgb_to_hsl(built.params.palette()[3]).0;
            for key in ["color_outset_hover", "color_outset", "color_inset"] {
                let hue = rgb_to_hsl(over(at("color_bg_app"), at(key))).0;
                assert!(apart(hue, ground) < 6.0, "{key} on dark={dark} is at {hue}, the background at {ground}");
            }
            // What is being pointed out.
            for key in ["color_selection_focus", "color_bg_highlight_inline", "color_text_cursor"] {
                assert_eq!(family(key), "tertiary", "{key} on dark={dark}");
            }
            // And all three are genuinely different colours, so that the
            // three assertions above are three answers and not one.
            let hue_of = |key: &str| rgb_to_hsl(at(key)).0;
            assert!(apart(hue_of("color_val"), hue_of("color_outset_active")) > 60.0);
            assert!(apart(hue_of("color_val"), hue_of("color_selection_focus")) > 60.0);
            assert!(apart(hue_of("color_outset_active"), hue_of("color_selection_focus")) > 60.0);
        }
    }

    /// A token spelt wrong pins nothing, moves nothing and fails no other
    /// test: the script would set a key the base theme does not have, the VM
    /// would carry it along beside the one that is really drawn, and the
    /// control would stay grey. So every name the mapping uses -- the tokens
    /// it pins, the grounds it measures on and the inks it protects -- has to
    /// be a key BOTH base theme files declare, and one this module can read a
    /// value for.
    #[test]
    fn every_name_the_mapping_uses_is_a_token_both_base_themes_declare() {
        let named: Vec<&str> = ACCENTED
            .iter()
            .flat_map(|row| row.tokens.iter().copied())
            .chain(accent_grounds())
            .collect();
        assert!(named.len() > 40, "only {} names", named.len());
        // The page is the one pair of names this module works out for itself
        // rather than reading, and the opaque ladder is derived off it, so a
        // reading of the file is a reading that has already been given one.
        let page: BTreeMap<String, u32> =
            [("color_bg_app".to_string(), BLACK), ("color_fg_app".to_string(), 0x303030FF)].into_iter().collect();
        for scheme in [Scheme::Dark, Scheme::Light] {
            let keys = crate::theme_tokens::theme_keys(scheme.source());
            for key in &named {
                assert!(keys.contains(key), "{key} is not a key of {}", scheme.theme_name());
                let read = file_value(scheme, key, &page).is_some();
                assert!(read, "{key} has no value in {}", scheme.theme_name());
            }
        }
        // And no token is claimed by two rows, which would make the answer
        // depend on the order the table happens to be written in.
        let mut once: Vec<&str> = Vec::new();
        for key in ACCENTED.iter().flat_map(|row| row.tokens.iter().copied()) {
            assert!(!once.contains(&key), "{key} is in the table twice");
            once.push(key);
        }
        // Every lean is an amount of a mix.
        for row in ACCENTED {
            if let Reaches::Lean { most, .. } = row.reaches {
                assert!((0.0..=1.0).contains(&most));
            }
        }
    }

    /// A theme has two forms -- pins over the base object, and a whole file
    /// with the pins written into it -- and they have to be the same theme.
    /// They part the moment the mapping pins a token some OTHER key derives
    /// from: over the object that other key keeps the base value, while in
    /// the file it is worked out again off the pin.
    ///
    /// `color_icon_inactive` is `theme.color_inset`, and leaning the inset
    /// without leaning it too made the exported file disagree with the theme
    /// on the screen -- which `the_exported_theme_is_the_built_theme_as_a
    /// _file` caught, in one token, at the far end of a long test. This says
    /// the same thing about every token in the table at once, and says it in
    /// the file's own words.
    #[test]
    fn nothing_the_mapping_leaves_alone_is_derived_from_a_token_it_pins() {
        let pinned: Vec<&str> = ACCENTED.iter().flat_map(|row| row.tokens.iter().copied()).collect();
        for scheme in [Scheme::Dark, Scheme::Light] {
            let mut owner = "";
            for line in scheme.source().lines() {
                if let Some((key, _)) = line.strip_prefix("        ").and_then(|rest| rest.split_once(':')) {
                    if !key.is_empty() && key.chars().all(is_key_char) {
                        owner = key;
                    }
                }
                for key in &pinned {
                    assert!(
                        !mentions(line, key) || pinned.contains(&owner),
                        "{owner} derives from {key} in {} and is not pinned with it",
                        scheme.theme_name()
                    );
                }
            }
        }
    }

    /// The accents land the moment the palette moves; the control GROUNDS
    /// are the only thing the saturation slider is asked about.
    ///
    /// That is the whole of the reading of the slider this stage is written
    /// against. A person who picked a colour and left both sliders alone has
    /// a coloured mark, value fill, focus ring, selection and caret, because
    /// a control that did nothing would say the panel did nothing; and the
    /// buttons and fields keep the page's own grey until the slider is asked
    /// for colour in them.
    #[test]
    fn the_accents_land_at_once_and_only_the_grounds_wait_for_the_slider() {
        for dark in [true, false] {
            let at = |saturation: f64| {
                let params = BuilderParams { favourite: BLUE, saturation, ..BuilderParams::house(dark) };
                build(&params)
            };
            let (low, middle, top) = (at(0.0), at(0.5), at(1.0));
            let pinned = |built: &BuiltTheme, key: &str| built.overrides.iter().any(|(k, _)| k == key);
            for key in [
                "color_focus",
                "color_mark_active",
                "color_val",
                "color_selection_focus",
                "color_text_cursor",
                "color_inset_active",
                "color_outset_active",
                "color_label_inner_active",
            ] {
                assert!(pinned(&low, key), "{key} does not arrive with the accents");
            }
            // In full, at every place on the slider: each of these IS a
            // member of its family and not a step toward one, so the slider
            // moves the family and never how far the token went toward it.
            // Which member is not fixed -- an accent that cannot be told
            // from the ground it lands on gives way to the family's own
            // container ink, which is the point of having one -- so the test
            // asks for the family and the readability sweep asks the rest.
            for (key, family) in [
                ("color_focus", "primary"),
                ("color_mark_active", "primary"),
                ("color_val", "primary"),
                ("color_inset_active", "secondary"),
                ("color_outset_active", "secondary"),
                ("color_label_inner_active", "secondary"),
                ("color_text_cursor", "tertiary"),
            ] {
                for built in [&low, &middle, &top] {
                    let members = [
                        format!("color_{family}"),
                        format!("color_{family}_container"),
                        format!("color_on_{family}_container"),
                    ];
                    let is = members.iter().any(|role| built.color(role) == built.color(key));
                    assert!(is, "{key} is not one of {members:?} outright");
                }
            }
            // The resting and state grounds wait, and then lean further the
            // higher the slider goes.
            for key in ["color_outset", "color_outset_hover", "color_inset"] {
                assert!(!pinned(&low, key), "{key} leaned before it was asked");
                assert!(pinned(&top, key), "{key} never leans");
            }
            // And further the higher the slider goes, measured as plain
            // distance from the grey the file states: the container the lean
            // is toward moves with the slider too, so anything measured
            // against THAT would be measuring two things at once.
            let page: BTreeMap<String, u32> = ["color_bg_app", "color_fg_app"]
                .iter()
                .map(|key| (key.to_string(), low.color(key).unwrap()))
                .collect();
            let moved = |built: &BuiltTheme, key: &str| {
                let base = file_value(built.scheme, key, &page).unwrap();
                let pinned = built.color(key).unwrap();
                let channel = |shift: u32| ((base >> shift) & 0xFF) as f64 - ((pinned >> shift) & 0xFF) as f64;
                (channel(24).powi(2) + channel(16).powi(2) + channel(8).powi(2)).sqrt()
            };
            for key in ["color_outset", "color_outset_hover", "color_inset"] {
                assert_eq!(moved(&low, key), 0.0, "{key} moved with the slider at nought");
                assert!(moved(&middle, key) > 0.0, "{key} never leans");
                assert!(moved(&top, key) > moved(&middle, key), "{key} does not follow the slider");
            }
            // The two washes lean as well, and they do NOT wait: a selection
            // is a selection as soon as there is a palette. They keep the
            // base theme's alpha through all of it, because a text input
            // draws its selection over the words.
            for key in ["color_selection_focus", "color_bg_highlight_inline"] {
                assert!(moved(&low, key) > 0.0, "{key} waited for the slider");
                for built in [&low, &middle, &top] {
                    let base = file_value(built.scheme, key, &page).unwrap();
                    assert_eq!(built.color(key).unwrap() & 0xFF, base & 0xFF, "{key} stopped being a wash");
                }
            }
            // The one function stage two re-points, and what it answers now.
            let of = |saturation: f64| {
                control_ground_lean(&BuilderParams { saturation, ..BuilderParams::house(dark) })
            };
            assert_eq!((of(0.0), of(0.5), of(1.0)), (0.0, 0.5, 1.0));
        }
    }

    /// Somebody who moved the spacing did not ask for coloured controls. The
    /// mapping hangs off the palette alone, and a build whose palette is the
    /// house one pins exactly what it pinned before there was a mapping --
    /// which is what keeps the three house tests true. The background
    /// sliders are not the palette: on the house palette the saturation
    /// colours nothing, and the lightness moves the page and the text and
    /// no accent.
    #[test]
    fn only_a_moved_palette_colours_the_controls() {
        for dark in [true, false] {
            let house = BuilderParams::house(dark);
            assert!(!palette_moved(&house));
            let roomier = BuilderParams { spacing: house.spacing + 4.0, font_size: 14.0, ..house };
            assert!(!palette_moved(&roomier));
            let built = build(&roomier);
            assert_eq!(built.overrides.len(), 27, "a spacing drag coloured something: {:?}", built.overrides);
            assert_eq!(built.readability.measured, held_pairs().len());
            assert!(!built.globals.is_empty(), "the spacing did not move at all");
            // Each of the three palette settings on its own is enough.
            for moved in [
                BuilderParams { favourite: BLUE, ..house },
                BuilderParams { harmony: Harmony::Triadic, ..house },
                house.with_palette(house.palette()),
            ] {
                assert!(palette_moved(&moved), "{moved:?}");
                assert!(build(&moved).overrides.len() > 27, "{moved:?}");
            }
            // The saturation on the house palette is nothing at all.
            let grey = build(&BuilderParams { saturation: 0.0, ..house });
            assert_eq!(grey.overrides.len(), 27);
            assert!(grey.globals.is_empty(), "{:?}", grey.globals);
            // The lightness moves the page and puts the text to it, and pins
            // no older token that is not words.
            let other = if dark { 0.2 } else { 0.95 };
            let moved = build(&BuilderParams { lightness: other, ..house });
            assert!(!palette_moved(&moved.params));
            assert!(moved.globals.iter().any(|(key, _)| key == "color_bg_app"));
            for (key, _) in moved.overrides.iter().skip(27) {
                assert!(TEXT_INKS.contains(&key.as_str()), "{key} moved with the page");
            }
            // An alpha on the favourite is not a colour setting: `seed`
            // ignores it, so a build that pinned on it would pin on nothing.
            assert!(!palette_moved(&BuilderParams { favourite: house.favourite & !0xFF, ..house }));
        }
    }

    /// The values the mapping measures on are read off the theme files, and
    /// this is what fails when a file moves one. `what_build_predicts_is_what
    /// _the_vm_resolves` holds the same reading to the VM for every colour a
    /// build now predicts; this holds the parsing itself, including the hops
    /// through the aliases that stand between a control's token and the rung
    /// it ends on.
    #[test]
    fn the_values_the_mapping_reads_are_the_ones_the_files_state() {
        let nothing = BTreeMap::new();
        let dark = |key: &str| file_value(Scheme::Dark, key, &nothing);
        // Black at fifteen per cent, four aliases down: `color_inset_active`
        // is `color_inset_hover` is `color_inset` is `color_d_1`.
        assert_eq!(dark("color_d_1"), Some(0x00000026));
        assert_eq!(dark("color_inset_active"), Some(0x00000026));
        // White at thirty-five per cent, which is the dark theme's body ink.
        assert_eq!(dark("color_text"), Some(0xFFFFFFA5));
        assert_eq!(file_value(Scheme::Light, "color_inset", &nothing), Some(0x00000019));
        // A literal, in both of the two ways a file writes one.
        assert_eq!(dark("color_focus"), Some(0x7AA2F7FF));
        assert_eq!(dark("color_w"), Some(0xFFFFFFFF));
        // The opaque ladder, which the value fill and the handles live on:
        // the inverse page mixed toward white. It needs the page, and the
        // page is the build's, not the file's.
        let page: BTreeMap<String, u32> = [("color_fg_app".to_string(), 0x303030FF)].into_iter().collect();
        assert_eq!(file_value(Scheme::Dark, "color_opaque_u_2", &page), Some(vm_mix(0x303030FF, WHITE, 0.25)));
        assert_eq!(file_value(Scheme::Dark, "color_val", &page), Some(vm_mix(0x303030FF, WHITE, 0.25)));
        // A value already worked out beats the file, or a tinted page would
        // be measured against the untinted one.
        assert_eq!(file_value(Scheme::Dark, "color_fg_app", &page), Some(0x303030FF));
        // A token the file derives some other way comes back as nothing
        // rather than as a wrong answer.
        assert_eq!(dark("color_bg_container"), None);
        assert_eq!(dark("color_nothing_at_all"), None);
        // Every name the mapping uses resolves in both files, or the reading
        // it takes is taken against a ground it guessed.
        for scheme in [Scheme::Dark, Scheme::Light] {
            let page: BTreeMap<String, u32> =
                [("color_bg_app".to_string(), BLACK), ("color_fg_app".to_string(), 0x303030FF)]
                    .into_iter()
                    .collect();
            for key in accent_grounds() {
                assert!(file_value(scheme, key, &page).is_some(), "{key} in {}", scheme.theme_name());
            }
        }
    }

    /// A colour literal as the script's own parser reads one, which is how
    /// the opaque ladder's ends and the one fixed focus blue are written.
    #[test]
    fn a_colour_literal_is_read_the_way_the_script_reads_one() {
        assert_eq!(hash_color("#F"), Some(0xFFFFFFFF));
        assert_eq!(hash_color("#0"), Some(0x000000FF));
        assert_eq!(hash_color("#FA0"), Some(0xFFAA00FF));
        assert_eq!(hash_color("#x7aa2f7"), Some(0x7AA2F7FF));
        assert_eq!(hash_color("#FFFFFF00"), Some(0xFFFFFF00));
        assert_eq!(hash_color("#xE6A294FF"), Some(0xE6A294FF));
        // Two digits is not a colour in this language, and neither is a word.
        assert_eq!(hash_color("#FF"), None);
        assert_eq!(hash_color("#zz"), None);
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

    /// The operator, on the build before this one: "if saturation is 0 the
    /// color becomes grey... the brightness slider decides if it becomes dark
    /// or light." So the saturation slider moves the page's CHROMA and
    /// nothing else. At every lightness, on both sides, the page at nought is
    /// the grey whose luminance is the coloured page's -- not black, not
    /// white, not the stock page -- and at the top it is the background
    /// colour's own hue at its own saturation. It never goes past it.
    ///
    /// Seen failing on the build it replaces, where nought was the stock page
    /// and the top multiplied it by a tint: the luminance assertion fails
    /// there at every lightness but the house one.
    #[test]
    fn saturation_moves_the_backgrounds_chroma_and_nothing_else() {
        for dark in [true, false] {
            let chip = suggestions(BLUE, dark)[0].params(BuilderParams::house(dark));
            let [_, _, _, fourth] = chip.palette();
            let (fourth_hue, fourth_sat, _) = rgb_to_hsl(fourth);
            assert!(fourth_sat > 0.3, "the rule's background is not a colour: {fourth:08X}");
            let places = if dark { [0.05, 0.3, 0.5] } else { [0.51, house_lightness(false), 1.0] };
            for lightness in places {
                let page = |saturation: f64| {
                    build(&BuilderParams { saturation, lightness, ..chip }).color("color_bg_app").unwrap()
                };
                let (grey, half, full) = (page(0.0), page(0.5), page(1.0));
                let (_, grey_sat, _) = rgb_to_hsl(grey);
                assert_eq!(grey_sat, 0.0, "{grey:08X} at nought is not a grey");
                // The same lightness to the eye: within one grey step.
                let step = luminance(0x010101FF);
                let apart_by = (luminance(grey) - luminance(full)).abs();
                assert!(
                    apart_by <= luminance(grey).max(luminance(full)) * 0.06 + step,
                    "{grey:08X} and {full:08X} at {lightness} are not one lightness: {} against {}",
                    luminance(grey),
                    luminance(full)
                );
                assert_ne!(grey, BLACK);
                assert_ne!(grey, WHITE);
                let (full_hue, full_sat, _) = rgb_to_hsl(full);
                assert!(apart(full_hue, fourth_hue) < 4.0, "{full:08X} is not the background's hue at {lightness}");
                assert!(full_sat <= fourth_sat + 0.03, "the top went past the palette's own colour");
                assert!(full_sat > fourth_sat - 0.08, "the top stopped short of the palette's colour: {full_sat}");
                // Half way is half the colour, not most of it.
                let half_sat = rgb_to_hsl(half).1;
                assert!((half_sat - fourth_sat / 2.0).abs() < 0.08, "{half_sat} against {}", fourth_sat / 2.0);
            }
            // And the house palette's background colour has no colour in it,
            // so the slider colours nothing anywhere.
            for saturation in [0.0, 0.5, 1.0] {
                let house = build(&BuilderParams { saturation, ..BuilderParams::house(dark) });
                assert_eq!(house.color("color_bg_app"), Some(house_page(house.scheme)));
                assert!(house.globals.is_empty());
            }
        }
    }

    /// The lightness slider alone decides how dark or light the page is, over
    /// the whole range from dark to light, and that makes it the control of
    /// the appearance: the lower half builds on the dark base theme, the
    /// upper half on the light one, the appearance flips exactly once, and
    /// nowhere along the way is there a theme that fails a held pair. Even to
    /// the eye: each half of the travel is spent evenly on `L*`.
    #[test]
    fn the_lightness_alone_decides_how_light_the_page_is() {
        let chip = suggestions(0xE8730CFF, true)[5].params(BuilderParams::house(true));
        for saturation in [0.0, 1.0] {
            let mut flips = 0;
            let mut last: Option<(bool, f64)> = None;
            for step in 0..=100 {
                let lightness = step as f64 / 100.0;
                let built = build(&BuilderParams { saturation, lightness, ..chip });
                assert!(built.readability.holds(), "{lightness}: {:#?}", built.readability.failures);
                let dark = built.params.dark();
                assert_eq!(built.scheme, if dark { Scheme::Dark } else { Scheme::Light });
                let lstar = lstar_of(luminance(built.color("color_bg_app").unwrap()));
                if let Some((was_dark, was)) = last {
                    flips += (was_dark != dark) as usize;
                    assert!(lstar >= was - 0.8, "the page got darker going up: {was} then {lstar} at {lightness}");
                    if was_dark == dark {
                        // One percent of the travel is about half a step of
                        // L* in either half, and never a jump.
                        assert!(lstar - was < 1.6, "{was} to {lstar} at {lightness}");
                    }
                }
                last = Some((dark, lstar));
            }
            assert_eq!(flips, 1, "the appearance flipped {flips} times");
        }
        // The halves meet at a half, and the house pages are where the
        // settings a builder opens on say.
        assert!(BuilderParams { lightness: 0.5, ..chip }.dark());
        assert!(!BuilderParams { lightness: 0.5000001, ..chip }.dark());
        for dark in [true, false] {
            let house = BuilderParams::house(dark);
            assert_eq!(house.dark(), dark);
            assert_eq!(page_of(&house), house_page(house.scheme()));
        }
        assert_eq!(BuilderParams::house(true).lightness, 0.5);
        // The two ends of each half are where the constants say.
        let at = |lightness: f64| lstar_of(luminance(page_of(&BuilderParams { saturation: 0.0, lightness, ..chip })));
        assert!((at(0.0) - DARK_DARKEST).abs() < 0.8, "{}", at(0.0));
        assert!((at(0.5) - dark_lightest()).abs() < 0.8, "{}", at(0.5));
        assert!((at(0.5000001) - LIGHT_DARKEST).abs() < 0.8, "{}", at(0.5000001));
        assert!((at(1.0) - LIGHT_LIGHTEST).abs() < 0.8, "{}", at(1.0));
    }

    /// The band is where the doc says, and it is a band because no ink
    /// reads in it: just past each edge, on a grey page, the appearance's
    /// plain ink falls short on one of its rungs. So the slider's step over
    /// it is not a design taste but the only way across.
    #[test]
    fn the_band_the_lightness_steps_over_is_the_one_no_text_reads_in() {
        let grey = |lstar: f64| at_luminance(0.0, 0.0, luminance_at(lstar));
        assert!(plain_ink_reads(Scheme::Dark, grey(dark_lightest())));
        assert!(!plain_ink_reads(Scheme::Dark, grey(dark_lightest() + 1.5)));
        assert!(plain_ink_reads(Scheme::Light, grey(LIGHT_DARKEST)));
        assert!(!plain_ink_reads(Scheme::Light, grey(LIGHT_DARKEST - 2.5)));
        // And the lightest dark page is the house dark page, read off the file.
        assert_eq!(grey(dark_lightest()), house_page(Scheme::Dark));
    }


    /// "Text contrast is the contrast with the background color." At both
    /// ends of its range it moves the text inks and nothing else, and the
    /// body ink stands off the REAL page -- after both background sliders --
    /// at the ratio the setting names, to within one step of alpha. The
    /// quieter voices keep their order below the body ink, and the two the
    /// library holds to a bar never drop under it. At its house value on the
    /// house page it leaves the text exactly as the file has it.
    #[test]
    fn text_contrast_moves_the_text_inks_and_nothing_else() {
        for dark in [true, false] {
            let chip = suggestions(BLUE, dark)[3].params(BuilderParams::house(dark));
            for (saturation, lightness) in slider_ends(dark) {
                let at = BuilderParams { saturation, lightness, ..chip };
                let (least, most) = at.text_contrast_range();
                assert_eq!(least, READABLE);
                assert!(most > least + 1.0, "no room for the text at {saturation}/{lightness}: {most}");
                let low = build(&BuilderParams { text_contrast: least, ..at });
                let high = build(&BuilderParams { text_contrast: most, ..at });
                let page = low.color("color_bg_app").unwrap();
                assert_eq!(high.color("color_bg_app"), Some(page));
                for (built, want) in [(&low, least), (&high, most)] {
                    let body = built.color("color_text").unwrap();
                    let stands = reads_on(page, body);
                    assert!(stands >= want - 1e-9, "{stands} under {want}");
                    let one_less = (body & 0xFFFF_FF00) | ((body & 0xFF).saturating_sub(1));
                    assert!(body & 0xFF == 0 || reads_on(page, one_less) < want || body & 0xFF == 0xFF);
                    assert!(built.readability.holds(), "{:#?}", built.readability.failures);
                    // The quieter voices stay quieter.
                    for quiet in ["color_text_placeholder", "color_text_meta", "color_text_disabled"] {
                        assert!(reads_on(page, built.color(quiet).unwrap()) <= stands + 1e-9, "{quiet}");
                    }
                }
                assert!(reads_on(page, high.color("color_text").unwrap()) > reads_on(page, low.color("color_text").unwrap()));
                // Nothing but words moved between the two ends.
                let differ: Vec<&String> = low
                    .overrides
                    .iter()
                    .zip(high.overrides.iter())
                    .filter(|(a, b)| a != b)
                    .map(|((key, _), _)| key)
                    .collect();
                assert_eq!(low.overrides.len(), high.overrides.len());
                assert!(!differ.is_empty());
                for key in differ {
                    assert!(TEXT_INKS.contains(&key.as_str()), "{key} moved with the text contrast");
                }
                assert_eq!(low.globals, high.globals);
            }
        }
        // The house value on the house page is the file's text, untouched:
        // no ink is pinned at all.
        for dark in [true, false] {
            let built = build(&BuilderParams { favourite: BLUE, saturation: 0.0, ..BuilderParams::house(dark) });
            for key in TEXT_INKS {
                if *key != "color_label_inner_active" {
                    assert!(!built.overrides.iter().any(|(k, _)| k == key), "{key} was pinned on the house page");
                }
            }
        }
    }

    /// Every text ink is a key both files declare, and nothing left out of
    /// the set derives from one in it: a pin lands after the object has
    /// derived, so an alias left out would keep the old contrast while the
    /// ink it names moved.
    #[test]
    fn the_text_inks_are_every_token_the_words_are_drawn_in() {
        for scheme in [Scheme::Dark, Scheme::Light] {
            let keys = crate::theme_tokens::theme_keys(scheme.source());
            for key in TEXT_INKS {
                assert!(keys.contains(key), "{key} is not a key of {}", scheme.theme_name());
                assert!(file_value(scheme, key, &BTreeMap::new()).is_some(), "{key} has no value");
            }
            let pinned: Vec<&str> = TEXT_INKS.iter().copied().chain(ACCENTED.iter().flat_map(|row| row.tokens.iter().copied())).collect();
            for line in scheme.source().lines() {
                let Some((owner, value)) = line.strip_prefix("        ").and_then(|rest| rest.split_once(": ")) else {
                    continue;
                };
                for key in TEXT_INKS {
                    assert!(!mentions(value, key) || pinned.contains(&owner), "{owner} derives from {key} and is not pinned with it");
                }
            }
        }
    }

    /// The operator's ruling on the grounds of controls: they are
    /// backgrounds. At saturation nought every resting, hovered and pressed
    /// control ground is a grey on a grey page; at one it is in the
    /// background colour's hue; and what is SELECTED or ON -- the
    /// secondary's -- is byte for byte the same at both.
    ///
    /// Seen failing on the stage before, which leant these grounds toward
    /// the secondary: the hue assertion fails on the first of them.
    #[test]
    fn control_grounds_are_backgrounds() {
        let grounds = [
            "color_outset",
            "color_outset_1",
            "color_outset_hover",
            "color_outset_1_down",
            "color_inset",
            "color_inset_hover",
            "color_inset_down",
        ];
        let selected = [
            "color_inset_active",
            "color_outset_active",
            "color_outset_1_active",
            "color_outset_2_active",
            "color_highlight",
            "color_label_inner_active",
        ];
        for dark in [true, false] {
            for suggestion in suggestions(0xE8730CFF, dark).iter().take(6) {
                let chip = suggestion.params(BuilderParams::house(dark));
                let (grey, full) = (build(&BuilderParams { saturation: 0.0, ..chip }), build(&chip));
                let fourth = rgb_to_hsl(chip.palette()[3]).0;
                for key in grounds {
                    let drawn = |built: &BuiltTheme| over(built.color("color_bg_app").unwrap(), built.color(key).unwrap());
                    assert_eq!(rgb_to_hsl(drawn(&grey)).1, 0.0, "{key} under {} is not a grey", suggestion.label);
                    let hue = rgb_to_hsl(drawn(&full)).0;
                    assert!(apart(hue, fourth) < 6.0, "{key} under {} is at {hue}, the background at {fourth}", suggestion.label);
                    // And the wash itself was leant into the background's
                    // hue, not merely laid over a page that is in it.
                    let wash = full.color(key).unwrap() | 0xFF;
                    assert!(full.overrides.iter().any(|(k, _)| k == key), "{key} was never leant");
                    let own = rgb_to_hsl(wash).0;
                    assert!(apart(own, fourth) < 6.0, "{key} under {} was leant to {own}, not {fourth}", suggestion.label);
                }
                for key in selected {
                    assert_eq!(grey.color(key), full.color(key), "{key} under {} moved with the saturation", suggestion.label);
                }
            }
        }
    }

    /// Neither background slider touches an accent. The roles -- every
    /// member of all seven families -- are byte for byte the same at no
    /// colour and all of it and at both ends of the appearance's half of the
    /// lightness slider, while the page, which is what the sliders are for,
    /// moves.
    ///
    /// And every older token the palette reaches that is not a ground is the
    /// same at every saturation, save where the coloured page could not hold
    /// the grey page's choice: each token that differs is shown to be one
    /// whose grey-page value FAILS its pair on the coloured page, which is
    /// the only licence an accent has to move. The text contrast is at its
    /// floor here so that the words the fills give way to are the words
    /// measured.
    #[test]
    fn the_background_sliders_leave_every_accent_where_it_is() {
        let accents: Vec<&str> = ACCENTED
            .iter()
            .filter(|row| row.from != Source::Background)
            .flat_map(|row| row.tokens.iter().copied())
            .collect();
        let mut forced = 0;
        let mut kept = 0;
        for dark in [true, false] {
            for favourite in [0xE8730CFFu32, BLUE, 0x20A040FF] {
                for suggestion in suggestions(favourite, dark).iter().step_by(2) {
                    let chip = BuilderParams { text_contrast: READABLE, ..suggestion.params(BuilderParams::house(dark)) };
                    let places = slider_ends(dark);
                    let built: Vec<BuiltTheme> = places
                        .iter()
                        .map(|(saturation, lightness)| {
                            build(&BuilderParams { saturation: *saturation, lightness: *lightness, ..chip })
                        })
                        .collect();
                    for (at, other) in built.iter().enumerate().skip(1) {
                        assert_eq!(other.roles, built[0].roles, "{} at {:?}", suggestion.label, places[at]);
                        if at % 3 == 0 {
                            assert_ne!(other.color("color_bg_app"), built[0].color("color_bg_app"));
                        }
                    }
                    for ends in built.chunks(3) {
                        let grey = &ends[0];
                        for coloured in &ends[1..] {
                            for key in &accents {
                                if coloured.color(key) == grey.color(key) {
                                    kept += 1;
                                    continue;
                                }
                                let mut colors = coloured.colors.clone();
                                colors.insert(key.to_string(), grey.color(key).unwrap());
                                let page = coloured.color("color_bg_app").unwrap();
                                let reading = read_pairs(&colors, &coloured.accent_pairs, page);
                                assert!(
                                    !reading.holds(),
                                    "{key} under {} moved with the saturation when the grey page's choice reads",
                                    suggestion.label
                                );
                                forced += 1;
                            }
                        }
                    }
                }
            }
        }
        // Forced is the exception it is written up as.
        assert!(forced * 50 < kept, "{forced} forced against {kept} kept");
    }

    /// Nothing a control can hand over reaches the colour maths or a theme
    /// file raw.
    #[test]
    fn settings_are_brought_inside_what_they_can_mean() {
        let wild = BuilderParams {
            saturation: 9.0,
            lightness: -3.0,
            text_contrast: 90.0,
            spacing: 500.0,
            roundness: -4.0,
            font_size: 1000.0,
            font_contrast: -1.0,
            ..blue(true)
        };
        let tame = wild.clamped();
        assert_eq!((tame.saturation, tame.lightness), (1.0, 0.0));
        assert_eq!(tame.text_contrast, tame.text_contrast_range().1, "a contrast past white on the page");
        let low = BuilderParams { text_contrast: 1.0, ..blue(true) }.clamped();
        assert_eq!(low.text_contrast, READABLE, "a contrast under the readable floor");
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
        let lost =
            BuilderParams { saturation: f64::NAN, lightness: f64::NAN, text_contrast: f64::NAN, ..blue(true) }.clamped();
        assert_eq!((lost.saturation, lost.lightness), (1.0, 0.5));
        assert_eq!(lost.text_contrast, house_text_contrast(Scheme::Dark));
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
            if !pages.contains(&params.dark()) {
                pages.push(params.dark());
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
                // No colour in the page, so that the page is not one of the
                // globals that moved.
                let params = BuilderParams {
                    spacing: 9.0,
                    roundness: 7.0,
                    font_size: 12.0,
                    font_contrast: 3.0,
                    saturation: 0.0,
                    ..blue(dark)
                };
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
            assert!(looked.params().dark(), "a sheet written against the dark theme opens the dark page");
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

    /// The operator: "if default, single hue, analogous, complementary, split
    /// and triadic are just some variations, they can be the first 6 preset
    /// palettes". So the strip opens on the six harmonies as they are, in the
    /// order a harmony picker listed them and under the harmony's own name,
    /// and only then the moods -- a round robin whose front is varied -- with
    /// nothing called "vivid", which would be the first six again.
    #[test]
    fn a_favourite_offers_the_six_harmonies_first_and_then_their_moods() {
        for dark in [true, false] {
            let made = suggestions(BLUE, dark);
            assert_eq!(made.len(), Harmony::ALL.len() * (Mood::ALL.len() + 1), "{dark}");
            assert_eq!(made, suggestions(BLUE, dark), "the same colour asked twice");
            let labels: Vec<&str> = made[..6].iter().map(|s| s.label.as_str()).collect();
            assert_eq!(labels, ["Default", "Single hue", "Analogous", "Complementary", "Split", "Triadic"]);
            for (at, harmony) in Harmony::ALL.into_iter().enumerate() {
                assert_eq!((made[at].harmony, made[at].mood), (Some(harmony), None));
                // Plain: the primary is the favourite, exactly.
                assert_eq!(made[at].colors[0], BLUE);
            }
            // The moods after, every mood in the first three of them, and no
            // label that says vivid.
            for mood in Mood::ALL {
                assert!(made[6..9].iter().any(|s| s.mood == Some(mood)), "{mood:?} is not at the front of the moods");
            }
            assert!(made.iter().all(|s| !s.label.contains("vivid")));
            // Every pair the rule can make, once each, and named in words.
            let mut pairs: Vec<(Harmony, Option<Mood>)> = made.iter().map(|s| (s.harmony.unwrap(), s.mood)).collect();
            pairs.sort_by_key(|(h, m)| (format!("{h:?}"), format!("{m:?}")));
            pairs.dedup();
            assert_eq!(pairs.len(), made.len());
            let triadic = made.iter().find(|s| s.harmony == Some(Harmony::Triadic) && s.mood == Some(Mood::Muted));
            assert_eq!(triadic.unwrap().label, "Triadic, muted");
        }
    }

    /// The first six always stand. A later palette that came out like one of
    /// them is the one dropped, never the other way about -- here a person's
    /// own scheme that IS the plain triadic, offered after it.
    #[test]
    fn a_look_alike_later_on_never_pushes_out_one_of_the_first_six() {
        let triadic = suggestions(BLUE, true)[5].clone();
        let copy = vec![triadic.colors[0], triadic.colors[1], triadic.colors[2]];
        let all = all_suggestions(BLUE, true, &[copy]);
        assert_eq!(all[5], triadic);
        assert_eq!(theirs(&all), 0, "the copy was offered as well");
    }

    /// A favourite with no colour in it has no hue for a harmony to turn, so
    /// the six plain palettes are one palette and the strip says so by being
    /// shorter. What is left is that one and the moods that genuinely move
    /// something: muted takes half of no colour, which is none, so it goes
    /// too.
    #[test]
    fn a_grey_favourite_collapses_to_a_handful() {
        for dark in [true, false] {
            let grey = suggestions(0x808080FF, dark);
            let labels: Vec<String> = grey.iter().map(|s| s.label.clone()).collect();
            // The single-hue palette keeps its tertiary a step along the
            // lightness axis, grey or not, so it is the one plain palette
            // left beside the first; and a mood of a grey that lifts or
            // drops it is a palette of its own.
            assert_eq!(&labels[..2], ["Default", "Single hue"], "{dark}");
            assert!(grey.len() < 8, "{labels:?}");
            assert!(labels.iter().all(|label| !label.contains("muted")), "half of no colour is no colour: {labels:?}");
            for suggestion in &grey {
                assert_eq!(rgb_to_hsl(suggestion.seeds.background).1, 0.0, "a grey coloured the page");
            }
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
            seeds: Some(SuggestionSeeds { primary: BLUE, secondary: green, tertiary: purple, background: 0x404040FF }),
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

    /// "A palette is four colours, and the fourth is the background's
    /// colour." Every suggestion carries a fourth colour that is visibly a
    /// colour unless the favourite itself is a grey -- the rule's at least;
    /// a list's is taken as it is -- and none of them depends on the sliders.
    /// With the saturation at the top the built page is in that colour's
    /// hue; at nought it is a grey whatever the colour is.
    ///
    /// Seen failing on the build it replaces: the fourth square there was the
    /// page under a faint tint, a near grey for every palette, and the
    /// saturation assertion fails on the first suggestion.
    #[test]
    fn the_fourth_colour_is_the_background_and_the_page_wears_it() {
        // A colour out of the book, so that combinations are offered too.
        let favourite = COMBINATIONS.iter().find(|row| row.len() == 4).unwrap()[0] | 0xFF;
        let own = vec![vec![favourite, 0xE0A020FF, 0x20C080FF, 0x6A1B3AFF], vec![favourite, 0x40D0D0FF]];
        for dark in [true, false] {
            let base = BuilderParams { spacing: 9.0, font_size: 11.5, ..BuilderParams::house(dark) };
            let offered = all_suggestions(favourite, dark, &own);
            assert!(!book(&offered).is_empty() && theirs(&offered) > 0, "nothing but the rule's was offered");
            for suggestion in &offered {
                let [_, _, _, fourth] = suggestion.colors;
                assert_eq!(fourth, suggestion.seeds.background);
                let (hue, sat, _) = rgb_to_hsl(fourth);
                if suggestion.harmony.is_some() {
                    assert!(sat > 0.15, "{} on dark={dark}: {fourth:08X} is not a colour", suggestion.label);
                }
                // A chip moves neither background slider, and what it shows
                // is what it puts on.
                let params = suggestion.params(base);
                assert_eq!((params.saturation, params.lightness), (base.saturation, base.lightness));
                assert_eq!((params.spacing, params.font_size), (9.0, 11.5));
                assert_eq!(params.palette(), suggestion.colors);
                let top = build(&params);
                let page = top.color("color_bg_app").unwrap();
                if sat >= 0.08 {
                    let at = rgb_to_hsl(page).0;
                    assert!(apart(at, hue) < 4.0, "{}: {page:08X} is not {fourth:08X}'s hue", suggestion.label);
                }
                let none = build(&BuilderParams { saturation: 0.0, ..params });
                assert_eq!(rgb_to_hsl(none.color("color_bg_app").unwrap()).1, 0.0, "{}", suggestion.label);
            }
        }
        // A grey favourite's background is a grey.
        for suggestion in suggestions(0x777777FF, true) {
            assert_eq!(rgb_to_hsl(suggestion.colors[3]).1, 0.0);
        }
    }

    /// The operator, on the colour picked becoming the primary exactly: "if I
    /// don't like saturated color I should pick another color and strip????"
    /// No -- that is what the moods are for. A plain chip puts the colour
    /// picked on as the primary exactly; a mood chip puts on the favourite in
    /// that mood -- the same hue, less colour for muted -- and leaves the
    /// favourite itself where it was, so pressing a plain chip afterwards
    /// brings the exact colour back.
    #[test]
    fn a_mood_reaches_the_primary_and_the_favourite_stays_picked() {
        let favourite = 0xE8730CFF;
        for dark in [true, false] {
            let house = BuilderParams { favourite, ..BuilderParams::house(dark) };
            let offered = suggestions(favourite, dark);
            let muted = offered.iter().find(|s| s.mood == Some(Mood::Muted)).unwrap();
            let (hue, sat, _) = rgb_to_hsl(favourite);
            let (muted_hue, muted_sat, _) = rgb_to_hsl(muted.colors[0]);
            assert!(muted_sat < sat - 0.2, "{muted_sat} against {sat}");
            assert!(apart(muted_hue, hue) < 2.0);
            // The whole palette is calmer, the background colour too.
            let plain = offered.iter().find(|s| s.harmony == muted.harmony && s.mood.is_none()).unwrap();
            for at in 1..4 {
                assert!(rgb_to_hsl(muted.colors[at]).1 < rgb_to_hsl(plain.colors[at]).1, "colour {at}");
            }
            let pressed = muted.params(house);
            assert_eq!(pressed.favourite, favourite, "the mood chip moved the colour picked");
            let built = build(&pressed);
            let primary = built.color("color_primary").unwrap();
            if contrast_of(muted.colors[0], house_page(built.scheme)) >= LEGIBLE {
                assert_eq!(primary, muted.colors[0], "the primary is not the chip's");
            } else {
                assert!(apart(rgb_to_hsl(primary).0, muted_hue) < 3.0, "reading moved the hue");
            }
            let back = plain.params(pressed);
            assert_eq!(back.favourite, favourite);
            assert_eq!(back.palette()[0], favourite, "the plain chip did not bring it back");
            let orange = build(&back).color("color_primary").unwrap();
            if contrast_of(favourite, house_page(built.scheme)) >= LEGIBLE {
                assert_eq!(orange, favourite, "the plain chip did not bring it back");
            }
        }
    }

    fn contrast_of(a: u32, b: u32) -> f64 {
        crate::theme_tokens::contrast(a, b)
    }

    /// The colour picked IS the primary, byte for byte, wherever it reads --
    /// and where it cannot be told from the page it moves along the
    /// lightness axis only, never its hue or its saturation, and only as far
    /// as it takes to stand off the page at `LEGIBLE`. Container and
    /// on-container follow from it.
    ///
    /// Seen failing on the build it replaces, where the favourite was read
    /// for its hue and the rule put its own saturation and lightness on it.
    #[test]
    fn the_colour_picked_comes_back_as_the_primary() {
        for dark in [true, false] {
            for step in 0..24 {
                for (sat, light) in [(0.9, 0.5), (0.6, 0.3), (0.5, 0.75), (0.3, 0.55)] {
                    let favourite = hsl_to_rgb(step as f64 * 15.0, sat, light);
                    let built = build(&BuilderParams { favourite, ..BuilderParams::house(dark) });
                    let primary = built.color("color_primary").unwrap();
                    let page = house_page(built.scheme);
                    if contrast_of(favourite, page) >= LEGIBLE {
                        assert_eq!(primary, favourite, "{favourite:08X} on dark={dark}");
                    } else {
                        let (h, s, _) = rgb_to_hsl(favourite);
                        let (ph, ps, pl) = rgb_to_hsl(primary);
                        assert!(apart(h, ph) < 3.0 && (s - ps).abs() < 0.04, "{favourite:08X} became {primary:08X}");
                        assert!(contrast_of(primary, page) >= LEGIBLE);
                        // And no further than it had to: a step back and it
                        // would not have stood off the page.
                        let back = hsl_to_rgb(h, s, if dark { pl - 0.012 } else { pl + 0.012 });
                        assert!(contrast_of(back, page) < LEGIBLE, "{favourite:08X} went further than it had to");
                    }
                    assert!(reads_on(primary, built.color("color_on_primary").unwrap()) >= READABLE);
                }
            }
        }
    }

    /// The four colours can be named by hand, and the palette in force can
    /// be asked for: `with_palette(p).palette() == p`, and a chip's own
    /// settings and the same four colours named by hand build one theme.
    #[test]
    fn four_colours_named_by_hand_round_trip() {
        for dark in [true, false] {
            let house = BuilderParams::house(dark);
            for suggestion in all_suggestions(BLUE, dark, &[]).iter().step_by(3) {
                let chip = suggestion.params(house);
                assert_eq!(chip.palette(), suggestion.colors);
                let by_hand = house.with_palette(suggestion.colors);
                assert_eq!(by_hand.palette(), suggestion.colors);
                assert_eq!(by_hand.favourite, suggestion.colors[0]);
                let (a, b) = (build(&chip), build(&by_hand));
                assert_eq!(a.colors, b.colors, "{}", suggestion.label);
                assert_eq!(a.overrides, b.overrides, "{}", suggestion.label);
                assert_eq!(a.globals, b.globals, "{}", suggestion.label);
                // And the built primary, secondary and tertiary are those
                // colours wherever they stand off the house page.
                for (at, key) in ["color_primary", "color_secondary", "color_tertiary"].into_iter().enumerate() {
                    let named = suggestion.colors[at];
                    if contrast_of(named, house_page(a.scheme)) >= LEGIBLE {
                        assert_eq!(a.color(key), Some(named), "{key} of {}", suggestion.label);
                    }
                }
            }
            // Anything named, whatever it is: the four come back as named.
            let named = [0x123456FF, 0xABCDEFFF, 0x7F7F00FF, 0x302010FF];
            assert_eq!(house.with_palette(named).palette(), named);
            // Naming leaves the sliders where they were.
            let set = BuilderParams { saturation: 0.3, ..house };
            let named_over = set.with_palette(named);
            assert_eq!((named_over.saturation, named_over.lightness), (0.3, house.lightness));
        }
        // The house palette is the house roles, and the house page.
        let house = BuilderParams::house(true);
        let roles = roles_for(Scheme::Dark);
        let want = [roles.primary.base, roles.secondary.base, roles.tertiary.base, house_page(Scheme::Dark)];
        assert_eq!(house.palette(), want);
    }

    /// Both ends of an appearance's half of the lightness slider, crossed
    /// with no colour in the page, half of it and all of it: the six places
    /// every sweep over the suggestions builds each one at.
    fn slider_ends(dark: bool) -> Vec<(f64, f64)> {
        let ends = if dark { [0.0, 0.5] } else { [0.500001, 1.0] };
        ends.iter().flat_map(|l| [0.0, 0.5, 1.0].map(|s| (s, *l))).collect()
    }

    /// The sweep the plain builder goes through, over the suggestions
    /// instead: every palette offered for a hue at ten degree steps, on both
    /// pages, at both ends of the page's lightness and with none, half and
    /// all of the background colour, and the greys, builds a theme where
    /// every pair the library holds a theme to meets its bar. The colours a
    /// suggestion names are used as they are -- the primary is the colour
    /// picked -- and this is what says that costs no reading.
    #[test]
    fn every_suggested_theme_reads() {
        let mut checked = 0;
        for dark in [true, false] {
            for step in 0..36 {
                let favourite = hsl_to_rgb(step as f64 * 10.0, 0.85, 0.5);
                for suggestion in suggestions(favourite, dark) {
                    for (saturation, lightness) in slider_ends(dark) {
                        let base = BuilderParams { saturation, lightness, ..BuilderParams::house(dark) };
                        let built = build(&suggestion.params(base));
                        assert!(built.readability.measured > held_pairs().len());
                        assert!(
                            built.readability.holds(),
                            "{} for {favourite:08X} at {saturation}/{lightness}: {:#?}",
                            suggestion.label,
                            built.readability.failures
                        );
                        assert!(built.readability.margin >= 0.0);
                        checked += 1;
                    }
                }
            }
        }
        assert_eq!(checked, 2 * 36 * 24 * 6);
        for favourite in [0x000000FFu32, 0x808080FF, 0xFFFFFFFF] {
            for dark in [true, false] {
                for suggestion in suggestions(favourite, dark) {
                    for (saturation, lightness) in slider_ends(dark) {
                        let base = BuilderParams { saturation, lightness, ..BuilderParams::house(dark) };
                        let built = build(&suggestion.params(base));
                        assert!(built.readability.holds(), "{favourite:08X}: {:#?}", built.readability.failures);
                    }
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

    /// A scheme that went wholly past one end comes back whole, and not as
    /// one colour repeated.
    ///
    /// The rescale lays the moved lightnesses over the range they land in
    /// once it is clamped, and a scheme entirely below nought clamps to the
    /// single point nought -- so every member is mapped onto it and two or
    /// three colours a person wrote down come back as one. The strip then
    /// offers a chip of identical bands under their own name.
    #[test]
    fn a_scheme_that_went_past_the_end_comes_back_whole() {
        let at = |light: f64| hsl_to_rgb(200.0, 0.6, light);
        let light = |rgba: u32| rgb_to_hsl(rgba).2;
        // Offsets of nought, -0.10 and -0.20 laid on a favourite at nought:
        // every one of them wants to be below the floor.
        let under = adjust_scheme(at(0.0), &[at(0.30), at(0.20), at(0.10)]);
        assert!(under.iter().all(|c| (0.0..=1.0).contains(&light(*c))));
        assert!(light(under[0]) > light(under[1]) && light(under[1]) > light(under[2]));
        assert!((light(under[2]) - 0.0).abs() < 0.01, "{}", light(under[2]));
        // The spacing it was written with, kept: a scheme that fits inside
        // the range once it is slid back in has nothing to be squeezed for.
        assert!((light(under[0]) - 0.20).abs() < 0.01, "{}", light(under[0]));
        assert!((light(under[1]) - 0.10).abs() < 0.01, "{}", light(under[1]));
        // And the other end, where the whole scheme wants to be over one.
        let over = adjust_scheme(at(1.0), &[at(0.60), at(0.70), at(0.80)]);
        assert!(over.iter().all(|c| (0.0..=1.0).contains(&light(*c))));
        assert!(light(over[0]) < light(over[1]) && light(over[1]) < light(over[2]));
        assert!((light(over[0]) - 0.80).abs() < 0.01, "{}", light(over[0]));
        assert!((light(over[2]) - 1.0).abs() < 0.01, "{}", light(over[2]));
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
                    for (saturation, lightness) in slider_ends(dark) {
                        let base = BuilderParams { saturation, lightness, ..BuilderParams::house(dark) };
                        let built = build(&suggestion.params(base));
                        assert!(built.readability.measured > held_pairs().len());
                        assert!(
                            built.readability.holds(),
                            "{} for {favourite:08X} at {saturation}/{lightness}: {:#?}",
                            suggestion.label,
                            built.readability.failures
                        );
                        assert!(built.readability.margin >= 0.0);
                    }
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
    /// the dark end is the background in a dark theme and the light end in a
    /// light one, and of the two left the one nearer the favourite round the
    /// circle is the secondary and the farther the tertiary.
    #[test]
    fn a_scheme_takes_its_roles_from_the_colours_and_not_the_written_order() {
        let favourite = hsl_to_rgb(210.0, 0.7, 0.5);
        let near = hsl_to_rgb(250.0, 0.7, 0.5);
        let far = hsl_to_rgb(40.0, 0.75, 0.5);
        for (dark, ground) in [(true, hsl_to_rgb(100.0, 0.5, 0.12)), (false, hsl_to_rgb(100.0, 0.5, 0.93))] {
            let rest = [near, far, ground];
            for way in WRITTEN_WAYS {
                let written: Vec<u32> = std::iter::once(favourite).chain(way.map(|at| rest[at])).collect();
                // And the anchor itself written in every place, because a
                // list does not put the colour somebody will pick first
                // either.
                for turn in 0..written.len() {
                    let mut scheme = written.clone();
                    scheme.rotate_left(turn);
                    let offered = all_suggestions(favourite, dark, &[scheme.clone()]);
                    let mine = off_the_list(&offered, OWN_LABEL);
                    let hue = |packed: u32| rgb_to_hsl(packed).0;
                    assert!(apart(hue(mine.seeds.secondary), 250.0) < 1.0, "{scheme:08X?} took {:08X} as the secondary", mine.seeds.secondary);
                    assert!(apart(hue(mine.seeds.tertiary), 40.0) < 1.0, "{scheme:08X?} took {:08X} as the tertiary", mine.seeds.tertiary);
                    let background = mine.seeds.background;
                    assert!(apart(hue(background), 100.0) < 2.0, "{scheme:08X?} on dark={dark} took {background:08X}");
                }
            }
        }
    }

    /// Which of a four-colour scheme's colours becomes the background, over
    /// the whole book, measured the way the operator measured it: least
    /// saturated picked a grey for nineteen of the hundred and eight
    /// four-colour rows. The dark end in a dark theme and the light end in a
    /// light one lands in at least five of the six sextants of the hue
    /// circle, and on fewer than fifteen greys, in each appearance.
    ///
    /// Seen failing with least-saturated put back in place of the dark and
    /// light ends.
    #[test]
    fn the_background_of_a_scheme_is_its_dark_end_or_its_light_end() {
        let fours: Vec<&[u32]> = COMBINATIONS.iter().copied().filter(|row| row.len() == 4).collect();
        assert_eq!(fours.len(), 108);
        for dark in [true, false] {
            let mut sextants = [0usize; 6];
            let mut greys = 0;
            for row in &fours {
                let ordered = in_role_order(row, dark);
                let (hue, sat, _) = rgb_to_hsl(ordered[3] | 0xFF);
                if sat < HAS_HUE {
                    greys += 1;
                } else {
                    sextants[(hue.rem_euclid(360.0) / 60.0) as usize % 6] += 1;
                }
                // The background is the end of the palette on this side.
                let y = luminance(ordered[3] | 0xFF);
                for other in &ordered[1..3] {
                    let theirs = luminance(*other | 0xFF);
                    assert!(if dark { y <= theirs } else { y >= theirs }, "{row:08X?} on dark={dark}");
                }
            }
            let covered = sextants.iter().filter(|n| **n > 0).count();
            assert!(covered >= 5, "the backgrounds land in {covered} sextants on dark={dark}: {sextants:?}");
            assert!(greys < 15, "{greys} greys on dark={dark}");
        }
    }

    /// Three colours are the three accent families exactly, so none of them
    /// is taken for the page -- the rule supplies one, in the widest stretch
    /// of the circle the three leave empty -- and the two that are not the
    /// favourite go near-then-far like the four's do. A colour with no colour
    /// in it cannot be near anything, so it sorts farthest and lands in the
    /// contrast place.
    #[test]
    fn three_colours_keep_their_accents_and_a_grey_sorts_farthest() {
        let favourite = hsl_to_rgb(210.0, 0.7, 0.5);
        let near = hsl_to_rgb(250.0, 0.7, 0.5);
        let far = hsl_to_rgb(40.0, 0.75, 0.5);
        for way in [[0, 1], [1, 0]] {
            let rest = [near, far];
            let scheme: Vec<u32> = std::iter::once(favourite).chain(way.map(|at| rest[at])).collect();
            let offered = all_suggestions(favourite, true, &[scheme.clone()]);
            let mine = off_the_list(&offered, OWN_LABEL);
            assert!(apart(rgb_to_hsl(mine.seeds.secondary).0, 250.0) < 1.0, "{scheme:08X?}");
            assert!(apart(rgb_to_hsl(mine.seeds.tertiary).0, 40.0) < 1.0, "{scheme:08X?}");
            // 40, 210 and 250 leave 40..210 widest: the page is at 125.
            let (hue, sat, _) = rgb_to_hsl(mine.seeds.background);
            assert!(apart(hue, 125.0) < 3.0 && sat > 0.3, "{:08X}", mine.seeds.background);
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

    /// What a widget template says a property reads: the key after `theme.`
    /// on the first line that opens with `property: `, inside the block that
    /// opens with `within` where one is named. Read off the widget's own text
    /// so that a test about a widget's colours is a test about the colours
    /// the widget really draws with, and follows it when it changes.
    fn template_token(source: &str, within: Option<&str>, property: &str) -> String {
        let opening = format!("{property}: ");
        let from = within.map(|block| source.find(block).expect("the block is in the template")).unwrap_or(0);
        let line = source[from..]
            .lines()
            .map(str::trim)
            .find(|line| line.starts_with(&opening))
            .unwrap_or_else(|| panic!("no {property} in the template"));
        let at = line.find("theme.").unwrap_or_else(|| panic!("{property} reads no token: {line}")) + "theme.".len();
        line[at..].chars().take_while(|c| is_key_char(*c)).collect()
    }

    /// A token of a built theme, whether the build pinned it, measured it or
    /// left it to the base theme's file.
    fn resolved(built: &BuiltTheme, key: &str) -> u32 {
        file_value(built.scheme, key, &built.colors).unwrap_or_else(|| panic!("{key} has no value"))
    }

    /// Every palette the readability sweeps build, handed over one at a time
    /// with a name for it: each rule-grown suggestion for a hue every ten
    /// degrees and for the three greys, and each combination out of the book
    /// a middling colour finds, on both pages, at both ends of the page's
    /// lightness and with none, half and all of the background colour.
    fn each_swept_palette(mut each: impl FnMut(String, &BuiltTheme)) {
        let mut at = |label: &str, favourite: u32, dark: bool, suggestion: &Suggestion| {
            for (saturation, lightness) in slider_ends(dark) {
                let base = BuilderParams { saturation, lightness, ..BuilderParams::house(dark) };
                let built = build(&suggestion.params(base));
                each(format!("{label} for {favourite:08X} at {saturation}/{lightness}"), &built);
            }
        };
        for dark in [true, false] {
            for step in 0..36 {
                let favourite = hsl_to_rgb(step as f64 * 10.0, 0.85, 0.5);
                for suggestion in suggestions(favourite, dark) {
                    at(&suggestion.label, favourite, dark, &suggestion);
                }
                let middling = hsl_to_rgb(step as f64 * 10.0, 0.55, 0.5);
                for suggestion in book(&all_suggestions(middling, dark, &[])) {
                    at(&suggestion.label, middling, dark, suggestion);
                }
            }
            for favourite in [0x000000FFu32, 0x808080FF, 0xFFFFFFFF] {
                for suggestion in suggestions(favourite, dark) {
                    at(&suggestion.label, favourite, dark, &suggestion);
                }
            }
        }
    }

    /// The WheelPicker's band in each state it has, as the template draws
    /// it: the well of that state, the band laid over it, and the ink of the
    /// row in the band, with the bar that state's words answer to. Read off
    /// the widget's own text, so the test follows the widget when it moves.
    fn wheel_band_states() -> Vec<(&'static str, String, String, String, f64)> {
        let source = include_str!("wheel_picker.rs");
        let well = |property: &str| template_token(source, Some("draw_bg +: {"), property);
        let band = |property: &str| template_token(source, None, property);
        let ink = template_token(source, None, "color_selected");
        let disabled_ink = template_token(source, None, "color_selected_disabled");
        vec![
            ("rest", well("color"), band("band_color"), ink.clone(), READABLE),
            ("hover", well("color_hover"), band("band_color_hover"), ink.clone(), READABLE),
            ("focus", well("color_focus"), band("band_color_focus"), ink.clone(), READABLE),
            ("drag", well("color_drag"), band("band_color_drag"), ink, READABLE),
            ("disabled", well("color_disabled"), band("band_color_disabled"), disabled_ink, DISABLED_WORDS),
        ]
    }

    /// How the ink of the row in the band reads in one state: the band laid
    /// over the well and the well over the page, which is the colour a
    /// person actually reads the digits against.
    fn band_reading(built: &BuiltTheme, well: &str, band: &str, ink: &str) -> f64 {
        let page = resolved(built, "color_bg_app");
        let ground = over(over(page, resolved(built, well)), resolved(built, band));
        reads_on(ground, resolved(built, ink))
    }

    /// The report that started this: a palette grown on a LIGHT page made
    /// the WheelPicker's band a dark green and left the digits in it dark
    /// too, and darker still under the pointer. The band borrowed the value
    /// fill, which the mapping pushes dark to stand off its track, and wrote
    /// the body ink on it, which nothing ever held to a fill because nothing
    /// is ever written on one.
    ///
    /// So: over every palette the sweeps build, in every state the band has
    /// -- at rest, under the pointer, focused, spinning and disabled -- the
    /// ink of the row in the band reads on the band at the bar for words,
    /// and a disabled one at the bar for disabled words.
    #[test]
    fn the_wheel_pickers_band_reads_on_every_built_palette() {
        let states = wheel_band_states();
        let (mut checked, mut failed) = (0, 0);
        let mut failures: Vec<String> = Vec::new();
        each_swept_palette(|label, built| {
            for (state, well, band, ink, need) in &states {
                let stands = band_reading(built, well, band, ink);
                if stands < *need {
                    failed += 1;
                    if failures.len() < 12 {
                        failures.push(format!("{state}: {ink} on {band} = {stands:.2} in {label}"));
                    }
                }
            }
            checked += 1;
        });
        assert!(checked > 10_000, "the sweep only built {checked} palettes");
        assert!(failed == 0, "{failed} of {} readings fail, the first of them {failures:#?}", checked * states.len());
    }

    /// The band changes the widget everywhere, not only under a built
    /// palette, so it has to read on the two themes the library ships as
    /// well, in every state: at the bar, or where the theme's own chosen
    /// row does not reach it -- the dark theme's menus write their chosen
    /// item at about three to one -- no worse than that. That second half
    /// is what keeps a hover from paling the band toward its digits, which
    /// the obvious louder rung did, to 2.3.
    #[test]
    fn the_wheel_pickers_band_reads_on_both_base_themes() {
        for dark in [true, false] {
            let built = build(&BuilderParams::house(dark));
            let states = wheel_band_states();
            let (_, well, band, ink, _) = &states[0];
            let selection = band_reading(&built, well, band, ink);
            for (state, well, band, ink, need) in &states {
                let stands = band_reading(&built, well, band, ink);
                let bar = need.min(selection);
                assert!(stands >= bar, "{}: {state}: {ink} on {band} = {stands:.2}, wants {bar:.2}", built.scheme.theme_name());
            }
        }
    }

    /// The band is a selection, so it wears what a selected row wears: the
    /// ground a menu, a list and a file tree draw their chosen row on, and
    /// the ink they write it in. Held here and not left to the band tests,
    /// because a band that read well in some other pair of tokens would pass
    /// those and still not be the theme's selection.
    #[test]
    fn the_wheel_pickers_band_is_drawn_in_the_selected_row_tokens() {
        let selected = ACCENTED
            .iter()
            .find(|row| matches!(row.reaches, Reaches::Ground { ink: "color_label_inner_active", .. }))
            .expect("the mapping has a selected-row ground");
        for (state, _, band, ink, _) in wheel_band_states() {
            if state == "disabled" {
                assert_eq!(band, "color_outset_disabled");
                assert_eq!(ink, "color_label_inner");
                continue;
            }
            assert!(selected.tokens.contains(&band.as_str()), "{state}: {band} is not a selected-row ground");
            assert_eq!(ink, "color_label_inner_active", "{state}");
        }
        // And the time picker's plate, which marks its chosen value the same way.
        let source = include_str!("time_picker.rs");
        let plate = template_token(source, Some("draw_row +: {"), "color_active");
        let chosen = template_token(source, Some("draw_text_active +: {"), "color");
        assert!(selected.tokens.contains(&plate.as_str()), "{plate}");
        assert_eq!(chosen, "color_label_inner_active");
    }

    /// Every pair [`WRITTEN`] names is measured by a built palette's reading,
    /// and every accented ground a widget that draws words reads is either
    /// in [`WRITTEN`] under that widget or in [`UNWRITTEN`] with a reason.
    /// A widget that starts writing on a coloured ground fails here until
    /// the pair is listed -- and, being listed, measured and given way to.
    #[test]
    fn written_words_are_all_measured() {
        // Which tokens are grounds: every token a row makes a ground, a lean
        // or a veil of, and the value fills, which are drawn as inks on a
        // track but are grounds to anything written on them.
        let grounds: Vec<&str> = ACCENTED
            .iter()
            .filter(|row| {
                matches!(row.reaches, Reaches::Ground { .. } | Reaches::Lean { .. } | Reaches::Veil)
                    || row.tokens.iter().any(|key| key.starts_with("color_val"))
            })
            .flat_map(|row| row.tokens.iter().copied())
            .collect();
        let reads = |text: &str, key: &str| {
            text.match_indices(&format!("theme.{key}")).any(|(at, found)| {
                !text[at + found.len()..].chars().next().is_some_and(is_key_char)
            })
        };
        // Every widget file that draws words and reads an accented ground,
        // found on disk, so a new one cannot be missed by a list.
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let mut files: Vec<(String, String)> = Vec::new();
        for entry in std::fs::read_dir(&dir).expect("the widget sources") {
            let path = entry.expect("an entry").path();
            let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("").to_string();
            if !name.ends_with(".rs") || name.starts_with("theme_") {
                continue;
            }
            let text = std::fs::read_to_string(&path).unwrap_or_default();
            let draws_words = text.contains("draw_text") || text.contains("text_style") || text.contains("DrawText");
            if draws_words && grounds.iter().any(|key| reads(&text, key)) {
                files.push((name, text));
            }
        }
        assert!(files.len() > 20, "only {} widget files found", files.len());
        for (file, text) in &files {
            for key in &grounds {
                if !reads(text, key) {
                    continue;
                }
                let written = WRITTEN.iter().any(|w| w.widgets.contains(&file.as_str()) && w.grounds.contains(key));
                let unwritten = UNWRITTEN.iter().any(|(f, keys, _)| f == file && keys.contains(key));
                assert!(written || unwritten, "{file} reads {key}: say what it writes on it, or why it writes nothing");
                assert!(!(written && unwritten), "{file}: {key} is both written on and not");
            }
        }
        // Every name is a token both base themes declare.
        for scheme in [Scheme::Dark, Scheme::Light] {
            let keys = crate::theme_tokens::theme_keys(scheme.source());
            for written in WRITTEN {
                for key in written.grounds.iter().chain(std::iter::once(&written.ink)) {
                    assert!(keys.contains(key), "{key} is not a key of {}", scheme.theme_name());
                }
            }
        }
        // And every pair is one a built palette's reading measures, on both
        // pages.
        for dark in [true, false] {
            let params = BuilderParams { favourite: 0x2E8B57FF, ..BuilderParams::house(dark) };
            let built = build(&params);
            for written in WRITTEN {
                for key in written.grounds {
                    let measured = built.accent_pairs.iter().any(|(g, i, _)| g == key && i == written.ink);
                    assert!(measured, "{} on {key} is not measured on a {} page", written.ink, built.scheme.theme_name());
                }
            }
        }
    }

}
