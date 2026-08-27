//! Quorum's application commands and the UI-independent palette model.

use std::path::PathBuf;

use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

/// The visible commands owned by Quorum, not commands reported by an agent.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ApplicationCommand {
    OpenPalette,
    Quit,
    NextProject,
    PreviousProject,
    ToggleProject,
    NextWorkspace,
    PreviousWorkspace,
    OpenSelectedFile,
    NextFile,
    PreviousFile,
    ToggleFileList,
    ToggleFileTreeNode,
    NextDiffLine,
    PreviousDiffLine,
    FirstDiff,
    LastDiff,
    NextDiffPage,
    PreviousDiffPage,
    ScrollDiffLeft,
    ScrollDiffRight,
    NextHunk,
    PreviousHunk,
    ToggleLayout,
    ToggleWrapping,
    EqualizeDividers,
    EnterPrompt,
    AnswerPermission(usize),
}

/// A key combination independent of key press/release reporting.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Shortcut {
    pub code: KeyCode,
    pub modifiers: KeyModifiers,
}

impl Shortcut {
    pub const fn new(code: KeyCode, modifiers: KeyModifiers) -> Self {
        Self { code, modifiers }
    }

    pub fn display(self) -> String {
        let key = match self.code {
            KeyCode::Char(character) if self.modifiers.is_empty() => character.to_string(),
            KeyCode::Char(character) => character.to_uppercase().to_string(),
            KeyCode::Backspace => "Backspace".to_owned(),
            KeyCode::Enter => "Enter".to_owned(),
            KeyCode::Left => "Left".to_owned(),
            KeyCode::Right => "Right".to_owned(),
            KeyCode::Up => "Up".to_owned(),
            KeyCode::Down => "Down".to_owned(),
            KeyCode::Home => "Home".to_owned(),
            KeyCode::End => "End".to_owned(),
            KeyCode::PageUp => "PageUp".to_owned(),
            KeyCode::PageDown => "PageDown".to_owned(),
            KeyCode::Tab => "Tab".to_owned(),
            KeyCode::BackTab => "BackTab".to_owned(),
            KeyCode::Delete => "Delete".to_owned(),
            KeyCode::Insert => "Insert".to_owned(),
            KeyCode::F(number) => format!("F{number}"),
            KeyCode::Esc => "Esc".to_owned(),
            code => format!("{code:?}"),
        };
        let modifiers = [
            (KeyModifiers::CONTROL, "Ctrl"),
            (KeyModifiers::ALT, "Alt"),
            (KeyModifiers::SHIFT, "Shift"),
            (KeyModifiers::SUPER, "Super"),
        ]
        .into_iter()
        .filter_map(|(modifier, name)| self.modifiers.contains(modifier).then_some(name))
        .collect::<Vec<_>>()
        .join("-");

        if modifiers.is_empty() { key } else { format!("{modifiers}-{key}") }
    }

    pub fn matches(self, key: KeyEvent) -> bool {
        self.code == key.code && self.modifiers == key.modifiers
    }
}

impl From<KeyEvent> for Shortcut {
    fn from(key: KeyEvent) -> Self {
        Self::new(key.code, key.modifiers)
    }
}

/// Explicit facts that determine whether an application command can run.
#[expect(
    clippy::struct_excessive_bools,
    reason = "these independent facts are the explicit application state used by availability"
)]
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct ApplicationContext {
    pub project_count: usize,
    pub workspace_count: usize,
    pub file_count: usize,
    pub active_project: bool,
    pub active_workspace: bool,
    pub selected_file: bool,
    pub attached_agent: bool,
    pub interruption: Option<InterruptionState>,
}

/// The interruption state in which permission choices are meaningful.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct InterruptionState {
    pub choice_count: usize,
}

impl InterruptionState {
    pub const fn new(choice_count: usize) -> Self {
        Self { choice_count }
    }
}

/// The target facts a command needs before it is available.
#[expect(
    clippy::struct_excessive_bools,
    reason = "each requirement names a separate target or capture fact"
)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct CommandRequirements {
    pub active_project: bool,
    pub active_workspace: bool,
    pub selected_file: bool,
    pub attached_agent: bool,
    pub interruption: bool,
    pub minimum_projects: usize,
    pub minimum_workspaces: usize,
    pub minimum_files: usize,
}

impl CommandRequirements {
    fn satisfied_by(self, command: ApplicationCommand, context: &ApplicationContext) -> bool {
        if self.active_project && !context.active_project
            || self.active_workspace && !context.active_workspace
            || self.selected_file && !context.selected_file
            || self.attached_agent && !context.attached_agent
            || self.minimum_projects > context.project_count
            || self.minimum_workspaces > context.workspace_count
            || self.minimum_files > context.file_count
        {
            return false;
        }

        let Some(interruption) = context.interruption else {
            return !self.interruption;
        };
        if !self.interruption {
            return true;
        }

        match command {
            ApplicationCommand::AnswerPermission(choice) => interruption.choice_count >= choice,
            _ => true,
        }
    }
}

/// One catalog entry, including the identity used for dispatch.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct CommandDefinition {
    command: ApplicationCommand,
    label: String,
    shortcuts: Vec<Shortcut>,
    category: CommandCategory,
    requirements: CommandRequirements,
}

impl CommandDefinition {
    pub fn command(&self) -> ApplicationCommand {
        self.command
    }

    pub fn label(&self) -> &str {
        &self.label
    }

    pub fn shortcuts(&self) -> &[Shortcut] {
        &self.shortcuts
    }

    pub fn category(&self) -> CommandCategory {
        self.category
    }

    pub fn requirements(&self) -> CommandRequirements {
        self.requirements
    }

    pub fn available(&self, context: &ApplicationContext) -> bool {
        self.requirements.satisfied_by(self.command, context)
    }

    pub fn globally_available(&self) -> bool {
        self.available(&ApplicationContext::default())
    }
}

/// The palette grouping for an application command.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum CommandCategory {
    Application,
    Projects,
    Workspaces,
    Files,
    Diff,
    Layout,
    Agent,
    Permission,
}

/// The complete catalog of application commands currently understood by Quorum.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct CommandCatalog {
    commands: Vec<CommandDefinition>,
}

impl CommandCatalog {
    #[expect(
        clippy::too_many_lines,
        reason = "the catalog stays as one ordered inventory so palette order is explicit"
    )]
    pub fn new() -> Self {
        let mut catalog = Self { commands: Vec::new() };
        catalog.add(
            ApplicationCommand::OpenPalette,
            "Open palette",
            &[ctrl('t')],
            CommandCategory::Application,
            CommandRequirements::default(),
        );
        catalog.add(
            ApplicationCommand::Quit,
            "Quit",
            &[plain('q'), plain_code(KeyCode::Esc), ctrl('c')],
            CommandCategory::Application,
            CommandRequirements::default(),
        );
        catalog.add(
            ApplicationCommand::NextProject,
            "Next project",
            &[],
            CommandCategory::Projects,
            CommandRequirements { minimum_projects: 2, ..Default::default() },
        );
        catalog.add(
            ApplicationCommand::PreviousProject,
            "Previous project",
            &[],
            CommandCategory::Projects,
            CommandRequirements { minimum_projects: 2, ..Default::default() },
        );
        catalog.add(
            ApplicationCommand::ToggleProject,
            "Toggle project",
            &[],
            CommandCategory::Projects,
            CommandRequirements { active_project: true, ..Default::default() },
        );
        catalog.add(
            ApplicationCommand::NextWorkspace,
            "Next workspace",
            &[],
            CommandCategory::Workspaces,
            CommandRequirements { minimum_workspaces: 2, ..Default::default() },
        );
        catalog.add(
            ApplicationCommand::PreviousWorkspace,
            "Previous workspace",
            &[],
            CommandCategory::Workspaces,
            CommandRequirements { minimum_workspaces: 2, ..Default::default() },
        );
        catalog.add(
            ApplicationCommand::OpenSelectedFile,
            "Open selected file",
            &[],
            CommandCategory::Files,
            CommandRequirements {
                active_workspace: true,
                selected_file: true,
                ..Default::default()
            },
        );
        catalog.add(
            ApplicationCommand::NextFile,
            "Next file",
            &[plain(']')],
            CommandCategory::Files,
            CommandRequirements { active_workspace: true, minimum_files: 2, ..Default::default() },
        );
        catalog.add(
            ApplicationCommand::PreviousFile,
            "Previous file",
            &[plain('[')],
            CommandCategory::Files,
            CommandRequirements { active_workspace: true, minimum_files: 2, ..Default::default() },
        );
        catalog.add(
            ApplicationCommand::ToggleFileList,
            "Toggle changed files",
            &[plain('F')],
            CommandCategory::Files,
            CommandRequirements { active_workspace: true, ..Default::default() },
        );
        catalog.add(
            ApplicationCommand::ToggleFileTreeNode,
            "Toggle file group",
            &[plain_code(KeyCode::Enter), plain_code(KeyCode::Char(' '))],
            CommandCategory::Files,
            CommandRequirements { active_workspace: true, ..Default::default() },
        );
        catalog.add(
            ApplicationCommand::NextDiffLine,
            "Next diff line",
            &[plain('j'), plain_code(KeyCode::Down)],
            CommandCategory::Diff,
            CommandRequirements {
                active_workspace: true,
                selected_file: true,
                ..Default::default()
            },
        );
        catalog.add(
            ApplicationCommand::PreviousDiffLine,
            "Previous diff line",
            &[plain('k'), plain_code(KeyCode::Up)],
            CommandCategory::Diff,
            CommandRequirements {
                active_workspace: true,
                selected_file: true,
                ..Default::default()
            },
        );
        catalog.add(
            ApplicationCommand::FirstDiff,
            "First diff line",
            &[plain('g'), plain_code(KeyCode::Home)],
            CommandCategory::Diff,
            CommandRequirements {
                active_workspace: true,
                selected_file: true,
                ..Default::default()
            },
        );
        catalog.add(
            ApplicationCommand::LastDiff,
            "Last diff line",
            &[plain('G'), plain_code(KeyCode::End)],
            CommandCategory::Diff,
            CommandRequirements {
                active_workspace: true,
                selected_file: true,
                ..Default::default()
            },
        );
        catalog.add(
            ApplicationCommand::NextDiffPage,
            "Next diff page",
            &[plain_code(KeyCode::PageDown)],
            CommandCategory::Diff,
            CommandRequirements {
                active_workspace: true,
                selected_file: true,
                ..Default::default()
            },
        );
        catalog.add(
            ApplicationCommand::PreviousDiffPage,
            "Previous diff page",
            &[plain_code(KeyCode::PageUp)],
            CommandCategory::Diff,
            CommandRequirements {
                active_workspace: true,
                selected_file: true,
                ..Default::default()
            },
        );
        catalog.add(
            ApplicationCommand::ScrollDiffLeft,
            "Scroll diff left",
            &[plain('h'), plain_code(KeyCode::Left)],
            CommandCategory::Diff,
            CommandRequirements {
                active_workspace: true,
                selected_file: true,
                ..Default::default()
            },
        );
        catalog.add(
            ApplicationCommand::ScrollDiffRight,
            "Scroll diff right",
            &[plain('l'), plain_code(KeyCode::Right)],
            CommandCategory::Diff,
            CommandRequirements {
                active_workspace: true,
                selected_file: true,
                ..Default::default()
            },
        );
        catalog.add(
            ApplicationCommand::NextHunk,
            "Next hunk",
            &[plain('}')],
            CommandCategory::Diff,
            CommandRequirements {
                active_workspace: true,
                selected_file: true,
                ..Default::default()
            },
        );
        catalog.add(
            ApplicationCommand::PreviousHunk,
            "Previous hunk",
            &[plain('{')],
            CommandCategory::Diff,
            CommandRequirements {
                active_workspace: true,
                selected_file: true,
                ..Default::default()
            },
        );
        catalog.add(
            ApplicationCommand::ToggleLayout,
            "Toggle layout",
            &[plain('s')],
            CommandCategory::Layout,
            CommandRequirements { active_workspace: true, ..Default::default() },
        );
        catalog.add(
            ApplicationCommand::ToggleWrapping,
            "Toggle wrapping",
            &[plain('w')],
            CommandCategory::Layout,
            CommandRequirements { active_workspace: true, ..Default::default() },
        );
        catalog.add(
            ApplicationCommand::EqualizeDividers,
            "Equalize dividers",
            &[plain('=')],
            CommandCategory::Layout,
            CommandRequirements { active_workspace: true, ..Default::default() },
        );
        catalog.add(
            ApplicationCommand::EnterPrompt,
            "Enter prompt",
            &[plain('i')],
            CommandCategory::Agent,
            CommandRequirements {
                active_workspace: true,
                attached_agent: true,
                ..Default::default()
            },
        );
        for (choice, digit) in ('1'..='9').enumerate() {
            let choice = choice + 1;
            catalog.add(
                ApplicationCommand::AnswerPermission(choice),
                format!("Answer permission {choice}"),
                &[plain(digit)],
                CommandCategory::Permission,
                CommandRequirements { interruption: true, ..Default::default() },
            );
        }
        catalog
    }

    pub fn commands(&self) -> &[CommandDefinition] {
        &self.commands
    }

    pub fn definition(&self, command: ApplicationCommand) -> Option<&CommandDefinition> {
        self.commands.iter().find(|definition| definition.command == command)
    }

    pub fn by_shortcut(
        &self,
        shortcut: Shortcut,
        context: &ApplicationContext,
    ) -> Option<&CommandDefinition> {
        self.commands.iter().find(|definition| {
            definition.available(context) && definition.shortcuts.contains(&shortcut)
        })
    }

    pub fn command_for_shortcut(
        &self,
        shortcut: Shortcut,
        context: &ApplicationContext,
    ) -> Option<ApplicationCommand> {
        self.by_shortcut(shortcut, context).map(CommandDefinition::command)
    }

    pub fn by_key(
        &self,
        key: KeyEvent,
        context: &ApplicationContext,
    ) -> Option<&CommandDefinition> {
        self.by_shortcut(key.into(), context)
    }

    pub fn command_for_key(
        &self,
        key: KeyEvent,
        context: &ApplicationContext,
    ) -> Option<ApplicationCommand> {
        self.by_key(key, context).map(CommandDefinition::command)
    }

    pub fn available(&self, context: &ApplicationContext) -> Vec<&CommandDefinition> {
        self.commands.iter().filter(|command| command.available(context)).collect()
    }

    pub fn palette_entries(&self, context: &ApplicationContext) -> Vec<PaletteEntry> {
        self.available(context).into_iter().map(PaletteEntry::command).collect()
    }

    fn add(
        &mut self,
        command: ApplicationCommand,
        label: impl Into<String>,
        shortcuts: &[Shortcut],
        category: CommandCategory,
        requirements: CommandRequirements,
    ) {
        self.commands.push(CommandDefinition {
            command,
            label: label.into(),
            shortcuts: shortcuts.to_vec(),
            category,
            requirements,
        });
    }
}

impl Default for CommandCatalog {
    fn default() -> Self {
        Self::new()
    }
}

fn plain(character: char) -> Shortcut {
    Shortcut::new(KeyCode::Char(character), KeyModifiers::NONE)
}

fn plain_code(code: KeyCode) -> Shortcut {
    Shortcut::new(code, KeyModifiers::NONE)
}

fn ctrl(character: char) -> Shortcut {
    Shortcut::new(KeyCode::Char(character), KeyModifiers::CONTROL)
}

/// A project identity and its palette label.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct ProjectTarget {
    pub id: String,
    pub label: String,
}

impl ProjectTarget {
    pub fn new(id: impl Into<String>, label: impl Into<String>) -> Self {
        Self { id: id.into(), label: label.into() }
    }
}

/// A workspace identity, retaining the project that owns it.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct WorkspaceTarget {
    pub project: String,
    pub id: String,
    pub label: String,
}

impl WorkspaceTarget {
    pub fn new(
        project: impl Into<String>,
        id: impl Into<String>,
        label: impl Into<String>,
    ) -> Self {
        Self { project: project.into(), id: id.into(), label: label.into() }
    }
}

/// A file identity, retaining both its workspace and project ownership.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct FileTarget {
    pub project: String,
    pub workspace: String,
    pub path: PathBuf,
    pub label: String,
}

impl FileTarget {
    pub fn new(
        project: impl Into<String>,
        workspace: impl Into<String>,
        path: impl Into<PathBuf>,
    ) -> Self {
        let path = path.into();
        let label = path.display().to_string();
        Self { project: project.into(), workspace: workspace.into(), path, label }
    }
}

/// A future skill target without loading or invoking skills.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct SkillTarget {
    pub namespace: String,
    pub name: String,
    pub label: String,
}

impl SkillTarget {
    pub fn new(
        namespace: impl Into<String>,
        name: impl Into<String>,
        label: impl Into<String>,
    ) -> Self {
        Self { namespace: namespace.into(), name: name.into(), label: label.into() }
    }
}

/// A typed item that can appear in the unified palette.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum PaletteTarget {
    ApplicationCommand(ApplicationCommand),
    Project(ProjectTarget),
    Workspace(WorkspaceTarget),
    File(FileTarget),
    Skill(SkillTarget),
}

/// A palette item with the label and shortcuts shown by the UI.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct PaletteEntry {
    target: PaletteTarget,
    label: String,
    shortcuts: Vec<Shortcut>,
}

impl PaletteEntry {
    pub fn new(target: PaletteTarget, label: impl Into<String>) -> Self {
        Self { target, label: label.into(), shortcuts: Vec::new() }
    }

    pub fn command(definition: &CommandDefinition) -> Self {
        Self {
            target: PaletteTarget::ApplicationCommand(definition.command),
            label: definition.label.clone(),
            shortcuts: definition.shortcuts.clone(),
        }
    }

    pub fn target(&self) -> &PaletteTarget {
        &self.target
    }

    pub fn label(&self) -> &str {
        &self.label
    }

    pub fn shortcuts(&self) -> &[Shortcut] {
        &self.shortcuts
    }
}

/// A deterministic, case-insensitive ranker for mixed palette entries.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct PaletteModel;

impl PaletteModel {
    pub fn rank<'entry>(
        &self,
        query: &str,
        entries: &'entry [PaletteEntry],
    ) -> Vec<&'entry PaletteEntry> {
        let query = lower(query);
        if query.is_empty() {
            return entries.iter().collect();
        }

        let mut ranked = entries
            .iter()
            .enumerate()
            .filter_map(|(order, entry)| {
                let score = score(&query, &lower(entry.label()))?;
                Some((score, order, entry))
            })
            .collect::<Vec<_>>();
        ranked.sort_by_key(|(score, order, _)| (*score, *order));
        ranked.into_iter().map(|(_, _, entry)| entry).collect()
    }
}

fn lower(text: &str) -> Vec<char> {
    text.chars().flat_map(char::to_lowercase).collect()
}

fn score(query: &[char], label: &[char]) -> Option<u8> {
    if query == label {
        return Some(0);
    }
    if label.starts_with(query) {
        return Some(1);
    }

    let mut label = label.iter();
    for character in query {
        if !label.any(|candidate| candidate == character) {
            return None;
        }
    }
    Some(2)
}
