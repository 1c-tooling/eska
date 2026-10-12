//! Project migration plans keep working values and independently pinned authority separate.

mod publication;
mod workspace;

pub use workspace::{
    WORKSPACE_LAYOUT_RECEIPT, WorkspaceAcceptedRefs, WorkspaceLayoutMember, WorkspaceLayoutPlan,
    prepare_readable_workspace,
};

#[cfg(test)]
mod tests;

use std::{
    collections::BTreeMap,
    fs,
    path::{Component, Path, PathBuf},
};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use super::{
    AcceptedBase, AcceptedLayoutPlan, AcceptedRef, Error, FileRole, LayoutChange, SourceLayoutPlan,
    SourcePath, files, prepare_accepted_readable_layout, prepare_readable_layout,
};
use crate::project_core::{
    Project, SourceFormat,
    config::{ProjectConfig, WorkspaceConfig},
    discovery::resolve_member_configuration,
    metadata_model::ProjectScope,
    publish,
};

/// A project is not ready while its independently resumable source and base are being published.
pub const PROJECT_LAYOUT_RECEIPT: &str = ".eska-project-layout.json";

/// The receipt binds both storage plans and exact settings, without absolute paths or editable data.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Receipt {
    schema_version: u32,
    source_input: String,
    source_output: String,
    base_input: String,
    base_output: String,
    previous_pin: String,
    accepted_pin: String,
    ancillary: BTreeMap<SourcePath, String>,
}

/// A full project copy publishes its settings last; working edits never enter the accepted plan.
#[derive(Debug)]
pub struct ProjectLayoutPlan {
    root: PathBuf,
    destination: PathBuf,
    source_path: SourcePath,
    base_path: SourcePath,
    source: SourceLayoutPlan,
    base: AcceptedLayoutPlan,
    ancillary: BTreeMap<SourcePath, Option<String>>,
    receipt: Receipt,
}

/// Prepare a readable standalone project in a new directory, without changing the current project.
///
/// The exact member manifest cannot safely carry inherited workspace settings into a standalone
/// copy; this first orchestration therefore refuses member scope explicitly. Source and base use
/// their existing independent expected-byte plans. Extra project files and Git state are not copied.
///
/// # Errors
/// Returns unsupported scope/format, invalid or untrusted input, overlap or foreign output failures.
pub fn prepare_readable_project(
    project: &Project,
    scope: &ProjectScope,
    expected: &AcceptedRef,
    destination: &Path,
) -> Result<ProjectLayoutPlan, Error> {
    if *scope != ProjectScope::Standalone {
        return Err(Error::root("source_migration_scope_unsupported"));
    }
    prepare_project(project, expected, destination, None)
}

/// A workspace supplies verified defaults and a destination below its already resolved new root.
fn prepare_project(
    project: &Project,
    expected: &AcceptedRef,
    destination: &Path,
    workspace: Option<&WorkspaceConfig>,
) -> Result<ProjectLayoutPlan, Error> {
    let SourceFormat::Eska { accepted_base } = project.configuration().source_format() else {
        return Err(Error::root("source_format_mismatch"));
    };
    let root = project
        .root()
        .canonicalize()
        .map_err(|error| files::io(&error))?;
    if root != project.root() {
        return Err(Error::root("source_path_invalid"));
    }
    require_project_ready(&root)?;
    let source_path = portable(
        project
            .source()
            .strip_prefix(&root)
            .map_err(|_| Error::root("source_path_invalid"))?,
    )?;
    let base_path = accepted_base.clone();
    validate_paths(&source_path, &base_path)?;
    let ancillary = ancillary(&root)?;
    let manifest = ancillary
        .get(&SourcePath::parse("eska.toml")?)
        .and_then(Option::as_deref)
        .ok_or_else(|| Error::root("source_file_missing"))?;
    let config =
        ProjectConfig::from_toml(manifest).map_err(|_| Error::root("source_schema_invalid"))?;
    let loaded = if let Some(workspace) = workspace {
        let configuration = resolve_member_configuration(workspace, &config)
            .map_err(|_| Error::root("source_schema_invalid"))?;
        Project::new(root.clone(), root.join(config.source()), configuration)
            .map_err(|_| Error::root("source_path_invalid"))?
    } else {
        config
            .into_project(root.clone())
            .map_err(|_| Error::root("source_path_invalid"))?
    };
    if loaded != *project {
        return Err(Error::root("source_input_changed"));
    }
    let accepted = AcceptedBase::open(&root.join(base_path.as_path()), expected)?;
    let source = prepare_readable_layout(project.source())?;
    let base = prepare_accepted_readable_layout(&accepted)?;
    if source.manifest().input_revision != base.source_manifest().input_revision
        || configuration_uuid(source.manifest()) != configuration_uuid(base.source_manifest())
    {
        return Err(Error::root("source_baseline_mismatch"));
    }
    let receipt = Receipt {
        schema_version: 1,
        source_input: source.input_revision().into(),
        source_output: source.output_revision().into(),
        base_input: base.publication_view().input.revision.clone(),
        base_output: base.publication_view().output.revision.clone(),
        previous_pin: base.previous_reference().sha256.clone(),
        accepted_pin: base.accepted_reference().sha256.clone(),
        ancillary: ancillary
            .iter()
            .filter_map(|(path, text)| {
                text.as_ref()
                    .map(|text| (path.clone(), files::hex(&Sha256::digest(text.as_bytes()))))
            })
            .collect(),
    };
    let plan = ProjectLayoutPlan {
        destination: if workspace.is_some() {
            destination.to_owned()
        } else {
            destination_path(&root, destination)?
        },
        root,
        source_path,
        base_path,
        source,
        base,
        ancillary,
        receipt,
    };
    plan.recheck()?;
    plan.check_output()?;
    Ok(plan)
}

impl ProjectLayoutPlan {
    /// The destination is absolute and independent of the current source or accepted tree.
    #[must_use]
    pub fn destination(&self) -> &Path {
        &self.destination
    }

    /// Working-source preview preserves current values and annotations.
    #[must_use]
    pub fn source_changes(&self) -> &[LayoutChange] {
        self.source.changes()
    }

    /// Accepted-source preview comes exclusively from the independently supplied old pin.
    #[must_use]
    pub fn accepted_changes(&self) -> &[LayoutChange] {
        self.base.source_changes()
    }

    /// Retain the old trust reference for review and recovery.
    #[must_use]
    pub const fn previous_reference(&self) -> &AcceptedRef {
        self.base.previous_reference()
    }

    /// CI must explicitly approve and select this pin after publication.
    #[must_use]
    pub const fn accepted_reference(&self) -> &AcceptedRef {
        self.base.accepted_reference()
    }

    /// Layout no-op still copies a new project; exact repeated publication writes no files.
    #[must_use]
    pub fn layout_changed(&self) -> bool {
        !self.source.is_empty() || !self.base.is_empty()
    }

    /// List only the project files actually copied, separately from source and base storage.
    pub fn ancillary_files(&self) -> impl Iterator<Item = &SourcePath> {
        self.ancillary
            .iter()
            .filter_map(|(path, text)| text.is_some().then_some(path))
    }

    /// Check settings as well as both complete inventories before accepting stale work.
    fn recheck(&self) -> Result<(), Error> {
        if ancillary(&self.root)? != self.ancillary {
            return Err(Error::root("source_input_changed"));
        }
        self.source.publication_view().recheck()?;
        self.base.publication_view().recheck()
    }
}

/// Even a published manifest cannot expose a project whose outer operation is incomplete.
pub fn require_project_ready(root: &Path) -> Result<(), Error> {
    require_marker_absent(root, PROJECT_LAYOUT_RECEIPT)?;
    for ancestor in root.ancestors() {
        require_marker_absent(ancestor, WORKSPACE_LAYOUT_RECEIPT)?;
    }
    Ok(())
}

/// A containing workspace remains unavailable even before its root manifest has appeared.
fn require_marker_absent(root: &Path, name: &str) -> Result<(), Error> {
    match fs::symlink_metadata(root.join(name)) {
        Ok(_) => Err(Error::root("source_migration_incomplete")),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(files::io(&error)),
    }
}

/// Ancillary text is copied byte-for-byte; absence is also an expected input state.
fn ancillary(root: &Path) -> Result<BTreeMap<SourcePath, Option<String>>, Error> {
    ["eska.toml", ".gitattributes", ".gitignore"]
        .into_iter()
        .map(|name| Ok((SourcePath::parse(name)?, read_optional(&root.join(name))?)))
        .collect()
}

/// Refuse symlinks and non-text entries rather than following aliases during a project copy.
fn read_optional(path: &Path) -> Result<Option<String>, Error> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_file() => files::read_text(path).map(Some),
        Ok(_) => Err(Error::root("source_entry_unsupported")),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(files::io(&error)),
    }
}

/// Root settings and the operation receipt cannot become source/base directory names.
fn validate_paths(source: &SourcePath, base: &SourcePath) -> Result<(), Error> {
    validate_distinct_paths(source, base)?;
    for path in [source, base] {
        if [
            "eska.toml",
            ".gitattributes",
            ".gitignore",
            PROJECT_LAYOUT_RECEIPT,
            WORKSPACE_LAYOUT_RECEIPT,
        ]
        .iter()
        .any(|name| {
            path.as_str()
                .split('/')
                .next()
                .is_some_and(|root| root.eq_ignore_ascii_case(name))
        }) {
            return Err(Error::root("source_path_collision"));
        }
    }
    Ok(())
}

/// Two owned trees must neither overlap nor spell a shared ancestor differently by case.
fn validate_distinct_paths(source: &SourcePath, base: &SourcePath) -> Result<(), Error> {
    for (left, right) in source.as_str().split('/').zip(base.as_str().split('/')) {
        if left.to_lowercase() != right.to_lowercase() {
            break;
        }
        if left != right {
            return Err(Error::root("source_path_collision"));
        }
    }
    let left = source.as_str().to_lowercase();
    let right = base.as_str().to_lowercase();
    if left == right
        || left.starts_with(&format!("{right}/"))
        || right.starts_with(&format!("{left}/"))
    {
        return Err(Error::root("source_baseline_overlap"));
    }
    Ok(())
}

/// Project settings use OS paths; source maps use portable normal UTF-8 components.
fn portable(path: &Path) -> Result<SourcePath, Error> {
    let parts = path
        .components()
        .map(|part| match part {
            Component::Normal(name) => name
                .to_str()
                .ok_or_else(|| Error::root("source_path_invalid")),
            _ => Err(Error::root("source_path_invalid")),
        })
        .collect::<Result<Vec<_>, _>>()?;
    SourcePath::parse(&parts.join("/"))
}

/// Configuration identity is declared in the native map, never inferred from an output directory.
fn configuration_uuid(manifest: &super::SourceManifest) -> Option<&super::NativeUuid> {
    manifest.files.iter().find_map(|entry| match &entry.role {
        FileRole::Descriptor { uuid, .. } if entry.path.as_str() == "Configuration.xml" => {
            Some(uuid)
        }
        _ => None,
    })
}

/// Resolve an existing parent and reject any write into or around the original project.
fn destination_path(root: &Path, destination: &Path) -> Result<PathBuf, Error> {
    let name = destination
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| Error::root("source_path_invalid"))?;
    SourcePath::parse(name)?;
    let parent = destination
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let destination = parent
        .canonicalize()
        .map_err(|error| files::io(&error))?
        .join(name);
    let existing = match destination.canonicalize() {
        Ok(path) => path,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => destination.clone(),
        Err(error) => return Err(files::io(&error)),
    };
    if existing.starts_with(root) || root.starts_with(&existing) {
        return Err(Error::root("source_tree_overlap"));
    }
    Ok(destination)
}

/// Reuse the exact-byte core publisher without localized or CLI-specific failures.
fn publish_new(root: &Path, path: &str, text: &str) -> Result<(), Error> {
    publish::publish_new(root, Path::new(path), text).map_err(|error| {
        Error::root(match error {
            publish::PublishError::Io(_) => "source_io_error",
            publish::PublishError::Conflict => "source_input_changed",
            publish::PublishError::ReadOnly => "source_read_only",
            publish::PublishError::UnsafePath => "source_path_invalid",
        })
    })
}
