//! Committed patch planning and a deliberately narrow common-module support gate.

use super::{PatchChange, PatchError, PatchModule, PatchPlan};
use crate::project::{Project, ProjectType};
use crate::vcs::{repository::Repository, status::Change};
use gix::{ObjectId, bstr::ByteSlice};
use std::{
    collections::BTreeMap,
    path::{Component, Path},
};

struct ClassifiedChanges {
    modules: Vec<PatchModule>,
    ignored_paths: Vec<String>,
    changes: Vec<PatchChange>,
}

/// Resolve immutable Git endpoints and reject every unsupported source change before execution.
///
/// # Errors
/// Returns a structured error for dirty worktrees, ambiguous history or unsupported source input.
pub fn plan(project: &Project, base: Option<&str>) -> Result<PatchPlan, PatchError> {
    if project.configuration().project_type() != ProjectType::Configuration {
        return Err(PatchError::new("project-type", ""));
    }
    let repository = Repository::discover(project.root())
        .map_err(|e| PatchError::new("repository", format!("{e:?}")))?;
    if repository.has_in_progress_operation()
        || repository
            .status()
            .map_err(|e| PatchError::new("repository", format!("{e:?}")))?
            .is_dirty()
    {
        return Err(PatchError::new("dirty", ""));
    }
    let base = resolve_base(project, base)?;
    let from = repository
        .resolve_commit(&base)
        .map_err(|e| PatchError::new("repository", format!("{e:?}")))?;
    let head = repository
        .resolve_commit("HEAD")
        .map_err(|e| PatchError::new("repository", format!("{e:?}")))?;
    let bases = repository
        .merge_bases(from, head)
        .map_err(|e| PatchError::new("repository", format!("{e:?}")))?;
    let [ancestor] = bases.as_slice() else {
        return Err(PatchError::new("merge-base", bases.len().to_string()));
    };
    let root = project
        .root()
        .strip_prefix(repository.work_dir())
        .map_err(|e| PatchError::new("path", e.to_string()))?;
    let source = project
        .source()
        .strip_prefix(repository.work_dir())
        .map_err(|e| PatchError::new("path", e.to_string()))?;
    let source_prefix = gix::path::to_unix_separators_on_windows(gix::path::into_bstr(source));
    let source_prefix = source_prefix
        .to_str()
        .map_err(|e| PatchError::new("path", e.to_string()))?;
    let prefix = if source_prefix.is_empty() {
        String::new()
    } else {
        format!("{source_prefix}/")
    };
    let config_path =
        gix::path::to_unix_separators_on_windows(gix::path::into_bstr(root.join("eska.toml")))
            .into_owned();
    let source_files = collect_sources(&repository, *ancestor, &prefix, config_path.as_bstr())?;
    let name = format!("EskaPatch_{}", &head.id.to_string()[..12]);
    let classified = collect_changes(
        &repository,
        *ancestor,
        head,
        config_path.as_bstr(),
        &prefix,
        &source_files,
        &name,
    )?;
    Ok(PatchPlan {
        base,
        base_commit: from.id.to_string(),
        merge_base: ancestor.id.to_string(),
        head: head.id.to_string(),
        name,
        modules: classified.modules,
        ignored_paths: classified.ignored_paths,
        changes: classified.changes,
        repository_root: repository.work_dir().to_owned(),
        source_files,
        platform_version: project
            .configuration()
            .build_settings()
            .platform_version()
            .cloned(),
    })
}

/// Use the explicit revision or the configured local integration target.
fn resolve_base(project: &Project, base: Option<&str>) -> Result<String, PatchError> {
    if let Some(base) = base {
        return Ok(base.to_owned());
    }
    let settings = project
        .configuration()
        .workflow_settings()
        .ok_or_else(|| PatchError::new("base", ""))?;
    let policy = settings
        .resolve(None)
        .map_err(|e| PatchError::new("base", format!("{e:?}")))?;
    Ok(format!("refs/heads/{}", policy.integration_target()))
}

/// Classify the complete committed delta and build supported module projections.
fn collect_changes(
    repository: &Repository,
    ancestor: crate::vcs::diff::ResolvedCommit,
    head: crate::vcs::diff::ResolvedCommit,
    config_path: &gix::bstr::BStr,
    prefix: &str,
    source_files: &BTreeMap<String, ObjectId>,
    patch_name: &str,
) -> Result<ClassifiedChanges, PatchError> {
    let mut modules = Vec::new();
    let mut ignored_paths = Vec::new();
    let mut changes = Vec::new();
    for change in repository
        .diff_commits(ancestor, head)
        .map_err(|e| PatchError::new("repository", format!("{e:?}")))?
    {
        if change.path == config_path {
            return Err(PatchError::new("unsupported", "eska.toml"));
        }
        let path = change
            .path
            .to_str()
            .map_err(|e| PatchError::new("path", e.to_string()))?;
        let Some(relative) = path.strip_prefix(prefix) else {
            ignored_paths.push(path.to_owned());
            changes.push(PatchChange {
                path: path.to_owned(),
                decision: "ignored",
                reason: "outside_project_source",
            });
            continue;
        };
        let module = relative
            .strip_prefix("CommonModules/")
            .and_then(|s| s.strip_suffix("/Ext/Module.bsl"))
            .filter(|s| !s.contains('/') && valid_name(s))
            .ok_or_else(|| PatchError::new("unsupported", path))?;
        if change.change != Change::Modified {
            return Err(PatchError::new("unsupported", path));
        }
        let before = text_blob(
            repository,
            change
                .before
                .ok_or_else(|| PatchError::new("unsupported", path))?,
        )?;
        let after = text_blob(
            repository,
            change
                .after
                .ok_or_else(|| PatchError::new("unsupported", path))?,
        )?;
        let (code, methods) =
            super::methods::replacements(&before, &after, &format!("{patch_name}_"))
                .ok_or_else(|| PatchError::new("bsl", path))?;
        if methods.is_empty() {
            changes.push(PatchChange {
                path: path.to_owned(),
                decision: "ignored",
                reason: "nonsemantic",
            });
            continue;
        }
        let descriptor_path = format!("CommonModules/{module}.xml");
        let descriptor = text_blob(
            repository,
            *source_files
                .get(&descriptor_path)
                .ok_or_else(|| PatchError::new("descriptor", &descriptor_path))?,
        )?;
        let descriptor = super::descriptor::adopt(&descriptor, module, &head.id.to_string())?;
        changes.push(PatchChange {
            path: path.to_owned(),
            decision: "included",
            reason: "changed_method_bodies",
        });
        modules.push(PatchModule {
            name: module.to_owned(),
            methods,
            descriptor,
            code,
        });
    }
    Ok(ClassifiedChanges {
        modules,
        ignored_paths,
        changes,
    })
}

/// Read bounded UTF-8 descriptors/modules directly from committed blobs.
fn text_blob(repository: &Repository, id: ObjectId) -> Result<String, PatchError> {
    let bytes = repository
        .blob(id)
        .map_err(|e| PatchError::new("repository", format!("{e:?}")))?;
    if bytes.len() > 64 * 1024 * 1024 {
        return Err(PatchError::new("unsupported", id.to_string()));
    }
    String::from_utf8(bytes).map_err(|e| PatchError::new("descriptor", e.to_string()))
}

/// Reject paths that could escape staging or mean something different on another supported OS.
fn validate_path(path: &str) -> Result<(), PatchError> {
    if path.is_empty()
        || path.contains(['\\', ':'])
        || path
            .split('/')
            .any(|s| s.is_empty() || s.eq_ignore_ascii_case(".git"))
        || Path::new(path)
            .components()
            .any(|c| !matches!(c, Component::Normal(_)))
    {
        return Err(PatchError::new("path", path));
    }
    Ok(())
}

/// Admit simple platform identifiers without quoting or path syntax.
fn valid_name(name: &str) -> bool {
    let mut chars = name.chars();
    chars.next().is_some_and(|c| c.is_alphabetic() || c == '_')
        && chars.all(|c| c.is_alphanumeric() || c == '_')
}

/// Collect only regular committed source entries without materializing worktree files.
fn collect_sources(
    repository: &Repository,
    ancestor: crate::vcs::diff::ResolvedCommit,
    prefix: &str,
    config_path: &gix::bstr::BStr,
) -> Result<BTreeMap<String, ObjectId>, PatchError> {
    let entries = repository
        .snapshot_entries(ancestor)
        .map_err(|e| PatchError::new("repository", format!("{e:?}")))?;
    let mut source_files = BTreeMap::new();
    for entry in entries {
        if entry.mode.is_tree() || entry.filepath == config_path {
            continue;
        }
        let path = entry
            .filepath
            .to_str()
            .map_err(|e| PatchError::new("path", e.to_string()))?;
        if let Some(relative) = path.strip_prefix(prefix) {
            validate_path(relative)?;
            if !entry.mode.is_blob() {
                return Err(PatchError::new("unsupported", path));
            }
            source_files.insert(relative.to_owned(), entry.oid);
        }
    }
    if !source_files.contains_key("Configuration.xml") {
        return Err(PatchError::new("descriptor", "Configuration.xml"));
    }
    Ok(source_files)
}
