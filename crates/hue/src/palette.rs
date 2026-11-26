//! The colours of a terminal's theme, as far as it answered.

use crate::rgb::Rgb;

const ESCAPE: u8 = 0x1b;
const BELL: u8 = 0x07;

/// One of the sixteen colours a terminal theme names, in the order of their
/// palette indices.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Ansi {
    Black,
    Red,
    Green,
    Yellow,
    Blue,
    Magenta,
    Cyan,
    White,
    BrightBlack,
    BrightRed,
    BrightGreen,
    BrightYellow,
    BrightBlue,
    BrightMagenta,
    BrightCyan,
    BrightWhite,
}

/// A colour the terminal can be asked for.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Slot {
    Foreground,
    Background,
    Cursor,
    Ansi(Ansi),
}

/// The colours of a terminal's theme. Each is `None` when it was not asked
/// for, or the terminal left it unanswered.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Palette {
    foreground: Option<Rgb>,
    background: Option<Rgb>,
    cursor: Option<Rgb>,
    ansi: [Option<Rgb>; 16],
}

impl Palette {
    pub const fn get(&self, slot: Slot) -> Option<Rgb> {
        match slot {
            Slot::Foreground => self.foreground,
            Slot::Background => self.background,
            Slot::Cursor => self.cursor,
            Slot::Ansi(color) => self.ansi(color),
        }
    }

    pub const fn foreground(&self) -> Option<Rgb> {
        self.foreground
    }

    pub const fn background(&self) -> Option<Rgb> {
        self.background
    }

    pub const fn cursor(&self) -> Option<Rgb> {
        self.cursor
    }

    pub const fn ansi(&self, color: Ansi) -> Option<Rgb> {
        self.ansi[color as usize]
    }

    pub fn from_replies(replies: &[u8]) -> Self {
        let mut palette = Self::default();

        for reply in operating_system_commands(replies) {
            let Some((slot, color)) =
                std::str::from_utf8(reply).ok().and_then(|reply| reply.rsplit_once(';'))
            else {
                continue;
            };
            let color = parse(color);
            match slot {
                "10" => palette.foreground = color,
                "11" => palette.background = color,
                "12" => palette.cursor = color,
                slot => {
                    if let Some(entry) = slot
                        .strip_prefix("4;")
                        .and_then(|index| index.parse::<usize>().ok())
                        .and_then(|index| palette.ansi.get_mut(index))
                    {
                        *entry = color;
                    }
                }
            }
        }

        palette
    }
}

fn operating_system_commands(replies: &[u8]) -> impl Iterator<Item = &[u8]> {
    let mut rest = replies;
    std::iter::from_fn(move || {
        let start = rest.windows(2).position(|window| window == [ESCAPE, b']'])? + 2;
        let body = &rest[start..];
        let end = body.iter().position(|byte| *byte == BELL || *byte == ESCAPE)?;
        rest = &body[end..];

        Some(&body[..end])
    })
}

fn parse(color: &str) -> Option<Rgb> {
    let mut channels = color.strip_prefix("rgb:")?.split('/').map(intensity);
    let color = Rgb::new(channels.next()??, channels.next()??, channels.next()??);

    channels.next().is_none().then_some(color)
}

fn intensity(hex: &str) -> Option<u8> {
    let digits = u32::try_from(hex.len()).ok().filter(|digits| (1..=4).contains(digits))?;
    let value = u32::from_str_radix(hex, 16).ok()?;
    let most = 16u32.pow(digits) - 1;

    u8::try_from((value * 255 + most / 2) / most).ok()
}
