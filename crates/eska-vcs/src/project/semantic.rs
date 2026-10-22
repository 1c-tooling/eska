//! Semantic comparisons over exact Git snapshots with explicit conservative fallbacks.

mod changes;
mod descriptor;
mod model;
mod routines;

pub use changes::{
    ChangeCounts, ChangeSet, ChangeStage, ChangeSummary, ChangedPath, ObjectChangeSummary,
    ObjectPathChange, ObjectPathRole, SemanticChangeAnalyzer,
};
pub use model::{
    SemanticDiff, SemanticDiffError, SemanticEvent, SemanticEventKind, SemanticFallback,
    SemanticFallbackReason, SemanticObject, SourceLocation,
};

use super::{
    Project,
    diff::{ProjectDiff, RevisionProjectDiff},
    metadata::{self, MetadataPath},
    object_model::{LogicalObject, ObjectModel},
};
use crate::vcs::{diff::ResolvedCommit, repository::Repository, status::Change};
use descriptor::{analyze_descriptor, descriptor_objects};
use gix::bstr::{BStr, BString, ByteSlice};
use routines::{RoutineKind, parse_routines};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
};

/// Analyze current changes after indexing only descriptors that own changed paths.
///
/// # Errors
/// Returns a structured error when the source root, repository or snapshots cannot be read.
pub fn diff_workspace_affected(
    project: &Project,
    diff: &ProjectDiff,
) -> Result<SemanticDiff, SemanticDiffError> {
    if diff.files.is_empty() {
        return Ok(SemanticDiff::default());
    }
    let source_paths = diff
        .files
        .iter()
        .filter_map(|file| source_relative_path(project, file.path.as_bstr()))
        .collect::<Vec<_>>();
    let affected = super::object_model::discover_affected(project, &source_paths)
        .map_err(SemanticDiffError::ObjectModel)?;
    diff_workspace(project, affected.model(), diff)
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
    let mut versions_reader = None;
    let mut events = BTreeSet::new();
    let mut fallbacks = BTreeSet::new();
    for file in &diff.files {
        let source_path = source_relative_path(project, file.path.as_bstr());
        let Some(source_path) = source_path else {
            continue;
        };
        let repository_path = join_git_path(prefix.as_bstr(), file.path.as_bstr());
        let reader = match &versions_reader {
            Some(reader) => reader,
            None => versions_reader.insert(
                repository
                    .file_version_reader()
                    .map_err(SemanticDiffError::Repository)?,
            ),
        };
        let versions = reader
            .read(repository_path.as_bstr())
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
            analyze_snapshots(
                project,
                Some(objects),
                &snapshot,
                &mut events,
                &mut fallbacks,
            );
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
            analyze_snapshots(
                project,
                Some(objects),
                &snapshot,
                &mut events,
                &mut fallbacks,
            );
        }
    }
    Ok(build_semantic_diff(events, fallbacks))
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
    let mut fallbacks = BTreeSet::new();
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
        analyze_snapshots(project, None, &snapshot, &mut events, &mut fallbacks);
    }
    Ok(build_semantic_diff(events, fallbacks))
}

/// Suppress derived changes when the same identity has an object lifecycle event.
fn build_semantic_diff(
    events: BTreeSet<SemanticEvent>,
    fallbacks: BTreeSet<SemanticFallback>,
) -> SemanticDiff {
    let mut lifecycle: BTreeMap<ChangeStage, BTreeSet<String>> = BTreeMap::new();
    for event in &events {
        if matches!(
            event.kind,
            SemanticEventKind::ObjectAdded | SemanticEventKind::ObjectRemoved
        ) {
            lifecycle
                .entry(event.stage)
                .or_default()
                .insert(event.object.id.clone());
        }
    }
    let events = events
        .into_iter()
        .filter(|event| {
            matches!(
                event.kind,
                SemanticEventKind::ObjectAdded | SemanticEventKind::ObjectRemoved
            ) || !lifecycle
                .get(&event.stage)
                .is_some_and(|objects| objects.contains(&event.object.id))
        })
        .collect();
    SemanticDiff {
        events,
        fallbacks: fallbacks.into_iter().collect(),
    }
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
    fallbacks: &mut BTreeSet<SemanticFallback>,
) {
    let owners = objects
        .map(|objects| objects.objects_for_changed_path(snapshot.source_path))
        .unwrap_or_default();
    let owner = owners.first().map(|value| semantic_object(value));
    let project_type = project.configuration().project_type();
    let source_bytes =
        gix::path::to_unix_separators_on_windows(gix::path::into_bstr(snapshot.source_path));

    if metadata::is_object_descriptor(project_type, source_bytes.as_ref()) {
        analyze_descriptor(
            project_type,
            source_bytes.as_ref(),
            snapshot,
            owner,
            events,
            fallbacks,
        );
        return;
    }

    let inferred = fallback_object(project_type, source_bytes.as_ref(), None);
    let Some(object) = owner.or(inferred) else {
        record_fallback(fallbacks, SemanticFallbackReason::OwnerUnresolved, snapshot);
        return;
    };
    if owners.is_empty()
        && (objects.is_some()
            || owner_identity_requires_descriptor(project_type, source_bytes.as_ref()))
    {
        record_fallback(fallbacks, SemanticFallbackReason::OwnerInferred, snapshot);
    }
    let is_module = metadata::is_module_path(snapshot.source_path);
    if is_module {
        emit(
            events,
            SemanticEventKind::ModuleChanged,
            snapshot.stage,
            object.clone(),
            None,
            snapshot.project_path.clone(),
        );
        if !analyze_routines(
            snapshot.before,
            snapshot.after,
            snapshot.stage,
            &object,
            &snapshot.project_path,
            events,
        ) {
            record_fallback(fallbacks, SemanticFallbackReason::RoutineParse, snapshot);
        }
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

/// Tell whether path-only identity omits a descriptor-defined root name.
fn owner_identity_requires_descriptor(
    project_type: super::ProjectType,
    source_path: &BStr,
) -> bool {
    match project_type {
        super::ProjectType::Configuration | super::ProjectType::Extension => {
            source_path.starts_with(b"Ext/")
        }
        super::ProjectType::Processing | super::ProjectType::Report => true,
    }
}

/// Retain a deterministic non-fatal fallback for later CLI presentation.
fn record_fallback(
    fallbacks: &mut BTreeSet<SemanticFallback>,
    reason: SemanticFallbackReason,
    snapshot: &SnapshotChange<'_>,
) {
    fallbacks.insert(SemanticFallback {
        reason,
        stage: snapshot.stage,
        path: snapshot.project_path.clone(),
    });
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

/// Compare top-level BSL procedures and functions when both endpoint modules are UTF-8.
fn analyze_routines(
    before: Option<&[u8]>,
    after: Option<&[u8]>,
    stage: ChangeStage,
    object: &SemanticObject,
    path: &BString,
    events: &mut BTreeSet<SemanticEvent>,
) -> bool {
    let (Some(before), Some(after)) = (before, after) else {
        return true;
    };
    let (Some(before), Some(after)) = (parse_routines(before), parse_routines(after)) else {
        return false;
    };
    let mut keys: BTreeSet<_> = before.keys().cloned().collect();
    keys.extend(after.keys().cloned());
    for key in keys {
        let (kind, routine) = match (before.get(&key), after.get(&key)) {
            (None, Some(routine)) => (routine_event_kind(routine.kind, Change::Added), routine),
            (Some(routine), None) => (routine_event_kind(routine.kind, Change::Deleted), routine),
            (Some(previous), Some(current)) if previous.body != current.body => {
                (routine_event_kind(current.kind, Change::Modified), current)
            }
            _ => continue,
        };
        emit_at(
            events,
            kind,
            stage,
            object.clone(),
            Some(routine.name.clone()),
            Some(SourceLocation {
                line: routine.line,
                column: routine.column,
            }),
            path.clone(),
        );
    }
    true
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

/// Insert one event into the deterministic de-duplicating set.
fn emit(
    events: &mut BTreeSet<SemanticEvent>,
    kind: SemanticEventKind,
    stage: ChangeStage,
    object: SemanticObject,
    member: Option<String>,
    path: BString,
) {
    emit_at(events, kind, stage, object, member, None, path);
}

/// Insert one event with optional source coordinates into the deterministic set.
fn emit_at(
    events: &mut BTreeSet<SemanticEvent>,
    kind: SemanticEventKind,
    stage: ChangeStage,
    object: SemanticObject,
    member: Option<String>,
    location: Option<SourceLocation>,
    path: BString,
) {
    events.insert(SemanticEvent {
        kind,
        stage,
        object,
        member,
        location,
        path,
    });
}

#[cfg(test)]
mod tests;
