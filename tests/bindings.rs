//! That a host adds its own keys to the catalog, and hears when one is pressed.

use std::fmt::Write;
use std::path::PathBuf;

use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use sift::command::{self, Binding, Category, Command, Shortcut};
use sift::diff::{FileDiff, Side, Source};
use sift::git::{Content, FileChange, Status};
use sift::ui::{Anchor, Annotation, Control, Error, Opening, Surface};

const REVIEW: Category = Category::Host("Review");
const PULL_REQUEST: Category = Category::Host("Pull request");

fn key(character: char) -> Shortcut {
    Shortcut::new(KeyCode::Char(character), KeyModifiers::NONE)
}

fn host(
    name: &'static str,
    label: &'static str,
    character: char,
    category: Category,
    hint: Option<&'static str>,
) -> Binding {
    Binding { command: Command::Host(name), label, shortcuts: vec![key(character)], category, hint }
}

fn glean() -> Vec<Binding> {
    vec![
        host("comment", "Comment on this line", 'c', REVIEW, Some("comment")),
        host("reply", "Reply to this thread", 'R', REVIEW, Some("reply")),
        host("resolve", "Resolve or unresolve", 'x', REVIEW, None),
        host("approve", "Approve", 'A', PULL_REQUEST, None),
        host("merge", "Merge", 'M', PULL_REQUEST, Some("merge")),
    ]
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
        modified("src/total.rs", "let total = 1;\n", "let total = 42;\n"),
        modified("src/other.rs", "one\ntwo\n", "one\nthree\n"),
    ]
}

fn opened(bindings: Vec<Binding>) -> sift::ui::Result<Surface> {
    unbinding(Vec::new(), bindings)
}

fn unbinding(unbound: Vec<Command>, bindings: Vec<Binding>) -> sift::ui::Result<Surface> {
    let opening = Opening {
        branch: Some("main".to_owned()),
        comparing: "HEAD → worktree".to_owned(),
        bindings,
        unbound,
        ..Opening::default()
    };

    opening.surface(changes())
}

fn without_edit() -> Surface {
    unbinding(vec![Command::Edit], glean()).expect("edit can be unbound")
}

fn refused_unbinding(unbound: Vec<Command>) -> command::Error {
    match unbinding(unbound, glean()) {
        Err(Error::Binding(error)) => error,
        Err(error) => panic!("refused for the wrong reason: {error}"),
        Ok(_) => panic!("the unbinding was accepted"),
    }
}

fn surface() -> Surface {
    opened(glean()).expect("the host bindings are free")
}

fn refused(bindings: Vec<Binding>) -> command::Error {
    match opened(bindings) {
        Err(Error::Binding(error)) => error,
        Err(error) => panic!("refused for the wrong reason: {error}"),
        Ok(_) => panic!("the bindings were accepted"),
    }
}

fn press(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, KeyModifiers::NONE)
}

fn typed(surface: &mut Surface, keys: &[KeyEvent]) -> Vec<Control> {
    keys.iter().map(|key| surface.key(*key).expect("the fixtures highlight")).collect()
}

fn screen(surface: &mut Surface, width: u16, height: u16) -> String {
    let mut terminal = Terminal::new(TestBackend::new(width, height)).expect("test backend starts");
    terminal.draw(|frame| surface.draw(frame)).expect("draw succeeds");

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
fn a_host_key_returns_its_name_and_changes_nothing_in_sift() {
    let mut surface = surface();
    let before = screen(&mut surface, 100, 6);
    let cursor = surface.cursor();

    let controls = typed(&mut surface, &[press(KeyCode::Char('R'))]);

    assert_eq!(controls, vec![Control::Host("reply")]);
    assert_eq!(surface.cursor(), cursor);
    assert_eq!(screen(&mut surface, 100, 6), before);
}

#[test]
fn a_shifted_host_key_matches_however_the_terminal_reports_it() {
    let mut surface = surface();

    let controls = typed(&mut surface, &[KeyEvent::new(KeyCode::Char('R'), KeyModifiers::SHIFT)]);

    assert_eq!(controls, vec![Control::Host("reply")]);
}

#[test]
fn a_host_key_does_nothing_while_the_palette_is_open() {
    let mut surface = surface();

    let controls = typed(&mut surface, &[press(KeyCode::Char(':')), press(KeyCode::Char('c'))]);

    assert_eq!(controls, vec![Control::Continue, Control::Continue]);
    insta::assert_snapshot!(screen(&mut surface, 100, 8));
}

#[test]
fn a_host_key_does_nothing_while_help_is_open() {
    let mut surface = surface();
    let before = screen(&mut surface, 100, 6);

    let controls = typed(&mut surface, &[press(KeyCode::Char('?')), press(KeyCode::Char('c'))]);

    assert_eq!(controls, vec![Control::Continue, Control::Continue]);
    assert_eq!(screen(&mut surface, 100, 6), before);
}

#[test]
fn choosing_a_host_command_in_the_palette_returns_its_name() {
    let mut surface = surface();
    typed(&mut surface, &[press(KeyCode::Char(':'))]);
    for character in "merge".chars() {
        typed(&mut surface, &[press(KeyCode::Char(character))]);
    }

    let controls = typed(&mut surface, &[press(KeyCode::Enter)]);

    assert_eq!(controls, vec![Control::Host("merge")]);
}

#[test]
fn running_a_host_command_returns_its_name() {
    let mut surface = surface();

    let control = surface.run(Command::Host("approve")).expect("the fixtures highlight");

    assert_eq!(control, Control::Host("approve"));
}

#[test]
fn a_host_key_works_on_a_binary_file_and_while_peeking() {
    let mut surface = opened(glean()).expect("the host bindings are free");
    surface
        .refresh(vec![FileChange {
            status: Status::Modified,
            path: PathBuf::from("logo.png"),
            previous: None,
            generated: false,
            content: Content::Binary,
        }])
        .expect("the fixtures highlight");
    screen(&mut surface, 100, 6);
    let on_binary = typed(&mut surface, &[press(KeyCode::Char('c'))]);

    let unchanged = Content::Text(FileDiff::compute(Source::new("a\n"), Source::new("a\n")));
    surface.peek(PathBuf::from("src/git.rs"), &unchanged, None).expect("the file highlights");
    screen(&mut surface, 100, 6);
    let while_peeking = typed(&mut surface, &[press(KeyCode::Char('c'))]);

    assert_eq!(on_binary, vec![Control::Host("comment")]);
    assert_eq!(while_peeking, vec![Control::Host("comment")]);
    assert_eq!(surface.peeked(), Some(PathBuf::from("src/git.rs").as_path()));
}

#[test]
fn a_host_shortcut_bound_by_sift_is_refused() {
    let error = refused(vec![host("reply", "Reply to this thread", 'r', REVIEW, None)]);

    assert!(matches!(
        error,
        command::Error::SharedShortcut {
            first: "Reload from git",
            second: "Reply to this thread",
            ..
        }
    ));
    assert_eq!(error.to_string(), "r is bound to both Reload from git and Reply to this thread");
}

#[test]
fn two_host_bindings_with_one_shortcut_are_refused() {
    let error = refused(vec![
        host("comment", "Comment on this line", 'c', REVIEW, None),
        host("close", "Close the pull request", 'c', PULL_REQUEST, None),
    ]);

    assert!(matches!(
        error,
        command::Error::SharedShortcut {
            first: "Comment on this line",
            second: "Close the pull request",
            ..
        }
    ));
}

#[test]
fn two_host_bindings_with_one_name_are_refused() {
    let error = refused(vec![
        host("comment", "Comment on this line", 'c', REVIEW, None),
        host("comment", "Comment on the file", 'C', REVIEW, None),
    ]);

    assert!(matches!(error, command::Error::SharedName("comment")));
}

#[test]
fn a_host_binding_that_reuses_a_sift_command_is_refused() {
    let error = refused(vec![Binding {
        command: Command::Reload,
        label: "Refetch the pull request",
        shortcuts: vec![key('F')],
        category: REVIEW,
        hint: None,
    }]);

    assert!(matches!(error, command::Error::NotHost("Refetch the pull request")));
}

#[test]
fn a_host_binding_in_one_of_sifts_sections_is_refused() {
    let error = refused(vec![host("comment", "Comment on this line", 'c', Category::Diff, None)]);

    assert!(matches!(error, command::Error::NotHost("Comment on this line")));
}

#[test]
fn a_character_shortcut_written_with_shift_is_refused() {
    let error = refused(vec![Binding {
        shortcuts: vec![Shortcut::new(KeyCode::Char('r'), KeyModifiers::SHIFT)],
        ..host("reply", "Reply to this thread", 'R', REVIEW, None)
    }]);

    assert!(matches!(error, command::Error::Unreachable { label: "Reply to this thread", .. }));
}

#[test]
fn a_host_key_on_an_annotation_lets_the_host_read_that_annotation() {
    let mut surface = surface();
    surface
        .annotate(vec![Annotation {
            id: "thread-7".to_owned(),
            anchor: Anchor { path: PathBuf::from("src/total.rs"), side: Side::New, line: 1 },
            heading: "alice".to_owned(),
            body: "Why 42?".to_owned(),
            faded: false,
            collapsed: false,
        }])
        .expect("the fixtures highlight");
    screen(&mut surface, 100, 8);
    surface.run(Command::NextAnnotation).expect("the fixtures highlight");
    screen(&mut surface, 100, 8);

    let controls = typed(&mut surface, &[press(KeyCode::Char('R'))]);

    assert_eq!(controls, vec![Control::Host("reply")]);
    assert_eq!(surface.annotation().map(|annotation| annotation.id.as_str()), Some("thread-7"));
}

#[test]
fn a_host_key_on_a_line_lets_the_host_read_its_anchor() {
    let mut surface = surface();
    screen(&mut surface, 100, 6);
    typed(&mut surface, &[press(KeyCode::Char('j'))]);

    let controls = typed(&mut surface, &[press(KeyCode::Char('c'))]);

    assert_eq!(controls, vec![Control::Host("comment")]);
    assert_eq!(
        surface.cursor(),
        Some(Anchor { path: PathBuf::from("src/other.rs"), side: Side::New, line: 2 })
    );
}

#[test]
fn help_lists_host_sections_after_sifts_own() {
    let mut surface = surface();
    typed(&mut surface, &[press(KeyCode::Char('?'))]);

    insta::assert_snapshot!(screen(&mut surface, 70, 52));
}

#[test]
fn host_hints_come_first_in_the_status_line_and_help_is_never_dropped() {
    let mut surface = surface();

    insta::assert_snapshot!(format!(
        "{}{}",
        screen(&mut surface, 200, 2),
        screen(&mut surface, 80, 2)
    ));
}

#[test]
fn an_unbound_key_does_nothing() {
    let mut surface = without_edit();

    let controls = typed(&mut surface, &[press(KeyCode::Char('e'))]);

    assert_eq!(controls, vec![Control::Continue]);
}

#[test]
fn an_unbound_command_leaves_the_status_line() {
    let mut surface = without_edit();

    insta::assert_snapshot!(screen(&mut surface, 200, 2));
}

#[test]
fn an_unbound_command_leaves_help() {
    let mut surface = without_edit();
    typed(&mut surface, &[press(KeyCode::Char('?'))]);

    insta::assert_snapshot!(screen(&mut surface, 70, 52));
}

#[test]
fn an_unbound_command_leaves_the_palette() {
    let mut surface = without_edit();
    typed(&mut surface, &[press(KeyCode::Char(':'))]);
    for character in "editor".chars() {
        typed(&mut surface, &[press(KeyCode::Char(character))]);
    }

    insta::assert_snapshot!(screen(&mut surface, 100, 8));
}

#[test]
fn a_host_can_bind_a_key_once_its_command_is_unbound() {
    let mut bindings = glean();
    bindings.push(host("browse", "Open in the browser", 'e', PULL_REQUEST, None));
    let mut surface = unbinding(vec![Command::Edit], bindings).expect("e is free");

    let controls = typed(&mut surface, &[press(KeyCode::Char('e'))]);

    assert_eq!(controls, vec![Control::Host("browse")]);
}

#[test]
fn a_host_key_held_by_a_command_still_bound_is_refused() {
    let error = refused(vec![host("browse", "Open in the browser", 'e', PULL_REQUEST, None)]);

    assert!(matches!(
        error,
        command::Error::SharedShortcut {
            first: "Open in $EDITOR",
            second: "Open in the browser",
            ..
        }
    ));
}

#[test]
fn unbinding_quit_is_refused() {
    let error = refused_unbinding(vec![Command::Quit]);

    assert!(matches!(error, command::Error::Required("Quit")));
    assert_eq!(error.to_string(), "Quit cannot be unbound");
}

#[test]
fn unbinding_help_is_refused() {
    let error = refused_unbinding(vec![Command::Edit, Command::Help]);

    assert!(matches!(error, command::Error::Required("Show every key")));
}

#[test]
fn unbinding_a_host_command_is_refused() {
    let error = refused_unbinding(vec![Command::Host("comment")]);

    assert!(matches!(error, command::Error::NotSift("comment")));
}
