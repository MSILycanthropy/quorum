//! What a person sets for every diff Sift shows, from `config.toml` or a flag of the same name.

use keys::Rebindings;
use serde::{Deserialize, Serialize};

use crate::diff::{Emphasis, Whitespace};
use crate::theme::Scheme;
use crate::ui::{Indicators, Mode, Presentation};

/// Sift's settings, each one a key in `config.toml` and a flag of the same
/// name. A key left unset falls back to the layer under it, and then to the
/// default.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash, Deserialize, Serialize, clap::Args)]
#[serde(default, deny_unknown_fields, rename_all = "kebab-case")]
pub struct Config {
    #[arg(long, value_enum, help = "Side by side, or one column")]
    pub layout: Option<Mode>,
    #[arg(
        long,
        num_args = 0..=1,
        require_equals = true,
        default_missing_value = "true",
        value_name = "BOOL",
        help = "Wrap long lines, or --wrap=false to cut them off at the edge"
    )]
    pub wrap: Option<bool>,
    #[arg(long, value_name = "COLUMNS", help = "Columns a tab expands to")]
    pub tab_width: Option<usize>,
    #[arg(long, value_enum, help = "What marks a changed line beside its number")]
    pub indicators: Option<Indicators>,
    #[arg(
        long,
        num_args = 0..=1,
        require_equals = true,
        default_missing_value = "true",
        value_name = "BOOL",
        help = "Tint changed lines, or --backgrounds=false to leave them on the plain background"
    )]
    pub backgrounds: Option<bool>,
    #[arg(
        long,
        num_args = 0..=1,
        require_equals = true,
        default_missing_value = "true",
        value_name = "BOOL",
        help = "Number each line, or --line-numbers=false to give the columns to the code"
    )]
    pub line_numbers: Option<bool>,
    #[arg(
        long,
        value_enum,
        help = "Whether lines that differ only in whitespace count as changed"
    )]
    pub whitespace: Option<Whitespace>,
    #[arg(
        long,
        value_name = "LINES",
        help = "Unchanged lines kept around a change before the rest fold"
    )]
    pub context: Option<usize>,
    #[arg(long, value_enum, help = "What within a changed line is picked out from the rest of it")]
    pub emphasis: Option<Emphasis>,
    #[arg(long, value_enum, help = "Where the background and diff colours come from")]
    pub theme: Option<Scheme>,
    #[arg(
        long,
        value_name = "NAME|FILE",
        help = "A Helix theme for the code: a name from ~/.config/helix/themes or bundled, or a file"
    )]
    pub syntax_theme: Option<String>,
    #[arg(skip)]
    #[serde(skip_serializing_if = "Rebindings::is_empty")]
    pub keys: Rebindings,
}

impl Config {
    pub fn presentation(&self) -> Presentation {
        let default = Presentation::default();

        Presentation {
            mode: self.layout.unwrap_or(default.mode),
            wrap: self.wrap.unwrap_or(default.wrap),
            tab_width: self.tab_width.unwrap_or(default.tab_width),
            indicators: self.indicators.unwrap_or(default.indicators),
            backgrounds: self.backgrounds.unwrap_or(default.backgrounds),
            line_numbers: self.line_numbers.unwrap_or(default.line_numbers),
            whitespace: self.whitespace.unwrap_or(default.whitespace),
            context: self.context.unwrap_or(default.context),
            emphasis: self.emphasis.unwrap_or(default.emphasis),
        }
    }
}
