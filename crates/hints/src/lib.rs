//! The footer along the bottom of a terminal screen: what the screen shows on
//! the left, and the keys worth knowing on the right. The keys come from a key
//! map, so they are always the ones that work, or are given by hand.

mod footer;
mod hint;

pub use footer::Footer;
pub use hint::Hint;

use ratatui::style::Style;

/// The styles the footer is drawn in, so it takes the colours of the program
/// that shows it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Styles {
    pub base: Style,
    pub muted: Style,
    pub key: Style,
}

/// Hints written out by hand: `"Enter" => "change"` for a key and what it
/// does, `pinned` after one to keep it longest, and words alone for a hint
/// with no key.
///
/// ```
/// use hints::hints;
///
/// let hints = hints![
///     "type the value",
///     "Enter" => "keep it",
///     "Esc" => "leave it" pinned,
/// ];
///
/// assert_eq!(hints[2], hints::Hint::new("Esc", "leave it").pinned());
/// ```
#[macro_export]
macro_rules! hints {
    (@ [$($done:expr,)*]) => {
        vec![$($done),*]
    };
    (@ [$($done:expr,)*] $keys:literal => $label:literal pinned $(, $($rest:tt)*)?) => {
        $crate::hints!(@ [$($done,)* $crate::Hint::new($keys, $label).pinned(),] $($($rest)*)?)
    };
    (@ [$($done:expr,)*] $keys:literal => $label:literal $(, $($rest:tt)*)?) => {
        $crate::hints!(@ [$($done,)* $crate::Hint::new($keys, $label),] $($($rest)*)?)
    };
    (@ [$($done:expr,)*] $label:literal $(, $($rest:tt)*)?) => {
        $crate::hints!(@ [$($done,)* $crate::Hint::new("", $label),] $($($rest)*)?)
    };
    ($($rest:tt)*) => {
        $crate::hints!(@ [] $($rest)*)
    };
}
