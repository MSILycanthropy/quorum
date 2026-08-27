//! Git plumbing for Quorum: worktree discovery and agent-base resolution.
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
    Revision(#[from] gix::revision::spec::parse::single::Error),
    #[error(transparent)]
    Object(#[from] gix::object::find::existing::Error),
    #[error(transparent)]
    Reference(#[from] gix::reference::find::existing::Error),
    #[error(transparent)]
    Reflog(#[from] gix::refs::file::log::iter::decode::Error),
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
    Io(#[from] std::io::Error),
}

pub type Result<T> = std::result::Result<T, Error>;

/// Where a file stood in a revision compared to where it stands in the worktree.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Status {
    Added,
    Modified,
    Deleted,
}

impl Status {
    pub fn letter(self) -> char {
        match self {
            Status::Added => 'A',
            Status::Modified => 'M',
            Status::Deleted => 'D',
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

/// One file, as a revision has it and as the worktree has it.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct FileChange {
    pub status: Status,
    pub path: PathBuf,
    pub content: Content,
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
        reason = "the Error enum names the only failures, an unborn HEAD or an unreadable reflog"
    )]
    pub fn base(&self) -> Result<String> {
        if self.linked()
            && let Some(created) = self.created_at()?
        {
            return Ok(created);
        }

        Ok(self.inner.rev_parse_single("HEAD")?.detach().to_string())
    }

    fn linked(&self) -> bool {
        match (self.inner.git_dir().canonicalize(), self.inner.common_dir().canonicalize()) {
            (Ok(git_dir), Ok(common_dir)) => git_dir != common_dir,
            _ => false,
        }
    }

    fn created_at(&self) -> Result<Option<String>> {
        let head = self.inner.find_reference("HEAD")?;
        let mut log = head.log_iter();
        let Some(mut entries) = log.all()? else {
            return Ok(None);
        };
        let Some(first) = entries.next() else {
            return Ok(None);
        };

        Ok(Some(first?.new_oid.to_string()))
    }

    #[expect(
        clippy::missing_errors_doc,
        reason = "the Error enum names the only failures, a missing revision or a path in neither side"
    )]
    pub fn change(&self, revision: &str, path: &Path) -> Result<FileChange> {
        let relative = self.relative_path(path)?;

        Reader::new(self, revision)?.change(&relative)
    }

    #[expect(
        clippy::missing_errors_doc,
        reason = "the Error enum names the only failures, a missing revision or an unreadable worktree"
    )]
    pub fn changes(&self, revision: &str) -> Result<Vec<FileChange>> {
        let mut reader = Reader::new(self, revision)?;
        let paths = self.changed_paths(reader.tree.id)?;

        paths.iter().map(|path| reader.change(path)).collect()
    }

    fn changed_paths(&self, tree: gix::ObjectId) -> Result<Vec<PathBuf>> {
        let mut paths = Vec::new();
        let walked = self
            .inner
            .status(gix::progress::Discard)?
            .untracked_files(gix::status::UntrackedFiles::Files)
            .head_tree(tree)
            .into_iter(None)?;
        for item in walked {
            let location = match item? {
                gix::status::Item::IndexWorktree(change) => change.rela_path().to_owned(),
                gix::status::Item::TreeIndex(change) => change.location().to_owned(),
            };
            let path = gix::path::from_bstring(location);
            if self.workdir.join(&path).is_dir() {
                continue;
            }
            paths.push(path);
        }
        paths.sort();
        paths.dedup();

        Ok(paths)
    }
}

/// One revision's tree and one checkout pipeline, held open across the files
/// read out of them.
struct Reader<'repository> {
    workdir: &'repository Path,
    revision: &'repository str,
    tree: gix::Tree<'repository>,
    pipeline: gix::filter::Pipeline<'repository>,
}

impl<'repository> Reader<'repository> {
    fn new(repository: &'repository Repository, revision: &'repository str) -> Result<Self> {
        let tree = repository.inner.rev_parse_single(revision)?.object()?.peel_to_tree()?;
        let (pipeline, _index) = repository.inner.filter_pipeline(None)?;

        Ok(Self { workdir: &repository.workdir, revision, tree, pipeline })
    }

    fn change(&mut self, relative: &Path) -> Result<FileChange> {
        let stored = self.blob(relative)?;
        let checked_out = match std::fs::read(self.workdir.join(relative)) {
            Ok(bytes) => Some(bytes),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
            Err(error) => return Err(error.into()),
        };

        let status = match (&stored, &checked_out) {
            (None, Some(_)) => Status::Added,
            (Some(_), None) => Status::Deleted,
            (Some(_), Some(_)) => Status::Modified,
            (None, None) => {
                return Err(Error::Missing {
                    revision: self.revision.to_owned(),
                    path: relative.to_path_buf(),
                });
            }
        };

        let old = stored.map_or_else(|| Some(String::new()), text);
        let new = checked_out.map_or_else(|| Some(String::new()), text);
        let content = match (old, new) {
            (Some(old), Some(new)) => {
                Content::Text(FileDiff::compute(Source::new(old), Source::new(new)))
            }
            _ => Content::Binary,
        };

        Ok(FileChange { status, path: relative.to_path_buf(), content })
    }

    fn blob(&mut self, relative: &Path) -> Result<Option<Vec<u8>>> {
        let Some(entry) = self.tree.lookup_entry_by_path(relative)? else {
            return Ok(None);
        };

        let stored = entry.object()?;
        let mut checked_out = Vec::new();
        self.pipeline
            .convert_to_worktree(
                &stored.data,
                gix::path::into_bstr(relative).as_ref(),
                gix::filter::plumbing::pipeline::convert::to_worktree::Options::default(),
            )?
            .read_to_end(&mut checked_out)?;

        Ok(Some(checked_out))
    }
}

fn text(bytes: Vec<u8>) -> Option<String> {
    if bytes[..bytes.len().min(8000)].contains(&0) {
        return None;
    }

    String::from_utf8(bytes).ok()
}
