//! The palette handles its complete open, filter, navigation and dispatch loop.

use quorum::command::{ApplicationCommand, PaletteEntry, PaletteTarget, ProjectTarget};
use quorum::ui::{Palette, PaletteAction};
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

fn key(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, KeyModifiers::NONE)
}

#[test]
fn palette_filters_moves_selection_and_executes_the_selected_target() {
    let target = PaletteTarget::Project(ProjectTarget::new(
        quorum::project::ProjectId::new("project").expect("test identity is valid"),
        "Project",
    ));
    let entries = vec![
        PaletteEntry::new(PaletteTarget::ApplicationCommand(ApplicationCommand::Quit), "Quit"),
        PaletteEntry::new(target.clone(), "Project"),
    ];
    let mut palette = Palette::default();
    palette.open(entries);
    assert!(palette.is_open());
    assert_eq!(palette.key(key(KeyCode::Char('p'))), PaletteAction::None);
    assert_eq!(palette.selected().map(PaletteEntry::label), Some("Project"));
    assert_eq!(palette.key(key(KeyCode::Enter)), PaletteAction::Execute(target));
    assert!(!palette.is_open());
}

#[test]
fn escape_closes_the_palette_without_executing_a_target() {
    let mut palette = Palette::default();
    palette.open(vec![PaletteEntry::new(
        PaletteTarget::ApplicationCommand(ApplicationCommand::Quit),
        "Quit",
    )]);
    assert_eq!(palette.key(key(KeyCode::Esc)), PaletteAction::Close);
    assert!(!palette.is_open());
}
