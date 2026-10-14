//! Explicit attachment preserves existing source bytes and independently selected support evidence.

mod publication;
mod staging;
#[cfg(test)]
mod tests;

use sha2::{Digest, Sha256};
use std::{
    fs,
    path::{Path, PathBuf},
};

use crate::project_core::{
    Project, ProjectName, SourceFormat,
    config::{self, ProjectConfig},
    native_version,
    platform_settings::{BuildSettings, PlatformVersion},
    transport::TransportPlan,
};

use super::{
    AcceptedBase, AcceptedRef, EXTERNAL_LAYOUT_VERSION, Error, ImportAncestorRef,
    PROJECT_SOURCE_IMPORT_DIRECTORY, QUERY_LAYOUT_VERSION, SourcePath, SourceReader,
    export::Package,
    files::{self, Inventory},
    require_project_ready,
};

const ACCEPTED: &str = ".eska/accepted";

/// A read-only attachment plan binds existing authority bytes before any new file is claimed.
pub struct ProjectInitializationPlan {
    root: PathBuf,
    project: Project,
    source: SourcePath,
    inventory: Inventory,
    approved: Option<AcceptedRef>,
    accepted: Option<Inventory>,
    history: Option<Inventory>,
    ancestor: Option<ImportAncestorRef>,
    manifest: String,
    transport: TransportPlan,
    version: native_version::ProjectVersion,
    metadata_name: String,
    layout_version: u32,
}

impl ProjectInitializationPlan {
    /// Return the canonical existing directory; neither this root nor its source belongs to cleanup.
    #[must_use]
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Expose the exact source directory without discovering an enclosing workspace.
    #[must_use]
    pub const fn project(&self) -> &Project {
        &self.project
    }

    /// Preserve the sole authoritative version read before attachment publication.
    #[must_use]
    pub const fn version(&self) -> &native_version::ProjectVersion {
        &self.version
    }

    /// Native metadata name remains distinct from the selected project label.
    #[must_use]
    pub fn metadata_name(&self) -> &str {
        &self.metadata_name
    }

    /// Report the actual input layout; attachment does not upgrade it.
    #[must_use]
    pub const fn source_layout_version(&self) -> u32 {
        self.layout_version
    }

    /// Existing independently selected support is retained rather than recaptured from working edits.
    #[must_use]
    pub const fn accepted_base_created(&self) -> bool {
        self.approved.is_none()
    }

    /// Existing raw history is copied neither into a new representation nor into a new approval.
    #[must_use]
    pub const fn ancestry_preserved(&self) -> bool {
        self.history.is_some()
    }

    /// Verify a child receipt before parent workflow, Git or workspace enrollment finalization.
    ///
    /// The manifest must still be the original publication, saved authoring must match preflight,
    /// existing support/history remain exact, and new candidate snapshots match the starting source.
    /// Caller references select integrity checks; this method grants neither support nor ancestry approval.
    ///
    /// # Errors
    /// Returns changed settings/source/evidence, unresolved import or invalid candidate references.
    pub fn verify_published(
        &self,
        expected_base: &AcceptedRef,
        expected_ancestor: &ImportAncestorRef,
        manifest_sha256: &str,
    ) -> Result<(), Error> {
        check_root(&self.root)?;
        require_project_ready(&self.root)?;
        checked_source(&self.root, &self.source)?;
        check_service_parent(&self.root)?;
        let path = self.root.join(config::FILE_NAME);
        if !fs::symlink_metadata(&path)
            .map_err(|error| files::io(&error))?
            .file_type()
            .is_file()
            || files::hex(&Sha256::digest(self.manifest.as_bytes())) != manifest_sha256
            || files::read_text_limited(&path, 1024 * 1024)? != self.manifest
            || Inventory::read(self.project.source())? != self.inventory
        {
            return Err(Error::root("source_input_changed"));
        }
        let base = AcceptedBase::open(&self.root.join(ACCEPTED), expected_base)?;
        base.validate_source_root(&SourceReader::open(self.project.source())?)?;
        match &self.accepted {
            Some(original)
                if self.approved.as_ref() != Some(expected_base)
                    || Inventory::read(&self.root.join(ACCEPTED))? != *original =>
            {
                return Err(Error::root("source_input_changed"));
            }
            None if base.package.snapshot != self.inventory => {
                return Err(Error::root("source_input_changed"));
            }
            _ => (),
        }
        super::verify_project_import_history(&self.root, &self.source, &base)?;
        if history_reference(&self.root)? != *expected_ancestor {
            return Err(Error::root("source_ancestor_untrusted"));
        }
        if let Some(original) = &self.history {
            if Inventory::read(&self.root.join(PROJECT_SOURCE_IMPORT_DIRECTORY))? != *original {
                return Err(Error::root("source_input_changed"));
            }
        } else {
            let initial =
                super::open_initial_project_import_ancestor(&self.project, expected_ancestor)?;
            if initial.revision() != self.inventory.revision {
                return Err(Error::root("source_input_changed"));
            }
        }
        require_project_ready(&self.root)
    }

    /// Refuse changed authoring, support, history, guards or a concurrently published manifest.
    fn recheck(&self) -> Result<(), Error> {
        self.recheck_source()?;
        if optional_inventory(&self.root.join(ACCEPTED))? != self.accepted
            || optional_inventory(&self.root.join(PROJECT_SOURCE_IMPORT_DIRECTORY))? != self.history
        {
            return Err(Error::root("source_input_changed"));
        }
        Ok(())
    }

    /// Service creation may change only its owned paths while the starting authoring bytes stay exact.
    fn recheck_source(&self) -> Result<(), Error> {
        check_root(&self.root)?;
        require_project_ready(&self.root)?;
        absent(&self.root.join(config::FILE_NAME))?;
        checked_source(&self.root, &self.source)?;
        check_service_parent(&self.root)?;
        if Inventory::read(self.project.source())? != self.inventory {
            return Err(Error::root("source_input_changed"));
        }
        Ok(())
    }
}

/// Attach unmanaged Queries sources without converting, moving or rewriting their authoring files.
///
/// An existing accepted store requires an independently supplied pin. If it is absent, explicit
/// initialization creates a new unapproved candidate. Existing verified raw history is preserved;
/// otherwise the starting source receives its own unapproved initial ancestry candidate.
/// Platform selection, workspace enrollment, workflow and Git remain outside this source operation.
///
/// # Errors
/// Returns unsafe paths, existing settings, incomplete source, unsupported layout, invalid version,
/// missing/mismatched independent support, unresolved import or corrupt historical evidence.
pub fn prepare_project_initialization(
    root: &Path,
    source: SourcePath,
    name: ProjectName,
    platform: Option<&PlatformVersion>,
    approved: Option<&AcceptedRef>,
) -> Result<ProjectInitializationPlan, Error> {
    let root = root.canonicalize().map_err(|error| files::io(&error))?;
    checked_directory(&root)?;
    require_project_ready(&root)?;
    absent(&root.join(config::FILE_NAME))?;
    checked_source(&root, &source)?;
    check_service_parent(&root)?;
    let package = Package::read(&root.join(source.as_path()))?;
    if !matches!(
        package.manifest.schema_version,
        QUERY_LAYOUT_VERSION | EXTERNAL_LAYOUT_VERSION
    ) {
        return Err(Error::root("source_init_layout_unsupported"));
    }
    let reader = SourceReader::open(&root.join(source.as_path()))?;
    let metadata_name = reader
        .configuration()
        .declarations
        .first()
        .ok_or_else(|| Error::root("source_configuration_missing"))?
        .name
        .clone();
    let (project, version, manifest) = project_settings(&root, &source, &reader, name, platform)?;
    let accepted = optional_inventory(&root.join(ACCEPTED))?;
    let history = optional_inventory(&root.join(PROJECT_SOURCE_IMPORT_DIRECTORY))?;
    let ancestor = match &accepted {
        Some(_) => {
            let approved = approved.ok_or_else(|| Error::root("accepted_base_pin_required"))?;
            let base = AcceptedBase::open(&root.join(ACCEPTED), approved)?;
            base.validate_source_root(&reader)?;
            if history.is_some() {
                super::verify_project_import_history(&root, &source, &base)?;
                Some(history_reference(&root)?)
            } else {
                None
            }
        }
        None if approved.is_some() => return Err(Error::root("source_file_missing")),
        None if history.is_some() => return Err(Error::root("source_import_state_invalid")),
        None => None,
    };
    let transport = TransportPlan::prepare(&project)?;
    let plan = ProjectInitializationPlan {
        root,
        project,
        source,
        inventory: package.snapshot,
        approved: approved.cloned(),
        accepted,
        history,
        ancestor,
        manifest,
        transport,
        version,
        metadata_name,
        layout_version: package.manifest.schema_version,
    };
    plan.recheck()?;
    Ok(plan)
}

/// Build selected source settings while the native version remains in its sole authoring file.
fn project_settings(
    root: &Path,
    source: &SourcePath,
    reader: &SourceReader,
    name: ProjectName,
    platform: Option<&PlatformVersion>,
) -> Result<(Project, native_version::ProjectVersion, String), Error> {
    let accepted_path = SourcePath::parse(ACCEPTED)?;
    let mut config = ProjectConfig::new(reader.project_type()?)
        .with_name(name)
        .with_source(source.as_path().to_owned())
        .and_then(|config| {
            config.with_source_format(SourceFormat::Eska {
                accepted_base: accepted_path,
            })
        })
        .map_err(|_| Error::root("source_project_settings_invalid"))?;
    if let Some(platform) = platform {
        config = config.with_build_settings(
            BuildSettings::new(platform.as_str(), PathBuf::from("build"))
                .map_err(|_| Error::root("source_project_settings_invalid"))?,
        );
    }
    let unversioned = config
        .clone()
        .into_project(root.to_owned())
        .map_err(|_| Error::root("source_path_invalid"))?;
    let version = native_version::project_source_version(&unversioned)
        .map_err(|_| Error::root("source_init_version_invalid"))?;
    config = config.with_version_scheme(version.scheme());
    let manifest = config
        .to_workspace_member_toml()
        .map_err(|_| Error::root("source_schema_invalid"))?;
    let project = config
        .into_project(root.to_owned())
        .map_err(|_| Error::root("source_path_invalid"))?;
    Ok((project, version, manifest))
}

/// A state already checked by the typed history reader reports a candidate, never its approval.
fn history_reference(root: &Path) -> Result<ImportAncestorRef, Error> {
    let text = files::read_text(
        &root
            .join(PROJECT_SOURCE_IMPORT_DIRECTORY)
            .join("state.json"),
    )?;
    let state: serde_json::Value =
        serde_json::from_str(&text).map_err(|_| Error::root("source_import_state_invalid"))?;
    ImportAncestorRef::from_sha256(
        state["ancestor"]["sha256"]
            .as_str()
            .ok_or_else(|| Error::root("source_import_state_invalid"))?,
    )
}

/// Every existing ancestor of a selected relative source must be an ordinary directory.
fn checked_source(root: &Path, source: &SourcePath) -> Result<(), Error> {
    let mut path = root.to_owned();
    for component in source.as_path().components() {
        path.push(component);
        checked_directory(&path)?;
    }
    Ok(())
}

/// Missing service data stays absent; filesystem aliases are never legitimate historical evidence.
fn optional_inventory(path: &Path) -> Result<Option<Inventory>, Error> {
    match fs::symlink_metadata(path) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(files::io(&error)),
        Ok(_) => Inventory::read(path).map(Some),
    }
}

/// Existing manifests, including dangling aliases, refuse attachment without replacing user state.
fn absent(path: &Path) -> Result<(), Error> {
    match fs::symlink_metadata(path) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(files::io(&error)),
        Ok(_) => Err(Error::root("source_destination_exists")),
    }
}

/// Directory aliases cannot redirect service claims or rollback outside the selected root.
fn checked_directory(path: &Path) -> Result<(), Error> {
    if fs::symlink_metadata(path)
        .map_err(|error| files::io(&error))?
        .file_type()
        .is_dir()
    {
        Ok(())
    } else {
        Err(Error::root("source_entry_unsupported"))
    }
}

/// A planned canonical root cannot be redirected by a later filesystem alias.
fn check_root(root: &Path) -> Result<(), Error> {
    checked_directory(root)?;
    if root.canonicalize().map_err(|error| files::io(&error))? == root {
        Ok(())
    } else {
        Err(Error::root("source_entry_unsupported"))
    }
}

/// Existing unrelated service entries stay untouched, but their parent may not be an alias.
fn check_service_parent(root: &Path) -> Result<(), Error> {
    match fs::symlink_metadata(root.join(".eska")) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(files::io(&error)),
        Ok(metadata) if metadata.file_type().is_dir() => Ok(()),
        Ok(_) => Err(Error::root("source_entry_unsupported")),
    }
}
