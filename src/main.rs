//! The Sift binary.
#![expect(
    clippy::result_large_err,
    reason = "gix builds large error types, and every call here is already bound by disk"
)]

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Sender, channel};
use std::time::Duration;

use clap::Parser;
use ratatui::DefaultTerminal;
use ratatui::crossterm::event::{
    self, DisableMouseCapture, EnableMouseCapture, KeyEvent, KeyEventKind,
    KeyboardEnhancementFlags, MouseEvent, PopKeyboardEnhancementFlags,
    PushKeyboardEnhancementFlags,
};
use ratatui::crossterm::execute;
use ratatui::crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use sift::diff::{FileDiff, Source};
use sift::git::{Comparison, Content, FileChange, Repository, Snapshot, Status, looks_generated};
use sift::ui::{Control, Mode, Opening, Surface};

#[derive(Debug, thiserror::Error)]
enum Error {
    #[error(transparent)]
    Git(Box<sift::git::Error>),
    #[error(transparent)]
    Ui(#[from] sift::ui::Error),
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
#[expect(
    clippy::struct_excessive_bools,
    reason = "each bool is an independent command-line switch"
)]
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
        Press ? inside for every key."
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
    #[arg(long, help = "Start with long lines wrapped")]
    wrap: bool,
    #[arg(long, help = "Start in the unified layout")]
    unified: bool,
    #[arg(long, help = "Re-read the diff from git whenever it changes")]
    watch: bool,
    #[arg(
        long,
        default_value_t = 1000,
        value_name = "MILLISECONDS",
        help = "How often --watch checks git"
    )]
    interval: u64,
    #[arg(long, default_value_t = 4, help = "Columns a tab expands to")]
    tab_width: usize,
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
enum Event {
    Key(KeyEvent),
    Mouse(MouseEvent),
    Redraw,
    Tick,
}

fn view(arguments: &Arguments) -> Result<()> {
    let subject = Subject::open(arguments)?;
    let opening = Opening {
        mode: if arguments.unified { Mode::Unified } else { Mode::SideBySide },
        wrap: arguments.wrap,
        tab_width: arguments.tab_width,
        branch: subject.branch()?,
        comparing: subject.describe(),
        files: subject.files()?,
        bindings: Vec::new(),
    };
    let mut shown = subject.changes()?;
    let mut surface = opening.surface(shown.clone())?;

    let (sender, events) = channel();
    let paused = Arc::new(AtomicBool::new(false));
    if arguments.watch {
        tick(sender.clone(), Duration::from_millis(arguments.interval.max(100)));
    }
    watch(sender, Arc::clone(&paused));

    ratatui::run(|terminal| -> Result<()> {
        let _reporting = Reporting::on()?;
        terminal.draw(|frame| surface.draw(frame))?;

        while let Ok(event) = events.recv() {
            match event {
                Event::Key(key) => match surface.key(key)? {
                    Control::Continue | Control::Host(_) => {}
                    Control::Quit => return Ok(()),
                    Control::Reload => {
                        reload(&mut surface, &subject, &mut shown)?;
                        surface.notify("reloaded");
                    }
                    Control::Open { path, line } => match subject.file(&path) {
                        Ok(content) => surface.peek(path, &content, line)?,
                        Err(error) => surface.notify(error.to_string()),
                    },
                    Control::Copy(text) => {
                        copy(&text)?;
                        surface.notify(format!("copied {text}"));
                    }
                    Control::Edit { path, line } => {
                        let root = subject.root();
                        let outcome =
                            suspended(terminal, &paused, || edit(&root.join(&path), line));
                        match outcome? {
                            Ok(()) => {
                                reload(&mut surface, &subject, &mut shown)?;
                            }
                            Err(error) => surface.notify(error),
                        }
                    }
                },
                Event::Mouse(mouse) => surface.mouse(mouse)?,
                Event::Redraw => {}
                Event::Tick => {
                    if !reload(&mut surface, &subject, &mut shown).unwrap_or(false) {
                        continue;
                    }
                }
            }
            terminal.draw(|frame| surface.draw(frame))?;
        }

        Ok(())
    })
}

fn reload(surface: &mut Surface, subject: &Subject, shown: &mut Vec<FileChange>) -> Result<bool> {
    let changes = subject.changes()?;
    if changes == *shown {
        return Ok(false);
    }
    surface.refresh(changes.clone())?;
    surface.replace_files(subject.files()?);
    if let Some(path) = surface.peeked().map(Path::to_path_buf) {
        match subject.file(&path) {
            Ok(content) => surface.peek(path, &content, None)?,
            Err(error) => surface.notify(error.to_string()),
        }
    }
    *shown = changes;

    Ok(true)
}

fn tick(sender: Sender<Event>, interval: Duration) {
    std::thread::Builder::new()
        .name("watch".to_owned())
        .spawn(move || {
            while sender.send(Event::Tick).is_ok() {
                std::thread::sleep(interval);
            }
        })
        .expect("the watch thread cannot fail to start");
}

fn watch(sender: Sender<Event>, paused: Arc<AtomicBool>) {
    std::thread::Builder::new()
        .name("terminal".to_owned())
        .spawn(move || {
            loop {
                if paused.load(Ordering::Acquire) {
                    std::thread::sleep(Duration::from_millis(20));
                    continue;
                }
                match event::poll(Duration::from_millis(20)) {
                    Ok(true) => {}
                    Ok(false) => continue,
                    Err(_) => return,
                }
                if paused.load(Ordering::Acquire) {
                    continue;
                }
                let Ok(event) = event::read() else {
                    return;
                };
                let event = match event {
                    event::Event::Key(key) if key.kind == KeyEventKind::Press => Event::Key(key),
                    event::Event::Mouse(mouse) => Event::Mouse(mouse),
                    event::Event::Resize(_, _) => Event::Redraw,
                    _ => continue,
                };
                if sender.send(event).is_err() {
                    return;
                }
            }
        })
        .expect("the terminal reader thread cannot fail to start");
}

fn suspended<T>(
    terminal: &mut DefaultTerminal,
    paused: &AtomicBool,
    work: impl FnOnce() -> T,
) -> Result<T> {
    paused.store(true, Ordering::Release);
    std::thread::sleep(Duration::from_millis(30));
    execute!(
        std::io::stdout(),
        PopKeyboardEnhancementFlags,
        DisableMouseCapture,
        LeaveAlternateScreen
    )?;
    disable_raw_mode()?;

    let outcome = work();

    enable_raw_mode()?;
    execute!(std::io::stdout(), EnterAlternateScreen)?;
    Reporting::switch_on()?;
    terminal.clear()?;
    paused.store(false, Ordering::Release);

    Ok(outcome)
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

fn copy(text: &str) -> Result<()> {
    let mut stdout = std::io::stdout();
    write!(stdout, "\x1b]52;c;{}\x07", base64(text.as_bytes()))?;
    stdout.flush()?;

    if cfg!(target_os = "macos")
        && let Ok(mut child) =
            std::process::Command::new("pbcopy").stdin(std::process::Stdio::piped()).spawn()
    {
        if let Some(stdin) = child.stdin.as_mut() {
            stdin.write_all(text.as_bytes())?;
        }
        child.wait()?;
    }

    Ok(())
}

fn base64(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut encoded = String::with_capacity(bytes.len().div_ceil(3) * 4);

    for chunk in bytes.chunks(3) {
        let triple = chunk
            .iter()
            .enumerate()
            .fold(0u32, |triple, (index, byte)| triple | u32::from(*byte) << (16 - 8 * index));
        for position in 0..4 {
            if position <= chunk.len() {
                let index = (triple >> (18 - 6 * position)) & 0x3f;
                encoded.push(char::from(ALPHABET[index as usize]));
            } else {
                encoded.push('=');
            }
        }
    }

    encoded
}

/// Mouse and key reporting, switched back off when this is dropped, so an
/// unwind cannot leave the terminal capturing.
struct Reporting;

impl Reporting {
    fn on() -> Result<Self> {
        Self::switch_on()?;

        Ok(Self)
    }

    fn switch_on() -> Result<()> {
        execute!(
            std::io::stdout(),
            EnableMouseCapture,
            PushKeyboardEnhancementFlags(KeyboardEnhancementFlags::DISAMBIGUATE_ESCAPE_CODES)
        )?;

        Ok(())
    }
}

impl Drop for Reporting {
    fn drop(&mut self) {
        let _ = execute!(std::io::stdout(), PopKeyboardEnhancementFlags, DisableMouseCapture);
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
