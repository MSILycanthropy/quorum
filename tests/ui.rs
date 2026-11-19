//! That rows reach the terminal buffer with the right text and the right tints.

use std::fmt::Write;
use std::path::Path;

use quorum::color::Rgb;
use quorum::command::Command;
use quorum::diff::{FileDiff, Source};
use quorum::theme::TokyoNight;
use quorum::ui::{Mode, Viewer};
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::buffer::Buffer;
use ratatui::style::Color;

const OLD: &str = "fn total(cart: &Cart) -> u32 {\n    let total = 1;\n    dead();\n    total\n}\n";
const NEW: &str = "fn total(cart: &Cart) -> u32 {\n    let total = 42;\n    total\n}\n";

fn rendered(width: u16, height: u16, wrap: bool) -> String {
    scrolled(width, height, wrap, &[])
}

fn scrolled(width: u16, height: u16, wrap: bool, commands: &[Command]) -> String {
    shown(Mode::SideBySide, width, height, wrap, commands)
}

fn shown(mode: Mode, width: u16, height: u16, wrap: bool, commands: &[Command]) -> String {
    let diff = FileDiff::compute(Source::new(OLD), Source::new(NEW));
    let mut viewer =
        Viewer::new(diff, Path::new("demo.rs"), mode, wrap, 4).expect("fixtures highlight");
    let mut terminal = Terminal::new(TestBackend::new(width, height)).expect("test backend starts");

    terminal.draw(|frame| viewer.draw(frame, frame.area())).expect("draw succeeds");
    for command in commands {
        viewer.run(*command);
        terminal.draw(|frame| viewer.draw(frame, frame.area())).expect("draw succeeds");
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
fn a_diff_reaches_the_buffer_with_line_numbers_tints_and_emphasis() {
    insta::assert_snapshot!(rendered(64, 6, false));
}

#[test]
fn wrapping_repeats_the_tint_but_not_the_line_number() {
    insta::assert_snapshot!(rendered(34, 8, true));
}

#[test]
fn holding_a_scroll_key_stops_where_jumping_to_the_end_does() {
    let commands = [Command::NextLine; 40];

    assert_eq!(scrolled(48, 4, false, &commands), scrolled(48, 4, false, &[Command::Bottom]));
}

#[test]
fn scrolling_to_the_end_leaves_the_last_row_at_the_top() {
    insta::assert_snapshot!(scrolled(48, 4, false, &[Command::Bottom]));
}

#[test]
fn unified_mode_stacks_a_change_as_a_deletion_above_its_insertion() {
    insta::assert_snapshot!(shown(Mode::Unified, 44, 7, false, &[]));
}

#[test]
fn toggling_the_layout_switches_between_the_two_layouts() {
    let toggled = shown(Mode::SideBySide, 44, 7, false, &[Command::ToggleLayout]);

    assert_eq!(toggled, shown(Mode::Unified, 44, 7, false, &[]));
}

fn far_apart() -> (String, String) {
    let mut old = String::new();
    for line in 1..=40 {
        writeln!(old, "line {line}").expect("writing to a String cannot fail");
    }
    let new = old.replace("line 5\n", "line five\n").replace("line 30\n", "line thirty\n");

    (old, new)
}

fn hunked(commands: &[Command]) -> (String, (Option<usize>, usize), Option<usize>) {
    let (old, new) = far_apart();
    let diff = FileDiff::compute(Source::new(old), Source::new(new));
    let mut viewer = Viewer::new(diff, Path::new("demo.txt"), Mode::SideBySide, false, 4)
        .expect("fixtures highlight");
    let mut terminal = Terminal::new(TestBackend::new(48, 6)).expect("test backend starts");

    terminal.draw(|frame| viewer.draw(frame, frame.area())).expect("draw succeeds");
    for command in commands {
        viewer.run(*command);
        terminal.draw(|frame| viewer.draw(frame, frame.area())).expect("draw succeeds");
    }

    let text = describe(terminal.backend().buffer());
    (text, viewer.hunk(), viewer.line())
}

#[test]
fn the_next_hunk_lands_with_three_lines_of_context_above_it() {
    let (text, hunk, line) = hunked(&[Command::NextHunk, Command::NextHunk]);

    assert!(text.lines().next().is_some_and(|row| row.contains("27")), "{text}");
    assert_eq!(hunk, (Some(2), 2));
    assert_eq!(line, Some(30));
}

#[test]
fn the_previous_hunk_comes_back_to_the_first() {
    let (_, hunk, line) = hunked(&[Command::NextHunk, Command::NextHunk, Command::PreviousHunk]);

    assert_eq!(hunk, (Some(1), 2));
    assert_eq!(line, Some(5));
}

#[test]
fn there_is_no_hunk_past_the_last() {
    let last = hunked(&[Command::NextHunk, Command::NextHunk]);

    assert_eq!(hunked(&[Command::NextHunk, Command::NextHunk, Command::NextHunk]), last);
}

#[test]
fn half_a_page_is_half_the_rows_on_screen() {
    let (halved, _, _) = hunked(&[Command::HalfPageDown]);
    let (stepped, _, _) = hunked(&[Command::NextLine; 3]);

    assert_eq!(halved, stepped);
}
