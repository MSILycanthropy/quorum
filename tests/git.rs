use std::path::{Path, PathBuf};
use std::process::Command;

use quorum::git::{Content, Error, FileChange, Repository, Status};
use quorum::project::WorkspaceKind;

fn git(root: &Path, arguments: &[&str]) {
    let status = Command::new("git")
        .current_dir(root)
        .args(["-c", "user.email=quorum@example.com", "-c", "user.name=Quorum"])
        .args(arguments)
        .status()
        .expect("git is on PATH");

    assert!(status.success(), "git {arguments:?} failed");
}

fn write(root: &Path, path: &str, bytes: impl AsRef<[u8]>) {
    std::fs::write(root.join(path), bytes).expect("the fixture directory is writable");
}

fn fixture(name: &str) -> PathBuf {
    let directory = Path::new(env!("CARGO_MANIFEST_DIR")).join("target/fixtures").join(name);
    std::fs::remove_dir_all(&directory).ok();

    let root = directory.join("main");
    std::fs::create_dir_all(&root).expect("the fixture directory is creatable");
    git(&root, &["init", "--quiet", "--initial-branch", "main"]);

    root
}

fn committed(name: &str, path: &str, bytes: impl AsRef<[u8]>) -> PathBuf {
    let root = fixture(name);
    write(&root, path, bytes);
    git(&root, &["add", "--all"]);
    git(&root, &["commit", "--quiet", "--message", "base"]);

    root
}

fn sides(change: &FileChange) -> (&str, &str) {
    match &change.content {
        Content::Text(diff) => (diff.old.text(), diff.new.text()),
        Content::Binary => panic!("the fixture is text"),
    }
}

#[test]
fn a_modified_file_holds_the_committed_text_against_the_text_on_disk() {
    let root = committed("modified_file", "file.rs", "fn main() {}\n");
    write(&root, "file.rs", "fn main() {\n    println!(\"hello\");\n}\n");

    let repository = Repository::discover(&root).expect("the fixture is a repository");
    let change =
        repository.change("HEAD", &root.join("file.rs")).expect("the file is on both sides");

    assert_eq!(change.status, Status::Modified);
    assert_eq!(change.path, Path::new("file.rs"));
    assert_eq!(sides(&change), ("fn main() {}\n", "fn main() {\n    println!(\"hello\");\n}\n"));
}

#[test]
fn an_earlier_revision_is_readable_by_name() {
    let root = committed("earlier_revision", "file.rs", "first\n");
    git(&root, &["tag", "base"]);
    write(&root, "file.rs", "second\n");
    git(&root, &["commit", "--quiet", "--all", "--message", "second"]);

    let repository = Repository::discover(&root).expect("the fixture is a repository");
    let path = root.join("file.rs");
    let tagged = repository.change("base", &path).expect("the file is on both sides");
    let head = repository.change("HEAD", &path).expect("the file is on both sides");

    assert_eq!(sides(&tagged), ("first\n", "second\n"));
    assert_eq!(sides(&head), ("second\n", "second\n"));
}

#[test]
fn a_file_the_revision_never_had_is_added_against_an_empty_side() {
    let root = committed("added_file", "file.rs", "fn main() {}\n");
    write(&root, "other.rs", "fn other() {}\n");

    let repository = Repository::discover(&root).expect("the fixture is a repository");
    let change = repository.change("HEAD", &root.join("other.rs")).expect("the file is on disk");

    assert_eq!(change.status, Status::Added);
    assert_eq!(sides(&change), ("", "fn other() {}\n"));
}

#[test]
fn a_file_gone_from_the_worktree_is_deleted_and_keeps_its_committed_side() {
    let root = committed("deleted_file", "file.rs", "fn main() {}\n");
    std::fs::remove_file(root.join("file.rs")).expect("the fixture file is removable");

    let repository = Repository::discover(&root).expect("the fixture is a repository");
    let change =
        repository.change("HEAD", &root.join("file.rs")).expect("the file is in the revision");

    assert_eq!(change.status, Status::Deleted);
    assert_eq!(sides(&change), ("fn main() {}\n", ""));
}

#[test]
fn a_file_in_neither_the_revision_nor_the_worktree_is_missing() {
    let root = committed("missing_file", "file.rs", "fn main() {}\n");

    let repository = Repository::discover(&root).expect("the fixture is a repository");
    let error = repository
        .change("HEAD", &root.join("nowhere.rs"))
        .expect_err("the file is on neither side");

    assert!(matches!(error, Error::Missing { .. }), "{error:?}");
}

#[test]
fn a_file_holding_a_nul_byte_is_binary() {
    let root = committed("nul_byte", "file.bin", b"one\0two\n");

    let repository = Repository::discover(&root).expect("the fixture is a repository");
    let change =
        repository.change("HEAD", &root.join("file.bin")).expect("the file is on both sides");

    assert_eq!(&change.content, &Content::Binary);
}

#[test]
fn a_file_that_is_not_utf_8_is_binary() {
    let root = committed("not_utf_8", "file.txt", b"caf\xe9\n");

    let repository = Repository::discover(&root).expect("the fixture is a repository");
    let change =
        repository.change("HEAD", &root.join("file.txt")).expect("the file is on both sides");

    assert_eq!(&change.content, &Content::Binary);
}

#[test]
fn a_path_outside_the_worktree_is_rejected() {
    let root = committed("outside_worktree", "file.rs", "fn main() {}\n");

    let repository = Repository::discover(&root).expect("the fixture is a repository");
    let error = repository
        .change("HEAD", Path::new("/etc/hosts"))
        .expect_err("the path is not in the repository");

    assert!(matches!(error, Error::OutsideRepository(_)), "{error:?}");
}

#[test]
fn checkout_filters_are_applied_so_the_revision_matches_what_is_on_disk() {
    let root = fixture("checkout_filters");
    write(&root, ".gitattributes", "*.txt text eol=crlf\n");
    write(&root, "file.txt", "one\ntwo\n");
    git(&root, &["add", "--all"]);
    git(&root, &["commit", "--quiet", "--message", "base"]);

    let repository = Repository::discover(&root).expect("the fixture is a repository");
    let change =
        repository.change("HEAD", &root.join("file.txt")).expect("the file is on both sides");

    assert_eq!(sides(&change).0, "one\r\ntwo\r\n");
}

#[test]
fn changes_covers_every_file_that_differs_from_the_revision() {
    let root = committed("changes", "file.rs", "fn main() {}\n");
    write(&root, "gone.rs", "fn gone() {}\n");
    git(&root, &["add", "--all"]);
    git(&root, &["commit", "--quiet", "--message", "gone"]);

    write(&root, "file.rs", "fn main() { }\n");
    std::fs::remove_file(root.join("gone.rs")).expect("the fixture file is removable");
    write(&root, "new.rs", "fn new() {}\n");

    let repository = Repository::discover(&root).expect("the fixture is a repository");
    let changes = repository.changes("HEAD").expect("the worktree is readable");
    let seen: Vec<_> = changes.iter().map(|change| (change.status, change.path.clone())).collect();

    assert_eq!(
        seen,
        vec![
            (Status::Modified, PathBuf::from("file.rs")),
            (Status::Deleted, PathBuf::from("gone.rs")),
            (Status::Added, PathBuf::from("new.rs")),
        ]
    );
}

#[test]
fn changes_lists_the_files_inside_an_untracked_directory_not_the_directory() {
    let root = committed("untracked_directory", "file.rs", "fn main() {}\n");
    std::fs::create_dir(root.join("fresh")).expect("the fixture directory is creatable");
    write(&root, "fresh/one.rs", "fn one() {}\n");
    write(&root, "fresh/two.rs", "fn two() {}\n");

    let repository = Repository::discover(&root).expect("the fixture is a repository");
    let changes = repository.changes("HEAD").expect("the worktree is readable");
    let seen: Vec<_> = changes.iter().map(|change| change.path.clone()).collect();

    assert_eq!(seen, vec![PathBuf::from("fresh/one.rs"), PathBuf::from("fresh/two.rs")]);
}

#[test]
fn changes_against_an_earlier_revision_include_what_later_commits_changed() {
    let root = committed("changes_earlier", "file.rs", "first\n");
    git(&root, &["tag", "base"]);
    write(&root, "file.rs", "second\n");
    write(&root, "later.rs", "fn later() {}\n");
    git(&root, &["add", "--all"]);
    git(&root, &["commit", "--quiet", "--message", "second"]);

    let repository = Repository::discover(&root).expect("the fixture is a repository");
    let changes = repository.changes("base").expect("the worktree is readable");
    let seen: Vec<_> = changes.iter().map(|change| (change.status, change.path.clone())).collect();

    assert_eq!(
        seen,
        vec![
            (Status::Modified, PathBuf::from("file.rs")),
            (Status::Added, PathBuf::from("later.rs")),
        ]
    );
}

#[test]
fn the_main_worktree_is_anchored_on_head() {
    let root = committed("main_base", "file.rs", "fn main() {}\n");

    let repository = Repository::discover(&root).expect("the fixture is a repository");
    let head =
        repository.change("HEAD", &root.join("file.rs")).expect("the file is committed at HEAD");
    let base = repository
        .change(&repository.base().expect("HEAD is born"), &root.join("file.rs"))
        .expect("the base is a commit");

    assert_eq!(sides(&base), sides(&head));
}

#[test]
fn a_linked_worktree_off_a_feature_branch_is_anchored_on_that_branch_not_on_main() {
    let root = committed("feature_base", "file.rs", "on main\n");
    git(&root, &["switch", "--quiet", "--create", "feature"]);
    write(&root, "file.rs", "on feature\n");
    git(&root, &["commit", "--quiet", "--all", "--message", "feature"]);
    git(&root, &["switch", "--quiet", "main"]);

    let linked = root.with_file_name("agent");
    git(&root, &["worktree", "add", "--quiet", "../agent", "feature"]);
    write(&linked, "file.rs", "by the agent\n");

    let repository = Repository::discover(&linked).expect("the linked worktree is a repository");
    let base = repository.base().expect("the worktree records what it was created at");
    let change =
        repository.change(&base, &linked.join("file.rs")).expect("the file is on both sides");

    assert_eq!(sides(&change), ("on feature\n", "by the agent\n"));
}

#[test]
fn a_commit_made_inside_a_linked_worktree_still_shows_against_its_creation_commit() {
    let root = committed("worktree_commit", "file.rs", "at creation\n");
    git(&root, &["branch", "agent"]);

    let linked = root.with_file_name("agent");
    git(&root, &["worktree", "add", "--quiet", "../agent", "agent"]);
    write(&linked, "file.rs", "committed by the agent\n");
    git(&linked, &["commit", "--quiet", "--all", "--message", "agent"]);

    let repository = Repository::discover(&linked).expect("the linked worktree is a repository");
    let base = repository.base().expect("the worktree records what it was created at");
    let change =
        repository.change(&base, &linked.join("file.rs")).expect("the file is on both sides");

    assert_eq!(sides(&change), ("at creation\n", "committed by the agent\n"));
}

#[test]
fn a_linked_worktree_reads_its_own_checked_out_revision() {
    let root = committed("linked_worktree", "file.rs", "on main\n");
    git(&root, &["branch", "agent"]);
    git(&root, &["switch", "--quiet", "agent"]);
    write(&root, "file.rs", "on agent\n");
    git(&root, &["commit", "--quiet", "--all", "--message", "agent"]);
    git(&root, &["switch", "--quiet", "main"]);

    let linked = root.with_file_name("agent");
    git(&root, &["worktree", "add", "--quiet", "../agent", "agent"]);

    let repository = Repository::discover(&linked).expect("the linked worktree is a repository");
    let change =
        repository.change("HEAD", &linked.join("file.rs")).expect("the file is on both sides");

    assert_eq!(sides(&change), ("on agent\n", "on agent\n"));
}

#[test]
fn discovering_from_main_and_linked_worktrees_returns_the_same_project_inventory() {
    let root = committed("inventory_from_linked", "file.rs", "on main\n");
    git(&root, &["branch", "agent"]);

    let linked = root.with_file_name("agent");
    git(&root, &["worktree", "add", "--quiet", "../agent", "agent"]);

    let from_main = Repository::discover(&root)
        .expect("the main worktree is a repository")
        .inventory()
        .expect("the main inventory is readable");
    let from_linked = Repository::discover(&linked)
        .expect("the linked worktree is a repository")
        .inventory()
        .expect("the linked inventory is readable");

    assert_eq!(from_main, from_linked);
    assert_eq!(
        from_main.common_directory,
        root.join(".git").canonicalize().expect("the common directory is valid")
    );
    assert_eq!(from_main.worktrees.len(), 2);
    assert_eq!(
        from_main.worktrees[0].worktree.location(),
        root.canonicalize().expect("the main path is valid")
    );
    assert_eq!(from_main.worktrees[0].kind, WorkspaceKind::Main);
    assert_eq!(from_main.worktrees[0].worktree.branch(), Some("main"));
    assert_eq!(
        from_main.worktrees[1].worktree.location(),
        linked.canonicalize().expect("the linked path is valid")
    );
    assert_eq!(from_main.worktrees[1].kind, WorkspaceKind::Linked);
    assert_eq!(from_main.worktrees[1].worktree.branch(), Some("agent"));
}

#[test]
fn inventory_lists_linked_worktrees_in_stable_path_order_and_keeps_detached_heads_detached() {
    let root = committed("inventory_order", "file.rs", "on main\n");
    git(&root, &["branch", "feature"]);
    git(&root, &["branch", "second"]);

    let detached = root.with_file_name("a detached");
    let second = root.with_file_name("m second");
    let feature = root.with_file_name("z feature");
    git(&root, &["worktree", "add", "--quiet", "--detach", "../a detached", "HEAD"]);
    git(&root, &["worktree", "add", "--quiet", "../m second", "second"]);
    git(&root, &["worktree", "add", "--quiet", "../z feature", "feature"]);

    let inventory = Repository::discover(&root)
        .expect("the repository is readable")
        .inventory()
        .expect("the inventory is readable");
    let worktrees: Vec<_> = inventory
        .worktrees
        .iter()
        .map(|entry| {
            (
                entry.worktree.location().to_owned(),
                entry.kind,
                entry.worktree.branch().map(str::to_owned),
            )
        })
        .collect();

    assert_eq!(
        worktrees,
        vec![
            (
                root.canonicalize().expect("the main path is valid"),
                WorkspaceKind::Main,
                Some("main".into())
            ),
            (
                detached.canonicalize().expect("the detached path is valid"),
                WorkspaceKind::Linked,
                None,
            ),
            (
                second.canonicalize().expect("the second path is valid"),
                WorkspaceKind::Linked,
                Some("second".into()),
            ),
            (
                feature.canonicalize().expect("the feature path is valid"),
                WorkspaceKind::Linked,
                Some("feature".into()),
            ),
        ]
    );
}

#[test]
fn inventory_skips_a_linked_worktree_whose_checkout_is_stale() {
    let root = committed("inventory_stale", "file.rs", "on main\n");
    git(&root, &["branch", "stale"]);

    let stale = root.with_file_name("stale");
    git(&root, &["worktree", "add", "--quiet", "../stale", "stale"]);
    std::fs::remove_dir_all(&stale).expect("the stale worktree is removable");

    let inventory = Repository::discover(&root)
        .expect("the repository is readable")
        .inventory()
        .expect("a stale worktree does not make inventory fail");

    assert_eq!(inventory.worktrees.len(), 1);
    assert_eq!(
        inventory.worktrees[0].worktree.location(),
        root.canonicalize().expect("the main path is valid")
    );
}
