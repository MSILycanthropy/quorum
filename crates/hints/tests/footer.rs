//! That the footer shows the keys that work now, and keeps the ones that matter most when it is narrow.

use std::collections::BTreeMap;

use hints::{Footer, Hint, hints};
use keys::{Key, Keymap, Pending, Rebindings, Sequence, keymap};
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::text::Line;
use ratatui::widgets::Widget;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum Command {
    Quit,
    NextHunk,
    Top,
    Comment,
    Copy,
}

fn keymap() -> Keymap<Command> {
    keymap! {
        "General" {
            "quit" => Command::Quit, "Quit", ["q"], hint: "quit", pinned: true;
        }
        "Review" {
            "next-hunk" => Command::NextHunk, "Next hunk", ["n", "]c"], hint: "hunk";
            "top" => Command::Top, "Top", ["gg"];
            "comment" => Command::Comment, "Comment on this line", ["c"], hint: "comment";
            "copy" => Command::Copy, "Copy the lines", ["y"], hint: "copy", when: "selecting";
        }
    }
    .expect("the keys do not clash")
}

fn row(footer: &Footer, width: u16) -> String {
    let area = Rect::new(0, 0, width, 1);
    let mut buffer = Buffer::empty(area);
    footer.render(area, &mut buffer);

    (0..width).map(|x| buffer[(x, 0)].symbol().to_owned()).collect::<String>().trim_end().to_owned()
}

fn status(text: &'static str) -> Footer {
    Footer { status: Line::raw(text), ..Footer::of(&keymap(), &Pending::default(), &[]) }
}

#[test]
fn the_hints_are_each_hinted_bindings_first_key_in_keymap_order() {
    assert_eq!(
        row(&status("main · 2 files"), 60),
        " main · 2 files                   q quit  n hunk  c comment"
    );
}

#[test]
fn a_rebound_command_shows_its_new_key() {
    let commands =
        BTreeMap::from([("next-hunk".to_owned(), Sequence::all(&["J"]).expect("J is a key"))]);
    let keymap = keymap().rebound(&Rebindings { commands }).expect("J is free");
    let footer = Footer::of(&keymap, &Pending::default(), &[]);

    assert_eq!(footer.hints[1], Hint::new("J", "hunk"));
}

#[test]
fn a_hint_with_a_condition_shows_only_while_it_holds() {
    let keymap = keymap();
    let idle = Footer::of(&keymap, &Pending::default(), &[]);
    let selecting = Footer::of(&keymap, &Pending::default(), &["selecting"]);

    assert!(!idle.hints.contains(&Hint::new("y", "copy")));
    assert!(selecting.hints.contains(&Hint::new("y", "copy")));
}

#[test]
fn keys_pressed_so_far_take_the_status_and_the_hints_say_what_finishes_them() {
    let keymap = keymap();
    let mut pending = Pending::default();
    keymap.press(&mut pending, "]".parse::<Key>().expect("] is a key"), &[]);
    let footer =
        Footer { status: Line::raw("main · 2 files"), ..Footer::of(&keymap, &pending, &[]) };

    assert_eq!(row(&footer, 40), " ]                          c Next hunk");
}

#[test]
fn a_narrow_row_drops_hints_from_the_end_and_keeps_the_pinned_one() {
    let footer = status("main · 2 files");

    insta::assert_snapshot!(
        [60, 44, 32, 24, 20, 8, 6, 2]
            .map(|width| format!("{width:>2}|{}|", row(&footer, width)))
            .join("\n")
    );
}

#[test]
fn with_no_status_the_hints_start_at_the_left() {
    let footer = Footer {
        hints: hints!["Enter" => "change", "or type", "q" => "close" pinned],
        ..Footer::default()
    };

    assert_eq!(row(&footer, 60), " Enter change  or type  q close");
}

#[test]
fn hints_written_by_hand_are_a_key_and_its_words_words_alone_or_pinned() {
    assert_eq!(
        hints!["type the value", "Enter" => "keep it", "Esc" => "leave it" pinned,],
        vec![
            Hint::new("", "type the value"),
            Hint::new("Enter", "keep it"),
            Hint { pinned: true, ..Hint::new("Esc", "leave it") },
        ]
    );
}

#[test]
fn a_binding_with_no_keys_has_no_hint() {
    let keymap = keymap! { "General" { "quit" => Command::Quit, "Quit", [], hint: "quit"; } }
        .expect("one binding cannot clash");

    assert_eq!(Hint::of(&keymap.bindings()[0], &[]), None);
}

#[test]
fn picking_names_the_hints_in_the_order_named() {
    let footer =
        Footer::picking(&keymap(), &Pending::default(), &[], &["comment", "next-hunk", "top"]);

    assert_eq!(footer.hints, hints!["c" => "comment", "n" => "hunk"]);
}

#[test]
fn picking_still_says_what_finishes_pending_keys() {
    let keymap = keymap();
    let mut pending = Pending::default();
    keymap.press(&mut pending, "]".parse::<Key>().expect("] is a key"), &[]);

    assert_eq!(
        Footer::picking(&keymap, &pending, &[], &["quit"]).hints,
        hints!["c" => "Next hunk"]
    );
}

#[test]
fn a_notice_takes_the_status_place_and_pending_keys_take_the_notice() {
    let noticed = Footer { notice: Some("copied".to_owned()), ..status("main · 2 files") };
    let pending = Footer { pending: "g".to_owned(), ..noticed.clone() };

    assert!(row(&noticed, 60).starts_with(" copied "), "{}", row(&noticed, 60));
    assert!(row(&pending, 60).starts_with(" g "), "{}", row(&pending, 60));
}
