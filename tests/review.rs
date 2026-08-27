//! That every changed file is listed, and that the list drives what is shown.

use std::fmt::Write;
use std::path::PathBuf;

use quorum::color::Rgb;
use quorum::diff::{FileDiff, Source};
use quorum::git::{Content, FileChange, Status};
use quorum::theme::TokyoNight;
use quorum::ui::{Mode, Review};
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::buffer::Buffer;
use ratatui::crossterm::event::{
    KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};
use ratatui::style::Color;

fn modified(path: &str, old: &str, new: &str) -> FileChange {
    FileChange {
        status: Status::Modified,
        path: PathBuf::from(path),
        content: Content::Text(FileDiff::compute(Source::new(old), Source::new(new))),
    }
}

fn added(path: &str, new: &str) -> FileChange {
    FileChange {
        status: Status::Added,
        path: PathBuf::from(path),
        content: Content::Text(FileDiff::compute(Source::new(""), Source::new(new))),
    }
}

fn deleted(path: &str, old: &str) -> FileChange {
    FileChange {
        status: Status::Deleted,
        path: PathBuf::from(path),
        content: Content::Text(FileDiff::compute(Source::new(old), Source::new(""))),
    }
}

fn binary(path: &str) -> FileChange {
    FileChange { status: Status::Modified, path: PathBuf::from(path), content: Content::Binary }
}

fn fixtures() -> Vec<FileChange> {
    vec![
        modified("src/total.rs", "let total = 1;\n", "let total = 42;\n"),
        added("src/new.rs", "fn new() {}\n"),
        deleted("src/gone.rs", "fn gone() {}\n"),
    ]
}

fn tall(path: &str) -> FileChange {
    let mut old = String::new();
    for line in 1..=30 {
        writeln!(old, "line {line}").expect("writing to a String cannot fail");
    }
    let new = old.replace("line 4\n", "line 4 changed\n");

    modified(path, &old, &new)
}

fn at(kind: MouseEventKind, column: u16, row: u16) -> MouseEvent {
    MouseEvent { kind, column, row, modifiers: KeyModifiers::NONE }
}

fn click(column: u16, row: u16) -> MouseEvent {
    at(MouseEventKind::Down(MouseButton::Left), column, row)
}

fn moused(changes: Vec<FileChange>, width: u16, height: u16, mouse: &[MouseEvent]) -> String {
    let mut review =
        Review::new(changes, Mode::SideBySide, false, 4).expect("the fixtures highlight");
    let mut terminal = Terminal::new(TestBackend::new(width, height)).expect("test backend starts");

    terminal.draw(|frame| review.draw(frame, frame.area())).expect("draw succeeds");
    for event in mouse {
        review.mouse(*event).expect("the fixtures highlight");
        terminal.draw(|frame| review.draw(frame, frame.area())).expect("draw succeeds");
    }

    describe(terminal.backend().buffer())
}

fn shown(changes: Vec<FileChange>, width: u16, height: u16, keys: &[KeyCode]) -> String {
    let mut review =
        Review::new(changes, Mode::SideBySide, false, 4).expect("the fixtures highlight");
    let mut terminal = Terminal::new(TestBackend::new(width, height)).expect("test backend starts");

    terminal.draw(|frame| review.draw(frame, frame.area())).expect("draw succeeds");
    for key in keys {
        review.key(KeyEvent::new(*key, KeyModifiers::NONE)).expect("the fixtures highlight");
        terminal.draw(|frame| review.draw(frame, frame.area())).expect("draw succeeds");
    }

    describe(terminal.backend().buffer())
}

fn describe(buffer: &Buffer) -> String {
    let area = buffer.area();
    let mut out = String::new();

    for y in 0..area.height {
        let mut text = String::new();
        let mut tints = String::new();
        for x in 0..area.width {
            let cell = &buffer[(x, y)];
            text.push_str(cell.symbol());
            tints.push(tint(cell.bg));
        }
        writeln!(out, "{text}\n{tints}").expect("writing to a String cannot fail");
    }

    out
}

fn tint(color: Color) -> char {
    let tints = TokyoNight::TINTS;
    let Color::Rgb(red, green, blue) = color else {
        return '?';
    };
    match Rgb::new(red, green, blue) {
        found if found == tints.background => '.',
        found if found == tints.filler => ' ',
        found if found == tints.removed => '-',
        found if found == tints.removed_emphasis => '=',
        found if found == tints.added => '+',
        found if found == tints.added_emphasis => '#',
        _ => '?',
    }
}

#[test]
fn the_sidebar_lists_every_change_beside_the_diff_of_the_selected_one() {
    insta::assert_snapshot!(shown(fixtures(), 90, 8, &[]));
}

#[test]
fn moving_to_the_next_file_selects_it_and_swaps_the_diff() {
    insta::assert_snapshot!(shown(fixtures(), 90, 8, &[KeyCode::Char(']')]));
}

#[test]
fn moving_past_the_last_file_stays_on_it() {
    let keys = [KeyCode::Char(']'); 9];

    assert_eq!(shown(fixtures(), 90, 8, &keys), shown(fixtures(), 90, 8, &[KeyCode::Char(']'); 2]));
}

#[test]
fn moving_back_from_the_first_file_lands_on_the_directory_holding_it() {
    insta::assert_snapshot!(shown(fixtures(), 90, 8, &[KeyCode::Char('[')]));
}

#[test]
fn hiding_the_sidebar_gives_its_width_to_the_diff() {
    insta::assert_snapshot!(shown(fixtures(), 90, 8, &[KeyCode::Char('F')]));
}

#[test]
fn a_terminal_too_narrow_for_both_drops_the_sidebar() {
    assert_eq!(shown(fixtures(), 60, 6, &[]), shown(fixtures(), 60, 6, &[KeyCode::Char('F')]));
}

#[test]
fn a_binary_file_says_so_where_its_diff_would_be() {
    insta::assert_snapshot!(shown(vec![binary("logo.png")], 60, 4, &[]));
}

#[test]
fn a_review_of_nothing_says_so() {
    insta::assert_snapshot!(shown(Vec::new(), 40, 3, &[]));
}

#[test]
fn the_layout_chosen_in_one_file_survives_the_move_to_the_next() {
    let switched = shown(fixtures(), 90, 8, &[KeyCode::Char('s'), KeyCode::Char(']')]);
    let unified = shown(fixtures(), 90, 8, &[KeyCode::Char(']'), KeyCode::Char('s')]);

    assert_eq!(switched, unified);
}

#[test]
fn clicking_a_file_in_the_list_selects_it() {
    let clicked = moused(fixtures(), 90, 8, &[click(6, 3)]);

    assert_eq!(clicked, shown(fixtures(), 90, 8, &[KeyCode::Char(']')]));
}

#[test]
fn clicking_below_the_last_file_selects_nothing() {
    let clicked = moused(fixtures(), 90, 8, &[click(6, 7)]);

    assert_eq!(clicked, shown(fixtures(), 90, 8, &[]));
}

#[test]
fn clicking_in_the_diff_does_not_change_the_file() {
    let clicked = moused(fixtures(), 90, 8, &[click(60, 3)]);

    assert_eq!(clicked, shown(fixtures(), 90, 8, &[]));
}

#[test]
fn the_wheel_over_the_list_moves_between_files() {
    let rolled = moused(fixtures(), 90, 8, &[at(MouseEventKind::ScrollDown, 6, 2)]);

    assert_eq!(rolled, shown(fixtures(), 90, 8, &[KeyCode::Char(']')]));
}

#[test]
fn the_wheel_over_the_diff_scrolls_it_and_leaves_the_file_alone() {
    let rolled = moused(vec![tall("src/tall.rs")], 90, 8, &[at(MouseEventKind::ScrollDown, 60, 4)]);

    assert_eq!(rolled, shown(vec![tall("src/tall.rs")], 90, 8, &[KeyCode::Char('j'); 3]));
}

#[test]
fn a_rescan_that_adds_a_file_above_the_selection_keeps_the_file_you_were_reading() {
    let mut review =
        Review::new(fixtures(), Mode::SideBySide, false, 4).expect("the fixtures highlight");
    review.key(KeyEvent::new(KeyCode::Char(']'), KeyModifiers::NONE)).expect("moving succeeds");

    let mut grown = fixtures();
    grown.insert(0, added("src/first.rs", "fn first() {}\n"));
    review.refresh(grown).expect("the fixtures highlight");

    let mut terminal = Terminal::new(TestBackend::new(80, 6)).expect("test backend starts");
    terminal.draw(|frame| review.draw(frame, frame.area())).expect("draw succeeds");

    insta::assert_snapshot!(describe(terminal.backend().buffer()));
}

#[test]
fn a_rescan_that_drops_the_file_you_were_reading_falls_back_to_the_first() {
    let mut review =
        Review::new(fixtures(), Mode::SideBySide, false, 4).expect("the fixtures highlight");
    review.key(KeyEvent::new(KeyCode::Char(']'), KeyModifiers::NONE)).expect("moving succeeds");
    review
        .refresh(vec![modified("src/total.rs", "let total = 1;\n", "let total = 42;\n")])
        .expect("the fixtures highlight");

    let mut terminal = Terminal::new(TestBackend::new(80, 6)).expect("test backend starts");
    terminal.draw(|frame| review.draw(frame, frame.area())).expect("draw succeeds");

    insta::assert_snapshot!(describe(terminal.backend().buffer()));
}
