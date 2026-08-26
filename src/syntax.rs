//! Syntax highlighting for a snapshot, as coloured byte spans per line.

use std::ffi::OsStr;
use std::path::Path;

use two_face::re_exports::syntect::highlighting::{
    HighlightState, Highlighter, RangedHighlightIterator, Theme,
};
use two_face::re_exports::syntect::parsing::{ParseState, ScopeStack, SyntaxSet};

use crate::color::Rgb;
use crate::diff::{Highlight, Highlighting, Source, Span};
use crate::theme::TokyoNight;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("could not highlight {path}: {source}")]
    Parse {
        path: String,
        #[source]
        source: two_face::re_exports::syntect::parsing::ParsingError,
    },
}

pub type Result<T> = std::result::Result<T, Error>;

/// The grammars and theme highlighting runs against.
pub struct Syntax {
    syntaxes: SyntaxSet,
    theme: Theme,
}

impl Default for Syntax {
    fn default() -> Self {
        Self { syntaxes: two_face::syntax::extra_no_newlines(), theme: TokyoNight::theme() }
    }
}

impl Syntax {
    #[expect(
        clippy::missing_errors_doc,
        reason = "the Error enum names the only failure, which is a grammar rejecting a line"
    )]
    pub fn highlight(&self, source: &Source, path: &Path) -> Result<Highlighting> {
        let name = path.file_name().and_then(OsStr::to_str).unwrap_or_default();
        let extension = path.extension().and_then(OsStr::to_str).unwrap_or_default();

        let syntax = self
            .syntaxes
            .find_syntax_by_extension(extension)
            .or_else(|| self.syntaxes.find_syntax_by_extension(name))
            .or_else(|| self.syntaxes.find_syntax_by_first_line(source.text()))
            .unwrap_or_else(|| self.syntaxes.find_syntax_plain_text());

        let highlighter = Highlighter::new(&self.theme);
        let mut parser = ParseState::new(syntax);
        let mut state = HighlightState::new(&highlighter, ScopeStack::new());

        let mut lines = Vec::with_capacity(source.len());
        for line in source.lines() {
            let operations = parser.parse_line(line, &self.syntaxes).map_err(|error| {
                Error::Parse { path: path.display().to_string(), source: error }
            })?;
            lines.push(
                RangedHighlightIterator::new(&mut state, &operations, line, &highlighter)
                    .map(|(style, _, range)| Highlight {
                        span: Span { start: range.start, end: range.end },
                        color: Rgb::new(style.foreground.r, style.foreground.g, style.foreground.b),
                    })
                    .collect(),
            );
        }

        Ok(Highlighting::new(lines))
    }
}
