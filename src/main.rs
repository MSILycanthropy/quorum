//! The Quorum binary.
#![expect(
    clippy::result_large_err,
    reason = "gix builds large error types, and every call here is already bound by disk"
)]

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use clap::Parser;
use quorum::diff::{FileDiff, Source};
use quorum::git::{Content, FileChange, Repository, Status};
use quorum::ui::{Control, Mode, Review};
use ratatui::crossterm::event::{
    self, DisableMouseCapture, EnableMouseCapture, Event, KeyEventKind,
};
use ratatui::crossterm::execute;

#[derive(Debug, thiserror::Error)]
enum Error {
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

fn view(arguments: &Arguments) -> Result<()> {
    let (changes, against) = changes(arguments)?;
    if changes.is_empty() {
        println!("quorum: no changes against {against}");
        return Ok(());
    }

    let mode = if arguments.unified { Mode::Unified } else { Mode::SideBySide };
    let mut review = Review::new(changes, mode, arguments.wrap, arguments.tab_width)?;

    ratatui::run(|terminal| -> Result<()> {
        let _reporting = Reporting::on()?;
        loop {
            terminal.draw(|frame| review.draw(frame))?;
            match event::read()? {
                Event::Key(key) if key.kind == KeyEventKind::Press => {
                    if review.key(key)? == Control::Quit {
                        return Ok(());
                    }
                }
                Event::Mouse(mouse) => review.mouse(mouse)?,
                _ => {}
            }
        }
    })
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

fn changes(arguments: &Arguments) -> Result<(Vec<FileChange>, String)> {
    match arguments.paths.as_slice() {
        [] => {
            let repository = Repository::discover(Path::new("."))?;
            let against = against(arguments, &repository)?;
            let changes = repository.changes(&against)?;

            Ok((changes, against))
        }
        [path] => {
            let repository = Repository::discover(path)?;
            let against = against(arguments, &repository)?;
            let change = repository.change(&against, path)?;

            Ok((vec![change], against))
        }
        [old, new] => {
            let diff = FileDiff::compute(
                Source::new(std::fs::read_to_string(old)?),
                Source::new(std::fs::read_to_string(new)?),
            );
            let change = FileChange {
                status: Status::Modified,
                path: new.clone(),
                content: Content::Text(diff),
            };

            Ok((vec![change], old.display().to_string()))
        }
        _ => panic!("clap accepts at most two paths"),
    }
}

fn against(arguments: &Arguments, repository: &Repository) -> Result<String> {
    match &arguments.revision {
        Some(revision) => Ok(revision.clone()),
        None => Ok(repository.base()?),
    }
}
