//! An opaque 8-bit-per-channel colour.

use std::fmt;

/// A colour, in the only form every frontend agrees on.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Rgb {
    pub red: u8,
    pub green: u8,
    pub blue: u8,
}

impl Rgb {
    pub const fn new(red: u8, green: u8, blue: u8) -> Self {
        Self { red, green, blue }
    }

    pub const fn is_light(self) -> bool {
        let (red, green, blue) = (self.red as u32, self.green as u32, self.blue as u32);
        299 * red + 587 * green + 114 * blue > 128 * 1000
    }

    #[must_use]
    pub const fn blend(self, other: Self, amount: u8) -> Self {
        Self::new(
            mix(self.red, other.red, amount),
            mix(self.green, other.green, amount),
            mix(self.blue, other.blue, amount),
        )
    }
}

impl fmt::Display for Rgb {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "#{:02x}{:02x}{:02x}", self.red, self.green, self.blue)
    }
}

#[expect(clippy::cast_possible_truncation, reason = "a weighted average of two u8 is a u8")]
const fn mix(base: u8, accent: u8, amount: u8) -> u8 {
    let (base, accent, amount) = (base as u16, accent as u16, amount as u16);
    ((base * (255 - amount) + accent * amount) / 255) as u8
}
