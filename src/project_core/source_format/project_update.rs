//! Project publication binds source choices and independently approved incoming authority.

mod publication;
mod workspace;

pub use workspace::{
    WORKSPACE_BASE_UPDATE_RECEIPT, WorkspaceBaseUpdateMember, WorkspaceBaseUpdatePlan,
    prepare_workspace_base_update,
};

#[cfg(test)]
mod tests;

use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use super::{
    AcceptedBase, AcceptedRef, BaseUpdatePlan, Error, SourcePath, files, prepare_base_update,
    project_migration::{
        ancillary, destination_path, portable, require_project_ready, validate_paths,
    },
};
use crate::project_core::{
    Project, SourceFormat,
    config::{ProjectConfig, WorkspaceConfig},
    discovery::resolve_member_configuration,
    metadata_model::ProjectScope,
};

/// Source and base are unavailable to project consumers while either publication is incomplete.
pub const PROJECT_BASE_UPDATE_RECEIPT: &str = ".eska-project-base-update.json";

/// No machine paths or working values are stored in an independently verifiable recovery marker.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Receipt {
    schema_version: u32,
    working_revision: String,
    output_revision: String,
    previous_pin: String,
    incoming_pin: String,
    ancillary: BTreeMap<SourcePath, String>,
}

/// The original project and incoming project remain available through preview, write and recovery.
pub struct ProjectBaseUpdatePlan {
    root: PathBuf,
    incoming_root: PathBuf,
    incoming_settings: String,
    destination: PathBuf,
    source_path: SourcePath,
    base_path: SourcePath,
    source: BaseUpdatePlan,
    ancillary: BTreeMap<SourcePath, Option<String>>,
    receipt: Receipt,
}

/// Prepare an explicit standalone copy using two separately approved base references.
///
/// Only the incoming project's pinned accepted store supplies new bytes and rights; its editable
/// source is not reaccepted. The original manifest and attributes are copied exactly. Member scope
/// is refused instead of turning inherited settings into a different standalone configuration.
///
/// # Errors
/// Returns unsupported scope/format/type, changed settings, untrusted pins, overlap or foreign output.
pub fn prepare_project_base_update(
    project: &Project,
    scope: &ProjectScope,
    previous: &AcceptedRef,
    incoming: &Project,
    incoming_expected: &AcceptedRef,
    destination: &Path,
) -> Result<ProjectBaseUpdatePlan, Error> {
    if *scope != ProjectScope::Standalone {
        return Err(Error::root("source_update_scope_unsupported"));
    }
    prepare(
        project,
        previous,
        incoming,
        incoming_expected,
        destination,
        None,
    )
}

/// Only whole-workspace orchestration may retain an inherited member manifest in a new copy.
fn prepare(
    project: &Project,
    previous: &AcceptedRef,
    incoming: &Project,
    incoming_expected: &AcceptedRef,
    destination: &Path,
    workspace: Option<&WorkspaceConfig>,
) -> Result<ProjectBaseUpdatePlan, Error> {
    require_project_ready(project.root())?;
    require_project_ready(incoming.root())?;
    let SourceFormat::Eska { accepted_base } = project.configuration().source_format() else {
        return Err(Error::root("source_format_mismatch"));
    };
    let SourceFormat::Eska {
        accepted_base: incoming_base,
    } = incoming.configuration().source_format()
    else {
        return Err(Error::root("source_format_mismatch"));
    };
    let ancillary = ancillary(project.root())?;
    let manifest = ancillary
        .get(&SourcePath::parse("eska.toml")?)
        .and_then(Option::as_deref)
        .ok_or_else(|| Error::root("source_file_missing"))?;
    let config =
        ProjectConfig::from_toml(manifest).map_err(|_| Error::root("source_schema_invalid"))?;
    verify_selected_project(project, config, workspace)?;
    let incoming_settings =
        super::project_migration::read_optional(&incoming.root().join("eska.toml"))?
            .ok_or_else(|| Error::root("source_file_missing"))?;
    let declared = ProjectConfig::from_toml(&incoming_settings)
        .map_err(|_| Error::root("source_schema_invalid"))?;
    // Only these locally declared fields select incoming bytes; inherited build settings are unused.
    if declared.configuration().source_format() != incoming.configuration().source_format()
        || declared.configuration().project_type() != incoming.configuration().project_type()
        || incoming.root().join(declared.source()) != incoming.source()
    {
        return Err(Error::root("source_input_changed"));
    }
    let source_path = portable(
        project
            .source()
            .strip_prefix(project.root())
            .map_err(|_| Error::root("source_path_invalid"))?,
    )?;
    validate_paths(&source_path, accepted_base)?;
    let old = AcceptedBase::open(&project.root().join(accepted_base.as_path()), previous)?;
    let new = AcceptedBase::open(
        &incoming.root().join(incoming_base.as_path()),
        incoming_expected,
    )?;
    for (project, base) in [(project, &old), (incoming, &new)] {
        let document = base
            .package
            .documents
            .get(&base.package.manifest.root_path()?)
            .ok_or_else(|| Error::root("source_descriptor_missing"))?;
        if super::root::project_type(&document.descriptor)?
            != project.configuration().project_type()
        {
            return Err(Error::root("source_update_profile_mismatch"));
        }
    }
    let source = prepare_base_update(project.source(), &old, &new)?;
    let destination = if workspace.is_some() {
        destination.to_owned()
    } else {
        let resolved = destination_path(project.root(), destination)?;
        destination_path(incoming.root(), &resolved)?;
        resolved
    };
    let receipt = Receipt {
        schema_version: 1,
        working_revision: source.working_revision().into(),
        output_revision: source.output_revision().into(),
        previous_pin: previous.sha256.clone(),
        incoming_pin: incoming_expected.sha256.clone(),
        ancillary: ancillary
            .iter()
            .filter_map(|(path, text)| {
                text.as_ref()
                    .map(|text| (path.clone(), files::hex(&Sha256::digest(text.as_bytes()))))
            })
            .collect(),
    };
    let plan = ProjectBaseUpdatePlan {
        root: project.root().to_owned(),
        incoming_root: incoming.root().to_owned(),
        incoming_settings,
        destination,
        source_path,
        base_path: accepted_base.clone(),
        source,
        ancillary,
        receipt,
    };
    plan.recheck()?;
    plan.check_output()?;
    Ok(plan)
}

/// Re-resolve saved settings before either scope consumes paths or inherited policy.
fn verify_selected_project(
    project: &Project,
    config: ProjectConfig,
    workspace: Option<&WorkspaceConfig>,
) -> Result<(), Error> {
    let loaded = if let Some(workspace) = workspace {
        let configuration = resolve_member_configuration(workspace, &config)
            .map_err(|_| Error::root("source_schema_invalid"))?;
        Project::new(
            project.root().to_owned(),
            project.root().join(config.source()),
            configuration,
        )
        .map_err(|_| Error::root("source_path_invalid"))?
    } else {
        config
            .into_project(project.root().to_owned())
            .map_err(|_| Error::root("source_schema_invalid"))?
    };
    if loaded != *project {
        return Err(Error::root("source_input_changed"));
    }
    Ok(())
}

impl ProjectBaseUpdatePlan {
    /// The destination is a distinct project root; input manifests and CI references stay intact.
    #[must_use]
    pub fn destination(&self) -> &Path {
        &self.destination
    }

    /// The project wrapper adds publication boundaries without another source comparison model.
    #[must_use]
    pub const fn source_plan(&self) -> &BaseUpdatePlan {
        &self.source
    }

    /// Enumerate only exact ancillary settings included in this publication.
    pub fn ancillary_files(&self) -> impl Iterator<Item = &SourcePath> {
        self.ancillary
            .iter()
            .filter_map(|(path, text)| text.is_some().then_some(path))
    }

    /// Changed manifests cannot reuse a plan that selected paths or logical types from older settings.
    fn recheck(&self) -> Result<(), Error> {
        if ancillary(&self.root)? != self.ancillary
            || super::project_migration::read_optional(&self.incoming_root.join("eska.toml"))?
                .as_deref()
                != Some(&self.incoming_settings)
        {
            return Err(Error::root("source_input_changed"));
        }
        self.source.recheck()
    }
}
