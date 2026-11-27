//! Every command the viewer understands, the keys bound to it, and the palette model.

use std::path::PathBuf;

use hints::Hint;
use keys::{Key, Keymap, Pending, Pressed, Rebindings, keymap};
use picker::Entry;
use ratatui::crossterm::event::KeyEvent;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Command {
    Settings,
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

/// The sections Sift's own commands are grouped under, in the order they are listed.
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

        let keymap = keymap! {
            "General" {
                "palette" => C::Palette, "Command palette", [":", "C-t"];
                "settings" => C::Settings, "Settings and keys", ["C-,"], hint: "keys", pinned: true;
                "reload" => C::Reload, "Reload from git", ["r", "F5"];
                "quit" => C::Quit, "Quit", ["q", "ZZ", "ZQ", "C-c"], hint: "quit", pinned: true;
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

    pub fn keymap(&self) -> &Keymap<Command> {
        &self.keymap
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

    pub fn hints(&self, holding: &[&str]) -> Vec<Hint> {
        let host =
            self.bindings().iter().filter(|binding| matches!(binding.command, Command::Host(_)));
        let sift = HINTED
            .iter()
            .filter_map(|name| self.bindings().iter().find(|binding| binding.name == *name));

        host.chain(sift).filter_map(|binding| Hint::of(binding, holding)).collect()
    }

    pub fn palette_entries(&self) -> Vec<PaletteEntry> {
        self.bindings()
            .iter()
            .filter(|binding| binding.command != Command::Palette)
            .filter_map(|binding| Entry::of(binding, &[]))
            .map(|entry| entry.map(PaletteTarget::Command))
            .collect()
    }
}

const REQUIRED: [Command; 1] = [Command::Quit];

const HINTED: [&str; 9] = [
    "next-hunk",
    "next-file",
    "toggle-viewed",
    "toggle-file-list",
    "find-file",
    "edit",
    "toggle-layout",
    "settings",
    "quit",
];

/// What choosing a palette entry does.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum PaletteTarget {
    Command(Command),
    Line(usize),
    File(PathBuf),
}

/// A palette row: what it does, what it says, and the keys that do it directly.
pub type PaletteEntry = Entry<PaletteTarget>;

pub fn file(path: PathBuf) -> PaletteEntry {
    let label = path.display().to_string();

    Entry::new(PaletteTarget::File(path), label)
}

const QUITTING: [&str; 6] = ["q!", "qa", "qa!", "wq", "wqa", "x"];

pub fn ex(query: &str) -> Option<PaletteEntry> {
    let query = query.trim();
    if let Ok(line) = query.parse::<usize>()
        && line > 0
    {
        return Some(Entry::new(PaletteTarget::Line(line), format!("Go to line {line}")));
    }

    QUITTING.contains(&query).then(|| Entry::new(PaletteTarget::Command(Command::Quit), "Quit"))
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
