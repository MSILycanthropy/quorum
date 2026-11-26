//! Nerd Font glyphs for the rows of the file list.

use std::path::Path;

use crate::color::Rgb;

/// A glyph and the colour it is drawn in.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Icon {
    pub glyph: char,
    pub color: Rgb,
}

impl Icon {
    pub const BRANCH: char = '\u{e725}';
    const FILE: char = '\u{f016}';
    const FOLDER: char = '\u{f07b}';
    const FOLDER_OPEN: char = '\u{f07c}';
    const FOLDER_COLOR: Rgb = Rgb::new(0x7e, 0x8e, 0xa8);

    pub fn file(path: &Path) -> Self {
        let found = devicons::icon_for_file(path, &None);
        let glyph =
            if found.icon == devicons::FileIcon::default().icon { Self::FILE } else { found.icon };

        Self { glyph, color: hex(found.color) }
    }

    pub fn folder(open: bool) -> Self {
        Self {
            glyph: if open { Self::FOLDER_OPEN } else { Self::FOLDER },
            color: Self::FOLDER_COLOR,
        }
    }
}

fn hex(color: &str) -> Rgb {
    let channel = |at: usize| {
        color
            .get(at..at + 2)
            .and_then(|pair| u8::from_str_radix(pair, 16).ok())
            .expect("devicons colours are #rrggbb")
    };

    Rgb::new(channel(1), channel(3), channel(5))
}
