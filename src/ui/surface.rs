//! The whole screen: the review, the status line beneath it, and the overlays above it.

use std::path::{Path, PathBuf};

use ratatui::Frame;
use ratatui::buffer::Buffer;
use ratatui::crossterm::event::{KeyEvent, MouseEvent};
use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use unicode_width::UnicodeWidthStr;

use crate::color::Rgb;
use crate::command::{Binding, Catalog, Category, Command, PaletteEntry, PaletteTarget};
use crate::git::{Content, FileChange};
use crate::theme::{Tints, TokyoNight};
use crate::ui::{
    Anchor, Annotation, Control, Help, Icon, Mode, Palette, PaletteAction, Result, Review,
};

/// What the surface is opened with, beyond its changes.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Opening {
    pub mode: Mode,
    pub wrap: bool,
    pub tab_width: usize,
    pub branch: Option<String>,
    pub comparing: String,
    pub files: Vec<PathBuf>,
    pub bindings: Vec<Binding>,
    pub unbound: Vec<Command>,
}

impl Default for Opening {
    fn default() -> Self {
        Self {
            mode: Mode::default(),
            wrap: false,
            tab_width: 4,
            branch: None,
            comparing: String::new(),
            files: Vec::new(),
            bindings: Vec::new(),
            unbound: Vec::new(),
        }
    }
}

impl Opening {
    #[expect(
        clippy::missing_errors_doc,
        reason = "the Error enum names the failures, a refused unbinding or host binding and highlighting the selected file"
    )]
    pub fn surface(self, changes: Vec<FileChange>) -> Result<Surface> {
        Ok(Surface {
            catalog: Catalog::default().unbind(&self.unbound)?.with(self.bindings)?,
            review: Review::new(changes, self.mode, self.wrap, self.tab_width)?,
            palette: Palette::default(),
            help: Help::default(),
            branch: self.branch,
            comparing: self.comparing,
            files: self.files,
            notice: None,
            annotating: false,
            tints: TokyoNight::TINTS,
        })
    }
}

/// Everything on screen, and the one place a key is turned into a command.
pub struct Surface {
    review: Review,
    catalog: Catalog,
    palette: Palette,
    help: Help,
    branch: Option<String>,
    comparing: String,
    files: Vec<PathBuf>,
    notice: Option<String>,
    annotating: bool,
    tints: Tints,
}

impl Surface {
    pub fn draw(&mut self, frame: &mut Frame) {
        let whole = frame.area();
        if whole.height == 0 {
            return;
        }
        let status = Rect::new(whole.x, whole.y + whole.height - 1, whole.width, 1);
        let area = Rect::new(whole.x, whole.y, whole.width, whole.height - 1);

        self.review.draw(frame, area);
        self.status(frame.buffer_mut(), status);
        self.help.draw(frame, &self.shown_catalog());
        self.palette.draw(frame);
    }

    #[expect(
        clippy::missing_errors_doc,
        reason = "the Error enum names the only failure, highlighting the file moved to"
    )]
    pub fn key(&mut self, key: KeyEvent) -> Result<Control> {
        self.notice = None;

        if self.help.is_open() {
            self.help.close();
            return Ok(Control::Continue);
        }

        if self.palette.is_open() {
            return match self.palette.key(key) {
                PaletteAction::Execute(PaletteTarget::Command(command)) => self.run(command),
                PaletteAction::Execute(PaletteTarget::File { path, line }) => {
                    if !self.review.is_changed(&path) {
                        return Ok(Control::Open { path, line });
                    }
                    self.review.select_path(&path, line)?;
                    Ok(Control::Continue)
                }
                PaletteAction::Close | PaletteAction::None => Ok(Control::Continue),
            };
        }

        match self.catalog.command(key) {
            Some(command) => self.run(command),
            None => Ok(Control::Continue),
        }
    }

    #[expect(
        clippy::missing_errors_doc,
        reason = "the Error enum names the only failure, highlighting the file clicked on"
    )]
    pub fn mouse(&mut self, mouse: MouseEvent) -> Result<()> {
        if self.help.is_open() || self.palette.is_open() {
            return Ok(());
        }

        self.review.mouse(mouse)
    }

    #[expect(
        clippy::missing_errors_doc,
        reason = "the Error enum names the only failure, highlighting the file left selected"
    )]
    pub fn refresh(&mut self, changes: Vec<FileChange>) -> Result<()> {
        self.review.refresh(changes)
    }

    pub fn replace_files(&mut self, files: Vec<PathBuf>) {
        self.files = files;
    }

    pub fn replace_viewed(&mut self, paths: Vec<PathBuf>) {
        self.review.replace_viewed(paths);
    }

    #[expect(
        clippy::missing_errors_doc,
        reason = "the Error enum names the only failure, highlighting the file opened"
    )]
    pub fn peek(&mut self, path: PathBuf, content: &Content, line: Option<usize>) -> Result<()> {
        self.review.peek(path, content, line)
    }

    pub fn peeked(&self) -> Option<&Path> {
        self.review.peeked()
    }

    pub fn notify(&mut self, notice: impl Into<String>) {
        self.notice = Some(notice.into());
    }

    #[expect(
        clippy::missing_errors_doc,
        reason = "the Error enum names the only failure, highlighting the file moved to"
    )]
    pub fn run(&mut self, command: Command) -> Result<Control> {
        match command {
            Command::Quit => return Ok(Control::Quit),
            Command::Host(name) => return Ok(Control::Host(name)),
            Command::Back => {
                if !self.review.close_peek() {
                    return Ok(Control::Quit);
                }
            }
            Command::Help => self.help.toggle(),
            Command::Palette => {
                self.palette.open("command", self.shown_catalog().palette_entries());
            }
            Command::FindFile => {
                let changed = self.review.paths();
                let unchanged = self.files.iter().filter(|path| !changed.contains(path)).cloned();
                let files = changed.iter().cloned().chain(unchanged).map(PaletteEntry::file);
                self.palette.open("file", files.collect());
            }
            Command::Reload => return Ok(Control::Reload),
            Command::ToggleViewed => return self.review.toggle_viewed(),
            Command::Edit => {
                if let Some(path) = self.review.path() {
                    return Ok(Control::Edit {
                        path: path.to_path_buf(),
                        line: self.review.line(),
                    });
                }
            }
            Command::CopyLocation => {
                if let Some(path) = self.review.path() {
                    let location = match self.review.line() {
                        Some(line) => format!("{}:{line}", path.display()),
                        None => path.display().to_string(),
                    };
                    return Ok(Control::Copy(location));
                }
            }
            Command::CopyPath => {
                if let Some(path) = self.review.path() {
                    return Ok(Control::Copy(path.display().to_string()));
                }
            }
            command => self.review.run(command)?,
        }

        Ok(Control::Continue)
    }

    pub fn paths(&self) -> Vec<PathBuf> {
        self.review.paths()
    }

    pub fn cursor(&self) -> Option<Anchor> {
        self.review.cursor()
    }

    #[expect(
        clippy::missing_errors_doc,
        reason = "the Error enum names the failures, a repeated id and highlighting the file on screen"
    )]
    pub fn annotate(&mut self, annotations: Vec<Annotation>) -> Result<()> {
        self.review.annotate(annotations)?;
        self.annotating = true;

        Ok(())
    }

    pub fn annotation(&self) -> Option<&Annotation> {
        self.review.annotation()
    }

    pub fn unplaced(&self) -> Vec<&Annotation> {
        self.review.unplaced()
    }

    #[expect(
        clippy::missing_errors_doc,
        reason = "the Error enum names the only failure, highlighting the file moved to"
    )]
    pub fn go_to_annotation(&mut self, id: &str) -> Result<bool> {
        self.review.go_to_annotation(id)
    }

    fn shown_catalog(&self) -> Catalog {
        if self.annotating {
            self.catalog.clone()
        } else {
            self.catalog.without(Category::Annotations)
        }
    }

    fn status(&self, buffer: &mut Buffer, area: Rect) {
        let base = Style::new().bg(color(self.tints.filler));
        let muted = base.fg(color(self.tints.muted));
        let key = base.fg(color(self.tints.attention));
        buffer.set_style(area, base);

        let left = self.notice.clone().unwrap_or_else(|| self.summary());
        let left_style = if self.notice.is_some() { key } else { muted };

        let room = usize::from(area.width);
        let mut hints: Vec<(String, &str)> = self.catalog.hints().collect();
        while hints.len() > 1 && left.width() + hinted(&hints) + 4 > room {
            let Some(dropped) = hints.iter().rposition(|(_, hint)| *hint != "help") else {
                break;
            };
            hints.remove(dropped);
        }
        let hints_width = hinted(&hints);
        if hints_width + 4 > room {
            buffer.set_stringn(area.x + 1, area.y, &left, room.saturating_sub(2), left_style);
            return;
        }

        let left_room = room - hints_width - 4;
        buffer.set_stringn(area.x + 1, area.y, &left, left_room, left_style);

        let mut x = area.x + area.width - 1 - u16::try_from(hints_width).unwrap_or(0);
        for (index, (keys, hint)) in hints.into_iter().enumerate() {
            if index > 0 {
                x += 2;
            }
            (x, _) = buffer.set_stringn(x, area.y, &keys, usize::from(area.width), key);
            (x, _) = buffer.set_stringn(x, area.y, format!(" {hint}"), usize::MAX, muted);
        }
    }

    fn summary(&self) -> String {
        let branch = self.branch.clone().unwrap_or_else(|| "detached".to_owned());
        let mut parts = vec![
            format!("{} {branch}", Icon::BRANCH),
            self.comparing.clone(),
            self.review.counted(),
        ];
        match self.review.hunk() {
            (_, 0) => {}
            (Some(current), total) => parts.push(format!("hunk {current}/{total}")),
            (None, total) => parts.push(format!("{total} hunks")),
        }
        match self.review.annotated() {
            (_, 0) => {}
            (Some(current), total) => parts.push(format!("annotation {current}/{total}")),
            (None, 1) => parts.push("1 annotation".to_owned()),
            (None, total) => parts.push(format!("{total} annotations")),
        }
        parts.push(self.review.laid());

        parts.into_iter().filter(|part| !part.is_empty()).collect::<Vec<_>>().join(" · ")
    }
}

fn hinted(hints: &[(String, &str)]) -> usize {
    hints
        .iter()
        .map(|(keys, hint)| keys.width() + hint.width() + 3)
        .sum::<usize>()
        .saturating_sub(2)
}

fn color(rgb: Rgb) -> Color {
    Color::Rgb(rgb.red, rgb.green, rgb.blue)
}
