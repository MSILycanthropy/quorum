//! That the whole surface lays out in three columns above one status line.

use std::fmt::Write;
use std::path::PathBuf;

use quorum::agent::{Transcript, Update};
use quorum::diff::{FileDiff, Source};
use quorum::git::{Content, FileChange, Status};
use quorum::ui::{Opening, Prompt};
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::buffer::Buffer;
use ratatui::crossterm::event::{
    KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};

const OLD: &str = "fn total(cart: &Cart) -> u32 {\n    let total = 1;\n    total\n}\n";
const NEW: &str = "fn total(cart: &Cart) -> u32 {\n    let total = 42;\n    total\n}\n";

fn fixtures() -> Vec<FileChange> {
    vec![
        FileChange {
            status: Status::Modified,
            path: PathBuf::from("src/cart.rs"),
            content: Content::Text(FileDiff::compute(Source::new(OLD), Source::new(NEW))),
        },
        FileChange {
            status: Status::Added,
            path: PathBuf::from("src/ui/tab.rs"),
            content: Content::Text(FileDiff::compute(Source::new(""), Source::new(NEW))),
        },
    ]
}

fn at(kind: MouseEventKind, column: u16, row: u16) -> MouseEvent {
    MouseEvent { kind, column, row, modifiers: KeyModifiers::NONE }
}

fn shown(width: u16, height: u16, mouse: &[MouseEvent]) -> String {
    let opening = Opening {
        branch: Some("main".to_owned()),
        base: "abc1234def".to_owned(),
        ..Opening::default()
    };
    let mut tab = opening.tab(fixtures(), None).expect("the fixtures highlight");
    let mut terminal = Terminal::new(TestBackend::new(width, height)).expect("test backend starts");

    terminal.draw(|frame| tab.draw(frame)).expect("draw succeeds");
    for event in mouse {
        tab.mouse(*event).expect("the fixtures highlight");
        terminal.draw(|frame| tab.draw(frame)).expect("draw succeeds");
    }

    describe(terminal.backend().buffer())
}

fn describe(buffer: &Buffer) -> String {
    let area = buffer.area();
    let mut out = String::new();

    for y in 0..area.height {
        let mut text = String::new();
        for x in 0..area.width {
            text.push_str(buffer[(x, y)].symbol());
        }
        writeln!(out, "{}", text.trim_end()).expect("writing to a String cannot fail");
    }

    out
}

fn typed(keys: &[KeyEvent]) -> Prompt {
    let mut prompt = Prompt::default();
    prompt.enter();
    for key in keys {
        prompt.key(*key);
    }

    prompt
}

fn key(code: KeyCode, modifiers: KeyModifiers) -> KeyEvent {
    KeyEvent::new(code, modifiers)
}

#[test]
fn the_surface_is_three_columns_over_a_status_line() {
    insta::assert_snapshot!(shown(170, 14, &[]));
}

#[test]
fn dragging_the_conversation_divider_widens_the_agent_column() {
    insta::assert_snapshot!(shown(
        170,
        14,
        &[
            at(MouseEventKind::Down(MouseButton::Left), 114, 5),
            at(MouseEventKind::Drag(MouseButton::Left), 90, 5),
            at(MouseEventKind::Up(MouseButton::Left), 90, 5),
        ]
    ));
}

#[test]
fn shift_enter_adds_a_line_instead_of_sending() {
    let prompt = typed(&[
        key(KeyCode::Char('a'), KeyModifiers::NONE),
        key(KeyCode::Enter, KeyModifiers::SHIFT),
        key(KeyCode::Char('b'), KeyModifiers::NONE),
    ]);

    assert_eq!(prompt.height(44), 4);
    assert!(prompt.entered());
}

#[test]
fn ctrl_j_adds_a_line_where_the_terminal_cannot_report_shift_enter() {
    let prompt = typed(&[
        key(KeyCode::Char('a'), KeyModifiers::NONE),
        key(KeyCode::Char('j'), KeyModifiers::CONTROL),
        key(KeyCode::Char('b'), KeyModifiers::NONE),
    ]);

    assert_eq!(prompt.height(44), 4);
}

#[test]
fn a_bare_enter_sends_every_line_at_once() {
    let mut prompt = typed(&[
        key(KeyCode::Char('a'), KeyModifiers::NONE),
        key(KeyCode::Enter, KeyModifiers::SHIFT),
        key(KeyCode::Char('b'), KeyModifiers::NONE),
    ]);

    assert_eq!(prompt.key(key(KeyCode::Enter, KeyModifiers::NONE)), Some("a\nb".to_owned()));
    assert!(!prompt.entered());
    assert_eq!(prompt.height(44), 3);
}

#[test]
fn retained_workspace_surfaces_keep_their_transcripts_independent() {
    let mut first_transcript = Transcript::default();
    first_transcript.prompted("first".to_owned());
    let mut second_transcript = Transcript::default();
    second_transcript.apply(Update::Said("second".to_owned()));
    let first = Opening::default()
        .tab_with_transcript(Vec::new(), None, first_transcript)
        .expect("an empty surface can be opened");
    let second = Opening::default()
        .tab_with_transcript(Vec::new(), None, second_transcript)
        .expect("an empty surface can be opened");

    assert_eq!(first.transcript().entries().len(), 1);
    assert_eq!(second.transcript().entries().len(), 1);
    assert_ne!(first.transcript(), second.transcript());
}
