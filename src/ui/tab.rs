//! One worktree, one agent: the diff, the transcript and the keys over both.

use ratatui::Frame;
use ratatui::buffer::Buffer;
use ratatui::crossterm::event::{KeyCode, KeyEvent, MouseButton, MouseEvent, MouseEventKind};
use ratatui::layout::{Position, Rect};
use ratatui::style::{Color, Style};
use unicode_width::UnicodeWidthStr;

use crate::color::Rgb;

use crate::agent::{Consequence, Session, Transcript, Update};
use crate::git::FileChange;
use crate::theme::{Tints, TokyoNight};
use crate::ui::{Control, Conversation, Mode, Prompt, Result, Review, Split};

const TOGETHER: u16 = 159;

/// What a tab is opened with, beyond its changes and its agent.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Opening {
    pub mode: Mode,
    pub wrap: bool,
    pub tab_width: usize,
    pub branch: Option<String>,
    pub base: String,
}

impl Default for Opening {
    fn default() -> Self {
        Self { mode: Mode::default(), wrap: false, tab_width: 4, branch: None, base: String::new() }
    }
}

impl Opening {
    #[expect(
        clippy::missing_errors_doc,
        reason = "the Error enum names the only failure, highlighting the selected file"
    )]
    pub fn tab(self, changes: Vec<FileChange>, session: Option<Session>) -> Result<Tab> {
        self.tab_with_transcript(changes, session, Transcript::default())
    }

    #[expect(
        clippy::missing_errors_doc,
        reason = "the Error enum names the only failure, highlighting the selected file"
    )]
    pub fn tab_with_transcript(
        self,
        changes: Vec<FileChange>,
        session: Option<Session>,
        transcript: Transcript,
    ) -> Result<Tab> {
        Ok(Tab {
            review: Review::new(changes, self.mode, self.wrap, self.tab_width)?,
            conversation: Conversation::default(),
            transcript,
            prompt: Prompt::default(),
            split: Split::default(),
            session,
            branch: self.branch,
            base: self.base,
            tints: TokyoNight::TINTS,
            activities: Vec::new(),
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Activity {
    Prompt(String),
    PermissionAnswer { permission: String, choice: String },
}

/// A worktree and the agent working in it, as one surface.
pub struct Tab {
    review: Review,
    conversation: Conversation,
    transcript: Transcript,
    prompt: Prompt,
    split: Split,
    session: Option<Session>,
    branch: Option<String>,
    base: String,
    tints: Tints,
    activities: Vec<Activity>,
}

impl Tab {
    pub fn draw(&mut self, frame: &mut Frame) {
        self.draw_in(frame, frame.area());
    }

    pub fn draw_in(&mut self, frame: &mut Frame, whole: Rect) {
        let standing = Rect::new(whole.x, whole.y + whole.height - 1, whole.width, 1);
        let area = Rect::new(whole.x, whole.y, whole.width, whole.height - 1);
        self.standing(frame.buffer_mut(), standing);

        let listing = self.review.sidebar().is_some();

        if area.width >= TOGETHER {
            let columns = self.split.columns(area, [listing, true, true]);
            let reviewed =
                Rect::new(area.x, area.y, columns.tree.width + columns.diff.width, area.height);
            self.review.resize(columns.tree.width);

            self.conversation.gutter(frame.buffer_mut(), columns.conversation);
            let beside = Rect::new(
                columns.conversation.x + 1,
                columns.conversation.y,
                columns.conversation.width.saturating_sub(1),
                columns.conversation.height,
            );

            self.review.draw(frame, reviewed);
            let spoken = self.foot(frame.buffer_mut(), beside);
            self.conversation.draw(frame.buffer_mut(), spoken, &self.transcript);

            return;
        }

        let line = Rect::new(area.x, area.y, area.width, 1.min(area.height));
        let rest = Rect::new(area.x, area.y + line.height, area.width, area.height - line.height);

        self.conversation.status(frame.buffer_mut(), line, &self.transcript);
        let reviewed = self.foot(frame.buffer_mut(), rest);
        let columns = self.split.columns(reviewed, [listing, true, false]);
        self.review.resize(columns.tree.width);
        self.review.draw(frame, reviewed);
    }

    fn standing(&self, buffer: &mut Buffer, area: Rect) {
        buffer.set_style(area, Style::new().bg(color(self.tints.filler)));

        let branch = self.branch.clone().unwrap_or_else(|| "detached".to_owned());
        let base: String = self.base.chars().take(7).collect();
        let left = format!("⎇ {branch} · {base} · {}", self.review.counted());
        let right = format!("{} · tab {}", self.review.laid(), self.review.tabbed());
        let muted = Style::new().fg(color(self.tints.muted)).bg(color(self.tints.filler));

        buffer.set_stringn(area.x + 1, area.y, &left, usize::from(area.width), muted);

        let room = u16::try_from(right.width()).unwrap_or(0);
        if u16::try_from(left.width()).unwrap_or(0) + room + 4 <= area.width {
            buffer.set_string(area.x + area.width - room - 1, area.y, &right, muted);
        }
    }

    fn foot(&mut self, buffer: &mut Buffer, area: Rect) -> Rect {
        let suggestions = self.prompt.suggestions(&self.transcript);
        let (rest, typed) = split(area, self.prompt.height(area.width));
        self.prompt.draw(buffer, typed, &self.transcript);

        if !suggestions.is_empty() {
            let tall = u16::try_from(suggestions.len()).unwrap_or(0);
            let (rest, listed) = split(rest, tall);
            self.prompt.suggest(buffer, listed, &suggestions);

            return rest;
        }

        let planned = self.conversation.planned(&self.transcript);
        let (rest, listed) = split(rest, planned);
        self.conversation.plan(buffer, listed, &self.transcript);

        rest
    }

    #[expect(
        clippy::missing_errors_doc,
        reason = "the Error enum names the only failure, highlighting the file moved to"
    )]
    pub fn key(&mut self, key: KeyEvent) -> Result<Control> {
        if self.prompt.entered() {
            if let Some(text) = self.prompt.key(key) {
                self.ask(text);
            }

            return Ok(Control::Continue);
        }

        match key.code {
            KeyCode::Char('i') if self.session.is_some() => {
                self.prompt.enter();

                Ok(Control::Continue)
            }
            KeyCode::Char('=') => {
                self.split.even();

                Ok(Control::Continue)
            }
            KeyCode::Char(digit @ '1'..='9') if self.transcript.pending().is_some() => {
                self.choose(digit);

                Ok(Control::Continue)
            }
            _ => self.review.key(key),
        }
    }

    #[expect(
        clippy::missing_errors_doc,
        reason = "the Error enum names the only failure, highlighting the file clicked on"
    )]
    pub fn mouse(&mut self, mouse: MouseEvent) -> Result<()> {
        let at = Position::new(mouse.column, mouse.row);

        match mouse.kind {
            MouseEventKind::Down(MouseButton::Left) if self.split.grab(at) => return Ok(()),
            MouseEventKind::Drag(MouseButton::Left) if self.split.held() => {
                self.split.drag(at);

                return Ok(());
            }
            MouseEventKind::Up(MouseButton::Left) if self.split.held() => {
                self.split.release();

                return Ok(());
            }
            _ => {}
        }

        self.review.mouse(mouse)
    }

    pub fn apply(&mut self, update: Update) -> Consequence {
        self.transcript.apply(update)
    }

    pub fn transcript(&self) -> &Transcript {
        &self.transcript
    }

    pub fn has_session(&self) -> bool {
        self.session.is_some()
    }

    pub fn attach_session(&mut self, session: Session) {
        self.session = Some(session);
    }

    pub fn pending_activity(&self) -> Option<&Activity> {
        self.activities.first()
    }

    pub fn apply_pending_activity(&mut self) {
        if self.activities.is_empty() {
            return;
        }
        let activity = self.activities.remove(0);
        match activity {
            Activity::Prompt(text) => {
                if let Some(session) = &self.session {
                    session.prompt(text.clone());
                }
                self.transcript.prompted(text);
            }
            Activity::PermissionAnswer { permission, choice } => {
                if let Some(session) = &self.session {
                    session.answer(&permission, &choice);
                }
                self.transcript.answered(&permission, &choice);
            }
        }
    }

    pub fn apply_pending_activities(&mut self) {
        while self.pending_activity().is_some() {
            self.apply_pending_activity();
        }
    }

    pub fn file_paths(&self) -> Vec<std::path::PathBuf> {
        self.review.paths()
    }

    #[expect(
        clippy::missing_errors_doc,
        reason = "the Error enum names the only failure, selecting a changed file"
    )]
    pub fn select_file(&mut self, path: &std::path::Path) -> Result<()> {
        self.review.select_path(path)
    }

    #[expect(
        clippy::missing_errors_doc,
        reason = "the Error enum names the only failure, highlighting the file left selected"
    )]
    pub fn refresh(&mut self, changes: Vec<FileChange>) -> Result<()> {
        self.review.refresh(changes)
    }

    fn ask(&mut self, text: String) {
        self.activities.push(Activity::Prompt(text));
    }

    fn choose(&mut self, digit: char) {
        let index = digit.to_digit(10).expect("the pattern matched an ascii digit") as usize - 1;
        let pending = self.transcript.pending().expect("the guard proved a permission is pending");
        let Some(choice) = pending.choices.get(index) else {
            return;
        };

        let permission = pending.id.clone();
        let choice = choice.id.clone();
        self.activities.push(Activity::PermissionAnswer { permission, choice });
    }
}

fn split(area: Rect, foot: u16) -> (Rect, Rect) {
    let foot = foot.min(area.height);
    let rest = Rect::new(area.x, area.y, area.width, area.height - foot);
    let typed = Rect::new(area.x, area.y + rest.height, area.width, foot);

    (rest, typed)
}

fn color(rgb: Rgb) -> Color {
    Color::Rgb(rgb.red, rgb.green, rgb.blue)
}
