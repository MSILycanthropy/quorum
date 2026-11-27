//! Sift's own frame around the diff: its keys beside the diff's, the rows its palette offers, and its status line.

use std::path::PathBuf;

use keys::{Keymap, Rebindings, keymap};
use picker::Entry;

use crate::command::{self, ANNOTATIONS, Command};
use crate::ui::Icon;

/// What a key does in Sift: one of the diff's commands, or one of Sift's own.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Action {
    Diff(Command),
    Palette,
    Settings,
    Reload,
    Quit,
    FindFile,
    GoToLine,
    Edit,
}

/// What choosing a row of Sift's palette does.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Target {
    Action(Action),
    Line(usize),
    File(PathBuf),
}

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Keys(#[from] keys::Error),
    #[error("{0} needs a key")]
    Required(&'static str),
}

pub type Result<T> = std::result::Result<T, Error>;

/// The sections Sift's keys are listed under, in order.
pub const SECTIONS: [&str; 4] = ["General", "Files", "Diff", "View"];

/// The commands the footer names, most wanted first.
pub const HINTED: [&str; 9] = [
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

const QUITTING: [&str; 6] = ["q!", "qa", "qa!", "wq", "wqa", "x"];

pub fn keymap() -> Result<Keymap<Action>> {
    use Action as A;

    let own = keymap! {
        "General" {
            "palette" => A::Palette, "Command palette", [":", "C-t"];
            "settings" => A::Settings, "Settings and keys", ["C-,"], hint: "keys", pinned: true;
            "reload" => A::Reload, "Reload from git", ["r", "F5"];
            "quit" => A::Quit, "Quit", ["q", "ZZ", "ZQ", "C-c"], hint: "quit", pinned: true;
            "back" => A::Quit, "Quit, once nothing is selected or open", ["Esc"];
        }
        "Files" {
            "find-file" => A::FindFile, "Find file", ["/", "C-f", "C-p"], hint: "find";
            "edit" => A::Edit, "Open in $EDITOR", ["e"], hint: "edit";
        }
        "Diff" {
            "go-to-line" => A::GoToLine, "Go to a line", ["C-g"];
        }
    }?;
    let diff = command::keymap().retain(|binding| binding.section != ANNOTATIONS).map(A::Diff);

    Ok(diff.joined(own)?.arranged(&SECTIONS))
}

pub fn rebound(keymap: &Keymap<Action>, keys: &Rebindings) -> Result<Keymap<Action>> {
    let keymap = keymap.clone().rebound(keys)?;
    let keyless =
        keymap.bindings().iter().find(|binding| binding.name == "quit" && binding.keys.is_empty());
    if let Some(binding) = keyless {
        return Err(Error::Required(binding.label));
    }

    Ok(keymap)
}

pub fn commands(keymap: &Keymap<Action>, holding: &[&str]) -> Vec<Entry<Target>> {
    keymap
        .bindings()
        .iter()
        .filter(|binding| binding.command != Action::Palette)
        .filter_map(|binding| Entry::of(binding, holding))
        .map(|entry| entry.map(Target::Action))
        .collect()
}

pub fn files(changed: Vec<PathBuf>, known: &[PathBuf]) -> Vec<Entry<Target>> {
    let unchanged: Vec<PathBuf> =
        known.iter().filter(|path| !changed.contains(path)).cloned().collect();

    changed
        .into_iter()
        .chain(unchanged)
        .map(|path| {
            let label = path.display().to_string();
            Entry::new(Target::File(path), label)
        })
        .collect()
}

pub fn ex(query: &str) -> Option<Entry<Target>> {
    let query = query.trim();
    if let Ok(line) = query.parse::<usize>()
        && line > 0
    {
        return Some(Entry::new(Target::Line(line), format!("Go to line {line}")));
    }

    QUITTING.contains(&query).then(|| Entry::new(Target::Action(Action::Quit), "Quit"))
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

pub fn leading(query: &str, entries: &[Entry<Target>]) -> Vec<Entry<Target>> {
    let commands = entries.iter().any(|entry| matches!(entry.target, Target::Action(_)));

    ex(query).filter(|_| commands).into_iter().collect()
}

pub fn status(branch: Option<&str>, comparing: &str, review: &str) -> String {
    let branch = format!("{} {}", Icon::BRANCH, branch.unwrap_or("detached"));

    [branch.as_str(), comparing, review]
        .into_iter()
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join(" · ")
}
