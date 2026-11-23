//! Git plumbing for Quorum: what changed between a revision and another snapshot.
#![expect(
    clippy::result_large_err,
    reason = "gix builds large error types, and every call here is already bound by disk"
)]

use std::io::Read;
use std::path::{Path, PathBuf};

use crate::diff::{FileDiff, Source};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("bare repository has no worktree: {0}")]
    BareRepository(PathBuf),
    #[error("outside the repository: {0}")]
    OutsideRepository(PathBuf),
    #[error("{path} exists neither in {revision} nor in the worktree")]
    Missing { revision: String, path: PathBuf },
    #[error(transparent)]
    Discover(#[from] gix::discover::Error),
    #[error(transparent)]
    Open(#[from] gix::open::Error),
    #[error(transparent)]
    Revision(#[from] gix::revision::spec::parse::single::Error),
    #[error(transparent)]
    Object(#[from] gix::object::find::existing::Error),
    #[error(transparent)]
    Reference(#[from] gix::reference::find::existing::Error),
    #[error(transparent)]
    Peel(#[from] gix::object::peel::to_kind::Error),
    #[error(transparent)]
    Pipeline(#[from] gix::repository::filter::pipeline::Error),
    #[error(transparent)]
    Convert(#[from] gix::filter::pipeline::convert_to_worktree::Error),
    #[error(transparent)]
    Status(#[from] gix::status::Error),
    #[error(transparent)]
    StatusIteration(#[from] gix::status::into_iter::Error),
    #[error(transparent)]
    StatusItem(#[from] gix::status::iter::Error),
    #[error(transparent)]
    Index(#[from] gix::worktree::open_index::Error),
    #[error(transparent)]
    TreeDiff(#[from] gix::repository::diff_tree_to_tree::Error),
    #[error(transparent)]
    Traverse(#[from] gix::traverse::tree::breadthfirst::Error),
    #[error(transparent)]
    Attributes(#[from] gix::config::attribute_stack::Error),
    #[error(transparent)]
    MergeBase(#[from] gix::repository::merge_base::Error),
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

pub type Result<T> = std::result::Result<T, Error>;

/// Where a file stood in a revision compared to where it stands in the worktree.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Status {
    Added,
    Modified,
    Deleted,
    Renamed,
}

impl Status {
    pub fn letter(self) -> char {
        match self {
            Status::Added => 'A',
            Status::Modified => 'M',
            Status::Deleted => 'D',
            Status::Renamed => 'R',
        }
    }
}

/// Both sides of a file, or the fact that Quorum will not read them as text:
/// they hold a NUL byte, or they are not UTF-8.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Content {
    Text(FileDiff),
    Binary,
}

/// One file, as a revision has it and as the worktree has it. A renamed file
/// keeps the path it was renamed from; a generated one was written by a tool.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct FileChange {
    pub status: Status,
    pub path: PathBuf,
    pub previous: Option<PathBuf>,
    pub generated: bool,
    pub content: Content,
}

/// Files whose diff is written by a tool, by name, when `.gitattributes` says nothing.
const GENERATED_NAMES: [&str; 12] = [
    "Cargo.lock",
    "package-lock.json",
    "npm-shrinkwrap.json",
    "yarn.lock",
    "pnpm-lock.yaml",
    "bun.lock",
    "bun.lockb",
    "Gemfile.lock",
    "poetry.lock",
    "composer.lock",
    "flake.lock",
    "go.sum",
];
const GENERATED_SUFFIXES: [&str; 3] = [".min.js", ".min.css", ".map"];
const GENERATED_MARKERS: [&str; 2] = ["DO NOT EDIT", "@generated"];

/// How far into a file a generated marker is looked for, in lines.
const MARKED_WITHIN: usize = 5;

pub fn looks_generated(path: &Path, content: &Content) -> bool {
    let name = path.file_name().and_then(|name| name.to_str()).unwrap_or_default();
    if GENERATED_NAMES.contains(&name) || GENERATED_SUFFIXES.iter().any(|end| name.ends_with(end)) {
        return true;
    }
    let Content::Text(diff) = content else {
        return false;
    };
    let source = if diff.new.is_empty() { &diff.old } else { &diff.new };

    source
        .lines()
        .take(MARKED_WITHIN)
        .any(|line| GENERATED_MARKERS.iter().any(|marker| line.contains(marker)))
}

/// A repository open on a worktree, addressed by paths relative to that worktree.
pub struct Repository {
    inner: gix::Repository,
    workdir: PathBuf,
}

impl Repository {
    #[expect(
        clippy::missing_errors_doc,
        reason = "the Error enum names the only failures, an unreadable or bare repository"
    )]
    pub fn discover(path: &Path) -> Result<Self> {
        let parent = path.parent().filter(|parent| !parent.as_os_str().is_empty());
        let start = if path.is_dir() { path } else { parent.unwrap_or(Path::new(".")) };
        let inner = gix::discover(start)?;
        let workdir = inner
            .workdir()
            .ok_or_else(|| Error::BareRepository(inner.git_dir().to_path_buf()))?
            .canonicalize()?;

        Ok(Self { inner, workdir })
    }

    pub fn workdir(&self) -> &Path {
        &self.workdir
    }

    #[expect(
        clippy::missing_errors_doc,
        reason = "the Error enum names the only failure, a path outside the worktree"
    )]
    pub fn relative_path(&self, path: &Path) -> Result<PathBuf> {
        let absolute = if path.is_absolute() {
            path.to_path_buf()
        } else {
            std::env::current_dir()?.join(path)
        };
        let absolute = if let Ok(canonical) = absolute.canonicalize() {
            canonical
        } else {
            let parent = absolute.parent().unwrap_or(Path::new(""));
            let name = absolute.file_name().unwrap_or_default();
            parent.canonicalize()?.join(name)
        };

        absolute
            .strip_prefix(&self.workdir)
            .map(Path::to_path_buf)
            .map_err(|_| Error::OutsideRepository(path.to_path_buf()))
    }

    #[expect(
        clippy::missing_errors_doc,
        reason = "the Error enum names the only failure, an unreadable HEAD"
    )]
    pub fn branch(&self) -> Result<Option<String>> {
        Ok(self.inner.head_name()?.map(|name| name.shorten().to_string()))
    }

    #[expect(
        clippy::missing_errors_doc,
        reason = "the Error enum names the only failures, a missing revision or a path in neither side"
    )]
    pub fn change(&self, comparison: &Comparison, path: &Path) -> Result<FileChange> {
        let relative = self.relative_path(path)?;

        Reader::new(self, comparison)?.change(&relative)
    }

    #[expect(
        clippy::missing_errors_doc,
        reason = "the Error enum names the only failures, a missing revision or an unreadable snapshot"
    )]
    pub fn changes(&self, comparison: &Comparison) -> Result<Vec<FileChange>> {
        let mut reader = Reader::new(self, comparison)?;
        let paths = reader.changed_paths()?;

        let changes = paths.iter().map(|path| reader.change(path)).collect::<Result<_>>()?;
        let mut changes = renamed(changes);

        let index = self.inner.index_or_empty()?;
        let mut attributes = self.inner.attributes_only(
            &index,
            gix::worktree::stack::state::attributes::Source::WorktreeThenIdMapping,
        )?;
        for change in &mut changes {
            let mut outcome = attributes.selected_attribute_matches(["linguist-generated", "diff"]);
            attributes.at_path(&change.path, None)?.matching_attributes(&mut outcome);
            change.generated = match declared(&outcome) {
                Some(generated) => generated,
                None => looks_generated(&change.path, &change.content),
            };
        }

        Ok(changes)
    }

    #[expect(
        clippy::missing_errors_doc,
        reason = "the Error enum names the only failures, a missing revision or an unreadable snapshot"
    )]
    pub fn files(&self, comparison: &Comparison) -> Result<Vec<PathBuf>> {
        let mut paths = match &comparison.new {
            Snapshot::Worktree | Snapshot::Index => {
                let index = self.inner.index_or_empty()?;
                index
                    .entries()
                    .iter()
                    .map(|entry| gix::path::from_bstr(entry.path(&index)).into_owned())
                    .collect()
            }
            Snapshot::Revision(revision) => {
                let mut recorder = gix::traverse::tree::Recorder::default();
                self.tree(revision)?.traverse().breadthfirst(&mut recorder)?;
                recorder
                    .records
                    .into_iter()
                    .filter(|entry| !entry.mode.is_tree())
                    .map(|entry| gix::path::from_bstring(entry.filepath))
                    .collect::<Vec<_>>()
            }
        };
        paths.sort();
        paths.dedup();

        Ok(paths)
    }

    #[expect(
        clippy::missing_errors_doc,
        reason = "the Error enum names the only failures, a missing revision or a path in neither side"
    )]
    pub fn file(&self, comparison: &Comparison, path: &Path) -> Result<Content> {
        let mut reader = Reader::new(self, comparison)?;
        let Some(bytes) = reader.current(path)? else {
            return Err(Error::Missing { revision: reader.label, path: path.to_path_buf() });
        };

        Ok(match text(bytes) {
            Some(text) => {
                Content::Text(FileDiff::compute(Source::new(text.clone()), Source::new(text)))
            }
            None => Content::Binary,
        })
    }

    #[expect(
        clippy::missing_errors_doc,
        reason = "the Error enum names the only failures, a missing revision or unrelated histories"
    )]
    pub fn merge_base(&self, one: &str, two: &str) -> Result<String> {
        let one = self.inner.rev_parse_single(one)?.detach();
        let two = self.inner.rev_parse_single(two)?.detach();

        Ok(self.inner.merge_base(one, two)?.detach().to_string())
    }

    #[expect(
        clippy::missing_errors_doc,
        reason = "the Error enum names the only failures, a missing revision or one that is not a commit"
    )]
    pub fn parent(&self, revision: &str) -> Result<String> {
        let commit = self.inner.rev_parse_single(revision)?.object()?.peel_to_commit()?;

        Ok(match commit.parent_ids().next() {
            Some(parent) => parent.detach().to_string(),
            None => gix::ObjectId::empty_tree(self.inner.object_hash()).to_string(),
        })
    }

    fn tree(&self, revision: &str) -> Result<gix::Tree<'_>> {
        Ok(self.inner.rev_parse_single(revision)?.object()?.peel_to_tree()?)
    }

    fn status_paths(&self, tree: gix::ObjectId, staged_only: bool) -> Result<Vec<PathBuf>> {
        let untracked = if staged_only {
            gix::status::UntrackedFiles::None
        } else {
            gix::status::UntrackedFiles::Files
        };
        let walked = self
            .inner
            .status(gix::progress::Discard)?
            .untracked_files(untracked)
            .tree_index_track_renames(gix::status::tree_index::TrackRenames::Disabled)
            .head_tree(tree)
            .into_iter(None)?;

        let mut paths = Vec::new();
        for item in walked {
            let location = match item? {
                gix::status::Item::IndexWorktree(_) if staged_only => continue,
                gix::status::Item::IndexWorktree(change) => change.rela_path().to_owned(),
                gix::status::Item::TreeIndex(change) => change.location().to_owned(),
            };
            let path = gix::path::from_bstring(location);
            if !staged_only && self.workdir.join(&path).is_dir() {
                continue;
            }
            paths.push(path);
        }

        Ok(paths)
    }
}

/// Where the newer side of a comparison is read from.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub enum Snapshot {
    #[default]
    Worktree,
    Index,
    Revision(String),
}

/// A revision, and the snapshot it is compared against.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Comparison {
    pub old: String,
    pub new: Snapshot,
}

impl Default for Comparison {
    fn default() -> Self {
        Self { old: "HEAD".to_owned(), new: Snapshot::Worktree }
    }
}

/// The newer side, opened once for every file read out of it.
enum Newer<'repository> {
    Worktree,
    Index(gix::worktree::Index),
    Tree(gix::Tree<'repository>),
}

/// Both sides of a comparison and one checkout pipeline, held open across the
/// files read out of them.
struct Reader<'repository> {
    repository: &'repository Repository,
    label: String,
    old: gix::Tree<'repository>,
    new: Newer<'repository>,
    pipeline: gix::filter::Pipeline<'repository>,
}

impl<'repository> Reader<'repository> {
    fn new(repository: &'repository Repository, comparison: &Comparison) -> Result<Self> {
        let old = repository.tree(&comparison.old)?;
        let new = match &comparison.new {
            Snapshot::Worktree => Newer::Worktree,
            Snapshot::Index => Newer::Index(repository.inner.index_or_empty()?),
            Snapshot::Revision(revision) => Newer::Tree(repository.tree(revision)?),
        };
        let (pipeline, _index) = repository.inner.filter_pipeline(None)?;

        Ok(Self { repository, label: comparison.old.clone(), old, new, pipeline })
    }

    fn changed_paths(&self) -> Result<Vec<PathBuf>> {
        let mut paths = match &self.new {
            Newer::Worktree => self.repository.status_paths(self.old.id, false)?,
            Newer::Index(_) => self.repository.status_paths(self.old.id, true)?,
            Newer::Tree(new) => {
                let mut options = gix::diff::Options::default().with_rewrites(None);
                options.track_path();
                self.repository
                    .inner
                    .diff_tree_to_tree(&self.old, new, options)?
                    .into_iter()
                    .filter(|change| !change.entry_mode().is_tree())
                    .map(|change| gix::path::from_bstr(change.location()).into_owned())
                    .collect()
            }
        };
        paths.sort();
        paths.dedup();

        Ok(paths)
    }

    fn change(&mut self, relative: &Path) -> Result<FileChange> {
        let stored = self.stored(relative)?;
        let current = self.current(relative)?;

        let status = match (&stored, &current) {
            (None, Some(_)) => Status::Added,
            (Some(_), None) => Status::Deleted,
            (Some(_), Some(_)) => Status::Modified,
            (None, None) => {
                return Err(Error::Missing {
                    revision: self.label.clone(),
                    path: relative.to_path_buf(),
                });
            }
        };

        let old = stored.map_or_else(|| Some(String::new()), text);
        let new = current.map_or_else(|| Some(String::new()), text);
        let content = match (old, new) {
            (Some(old), Some(new)) => {
                Content::Text(FileDiff::compute(Source::new(old), Source::new(new)))
            }
            _ => Content::Binary,
        };

        Ok(FileChange {
            status,
            path: relative.to_path_buf(),
            previous: None,
            generated: false,
            content,
        })
    }

    fn stored(&mut self, relative: &Path) -> Result<Option<Vec<u8>>> {
        let Some(entry) = self.old.lookup_entry_by_path(relative)? else {
            return Ok(None);
        };
        let id = entry.object_id();

        self.checked_out(id, relative).map(Some)
    }

    fn current(&mut self, relative: &Path) -> Result<Option<Vec<u8>>> {
        let id = match &self.new {
            Newer::Worktree => {
                return match std::fs::read(self.repository.workdir.join(relative)) {
                    Ok(bytes) => Ok(Some(bytes)),
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
                    Err(error) => Err(error.into()),
                };
            }
            Newer::Index(index) => {
                let Some(entry) = index.entry_by_path(gix::path::into_bstr(relative).as_ref())
                else {
                    return Ok(None);
                };
                entry.id
            }
            Newer::Tree(tree) => {
                let Some(entry) = tree.lookup_entry_by_path(relative)? else {
                    return Ok(None);
                };
                entry.object_id()
            }
        };

        self.checked_out(id, relative).map(Some)
    }

    fn checked_out(&mut self, id: gix::ObjectId, relative: &Path) -> Result<Vec<u8>> {
        let stored = self.repository.inner.find_object(id)?;
        let mut checked_out = Vec::new();
        self.pipeline
            .convert_to_worktree(
                &stored.data,
                gix::path::into_bstr(relative).as_ref(),
                gix::filter::plumbing::pipeline::convert::to_worktree::Options::default(),
            )?
            .read_to_end(&mut checked_out)?;

        Ok(checked_out)
    }
}

fn declared(outcome: &gix::attrs::search::Outcome) -> Option<bool> {
    use gix::attrs::StateRef;

    let mut declared = None;
    for matched in outcome.iter_selected() {
        let assignment = &matched.assignment;
        match (assignment.name.as_str(), assignment.state) {
            ("linguist-generated", StateRef::Set) => return Some(true),
            ("linguist-generated", StateRef::Unset) => return Some(false),
            ("linguist-generated", StateRef::Value(value)) => {
                return Some(value.as_bstr() != "false");
            }
            ("diff", StateRef::Unset) => declared = Some(true),
            _ => {}
        }
    }

    declared
}

/// How alike a deleted and an added file must be to count as one renamed file.
const RENAMED: f32 = 0.5;

/// Past this many deleted-added pairs, only identical files are paired.
const COMPARED: usize = 10_000;

fn renamed(mut changes: Vec<FileChange>) -> Vec<FileChange> {
    let deleted: Vec<usize> =
        (0..changes.len()).filter(|&index| changes[index].status == Status::Deleted).collect();
    let added: Vec<usize> =
        (0..changes.len()).filter(|&index| changes[index].status == Status::Added).collect();
    let thorough = deleted.len() * added.len() <= COMPARED;

    let mut paired = Vec::new();
    let mut taken = vec![false; changes.len()];
    for &addition in &added {
        let Content::Text(new) = &changes[addition].content else {
            continue;
        };
        let best = deleted
            .iter()
            .filter(|&&deletion| !taken[deletion])
            .filter_map(|&deletion| {
                let Content::Text(old) = &changes[deletion].content else {
                    return None;
                };
                let similarity = if old.old.text() == new.new.text() {
                    1.0
                } else if thorough {
                    similar::TextDiff::from_lines(old.old.text(), new.new.text()).ratio()
                } else {
                    0.0
                };
                (similarity >= RENAMED).then_some((deletion, similarity))
            })
            .max_by(|one, two| one.1.total_cmp(&two.1));
        if let Some((deletion, _)) = best {
            taken[deletion] = true;
            paired.push((deletion, addition));
        }
    }

    for &(deletion, addition) in &paired {
        let (Content::Text(old), Content::Text(new)) =
            (&changes[deletion].content, &changes[addition].content)
        else {
            panic!("only text files are paired");
        };
        let diff = FileDiff::compute(Source::new(old.old.text()), Source::new(new.new.text()));
        let previous = changes[deletion].path.clone();
        let renamed = &mut changes[addition];
        renamed.status = Status::Renamed;
        renamed.previous = Some(previous);
        renamed.content = Content::Text(diff);
    }

    changes
        .into_iter()
        .enumerate()
        .filter(|(index, _)| !taken[*index])
        .map(|(_, change)| change)
        .collect()
}

fn text(bytes: Vec<u8>) -> Option<String> {
    if bytes[..bytes.len().min(8000)].contains(&0) {
        return None;
    }

    String::from_utf8(bytes).ok()
}
