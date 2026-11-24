//! Syntax highlighting for a snapshot, as coloured byte spans per line.

use std::ffi::OsStr;
use std::ops::Range;
use std::path::Path;
use std::sync::LazyLock;

use inkjet::Language;
use inkjet::constants::HIGHLIGHT_NAMES;
use inkjet::theme::{Theme, vendored};
use inkjet::tree_sitter_highlight::{self, HighlightConfiguration, HighlightEvent, Highlighter};

use crate::color::Rgb;
use crate::diff::{Highlight, Highlighting, Source, Span};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("could not highlight {path}: {source}")]
    Parse {
        path: String,
        #[source]
        source: tree_sitter_highlight::Error,
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

/// The largest source, in bytes, that is parsed whole even when only some of
/// its lines are wanted. A larger one is parsed a stretch at a time, each
/// stretch on its own, unless most of its lines are wanted.
const WHOLE: usize = 64 * 1024;

impl Syntax {
    #[expect(
        clippy::missing_errors_doc,
        reason = "the Error enum names the only failure, which is a grammar rejecting the source"
    )]
    pub fn highlight(&self, source: &Source, path: &Path) -> Result<Highlighting> {
        self.stretches(source, path, std::slice::from_ref(&(0..source.len())))
    }

    #[expect(
        clippy::missing_errors_doc,
        reason = "the Error enum names the only failure, which is a grammar rejecting the source"
    )]
    pub fn highlight_lines(
        &self,
        source: &Source,
        path: &Path,
        lines: &[Range<usize>],
    ) -> Result<Highlighting> {
        let wanted: usize = lines.iter().map(Range::len).sum();
        if source.text().len() <= WHOLE || 2 * wanted > source.len() {
            self.highlight(source, path)
        } else {
            self.stretches(source, path, lines)
        }
    }

    fn stretches(
        &self,
        source: &Source,
        path: &Path,
        stretches: &[Range<usize>],
    ) -> Result<Highlighting> {
        let grammar = grammar(source, path);
        let mut highlighter = Highlighter::new();
        let mut coloured = vec![Vec::new(); source.len()];

        let enclosing = enclosing(source, stretches);
        for (stretch, enclosing) in stretches.iter().zip(&enclosing) {
            if stretch.is_empty() {
                continue;
            }
            let mut text = String::new();
            for line in enclosing {
                text.push_str(source.line(*line));
                text.push('\n');
            }
            text.push_str(source.slice(stretch.clone()));

            let runs = match grammar {
                Some(grammar) => self.runs(&mut highlighter, &text, grammar).map_err(|error| {
                    Error::Parse { path: path.display().to_string(), source: error }
                })?,
                None => vec![(0..text.len(), self.foreground)],
            };
            let lines = enclosing.iter().copied().chain(stretch.clone());
            let split = split_into_lines(&text, lines.map(|line| source.line(line)), &runs);
            for (line, highlights) in stretch.clone().zip(split.into_iter().skip(enclosing.len())) {
                coloured[line] = highlights;
            }
        }

        Ok(Highlighting::new(coloured))
    }

    fn runs(
        &self,
        highlighter: &mut Highlighter,
        text: &str,
        grammar: &HighlightConfiguration,
    ) -> std::result::Result<Vec<(Range<usize>, Rgb)>, tree_sitter_highlight::Error> {
        let mut stack: Vec<Rgb> = Vec::new();
        let mut runs: Vec<(Range<usize>, Rgb)> = Vec::new();
        let events = highlighter.highlight(grammar, text.as_bytes(), None, |token| {
            from_token(token).map(|language| language.config())
        })?;

        for event in events {
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

static EMBEDDED_RUBY: LazyLock<HighlightConfiguration> = LazyLock::new(|| {
    let mut grammar = HighlightConfiguration::new(
        tree_sitter_embedded_template::LANGUAGE.into(),
        "erb",
        tree_sitter_embedded_template::HIGHLIGHTS_QUERY,
        tree_sitter_embedded_template::INJECTIONS_ERB_QUERY,
        "",
    )
    .expect("the bundled embedded template queries compile");
    grammar.configure(HIGHLIGHT_NAMES);
    grammar
});

fn grammar(source: &Source, path: &Path) -> Option<&'static HighlightConfiguration> {
    let lowercase = |part: Option<&OsStr>| part.and_then(OsStr::to_str).map(str::to_lowercase);

    lowercase(path.extension())
        .and_then(|token| grammar_for(&token))
        .or_else(|| lowercase(path.file_name()).and_then(|token| grammar_for(&token)))
        .or_else(|| interpreter(source).and_then(grammar_for))
}

fn grammar_for(token: &str) -> Option<&'static HighlightConfiguration> {
    match token {
        "erb" | "rhtml" => Some(&EMBEDDED_RUBY),
        other => from_token(other).map(|language| language.config()),
    }
}

fn from_token(token: &str) -> Option<Language> {
    let canonical = match token {
        "yml" => "yaml",
        "rake" | "gemspec" | "ru" | "gemfile" | "rakefile" => "ruby",
        "mjs" | "cjs" => "javascript",
        "zsh" => "bash",
        other => other,
    };

    Language::from_token(canonical)
}

fn interpreter(source: &Source) -> Option<&str> {
    let first = source.lines().next()?.strip_prefix("#!")?;
    let mut words = first.split_whitespace();
    let program = words.next()?.rsplit('/').next()?;
    let program =
        if program == "env" { words.find(|word| !word.starts_with('-'))? } else { program };

    Some(program.trim_end_matches(|character: char| character.is_ascii_digit() || character == '.'))
}

fn enclosing(source: &Source, stretches: &[Range<usize>]) -> Vec<Vec<usize>> {
    let mut starts: Vec<(usize, usize)> =
        stretches.iter().enumerate().map(|(index, stretch)| (stretch.start, index)).collect();
    starts.sort_unstable();
    let mut starts = starts.into_iter().peekable();

    let mut enclosing = vec![Vec::new(); stretches.len()];
    let mut open: Vec<(usize, usize)> = Vec::new();
    for (line, text) in source.lines().enumerate() {
        if starts.peek().is_none() {
            break;
        }
        let indentation = (!text.trim().is_empty()).then(|| text.len() - text.trim_start().len());
        if let Some(indentation) = indentation {
            while open.last().is_some_and(|(depth, _)| *depth >= indentation) {
                open.pop();
            }
        }
        while let Some((_, stretch)) = starts.next_if(|(start, _)| *start == line) {
            enclosing[stretch] = open.iter().map(|(_, line)| *line).collect();
        }
        if let Some(indentation) = indentation {
            open.push((indentation, line));
        }
    }

    enclosing
}

fn split_into_lines<'a>(
    text: &str,
    lines: impl Iterator<Item = &'a str>,
    runs: &[(Range<usize>, Rgb)],
) -> Vec<Vec<Highlight>> {
    let text = text.as_bytes();
    let mut split = Vec::new();
    let mut runs = runs.iter().peekable();
    let mut start = 0;

    for line in lines {
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
        split.push(highlights);

        start = end;
        if text.get(start) == Some(&b'\r') {
            start += 1;
        }
        start += 1;
    }

    split
}

fn rgb(color: inkjet::theme::Color) -> Rgb {
    Rgb::new(color.r, color.g, color.b)
}
