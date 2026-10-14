//! Full descriptor identities address independent nested-form slots within one YAML owner.

#[cfg(test)]
mod tests;

use std::collections::BTreeMap;

use base_db::{SourceDatabase, SourceRootId};
use eska::{
    analysis_core::{InputState, OwnerInput},
    project_core::{
        MetadataKind,
        source_format::{SourcePath, SourceReader},
    },
};
use ide_db::{
    RootDatabaseImpl,
    metadata::{CanonicalMetadataInput, CanonicalMetadataInputs},
};
use vfs::{FileId, VfsPath, file_set::FileSet};

use super::{OwnerEntry, ProjectionRevision, metadata_resources::MetadataResourceEntry, set_slot};
use crate::{AnalysisScope, FormModel, ModelState, SessionError, SessionRevision, projection};

/// Stable slots belong to the enclosing owner; only their current dependency pair can change them.
pub(super) struct FormEntry {
    slot: CanonicalMetadataInput,
    dependencies: Option<ProjectionRevision>,
    pub(super) model: FormModel,
}

/// Register every nested descriptor, including forms whose declared dialog is absent or binary.
pub(super) fn register(
    reader: &SourceReader,
    owners: &mut BTreeMap<SourcePath, OwnerEntry>,
    database: &mut RootDatabaseImpl,
    file_set: &mut FileSet,
    slots: &mut CanonicalMetadataInputs,
) -> Result<usize, SessionError> {
    let mut next_file = owners.len();
    for path in reader.form_paths() {
        let identity = reader
            .declared_form(path)
            .ok_or(SessionError::MissingListing)?;
        if identity.kind != MetadataKind::Form
            || reader
                .declared_owner(identity.owner)
                .map_err(SessionError::Source)?
                .native_path
                == path
        {
            continue;
        }
        let file = FileId(u32::try_from(next_file).map_err(|_| SessionError::TooManyFiles)?);
        next_file = next_file.checked_add(1).ok_or(SessionError::TooManyFiles)?;
        file_set.insert(file, VfsPath::new(reader.root().join(path.as_path())));
        database.set_file_source_root(file, SourceRootId(0));
        database.set_file_unreadable(file);
        let slot = CanonicalMetadataInput::new(database, None, "pending".into());
        slots.insert(file, slot);
        let entry = owners
            .get_mut(identity.owner)
            .ok_or(SessionError::MissingListing)?;
        let model = FormModel {
            native_path: path.clone(),
            dialog: identity.dialog.cloned(),
            revision: SessionRevision::default(),
            state: ModelState::Pending,
        };
        entry.model.forms.push(model.clone());
        entry.forms.insert(
            path.clone(),
            FormEntry {
                slot,
                dependencies: None,
                model,
            },
        );
    }
    Ok(next_file)
}

/// A sibling resource edit reuses unchanged form values and their Salsa dependency revisions.
pub(super) fn publish(
    forms: &mut BTreeMap<SourcePath, FormEntry>,
    input: &OwnerInput,
    resources: &BTreeMap<SourcePath, MetadataResourceEntry>,
    database: &mut RootDatabaseImpl,
    revision: SessionRevision,
    scope: AnalysisScope,
) {
    if forms.is_empty() {
        return;
    }
    let documents = match &input.state {
        InputState::Ready(snapshot) => snapshot
            .documents
            .iter()
            .map(|document| (&document.path, document))
            .collect::<BTreeMap<_, _>>(),
        InputState::Pending | InputState::Unavailable(_) => BTreeMap::new(),
    };
    for (path, entry) in forms {
        let resource = entry
            .model
            .dialog
            .as_ref()
            .and_then(|path| resources.get(path).map(|entry| (path, &entry.input)));
        let dependencies = ProjectionRevision {
            owner: input.revision,
            resource: resource.map(|(_, input)| input.revision),
        };
        if entry.dependencies == Some(dependencies) {
            continue;
        }
        entry.model.state = if scope == AnalysisScope::LocalSyntax {
            projection::source_state(input)
        } else {
            projection::form_state(input, documents.get(path).copied(), path, resource)
        };
        entry.model.revision = revision;
        entry.dependencies = Some(dependencies);
        set_slot(database, entry.slot, &entry.model.state, revision);
    }
}
