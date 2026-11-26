//! Settings files: where they live, and reading them into a program's own types.

use std::path::{Path, PathBuf};

use etcetera::BaseStrategy;
use serde::de::DeserializeOwned;
use toml::{Table, Value};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("{path}: {source}")]
    Read { path: PathBuf, source: std::io::Error },
    #[error("{path}: {source}")]
    Parse { path: PathBuf, source: toml::de::Error },
    #[error(transparent)]
    Layered(#[from] toml::de::Error),
}

pub type Result<T> = std::result::Result<T, Error>;

pub fn directory(application: &str) -> Option<PathBuf> {
    etcetera::choose_base_strategy().ok().map(|strategy| strategy.config_dir().join(application))
}

/// The keys of one or more settings files, each checked against the type it
/// will become, with a later file winning key by key.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Settings {
    table: Table,
}

impl Settings {
    #[expect(
        clippy::missing_errors_doc,
        reason = "the Error enum names the failures, a file that cannot be read or does not parse as T"
    )]
    pub fn of<T: DeserializeOwned>(application: &str) -> Result<Self> {
        match directory(application) {
            Some(directory) => Self::read::<T>(&directory.join("config.toml")),
            None => Ok(Self::default()),
        }
    }

    #[expect(
        clippy::missing_errors_doc,
        reason = "the Error enum names the failures, a file that cannot be read or does not parse as T"
    )]
    pub fn read<T: DeserializeOwned>(path: &Path) -> Result<Self> {
        let text = match std::fs::read_to_string(path) {
            Ok(text) => text,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Ok(Self::default());
            }
            Err(source) => return Err(Error::Read { path: path.to_path_buf(), source }),
        };
        let parse = |source| Error::Parse { path: path.to_path_buf(), source };
        toml::from_str::<T>(&text).map_err(parse)?;

        Ok(Self { table: toml::from_str(&text).map_err(parse)? })
    }

    #[must_use]
    pub fn over(self, top: Self) -> Self {
        Self { table: merged(self.table, top.table) }
    }

    #[must_use]
    pub fn section(&self, key: &str) -> Self {
        match self.table.get(key) {
            Some(Value::Table(table)) => Self { table: table.clone() },
            _ => Self::default(),
        }
    }

    #[expect(
        clippy::missing_errors_doc,
        reason = "the Error enum names the only failure, layers that each parse but not together"
    )]
    pub fn parse<T: DeserializeOwned>(self) -> Result<T> {
        Ok(self.table.try_into()?)
    }
}

fn merged(mut bottom: Table, top: Table) -> Table {
    for (key, value) in top {
        let value = match (bottom.remove(&key), value) {
            (Some(Value::Table(under)), Value::Table(over)) => Value::Table(merged(under, over)),
            (_, value) => value,
        };
        bottom.insert(key, value);
    }

    bottom
}
