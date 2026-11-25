//! Which repository on a forge a git repository's pull requests live in.

use std::fmt;
use std::path::Path;
use std::process::Command;
use std::str::FromStr;

use crate::model::{Error, Result};

const REMOTES: [&str; 2] = ["upstream", "origin"];

/// A repository on a forge, named the way forges name it: `owner/name`.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct Repository {
    pub owner: String,
    pub name: String,
}

impl Repository {
    pub fn parse(url: &str) -> Option<Self> {
        let url = url.trim();
        let path = ["git@github.com:", "ssh://git@github.com/", "https://github.com/"]
            .iter()
            .find_map(|prefix| url.strip_prefix(prefix))?;
        let path = path.trim_end_matches('/');
        let path = path.strip_suffix(".git").unwrap_or(path);

        path.parse().ok()
    }

    #[expect(
        clippy::missing_errors_doc,
        reason = "the Error enum names every failure: no repository, no GitHub remote, git not running"
    )]
    pub fn of(directory: &Path) -> Result<Self> {
        let inside = git(directory, &["rev-parse", "--git-dir"])?;
        if inside.is_none() {
            return Err(Error::NotARepository(directory.to_path_buf()));
        }

        for remote in REMOTES {
            let url = git(directory, &["remote", "get-url", remote])?;
            if let Some(repository) = url.as_deref().and_then(Self::parse) {
                return Ok(repository);
            }
        }

        Err(Error::NoGitHubRemote)
    }
}

impl FromStr for Repository {
    type Err = Error;

    fn from_str(text: &str) -> Result<Self> {
        let valid = |part: &str| {
            !part.is_empty()
                && part.chars().all(|character| {
                    character.is_ascii_alphanumeric() || matches!(character, '-' | '_' | '.')
                })
        };
        match text.split_once('/') {
            Some((owner, name)) if valid(owner) && valid(name) => {
                Ok(Self { owner: owner.to_owned(), name: name.to_owned() })
            }
            _ => Err(Error::NotARepositoryName(text.to_owned())),
        }
    }
}

impl fmt::Display for Repository {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}/{}", self.owner, self.name)
    }
}

fn git(directory: &Path, arguments: &[&str]) -> Result<Option<String>> {
    let output = Command::new("git").arg("-C").arg(directory).args(arguments).output()?;
    if !output.status.success() {
        return Ok(None);
    }

    Ok(Some(String::from_utf8_lossy(&output.stdout).trim().to_owned()))
}
