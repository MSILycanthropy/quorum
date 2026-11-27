//! That a key press reaches its command, waits on a prefix, counts, and keeps clear of other bindings.

use std::collections::BTreeMap;

use keys::{Binding, Error, Key, Keymap, Pending, Pressed, Rebindings, Sequence};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum Command {
    Quit,
    Down,
    Top,
    Bottom,
    NextHunk,
    NextFile,
    Copy,
    Reload,
    Five,
}

fn keymap() -> Keymap<Command> {
    keys::keymap! {
        "General" {
            "quit" => Command::Quit, "Quit", ["q", "ctrl-c"], hint: "quit";
            "reload" => Command::Reload, "Reload", ["r"];
        }
        "Moving" {
            "down" => Command::Down, "Down a line", ["j", "Down"];
            "top" => Command::Top, "Top of the file", ["gg"];
            "bottom" => Command::Bottom, "Bottom of the file", ["G"];
            "next-hunk" => Command::NextHunk, "Next hunk", ["]c"];
            "next-file" => Command::NextFile, "Next file", ["]f", "Tab"];
        }
        "Copying" {
            "copy" => Command::Copy, "Copy the line", ["yy"];
            "copy-selection" => Command::Copy, "Copy the selection", ["y", "ctrl-c"], when: "selecting";
        }
    }
    .expect("the keys do not clash")
}

fn key(written: &str) -> Key {
    written.parse().unwrap_or_else(|error| panic!("{written} should read: {error}"))
}

fn typed(keymap: &Keymap<Command>, keys: &[&str], holding: &[&str]) -> Vec<Pressed<Command>> {
    let mut pending = Pending::default();

    keys.iter().map(|written| keymap.press(&mut pending, key(written), holding)).collect()
}

fn command(command: Command) -> Pressed<Command> {
    Pressed::Command { command, count: None }
}

fn counted(command: Command, count: usize) -> Pressed<Command> {
    Pressed::Command { command, count: Some(count) }
}

fn binding(
    command: Command,
    name: &'static str,
    label: &'static str,
    keys: &[&str],
) -> Binding<Command> {
    Binding::new(command, name, label, "Host", keys).expect("the keys are spelt right")
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
fn a_single_key_runs_its_command() {
    assert_eq!(typed(&keymap(), &["j"], &[]), [command(Command::Down)]);
}

#[test]
fn a_prefix_waits_for_the_key_that_finishes_it() {
    assert_eq!(typed(&keymap(), &["g", "g"], &[]), [Pressed::Pending, command(Command::Top)]);
}

#[test]
fn esc_cancels_pending_keys_and_nothing_runs() {
    assert_eq!(
        typed(&keymap(), &["]", "Esc", "c"], &[]),
        [Pressed::Pending, Pressed::Nothing, Pressed::Nothing]
    );
}

#[test]
fn a_dead_end_tries_the_last_key_on_its_own() {
    assert_eq!(typed(&keymap(), &["g", "j"], &[]), [Pressed::Pending, command(Command::Down)]);
}

#[test]
fn a_key_nothing_is_bound_to_does_nothing() {
    assert_eq!(typed(&keymap(), &["z"], &[]), [Pressed::Nothing]);
}

#[test]
fn digits_before_a_command_count_it() {
    assert_eq!(typed(&keymap(), &["5", "j"], &[]), [Pressed::Pending, counted(Command::Down, 5)]);
    assert_eq!(
        typed(&keymap(), &["4", "2", "G"], &[]),
        [Pressed::Pending, Pressed::Pending, counted(Command::Bottom, 42)]
    );
    assert_eq!(
        typed(&keymap(), &["3", "]", "c"], &[]),
        [Pressed::Pending, Pressed::Pending, counted(Command::NextHunk, 3)]
    );
}

#[test]
fn zero_does_not_start_a_count() {
    assert_eq!(typed(&keymap(), &["0", "j"], &[]), [Pressed::Nothing, command(Command::Down)]);
}

#[test]
fn a_bound_digit_stays_a_key() {
    let keymap =
        keymap().with([binding(Command::Five, "five", "Five", &["5"])]).expect("5 is free");

    assert_eq!(typed(&keymap, &["5"], &[]), [command(Command::Five)]);
}

#[test]
fn a_dead_end_drops_the_count() {
    assert_eq!(
        typed(&keymap(), &["3", "]", "j"], &[]),
        [Pressed::Pending, Pressed::Pending, command(Command::Down)]
    );
}

#[test]
fn a_binding_under_a_condition_wins_while_the_condition_holds() {
    assert_eq!(typed(&keymap(), &["y"], &["selecting"]), [command(Command::Copy)]);
    assert_eq!(typed(&keymap(), &["ctrl-c"], &["selecting"]), [command(Command::Copy)]);
}

#[test]
fn a_binding_under_a_condition_that_does_not_hold_is_passed_over() {
    assert_eq!(typed(&keymap(), &["y"], &[]), [Pressed::Pending]);
    assert_eq!(typed(&keymap(), &["ctrl-c"], &[]), [command(Command::Quit)]);
}

#[test]
fn keys_shared_by_two_bindings_are_refused() {
    let error =
        keymap().with([binding(Command::Five, "reply", "Reply", &["r"])]).expect_err("r is taken");

    assert_eq!(error.to_string(), "r is bound to both Reload and Reply");
}

#[test]
fn a_key_that_starts_another_binding_is_refused() {
    let error =
        keymap().with([binding(Command::Five, "go", "Go", &["g"])]).expect_err("g starts gg");

    assert!(matches!(error, Error::Prefix { first: "Go", second: "Top of the file", .. }));
    assert_eq!(error.to_string(), "g is Go, so gg for Top of the file can never be reached");
}

#[test]
fn two_bindings_with_one_name_are_refused() {
    let error = keymap()
        .with([binding(Command::Five, "quit", "Leave", &["Q"])])
        .expect_err("quit is taken");

    assert!(matches!(error, Error::Name("quit")));
}

#[test]
fn keys_added_to_a_keymap_may_not_shadow_it_even_under_a_condition() {
    let added =
        Binding { when: Some("selecting"), ..binding(Command::Five, "five", "Five", &["j"]) };

    assert!(matches!(keymap().with([added]), Err(Error::Shared { first: "Down a line", .. })));
}

#[test]
fn what_can_follow_pending_keys_is_listed() {
    let keymap = keymap();
    let mut pending = Pending::default();
    keymap.press(&mut pending, key("]"), &[]);

    let continuations: Vec<(String, &str)> = keymap
        .continuations(&pending, &[])
        .into_iter()
        .map(|continuation| (continuation.keys.to_string(), continuation.label))
        .collect();

    assert_eq!(continuations, [("c".to_owned(), "Next hunk"), ("f".to_owned(), "Next file")]);
}

#[test]
fn nothing_follows_when_nothing_is_pending() {
    assert_eq!(keymap().continuations(&Pending::default(), &[]), []);
}

#[test]
fn pending_keys_show_as_they_were_typed() {
    let keymap = keymap();
    let mut pending = Pending::default();
    keymap.press(&mut pending, key("3"), &[]);
    keymap.press(&mut pending, key("]"), &[]);

    assert_eq!(pending.to_string(), "3]");
}

#[test]
fn a_rebinding_replaces_the_keys_a_command_came_with() {
    let keymap = keymap().rebound(&rebinding(&[("down", &["n"])])).expect("n is free");

    assert_eq!(typed(&keymap, &["n"], &[]), [command(Command::Down)]);
    assert_eq!(typed(&keymap, &["j"], &[]), [Pressed::Nothing]);
}

#[test]
fn a_rebinding_to_nothing_leaves_the_command_without_keys() {
    let keymap = keymap().rebound(&rebinding(&[("reload", &[])])).expect("an empty list is fine");

    assert_eq!(typed(&keymap, &["r"], &[]), [Pressed::Nothing]);
    assert!(keymap.names().any(|name| name == "reload"));
}

#[test]
fn a_rebinding_onto_another_commands_key_is_refused_under_its_own_name() {
    let error = keymap().rebound(&rebinding(&[("reload", &["j"])])).expect_err("j is taken");

    assert_eq!(error.to_string(), "keys.reload: j is bound to both Reload and Down a line");
}

#[test]
fn moving_a_command_off_a_key_frees_it_for_another_in_the_same_table() {
    let keymap = keymap()
        .rebound(&rebinding(&[("reload", &["j"]), ("down", &["n"])]))
        .expect("j moves to reload once down leaves it");

    assert_eq!(typed(&keymap, &["j"], &[]), [command(Command::Reload)]);
}

#[test]
fn a_rebinding_no_keymap_knows_is_refused() {
    let rebindings = rebinding(&[("quit", &["Q"]), ("frobnicate", &["f"])]);

    let error = rebindings.unknown(keymap().names()).expect_err("nothing is called frobnicate");

    assert_eq!(error.to_string(), "keys.frobnicate: no command has this name");
}

#[test]
fn the_macro_fills_in_the_section_hint_and_condition() {
    let keymap = keymap();
    let quit = &keymap.bindings()[0];
    let selection = keymap.bindings().last().expect("the keymap has bindings");

    assert_eq!((quit.section, quit.hint, quit.when), ("General", Some("quit"), None));
    assert_eq!((selection.section, selection.when), ("Copying", Some("selecting")));
    assert_eq!(quit.shown(), "q C-c");
}

#[test]
fn a_misspelt_key_in_the_macro_names_its_binding() {
    let error = keys::keymap! {
        "General" {
            "help" => Command::Quit, "Show every key", ["ctrl-Hlep"];
        }
    }
    .expect_err("Hlep is no key");

    assert_eq!(error.to_string(), "help: no key is called Hlep");
}
