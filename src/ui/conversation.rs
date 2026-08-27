//! What the agent has said and done, read top to bottom.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use unicode_width::UnicodeWidthStr;

use crate::agent::{Entry, Progress, State, Task, Transcript};
use crate::color::Rgb;
use crate::theme::{Tints, TokyoNight};

const OUTPUT: usize = 6;
const PLAN: usize = 6;

/// The transcript column, pinned to the newest entry.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Conversation {
    pub tints: Tints,
}

impl Default for Conversation {
    fn default() -> Self {
        Self { tints: TokyoNight::TINTS }
    }
}

impl Conversation {
    pub fn draw(&self, buffer: &mut Buffer, area: Rect, transcript: &Transcript) {
        buffer.set_style(area, Style::new().bg(color(self.tints.background)));

        let header =
            Rect::new(area.x + 1, area.y, area.width.saturating_sub(1), 1.min(area.height));
        buffer.set_style(header, Style::new().bg(color(self.tints.filler)));
        buffer.set_stringn(
            header.x + 1,
            header.y,
            "AGENT",
            usize::from(header.width.saturating_sub(2)),
            Style::new().fg(color(self.tints.muted)).bg(color(self.tints.filler)),
        );

        let (glyph, tint) = state(transcript.state(), self.tints);
        buffer.set_string(
            header.x + header.width.saturating_sub(1),
            header.y,
            glyph,
            Style::new().fg(color(tint)).bg(color(self.tints.filler)),
        );

        for y in 0..area.height {
            let ground = if y == 0 { self.tints.filler } else { self.tints.background };
            buffer.set_string(
                area.x,
                area.y + y,
                "│",
                Style::new().fg(color(self.tints.gutter)).bg(color(ground)),
            );
        }

        let area =
            Rect::new(area.x, area.y + header.height, area.width, area.height - header.height);

        let text = Rect::new(area.x + 2, area.y, area.width.saturating_sub(3), area.height);
        if text.width == 0 || text.height == 0 {
            return;
        }

        let lines = self.lines(transcript, usize::from(text.width));
        let height = usize::from(text.height);
        let mut first = lines.len().saturating_sub(height);
        while lines.get(first).is_some_and(|line| line.spans.is_empty()) {
            first += 1;
        }

        for (y, line) in (0..text.height).zip(lines[first..].iter()) {
            buffer.set_line(text.x, text.y + y, line, text.width);
        }
    }

    pub fn planned(&self, transcript: &Transcript) -> u16 {
        let tasks = transcript.plan().len().min(PLAN);

        u16::try_from(tasks).unwrap_or(0)
    }

    pub fn plan(&self, buffer: &mut Buffer, area: Rect, transcript: &Transcript) {
        buffer.set_style(area, Style::new().bg(color(self.tints.background)));

        for (y, task) in (0..area.height).zip(transcript.plan()) {
            buffer.set_line(
                area.x + 2,
                area.y + y,
                &self.task(task, usize::from(area.width.saturating_sub(3))),
                area.width.saturating_sub(3),
            );
        }
    }

    fn task(&self, task: &Task, width: usize) -> Line<'static> {
        let tint = match task.progress {
            Progress::Running => self.tints.foreground,
            _ => self.tints.muted,
        };
        let content: String = task.content.chars().take(width.saturating_sub(2)).collect();

        Line::from(vec![
            Span::styled(
                format!("{} ", glyph(task.progress)),
                Style::new().fg(color(self.mark(task.progress))),
            ),
            Span::styled(content, Style::new().fg(color(tint))),
        ])
    }

    fn mark(&self, progress: Progress) -> Rgb {
        match progress {
            Progress::Pending | Progress::Done => self.tints.muted,
            Progress::Running => self.tints.foreground,
            Progress::Failed => self.tints.attention,
        }
    }

    pub fn status(&self, buffer: &mut Buffer, area: Rect, transcript: &Transcript) {
        let (glyph, tint) = state(transcript.state(), self.tints);
        let title = transcript.title().unwrap_or("no agent");
        let line = Line::from(vec![
            Span::styled(format!("{glyph} "), Style::new().fg(color(tint))),
            Span::styled(title.to_owned(), Style::new().fg(color(self.tints.muted))),
        ]);

        buffer.set_style(area, Style::new().bg(color(self.tints.filler)));
        buffer.set_line(area.x + 1, area.y, &line, area.width.saturating_sub(1));
    }

    fn lines(&self, transcript: &Transcript, width: usize) -> Vec<Line<'static>> {
        let mut lines = Vec::new();
        let mut previous = None;

        for entry in transcript.entries() {
            if !lines.is_empty() && !consecutive_calls(previous, entry) {
                lines.push(Line::default());
            }
            lines.extend(self.entry(entry, width));
            previous = Some(entry);
        }

        lines
    }

    fn entry(&self, entry: &Entry, width: usize) -> Vec<Line<'static>> {
        match entry {
            Entry::Prompt(text) => block("› ", text, width, self.tints.foreground),
            Entry::Said(text) => block("", text, width, self.tints.foreground),
            Entry::Thought(text) => block("", text, width, self.tints.muted),
            Entry::Ran(call) => {
                let mut lines = marked(
                    glyph(call.progress),
                    self.mark(call.progress),
                    &call.title,
                    width,
                    self.tints.muted,
                );
                for line in call.output.iter().take(OUTPUT) {
                    lines.extend(block("  ", line, width, self.tints.gutter));
                }
                if call.output.len() > OUTPUT {
                    let more = format!("… {} more lines", call.output.len() - OUTPUT);
                    lines.extend(block("  ", &more, width, self.tints.gutter));
                }

                lines
            }
            Entry::Answered(choice) => {
                marked("↳", self.tints.muted, choice, width, self.tints.muted)
            }
            Entry::Failed(message) => {
                marked("✗", self.tints.attention, message, width, self.tints.attention)
            }
            Entry::Asked(permission) => {
                let mut lines = marked(
                    "!",
                    self.tints.attention,
                    &permission.title,
                    width,
                    self.tints.foreground,
                );
                for (index, choice) in permission.choices.iter().enumerate() {
                    lines.push(Line::from(vec![
                        Span::styled(
                            format!("  {} ", index + 1),
                            Style::new().fg(color(self.tints.attention)),
                        ),
                        Span::styled(
                            choice.name.clone(),
                            Style::new().fg(color(self.tints.foreground)),
                        ),
                    ]));
                }

                lines
            }
        }
    }
}

fn state(state: State, tints: Tints) -> (&'static str, Rgb) {
    match state {
        State::Working => ("●", tints.foreground),
        State::Blocked => ("!", tints.attention),
        State::Idle => ("○", tints.muted),
    }
}

fn glyph(progress: Progress) -> &'static str {
    match progress {
        Progress::Pending => "·",
        Progress::Running => "▸",
        Progress::Done => "✓",
        Progress::Failed => "✗",
    }
}

fn consecutive_calls(previous: Option<&Entry>, entry: &Entry) -> bool {
    matches!((previous, entry), (Some(Entry::Ran(_)), Entry::Ran(_)))
}

fn marked(glyph: &str, mark: Rgb, text: &str, width: usize, tint: Rgb) -> Vec<Line<'static>> {
    let mut lines = block("  ", text, width, tint);
    let Some(first) = lines.first_mut() else {
        return lines;
    };

    first.spans[0] = Span::styled(format!("{glyph} "), Style::new().fg(color(mark)));

    lines
}

fn block(marker: &str, text: &str, width: usize, tint: Rgb) -> Vec<Line<'static>> {
    let indent = marker.width();
    let room = width.saturating_sub(indent).max(1);
    let style = Style::new().fg(color(tint));
    let blank = " ".repeat(indent);

    wrapped(text, room)
        .into_iter()
        .enumerate()
        .map(|(index, run)| {
            let lead = if index == 0 { marker.to_owned() } else { blank.clone() };

            Line::from(vec![
                Span::styled(lead, Style::new().fg(color(tint))),
                Span::styled(run, style),
            ])
        })
        .collect()
}

fn wrapped(text: &str, width: usize) -> Vec<String> {
    let mut lines = Vec::new();
    for paragraph in text.split('\n') {
        let mut run = String::new();
        for word in paragraph.split_inclusive(' ') {
            if !run.is_empty() && run.width() + word.trim_end().width() > width {
                lines.push(std::mem::take(&mut run).trim_end().to_owned());
            }
            run.push_str(word);
        }
        lines.push(run.trim_end().to_owned());
    }

    lines
}

fn color(rgb: Rgb) -> Color {
    Color::Rgb(rgb.red, rgb.green, rgb.blue)
}
