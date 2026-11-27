//! What a person sets for every diff Sift shows and every key it answers, from `config.toml` or a flag.

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
    #[arg(long, value_enum, help = "Two panes side by side, or one column")]
    pub layout: Option<Mode>,
    #[arg(
        long,
        num_args = 0..=1,
        require_equals = true,
        default_missing_value = "true",
        value_name = "BOOL",
        help = "Wrap long lines instead of cutting them off at the edge"
    )]
    pub wrap: Option<bool>,
    #[arg(long, value_name = "COLUMNS", help = "How many columns a tab takes up")]
    pub tab_width: Option<usize>,
    #[arg(long, value_enum, help = "What marks a changed line next to its number")]
    pub indicators: Option<Indicators>,
    #[arg(
        long,
        num_args = 0..=1,
        require_equals = true,
        default_missing_value = "true",
        value_name = "BOOL",
        help = "Tint the whole of a changed line, not only the words that changed"
    )]
    pub backgrounds: Option<bool>,
    #[arg(
        long,
        num_args = 0..=1,
        require_equals = true,
        default_missing_value = "true",
        value_name = "BOOL",
        help = "Show line numbers"
    )]
    pub line_numbers: Option<bool>,
    #[arg(long, value_enum, help = "Whether a change in whitespace alone counts as a change")]
    pub whitespace: Option<Whitespace>,
    #[arg(
        long,
        value_name = "LINES",
        help = "How many unchanged lines stay around a change before the rest fold away"
    )]
    pub context: Option<usize>,
    #[arg(long, value_enum, help = "What stands out within a changed line")]
    pub emphasis: Option<Emphasis>,
    #[arg(long, value_enum, help = "Your terminal's own colours, or a theme built into Sift")]
    pub theme: Option<Scheme>,
    #[arg(
        long,
        value_name = "NAME|FILE",
        help = "The colours for the code, from the themes built into Sift"
    )]
    pub syntax_theme: Option<String>,
}

/// Sift's `config.toml`: its settings under `[diff]`, as Glean's file holds
/// them too, and its keys under `[keys]`.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields, rename_all = "kebab-case")]
pub struct File {
    pub diff: Config,
    #[serde(skip_serializing_if = "Rebindings::is_empty")]
    pub keys: Rebindings,
}

pub fn section() -> settings::Section {
    settings::Section { title: "Diff", path: vec!["diff"], fields: ::config::fields::<Config>() }
}

impl Config {
    #[must_use]
    pub fn resolved(&self) -> Self {
        let presentation = self.presentation();

        Self {
            layout: Some(presentation.mode),
            wrap: Some(presentation.wrap),
            tab_width: Some(presentation.tab_width),
            indicators: Some(presentation.indicators),
            backgrounds: Some(presentation.backgrounds),
            line_numbers: Some(presentation.line_numbers),
            whitespace: Some(presentation.whitespace),
            context: Some(presentation.context),
            emphasis: Some(presentation.emphasis),
            theme: Some(self.theme.unwrap_or_default()),
            syntax_theme: Some(self.syntax_theme.clone().unwrap_or_default()),
        }
    }

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
