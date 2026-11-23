//! Every command the viewer understands, the keys bound to it, and the palette model.

use std::path::PathBuf;

use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

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
    ScrollLeft,
    ScrollRight,
    ToggleLayout,
    ToggleWrapping,
    ToggleWhitespace,
}

/// A key combination. Shift is folded into the character it produces, so `G`
/// and `?` match however the terminal reports them.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Shortcut {
    pub code: KeyCode,
    pub modifiers: KeyModifiers,
}

impl Shortcut {
    pub const fn new(code: KeyCode, modifiers: KeyModifiers) -> Self {
        Self { code, modifiers }
    }

    pub fn display(self) -> String {
        let key = match self.code {
            KeyCode::Char(' ') => "Space".to_owned(),
            KeyCode::Char(character) => character.to_string(),
            KeyCode::Enter => "Enter".to_owned(),
            KeyCode::Left => "←".to_owned(),
            KeyCode::Right => "→".to_owned(),
            KeyCode::Up => "↑".to_owned(),
            KeyCode::Down => "↓".to_owned(),
            KeyCode::Home => "Home".to_owned(),
            KeyCode::End => "End".to_owned(),
            KeyCode::PageUp => "PgUp".to_owned(),
            KeyCode::PageDown => "PgDn".to_owned(),
            KeyCode::Tab => "Tab".to_owned(),
            KeyCode::BackTab => "S-Tab".to_owned(),
            KeyCode::Esc => "Esc".to_owned(),
            code => format!("{code:?}"),
        };

        if self.modifiers.contains(KeyModifiers::CONTROL) { format!("C-{key}") } else { key }
    }
}

impl From<KeyEvent> for Shortcut {
    fn from(key: KeyEvent) -> Self {
        let modifiers = match key.code {
            KeyCode::Char(_) | KeyCode::BackTab => key.modifiers.difference(KeyModifiers::SHIFT),
            _ => key.modifiers,
        };

        Self::new(key.code, modifiers)
    }
}

/// The groups the help overlay lays commands out under.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Category {
    General,
    Files,
    Diff,
    View,
}

impl Category {
    pub const ALL: [Self; 4] = [Self::General, Self::Files, Self::Diff, Self::View];

    pub fn title(self) -> &'static str {
        match self {
            Self::General => "General",
            Self::Files => "Files",
            Self::Diff => "Diff",
            Self::View => "View",
        }
    }
}

/// One command, what it is called, and how to reach it. A binding with a hint
/// is advertised in the status line.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Binding {
    pub command: Command,
    pub label: &'static str,
    pub shortcuts: Vec<Shortcut>,
    pub category: Category,
    pub hint: Option<&'static str>,
}

impl Binding {
    pub fn keys(&self) -> String {
        self.shortcuts.iter().map(|shortcut| shortcut.display()).collect::<Vec<_>>().join(" ")
    }
}

/// The single source of truth for what a key does.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Catalog {
    bindings: Vec<Binding>,
}

impl Default for Catalog {
    fn default() -> Self {
        use Category::{Diff, Files, General, View};
        use Command as C;

        let bind = |command, label, shortcuts: &[Shortcut], category, hint| Binding {
            command,
            label,
            shortcuts: shortcuts.to_vec(),
            category,
            hint,
        };

        Self {
            bindings: vec![
                bind(C::Help, "Show every key", &[plain('?')], General, Some("help")),
                bind(C::Palette, "Command palette", &[plain(':'), ctrl('p')], General, None),
                bind(C::Reload, "Reload from git", &[plain('r')], General, None),
                bind(C::Quit, "Quit", &[plain('q'), ctrl('c')], General, Some("quit")),
                bind(C::Back, "Back, or quit", &[code(KeyCode::Esc)], General, None),
                bind(
                    C::NextFile,
                    "Next file",
                    &[plain(']'), code(KeyCode::Tab)],
                    Files,
                    Some("file"),
                ),
                bind(
                    C::PreviousFile,
                    "Previous file",
                    &[plain('['), code(KeyCode::BackTab)],
                    Files,
                    None,
                ),
                bind(C::FindFile, "Find file", &[plain('/')], Files, Some("find")),
                bind(
                    C::ToggleFileList,
                    "Toggle file list",
                    &[ctrl('b'), plain('f')],
                    Files,
                    Some("files"),
                ),
                bind(C::Edit, "Open in $EDITOR", &[plain('e')], Files, Some("edit")),
                bind(C::CopyPath, "Copy path", &[plain('y')], Files, None),
                bind(C::CopyLocation, "Copy path:line", &[plain('Y')], Files, None),
                bind(
                    C::ToggleViewed,
                    "Mark viewed, go to next",
                    &[plain('v')],
                    Files,
                    Some("viewed"),
                ),
                bind(C::NextUnviewed, "Next unviewed file", &[plain('u')], Files, None),
                bind(C::NextHunk, "Next hunk", &[plain('n')], Diff, Some("hunk")),
                bind(C::PreviousHunk, "Previous hunk", &[plain('N'), plain('p')], Diff, None),
                bind(
                    C::ExpandFold,
                    "Expand folded lines",
                    &[code(KeyCode::Enter), plain('o')],
                    Diff,
                    None,
                ),
                bind(C::NextLine, "Down a line", &[plain('j'), code(KeyCode::Down)], Diff, None),
                bind(C::PreviousLine, "Up a line", &[plain('k'), code(KeyCode::Up)], Diff, None),
                bind(C::HalfPageDown, "Down half a page", &[ctrl('d')], Diff, None),
                bind(C::HalfPageUp, "Up half a page", &[ctrl('u')], Diff, None),
                bind(
                    C::PageDown,
                    "Down a page",
                    &[code(KeyCode::PageDown), ctrl('f'), plain(' ')],
                    Diff,
                    None,
                ),
                bind(C::PageUp, "Up a page", &[code(KeyCode::PageUp)], Diff, None),
                bind(C::Top, "Top of the file", &[plain('g'), code(KeyCode::Home)], Diff, None),
                bind(
                    C::Bottom,
                    "Bottom of the file",
                    &[plain('G'), code(KeyCode::End)],
                    Diff,
                    None,
                ),
                bind(C::ScrollLeft, "Scroll left", &[plain('h'), code(KeyCode::Left)], Diff, None),
                bind(
                    C::ScrollRight,
                    "Scroll right",
                    &[plain('l'), code(KeyCode::Right)],
                    Diff,
                    None,
                ),
                bind(C::ToggleLayout, "Split or unified", &[plain('s')], View, Some("layout")),
                bind(C::ToggleWrapping, "Wrap long lines", &[plain('w')], View, None),
                bind(C::ToggleFolds, "Fold unchanged lines", &[plain('z')], View, None),
                bind(C::ToggleWhitespace, "Ignore whitespace", &[plain('W')], View, None),
            ],
        }
    }
}

impl Catalog {
    pub fn bindings(&self) -> &[Binding] {
        &self.bindings
    }

    pub fn command(&self, key: KeyEvent) -> Option<Command> {
        let pressed = Shortcut::from(key);

        self.bindings
            .iter()
            .find(|binding| binding.shortcuts.contains(&pressed))
            .map(|binding| binding.command)
    }

    pub fn hints(&self) -> impl Iterator<Item = (String, &'static str)> + '_ {
        HINTED.iter().filter_map(|command| {
            let binding = self.bindings.iter().find(|binding| binding.command == *command)?;
            let hint = binding.hint?;
            let key = binding.shortcuts.first()?.display();

            Some((key, hint))
        })
    }

    pub fn palette_entries(&self) -> Vec<PaletteEntry> {
        self.bindings
            .iter()
            .filter(|binding| binding.command != Command::Palette)
            .map(|binding| PaletteEntry {
                target: PaletteTarget::Command(binding.command),
                label: binding.label.to_owned(),
                keys: binding.keys(),
            })
            .collect()
    }
}

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

const fn plain(character: char) -> Shortcut {
    Shortcut::new(KeyCode::Char(character), KeyModifiers::NONE)
}

const fn code(code: KeyCode) -> Shortcut {
    Shortcut::new(code, KeyModifiers::NONE)
}

const fn ctrl(character: char) -> Shortcut {
    Shortcut::new(KeyCode::Char(character), KeyModifiers::CONTROL)
}

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
