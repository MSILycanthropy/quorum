//! That changed paths become a tree whose rows are short enough to read.

use std::path::PathBuf;

use quorum::diff::{FileDiff, Source};
use quorum::git::{Content, FileChange, Status};
use quorum::ui::Tree;

fn changed(paths: &[&str]) -> Vec<FileChange> {
    paths
        .iter()
        .map(|path| FileChange {
            status: Status::Modified,
            path: PathBuf::from(path),
            content: Content::Text(FileDiff::compute(Source::new("a\n"), Source::new("b\n"))),
        })
        .collect()
}

fn drawn(tree: &Tree) -> String {
    tree.rows()
        .iter()
        .map(|row| {
            let mark = if row.change.is_some() {
                " "
            } else if row.open {
                "▾"
            } else {
                "▸"
            };

            format!("{}{mark} {}", "  ".repeat(row.depth), row.label)
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn a_chain_of_single_child_directories_renders_as_one_row() {
    let tree = Tree::of(&changed(&["src/ui/widgets/tab.rs"]));

    insta::assert_snapshot!(drawn(&tree));
}

#[test]
fn a_directory_that_branches_stops_being_compacted_there() {
    let tree = Tree::of(&changed(&["src/ui/tab.rs", "src/ui/review.rs", "src/git.rs"]));

    insta::assert_snapshot!(drawn(&tree));
}

#[test]
fn directories_come_before_files_at_every_depth() {
    let tree = Tree::of(&changed(&["README.md", "src/git.rs", "Cargo.toml"]));

    insta::assert_snapshot!(drawn(&tree));
}

#[test]
fn shutting_a_directory_hides_everything_beneath_it() {
    let mut tree = Tree::of(&changed(&["src/ui/tab.rs", "tests/ui.rs"]));
    let src = tree.row_of(&PathBuf::from("src/ui")).expect("src/ui is a row");
    tree.toggle(src);

    insta::assert_snapshot!(drawn(&tree));
}

#[test]
fn toggling_a_file_does_nothing() {
    let mut tree = Tree::of(&changed(&["src/git.rs"]));
    let before = drawn(&tree);
    let file = tree.row_of(&PathBuf::from("src/git.rs")).expect("src/git.rs is a row");
    tree.toggle(file);

    assert_eq!(drawn(&tree), before);
}
