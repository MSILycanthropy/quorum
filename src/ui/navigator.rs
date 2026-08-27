//! The project and workspace navigator, independent of the application shell.

use std::collections::HashSet;

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

use crate::color::Rgb;
pub use crate::project::{ProjectId, WorkspaceId};
use crate::theme::{Tints, TokyoNight};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum WorkspaceState {
    Working,
    Blocked,
    Finished {
        visited: bool,
    },
    #[default]
    Idle,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct WorkspacePresentation {
    pub id: WorkspaceId,
    pub label: String,
    pub order: usize,
    pub state: WorkspaceState,
    pub changed_files: usize,
    pub main: bool,
    pub unavailable: bool,
}

impl WorkspacePresentation {
    pub fn new(id: WorkspaceId, label: impl Into<String>, order: usize) -> Self {
        Self {
            id,
            label: label.into(),
            order,
            state: WorkspaceState::default(),
            changed_files: 0,
            main: false,
            unavailable: false,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct ProjectPresentation {
    pub id: ProjectId,
    pub name: String,
    pub order: usize,
    pub workspaces: Vec<WorkspacePresentation>,
}

impl ProjectPresentation {
    pub fn new(
        id: ProjectId,
        name: impl Into<String>,
        order: usize,
        workspaces: Vec<WorkspacePresentation>,
    ) -> Self {
        Self { id, name: name.into(), order, workspaces }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct NavigatorSnapshot {
    pub projects: Vec<ProjectPresentation>,
}

impl NavigatorSnapshot {
    pub fn new(projects: Vec<ProjectPresentation>) -> Self {
        Self { projects }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Navigator {
    snapshot: NavigatorSnapshot,
    selected: Option<WorkspaceId>,
    collapsed: HashSet<ProjectId>,
    scroll: usize,
    tints: Tints,
}

impl Default for Navigator {
    fn default() -> Self {
        Self::new(NavigatorSnapshot::default())
    }
}

impl Navigator {
    pub fn new(snapshot: NavigatorSnapshot) -> Self {
        let mut navigator = Self {
            snapshot,
            selected: None,
            collapsed: HashSet::new(),
            scroll: 0,
            tints: TokyoNight::TINTS,
        };
        navigator.reconcile_selection();
        navigator
    }

    pub fn snapshot(&self) -> &NavigatorSnapshot {
        &self.snapshot
    }

    pub fn selected(&self) -> Option<&WorkspaceId> {
        self.selected.as_ref()
    }

    pub fn collapsed(&self) -> &HashSet<ProjectId> {
        &self.collapsed
    }

    pub fn scroll_position(&self) -> usize {
        self.scroll
    }

    pub fn set_snapshot(&mut self, snapshot: NavigatorSnapshot) {
        self.snapshot = snapshot;
        self.reconcile_selection();
    }

    pub fn select_workspace(&mut self, id: &WorkspaceId) -> bool {
        let project = self.snapshot.projects.iter().find_map(|project| {
            project
                .workspaces
                .iter()
                .any(|workspace| workspace.id == *id)
                .then(|| project.id.clone())
        });
        let Some(project) = project else {
            return false;
        };

        self.collapsed.remove(&project);
        self.selected = Some(id.clone());
        true
    }

    pub fn toggle_project(&mut self, id: &ProjectId) -> bool {
        if self.project(id).is_none() {
            return false;
        }

        if !self.collapsed.remove(id) {
            self.collapsed.insert(id.clone());
        }
        true
    }

    pub fn next_workspace(&mut self) -> Option<&WorkspaceId> {
        self.move_workspace(1)
    }

    pub fn previous_workspace(&mut self) -> Option<&WorkspaceId> {
        self.move_workspace(-1)
    }

    pub fn scroll_by(&mut self, rows: isize) {
        self.scroll = self.scroll.saturating_add_signed(rows);
    }

    pub fn workspace_at(&self, area: Rect, row: u16) -> Option<WorkspaceId> {
        if row <= area.y {
            return None;
        }
        let index = self.scroll + usize::from(row - area.y - 1);
        match self.rows().get(index) {
            Some(VisibleRow::Workspace { workspace }) => Some(workspace.id.clone()),
            Some(VisibleRow::Project { .. } | VisibleRow::EmptyProject) | None => None,
        }
    }

    pub fn project_at(&self, area: Rect, row: u16) -> Option<ProjectId> {
        if row <= area.y {
            return None;
        }
        let index = self.scroll + usize::from(row - area.y - 1);
        match self.rows().get(index) {
            Some(VisibleRow::Project { id, .. }) => Some(id.clone()),
            Some(VisibleRow::Workspace { .. } | VisibleRow::EmptyProject) | None => None,
        }
    }

    pub fn draw(&mut self, buffer: &mut Buffer, area: Rect) {
        self.render(buffer, area);
    }

    pub fn render(&mut self, buffer: &mut Buffer, area: Rect) {
        if area.width == 0 || area.height == 0 {
            return;
        }

        let background = Style::new().bg(color(self.tints.background));
        buffer.set_style(area, background);

        let rows = self.rows();
        let content_height = usize::from(area.height.saturating_sub(1));
        self.keep_selected_visible(&rows, content_height);

        let heading = Style::new()
            .fg(color(self.tints.foreground))
            .bg(color(self.tints.background))
            .add_modifier(Modifier::BOLD);
        write_text(buffer, area.x, area.y, area.width, "PROJECTS", heading);

        if rows.is_empty() {
            let empty = Style::new().fg(color(self.tints.muted)).bg(color(self.tints.background));
            if area.height > 1 {
                write_text(
                    buffer,
                    area.x,
                    area.y.saturating_add(1),
                    area.width,
                    "  (no projects)",
                    empty,
                );
            }
            return;
        }

        for (offset, row) in rows.iter().skip(self.scroll).take(content_height).enumerate() {
            let y =
                area.y.saturating_add(1).saturating_add(u16::try_from(offset).unwrap_or(u16::MAX));
            if y >= area.y.saturating_add(area.height) {
                break;
            }
            self.draw_row(buffer, area, y, row);
        }
    }

    fn draw_row(&self, buffer: &mut Buffer, area: Rect, y: u16, row: &VisibleRow) {
        match row {
            VisibleRow::Project { name, open, blocked, .. } => {
                self.draw_project(buffer, area, y, name, *open, *blocked);
            }
            VisibleRow::EmptyProject => self.draw_empty_project(buffer, area, y),
            VisibleRow::Workspace { workspace } => self.draw_workspace(buffer, area, y, workspace),
        }
    }

    fn draw_project(
        &self,
        buffer: &mut Buffer,
        area: Rect,
        y: u16,
        name: &str,
        open: bool,
        blocked: bool,
    ) {
        let normal = Style::new().fg(color(self.tints.foreground)).bg(color(self.tints.background));
        let marker = if open { "▾ " } else { "▸ " };
        let suffix = if blocked { " !" } else { "" };
        write_text(buffer, area.x, y, area.width, marker, normal);
        let marker_width = marker.width();
        let suffix_width = suffix.width();
        let remaining = usize::from(area.width).saturating_sub(marker_width + suffix_width);
        let name = truncate(name, remaining);
        write_text(
            buffer,
            area.x + u16::try_from(marker_width).unwrap_or(u16::MAX),
            y,
            u16::try_from(remaining).unwrap_or(u16::MAX),
            &name,
            normal,
        );
        if blocked {
            let suffix_width = u16::try_from(suffix_width).unwrap_or(u16::MAX).min(area.width);
            let x = area.x + area.width.saturating_sub(suffix_width);
            let suffix =
                if area.width == 1 { "!" } else { &truncate(suffix, usize::from(suffix_width)) };
            write_text(
                buffer,
                x,
                y,
                suffix_width,
                suffix,
                Style::new().fg(color(self.tints.attention)).bg(color(self.tints.background)),
            );
        }
    }

    fn draw_empty_project(&self, buffer: &mut Buffer, area: Rect, y: u16) {
        let style = Style::new().fg(color(self.tints.muted)).bg(color(self.tints.background));
        write_text(buffer, area.x, y, area.width, "  (empty project)", style);
    }

    fn draw_workspace(
        &self,
        buffer: &mut Buffer,
        area: Rect,
        y: u16,
        workspace: &WorkspacePresentation,
    ) {
        let selected = self.selected.as_ref() == Some(&workspace.id);
        let background = if selected { self.tints.filler } else { self.tints.background };
        let row_style = Style::new().bg(color(background));
        let state_style = match workspace.state {
            WorkspaceState::Blocked => {
                row_style.fg(color(self.tints.attention)).add_modifier(Modifier::BOLD)
            }
            WorkspaceState::Finished { visited: true } | WorkspaceState::Idle => {
                row_style.fg(color(self.tints.muted))
            }
            WorkspaceState::Working | WorkspaceState::Finished { visited: false } => {
                row_style.fg(color(self.tints.foreground))
            }
        };
        let label_style = match workspace.state {
            WorkspaceState::Finished { visited: true } | WorkspaceState::Idle => {
                row_style.fg(color(self.tints.muted))
            }
            WorkspaceState::Working
            | WorkspaceState::Blocked
            | WorkspaceState::Finished { visited: false } => {
                row_style.fg(color(self.tints.foreground))
            }
        };
        buffer.set_style(Rect::new(area.x, y, area.width, 1), row_style);

        let glyph = workspace.state.glyph();
        if area.width >= 3 {
            write_text(buffer, area.x, y, area.width, "  ", row_style);
            write_text(buffer, area.x + 2, y, area.width.saturating_sub(2), glyph, state_style);
        } else {
            write_text(buffer, area.x, y, area.width, glyph, state_style);
        }

        let count = if workspace.changed_files == 0 {
            String::new()
        } else {
            format!(" {}", workspace.changed_files)
        };
        let unavailable = if workspace.unavailable { " (unavailable)" } else { "" };
        let suffix = format!("{count}{unavailable}");
        let prefix_width = 4usize;
        let remaining = usize::from(area.width).saturating_sub(prefix_width + suffix.width());
        let label = truncate(&workspace.label, remaining);
        write_text(buffer, area.x + 4, y, area.width.saturating_sub(4), &label, label_style);
        let suffix_width = u16::try_from(suffix.width()).unwrap_or(u16::MAX).min(area.width);
        let suffix_x = area.x + area.width.saturating_sub(suffix_width);
        let suffix_style =
            if unavailable.is_empty() { row_style } else { row_style.fg(color(self.tints.muted)) };
        write_text(
            buffer,
            suffix_x,
            y,
            suffix_width,
            &truncate(&suffix, usize::from(suffix_width)),
            suffix_style,
        );
    }

    fn rows(&self) -> Vec<VisibleRow> {
        let mut rows = Vec::new();
        for project in self.ordered_projects() {
            let open = !self.collapsed.contains(&project.id);
            let blocked = project
                .workspaces
                .iter()
                .any(|workspace| workspace.state == WorkspaceState::Blocked);
            rows.push(VisibleRow::Project {
                id: project.id.clone(),
                name: project.name.clone(),
                open,
                blocked,
            });
            if open {
                rows.extend(
                    Self::ordered_workspaces(project)
                        .into_iter()
                        .cloned()
                        .map(|workspace| VisibleRow::Workspace { workspace }),
                );
                if project.workspaces.is_empty() {
                    rows.push(VisibleRow::EmptyProject);
                }
            }
        }
        rows
    }

    fn workspace(&self, id: &WorkspaceId) -> Option<&WorkspacePresentation> {
        self.snapshot
            .projects
            .iter()
            .flat_map(|project| &project.workspaces)
            .find(|workspace| workspace.id == *id)
    }

    fn project(&self, id: &ProjectId) -> Option<&ProjectPresentation> {
        self.snapshot.projects.iter().find(|project| project.id == *id)
    }

    fn ordered_projects(&self) -> Vec<&ProjectPresentation> {
        let mut projects: Vec<_> = self.snapshot.projects.iter().collect();
        projects.sort_by_key(|project| project.order);
        projects
    }

    fn ordered_workspaces(project: &ProjectPresentation) -> Vec<&WorkspacePresentation> {
        let mut workspaces: Vec<_> = project.workspaces.iter().collect();
        workspaces.sort_by_key(|workspace| workspace.order);
        workspaces
    }

    fn reconcile_selection(&mut self) {
        if self.selected.as_ref().is_some_and(|id| self.workspace(id).is_some()) {
            return;
        }
        self.selected = self
            .ordered_projects()
            .into_iter()
            .flat_map(Self::ordered_workspaces)
            .map(|workspace| workspace.id.clone())
            .next();
    }

    fn move_workspace(&mut self, direction: isize) -> Option<&WorkspaceId> {
        let visible: Vec<WorkspaceId> = self
            .rows()
            .into_iter()
            .filter_map(|row| match row {
                VisibleRow::Workspace { workspace } => Some(workspace.id),
                VisibleRow::Project { .. } | VisibleRow::EmptyProject => None,
            })
            .collect();
        if visible.is_empty() {
            self.selected = None;
            return None;
        }

        let current = self
            .selected
            .as_ref()
            .and_then(|selected| visible.iter().position(|id| id == selected));
        let next = match (current, direction) {
            (None, previous) if previous < 0 => visible.len() - 1,
            (None, _) => 0,
            (Some(0), previous) if previous < 0 => visible.len() - 1,
            (Some(index), previous) if previous < 0 => index - 1,
            (Some(index), _) => (index + 1) % visible.len(),
        };
        self.selected = Some(visible[next].clone());
        self.selected.as_ref()
    }

    fn keep_selected_visible(&mut self, rows: &[VisibleRow], content_height: usize) {
        if content_height == 0 {
            self.scroll = 0;
            return;
        }

        let Some(selected) = &self.selected else {
            self.scroll = self.scroll.min(rows.len().saturating_sub(content_height));
            return;
        };
        let Some(index) = rows.iter().position(|row| row.workspace_id() == Some(selected)) else {
            self.scroll = self.scroll.min(rows.len().saturating_sub(content_height));
            return;
        };

        if index < self.scroll {
            self.scroll = index;
        } else if index >= self.scroll + content_height {
            self.scroll = index + 1 - content_height;
        }
        self.scroll = self.scroll.min(rows.len().saturating_sub(content_height));
    }
}

impl ratatui::widgets::Widget for &mut Navigator {
    fn render(self, area: Rect, buffer: &mut Buffer) {
        Navigator::render(self, buffer, area);
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum VisibleRow {
    Project { id: ProjectId, name: String, open: bool, blocked: bool },
    EmptyProject,
    Workspace { workspace: WorkspacePresentation },
}

impl VisibleRow {
    fn workspace_id(&self) -> Option<&WorkspaceId> {
        match self {
            Self::Project { .. } | Self::EmptyProject => None,
            Self::Workspace { workspace } => Some(&workspace.id),
        }
    }
}

impl WorkspaceState {
    fn glyph(self) -> &'static str {
        match self {
            Self::Working => "●",
            Self::Blocked => "!",
            Self::Finished { .. } => "✓",
            Self::Idle => "○",
        }
    }
}

fn write_text(buffer: &mut Buffer, x: u16, y: u16, width: u16, text: &str, style: Style) {
    if width == 0 {
        return;
    }
    buffer.set_stringn(x, y, text, usize::from(width), style);
}

fn truncate(value: &str, width: usize) -> String {
    if value.width() <= width {
        return value.to_owned();
    }
    if width == 0 {
        return String::new();
    }
    if width == 1 {
        return "…".to_owned();
    }

    let mut result = String::new();
    let mut used = 0;
    for character in value.chars() {
        let character_width = character.width().unwrap_or(0);
        if used + character_width > width - 1 {
            break;
        }
        result.push(character);
        used += character_width;
    }
    result.push('…');
    result
}

fn color(rgb: Rgb) -> Color {
    Color::Rgb(rgb.red, rgb.green, rgb.blue)
}
