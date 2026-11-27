//! That the diff answers its own keys, hands back what it needs from the program, and draws only where it is put.

use std::fmt::Write;
use std::path::PathBuf;

use keys::{Key, Keymap, Pending, Pressed};
use ratatui::buffer::Buffer;
use ratatui::crossterm::event::{KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use ratatui::layout::Rect;
use ratatui::widgets::Widget;
use sift::command::{self, Command};
use sift::diff::{FileDiff, Side, Source};
use sift::git::{Content, FileChange, Status};
use sift::theme::Theme;
use sift::ui::{Anchor, Location, PEEKING, Presentation, Request, Review, SELECTING};

const PATH: &str = "src/numbers.rs";

fn modified(path: &str, old: &str, new: &str) -> FileChange {
    FileChange {
        status: Status::Modified,
        path: PathBuf::from(path),
        previous: None,
        generated: false,
        content: Content::Text(FileDiff::compute(Source::new(old), Source::new(new))),
    }
}

fn numbered(changed: &[usize]) -> FileChange {
    let (mut old, mut new) = (String::new(), String::new());
    for line in 1..=40 {
        writeln!(old, "line {line}").expect("writing to a String cannot fail");
        if changed.contains(&line) {
            writeln!(new, "line {line} changed").expect("writing to a String cannot fail");
        } else {
            writeln!(new, "line {line}").expect("writing to a String cannot fail");
        }
    }

    modified(PATH, &old, &new)
}

fn two_files() -> Vec<FileChange> {
    vec![
        modified("src/total.rs", "let total = 1;\n", "let total = 42;\n"),
        modified("src/other.rs", "one\ntwo\n", "one\nthree\n"),
    ]
}

/// A review driven by the diff's own keymap, the way a program would drive it.
struct Driven {
    review: Review,
    keymap: Keymap<Command>,
    pending: Pending,
    size: (u16, u16),
}

impl Driven {
    fn new(changes: Vec<FileChange>, size: (u16, u16)) -> Self {
        let review = Review::new(changes, Presentation::default(), Theme::default())
            .expect("the fixtures highlight");
        let mut driven =
            Self { review, keymap: command::keymap(), pending: Pending::default(), size };
        driven.screen();

        driven
    }

    fn typed(&mut self, keys: &[&str]) -> Vec<Request> {
        keys.iter()
            .map(|written| {
                let key: Key = written.parse().unwrap_or_else(|error| panic!("{written}: {error}"));
                let holding = self.review.holding();
                let request = match self.keymap.press(&mut self.pending, key, &holding) {
                    Pressed::Command { command, count } => {
                        self.review.repeated(command, count).expect("the fixtures highlight")
                    }
                    Pressed::Pending | Pressed::Nothing => Request::Nothing,
                };
                self.screen();
                request
            })
            .collect()
    }

    fn plain(&mut self, keys: &str) -> Vec<Request> {
        let keys: Vec<String> = keys.chars().map(String::from).collect();
        let keys: Vec<&str> = keys.iter().map(String::as_str).collect();
        self.typed(&keys)
    }

    fn screen(&mut self) -> String {
        let (width, height) = self.size;
        drawn(&mut self.review, Rect::new(0, 0, width, height))
    }

    fn line(&self) -> Option<usize> {
        self.review.cursor().map(|anchor| anchor.line)
    }

    fn first_shown(&mut self) -> String {
        let screen = self.screen();
        let row = screen.lines().nth(1).unwrap_or_default();

        row.split_once('│').map_or(row, |(_, diff)| diff).trim().to_owned()
    }
}

fn drawn(review: &mut Review, area: Rect) -> String {
    let mut buffer = Buffer::empty(area);
    review.render(area, &mut buffer);

    text(&buffer)
}

fn text(buffer: &Buffer) -> String {
    let mut out = String::new();
    for y in 0..buffer.area.height {
        let line: String =
            (0..buffer.area.width).map(|x| buffer[(x, y)].symbol().to_owned()).collect();
        writeln!(out, "{}", line.trim_end()).expect("writing to a String cannot fail");
    }

    out
}

fn unfolded() -> Driven {
    let mut driven = Driven::new(vec![numbered(&[10, 30])], (100, 11));
    driven.typed(&["z", "R", "g", "g"]);

    driven
}

fn copied(text: &str) -> Request {
    Request::Copy(text.to_owned())
}

#[test]
fn a_count_repeats_a_move() {
    let mut driven = unfolded();
    driven.plain("5j");

    assert_eq!(driven.line(), Some(6));
}

#[test]
fn gg_and_g_with_a_count_go_to_that_line() {
    let mut driven = unfolded();
    driven.plain("17G");
    let after_capital = driven.line();
    driven.plain("4gg");

    assert_eq!((after_capital, driven.line()), (Some(17), Some(4)));
}

#[test]
fn ctrl_home_and_ctrl_end_go_to_either_end() {
    let mut driven = unfolded();
    driven.typed(&["C-End"]);
    let bottom = driven.line();
    driven.typed(&["C-Home"]);

    assert_eq!((bottom, driven.line()), (Some(41), Some(1)));
}

#[test]
fn yy_copies_the_code_on_the_cursors_line() {
    let mut driven = unfolded();

    assert_eq!(driven.plain("jyy").last(), Some(&copied("line 2")));
}

#[test]
fn y_in_a_selection_copies_the_code_on_every_line_selected() {
    let mut driven = unfolded();

    assert_eq!(driven.plain("Vjjy").last(), Some(&copied("line 1\nline 2\nline 3")));
}

#[test]
fn ctrl_c_copies_only_while_something_is_selected() {
    let mut driven = unfolded();
    driven.plain("Vj");
    let selected = driven.typed(&["C-c"]);
    driven.typed(&["Esc"]);
    let unselected = driven.typed(&["C-c"]);

    assert_eq!(selected, [copied("line 1\nline 2")]);
    assert_eq!(unselected, [Request::Nothing]);
}

#[test]
fn yl_and_yp_copy_where_the_cursor_is_and_the_path() {
    let mut driven = unfolded();
    let location = driven.plain("jjyl");
    let path = driven.plain("yp");

    assert_eq!(location.last(), Some(&copied(&format!("{PATH}:3"))));
    assert_eq!(path.last(), Some(&copied(PATH)));
}

#[test]
fn shift_down_selects_and_a_plain_move_lets_go() {
    let mut driven = unfolded();
    driven.typed(&["S-↓", "S-↓"]);
    let selected = driven.review.selection().map(|selection| selection.lines);
    driven.typed(&["↓"]);

    assert_eq!(selected, Some(1..=3));
    assert_eq!(driven.review.selection().map(|selection| selection.lines), Some(4..=4));
}

#[test]
fn a_selection_begun_with_v_keeps_growing_with_any_move() {
    let mut driven = unfolded();
    driven.typed(&["v", "↓", "j"]);

    assert_eq!(driven.review.selection().map(|selection| selection.lines), Some(1..=3));
}

#[test]
fn selecting_holds_while_lines_are_selected_and_escape_lets_go() {
    let mut driven = unfolded();
    driven.plain("jVj");
    let holding = driven.review.holding();
    driven.typed(&["Esc"]);

    assert_eq!(holding, [SELECTING]);
    assert_eq!(driven.review.holding(), Vec::<&str>::new());
    assert_eq!(driven.review.selection().map(|selection| selection.lines), Some(3..=3));
}

#[test]
fn zt_puts_the_cursors_line_at_the_top() {
    let mut driven = unfolded();
    driven.plain("19jzt");

    assert!(driven.first_shown().starts_with("20 line 20"), "{}", driven.screen());
}

#[test]
fn ctrl_e_scrolls_a_line_and_leaves_the_cursor_where_it_is() {
    let mut driven = unfolded();
    driven.plain("3j");
    driven.typed(&["C-e"]);

    assert!(driven.first_shown().starts_with("2 line 2"), "{}", driven.screen());
    assert_eq!(driven.line(), Some(4));
}

#[test]
fn zc_folds_an_opened_run_back_up() {
    let mut driven = Driven::new(vec![numbered(&[10, 30])], (100, 11));
    let folded = driven.screen();
    driven.typed(&["g", "g", "Enter", "j", "j"]);
    let opened = driven.screen();
    driven.plain("zc");

    assert_ne!(opened, folded);
    assert_eq!(driven.screen(), folded);
}

#[test]
fn zm_folds_everything_that_zr_opened() {
    let mut driven = Driven::new(vec![numbered(&[10, 30])], (100, 11));
    let folded = driven.screen();
    driven.plain("zR");
    let unfolded = driven.screen();
    driven.plain("zM");

    assert_ne!(unfolded, folded);
    assert_eq!(driven.screen(), folded);
}

#[test]
fn going_to_a_line_lands_on_it() {
    let mut driven = unfolded();
    driven.review.go_to_line(23).expect("the fixtures highlight");
    driven.screen();

    assert_eq!(
        driven.review.cursor(),
        Some(Anchor { path: PathBuf::from(PATH), side: Side::New, line: 23 })
    );
}

#[test]
fn previous_unviewed_goes_back_to_the_unviewed_file_before() {
    let mut driven = Driven::new(
        vec![
            modified("src/a.rs", "a\n", "b\n"),
            modified("src/b.rs", "a\n", "b\n"),
            modified("src/c.rs", "a\n", "b\n"),
        ],
        (100, 11),
    );
    driven.typed(&["Tab", "Tab"]);

    assert_eq!(driven.typed(&["[", "u", "y", "p"]).last(), Some(&copied("src/b.rs")));
}

#[test]
fn showing_a_changed_file_selects_it() {
    let mut driven = Driven::new(two_files(), (100, 5));

    assert_eq!(
        driven.review.show(&PathBuf::from("src/total.rs"), None).ok(),
        Some(Request::Nothing)
    );
    assert_eq!(driven.plain("yp").last(), Some(&copied("src/total.rs")));
}

#[test]
fn showing_a_file_outside_the_review_asks_for_it_at_the_line_named() {
    let mut driven = Driven::new(two_files(), (100, 5));

    assert_eq!(
        driven.review.show(&PathBuf::from("src/git.rs"), Some(142)).ok(),
        Some(Request::Open { path: PathBuf::from("src/git.rs"), line: Some(142) })
    );
}

#[test]
fn the_location_is_the_file_on_screen_at_the_cursors_line() {
    let mut driven = Driven::new(two_files(), (100, 5));
    driven.plain("j");

    assert_eq!(
        driven.review.location(),
        Some(Location { path: PathBuf::from("src/other.rs"), lines: Some(2..=2) })
    );
    assert_eq!(driven.plain("Y"), [copied("src/other.rs:2")]);
}

#[test]
fn a_location_reads_as_a_path_a_line_or_a_run_of_lines() {
    let location = |lines| Location { path: PathBuf::from("src/a.rs"), lines };

    assert_eq!(location(None).to_string(), "src/a.rs");
    assert_eq!(location(Some(4..=4)).to_string(), "src/a.rs:4");
    assert_eq!(location(Some(4..=9)).to_string(), "src/a.rs:4-9");
}

#[test]
fn control_b_hides_and_shows_the_file_list() {
    let mut driven = Driven::new(two_files(), (100, 5));
    let shown = driven.screen();
    driven.typed(&["C-b"]);
    let hidden = driven.screen();
    driven.typed(&["C-b"]);

    assert!(shown.contains("CHANGES") && !hidden.contains("CHANGES"), "{hidden}");
    assert_eq!(driven.screen(), shown);
}

#[test]
fn a_file_outside_the_review_holds_peeking_and_escape_closes_it() {
    let mut driven = Driven::new(two_files(), (100, 5));
    let content = Content::Text(FileDiff::compute(Source::new("a\n"), Source::new("a\n")));
    driven.review.peek(PathBuf::from("src/git.rs"), &content, None).expect("the file highlights");
    let holding = driven.review.holding();
    driven.typed(&["Esc"]);

    assert_eq!(holding, [PEEKING]);
    assert_eq!(driven.review.peeked(), None);
    assert_eq!(driven.review.holding(), Vec::<&str>::new());
}

#[test]
fn the_cursor_on_a_fold_has_no_anchor() {
    let mut text = String::new();
    for line in 1..=30 {
        writeln!(text, "line {line}").expect("writing to a String cannot fail");
    }
    let changed = text.replace("line 15\n", "line 15 changed\n");
    let mut driven = Driven::new(vec![modified("src/middle.rs", &text, &changed)], (100, 11));
    let folded = driven.review.cursor();
    driven.plain("o");

    assert_eq!(folded, None);
    assert_eq!(
        driven.review.cursor(),
        Some(Anchor { path: PathBuf::from("src/middle.rs"), side: Side::New, line: 1 })
    );
}

#[test]
fn shifted_h_and_l_and_shifted_arrows_switch_the_cursors_side() {
    for (old, new) in [("H", "L"), ("S-←", "S-→")] {
        let mut driven = Driven::new(two_files(), (100, 5));
        driven.typed(&["j", old]);
        let left = driven.review.cursor();
        driven.typed(&[new]);

        assert_eq!(
            left,
            Some(Anchor { path: PathBuf::from("src/other.rs"), side: Side::Old, line: 2 })
        );
        assert_eq!(
            driven.review.cursor(),
            Some(Anchor { path: PathBuf::from("src/other.rs"), side: Side::New, line: 2 })
        );
    }
}

#[test]
fn marking_a_file_viewed_says_which_and_moves_on() {
    let mut driven = Driven::new(two_files(), (100, 5));

    assert_eq!(
        driven.plain("xx"),
        [
            Request::Viewed(PathBuf::from("src/other.rs")),
            Request::Viewed(PathBuf::from("src/total.rs"))
        ]
    );
}

#[test]
fn unmarking_a_file_marked_viewed_says_which() {
    let mut driven = Driven::new(two_files(), (100, 5));
    driven.review.replace_viewed(vec![PathBuf::from("src/other.rs")]);

    assert_eq!(driven.plain("x"), [Request::Unviewed(PathBuf::from("src/other.rs"))]);
}

#[test]
fn replacing_the_viewed_marks_names_only_changed_files() {
    let mut driven = Driven::new(two_files(), (100, 5));
    driven.plain("x");
    driven.review.replace_viewed(vec![PathBuf::from("src/total.rs"), PathBuf::from("src/gone.rs")]);

    insta::assert_snapshot!(driven.screen());
}

#[test]
fn a_viewed_mark_lapses_when_the_file_changes() {
    let mut driven = Driven::new(two_files(), (100, 5));
    driven
        .review
        .replace_viewed(vec![PathBuf::from("src/total.rs"), PathBuf::from("src/other.rs")]);
    driven
        .review
        .refresh(vec![
            modified("src/total.rs", "let total = 1;\n", "let total = 42;\n"),
            modified("src/other.rs", "one\ntwo\n", "one\nfour\n"),
        ])
        .expect("the fixtures highlight");

    insta::assert_snapshot!(driven.screen());
}

#[test]
fn the_status_says_how_many_files_and_hunks_and_how_they_are_laid_out() {
    let driven = Driven::new(two_files(), (100, 5));

    assert_eq!(driven.review.status(), "2 files · 1 hunks · side by side");
}

const SCREEN: Rect = Rect { x: 0, y: 0, width: 100, height: 9 };
const GIVEN: Rect = Rect { x: 8, y: 1, width: 92, height: 8 };

fn hosted(review: &mut Review) -> String {
    let mut buffer = Buffer::empty(SCREEN);
    buffer.set_string(0, 0, "HOST BAR · pull request #42", ratatui::style::Style::new());
    for y in GIVEN.y..GIVEN.bottom() {
        buffer.set_string(0, y, "│ host │", ratatui::style::Style::new());
    }
    review.render(GIVEN, &mut buffer);

    text(&buffer)
}

fn long() -> Review {
    let mut old = String::new();
    let mut new = String::new();
    for line in 1..=40 {
        writeln!(old, "line {line}").expect("writing to a String cannot fail");
        writeln!(new, "line {line} changed").expect("writing to a String cannot fail");
    }

    Review::new(
        vec![modified("src/long.rs", &old, &new)],
        Presentation::default(),
        Theme::default(),
    )
    .expect("the fixtures highlight")
}

fn at(kind: MouseEventKind, column: u16, row: u16) -> MouseEvent {
    MouseEvent { kind, column, row, modifiers: KeyModifiers::NONE }
}

#[test]
fn a_review_drawn_beside_and_below_the_programs_bars_leaves_them_alone() {
    let mut driven = Driven::new(two_files(), (100, 5));

    insta::assert_snapshot!(hosted(&mut driven.review));
}

#[test]
fn a_click_is_measured_from_where_the_review_was_drawn() {
    let mut driven = Driven::new(two_files(), (100, 5));
    hosted(&mut driven.review);
    driven
        .review
        .mouse(at(MouseEventKind::Down(MouseButton::Left), GIVEN.x + 6, GIVEN.y + 3))
        .expect("the fixtures highlight");

    assert_eq!(driven.review.run(Command::CopyPath).ok(), Some(copied("src/total.rs")));
}

#[test]
fn scrolling_outside_the_review_leaves_the_diff_where_it_was() {
    let mut review = long();
    let before = hosted(&mut review);
    review.mouse(at(MouseEventKind::ScrollDown, 50, 0)).expect("the fixtures highlight");

    assert_eq!(hosted(&mut review), before);
}

#[test]
fn scrolling_over_the_review_scrolls_the_diff() {
    let mut review = long();
    let before = hosted(&mut review);
    review.mouse(at(MouseEventKind::ScrollDown, 50, 4)).expect("the fixtures highlight");

    assert_ne!(hosted(&mut review), before);
}

#[test]
fn nothing_is_drawn_outside_the_area_however_small_it_is() {
    let screen = Rect::new(0, 0, 54, 9);
    for width in 0..=48 {
        for height in 0..=6 {
            let mut driven = Driven::new(two_files(), (100, 5));
            let given = Rect::new(3, 2, width, height);
            let mut buffer = Buffer::empty(screen);
            driven.review.render(given, &mut buffer);

            for position in screen.positions().filter(|position| !given.contains(*position)) {
                assert_eq!(
                    buffer[position],
                    ratatui::buffer::Cell::default(),
                    "{given:?} drew at {position:?}"
                );
            }
        }
    }
}
