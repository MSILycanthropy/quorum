//! Commands and the keys that reach them, and what each key press amounts to.

use std::fmt;

use ratatui::crossterm::event::KeyCode;

use crate::key::shown;
use crate::{Error, Key, Rebindings, Result, Sequence};

/// One command: its name in the settings file, what help calls it, and the
/// keys that reach it. With a condition, it counts only while the program says
/// that condition holds, and then wins over the bindings without one.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Binding<C> {
    pub command: C,
    pub name: &'static str,
    pub label: &'static str,
    pub section: &'static str,
    pub keys: Vec<Sequence>,
    pub hint: Option<&'static str>,
    pub when: Option<&'static str>,
}

impl<C> Binding<C> {
    pub fn new(
        command: C,
        name: &'static str,
        label: &'static str,
        section: &'static str,
        keys: &[&str],
    ) -> Result<Self> {
        let keys = Sequence::all(keys)
            .map_err(|source| Error::Binding { name, source: Box::new(source) })?;

        Ok(Self { command, name, label, section, keys, hint: None, when: None })
    }

    pub fn shown(&self) -> String {
        self.keys.iter().map(ToString::to_string).collect::<Vec<_>>().join(" ")
    }
}

/// Every binding of one screen, with no two in each other's way.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Keymap<C> {
    bindings: Vec<Binding<C>>,
}

impl<C> Default for Keymap<C> {
    fn default() -> Self {
        Self { bindings: Vec::new() }
    }
}

/// What a key press amounted to.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Pressed<C> {
    Command { command: C, count: Option<usize> },
    Pending,
    Nothing,
}

/// The count and keys pressed so far that have not reached a command yet.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct Pending {
    count: Option<usize>,
    keys: Vec<Key>,
}

impl Pending {
    pub fn is_empty(&self) -> bool {
        self.count.is_none() && self.keys.is_empty()
    }

    pub fn clear(&mut self) {
        self.count = None;
        self.keys.clear();
    }
}

impl fmt::Display for Pending {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(count) = self.count {
            write!(formatter, "{count}")?;
        }
        if !self.keys.is_empty() {
            formatter.write_str(&shown(&self.keys))?;
        }

        Ok(())
    }
}

/// One way the pending keys can go on: the keys still to press, and the
/// command they reach.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Continuation {
    pub keys: Sequence,
    pub label: &'static str,
}

/// How strictly a binding being added must keep clear of the ones already
/// there: only of those under the same condition, or of every one.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum Clear {
    SameCondition,
    Everything,
}

enum Found<C> {
    Command(C),
    Prefix,
    Nothing,
}

impl<C: Clone> Keymap<C> {
    pub fn new(bindings: impl IntoIterator<Item = Binding<C>>) -> Result<Self> {
        Self::default().admit(bindings, Clear::SameCondition)
    }

    pub fn with(self, more: impl IntoIterator<Item = Binding<C>>) -> Result<Self> {
        self.admit(more, Clear::Everything)
    }

    #[must_use]
    pub fn retain(mut self, keep: impl Fn(&Binding<C>) -> bool) -> Self {
        self.bindings.retain(keep);
        self
    }

    pub fn rebound(self, rebindings: &Rebindings) -> Result<Self> {
        let names: Vec<(&'static str, &'static str)> =
            self.bindings.iter().map(|binding| (binding.label, binding.name)).collect();
        let rebound: Vec<Binding<C>> = self
            .bindings
            .into_iter()
            .map(|binding| match rebindings.commands.get(binding.name) {
                Some(keys) => Binding { keys: keys.clone(), ..binding },
                None => binding,
            })
            .collect();

        Self::default().admit(rebound, Clear::SameCondition).map_err(|error| {
            let labels = match &error {
                Error::Shared { first, second, .. } | Error::Prefix { first, second, .. } => {
                    vec![*second, *first]
                }
                _ => Vec::new(),
            };
            let name = labels
                .iter()
                .filter_map(|label| names.iter().find(|(named, _)| named == label))
                .map(|(_, name)| *name)
                .find(|name| rebindings.commands.contains_key(*name))
                .unwrap_or_default();
            Error::Rebinding { name: name.to_owned(), source: Box::new(error) }
        })
    }

    pub fn press(&self, pending: &mut Pending, key: Key, holding: &[&str]) -> Pressed<C> {
        let escape = key.code() == KeyCode::Esc && key.modifiers().is_empty();
        if escape && !pending.is_empty() {
            pending.clear();
            return Pressed::Nothing;
        }
        let midway = !pending.keys.is_empty();

        match self.advance(pending, key, holding) {
            Pressed::Nothing if midway => self.advance(pending, key, holding),
            pressed => pressed,
        }
    }

    pub fn continuations(&self, pending: &Pending, holding: &[&str]) -> Vec<Continuation> {
        if pending.keys.is_empty() {
            return Vec::new();
        }
        for condition in tiers(holding) {
            let mut continuations: Vec<Continuation> = Vec::new();
            for binding in self.bindings.iter().filter(|binding| binding.when == condition) {
                for sequence in &binding.keys {
                    let Some(rest) = sequence.keys().strip_prefix(pending.keys.as_slice()) else {
                        continue;
                    };
                    let Ok(keys) = Sequence::new(rest.to_vec()) else {
                        continue;
                    };
                    if !continuations.iter().any(|continuation| continuation.keys == keys) {
                        continuations.push(Continuation { keys, label: binding.label });
                    }
                }
            }
            if !continuations.is_empty() {
                return continuations;
            }
        }

        Vec::new()
    }

    pub fn bindings(&self) -> &[Binding<C>] {
        &self.bindings
    }

    pub fn names(&self) -> impl Iterator<Item = &'static str> + '_ {
        self.bindings.iter().map(|binding| binding.name)
    }

    fn admit(
        mut self,
        bindings: impl IntoIterator<Item = Binding<C>>,
        clear: Clear,
    ) -> Result<Self> {
        for binding in bindings {
            if self.bindings.iter().any(|bound| bound.name == binding.name) {
                return Err(Error::Name(binding.name));
            }
            for (index, sequence) in binding.keys.iter().enumerate() {
                for other in &binding.keys[..index] {
                    clash(other, binding.label, sequence, binding.label)?;
                }
                let rivals = self
                    .bindings
                    .iter()
                    .filter(|bound| clear == Clear::Everything || bound.when == binding.when);
                for bound in rivals {
                    for held in &bound.keys {
                        clash(held, bound.label, sequence, binding.label)?;
                    }
                }
            }
            self.bindings.push(binding);
        }

        Ok(self)
    }

    fn advance(&self, pending: &mut Pending, key: Key, holding: &[&str]) -> Pressed<C> {
        if pending.keys.is_empty()
            && let Some(digit) = digit(key)
            && (pending.count.is_some()
                || (digit > 0 && matches!(self.find(&[key], holding), Found::Nothing)))
        {
            let count = pending.count.unwrap_or(0).saturating_mul(10).saturating_add(digit);
            pending.count = Some(count);
            return Pressed::Pending;
        }

        pending.keys.push(key);
        match self.find(&pending.keys, holding) {
            Found::Command(command) => {
                let count = pending.count;
                pending.clear();
                Pressed::Command { command, count }
            }
            Found::Prefix => Pressed::Pending,
            Found::Nothing => {
                pending.clear();
                Pressed::Nothing
            }
        }
    }

    fn find(&self, keys: &[Key], holding: &[&str]) -> Found<C> {
        for condition in tiers(holding) {
            let mut prefix = false;
            for binding in self.bindings.iter().filter(|binding| binding.when == condition) {
                for sequence in &binding.keys {
                    if sequence.keys() == keys {
                        return Found::Command(binding.command.clone());
                    }
                    prefix |=
                        sequence.keys().len() > keys.len() && sequence.keys().starts_with(keys);
                }
            }
            if prefix {
                return Found::Prefix;
            }
        }

        Found::Nothing
    }
}

impl<C> IntoIterator for Keymap<C> {
    type Item = Binding<C>;
    type IntoIter = std::vec::IntoIter<Binding<C>>;

    fn into_iter(self) -> Self::IntoIter {
        self.bindings.into_iter()
    }
}

fn tiers<'condition>(
    holding: &'condition [&'condition str],
) -> impl Iterator<Item = Option<&'condition str>> + 'condition {
    holding.iter().copied().map(Some).chain([None])
}

fn digit(key: Key) -> Option<usize> {
    match key.code() {
        KeyCode::Char(character) if key.modifiers().is_empty() => {
            character.to_digit(10).map(|digit| digit as usize)
        }
        _ => None,
    }
}

fn clash(
    held: &Sequence,
    first: &'static str,
    added: &Sequence,
    second: &'static str,
) -> Result<()> {
    if held == added {
        return Err(Error::Shared { keys: added.clone(), first, second });
    }
    let starts = |shorter: &Sequence, longer: &Sequence| {
        shorter.keys().len() < longer.keys().len() && longer.keys().starts_with(shorter.keys())
    };
    if starts(held, added) {
        return Err(Error::Prefix { shorter: held.clone(), longer: added.clone(), first, second });
    }
    if starts(added, held) {
        return Err(Error::Prefix {
            shorter: added.clone(),
            longer: held.clone(),
            first: second,
            second: first,
        });
    }

    Ok(())
}
