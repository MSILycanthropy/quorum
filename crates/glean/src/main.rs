//! The Glean binary.

use std::process::ExitCode;

use clap::Parser;
use glean::model::{Error, Repository};

#[derive(Debug, Parser)]
#[command(version, about = "Pull requests for the GitHub repository you're in.")]
struct Arguments {}

fn main() -> ExitCode {
    let Arguments {} = Arguments::parse();
    let found = std::env::current_dir()
        .map_err(Error::from)
        .and_then(|directory| Repository::of(&directory));

    match found {
        Ok(repository) => {
            println!("{repository}");
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("glean: {error}");
            ExitCode::FAILURE
        }
    }
}
