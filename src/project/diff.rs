//! Locale-independent file changes scoped to one project inside a Git worktree.

use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

use gix::bstr::{BString, ByteSlice};

use super::{
    Project,
    metadata::{self, MetadataPath},
};
use crate::vcs::{
    repository::{Error as RepositoryError, Repository},
    status::Change,
};

/// File-level project diff. Object-aware details can be added without changing the command.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ProjectDiff {
    pub files: Vec<FileChange>,
    pub display: Vec<DisplayChange>,
}

/// Human-facing target that is either a logical metadata object or an unchanged file path.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum DisplayTarget {
    Metadata(MetadataPath),
    File(BString),
}

/// Aggregated state for one human-facing target.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DisplayChange {
    pub target: DisplayTarget,
    pub index: Option<Change>,
    pub worktree: Option<Change>,
}

/// One changed path relative to the project root, retaining both Git comparison stages.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileChange {
    pub path: BString,
    pub index: Option<Change>,
    pub worktree: Option<Change>,
}

/// Errors produced while reading a project diff.
#[derive(Debug)]
pub enum DiffError {
    Repository(RepositoryError),
    ProjectOutsideRepository {
        project: PathBuf,
        repository: PathBuf,
    },
}

/// Inspect current file changes without modifying the worktree, index or references.
///
/// # Errors
/// Returns a structured error when the repository cannot be read or does not contain the project.
pub fn inspect(project: &Project) -> Result<ProjectDiff, DiffError> {
    let repository = Repository::discover(project.root()).map_err(DiffError::Repository)?;
    if !project.root().starts_with(repository.work_dir()) {
        return Err(DiffError::ProjectOutsideRepository {
            project: project.root().to_owned(),
            repository: repository.work_dir().to_owned(),
        });
    }

    let status = repository.status().map_err(DiffError::Repository)?;
    let mut files = Vec::new();
    let mut display = BTreeMap::new();
    for entry in status.entries {
        let Some(path) =
            project_relative_path(repository.work_dir(), project.root(), entry.path.as_bstr())
        else {
            continue;
        };
        let source_path = source_relative_path(project.root(), project.source(), path.as_bstr());
        if let Some((base, source_path)) = source_path.and_then(|source_path| {
            metadata::from_path(
                project.configuration().project_type(),
                source_path.as_bstr(),
            )
            .map(|base| (base, source_path))
        }) {
            let versions = metadata::is_main_descriptor(
                project.configuration().project_type(),
                source_path.as_bstr(),
            )
            .then(|| repository.file_versions(entry.path.as_bstr()).ok())
            .flatten();
            record_metadata_stage(
                &mut display,
                &base,
                entry.index,
                versions.as_ref().and_then(|value| value.head.as_deref()),
                versions.as_ref().and_then(|value| value.index.as_deref()),
                true,
            );
            record_metadata_stage(
                &mut display,
                &base,
                entry.worktree,
                versions.as_ref().and_then(|value| value.index.as_deref()),
                versions
                    .as_ref()
                    .and_then(|value| value.worktree.as_deref()),
                false,
            );
        } else {
            record_display(
                &mut display,
                DisplayTarget::File(path.clone()),
                entry.index,
                entry.worktree,
            );
        }
        files.push(FileChange {
            path,
            index: entry.index,
            worktree: entry.worktree,
        });
    }
    Ok(ProjectDiff {
        files,
        display: display.into_values().collect(),
    })
}

/// Convert a project-relative Git path into a path relative to the configured source root.
fn source_relative_path(
    project_root: &Path,
    source_root: &Path,
    project_path: &gix::bstr::BStr,
) -> Option<BString> {
    let absolute = project_root.join(gix::path::from_bstr(project_path));
    let relative = absolute.strip_prefix(source_root).ok()?;
    Some(gix::path::to_unix_separators_on_windows(gix::path::into_bstr(relative)).into_owned())
}

/// Refine a modified descriptor into changed child identities when both snapshots parse.
fn record_metadata_stage(
    display: &mut BTreeMap<DisplayTarget, DisplayChange>,
    base: &MetadataPath,
    change: Option<Change>,
    before: Option<&[u8]>,
    after: Option<&[u8]>,
    index: bool,
) {
    let Some(change) = change else {
        return;
    };
    let paths = if change == Change::Modified {
        before
            .zip(after)
            .and_then(|(before, after)| metadata::changed_children(before, after))
            .filter(|paths| !paths.is_empty())
            .map_or_else(
                || vec![base.clone()],
                |paths| {
                    paths
                        .iter()
                        .map(|suffix| base.with_suffix(suffix))
                        .collect()
                },
            )
    } else {
        vec![base.clone()]
    };
    for path in paths {
        let (index_change, worktree_change) = if index {
            (Some(change), None)
        } else {
            (None, Some(change))
        };
        record_display(
            display,
            DisplayTarget::Metadata(path),
            index_change,
            worktree_change,
        );
    }
}

/// Merge multiple backing files that represent the same logical object.
fn record_display(
    display: &mut BTreeMap<DisplayTarget, DisplayChange>,
    target: DisplayTarget,
    index: Option<Change>,
    worktree: Option<Change>,
) {
    let value = display.entry(target.clone()).or_insert(DisplayChange {
        target,
        index: None,
        worktree: None,
    });
    value.index = merge_change(value.index, index);
    value.worktree = merge_change(value.worktree, worktree);
}

/// Collapse differing states from multiple files into a conservative logical modification.
fn merge_change(current: Option<Change>, incoming: Option<Change>) -> Option<Change> {
    match (current, incoming) {
        (None, value) | (value, None) => value,
        (Some(left), Some(right)) if left == right => Some(left),
        (Some(Change::Conflict), _) | (_, Some(Change::Conflict)) => Some(Change::Conflict),
        _ => Some(Change::Modified),
    }
}

/// Convert a repository-relative byte path into a project-relative byte path.
fn project_relative_path(
    work_dir: &Path,
    project_root: &Path,
    repository_path: &gix::bstr::BStr,
) -> Option<BString> {
    let absolute = work_dir.join(gix::path::from_bstr(repository_path));
    let relative = absolute.strip_prefix(project_root).ok()?;
    Some(gix::path::to_unix_separators_on_windows(gix::path::into_bstr(relative)).into_owned())
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use gix::bstr::ByteSlice;

    use super::{DisplayTarget, project_relative_path, record_metadata_stage};
    use crate::{
        project::{ProjectType, metadata},
        vcs::status::Change,
    };

    /// Project scoping must remove an ancestor repository prefix without admitting sibling paths.
    #[test]
    fn relativizes_only_paths_inside_the_project() {
        let worktree = Path::new("/workspace");
        let project = Path::new("/workspace/components/app");

        assert_eq!(
            project_relative_path(
                worktree,
                project,
                b"components/app/src/module.bsl".as_bstr()
            ),
            Some("src/module.bsl".into())
        );
        assert_eq!(
            project_relative_path(worktree, project, b"components/other/file".as_bstr()),
            None
        );
    }

    /// A modified owner descriptor is refined to the exact changed child object.
    #[test]
    fn refines_modified_xml_to_a_metadata_attribute() {
        let base = metadata::from_path(
            ProjectType::Configuration,
            b"Catalogs/Partners.xml".as_bstr(),
        )
        .unwrap();
        let before = descriptor("Old");
        let after = descriptor("New");
        let mut display = std::collections::BTreeMap::new();
        record_metadata_stage(
            &mut display,
            &base,
            Some(Change::Modified),
            Some(before.as_bytes()),
            Some(after.as_bytes()),
            false,
        );

        let change = display.into_values().next().unwrap();
        let DisplayTarget::Metadata(path) = change.target else {
            panic!("metadata target expected");
        };
        assert_eq!(path.parts.last().unwrap().kind, "attribute");
        assert_eq!(path.parts.last().unwrap().name.as_deref(), Some("Code"));
        assert_eq!(change.worktree, Some(Change::Modified));
    }

    /// Build a minimal metadata owner used to test logical child refinement.
    fn descriptor(comment: &str) -> String {
        format!(
            r#"<MetaDataObject xmlns="http://v8.1c.ru/8.3/MDClasses"><Catalog><Properties><Name>Partners</Name></Properties><ChildObjects><Attribute><Properties><Name>Code</Name><Comment>{comment}</Comment></Properties></Attribute></ChildObjects></Catalog></MetaDataObject>"#
        )
    }
}
