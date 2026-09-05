//! Reusable semantic ownership pipeline over file-level project changes.

use std::{
    collections::{BTreeMap, BTreeSet},
    fmt,
    path::{Path, PathBuf},
};

use gix::bstr::{BStr, BString, ByteSlice};

use super::{
    Project,
    diff::{ProjectDiff, RevisionProjectDiff},
    metadata::{self, MetadataPart, MetadataPath},
    object_model::{LogicalObject, ObjectId, ObjectModel},
};
use crate::vcs::{
    diff::ResolvedCommit,
    repository::{Error as RepositoryError, Repository},
    status::Change,
};

const MD_NAMESPACE: &str = "http://v8.1c.ru/8.3/MDClasses";

/// The comparison edge represented by one file change.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum ChangeStage {
    /// HEAD to index in a workspace comparison.
    Index,
    /// Index to worktree in a workspace comparison.
    Worktree,
    /// One committed tree to another committed tree.
    Revision,
}

impl ChangeStage {
    /// Return the stable machine-facing comparison edge name.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Index => "index",
            Self::Worktree => "worktree",
            Self::Revision => "revision",
        }
    }
}

/// One normalized file-level change retaining its exact comparison edge.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ChangedPath {
    path: BString,
    stage: ChangeStage,
    change: Change,
}

impl ChangedPath {
    /// Return the project-relative path without lossy UTF-8 conversion.
    #[must_use]
    pub fn path(&self) -> &BStr {
        self.path.as_bstr()
    }

    /// Return the comparison edge that produced this change.
    #[must_use]
    pub const fn stage(&self) -> ChangeStage {
        self.stage
    }

    /// Return the normalized file state reported by the repository layer.
    #[must_use]
    pub const fn change(&self) -> Change {
        self.change
    }
}

/// Deterministic set of file changes accepted by semantic analyzers.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ChangeSet {
    changes: Vec<ChangedPath>,
}

impl ChangeSet {
    /// Normalize both workspace comparison edges from a file-level project diff.
    #[must_use]
    pub fn from_workspace(diff: &ProjectDiff) -> Self {
        let changes = diff.files.iter().flat_map(|file| {
            [
                file.index.map(|change| ChangedPath {
                    path: file.path.clone(),
                    stage: ChangeStage::Index,
                    change,
                }),
                file.worktree.map(|change| ChangedPath {
                    path: file.path.clone(),
                    stage: ChangeStage::Worktree,
                    change,
                }),
            ]
            .into_iter()
            .flatten()
        });
        Self::normalized(changes)
    }

    /// Normalize one committed-tree comparison from a revision project diff.
    #[must_use]
    pub fn from_revision(diff: &RevisionProjectDiff) -> Self {
        Self::normalized(diff.files.iter().map(|file| ChangedPath {
            path: file.path.clone(),
            stage: ChangeStage::Revision,
            change: file.change,
        }))
    }

    /// Return normalized changes sorted by path and comparison stage.
    #[must_use]
    pub fn changes(&self) -> &[ChangedPath] {
        &self.changes
    }

    /// Return whether the set contains no file changes.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.changes.is_empty()
    }

    /// Sort changes and conservatively merge duplicate path/stage entries.
    fn normalized(changes: impl IntoIterator<Item = ChangedPath>) -> Self {
        let mut normalized = BTreeMap::new();
        for change in changes {
            normalized
                .entry((change.path, change.stage))
                .and_modify(|current| *current = merge_change(*current, change.change))
                .or_insert(change.change);
        }
        Self {
            changes: normalized
                .into_iter()
                .map(|((path, stage), change)| ChangedPath {
                    path,
                    stage,
                    change,
                })
                .collect(),
        }
    }
}

/// Semantic role of one changed path relative to an affected logical object.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum ObjectPathRole {
    Descriptor,
    Module,
    Form,
    Artifact,
}

/// One file change attributed to one logical metadata object.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ObjectPathChange {
    path: BString,
    stage: ChangeStage,
    change: Change,
    role: ObjectPathRole,
}

impl ObjectPathChange {
    /// Return the original project-relative path.
    #[must_use]
    pub fn path(&self) -> &BStr {
        self.path.as_bstr()
    }

    /// Return the comparison edge of the attributed change.
    #[must_use]
    pub const fn stage(&self) -> ChangeStage {
        self.stage
    }

    /// Return the normalized file state.
    #[must_use]
    pub const fn change(&self) -> Change {
        self.change
    }

    /// Return how the path participates in the logical object.
    #[must_use]
    pub const fn role(&self) -> ObjectPathRole {
        self.role
    }
}

/// All file changes attributed to one stable logical object.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ObjectChangeSummary {
    id: ObjectId,
    metadata_type: &'static str,
    name: String,
    changes: Vec<ObjectPathChange>,
}

impl ObjectChangeSummary {
    /// Return the stable logical object identifier.
    #[must_use]
    pub const fn id(&self) -> &ObjectId {
        &self.id
    }

    /// Return the stable machine-facing metadata type.
    #[must_use]
    pub const fn metadata_type(&self) -> &'static str {
        self.metadata_type
    }

    /// Return the metadata name.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Return all changes attributed to this object in deterministic order.
    #[must_use]
    pub fn changes(&self) -> &[ObjectPathChange] {
        &self.changes
    }
}

/// Aggregate counts over normalized comparison edges.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ChangeCounts {
    pub added: usize,
    pub modified: usize,
    pub deleted: usize,
    pub type_changed: usize,
    pub untracked: usize,
    pub intent_to_add: usize,
    pub conflicts: usize,
}

impl ChangeCounts {
    /// Record one normalized repository change.
    const fn record(&mut self, change: Change) {
        match change {
            Change::Added => self.added += 1,
            Change::Modified => self.modified += 1,
            Change::Deleted => self.deleted += 1,
            Change::TypeChanged => self.type_changed += 1,
            Change::Untracked => self.untracked += 1,
            Change::IntentToAdd => self.intent_to_add += 1,
            Change::Conflict => self.conflicts += 1,
        }
    }
}

/// Deterministic semantic ownership projection of one `ChangeSet`.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ChangeSummary {
    files: usize,
    counts: ChangeCounts,
    objects: Vec<ObjectChangeSummary>,
    unowned: Vec<ChangedPath>,
}

impl ChangeSummary {
    /// Return the number of unique project-relative paths.
    #[must_use]
    pub const fn files(&self) -> usize {
        self.files
    }

    /// Return counts over comparison edges, preserving staged/unstaged separation.
    #[must_use]
    pub const fn counts(&self) -> ChangeCounts {
        self.counts
    }

    /// Return affected logical objects in stable `ObjectId` order.
    #[must_use]
    pub fn objects(&self) -> &[ObjectChangeSummary] {
        &self.objects
    }

    /// Return changes outside the source tree or without a discovered owner.
    #[must_use]
    pub fn unowned_changes(&self) -> &[ChangedPath] {
        &self.unowned
    }
}

/// Stateless analyzer that projects changed paths through one Designer object model.
#[derive(Debug)]
pub struct SemanticChangeAnalyzer<'a> {
    project: &'a Project,
    objects: &'a ObjectModel,
}

impl<'a> SemanticChangeAnalyzer<'a> {
    /// Bind the analyzer to matching project and object-model snapshots.
    #[must_use]
    pub const fn new(project: &'a Project, objects: &'a ObjectModel) -> Self {
        Self { project, objects }
    }

    /// Attribute file-level changes to logical objects without parsing file contents.
    #[must_use]
    pub fn analyze(&self, changes: &ChangeSet) -> ChangeSummary {
        let mut files = BTreeSet::new();
        let mut counts = ChangeCounts::default();
        let mut objects: BTreeMap<ObjectId, ObjectChangeSummary> = BTreeMap::new();
        let mut unowned = Vec::new();

        for change in changes.changes() {
            files.insert(change.path.clone());
            counts.record(change.change);
            let Some(source_path) = self.source_relative_path(change.path()) else {
                unowned.push(change.clone());
                continue;
            };
            let owners = self.objects.objects_for_changed_path(&source_path);
            if owners.is_empty() {
                unowned.push(change.clone());
                continue;
            }
            for owner in owners {
                let value =
                    objects
                        .entry(owner.id().clone())
                        .or_insert_with(|| ObjectChangeSummary {
                            id: owner.id().clone(),
                            metadata_type: owner.metadata_type(),
                            name: owner.name().to_owned(),
                            changes: Vec::new(),
                        });
                value.changes.push(ObjectPathChange {
                    path: change.path.clone(),
                    stage: change.stage,
                    change: change.change,
                    role: object_path_role(owner, &source_path),
                });
            }
        }

        ChangeSummary {
            files: files.len(),
            counts,
            objects: objects.into_values().collect(),
            unowned,
        }
    }

    /// Convert a project-relative byte path into a source-relative platform path.
    fn source_relative_path(&self, project_path: &BStr) -> Option<PathBuf> {
        let absolute = self.project.root().join(gix::path::from_bstr(project_path));
        absolute
            .strip_prefix(self.project.source())
            .ok()
            .map(Path::to_path_buf)
    }
}

/// Classify one changed path relative to its resolved logical owner.
fn object_path_role(owner: &LogicalObject, source_path: &Path) -> ObjectPathRole {
    if owner.module_paths().any(|path| path == source_path) {
        ObjectPathRole::Module
    } else if owner.metadata_type() == "form" {
        ObjectPathRole::Form
    } else if owner.descriptor_path() == source_path {
        ObjectPathRole::Descriptor
    } else {
        ObjectPathRole::Artifact
    }
}

/// Stable semantic event emitted from one pair of Designer source snapshots.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct SemanticEvent {
    kind: SemanticEventKind,
    stage: ChangeStage,
    object: SemanticObject,
    member: Option<String>,
    path: BString,
}

impl SemanticEvent {
    /// Return the stable event kind.
    #[must_use]
    pub const fn kind(&self) -> SemanticEventKind {
        self.kind
    }

    /// Return the comparison edge that produced the event.
    #[must_use]
    pub const fn stage(&self) -> ChangeStage {
        self.stage
    }

    /// Return the affected logical object.
    #[must_use]
    pub const fn object(&self) -> &SemanticObject {
        &self.object
    }

    /// Return the affected procedure or function name when applicable.
    #[must_use]
    pub fn member(&self) -> Option<&str> {
        self.member.as_deref()
    }

    /// Return the original project-relative source path.
    #[must_use]
    pub fn path(&self) -> &BStr {
        self.path.as_bstr()
    }
}

/// Locale-independent classification of reliable semantic changes.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum SemanticEventKind {
    ObjectAdded,
    ObjectRemoved,
    ObjectChanged,
    ModuleChanged,
    MethodAdded,
    MethodRemoved,
    MethodChanged,
    FunctionAdded,
    FunctionRemoved,
    FunctionChanged,
    FormChanged,
    MetadataAttributeChanged,
}

impl SemanticEventKind {
    /// Return the stable JSON and raw-output value.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::ObjectAdded => "object_added",
            Self::ObjectRemoved => "object_removed",
            Self::ObjectChanged => "object_changed",
            Self::ModuleChanged => "module_changed",
            Self::MethodAdded => "method_added",
            Self::MethodRemoved => "method_removed",
            Self::MethodChanged => "method_changed",
            Self::FunctionAdded => "function_added",
            Self::FunctionRemoved => "function_removed",
            Self::FunctionChanged => "function_changed",
            Self::FormChanged => "form_changed",
            Self::MetadataAttributeChanged => "metadata_attribute_changed",
        }
    }
}

/// Stable identity fields shared by human and machine presentations.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct SemanticObject {
    id: String,
    metadata_type: &'static str,
    name: String,
}

impl SemanticObject {
    /// Return the readable hierarchical identity.
    #[must_use]
    pub fn id(&self) -> &str {
        &self.id
    }

    /// Return the stable Designer metadata type.
    #[must_use]
    pub const fn metadata_type(&self) -> &'static str {
        self.metadata_type
    }

    /// Return the Designer object name.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }
}

/// Deterministically ordered semantic events for one comparison.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct SemanticDiff {
    events: Vec<SemanticEvent>,
}

impl SemanticDiff {
    /// Return semantic events sorted by kind, stage, object, member and path.
    #[must_use]
    pub fn events(&self) -> &[SemanticEvent] {
        &self.events
    }

    /// Return whether no reliable semantic event was detected.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.events.is_empty()
    }
}

/// Failures while loading exact before/after snapshots for semantic analysis.
#[derive(Debug)]
pub enum SemanticDiffError {
    Repository(RepositoryError),
    ProjectOutsideRepository {
        project: PathBuf,
        repository: PathBuf,
    },
}

impl fmt::Display for SemanticDiffError {
    /// Render a locale-independent diagnostic for library callers.
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Repository(_) => formatter.write_str("repository operation failed"),
            Self::ProjectOutsideRepository {
                project,
                repository,
            } => write!(
                formatter,
                "project {} is outside repository {}",
                project.display(),
                repository.display()
            ),
        }
    }
}

impl std::error::Error for SemanticDiffError {
    /// Preserve repository causes for diagnostics.
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Repository(_) | Self::ProjectOutsideRepository { .. } => None,
        }
    }
}

/// Analyze current index and worktree edges from exact Git/file snapshots.
///
/// # Errors
/// Returns a structured error when the repository or a required snapshot cannot be read.
pub fn diff_workspace(
    project: &Project,
    objects: &ObjectModel,
    diff: &ProjectDiff,
) -> Result<SemanticDiff, SemanticDiffError> {
    let repository = semantic_repository(project)?;
    let prefix = repository_project_prefix(&repository, project);
    let mut events = BTreeSet::new();
    for file in &diff.files {
        let source_path = source_relative_path(project, file.path.as_bstr());
        let Some(source_path) = source_path else {
            continue;
        };
        let repository_path = join_git_path(prefix.as_bstr(), file.path.as_bstr());
        let versions = repository
            .file_versions(repository_path.as_bstr())
            .map_err(SemanticDiffError::Repository)?;
        if let Some(change) = file.index {
            let snapshot = SnapshotChange {
                source_path: &source_path,
                project_path: file.path.clone(),
                stage: ChangeStage::Index,
                change,
                before: versions.head.as_deref(),
                after: versions.index.as_deref(),
            };
            analyze_snapshots(project, Some(objects), &snapshot, &mut events);
        }
        if let Some(change) = file.worktree {
            let snapshot = SnapshotChange {
                source_path: &source_path,
                project_path: file.path.clone(),
                stage: ChangeStage::Worktree,
                change,
                before: versions.index.as_deref(),
                after: versions.worktree.as_deref(),
            };
            analyze_snapshots(project, Some(objects), &snapshot, &mut events);
        }
    }
    Ok(SemanticDiff {
        events: events.into_iter().collect(),
    })
}

/// Analyze one committed comparison from exact tree blob pairs.
///
/// # Errors
/// Returns a structured error when commits, trees or blobs cannot be read.
pub fn diff_revisions(
    project: &Project,
    diff: &RevisionProjectDiff,
) -> Result<SemanticDiff, SemanticDiffError> {
    let repository = semantic_repository(project)?;
    let effective_from = diff
        .comparison
        .merge_base_commit
        .unwrap_or(diff.comparison.from_commit);
    let changes = repository
        .diff_commits(
            ResolvedCommit { id: effective_from },
            ResolvedCommit {
                id: diff.comparison.to_commit,
            },
        )
        .map_err(SemanticDiffError::Repository)?;
    let mut events = BTreeSet::new();
    for change in changes {
        let Some(project_path) = project_relative_path(&repository, project, change.path.as_bstr())
        else {
            continue;
        };
        let Some(source_path) = source_relative_path(project, project_path.as_bstr()) else {
            continue;
        };
        let before = load_blob(&repository, change.before)?;
        let after = load_blob(&repository, change.after)?;
        let snapshot = SnapshotChange {
            source_path: &source_path,
            project_path,
            stage: ChangeStage::Revision,
            change: change.change,
            before: before.as_deref(),
            after: after.as_deref(),
        };
        analyze_snapshots(project, None, &snapshot, &mut events);
    }
    Ok(SemanticDiff {
        events: events.into_iter().collect(),
    })
}

/// Open the containing worktree and enforce project scoping.
fn semantic_repository(project: &Project) -> Result<Repository, SemanticDiffError> {
    let repository = Repository::discover(project.root()).map_err(SemanticDiffError::Repository)?;
    if !project.root().starts_with(repository.work_dir()) {
        return Err(SemanticDiffError::ProjectOutsideRepository {
            project: project.root().to_owned(),
            repository: repository.work_dir().to_owned(),
        });
    }
    Ok(repository)
}

/// Return the repository-relative prefix of the project root.
fn repository_project_prefix(repository: &Repository, project: &Project) -> BString {
    let relative = project
        .root()
        .strip_prefix(repository.work_dir())
        .expect("semantic repository preflight checked containment");
    gix::path::to_unix_separators_on_windows(gix::path::into_bstr(relative)).into_owned()
}

/// Join two Git byte paths without converting either through UTF-8.
fn join_git_path(prefix: &BStr, path: &BStr) -> BString {
    if prefix.is_empty() {
        return path.to_owned();
    }
    let mut joined = prefix.to_owned();
    joined.push(b'/');
    joined.extend_from_slice(path);
    joined
}

/// Scope one repository-relative path to this project.
fn project_relative_path(
    repository: &Repository,
    project: &Project,
    repository_path: &BStr,
) -> Option<BString> {
    let absolute = repository
        .work_dir()
        .join(gix::path::from_bstr(repository_path));
    let relative = absolute.strip_prefix(project.root()).ok()?;
    Some(gix::path::to_unix_separators_on_windows(gix::path::into_bstr(relative)).into_owned())
}

/// Scope one project-relative path to its configured source directory.
fn source_relative_path(project: &Project, project_path: &BStr) -> Option<PathBuf> {
    project
        .root()
        .join(gix::path::from_bstr(project_path))
        .strip_prefix(project.source())
        .ok()
        .map(Path::to_path_buf)
}

/// Read an optional tree blob while preserving absence as one comparison endpoint.
fn load_blob(
    repository: &Repository,
    id: Option<gix::ObjectId>,
) -> Result<Option<Vec<u8>>, SemanticDiffError> {
    id.map(|id| repository.blob(id).map_err(SemanticDiffError::Repository))
        .transpose()
}

/// Exact endpoint pair for one normalized comparison edge.
struct SnapshotChange<'a> {
    source_path: &'a Path,
    project_path: BString,
    stage: ChangeStage,
    change: Change,
    before: Option<&'a [u8]>,
    after: Option<&'a [u8]>,
}

/// Project one changed file into the most specific reliable semantic events.
fn analyze_snapshots(
    project: &Project,
    objects: Option<&ObjectModel>,
    snapshot: &SnapshotChange<'_>,
    events: &mut BTreeSet<SemanticEvent>,
) {
    let owners = objects
        .map(|objects| objects.objects_for_changed_path(snapshot.source_path))
        .unwrap_or_default();
    let owner = owners.first().map(|value| semantic_object(value));
    let project_type = project.configuration().project_type();
    let source_bytes =
        gix::path::to_unix_separators_on_windows(gix::path::into_bstr(snapshot.source_path));

    if metadata::is_object_descriptor(project_type, source_bytes.as_ref()) {
        analyze_descriptor(project_type, source_bytes.as_ref(), snapshot, owner, events);
        return;
    }

    let Some(object) = owner.or_else(|| fallback_object(project_type, source_bytes.as_ref(), None))
    else {
        return;
    };
    let is_module = is_module_path(snapshot.source_path);
    if is_module {
        emit(
            events,
            SemanticEventKind::ModuleChanged,
            snapshot.stage,
            object.clone(),
            None,
            snapshot.project_path.clone(),
        );
        analyze_routines(
            snapshot.before,
            snapshot.after,
            snapshot.stage,
            &object,
            &snapshot.project_path,
            events,
        );
    } else if object.metadata_type == "form" || is_form_artifact(snapshot.source_path) {
        emit(
            events,
            SemanticEventKind::FormChanged,
            snapshot.stage,
            object,
            None,
            snapshot.project_path.clone(),
        );
    }
}

/// Compare logical objects and their metadata properties inside one descriptor.
fn analyze_descriptor(
    project_type: super::ProjectType,
    source_path: &BStr,
    snapshot: &SnapshotChange<'_>,
    fallback: Option<SemanticObject>,
    events: &mut BTreeSet<SemanticEvent>,
) {
    let base = metadata::from_path(project_type, source_path);
    let before_objects = base.as_ref().and_then(|base| {
        snapshot
            .before
            .and_then(|contents| descriptor_objects(contents, base))
    });
    let after_objects = base.as_ref().and_then(|base| {
        snapshot
            .after
            .and_then(|contents| descriptor_objects(contents, base))
    });

    if let (Some(before), Some(after)) = (&before_objects, &after_objects) {
        compare_descriptor_objects(before, after, snapshot, events);
        return;
    }

    let parsed = match snapshot.change {
        Change::Added | Change::Untracked | Change::IntentToAdd => after_objects,
        Change::Deleted => before_objects,
        Change::Modified | Change::TypeChanged | Change::Conflict => None,
    };
    if let Some(objects) = parsed {
        emit_descriptor_lifecycle(objects, snapshot, events);
        return;
    }

    let object = fallback.or_else(|| {
        fallback_object(
            project_type,
            source_path,
            snapshot.before.or(snapshot.after),
        )
    });
    emit_descriptor_fallback(object, snapshot, events);
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct DescriptorObject {
    object: SemanticObject,
    properties: String,
}

/// Emit precise lifecycle and property changes from two parsed descriptors.
fn compare_descriptor_objects(
    before: &BTreeMap<String, DescriptorObject>,
    after: &BTreeMap<String, DescriptorObject>,
    snapshot: &SnapshotChange<'_>,
    events: &mut BTreeSet<SemanticEvent>,
) {
    let mut keys: BTreeSet<_> = before.keys().cloned().collect();
    keys.extend(after.keys().cloned());
    for key in keys {
        match (before.get(&key), after.get(&key)) {
            (None, Some(current)) => emit(
                events,
                SemanticEventKind::ObjectAdded,
                snapshot.stage,
                current.object.clone(),
                None,
                snapshot.project_path.clone(),
            ),
            (Some(previous), None) => emit(
                events,
                SemanticEventKind::ObjectRemoved,
                snapshot.stage,
                previous.object.clone(),
                None,
                snapshot.project_path.clone(),
            ),
            (Some(previous), Some(current)) if previous.properties != current.properties => {
                emit(
                    events,
                    SemanticEventKind::ObjectChanged,
                    snapshot.stage,
                    current.object.clone(),
                    None,
                    snapshot.project_path.clone(),
                );
                emit(
                    events,
                    SemanticEventKind::MetadataAttributeChanged,
                    snapshot.stage,
                    current.object.clone(),
                    None,
                    snapshot.project_path.clone(),
                );
                if matches!(current.object.metadata_type, "form" | "common-form") {
                    emit(
                        events,
                        SemanticEventKind::FormChanged,
                        snapshot.stage,
                        current.object.clone(),
                        None,
                        snapshot.project_path.clone(),
                    );
                }
            }
            _ => {}
        }
    }
}

/// Emit all objects from a parsed added or removed descriptor.
fn emit_descriptor_lifecycle(
    objects: BTreeMap<String, DescriptorObject>,
    snapshot: &SnapshotChange<'_>,
    events: &mut BTreeSet<SemanticEvent>,
) {
    let kind = match snapshot.change {
        Change::Added | Change::Untracked | Change::IntentToAdd => SemanticEventKind::ObjectAdded,
        Change::Deleted => SemanticEventKind::ObjectRemoved,
        Change::Modified | Change::TypeChanged | Change::Conflict => return,
    };
    for descriptor in objects.into_values() {
        emit(
            events,
            kind,
            snapshot.stage,
            descriptor.object,
            None,
            snapshot.project_path.clone(),
        );
    }
}

/// Emit a conservative owner-level event when a descriptor is not parseable at both endpoints.
fn emit_descriptor_fallback(
    object: Option<SemanticObject>,
    snapshot: &SnapshotChange<'_>,
    events: &mut BTreeSet<SemanticEvent>,
) {
    let Some(object) = object else {
        return;
    };
    let kind = match snapshot.change {
        Change::Added | Change::Untracked | Change::IntentToAdd => SemanticEventKind::ObjectAdded,
        Change::Deleted => SemanticEventKind::ObjectRemoved,
        Change::Modified | Change::TypeChanged | Change::Conflict => {
            SemanticEventKind::ObjectChanged
        }
    };
    emit(
        events,
        kind,
        snapshot.stage,
        object,
        None,
        snapshot.project_path.clone(),
    );
}

/// Parse object identities and property payloads without depending on formatting elsewhere.
fn descriptor_objects(
    contents: &[u8],
    base: &MetadataPath,
) -> Option<BTreeMap<String, DescriptorObject>> {
    let text = std::str::from_utf8(contents)
        .ok()?
        .trim_start_matches('\u{feff}');
    let document = roxmltree::Document::parse_with_options(
        text,
        roxmltree::ParsingOptions {
            nodes_limit: 1_000_000,
            ..Default::default()
        },
    )
    .ok()?;
    let root = document.root_element();
    if !root.has_tag_name((MD_NAMESPACE, "MetaDataObject")) {
        return None;
    }
    let mut roots = root.children().filter(roxmltree::Node::is_element);
    let object = roots.next()?;
    if roots.next().is_some() || object.tag_name().namespace() != Some(MD_NAMESPACE) {
        return None;
    }
    let mut result = BTreeMap::new();
    collect_descriptor_snapshots(object, base, &mut result)?;
    Some(result)
}

/// Recursively retain each supported inline object as an independent semantic identity.
fn collect_descriptor_snapshots(
    node: roxmltree::Node<'_, '_>,
    logical_path: &MetadataPath,
    output: &mut BTreeMap<String, DescriptorObject>,
) -> Option<()> {
    let metadata_type = metadata::kind_from_tag(node.tag_name().name())?;
    let properties = node
        .children()
        .find(|child| child.has_tag_name((MD_NAMESPACE, "Properties")))?;
    let name = properties
        .children()
        .find(|child| child.has_tag_name((MD_NAMESPACE, "Name")))?
        .text()?
        .to_owned();
    let mut path = logical_path.clone();
    if let Some(last) = path.parts.last_mut() {
        last.kind = metadata_type;
        last.name = Some(name);
    }
    let object = semantic_object_from_path(&path)?;
    output.insert(
        object.id.clone(),
        DescriptorObject {
            object,
            properties: xml_signature(properties),
        },
    );
    if let Some(children) = node
        .children()
        .find(|child| child.has_tag_name((MD_NAMESPACE, "ChildObjects")))
    {
        for child in children.children().filter(roxmltree::Node::is_element) {
            let Some(kind) = metadata::kind_from_tag(child.tag_name().name()) else {
                continue;
            };
            let Some(child_name) = child
                .children()
                .find(|item| item.has_tag_name((MD_NAMESPACE, "Properties")))
                .and_then(|item| {
                    item.children()
                        .find(|property| property.has_tag_name((MD_NAMESPACE, "Name")))
                })
                .and_then(|item| item.text())
            else {
                continue;
            };
            let nested = path.with_suffix(&[MetadataPart {
                kind,
                name: Some(child_name.to_owned()),
            }]);
            collect_descriptor_snapshots(child, &nested, output)?;
        }
    }
    Some(())
}

/// Build a formatting-independent signature of one XML subtree.
fn xml_signature(node: roxmltree::Node<'_, '_>) -> String {
    use std::fmt::Write as _;

    let mut signature = String::new();
    if node.is_element() {
        signature.push('<');
        signature.push_str(node.tag_name().name());
        let mut attributes: Vec<_> = node.attributes().collect();
        attributes.sort_by_key(|attribute| (attribute.namespace(), attribute.name()));
        for attribute in attributes {
            write!(signature, " {}={:?}", attribute.name(), attribute.value())
                .expect("writing to String cannot fail");
        }
        signature.push('>');
    } else if let Some(text) = node.text() {
        signature.push_str(text.trim());
    }
    for child in node.children() {
        signature.push_str(&xml_signature(child));
    }
    if node.is_element() {
        signature.push_str("</");
        signature.push_str(node.tag_name().name());
        signature.push('>');
    }
    signature
}

/// Convert one current object-model entry into a presentation-neutral identity.
fn semantic_object(object: &LogicalObject) -> SemanticObject {
    SemanticObject {
        id: object.id().as_str().to_owned(),
        metadata_type: object.metadata_type(),
        name: object.name().to_owned(),
    }
}

/// Build a conservative identity when the object no longer exists in the worktree model.
fn fallback_object(
    project_type: super::ProjectType,
    source_path: &BStr,
    descriptor: Option<&[u8]>,
) -> Option<SemanticObject> {
    let mut path = metadata::from_path(project_type, source_path)?;
    if let Some(objects) = descriptor.and_then(|contents| descriptor_objects(contents, &path)) {
        return objects.into_values().next().map(|value| value.object);
    }
    if path.parts.last().is_some_and(|part| part.name.is_none()) && path.parts.len() > 1 {
        path.parts.pop();
    }
    semantic_object_from_path(&path)
}

/// Construct the same readable segment format used by the logical object model.
fn semantic_object_from_path(path: &MetadataPath) -> Option<SemanticObject> {
    let last = path.parts.last()?;
    let name = last.name.clone().unwrap_or_else(|| last.kind.to_owned());
    let id = path
        .parts
        .iter()
        .map(|part| {
            part.name.as_ref().map_or_else(
                || part.kind.to_owned(),
                |name| format!("{}:{}", part.kind, escape_id_name(name)),
            )
        })
        .collect::<Vec<_>>()
        .join("/");
    Some(SemanticObject {
        id,
        metadata_type: last.kind,
        name,
    })
}

/// Escape stable identity separators in Designer names.
fn escape_id_name(name: &str) -> String {
    name.replace('%', "%25")
        .replace('/', "%2F")
        .replace(':', "%3A")
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
enum RoutineKind {
    Method,
    Function,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct RoutineSnapshot {
    kind: RoutineKind,
    name: String,
    body: String,
}

/// Compare top-level BSL procedures and functions when both endpoint modules are UTF-8.
fn analyze_routines(
    before: Option<&[u8]>,
    after: Option<&[u8]>,
    stage: ChangeStage,
    object: &SemanticObject,
    path: &BString,
    events: &mut BTreeSet<SemanticEvent>,
) {
    let before = before.and_then(parse_routines);
    let after = after.and_then(parse_routines);
    let (Some(before), Some(after)) = (before, after) else {
        return;
    };
    let mut keys: BTreeSet<_> = before.keys().cloned().collect();
    keys.extend(after.keys().cloned());
    for key in keys {
        let (kind, member) = match (before.get(&key), after.get(&key)) {
            (None, Some(routine)) => (
                routine_event_kind(routine.kind, Change::Added),
                routine.name.clone(),
            ),
            (Some(routine), None) => (
                routine_event_kind(routine.kind, Change::Deleted),
                routine.name.clone(),
            ),
            (Some(previous), Some(current)) if previous.body != current.body => (
                routine_event_kind(current.kind, Change::Modified),
                current.name.clone(),
            ),
            _ => continue,
        };
        emit(
            events,
            kind,
            stage,
            object.clone(),
            Some(member),
            path.clone(),
        );
    }
}

/// Parse only unambiguous top-level BSL declarations and their complete bodies.
fn parse_routines(contents: &[u8]) -> Option<BTreeMap<(RoutineKind, String), RoutineSnapshot>> {
    let text = std::str::from_utf8(contents).ok()?.replace("\r\n", "\n");
    let lines: Vec<_> = text.lines().collect();
    let mut routines = BTreeMap::new();
    let mut index = 0;
    while index < lines.len() {
        let Some((kind, name)) = routine_declaration(lines[index]) else {
            index += 1;
            continue;
        };
        let start = index;
        index += 1;
        while index < lines.len() && !routine_end(lines[index], kind) {
            index += 1;
        }
        if index == lines.len() {
            return None;
        }
        let body = lines[start..=index]
            .iter()
            .map(|line| line.trim_end())
            .collect::<Vec<_>>()
            .join("\n");
        let key = (kind, name.to_lowercase());
        if routines
            .insert(key, RoutineSnapshot { kind, name, body })
            .is_some()
        {
            return None;
        }
        index += 1;
    }
    Some(routines)
}

/// Recognize Russian and English BSL declaration keywords at the start of a line.
fn routine_declaration(line: &str) -> Option<(RoutineKind, String)> {
    let trimmed = line.trim_start();
    if trimmed.starts_with("//") {
        return None;
    }
    let lowered = trimmed.to_lowercase();
    let async_prefix = ["асинх ", "async "]
        .into_iter()
        .find(|prefix| lowered.starts_with(prefix));
    let declaration = async_prefix.map_or(trimmed, |prefix| &trimmed[prefix.len()..]);
    let lowered = declaration.to_lowercase();
    let (kind, keyword) = [
        (RoutineKind::Method, "процедура "),
        (RoutineKind::Method, "procedure "),
        (RoutineKind::Function, "функция "),
        (RoutineKind::Function, "function "),
    ]
    .into_iter()
    .find(|(_, keyword)| lowered.starts_with(keyword))?;
    let remainder = &declaration[keyword.len()..];
    let name = remainder.split_once('(')?.0.trim();
    (!name.is_empty()
        && name
            .chars()
            .all(|value| value == '_' || value.is_alphanumeric()))
    .then(|| (kind, name.to_owned()))
}

/// Recognize the matching Russian or English end keyword.
fn routine_end(line: &str, kind: RoutineKind) -> bool {
    let lowered = line.trim_start().to_lowercase();
    let keyword = match kind {
        RoutineKind::Method => ["конецпроцедуры", "endprocedure"],
        RoutineKind::Function => ["конецфункции", "endfunction"],
    };
    keyword.iter().any(|keyword| {
        lowered.strip_prefix(keyword).is_some_and(|suffix| {
            suffix.is_empty()
                || suffix.starts_with(char::is_whitespace)
                || suffix.starts_with(';')
                || suffix.starts_with("//")
        })
    })
}

/// Map a routine kind and lifecycle state to its stable event kind.
const fn routine_event_kind(kind: RoutineKind, change: Change) -> SemanticEventKind {
    match (kind, change) {
        (RoutineKind::Method, Change::Added) => SemanticEventKind::MethodAdded,
        (RoutineKind::Method, Change::Deleted) => SemanticEventKind::MethodRemoved,
        (RoutineKind::Method, _) => SemanticEventKind::MethodChanged,
        (RoutineKind::Function, Change::Added) => SemanticEventKind::FunctionAdded,
        (RoutineKind::Function, Change::Deleted) => SemanticEventKind::FunctionRemoved,
        (RoutineKind::Function, _) => SemanticEventKind::FunctionChanged,
    }
}

/// Recognize files whose content defines a managed or ordinary form.
fn is_form_artifact(path: &Path) -> bool {
    let components: Vec<_> = path
        .components()
        .filter_map(|component| component.as_os_str().to_str())
        .collect();
    components
        .windows(2)
        .any(|pair| pair[0] == "Ext" && matches!(pair[1], "Form" | "Form.xml" | "Form.bin"))
        || components.contains(&"Form.bin")
}

/// Recognize standard Designer BSL implementation module names.
fn is_module_path(path: &Path) -> bool {
    let Some(file) = path.file_name().and_then(|value| value.to_str()) else {
        return false;
    };
    matches!(
        file,
        "Module.bsl"
            | "Module.bin"
            | "ObjectModule.bsl"
            | "ObjectModule.bin"
            | "ManagerModule.bsl"
            | "ManagerModule.bin"
            | "RecordSetModule.bsl"
            | "RecordSetModule.bin"
            | "ValueManagerModule.bsl"
            | "ValueManagerModule.bin"
            | "ManagedApplicationModule.bsl"
            | "OrdinaryApplicationModule.bsl"
            | "SessionModule.bsl"
            | "ExternalConnectionModule.bsl"
            | "CommandModule.bsl"
    )
}

/// Insert one event into the deterministic de-duplicating set.
fn emit(
    events: &mut BTreeSet<SemanticEvent>,
    kind: SemanticEventKind,
    stage: ChangeStage,
    object: SemanticObject,
    member: Option<String>,
    path: BString,
) {
    events.insert(SemanticEvent {
        kind,
        stage,
        object,
        member,
        path,
    });
}

/// Conservatively combine duplicate states without losing conflicts.
fn merge_change(current: Change, incoming: Change) -> Change {
    if current == incoming {
        current
    } else if current == Change::Conflict || incoming == Change::Conflict {
        Change::Conflict
    } else {
        Change::Modified
    }
}

#[cfg(test)]
mod tests {
    use gix::bstr::ByteSlice;

    use super::{
        ChangeSet, ChangeStage, RoutineKind, SemanticEventKind, descriptor_objects, parse_routines,
    };
    use crate::{
        project::{
            ProjectType,
            diff::{FileChange, ProjectDiff},
            metadata,
        },
        vcs::status::Change,
    };

    /// Workspace conversion retains both comparison edges and deterministic ordering.
    #[test]
    fn normalizes_workspace_edges() {
        let diff = ProjectDiff {
            files: vec![
                FileChange {
                    path: b"src/B.bsl".as_bstr().to_owned(),
                    index: None,
                    worktree: Some(Change::Untracked),
                },
                FileChange {
                    path: b"src/A.bsl".as_bstr().to_owned(),
                    index: Some(Change::Modified),
                    worktree: Some(Change::Modified),
                },
            ],
            display: Vec::new(),
        };

        let changes = ChangeSet::from_workspace(&diff);

        assert_eq!(changes.changes().len(), 3);
        assert_eq!(changes.changes()[0].path(), b"src/A.bsl".as_bstr());
        assert_eq!(changes.changes()[0].stage(), ChangeStage::Index);
        assert_eq!(changes.changes()[1].stage(), ChangeStage::Worktree);
        assert_eq!(changes.changes()[2].path(), b"src/B.bsl".as_bstr());
    }

    /// BSL parsing distinguishes procedure and function lifecycle without matching comments.
    #[test]
    fn parses_complete_russian_and_english_routines() {
        let routines = parse_routines(
            "// Процедура Ложная()\nПроцедура Выполнить()\nКонецПроцедуры\nFunction Value()\n    Return 1;\nEndFunction\n"
                .as_bytes(),
        )
        .expect("valid routines");

        assert!(routines.contains_key(&(RoutineKind::Method, "выполнить".to_owned())));
        assert!(routines.contains_key(&(RoutineKind::Function, "value".to_owned())));
        assert_eq!(routines.len(), 2);
    }

    /// Incomplete BSL is rejected so callers retain only the reliable module event.
    #[test]
    fn rejects_incomplete_routine_body() {
        assert!(parse_routines("Процедура Выполнить()\n".as_bytes()).is_none());
    }

    /// Descriptor parsing assigns independent stable identities to inline metadata objects.
    #[test]
    fn parses_descriptor_objects_and_property_signatures() {
        let source = br#"<MetaDataObject xmlns="http://v8.1c.ru/8.3/MDClasses"><Catalog><Properties><Name>Customers</Name></Properties><ChildObjects><Attribute><Properties><Name>Code</Name><Comment>value</Comment></Properties></Attribute></ChildObjects></Catalog></MetaDataObject>"#;
        let base = metadata::from_path(
            ProjectType::Configuration,
            b"Catalogs/Customers.xml".as_bstr(),
        )
        .expect("metadata path");

        let objects = descriptor_objects(source, &base).expect("valid descriptor");

        assert!(objects.contains_key("catalog:Customers"));
        assert!(objects.contains_key("catalog:Customers/attribute:Code"));
        assert!(
            objects["catalog:Customers/attribute:Code"]
                .properties
                .contains("value")
        );
    }

    /// Every event kind has an explicit stable machine name.
    #[test]
    fn semantic_event_names_are_unique() {
        let kinds = [
            SemanticEventKind::ObjectAdded,
            SemanticEventKind::ObjectRemoved,
            SemanticEventKind::ObjectChanged,
            SemanticEventKind::ModuleChanged,
            SemanticEventKind::MethodAdded,
            SemanticEventKind::MethodRemoved,
            SemanticEventKind::MethodChanged,
            SemanticEventKind::FunctionAdded,
            SemanticEventKind::FunctionRemoved,
            SemanticEventKind::FunctionChanged,
            SemanticEventKind::FormChanged,
            SemanticEventKind::MetadataAttributeChanged,
        ];
        let names: std::collections::BTreeSet<_> =
            kinds.into_iter().map(SemanticEventKind::as_str).collect();
        assert_eq!(names.len(), kinds.len());
    }
}
