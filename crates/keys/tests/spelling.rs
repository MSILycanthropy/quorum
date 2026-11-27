//! That keys are written one way, read back to the same key, and refused when no key could match.

use keys::{Error, Key, Rebindings, Sequence};
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

fn sequence(written: &str) -> Sequence {
    written.parse().unwrap_or_else(|error| panic!("{written} should read: {error}"))
}

fn refused(written: &str) -> Error {
    match written.parse::<Sequence>() {
        Ok(sequence) => panic!("{written} was read as {sequence}"),
        Err(error) => error,
    }
}

fn pressed(code: KeyCode, modifiers: KeyModifiers) -> Sequence {
    Sequence::from(Key::from(KeyEvent::new(code, modifiers)))
}

#[test]
fn every_key_prints_the_way_it_is_read_back() {
    for written in [
        "?", "C-p", "S-Tab", "S-←", "PgDn", "Space", "Enter", "gg", "]c", "ZZ", "C-x C-s", "F7",
        "S-F7", "A-z", "C-S-c", "g Enter", "e n d",
    ] {
        assert_eq!(sequence(written).to_string(), written);
    }
}

#[test]
fn the_long_spelling_reads_as_the_short_one() {
    assert_eq!(sequence("ctrl-d"), sequence("C-d"));
    assert_eq!(sequence("shift-F7"), sequence("S-F7"));
    assert_eq!(sequence("alt-z"), sequence("A-z"));
    assert_eq!(sequence("shift-Tab"), sequence("S-Tab"));
    assert_eq!(sequence("ctrl-shift-c"), sequence("C-S-c"));
    assert_eq!(sequence("ctrl-C"), sequence("C-S-c"));
    assert_eq!(sequence("Down"), sequence("↓"));
}

#[test]
fn a_run_of_characters_is_one_key_each() {
    assert_eq!(sequence("]c").keys().len(), 2);
    assert_eq!(sequence("ctrl-x ctrl-s").keys().len(), 2);
}

#[test]
fn a_key_name_wins_over_the_characters_that_spell_it() {
    assert_eq!(
        sequence("F7").keys(),
        [Key::new(KeyCode::F(7), KeyModifiers::NONE).expect("F7 is a key")]
    );
    assert_eq!(sequence("End").keys().len(), 1);
}

#[test]
fn a_key_name_in_the_wrong_case_is_refused_with_its_spelling() {
    let error = refused("down");

    assert!(matches!(error, Error::Spelt { name: "Down", .. }));
    assert_eq!(
        error.to_string(),
        "down is not a key name: write Down, or put spaces between the keys"
    );
    assert!(matches!(refused("f7"), Error::Spelt { name: "F7", .. }));
}

#[test]
fn shift_on_a_character_is_refused() {
    assert!(matches!(refused("shift-r"), Error::Shifted(_)));
    assert!(matches!(
        Key::new(KeyCode::Char('r'), KeyModifiers::SHIFT),
        Err(Error::Shifted(written)) if written == "S-r"
    ));
}

#[test]
fn a_modifier_on_a_name_no_key_has_is_refused() {
    assert_eq!(refused("ctrl-Hlep").to_string(), "no key is called Hlep");
}

#[test]
fn esc_can_only_start_a_sequence() {
    assert!(matches!(refused("g Esc"), Error::Escape(_)));
    assert_eq!(sequence("Esc").keys().len(), 1);
}

#[test]
fn nothing_written_is_refused() {
    assert!(matches!(refused(""), Error::Empty));
}

#[test]
fn a_shifted_key_from_the_terminal_matches_the_character_it_produces() {
    assert_eq!(pressed(KeyCode::Char('?'), KeyModifiers::SHIFT), sequence("?"));
    assert_eq!(pressed(KeyCode::Char('N'), KeyModifiers::SHIFT), sequence("N"));
    assert_eq!(pressed(KeyCode::Char('n'), KeyModifiers::SHIFT), sequence("N"));
    assert_eq!(pressed(KeyCode::BackTab, KeyModifiers::SHIFT), sequence("S-Tab"));
    assert_eq!(
        pressed(KeyCode::Char('c'), KeyModifiers::CONTROL | KeyModifiers::SHIFT),
        sequence("ctrl-shift-c")
    );
}

#[test]
fn a_keys_table_reads_from_toml() {
    let rebindings: Rebindings =
        toml::from_str("quit = [\"q\", \"ctrl-c\"]\ncopy-path = []\n").expect("the table reads");

    assert_eq!(rebindings.commands["quit"], vec![sequence("q"), sequence("C-c")]);
    assert_eq!(rebindings.commands["copy-path"], Vec::<Sequence>::new());
}

#[test]
fn a_misspelt_key_in_a_keys_table_points_at_its_line() {
    let error = toml::from_str::<Rebindings>("quit = [\"q\"]\nnext-line = [\"down\"]\n")
        .expect_err("down is not a key");

    let message = error.to_string();
    assert!(message.contains("line 2"), "{message}");
    assert!(message.contains("write Down"), "{message}");
}
