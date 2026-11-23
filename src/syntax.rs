//! Syntax highlighting for a snapshot, as coloured byte spans per line.

use std::ffi::OsStr;
use std::ops::Range;
use std::path::Path;

use inkjet::constants::HIGHLIGHT_NAMES;
use inkjet::theme::{Theme, vendored};
use inkjet::tree_sitter_highlight::HighlightEvent;
use inkjet::{Highlighter, InkjetError, Language};

use crate::color::Rgb;
use crate::diff::{Highlight, Highlighting, Source, Span};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("could not highlight {path}: {source}")]
    Parse {
        path: String,
        #[source]
        source: InkjetError,
    },
}

pub type Result<T> = std::result::Result<T, Error>;

/// Tokyo Night's colour for every highlight name a grammar can report, by
/// the index tree-sitter reports it under.
pub struct Syntax {
    colors: Vec<Rgb>,
    foreground: Rgb,
}

impl Default for Syntax {
    fn default() -> Self {
        let theme =
            Theme::from_helix(vendored::TOKYONIGHT).expect("the vendored Tokyo Night theme parses");
        let foreground = rgb(theme.fg);
        let colors = HIGHLIGHT_NAMES
            .iter()
            .map(|name| theme.get_style(name).and_then(|style| style.fg).map_or(foreground, rgb))
            .collect();

        Self { colors, foreground }
    }
}

impl Syntax {
    #[expect(
        clippy::missing_errors_doc,
        reason = "the Error enum names the only failure, which is a grammar rejecting the source"
    )]
    pub fn highlight(&self, source: &Source, path: &Path) -> Result<Highlighting> {
        let runs = match language(source, path) {
            Some(language) => self.runs(source, language).map_err(|error| Error::Parse {
                path: path.display().to_string(),
                source: error,
            })?,
            None => vec![(0..source.text().len(), self.foreground)],
        };

        Ok(Highlighting::new(split_into_lines(source, &runs)))
    }

    fn runs(
        &self,
        source: &Source,
        language: Language,
    ) -> inkjet::Result<Vec<(Range<usize>, Rgb)>> {
        let text = source.text();
        let mut highlighter = Highlighter::new();
        let mut stack: Vec<Rgb> = Vec::new();
        let mut runs: Vec<(Range<usize>, Rgb)> = Vec::new();

        for event in highlighter.highlight_raw(language, &text)? {
            match event? {
                HighlightEvent::HighlightStart(highlight) => {
                    stack.push(self.colors.get(highlight.0).copied().unwrap_or(self.foreground));
                }
                HighlightEvent::HighlightEnd => {
                    stack.pop();
                }
                HighlightEvent::Source { start, end } => {
                    let color = stack.last().copied().unwrap_or(self.foreground);
                    match runs.last_mut() {
                        Some((range, last)) if *last == color && range.end == start => {
                            range.end = end;
                        }
                        _ => runs.push((start..end, color)),
                    }
                }
            }
        }

        Ok(runs)
    }
}

fn language(source: &Source, path: &Path) -> Option<Language> {
    let lowercase = |part: Option<&OsStr>| part.and_then(OsStr::to_str).map(str::to_lowercase);

    lowercase(path.extension())
        .and_then(Language::from_token)
        .or_else(|| lowercase(path.file_name()).and_then(Language::from_token))
        .or_else(|| interpreter(source).and_then(Language::from_token))
}

fn interpreter(source: &Source) -> Option<&str> {
    let first = source.lines().next()?.strip_prefix("#!")?;
    let mut words = first.split_whitespace();
    let program = words.next()?.rsplit('/').next()?;
    let program =
        if program == "env" { words.find(|word| !word.starts_with('-'))? } else { program };

    Some(program.trim_end_matches(|character: char| character.is_ascii_digit() || character == '.'))
}

fn split_into_lines(source: &Source, runs: &[(Range<usize>, Rgb)]) -> Vec<Vec<Highlight>> {
    let text = source.text().as_bytes();
    let mut lines = Vec::with_capacity(source.len());
    let mut runs = runs.iter().peekable();
    let mut start = 0;

    for line in source.lines() {
        let end = start + line.len();
        let mut highlights: Vec<Highlight> = Vec::new();
        while runs.next_if(|(range, _)| range.end <= start).is_some() {}
        for (range, color) in runs.clone() {
            if range.start >= end {
                break;
            }
            let span =
                Span { start: range.start.max(start) - start, end: range.end.min(end) - start };
            if span.start < span.end {
                highlights.push(Highlight { span, color: *color });
            }
        }
        lines.push(highlights);

        start = end;
        if text.get(start) == Some(&b'\r') {
            start += 1;
        }
        start += 1;
    }

    lines
}

fn rgb(color: inkjet::theme::Color) -> Rgb {
    Rgb::new(color.r, color.g, color.b)
}
