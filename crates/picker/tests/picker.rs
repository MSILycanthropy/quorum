//! That the picker finds what was typed, best first, and hands back what was chosen.

use std::collections::BTreeMap;
use std::fmt::Write;

use hints::{Hint, hints};
use keys::{Key, Keymap, Rebindings, Sequence, keymap};
use picker::{Entry, Outcome, Picker};
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::widgets::Widget;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum Pick {
    Track(u8),
    Seek(u8),
}

fn tracks() -> Vec<Entry<Pick>> {
    ["Blue in Green", "So What", "Freddie Freeloader", "jazz/Blue Train", "Green Onions"]
        .into_iter()
        .zip(0..)
        .map(|(title, id)| Entry::new(Pick::Track(id), title))
        .collect()
}

fn opened() -> Picker<Pick> {
    let mut picker = Picker::default();
    picker.open("play", tracks());

    picker
}

fn key(written: &str) -> Key {
    written.parse().unwrap_or_else(|error| panic!("{written} should read: {error}"))
}

fn typed(picker: &mut Picker<Pick>, text: &str) -> Vec<Outcome<Pick>> {
    text.chars().map(|character| picker.key(key(&character.to_string()))).collect()
}

fn labels(picker: &Picker<Pick>) -> Vec<String> {
    let mut shown = Vec::new();
    let mut picker = picker.clone();
    while let Some(entry) = picker.selected() {
        if shown.contains(&entry.label) {
            break;
        }
        shown.push(entry.label.clone());
        picker.key(key("↓"));
    }

    shown
}

fn screen(picker: &Picker<Pick>, width: u16, height: u16) -> String {
    let area = Rect::new(0, 0, width, height);
    let mut buffer = Buffer::empty(area);
    picker.render(area, &mut buffer);
    let mut out = String::new();
    for y in 0..height {
        let line: String = (0..width).map(|x| buffer[(x, y)].symbol().to_owned()).collect();
        writeln!(out, "{}", line.trim_end()).expect("writing to a String cannot fail");
    }

    out
}

#[test]
fn nothing_typed_lists_every_entry_in_the_order_given() {
    assert_eq!(
        labels(&opened()),
        ["Blue in Green", "So What", "Freddie Freeloader", "jazz/Blue Train", "Green Onions"]
    );
}

#[test]
fn a_match_at_the_start_beats_one_in_the_middle_which_beats_letters_in_order() {
    let mut picker = Picker::default();
    let given = ["Gallop On", "Mango", "Go Slow", "Blue"];
    picker.open(
        "play",
        given.into_iter().zip(0..).map(|(title, id)| Entry::new(Pick::Track(id), title)).collect(),
    );
    typed(&mut picker, "go");

    assert_eq!(labels(&picker), ["Go Slow", "Mango", "Gallop On"]);
}

#[test]
fn what_follows_the_last_slash_counts_as_the_start() {
    let mut picker = opened();
    typed(&mut picker, "blue t");

    assert_eq!(labels(&picker), ["jazz/Blue Train"]);
}

#[test]
fn entries_that_match_equally_well_keep_the_order_given() {
    let mut picker = opened();
    typed(&mut picker, "e");

    assert_eq!(labels(&picker)[..2], ["Blue in Green".to_owned(), "Freddie Freeloader".to_owned()]);
}

#[test]
fn every_typed_key_says_so_and_digits_are_typed_too() {
    let mut picker = opened();

    assert_eq!(typed(&mut picker, "42"), [Outcome::Typed, Outcome::Typed]);
    assert_eq!(picker.query(), "42");
}

#[test]
fn rows_put_in_the_lead_come_first_until_the_next_key() {
    let mut picker = opened();
    typed(&mut picker, "4");
    picker.lead(vec![Entry::new(Pick::Seek(4), "Seek to 0:04")]);
    let led = labels(&picker);
    typed(&mut picker, "2");

    assert_eq!(led, ["Seek to 0:04"]);
    assert_eq!(labels(&picker), Vec::<String>::new());
}

#[test]
fn a_search_matches_against_what_it_is_given_not_what_was_typed() {
    let mut picker = opened();
    typed(&mut picker, "so what:12");
    picker.search("so what");

    assert_eq!(labels(&picker), ["So What"]);
}

#[test]
fn choosing_hands_back_the_target_and_the_query_and_closes() {
    let mut picker = opened();
    typed(&mut picker, "so");

    assert_eq!(
        picker.key(key("Enter")),
        Outcome::Chosen { target: Pick::Track(1), query: "so".to_owned() }
    );
    assert!(!picker.is_open());
}

#[test]
fn moving_down_chooses_the_next_match() {
    let mut picker = opened();
    picker.key(key("Tab"));
    picker.key(key("C-n"));
    picker.key(key("S-Tab"));

    assert_eq!(picker.selected().map(|entry| entry.target), Some(Pick::Track(1)));
}

#[test]
fn enter_with_nothing_matched_closes() {
    let mut picker = opened();
    typed(&mut picker, "zzz");

    assert_eq!(picker.key(key("Enter")), Outcome::Closed);
    assert!(!picker.is_open());
}

#[test]
fn escape_and_control_c_close() {
    for written in ["Esc", "C-c"] {
        let mut picker = opened();

        assert_eq!(picker.key(key(written)), Outcome::Closed);
        assert!(!picker.is_open());
    }
}

#[test]
fn a_closed_picker_does_nothing_with_keys() {
    let mut picker = Picker::<Pick>::default();

    assert_eq!(picker.key(key("a")), Outcome::Nothing);
}

#[test]
fn an_empty_list_still_opens_and_says_nothing_matches() {
    let mut picker = Picker::<Pick>::default();
    picker.open("play", Vec::new());

    assert!(picker.is_open());
    insta::assert_snapshot!(screen(&picker, 40, 8));
}

#[test]
fn the_title_the_query_and_each_rows_detail_are_drawn() {
    let mut picker = Picker::default();
    picker.open(
        "play",
        tracks().into_iter().map(|entry| entry.detail("Miles Davis")).take(3).collect(),
    );
    typed(&mut picker, "e");

    insta::assert_snapshot!(screen(&picker, 50, 9));
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum Command {
    Play,
    Shuffle,
    Copy,
}

fn commands() -> Keymap<Command> {
    keymap! {
        "Player" {
            "play" => Command::Play, "Play or pause", ["Space", "p"];
            "shuffle" => Command::Shuffle, "Shuffle", [];
            "copy" => Command::Copy, "Copy the title", ["y"], when: "playing";
        }
    }
    .expect("the keys do not clash")
}

#[test]
fn an_entry_from_a_binding_shows_its_label_and_every_key_it_has() {
    let keymap = commands();
    let entries: Vec<_> =
        keymap.bindings().iter().filter_map(|binding| Entry::of(binding, &[])).collect();

    assert_eq!(
        entries,
        [
            Entry::new(Command::Play, "Play or pause").detail("Space p"),
            Entry::new(Command::Shuffle, "Shuffle"),
        ]
    );
}

#[test]
fn a_binding_with_a_condition_is_an_entry_only_while_it_holds() {
    let keymap = commands();
    let copy = &keymap.bindings()[2];

    assert_eq!(Entry::of(copy, &[]), None);
    assert_eq!(
        Entry::of(copy, &["playing"]),
        Some(Entry::new(Command::Copy, "Copy the title").detail("y"))
    );
}

#[test]
fn a_rebound_command_shows_its_new_keys() {
    let commands_rebound =
        BTreeMap::from([("play".to_owned(), Sequence::all(&["P"]).expect("P is a key"))]);
    let keymap = commands().rebound(&Rebindings { commands: commands_rebound }).expect("P is free");

    assert_eq!(
        Entry::of(&keymap.bindings()[0], &[]).map(|entry| entry.detail),
        Some("P".to_owned())
    );
}

#[test]
fn an_entry_can_be_wrapped_in_a_programs_own_target() {
    let entry = Entry::new(3, "Track three").detail("3:20").map(Pick::Track);

    assert_eq!(entry, Entry::new(Pick::Track(3), "Track three").detail("3:20"));
}

#[test]
fn the_hints_name_the_keys_to_choose_and_close() {
    let picker = Picker::<Pick>::default();

    assert_eq!(picker.hints(), hints!["Enter" => "choose", "Esc" => "close" pinned]);
    assert!(picker.hints().iter().all(|hint| hint != &Hint::new("↑", "up")));
}

#[test]
fn an_open_picker_puts_its_own_keys_in_the_footer_and_a_closed_one_leaves_it() {
    let footer = hints::Footer { hints: hints!["q" => "quit"], ..hints::Footer::default() };
    let mut picker = Picker::<Pick>::default();
    let closed = picker.hinting(footer.clone());
    picker.open("play", tracks());

    assert_eq!(closed.hints, hints!["q" => "quit"]);
    assert_eq!(picker.hinting(footer).hints, picker.hints());
}

#[test]
fn nothing_is_drawn_outside_the_area_however_small_it_is() {
    let screen = Rect::new(0, 0, 54, 12);
    for width in 0..=48 {
        for height in 0..=8 {
            let given = Rect::new(3, 2, width, height);
            let mut buffer = Buffer::empty(screen);
            opened().render(given, &mut buffer);

            for y in 0..screen.height {
                for x in 0..screen.width {
                    if !given.contains(ratatui::layout::Position::new(x, y)) {
                        assert_eq!(
                            buffer[(x, y)].symbol(),
                            " ",
                            "{width}x{height} drew at {x},{y}"
                        );
                    }
                }
            }
        }
    }
}

#[test]
fn the_picker_opens_over_the_area_it_is_given_not_the_whole_screen() {
    let mut buffer = Buffer::empty(Rect::new(0, 0, 70, 12));
    opened().render(Rect::new(10, 2, 50, 8), &mut buffer);
    let mut out = String::new();
    for y in 0..12 {
        let line: String = (0..70).map(|x| buffer[(x, y)].symbol().to_owned()).collect();
        writeln!(out, "{}", line.trim_end()).expect("writing to a String cannot fail");
    }

    insta::assert_snapshot!(out);
}
