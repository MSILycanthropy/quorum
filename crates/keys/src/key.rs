//! One key, and keys pressed one after another, as a terminal reports them and as people write them.

use std::fmt::{self, Write};
use std::str::FromStr;

use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::{Error, Result};

const NAMES: [(&str, KeyCode); 19] = [
    ("Enter", KeyCode::Enter),
    ("Esc", KeyCode::Esc),
    ("Tab", KeyCode::Tab),
    ("Space", KeyCode::Char(' ')),
    ("Backspace", KeyCode::Backspace),
    ("Delete", KeyCode::Delete),
    ("Insert", KeyCode::Insert),
    ("Home", KeyCode::Home),
    ("End", KeyCode::End),
    ("PgUp", KeyCode::PageUp),
    ("PgDn", KeyCode::PageDown),
    ("Up", KeyCode::Up),
    ("Down", KeyCode::Down),
    ("Left", KeyCode::Left),
    ("Right", KeyCode::Right),
    ("↑", KeyCode::Up),
    ("↓", KeyCode::Down),
    ("←", KeyCode::Left),
    ("→", KeyCode::Right),
];

const MODIFIERS: [(&str, KeyModifiers); 6] = [
    ("ctrl-", KeyModifiers::CONTROL),
    ("C-", KeyModifiers::CONTROL),
    ("alt-", KeyModifiers::ALT),
    ("A-", KeyModifiers::ALT),
    ("shift-", KeyModifiers::SHIFT),
    ("S-", KeyModifiers::SHIFT),
];

const HIGHEST_FUNCTION_KEY: u8 = 24;

/// One key as the terminal reports it. Shift is folded into the character it
/// produces, so `G` is `G`, and `ctrl-shift-c` is Control with `C`.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct Key {
    code: KeyCode,
    modifiers: KeyModifiers,
}

impl Key {
    pub fn new(code: KeyCode, modifiers: KeyModifiers) -> Result<Self> {
        let others = modifiers.difference(KeyModifiers::SHIFT);
        match code {
            KeyCode::BackTab => {
                Ok(Self { code: KeyCode::Tab, modifiers: modifiers | KeyModifiers::SHIFT })
            }
            KeyCode::Char(character) if modifiers.contains(KeyModifiers::SHIFT) => {
                let held = others.intersects(KeyModifiers::CONTROL | KeyModifiers::ALT);
                if held && character.is_ascii_alphabetic() {
                    Ok(Self {
                        code: KeyCode::Char(character.to_ascii_uppercase()),
                        modifiers: others,
                    })
                } else {
                    Err(Error::Shifted(spelt(code, modifiers)))
                }
            }
            code => Ok(Self { code, modifiers }),
        }
    }

    pub fn code(self) -> KeyCode {
        self.code
    }

    pub fn modifiers(self) -> KeyModifiers {
        self.modifiers
    }

    fn plain_character(self) -> Option<char> {
        match self.code {
            KeyCode::Char(character) if self.modifiers.is_empty() && character != ' ' => {
                Some(character)
            }
            _ => None,
        }
    }
}

impl From<KeyEvent> for Key {
    fn from(event: KeyEvent) -> Self {
        let others = event.modifiers.difference(KeyModifiers::SHIFT);
        match event.code {
            KeyCode::BackTab => {
                Self { code: KeyCode::Tab, modifiers: event.modifiers | KeyModifiers::SHIFT }
            }
            KeyCode::Char(character) if event.modifiers.contains(KeyModifiers::SHIFT) => {
                Self { code: KeyCode::Char(character.to_ascii_uppercase()), modifiers: others }
            }
            code => Self { code, modifiers: event.modifiers },
        }
    }
}

impl fmt::Display for Key {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let (code, modifiers) = match self.code {
            KeyCode::Char(character)
                if character.is_ascii_uppercase()
                    && self.modifiers.intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) =>
            {
                (
                    KeyCode::Char(character.to_ascii_lowercase()),
                    self.modifiers | KeyModifiers::SHIFT,
                )
            }
            code => (code, self.modifiers),
        };

        formatter.write_str(&spelt(code, modifiers))
    }
}

impl fmt::Debug for Key {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.debug_tuple("Key").field(&self.to_string()).finish()
    }
}

impl FromStr for Key {
    type Err = Error;

    fn from_str(written: &str) -> Result<Self> {
        match word(written)?.as_slice() {
            [key] => Ok(*key),
            _ => Err(Error::Unnamed(written.to_owned())),
        }
    }
}

fn spelt(code: KeyCode, modifiers: KeyModifiers) -> String {
    let mut written = String::new();
    for (prefix, modifier) in
        [("C-", KeyModifiers::CONTROL), ("A-", KeyModifiers::ALT), ("S-", KeyModifiers::SHIFT)]
    {
        if modifiers.contains(modifier) {
            written.push_str(prefix);
        }
    }
    let name = NAMES.iter().find(|(name, named)| *named == code && name.chars().count() > 1);
    match (code, name) {
        (KeyCode::Left, _) => written.push('←'),
        (KeyCode::Right, _) => written.push('→'),
        (KeyCode::Up, _) => written.push('↑'),
        (KeyCode::Down, _) => written.push('↓'),
        (_, Some((name, _))) => written.push_str(name),
        (KeyCode::Char(character), None) => written.push(character),
        (KeyCode::F(number), None) => {
            write!(written, "F{number}").expect("writing to a String cannot fail");
        }
        (code, None) => write!(written, "{code:?}").expect("writing to a String cannot fail"),
    }

    written
}

fn named(written: &str) -> Option<KeyCode> {
    if let Some((_, code)) = NAMES.iter().find(|(name, _)| *name == written) {
        return Some(*code);
    }
    let number: u8 = written.strip_prefix('F')?.parse().ok()?;

    (1..=HIGHEST_FUNCTION_KEY).contains(&number).then_some(KeyCode::F(number))
}

fn misspelt(written: &str) -> Option<&'static str> {
    if written.chars().count() < 2 {
        return None;
    }
    if let Some((name, _)) = NAMES.iter().find(|(name, _)| name.eq_ignore_ascii_case(written)) {
        return Some(name);
    }
    let number: u8 = written.strip_prefix(['f', 'F'])?.parse().ok()?;
    let name = FUNCTION_KEYS.get(usize::from(number).checked_sub(1)?)?;

    Some(name)
}

const FUNCTION_KEYS: [&str; HIGHEST_FUNCTION_KEY as usize] = [
    "F1", "F2", "F3", "F4", "F5", "F6", "F7", "F8", "F9", "F10", "F11", "F12", "F13", "F14", "F15",
    "F16", "F17", "F18", "F19", "F20", "F21", "F22", "F23", "F24",
];

fn word(written: &str) -> Result<Vec<Key>> {
    let mut rest = written;
    let mut modifiers = KeyModifiers::NONE;
    while let Some((modifier, after)) = MODIFIERS.iter().find_map(|(prefix, modifier)| {
        rest.strip_prefix(prefix).filter(|after| !after.is_empty()).map(|after| (*modifier, after))
    }) {
        modifiers |= modifier;
        rest = after;
    }

    if let Some(code) = named(rest) {
        return Ok(vec![Key::new(code, modifiers)?]);
    }
    let mut characters = rest.chars();
    if let (Some(character), None) = (characters.next(), characters.next()) {
        return Ok(vec![Key::new(KeyCode::Char(character), modifiers)?]);
    }
    if !modifiers.is_empty() {
        return Err(Error::Unnamed(rest.to_owned()));
    }
    if let Some(name) = misspelt(rest) {
        return Err(Error::Spelt { written: rest.to_owned(), name });
    }

    Ok(rest
        .chars()
        .map(|character| Key { code: KeyCode::Char(character), modifiers: KeyModifiers::NONE })
        .collect())
}

/// Keys pressed one after another to reach a command, like `gg` or `]c`. Never
/// empty, and Esc can only come first.
#[derive(Clone, PartialEq, Eq, Hash)]
pub struct Sequence(Vec<Key>);

impl Sequence {
    pub fn new(keys: Vec<Key>) -> Result<Self> {
        if keys.is_empty() {
            return Err(Error::Empty);
        }
        let escape = Key { code: KeyCode::Esc, modifiers: KeyModifiers::NONE };
        if keys.iter().skip(1).any(|key| *key == escape) {
            return Err(Error::Escape(shown(&keys)));
        }

        Ok(Self(keys))
    }

    pub fn all(written: &[&str]) -> Result<Vec<Self>> {
        written.iter().map(|sequence| sequence.parse()).collect()
    }

    pub fn keys(&self) -> &[Key] {
        &self.0
    }
}

impl From<Key> for Sequence {
    fn from(key: Key) -> Self {
        Self(vec![key])
    }
}

impl FromStr for Sequence {
    type Err = Error;

    fn from_str(written: &str) -> Result<Self> {
        let mut keys = Vec::new();
        for part in written.split_whitespace() {
            keys.extend(word(part)?);
        }

        Self::new(keys)
    }
}

impl fmt::Display for Sequence {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&shown(&self.0))
    }
}

impl fmt::Debug for Sequence {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.debug_tuple("Sequence").field(&self.to_string()).finish()
    }
}

impl serde::Serialize for Sequence {
    fn serialize<S: serde::Serializer>(
        &self,
        serializer: S,
    ) -> std::result::Result<S::Ok, S::Error> {
        serializer.collect_str(self)
    }
}

impl<'de> serde::Deserialize<'de> for Sequence {
    fn deserialize<D: serde::Deserializer<'de>>(
        deserializer: D,
    ) -> std::result::Result<Self, D::Error> {
        let written = String::deserialize(deserializer)?;

        written.parse().map_err(serde::de::Error::custom)
    }
}

pub(crate) fn shown(keys: &[Key]) -> String {
    let characters: Option<String> = keys.iter().map(|key| key.plain_character()).collect();
    match characters {
        Some(run) if keys.len() > 1 && named(&run).is_none() && misspelt(&run).is_none() => run,
        _ => keys.iter().map(ToString::to_string).collect::<Vec<_>>().join(" "),
    }
}
