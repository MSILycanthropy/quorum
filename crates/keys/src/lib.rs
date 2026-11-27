//! Key maps: commands, the key sequences that reach them, and what a key press amounts to.

mod key;
mod keymap;
mod rebinding;

pub use key::{Key, Sequence};
pub use keymap::{Binding, Continuation, Keymap, Pending, Pressed};
pub use rebinding::Rebindings;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("no key is called {0}")]
    Unnamed(String),
    #[error("{written} is not a key name: write {name}, or put spaces between the keys")]
    Spelt { written: String, name: &'static str },
    #[error("{0} can never be pressed: write the character Shift produces")]
    Shifted(String),
    #[error("a binding needs at least one key")]
    Empty,
    #[error("{0}: Esc cancels pending keys, so it can only come first")]
    Escape(String),
    #[error("{keys} is bound to both {first} and {second}")]
    Shared { keys: Sequence, first: &'static str, second: &'static str },
    #[error("{shorter} is {first}, so {longer} for {second} can never be reached")]
    Prefix { shorter: Sequence, longer: Sequence, first: &'static str, second: &'static str },
    #[error("two bindings share the name {0}")]
    Name(&'static str),
    #[error("{name}: {source}")]
    Binding { name: &'static str, source: Box<Error> },
    #[error("keys.{name}: no command has this name")]
    Unknown { name: String },
    #[error("keys.{name}: {source}")]
    Rebinding { name: String, source: Box<Error> },
}

pub type Result<T> = std::result::Result<T, Error>;

#[macro_export]
macro_rules! keymap {
    ($(
        $section:literal {
            $(
                $name:literal => $command:expr, $label:literal, [$($key:literal),* $(,)?]
                $(, $field:ident: $value:expr)* ;
            )*
        }
    )*) => {
        [$($(
            $crate::Binding::new($command, $name, $label, $section, &[$($key),*])
                .map(|binding| $crate::Binding { $($field: Some($value),)* ..binding }),
        )*)*]
        .into_iter()
        .collect::<$crate::Result<Vec<_>>>()
        .and_then($crate::Keymap::new)
    };
}
