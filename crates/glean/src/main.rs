//! The Glean binary.

use std::process::ExitCode;

use clap::Parser;
use glean::config::Config;
use glean::github::Client;
use glean::model::{PullRequest, Repository, State};

#[derive(Debug, thiserror::Error)]
enum Error {
    #[error(transparent)]
    Model(#[from] glean::model::Error),
    #[error(transparent)]
    GitHub(#[from] glean::github::Error),
    #[error(transparent)]
    Config(#[from] config::Error),
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, Parser)]
#[command(version, about = "Pull requests for the GitHub repository you're in.")]
struct Arguments {
    #[arg(
        value_name = "OWNER/NAME",
        help = "The repository to list, instead of the one the current directory's remotes name"
    )]
    repository: Option<Repository>,
    #[command(flatten, next_help_heading = "Settings, also keys in config.toml")]
    config: Config,
    #[command(flatten, next_help_heading = "Diff settings, also keys under [diff] in config.toml")]
    diff: sift::config::Config,
}

fn main() -> ExitCode {
    match list(Arguments::parse()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("glean: {error}");
            ExitCode::FAILURE
        }
    }
}

fn list(arguments: Arguments) -> Result<()> {
    let config = config(&arguments)?;
    let repository = match arguments.repository {
        Some(repository) => repository,
        None => Repository::of(&std::env::current_dir()?)?,
    };
    let client = Client::signed_in()?;
    let mut listed = client
        .pull_requests(&repository, config.filter.unwrap_or_default().into(), None)?
        .pull_requests;
    let ids: Vec<String> = listed.iter().map(|pull_request| pull_request.id.clone()).collect();
    for answer in client.standings(&ids) {
        for (id, standing) in answer? {
            if let Some(pull_request) = listed.iter_mut().find(|pull_request| pull_request.id == id)
            {
                pull_request.standing = Some(standing);
            }
        }
    }

    println!("{repository}");
    for pull_request in &listed {
        println!("{}", row(pull_request));
    }
    if listed.is_empty() {
        println!("no pull requests");
    }

    Ok(())
}

fn config(arguments: &Arguments) -> Result<Config> {
    let file = config::Settings::of::<Config>("glean")?;
    let flags = config::Settings::serialized(&Config {
        diff: arguments.diff.clone(),
        ..arguments.config.clone()
    })?;

    Ok(file.over(flags).parse()?)
}

fn row(pull_request: &PullRequest) -> String {
    let state = match pull_request.state {
        State::Draft => "draft",
        State::Open => "open",
        State::Closed => "closed",
        State::Merged => "merged",
    };

    let size = pull_request
        .standing
        .as_ref()
        .map_or_else(String::new, |standing| format!("+{} −{}", standing.added, standing.removed));

    format!(
        "#{:<5} {state:<7} {:<16} {size:<13} {}",
        pull_request.number, pull_request.author, pull_request.title
    )
}
