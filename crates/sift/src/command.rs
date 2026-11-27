//! Every command the viewer understands, the keys bound to it, and the palette model.

use std::path::PathBuf;

use keys::{Key, Keymap, Pending, Pressed};
use ratatui::crossterm::event::KeyEvent;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Command {
    Help,
    Palette,
    FindFile,
    Quit,
    Back,
    Reload,
    NextFile,
    PreviousFile,
    ToggleFileList,
    Edit,
    CopyPath,
    CopyLocation,
    Select,
    ToggleViewed,
    NextUnviewed,
    NextHunk,
    PreviousHunk,
    ExpandFold,
    ToggleFolds,
    NextLine,
    PreviousLine,
    HalfPageDown,
    HalfPageUp,
    PageDown,
    PageUp,
    Top,
    Bottom,
    OldSide,
    NewSide,
    ScrollLeft,
    ScrollRight,
    ToggleLayout,
    ToggleWrapping,
    ToggleWhitespace,
    NextAnnotation,
    PreviousAnnotation,
    ToggleAnnotations,
    Host(&'static str),
}

/// One command, what it is called, and how to reach it.
pub type Binding = keys::Binding<Command>;

/// The sections Sift's own commands are grouped under, in the order help lays them out.
pub const SECTIONS: [&str; 5] = ["General", "Files", "Diff", "View", "Annotations"];

pub const ANNOTATIONS: &str = "Annotations";

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Keys(#[from] keys::Error),
    #[error("{0} must be a host command in a host section")]
    NotHost(&'static str),
    #[error("{0} is a host command: leave it out of the host bindings instead")]
    NotSift(&'static str),
    #[error("{0} cannot be unbound")]
    Required(&'static str),
}

pub type Result<T> = std::result::Result<T, Error>;

/// The single source of truth for what a key does.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Catalog {
    keymap: Keymap<Command>,
}

impl Default for Catalog {
    fn default() -> Self {
        use Command as C;

        let keymap = keys::keymap! {
            "General" {
                "help" => C::Help, "Show every key", ["?"], hint: "help";
                "palette" => C::Palette, "Command palette", [":", "C-p"];
                "reload" => C::Reload, "Reload from git", ["r"];
                "quit" => C::Quit, "Quit", ["q", "C-c"], hint: "quit";
                "back" => C::Back, "Back, or quit", ["Esc"];
            }
            "Files" {
                "next-file" => C::NextFile, "Next file", ["]", "Tab"], hint: "file";
                "previous-file" => C::PreviousFile, "Previous file", ["[", "S-Tab"];
                "find-file" => C::FindFile, "Find file", ["/"], hint: "find";
                "toggle-file-list" => C::ToggleFileList, "Toggle file list", ["C-b", "f"], hint: "files";
                "edit" => C::Edit, "Open in $EDITOR", ["e"], hint: "edit";
                "copy-path" => C::CopyPath, "Copy path", ["y"];
                "copy-location" => C::CopyLocation, "Copy path:line or lines", ["Y"];
                "select" => C::Select, "Select lines", ["V"];
                "toggle-viewed" => C::ToggleViewed, "Mark viewed, go to next", ["v"], hint: "viewed";
                "next-unviewed" => C::NextUnviewed, "Next unviewed file", ["u"];
            }
            "Diff" {
                "next-hunk" => C::NextHunk, "Next hunk", ["n"], hint: "hunk";
                "previous-hunk" => C::PreviousHunk, "Previous hunk", ["N", "p"];
                "expand-fold" => C::ExpandFold, "Unfold, or toggle annotation", ["Enter", "o"];
                "next-line" => C::NextLine, "Down a line", ["j", "↓"];
                "previous-line" => C::PreviousLine, "Up a line", ["k", "↑"];
                "half-page-down" => C::HalfPageDown, "Down half a page", ["C-d"];
                "half-page-up" => C::HalfPageUp, "Up half a page", ["C-u"];
                "page-down" => C::PageDown, "Down a page", ["PgDn", "C-f", "Space"];
                "page-up" => C::PageUp, "Up a page", ["PgUp"];
                "top" => C::Top, "Top of the file", ["g", "Home"];
                "bottom" => C::Bottom, "Bottom of the file", ["G", "End"];
                "old-side" => C::OldSide, "Cursor to the old side", ["H", "S-←"];
                "new-side" => C::NewSide, "Cursor to the new side", ["L", "S-→"];
                "scroll-left" => C::ScrollLeft, "Scroll left", ["h", "←"];
                "scroll-right" => C::ScrollRight, "Scroll right", ["l", "→"];
            }
            "View" {
                "toggle-layout" => C::ToggleLayout, "Split or unified", ["s"], hint: "layout";
                "toggle-wrapping" => C::ToggleWrapping, "Wrap long lines", ["w"];
                "toggle-folds" => C::ToggleFolds, "Fold unchanged lines", ["z"];
                "toggle-whitespace" => C::ToggleWhitespace, "Ignore whitespace", ["W"];
            }
            "Annotations" {
                "next-annotation" => C::NextAnnotation, "Next annotation", ["}"];
                "previous-annotation" => C::PreviousAnnotation, "Previous annotation", ["{"];
                "toggle-annotations" => C::ToggleAnnotations, "Collapse every annotation", ["Z"];
            }
        };

        Self { keymap: keymap.expect("Sift's own keys are spelt right and do not clash") }
    }
}

impl Catalog {
    pub fn bindings(&self) -> &[Binding] {
        self.keymap.bindings()
    }

    pub fn unbind(self, commands: &[Command]) -> Result<Self> {
        for command in commands {
            if let Command::Host(name) = command {
                return Err(Error::NotSift(name));
            }
            if let Some(binding) =
                self.bindings().iter().find(|binding| binding.command == *command)
                && REQUIRED.contains(command)
            {
                return Err(Error::Required(binding.label));
            }
        }
        let keymap = self.keymap.retain(|binding| !commands.contains(&binding.command));

        Ok(Self { keymap })
    }

    pub fn with(self, bindings: Vec<Binding>) -> Result<Self> {
        for binding in &bindings {
            let host = matches!(binding.command, Command::Host(_));
            if !host || SECTIONS.contains(&binding.section) {
                return Err(Error::NotHost(binding.label));
            }
        }

        Ok(Self { keymap: self.keymap.with(bindings)? })
    }

    pub fn sections(&self) -> Vec<&'static str> {
        let mut sections = SECTIONS.to_vec();
        for binding in self.bindings() {
            if !sections.contains(&binding.section) {
                sections.push(binding.section);
            }
        }

        sections
    }

    #[must_use]
    pub fn without(&self, section: &str) -> Self {
        Self { keymap: self.keymap.clone().retain(|binding| binding.section != section) }
    }

    pub fn press(&self, pending: &mut Pending, key: KeyEvent) -> Pressed<Command> {
        self.keymap.press(pending, Key::from(key), &[])
    }

    pub fn hints(&self) -> impl Iterator<Item = (String, &'static str)> + '_ {
        let host =
            self.bindings().iter().filter(|binding| matches!(binding.command, Command::Host(_)));
        let sift = HINTED.iter().filter_map(|command| {
            self.bindings().iter().find(|binding| binding.command == *command)
        });

        host.chain(sift).filter_map(|binding| {
            let hint = binding.hint?;
            let key = binding.keys.first()?.to_string();

            Some((key, hint))
        })
    }

    pub fn palette_entries(&self) -> Vec<PaletteEntry> {
        self.bindings()
            .iter()
            .filter(|binding| binding.command != Command::Palette)
            .map(|binding| PaletteEntry {
                target: PaletteTarget::Command(binding.command),
                label: binding.label.to_owned(),
                keys: binding.shown(),
            })
            .collect()
    }
}

const REQUIRED: [Command; 2] = [Command::Quit, Command::Help];

const HINTED: [Command; 9] = [
    Command::NextHunk,
    Command::NextFile,
    Command::ToggleViewed,
    Command::ToggleFileList,
    Command::FindFile,
    Command::Edit,
    Command::ToggleLayout,
    Command::Help,
    Command::Quit,
];

/// What choosing a palette entry does.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum PaletteTarget {
    Command(Command),
    File { path: PathBuf, line: Option<usize> },
}

/// A palette row: what it does, what it says, and the keys that do it directly.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct PaletteEntry {
    pub target: PaletteTarget,
    pub label: String,
    pub keys: String,
}

impl PaletteEntry {
    pub fn file(path: PathBuf) -> Self {
        let label = path.display().to_string();

        Self { target: PaletteTarget::File { path, line: None }, label, keys: String::new() }
    }
}

pub fn located(query: &str) -> (&str, Option<usize>) {
    let query = query.trim().trim_start_matches("./");
    let Some((rest, last)) = query.rsplit_once(':') else {
        return (query, None);
    };
    let Ok(last) = last.parse::<usize>() else {
        return (query, None);
    };
    if let Some((path, line)) = rest.rsplit_once(':')
        && let Ok(line) = line.parse::<usize>()
    {
        return (path, Some(line));
    }

    (rest, Some(last))
}

/// A deterministic, case-insensitive ranker for palette entries.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct PaletteModel;

impl PaletteModel {
    pub fn rank<'entry>(
        &self,
        query: &str,
        entries: &'entry [PaletteEntry],
    ) -> Vec<&'entry PaletteEntry> {
        let query = lower(located(query).0);
        if query.is_empty() {
            return entries.iter().collect();
        }

        let mut ranked = entries
            .iter()
            .enumerate()
            .filter_map(|(order, entry)| {
                let score = score(&query, &lower(&entry.label))?;
                Some((score, order, entry))
            })
            .collect::<Vec<_>>();
        ranked.sort_by_key(|(score, order, _)| (*score, *order));
        ranked.into_iter().map(|(_, _, entry)| entry).collect()
    }
}

fn lower(text: &str) -> Vec<char> {
    text.chars().flat_map(char::to_lowercase).collect()
}

fn score(query: &[char], label: &[char]) -> Option<u8> {
    if query == label {
        return Some(0);
    }
    if label.starts_with(query) {
        return Some(1);
    }
    let name = label.rsplit(|character| *character == '/').next().unwrap_or(label);
    if name.starts_with(query) {
        return Some(1);
    }
    if label.windows(query.len()).any(|window| window == query) {
        return Some(2);
    }

    let mut label = label.iter();
    for character in query {
        if !label.any(|candidate| candidate == character) {
            return None;
        }
    }
    Some(3)
}
