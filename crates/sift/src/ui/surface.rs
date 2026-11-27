//! The whole screen: the review, the status line beneath it, and the palette above it.

use std::path::{Path, PathBuf};

use hints::{Footer, Hint};
use keys::{Key, Keymap, Pending, Pressed, Rebindings};
use picker::{Outcome, Picker};
use ratatui::buffer::Buffer;
use ratatui::crossterm::event::{KeyEvent, MouseEvent};
use ratatui::layout::{Position, Rect};
use ratatui::text::Line;
use ratatui::widgets::Widget;

use crate::command::{ANNOTATIONS, Binding, Catalog, Command, PaletteTarget, ex, file, located};
use crate::git::{Content, FileChange};
use crate::theme::{Theme, Tints};
use crate::ui::{Anchor, Annotation, Control, Icon, Presentation, Result, Review, Selection};

const MOST_REPEATS: usize = 10_000;

/// What the surface is opened with, beyond its changes.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct Opening {
    pub presentation: Presentation,
    pub branch: Option<String>,
    pub comparing: String,
    pub files: Vec<PathBuf>,
    pub bindings: Vec<Binding>,
    pub unbound: Vec<Command>,
    pub keys: Rebindings,
    pub theme: Theme,
}

impl Opening {
    pub fn surface(self, changes: Vec<FileChange>) -> Result<Surface> {
        let base = Catalog::default().unbind(&self.unbound)?.with(self.bindings)?;

        Ok(Surface {
            catalog: base.clone().rebound(&self.keys)?,
            base,
            pending: Pending::default(),
            review: Review::new(changes, self.presentation, self.theme.clone())?,
            palette: Picker::new(self.theme.tints.picker()),
            branch: self.branch,
            comparing: self.comparing,
            files: self.files,
            notice: None,
            annotating: false,
            tints: self.theme.tints,
            area: Rect::default(),
        })
    }
}

/// Everything on screen, and the one place a key is turned into a command.
pub struct Surface {
    review: Review,
    catalog: Catalog,
    base: Catalog,
    pending: Pending,
    palette: Picker<PaletteTarget>,
    branch: Option<String>,
    comparing: String,
    files: Vec<PathBuf>,
    notice: Option<String>,
    annotating: bool,
    tints: Tints,
    area: Rect,
}

impl Surface {
    pub fn key(&mut self, key: KeyEvent) -> Result<Control> {
        self.notice = None;

        if self.palette.is_open() {
            return match self.palette.key(Key::from(key)) {
                Outcome::Typed => {
                    self.suggest();
                    Ok(Control::Continue)
                }
                Outcome::Chosen { target: PaletteTarget::Command(command), .. } => {
                    self.run(command)
                }
                Outcome::Chosen { target: PaletteTarget::Line(line), .. } => {
                    self.review.release_shifted();
                    self.review.go_to_line(line)?;
                    Ok(Control::Continue)
                }
                Outcome::Chosen { target: PaletteTarget::File(path), query } => {
                    let line = located(&query).1;
                    if !self.review.is_changed(&path) {
                        return Ok(Control::Open { path, line });
                    }
                    self.review.select_path(&path, line)?;
                    Ok(Control::Continue)
                }
                Outcome::Closed | Outcome::Nothing => Ok(Control::Continue),
            };
        }

        let holding = self.holding();
        match self.catalog.press(&mut self.pending, key, holding) {
            Pressed::Command { command, count } => self.counted(command, count),
            Pressed::Pending | Pressed::Nothing => Ok(Control::Continue),
        }
    }

    fn suggest(&mut self) {
        let query = self.palette.query().to_owned();
        let commands = self
            .palette
            .entries()
            .iter()
            .any(|entry| matches!(entry.target, PaletteTarget::Command(_)));
        self.palette.search(located(&query).0);
        self.palette.lead(ex(&query).filter(|_| commands).into_iter().collect());
    }

    fn counted(&mut self, command: Command, count: Option<usize>) -> Result<Control> {
        match (command, count) {
            (Command::Top | Command::Bottom, Some(line)) => {
                self.review.release_shifted();
                self.review.go_to_line(line)?;
                Ok(Control::Continue)
            }
            (command, Some(count)) if command.repeats() => {
                let mut control = Control::Continue;
                for _ in 0..count.min(MOST_REPEATS) {
                    control = self.run(command)?;
                    if control != Control::Continue {
                        break;
                    }
                }
                Ok(control)
            }
            (command, _) => self.run(command),
        }
    }

    fn holding(&self) -> &'static [&'static str] {
        if self.review.selecting() { &["selecting"] } else { &[] }
    }

    pub fn mouse(&mut self, mouse: MouseEvent) -> Result<()> {
        self.pending.clear();
        let outside = !self.area.contains(Position::new(mouse.column, mouse.row));
        if outside || self.palette.is_open() {
            return Ok(());
        }

        self.review.mouse(mouse)
    }

    pub fn refresh(&mut self, changes: Vec<FileChange>) -> Result<()> {
        self.review.refresh(changes)
    }

    pub fn replace_files(&mut self, files: Vec<PathBuf>) {
        self.files = files;
    }

    pub fn replace_viewed(&mut self, paths: Vec<PathBuf>) {
        self.review.replace_viewed(paths);
    }

    pub fn peek(&mut self, path: PathBuf, content: &Content, line: Option<usize>) -> Result<()> {
        self.review.peek(path, content, line)
    }

    pub fn peeked(&self) -> Option<&Path> {
        self.review.peeked()
    }

    pub fn notify(&mut self, notice: impl Into<String>) {
        self.notice = Some(notice.into());
    }

    pub fn run(&mut self, command: Command) -> Result<Control> {
        if command.moves() {
            self.review.release_shifted();
        }
        match command {
            Command::Quit => return Ok(Control::Quit),
            Command::Host(name) => return Ok(Control::Host(name)),
            Command::Back => {
                if !self.review.clear_selection() && !self.review.close_peek() {
                    return Ok(Control::Quit);
                }
            }
            Command::Palette => {
                self.palette.open("command", self.shown_catalog().palette_entries());
            }
            Command::GoToLine => {
                self.palette.open("line", self.shown_catalog().palette_entries());
            }
            Command::Copy => {
                if let Some(text) = self.review.text() {
                    return Ok(Control::Copy(text));
                }
            }
            Command::FindFile => {
                let changed = self.review.paths();
                let unchanged = self.files.iter().filter(|path| !changed.contains(path)).cloned();
                let files = changed.iter().cloned().chain(unchanged).map(file);
                self.palette.open("file", files.collect());
            }
            Command::Reload => return Ok(Control::Reload),
            Command::Settings => return Ok(Control::Settings),
            Command::ToggleViewed => return self.review.toggle_viewed(),
            Command::Edit => {
                if let Some(path) = self.review.path() {
                    return Ok(Control::Edit {
                        path: path.to_path_buf(),
                        line: self.review.located().map(|lines| *lines.start()),
                    });
                }
            }
            Command::CopyLocation => {
                if let Some(path) = self.review.path() {
                    let location = match self.review.located() {
                        Some(lines) if lines.start() == lines.end() => {
                            format!("{}:{}", path.display(), lines.start())
                        }
                        Some(lines) => {
                            format!("{}:{}-{}", path.display(), lines.start(), lines.end())
                        }
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

    pub fn keymap(&self) -> &Keymap<Command> {
        self.base.keymap()
    }

    pub fn rebind(&mut self, keys: &Rebindings) -> Result<()> {
        self.catalog = self.base.clone().rebound(keys)?;
        self.pending.clear();

        Ok(())
    }

    pub fn present(&mut self, presentation: Presentation) -> Result<()> {
        self.review.present(presentation)
    }

    pub fn retheme(&mut self, theme: Theme) -> Result<()> {
        self.tints = theme.tints;
        self.palette = Picker::new(theme.tints.picker());
        self.review.retheme(theme)
    }

    pub fn names(&self) -> Vec<&'static str> {
        self.catalog.names().collect()
    }

    pub fn cursor(&self) -> Option<Anchor> {
        self.review.cursor()
    }

    pub fn selection(&self) -> Option<Selection> {
        self.review.selection()
    }

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

    pub fn go_to_annotation(&mut self, id: &str) -> Result<bool> {
        self.review.go_to_annotation(id)
    }

    fn shown_catalog(&self) -> Catalog {
        if self.annotating { self.catalog.clone() } else { self.catalog.without(ANNOTATIONS) }
    }

    pub fn footer(&self) -> Footer {
        let holding = self.holding();
        let catalog = self.shown_catalog();
        let following = catalog.keymap().continuations(&self.pending, holding);
        let styles = self.tints.footer();
        let status = match &self.notice {
            Some(notice) => Line::styled(notice.clone(), styles.key),
            None => Line::raw(self.summary()),
        };
        let hints = if self.palette.is_open() {
            self.palette.hints()
        } else if following.is_empty() {
            catalog.hints(holding)
        } else {
            following.into_iter().map(Hint::from).collect()
        };

        Footer { status, pending: self.pending.to_string(), hints, styles }
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
        if self.review.selecting()
            && let Some(selection) = self.review.selection()
        {
            let (first, last) = (selection.lines.start(), selection.lines.end());
            parts.push(if first == last {
                format!("line {first}")
            } else {
                format!("lines {first}–{last}")
            });
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

impl Widget for &mut Surface {
    fn render(self, area: Rect, buffer: &mut Buffer) {
        self.area = area;
        if area.is_empty() {
            return;
        }
        let status = Rect::new(area.x, area.bottom() - 1, area.width, 1);
        let above = Rect::new(area.x, area.y, area.width, area.height - 1);

        self.review.render(above, buffer);
        self.footer().render(status, buffer);
        self.palette.render(area, buffer);
    }
}
