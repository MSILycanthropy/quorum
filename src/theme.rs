//! Tokyo Night's tints for diff rows.

use crate::color::Rgb;

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
    pub added_text: Rgb,
    pub removed_text: Rgb,
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
    };
}
