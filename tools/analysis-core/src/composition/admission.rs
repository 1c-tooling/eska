//! Explicit analyzer topology is validated against supplied ESKA readers, never native XML discovery.

use std::{
    collections::{BTreeMap, BTreeSet},
    fmt::Write as _,
    path::{Path, PathBuf},
};

use eska::project_core::{ProjectType, source_format::SourceReader};
use ide_db::metadata::{RootKind, WorkspaceConfigsSnapshot};
use project_model::{
    ExtensionDecl, ExtensionNodeSpec, ExtensionTopology, NodeKind, ProjectConfig, TopologyError,
};
use sha2::{Digest, Sha256};

use super::{IdentityError, RootId};

/// One verified reader retains the actual source profile and root header identity.
pub struct AdmittedRoot {
    id: RootId,
    reader: SourceReader,
    kind: RootKind,
}

/// No database inputs are published until this complete immutable admission succeeds.
pub struct AdmittedComposition {
    selected: RootId,
    roots: Vec<AdmittedRoot>,
    snapshot: WorkspaceConfigsSnapshot,
    /// Topology alone does not fingerprint the current source headers or manifests.
    source_sha256: String,
}

impl AdmittedRoot {
    /// Preserve the index that was validated against the complete topology.
    #[must_use]
    pub const fn id(&self) -> RootId {
        self.id
    }

    /// Expose the immutable source snapshot without allowing replacement of its admitted role.
    #[must_use]
    pub const fn reader(&self) -> &SourceReader {
        &self.reader
    }

    /// The validated source profile supplies the role, independently of path names or XML files.
    #[must_use]
    pub const fn kind(&self) -> RootKind {
        self.kind
    }

    /// Transfer this verified reader to its existing input controller without another filesystem open.
    #[must_use]
    pub fn into_reader(self) -> SourceReader {
        self.reader
    }
}

impl AdmittedComposition {
    /// The selected input remains part of this exact admitted source set.
    #[must_use]
    pub const fn selected(&self) -> RootId {
        self.selected
    }

    /// Inspection cannot mutate root order, source roles or the snapshot's derived closures.
    #[must_use]
    pub fn roots(&self) -> &[AdmittedRoot] {
        &self.roots
    }

    /// The complete snapshot is published only after all readers and dependencies were verified.
    #[must_use]
    pub const fn snapshot(&self) -> &WorkspaceConfigsSnapshot {
        &self.snapshot
    }

    /// This digest addresses source controls and topology, never support-policy approval.
    #[must_use]
    pub fn source_sha256(&self) -> &str {
        &self.source_sha256
    }

    /// The resident host takes ownership of one validated source set rather than cloning reader state.
    #[must_use]
    pub fn into_parts(self) -> (RootId, Vec<AdmittedRoot>, WorkspaceConfigsSnapshot, String) {
        (self.selected, self.roots, self.snapshot, self.source_sha256)
    }
}

/// Explicit refusals prevent a malformed or partial topology from becoming a narrower local context.
#[derive(Debug)]
pub enum AdmissionError {
    UnsupportedSourceSet,
    MissingBase,
    PathUnavailable(PathBuf),
    MissingReader(PathBuf),
    DuplicatePath(PathBuf),
    OverlappingRoots(PathBuf, PathBuf),
    UndeclaredReader(PathBuf),
    MissingSelectedRoot,
    RoleMismatch {
        path: PathBuf,
        expected: ProjectType,
        actual: ProjectType,
    },
    MissingHeader(PathBuf),
    DuplicateHeaderUuid(String),
    Source(eska::project_core::source_format::Error),
    Topology(TopologyError),
    Identity(IdentityError),
}

/// Build only explicitly declared CF/CFE topology from the caller's already opened source inputs.
///
/// Unset topology preserves a selected CF's single-root context. A CFE never becomes an anonymous base.
/// # Errors
/// Refuses unsupported source declarations, missing/duplicate roots, role or header mismatches and invalid dependencies.
pub fn admit(
    anchor: &Path,
    config: &ProjectConfig,
    readers: Vec<SourceReader>,
    selected: &Path,
) -> Result<AdmittedComposition, AdmissionError> {
    require_supported_settings(config)?;
    let selected = canonical(selected)?;
    let mut supplied = BTreeMap::new();
    for reader in readers {
        let root = reader.root().to_owned();
        if supplied.insert(root.clone(), reader).is_some() {
            return Err(AdmissionError::DuplicatePath(root));
        }
    }
    let base = config
        .resolve_configuration_path(anchor)
        .map_err(|_| AdmissionError::UnsupportedSourceSet)?
        .unwrap_or_else(|| selected.clone());
    let canonical_base = canonical(&base)?;
    let mut paths = vec![(None, base)];
    let mut canonical_paths = vec![canonical_base.clone()];
    let mut specs = Vec::new();
    for declaration in config.extensions.iter().flatten() {
        let ExtensionDecl::Structured(extension) = declaration else {
            return Err(AdmissionError::UnsupportedSourceSet);
        };
        if extension.path.is_empty() || extension.path.contains('*') {
            return Err(AdmissionError::UnsupportedSourceSet);
        }
        let path = anchor.join(&extension.path);
        let canonical_path = canonical(&path)?;
        if canonical_paths.contains(&canonical_path) {
            return Err(AdmissionError::DuplicatePath(canonical_path));
        }
        specs.push(ExtensionNodeSpec {
            kind: NodeKind::Extension,
            name: extension.name.clone(),
            path: path.clone(),
            canonical_path: canonical_path.clone(),
            depends_on: Some(extension.depends_on.clone()),
            structured: true,
        });
        paths.push((Some(extension.name.clone()), path));
        canonical_paths.push(canonical_path);
    }
    require_disjoint_roots(&canonical_paths)?;
    let topology =
        ExtensionTopology::build(&canonical_base, specs).map_err(AdmissionError::Topology)?;
    let mut roots = Vec::with_capacity(paths.len());
    let mut header_uuids = BTreeSet::new();
    for (index, path) in canonical_paths.iter().enumerate() {
        let reader = supplied
            .remove(path)
            .ok_or_else(|| AdmissionError::MissingReader(path.clone()))?;
        let expected = if index == 0 {
            ProjectType::Configuration
        } else {
            ProjectType::Extension
        };
        require_role(&reader, expected, config.configuration_root.is_none())?;
        let header = reader
            .configuration()
            .declarations
            .first()
            .ok_or_else(|| AdmissionError::MissingHeader(path.clone()))?;
        let uuid = header.uuid.as_str().to_ascii_lowercase();
        if !header_uuids.insert(uuid.clone()) {
            return Err(AdmissionError::DuplicateHeaderUuid(uuid));
        }
        roots.push(AdmittedRoot {
            id: RootId::from_index(index).map_err(AdmissionError::Identity)?,
            reader,
            kind: if index == 0 {
                RootKind::Base
            } else {
                RootKind::Extension
            },
        });
    }
    if let Some(path) = supplied.into_keys().next() {
        return Err(AdmissionError::UndeclaredReader(path));
    }
    let selected = canonical_paths
        .iter()
        .position(|path| path == &selected)
        .ok_or(AdmissionError::MissingSelectedRoot)
        .and_then(|index| RootId::from_index(index).map_err(AdmissionError::Identity))?;
    let snapshot = snapshot(paths, canonical_paths, &roots, &topology);
    let source_sha256 = fingerprint(&roots, &topology, selected);
    Ok(AdmittedComposition {
        selected,
        roots,
        snapshot,
        source_sha256,
    })
}

/// Explicit unsupported declarations remain errors rather than enabling native discovery or exclusions.
fn require_supported_settings(config: &ProjectConfig) -> Result<(), AdmissionError> {
    if config.configuration_dependency.is_some()
        || !config.source_exclude.is_empty()
        || config
            .externals
            .as_ref()
            .is_some_and(|entries| !entries.is_empty())
        || config
            .configuration_root
            .as_ref()
            .is_some_and(|path| path.is_empty() || path.contains('*'))
    {
        return Err(AdmissionError::UnsupportedSourceSet);
    }
    Ok(())
}

/// Filesystem identity is exact; a missing configured path never becomes a lexical fallback.
fn canonical(path: &Path) -> Result<PathBuf, AdmissionError> {
    std::fs::canonicalize(path).map_err(|_| AdmissionError::PathUnavailable(path.to_owned()))
}

/// Overlapping roots could assign one physical input two different session-local identities.
fn require_disjoint_roots(paths: &[PathBuf]) -> Result<(), AdmissionError> {
    for (index, first) in paths.iter().enumerate() {
        for second in &paths[index + 1..] {
            if first.starts_with(second) || second.starts_with(first) {
                return Err(AdmissionError::OverlappingRoots(
                    first.clone(),
                    second.clone(),
                ));
            }
        }
    }
    Ok(())
}

/// Only the actual reversible descriptor profile establishes each root's semantic role.
fn require_role(
    reader: &SourceReader,
    expected: ProjectType,
    implicit_base: bool,
) -> Result<(), AdmissionError> {
    let actual = reader.project_type().map_err(AdmissionError::Source)?;
    if actual != expected {
        if implicit_base && expected == ProjectType::Configuration {
            return Err(AdmissionError::MissingBase);
        }
        return Err(AdmissionError::RoleMismatch {
            path: reader.root().to_owned(),
            expected,
            actual,
        });
    }
    Ok(())
}

/// One snapshot applies configured spellings, canonical identities and transitive closures atomically.
fn snapshot(
    paths: Vec<(Option<String>, PathBuf)>,
    canonical_paths: Vec<PathBuf>,
    roots: &[AdmittedRoot],
    topology: &ExtensionTopology,
) -> WorkspaceConfigsSnapshot {
    WorkspaceConfigsSnapshot {
        paths,
        canonical_paths,
        kinds: roots.iter().map(|root| root.kind).collect(),
        closures: std::iter::once(Vec::new())
            .chain(
                topology
                    .nodes()
                    .iter()
                    .map(|node| node.closure().iter().map(|id| id.index() + 1).collect()),
            )
            .collect(),
        topological_order: std::iter::once(0)
            .chain(topology.topological_order().iter().map(|id| id.index() + 1))
            .collect(),
        fingerprint: Some(topology.fingerprint().to_hex()),
        ..WorkspaceConfigsSnapshot::from_paths(Vec::new())
    }
}

/// Domain-separated exact source controls complement the topology's existing path/edge digest.
fn fingerprint(roots: &[AdmittedRoot], topology: &ExtensionTopology, selected: RootId) -> String {
    let mut hash = Sha256::new();
    field(&mut hash, b"eska-analysis/source-composition/v1");
    field(&mut hash, topology.fingerprint().as_bytes());
    hash.update(selected.index().to_le_bytes());
    for root in roots {
        let header = root.reader.configuration();
        hash.update(root.id.index().to_le_bytes());
        field(&mut hash, header.manifest_sha256.as_bytes());
        field(&mut hash, header.source_sha256.as_bytes());
    }
    hash.finalize()
        .iter()
        .fold(String::with_capacity(64), |mut text, byte| {
            let _ = write!(text, "{byte:02x}");
            text
        })
}

/// Length prefixes keep adjacent variable-size controls distinct without a serialization dependency.
fn field(hash: &mut Sha256, value: &[u8]) {
    hash.update((value.len() as u64).to_le_bytes());
    hash.update(value);
}
