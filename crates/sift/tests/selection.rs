//! That a run of lines can be selected by key or by dragging, and that the
//! selection is what gets copied, opened and handed to a host.

use std::fmt::Write;
use std::path::PathBuf;

use hue::Rgb;
use keys::{Key, Pending, Pressed};
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::buffer::Buffer;
use ratatui::crossterm::event::{KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use ratatui::layout::Rect;
use ratatui::style::Color;
use ratatui::widgets::Widget;
use sift::command::{self, Command};
use sift::diff::{FileDiff, Side, Source};
use sift::git::{Content, FileChange, Status};
use sift::theme::{Theme, TokyoNight};
use sift::ui::{Mode, Presentation, Request, Review, SELECTING, Selection};

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

fn numbers() -> Vec<FileChange> {
    vec![modified(
        PATH,
        "one\ntwo\nthree\nfour\nfive\nsix\n",
        "one\ntwo\nTHREE\nFOUR\nfive\nsix\nseven\n",
    )]
}

fn far_apart() -> Vec<FileChange> {
    let mut old = String::new();
    for line in 1..=30 {
        writeln!(old, "line {line}").expect("writing to a String cannot fail");
    }
    let new = old.replace("line 5\n", "line 5 changed\n").replace("line 25\n", "line 25 changed\n");

    vec![modified(PATH, &old, &new)]
}

fn every_line_changed() -> Vec<FileChange> {
    let (mut old, mut new) = (String::new(), String::new());
    for line in 1..=30 {
        writeln!(old, "let old_{line} = {line};").expect("writing to a String cannot fail");
        writeln!(new, "let new_{line} = {line};").expect("writing to a String cannot fail");
    }

    vec![modified(PATH, &old, &new)]
}

fn selected(side: Side, lines: std::ops::RangeInclusive<usize>) -> Selection {
    Selection { path: PathBuf::from(PATH), side, lines }
}

/// A review drawn to a test terminal after every step, with the file list
/// hidden so the diff starts at the left edge, one row under the header.
struct Reviewing {
    review: Review,
    terminal: Terminal<TestBackend>,
}

impl Reviewing {
    fn new(changes: Vec<FileChange>, mode: Mode, height: u16) -> Self {
        let presentation = Presentation { mode, ..Presentation::default() };
        let review =
            Review::new(changes, presentation, Theme::default()).expect("the fixtures highlight");
        let terminal = Terminal::new(TestBackend::new(90, height)).expect("test backend starts");
        let mut reviewing = Self { review, terminal };
        reviewing.run(&[Command::ToggleFileList, Command::Top]);

        reviewing
    }

    fn run(&mut self, commands: &[Command]) -> &mut Self {
        for command in commands {
            self.review.run(*command).expect("the fixtures highlight");
            self.draw();
        }

        self
    }

    fn mouse(&mut self, events: &[MouseEvent]) -> &mut Self {
        for event in events {
            self.review.mouse(*event).expect("the fixtures highlight");
            self.draw();
        }

        self
    }

    fn draw(&mut self) {
        let review = &mut self.review;
        self.terminal
            .draw(|frame| frame.render_widget(&mut *review, frame.area()))
            .expect("draw succeeds");
    }

    fn selection(&self) -> Option<Selection> {
        self.review.selection()
    }

    fn screen(&self) -> String {
        describe(self.terminal.backend().buffer())
    }
}

fn mouse(kind: MouseEventKind, column: u16, row: u16) -> MouseEvent {
    MouseEvent { kind, column, row, modifiers: KeyModifiers::NONE }
}

fn press(column: u16, row: u16) -> MouseEvent {
    mouse(MouseEventKind::Down(MouseButton::Left), column, row)
}

fn drag(column: u16, row: u16) -> MouseEvent {
    mouse(MouseEventKind::Drag(MouseButton::Left), column, row)
}

fn release(column: u16, row: u16) -> MouseEvent {
    mouse(MouseEventKind::Up(MouseButton::Left), column, row)
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
        writeln!(out, "{}\n{}", text.trim_end(), tints.trim_end())
            .expect("writing to a String cannot fail");
    }

    out
}

fn tint(color: Color) -> char {
    let tints = TokyoNight::TINTS;
    let Color::Rgb(red, green, blue) = color else {
        return '?';
    };
    match Rgb::new(red, green, blue) {
        found if found == tints.background => ' ',
        found if found == tints.filler => '_',
        found if found == tints.removed => '-',
        found if found == tints.removed_emphasis => '=',
        found if found == tints.added => '+',
        found if found == tints.added_emphasis => '#',
        found if found == tints.under_cursor(tints.background) => ':',
        found if found == tints.under_cursor(tints.filler) => '.',
        found if found == tints.under_cursor(tints.removed) => '~',
        found if found == tints.under_cursor(tints.removed_emphasis) => '!',
        found if found == tints.under_cursor(tints.added) => '*',
        found if found == tints.under_cursor(tints.added_emphasis) => '%',
        _ => '?',
    }
}

#[test]
fn nothing_selected_is_the_cursors_line_on_its_own() {
    let mut reviewing = Reviewing::new(numbers(), Mode::SideBySide, 10);
    reviewing.run(&[Command::NextLine]);

    assert!(!reviewing.review.selecting());
    assert_eq!(reviewing.selection(), Some(selected(Side::New, 2..=2)));
}

#[test]
fn moving_down_from_where_selecting_began_tints_every_line_in_between() {
    let mut reviewing = Reviewing::new(numbers(), Mode::SideBySide, 10);
    reviewing.run(&[Command::Select, Command::NextLine, Command::NextLine]);

    assert_eq!(reviewing.selection(), Some(selected(Side::New, 1..=3)));
    insta::assert_snapshot!(reviewing.screen());
}

#[test]
fn moving_back_above_where_selecting_began_selects_upward() {
    let mut reviewing = Reviewing::new(numbers(), Mode::SideBySide, 10);
    reviewing.run(&[Command::Bottom, Command::Select, Command::PreviousLine]);

    assert_eq!(reviewing.selection(), Some(selected(Side::New, 7..=8)));
}

#[test]
fn selecting_again_lets_go() {
    let mut reviewing = Reviewing::new(numbers(), Mode::SideBySide, 10);
    reviewing.run(&[Command::Select, Command::NextLine, Command::Select]);

    assert!(!reviewing.review.selecting());
    assert_eq!(reviewing.selection(), Some(selected(Side::New, 2..=2)));
}

#[test]
fn a_selection_in_one_column_keeps_to_the_side_it_began_on() {
    let mut reviewing = Reviewing::new(numbers(), Mode::Unified, 12);
    reviewing.run(&[Command::NextLine, Command::NextLine, Command::Select]);
    reviewing.run(&[Command::NextLine; 4]);

    assert_eq!(reviewing.selection(), Some(selected(Side::Old, 3..=5)));
    insta::assert_snapshot!(reviewing.screen());
}

#[test]
fn changing_sides_takes_the_selection_with_the_cursor() {
    let mut reviewing = Reviewing::new(numbers(), Mode::SideBySide, 10);
    reviewing.run(&[Command::NextLine, Command::NextLine, Command::Select, Command::NextLine]);
    reviewing.run(&[Command::OldSide]);

    assert_eq!(reviewing.selection(), Some(selected(Side::Old, 3..=4)));
}

#[test]
fn a_fold_inside_the_selection_counts_the_lines_it_hides() {
    let mut reviewing = Reviewing::new(far_apart(), Mode::SideBySide, 24);
    reviewing.run(&[Command::NextHunk, Command::Select, Command::NextHunk]);

    assert_eq!(reviewing.selection(), Some(selected(Side::New, 5..=25)));
}

#[test]
fn the_selection_outlasts_a_change_of_layout() {
    let mut reviewing = Reviewing::new(numbers(), Mode::SideBySide, 12);
    reviewing.run(&[Command::Select, Command::NextLine, Command::NextLine]);
    reviewing.run(&[Command::ToggleLayout]);

    assert_eq!(reviewing.selection(), Some(selected(Side::New, 1..=3)));
}

#[test]
fn the_selection_outlasts_a_reload() {
    let mut reviewing = Reviewing::new(numbers(), Mode::SideBySide, 10);
    reviewing.run(&[Command::Select, Command::NextLine, Command::NextLine]);
    reviewing.review.refresh(numbers()).expect("the fixtures highlight");
    reviewing.draw();

    assert_eq!(reviewing.selection(), Some(selected(Side::New, 1..=3)));
}

#[test]
fn dragging_selects_from_where_the_button_went_down_to_where_it_came_up() {
    let mut dragged = Reviewing::new(numbers(), Mode::SideBySide, 10);
    dragged.mouse(&[press(60, 1), drag(60, 2), drag(60, 3), release(60, 3)]);
    let mut typed = Reviewing::new(numbers(), Mode::SideBySide, 10);
    typed.run(&[Command::Select, Command::NextLine, Command::NextLine]);

    assert_eq!(dragged.selection(), Some(selected(Side::New, 1..=3)));
    assert_eq!(dragged.screen(), typed.screen());
}

#[test]
fn a_click_that_does_not_move_selects_nothing() {
    let mut reviewing = Reviewing::new(numbers(), Mode::SideBySide, 10);
    reviewing.mouse(&[press(60, 2), drag(61, 2), release(61, 2)]);

    assert!(!reviewing.review.selecting());
    assert_eq!(reviewing.selection(), Some(selected(Side::New, 2..=2)));
}

#[test]
fn a_drag_keeps_to_the_side_it_began_on() {
    let mut reviewing = Reviewing::new(numbers(), Mode::SideBySide, 10);
    reviewing.mouse(&[press(10, 3), drag(60, 4), release(60, 4)]);

    assert_eq!(reviewing.selection(), Some(selected(Side::Old, 3..=4)));
}

#[test]
fn dragging_past_the_bottom_scrolls_a_row_at_a_time() {
    let mut reviewing = Reviewing::new(every_line_changed(), Mode::SideBySide, 8);
    reviewing.mouse(&[press(60, 1), drag(60, 20), drag(60, 20), drag(60, 20)]);

    assert_eq!(reviewing.selection(), Some(selected(Side::New, 1..=10)));
}

#[test]
fn dragging_past_the_top_scrolls_back_up() {
    let mut reviewing = Reviewing::new(every_line_changed(), Mode::SideBySide, 8);
    reviewing.run(&[Command::PageDown, Command::PageDown]);
    reviewing.mouse(&[press(60, 7), drag(60, 0), drag(60, 0)]);
    let Some(selection) = reviewing.selection() else {
        panic!("a drag selects");
    };

    assert_eq!(selection.lines.end() - selection.lines.start(), 8);
}

#[test]
fn a_click_lets_go_of_the_selection() {
    let mut reviewing = Reviewing::new(numbers(), Mode::SideBySide, 10);
    reviewing.run(&[Command::Select, Command::NextLine]);
    reviewing.mouse(&[press(60, 5), release(60, 5)]);

    assert!(!reviewing.review.selecting());
    assert_eq!(reviewing.selection(), Some(selected(Side::New, 5..=5)));
}

#[test]
fn a_shift_click_selects_from_the_cursor_to_the_line_clicked() {
    let mut reviewing = Reviewing::new(numbers(), Mode::SideBySide, 10);
    reviewing.mouse(&[MouseEvent { modifiers: KeyModifiers::SHIFT, ..press(60, 3) }]);

    assert_eq!(reviewing.selection(), Some(selected(Side::New, 1..=3)));
}

fn keyed() -> Review {
    let mut review = Review::new(numbers(), Presentation::default(), Theme::default())
        .expect("the fixtures highlight");
    drawn(&mut review);

    review
}

fn drawn(review: &mut Review) {
    let area = Rect::new(0, 0, 120, 11);
    review.render(area, &mut Buffer::empty(area));
}

fn typed(review: &mut Review, keys: &str) -> Vec<Request> {
    let keymap = command::keymap();
    let mut pending = Pending::default();
    keys.chars()
        .map(|character| {
            let key: Key = character.to_string().parse().expect("a character is a key");
            let request = match keymap.press(&mut pending, key, &review.holding()) {
                Pressed::Command { command, count } => {
                    review.repeated(command, count).expect("the fixtures highlight")
                }
                Pressed::Pending | Pressed::Nothing => Request::Nothing,
            };
            drawn(review);
            request
        })
        .collect()
}

#[test]
fn copying_a_selection_copies_its_range() {
    let mut review = keyed();

    assert_eq!(typed(&mut review, "ggVjjY").last(), Some(&Request::Copy(format!("{PATH}:1-3"))));
}

#[test]
fn copying_without_a_selection_still_copies_one_line() {
    let mut review = keyed();

    assert_eq!(typed(&mut review, "ggjY").last(), Some(&Request::Copy(format!("{PATH}:2"))));
}

#[test]
fn the_location_starts_at_the_first_line_selected() {
    let mut review = keyed();
    typed(&mut review, "GVkk");

    assert_eq!(review.location().and_then(|location| location.lines), Some(6..=8));
}

#[test]
fn escape_lets_go_of_the_selection_and_leaves_the_cursor_where_it_was() {
    let mut review = keyed();
    typed(&mut review, "ggVj");
    let held = review.holding();
    review.run(Command::ClearSelection).expect("the fixtures highlight");

    assert_eq!(held, [SELECTING]);
    assert_eq!(review.selection(), Some(selected(Side::New, 2..=2)));
    assert_eq!(review.holding(), Vec::<&str>::new());
}

#[test]
fn the_status_counts_the_lines_selected() {
    let mut review = keyed();
    typed(&mut review, "ggVjj");

    assert!(review.status().contains("lines 1–3"), "{}", review.status());
}
