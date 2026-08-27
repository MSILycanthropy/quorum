//! One worktree, one agent: the diff, the transcript and the keys over both.

use ratatui::Frame;
use ratatui::buffer::Buffer;
use ratatui::crossterm::event::{KeyCode, KeyEvent, MouseEvent};
use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use unicode_width::UnicodeWidthStr;

use crate::color::Rgb;

use crate::agent::{Consequence, Session, Transcript, Update};
use crate::git::FileChange;
use crate::theme::{Tints, TokyoNight};
use crate::ui::{Control, Conversation, Mode, Prompt, Result, Review};

const CONVERSATION: u16 = 44;
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
        Ok(Tab {
            review: Review::new(changes, self.mode, self.wrap, self.tab_width)?,
            conversation: Conversation::default(),
            transcript: Transcript::default(),
            prompt: Prompt::default(),
            session,
            branch: self.branch,
            base: self.base,
            tints: TokyoNight::TINTS,
        })
    }
}

/// A worktree and the agent working in it, as one surface.
pub struct Tab {
    review: Review,
    conversation: Conversation,
    transcript: Transcript,
    prompt: Prompt,
    session: Option<Session>,
    branch: Option<String>,
    base: String,
    tints: Tints,
}

impl Tab {
    pub fn draw(&mut self, frame: &mut Frame) {
        let whole = frame.area();
        let standing = Rect::new(whole.x, whole.y + whole.height - 1, whole.width, 1);
        let area = Rect::new(whole.x, whole.y, whole.width, whole.height - 1);
        self.standing(frame.buffer_mut(), standing);

        if area.width >= TOGETHER {
            let reviewed = Rect::new(area.x, area.y, area.width - CONVERSATION, area.height);
            let beside = Rect::new(area.x + reviewed.width, area.y, CONVERSATION, area.height);

            self.review.draw(frame, reviewed);
            let spoken = self.foot(frame.buffer_mut(), beside);
            self.conversation.draw(frame.buffer_mut(), spoken, &self.transcript);

            return;
        }

        let line = Rect::new(area.x, area.y, area.width, 1.min(area.height));
        let rest = Rect::new(area.x, area.y + line.height, area.width, area.height - line.height);

        self.conversation.status(frame.buffer_mut(), line, &self.transcript);
        let reviewed = self.foot(frame.buffer_mut(), rest);
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
        self.review.mouse(mouse)
    }

    pub fn apply(&mut self, update: Update) -> Consequence {
        self.transcript.apply(update)
    }

    #[expect(
        clippy::missing_errors_doc,
        reason = "the Error enum names the only failure, highlighting the file left selected"
    )]
    pub fn refresh(&mut self, changes: Vec<FileChange>) -> Result<()> {
        self.review.refresh(changes)
    }

    fn ask(&mut self, text: String) {
        if let Some(session) = &self.session {
            session.prompt(text.clone());
        }
        self.transcript.prompted(text);
    }

    fn choose(&mut self, digit: char) {
        let index = digit.to_digit(10).expect("the pattern matched an ascii digit") as usize - 1;
        let pending = self.transcript.pending().expect("the guard proved a permission is pending");
        let Some(choice) = pending.choices.get(index) else {
            return;
        };

        let permission = pending.id.clone();
        let choice = choice.id.clone();
        if let Some(session) = &self.session {
            session.answer(&permission, &choice);
        }
        self.transcript.answered(&permission, &choice);
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
