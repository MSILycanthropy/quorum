//! Every command the viewer understands, the keys bound to it, and the palette model.

use std::path::PathBuf;

use keys::{Key, Keymap, Pending, Pressed, Rebindings};
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
    Copy,
    CopyPath,
    CopyLocation,
    Select,
    SelectDown,
    SelectUp,
    ToggleViewed,
    NextUnviewed,
    PreviousUnviewed,
    NextHunk,
    PreviousHunk,
    ExpandFold,
    CloseFold,
    ToggleFold,
    OpenEverything,
    FoldEverything,
    NextLine,
    PreviousLine,
    ScrollLineDown,
    ScrollLineUp,
    HalfPageDown,
    HalfPageUp,
    PageDown,
    PageUp,
    Top,
    Bottom,
    GoToLine,
    CursorLineToMiddle,
    CursorLineToTop,
    CursorLineToBottom,
    OldSide,
    NewSide,
    ScrollLeft,
    ScrollRight,
    ToggleLayout,
    ToggleWrapping,
    ToggleWhitespace,
    NextAnnotation,
    PreviousAnnotation,
    Host(&'static str),
}

impl Command {
    pub fn moves(self) -> bool {
        matches!(
            self,
            Self::NextFile
                | Self::PreviousFile
                | Self::NextUnviewed
                | Self::PreviousUnviewed
                | Self::NextHunk
                | Self::PreviousHunk
                | Self::NextLine
                | Self::PreviousLine
                | Self::HalfPageDown
                | Self::HalfPageUp
                | Self::PageDown
                | Self::PageUp
                | Self::Top
                | Self::Bottom
                | Self::GoToLine
                | Self::NextAnnotation
                | Self::PreviousAnnotation
        )
    }

    pub fn repeats(self) -> bool {
        matches!(
            self,
            Self::NextFile
                | Self::PreviousFile
                | Self::NextUnviewed
                | Self::PreviousUnviewed
                | Self::NextHunk
                | Self::PreviousHunk
                | Self::NextLine
                | Self::PreviousLine
                | Self::ScrollLineDown
                | Self::ScrollLineUp
                | Self::HalfPageDown
                | Self::HalfPageUp
                | Self::PageDown
                | Self::PageUp
                | Self::SelectDown
                | Self::SelectUp
                | Self::ScrollLeft
                | Self::ScrollRight
                | Self::NextAnnotation
                | Self::PreviousAnnotation
        )
    }
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
                "palette" => C::Palette, "Command palette", [":", "C-t"];
                "reload" => C::Reload, "Reload from git", ["r", "F5"];
                "quit" => C::Quit, "Quit", ["q", "ZZ", "ZQ", "C-c"], hint: "quit";
                "back" => C::Back, "Back, or quit", ["Esc"];
            }
            "Files" {
                "next-file" => C::NextFile, "Next file", ["Tab", "]f"], hint: "file";
                "previous-file" => C::PreviousFile, "Previous file", ["S-Tab", "[f"];
                "find-file" => C::FindFile, "Find file", ["/", "C-f", "C-p"], hint: "find";
                "toggle-file-list" => C::ToggleFileList, "Toggle file list", ["f", "C-b"], hint: "files";
                "edit" => C::Edit, "Open in $EDITOR", ["e"], hint: "edit";
                "copy" => C::Copy, "Copy the line", ["yy", "C-S-c"];
                "copy-selection" => C::Copy, "Copy the selected lines", ["y", "C-c", "C-S-c"], when: "selecting";
                "copy-path" => C::CopyPath, "Copy path", ["yp"];
                "copy-location" => C::CopyLocation, "Copy path:line or lines", ["yl", "Y"];
                "toggle-viewed" => C::ToggleViewed, "Mark viewed, go to next", ["x"], hint: "viewed";
                "next-unviewed" => C::NextUnviewed, "Next unviewed file", ["]u"];
                "previous-unviewed" => C::PreviousUnviewed, "Previous unviewed file", ["[u"];
            }
            "Diff" {
                "next-hunk" => C::NextHunk, "Next hunk", ["n", "]c", "F7"], hint: "hunk";
                "previous-hunk" => C::PreviousHunk, "Previous hunk", ["N", "[c", "S-F7"];
                "next-line" => C::NextLine, "Down a line", ["j", "↓"];
                "previous-line" => C::PreviousLine, "Up a line", ["k", "↑"];
                "scroll-line-down" => C::ScrollLineDown, "Scroll down a line", ["C-e", "C-↓"];
                "scroll-line-up" => C::ScrollLineUp, "Scroll up a line", ["C-y", "C-↑"];
                "half-page-down" => C::HalfPageDown, "Down half a page", ["C-d"];
                "half-page-up" => C::HalfPageUp, "Up half a page", ["C-u"];
                "page-down" => C::PageDown, "Down a page", ["PgDn", "Space"];
                "page-up" => C::PageUp, "Up a page", ["PgUp"];
                "top" => C::Top, "Top, or the line counted", ["gg", "Home", "C-Home"];
                "bottom" => C::Bottom, "Bottom, or the line counted", ["G", "End", "C-End"];
                "go-to-line" => C::GoToLine, "Go to a line", ["C-g"];
                "cursor-line-to-middle" => C::CursorLineToMiddle, "Cursor line to the middle", ["zz"];
                "cursor-line-to-top" => C::CursorLineToTop, "Cursor line to the top", ["zt"];
                "cursor-line-to-bottom" => C::CursorLineToBottom, "Cursor line to the bottom", ["zb"];
                "old-side" => C::OldSide, "Cursor to the old side", ["H", "S-←"];
                "new-side" => C::NewSide, "Cursor to the new side", ["L", "S-→"];
                "scroll-left" => C::ScrollLeft, "Scroll left", ["h", "←"];
                "scroll-right" => C::ScrollRight, "Scroll right", ["l", "→"];
                "select" => C::Select, "Select lines", ["V", "v"];
                "select-down" => C::SelectDown, "Select down a line", ["S-↓"];
                "select-up" => C::SelectUp, "Select up a line", ["S-↑"];
                "expand-fold" => C::ExpandFold, "Unfold, or toggle annotation", ["Enter", "o", "zo"];
                "close-fold" => C::CloseFold, "Fold back up", ["zc"];
                "toggle-fold" => C::ToggleFold, "Unfold or fold back up", ["za"];
                "open-everything" => C::OpenEverything, "Open every fold and annotation", ["zR"];
                "fold-everything" => C::FoldEverything, "Fold everything up again", ["zM"];
            }
            "View" {
                "toggle-layout" => C::ToggleLayout, "Split or unified", ["s"], hint: "layout";
                "toggle-wrapping" => C::ToggleWrapping, "Wrap long lines", ["w", "A-z"];
                "toggle-whitespace" => C::ToggleWhitespace, "Ignore whitespace", ["W"];
            }
            "Annotations" {
                "next-annotation" => C::NextAnnotation, "Next annotation", ["]d", "F8"];
                "previous-annotation" => C::PreviousAnnotation, "Previous annotation", ["[d", "S-F8"];
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

    pub fn rebound(self, rebindings: &Rebindings) -> Result<Self> {
        let keymap = self.keymap.rebound(rebindings)?;
        let keyless = keymap
            .bindings()
            .iter()
            .find(|binding| REQUIRED.contains(&binding.command) && binding.keys.is_empty());
        if let Some(binding) = keyless {
            return Err(Error::Required(binding.label));
        }

        Ok(Self { keymap })
    }

    pub fn names(&self) -> impl Iterator<Item = &'static str> + '_ {
        self.keymap.names()
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

    pub fn press(
        &self,
        pending: &mut Pending,
        key: KeyEvent,
        holding: &[&str],
    ) -> Pressed<Command> {
        self.keymap.press(pending, Key::from(key), holding)
    }

    pub fn continuations(
        &self,
        pending: &Pending,
        holding: &[&str],
    ) -> Vec<(String, &'static str)> {
        self.keymap
            .continuations(pending, holding)
            .into_iter()
            .map(|continuation| (continuation.keys.to_string(), continuation.label))
            .collect()
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
            .filter(|binding| binding.command != Command::Palette && binding.when.is_none())
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
    Line(usize),
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

const QUITTING: [&str; 6] = ["q!", "qa", "qa!", "wq", "wqa", "x"];

pub fn ex(query: &str) -> Option<PaletteEntry> {
    let query = query.trim();
    if let Ok(line) = query.parse::<usize>()
        && line > 0
    {
        let label = format!("Go to line {line}");
        return Some(PaletteEntry {
            target: PaletteTarget::Line(line),
            label,
            keys: String::new(),
        });
    }

    QUITTING.contains(&query).then(|| PaletteEntry {
        target: PaletteTarget::Command(Command::Quit),
        label: "Quit".to_owned(),
        keys: String::new(),
    })
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
