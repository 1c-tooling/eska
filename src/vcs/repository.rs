//! Read-only repository operations. Git names and messages retain their original bytes.

use std::{
    fmt, fs, io,
    path::{Path, PathBuf},
};

use gix::{
    ObjectId,
    bstr::{BString, ByteSlice},
};

pub use super::git::ExistingError as OpenError;

/// An opened working repository. Reopen after external configuration changes.
pub struct Repository {
    pub(super) inner: gix::Repository,
    work_dir: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Head {
    Unborn { reference: BString },
    Attached { reference: BString, id: ObjectId },
    Detached { id: ObjectId },
}

impl Head {
    #[must_use]
    pub const fn id(&self) -> Option<ObjectId> {
        match self {
            Self::Unborn { .. } => None,
            Self::Attached { id, .. } | Self::Detached { id } => Some(*id),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReferenceTarget {
    Object(ObjectId),
    Symbolic(BString),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Reference {
    /// Full name, including `refs/heads/`, `refs/remotes/` or `refs/tags/`.
    pub name: BString,
    /// Annotated tags retain their tag object ID; symbolic refs retain their target name.
    pub target: ReferenceTarget,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Commit {
    pub id: ObjectId,
    pub parents: Vec<ObjectId>,
    pub message: BString,
}

/// A configured fetch remote with a display-safe URL.
#[derive(Clone, PartialEq, Eq)]
pub struct Remote {
    name: String,
    url: String,
    raw_url: BString,
    password: Option<String>,
}

impl fmt::Debug for Remote {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Remote")
            .field("name", &self.name)
            .field("url", &self.url)
            .finish_non_exhaustive()
    }
}

impl Remote {
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Return the fetch URL with a password redacted by `gix` formatting.
    #[must_use]
    pub fn url(&self) -> &str {
        &self.url
    }

    /// Preserve Git's diagnostic while replacing the configured URL with its safe form.
    #[must_use]
    pub fn sanitize_diagnostic(&self, diagnostic: &[u8]) -> String {
        let diagnostic = String::from_utf8_lossy(diagnostic);
        let raw_url = self.raw_url.to_str_lossy();
        let mut diagnostic = if raw_url.is_empty() {
            diagnostic.into_owned()
        } else {
            diagnostic.replace(raw_url.as_ref(), &self.url)
        };
        if let Some(password) = &self.password
            && !password.is_empty()
        {
            diagnostic = diagnostic.replace(password, "redacted");
        }
        diagnostic.trim().to_owned()
    }
}

/// Commit counts unique to HEAD and to a comparison reference.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Divergence {
    pub ahead: usize,
    pub behind: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Operation {
    Head,
    References,
    History,
    Status,
    Revision,
    TreeDiff,
    MergeBase,
    Divergence,
    Remotes,
}

#[derive(Debug)]
pub enum Error {
    Io {
        path: PathBuf,
        source: io::Error,
    },
    NotFound {
        start: PathBuf,
    },
    NotDirectory {
        path: PathBuf,
    },
    InvalidIndex {
        path: PathBuf,
    },
    Open(OpenError),
    Operation {
        operation: Operation,
        source: Box<dyn std::error::Error + Send + Sync>,
    },
}

impl Error {
    pub(super) fn operation(
        operation: Operation,
        source: impl Into<Box<dyn std::error::Error + Send + Sync>>,
    ) -> Self {
        Self::Operation {
            operation,
            source: source.into(),
        }
    }
}

impl Repository {
    /// Find the nearest working repository from an existing directory, resolving symlinks.
    ///
    /// Uses the shared `gix` opening boundary, including `.git` files and linked worktrees.
    /// Only repository-local configuration is read; Git environment redirects are ignored.
    ///
    /// # Errors
    /// Returns a structured error for missing, bare, inaccessible or malformed repositories.
    pub fn discover(start: &Path) -> Result<Self, Error> {
        let start = fs::canonicalize(start).map_err(|source| Error::Io {
            path: start.to_owned(),
            source,
        })?;
        if !start.is_dir() {
            return Err(Error::NotDirectory { path: start });
        }
        let inner = super::git::open_existing(&start)
            .map_err(Error::Open)?
            .ok_or(Error::NotFound { start })?;
        let work_dir = inner
            .workdir()
            .ok_or(Error::Open(OpenError::Bare))?
            .to_owned();
        Ok(Self { inner, work_dir })
    }

    #[must_use]
    pub fn work_dir(&self) -> &Path {
        &self.work_dir
    }

    /// Path to this worktree's Git metadata (distinct for linked worktrees).
    #[must_use]
    pub fn git_dir(&self) -> &Path {
        self.inner.git_dir()
    }

    /// Return this worktree's effective index path, including linked worktrees.
    #[must_use]
    pub fn index_path(&self) -> PathBuf {
        self.inner.index_path()
    }

    /// Read HEAD and validate its commit target, distinguishing unborn and detached states.
    ///
    /// # Errors
    /// Returns `Operation::Head` on malformed refs or missing/non-commit objects.
    pub fn head(&self) -> Result<Head, Error> {
        let error = |source| Error::operation(Operation::Head, source);
        let mut head = self.inner.head().map_err(error)?;
        if let gix::head::Kind::Unborn(reference) = &head.kind {
            return Ok(Head::Unborn {
                reference: reference.as_bstr().to_owned(),
            });
        }
        let reference = head.referent_name().map(|name| name.as_bstr().to_owned());
        let id = head
            .peel_to_commit()
            .map_err(|source| Error::operation(Operation::Head, source))?
            .id;
        Ok(
            reference.map_or(Head::Detached { id }, |reference| Head::Attached {
                reference,
                id,
            }),
        )
    }

    /// Read refs, sorted by full byte name. HEAD itself is returned by `head()`.
    ///
    /// # Errors
    /// Returns `Operation::References` if loose or packed refs cannot be read.
    pub fn references(&self) -> Result<Vec<Reference>, Error> {
        let platform = self
            .inner
            .references()
            .map_err(|source| Error::operation(Operation::References, source))?;
        let iter = platform
            .all()
            .map_err(|source| Error::operation(Operation::References, source))?;
        let mut refs = Vec::new();
        for reference in iter {
            let reference =
                reference.map_err(|source| Error::operation(Operation::References, source))?;
            let target = match &reference.inner.target {
                gix::refs::Target::Object(id) => ReferenceTarget::Object(*id),
                gix::refs::Target::Symbolic(name) => {
                    ReferenceTarget::Symbolic(name.as_bstr().to_owned())
                }
            };
            refs.push(Reference {
                name: reference.name().as_bstr().to_owned(),
                target,
            });
        }
        refs.sort_by(|left, right| left.name.cmp(&right.name));
        Ok(refs)
    }

    /// Return a configured named remote and its fetch URL, or `None` when it is absent.
    ///
    /// # Errors
    /// Returns `Operation::Remotes` for malformed remote configuration.
    pub fn remote(&self, name: &str) -> Result<Option<Remote>, Error> {
        let Some(remote) = self
            .inner
            .try_find_remote_without_url_rewrite(name.as_bytes().as_bstr())
        else {
            return Ok(None);
        };
        let remote = remote.map_err(|source| Error::operation(Operation::Remotes, source))?;
        let url = remote
            .url(gix::remote::Direction::Fetch)
            .ok_or_else(|| Error::operation(Operation::Remotes, "fetch URL is not configured"))?;
        Ok(Some(Remote {
            name: name.to_owned(),
            url: url.to_string(),
            raw_url: url.to_bstring(),
            password: url.password().map(str::to_owned),
        }))
    }

    /// Read at most `limit` commits reachable from HEAD in breadth-first parent order.
    /// Unborn HEAD yields an empty history. Shallow boundaries are respected by `gix`.
    /// This is not a chronological or topological `git log` ordering.
    ///
    /// # Errors
    /// Returns a HEAD error or `Operation::History` for unreadable commit graphs/objects.
    pub fn history(&self, limit: usize) -> Result<Vec<Commit>, Error> {
        if limit == 0 {
            return Ok(Vec::new());
        }
        let Some(id) = self.head()?.id() else {
            return Ok(Vec::new());
        };
        let walk = self
            .inner
            .rev_walk([id])
            .all()
            .map_err(|source| Error::operation(Operation::History, source))?;
        walk.take(limit)
            .map(|item| {
                let info = item.map_err(|source| Error::operation(Operation::History, source))?;
                let commit = self
                    .inner
                    .find_commit(info.id)
                    .map_err(|source| Error::operation(Operation::History, source))?;
                let decoded = commit
                    .decode()
                    .map_err(|source| Error::operation(Operation::History, source))?;
                Ok(Commit {
                    id: info.id,
                    parents: info.parent_ids.iter().copied().collect(),
                    message: decoded.message.to_owned(),
                })
            })
            .collect()
    }

    /// Count commits unique to HEAD and to a fully qualified comparison reference.
    /// Missing references and unborn HEAD produce no comparison data.
    ///
    /// # Errors
    /// Returns a HEAD error or `Operation::Divergence` for malformed refs or commit graphs.
    pub fn divergence(&self, reference: &str) -> Result<Option<Divergence>, Error> {
        let Some(head) = self.head()?.id() else {
            return Ok(None);
        };
        let Some(mut reference) = self
            .inner
            .try_find_reference(reference)
            .map_err(|source| Error::operation(Operation::Divergence, source))?
        else {
            return Ok(None);
        };
        let other = reference
            .peel_to_commit()
            .map_err(|source| Error::operation(Operation::Divergence, source))?
            .id;

        Ok(Some(Divergence {
            ahead: self.unique_commit_count(head, other)?,
            behind: self.unique_commit_count(other, head)?,
        }))
    }

    fn unique_commit_count(&self, tip: ObjectId, hidden: ObjectId) -> Result<usize, Error> {
        let mut walk = self
            .inner
            .rev_walk([tip])
            .with_hidden([hidden])
            .all()
            .map_err(|source| Error::operation(Operation::Divergence, source))?;
        walk.try_fold(0_usize, |count, item| {
            item.map(|_| count + 1)
                .map_err(|source| Error::operation(Operation::Divergence, source))
        })
    }
}
