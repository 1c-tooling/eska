//! Committed patch planning and a deliberately narrow common-module support gate.

mod bsl;
mod execute;

use super::{Project, ProjectType, build::PlatformVersion};
use crate::vcs::{repository::Repository, status::Change};
use gix::{ObjectId, bstr::ByteSlice};
use serde::Serialize;
use std::{
    collections::BTreeMap,
    path::{Component, Path, PathBuf},
};

pub use execute::execute;

const MD: &str = "http://v8.1c.ru/8.3/MDClasses";

#[derive(Debug)]
pub struct PatchError {
    pub code: &'static str,
    pub detail: String,
}

/// Keep machine-facing error classification separate from localized presentation.
fn error(code: &'static str, detail: impl Into<String>) -> PatchError {
    PatchError {
        code,
        detail: detail.into(),
    }
}

#[derive(Debug, Serialize)]
pub struct PatchPlan {
    pub base: String,
    pub base_commit: String,
    pub merge_base: String,
    pub head: String,
    pub name: String,
    pub modules: Vec<PatchModule>,
    pub ignored_paths: Vec<String>,
    pub changes: Vec<PatchChange>,
    #[serde(skip)]
    repository_root: PathBuf,
    #[serde(skip)]
    source_files: BTreeMap<String, ObjectId>,
    #[serde(skip)]
    pub platform_version: Option<PlatformVersion>,
}

#[derive(Debug, Serialize)]
pub struct PatchModule {
    pub name: String,
    pub methods: Vec<String>,
    #[serde(skip)]
    descriptor: String,
    #[serde(skip)]
    code: String,
}

#[derive(Debug, Serialize)]
pub struct PatchChange {
    pub path: String,
    pub decision: &'static str,
    pub reason: &'static str,
}

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
        return Err(error("project-type", ""));
    }
    let repository =
        Repository::discover(project.root()).map_err(|e| error("repository", format!("{e:?}")))?;
    if repository.has_in_progress_operation()
        || repository
            .status()
            .map_err(|e| error("repository", format!("{e:?}")))?
            .is_dirty()
    {
        return Err(error("dirty", ""));
    }
    let base = resolve_base(project, base)?;
    let from = repository
        .resolve_commit(&base)
        .map_err(|e| error("repository", format!("{e:?}")))?;
    let head = repository
        .resolve_commit("HEAD")
        .map_err(|e| error("repository", format!("{e:?}")))?;
    let bases = repository
        .merge_bases(from, head)
        .map_err(|e| error("repository", format!("{e:?}")))?;
    let [ancestor] = bases.as_slice() else {
        return Err(error("merge-base", bases.len().to_string()));
    };
    let root = project
        .root()
        .strip_prefix(repository.work_dir())
        .map_err(|e| error("path", e.to_string()))?;
    let source = project
        .source()
        .strip_prefix(repository.work_dir())
        .map_err(|e| error("path", e.to_string()))?;
    let source_prefix = gix::path::to_unix_separators_on_windows(gix::path::into_bstr(source));
    let source_prefix = source_prefix
        .to_str()
        .map_err(|e| error("path", e.to_string()))?;
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
        .ok_or_else(|| error("base", ""))?;
    let policy = settings
        .resolve(None)
        .map_err(|e| error("base", format!("{e:?}")))?;
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
        .map_err(|e| error("repository", format!("{e:?}")))?
    {
        if change.path == config_path {
            return Err(error("unsupported", "eska.toml"));
        }
        let path = change
            .path
            .to_str()
            .map_err(|e| error("path", e.to_string()))?;
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
            .ok_or_else(|| error("unsupported", path))?;
        if change.change != Change::Modified {
            return Err(error("unsupported", path));
        }
        let before = text_blob(
            repository,
            change.before.ok_or_else(|| error("unsupported", path))?,
        )?;
        let after = text_blob(
            repository,
            change.after.ok_or_else(|| error("unsupported", path))?,
        )?;
        let (code, methods) = bsl::replacements(&before, &after, &format!("{patch_name}_"))
            .ok_or_else(|| error("bsl", path))?;
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
                .ok_or_else(|| error("descriptor", &descriptor_path))?,
        )?;
        let descriptor = adopted_descriptor(&descriptor, module, &head.id.to_string())?;
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
        .map_err(|e| error("repository", format!("{e:?}")))?;
    if bytes.len() > 64 * 1024 * 1024 {
        return Err(error("unsupported", id.to_string()));
    }
    String::from_utf8(bytes).map_err(|e| error("descriptor", e.to_string()))
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
        return Err(error("path", path));
    }
    Ok(())
}

/// Admit simple platform identifiers without quoting or path syntax.
fn valid_name(name: &str) -> bool {
    let mut chars = name.chars();
    chars.next().is_some_and(|c| c.is_alphabetic() || c == '_')
        && chars.all(|c| c.is_alphanumeric() || c == '_')
}

/// Preserve the descriptor bytes except for the UUID and explicit adoption properties.
fn adopted_descriptor(xml: &str, name: &str, head: &str) -> Result<String, PatchError> {
    let document =
        roxmltree::Document::parse(xml).map_err(|e| error("descriptor", e.to_string()))?;
    let root = document.root_element();
    let children: Vec<_> = root
        .children()
        .filter(roxmltree::Node::is_element)
        .collect();
    let [module] = children.as_slice() else {
        return Err(error("descriptor", name));
    };
    if !root.has_tag_name((MD, "MetaDataObject")) || !module.has_tag_name((MD, "CommonModule")) {
        return Err(error("descriptor", name));
    }
    let properties = module
        .children()
        .find(|n| n.has_tag_name((MD, "Properties")))
        .ok_or_else(|| error("descriptor", name))?;
    let property = |key| {
        properties
            .children()
            .find(|n| n.has_tag_name((MD, key)))
            .and_then(|n| n.text())
    };
    for (key, value) in [
        ("Name", name),
        ("Global", "false"),
        ("Server", "true"),
        ("ClientManagedApplication", "false"),
        ("ClientOrdinaryApplication", "false"),
        ("Privileged", "false"),
        ("ReturnValuesReuse", "DontUse"),
    ] {
        if property(key) != Some(value) {
            return Err(error("descriptor", format!("{name}.{key}")));
        }
    }
    let uuid = module
        .attribute_node("uuid")
        .ok_or_else(|| error("descriptor", name))?;
    let original = uuid.value();
    if original.len() != 36 || !original.chars().all(|c| c.is_ascii_hexdigit() || c == '-') {
        return Err(error("descriptor", name));
    }
    let hash = gix::objs::compute_hash(
        gix::hash::Kind::Sha1,
        gix::objs::Kind::Blob,
        format!("{original}:{head}").as_bytes(),
    )
    .map_err(|e| error("descriptor", e.to_string()))?
    .to_string();
    let generated = format!(
        "{}-{}-4{}-a{}-{}",
        &hash[..8],
        &hash[8..12],
        &hash[13..16],
        &hash[17..20],
        &hash[20..32]
    );
    let start = properties.range().start;
    let insertion = start
        + xml[start..]
            .find('>')
            .ok_or_else(|| error("descriptor", name))?
        + 1;
    let mut result = xml.to_owned();
    result.insert_str(insertion, &format!("<ObjectBelonging xmlns=\"{MD}\">Adopted</ObjectBelonging><ExtendedConfigurationObject xmlns=\"{MD}\">{original}</ExtendedConfigurationObject>"));
    result.replace_range(uuid.range_value(), &generated);
    Ok(result)
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
        .map_err(|e| error("repository", format!("{e:?}")))?;
    let mut source_files = BTreeMap::new();
    for entry in entries {
        if entry.mode.is_tree() || entry.filepath == config_path {
            continue;
        }
        let path = entry
            .filepath
            .to_str()
            .map_err(|e| error("path", e.to_string()))?;
        if let Some(relative) = path.strip_prefix(prefix) {
            validate_path(relative)?;
            if !entry.mode.is_blob() {
                return Err(error("unsupported", path));
            }
            source_files.insert(relative.to_owned(), entry.oid);
        }
    }
    if !source_files.contains_key("Configuration.xml") {
        return Err(error("descriptor", "Configuration.xml"));
    }
    Ok(source_files)
}
