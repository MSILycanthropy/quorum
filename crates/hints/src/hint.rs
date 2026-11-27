//! One key and what it does, as a footer words it.

use keys::{Binding, Continuation};

/// One key and what it does, as a footer words it. A pinned hint is the last
/// to leave a footer too narrow for all of them.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct Hint {
    pub keys: String,
    pub label: String,
    pub pinned: bool,
}

impl Hint {
    pub fn new(keys: impl Into<String>, label: impl Into<String>) -> Self {
        Self { keys: keys.into(), label: label.into(), pinned: false }
    }

    #[must_use]
    pub fn pinned(self) -> Self {
        Self { pinned: true, ..self }
    }

    pub fn of<C>(binding: &Binding<C>, holding: &[&str]) -> Option<Self> {
        let label = binding.hint?;
        if binding.when.is_some_and(|condition| !holding.contains(&condition)) {
            return None;
        }
        let keys = binding.keys.first()?;

        Some(Self { keys: keys.to_string(), label: label.to_owned(), pinned: binding.pinned })
    }
}

impl From<Continuation> for Hint {
    fn from(continuation: Continuation) -> Self {
        Self::new(continuation.keys.to_string(), continuation.label)
    }
}
