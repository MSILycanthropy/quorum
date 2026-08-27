//! That the project navigator preserves hierarchy, attention, and identity.

use std::fmt::Write;

use quorum::theme::TokyoNight;
use quorum::ui::{
    Navigator, NavigatorSnapshot, ProjectId, ProjectPresentation, WorkspaceId,
    WorkspacePresentation, WorkspaceState,
};
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::buffer::Buffer;
use ratatui::style::Color;

fn workspace(
    id: &str,
    label: &str,
    state: WorkspaceState,
    changed_files: usize,
) -> WorkspacePresentation {
    WorkspacePresentation {
        state,
        changed_files,
        ..WorkspacePresentation::new(workspace_id(id), label, 0)
    }
}

fn project(id: &str, name: &str, workspaces: Vec<WorkspacePresentation>) -> ProjectPresentation {
    ProjectPresentation { id: project_id(id), name: name.to_owned(), order: 0, workspaces }
}

fn project_id(value: &str) -> ProjectId {
    ProjectId::new(value).expect("test project identity is valid")
}

fn workspace_id(value: &str) -> WorkspaceId {
    WorkspaceId::new(value).expect("test workspace identity is valid")
}

fn shown(navigator: &mut Navigator, width: u16, height: u16) -> String {
    let mut terminal = Terminal::new(TestBackend::new(width, height)).expect("test backend starts");
    terminal
        .draw(|frame| {
            let area = frame.area();
            navigator.draw(frame.buffer_mut(), area);
        })
        .expect("draw succeeds");
    describe(terminal.backend().buffer())
}

fn describe(buffer: &Buffer) -> String {
    let area = buffer.area();
    let mut output = String::new();
    for y in 0..area.height {
        let mut line = String::new();
        for x in 0..area.width {
            line.push_str(buffer[(x, y)].symbol());
        }
        writeln!(output, "{}", line.trim_end()).expect("writing to a String cannot fail");
    }
    output
}

fn navigator() -> Navigator {
    Navigator::new(NavigatorSnapshot::new(vec![
        project(
            "quorum",
            "quorum",
            vec![
                workspace("auth", "auth-fix", WorkspaceState::Working, 3),
                workspace("dashboard", "dashboard", WorkspaceState::Blocked, 0),
                workspace("perf", "perf-pass", WorkspaceState::Finished { visited: false }, 12),
                workspace("reviewed", "reviewed", WorkspaceState::Finished { visited: true }, 1),
                workspace("main", "main", WorkspaceState::Idle, 4),
            ],
        ),
        project(
            "kicksite",
            "kicksite-web",
            vec![workspace("comms", "COMMS-434", WorkspaceState::Idle, 5)],
        ),
    ]))
}

#[test]
fn all_workspace_states_projects_and_file_counts_have_stable_rows() {
    insta::assert_snapshot!(shown(&mut navigator(), 32, 10));
}

#[test]
fn explicit_project_and_workspace_orders_control_the_rendered_hierarchy() {
    let mut later_workspace =
        WorkspacePresentation::new(workspace_id("later-workspace"), "later", 1);
    later_workspace.state = WorkspaceState::Working;
    let earlier_workspace =
        WorkspacePresentation::new(workspace_id("earlier-workspace"), "earlier", 0);
    let snapshot = NavigatorSnapshot::new(vec![
        ProjectPresentation::new(
            project_id("later-project"),
            "later project",
            1,
            vec![later_workspace],
        ),
        ProjectPresentation::new(
            project_id("earlier-project"),
            "earlier project",
            0,
            vec![earlier_workspace],
        ),
    ]);
    let mut navigator = Navigator::new(snapshot);

    insta::assert_snapshot!(
        "explicit_project_and_workspace_orders_control_the_rendered_hierarchy",
        shown(&mut navigator, 24, 5)
    );
}

#[test]
fn collapsing_a_project_keeps_a_blocked_descendant_loud() {
    let mut navigator = navigator();
    navigator.toggle_project(&project_id("quorum"));

    insta::assert_snapshot!(shown(&mut navigator, 32, 6));
}

#[test]
fn unavailable_worktrees_and_main_workspaces_remain_visible() {
    let mut main = WorkspacePresentation::new(workspace_id("main"), "main", 0);
    main.main = true;
    main.unavailable = true;
    let mut navigator =
        Navigator::new(NavigatorSnapshot::new(vec![project("project", "project", vec![main])]));

    insta::assert_snapshot!(shown(&mut navigator, 32, 4));
}

#[test]
fn long_names_truncate_inside_narrow_areas() {
    let mut navigator = Navigator::new(NavigatorSnapshot::new(vec![project(
        "project",
        "a-project-with-a-long-name",
        vec![workspace("workspace", "a-workspace-with-a-long-name", WorkspaceState::Working, 7)],
    )]));

    insta::assert_snapshot!(shown(&mut navigator, 15, 4));
}

#[test]
fn selection_moves_through_visible_workspaces_and_scrolls_into_view() {
    let mut navigator = navigator();
    for _ in 0..5 {
        navigator.next_workspace();
    }

    assert_eq!(navigator.selected(), Some(&workspace_id("comms")));
    insta::assert_snapshot!(shown(&mut navigator, 32, 4));
    assert!(navigator.scroll_position() > 0);
}

#[test]
fn workspace_movement_cycles_at_both_ends() {
    let mut navigator = navigator();

    assert_eq!(navigator.previous_workspace(), Some(&workspace_id("comms")));
    assert_eq!(navigator.next_workspace(), Some(&workspace_id("auth")));
}

#[test]
fn selecting_a_workspace_expands_its_project() {
    let mut navigator = navigator();
    let project = project_id("kicksite");
    navigator.toggle_project(&project);

    assert!(navigator.collapsed().contains(&project));
    assert!(navigator.select_workspace(&workspace_id("comms")));
    assert!(!navigator.collapsed().contains(&project));
    assert_eq!(navigator.selected(), Some(&workspace_id("comms")));
}

#[test]
fn selection_and_collapse_survive_insertions_and_updates_by_identity() {
    let mut navigator = navigator();
    navigator.select_workspace(&workspace_id("perf"));
    navigator.toggle_project(&project_id("kicksite"));
    navigator.set_snapshot(NavigatorSnapshot::new(vec![
        project(
            "quorum",
            "renamed quorum",
            vec![
                workspace("inserted", "inserted", WorkspaceState::Idle, 0),
                workspace("auth", "auth-fix", WorkspaceState::Finished { visited: true }, 1),
                workspace("perf", "updated perf", WorkspaceState::Working, 8),
            ],
        ),
        project("kicksite", "kicksite-web", vec![]),
    ]));

    assert_eq!(navigator.selected(), Some(&workspace_id("perf")));
    assert!(navigator.collapsed().contains(&project_id("kicksite")));
}

#[test]
fn selection_falls_back_when_the_selected_workspace_disappears() {
    let mut navigator = navigator();
    navigator.select_workspace(&workspace_id("dashboard"));
    navigator.set_snapshot(NavigatorSnapshot::new(vec![project(
        "quorum",
        "quorum",
        vec![workspace("auth", "auth-fix", WorkspaceState::Idle, 0)],
    )]));

    assert_eq!(navigator.selected(), Some(&workspace_id("auth")));
}

#[test]
fn empty_navigator_and_empty_projects_have_deliberate_output() {
    let mut navigator =
        Navigator::new(NavigatorSnapshot::new(vec![project("empty", "empty", vec![])]));
    insta::assert_snapshot!("empty_project", shown(&mut navigator, 24, 4));

    let mut navigator = Navigator::new(NavigatorSnapshot::default());
    insta::assert_snapshot!("empty_navigator", shown(&mut navigator, 24, 3));
}

#[test]
fn selecting_and_moving_in_an_empty_navigator_is_safe() {
    let mut empty_navigator = Navigator::default();

    assert!(!empty_navigator.select_workspace(&workspace_id("missing")));
    assert_eq!(empty_navigator.next_workspace(), None);
    assert_eq!(empty_navigator.previous_workspace(), None);
    assert!(!empty_navigator.toggle_project(&project_id("missing")));
    assert_eq!(shown(&mut empty_navigator, 0, 0), "");
    assert_eq!(shown(&mut empty_navigator, 1, 1), "P\n");
    assert!(!shown(&mut navigator(), 1, 4).is_empty());
}

#[test]
fn selection_survives_a_changed_attention_state() {
    let mut navigator = navigator();
    navigator.select_workspace(&workspace_id("auth"));
    navigator.set_snapshot(NavigatorSnapshot::new(vec![project(
        "quorum",
        "quorum",
        vec![workspace("auth", "auth-fix", WorkspaceState::Blocked, 10)],
    )]));

    assert_eq!(navigator.selected(), Some(&workspace_id("auth")));
}

#[test]
fn changing_attention_does_not_reorder_workspaces() {
    let mut navigator = navigator();
    let before = workspace_labels(&shown(&mut navigator, 32, 10));
    let mut snapshot = navigator.snapshot().clone();
    snapshot.projects[0].workspaces[0].state = WorkspaceState::Blocked;
    snapshot.projects[0].workspaces[1].state = WorkspaceState::Working;
    snapshot.projects[0].workspaces[2].state = WorkspaceState::Finished { visited: true };
    navigator.set_snapshot(snapshot);

    assert_eq!(workspace_labels(&shown(&mut navigator, 32, 10)), before);
}

#[test]
fn finished_workspaces_dim_after_they_have_been_visited() {
    let mut navigator = Navigator::new(NavigatorSnapshot::new(vec![project(
        "project",
        "project",
        vec![workspace("done", "done", WorkspaceState::Finished { visited: true }, 0)],
    )]));
    let mut terminal = Terminal::new(TestBackend::new(24, 3)).expect("test backend starts");
    terminal
        .draw(|frame| {
            let area = frame.area();
            navigator.draw(frame.buffer_mut(), area);
        })
        .expect("draw succeeds");

    assert_eq!(
        terminal.backend().buffer()[(2, 2)].fg,
        Color::Rgb(
            TokyoNight::TINTS.muted.red,
            TokyoNight::TINTS.muted.green,
            TokyoNight::TINTS.muted.blue
        )
    );
}

fn workspace_labels(output: &str) -> Vec<String> {
    output
        .lines()
        .filter(|line| line.starts_with("  "))
        .filter_map(|line| line.split_whitespace().nth(1))
        .map(str::to_owned)
        .collect()
}
