//! What a person sets once for every diff Sift shows, read from `config.toml`.

use serde::Deserialize;

use crate::theme::Scheme;
use crate::ui::Mode;

/// Sift's settings, each defaulting on its own, so a file need hold only the
/// keys it changes.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Deserialize)]
#[serde(default, deny_unknown_fields, rename_all = "kebab-case")]
pub struct Config {
    pub layout: Mode,
    pub wrap: bool,
    pub tab_width: usize,
    pub theme: Scheme,
    pub syntax_theme: Option<String>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            layout: Mode::default(),
            wrap: false,
            tab_width: 4,
            theme: Scheme::default(),
            syntax_theme: None,
        }
    }
}
