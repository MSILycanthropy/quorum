//! One row to pick from.

use keys::Binding;

/// One row to pick: what choosing it hands back, what it says, and a few
/// words at the right-hand edge, such as the keys that do the same thing.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct Entry<T> {
    pub target: T,
    pub label: String,
    pub detail: String,
}

impl<T> Entry<T> {
    pub fn new(target: T, label: impl Into<String>) -> Self {
        Self { target, label: label.into(), detail: String::new() }
    }

    #[must_use]
    pub fn detail(self, detail: impl Into<String>) -> Self {
        Self { detail: detail.into(), ..self }
    }

    pub fn map<U>(self, wrap: impl FnOnce(T) -> U) -> Entry<U> {
        Entry { target: wrap(self.target), label: self.label, detail: self.detail }
    }
}

impl<C: Clone> Entry<C> {
    pub fn of(binding: &Binding<C>, holding: &[&str]) -> Option<Self> {
        if binding.when.is_some_and(|condition| !holding.contains(&condition)) {
            return None;
        }

        Some(Self::new(binding.command.clone(), binding.label).detail(binding.shown()))
    }
}
