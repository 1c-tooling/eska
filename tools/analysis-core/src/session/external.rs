//! One canonical external artifact owns its real metadata without impersonating a CF/CFE root.

use std::sync::Arc;

use bsl_metadata::ExternalObjectKind;
use eska::{
    analysis_core::{InputState, OwnerInput},
    project_core::{
        MetadataKind, ProjectType,
        source_format::{SourcePath, SourceReader},
    },
};
use ide_db::{
    RootDatabaseImpl,
    metadata::{
        CanonicalMetadata, MdoEntry, MetadataListingData, RootKind, WorkspaceConfigsSnapshot,
    },
};

use crate::{ModelState, OwnerModel, SessionError, SessionRevision, binding::OwnerBinding};

/// Validate admission before any kernel context is installed, retaining explicit syntax and CFE guards.
pub(super) fn admit(
    reader: &SourceReader,
    scope: super::AnalysisScope,
) -> Result<Option<ExternalObjectKind>, SessionError> {
    match scope {
        super::AnalysisScope::CanonicalExternal => kind(reader).map(Some),
        super::AnalysisScope::CanonicalConfiguration => {
            super::require_supported_project(reader)?;
            Ok(None)
        }
        super::AnalysisScope::LocalSyntax => {
            reader.project_type().map_err(SessionError::Source)?;
            Ok(None)
        }
    }
}

impl crate::AnalysisSession {
    /// Native target discovery cannot resolve foreign metadata using only this artifact's own model.
    pub(crate) fn require_own_external_target(
        &self,
        kind: bsl_metadata::MdoType,
        name: &str,
    ) -> Result<(), crate::QueryError> {
        if let Some(external_kind) = self.selected_root.external_kind
            && (kind != external_kind.mdo_type()
                || self
                    .selected_root
                    .inputs
                    .source_reader()
                    .configuration()
                    .declarations
                    .first()
                    .is_none_or(|owner| {
                        !hir::Name::new(&owner.name).eq_ignore_case(&hir::Name::new(name))
                    }))
        {
            return Err(crate::QueryError::Request(
                crate::SessionRequestError::ConfigurationContextRequired,
            ));
        }
        Ok(())
    }
}

/// Only the reader's validated artifact profile supplies the external semantic discriminator.
pub(super) fn kind(reader: &SourceReader) -> Result<ExternalObjectKind, SessionError> {
    match reader.project_type().map_err(SessionError::Source)? {
        ProjectType::Processing => Ok(ExternalObjectKind::DataProcessor),
        ProjectType::Report => Ok(ExternalObjectKind::Report),
        actual => Err(SessionError::UnsupportedProjectType(actual)),
    }
}

/// Register the one known leaf root with no semantic base and no whole-Configuration cache.
pub(super) fn install(
    reader: &SourceReader,
    database: &mut RootDatabaseImpl,
) -> Result<(), SessionError> {
    let kind = kind(reader)?;
    let name = reader
        .configuration()
        .declarations
        .first()
        .ok_or(SessionError::MissingListing)?
        .name
        .clone();
    let root = reader.root().to_path_buf();
    database.set_workspace_configs_snapshot(WorkspaceConfigsSnapshot {
        paths: vec![(Some(name), root.clone())],
        kinds: vec![RootKind::External(kind)],
        canonical_paths: vec![root],
        closures: vec![Vec::new()],
        topological_order: vec![0],
        ..WorkspaceConfigsSnapshot::from_paths(Vec::new())
    });
    Ok(())
}

/// The standalone root's normalized kind gains its external identity only at this explicit boundary.
pub(super) fn bind_root(
    reader: &SourceReader,
    source: &SourcePath,
    binding: &mut OwnerBinding,
    listing: &mut MetadataListingData,
) -> Result<bool, SessionError> {
    if source != &reader.configuration().path {
        return Ok(false);
    }
    let kind = kind(reader)?;
    if binding.kind != normalized(kind) || binding.nested {
        return Err(SessionError::MissingListing);
    }
    let declaration = reader
        .configuration()
        .declarations
        .first()
        .ok_or(SessionError::MissingListing)?;
    binding.name = Some(declaration.name.clone());
    listing.entries.push(MdoEntry {
        kind: kind.mdo_type(),
        name: declaration.name.clone(),
        main: binding.main,
        predefined: None,
    });
    Ok(true)
}

/// Project current root properties through the same accepted-input clock without changing support.
pub(super) fn project(
    kind: ExternalObjectKind,
    input: &OwnerInput,
    revision: SessionRevision,
) -> OwnerModel {
    let mut model = OwnerModel {
        revision,
        origin: input.origin,
        source_sha256: input.source_sha256.clone(),
        descriptors: 0,
        parse_issues: 0,
        modeled_descriptors: 0,
        state: ModelState::Pending,
        forms: Vec::new(),
    };
    model.state = match &input.state {
        InputState::Pending => ModelState::Pending,
        InputState::Unavailable(failure) => ModelState::Unavailable(failure.clone()),
        InputState::Ready(snapshot) => {
            model.descriptors = snapshot.documents.len();
            model.parse_issues = snapshot
                .documents
                .iter()
                .map(|document| document.descriptor.diagnostics.len())
                .sum();
            let value = snapshot.documents.first().map_or_else(
                || {
                    Err(eska_bsl_adapter::AdapterError {
                        issue: eska_bsl_adapter::AdapterIssue::MissingRoot,
                        range: 0..0,
                    })
                },
                |document| eska_bsl_adapter::external_object(&document.descriptor, kind),
            );
            match value {
                Ok(value) => {
                    model.modeled_descriptors = 1;
                    ModelState::Ready(Arc::new(CanonicalMetadata::Object(Arc::new(value))))
                }
                Err(error) => ModelState::Invalid {
                    source: snapshot.path.clone(),
                    error,
                },
            }
        }
    };
    model
}

/// Normalized core kinds remain useful for YAML/IDE identity; they never select the platform type alone.
pub(super) const fn normalized(kind: ExternalObjectKind) -> MetadataKind {
    match kind {
        ExternalObjectKind::DataProcessor => MetadataKind::DataProcessor,
        ExternalObjectKind::Report => MetadataKind::Report,
    }
}
