//! That Sift's own keys, palette rows and status line say what Sift means, with no screen involved.

use std::collections::BTreeMap;
use std::path::PathBuf;

use hints::{Footer, hints};
use keys::{Key, Keymap, Pending, Pressed, Rebindings, Sequence};
use picker::Entry;
use sift::command::{self, Command};
use sift::frame::{self, Action, Target};
use sift::ui::{Icon, PEEKING, SELECTING};

fn keymap() -> Keymap<Action> {
    frame::keymap().expect("Sift's own keys keep clear of the diff's")
}

fn pressed(keymap: &Keymap<Action>, keys: &[&str], holding: &[&str]) -> Option<Action> {
    let mut pending = Pending::default();
    let mut last = None;
    for written in keys {
        let key: Key = written.parse().unwrap_or_else(|error| panic!("{written}: {error}"));
        last = match keymap.press(&mut pending, key, holding) {
            Pressed::Command { command, .. } => Some(command),
            Pressed::Pending | Pressed::Nothing => None,
        };
    }

    last
}

fn rebinding(entries: &[(&str, &[&str])]) -> Rebindings {
    let commands: BTreeMap<String, Vec<Sequence>> = entries
        .iter()
        .map(|(name, keys)| {
            ((*name).to_owned(), Sequence::all(keys).expect("the keys are spelt right"))
        })
        .collect();

    Rebindings { commands }
}

#[test]
fn sifts_keys_and_the_diffs_go_together_without_a_clash() {
    assert!(frame::keymap().is_ok());
}

#[test]
fn ctrl_c_copies_while_something_is_selected_and_quits_otherwise() {
    let keymap = keymap();

    assert_eq!(pressed(&keymap, &["C-c"], &[SELECTING]), Some(Action::Diff(Command::Copy)));
    assert_eq!(pressed(&keymap, &["C-c"], &[]), Some(Action::Quit));
}

#[test]
fn escape_lets_go_of_a_selection_then_closes_an_outside_file_then_quits() {
    let keymap = keymap();

    assert_eq!(
        pressed(&keymap, &["Esc"], &[SELECTING, PEEKING]),
        Some(Action::Diff(Command::ClearSelection))
    );
    assert_eq!(pressed(&keymap, &["Esc"], &[PEEKING]), Some(Action::Diff(Command::ClosePeek)));
    assert_eq!(pressed(&keymap, &["Esc"], &[]), Some(Action::Quit));
}

#[test]
fn vims_ways_of_quitting_quit() {
    let keymap = keymap();

    assert_eq!(pressed(&keymap, &["Z", "Z"], &[]), Some(Action::Quit));
    assert_eq!(pressed(&keymap, &["Z", "Q"], &[]), Some(Action::Quit));
    for written in ["q!", "qa", "qa!", "wq", "wqa", "x"] {
        assert_eq!(
            frame::ex(written).map(|entry| entry.target),
            Some(Target::Action(Action::Quit))
        );
    }
}

#[test]
fn a_number_typed_in_the_palette_is_a_line_to_go_to() {
    assert_eq!(frame::ex(" 23 "), Some(Entry::new(Target::Line(23), "Go to line 23")));
    assert_eq!(frame::ex("0"), None);
    assert_eq!(frame::ex("quit"), None);
}

#[test]
fn ctrl_g_asks_for_a_line_to_go_to() {
    assert_eq!(pressed(&keymap(), &["C-g"], &[]), Some(Action::GoToLine));
}

#[test]
fn a_path_with_a_line_after_it_names_both() {
    assert_eq!(frame::located("src/git.rs:142:7"), ("src/git.rs", Some(142)));
    assert_eq!(frame::located("./src/git.rs:142"), ("src/git.rs", Some(142)));
    assert_eq!(frame::located("src/git.rs"), ("src/git.rs", None));
    assert_eq!(frame::located("toggle:layout"), ("toggle:layout", None));
}

#[test]
fn finding_lists_changed_files_before_unchanged_ones_and_each_once() {
    let known =
        [PathBuf::from("src/a.rs"), PathBuf::from("src/total.rs"), PathBuf::from("README.md")];
    let entries = frame::files(vec![PathBuf::from("src/total.rs")], &known);
    let labels: Vec<&str> = entries.iter().map(|entry| entry.label.as_str()).collect();

    assert_eq!(labels, ["src/total.rs", "src/a.rs", "README.md"]);
    assert_eq!(entries[0].target, Target::File(PathBuf::from("src/total.rs")));
}

#[test]
fn the_palette_lists_every_command_but_itself_with_its_keys() {
    let entries = frame::commands(&keymap(), &[]);

    assert!(
        entries.contains(&Entry::new(Target::Action(Action::Quit), "Quit").detail("q ZZ ZQ C-c"))
    );
    assert!(entries.iter().all(|entry| entry.target != Target::Action(Action::Palette)));
}

#[test]
fn sifts_palette_leaves_the_annotation_commands_out_and_the_diffs_keymap_has_them() {
    let entries = frame::commands(&keymap(), &[]);
    let diff = command::keymap();

    assert!(entries.iter().all(|entry| entry.label != "Next annotation"));
    assert!(diff.bindings().iter().any(|binding| binding.label == "Next annotation"));
}

#[test]
fn a_line_to_go_to_leads_only_where_commands_are_listed() {
    let commands = frame::commands(&keymap(), &[]);
    let files = frame::files(vec![PathBuf::from("src/a.rs")], &[]);

    assert_eq!(frame::leading("12", &commands), [Entry::new(Target::Line(12), "Go to line 12")]);
    assert_eq!(frame::leading("12", &files), []);
}

#[test]
fn the_footer_names_the_keys_to_know_in_order() {
    let footer = Footer::picking(&keymap(), &Pending::default(), &[], &frame::HINTED);

    assert_eq!(
        footer.hints,
        hints![
            "n" => "hunk",
            "Tab" => "file",
            "x" => "viewed",
            "f" => "files",
            "/" => "find",
            "e" => "edit",
            "s" => "layout",
            "C-," => "keys" pinned,
            "q" => "quit" pinned,
        ]
    );
}

#[test]
fn a_command_rebound_to_no_keys_leaves_the_footer_but_stays_in_the_palette() {
    let keymap =
        frame::rebound(&keymap(), &rebinding(&[("edit", &[])])).expect("edit can lose its keys");
    let footer = Footer::picking(&keymap, &Pending::default(), &[], &frame::HINTED);
    let entries = frame::commands(&keymap, &[]);

    assert!(footer.hints.iter().all(|hint| hint.label != "edit"));
    assert!(entries.contains(&Entry::new(Target::Action(Action::Edit), "Open in $EDITOR")));
}

#[test]
fn rebinding_quit_to_no_keys_is_refused() {
    let error =
        frame::rebound(&keymap(), &rebinding(&[("quit", &[])])).expect_err("quit needs a key");

    assert_eq!(error.to_string(), "Quit needs a key");
}

#[test]
fn the_status_names_the_branch_what_is_compared_and_the_review() {
    assert_eq!(
        frame::status(Some("main"), "HEAD → worktree", "2 files · side by side"),
        format!("{} main · HEAD → worktree · 2 files · side by side", Icon::BRANCH)
    );
    assert_eq!(frame::status(None, "", "1 file"), format!("{} detached · 1 file", Icon::BRANCH));
}
