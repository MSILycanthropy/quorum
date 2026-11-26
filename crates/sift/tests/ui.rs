//! That rows reach the terminal buffer with the right text and the right tints.

use std::fmt::Write;
use std::path::Path;

use hue::Rgb;
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::buffer::Buffer;
use ratatui::style::Color;
use sift::command::Command;
use sift::diff::{FileDiff, Side, Source};
use sift::theme::{Theme, TokyoNight};
use sift::ui::{Indicators, Mode, Presentation, Viewer};

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
    let mut viewer = Viewer::new(
        diff,
        Path::new("demo.rs"),
        Presentation { mode, wrap, ..Presentation::default() },
        &Theme::default(),
    )
    .expect("fixtures highlight");
    let mut terminal = Terminal::new(TestBackend::new(width, height)).expect("test backend starts");

    terminal.draw(|frame| frame.render_widget(&mut viewer, frame.area())).expect("draw succeeds");
    for command in commands {
        viewer.run(*command).expect("fixtures highlight");
        terminal
            .draw(|frame| frame.render_widget(&mut viewer, frame.area()))
            .expect("draw succeeds");
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
        found if found == tints.under_cursor(tints.background) => ':',
        found if found == tints.under_cursor(tints.filler) => '_',
        found if found == tints.under_cursor(tints.removed) => '~',
        found if found == tints.under_cursor(tints.removed_emphasis) => '!',
        found if found == tints.under_cursor(tints.added) => '*',
        found if found == tints.under_cursor(tints.added_emphasis) => '%',
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
fn holding_down_stops_where_jumping_to_the_end_does() {
    let commands = [Command::NextLine; 40];

    assert_eq!(scrolled(48, 4, false, &commands), scrolled(48, 4, false, &[Command::Bottom]));
}

#[test]
fn jumping_to_the_end_puts_the_cursor_on_the_last_row() {
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
    let mut viewer =
        Viewer::new(diff, Path::new("demo.txt"), Presentation::default(), &Theme::default())
            .expect("fixtures highlight");
    let mut terminal = Terminal::new(TestBackend::new(48, 6)).expect("test backend starts");

    terminal.draw(|frame| frame.render_widget(&mut viewer, frame.area())).expect("draw succeeds");
    for command in commands {
        viewer.run(*command).expect("fixtures highlight");
        terminal
            .draw(|frame| frame.render_widget(&mut viewer, frame.area()))
            .expect("draw succeeds");
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

fn drawn(
    (old, new): (&str, &str),
    mode: Mode,
    (width, height): (u16, u16),
    wrap: bool,
    commands: &[Command],
) -> (Viewer, String) {
    let diff = FileDiff::compute(Source::new(old), Source::new(new));
    let mut viewer = Viewer::new(
        diff,
        Path::new("demo.rs"),
        Presentation { mode, wrap, ..Presentation::default() },
        &Theme::default(),
    )
    .expect("fixtures highlight");
    let mut terminal = Terminal::new(TestBackend::new(width, height)).expect("test backend starts");

    terminal.draw(|frame| frame.render_widget(&mut viewer, frame.area())).expect("draw succeeds");
    for command in commands {
        viewer.run(*command).expect("fixtures highlight");
        terminal
            .draw(|frame| frame.render_widget(&mut viewer, frame.area()))
            .expect("draw succeeds");
    }

    let text = describe(terminal.backend().buffer());
    (viewer, text)
}

fn far_apart_unfolded(height: u16, commands: &[Command]) -> (Viewer, String) {
    let (old, new) = far_apart();
    let mut unfolded = vec![Command::ToggleFolds];
    unfolded.extend_from_slice(commands);

    drawn((&old, &new), Mode::SideBySide, (48, height), false, &unfolded)
}

const EVERY_KIND_OLD: &str = "fn main() {\n    let a = 1;\n    let gone = 0;\n    print(a);\n}\n";
const EVERY_KIND_NEW: &str = "fn main() {\n    let a = 2;\n    print(a);\n    print(b);\n}\n";

fn every_row(mode: Mode, first: &[Command]) -> String {
    let mut frames = String::new();
    for row in 0..6 {
        let mut commands = first.to_vec();
        commands.extend(std::iter::repeat_n(Command::NextLine, row));
        let (_, text) = drawn((EVERY_KIND_OLD, EVERY_KIND_NEW), mode, (56, 7), false, &commands);
        writeln!(frames, "{text}").expect("writing to a String cannot fail");
    }

    frames
}

#[test]
fn the_cursor_starts_on_the_first_row() {
    let (viewer, _) = drawn((OLD, NEW), Mode::SideBySide, (64, 6), false, &[]);

    assert_eq!(viewer.cursor(), Some((Side::New, 1)));
}

#[test]
fn moving_down_skips_the_continuation_of_a_wrapped_row() {
    let (viewer, _) = drawn((OLD, NEW), Mode::SideBySide, (34, 8), true, &[Command::NextLine]);

    assert_eq!(viewer.cursor(), Some((Side::New, 2)));
}

#[test]
fn the_view_follows_the_cursor_with_three_rows_of_context() {
    let (_, text) = far_apart_unfolded(10, &[Command::NextLine; 7]);

    insta::assert_snapshot!(text);
}

#[test]
fn the_view_stops_at_the_top_rather_than_keep_context_above_the_first_row() {
    let (_, top) = far_apart_unfolded(10, &[]);
    let (_, back) = far_apart_unfolded(10, &[Command::NextLine, Command::PreviousLine]);

    assert_eq!(back, top);
}

#[test]
fn half_a_page_moves_the_cursor_and_the_view_together() {
    let (viewer, text) = far_apart_unfolded(6, &[Command::NextLine, Command::HalfPageDown]);

    assert_eq!(viewer.cursor(), Some((Side::New, 5)));
    insta::assert_snapshot!(text);
}

#[test]
fn a_pure_insertion_puts_the_cursor_on_the_new_side_whatever_side_was_chosen() {
    let commands = [
        Command::OldSide,
        Command::NextLine,
        Command::NextLine,
        Command::NextLine,
        Command::NextLine,
    ];
    let (viewer, _) =
        drawn((EVERY_KIND_OLD, EVERY_KIND_NEW), Mode::SideBySide, (56, 7), false, &commands);

    assert_eq!(viewer.cursor(), Some((Side::New, 4)));
}

#[test]
fn a_pure_deletion_puts_the_cursor_on_the_old_side_whatever_side_was_chosen() {
    let commands = [Command::NextLine, Command::NextLine];
    let (viewer, _) =
        drawn((EVERY_KIND_OLD, EVERY_KIND_NEW), Mode::SideBySide, (56, 7), false, &commands);

    assert_eq!(viewer.cursor(), Some((Side::Old, 3)));
}

#[test]
fn the_chosen_side_comes_back_on_the_next_row_with_both_sides() {
    let mut commands = vec![Command::OldSide];
    commands.extend([Command::NextLine; 5]);
    let (viewer, _) =
        drawn((EVERY_KIND_OLD, EVERY_KIND_NEW), Mode::SideBySide, (56, 7), false, &commands);

    assert_eq!(viewer.cursor(), Some((Side::Old, 5)));
}

#[test]
fn switching_sides_on_a_row_with_one_side_does_nothing() {
    let commands = [Command::NextLine, Command::NextLine, Command::NewSide];
    let (viewer, _) =
        drawn((EVERY_KIND_OLD, EVERY_KIND_NEW), Mode::SideBySide, (56, 7), false, &commands);

    assert_eq!(viewer.cursor(), Some((Side::Old, 3)));
}

#[test]
fn switching_sides_in_unified_mode_does_nothing() {
    let commands = [Command::NextLine, Command::OldSide];
    let (viewer, _) = drawn((OLD, NEW), Mode::Unified, (44, 7), false, &commands);

    assert_eq!(viewer.cursor(), Some((Side::Old, 2)));
}

#[test]
fn toggling_the_layout_keeps_the_cursor_on_the_same_line() {
    let chosen = [Command::NextLine, Command::OldSide];
    let (split, _) = drawn((OLD, NEW), Mode::SideBySide, (64, 7), false, &chosen);
    let (unified, _) = drawn(
        (OLD, NEW),
        Mode::SideBySide,
        (64, 7),
        false,
        &[Command::NextLine, Command::OldSide, Command::ToggleLayout],
    );
    let (back, _) = drawn(
        (OLD, NEW),
        Mode::SideBySide,
        (64, 7),
        false,
        &[Command::NextLine, Command::OldSide, Command::ToggleLayout, Command::ToggleLayout],
    );

    assert_eq!(split.cursor(), Some((Side::Old, 2)));
    assert_eq!(unified.cursor(), Some((Side::Old, 2)));
    assert_eq!(back.cursor(), Some((Side::Old, 2)));
}

#[test]
fn toggling_wrap_keeps_the_cursor_on_the_same_line() {
    let moved = [Command::NextLine, Command::NextLine, Command::NextLine];
    let (wrapped, _) = drawn((OLD, NEW), Mode::SideBySide, (34, 8), true, &moved);
    let (unwrapped, _) = drawn(
        (OLD, NEW),
        Mode::SideBySide,
        (34, 8),
        true,
        &[Command::NextLine, Command::NextLine, Command::NextLine, Command::ToggleWrapping],
    );

    assert_eq!(wrapped.cursor(), Some((Side::New, 3)));
    assert_eq!(unwrapped.cursor(), wrapped.cursor());
}

#[test]
fn a_hunk_jump_puts_the_cursor_on_the_first_changed_row() {
    let (old, new) = far_apart();
    let (viewer, _) = drawn((&old, &new), Mode::SideBySide, (48, 6), false, &[Command::NextHunk]);

    assert_eq!(viewer.cursor(), Some((Side::New, 5)));
}

#[test]
fn enter_opens_the_fold_under_the_cursor() {
    let (old, new) = far_apart();
    let (viewer, text) = drawn(
        (&old, &new),
        Mode::SideBySide,
        (48, 30),
        false,
        &[Command::Bottom, Command::ExpandFold],
    );

    assert_eq!(viewer.cursor(), Some((Side::New, 34)));
    insta::assert_snapshot!(text);
}

#[test]
fn a_deleted_line_reports_the_nearest_newer_line_to_edit() {
    let moved = [Command::NextLine, Command::NextLine, Command::NextLine];
    let (viewer, _) = drawn((OLD, NEW), Mode::Unified, (44, 7), false, &moved);

    assert_eq!(viewer.cursor(), Some((Side::Old, 3)));
    assert_eq!(viewer.line(), Some(3));
}

#[test]
fn the_wheel_drags_the_cursor_only_when_it_would_leave_the_screen() {
    let (mut viewer, _) = far_apart_unfolded(6, &[Command::NextLine; 4]);
    assert_eq!(viewer.cursor(), Some((Side::New, 5)));

    viewer.scroll_by(1);
    assert_eq!(viewer.cursor(), Some((Side::New, 5)));

    viewer.scroll_by(3);
    assert_eq!(viewer.cursor(), Some((Side::New, 6)));

    for _ in 0..5 {
        viewer.run(Command::NextLine).expect("fixtures highlight");
    }
    assert_eq!(viewer.cursor(), Some((Side::New, 11)));

    viewer.scroll_by(-1);
    assert_eq!(viewer.cursor(), Some((Side::New, 11)));

    viewer.scroll_by(-4);
    assert_eq!(viewer.cursor(), Some((Side::New, 8)));
}

#[test]
fn the_cursors_line_number_is_drawn_in_the_attention_tint() {
    let diff = FileDiff::compute(Source::new(OLD), Source::new(NEW));
    let mut viewer =
        Viewer::new(diff, Path::new("demo.rs"), Presentation::default(), &Theme::default())
            .expect("fixtures highlight");
    let mut terminal = Terminal::new(TestBackend::new(64, 6)).expect("test backend starts");
    terminal.draw(|frame| frame.render_widget(&mut viewer, frame.area())).expect("draw succeeds");
    let buffer = terminal.backend().buffer();
    let attention = TokyoNight::TINTS.attention;
    let attention = Color::Rgb(attention.red, attention.green, attention.blue);

    assert_ne!(buffer[(0, 0)].fg, attention);
    assert_eq!(buffer[(32, 0)].fg, attention);
    assert_ne!(buffer[(32, 1)].fg, attention);
}

#[test]
fn the_cursor_lights_each_kind_of_row_on_the_new_side() {
    insta::assert_snapshot!(every_row(Mode::SideBySide, &[]));
}

#[test]
fn the_cursor_lights_each_kind_of_row_on_the_old_side() {
    insta::assert_snapshot!(every_row(Mode::SideBySide, &[Command::OldSide]));
}

#[test]
fn the_cursor_lights_each_kind_of_row_in_unified_mode() {
    insta::assert_snapshot!(every_row(Mode::Unified, &[]));
}

fn presented(presentation: Presentation, (old, new): (&str, &str), height: u16) -> String {
    let diff = FileDiff::compute(Source::new(old), Source::new(new));
    let mut viewer = Viewer::new(diff, Path::new("demo.rs"), presentation, &Theme::default())
        .expect("fixtures highlight");
    let mut terminal = Terminal::new(TestBackend::new(56, height)).expect("test backend starts");
    terminal.draw(|frame| frame.render_widget(&mut viewer, frame.area())).expect("draw succeeds");

    describe(terminal.backend().buffer())
}

fn marked(indicators: Indicators, mode: Mode, diff: (&str, &str), wrap: bool) -> String {
    presented(Presentation { mode, wrap, indicators, ..Presentation::default() }, diff, 7)
}

const LONG_OLD: &str = "let short = 1;\n";
const LONG_NEW: &str = "let long = one + two + three + four + five + six + seven;\n";

#[test]
fn bars_mark_each_changed_side_in_a_column_of_their_own() {
    insta::assert_snapshot!(marked(
        Indicators::Bars,
        Mode::SideBySide,
        (EVERY_KIND_OLD, EVERY_KIND_NEW),
        false
    ));
}

#[test]
fn signs_mark_removed_lines_minus_and_added_lines_plus() {
    insta::assert_snapshot!(marked(
        Indicators::Signs,
        Mode::Unified,
        (EVERY_KIND_OLD, EVERY_KIND_NEW),
        false
    ));
}

#[test]
fn a_wrapped_line_takes_its_sign_once_and_its_bar_on_every_row() {
    let signs = marked(Indicators::Signs, Mode::SideBySide, (LONG_OLD, LONG_NEW), true);
    let bars = marked(Indicators::Bars, Mode::SideBySide, (LONG_OLD, LONG_NEW), true);

    insta::assert_snapshot!(format!("{signs}\n{bars}"));
}

#[test]
fn without_backgrounds_a_changed_line_keeps_only_its_emphasis_and_its_bar() {
    let presentation = Presentation {
        backgrounds: false,
        indicators: Indicators::Bars,
        ..Presentation::default()
    };

    insta::assert_snapshot!(presented(presentation, (EVERY_KIND_OLD, EVERY_KIND_NEW), 7));
}

#[test]
fn without_line_numbers_the_code_starts_a_column_in() {
    let presentation = Presentation { line_numbers: false, ..Presentation::default() };

    insta::assert_snapshot!(presented(presentation, (EVERY_KIND_OLD, EVERY_KIND_NEW), 7));
}

#[test]
fn a_context_of_one_keeps_a_line_either_side_of_each_change_before_folding() {
    let (old, new) = far_apart();
    let presentation = Presentation { context: 1, ..Presentation::default() };

    insta::assert_snapshot!(presented(presentation, (&old, &new), 10));
}
