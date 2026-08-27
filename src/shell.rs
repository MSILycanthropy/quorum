//! The terminal application shell connecting persistence, agents and surfaces.
#![expect(
    clippy::result_large_err,
    clippy::missing_errors_doc,
    clippy::missing_panics_doc,
    clippy::needless_pass_by_value,
    reason = "the shell forwards Git and UI errors, and owns values while constructing its retained surfaces"
)]

use std::collections::HashMap;
use std::path::Path;
use std::sync::mpsc::{Receiver, Sender, channel};
use std::time::{SystemTime, UNIX_EPOCH};

use ratatui::Frame;
use ratatui::buffer::Buffer;
use ratatui::crossterm::event::{
    self, DisableMouseCapture, EnableMouseCapture, KeyCode, KeyEvent, KeyEventKind,
    KeyboardEnhancementFlags, MouseButton, MouseEvent, MouseEventKind, PopKeyboardEnhancementFlags,
    PushKeyboardEnhancementFlags,
};
use ratatui::crossterm::execute;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Style};

use crate::agent::{Consequence, Spawn, State, Transcript, Update};
use crate::application::{ProjectState, WorkspaceCatalog};
use crate::command::{
    ApplicationCommand, ApplicationContext, CommandCatalog, FileTarget, PaletteEntry,
    PaletteTarget, ProjectTarget, WorkspaceTarget,
};
use crate::git::Repository;
use crate::project::{ProjectId, Workspace, WorkspaceId};
use crate::store::{DEFAULT_LEASE_POLICY, LeaseAcquisition, Store};
use crate::ui::{
    Activity, Control, Mode, Navigator, NavigatorSnapshot, Opening, Palette, PaletteAction,
    ProjectPresentation, Tab, WorkspacePresentation, WorkspaceState,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RunOptions {
    pub mode: Mode,
    pub wrap: bool,
    pub tab_width: usize,
    pub revision: Option<String>,
    pub agent: Option<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Agent(#[from] crate::agent::Error),
    #[error(transparent)]
    Git(#[from] crate::git::Error),
    #[error(transparent)]
    Store(#[from] crate::store::Error),
    #[error(transparent)]
    Application(#[from] crate::application::Error),
    #[error(transparent)]
    Ui(#[from] crate::ui::Error),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error("workspace {workspace} is controlled by another Quorum process")]
    WorkspaceOwned { workspace: WorkspaceId },
    #[error("the system clock is before the Unix epoch")]
    Clock,
}

pub type Result<T> = std::result::Result<T, Error>;

enum Event {
    Key(KeyEvent),
    Mouse(MouseEvent),
    Redraw,
    Agent { workspace_id: WorkspaceId, update: Update },
}

struct Surface {
    tab: Tab,
    repository: Repository,
    against: String,
}

pub struct Application {
    store: Store,
    catalog: WorkspaceCatalog,
    navigator: Navigator,
    surfaces: HashMap<WorkspaceId, Surface>,
    transcripts: HashMap<WorkspaceId, Transcript>,
    active: WorkspaceId,
    commands: CommandCatalog,
    palette: Palette,
    owner: String,
    leased: Option<WorkspaceId>,
}

impl Application {
    fn open(
        store: Store,
        catalog: WorkspaceCatalog,
        active: WorkspaceId,
        opening: Opening,
        options: &RunOptions,
        sender: Sender<Event>,
    ) -> Result<Self> {
        let owner = owner_id()?;
        let mut application = Self {
            store,
            catalog,
            navigator: Navigator::default(),
            surfaces: HashMap::new(),
            transcripts: HashMap::new(),
            active,
            commands: CommandCatalog::default(),
            palette: Palette::default(),
            owner,
            leased: None,
        };
        application.open_surfaces(&opening, options.revision.clone())?;
        application.refresh_navigator();
        application.navigator.select_workspace(&application.active);
        if let Some(command) = &options.agent {
            application.attach_agent(command, sender)?;
        }
        Ok(application)
    }

    fn open_surfaces(&mut self, opening: &Opening, revision: Option<String>) -> Result<()> {
        let workspaces = self.catalog.projects.iter().flat_map(|state| &state.workspaces).cloned();
        let all_workspaces = workspaces.clone().collect::<Vec<_>>();
        for workspace in all_workspaces {
            self.transcripts
                .insert(workspace.id().clone(), self.store.load_transcript(workspace.id())?);
        }
        for workspace in workspaces.filter(Workspace::is_attached) {
            let Ok(repository) = Repository::discover(workspace.last_known_location()) else {
                continue;
            };
            let against = if let Some(revision) = revision.clone() {
                revision
            } else {
                let Ok(against) = repository.base() else {
                    continue;
                };
                against
            };
            let Ok(changes) = repository.changes(&against) else {
                continue;
            };
            let Ok(branch) = repository.branch() else {
                continue;
            };
            let transcript = self
                .transcripts
                .get(workspace.id())
                .cloned()
                .expect("every workspace transcript was restored before surfaces opened");
            let tab_opening = Opening { branch, base: against.clone(), ..opening.clone() };
            let tab = tab_opening.tab_with_transcript(changes, None, transcript)?;
            self.surfaces.insert(workspace.id().clone(), Surface { tab, repository, against });
        }
        Ok(())
    }

    fn attach_agent(&mut self, command: &str, sender: Sender<Event>) -> Result<()> {
        let workspace = self.active.clone();
        self.acquire(&workspace)?;
        let directory = self
            .surfaces
            .get(&workspace)
            .map(|surface| surface.repository.workdir().to_path_buf())
            .ok_or_else(|| Error::WorkspaceOwned { workspace: workspace.clone() })?;
        let session = Spawn { command: command.to_owned(), directory }
            .session(forward(sender, workspace.clone()))?;
        let surface = self
            .surfaces
            .get_mut(&workspace)
            .expect("the active workspace was used to build the agent directory");
        surface.tab.attach_session(session);
        Ok(())
    }

    pub fn draw(&mut self, frame: &mut Frame) {
        let columns =
            Layout::horizontal([Constraint::Length(28), Constraint::Min(1)]).split(frame.area());
        self.navigator.render(frame.buffer_mut(), columns[0]);
        if let Some(surface) = self.surfaces.get_mut(&self.active) {
            surface.tab.draw_in(frame, columns[1]);
        } else {
            draw_unavailable(frame.buffer_mut(), columns[1]);
        }
        self.palette.draw(frame);
    }

    pub fn key(&mut self, key: KeyEvent) -> Result<Control> {
        self.renew()?;
        let active_has_session =
            self.surfaces.get(&self.active).is_some_and(|surface| surface.tab.has_session());
        if active_has_session {
            self.acquire(&self.active.clone())?;
        }
        if self.palette.is_open() {
            return match self.palette.key(key) {
                PaletteAction::Execute(target) => {
                    self.dispatch_target(target)?;
                    Ok(Control::Continue)
                }
                PaletteAction::Close | PaletteAction::None => Ok(Control::Continue),
            };
        }

        let context = self.context();
        if let Some(command) = self.commands.command_for_key(key, &context) {
            return self.dispatch(command, key);
        }
        let Some(surface) = self.surfaces.get_mut(&self.active) else {
            return Ok(Control::Continue);
        };
        let control = surface.tab.key(key)?;
        self.persist_activities(&self.active.clone())?;
        Ok(control)
    }

    pub fn mouse(&mut self, mouse: MouseEvent) -> Result<()> {
        let navigator_area = Rect::new(0, 0, 28, u16::MAX);
        if mouse.column < navigator_area.width
            && matches!(mouse.kind, MouseEventKind::Down(MouseButton::Left))
        {
            if let Some(workspace) = self.navigator.workspace_at(navigator_area, mouse.row) {
                self.activate(workspace);
            } else if let Some(project) = self.navigator.project_at(navigator_area, mouse.row) {
                self.navigator.toggle_project(&project);
            }
            return Ok(());
        }
        if let Some(surface) = self.surfaces.get_mut(&self.active) {
            surface.tab.mouse(mouse)?;
        }
        Ok(())
    }

    pub fn update(&mut self, workspace_id: WorkspaceId, update: Update) -> Result<()> {
        self.renew()?;
        let Some(surface) = self.surfaces.get_mut(&workspace_id) else {
            return Ok(());
        };
        self.store.append_update(&workspace_id, update.clone())?;
        if surface.tab.apply(update) == Consequence::Rescan {
            let changes = surface.repository.changes(&surface.against)?;
            surface.tab.refresh(changes)?;
        }
        self.refresh_navigator();
        Ok(())
    }

    pub fn release(&mut self) {
        if let Some(workspace) = self.leased.take() {
            let _ = self.store.release_lease(&workspace, &self.owner);
        }
    }

    fn dispatch(&mut self, command: ApplicationCommand, key: KeyEvent) -> Result<Control> {
        match command {
            ApplicationCommand::OpenPalette => {
                self.palette.open(self.palette_entries());
                Ok(Control::Continue)
            }
            ApplicationCommand::Quit => Ok(Control::Quit),
            ApplicationCommand::NextWorkspace => {
                if let Some(id) = self.navigator.next_workspace().cloned() {
                    self.activate(id);
                }
                Ok(Control::Continue)
            }
            ApplicationCommand::PreviousWorkspace => {
                if let Some(id) = self.navigator.previous_workspace().cloned() {
                    self.activate(id);
                }
                Ok(Control::Continue)
            }
            ApplicationCommand::ToggleProject => {
                if let Some(project) = self.project_of(&self.active) {
                    self.navigator.toggle_project(&project);
                }
                Ok(Control::Continue)
            }
            ApplicationCommand::NextFile
            | ApplicationCommand::PreviousFile
            | ApplicationCommand::ToggleFileList
            | ApplicationCommand::ToggleFileTreeNode
            | ApplicationCommand::NextDiffLine
            | ApplicationCommand::PreviousDiffLine
            | ApplicationCommand::FirstDiff
            | ApplicationCommand::LastDiff
            | ApplicationCommand::NextDiffPage
            | ApplicationCommand::PreviousDiffPage
            | ApplicationCommand::ScrollDiffLeft
            | ApplicationCommand::ScrollDiffRight
            | ApplicationCommand::ToggleLayout
            | ApplicationCommand::ToggleWrapping
            | ApplicationCommand::EqualizeDividers
            | ApplicationCommand::OpenSelectedFile => self.forward_key(key),
            ApplicationCommand::AnswerPermission(_) | ApplicationCommand::EnterPrompt => {
                self.forward_key(key)
            }
            ApplicationCommand::NextProject => {
                self.move_project(1);
                Ok(Control::Continue)
            }
            ApplicationCommand::PreviousProject => {
                self.move_project(-1);
                Ok(Control::Continue)
            }
        }
    }

    fn forward_key(&mut self, key: KeyEvent) -> Result<Control> {
        if let Some(surface) = self.surfaces.get_mut(&self.active) {
            let control = surface.tab.key(key)?;
            self.persist_activities(&self.active.clone())?;
            Ok(control)
        } else {
            Ok(Control::Continue)
        }
    }

    fn dispatch_target(&mut self, target: PaletteTarget) -> Result<()> {
        match target {
            PaletteTarget::ApplicationCommand(command) => {
                if let Some(definition) = self.commands.definition(command)
                    && definition.available(&self.context())
                {
                    let key = definition.shortcuts().first().map_or_else(
                        || {
                            KeyEvent::new(
                                KeyCode::Null,
                                ratatui::crossterm::event::KeyModifiers::NONE,
                            )
                        },
                        |shortcut| KeyEvent::new(shortcut.code, shortcut.modifiers),
                    );
                    self.dispatch(command, key)?;
                }
            }
            PaletteTarget::Project(project) => {
                if let Some(id) = self
                    .catalog
                    .project(&project.id)
                    .and_then(|state| state.workspaces.first())
                    .map(|workspace| workspace.id().clone())
                {
                    self.activate(id);
                }
            }
            PaletteTarget::Workspace(workspace) => self.activate(workspace.id),
            PaletteTarget::File(file) => {
                self.activate(file.workspace.clone());
                if let Some(surface) = self.surfaces.get_mut(&file.workspace) {
                    surface.tab.select_file(&file.path)?;
                }
            }
            PaletteTarget::Skill(_) => {}
        }
        Ok(())
    }

    fn palette_entries(&self) -> Vec<PaletteEntry> {
        let mut entries = self.commands.palette_entries(&self.context());
        for state in &self.catalog.projects {
            entries.push(PaletteEntry::new(
                PaletteTarget::Project(ProjectTarget::new(
                    state.project.id().clone(),
                    state.project.display_name(),
                )),
                state.project.display_name(),
            ));
            for workspace in &state.workspaces {
                let label = workspace.branch().map_or_else(
                    || workspace.last_known_location().display().to_string(),
                    str::to_owned,
                );
                entries.push(PaletteEntry::new(
                    PaletteTarget::Workspace(WorkspaceTarget::new(
                        state.project.id().clone(),
                        workspace.id().clone(),
                        label.clone(),
                    )),
                    label,
                ));
            }
        }
        if let Some(surface) = self.surfaces.get(&self.active) {
            for path in surface.tab.file_paths() {
                let label = path.display().to_string();
                entries.push(PaletteEntry::new(
                    PaletteTarget::File(FileTarget::new(
                        self.project_of(&self.active).expect("active workspace has a project"),
                        self.active.clone(),
                        path,
                    )),
                    label,
                ));
            }
        }
        entries
    }

    fn context(&self) -> ApplicationContext {
        let file_count =
            self.surfaces.get(&self.active).map_or(0, |surface| surface.tab.file_paths().len());
        let interruption = self
            .surfaces
            .get(&self.active)
            .and_then(|surface| surface.tab.transcript().pending())
            .map(|permission| crate::command::InterruptionState::new(permission.choices.len()));
        ApplicationContext {
            project_count: self.catalog.projects.len(),
            workspace_count: self.catalog.workspaces().count(),
            file_count,
            active_project: self.project_of(&self.active).is_some(),
            active_workspace: self.catalog.workspace(&self.active).is_some(),
            selected_file: file_count > 0,
            attached_agent: self
                .surfaces
                .get(&self.active)
                .is_some_and(|surface| surface.tab.has_session()),
            interruption,
        }
    }

    fn activate(&mut self, workspace: WorkspaceId) {
        if self.catalog.workspace(&workspace).is_some() {
            self.active = workspace.clone();
            self.navigator.select_workspace(&workspace);
        }
    }

    fn move_project(&mut self, direction: isize) {
        let Some(current) = self.project_of(&self.active) else {
            return;
        };
        let Some(index) =
            self.catalog.projects.iter().position(|state| state.project.id() == &current)
        else {
            return;
        };
        let count = self.catalog.projects.len();
        if count == 0 {
            return;
        }
        let next =
            usize::try_from((index.cast_signed() + direction).rem_euclid(count.cast_signed()))
                .expect("the project index is reduced modulo the project count");
        if let Some(workspace) = self.catalog.projects[next].workspaces.first() {
            self.activate(workspace.id().clone());
        }
    }

    fn project_of(&self, workspace: &WorkspaceId) -> Option<ProjectId> {
        self.catalog
            .projects
            .iter()
            .find(|state| state.workspaces.iter().any(|candidate| candidate.id() == workspace))
            .map(|state| state.project.id().clone())
    }

    fn acquire(&mut self, workspace: &WorkspaceId) -> Result<()> {
        let now = now()?;
        match self.store.acquire_lease(workspace, &self.owner, now, DEFAULT_LEASE_POLICY, false)? {
            LeaseAcquisition::Held(_) => {
                Err(Error::WorkspaceOwned { workspace: workspace.clone() })
            }
            LeaseAcquisition::Acquired(_)
            | LeaseAcquisition::Renewed(_)
            | LeaseAcquisition::Reclaimed(_)
            | LeaseAcquisition::TakenOver(_) => {
                self.leased = Some(workspace.clone());
                Ok(())
            }
        }
    }

    fn renew(&self) -> Result<()> {
        let Some(workspace) = &self.leased else {
            return Ok(());
        };
        let now = now()?;
        self.store.renew_lease(workspace, &self.owner, now, DEFAULT_LEASE_POLICY)?;
        Ok(())
    }

    fn persist_activities(&mut self, workspace: &WorkspaceId) -> Result<()> {
        let Some(surface) = self.surfaces.get_mut(workspace) else {
            return Ok(());
        };
        for activity in surface.tab.take_activities() {
            match activity {
                Activity::Prompt(prompt) => {
                    self.store.append_prompt(workspace, prompt)?;
                }
                Activity::PermissionAnswer { permission, choice } => {
                    self.store.append_permission_answer(workspace, permission, choice)?;
                }
            }
        }
        Ok(())
    }

    fn refresh_navigator(&mut self) {
        let projects = self
            .catalog
            .projects
            .iter()
            .map(|state| project_presentation(state, &self.surfaces))
            .collect();
        self.navigator.set_snapshot(NavigatorSnapshot::new(projects));
    }
}

impl Drop for Application {
    fn drop(&mut self) {
        self.release();
    }
}

fn project_presentation(
    state: &ProjectState,
    surfaces: &HashMap<WorkspaceId, Surface>,
) -> ProjectPresentation {
    let workspaces = state
        .workspaces
        .iter()
        .map(|workspace| {
            let (state, changed_files) = surfaces.get(workspace.id()).map_or_else(
                || (WorkspaceState::Idle, 0),
                |surface| {
                    let state = match surface.tab.transcript().state() {
                        State::Working => WorkspaceState::Working,
                        State::Blocked => WorkspaceState::Blocked,
                        State::Idle => WorkspaceState::Idle,
                    };
                    (state, surface.tab.file_paths().len())
                },
            );
            WorkspacePresentation {
                id: workspace.id().clone(),
                label: workspace.branch().unwrap_or("main").to_owned(),
                order: workspace.order(),
                state,
                changed_files,
                main: workspace.kind() == crate::project::WorkspaceKind::Main,
                unavailable: !workspace.is_attached() || !surfaces.contains_key(workspace.id()),
            }
        })
        .collect();
    ProjectPresentation::new(
        state.project.id().clone(),
        state.project.display_name(),
        state.project.order(),
        workspaces,
    )
}

fn draw_unavailable(buffer: &mut Buffer, area: Rect) {
    buffer.set_style(area, Style::default().bg(Color::Black).fg(Color::DarkGray));
    buffer.set_stringn(
        area.x.saturating_add(2),
        area.y.saturating_add(2),
        "workspace unavailable",
        usize::from(area.width.saturating_sub(4)),
        Style::default().bg(Color::Black).fg(Color::DarkGray),
    );
}

fn forward(sender: Sender<Event>, workspace_id: WorkspaceId) -> Sender<Update> {
    let (updates, incoming) = channel();
    std::thread::Builder::new()
        .name("agent-events".to_owned())
        .spawn(move || {
            while let Ok(update) = incoming.recv() {
                if sender.send(Event::Agent { workspace_id: workspace_id.clone(), update }).is_err()
                {
                    return;
                }
            }
        })
        .expect("an agent event reader cannot fail to start");
    updates
}

pub fn run(
    repository: Repository,
    inventory: crate::git::ProjectInventory,
    against: String,
    options: RunOptions,
) -> Result<()> {
    let store = Store::open_user_local()?;
    let catalog = crate::application::reconcile_all(&store, &inventory)?;
    let project = catalog
        .projects
        .iter()
        .find(|state| same_path(state.project.common_directory(), &inventory.common_directory))
        .expect("reconciliation includes the discovered project");
    let active = project
        .workspaces
        .iter()
        .find(|workspace| same_path(workspace.last_known_location(), repository.workdir()))
        .or_else(|| project.workspaces.iter().find(|workspace| workspace.is_attached()))
        .expect("the discovered repository has an attached workspace")
        .id()
        .clone();
    let opening = Opening {
        mode: options.mode,
        wrap: options.wrap,
        tab_width: options.tab_width,
        branch: repository.branch()?,
        base: against,
    };
    let (sender, events) = channel();
    let mut application =
        Application::open(store, catalog, active, opening, &options, sender.clone())?;
    watch(sender);
    run_terminal(&mut application, events)
}

fn same_path(left: &Path, right: &Path) -> bool {
    let normalize = |path: &Path| {
        path.canonicalize().unwrap_or_else(|_| {
            if path.is_absolute() {
                path.to_path_buf()
            } else {
                std::env::current_dir()
                    .map_or_else(|_| path.to_path_buf(), |current| current.join(path))
            }
        })
    };
    normalize(left) == normalize(right)
}

fn run_terminal(application: &mut Application, events: Receiver<Event>) -> Result<()> {
    ratatui::run(|terminal| -> Result<()> {
        let _reporting = Reporting::on()?;
        terminal.draw(|frame| application.draw(frame))?;
        while let Ok(event) = events.recv() {
            let control = match event {
                Event::Key(key) => application.key(key)?,
                Event::Mouse(mouse) => {
                    application.mouse(mouse)?;
                    Control::Continue
                }
                Event::Agent { workspace_id, update } => {
                    application.update(workspace_id, update)?;
                    Control::Continue
                }
                Event::Redraw => Control::Continue,
            };
            if control == Control::Quit {
                application.release();
                return Ok(());
            }
            terminal.draw(|frame| application.draw(frame))?;
        }
        application.release();
        Ok(())
    })
}

fn watch(sender: Sender<Event>) {
    std::thread::Builder::new()
        .name("terminal".to_owned())
        .spawn(move || {
            while let Ok(input) = event::read() {
                let input = match input {
                    ratatui::crossterm::event::Event::Key(key)
                        if key.kind == KeyEventKind::Press =>
                    {
                        Event::Key(key)
                    }
                    ratatui::crossterm::event::Event::Mouse(mouse) => Event::Mouse(mouse),
                    ratatui::crossterm::event::Event::Resize(_, _) => Event::Redraw,
                    _ => continue,
                };
                if sender.send(input).is_err() {
                    return;
                }
            }
        })
        .expect("a terminal event reader cannot fail to start");
}

struct Reporting;

impl Reporting {
    fn on() -> Result<Self> {
        execute!(
            std::io::stdout(),
            EnableMouseCapture,
            PushKeyboardEnhancementFlags(KeyboardEnhancementFlags::DISAMBIGUATE_ESCAPE_CODES)
        )?;
        Ok(Self)
    }
}

impl Drop for Reporting {
    fn drop(&mut self) {
        let _ = execute!(std::io::stdout(), PopKeyboardEnhancementFlags, DisableMouseCapture);
    }
}

fn owner_id() -> Result<String> {
    let time = SystemTime::now().duration_since(UNIX_EPOCH).map_err(|_| Error::Clock)?.as_nanos();
    Ok(format!("quorum-{}-{time}", std::process::id()))
}

fn now() -> Result<i64> {
    let millis =
        SystemTime::now().duration_since(UNIX_EPOCH).map_err(|_| Error::Clock)?.as_millis();
    i64::try_from(millis).map_err(|_| Error::Clock)
}
