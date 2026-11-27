//! The colours Sift draws with: tints for rows and chrome, and a syntax theme for code.

use hue::{Ansi, Palette, Rgb, Slot};
use ratatui::style::{Color, Modifier, Style};

use crate::syntax::Syntax;

const BACKGROUND: Rgb = Rgb::new(0x1a, 0x1b, 0x26);
const FOREGROUND: Rgb = Rgb::new(0xc0, 0xca, 0xf5);
const GUTTER: Rgb = Rgb::new(0x3b, 0x42, 0x61);
const FILLER: Rgb = Rgb::new(0x16, 0x16, 0x1e);

const COMMENT: Rgb = Rgb::new(0x56, 0x5f, 0x89);
const RED: Rgb = Rgb::new(0xf7, 0x76, 0x8e);
const ORANGE: Rgb = Rgb::new(0xff, 0x9e, 0x64);
const GREEN: Rgb = Rgb::new(0x9e, 0xce, 0x6a);

const ADDED: Rgb = Rgb::new(0x20, 0x30, 0x3b);
const REMOVED: Rgb = Rgb::new(0x37, 0x22, 0x2c);

const BLACK: Rgb = Rgb::new(0, 0, 0);

/// How strongly an emphasis tint leans toward its accent, out of 255.
const EMPHASIS_MIX: u8 = 0x40;

/// How strongly a tint under the cursor leans toward the cursor colour, out of 255.
const CURSOR_MIX: u8 = 0x1c;

/// How strongly a derived line tint leans from the background toward its accent, out of 255.
const LINE_MIX: u8 = 0x26;

/// How far derived muted text and gutter lean from the background toward the
/// foreground, out of 255. A light background needs more to stay legible.
const MUTED_MIX: u8 = 0x66;
const LIGHT_MUTED_MIX: u8 = 0xa0;
const GUTTER_MIX: u8 = 0x33;

/// How far a derived filler darkens the background, out of 255, and how far it
/// lightens one already too dark to darken.
const FILLER_SHADE: u8 = 0x30;
const LIGHT_FILLER_SHADE: u8 = 0x14;
const FILLER_LIFT: u8 = 0x10;

/// The backgrounds Sift paints diff rows with, under the syntax foreground.
/// Under the cursor, each leans toward `cursor`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Tints {
    pub added: Rgb,
    pub removed: Rgb,
    pub added_emphasis: Rgb,
    pub removed_emphasis: Rgb,
    pub background: Rgb,
    pub foreground: Rgb,
    pub gutter: Rgb,
    pub filler: Rgb,
    pub muted: Rgb,
    pub attention: Rgb,
    pub added_text: Rgb,
    pub removed_text: Rgb,
    pub cursor: Rgb,
}

impl Tints {
    pub fn footer(&self) -> hints::Styles {
        let base = Style::new().bg(Color::from(self.filler));

        hints::Styles {
            base,
            muted: base.fg(Color::from(self.muted)),
            key: base.fg(Color::from(self.attention)),
        }
    }

    pub fn picker(&self) -> picker::Styles {
        let base = Style::new().bg(Color::from(self.filler)).fg(Color::from(self.foreground));

        picker::Styles {
            base,
            muted: base.fg(Color::from(self.muted)),
            key: base.fg(Color::from(self.attention)),
            selected: base.bg(Color::from(self.gutter)),
        }
    }

    pub fn page(&self) -> settings::Styles {
        let base = Style::new().bg(Color::from(self.background)).fg(Color::from(self.foreground));

        settings::Styles {
            base,
            muted: base.fg(Color::from(self.muted)),
            key: base.fg(Color::from(self.attention)),
            heading: base.fg(Color::from(self.muted)).add_modifier(Modifier::BOLD),
            selected: base.bg(Color::from(self.under_cursor(self.background))),
            warning: base.fg(Color::from(self.removed_text)),
        }
    }

    pub const fn under_cursor(&self, tint: Rgb) -> Rgb {
        tint.blend(self.cursor, CURSOR_MIX)
    }
}

impl Default for Tints {
    fn default() -> Self {
        TokyoNight::TINTS
    }
}

/// Tokyo Night, Night variant.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct TokyoNight;

impl TokyoNight {
    pub const TINTS: Tints = Tints {
        added: ADDED,
        removed: REMOVED,
        added_emphasis: ADDED.blend(GREEN, EMPHASIS_MIX),
        removed_emphasis: REMOVED.blend(RED, EMPHASIS_MIX),
        background: BACKGROUND,
        foreground: FOREGROUND,
        gutter: GUTTER,
        filler: FILLER,
        muted: COMMENT,
        attention: ORANGE,
        added_text: GREEN,
        removed_text: RED,
        cursor: FOREGROUND,
    };
}

/// The handful of colours a terminal reports of its own theme, enough to
/// derive every tint from.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct TerminalColors {
    pub background: Rgb,
    pub foreground: Rgb,
    pub red: Rgb,
    pub green: Rgb,
    pub yellow: Rgb,
}

impl TerminalColors {
    pub const SLOTS: &[Slot] = &[
        Slot::Background,
        Slot::Foreground,
        Slot::Ansi(Ansi::Red),
        Slot::Ansi(Ansi::Green),
        Slot::Ansi(Ansi::Yellow),
    ];

    pub const fn from_palette(palette: &Palette) -> Option<Self> {
        let (Some(background), Some(foreground), Some(red), Some(green), Some(yellow)) = (
            palette.background(),
            palette.foreground(),
            palette.ansi(Ansi::Red),
            palette.ansi(Ansi::Green),
            palette.ansi(Ansi::Yellow),
        ) else {
            return None;
        };

        Some(Self { background, foreground, red, green, yellow })
    }

    pub const fn tints(&self) -> Tints {
        let background = self.background;
        let added = background.blend(self.green, LINE_MIX);
        let removed = background.blend(self.red, LINE_MIX);
        let light = background.is_light();
        let black = background.red < 0x10 && background.green < 0x10 && background.blue < 0x10;
        let filler = if black {
            background.blend(self.foreground, FILLER_LIFT)
        } else if light {
            background.blend(BLACK, LIGHT_FILLER_SHADE)
        } else {
            background.blend(BLACK, FILLER_SHADE)
        };
        let muted = if light { LIGHT_MUTED_MIX } else { MUTED_MIX };

        Tints {
            added,
            removed,
            added_emphasis: added.blend(self.green, EMPHASIS_MIX),
            removed_emphasis: removed.blend(self.red, EMPHASIS_MIX),
            background,
            foreground: self.foreground,
            gutter: background.blend(self.foreground, GUTTER_MIX),
            filler,
            muted: background.blend(self.foreground, muted),
            attention: self.yellow,
            added_text: self.green,
            removed_text: self.red,
            cursor: self.foreground,
        }
    }

    #[expect(
        clippy::missing_panics_doc,
        reason = "only the bundled names are looked up, and every bundled theme parses"
    )]
    pub fn theme(&self) -> Theme {
        let syntax = if self.background.is_light() { LIGHT_SYNTAX } else { DARK_SYNTAX };

        Theme {
            tints: self.tints(),
            syntax: Syntax::named(syntax).expect("the bundled default syntax themes parse"),
        }
    }
}

const DARK_SYNTAX: &str = "tokyonight";
const LIGHT_SYNTAX: &str = "github_light";

/// Everything Sift colours with.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct Theme {
    pub tints: Tints,
    pub syntax: Syntax,
}

impl Theme {
    pub fn from_palette(palette: &Palette) -> Self {
        TerminalColors::from_palette(palette).map_or_else(Self::default, |colors| colors.theme())
    }
}

/// Where the tints come from: the terminal's own colours, or a bundled theme.
#[derive(
    Clone,
    Copy,
    Debug,
    Default,
    PartialEq,
    Eq,
    Hash,
    serde::Deserialize,
    serde::Serialize,
    clap::ValueEnum,
)]
#[serde(rename_all = "kebab-case")]
pub enum Scheme {
    #[default]
    Terminal,
    TokyoNight,
}
