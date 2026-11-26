//! That host text lands under the line it is anchored to, and that the reader can move onto it.

use std::fmt::Write;
use std::path::{Path, PathBuf};

use hue::Rgb;
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::buffer::Buffer;
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::style::Color;
use sift::command::Command;
use sift::diff::{FileDiff, Side, Source};
use sift::git::{Content, FileChange, Status};
use sift::theme::{Theme, TokyoNight};
use sift::ui::{Anchor, Annotation, Error, Mode, Opening, Surface, Viewer};

const OLD: &str = "fn total(cart: &Cart) -> u32 {\n    let total = 1;\n    dead();\n    total\n}\n";
const NEW: &str = "fn total(cart: &Cart) -> u32 {\n    let total = 42;\n    total\n}\n";

fn note(id: &str, side: Side, line: usize, heading: &str, body: &str) -> Annotation {
    Annotation {
        id: id.to_owned(),
        anchor: Anchor { path: PathBuf::from("demo.rs"), side, line },
        heading: heading.to_owned(),
        body: body.to_owned(),
        faded: false,
        collapsed: false,
    }
}

fn on(path: &str, annotation: Annotation) -> Annotation {
    Annotation { anchor: Anchor { path: PathBuf::from(path), ..annotation.anchor }, ..annotation }
}

fn why() -> Annotation {
    note("why", Side::New, 2, "alice · 1 comment", "Why 42?")
}

fn far_apart() -> (String, String) {
    let mut old = String::new();
    for line in 1..=40 {
        writeln!(old, "line {line}").expect("writing to a String cannot fail");
    }
    let new = old.replace("line 5\n", "line five\n").replace("line 30\n", "line thirty\n");

    (old, new)
}

/// A viewer over a diff, drawn into a test terminal after each command.
struct Shown {
    viewer: Viewer,
    terminal: Terminal<TestBackend>,
}

impl Shown {
    fn new(old: &str, new: &str, mode: Mode, wrap: bool, size: (u16, u16)) -> Self {
        let diff = FileDiff::compute(Source::new(old), Source::new(new));
        let viewer = Viewer::new(diff, Path::new("demo.rs"), mode, wrap, 4, &Theme::default())
            .expect("fixtures highlight");
        let terminal =
            Terminal::new(TestBackend::new(size.0, size.1)).expect("test backend starts");
        let mut shown = Self { viewer, terminal };
        shown.draw();

        shown
    }

    fn demo(mode: Mode, size: (u16, u16), annotations: Vec<Annotation>) -> Self {
        let mut shown = Self::new(OLD, NEW, mode, false, size);
        shown.annotate(annotations);

        shown
    }

    fn annotate(&mut self, annotations: Vec<Annotation>) {
        self.viewer.annotate(annotations).expect("fixtures highlight");
        self.draw();
    }

    fn run(&mut self, commands: &[Command]) -> &mut Self {
        for command in commands {
            self.viewer.run(*command).expect("fixtures highlight");
            self.draw();
        }
        self
    }

    fn draw(&mut self) {
        let viewer = &mut self.viewer;
        self.terminal
            .draw(|frame| frame.render_widget(&mut *viewer, frame.area()))
            .expect("draw succeeds");
    }

    fn text(&self) -> String {
        describe(self.terminal.backend().buffer(), background)
    }

    fn ink(&self) -> String {
        describe(self.terminal.backend().buffer(), foreground)
    }
}

fn describe(buffer: &Buffer, tint: fn(&ratatui::buffer::Cell) -> char) -> String {
    let area = buffer.area();
    let mut out = String::new();

    for y in 0..area.height {
        let mut text = String::new();
        let mut tints = String::new();
        for x in 0..area.width {
            let cell = &buffer[(x, y)];
            text.push_str(cell.symbol());
            tints.push(tint(cell));
        }
        writeln!(out, "{text}\n{tints}").expect("writing to a String cannot fail");
    }
    out
}

fn rgb(color: Color) -> Option<Rgb> {
    let Color::Rgb(red, green, blue) = color else {
        return None;
    };
    Some(Rgb::new(red, green, blue))
}

fn background(cell: &ratatui::buffer::Cell) -> char {
    let tints = TokyoNight::TINTS;
    match rgb(cell.bg) {
        Some(found) if found == tints.background => '.',
        Some(found) if found == tints.filler => ' ',
        Some(found) if found == tints.removed => '-',
        Some(found) if found == tints.removed_emphasis => '=',
        Some(found) if found == tints.added => '+',
        Some(found) if found == tints.added_emphasis => '#',
        Some(found) if found == tints.under_cursor(tints.background) => ':',
        Some(found) if found == tints.under_cursor(tints.filler) => '_',
        Some(found) if found == tints.under_cursor(tints.removed) => '~',
        Some(found) if found == tints.under_cursor(tints.removed_emphasis) => '!',
        Some(found) if found == tints.under_cursor(tints.added) => '*',
        Some(found) if found == tints.under_cursor(tints.added_emphasis) => '%',
        _ => '?',
    }
}

fn foreground(cell: &ratatui::buffer::Cell) -> char {
    let tints = TokyoNight::TINTS;
    if cell.symbol() == " " {
        return ' ';
    }
    match rgb(cell.fg) {
        Some(found) if found == tints.foreground => 'f',
        Some(found) if found == tints.muted => 'm',
        Some(found) if found == tints.attention => 'a',
        Some(found) if found == tints.gutter => 'g',
        _ => '?',
    }
}

#[test]
fn an_annotation_is_drawn_under_the_line_it_is_anchored_to() {
    insta::assert_snapshot!(Shown::demo(Mode::SideBySide, (64, 8), vec![why()]).text());
}

#[test]
fn an_old_side_annotation_sits_in_the_left_pane_in_side_by_side() {
    let dead = note("dead", Side::Old, 3, "bob", "Good riddance.");

    insta::assert_snapshot!(Shown::demo(Mode::SideBySide, (64, 8), vec![dead]).text());
}

#[test]
fn unified_puts_an_old_side_annotation_under_the_deleted_line() {
    let one = note("one", Side::Old, 2, "bob", "Was one.");

    insta::assert_snapshot!(Shown::demo(Mode::Unified, (44, 10), vec![one]).text());
}

#[test]
fn annotations_on_one_row_stack_old_side_first_then_in_the_order_given() {
    let annotations = vec![
        note("first", Side::New, 2, "first new", ""),
        note("old", Side::Old, 2, "old", ""),
        note("second", Side::New, 2, "second new", ""),
    ];

    insta::assert_snapshot!(Shown::demo(Mode::SideBySide, (64, 9), annotations).text());
}

#[test]
fn an_annotation_on_a_wrapped_line_comes_after_its_last_piece() {
    let mut shown = Shown::new(OLD, NEW, Mode::SideBySide, true, (34, 10));
    shown.annotate(vec![note("signature", Side::New, 1, "alice", "Too long.")]);

    insta::assert_snapshot!(shown.text());
}

#[test]
fn a_body_wraps_at_words_and_keeps_its_hard_line_breaks() {
    let body = "This number is far too magic to live here.\nName it.";

    insta::assert_snapshot!(
        Shown::demo(Mode::Unified, (36, 10), vec![note("magic", Side::New, 2, "alice", body)])
            .text()
    );
}

#[test]
fn a_wrapped_body_line_keeps_its_indentation() {
    let body = "Try this:\n    let total = cart.items().map(Item::price).sum();";

    insta::assert_snapshot!(
        Shown::demo(Mode::Unified, (40, 12), vec![note("try", Side::New, 2, "alice", body)]).text()
    );
}

#[test]
fn a_word_wider_than_the_box_breaks_at_its_edge() {
    let body = "see https://example.com/a/very/long/path/that/does/not/fit";

    insta::assert_snapshot!(
        Shown::demo(Mode::Unified, (36, 10), vec![note("link", Side::New, 2, "alice", body)])
            .text()
    );
}

#[test]
fn scrolling_sideways_leaves_annotations_where_they_are() {
    let mut shown = Shown::demo(Mode::SideBySide, (64, 8), vec![why()]);
    shown.run(&[Command::ScrollRight, Command::ScrollRight]);

    insta::assert_snapshot!(shown.text());
}

#[test]
fn a_heading_too_long_for_its_row_is_cut_with_an_ellipsis() {
    let long = note("long", Side::New, 2, "alice, bob, carol and dave · 9 comments", "");

    insta::assert_snapshot!(Shown::demo(Mode::SideBySide, (64, 8), vec![long]).text());
}

#[test]
fn a_faded_annotation_is_drawn_in_muted_colours() {
    let annotations =
        vec![Annotation { faded: true, ..why() }, note("live", Side::New, 3, "bob", "Live.")];

    insta::assert_snapshot!(Shown::demo(Mode::Unified, (44, 10), annotations).ink());
}

#[test]
fn a_collapsed_annotation_is_its_heading_alone() {
    let collapsed = Annotation { collapsed: true, ..why() };

    insta::assert_snapshot!(Shown::demo(Mode::SideBySide, (64, 8), vec![collapsed]).text());
}

#[test]
fn an_annotation_with_an_empty_body_is_a_heading_alone() {
    let empty = Annotation { body: String::new(), ..why() };

    insta::assert_snapshot!(Shown::demo(Mode::SideBySide, (64, 8), vec![empty]).text());
}

#[test]
fn control_characters_in_a_body_never_reach_the_buffer() {
    let hostile =
        note("hostile", Side::New, 2, "eve\x1b]0;owned\x07", "red \x1b[31mtext\x07\x7f\u{9b}done");

    insta::assert_snapshot!(Shown::demo(Mode::Unified, (44, 8), vec![hostile]).text());
}

#[test]
fn carriage_returns_in_a_body_add_no_rows() {
    let windows = note("lines", Side::New, 2, "alice", "one\r\ntwo\rthree\r\n");
    let unix = note("lines", Side::New, 2, "alice", "one\ntwo\nthree");

    assert_eq!(
        Shown::demo(Mode::Unified, (44, 10), vec![windows]).text(),
        Shown::demo(Mode::Unified, (44, 10), vec![unix]).text()
    );
}

#[test]
fn a_pane_too_narrow_for_text_still_draws_without_panicking() {
    insta::assert_snapshot!(Shown::demo(Mode::SideBySide, (12, 8), vec![why()]).text());
}

fn far_apart_shown(height: u16, annotations: Vec<Annotation>) -> Shown {
    let (old, new) = far_apart();
    let mut shown = Shown::new(&old, &new, Mode::SideBySide, false, (48, height));
    shown.annotate(annotations);

    shown
}

#[test]
fn a_fold_never_hides_an_annotated_line() {
    let middle = note("middle", Side::New, 18, "alice", "Why is this here?");

    insta::assert_snapshot!(far_apart_shown(24, vec![middle]).text());
}

#[test]
fn opening_one_piece_of_a_split_fold_leaves_the_other_folded() {
    let middle = note("middle", Side::New, 18, "alice", "Why is this here?");
    let mut shown = far_apart_shown(36, vec![middle]);
    shown.run(&[Command::ExpandFold]);

    insta::assert_snapshot!(shown.text());
}

#[test]
fn the_cursor_stops_on_every_row_of_an_annotation() {
    let body = "One.\nTwo.\nThree.";
    let mut shown =
        Shown::demo(Mode::Unified, (44, 12), vec![note("rows", Side::New, 2, "alice", body)]);
    shown.run(&[Command::NextAnnotation, Command::NextLine, Command::NextLine]);

    insta::assert_snapshot!(shown.text());
}

#[test]
fn holding_down_through_an_annotation_stops_where_jumping_to_the_end_does() {
    let body = "One.\nTwo.\nThree.";
    let rows = || vec![note("rows", Side::New, 2, "alice", body)];
    let mut held = Shown::demo(Mode::Unified, (44, 5), rows());
    held.run(&[Command::NextLine; 30]);
    let mut jumped = Shown::demo(Mode::Unified, (44, 5), rows());
    jumped.run(&[Command::Bottom]);

    assert_eq!(held.text(), jumped.text());
}

#[test]
fn an_annotation_does_not_split_its_hunk_in_two() {
    let shown = Shown::demo(Mode::SideBySide, (64, 8), vec![why()]);

    assert_eq!(shown.viewer.hunk().1, 1);
}

fn on_second_body_row() -> Shown {
    let body = "One.\nTwo.\nThree.";
    let mut shown =
        Shown::demo(Mode::Unified, (44, 12), vec![note("rows", Side::New, 2, "alice", body)]);
    shown.run(&[Command::NextAnnotation, Command::NextLine, Command::NextLine]);

    shown
}

#[test]
fn enter_on_an_annotation_collapses_it_and_puts_the_cursor_on_its_heading() {
    let mut shown = on_second_body_row();
    shown.run(&[Command::ExpandFold]);

    insta::assert_snapshot!(shown.text());
}

#[test]
fn enter_on_a_collapsed_annotation_opens_it() {
    let mut shown = on_second_body_row();
    shown.run(&[Command::ExpandFold, Command::ExpandFold]);

    insta::assert_snapshot!(shown.text());
}

#[test]
fn enter_on_an_annotation_with_an_empty_body_changes_nothing() {
    let empty = || vec![Annotation { body: String::new(), ..why() }];
    let mut entered = Shown::demo(Mode::SideBySide, (64, 8), empty());
    entered.run(&[Command::NextAnnotation, Command::ExpandFold]);
    let mut moved = Shown::demo(Mode::SideBySide, (64, 8), empty());
    moved.run(&[Command::NextAnnotation]);

    assert_eq!(entered.text(), moved.text());
}

#[test]
fn collapsing_every_annotation_leaves_one_heading_row_each() {
    let annotations = vec![why(), note("dead", Side::Old, 3, "bob", "Good riddance.\nReally.")];
    let mut shown = Shown::demo(Mode::Unified, (44, 12), annotations);
    shown.run(&[Command::ToggleAnnotations]);

    insta::assert_snapshot!(shown.text());
}

#[test]
fn next_annotation_lands_on_the_heading_of_the_one_below() {
    let annotations = vec![why(), note("last", Side::New, 4, "bob", "Close it.")];
    let mut shown = Shown::demo(Mode::Unified, (44, 12), annotations);
    shown.run(&[Command::NextAnnotation, Command::NextAnnotation]);

    insta::assert_snapshot!(shown.text());
}

#[test]
fn previous_annotation_from_a_body_row_skips_that_annotations_own_heading() {
    let annotations = vec![
        note("first", Side::New, 1, "alice", "Hello."),
        note("rows", Side::New, 2, "bob", "One.\nTwo."),
    ];
    let mut shown = Shown::demo(Mode::Unified, (44, 14), annotations);
    shown.run(&[
        Command::NextAnnotation,
        Command::NextAnnotation,
        Command::NextLine,
        Command::PreviousAnnotation,
    ]);

    assert_eq!(shown.viewer.annotation().map(|annotation| annotation.id.as_str()), Some("first"));
}

#[test]
fn the_cursor_on_an_annotation_reports_its_anchor() {
    let mut shown =
        Shown::demo(Mode::SideBySide, (64, 8), vec![note("dead", Side::Old, 3, "bob", "Gone.")]);
    shown.run(&[Command::NextAnnotation]);

    assert_eq!(shown.viewer.cursor(), Some((Side::Old, 3)));
}

#[test]
fn the_cursor_on_a_diff_line_reports_no_annotation() {
    let shown = Shown::demo(Mode::SideBySide, (64, 8), vec![why()]);

    assert_eq!(shown.viewer.annotation(), None);
}

#[test]
fn the_line_on_an_annotation_is_the_line_it_is_anchored_to() {
    let mut shown = Shown::demo(Mode::SideBySide, (64, 8), vec![why()]);
    shown.run(&[Command::NextAnnotation]);

    assert_eq!(shown.viewer.line(), Some(2));
}

#[test]
fn side_keys_do_nothing_on_an_annotation() {
    let mut sided = Shown::demo(Mode::SideBySide, (64, 8), vec![why()]);
    sided.run(&[Command::NextAnnotation, Command::OldSide]);
    let mut still = Shown::demo(Mode::SideBySide, (64, 8), vec![why()]);
    still.run(&[Command::NextAnnotation]);

    assert_eq!(sided.text(), still.text());
}

#[test]
fn the_cursor_on_an_annotation_stays_on_it_when_the_layout_switches() {
    let mut shown = Shown::demo(Mode::SideBySide, (64, 8), vec![why()]);
    shown.run(&[Command::NextAnnotation, Command::ToggleLayout]);

    assert_eq!(shown.viewer.annotation().map(|annotation| annotation.id.as_str()), Some("why"));
}

#[test]
fn the_cursor_on_an_annotation_stays_on_it_when_wrapping_toggles() {
    let mut shown = Shown::demo(Mode::SideBySide, (64, 8), vec![why()]);
    shown.run(&[Command::NextAnnotation, Command::ToggleWrapping]);

    assert_eq!(shown.viewer.annotation().map(|annotation| annotation.id.as_str()), Some("why"));
}

#[test]
fn the_cursor_on_a_body_row_stays_on_that_row_across_a_relayout() {
    let mut shown = on_second_body_row();
    let before = shown.text();
    shown.run(&[Command::ToggleWrapping, Command::ToggleWrapping]);

    assert_eq!(shown.text(), before);
}

#[test]
fn clicking_an_annotation_puts_the_cursor_on_it_without_collapsing_it() {
    let mut shown = Shown::demo(Mode::SideBySide, (64, 8), vec![why()]);
    shown.viewer.click(10, 3).expect("fixtures highlight");
    shown.draw();

    insta::assert_snapshot!(shown.text());
}

fn modified(path: &str, old: &str, new: &str) -> FileChange {
    FileChange {
        status: Status::Modified,
        path: PathBuf::from(path),
        previous: None,
        generated: false,
        content: Content::Text(FileDiff::compute(Source::new(old), Source::new(new))),
    }
}

fn changes() -> Vec<FileChange> {
    vec![
        modified("src/a.rs", "one\ntwo\nthree\n", "one\n2\nthree\n"),
        modified("src/b.rs", "four\nfive\n", "four\n5\n"),
        modified("src/c.rs", "six\nseven\n", "six\n7\n"),
        FileChange {
            status: Status::Added,
            path: PathBuf::from("src/d.rs"),
            previous: None,
            generated: false,
            content: Content::Text(FileDiff::compute(Source::new(""), Source::new("eight\n"))),
        },
        FileChange {
            status: Status::Modified,
            path: PathBuf::from("src/e.png"),
            previous: None,
            generated: false,
            content: Content::Binary,
        },
    ]
}

fn surface(changes: Vec<FileChange>) -> Surface {
    let opening = Opening {
        branch: Some("main".to_owned()),
        comparing: "main → feature".to_owned(),
        ..Opening::default()
    };
    opening.surface(changes).expect("the fixtures highlight")
}

fn screen(surface: &mut Surface, width: u16, height: u16) -> String {
    let mut terminal = Terminal::new(TestBackend::new(width, height)).expect("test backend starts");
    terminal.draw(|frame| frame.render_widget(&mut *surface, frame.area())).expect("draw succeeds");

    describe(terminal.backend().buffer(), background)
}

fn words(surface: &mut Surface, width: u16, height: u16) -> String {
    let mut terminal = Terminal::new(TestBackend::new(width, height)).expect("test backend starts");
    terminal.draw(|frame| frame.render_widget(&mut *surface, frame.area())).expect("draw succeeds");

    let buffer = terminal.backend().buffer();
    let mut out = String::new();
    for y in 0..buffer.area().height {
        let line: String =
            (0..buffer.area().width).map(|x| buffer[(x, y)].symbol().to_owned()).collect();
        writeln!(out, "{}", line.trim_end()).expect("writing to a String cannot fail");
    }
    out
}

fn steps(surface: &mut Surface, commands: &[Command]) {
    for command in commands {
        screen(surface, 90, 12);
        surface.run(*command).expect("the fixtures highlight");
    }
    screen(surface, 90, 12);
}

fn reviewed() -> Surface {
    let mut surface = surface(changes());
    surface
        .annotate(vec![
            on("src/a.rs", note("a", Side::New, 2, "alice", "Why 2?\nSpell it out.")),
            on("src/c.rs", note("c", Side::New, 2, "bob", "And 7?")),
        ])
        .expect("the fixtures highlight");
    screen(&mut surface, 90, 12);

    surface
}

fn under(surface: &Surface) -> Option<String> {
    surface.annotation().map(|annotation| annotation.id.clone())
}

#[test]
fn an_annotation_reaches_the_screen_through_the_surface() {
    insta::assert_snapshot!(screen(&mut reviewed(), 90, 12));
}

#[test]
fn the_annotation_under_the_cursor_is_reported_by_id() {
    let mut surface = reviewed();
    steps(&mut surface, &[Command::NextAnnotation]);

    assert_eq!(under(&surface), Some("a".to_owned()));
}

#[test]
fn cursor_and_annotation_agree_on_every_row_of_an_annotation() {
    let mut surface = reviewed();
    steps(&mut surface, &[Command::NextAnnotation]);
    let anchor = Anchor { path: PathBuf::from("src/a.rs"), side: Side::New, line: 2 };

    for _ in 0..3 {
        assert_eq!(under(&surface), Some("a".to_owned()));
        assert_eq!(surface.cursor(), Some(anchor.clone()));
        steps(&mut surface, &[Command::NextLine]);
    }
    assert_eq!(under(&surface), None);
}

#[test]
fn two_annotations_with_one_id_are_rejected_and_the_previous_set_is_kept() {
    let mut surface = reviewed();
    let repeated = vec![
        on("src/b.rs", note("same", Side::New, 1, "one", "")),
        on("src/b.rs", note("same", Side::New, 2, "two", "")),
    ];
    let before = screen(&mut surface, 90, 12);

    let refused = surface.annotate(repeated);

    assert!(matches!(refused, Err(Error::DuplicateAnnotation(id)) if id == "same"));
    assert_eq!(screen(&mut surface, 90, 12), before);
}

fn unplaced_ids(annotations: Vec<Annotation>) -> Vec<String> {
    let mut surface = surface(changes());
    surface.annotate(annotations).expect("the fixtures highlight");

    surface.unplaced().into_iter().map(|annotation| annotation.id.clone()).collect()
}

#[test]
fn an_anchor_past_the_end_of_the_file_is_unplaced_and_not_drawn() {
    let past = on("src/a.rs", note("past", Side::New, 40, "alice", "Gone."));
    let mut surface = surface(changes());
    let bare = screen(&mut surface, 90, 12);
    surface.annotate(vec![past.clone()]).expect("the fixtures highlight");

    assert_eq!(unplaced_ids(vec![past]), ["past"]);
    assert_eq!(screen(&mut surface, 90, 12), bare);
}

#[test]
fn an_anchor_on_line_zero_is_unplaced() {
    assert_eq!(unplaced_ids(vec![on("src/a.rs", note("zero", Side::New, 0, "", ""))]), ["zero"]);
}

#[test]
fn an_old_side_anchor_in_an_added_file_is_unplaced() {
    assert_eq!(unplaced_ids(vec![on("src/d.rs", note("old", Side::Old, 1, "", ""))]), ["old"]);
}

#[test]
fn an_anchor_in_a_file_outside_the_review_is_unplaced() {
    assert_eq!(unplaced_ids(vec![on("src/z.rs", note("away", Side::New, 1, "", ""))]), ["away"]);
}

#[test]
fn an_anchor_in_a_binary_file_is_unplaced() {
    assert_eq!(unplaced_ids(vec![on("src/e.png", note("png", Side::New, 1, "", ""))]), ["png"]);
}

#[test]
fn the_next_annotation_past_the_last_one_in_a_file_is_the_first_in_the_next_annotated_file() {
    let mut surface = reviewed();
    steps(&mut surface, &[Command::NextAnnotation, Command::NextAnnotation]);

    assert_eq!(under(&surface), Some("c".to_owned()));
}

#[test]
fn next_annotation_skips_files_without_any() {
    let mut surface = reviewed();
    steps(&mut surface, &[Command::NextAnnotation, Command::NextAnnotation]);

    assert_eq!(surface.cursor().map(|anchor| anchor.path), Some(PathBuf::from("src/c.rs")));
}

#[test]
fn the_previous_annotation_before_the_first_is_the_last_in_the_previous_annotated_file() {
    let mut surface = reviewed();
    steps(&mut surface, &[Command::NextAnnotation, Command::NextAnnotation]);
    steps(&mut surface, &[Command::PreviousAnnotation]);

    assert_eq!(under(&surface), Some("a".to_owned()));
}

#[test]
fn next_annotation_past_the_last_annotated_file_stays_put() {
    let mut surface = reviewed();
    steps(&mut surface, &[Command::NextAnnotation, Command::NextAnnotation]);
    let before = screen(&mut surface, 90, 12);
    steps(&mut surface, &[Command::NextAnnotation]);

    assert_eq!(screen(&mut surface, 90, 12), before);
}

#[test]
fn going_to_an_annotation_by_id_selects_its_file_and_lands_on_its_heading() {
    let mut surface = reviewed();

    assert!(surface.go_to_annotation("c").expect("the fixtures highlight"));
    insta::assert_snapshot!(screen(&mut surface, 90, 12));
}

#[test]
fn going_to_an_unknown_annotation_changes_nothing() {
    let mut surface = reviewed();
    let before = screen(&mut surface, 90, 12);

    assert!(!surface.go_to_annotation("nowhere").expect("the fixtures highlight"));
    assert_eq!(screen(&mut surface, 90, 12), before);
}

#[test]
fn an_annotation_the_reader_collapsed_stays_collapsed_after_switching_files_and_back() {
    let mut surface = reviewed();
    steps(&mut surface, &[Command::NextAnnotation, Command::ExpandFold]);
    let collapsed = screen(&mut surface, 90, 12);
    steps(&mut surface, &[Command::NextFile, Command::PreviousFile, Command::NextAnnotation]);

    assert_eq!(screen(&mut surface, 90, 12), collapsed);
}

#[test]
fn the_readers_collapse_outlives_reannotation_but_new_ids_take_the_hosts_default() {
    let mut surface = reviewed();
    steps(&mut surface, &[Command::NextAnnotation, Command::ExpandFold]);
    surface
        .annotate(vec![
            on("src/a.rs", note("a", Side::New, 2, "alice", "Why 2?\nSpell it out.")),
            on(
                "src/a.rs",
                Annotation { collapsed: true, ..note("new", Side::New, 3, "bob", "Hidden.") },
            ),
        ])
        .expect("the fixtures highlight");

    insta::assert_snapshot!(screen(&mut surface, 90, 12));
}

#[test]
fn the_readers_collapse_outlives_the_annotation_leaving_and_coming_back() {
    let mut surface = reviewed();
    steps(&mut surface, &[Command::NextAnnotation, Command::ExpandFold]);
    surface.annotate(Vec::new()).expect("the fixtures highlight");
    surface
        .annotate(vec![
            on("src/a.rs", note("a", Side::New, 2, "alice", "Why 2?\nSpell it out.")),
            on("src/c.rs", note("c", Side::New, 2, "bob", "And 7?")),
        ])
        .expect("the fixtures highlight");

    screen(&mut surface, 90, 12);

    assert_eq!(surface.annotation().map(|annotation| annotation.collapsed), Some(true));
}

#[test]
fn the_cursor_on_an_annotation_stays_on_it_when_whitespace_is_ignored() {
    let mut surface = reviewed();
    steps(&mut surface, &[Command::NextAnnotation, Command::NextLine, Command::ToggleWhitespace]);

    assert_eq!(under(&surface), Some("a".to_owned()));
}

#[test]
fn the_cursor_on_an_annotation_stays_on_it_across_a_rescan() {
    let mut surface = reviewed();
    steps(&mut surface, &[Command::NextAnnotation, Command::NextLine]);
    let before = screen(&mut surface, 90, 12);
    surface.refresh(changes()).expect("the fixtures highlight");

    assert_eq!(screen(&mut surface, 90, 12), before);
}

#[test]
fn reannotating_without_the_annotation_under_the_cursor_falls_back_to_its_line() {
    let mut surface = reviewed();
    steps(&mut surface, &[Command::NextAnnotation]);
    surface.annotate(Vec::new()).expect("the fixtures highlight");
    screen(&mut surface, 90, 12);

    assert_eq!(
        surface.cursor(),
        Some(Anchor { path: PathBuf::from("src/a.rs"), side: Side::New, line: 2 })
    );
}

#[test]
fn annotating_with_the_same_set_again_keeps_the_cursor_and_the_scroll() {
    let mut surface = reviewed();
    steps(&mut surface, &[Command::NextAnnotation, Command::NextLine]);
    let before = screen(&mut surface, 90, 12);
    surface
        .annotate(vec![
            on("src/a.rs", note("a", Side::New, 2, "alice", "Why 2?\nSpell it out.")),
            on("src/c.rs", note("c", Side::New, 2, "bob", "And 7?")),
        ])
        .expect("the fixtures highlight");

    assert_eq!(screen(&mut surface, 90, 12), before);
}

#[test]
fn a_rescan_that_shortens_the_file_makes_an_annotation_past_its_end_unplaced() {
    let mut surface = reviewed();
    let mut shorter = changes();
    shorter[0] = modified("src/a.rs", "one", "1");
    surface.refresh(shorter).expect("the fixtures highlight");

    let unplaced: Vec<&str> =
        surface.unplaced().into_iter().map(|annotation| annotation.id.as_str()).collect();
    assert_eq!(unplaced, ["a"]);
}

#[test]
fn an_annotated_generated_file_is_not_collapsed() {
    let mut lockfile = modified("Cargo.lock", "a = 1\n", "a = 2\n");
    lockfile.generated = true;
    let mut surface = surface(vec![lockfile]);
    surface
        .annotate(vec![on("Cargo.lock", note("lock", Side::New, 1, "alice", "Why bump?"))])
        .expect("the fixtures highlight");

    insta::assert_snapshot!(screen(&mut surface, 90, 8));
}

#[test]
fn a_peeked_unchanged_file_shows_no_annotations() {
    let mut surface = reviewed();
    let text = "one\ntwo\n";
    let unchanged = Content::Text(FileDiff::compute(Source::new(text), Source::new(text)));
    surface
        .annotate(vec![on("src/lib.rs", note("lib", Side::New, 1, "alice", "Hidden."))])
        .expect("the fixtures highlight");
    surface.peek(PathBuf::from("src/lib.rs"), &unchanged, None).expect("the file highlights");

    insta::assert_snapshot!(screen(&mut surface, 90, 8));
}

#[test]
fn next_annotation_while_peeking_leaves_the_peek() {
    let mut surface = reviewed();
    let text = "one\ntwo\n";
    let unchanged = Content::Text(FileDiff::compute(Source::new(text), Source::new(text)));
    surface.peek(PathBuf::from("src/lib.rs"), &unchanged, None).expect("the file highlights");
    steps(&mut surface, &[Command::NextAnnotation]);

    assert_eq!(surface.peeked(), None);
    assert_eq!(under(&surface), Some("a".to_owned()));
}

#[test]
fn the_status_line_counts_the_annotations_in_the_file_on_screen() {
    let mut surface = reviewed();
    let before = words(&mut surface, 120, 12);
    steps(&mut surface, &[Command::NextAnnotation]);
    let after = words(&mut surface, 120, 12);

    insta::assert_snapshot!(format!("{before}\n{after}"));
}

#[test]
fn help_lists_the_annotation_keys_only_once_the_surface_is_annotated() {
    let help = KeyEvent::new(KeyCode::Char('?'), KeyModifiers::NONE);
    let mut bare = surface(changes());
    bare.key(help).expect("the fixtures highlight");
    let mut annotated = surface(changes());
    annotated.annotate(Vec::new()).expect("the fixtures highlight");
    annotated.key(help).expect("the fixtures highlight");

    insta::assert_snapshot!(format!(
        "{}\n{}",
        words(&mut bare, 120, 30),
        words(&mut annotated, 120, 30)
    ));
}
