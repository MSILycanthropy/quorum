//! What a person sets for Glean, and for the Sift diff inside it, from `config.toml` or a flag.

use keys::Rebindings;
use serde::{Deserialize, Serialize};

use crate::model::Filter;

/// Glean's `config.toml`: its own settings at the top, Sift's under
/// `[diff]` as Sift's own file holds them, and the keys of both under `[keys]`.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash, Deserialize, Serialize, clap::Args)]
#[serde(default, deny_unknown_fields, rename_all = "kebab-case")]
pub struct Config {
    #[arg(long, value_enum, help = "Which pull requests to list")]
    pub filter: Option<Listing>,
    #[arg(long, value_name = "SECONDS", help = "How often the list refreshes, in seconds")]
    pub refresh: Option<usize>,
    #[arg(skip)]
    pub diff: sift::config::Config,
    #[arg(skip)]
    #[serde(skip_serializing_if = "Rebindings::is_empty")]
    pub keys: Rebindings,
}

/// Which pull requests the list shows.
#[derive(
    Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Deserialize, Serialize, clap::ValueEnum,
)]
#[serde(rename_all = "kebab-case")]
pub enum Listing {
    #[default]
    Open,
    Mine,
    ReviewRequested,
    Drafts,
    Closed,
}

impl From<Listing> for Filter {
    fn from(listing: Listing) -> Self {
        match listing {
            Listing::Open => Self::Open,
            Listing::Mine => Self::Mine,
            Listing::ReviewRequested => Self::ReviewRequested,
            Listing::Drafts => Self::Drafts,
            Listing::Closed => Self::Closed,
        }
    }
}

pub fn sections() -> Vec<settings::Section> {
    vec![
        settings::Section {
            title: "Glean",
            path: Vec::new(),
            fields: ::config::fields::<Config>(),
        },
        sift::config::section(),
    ]
}
