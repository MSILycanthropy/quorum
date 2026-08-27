use std::collections::HashSet;
use std::path::PathBuf;

use quorum::command::{
    ApplicationCommand, ApplicationContext, CommandCatalog, FileTarget, InterruptionState,
    PaletteEntry, PaletteModel, PaletteTarget, ProjectTarget, SkillTarget, WorkspaceTarget,
};
use quorum::project::{ProjectId, WorkspaceId};

fn project_id(value: &str) -> ProjectId {
    ProjectId::new(value).expect("test project identity is valid")
}

fn workspace_id(value: &str) -> WorkspaceId {
    WorkspaceId::new(value).expect("test workspace identity is valid")
}

#[test]
fn every_catalog_command_has_a_unique_nonempty_label() {
    let catalog = CommandCatalog::new();
    let mut labels = catalog
        .commands()
        .iter()
        .map(quorum::command::CommandDefinition::label)
        .collect::<Vec<_>>();
    labels.sort_unstable();
    labels.dedup();

    assert_eq!(labels.len(), catalog.commands().len());
    assert!(catalog.commands().iter().all(|command| !command.label().is_empty()));
}

#[test]
fn globally_available_commands_do_not_claim_the_same_shortcut() {
    let catalog = CommandCatalog::new();
    let shortcuts = catalog
        .commands()
        .iter()
        .filter(|command| command.globally_available())
        .flat_map(quorum::command::CommandDefinition::shortcuts)
        .collect::<Vec<_>>();
    let unique = shortcuts.iter().collect::<HashSet<_>>().len();

    let count = catalog
        .commands()
        .iter()
        .filter(|command| command.globally_available())
        .flat_map(quorum::command::CommandDefinition::shortcuts)
        .count();
    assert_eq!(unique, count);
}

#[test]
fn shortcut_lookup_and_identity_lookup_agree() {
    let catalog = CommandCatalog::new();
    let context = ApplicationContext::default();

    for command in catalog.commands().iter().filter(|command| command.globally_available()) {
        for shortcut in command.shortcuts() {
            assert_eq!(catalog.command_for_shortcut(*shortcut, &context), Some(command.command()));
            assert_eq!(
                catalog
                    .definition(command.command())
                    .map(quorum::command::CommandDefinition::label),
                Some(command.label())
            );
        }
    }
}

#[test]
fn availability_uses_explicit_application_facts() {
    let catalog = CommandCatalog::new();
    let prompt = catalog.definition(ApplicationCommand::EnterPrompt).expect("prompt is catalogued");
    let mut context = ApplicationContext { active_workspace: true, ..Default::default() };
    assert!(!prompt.available(&context));

    context.attached_agent = true;
    assert!(prompt.available(&context));
}

#[test]
fn permission_shortcuts_require_an_explicit_interruption() {
    let catalog = CommandCatalog::new();
    let answer = catalog
        .definition(ApplicationCommand::AnswerPermission(1))
        .expect("permission answer is catalogued");
    let shortcut = answer.shortcuts()[0];

    assert!(!answer.globally_available());
    assert_eq!(catalog.command_for_shortcut(shortcut, &ApplicationContext::default()), None);

    let context =
        ApplicationContext { interruption: Some(InterruptionState::new(1)), ..Default::default() };
    assert_eq!(
        catalog.command_for_shortcut(shortcut, &context),
        Some(ApplicationCommand::AnswerPermission(1))
    );
}

#[test]
fn palette_ranks_exact_then_prefix_then_subsequence() {
    let entries = vec![
        PaletteEntry::new(
            PaletteTarget::Project(ProjectTarget::new(project_id("one"), "Open")),
            "Open",
        ),
        PaletteEntry::new(
            PaletteTarget::Project(ProjectTarget::new(project_id("two"), "Open changes")),
            "Open changes",
        ),
        PaletteEntry::new(
            PaletteTarget::Project(ProjectTarget::new(project_id("three"), "Drop event")),
            "Drop event",
        ),
    ];
    let ranked = PaletteModel.rank("open", &entries);

    assert_eq!(ranked[0].label(), "Open");
    assert_eq!(ranked[1].label(), "Open changes");
    assert_eq!(ranked[2].label(), "Drop event");
}

#[test]
fn palette_breaks_equal_scores_by_supplied_order() {
    let entries = vec![
        PaletteEntry::new(
            PaletteTarget::Project(ProjectTarget::new(project_id("first"), "Alpha")),
            "Alpha",
        ),
        PaletteEntry::new(
            PaletteTarget::Project(ProjectTarget::new(project_id("second"), "Alpine")),
            "Alpine",
        ),
    ];

    let ranked = PaletteModel.rank("al", &entries);

    assert_eq!(ranked[0].label(), "Alpha");
    assert_eq!(ranked[1].label(), "Alpine");
}

#[test]
fn an_empty_palette_query_preserves_supplied_order() {
    let entries = vec![
        PaletteEntry::new(
            PaletteTarget::Project(ProjectTarget::new(project_id("first"), "First")),
            "First",
        ),
        PaletteEntry::new(
            PaletteTarget::Project(ProjectTarget::new(project_id("second"), "Second")),
            "Second",
        ),
    ];

    let ranked = PaletteModel.rank("", &entries);

    assert_eq!(ranked, entries.iter().collect::<Vec<_>>());
}

#[test]
fn mixed_palette_entries_retain_typed_identity_and_ownership() {
    let entries = vec![
        PaletteEntry::new(
            PaletteTarget::Workspace(WorkspaceTarget::new(
                project_id("project"),
                workspace_id("workspace"),
                "Workspace",
            )),
            "Workspace",
        ),
        PaletteEntry::new(
            PaletteTarget::File(FileTarget::new(
                project_id("project"),
                workspace_id("workspace"),
                PathBuf::from("src/main.rs"),
            )),
            "src/main.rs",
        ),
        PaletteEntry::new(
            PaletteTarget::Skill(SkillTarget::new("quorum", "help", "quorum:help")),
            "quorum:help",
        ),
        PaletteEntry::new(
            PaletteTarget::ApplicationCommand(ApplicationCommand::OpenPalette),
            "Open palette",
        ),
    ];

    let ranked = PaletteModel.rank("", &entries);

    assert!(
        matches!(ranked[0].target(), PaletteTarget::Workspace(target) if target.project == project_id("project"))
    );
    assert!(
        matches!(ranked[1].target(), PaletteTarget::File(target) if target.workspace == workspace_id("workspace") && target.path == *"src/main.rs")
    );
    assert!(
        matches!(ranked[2].target(), PaletteTarget::Skill(target) if target.namespace == "quorum")
    );
    assert!(matches!(
        ranked[3].target(),
        PaletteTarget::ApplicationCommand(ApplicationCommand::OpenPalette)
    ));
}
