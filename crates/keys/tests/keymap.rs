//! That a key press reaches its command, waits on a prefix, counts, and keeps clear of other bindings.

use std::collections::BTreeMap;

use keys::{Binding, Error, Key, Keymap, Pending, Pressed, Rebindings, Sequence, keymap};

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
    keymap! {
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

fn typed<C: Clone>(keymap: &Keymap<C>, keys: &[&str], holding: &[&str]) -> Vec<Pressed<C>> {
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

fn one(binding: Binding<Command>) -> Keymap<Command> {
    Keymap::new([binding]).expect("one binding cannot clash")
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
        keymap().joined(one(binding(Command::Five, "five", "Five", &["5"]))).expect("5 is free");

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
    let error = keymap()
        .joined(one(binding(Command::Five, "reply", "Reply", &["r"])))
        .expect_err("r is taken");

    assert_eq!(error.to_string(), "r is bound to both Reload and Reply");
}

#[test]
fn a_key_that_starts_another_binding_is_refused() {
    let error =
        keymap().joined(one(binding(Command::Five, "go", "Go", &["g"]))).expect_err("g starts gg");

    assert!(matches!(error, Error::Prefix { first: "Go", second: "Top of the file", .. }));
    assert_eq!(error.to_string(), "g is Go, so gg for Top of the file can never be reached");
}

#[test]
fn two_bindings_with_one_name_are_refused() {
    let error = keymap()
        .joined(one(binding(Command::Five, "quit", "Leave", &["Q"])))
        .expect_err("quit is taken");

    assert!(matches!(error, Error::Name("quit")));
}

#[test]
fn a_key_joined_under_a_condition_wins_over_the_same_key_while_it_holds() {
    let added =
        Binding { when: Some("selecting"), ..binding(Command::Five, "five", "Five", &["j"]) };
    let keymap = keymap().joined(one(added)).expect("j is only taken without a condition");

    assert_eq!(typed(&keymap, &["j"], &["selecting"]), [command(Command::Five)]);
    assert_eq!(typed(&keymap, &["j"], &[]), [command(Command::Down)]);
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
    let error = keymap! {
        "General" {
            "help" => Command::Quit, "Show every key", ["ctrl-Hlep"];
        }
    }
    .expect_err("Hlep is no key");

    assert_eq!(error.to_string(), "help: no key is called Hlep");
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum Action {
    Diff(Command),
    Back,
    Comment,
}

fn host() -> Keymap<Action> {
    keymap! {
        "Review" {
            "back" => Action::Back, "Back to the list", ["Esc", "ctrl-c"];
            "comment" => Action::Comment, "Comment", ["c"];
        }
    }
    .expect("the keys do not clash")
}

fn joined() -> Keymap<Action> {
    host()
        .joined(keymap().retain(|binding| binding.command != Command::Quit).map(Action::Diff))
        .expect("the host's keys keep clear of the ones it keeps")
}

#[test]
fn a_mapped_keymap_keeps_every_name_key_and_condition() {
    let mapped = keymap().map(Action::Diff);
    let unmapped = keymap();

    for (mapped, unmapped) in mapped.bindings().iter().zip(unmapped.bindings()) {
        assert_eq!(mapped.command, Action::Diff(unmapped.command));
        assert_eq!(
            (mapped.name, mapped.label, mapped.section, &mapped.keys, mapped.hint, mapped.when),
            (
                unmapped.name,
                unmapped.label,
                unmapped.section,
                &unmapped.keys,
                unmapped.hint,
                unmapped.when
            )
        );
    }
}

#[test]
fn a_joined_keymap_answers_the_keys_of_both() {
    let keymap = joined();

    assert_eq!(
        typed(&keymap, &["c", "]", "c", "Esc"], &[]),
        [
            Pressed::Command { command: Action::Comment, count: None },
            Pressed::Pending,
            Pressed::Command { command: Action::Diff(Command::NextHunk), count: None },
            Pressed::Command { command: Action::Back, count: None },
        ]
    );
}

#[test]
fn a_key_under_a_condition_may_sit_over_the_same_key_joined_without_one() {
    let keymap = joined();

    assert_eq!(
        typed(&keymap, &["ctrl-c"], &["selecting"]),
        [Pressed::Command { command: Action::Diff(Command::Copy), count: None }]
    );
    assert_eq!(
        typed(&keymap, &["ctrl-c"], &[]),
        [Pressed::Command { command: Action::Back, count: None }]
    );
}

#[test]
fn a_key_with_no_condition_still_answers_while_a_condition_holds() {
    assert_eq!(typed(&keymap(), &["j"], &["selecting"]), [command(Command::Down)]);
}

#[test]
fn joining_refuses_a_key_both_bind_under_one_condition() {
    let error = host().joined(keymap().map(Action::Diff)).expect_err("C-c is quit and back");

    assert!(
        matches!(error, Error::Shared { first: "Back to the list", second: "Quit", .. }),
        "{error}"
    );
}

#[test]
fn joining_refuses_a_key_that_starts_one_the_other_binds() {
    let reaching = keymap! { "Review" { "goto" => Action::Comment, "Go to", ["g"]; } }
        .expect("one binding cannot clash");
    let error = reaching.joined(keymap().map(Action::Diff)).expect_err("g starts gg");

    assert!(matches!(error, Error::Prefix { .. }), "{error}");
}

#[test]
fn joining_refuses_a_name_both_use() {
    let named =
        keymap! { "Review" { "reload" => Action::Comment, "Reload the pull request", ["R"]; } }
            .expect("one binding cannot clash");
    let error = named.joined(keymap().map(Action::Diff)).expect_err("both say reload");

    assert!(matches!(error, Error::Name("reload")), "{error}");
}

#[test]
fn a_joined_keymap_knows_the_names_of_both() {
    let keymap = joined();
    let names: Vec<&str> = keymap.names().collect();

    assert!(names.contains(&"comment") && names.contains(&"next-hunk"), "{names:?}");
}

#[test]
fn a_rebinding_across_a_join_is_refused_under_the_name_written() {
    let error = joined().rebound(&rebinding(&[("next-hunk", &["c"])])).expect_err("c is comment's");

    assert!(matches!(error, Error::Rebinding { ref name, .. } if name == "next-hunk"), "{error}");
}

#[test]
fn a_binding_left_out_with_retain_frees_its_keys() {
    let kept = keymap().retain(|binding| binding.command != Command::Reload);
    let taken = keymap! { "Mine" { "refresh" => Command::Reload, "Refresh", ["r"]; } }
        .expect("one binding cannot clash");

    assert_eq!(typed(&kept, &["r"], &[]), [Pressed::Nothing]);
    assert!(kept.joined(taken).is_ok());
}

#[test]
fn arranging_puts_the_sections_named_first_and_keeps_the_rest_in_their_order() {
    let keymap = joined().arranged(&["Copying", "General"]);
    let sections: Vec<&str> = keymap.bindings().iter().map(|binding| binding.section).collect();

    assert_eq!(
        sections,
        [
            "Copying", "Copying", "General", "Review", "Review", "Moving", "Moving", "Moving",
            "Moving", "Moving"
        ]
    );
}
