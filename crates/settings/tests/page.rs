//! That the page lists a program's settings and keys, and hands back each change made on it.

use std::collections::BTreeMap;
use std::fmt::Write;

use keys::{Key, Keymap, Rebindings, Sequence, keymap};
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::crossterm::event::{KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use settings::{Change, Page, Section, Styles, Values};
use toml::{Table, Value};

#[derive(Clone, Copy, Debug, clap::ValueEnum)]
enum Listing {
    Open,
    Mine,
}

#[derive(Clone, Copy, Debug, clap::ValueEnum)]
enum Layout {
    SideBySide,
    Unified,
}

#[derive(Debug, clap::Args)]
struct Glean {
    #[arg(long, value_enum, help = "Which pull requests to list first")]
    listing: Option<Listing>,
    #[arg(long, help = "Seconds between refreshes")]
    refresh: Option<usize>,
}

#[derive(Debug, clap::Args)]
struct Diff {
    #[arg(long, value_enum, help = "Side by side, or one column")]
    layout: Option<Layout>,
    #[arg(long, num_args = 0..=1, default_missing_value = "true", help = "Wrap long lines")]
    wrap: Option<bool>,
    #[arg(long, help = "Columns a tab expands to")]
    tab_width: Option<usize>,
    #[arg(long, help = "A Helix theme for the code")]
    syntax_theme: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum Command {
    Quit,
    Comment,
    NextHunk,
}

fn keymap() -> Keymap<Command> {
    keymap! {
        "General" {
            "quit" => Command::Quit, "Quit", ["q"];
        }
        "Review" {
            "comment" => Command::Comment, "Comment on this line", ["c"];
            "next-hunk" => Command::NextHunk, "Next hunk", ["]c", "n"];
        }
    }
    .expect("the keys do not clash")
}

fn table(text: &str) -> Table {
    toml::from_str(text).expect("the table is well formed")
}

fn values() -> Values {
    Values {
        effective: table(
            "listing = \"open\"\nrefresh = 30\n[diff]\nlayout = \"unified\"\nwrap = true\ntab-width = 4\nsyntax-theme = \"\"\n",
        ),
        written: table("[diff]\nlayout = \"unified\"\n"),
        flagged: table("[diff]\nwrap = true\n"),
        defaults: table(
            "listing = \"open\"\nrefresh = 60\n[diff]\nlayout = \"side-by-side\"\nwrap = false\ntab-width = 8\nsyntax-theme = \"\"\n",
        ),
    }
}

fn page() -> Page<Command> {
    let sections = vec![
        Section { title: "Glean", path: Vec::new(), fields: config::fields::<Glean>() },
        Section { title: "Diff", path: vec!["diff"], fields: config::fields::<Diff>() },
    ];

    Page::new(sections, values(), keymap(), Rebindings::default(), Styles::default())
}

fn key(written: &str) -> Key {
    written.parse().unwrap_or_else(|error| panic!("{written} should read: {error}"))
}

fn pressed(page: &mut Page<Command>, keys: &[&str]) -> Vec<Option<Change>> {
    keys.iter()
        .map(|written| {
            let change = page.key(key(written));
            screen(page);
            change
        })
        .collect()
}

fn last(page: &mut Page<Command>, keys: &[&str]) -> Option<Change> {
    pressed(page, keys).pop().flatten()
}

fn screen(page: &mut Page<Command>) -> String {
    let mut terminal = Terminal::new(TestBackend::new(96, 14)).expect("test backend starts");
    terminal.draw(|frame| frame.render_widget(&mut *page, frame.area())).expect("draw succeeds");

    let buffer = terminal.backend().buffer();
    let mut out = String::new();
    for y in 0..buffer.area().height {
        let line: String =
            (0..buffer.area().width).map(|x| buffer[(x, y)].symbol().to_owned()).collect();
        writeln!(out, "{}", line.trim_end()).expect("writing to a String cannot fail");
    }

    out
}

fn setting(path: &[&str], value: Option<Value>) -> Change {
    Change::Setting { path: path.iter().map(|key| (*key).to_owned()).collect(), value }
}

fn rebound(entries: &[(&str, &[&str])]) -> Change {
    let commands: BTreeMap<String, Vec<Sequence>> = entries
        .iter()
        .map(|(name, keys)| {
            ((*name).to_owned(), Sequence::all(keys).expect("the keys are spelt right"))
        })
        .collect();

    Change::Keys(Rebindings { commands })
}

#[test]
fn every_section_is_listed_with_its_settings_their_values_and_where_they_were_set() {
    insta::assert_snapshot!(screen(&mut page()));
}

#[test]
fn a_setting_written_as_its_default_is_not_marked_and_says_what_the_default_is() {
    let mut page = page();
    page.update(Values {
        effective: table("listing = \"open\"\nrefresh = 60\n"),
        written: table("listing = \"open\"\nrefresh = 90\n"),
        ..values()
    });
    let screen = screen(&mut page);

    assert!(screen.contains("   listing"), "{screen}");
    assert!(screen.contains(" • refresh"), "{screen}");
    assert!(screen.contains("default open"), "{screen}");
}

#[test]
fn keys_put_back_as_they_were_are_not_marked() {
    let mut page = page();
    pressed(&mut page, &["Tab", "j", "j", "Enter", "]", "c", "Enter"]);
    page.key(key("a"));
    page.key(key("n"));
    let change = page.key(key("Enter"));
    let screen = screen(&mut page);

    assert_eq!(change, Some(rebound(&[("next-hunk", &["]c", "n"])])));
    assert!(screen.contains("   Next hunk"), "{screen}");
}

#[test]
fn the_keys_tab_lists_every_command_with_its_keys() {
    let mut page = page();
    pressed(&mut page, &["Tab"]);

    insta::assert_snapshot!(screen(&mut page));
}

#[test]
fn a_setting_in_the_top_section_is_written_at_the_top_of_the_file() {
    assert_eq!(
        last(&mut page(), &["l"]),
        Some(setting(&["listing"], Some(Value::String("mine".to_owned()))))
    );
}

#[test]
fn a_setting_in_a_section_is_written_under_its_table() {
    assert_eq!(
        last(&mut page(), &["j", "j", "l"]),
        Some(setting(&["diff", "layout"], Some(Value::String("side-by-side".to_owned()))))
    );
}

#[test]
fn stepping_past_the_last_choice_comes_round_to_the_first() {
    assert_eq!(
        last(&mut page(), &["j", "j", "Enter"]),
        Some(setting(&["diff", "layout"], Some(Value::String("side-by-side".to_owned()))))
    );
}

#[test]
fn enter_on_a_switch_turns_it_the_other_way() {
    assert_eq!(
        last(&mut page(), &["j", "j", "j", "Enter"]),
        Some(setting(&["diff", "wrap"], Some(Value::Boolean(false))))
    );
}

#[test]
fn a_number_steps_by_one_and_never_below_zero() {
    let mut page = page();

    assert_eq!(last(&mut page, &["j", "l"]), Some(setting(&["refresh"], Some(Value::Integer(31)))));
    page.update(Values { effective: table("refresh = 0\n"), ..values() });
    assert_eq!(last(&mut page, &["h"]), Some(setting(&["refresh"], Some(Value::Integer(0)))));
}

#[test]
fn a_number_typed_in_is_kept_on_enter() {
    assert_eq!(
        last(&mut page(), &["j", "j", "j", "j", "Enter", "Backspace", "2", "Enter"]),
        Some(setting(&["diff", "tab-width"], Some(Value::Integer(2))))
    );
}

#[test]
fn something_typed_that_is_not_a_number_stays_open_with_why() {
    let mut page = page();
    let changes = pressed(&mut page, &["j", "j", "j", "j", "Enter", "x", "Enter"]);

    assert_eq!(changes.last(), Some(&None));
    insta::assert_snapshot!(screen(&mut page));
}

#[test]
fn text_typed_in_is_kept_on_enter_and_dropped_on_esc() {
    let mut page = page();
    let theme = &["j", "j", "j", "j", "j", "Enter", "n", "o", "r", "d"];
    let kept = last(&mut page, &[theme.as_slice(), &["Enter"]].concat());
    let dropped = last(&mut page, &[theme.as_slice(), &["Esc"]].concat());

    assert_eq!(
        kept,
        Some(setting(&["diff", "syntax-theme"], Some(Value::String("nord".to_owned()))))
    );
    assert_eq!(dropped, None);
}

#[test]
fn backspace_puts_a_setting_back_to_its_default() {
    assert_eq!(
        last(&mut page(), &["j", "j", "Backspace"]),
        Some(setting(&["diff", "layout"], None))
    );
}

#[test]
fn keys_pressed_for_a_command_replace_the_ones_it_had() {
    assert_eq!(
        last(&mut page(), &["Tab", "j", "j", "Enter", "J", "Enter"]),
        Some(rebound(&[("next-hunk", &["J"])]))
    );
}

#[test]
fn keys_added_to_a_command_go_beside_the_ones_it_had() {
    assert_eq!(
        last(&mut page(), &["Tab", "j", "j", "a", "F7", "Enter"]),
        Some(rebound(&[("next-hunk", &["]c", "n", "F7"])]))
    );
}

#[test]
fn a_command_can_be_left_with_no_keys_and_put_back() {
    let mut page = page();

    assert_eq!(last(&mut page, &["Tab", "j", "d"]), Some(rebound(&[("comment", &[])])));
    assert_eq!(last(&mut page, &["Backspace"]), Some(rebound(&[])));
}

#[test]
fn keys_another_command_has_are_refused_with_why_and_nothing_changes() {
    let mut page = page();
    let changes = pressed(&mut page, &["Tab", "j", "Enter", "q", "Enter"]);

    assert_eq!(changes.last(), Some(&None));
    insta::assert_snapshot!(screen(&mut page));
}

#[test]
fn keys_the_program_refuses_are_put_back_as_they_were() {
    let mut page = page();
    pressed(&mut page, &["Tab", "Enter", "Q", "Enter"]);
    page.refuse("Quit needs a key");

    assert_eq!(last(&mut page, &["a", "Z", "Enter"]), Some(rebound(&[("quit", &["q", "Z"])])));
}

#[test]
fn q_closes_the_page_and_esc_does_too() {
    assert_eq!(last(&mut page(), &["q"]), Some(Change::Closed));
    assert_eq!(last(&mut page(), &["Esc"]), Some(Change::Closed));
}

fn click(page: &mut Page<Command>, column: u16, row: u16) -> Option<Change> {
    screen(page);
    let change = page.mouse(MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column,
        row,
        modifiers: KeyModifiers::NONE,
    });
    screen(page);
    change
}

#[test]
fn clicking_a_tabs_title_shows_that_tab() {
    let mut page = page();
    click(&mut page, 13, 0);
    let keys = screen(&mut page);
    click(&mut page, 3, 0);

    assert!(keys.contains("Comment on this line"), "{keys}");
    assert!(screen(&mut page).contains("syntax-theme"));
}

#[test]
fn clicking_a_row_chooses_it_and_clicking_it_again_changes_it() {
    let mut page = page();

    assert_eq!(click(&mut page, 5, 8), None);
    assert_eq!(
        click(&mut page, 5, 8),
        Some(setting(&["diff", "wrap"], Some(Value::Boolean(false))))
    );
}

#[test]
fn the_wheel_moves_down_the_rows() {
    let mut page = page();
    screen(&mut page);
    page.mouse(MouseEvent {
        kind: MouseEventKind::ScrollDown,
        column: 5,
        row: 5,
        modifiers: KeyModifiers::NONE,
    });

    assert_eq!(
        last(&mut page, &["Enter"]),
        Some(setting(&["diff", "wrap"], Some(Value::Boolean(false))))
    );
}
