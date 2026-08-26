//! The Quorum binary.

use std::path::PathBuf;
use std::process::ExitCode;

use clap::Parser;
use quorum::diff::{FileDiff, Source};
use quorum::ui::{Control, Mode, Result, Viewer};
use ratatui::crossterm::event::{self, Event, KeyEventKind};

#[derive(Debug, Parser)]
#[command(version, about = "A review surface for parallel coding agents.")]
struct Arguments {
    old: PathBuf,
    new: PathBuf,
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
    let old = Source::new(std::fs::read_to_string(&arguments.old)?);
    let new = Source::new(std::fs::read_to_string(&arguments.new)?);
    let diff = FileDiff::compute(old, new);

    let mode = if arguments.unified { Mode::Unified } else { Mode::SideBySide };
    let mut viewer = Viewer::new(diff, &arguments.new, mode, arguments.wrap, arguments.tab_width)?;

    ratatui::run(|terminal| {
        loop {
            terminal.draw(|frame| viewer.draw(frame))?;
            if let Event::Key(key) = event::read()?
                && key.kind == KeyEventKind::Press
                && viewer.key(key) == Control::Quit
            {
                return Ok(());
            }
        }
    })
}
