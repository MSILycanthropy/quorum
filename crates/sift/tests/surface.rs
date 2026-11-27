//! That keys reach commands through one catalog, and that the overlays advertise it.

use std::collections::HashSet;
use std::fmt::Write;
use std::path::PathBuf;

use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use sift::command::{Catalog, Command};
use sift::diff::{FileDiff, Side, Source};
use sift::git::{Content, FileChange, Status};
use sift::ui::{Anchor, Control, Opening, Surface};

fn modified(path: &str, old: &str, new: &str) -> FileChange {
    FileChange {
        status: Status::Modified,
        path: PathBuf::from(path),
        previous: None,
        generated: false,
        content: Content::Text(FileDiff::compute(Source::new(old), Source::new(new))),
    }
}

fn surface() -> Surface {
    let opening = Opening {
        branch: Some("main".to_owned()),
        comparing: "HEAD → worktree".to_owned(),
        ..Opening::default()
    };

    opening
        .surface(vec![
            modified("src/total.rs", "let total = 1;\n", "let total = 42;\n"),
            modified("src/other.rs", "one\ntwo\n", "one\nthree\n"),
        ])
        .expect("the fixtures highlight")
}

fn press(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, KeyModifiers::NONE)
}

fn typed(surface: &mut Surface, keys: &[KeyEvent]) -> Vec<Control> {
    keys.iter().map(|key| surface.key(*key).expect("the fixtures highlight")).collect()
}

fn screen(surface: &mut Surface, width: u16, height: u16) -> String {
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

#[test]
fn no_key_is_bound_to_two_commands() {
    let mut seen = HashSet::new();

    for binding in Catalog::default().bindings() {
        for shortcut in &binding.shortcuts {
            assert!(seen.insert(*shortcut), "{} is bound twice", shortcut.display());
        }
    }
}

#[test]
fn shift_is_folded_into_the_character_it_produces() {
    let catalog = Catalog::default();

    assert_eq!(
        catalog.command(KeyEvent::new(KeyCode::Char('?'), KeyModifiers::SHIFT)),
        Some(Command::Help)
    );
    assert_eq!(
        catalog.command(KeyEvent::new(KeyCode::Char('N'), KeyModifiers::SHIFT)),
        Some(Command::PreviousHunk)
    );
}

#[test]
fn the_status_line_names_what_is_compared_and_advertises_help() {
    insta::assert_snapshot!(screen(&mut surface(), 140, 5));
}

#[test]
fn question_mark_lists_every_key() {
    let mut surface = surface();
    typed(&mut surface, &[press(KeyCode::Char('?'))]);

    insta::assert_snapshot!(screen(&mut surface, 70, 45));
}

#[test]
fn any_key_closes_help_without_doing_anything_else() {
    let mut surface = surface();
    let before = screen(&mut surface, 100, 6);

    let controls = typed(&mut surface, &[press(KeyCode::Char('?')), press(KeyCode::Char('q'))]);

    assert_eq!(controls, vec![Control::Continue, Control::Continue]);
    assert_eq!(screen(&mut surface, 100, 6), before);
}

#[test]
fn finding_a_file_by_name_opens_it() {
    let mut surface = surface();
    typed(
        &mut surface,
        &[
            press(KeyCode::Char('/')),
            press(KeyCode::Char('t')),
            press(KeyCode::Char('o')),
            press(KeyCode::Char('t')),
            press(KeyCode::Enter),
        ],
    );

    let controls = typed(&mut surface, &[press(KeyCode::Char('y'))]);

    assert_eq!(controls, vec![Control::Copy("src/total.rs".to_owned())]);
}

#[test]
fn the_command_palette_runs_what_it_finds() {
    let mut surface = surface();
    let mut keys = vec![press(KeyCode::Char(':'))];
    keys.extend("quit".chars().map(|character| press(KeyCode::Char(character))));
    keys.push(press(KeyCode::Enter));

    let controls = typed(&mut surface, &keys);

    assert_eq!(controls.last(), Some(&Control::Quit));
}

#[test]
fn editing_asks_for_the_file_on_screen_at_the_cursors_line() {
    let mut surface = surface();
    screen(&mut surface, 100, 6);

    let controls = typed(&mut surface, &[press(KeyCode::Char('j')), press(KeyCode::Char('e'))]);

    assert_eq!(
        controls,
        vec![
            Control::Continue,
            Control::Edit { path: PathBuf::from("src/other.rs"), line: Some(2) }
        ]
    );
}

#[test]
fn help_flows_into_columns_when_the_terminal_is_short() {
    let mut surface = surface();
    typed(&mut surface, &[press(KeyCode::Char('?'))]);

    insta::assert_snapshot!(screen(&mut surface, 100, 24));
}

#[test]
fn a_file_whose_name_starts_with_the_query_outranks_one_that_merely_contains_it() {
    let opening = Opening::default();
    let mut surface = opening
        .surface(vec![
            modified("src/review.rs", "a\n", "b\n"),
            modified("src/viewer.rs", "a\n", "b\n"),
        ])
        .expect("the fixtures highlight");
    let mut keys = vec![press(KeyCode::Char('/'))];
    keys.extend("view".chars().map(|character| press(KeyCode::Char(character))));
    keys.extend([press(KeyCode::Enter), press(KeyCode::Char('y'))]);

    let controls = typed(&mut surface, &keys);

    assert_eq!(controls.last(), Some(&Control::Copy("src/viewer.rs".to_owned())));
}

#[test]
fn control_b_hides_and_shows_the_file_list() {
    let mut surface = surface();
    let shown = screen(&mut surface, 100, 6);
    let control_b = KeyEvent::new(KeyCode::Char('b'), KeyModifiers::CONTROL);

    typed(&mut surface, &[control_b]);
    let hidden = screen(&mut surface, 100, 6);
    typed(&mut surface, &[control_b]);

    assert!(shown.contains("CHANGES") && !hidden.contains("CHANGES"), "{hidden}");
    assert_eq!(screen(&mut surface, 100, 6), shown);
}

fn typing(surface: &mut Surface, text: &str) -> Vec<Control> {
    let keys: Vec<KeyEvent> =
        text.chars().map(|character| press(KeyCode::Char(character))).collect();
    typed(surface, &keys)
}

fn knowing(files: &[&str]) -> Surface {
    let opening =
        Opening { files: files.iter().map(PathBuf::from).collect(), ..Opening::default() };

    opening
        .surface(vec![modified("src/total.rs", "let total = 1;\n", "let total = 42;\n")])
        .expect("the fixtures highlight")
}

#[test]
fn finding_an_unchanged_file_asks_for_it_at_the_line_named() {
    let mut surface = knowing(&["src/git.rs", "src/total.rs"]);
    typing(&mut surface, "/src/git.rs:142:7");

    let controls = typed(&mut surface, &[press(KeyCode::Enter)]);

    assert_eq!(
        controls,
        vec![Control::Open { path: PathBuf::from("src/git.rs"), line: Some(142) }]
    );
}

#[test]
fn finding_lists_changed_files_before_unchanged_ones() {
    let mut surface = knowing(&["src/a.rs", "src/total.rs"]);
    typed(&mut surface, &[press(KeyCode::Char('/')), press(KeyCode::Enter)]);

    let controls = typed(&mut surface, &[press(KeyCode::Char('y'))]);

    assert_eq!(controls, vec![Control::Copy("src/total.rs".to_owned())]);
}

#[test]
fn escape_closes_an_unchanged_file_before_it_quits() {
    let mut surface = knowing(&["src/git.rs"]);
    let content = Content::Text(FileDiff::compute(Source::new("a\n"), Source::new("a\n")));
    surface.peek(PathBuf::from("src/git.rs"), &content, None).expect("the file highlights");

    let controls = typed(&mut surface, &[press(KeyCode::Esc), press(KeyCode::Esc)]);

    assert_eq!(controls, vec![Control::Continue, Control::Quit]);
}

#[test]
fn copying_the_location_names_the_cursors_line() {
    let mut surface = surface();
    screen(&mut surface, 100, 6);

    let controls = typed(&mut surface, &[press(KeyCode::Char('j')), press(KeyCode::Char('Y'))]);

    assert_eq!(controls, vec![Control::Continue, Control::Copy("src/other.rs:2".to_owned())]);
}

#[test]
fn the_cursor_on_a_fold_has_no_anchor() {
    let mut text = String::new();
    for line in 1..=30 {
        writeln!(text, "line {line}").expect("writing to a String cannot fail");
    }
    let changed = text.replace("line 15\n", "line 15 changed\n");
    let mut surface = Opening::default()
        .surface(vec![modified("src/middle.rs", &text, &changed)])
        .expect("the fixtures highlight");
    screen(&mut surface, 100, 12);
    assert_eq!(surface.cursor(), None);

    typed(&mut surface, &[press(KeyCode::Char('o'))]);
    screen(&mut surface, 100, 12);

    assert_eq!(
        surface.cursor(),
        Some(Anchor { path: PathBuf::from("src/middle.rs"), side: Side::New, line: 1 })
    );
}

#[test]
fn shifted_h_and_l_switch_the_cursors_side() {
    let mut surface = surface();
    screen(&mut surface, 100, 6);

    typed(&mut surface, &[press(KeyCode::Char('j')), press(KeyCode::Char('H'))]);
    let old = surface.cursor();
    typed(&mut surface, &[press(KeyCode::Char('L'))]);

    assert_eq!(old, Some(Anchor { path: PathBuf::from("src/other.rs"), side: Side::Old, line: 2 }));
    assert_eq!(
        surface.cursor(),
        Some(Anchor { path: PathBuf::from("src/other.rs"), side: Side::New, line: 2 })
    );
}

#[test]
fn shifted_arrows_switch_the_cursors_side_too() {
    let mut surface = surface();
    screen(&mut surface, 100, 6);
    let shifted = |code| KeyEvent::new(code, KeyModifiers::SHIFT);

    typed(&mut surface, &[press(KeyCode::Char('j')), shifted(KeyCode::Left)]);
    let old = surface.cursor();
    typed(&mut surface, &[shifted(KeyCode::Right)]);

    assert_eq!(old, Some(Anchor { path: PathBuf::from("src/other.rs"), side: Side::Old, line: 2 }));
    assert_eq!(
        surface.cursor(),
        Some(Anchor { path: PathBuf::from("src/other.rs"), side: Side::New, line: 2 })
    );
}

#[test]
fn marking_a_file_viewed_reports_it_to_the_host() {
    let mut surface = surface();

    let controls = typing(&mut surface, "vv");

    assert_eq!(
        controls,
        vec![
            Control::Viewed(PathBuf::from("src/other.rs")),
            Control::Viewed(PathBuf::from("src/total.rs")),
        ]
    );
}

#[test]
fn unmarking_a_file_the_host_marked_viewed_reports_it_to_the_host() {
    let mut surface = surface();
    surface.replace_viewed(vec![PathBuf::from("src/other.rs")]);

    let controls = typing(&mut surface, "v");

    assert_eq!(controls, vec![Control::Unviewed(PathBuf::from("src/other.rs"))]);
}

#[test]
fn the_host_replaces_every_viewed_mark_and_names_only_changed_files() {
    let mut surface = surface();
    typing(&mut surface, "v");

    surface.replace_viewed(vec![PathBuf::from("src/total.rs"), PathBuf::from("src/gone.rs")]);

    insta::assert_snapshot!(screen(&mut surface, 100, 6));
}

#[test]
fn a_mark_the_host_set_lapses_when_the_file_changes() {
    let mut surface = surface();
    surface.replace_viewed(vec![PathBuf::from("src/total.rs"), PathBuf::from("src/other.rs")]);

    surface
        .refresh(vec![
            modified("src/total.rs", "let total = 1;\n", "let total = 42;\n"),
            modified("src/other.rs", "one\ntwo\n", "one\nfour\n"),
        ])
        .expect("the fixtures highlight");

    insta::assert_snapshot!(screen(&mut surface, 100, 6));
}
