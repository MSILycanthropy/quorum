//! The Quorum binary.
#![expect(
    clippy::result_large_err,
    reason = "gix builds large error types, and every call here is already bound by disk"
)]

use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::mpsc::{Sender, channel};

use clap::Parser;
use quorum::agent::{Consequence, Spawn, Update};
use quorum::diff::{FileDiff, Source};
use quorum::git::{Content, FileChange, Repository, Status};
use quorum::ui::{Control, Mode, Opening};
use ratatui::crossterm::event::{
    self, DisableMouseCapture, EnableMouseCapture, KeyEvent, KeyEventKind, MouseEvent,
};
use ratatui::crossterm::execute;

#[derive(Debug, thiserror::Error)]
enum Error {
    #[error(transparent)]
    Agent(#[from] quorum::agent::Error),
    #[error(transparent)]
    Git(#[from] quorum::git::Error),
    #[error(transparent)]
    Ui(#[from] quorum::ui::Error),
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, Parser)]
#[command(version, about = "A review surface for parallel coding agents.")]
struct Arguments {
    #[arg(num_args = 0..=2)]
    paths: Vec<PathBuf>,
    #[arg(long)]
    revision: Option<String>,
    #[arg(long)]
    agent: Option<String>,
    #[arg(long)]
    wrap: bool,
    #[arg(long)]
    unified: bool,
    #[arg(long, default_value_t = 4)]
    tab_width: usize,
}

fn main() -> ExitCode {
    match view(&Arguments::parse()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("quorum: {error}");
            ExitCode::FAILURE
        }
    }
}

/// Everything the loop wakes for, from the terminal or from an agent.
enum Event {
    Key(KeyEvent),
    Mouse(MouseEvent),
    Redraw,
    Agent(Update),
}

fn view(arguments: &Arguments) -> Result<()> {
    let subject = Subject::open(arguments)?;
    let mode = if arguments.unified { Mode::Unified } else { Mode::SideBySide };
    let (sender, events) = channel();

    let session = match subject.directory() {
        Some(directory) => {
            let spawn = Spawn {
                command: arguments.agent.clone().unwrap_or(Spawn::default().command),
                directory: directory.to_path_buf(),
            };

            Some(spawn.session(forward(sender.clone()))?)
        }
        None => None,
    };
    let opening = Opening {
        mode,
        wrap: arguments.wrap,
        tab_width: arguments.tab_width,
        branch: subject.branch()?,
        base: subject.base(),
    };
    let mut tab = opening.tab(subject.changes()?, session)?;
    watch(sender);

    ratatui::run(|terminal| -> Result<()> {
        let _reporting = Reporting::on()?;
        terminal.draw(|frame| tab.draw(frame))?;

        while let Ok(event) = events.recv() {
            match event {
                Event::Key(key) => {
                    if tab.key(key)? == Control::Quit {
                        return Ok(());
                    }
                }
                Event::Mouse(mouse) => tab.mouse(mouse)?,
                Event::Agent(update) => {
                    if tab.apply(update) == Consequence::Rescan {
                        tab.refresh(subject.changes()?)?;
                    }
                }
                Event::Redraw => {}
            }
            terminal.draw(|frame| tab.draw(frame))?;
        }

        Ok(())
    })
}

fn watch(sender: Sender<Event>) {
    spawn("terminal", move || {
        while let Ok(event) = event::read() {
            let event = match event {
                ratatui::crossterm::event::Event::Key(key) if key.kind == KeyEventKind::Press => {
                    Event::Key(key)
                }
                ratatui::crossterm::event::Event::Mouse(mouse) => Event::Mouse(mouse),
                ratatui::crossterm::event::Event::Resize(_, _) => Event::Redraw,
                _ => continue,
            };
            if sender.send(event).is_err() {
                return;
            }
        }
    });
}

fn forward(sender: Sender<Event>) -> Sender<Update> {
    let (updates, incoming) = channel();
    spawn("agent-events", move || {
        while let Ok(update) = incoming.recv() {
            if sender.send(Event::Agent(update)).is_err() {
                return;
            }
        }
    });

    updates
}

fn spawn(name: &str, work: impl FnOnce() + Send + 'static) {
    std::thread::Builder::new()
        .name(name.to_owned())
        .spawn(work)
        .expect("a reader thread cannot fail to start");
}

/// Mouse reporting, switched back off when this is dropped, so an unwind
/// cannot leave the terminal capturing.
struct Reporting;

impl Reporting {
    fn on() -> Result<Self> {
        execute!(std::io::stdout(), EnableMouseCapture)?;

        Ok(Self)
    }
}

impl Drop for Reporting {
    fn drop(&mut self) {
        let _ = execute!(std::io::stdout(), DisableMouseCapture);
    }
}

/// What Quorum was pointed at, kept open so the diff can be read again.
enum Subject {
    Worktree { repository: Repository, against: String },
    File { repository: Repository, against: String, path: PathBuf },
    Pair { old: PathBuf, new: PathBuf },
}

impl Subject {
    fn open(arguments: &Arguments) -> Result<Self> {
        match arguments.paths.as_slice() {
            [] => {
                let repository = Repository::discover(Path::new("."))?;
                let against = against(arguments, &repository)?;

                Ok(Self::Worktree { repository, against })
            }
            [path] => {
                let repository = Repository::discover(path)?;
                let against = against(arguments, &repository)?;

                Ok(Self::File { repository, against, path: path.clone() })
            }
            [old, new] => Ok(Self::Pair { old: old.clone(), new: new.clone() }),
            _ => panic!("clap accepts at most two paths"),
        }
    }

    fn directory(&self) -> Option<&Path> {
        match self {
            Self::Worktree { repository, .. } | Self::File { repository, .. } => {
                Some(repository.workdir())
            }
            Self::Pair { .. } => None,
        }
    }

    fn branch(&self) -> Result<Option<String>> {
        match self {
            Self::Worktree { repository, .. } | Self::File { repository, .. } => {
                Ok(repository.branch()?)
            }
            Self::Pair { .. } => Ok(None),
        }
    }

    fn base(&self) -> String {
        match self {
            Self::Worktree { against, .. } | Self::File { against, .. } => against.clone(),
            Self::Pair { old, .. } => old.display().to_string(),
        }
    }

    fn changes(&self) -> Result<Vec<FileChange>> {
        match self {
            Self::Worktree { repository, against } => Ok(repository.changes(against)?),
            Self::File { repository, against, path } => Ok(vec![repository.change(against, path)?]),
            Self::Pair { old, new } => {
                let diff = FileDiff::compute(
                    Source::new(std::fs::read_to_string(old)?),
                    Source::new(std::fs::read_to_string(new)?),
                );

                Ok(vec![FileChange {
                    status: Status::Modified,
                    path: new.clone(),
                    content: Content::Text(diff),
                }])
            }
        }
    }
}

fn against(arguments: &Arguments, repository: &Repository) -> Result<String> {
    match &arguments.revision {
        Some(revision) => Ok(revision.clone()),
        None => Ok(repository.base()?),
    }
}
