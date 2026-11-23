//! That every changed file is listed, and that the list drives what is shown.

use std::fmt::Write;
use std::path::PathBuf;

use quorum::color::Rgb;
use quorum::command::Command;
use quorum::diff::{FileDiff, Source};
use quorum::git::{Content, FileChange, Status};
use quorum::theme::TokyoNight;
use quorum::ui::{Mode, Review};
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::buffer::Buffer;
use ratatui::crossterm::event::{KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use ratatui::style::Color;

fn modified(path: &str, old: &str, new: &str) -> FileChange {
    FileChange {
        status: Status::Modified,
        path: PathBuf::from(path),
        previous: None,
        generated: false,
        content: Content::Text(FileDiff::compute(Source::new(old), Source::new(new))),
    }
}

fn added(path: &str, new: &str) -> FileChange {
    FileChange {
        status: Status::Added,
        path: PathBuf::from(path),
        previous: None,
        generated: false,
        content: Content::Text(FileDiff::compute(Source::new(""), Source::new(new))),
    }
}

fn deleted(path: &str, old: &str) -> FileChange {
    FileChange {
        status: Status::Deleted,
        path: PathBuf::from(path),
        previous: None,
        generated: false,
        content: Content::Text(FileDiff::compute(Source::new(old), Source::new(""))),
    }
}

fn binary(path: &str) -> FileChange {
    FileChange {
        status: Status::Modified,
        path: PathBuf::from(path),
        previous: None,
        generated: false,
        content: Content::Binary,
    }
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

fn shown(changes: Vec<FileChange>, width: u16, height: u16, commands: &[Command]) -> String {
    let mut review =
        Review::new(changes, Mode::SideBySide, false, 4).expect("the fixtures highlight");
    let mut terminal = Terminal::new(TestBackend::new(width, height)).expect("test backend starts");

    terminal.draw(|frame| review.draw(frame, frame.area())).expect("draw succeeds");
    for command in commands {
        review.run(*command).expect("the fixtures highlight");
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
    insta::assert_snapshot!(shown(fixtures(), 90, 8, &[Command::NextFile]));
}

#[test]
fn moving_past_the_last_file_stays_on_it() {
    let commands = [Command::NextFile; 9];

    assert_eq!(
        shown(fixtures(), 90, 8, &commands),
        shown(fixtures(), 90, 8, &[Command::NextFile; 2])
    );
}

#[test]
fn moving_back_from_the_first_file_stays_on_it_rather_than_selecting_its_directory() {
    assert_eq!(shown(fixtures(), 90, 8, &[Command::PreviousFile]), shown(fixtures(), 90, 8, &[]));
}

#[test]
fn the_next_hunk_past_the_last_one_in_a_file_is_the_first_one_in_the_next_file() {
    let hunked = shown(fixtures(), 90, 8, &[Command::NextHunk]);

    assert_eq!(hunked, shown(fixtures(), 90, 8, &[Command::NextFile]));
}

#[test]
fn the_previous_hunk_before_the_first_one_is_the_last_one_in_the_previous_file() {
    let hunked = shown(fixtures(), 90, 8, &[Command::NextFile, Command::PreviousHunk]);

    assert_eq!(hunked, shown(fixtures(), 90, 8, &[]));
}

#[test]
fn hunks_are_counted_for_the_file_on_screen() {
    let mut review = Review::new(vec![tall("src/tall.rs")], Mode::SideBySide, false, 4)
        .expect("the fixtures highlight");
    let mut terminal = Terminal::new(TestBackend::new(90, 8)).expect("test backend starts");
    terminal.draw(|frame| review.draw(frame, frame.area())).expect("draw succeeds");

    assert_eq!(review.hunk(), (Some(1), 1));
    assert_eq!(review.line(), Some(4));
}

#[test]
fn hiding_the_sidebar_gives_its_width_to_the_diff() {
    insta::assert_snapshot!(shown(fixtures(), 90, 8, &[Command::ToggleFileList]));
}

#[test]
fn a_terminal_too_narrow_for_both_drops_the_sidebar() {
    assert_eq!(shown(fixtures(), 60, 6, &[]), shown(fixtures(), 60, 6, &[Command::ToggleFileList]));
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
    let switched = shown(fixtures(), 90, 8, &[Command::ToggleLayout, Command::NextFile]);
    let unified = shown(fixtures(), 90, 8, &[Command::NextFile, Command::ToggleLayout]);

    assert_eq!(switched, unified);
}

#[test]
fn clicking_a_file_in_the_list_selects_it() {
    let clicked = moused(fixtures(), 90, 8, &[click(6, 3)]);

    assert_eq!(clicked, shown(fixtures(), 90, 8, &[Command::NextFile]));
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

    assert_eq!(rolled, shown(fixtures(), 90, 8, &[Command::NextFile]));
}

#[test]
fn the_wheel_over_the_diff_scrolls_it_and_leaves_the_file_alone() {
    let rolled = moused(vec![tall("src/tall.rs")], 90, 8, &[at(MouseEventKind::ScrollDown, 60, 4)]);

    assert_eq!(rolled, shown(vec![tall("src/tall.rs")], 90, 8, &[Command::NextLine; 3]));
}

#[test]
fn a_rescan_that_adds_a_file_above_the_selection_keeps_the_file_you_were_reading() {
    let mut review =
        Review::new(fixtures(), Mode::SideBySide, false, 4).expect("the fixtures highlight");
    review.run(Command::NextFile).expect("moving succeeds");

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
    review.run(Command::NextFile).expect("moving succeeds");
    review
        .refresh(vec![modified("src/total.rs", "let total = 1;\n", "let total = 42;\n")])
        .expect("the fixtures highlight");

    let mut terminal = Terminal::new(TestBackend::new(80, 6)).expect("test backend starts");
    terminal.draw(|frame| review.draw(frame, frame.area())).expect("draw succeeds");

    insta::assert_snapshot!(describe(terminal.backend().buffer()));
}

#[test]
fn a_rescan_of_the_file_you_were_reading_keeps_your_place_and_layout() {
    let mut review = Review::new(vec![tall("src/tall.rs")], Mode::SideBySide, false, 4)
        .expect("the fixtures highlight");
    let mut terminal = Terminal::new(TestBackend::new(90, 8)).expect("test backend starts");
    terminal.draw(|frame| review.draw(frame, frame.area())).expect("draw succeeds");
    for command in [Command::ToggleLayout, Command::NextLine, Command::NextLine] {
        review.run(command).expect("the fixtures highlight");
    }
    terminal.draw(|frame| review.draw(frame, frame.area())).expect("draw succeeds");
    let before = describe(terminal.backend().buffer());

    review.refresh(vec![tall("src/tall.rs")]).expect("the fixtures highlight");
    terminal.draw(|frame| review.draw(frame, frame.area())).expect("draw succeeds");

    assert_eq!(describe(terminal.backend().buffer()), before);
}

fn unchanged(text: &str) -> Content {
    Content::Text(FileDiff::compute(Source::new(text), Source::new(text)))
}

fn peeked(line: Option<usize>, commands: &[Command]) -> String {
    let mut review =
        Review::new(fixtures(), Mode::SideBySide, false, 4).expect("the fixtures highlight");
    let mut terminal = Terminal::new(TestBackend::new(90, 8)).expect("test backend starts");
    let mut text = String::new();
    for line in 1..=30 {
        writeln!(text, "line {line}").expect("writing to a String cannot fail");
    }

    terminal.draw(|frame| review.draw(frame, frame.area())).expect("draw succeeds");
    review.peek(PathBuf::from("src/lib.rs"), &unchanged(&text), line).expect("the file highlights");
    for command in commands {
        review.run(*command).expect("the fixtures highlight");
    }
    terminal.draw(|frame| review.draw(frame, frame.area())).expect("draw succeeds");

    describe(terminal.backend().buffer())
}

#[test]
fn an_unchanged_file_opens_in_one_column_over_the_review() {
    insta::assert_snapshot!(peeked(None, &[]));
}

#[test]
fn an_unchanged_file_opened_at_a_line_scrolls_to_it() {
    insta::assert_snapshot!(peeked(Some(20), &[]));
}

#[test]
fn moving_to_the_next_file_leaves_the_unchanged_one() {
    assert_eq!(peeked(None, &[Command::NextFile]), shown(fixtures(), 90, 8, &[Command::NextFile]));
}

#[test]
fn closing_an_unchanged_file_returns_to_the_diff_as_it_was() {
    let mut review =
        Review::new(fixtures(), Mode::SideBySide, false, 4).expect("the fixtures highlight");
    review
        .peek(PathBuf::from("src/lib.rs"), &unchanged("fn lib() {}\n"), None)
        .expect("highlights");

    assert_eq!(review.path(), Some(std::path::Path::new("src/lib.rs")));
    assert!(review.close_peek());
    assert_eq!(review.path(), Some(std::path::Path::new("src/gone.rs")));
    assert!(!review.close_peek());
}

#[test]
fn an_unchanged_file_that_a_rescan_finds_changed_joins_the_review() {
    let mut review =
        Review::new(fixtures(), Mode::SideBySide, false, 4).expect("the fixtures highlight");
    review
        .peek(PathBuf::from("src/lib.rs"), &unchanged("fn lib() {}\n"), None)
        .expect("highlights");

    let mut grown = fixtures();
    grown.push(modified("src/lib.rs", "fn lib() {}\n", "fn lib() { 1 }\n"));
    review.refresh(grown).expect("the fixtures highlight");

    assert_eq!(review.peeked(), None);
    assert_eq!(review.path(), Some(std::path::Path::new("src/lib.rs")));
}

fn middling(path: &str) -> FileChange {
    let mut old = String::new();
    for line in 1..=30 {
        writeln!(old, "line {line}").expect("writing to a String cannot fail");
    }
    let new = old.replace("line 15\n", "line 15 changed\n");

    modified(path, &old, &new)
}

#[test]
fn unchanged_lines_away_from_a_change_fold_down_to_its_context() {
    insta::assert_snapshot!(shown(vec![middling("src/middle.rs")], 90, 12, &[]));
}

#[test]
fn expanding_opens_the_first_fold_on_screen() {
    insta::assert_snapshot!(shown(vec![middling("src/middle.rs")], 90, 12, &[Command::ExpandFold]));
}

#[test]
fn folding_can_be_turned_off_for_the_whole_file() {
    let middle = || vec![middling("src/middle.rs")];

    assert_eq!(
        shown(middle(), 90, 40, &[Command::ToggleFolds]),
        shown(middle(), 90, 40, &[Command::ExpandFold, Command::ExpandFold]),
    );
}

#[test]
fn going_to_a_folded_line_opens_its_fold() {
    let mut review = Review::new(
        vec![middling("src/middle.rs"), modified("src/a.rs", "a\n", "b\n")],
        Mode::SideBySide,
        false,
        4,
    )
    .expect("the fixtures highlight");
    let mut terminal = Terminal::new(TestBackend::new(90, 12)).expect("test backend starts");
    terminal.draw(|frame| review.draw(frame, frame.area())).expect("draw succeeds");

    review.select_path(std::path::Path::new("src/middle.rs"), Some(25)).expect("highlights");
    terminal.draw(|frame| review.draw(frame, frame.area())).expect("draw succeeds");

    insta::assert_snapshot!(describe(terminal.backend().buffer()));
}

fn review(changes: Vec<FileChange>) -> (Review, Terminal<TestBackend>) {
    let mut review =
        Review::new(changes, Mode::SideBySide, false, 4).expect("the fixtures highlight");
    let mut terminal = Terminal::new(TestBackend::new(90, 8)).expect("test backend starts");
    terminal.draw(|frame| review.draw(frame, frame.area())).expect("draw succeeds");

    (review, terminal)
}

#[test]
fn marking_a_file_viewed_moves_to_the_next_unviewed_one() {
    let (mut review, _) = review(fixtures());

    review.run(Command::ToggleViewed).expect("the fixtures highlight");

    assert!(review.is_viewed(std::path::Path::new("src/gone.rs")));
    assert_eq!(review.path(), Some(std::path::Path::new("src/new.rs")));
    assert_eq!(review.counted(), "3 files · 1 viewed");
}

#[test]
fn the_next_unviewed_file_wraps_past_the_end_and_skips_viewed_ones() {
    let (mut review, _) = review(fixtures());
    review.run(Command::ToggleViewed).expect("the fixtures highlight");
    review.run(Command::NextFile).expect("the fixtures highlight");

    review.run(Command::NextUnviewed).expect("the fixtures highlight");

    assert_eq!(review.path(), Some(std::path::Path::new("src/new.rs")));
}

#[test]
fn a_viewed_file_that_changes_again_is_unviewed() {
    let (mut review, _) = review(fixtures());
    review.run(Command::NextFile).expect("the fixtures highlight");
    review.run(Command::NextFile).expect("the fixtures highlight");
    review.run(Command::ToggleViewed).expect("the fixtures highlight");
    review.run(Command::ToggleViewed).expect("the fixtures highlight");
    assert!(review.is_viewed(std::path::Path::new("src/total.rs")));
    assert!(review.is_viewed(std::path::Path::new("src/gone.rs")));

    let mut rewritten = fixtures();
    rewritten[0] = modified("src/total.rs", "let total = 1;\n", "let total = 7;\n");
    review.refresh(rewritten).expect("the fixtures highlight");

    assert!(!review.is_viewed(std::path::Path::new("src/total.rs")));
    assert!(review.is_viewed(std::path::Path::new("src/gone.rs")));
}

#[test]
fn viewed_files_are_ticked_in_the_list() {
    insta::assert_snapshot!(shown(fixtures(), 90, 8, &[Command::ToggleViewed]));
}

#[test]
fn a_lockfile_is_folded_until_asked_for() {
    let lock =
        || vec![FileChange { generated: true, ..modified("Cargo.lock", "a = 1\n", "a = 2\n") }];

    insta::assert_snapshot!(shown(lock(), 90, 4, &[]));
    assert_eq!(
        shown(lock(), 90, 4, &[Command::ExpandFold]),
        shown(vec![modified("Cargo.toml", "a = 1\n", "a = 2\n")], 90, 4, &[])
            .replace("Cargo.toml", "Cargo.lock"),
    );
}

#[test]
fn ignoring_whitespace_hides_a_reindented_line() {
    let reindented = || vec![modified("src/a.rs", "fn a() {\n  x\n}\n", "fn a() {\n    x\n}\n")];
    let (mut review, mut terminal) = review(reindented());
    assert_eq!(review.hunk().1, 1);

    review.run(Command::ToggleWhitespace).expect("the fixtures highlight");
    terminal.draw(|frame| review.draw(frame, frame.area())).expect("draw succeeds");

    assert_eq!(review.hunk().1, 0);
    assert_eq!(review.laid(), "side by side · ignoring whitespace");
}

#[test]
fn a_sideways_wheel_scrolls_the_diff_sideways() {
    let wide = || vec![modified("src/a.rs", "abcdefghijklmnop\n", "abcdefghijklmnoq\n")];

    assert_eq!(
        moused(wide(), 90, 4, &[at(MouseEventKind::ScrollRight, 60, 2)]),
        shown(wide(), 90, 4, &[Command::ScrollRight, Command::ScrollRight]),
    );
    assert_eq!(
        moused(
            wide(),
            90,
            4,
            &[MouseEvent {
                kind: MouseEventKind::ScrollDown,
                column: 60,
                row: 2,
                modifiers: KeyModifiers::SHIFT,
            }]
        ),
        shown(wide(), 90, 4, &[Command::ScrollRight, Command::ScrollRight]),
    );
}
