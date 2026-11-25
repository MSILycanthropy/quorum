//! That Glean finds the GitHub repository behind a git repository's remotes.

use std::path::{Path, PathBuf};
use std::process::Command;

use glean::model::{Error, Repository};

fn named(owner: &str, name: &str) -> Repository {
    Repository { owner: owner.to_owned(), name: name.to_owned() }
}

fn scratch(test: &str, remotes: &[(&str, &str)]) -> PathBuf {
    let directory = std::env::temp_dir().join(format!("glean-{}-{test}", std::process::id()));
    if directory.exists() {
        std::fs::remove_dir_all(&directory).expect("an old scratch repository can be removed");
    }
    std::fs::create_dir_all(&directory).expect("a scratch directory can be made");
    git(&directory, &["init", "-q"]);
    for (remote, url) in remotes {
        git(&directory, &["remote", "add", remote, url]);
    }
    directory
}

fn git(directory: &Path, arguments: &[&str]) {
    let status =
        Command::new("git").arg("-C").arg(directory).args(arguments).status().expect("git runs");
    assert!(status.success(), "git {arguments:?} succeeds");
}

#[test]
fn ssh_https_and_scheme_ssh_urls_all_name_the_same_repository() {
    for url in [
        "git@github.com:MSILycanthropy/sift.git",
        "git@github.com:MSILycanthropy/sift",
        "ssh://git@github.com/MSILycanthropy/sift.git",
        "https://github.com/MSILycanthropy/sift.git",
        "https://github.com/MSILycanthropy/sift/",
        "https://github.com/MSILycanthropy/sift\n",
    ] {
        assert_eq!(Repository::parse(url), Some(named("MSILycanthropy", "sift")), "{url}");
    }
}

#[test]
fn a_url_that_is_not_on_github_names_no_repository() {
    for url in [
        "git@gitlab.com:someone/project.git",
        "https://codeberg.org/someone/project",
        "/srv/git/project.git",
    ] {
        assert_eq!(Repository::parse(url), None, "{url}");
    }
}

#[test]
fn a_url_without_both_an_owner_and_a_name_names_no_repository() {
    for url in [
        "https://github.com/",
        "https://github.com/someone",
        "https://github.com//project",
        "https://github.com/someone/project/tree/main",
    ] {
        assert_eq!(Repository::parse(url), None, "{url}");
    }
}

#[test]
fn a_repository_reads_as_owner_slash_name() {
    assert_eq!(named("MSILycanthropy", "sift").to_string(), "MSILycanthropy/sift");
}

#[test]
fn in_a_fork_the_upstream_remote_wins_over_origin() {
    let directory = scratch(
        "fork",
        &[
            ("origin", "git@github.com:someone/sift.git"),
            ("upstream", "git@github.com:MSILycanthropy/sift.git"),
        ],
    );
    let found = Repository::of(&directory).ok();
    assert_eq!(found, Some(named("MSILycanthropy", "sift")));
}

#[test]
fn without_an_upstream_the_origin_remote_is_used() {
    let directory = scratch("origin", &[("origin", "https://github.com/someone/project.git")]);
    let found = Repository::of(&directory).ok();
    assert_eq!(found, Some(named("someone", "project")));
}

#[test]
fn an_upstream_off_github_falls_back_to_a_github_origin() {
    let directory = scratch(
        "mirror",
        &[
            ("upstream", "https://codeberg.org/someone/project"),
            ("origin", "git@github.com:someone/project.git"),
        ],
    );
    let found = Repository::of(&directory).ok();
    assert_eq!(found, Some(named("someone", "project")));
}

#[test]
fn a_repository_with_no_github_remote_says_so() {
    let directory = scratch("bare", &[]);
    assert!(matches!(Repository::of(&directory), Err(Error::NoGitHubRemote)));
}

#[test]
fn a_directory_outside_any_repository_says_so() {
    let directory = std::env::temp_dir().join(format!("glean-{}-outside", std::process::id()));
    std::fs::create_dir_all(&directory).expect("a scratch directory can be made");
    assert!(matches!(Repository::of(&directory), Err(Error::NotARepository(_))));
}
