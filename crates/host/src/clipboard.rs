//! Putting text on the system clipboard, through the terminal and, on macOS, pbcopy.

use std::io::{self, Write};
use std::process::{Command, Stdio};

const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

pub(crate) fn copy(terminal: &mut impl Write, text: &str) -> io::Result<()> {
    write!(terminal, "\x1b]52;c;{}\x07", base64(text.as_bytes()))?;
    terminal.flush()?;

    if cfg!(target_os = "macos")
        && let Ok(mut child) = Command::new("pbcopy").stdin(Stdio::piped()).spawn()
    {
        if let Some(stdin) = child.stdin.as_mut() {
            stdin.write_all(text.as_bytes())?;
        }
        child.wait()?;
    }

    Ok(())
}

fn base64(bytes: &[u8]) -> String {
    let mut encoded = String::with_capacity(bytes.len().div_ceil(3) * 4);

    for chunk in bytes.chunks(3) {
        let triple = chunk
            .iter()
            .enumerate()
            .fold(0u32, |triple, (index, byte)| triple | u32::from(*byte) << (16 - 8 * index));
        for position in 0..4 {
            if position <= chunk.len() {
                let index = (triple >> (18 - 6 * position)) & 0x3f;
                encoded.push(char::from(ALPHABET[index as usize]));
            } else {
                encoded.push('=');
            }
        }
    }

    encoded
}
