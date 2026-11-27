//! That the keys a vim user and an arrow-keys user each reach for do what they expect.

use std::fmt::Write;
use std::path::PathBuf;

use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use sift::diff::{FileDiff, Side, Source};
use sift::git::{Content, FileChange, Status};
use sift::ui::{Anchor, Control, Opening, Surface};

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

fn numbered(changed: &[usize]) -> FileChange {
    let (mut old, mut new) = (String::new(), String::new());
    for line in 1..=40 {
        writeln!(old, "line {line}").expect("writing to a String cannot fail");
        if changed.contains(&line) {
            writeln!(new, "line {line} changed").expect("writing to a String cannot fail");
        } else {
            writeln!(new, "line {line}").expect("writing to a String cannot fail");
        }
    }

    modified(PATH, &old, &new)
}

fn opened(changes: Vec<FileChange>) -> Surface {
    let mut surface = Opening::default().surface(changes).expect("the fixtures highlight");
    screen(&mut surface);

    surface
}

fn unfolded() -> Surface {
    let mut surface = opened(vec![numbered(&[10, 30])]);
    typed(&mut surface, &[plain("zR"), plain("gg")].concat());

    surface
}

fn screen(surface: &mut Surface) -> String {
    let mut terminal = Terminal::new(TestBackend::new(100, 12)).expect("test backend starts");
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

fn plain(keys: &str) -> Vec<KeyEvent> {
    keys.chars().map(|key| KeyEvent::new(KeyCode::Char(key), KeyModifiers::NONE)).collect()
}

fn with(code: KeyCode, modifiers: KeyModifiers) -> Vec<KeyEvent> {
    vec![KeyEvent::new(code, modifiers)]
}

fn typed(surface: &mut Surface, keys: &[KeyEvent]) -> Vec<Control> {
    keys.iter()
        .map(|key| {
            let control = surface.key(*key).expect("the fixtures highlight");
            screen(surface);
            control
        })
        .collect()
}

fn line(surface: &Surface) -> Option<usize> {
    surface.cursor().map(|anchor| anchor.line)
}

fn first_shown(surface: &mut Surface) -> String {
    let screen = screen(surface);
    let row = screen.lines().nth(1).unwrap_or_default();

    row.split_once('│').map_or(row, |(_, diff)| diff).trim().to_owned()
}

#[test]
fn a_count_repeats_a_move() {
    let mut surface = unfolded();
    typed(&mut surface, &plain("5j"));

    assert_eq!(line(&surface), Some(6));
}

#[test]
fn gg_and_g_with_a_count_go_to_that_line() {
    let mut surface = unfolded();
    typed(&mut surface, &plain("17G"));
    let after_capital = line(&surface);
    typed(&mut surface, &plain("4gg"));

    assert_eq!((after_capital, line(&surface)), (Some(17), Some(4)));
}

#[test]
fn g_on_its_own_waits_and_gg_goes_to_the_top() {
    let mut surface = unfolded();
    typed(&mut surface, &plain("Gg"));
    let waiting = line(&surface);
    typed(&mut surface, &plain("g"));

    assert_eq!((waiting, line(&surface)), (Some(41), Some(1)));
}

#[test]
fn ctrl_home_and_ctrl_end_go_to_either_end() {
    let mut surface = unfolded();
    typed(&mut surface, &with(KeyCode::End, KeyModifiers::CONTROL));
    let bottom = line(&surface);
    typed(&mut surface, &with(KeyCode::Home, KeyModifiers::CONTROL));

    assert_eq!((bottom, line(&surface)), (Some(41), Some(1)));
}

#[test]
fn yy_copies_the_code_on_the_cursors_line() {
    let mut surface = unfolded();
    let controls = typed(&mut surface, &plain("jyy"));

    assert_eq!(controls.last(), Some(&Control::Copy("line 2".to_owned())));
}

#[test]
fn y_in_a_selection_copies_the_code_on_every_line_selected() {
    let mut surface = unfolded();
    let controls = typed(&mut surface, &plain("Vjjy"));

    assert_eq!(controls.last(), Some(&Control::Copy("line 1\nline 2\nline 3".to_owned())));
}

#[test]
fn ctrl_c_copies_while_something_is_selected_and_quits_otherwise() {
    let mut surface = unfolded();
    typed(&mut surface, &plain("Vj"));
    let selected = typed(&mut surface, &with(KeyCode::Char('c'), KeyModifiers::CONTROL));
    typed(&mut surface, &with(KeyCode::Esc, KeyModifiers::NONE));
    let unselected = typed(&mut surface, &with(KeyCode::Char('c'), KeyModifiers::CONTROL));

    assert_eq!(selected, vec![Control::Copy("line 1\nline 2".to_owned())]);
    assert_eq!(unselected, vec![Control::Quit]);
}

#[test]
fn yl_and_yp_copy_where_the_cursor_is_and_the_path() {
    let mut surface = unfolded();
    let location = typed(&mut surface, &plain("jjyl"));
    let path = typed(&mut surface, &plain("yp"));

    assert_eq!(location.last(), Some(&Control::Copy(format!("{PATH}:3"))));
    assert_eq!(path.last(), Some(&Control::Copy(PATH.to_owned())));
}

#[test]
fn shift_down_selects_and_a_plain_move_lets_go() {
    let mut surface = unfolded();
    typed(
        &mut surface,
        &[with(KeyCode::Down, KeyModifiers::SHIFT), with(KeyCode::Down, KeyModifiers::SHIFT)]
            .concat(),
    );
    let selected = surface.selection().map(|selection| selection.lines);
    typed(&mut surface, &with(KeyCode::Down, KeyModifiers::NONE));

    assert_eq!(selected, Some(1..=3));
    assert_eq!(surface.selection().map(|selection| selection.lines), Some(4..=4));
}

#[test]
fn a_selection_begun_with_v_keeps_growing_with_any_move() {
    let mut surface = unfolded();
    typed(&mut surface, &plain("v"));
    typed(&mut surface, &[with(KeyCode::Down, KeyModifiers::NONE), plain("j")].concat());

    assert_eq!(surface.selection().map(|selection| selection.lines), Some(1..=3));
}

#[test]
fn zt_puts_the_cursors_line_at_the_top() {
    let mut surface = unfolded();
    typed(&mut surface, &plain("19jzt"));

    assert!(first_shown(&mut surface).starts_with("20 line 20"), "{}", screen(&mut surface));
}

#[test]
fn ctrl_e_scrolls_a_line_and_leaves_the_cursor_where_it_is() {
    let mut surface = unfolded();
    typed(&mut surface, &plain("3j"));
    typed(&mut surface, &with(KeyCode::Char('e'), KeyModifiers::CONTROL));

    assert!(first_shown(&mut surface).starts_with("2 line 2"), "{}", screen(&mut surface));
    assert_eq!(line(&surface), Some(4));
}

#[test]
fn zc_folds_an_opened_run_back_up() {
    let mut surface = opened(vec![numbered(&[10, 30])]);
    let folded = screen(&mut surface);
    typed(
        &mut surface,
        &[plain("gg"), with(KeyCode::Enter, KeyModifiers::NONE), plain("jj")].concat(),
    );
    let opened = screen(&mut surface);
    typed(&mut surface, &plain("zc"));

    assert_ne!(opened, folded);
    assert_eq!(screen(&mut surface), folded);
}

#[test]
fn zm_folds_everything_that_zr_opened() {
    let mut surface = opened(vec![numbered(&[10, 30])]);
    let folded = screen(&mut surface);
    typed(&mut surface, &plain("zR"));
    let unfolded = screen(&mut surface);
    typed(&mut surface, &plain("zM"));

    assert_ne!(unfolded, folded);
    assert_eq!(screen(&mut surface), folded);
}

#[test]
fn a_line_number_in_the_palette_goes_to_that_line() {
    let mut surface = unfolded();
    typed(&mut surface, &[plain(":23"), with(KeyCode::Enter, KeyModifiers::NONE)].concat());

    assert_eq!(
        surface.cursor(),
        Some(Anchor { path: PathBuf::from(PATH), side: Side::New, line: 23 })
    );
}

#[test]
fn ctrl_g_asks_for_a_line_to_go_to() {
    let mut surface = unfolded();
    typed(&mut surface, &with(KeyCode::Char('g'), KeyModifiers::CONTROL));
    typed(&mut surface, &[plain("31"), with(KeyCode::Enter, KeyModifiers::NONE)].concat());

    assert_eq!(line(&surface), Some(31));
}

#[test]
fn vims_ways_of_quitting_quit() {
    for keys in [
        plain("ZZ"),
        plain("ZQ"),
        [plain(":q!"), with(KeyCode::Enter, KeyModifiers::NONE)].concat(),
        [plain(":wq"), with(KeyCode::Enter, KeyModifiers::NONE)].concat(),
    ] {
        let mut surface = unfolded();

        assert_eq!(typed(&mut surface, &keys).last(), Some(&Control::Quit));
    }
}

#[test]
fn previous_unviewed_goes_back_to_the_unviewed_file_before() {
    let mut surface = opened(vec![
        modified("src/a.rs", "a\n", "b\n"),
        modified("src/b.rs", "a\n", "b\n"),
        modified("src/c.rs", "a\n", "b\n"),
    ]);
    typed(&mut surface, &with(KeyCode::Tab, KeyModifiers::NONE));
    typed(&mut surface, &with(KeyCode::Tab, KeyModifiers::NONE));
    let controls = typed(&mut surface, &[plain("[u"), plain("yp")].concat());

    assert_eq!(controls.last(), Some(&Control::Copy("src/b.rs".to_owned())));
}
