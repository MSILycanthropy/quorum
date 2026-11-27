//! The Sift binary.
#![expect(
    clippy::result_large_err,
    reason = "gix builds large error types, and every call here is already bound by disk"
)]

use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Sender, channel};
use std::time::Duration;

use clap::Parser;
use hints::Footer;
use host::{Event, Session};
use hue::Palette;
use keys::{Key, Keymap, Pending, Pressed, Rebindings};
use picker::{Outcome, Picker};
use ratatui::Frame;
use ratatui::crossterm::event::{KeyEvent, MouseEvent};
use ratatui::layout::{Constraint, Layout};
use ratatui::text::Line;
use settings::{Change, Page, Values};
use sift::config::{Config, File};
use sift::diff::{FileDiff, Source};
use sift::frame::{self, Action, Target};
use sift::git::{Comparison, Content, FileChange, Repository, Snapshot, Status, looks_generated};
use sift::syntax::Syntax;
use sift::theme::{Scheme, TerminalColors, Theme, Tints};
use sift::ui::{Request, Review};

#[derive(Debug, thiserror::Error)]
enum Error {
    #[error(transparent)]
    Git(Box<sift::git::Error>),
    #[error(transparent)]
    Ui(#[from] sift::ui::Error),
    #[error(transparent)]
    Syntax(#[from] sift::syntax::Error),
    #[error(transparent)]
    Config(#[from] config::Error),
    #[error(transparent)]
    Keys(#[from] keys::Error),
    #[error(transparent)]
    Frame(#[from] sift::frame::Error),
    #[error(transparent)]
    Host(#[from] host::Error),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error("{0}")]
    Usage(String),
}

impl From<sift::git::Error> for Error {
    fn from(error: sift::git::Error) -> Self {
        Self::Git(Box::new(error))
    }
}

type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, Parser)]
#[command(
    version,
    about = "A terminal diff viewer over git.",
    after_help = "Examples:\n  \
        sift                  working tree against HEAD\n  \
        sift --staged         what is staged, against HEAD\n  \
        sift main             working tree against main\n  \
        sift main..feature    main against feature\n  \
        sift main...feature   what feature did since it left main\n  \
        sift --show HEAD~2    one commit against its parent\n  \
        sift -- src/          only paths under src/\n  \
        sift --watch          follow along while something else edits\n\n\
        Settings are read from ~/.config/sift/config.toml, and their flags override them.\n\
        Press C-, or type :settings inside for every key."
)]
struct Arguments {
    #[arg(num_args = 0..=2, value_name = "REVISION", help = "One revision, A..B, A...B, or two revisions")]
    revisions: Vec<String>,
    #[arg(last = true, value_name = "PATH", help = "Only show changes under these paths")]
    paths: Vec<PathBuf>,
    #[arg(long, visible_alias = "cached", help = "Compare the index instead of the working tree")]
    staged: bool,
    #[arg(long, value_name = "REVISION", help = "Show one commit against its parent", conflicts_with_all = ["revisions", "staged"])]
    show: Option<String>,
    #[arg(long, num_args = 2, help = "Compare two files outside any repository", value_names = ["OLD", "NEW"], conflicts_with_all = ["revisions", "staged", "show"])]
    no_index: Vec<PathBuf>,
    #[arg(long, help = "Re-read the diff from git whenever it changes")]
    watch: bool,
    #[arg(
        long,
        default_value_t = 1000,
        value_name = "MILLISECONDS",
        help = "How often --watch checks git"
    )]
    interval: u64,
    #[command(flatten, next_help_heading = "Settings, also keys under [diff] in config.toml")]
    config: Config,
}

fn main() -> ExitCode {
    match view(&Arguments::parse()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("sift: {error}");
            ExitCode::FAILURE
        }
    }
}

/// Everything the loop wakes for.
enum Message {
    Terminal(Event),
    Tick,
}

impl From<Event> for Message {
    fn from(event: Event) -> Self {
        Self::Terminal(event)
    }
}

fn view(arguments: &Arguments) -> Result<()> {
    let File { diff: config, keys } = config(arguments)?;
    let base = frame::keymap()?;
    let keymap = frame::rebound(&base, &keys)?;
    keys.unknown(keymap.names())?;
    let subject = Subject::open(arguments)?;
    let slots = match config.theme.unwrap_or_default() {
        Scheme::Terminal => TerminalColors::SLOTS,
        Scheme::TokyoNight => &[],
    };

    let (sender, messages) = channel();
    let pending = Arc::new(AtomicBool::new(false));
    if arguments.watch {
        let interval = Duration::from_millis(arguments.interval.max(100));
        tick(sender.clone(), Arc::clone(&pending), interval);
    }
    let mut session = Session::open(slots, sender)?;

    let theme = theme(&config, session.palette())?;
    let mut shown = subject.changes()?;
    let mut screen = Screen {
        palette: Picker::new(theme.tints.picker()),
        tints: theme.tints,
        review: Review::new(shown.clone(), config.presentation(), theme)?,
        base,
        keymap,
        pending: Pending::default(),
        notice: None,
        files: subject.files()?,
        branch: subject.branch()?,
        comparing: subject.describe(),
    };

    session.draw(|frame| screen.draw(frame))?;
    let mut settings: Option<Settings> = None;

    while let Ok(message) = messages.recv() {
        match message {
            Message::Terminal(Event::Key(key)) if settings.is_some() => {
                if let Some(open) = &mut settings
                    && let Some(change) = open.page.key(Key::from(key))
                    && !open.change(change, arguments, &mut screen, session.palette())?
                {
                    settings = None;
                }
            }
            Message::Terminal(Event::Key(key)) => match screen.key(key)? {
                Asked::Nothing
                | Asked::Action(
                    Action::Diff(_) | Action::Palette | Action::FindFile | Action::GoToLine,
                ) => {}
                Asked::Request(request) => answer(request, &mut screen, &subject, &mut session)?,
                Asked::Action(Action::Quit) => return Ok(()),
                Asked::Action(Action::Settings) => {
                    settings = Some(Settings::open(arguments, &screen, session.palette())?);
                }
                Asked::Action(Action::Reload) => {
                    reload(&mut screen, &subject, &mut shown)?;
                    screen.notice = Some("reloaded".to_owned());
                }
                Asked::Action(Action::Edit) => {
                    if let Some(location) = screen.review.location() {
                        let path = subject.root().join(&location.path);
                        let line = location.lines.map(|lines| *lines.start());
                        match session.suspended(|| edit(&path, line))? {
                            Ok(()) => {
                                reload(&mut screen, &subject, &mut shown)?;
                            }
                            Err(error) => screen.notice = Some(error),
                        }
                    }
                }
            },
            Message::Terminal(Event::Mouse(mouse)) if settings.is_some() => {
                if let Some(open) = &mut settings
                    && let Some(change) = open.page.mouse(mouse)
                    && !open.change(change, arguments, &mut screen, session.palette())?
                {
                    settings = None;
                }
            }
            Message::Terminal(Event::Mouse(mouse)) => screen.mouse(mouse)?,
            Message::Terminal(Event::Resize | Event::Paste(_)) => {}
            Message::Tick => {
                pending.store(false, Ordering::Release);
                if !reload(&mut screen, &subject, &mut shown).unwrap_or(false) {
                    continue;
                }
            }
        }
        session.draw(|frame| match &mut settings {
            Some(open) => frame.render_widget(&mut open.page, frame.area()),
            None => screen.draw(frame),
        })?;
    }

    Ok(())
}

/// What a key asks of the loop: something the review needs, or one of Sift's
/// own actions that reaches past the screen.
enum Asked {
    Nothing,
    Request(Request),
    Action(Action),
}

/// Everything on screen: the review, the palette over it, the footer under
/// it, and the keys that reach them.
struct Screen {
    review: Review,
    palette: Picker<Target>,
    base: Keymap<Action>,
    keymap: Keymap<Action>,
    pending: Pending,
    notice: Option<String>,
    files: Vec<PathBuf>,
    branch: Option<String>,
    comparing: String,
    tints: Tints,
}

impl Screen {
    fn key(&mut self, key: KeyEvent) -> Result<Asked> {
        self.notice = None;
        if self.palette.is_open() {
            return match self.palette.key(Key::from(key)) {
                Outcome::Typed => {
                    let query = self.palette.query().to_owned();
                    self.palette.search(frame::located(&query).0);
                    self.palette.lead(frame::leading(&query, self.palette.entries()));
                    Ok(Asked::Nothing)
                }
                Outcome::Chosen { target, query } => self.chosen(target, &query),
                Outcome::Closed | Outcome::Nothing => Ok(Asked::Nothing),
            };
        }

        let holding = self.review.holding();
        match self.keymap.press(&mut self.pending, Key::from(key), &holding) {
            Pressed::Command { command, count } => self.act(command, count),
            Pressed::Pending | Pressed::Nothing => Ok(Asked::Nothing),
        }
    }

    fn act(&mut self, action: Action, count: Option<usize>) -> Result<Asked> {
        let holding = self.review.holding();
        match action {
            Action::Diff(command) => {
                return Ok(Asked::Request(self.review.repeated(command, count)?));
            }
            Action::Palette => {
                self.palette.open("command", frame::commands(&self.keymap, &holding));
            }
            Action::GoToLine => {
                self.palette.open("line", frame::commands(&self.keymap, &holding));
            }
            Action::FindFile => {
                self.palette.open("file", frame::files(self.review.paths(), &self.files));
            }
            action @ (Action::Quit | Action::Reload | Action::Settings | Action::Edit) => {
                return Ok(Asked::Action(action));
            }
        }

        Ok(Asked::Nothing)
    }

    fn chosen(&mut self, target: Target, query: &str) -> Result<Asked> {
        match target {
            Target::Action(action) => self.act(action, None),
            Target::Line(line) => {
                self.review.go_to_line(line)?;
                Ok(Asked::Nothing)
            }
            Target::File(path) => {
                Ok(Asked::Request(self.review.show(&path, frame::located(query).1)?))
            }
        }
    }

    fn mouse(&mut self, mouse: MouseEvent) -> Result<()> {
        self.pending.clear();
        if self.palette.is_open() {
            return Ok(());
        }

        Ok(self.review.mouse(mouse)?)
    }

    fn rebind(&mut self, keys: &Rebindings) -> Result<()> {
        self.keymap = frame::rebound(&self.base, keys)?;
        self.pending.clear();

        Ok(())
    }

    fn retheme(&mut self, theme: Theme) -> Result<()> {
        self.tints = theme.tints;
        self.palette = Picker::new(theme.tints.picker());

        Ok(self.review.retheme(theme)?)
    }

    fn footer(&self) -> Footer {
        let holding = self.review.holding();
        let status = frame::status(self.branch.as_deref(), &self.comparing, &self.review.status());
        let footer = Footer {
            status: Line::raw(status),
            notice: self.notice.clone(),
            styles: self.tints.footer(),
            ..Footer::picking(&self.keymap, &self.pending, &holding, &frame::HINTED)
        };

        self.palette.hinting(footer)
    }

    fn draw(&mut self, frame: &mut Frame) {
        let [above, bottom] =
            Layout::vertical([Constraint::Min(0), Constraint::Length(1)]).areas(frame.area());
        frame.render_widget(&mut self.review, above);
        frame.render_widget(&self.footer(), bottom);
        frame.render_widget(&self.palette, frame.area());
    }
}

fn answer(
    request: Request,
    screen: &mut Screen,
    subject: &Subject,
    session: &mut Session,
) -> Result<()> {
    match request {
        Request::Copy(text) => {
            screen.notice = Some(match session.copy(&text) {
                Ok(()) => format!("copied {text}"),
                Err(error) => error.to_string(),
            });
        }
        Request::Open { path, line } => match subject.file(&path) {
            Ok(content) => screen.review.peek(path, &content, line)?,
            Err(error) => screen.notice = Some(error.to_string()),
        },
        Request::Nothing | Request::Viewed(_) | Request::Unviewed(_) => {}
    }

    Ok(())
}

/// The settings page while it is open, and the settings last applied to the
/// diff under it.
struct Settings {
    page: Page<Action>,
    applied: File,
}

impl Settings {
    fn open(arguments: &Arguments, screen: &Screen, palette: &Palette) -> Result<Self> {
        let applied = config(arguments)?;
        let tints = theme(&applied.diff, palette).unwrap_or_default().tints;
        let page = Page::new(
            vec![sift::config::section()],
            values(arguments)?,
            screen.base.clone(),
            applied.keys.clone(),
            tints.page(),
        );

        Ok(Self { page, applied })
    }

    fn change(
        &mut self,
        change: Change,
        arguments: &Arguments,
        screen: &mut Screen,
        palette: &Palette,
    ) -> Result<bool> {
        match change {
            Change::Closed => return Ok(false),
            Change::Setting { path, value } => match written(&path, value.as_ref()) {
                Ok(()) => self.apply(arguments, screen, palette)?,
                Err(error) => self.page.notify(error.to_string()),
            },
            Change::Keys(keys) => match screen.rebind(&keys) {
                Ok(()) => {
                    if let Err(error) = rebound(&self.applied.keys, &keys) {
                        self.page.notify(error.to_string());
                    }
                    self.applied.keys = keys;
                }
                Err(error) => self.page.refuse(error.to_string()),
            },
        }

        Ok(true)
    }

    fn apply(
        &mut self,
        arguments: &Arguments,
        screen: &mut Screen,
        palette: &Palette,
    ) -> Result<()> {
        let file = match config(arguments) {
            Ok(file) => file,
            Err(error) => {
                self.page.notify(error.to_string());
                return Ok(());
            }
        };
        screen.review.present(file.diff.presentation())?;
        let looks = |config: &Config| (config.theme, config.syntax_theme.clone());
        if looks(&file.diff) != looks(&self.applied.diff) {
            match theme(&file.diff, palette) {
                Ok(theme) => screen.retheme(theme)?,
                Err(error) => self.page.notify(error.to_string()),
            }
            if file.diff.theme == Some(Scheme::Terminal) && *palette == Palette::default() {
                self.page.notify("the terminal's colours are read as Sift starts, so they show from the next run");
            }
        }
        self.applied.diff = file.diff;
        self.page.update(values(arguments)?);

        Ok(())
    }
}

fn values(arguments: &Arguments) -> Result<Values> {
    let file = config::Settings::of::<File>("sift")?;
    let flags =
        config::Settings::serialized(&File { diff: arguments.config.clone(), ..File::default() })?;
    let written = file.table().clone();
    let flagged = flags.table().clone();
    let File { diff, .. } = file.over(flags).parse()?;
    let effective =
        config::Settings::serialized(&File { diff: diff.resolved(), ..File::default() })?;

    let defaults = config::Settings::serialized(&File {
        diff: Config::default().resolved(),
        ..File::default()
    })?;

    Ok(Values {
        effective: effective.table().clone(),
        written,
        flagged,
        defaults: defaults.table().clone(),
    })
}

fn written(path: &[String], value: Option<&toml::Value>) -> Result<()> {
    let mut document = config::Document::open("sift")?;
    let path: Vec<&str> = path.iter().map(String::as_str).collect();
    match value {
        Some(value) => document.set(&path, value),
        None => document.remove(&path),
    }

    Ok(document.save()?)
}

fn rebound(before: &Rebindings, after: &Rebindings) -> Result<()> {
    let mut document = config::Document::open("sift")?;
    for name in before.commands.keys().filter(|name| !after.commands.contains_key(*name)) {
        document.remove(&["keys", name]);
    }
    for (name, keys) in &after.commands {
        let keys = keys.iter().map(|sequence| toml::Value::String(sequence.to_string())).collect();
        document.set(&["keys", name], &toml::Value::Array(keys));
    }

    Ok(document.save()?)
}

fn theme(config: &Config, palette: &Palette) -> Result<Theme> {
    let mut theme = match config.theme.unwrap_or_default() {
        Scheme::Terminal => Theme::from_palette(palette),
        Scheme::TokyoNight => Theme::default(),
    };
    if let Some(name) = config.syntax_theme.as_deref().filter(|name| !name.is_empty()) {
        theme.syntax = Syntax::named(name)?;
    }

    Ok(theme)
}

fn config(arguments: &Arguments) -> Result<File> {
    let file = config::Settings::of::<File>("sift")?;
    let flags =
        config::Settings::serialized(&File { diff: arguments.config.clone(), ..File::default() })?;

    Ok(file.over(flags).parse()?)
}

fn reload(screen: &mut Screen, subject: &Subject, shown: &mut Vec<FileChange>) -> Result<bool> {
    let changes = subject.changes()?;
    if changes == *shown {
        return Ok(false);
    }
    screen.review.refresh(changes.clone())?;
    screen.files = subject.files()?;
    if let Some(path) = screen.review.peeked().map(Path::to_path_buf) {
        match subject.file(&path) {
            Ok(content) => screen.review.peek(path, &content, None)?,
            Err(error) => screen.notice = Some(error.to_string()),
        }
    }
    *shown = changes;

    Ok(true)
}

fn tick(sender: Sender<Message>, pending: Arc<AtomicBool>, interval: Duration) {
    std::thread::Builder::new()
        .name("watch".to_owned())
        .spawn(move || {
            loop {
                if !pending.swap(true, Ordering::AcqRel) && sender.send(Message::Tick).is_err() {
                    return;
                }
                std::thread::sleep(interval);
            }
        })
        .expect("the watch thread cannot fail to start");
}

fn edit(path: &Path, line: Option<usize>) -> std::result::Result<(), String> {
    let editor = std::env::var("VISUAL")
        .or_else(|_| std::env::var("EDITOR"))
        .unwrap_or_else(|_| "vi".to_owned());
    let mut words = editor.split_whitespace();
    let program = words.next().unwrap_or("vi");
    let mut command = std::process::Command::new(program);
    command.args(words);

    let name = Path::new(program).file_name().and_then(|name| name.to_str()).unwrap_or(program);
    let target = path.display();
    match (name, line) {
        (_, None) => command.arg(path),
        ("code" | "cursor" | "codium" | "windsurf", Some(line)) => {
            command.args(["--goto".to_owned(), format!("{target}:{line}")])
        }
        ("hx" | "helix" | "zed" | "subl", Some(line)) => command.arg(format!("{target}:{line}")),
        (_, Some(line)) => command.arg(format!("+{line}")).arg(path),
    };

    match command.status() {
        Ok(status) if status.success() => Ok(()),
        Ok(status) => Err(format!("{program} exited with {status}")),
        Err(error) => Err(format!("could not start {program}: {error}")),
    }
}

/// What Sift was pointed at, kept open so the diff can be read again.
#[expect(clippy::large_enum_variant, reason = "exactly one Subject lives for the whole run")]
enum Subject {
    Repository {
        repository: Repository,
        comparison: Comparison,
        label: String,
        paths: Vec<PathBuf>,
    },
    Pair {
        old: PathBuf,
        new: PathBuf,
    },
}

impl Subject {
    fn open(arguments: &Arguments) -> Result<Self> {
        if let [old, new] = arguments.no_index.as_slice() {
            return Ok(Self::Pair { old: old.clone(), new: new.clone() });
        }

        let repository = Repository::discover(Path::new("."))?;
        let (comparison, label) = compare(arguments, &repository)?;
        let paths = arguments
            .paths
            .iter()
            .map(|path| repository.relative_path(path))
            .collect::<std::result::Result<_, _>>()?;

        Ok(Self::Repository { repository, comparison, label, paths })
    }

    fn root(&self) -> &Path {
        match self {
            Self::Repository { repository, .. } => repository.workdir(),
            Self::Pair { .. } => Path::new(""),
        }
    }

    fn branch(&self) -> Result<Option<String>> {
        match self {
            Self::Repository { repository, .. } => Ok(repository.branch()?),
            Self::Pair { .. } => Ok(None),
        }
    }

    fn describe(&self) -> String {
        match self {
            Self::Repository { label, .. } => label.clone(),
            Self::Pair { old, new } => format!("{} → {}", old.display(), new.display()),
        }
    }

    fn files(&self) -> Result<Vec<PathBuf>> {
        match self {
            Self::Repository { repository, comparison, .. } => Ok(repository.files(comparison)?),
            Self::Pair { .. } => Ok(Vec::new()),
        }
    }

    fn file(&self, path: &Path) -> Result<Content> {
        match self {
            Self::Repository { repository, comparison, .. } => {
                Ok(repository.file(comparison, path)?)
            }
            Self::Pair { .. } => Err(Error::Usage(format!("{} is not compared", path.display()))),
        }
    }

    fn changes(&self) -> Result<Vec<FileChange>> {
        match self {
            Self::Repository { repository, comparison, paths, .. } => {
                let changes = repository.changes(comparison)?;
                if paths.is_empty() {
                    return Ok(changes);
                }

                Ok(changes
                    .into_iter()
                    .filter(|change| paths.iter().any(|path| change.path.starts_with(path)))
                    .collect())
            }
            Self::Pair { old, new } => {
                let diff = FileDiff::compute(
                    Source::new(std::fs::read_to_string(old)?),
                    Source::new(std::fs::read_to_string(new)?),
                );

                let content = Content::Text(diff);

                Ok(vec![FileChange {
                    status: Status::Modified,
                    path: new.clone(),
                    previous: None,
                    generated: looks_generated(new, &content),
                    content,
                }])
            }
        }
    }
}

fn compare(arguments: &Arguments, repository: &Repository) -> Result<(Comparison, String)> {
    let newer = if arguments.staged { Snapshot::Index } else { Snapshot::Worktree };
    let against = if arguments.staged { "index" } else { "worktree" };

    if let Some(revision) = &arguments.show {
        return show(repository, revision);
    }

    match arguments.revisions.as_slice() {
        [] => Ok((Comparison { old: "HEAD".to_owned(), new: newer }, format!("HEAD → {against}"))),
        [range] if range.contains("...") => {
            refuse_staged(arguments, range)?;
            let (old, new) = halves(range, "...");
            let base = repository.merge_base(&old, &new)?;
            let label = format!("{old}...{new}");

            Ok((Comparison { old: base, new: Snapshot::Revision(new) }, label))
        }
        [range] if range.contains("..") => {
            refuse_staged(arguments, range)?;
            let (old, new) = halves(range, "..");
            let label = format!("{old} → {new}");

            Ok((Comparison { old, new: Snapshot::Revision(new) }, label))
        }
        [revision] if revision.ends_with("^!") => {
            refuse_staged(arguments, revision)?;
            show(repository, revision.trim_end_matches("^!"))
        }
        [revision] => {
            let label = format!("{} → {against}", short(revision));

            Ok((Comparison { old: revision.clone(), new: newer }, label))
        }
        [old, new] => {
            refuse_staged(arguments, new)?;
            let label = format!("{} → {}", short(old), short(new));

            Ok((Comparison { old: old.clone(), new: Snapshot::Revision(new.clone()) }, label))
        }
        _ => panic!("clap accepts at most two revisions"),
    }
}

fn show(repository: &Repository, revision: &str) -> Result<(Comparison, String)> {
    let parent = repository.parent(revision)?;
    let label = format!("{} (one commit)", short(revision));

    Ok((Comparison { old: parent, new: Snapshot::Revision(revision.to_owned()) }, label))
}

fn halves(range: &str, separator: &str) -> (String, String) {
    let (old, new) = range.split_once(separator).expect("the caller checked for the separator");
    let side = |side: &str| if side.is_empty() { "HEAD".to_owned() } else { side.to_owned() };

    (side(old), side(new))
}

fn refuse_staged(arguments: &Arguments, revision: &str) -> Result<()> {
    if arguments.staged {
        return Err(Error::Usage(format!(
            "--staged compares the index against one revision, not {revision}"
        )));
    }

    Ok(())
}

fn short(revision: &str) -> String {
    let hex =
        revision.len() == 40 && revision.chars().all(|character| character.is_ascii_hexdigit());
    if hex { revision[..7].to_owned() } else { revision.to_owned() }
}
