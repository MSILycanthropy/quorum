//! That keys pressed for a new binding are gathered until Enter, and given up on at Esc.

use keys::{Key, Recorded, Recording, Sequence};

fn key(written: &str) -> Key {
    written.parse().unwrap_or_else(|error| panic!("{written} should read: {error}"))
}

fn recorded(keys: &[&str]) -> (Recording, Vec<Recorded>) {
    let mut recording = Recording::default();
    let pressed = keys.iter().map(|written| recording.press(key(written))).collect();

    (recording, pressed)
}

fn sequence(written: &str) -> Sequence {
    written.parse().unwrap_or_else(|error| panic!("{written} should read: {error}"))
}

#[test]
fn keys_pressed_then_enter_are_the_sequence_recorded() {
    let (_, pressed) = recorded(&["]", "c", "Enter"]);

    assert_eq!(pressed.last(), Some(&Recorded::Done(sequence("]c"))));
}

#[test]
fn a_key_with_a_modifier_is_recorded_as_it_was_pressed() {
    let (_, pressed) = recorded(&["C-x", "C-s", "Enter"]);

    assert_eq!(pressed.last(), Some(&Recorded::Done(sequence("C-x C-s"))));
}

#[test]
fn esc_gives_up_and_forgets_the_keys() {
    let (recording, pressed) = recorded(&["g", "Esc"]);

    assert_eq!(pressed, [Recorded::Typing, Recorded::Cancelled]);
    assert!(recording.is_empty());
}

#[test]
fn backspace_takes_the_last_key_back() {
    let (recording, _) = recorded(&["g", "x", "Backspace", "g"]);

    assert_eq!(recording.to_string(), "gg");
}

#[test]
fn enter_with_nothing_pressed_keeps_waiting() {
    let (_, pressed) = recorded(&["Enter"]);

    assert_eq!(pressed, [Recorded::Typing]);
}
