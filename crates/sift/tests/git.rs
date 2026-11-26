use std::fmt::Write;
use std::ops::Range;
use std::path::{Path, PathBuf};
use std::process::Command;

use sift::git::{Comparison, Content, Error, FileChange, Repository, Snapshot, Status};

fn git(root: &Path, arguments: &[&str]) {
    let status = Command::new("git")
        .current_dir(root)
        .args(["-c", "user.email=sift@example.com", "-c", "user.name=Sift"])
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

fn against(revision: &str) -> Comparison {
    Comparison { old: revision.to_owned(), new: Snapshot::Worktree }
}

fn numbered(lines: Range<usize>) -> String {
    lines.fold(String::new(), |mut text, line| {
        writeln!(text, "line {line}").expect("writing to a String cannot fail");
        text
    })
}

fn seen(changes: &[FileChange]) -> Vec<(Status, PathBuf)> {
    changes.iter().map(|change| (change.status, change.path.clone())).collect()
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
    let change = repository
        .change(&against("HEAD"), &root.join("file.rs"))
        .expect("the file is on both sides");

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
    let tagged = repository.change(&against("base"), &path).expect("the file is on both sides");
    let head = repository.change(&against("HEAD"), &path).expect("the file is on both sides");

    assert_eq!(sides(&tagged), ("first\n", "second\n"));
    assert_eq!(sides(&head), ("second\n", "second\n"));
}

#[test]
fn a_file_the_revision_never_had_is_added_against_an_empty_side() {
    let root = committed("added_file", "file.rs", "fn main() {}\n");
    write(&root, "other.rs", "fn other() {}\n");

    let repository = Repository::discover(&root).expect("the fixture is a repository");
    let change =
        repository.change(&against("HEAD"), &root.join("other.rs")).expect("the file is on disk");

    assert_eq!(change.status, Status::Added);
    assert_eq!(sides(&change), ("", "fn other() {}\n"));
}

#[test]
fn a_file_gone_from_the_worktree_is_deleted_and_keeps_its_committed_side() {
    let root = committed("deleted_file", "file.rs", "fn main() {}\n");
    std::fs::remove_file(root.join("file.rs")).expect("the fixture file is removable");

    let repository = Repository::discover(&root).expect("the fixture is a repository");
    let change = repository
        .change(&against("HEAD"), &root.join("file.rs"))
        .expect("the file is in the revision");

    assert_eq!(change.status, Status::Deleted);
    assert_eq!(sides(&change), ("fn main() {}\n", ""));
}

#[test]
fn a_file_in_neither_the_revision_nor_the_worktree_is_missing() {
    let root = committed("missing_file", "file.rs", "fn main() {}\n");

    let repository = Repository::discover(&root).expect("the fixture is a repository");
    let error = repository
        .change(&against("HEAD"), &root.join("nowhere.rs"))
        .expect_err("the file is on neither side");

    assert!(matches!(error, Error::Missing { .. }), "{error:?}");
}

#[test]
fn a_file_holding_a_nul_byte_is_binary() {
    let root = committed("nul_byte", "file.bin", b"one\0two\n");

    let repository = Repository::discover(&root).expect("the fixture is a repository");
    let change = repository
        .change(&against("HEAD"), &root.join("file.bin"))
        .expect("the file is on both sides");

    assert_eq!(&change.content, &Content::Binary);
}

#[test]
fn a_file_that_is_not_utf_8_is_binary() {
    let root = committed("not_utf_8", "file.txt", b"caf\xe9\n");

    let repository = Repository::discover(&root).expect("the fixture is a repository");
    let change = repository
        .change(&against("HEAD"), &root.join("file.txt"))
        .expect("the file is on both sides");

    assert_eq!(&change.content, &Content::Binary);
}

#[test]
fn a_path_outside_the_worktree_is_rejected() {
    let root = committed("outside_worktree", "file.rs", "fn main() {}\n");

    let repository = Repository::discover(&root).expect("the fixture is a repository");
    let error = repository
        .change(&against("HEAD"), Path::new("/etc/hosts"))
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
    let change = repository
        .change(&against("HEAD"), &root.join("file.txt"))
        .expect("the file is on both sides");

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
    let changes = repository.changes(&against("HEAD")).expect("the worktree is readable");
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
    let changes = repository.changes(&against("HEAD")).expect("the worktree is readable");
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
    let changes = repository.changes(&against("base")).expect("the worktree is readable");
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
    let change = repository
        .change(&against("HEAD"), &linked.join("file.rs"))
        .expect("the file is on both sides");

    assert_eq!(sides(&change), ("on agent\n", "on agent\n"));
}

#[test]
fn staged_changes_leave_out_what_is_only_on_disk() {
    let root = committed("staged", "file.rs", "first\n");
    write(&root, "file.rs", "staged\n");
    write(&root, "staged.rs", "fn staged() {}\n");
    git(&root, &["add", "--all"]);
    write(&root, "file.rs", "unstaged\n");
    write(&root, "untracked.rs", "fn untracked() {}\n");

    let repository = Repository::discover(&root).expect("the fixture is a repository");
    let comparison = Comparison { old: "HEAD".to_owned(), new: Snapshot::Index };
    let changes = repository.changes(&comparison).expect("the index is readable");

    assert_eq!(
        seen(&changes),
        vec![
            (Status::Modified, PathBuf::from("file.rs")),
            (Status::Added, PathBuf::from("staged.rs")),
        ]
    );
    assert_eq!(sides(&changes[0]), ("first\n", "staged\n"));
}

#[test]
fn two_revisions_compare_their_trees_and_ignore_the_worktree() {
    let root = committed("two_revisions", "file.rs", "first\n");
    write(&root, "gone.rs", "fn gone() {}\n");
    git(&root, &["add", "--all"]);
    git(&root, &["commit", "--quiet", "--message", "gone"]);
    git(&root, &["tag", "base"]);
    write(&root, "file.rs", "second\n");
    std::fs::create_dir(root.join("nested")).expect("the fixture directory is creatable");
    write(&root, "nested/new.rs", "fn new() {}\n");
    git(&root, &["rm", "--quiet", "gone.rs"]);
    git(&root, &["add", "--all"]);
    git(&root, &["commit", "--quiet", "--message", "second"]);
    write(&root, "file.rs", "on disk only\n");

    let repository = Repository::discover(&root).expect("the fixture is a repository");
    let comparison = Comparison { old: "base".to_owned(), new: Snapshot::Revision("HEAD".into()) };
    let changes = repository.changes(&comparison).expect("both trees are readable");

    assert_eq!(
        seen(&changes),
        vec![
            (Status::Modified, PathBuf::from("file.rs")),
            (Status::Deleted, PathBuf::from("gone.rs")),
            (Status::Added, PathBuf::from("nested/new.rs")),
        ]
    );
    assert_eq!(sides(&changes[0]), ("first\n", "second\n"));
}

#[test]
fn the_merge_base_of_diverged_branches_is_where_they_split() {
    let root = committed("merge_base", "file.rs", "shared\n");
    git(&root, &["tag", "fork"]);
    git(&root, &["switch", "--quiet", "--create", "feature"]);
    write(&root, "file.rs", "feature\n");
    git(&root, &["commit", "--quiet", "--all", "--message", "feature"]);
    git(&root, &["switch", "--quiet", "main"]);
    write(&root, "main.rs", "fn main() {}\n");
    git(&root, &["add", "--all"]);
    git(&root, &["commit", "--quiet", "--message", "main"]);

    let repository = Repository::discover(&root).expect("the fixture is a repository");
    let base = repository.merge_base("main", "feature").expect("the branches share history");
    let comparison = Comparison { old: base, new: Snapshot::Revision("feature".into()) };
    let changes = repository.changes(&comparison).expect("both trees are readable");

    assert_eq!(seen(&changes), vec![(Status::Modified, PathBuf::from("file.rs"))]);
    assert_eq!(sides(&changes[0]), ("shared\n", "feature\n"));
}

#[test]
fn a_root_commit_shows_against_the_empty_tree() {
    let root = committed("root_commit", "file.rs", "first\n");

    let repository = Repository::discover(&root).expect("the fixture is a repository");
    let parent = repository.parent("HEAD").expect("HEAD is a commit");
    let comparison = Comparison { old: parent, new: Snapshot::Revision("HEAD".into()) };
    let changes = repository.changes(&comparison).expect("both trees are readable");

    assert_eq!(seen(&changes), vec![(Status::Added, PathBuf::from("file.rs"))]);
}

#[test]
fn a_later_commit_shows_only_what_it_changed() {
    let root = committed("later_commit", "file.rs", "first\n");
    write(&root, "other.rs", "fn other() {}\n");
    git(&root, &["add", "--all"]);
    git(&root, &["commit", "--quiet", "--message", "other"]);

    let repository = Repository::discover(&root).expect("the fixture is a repository");
    let parent = repository.parent("HEAD").expect("HEAD is a commit");
    let comparison = Comparison { old: parent, new: Snapshot::Revision("HEAD".into()) };
    let changes = repository.changes(&comparison).expect("both trees are readable");

    assert_eq!(seen(&changes), vec![(Status::Added, PathBuf::from("other.rs"))]);
}

#[test]
fn every_tracked_file_is_listed_whether_or_not_it_changed() {
    let root = committed("tracked_files", "file.rs", "first\n");
    std::fs::create_dir_all(root.join("src")).expect("the fixture directory is writable");
    write(&root, "src/lib.rs", "lib\n");
    git(&root, &["add", "--all"]);
    git(&root, &["commit", "--quiet", "--message", "lib"]);

    let repository = Repository::discover(&root).expect("the fixture is a repository");

    assert_eq!(
        repository.files(&against("HEAD")).expect("the index is readable"),
        vec![PathBuf::from("file.rs"), PathBuf::from("src/lib.rs")]
    );
    assert_eq!(
        repository
            .files(&Comparison {
                old: "HEAD~1".to_owned(),
                new: Snapshot::Revision("HEAD~1".to_owned())
            })
            .expect("the tree is readable"),
        vec![PathBuf::from("file.rs")]
    );
}

#[test]
fn an_unchanged_file_reads_as_the_newer_side_has_it() {
    let root = committed("unchanged_file", "file.rs", "first\n");

    let repository = Repository::discover(&root).expect("the fixture is a repository");
    let Content::Text(diff) =
        repository.file(&against("HEAD"), Path::new("file.rs")).expect("the file exists")
    else {
        panic!("the fixture is text");
    };

    assert_eq!((diff.old.text(), diff.new.text()), ("first\n", "first\n"));
}

#[test]
fn a_moved_and_edited_file_is_one_rename() {
    let body = "one\ntwo\nthree\nfour\nfive\nsix\n";
    let root = committed("renamed_file", "old.rs", body);
    git(&root, &["mv", "old.rs", "new.rs"]);
    write(&root, "new.rs", body.replace("six", "seven"));

    let repository = Repository::discover(&root).expect("the fixture is a repository");
    let changes = repository.changes(&against("HEAD")).expect("the snapshots are readable");

    assert_eq!(seen(&changes), vec![(Status::Renamed, PathBuf::from("new.rs"))]);
    assert_eq!(changes[0].previous, Some(PathBuf::from("old.rs")));
    assert_eq!(sides(&changes[0]), (body, body.replace("six", "seven").as_str()));
}

#[test]
fn a_moved_file_that_grew_to_three_times_its_size_is_an_addition_and_a_deletion_as_git_says() {
    let root = committed("grown_file", "old.rs", numbered(0..100));
    git(&root, &["mv", "old.rs", "new.rs"]);
    write(&root, "new.rs", numbered(0..300));

    let repository = Repository::discover(&root).expect("the fixture is a repository");
    let changes = repository.changes(&against("HEAD")).expect("the snapshots are readable");

    assert_eq!(
        seen(&changes),
        vec![(Status::Added, PathBuf::from("new.rs")), (Status::Deleted, PathBuf::from("old.rs"))]
    );
}

#[test]
fn an_unrelated_file_added_as_another_is_deleted_is_not_a_rename() {
    let root = committed("unrelated_files", "old.rs", "one\ntwo\nthree\n");
    std::fs::remove_file(root.join("old.rs")).expect("the fixture is writable");
    write(&root, "new.rs", "alpha\nbeta\ngamma\n");

    let repository = Repository::discover(&root).expect("the fixture is a repository");
    let changes = repository.changes(&against("HEAD")).expect("the snapshots are readable");

    assert_eq!(
        seen(&changes),
        vec![(Status::Added, PathBuf::from("new.rs")), (Status::Deleted, PathBuf::from("old.rs"))]
    );
}

fn generated(root: &Path) -> Vec<(PathBuf, bool)> {
    let repository = Repository::discover(root).expect("the fixture is a repository");
    let changes = repository.changes(&against("HEAD")).expect("the snapshots are readable");

    changes.into_iter().map(|change| (change.path, change.generated)).collect()
}

#[test]
fn gitattributes_decide_what_is_generated_before_any_guess() {
    let root = fixture("generated_attributes");
    write(
        &root,
        ".gitattributes",
        "schema.rs linguist-generated\nblob.dat -diff\nCargo.lock linguist-generated=false\n",
    );
    git(&root, &["add", "--all"]);
    git(&root, &["commit", "--quiet", "--message", "attributes"]);
    for path in ["schema.rs", "blob.dat", "Cargo.lock", "plain.rs"] {
        write(&root, path, "fn main() {}\n");
    }

    assert_eq!(
        generated(&root),
        vec![
            (PathBuf::from("Cargo.lock"), false),
            (PathBuf::from("blob.dat"), true),
            (PathBuf::from("plain.rs"), false),
            (PathBuf::from("schema.rs"), true),
        ]
    );
}

#[test]
fn without_attributes_lockfiles_and_marked_files_count_as_generated() {
    let root = committed("generated_guesses", "README.md", "readme\n");
    write(&root, "Cargo.lock", "version = 4\n");
    write(&root, "api.go", "// Code generated by protoc. DO NOT EDIT.\npackage api\n");
    write(&root, "main.go", "package main\n");

    assert_eq!(
        generated(&root),
        vec![
            (PathBuf::from("Cargo.lock"), true),
            (PathBuf::from("api.go"), true),
            (PathBuf::from("main.go"), false),
        ]
    );
}
