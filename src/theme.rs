//! Tokyo Night, as a syntect theme for source and tints for diff rows.

use two_face::re_exports::syntect::highlighting::{
    Color, FontStyle, StyleModifier, Theme, ThemeItem, ThemeSettings,
};

use crate::color::Rgb;

const BACKGROUND: Rgb = Rgb::new(0x1a, 0x1b, 0x26);
const FOREGROUND: Rgb = Rgb::new(0xc0, 0xca, 0xf5);
const SELECTION: Rgb = Rgb::new(0x28, 0x34, 0x57);
const GUTTER: Rgb = Rgb::new(0x3b, 0x42, 0x61);
const FILLER: Rgb = Rgb::new(0x16, 0x16, 0x1e);

const COMMENT: Rgb = Rgb::new(0x56, 0x5f, 0x89);
const RED: Rgb = Rgb::new(0xf7, 0x76, 0x8e);
const INVALID: Rgb = Rgb::new(0xdb, 0x4b, 0x4b);
const ORANGE: Rgb = Rgb::new(0xff, 0x9e, 0x64);
const YELLOW: Rgb = Rgb::new(0xe0, 0xaf, 0x68);
const GREEN: Rgb = Rgb::new(0x9e, 0xce, 0x6a);
const TEAL: Rgb = Rgb::new(0x2a, 0xc3, 0xde);
const CYAN: Rgb = Rgb::new(0x7d, 0xcf, 0xff);
const PUNCTUATION: Rgb = Rgb::new(0x89, 0xdd, 0xff);
const BLUE: Rgb = Rgb::new(0x7a, 0xa2, 0xf7);
const MAGENTA: Rgb = Rgb::new(0xbb, 0x9a, 0xf7);

const ADDED: Rgb = Rgb::new(0x20, 0x30, 0x3b);
const REMOVED: Rgb = Rgb::new(0x37, 0x22, 0x2c);

/// How strongly an emphasis tint leans toward its accent, out of 255.
const EMPHASIS_MIX: u8 = 0x40;

/// The backgrounds Quorum paints diff rows with, under the syntax foreground.
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
    };

    #[expect(
        clippy::missing_panics_doc,
        reason = "selectors are constants in this module, so a failure is a typo, not input"
    )]
    pub fn theme() -> Theme {
        Theme {
            name: Some("Tokyo Night".to_owned()),
            settings: ThemeSettings {
                foreground: Some(color(FOREGROUND)),
                background: Some(color(BACKGROUND)),
                caret: Some(color(FOREGROUND)),
                selection: Some(color(SELECTION)),
                ..ThemeSettings::default()
            },
            scopes: SCOPES
                .iter()
                .map(|(selector, style)| ThemeItem {
                    scope: selector.parse().expect("theme scope selectors are valid"),
                    style: *style,
                })
                .collect(),
            ..Theme::default()
        }
    }
}

const SCOPES: &[(&str, StyleModifier)] = &[
    ("comment", italic(COMMENT)),
    ("punctuation.definition.comment", italic(COMMENT)),
    ("string", foreground(GREEN)),
    ("punctuation.definition.string", foreground(GREEN)),
    ("constant.character.escape", foreground(MAGENTA)),
    ("constant.numeric", foreground(ORANGE)),
    ("constant.language", foreground(ORANGE)),
    ("constant.other", foreground(ORANGE)),
    ("keyword", foreground(MAGENTA)),
    ("keyword.operator", foreground(PUNCTUATION)),
    ("storage", foreground(MAGENTA)),
    ("storage.type", foreground(MAGENTA)),
    ("storage.modifier", foreground(MAGENTA)),
    ("entity.name", foreground(TEAL)),
    ("entity.name.function", foreground(BLUE)),
    ("entity.name.macro", foreground(BLUE)),
    ("support.function", foreground(BLUE)),
    ("meta.function-call", foreground(BLUE)),
    ("variable.function", foreground(BLUE)),
    ("entity.name.type", foreground(TEAL)),
    ("entity.name.class", foreground(TEAL)),
    ("entity.name.struct", foreground(TEAL)),
    ("entity.name.enum", foreground(TEAL)),
    ("entity.name.namespace", foreground(TEAL)),
    ("support.type", foreground(TEAL)),
    ("support.class", foreground(TEAL)),
    ("support.constant", foreground(ORANGE)),
    ("entity.name.tag", foreground(RED)),
    ("entity.other.attribute-name", foreground(BLUE)),
    ("variable", foreground(FOREGROUND)),
    ("variable.parameter", foreground(YELLOW)),
    ("variable.other.member", foreground(BLUE)),
    ("punctuation", foreground(PUNCTUATION)),
    ("meta.preprocessor", foreground(CYAN)),
    ("keyword.other.preprocessor", foreground(CYAN)),
    ("invalid", foreground(INVALID)),
];

const fn color(Rgb { red, green, blue }: Rgb) -> Color {
    Color { r: red, g: green, b: blue, a: 0xff }
}

const fn foreground(rgb: Rgb) -> StyleModifier {
    StyleModifier { foreground: Some(color(rgb)), background: None, font_style: None }
}

const fn italic(rgb: Rgb) -> StyleModifier {
    StyleModifier {
        foreground: Some(color(rgb)),
        background: None,
        font_style: Some(FontStyle::ITALIC),
    }
}
