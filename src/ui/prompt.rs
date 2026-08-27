//! The input at the foot of the conversation, dimmed until it is entered.

use ratatui::buffer::Buffer;
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::widgets::Widget;
use ratatui_textarea::{TextArea, WrapMode};
use unicode_width::UnicodeWidthStr;

use crate::agent::{Command, Transcript, Usage};
use crate::color::Rgb;
use crate::theme::{Tints, TokyoNight};

const TALLEST: u16 = 9;
const SUGGESTIONS: usize = 6;
const NEWLINE: &str = "shift+enter for a newline";

/// What you are typing to the agent, and whether it has the keyboard.
#[derive(Clone, Debug)]
pub struct Prompt {
    typing: TextArea<'static>,
    entered: bool,
    pub tints: Tints,
}

impl Default for Prompt {
    fn default() -> Self {
        Self { typing: blank(), entered: false, tints: TokyoNight::TINTS }
    }
}

impl Prompt {
    pub fn entered(&self) -> bool {
        self.entered
    }

    pub fn enter(&mut self) {
        self.entered = true;
    }

    pub fn key(&mut self, key: KeyEvent) -> Option<String> {
        match key.code {
            KeyCode::Esc => self.entered = false,
            KeyCode::Char('j') if key.modifiers == KeyModifiers::CONTROL => {
                self.typing.insert_newline();
            }
            KeyCode::Enter if key.modifiers.intersects(KeyModifiers::SHIFT | KeyModifiers::ALT) => {
                self.typing.insert_newline();
            }
            KeyCode::Enter => {
                let written = self.written();
                if written.trim().is_empty() {
                    return None;
                }

                self.entered = false;
                self.typing = blank();

                return Some(written);
            }
            _ => {
                self.typing.input(key);
            }
        }

        None
    }

    pub fn suggestions(&self, transcript: &Transcript) -> Vec<Command> {
        let Some(stem) = self.stem() else {
            return Vec::new();
        };

        transcript
            .commands()
            .iter()
            .filter(|command| command.name.starts_with(&stem))
            .take(SUGGESTIONS)
            .cloned()
            .collect()
    }

    pub fn suggest(&self, buffer: &mut Buffer, area: Rect, suggestions: &[Command]) {
        buffer.set_style(area, Style::new().bg(color(self.tints.background)));

        for (y, command) in (0..area.height).zip(suggestions) {
            let name = format!("/{}", command.name);
            let room = usize::from(area.width.saturating_sub(4));
            buffer.set_stringn(
                area.x + 2,
                area.y + y,
                &name,
                room,
                Style::new().fg(color(self.tints.foreground)).bg(color(self.tints.background)),
            );
            buffer.set_stringn(
                area.x + 2 + u16::try_from(name.width() + 1).unwrap_or(0),
                area.y + y,
                &command.description,
                room.saturating_sub(name.width() + 1),
                Style::new().fg(color(self.tints.muted)).bg(color(self.tints.background)),
            );
        }
    }

    pub fn height(&self, width: u16) -> u16 {
        let room = usize::from(width.saturating_sub(4)).max(1);
        let rows: usize =
            self.typing.lines().iter().map(|line| line.width().div_ceil(room).max(1)).sum();

        u16::try_from(rows).unwrap_or(1).max(1).saturating_add(2).min(TALLEST)
    }

    pub fn draw(&mut self, buffer: &mut Buffer, area: Rect, transcript: &Transcript) {
        if area.height < 3 || area.width < 4 {
            return;
        }

        buffer.set_style(area, Style::new().bg(color(self.tints.background)));
        self.rule(buffer, Rect::new(area.x, area.y, area.width, 1));
        self.dress();

        let typed = Rect::new(area.x + 3, area.y + 1, area.width - 4, area.height - 2);
        buffer.set_string(
            area.x + 1,
            area.y + 1,
            "›",
            Style::new()
                .fg(color(if self.entered { self.tints.attention } else { self.tints.gutter }))
                .bg(color(self.tints.background)),
        );
        Widget::render(&self.typing, typed, buffer);

        self.foot(buffer, Rect::new(area.x, area.y + area.height - 1, area.width, 1), transcript);
    }

    fn written(&self) -> String {
        self.typing.lines().join("\n")
    }

    fn dress(&mut self) {
        let ground = color(self.tints.background);
        let (placeholder, cursor) = if self.entered {
            ("", Style::new().bg(color(self.tints.foreground)).fg(ground))
        } else {
            ("i to prompt", Style::new().bg(ground))
        };

        self.typing.set_wrap_mode(WrapMode::WordOrGlyph);
        self.typing.set_style(Style::new().fg(color(self.tints.foreground)).bg(ground));
        self.typing.set_cursor_line_style(Style::new().remove_modifier(Modifier::all()));
        self.typing.set_placeholder_text(placeholder);
        self.typing.set_placeholder_style(Style::new().fg(color(self.tints.muted)).bg(ground));
        self.typing.set_cursor_style(cursor);
    }

    fn stem(&self) -> Option<String> {
        if !self.entered {
            return None;
        }

        let [line] = self.typing.lines() else {
            return None;
        };

        line.strip_prefix('/').filter(|stem| !stem.contains(' ')).map(str::to_owned)
    }

    fn rule(&self, buffer: &mut Buffer, area: Rect) {
        let ruled = Style::new().fg(color(self.tints.gutter)).bg(color(self.tints.background));
        buffer.set_string(area.x, area.y, "─".repeat(usize::from(area.width)), ruled);
    }

    fn foot(&self, buffer: &mut Buffer, area: Rect, transcript: &Transcript) {
        let mode = transcript.mode();
        let left = mode
            .map(str::to_owned)
            .or_else(|| self.entered.then(|| NEWLINE.to_owned()))
            .unwrap_or_default();
        let right = right(transcript);
        let width = usize::from(area.width);
        let together = left.width() + right.width();
        let tint = if mode.is_some() { self.tints.attention } else { self.tints.gutter };

        if !left.is_empty() && left.width() + 2 <= width {
            buffer.set_string(
                area.x + 1,
                area.y,
                &left,
                Style::new().fg(color(tint)).bg(color(self.tints.background)),
            );
        }
        if !right.is_empty() && together + 3 <= width {
            let at = area.width - u16::try_from(right.width()).unwrap_or(0) - 1;
            buffer.set_string(
                area.x + at,
                area.y,
                &right,
                Style::new().fg(color(self.tints.muted)).bg(color(self.tints.background)),
            );
        }
    }
}

fn blank() -> TextArea<'static> {
    let mut typing = TextArea::default();
    typing.remove_line_number();

    typing
}

fn right(transcript: &Transcript) -> String {
    let mut parts: Vec<String> =
        transcript.settings().iter().map(|setting| setting.value.clone()).collect();
    if let Some(usage) = transcript.usage() {
        parts.push(spent(usage));
    }
    parts.join(" · ")
}

fn spent(usage: &Usage) -> String {
    let context = format!("{}/{}", thousands(usage.used), thousands(usage.size));
    match &usage.cost {
        Some(cost) => format!("{context} · {:.2} {}", cost.amount, cost.currency),
        None => context,
    }
}

fn thousands(count: u64) -> String {
    if count >= 1_000 {
        return format!("{}k", count / 1_000);
    }

    count.to_string()
}

fn color(rgb: Rgb) -> Color {
    Color::Rgb(rgb.red, rgb.green, rgb.blue)
}
