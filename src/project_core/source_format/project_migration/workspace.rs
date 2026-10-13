//! Whole-workspace migration retains exact manifests and independent authority for every member.

mod pins;
mod publication;

#[cfg(test)]
mod tests;

pub use pins::WorkspaceAcceptedRefs;

use super::{
    AcceptedRef, BTreeMap, Error, Path, PathBuf, ProjectLayoutPlan, Receipt, SourcePath, Target,
    WorkspaceConfig, ancillary, destination_path, files, portable, prepare_project,
    require_project_ready,
};
use crate::project_core::{
    ProjectName, Workspace,
    discovery::{DiscoveryContext, discover_context},
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// Nested projects remain unavailable until the entire workspace has been published.
pub const WORKSPACE_LAYOUT_RECEIPT: &str = ".eska-workspace-layout.json";

/// A member's trust and storage revisions are bound to its exact portable workspace coordinate.
#[derive(Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct MemberReceipt {
    path: SourcePath,
    project: Receipt,
}

/// The outer receipt authorizes no writes beyond the declared members and exact root settings.
#[derive(Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct WorkspaceReceipt {
    schema_version: u32,
    ancillary: BTreeMap<SourcePath, String>,
    members: BTreeMap<String, MemberReceipt>,
}

/// Public member previews expose pins and changes without leaking reconstruction authority.
#[derive(Debug)]
pub struct WorkspaceLayoutMember {
    name: ProjectName,
    path: SourcePath,
    plan: ProjectLayoutPlan,
}

impl WorkspaceLayoutMember {
    /// Stable member identity is independent of its native configuration name.
    #[must_use]
    pub const fn name(&self) -> &ProjectName {
        &self.name
    }

    /// The destination retains the configured relative member directory.
    #[must_use]
    pub const fn path(&self) -> &SourcePath {
        &self.path
    }

    /// A member destination is published exclusively through its containing workspace plan.
    #[must_use]
    pub fn destination(&self) -> &Path {
        self.plan.destination()
    }

    /// Working-source values and annotations have their own migration preview.
    #[must_use]
    pub fn source_changes(&self) -> &[super::LayoutChange] {
        self.plan.source_changes()
    }

    /// Accepted changes originate exclusively in this member's independently supplied old pin.
    #[must_use]
    pub fn accepted_changes(&self) -> &[super::LayoutChange] {
        self.plan.accepted_changes()
    }

    /// Keep this member's previous trust selection available for review.
    #[must_use]
    pub const fn previous_reference(&self) -> &AcceptedRef {
        self.plan.previous_reference()
    }

    /// Each member's new checksum requires independent CI approval after publication.
    #[must_use]
    pub const fn accepted_reference(&self) -> &AcceptedRef {
        self.plan.accepted_reference()
    }

    /// Layout no-op retains all authored bytes and the original member pin.
    #[must_use]
    pub fn layout_changed(&self) -> bool {
        self.plan.layout_changed()
    }

    /// Member settings remain separately visible from root workspace settings and storage.
    pub fn ancillary_files(&self) -> impl Iterator<Item = &SourcePath> {
        self.plan.ancillary_files()
    }
}

/// All members are planned before publication; the root manifest is the last ready input.
#[derive(Debug)]
pub struct WorkspaceLayoutPlan {
    workspace: Workspace,
    destination: PathBuf,
    ancillary: BTreeMap<SourcePath, Option<String>>,
    members: Vec<WorkspaceLayoutMember>,
    receipt: WorkspaceReceipt,
}

/// Prepare a whole readable workspace without creating directories or changing CI approval.
///
/// Every declared member must have an independently supplied pin and an ESKA configuration.
/// Root/member settings are retained byte-for-byte, including inheritance and explicit overrides.
/// Unlisted project files, Git state, artifacts and generations are not copied.
///
/// # Errors
/// Returns mismatched pins, changed settings, unsupported members, unsafe paths or foreign outputs.
pub fn prepare_readable_workspace(
    workspace: &Workspace,
    expected: &WorkspaceAcceptedRefs,
    destination: &Path,
) -> Result<WorkspaceLayoutPlan, Error> {
    prepare(workspace, expected, destination, Target::Readable)
}

/// Convert every declared member to map 4 with independent pins and exact inherited/explicit settings.
///
/// All plans validate before publication; the workspace ready marker remains the last published input.
///
/// # Errors
/// Returns untrusted/invalid members, unverified dialogs, unsafe paths or conflicting output.
pub fn prepare_form_workspace(
    workspace: &Workspace,
    expected: &WorkspaceAcceptedRefs,
    destination: &Path,
) -> Result<WorkspaceLayoutPlan, Error> {
    prepare(workspace, expected, destination, Target::Forms)
}

/// Both explicit project layouts retain the same independent member authority and outer receipt.
fn prepare(
    workspace: &Workspace,
    expected: &WorkspaceAcceptedRefs,
    destination: &Path,
    target: Target,
) -> Result<WorkspaceLayoutPlan, Error> {
    require_project_ready(workspace.root())?;
    verify_workspace(workspace)?;
    let ancillary = ancillary(workspace.root())?;
    let manifest = ancillary
        .get(&SourcePath::parse("eska.toml")?)
        .and_then(Option::as_deref)
        .ok_or_else(|| Error::root("source_file_missing"))?;
    let config =
        WorkspaceConfig::from_toml(manifest).map_err(|_| Error::root("source_schema_invalid"))?;
    expected.check_members(workspace)?;
    let paths = member_paths(workspace, &config)?;
    let destination = destination_path(workspace.root(), destination)?;
    let members = workspace
        .members()
        .iter()
        .zip(paths)
        .map(|(member, path)| {
            let pin = expected
                .reference(member.name())
                .ok_or_else(|| Error::root("source_workspace_pins_mismatch"))?;
            let plan = prepare_project(
                member.project(),
                pin,
                &destination.join(path.as_path()),
                Some(&config),
                target,
            )
            .map_err(|error| error.in_field(member.name().as_str()))?;
            Ok(WorkspaceLayoutMember {
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
    let plan = WorkspaceLayoutPlan {
        workspace: workspace.clone(),
        destination,
        ancillary,
        members,
        receipt,
    };
    plan.recheck()?;
    plan.check_output()?;
    Ok(plan)
}

impl WorkspaceLayoutPlan {
    /// The absolute output root contains the same relative declared member paths.
    #[must_use]
    pub fn destination(&self) -> &Path {
        &self.destination
    }

    /// Retain declaration order rather than inferring members from destination directories.
    #[must_use]
    pub fn members(&self) -> &[WorkspaceLayoutMember] {
        &self.members
    }

    /// A readable workspace may be copied with all pins and authored bytes unchanged.
    #[must_use]
    pub fn layout_changed(&self) -> bool {
        self.members
            .iter()
            .any(|member| member.plan.layout_changed())
    }

    /// Only exact existing root settings are copied, separately from member inventories.
    pub fn ancillary_files(&self) -> impl Iterator<Item = &SourcePath> {
        self.ancillary
            .iter()
            .filter_map(|(path, text)| text.is_some().then_some(path))
    }

    /// Any original root/member setting or source/base byte invalidates an old publication plan.
    fn recheck(&self) -> Result<(), Error> {
        require_project_ready(self.workspace.root())?;
        if ancillary(self.workspace.root())? != self.ancillary {
            return Err(Error::root("source_input_changed"));
        }
        verify_workspace(&self.workspace)?;
        for member in &self.members {
            member.plan.recheck()?;
        }
        Ok(())
    }
}

/// Reuse ordinary discovery so inheritance, member validation and names have one implementation.
fn verify_workspace(expected: &Workspace) -> Result<(), Error> {
    match discover_context(expected.root()) {
        Ok(DiscoveryContext::Workspace { workspace, .. }) if workspace == *expected => Ok(()),
        _ => Err(Error::root("source_input_changed")),
    }
}

/// Aliased or case-colliding directories cannot become a different workspace on another system.
fn member_paths(workspace: &Workspace, config: &WorkspaceConfig) -> Result<Vec<SourcePath>, Error> {
    let mut paths = Vec::new();
    for (member, configured) in workspace.members().iter().zip(config.members()) {
        let path = portable(configured)?;
        if workspace.root().join(path.as_path()) != member.root() {
            return Err(Error::root("source_path_invalid"));
        }
        for reserved in [
            "eska.toml",
            ".gitattributes",
            ".gitignore",
            ".git",
            super::PROJECT_LAYOUT_RECEIPT,
            WORKSPACE_LAYOUT_RECEIPT,
        ] {
            super::validate_distinct_paths(&path, &SourcePath::parse(reserved)?)?;
        }
        for existing in &paths {
            super::validate_distinct_paths(&path, existing)?;
        }
        paths.push(path);
    }
    Ok(paths)
}
