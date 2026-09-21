//! The ten families a theme's tokens are mixed in, and which family every
//! token answers to.
//!
//! The equalizer mixes themes with one weight per theme: so much of this one,
//! so much of that. The matrix is the same mix with the weight split ten
//! ways, so that a mix can take its grounds from one theme, its corners from
//! another and its spacing from a third. A weight per TOKEN would be the
//! honest version of that and nobody could drive it -- a theme carries over
//! four hundred tokens that blend -- so the tokens are gathered into families
//! that a person would actually reach for as one thing, and a family is what
//! gets a knob. [`MixGroup`] is the list of them, and [`group_of`] is the
//! whole of the membership rule.
//!
//! "Group" is already a word in this library: [`crate::theme_tokens`] calls
//! the dark themes and the light themes the two appearance groups, and a mix
//! never crosses them. That is a group of THEMES. This is a group of TOKENS,
//! and the two are at right angles: the appearance group says which rows the
//! matrix has, and these say which columns.
//!
//! # Where the table comes from
//!
//! From the style sheets, and not from the registry. A sheet is a flat list of
//! `mod.theme.<token> = ...` lines, and what a sheet assigns is exactly what
//! differs between one shipped theme and the next -- so the tokens the sheets
//! name are the tokens a knob can be heard through, and they are the older
//! vocabulary: `color_outset_1_hover`, `color_bevel_inset_2_focus`,
//! `color_label_inner_down`. They fall into families by their names, a state
//! ladder under each stem, which is why most of the table is a prefix and not
//! a list. The registry ([`crate::theme_tokens::THEME_TOKENS`]) is younger
//! than the sheets and shares only a handful of names with them, so it could
//! never have been the table; it is laid on top instead, see below.
//!
//! The table is a second copy of what the sheets say, and a second copy is
//! only safe while something fails when the first one moves. So the tests at
//! the bottom walk every sheet the library ships and refuse a token that has
//! no family: a token with no family has no knob, and a sheet that grew one
//! would have it mixed by no column at all with nothing on screen to say so.
//! That one test is the whole correctness argument for the matrix. The rest
//! hold the table to itself -- no row that names a token nothing carries, no
//! prefix that catches nothing, and the size of each family pinned so that
//! drift is a number in a diff.
//!
//! # What a family does not promise
//!
//! That its tokens can be mixed. A family says which column a token answers
//! to. `font_regular` is a text style and `mspace_1` is an inset, and neither
//! has a midpoint, so a blend carries neither and both come from the heaviest
//! theme whatever the Text and Spacing columns say. They are in the table all
//! the same, because the table is held to the sheets and the sheets assign
//! them; leaving them out would mean the gate has an exception list, and an
//! exception list is where the next ungrouped token would go to hide.
//!
//! # The registry, laid on top
//!
//! Where a registry group is one of the ten under another name its tokens
//! join that family, so that a role moves with the older token it was derived
//! from: `Space` and `Size` are Spacing (a control's height is a multiple of
//! the space unit, so how big and how far apart are one decision), `Radius`
//! is Shape, `Type` is Text, `ColorAccent` is Accent, and `ColorSurface` is
//! Backgrounds -- the inks of that group included, because `color_on_surface`
//! was chosen against `color_surface` and a pair chosen against each other is
//! only safe while it moves as one. The terminal's pair is kept together in
//! Backgrounds for the same reason.
//!
//! Five registry groups stay out, and their tokens are mixed by each theme's
//! mean weight across the ten columns, which is to say by the mix as a whole:
//!
//! * `Elevation`, `Motion` and `State` -- a shadow, a duration and an opacity
//!   are none of a ground, an ink, a fill, an edge, a gap or a corner, and no
//!   sheet assigns one. A knob nobody can hear is worse than no knob.
//! * `ColorStatus` -- a presence dot is green under every theme, so there is
//!   nothing between two themes to choose. Its two icon colours are caught by
//!   the Icons prefix first, which is where an icon's colour belongs.
//! * `Global` -- not a family but the inputs the families are derived from,
//!   so its tokens are placed one at a time by what they are: `space_factor`
//!   is Spacing, `corner_radius` and `beveling` are Shape, the two font knobs
//!   are Text. The three colour knobs -- contrast, tint, tint amount -- stay
//!   out: a resolved theme has already spent them on its ladder, and the
//!   number left behind moves nothing.
//!
//! The categorical palettes (`color_map_*`, `color_syntax_*`) have no family
//! and must not be given one. They are never mixed at all -- see
//! [`crate::theme_tokens::is_categorical`] -- so [`group_of`] answers `None`
//! for them before it looks at anything else.

use crate::theme_tokens::{is_categorical, token_spec, TokenGroup};

/// One family of tokens: a column of the matrix, and a knob per theme.
///
/// The order here is the order of [`MixGroup::ALL`], which is the order a
/// panel draws its columns in: what a page is made of first (grounds, then
/// the three inks, then the accent), then the control faces, then the two
/// families that are numbers and not colours.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum MixGroup {
    /// The grounds: the page, the panels on it, the highlight behind a
    /// selection, the surface roles and their ladder.
    Backgrounds,
    /// Body text in every state, and the face and size it is set in.
    Text,
    /// The text ON a control, inner and outer, in every state.
    Labels,
    /// Icon colours in every state.
    Icons,
    /// The accent, the focus colour, the selected control, the cursor, and
    /// the role families grown from them.
    Accent,
    /// The fill of a raised control -- a button, a tab -- in every state.
    Outset,
    /// The fill of a sunken control -- a field, a track -- in every state.
    Inset,
    /// The light and shadow edges of both, in every state.
    Bevels,
    /// The space unit, its rungs, and the control sizes that are multiples
    /// of it.
    Spacing,
    /// Corner radii and the width of the bevel.
    Shape,
}

impl MixGroup {
    /// How many families there are, for a fixed row of weights: see
    /// [`crate::theme_tokens::GroupWeights`].
    pub const COUNT: usize = 10;

    /// Every family, in column order. `ALL[i].index() == i`.
    pub const ALL: [MixGroup; MixGroup::COUNT] = [
        MixGroup::Backgrounds,
        MixGroup::Text,
        MixGroup::Labels,
        MixGroup::Icons,
        MixGroup::Accent,
        MixGroup::Outset,
        MixGroup::Inset,
        MixGroup::Bevels,
        MixGroup::Spacing,
        MixGroup::Shape,
    ];

    /// The column this family is, nought to `COUNT - 1`: the index into a
    /// row of weights, and into [`MixGroup::ALL`].
    pub fn index(self) -> usize {
        self as usize
    }

    /// The family of a column, and `None` past the last one. The inverse of
    /// [`MixGroup::index`], for a panel that counts its columns.
    pub fn from_index(index: usize) -> Option<MixGroup> {
        MixGroup::ALL.get(index).copied()
    }

    /// The column header: one whole word, short enough to sit over a knob.
    pub fn label(self) -> &'static str {
        match self {
            MixGroup::Backgrounds => "Backgrounds",
            MixGroup::Text => "Text",
            MixGroup::Labels => "Labels",
            MixGroup::Icons => "Icons",
            MixGroup::Accent => "Accent",
            MixGroup::Outset => "Outset",
            MixGroup::Inset => "Inset",
            MixGroup::Bevels => "Bevels",
            MixGroup::Spacing => "Spacing",
            MixGroup::Shape => "Shape",
        }
    }

    /// One plain sentence on what the column moves, for a tooltip: the header
    /// is a single word and two of them -- Outset, Inset -- are this
    /// library's own.
    pub fn describe(self) -> &'static str {
        match self {
            MixGroup::Backgrounds => "The page, the panels on it, the selection highlight and the surface roles.",
            MixGroup::Text => "Body text in every state, and the font and size it is set in.",
            MixGroup::Labels => "The text on a control, in every state.",
            MixGroup::Icons => "Icon colours, in every state.",
            MixGroup::Accent => "The accent, the focus colour, the selected control and the cursor.",
            MixGroup::Outset => "The fill of a raised control, such as a button or a tab, in every state.",
            MixGroup::Inset => "The fill of a sunken control, such as a field or a track, in every state.",
            MixGroup::Bevels => "The light and shadow edges of raised and sunken controls.",
            MixGroup::Spacing => "The space unit, the gaps made of it, and the control sizes that follow it.",
            MixGroup::Shape => "Corner radii and the width of the bevel.",
        }
    }

    /// The family a registry group is, where it is one of the ten under
    /// another name, and `None` for the five that stay out. The module doc
    /// says why each of those does.
    pub fn of_registry(group: TokenGroup) -> Option<MixGroup> {
        match group {
            TokenGroup::Space | TokenGroup::Size => Some(MixGroup::Spacing),
            TokenGroup::Radius => Some(MixGroup::Shape),
            TokenGroup::Type => Some(MixGroup::Text),
            TokenGroup::ColorAccent => Some(MixGroup::Accent),
            TokenGroup::ColorSurface => Some(MixGroup::Backgrounds),
            TokenGroup::Global
            | TokenGroup::Elevation
            | TokenGroup::Motion
            | TokenGroup::State
            | TokenGroup::ColorStatus => None,
        }
    }
}

/// The tokens placed by their whole name: the ones whose family cannot be
/// read off a stem, because the stem is theirs alone or because a prefix wide
/// enough to catch them would catch a stranger too.
///
/// Asked before [`PREFIXED`], so a name here always wins. Every row names a
/// token some theme really carries -- `no_row_of_the_table_is_dead` -- and no
/// row repeats what a prefix already says.
pub const NAMED: &[(&str, MixGroup)] = &[
    // The foreground of the app is a ground: it is the panel colour, the
    // other half of `color_bg_app`, and has never been an ink.
    ("color_fg_app", MixGroup::Backgrounds),
    ("color_focus", MixGroup::Accent),
    ("corner_radius", MixGroup::Shape),
    ("container_corner_radius", MixGroup::Shape),
    ("textselection_corner_radius", MixGroup::Shape),
    ("beveling", MixGroup::Shape),
];

/// The tokens placed by their stem: a family and the state ladder under it,
/// `color_outset`, `color_outset_hover`, `color_outset_1_down` and the rest.
///
/// The first stem a name starts with decides, so the order matters wherever
/// one stem could open another; none does today, and
/// `no_row_of_the_table_is_dead` would show a stem that had been shadowed
/// into catching nothing. A stem is written without its trailing underscore
/// where the bare word is itself a token (`color_text`, `color_bevel`), and
/// with it where the bare word would reach a stranger (`color_bg_`, so as not
/// to claim a token that merely starts with `color_bg`).
pub const PREFIXED: &[(&str, MixGroup)] = &[
    ("color_bevel", MixGroup::Bevels),
    ("color_outset", MixGroup::Outset),
    ("color_inset", MixGroup::Inset),
    ("color_label", MixGroup::Labels),
    ("color_icon", MixGroup::Icons),
    ("color_text", MixGroup::Text),
    ("font_", MixGroup::Text),
    ("color_bg_", MixGroup::Backgrounds),
    // The terminal's ink goes with the terminal's ground, not with Text: the
    // two were chosen against each other and nothing else is drawn on either.
    ("color_terminal_", MixGroup::Backgrounds),
    ("color_ctrl_", MixGroup::Accent),
    ("color_cursor", MixGroup::Accent),
    ("space_", MixGroup::Spacing),
    ("mspace_", MixGroup::Spacing),
];

/// The family a token is mixed in, and `None` for one that has no column.
///
/// Three questions, in order. Is it a categorical palette -- then it is never
/// mixed and has no family, whatever its name starts with. Is it in the table
/// -- by whole name first, then by stem. Is it a registered token whose
/// registry group is one of the ten -- see [`MixGroup::of_registry`].
///
/// A token that comes back `None` and is not categorical is still mixed: by
/// each theme's mean weight across its ten columns, which is the mix as a
/// whole. See [`crate::theme_tokens::BlendCache::blend_grouped`].
///
/// Cheap enough to ask once per token per blend: a dozen prefix tests, and a
/// walk of the registry only for a name the table did not place.
pub fn group_of(key: &str) -> Option<MixGroup> {
    if is_categorical(key) {
        return None;
    }
    if let Some((_, group)) = NAMED.iter().find(|(name, _)| *name == key) {
        return Some(*group);
    }
    if let Some((_, group)) = PREFIXED.iter().find(|(stem, _)| key.starts_with(stem)) {
        return Some(*group);
    }
    token_spec(key).and_then(|spec| MixGroup::of_registry(spec.group))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::desktop_style::StyleSheet;
    use crate::theme_tokens::{assigned_keys, base_theme_keys, BlendTheme, THEME_TOKENS};
    use std::collections::{BTreeMap, BTreeSet};

    /// Every sheet the library ships, as `(name, the text of its theme
    /// half)`. Drawn from [`BlendTheme::all`] and not from a list of its own,
    /// so a sheet added to the library is a sheet walked here.
    fn sheets() -> Vec<(String, String)> {
        BlendTheme::all()
            .into_iter()
            .filter_map(|theme| match theme {
                BlendTheme::Sheet(style, dark) => {
                    Some((theme.name(), StyleSheet::load_with_appearance(style, dark).theme))
                }
                BlendTheme::Base(_) => None,
            })
            .collect()
    }

    /// Every token any sheet assigns, once each.
    fn assigned_by_any_sheet() -> BTreeSet<String> {
        let mut out = BTreeSet::new();
        for (_, source) in sheets() {
            out.extend(assigned_keys(&source).into_iter().map(|key| key.to_string()));
        }
        out
    }

    /// Every token some theme carries: what the three theme files define and
    /// what the sheets add to them.
    fn every_token() -> BTreeSet<String> {
        let mut out = assigned_by_any_sheet();
        out.extend(base_theme_keys().into_iter().map(|key| key.to_string()));
        out
    }

    /// THE GATE. A token a sheet assigns and no family claims has no knob:
    /// the matrix would mix it by no column, and nothing on screen would say
    /// so. The sheets are the truth and the table is the copy, so this walks
    /// the sheets and names what the copy has lost.
    #[test]
    fn every_token_a_sheet_assigns_has_a_group() {
        let sheets = sheets();
        assert_eq!(sheets.len(), 12, "the library ships twelve sheets: {:?}", sheets.iter().map(|(n, _)| n).collect::<Vec<_>>());
        let mut ungrouped: BTreeMap<String, Vec<String>> = BTreeMap::new();
        let mut walked = 0;
        for (name, source) in &sheets {
            let keys = assigned_keys(source);
            assert!(keys.len() > 200, "{name} assigned only {} tokens, so it was not read", keys.len());
            for key in keys {
                if is_categorical(key) {
                    continue;
                }
                walked += 1;
                if group_of(key).is_none() {
                    ungrouped.entry(key.to_string()).or_default().push(name.clone());
                }
            }
        }
        assert!(walked > 12 * 100, "only {walked} tokens were walked, so the gate held nothing");
        assert!(
            ungrouped.is_empty(),
            "{} token(s) a sheet assigns have no group, so no knob of the matrix moves them:\n{}",
            ungrouped.len(),
            ungrouped
                .iter()
                .map(|(key, names)| format!("  {key}  (assigned by {})", names.join(", ")))
                .collect::<Vec<_>>()
                .join("\n")
        );
    }

    /// The other direction. A row naming a token that nothing carries is a
    /// row that was true once: the token was renamed and the table kept the
    /// old name, which no gate over the sheets would ever notice. A stem is
    /// held the same way -- it has to catch something -- and a whole name
    /// must not repeat what a stem already says, or the table has two rows
    /// to keep in step where it needs one.
    #[test]
    fn no_row_of_the_table_is_dead() {
        let tokens = every_token();
        for (name, group) in NAMED {
            assert!(tokens.contains(*name), "{name} ({group:?}) is carried by no theme and assigned by no sheet");
            let by_stem = PREFIXED.iter().find(|(stem, _)| name.starts_with(stem));
            assert!(by_stem.is_none(), "{name} is named, and the stem {by_stem:?} already places it");
        }
        for (at, (stem, group)) in PREFIXED.iter().enumerate() {
            // What this stem catches once the stems above it have had theirs,
            // which is what `group_of` does: a stem shadowed by an earlier
            // one catches nothing and fails here.
            let caught = tokens
                .iter()
                .filter(|key| !is_categorical(key))
                .filter(|key| PREFIXED.iter().position(|(other, _)| key.starts_with(other)) == Some(at))
                .count();
            assert!(caught > 0, "the stem {stem} ({group:?}) catches no token any theme carries");
        }
        let mut seen = BTreeSet::new();
        for (name, _) in NAMED.iter().chain(PREFIXED.iter()) {
            assert!(seen.insert(*name), "{name} is in the table twice");
        }
    }

    /// The size of every family, over the tokens the sheets assign, so that a
    /// sheet growing a token or a stem widening its reach is a number in a
    /// diff rather than a knob that quietly does more than it did.
    #[test]
    fn the_size_of_every_group_is_pinned() {
        let mut counts: BTreeMap<MixGroup, usize> = BTreeMap::new();
        let assigned = assigned_by_any_sheet();
        let mut blended = 0;
        for key in assigned.iter().filter(|key| !is_categorical(key)) {
            blended += 1;
            if let Some(group) = group_of(key) {
                *counts.entry(group).or_default() += 1;
            }
        }
        let got: Vec<(MixGroup, usize)> = MixGroup::ALL.iter().map(|g| (*g, counts.get(g).copied().unwrap_or(0))).collect();
        let want = vec![
            (MixGroup::Backgrounds, 7),
            (MixGroup::Text, 12),
            (MixGroup::Labels, 18),
            (MixGroup::Icons, 6),
            (MixGroup::Accent, 5),
            (MixGroup::Outset, 24),
            (MixGroup::Inset, 24),
            (MixGroup::Bevels, 32),
            (MixGroup::Spacing, 5),
            (MixGroup::Shape, 4),
        ];
        assert_eq!(got, want);
        assert_eq!(blended, 137, "the sheets assign {blended} tokens that are not palettes");
        assert_eq!(got.iter().map(|(_, n)| n).sum::<usize>(), blended, "a token the sheets assign is in no group");

        // And over the theme files, where most of a theme's tokens live and
        // no sheet reaches. What is left over is mixed by the mean.
        let mut counts: BTreeMap<Option<MixGroup>, usize> = BTreeMap::new();
        for key in base_theme_keys().into_iter().filter(|key| !is_categorical(key)) {
            *counts.entry(group_of(key)).or_default() += 1;
        }
        let got: Vec<usize> = MixGroup::ALL.iter().map(|g| counts.get(&Some(*g)).copied().unwrap_or(0)).collect();
        assert_eq!(got, vec![35, 51, 17, 6, 37, 22, 24, 38, 26, 11], "the theme files, group by group");
        assert_eq!(counts.get(&None).copied().unwrap_or(0), 157, "the theme files' tokens with no group");
    }

    /// The registry laid on top: a registered token whose registry group is
    /// one of the ten lands in that family, and the table never says
    /// otherwise -- a stem that pulled `color_surface` out of Backgrounds
    /// would split a ground from the ink chosen against it.
    #[test]
    fn a_registered_token_lands_where_its_registry_group_does() {
        let mut mapped = 0;
        for spec in THEME_TOKENS {
            if let Some(want) = MixGroup::of_registry(spec.group) {
                assert_eq!(group_of(spec.name), Some(want), "{} is {:?} in the registry", spec.name, spec.group);
                mapped += 1;
            }
        }
        assert_eq!(mapped, 100, "the registered tokens whose registry group is one of the ten");
        // The groups that stay out, by one token each -- and the two places
        // the table reaches into them on purpose.
        for out in ["motion_short_1", "state_hover_opacity", "elevation_1_radius", "color_presence_online", "color_contrast", "color_tint"] {
            assert_eq!(group_of(out), None, "{out}");
        }
        assert_eq!(group_of("space_factor"), Some(MixGroup::Spacing), "a global placed by what it is");
        assert_eq!(group_of("beveling"), Some(MixGroup::Shape));
        assert_eq!(group_of("font_size_base"), Some(MixGroup::Text));
        assert_eq!(group_of("color_icon_wait"), Some(MixGroup::Icons), "an icon colour is an icon colour first");
    }

    /// A palette has no family however its name starts, because it is never
    /// mixed at all; and a name nobody has heard of has none either.
    #[test]
    fn a_palette_and_a_stranger_have_no_group() {
        assert_eq!(group_of("color_map_1"), None);
        assert_eq!(group_of("color_map_12_h"), None);
        assert_eq!(group_of("color_syntax_string"), None);
        assert_eq!(group_of("no_such_token"), None);
        assert_eq!(group_of(""), None);
        // A stem with its underscore does not claim a neighbour without one.
        assert_eq!(group_of("color_bgx"), None);
        assert_eq!(group_of("spacecraft"), None);
    }

    /// One of each, by name, so that the families read as what their headers
    /// say and a reordered stem shows up as a wrong answer and not only as a
    /// changed count.
    #[test]
    fn each_group_holds_what_its_header_says() {
        for (key, want) in [
            ("color_bg_app", MixGroup::Backgrounds),
            ("color_fg_app", MixGroup::Backgrounds),
            ("color_bg_highlight_inline", MixGroup::Backgrounds),
            ("color_terminal_bg", MixGroup::Backgrounds),
            ("color_terminal_text", MixGroup::Backgrounds),
            ("color_surface_container_high", MixGroup::Backgrounds),
            ("color_on_surface", MixGroup::Backgrounds),
            ("color_text", MixGroup::Text),
            ("color_text_on_accent", MixGroup::Text),
            ("font_size_p", MixGroup::Text),
            ("font_regular", MixGroup::Text),
            ("type_body_m_size", MixGroup::Text),
            ("color_label", MixGroup::Labels),
            ("color_label_inner_hover", MixGroup::Labels),
            ("color_label_outer_disabled", MixGroup::Labels),
            ("color_icon", MixGroup::Icons),
            ("color_icon_down", MixGroup::Icons),
            ("color_focus", MixGroup::Accent),
            ("color_ctrl_selected", MixGroup::Accent),
            ("color_cursor", MixGroup::Accent),
            ("color_success", MixGroup::Accent),
            ("color_primary", MixGroup::Accent),
            ("color_outset", MixGroup::Outset),
            ("color_outset_2_empty", MixGroup::Outset),
            ("color_inset", MixGroup::Inset),
            ("color_inset_1_drag", MixGroup::Inset),
            ("color_bevel_inset_1", MixGroup::Bevels),
            ("color_bevel_outset_2_focus", MixGroup::Bevels),
            ("space_factor", MixGroup::Spacing),
            ("space_3", MixGroup::Spacing),
            ("mspace_1", MixGroup::Spacing),
            ("size_control_m", MixGroup::Spacing),
            ("corner_radius", MixGroup::Shape),
            ("container_corner_radius", MixGroup::Shape),
            ("textselection_corner_radius", MixGroup::Shape),
            ("beveling", MixGroup::Shape),
            ("radius_m", MixGroup::Shape),
        ] {
            assert_eq!(group_of(key), Some(want), "{key}");
        }
    }

    /// A panel draws a header per column and indexes a row of weights by
    /// column, so the order, the index and the header have to agree with one
    /// another; and a header has to fit over a knob.
    #[test]
    fn the_columns_are_in_order_and_their_headers_fit() {
        assert_eq!(MixGroup::ALL.len(), MixGroup::COUNT);
        let mut labels = BTreeSet::new();
        for (at, group) in MixGroup::ALL.iter().enumerate() {
            assert_eq!(group.index(), at, "{group:?}");
            assert_eq!(MixGroup::from_index(at), Some(*group));
            let label = group.label();
            assert!(labels.insert(label), "{label} heads two columns");
            assert!(!label.is_empty() && label.len() <= 11, "{label} will not fit over a knob");
            assert!(label.chars().all(|c| c.is_ascii_alphabetic()), "{label} is not one whole word");
            assert!(group.describe().ends_with('.'), "{group:?} is described by a fragment");
        }
        assert_eq!(MixGroup::from_index(MixGroup::COUNT), None);
    }
}
