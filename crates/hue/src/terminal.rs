//! Asking the terminal for the colours of its own theme.

use std::fmt::Write as _;
use std::fs::{File, OpenOptions};
use std::io::{self, ErrorKind, Read, Write};
use std::time::{Duration, Instant};

use rustix::termios::{
    OptionalActions, QueueSelector, SpecialCodeIndex, Termios, tcflush, tcgetattr, tcsetattr,
};

use crate::palette::{Palette, Slot};

const ATTRIBUTES: &str = "\x1b[c";
const READ_DECISECONDS: u8 = 1;
const DRAIN: Duration = Duration::from_millis(100);

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("TERM is unset, so the terminal is unknown")]
    UnknownTerminal,
    #[error("TERM={0} is known not to answer colour queries")]
    Unsupported(String),
    #[error(transparent)]
    Io(#[from] io::Error),
}

pub type Result<T> = std::result::Result<T, Error>;

/// One exchange with the terminal, and how long to wait for its replies.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Query {
    pub timeout: Duration,
}

impl Default for Query {
    fn default() -> Self {
        Self { timeout: Duration::from_millis(500) }
    }
}

impl Query {
    #[expect(
        clippy::missing_errors_doc,
        reason = "the Error enum names the failures, an unknown or unanswering terminal and the tty itself"
    )]
    pub fn ask(&self, slots: &[Slot]) -> Result<Palette> {
        supported()?;
        let terminal = Raw::open()?;
        terminal.write(&request(slots))?;

        let mut replies = Vec::new();
        terminal.read_until(&mut replies, Instant::now() + self.timeout)?;
        if !answered(&replies) {
            terminal.read_until(&mut replies, Instant::now() + DRAIN)?;
            terminal.discard_unread()?;
        }

        Ok(Palette::from_replies(&replies))
    }
}

/// The terminal, put into raw mode with a short read timeout until dropped.
struct Raw {
    terminal: File,
    saved: Termios,
}

impl Raw {
    fn open() -> io::Result<Self> {
        let terminal = OpenOptions::new().read(true).write(true).open("/dev/tty")?;
        let saved = tcgetattr(&terminal)?;
        let mut raw = saved.clone();
        raw.make_raw();
        raw.special_codes[SpecialCodeIndex::VMIN] = 0;
        raw.special_codes[SpecialCodeIndex::VTIME] = READ_DECISECONDS;
        tcsetattr(&terminal, OptionalActions::Now, &raw)?;

        Ok(Self { terminal, saved })
    }

    fn write(&self, request: &str) -> io::Result<()> {
        let mut terminal = &self.terminal;
        terminal.write_all(request.as_bytes())?;
        terminal.flush()
    }

    fn read_until(&self, replies: &mut Vec<u8>, deadline: Instant) -> io::Result<()> {
        let mut terminal = &self.terminal;
        let mut chunk = [0; 256];
        while !answered(replies) && Instant::now() < deadline {
            match terminal.read(&mut chunk) {
                Ok(read) => replies.extend_from_slice(&chunk[..read]),
                Err(error) if error.kind() == ErrorKind::Interrupted => {}
                Err(error) => return Err(error),
            }
        }

        Ok(())
    }

    fn discard_unread(&self) -> io::Result<()> {
        Ok(tcflush(&self.terminal, QueueSelector::IFlush)?)
    }
}

impl Drop for Raw {
    fn drop(&mut self) {
        let _ = tcsetattr(&self.terminal, OptionalActions::Now, &self.saved);
    }
}

fn supported() -> Result<()> {
    let terminal = std::env::var_os("TERM").ok_or(Error::UnknownTerminal)?;
    let terminal = terminal.to_string_lossy();
    let unanswering = terminal == "dumb"
        || terminal == "screen"
        || terminal.starts_with("screen.")
        || terminal == "Eterm";

    if unanswering { Err(Error::Unsupported(terminal.into_owned())) } else { Ok(()) }
}

fn request(slots: &[Slot]) -> String {
    let mut request = String::new();
    for slot in slots {
        let written = match slot {
            Slot::Foreground => write!(request, "\x1b]10;?\x07"),
            Slot::Background => write!(request, "\x1b]11;?\x07"),
            Slot::Cursor => write!(request, "\x1b]12;?\x07"),
            Slot::Ansi(color) => write!(request, "\x1b]4;{};?\x07", *color as usize),
        };
        written.expect("writing to a String cannot fail");
    }
    request.push_str(ATTRIBUTES);

    request
}

fn answered(replies: &[u8]) -> bool {
    replies.windows(3).enumerate().any(|(start, window)| {
        window == b"\x1b[?"
            && replies[start + 3..]
                .iter()
                .find(|byte| !(byte.is_ascii_digit() || **byte == b';'))
                .is_some_and(|byte| *byte == b'c')
    })
}
