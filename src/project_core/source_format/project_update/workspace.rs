//! Whole-workspace updates retain settings and validate independently pinned supplies by member name.

mod publication;
#[cfg(test)]
mod tests;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

use super::{ProjectBaseUpdatePlan, Receipt, prepare};
use crate::project_core::{
    ProjectName, Workspace,
    config::WorkspaceConfig,
    source_format::{
        BaseUpdatePlan, Error, SourcePath, WorkspaceAcceptedRefs, files,
        project_migration::{
            ancillary, destination_path, read_optional, require_project_ready,
            workspace::{member_paths, verify_workspace},
        },
    },
};

/// Every nested project remains unavailable until all independently reviewed updates are complete.
pub const WORKSPACE_BASE_UPDATE_RECEIPT: &str = ".eska-workspace-base-update.json";

/// Receipt coordinates refer to the original member paths, independently of incoming paths/order.
#[derive(Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct MemberReceipt {
    path: SourcePath,
    project: Receipt,
}

/// One recovery identity binds all member revisions, pins and exact workspace settings.
#[derive(Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct WorkspaceReceipt {
    schema_version: u32,
    ancillary: BTreeMap<SourcePath, String>,
    members: BTreeMap<String, MemberReceipt>,
}

/// Per-member previews retain each independent approval and the shared comparison model.
pub struct WorkspaceBaseUpdateMember {
    name: ProjectName,
    path: SourcePath,
    plan: ProjectBaseUpdatePlan,
}

impl WorkspaceBaseUpdateMember {
    /// Stable workspace names select supplies; native metadata UUIDs validate that selection.
    #[must_use]
    pub const fn name(&self) -> &ProjectName {
        &self.name
    }

    /// The output keeps the current workspace's relative member path.
    #[must_use]
    pub const fn path(&self) -> &SourcePath {
        &self.path
    }

    /// Publication belongs to the whole workspace rather than a detached member copy.
    #[must_use]
    pub fn destination(&self) -> &Path {
        self.plan.destination()
    }

    /// Source, support and conflict previews are identical to the standalone core model.
    #[must_use]
    pub const fn source_plan(&self) -> &BaseUpdatePlan {
        self.plan.source_plan()
    }

    /// Exact member settings are copied without rewriting inherited defaults.
    pub fn ancillary_files(&self) -> impl Iterator<Item = &SourcePath> {
        self.plan.ancillary_files()
    }
}

/// No member is published until every input and destination passes the same whole-workspace plan.
pub struct WorkspaceBaseUpdatePlan {
    workspace: Workspace,
    incoming: Workspace,
    incoming_manifest: String,
    destination: PathBuf,
    ancillary: BTreeMap<SourcePath, Option<String>>,
    members: Vec<WorkspaceBaseUpdateMember>,
    receipt: WorkspaceReceipt,
}

/// Prepare separately approved supplies for every current workspace member, retaining exact settings.
///
/// Both pin documents and member-name sets must be complete and equal. Incoming member paths and
/// declaration order may differ; native identity and profile still match independently per member.
/// All current root/member settings are retained, while incoming editable sources are ignored.
///
/// # Errors
/// Returns missing/untrusted pins, mismatched names, invalid members, stale inputs or foreign output.
pub fn prepare_workspace_base_update(
    workspace: &Workspace,
    previous: &WorkspaceAcceptedRefs,
    incoming: &Workspace,
    incoming_expected: &WorkspaceAcceptedRefs,
    destination: &Path,
) -> Result<WorkspaceBaseUpdatePlan, Error> {
    for current in [workspace, incoming] {
        require_project_ready(current.root())?;
        verify_workspace(current)?;
    }
    previous.check_members(workspace)?;
    incoming_expected.check_members(incoming)?;
    let by_name = incoming
        .members()
        .iter()
        .map(|member| (member.name(), member))
        .collect::<BTreeMap<_, _>>();
    if workspace.members().len() != by_name.len()
        || workspace
            .members()
            .iter()
            .any(|member| !by_name.contains_key(member.name()))
    {
        return Err(Error::root("source_update_members_mismatch"));
    }
    let ancillary = ancillary(workspace.root())?;
    let manifest = ancillary
        .get(&SourcePath::parse("eska.toml")?)
        .and_then(Option::as_deref)
        .ok_or_else(|| Error::root("source_file_missing"))?;
    let config =
        WorkspaceConfig::from_toml(manifest).map_err(|_| Error::root("source_schema_invalid"))?;
    let incoming_manifest = read_optional(&incoming.root().join("eska.toml"))?
        .ok_or_else(|| Error::root("source_file_missing"))?;
    let paths = member_paths(workspace, &config)?;
    let destination = destination_path(workspace.root(), destination)?;
    destination_path(incoming.root(), &destination)?;
    let members = workspace
        .members()
        .iter()
        .zip(paths)
        .map(|(member, path)| {
            let supplied = by_name
                .get(member.name())
                .ok_or_else(|| Error::root("source_update_members_mismatch"))?;
            let old = previous
                .reference(member.name())
                .ok_or_else(|| Error::root("source_workspace_pins_mismatch"))?;
            let new = incoming_expected
                .reference(member.name())
                .ok_or_else(|| Error::root("source_workspace_pins_mismatch"))?;
            let plan = prepare(
                member.project(),
                old,
                supplied.project(),
                new,
                &destination.join(path.as_path()),
                Some(&config),
            )
            .map_err(|error| error.in_field(member.name().as_str()))?;
            Ok(WorkspaceBaseUpdateMember {
                name: member.name().clone(),
                path,
                plan,
            })
        })
        .collect::<Result<Vec<_>, Error>>()?;
    let receipt = WorkspaceReceipt {
        schema_version: 1,
        ancillary: ancillary
            .iter()
            .filter_map(|(path, text)| {
                text.as_ref()
                    .map(|text| (path.clone(), files::hex(&Sha256::digest(text.as_bytes()))))
            })
            .collect(),
        members: members
            .iter()
            .map(|member| {
                (
                    member.name.to_string(),
                    MemberReceipt {
                        path: member.path.clone(),
                        project: member.plan.receipt.clone(),
                    },
                )
            })
            .collect(),
    };
    let plan = WorkspaceBaseUpdatePlan {
        workspace: workspace.clone(),
        incoming: incoming.clone(),
        incoming_manifest,
        destination,
        ancillary,
        members,
        receipt,
    };
    plan.recheck()?;
    plan.check_output()?;
    Ok(plan)
}

impl WorkspaceBaseUpdatePlan {
    /// The result remains a distinct root with the original workspace declaration order and paths.
    #[must_use]
    pub fn destination(&self) -> &Path {
        &self.destination
    }

    /// Member order follows current settings rather than incoming declaration order.
    #[must_use]
    pub fn members(&self) -> &[WorkspaceBaseUpdateMember] {
        &self.members
    }

    /// One unresolved conflict prevents publication of every member and the root manifest.
    #[must_use]
    pub fn can_write(&self) -> bool {
        self.members
            .iter()
            .all(|member| member.source_plan().conflicts().is_empty())
    }

    /// Root settings are preserved separately from member-local manifests and bytes.
    pub fn ancillary_files(&self) -> impl Iterator<Item = &SourcePath> {
        self.ancillary
            .iter()
            .filter_map(|(path, text)| text.is_some().then_some(path))
    }

    /// Both workspace selections and exact settings remain live inputs throughout publication.
    fn recheck(&self) -> Result<(), Error> {
        for workspace in [&self.workspace, &self.incoming] {
            require_project_ready(workspace.root())?;
            verify_workspace(workspace)?;
        }
        if ancillary(self.workspace.root())? != self.ancillary
            || read_optional(&self.incoming.root().join("eska.toml"))?.as_deref()
                != Some(&self.incoming_manifest)
        {
            return Err(Error::root("source_input_changed"));
        }
        for member in &self.members {
            member.plan.recheck()?;
        }
        Ok(())
    }
}
