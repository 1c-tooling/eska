//! A selected project's import journal owns recovery, never support approval or user artifacts.

mod history;
mod initial;
mod native;
mod publication;
mod recovery;

#[cfg(test)]
mod tests;

use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use super::{
    Error, ImportAncestor, ImportAncestorRef, Inventory, SourceImportPlan, SourcePath, files,
};
use crate::project_core::{
    Project, SourceFormat,
    discovery::{DiscoveryContext, discover_context},
    metadata_model::ProjectScope,
    source_format::{
        AcceptedBase, AcceptedRef, LayoutPublication, prepare_source_import,
        project_migration::{
            ancillary, portable, read_optional, require_project_ready, validate_paths,
        },
    },
};

pub(in crate::project_core::source_format) use history::verified_project_import_history;
pub use history::verify_project_import_history;
pub(in crate::project_core::source_format) use initial::stage_project_import_ancestor;
pub use initial::{initialize_project_import_ancestor, open_initial_project_import_ancestor};
pub use native::{NativeProjectSourceImport, prepare_project_native_source_import};
pub use recovery::recover_project_source_import;

/// Project readers and builders refuse a selected source until the guarded import is resolved.
pub const PROJECT_SOURCE_IMPORT_RECEIPT: &str = ".eska-project-source-import.json";

pub const PROJECT_SOURCE_IMPORT_DIRECTORY: &str = ".eska/source-import";
const SERVICE: &str = PROJECT_SOURCE_IMPORT_DIRECTORY;
const STATE: &str = ".eska/source-import/state.json";
const RECEIPT: &str = "receipt.json";

/// The caller retains this exact journal digest; a local marker cannot advertise its own trust.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectSourceImportRef {
    sha256: String,
}

impl ProjectSourceImportRef {
    /// Parse an independently retained recovery reference, separate from ancestry and CI pins.
    ///
    /// # Errors
    /// Returns an invalid reference unless the digest uses canonical lowercase SHA-256 spelling.
    pub fn from_sha256(sha256: &str) -> Result<Self, Error> {
        if !super::super::valid_hash(sha256) {
            return Err(Error::root("source_import_receipt_invalid"));
        }
        Ok(Self {
            sha256: sha256.into(),
        })
    }

    /// Return the recovery digest without granting support or source ancestry authority.
    #[must_use]
    pub fn sha256(&self) -> &str {
        &self.sha256
    }
}

/// Recovery is explicit; neither path is authorized to overwrite intervening authoring bytes.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SourceImportRecovery {
    Complete,
    RollBack,
}

/// Workspace coordinates are portable and bind inherited settings while only one member is changed.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct WorkspaceSettings {
    ancestor_levels: usize,
    member: String,
    files: BTreeMap<SourcePath, Option<String>>,
}

/// Fingerprints, not machine paths or authored source text, bind all publication decisions.
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Receipt {
    schema_version: u32,
    source: SourcePath,
    accepted: SourcePath,
    approved: AcceptedRef,
    approved_revision: String,
    working_revision: String,
    output_revision: String,
    ancestor: ImportAncestorRef,
    incoming: ImportAncestorRef,
    #[serde(skip_serializing_if = "Option::is_none")]
    native_revision: Option<String>,
    settings: BTreeMap<SourcePath, Option<String>>,
    workspace: Option<WorkspaceSettings>,
    previous_state: Option<String>,
}

/// This pointer selects already persisted incoming bytes, never the merged working source.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct State {
    schema_version: u32,
    source: SourcePath,
    transaction: ProjectSourceImportRef,
    ancestor: ImportAncestorRef,
}

/// Initial bytes are a candidate source snapshot, without a published journal or recovery authority.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct InitialState {
    schema_version: u32,
    origin: InitialOrigin,
    source: SourcePath,
    ancestor: ImportAncestorRef,
}

/// Closed origin spelling prevents an initial record from masquerading as a published transaction.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum InitialOrigin {
    Initial,
}

/// Old published history and the new creation snapshot retain distinct persistence contracts.
enum StoredState {
    Published(State),
    Initial(InitialState),
}

impl StoredState {
    /// Resolving a store never chooses or approves the independently required reference.
    const fn ancestor(&self) -> &ImportAncestorRef {
        match self {
            Self::Published(state) => &state.ancestor,
            Self::Initial(state) => &state.ancestor,
        }
    }

    /// Both locations are derived from closed state variants rather than arbitrary marker paths.
    fn store(&self, root: &Path) -> PathBuf {
        match self {
            Self::Published(state) => transaction(root, &state.transaction).join("incoming"),
            Self::Initial(_) => root.join(SERVICE).join("initial"),
        }
    }
}

/// A preview captures exact settings and a conflict-checked source plan before any project write.
pub struct ProjectSourceImportPlan {
    root: PathBuf,
    source: SourceImportPlan,
    receipt: Receipt,
    text: String,
    reference: ProjectSourceImportRef,
}

/// Prepare an import for the exact discovered standalone project or explicitly selected member.
///
/// Missing synchronization state requires the caller's explicitly confirmed ancestor. Existing
/// state must agree with it. Incoming data and retained local edits never become accepted support.
///
/// # Errors
/// Returns wrong scope/type, missing or untrusted ancestry, unsafe paths, stale inputs or settings.
pub fn prepare_project_source_import(
    project: &Project,
    scope: &ProjectScope,
    expected: &AcceptedRef,
    ancestor: &ImportAncestor,
    incoming: &ImportAncestor,
) -> Result<ProjectSourceImportPlan, Error> {
    require_project_ready(project.root())?;
    checked_directory(project.root())?;
    let workspace = selected_workspace(project, scope)?;
    let SourceFormat::Eska { accepted_base } = project.configuration().source_format() else {
        return Err(Error::root("source_format_mismatch"));
    };
    let source_path = portable(
        project
            .source()
            .strip_prefix(project.root())
            .map_err(|_| Error::root("source_path_invalid"))?,
    )?;
    validate_paths(&source_path, accepted_base)?;
    for path in [&source_path, accepted_base] {
        super::super::super::project_migration::validate_distinct_paths(
            path,
            &SourcePath::parse(SERVICE)?,
        )?;
    }
    checked_directory(project.source())?;
    let approved = AcceptedBase::open(&project.root().join(accepted_base.as_path()), expected)?;
    let root = approved.package.manifest.root_path()?;
    let descriptor = &approved
        .package
        .documents
        .get(&root)
        .ok_or_else(|| Error::root("source_descriptor_missing"))?
        .descriptor;
    if crate::project_core::source_format::root::project_type(descriptor)?
        != project.configuration().project_type()
    {
        return Err(Error::root("source_import_profile_mismatch"));
    }
    let previous_state = read_optional(&project.root().join(STATE))?;
    if let Some(text) = &previous_state {
        let saved = read_state(project.root(), &source_path, text)?;
        if saved.ancestor() != ancestor.reference() {
            return Err(Error::root("source_ancestor_untrusted"));
        }
    }
    let source = prepare_source_import(project.source(), ancestor, incoming, &approved)?;
    let receipt = Receipt {
        schema_version: 1,
        source: source_path,
        accepted: accepted_base.clone(),
        approved: expected.clone(),
        approved_revision: source.approved.inventory.revision.clone(),
        working_revision: source.working_revision().into(),
        output_revision: source.output_revision().into(),
        ancestor: ancestor.reference().clone(),
        incoming: incoming.reference().clone(),
        native_revision: None,
        settings: settings(project.root())?,
        workspace,
        previous_state,
    };
    let (text, reference) = fingerprint(&receipt)?;
    let plan = ProjectSourceImportPlan {
        root: project.root().into(),
        source,
        receipt,
        text,
        reference,
    };
    plan.verify(&|| false)?;
    Ok(plan)
}

/// Open the independently selected initial snapshot or last published raw incoming snapshot.
///
/// # Errors
/// Returns unresolved import, wrong selection, missing state or invalid persisted ancestry.
pub fn project_import_ancestor(
    project: &Project,
    scope: &ProjectScope,
    expected: &ImportAncestorRef,
) -> Result<ImportAncestor, Error> {
    require_project_ready(project.root())?;
    selected_workspace(project, scope)?;
    let path = portable(
        project
            .source()
            .strip_prefix(project.root())
            .map_err(|_| Error::root("source_path_invalid"))?,
    )?;
    let text = read_optional(&project.root().join(STATE))?
        .ok_or_else(|| Error::root("source_ancestor_missing"))?;
    let state = read_state(project.root(), &path, &text)?;
    if state.ancestor() != expected {
        return Err(Error::root("source_ancestor_untrusted"));
    }
    ImportAncestor::open(&state.store(project.root()), expected)
}

impl ProjectSourceImportPlan {
    /// Return the independent ancestry pin retained after successful publication.
    ///
    /// Repeated data with only dump bookkeeping changes keeps its already confirmed raw ancestor.
    /// The captured incoming reference still describes the exact preview input, which may differ.
    #[must_use]
    pub fn resulting_ancestor_reference(&self) -> &ImportAncestorRef {
        self.unchanged_ancestor().unwrap_or(&self.receipt.incoming)
    }

    /// A confirmed repeat needs neither a new journal nor a new independently retained recovery file.
    #[must_use]
    pub fn is_unchanged(&self) -> bool {
        self.unchanged_ancestor().is_some()
    }

    /// A first import must persist its ancestor even when its selected source bytes are unchanged.
    fn unchanged_ancestor(&self) -> Option<&ImportAncestorRef> {
        if self.receipt.working_revision != self.receipt.output_revision {
            return None;
        }
        let state = serde_json::from_str::<State>(self.receipt.previous_state.as_ref()?).ok()?;
        if state.ancestor == self.receipt.incoming {
            Some(&self.receipt.incoming)
        } else if state.ancestor == self.receipt.ancestor && self.source.incoming_data_unchanged {
            Some(&self.receipt.ancestor)
        } else {
            None
        }
    }

    /// Source decisions remain independently reviewable, including conflicts and retained local edits.
    #[must_use]
    pub const fn source_plan(&self) -> &SourceImportPlan {
        &self.source
    }

    /// Retain this reference before publication so recovery does not trust a damaged local marker.
    #[must_use]
    pub const fn reference(&self) -> &ProjectSourceImportRef {
        &self.reference
    }

    /// Fingerprints bind paths and inherited settings before staging or destructive publication.
    fn verify(&self, cancelled: &dyn Fn() -> bool) -> Result<(), Error> {
        verify_settings(&self.root, &self.receipt, cancelled)?;
        if read_optional(&self.root.join(STATE))? != self.receipt.previous_state {
            return Err(Error::root("source_input_changed"));
        }
        self.source.verify_cancellable(cancelled)
    }
}

/// Re-discovery checks member name, root, source path and fully resolved inherited configuration.
fn selected_workspace(
    project: &Project,
    scope: &ProjectScope,
) -> Result<Option<WorkspaceSettings>, Error> {
    match (
        discover_context(project.root()).map_err(|_| Error::root("source_input_changed"))?,
        scope,
    ) {
        (DiscoveryContext::Standalone(saved), ProjectScope::Standalone) if saved == *project => {
            Ok(None)
        }
        (
            DiscoveryContext::Workspace {
                workspace,
                current_member,
            },
            ProjectScope::Member(name),
        ) if current_member.as_ref() == Some(name)
            && workspace
                .members()
                .iter()
                .any(|member| member.name() == name && member.project() == project) =>
        {
            let ancestor_levels = project
                .root()
                .ancestors()
                .position(|root| root == workspace.root())
                .ok_or_else(|| Error::root("source_path_invalid"))?;
            Ok(Some(WorkspaceSettings {
                ancestor_levels,
                member: name.as_str().into(),
                files: settings(workspace.root())?,
            }))
        }
        _ => Err(Error::root("source_import_scope_mismatch")),
    }
}

/// Absence is an expected input too; authored settings remain outside the transaction.
fn settings(root: &Path) -> Result<BTreeMap<SourcePath, Option<String>>, Error> {
    ancillary(root).map(|files| {
        files
            .into_iter()
            .map(|(path, text)| {
                (
                    path,
                    text.map(|text| files::hex(&Sha256::digest(text.as_bytes()))),
                )
            })
            .collect()
    })
}

/// Recovery checks settings directly even while discovery cannot see a temporarily absent source.
fn verify_settings(
    root: &Path,
    receipt: &Receipt,
    cancelled: &dyn Fn() -> bool,
) -> Result<(), Error> {
    files::check_cancelled(cancelled)?;
    checked_directory(root)?;
    if settings(root)? != receipt.settings {
        return Err(Error::root("source_input_changed"));
    }
    if let Some(workspace) = &receipt.workspace {
        let root = root
            .ancestors()
            .nth(workspace.ancestor_levels)
            .ok_or_else(|| Error::root("source_import_receipt_invalid"))?;
        checked_directory(root)?;
        if workspace.ancestor_levels == 0 || settings(root)? != workspace.files {
            return Err(Error::root("source_input_changed"));
        }
    }
    checked_directory(&root.join(receipt.accepted.as_path()))?;
    let approved = AcceptedBase::open_cancellable(
        &root.join(receipt.accepted.as_path()),
        &receipt.approved,
        cancelled,
    )?;
    if approved.verified_inventory_cancellable(cancelled)?.revision != receipt.approved_revision {
        return Err(Error::root("source_input_changed"));
    }
    Ok(())
}

/// A state pointer must resolve to the independently verified incoming store it names.
fn read_state(root: &Path, source: &SourcePath, text: &str) -> Result<StoredState, Error> {
    let invalid = || Error::root("source_import_state_invalid");
    let value: serde_json::Value = serde_json::from_str(text).map_err(|_| invalid())?;
    let state = match value
        .get("schema_version")
        .and_then(serde_json::Value::as_u64)
    {
        Some(1) => {
            let state: State = serde_json::from_str(text).map_err(|_| invalid())?;
            if state.source != *source {
                return Err(invalid());
            }
            ProjectSourceImportRef::from_sha256(&state.transaction.sha256)?;
            checked_directory(&transaction(root, &state.transaction))?;
            StoredState::Published(state)
        }
        Some(2) => {
            let state: InitialState = serde_json::from_str(text).map_err(|_| invalid())?;
            if state.source != *source {
                return Err(invalid());
            }
            checked_directory(&root.join(SERVICE))?;
            StoredState::Initial(state)
        }
        _ => return Err(invalid()),
    };
    checked_directory(&state.store(root))?;
    ImportAncestor::open(&state.store(root), state.ancestor())?;
    Ok(state)
}

/// Content addressing avoids absolute paths in either synchronization state or recovery references.
fn transaction(root: &Path, reference: &ProjectSourceImportRef) -> PathBuf {
    root.join(SERVICE).join(&reference.sha256)
}

/// Both source-only and native preparations use the same independently retained recovery digest.
fn fingerprint(receipt: &Receipt) -> Result<(String, ProjectSourceImportRef), Error> {
    let text =
        serde_json::to_string_pretty(receipt).map_err(|_| Error::root("source_schema_invalid"))?;
    let reference =
        ProjectSourceImportRef::from_sha256(&files::hex(&Sha256::digest(text.as_bytes())))?;
    Ok((text, reference))
}

/// Aliases at any component cannot redirect a journal, backup, source or accepted store.
fn checked_directory(path: &Path) -> Result<(), Error> {
    if !fs::symlink_metadata(path)
        .map_err(|error| files::io(&error))?
        .file_type()
        .is_dir()
        || path.canonicalize().map_err(|error| files::io(&error))? != path
    {
        return Err(Error::root("source_path_invalid"));
    }
    Ok(())
}
