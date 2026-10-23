//! Read-only source views and access facts share backend ownership without depending on an editor URI.

use sha2::{Digest, Sha256};
use std::{
    fmt::Write,
    fs::File,
    io::{self, Read},
    path::{Path, PathBuf},
    sync::Arc,
};

use super::{FileSupport, ProjectSession, SourceRestrictionInfo, WorkspaceError};

/// Source views use the same per-document text budget as query owners; binary content is never decoded implicitly.
pub const MAX_SOURCE_SNAPSHOT_BYTES: usize = 8 * 1024 * 1024;

/// Native user restrictions are supplied separately from filesystem permissions and support policy.
#[derive(Clone, Debug)]
pub struct SourceAccessRequest {
    pub path: PathBuf,
    pub user_read_only: bool,
}

/// Physical-file access never inherits a query's temporary exemption or silently disables a support policy.
#[derive(Clone, Debug)]
pub struct SourceProtection {
    pub support: FileSupport,
    pub restrictions: Vec<SourceRestrictionInfo>,
    pub user_read_only: bool,
    pub filesystem: SourceFileState,
    pub support_enforced: bool,
    pub diagnostics: Vec<String>,
}

/// Missing sources and existing read-only files are distinct facts, with no contradictory flag combinations.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SourceFileState {
    Missing,
    ReadOnly,
    Writable,
}

impl SourceProtection {
    /// Current native write eligibility never grants a query lease exemption or removes another restriction.
    #[must_use]
    pub fn writable(&self) -> bool {
        self.filesystem == SourceFileState::Writable
            && !self.support.objects.is_empty()
            && !self.user_read_only
            && self.restrictions.is_empty()
            && (!self.support_enforced
                || (!self.support.read_only
                    && !self.support.unknown
                    && self.diagnostics.is_empty()))
    }

    /// A retained native buffer may remain inspectable after its disk file disappears.
    #[must_use]
    pub const fn exists(&self) -> bool {
        !matches!(self.filesystem, SourceFileState::Missing)
    }

    /// A missing file cannot be treated as writable merely because no permission bits can be read.
    #[must_use]
    pub const fn filesystem_read_only(&self) -> bool {
        !matches!(self.filesystem, SourceFileState::Writable)
    }
}

/// A native buffer is a bounded exact snapshot; it cannot supply a different path or source identity.
#[derive(Clone, Debug)]
pub struct NativeSourceSnapshot {
    pub version: u64,
    pub text: Arc<str>,
    pub user_read_only: bool,
}

/// The origin tells an adapter whether bytes came from disk or an explicitly retained native model.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SourceSnapshotOrigin {
    Disk,
    Editor,
    QuerySession,
}

#[derive(Clone, Debug)]
enum Body {
    Bytes(Arc<[u8]>),
    Text(Arc<str>),
}

/// Immutable handles can be chunked safely after a metadata event, without re-reading a changing file.
#[derive(Clone, Debug)]
pub struct SourceSnapshot {
    pub protection: SourceProtection,
    pub origin: SourceSnapshotOrigin,
    pub version: Option<u64>,
    pub snapshot: String,
    body: Body,
}

/// Read-only inspection failures stay structured and never embed source contents or localized strings.
#[derive(Debug)]
pub enum SourceSnapshotError {
    Workspace(Box<WorkspaceError>),
    UnknownSource,
    Missing,
    TooLarge,
    InvalidEditorState,
    Io { path: PathBuf, source: io::Error },
}

impl From<WorkspaceError> for SourceSnapshotError {
    /// Retain resolver and containment failures at the common source boundary.
    fn from(error: WorkspaceError) -> Self {
        Self::Workspace(Box::new(error))
    }
}

impl SourceSnapshot {
    /// Source bytes remain exact, including a disk encoding signature or native-model line endings.
    #[must_use]
    pub fn bytes(&self) -> &[u8] {
        match &self.body {
            Body::Bytes(bytes) => bytes,
            Body::Text(text) => text.as_bytes(),
        }
    }

    /// Query owners share their retained immutable text rather than manufacturing a second disk-based parent view.
    pub(super) fn retained_owner(
        protection: SourceProtection,
        text: Arc<str>,
        version: u64,
    ) -> Self {
        Self::new(
            protection,
            SourceSnapshotOrigin::QuerySession,
            Some(version),
            Body::Text(text),
        )
    }

    /// Hash the actual immutable body; a protection snapshot never authorizes a later source write by itself.
    fn new(
        protection: SourceProtection,
        origin: SourceSnapshotOrigin,
        version: Option<u64>,
        body: Body,
    ) -> Self {
        let bytes = match &body {
            Body::Bytes(bytes) => bytes.as_ref(),
            Body::Text(text) => text.as_bytes(),
        };
        let snapshot =
            Sha256::digest(bytes)
                .iter()
                .fold(String::with_capacity(64), |mut output, byte| {
                    let _ = write!(output, "{byte:02x}");
                    output
                });
        Self {
            protection,
            origin,
            version,
            snapshot,
            body,
        }
    }
}

impl ProjectSession {
    /// Resolve fresh support and temporary owners for only the supplied files; no project inventory is started.
    ///
    /// # Errors
    /// Rejects source escapes and resolver/IO failures. Disabling support is explicit and cannot remove temporary ownership.
    pub fn source_protection(
        &mut self,
        requests: &[SourceAccessRequest],
        enforce_support: bool,
    ) -> Result<Vec<SourceProtection>, WorkspaceError> {
        let paths = requests
            .iter()
            .map(|request| request.path.clone())
            .collect::<Vec<_>>();
        let support = self.support_files(&paths)?;
        support
            .files
            .into_iter()
            .zip(requests)
            .map(|(file, request)| {
                let physical = self
                    .source
                    .checked_file(&file.path)
                    .map_err(WorkspaceError::Source)?;
                let metadata = physical
                    .as_ref()
                    .map(std::fs::metadata)
                    .transpose()
                    .map_err(|source| {
                        WorkspaceError::Source(crate::project::designer_source::SourceError::Io {
                            path: file.path.clone(),
                            source,
                        })
                    })?;
                let filesystem = match metadata {
                    None => SourceFileState::Missing,
                    Some(metadata) if metadata.permissions().readonly() => {
                        SourceFileState::ReadOnly
                    }
                    Some(_) => SourceFileState::Writable,
                };
                let restrictions = self.source_protections.describe(&file.path);
                Ok(SourceProtection {
                    support: file,
                    restrictions,
                    user_read_only: request.user_read_only,
                    filesystem,
                    support_enforced: enforce_support,
                    diagnostics: support.diagnostics.clone(),
                })
            })
            .collect()
    }

    /// Capture exact bytes only for a declared source; native buffers do not grant new source identities or write access.
    ///
    /// # Errors
    /// Rejects unknown owners, source escapes, invalid native counters, missing files and snapshots exceeding 8 MiB.
    pub fn source_snapshot(
        &mut self,
        path: &Path,
        native: Option<NativeSourceSnapshot>,
    ) -> Result<SourceSnapshot, SourceSnapshotError> {
        let request = SourceAccessRequest {
            path: path.to_path_buf(),
            user_read_only: native.as_ref().is_some_and(|native| native.user_read_only),
        };
        let protection = self
            .source_protection(&[request], true)?
            .pop()
            .ok_or(SourceSnapshotError::UnknownSource)?;
        if protection.support.objects.is_empty() {
            return Err(SourceSnapshotError::UnknownSource);
        }
        if let Some(native) = native {
            if native.version == 0 {
                return Err(SourceSnapshotError::InvalidEditorState);
            }
            if native.text.len() > MAX_SOURCE_SNAPSHOT_BYTES {
                return Err(SourceSnapshotError::TooLarge);
            }
            return Ok(SourceSnapshot::new(
                protection,
                SourceSnapshotOrigin::Editor,
                Some(native.version),
                Body::Text(native.text),
            ));
        }
        let physical = self
            .source
            .checked_file(path)
            .map_err(WorkspaceError::Source)?
            .ok_or(SourceSnapshotError::Missing)?;
        let mut bytes = Vec::new();
        File::open(physical)
            .and_then(|file| {
                file.take(MAX_SOURCE_SNAPSHOT_BYTES as u64 + 1)
                    .read_to_end(&mut bytes)
            })
            .map_err(|source| SourceSnapshotError::Io {
                path: path.to_path_buf(),
                source,
            })?;
        if bytes.len() > MAX_SOURCE_SNAPSHOT_BYTES {
            return Err(SourceSnapshotError::TooLarge);
        }
        Ok(SourceSnapshot::new(
            protection,
            SourceSnapshotOrigin::Disk,
            None,
            Body::Bytes(bytes.into()),
        ))
    }
}
