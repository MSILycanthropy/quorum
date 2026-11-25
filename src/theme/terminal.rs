//! Asking the terminal for the colours of its own theme.

use std::fs::OpenOptions;
use std::io::{Read, Write};
use std::time::{Duration, Instant};

use ratatui::crossterm::terminal::{disable_raw_mode, enable_raw_mode, is_raw_mode_enabled};
use rustix::event::{PollFd, PollFlags, Timespec, poll};
use rustix::io::Errno;

use crate::color::Rgb;
use crate::theme::TerminalColors;

const ESCAPE: u8 = 0x1b;
const BELL: u8 = 0x07;

impl TerminalColors {
    pub const QUERY: &str = concat!(
        "\x1b]10;?\x1b\\",
        "\x1b]11;?\x1b\\",
        "\x1b]4;1;?\x1b\\",
        "\x1b]4;2;?\x1b\\",
        "\x1b]4;3;?\x1b\\",
        "\x1b[c",
    );

    pub fn query(timeout: Duration) -> Option<Self> {
        let raw = is_raw_mode_enabled().ok()?;
        if !raw {
            enable_raw_mode().ok()?;
        }
        let replies = exchange(timeout);
        if !raw {
            let _ = disable_raw_mode();
        }

        Self::from_replies(&replies?)
    }

    pub fn from_replies(replies: &[u8]) -> Option<Self> {
        let (mut background, mut foreground, mut red, mut green, mut yellow) =
            (None, None, None, None, None);

        for reply in operating_system_commands(replies) {
            let Some((slot, color)) =
                std::str::from_utf8(reply).ok().and_then(|reply| reply.rsplit_once(';'))
            else {
                continue;
            };
            let color = parse(color);
            match slot {
                "10" => foreground = color,
                "11" => background = color,
                "4;1" => red = color,
                "4;2" => green = color,
                "4;3" => yellow = color,
                _ => {}
            }
        }

        Some(Self {
            background: background?,
            foreground: foreground?,
            red: red?,
            green: green?,
            yellow: yellow?,
        })
    }
}

fn exchange(timeout: Duration) -> Option<Vec<u8>> {
    let mut terminal = OpenOptions::new().read(true).write(true).open("/dev/tty").ok()?;
    terminal.write_all(TerminalColors::QUERY.as_bytes()).ok()?;
    terminal.flush().ok()?;

    let deadline = Instant::now() + timeout;
    let mut replies = Vec::new();
    let mut chunk = [0; 256];
    while !answered(&replies) {
        let left = Timespec::try_from(deadline.checked_duration_since(Instant::now())?).ok()?;
        let mut waiting = [PollFd::new(&terminal, PollFlags::IN)];
        match poll(&mut waiting, Some(&left)) {
            Ok(_) if waiting[0].revents().contains(PollFlags::IN) => {}
            Err(Errno::INTR) => continue,
            Ok(_) | Err(_) => return None,
        }
        match terminal.read(&mut chunk) {
            Ok(0) | Err(_) => break,
            Ok(read) => replies.extend_from_slice(&chunk[..read]),
        }
    }

    Some(replies)
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

fn operating_system_commands(replies: &[u8]) -> impl Iterator<Item = &[u8]> {
    let mut rest = replies;
    std::iter::from_fn(move || {
        let start = rest.windows(2).position(|window| window == [ESCAPE, b']'])? + 2;
        let body = &rest[start..];
        let end = body.iter().position(|byte| *byte == BELL || *byte == ESCAPE)?;
        rest = &body[end..];

        Some(&body[..end])
    })
}

fn parse(color: &str) -> Option<Rgb> {
    let mut channels = color.strip_prefix("rgb:")?.split('/').map(intensity);
    let color = Rgb::new(channels.next()??, channels.next()??, channels.next()??);

    channels.next().is_none().then_some(color)
}

fn intensity(hex: &str) -> Option<u8> {
    let digits = u32::try_from(hex.len()).ok().filter(|digits| (1..=4).contains(digits))?;
    let value = u32::from_str_radix(hex, 16).ok()?;
    let most = 16u32.pow(digits) - 1;

    u8::try_from((value * 255 + most / 2) / most).ok()
}
