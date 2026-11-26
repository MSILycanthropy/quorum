//! GitHub, reached over its GraphQL API, and turned into the model.

mod client;
mod pull_requests;
mod standings;
mod timestamp;

pub use client::Client;
pub use pull_requests::{Cursor, Page};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("no GitHub token: run `gh auth login`, or set GH_TOKEN or GITHUB_TOKEN")]
    NoToken,
    #[error("GitHub refused the token: run `gh auth login` again")]
    Unauthorized,
    #[error("GitHub answered {status}: {message}")]
    Status { status: u16, message: String },
    #[error("GitHub: {0}")]
    NotFound(String),
    #[error("GitHub: {0}")]
    Query(String),
    #[error("GitHub sent a timestamp Glean cannot read: {0}")]
    Timestamp(String),
    #[error("GitHub's answer did not have the expected shape: {0}")]
    Decode(#[from] serde_json::Error),
    #[error(transparent)]
    Http(Box<ureq::Error>),
}

impl From<ureq::Error> for Error {
    fn from(error: ureq::Error) -> Self {
        Self::Http(Box::new(error))
    }
}

pub type Result<T> = std::result::Result<T, Error>;

fn printable(text: String) -> String {
    if text.chars().any(char::is_control) {
        return text.chars().filter(|character| !character.is_control()).collect();
    }

    text
}
