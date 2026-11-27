//! Holding the terminal for a TUI, and giving it back however the TUI ends.

use std::io::{self, IsTerminal};
use std::panic;
use std::sync::Arc;
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::mpsc::Sender;
use std::thread;

use hue::{Palette, Query, Slot};
use ratatui::crossterm::cursor::Show;
use ratatui::crossterm::event::{
    DisableBracketedPaste, DisableMouseCapture, EnableBracketedPaste, EnableMouseCapture,
    KeyboardEnhancementFlags, PopKeyboardEnhancementFlags, PushKeyboardEnhancementFlags,
};
use ratatui::crossterm::execute;
use ratatui::crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use ratatui::prelude::CrosstermBackend;
use ratatui::{DefaultTerminal, Frame, Terminal};

use crate::clipboard;
use crate::event::Event;
use crate::reader::Reader;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("not a terminal")]
    NotATerminal,
    #[error(transparent)]
    Io(#[from] io::Error),
}

pub type Result<T> = std::result::Result<T, Error>;

/// The terminal, held in raw mode on the alternate screen with mouse and key
/// reporting on, and its input read on a thread into the host's channel.
#[derive(Debug)]
pub struct Session {
    terminal: DefaultTerminal,
    palette: Palette,
    reader: Reader,
    phase: Arc<Phase>,
}

impl Session {
    pub fn open<E>(slots: &[Slot], sender: Sender<E>) -> Result<Self>
    where
        E: From<Event> + Send + 'static,
    {
        if !io::stdout().is_terminal() {
            return Err(Error::NotATerminal);
        }
        let palette = if slots.is_empty() {
            Palette::default()
        } else {
            Query::default().ask(slots).unwrap_or_default()
        };

        let phase = Phase::armed();
        let terminal =
            match take().and_then(|()| Terminal::new(CrosstermBackend::new(io::stdout()))) {
                Ok(terminal) => terminal,
                Err(error) => {
                    let _ = phase.restore();
                    return Err(error.into());
                }
            };

        Ok(Self { terminal, palette, reader: Reader::start(sender), phase })
    }

    pub const fn palette(&self) -> &Palette {
        &self.palette
    }

    pub fn draw(&mut self, render: impl FnOnce(&mut Frame)) -> Result<()> {
        self.terminal.draw(render)?;

        Ok(())
    }

    pub fn copy(&mut self, text: &str) -> Result<()> {
        clipboard::copy(self.terminal.backend_mut(), text)?;

        Ok(())
    }

    pub fn suspended<T>(&mut self, work: impl FnOnce() -> T) -> Result<T> {
        self.reader.pause();
        if let Err(error) = self.phase.suspend() {
            let _ = self.phase.resume();
            self.reader.resume();
            return Err(error.into());
        }

        let outcome = work();

        let taken = self.phase.resume().and_then(|()| self.terminal.clear());
        self.reader.resume();
        taken?;

        Ok(outcome)
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        self.reader.stop();
        let _ = self.phase.restore();
    }
}

/// Whether the terminal is held, let go for a while, or given back for good.
/// The panic hook shares it with the session, so whichever runs first restores
/// the terminal and the other finds nothing left to do.
#[derive(Debug, Default)]
struct Phase(AtomicU8);

impl Phase {
    const HELD: u8 = 0;
    const SUSPENDED: u8 = 1;
    const RESTORED: u8 = 2;

    fn armed() -> Arc<Self> {
        let phase = Arc::new(Self::default());
        let shared = Arc::clone(&phase);
        let opener = thread::current().id();
        let previous = panic::take_hook();
        panic::set_hook(Box::new(move |info| {
            if thread::current().id() == opener {
                let _ = shared.restore();
            }
            previous(info);
        }));

        phase
    }

    fn suspend(&self) -> io::Result<()> {
        match self.0.compare_exchange(
            Self::HELD,
            Self::SUSPENDED,
            Ordering::AcqRel,
            Ordering::Acquire,
        ) {
            Ok(_) => release(),
            Err(_) => Ok(()),
        }
    }

    fn resume(&self) -> io::Result<()> {
        match self.0.compare_exchange(
            Self::SUSPENDED,
            Self::HELD,
            Ordering::AcqRel,
            Ordering::Acquire,
        ) {
            Ok(_) => take(),
            Err(_) => Ok(()),
        }
    }

    fn restore(&self) -> io::Result<()> {
        match self.0.swap(Self::RESTORED, Ordering::AcqRel) {
            Self::HELD => release(),
            _ => Ok(()),
        }
    }
}

fn take() -> io::Result<()> {
    enable_raw_mode()?;
    execute!(
        io::stdout(),
        EnterAlternateScreen,
        PushKeyboardEnhancementFlags(KeyboardEnhancementFlags::DISAMBIGUATE_ESCAPE_CODES),
        EnableMouseCapture,
        EnableBracketedPaste
    )
}

fn release() -> io::Result<()> {
    let mut stdout = io::stdout();
    let steps = [
        execute!(stdout, DisableBracketedPaste),
        execute!(stdout, DisableMouseCapture),
        execute!(stdout, PopKeyboardEnhancementFlags),
        execute!(stdout, LeaveAlternateScreen),
        execute!(stdout, Show),
        disable_raw_mode(),
    ];

    steps.into_iter().find_map(io::Result::err).map_or(Ok(()), Err)
}
