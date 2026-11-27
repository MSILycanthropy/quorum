//! That a host can give sift part of its screen and keep the rest for itself.

use std::fmt::Write;
use std::path::PathBuf;

use ratatui::buffer::Buffer;
use ratatui::crossterm::event::{
    KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};
use ratatui::layout::Rect;
use ratatui::widgets::Widget;
use sift::diff::{FileDiff, Source};
use sift::git::{Content, FileChange, Status};
use sift::ui::{Control, Opening, Surface};

const SCREEN: Rect = Rect { x: 0, y: 0, width: 100, height: 9 };
const GIVEN: Rect = Rect { x: 8, y: 1, width: 92, height: 8 };

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

fn long() -> Surface {
    let mut old = String::new();
    let mut new = String::new();
    for line in 1..=40 {
        writeln!(old, "line {line}").expect("writing to a String cannot fail");
        writeln!(new, "line {line} changed").expect("writing to a String cannot fail");
    }

    Opening::default()
        .surface(vec![modified("src/long.rs", &old, &new)])
        .expect("the fixtures highlight")
}

fn press(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, KeyModifiers::NONE)
}

fn at(kind: MouseEventKind, column: u16, row: u16) -> MouseEvent {
    MouseEvent { kind, column, row, modifiers: KeyModifiers::NONE }
}

fn hosted(surface: &mut Surface) -> String {
    let mut buffer = Buffer::empty(SCREEN);
    buffer.set_string(0, 0, "HOST BAR · pull request #42", ratatui::style::Style::new());
    for y in GIVEN.y..GIVEN.bottom() {
        buffer.set_string(0, y, "│ host │", ratatui::style::Style::new());
    }
    surface.render(GIVEN, &mut buffer);

    let mut out = String::new();
    for y in 0..buffer.area.height {
        let line: String =
            (0..buffer.area.width).map(|x| buffer[(x, y)].symbol().to_owned()).collect();
        writeln!(out, "{}", line.trim_end()).expect("writing to a String cannot fail");
    }

    out
}

#[test]
fn a_surface_drawn_beside_and_below_the_hosts_bars_leaves_them_alone() {
    insta::assert_snapshot!(hosted(&mut surface()));
}

#[test]
fn a_click_is_measured_from_where_the_surface_was_drawn() {
    let mut surface = surface();
    hosted(&mut surface);

    surface
        .mouse(at(MouseEventKind::Down(MouseButton::Left), GIVEN.x + 6, GIVEN.y + 3))
        .expect("the fixtures highlight");
    surface.key(press(KeyCode::Char('y'))).expect("the fixtures highlight");
    let controls = surface.key(press(KeyCode::Char('p'))).expect("the fixtures highlight");

    assert_eq!(controls, Control::Copy("src/total.rs".to_owned()));
}

#[test]
fn scrolling_over_the_hosts_bar_leaves_the_diff_where_it_was() {
    let mut surface = long();
    let before = hosted(&mut surface);

    surface.mouse(at(MouseEventKind::ScrollDown, 50, 0)).expect("the fixtures highlight");

    assert_eq!(hosted(&mut surface), before);
}

#[test]
fn scrolling_over_the_surface_still_scrolls_the_diff() {
    let mut surface = long();
    let before = hosted(&mut surface);

    surface.mouse(at(MouseEventKind::ScrollDown, 50, 4)).expect("the fixtures highlight");

    assert_ne!(hosted(&mut surface), before);
}

#[test]
fn the_palette_opens_over_the_surface_not_the_whole_screen() {
    let mut surface = surface();
    surface.key(press(KeyCode::Char('/'))).expect("the fixtures highlight");

    insta::assert_snapshot!(hosted(&mut surface));
}

#[test]
fn a_surface_one_row_tall_is_its_status_line() {
    let mut buffer = Buffer::empty(Rect::new(0, 0, 100, 3));
    surface().render(Rect::new(0, 1, 100, 1), &mut buffer);

    let line: String = (0..buffer.area.width).map(|x| buffer[(x, 1)].symbol().to_owned()).collect();
    insta::assert_snapshot!(line.trim_end());
}

#[test]
fn nothing_is_drawn_outside_the_area_however_small_it_is() {
    let screen = Rect::new(0, 0, 54, 9);
    for opener in [None, Some(':'), Some('/')] {
        for width in 0..=48 {
            for height in 0..=6 {
                let mut surface = surface();
                if let Some(key) = opener {
                    surface.key(press(KeyCode::Char(key))).expect("the fixtures highlight");
                }
                let given = Rect::new(3, 2, width, height);
                let mut buffer = Buffer::empty(screen);

                surface.render(given, &mut buffer);

                for position in screen.positions().filter(|position| !given.contains(*position)) {
                    assert_eq!(
                        buffer[position],
                        ratatui::buffer::Cell::default(),
                        "{opener:?} in {given:?} drew at {position:?}"
                    );
                }
            }
        }
    }
}
