//! Creation candidates are checked for integrity without granting policy approval or publishing XML.

use crate::project_core::{
    Project, ProjectType,
    eska_source::EskaSource,
    metadata_model::ProjectScope,
    source_format::{
        AcceptedBase, NativeUuid, SourceImportPlan, open_initial_project_import_ancestor,
        prepare_source_import,
    },
};

use super::protocol::{self, Created, Error};

/// A pure source plan also retains the exact source/base/ancestor input fence for finalization.
pub(super) struct Verified {
    pub(super) project: Project,
    proof: SourceImportPlan,
}

impl Verified {
    /// Recheck captured source/authority bytes immediately before parent-owned publication.
    pub(super) fn recheck(&self, created: &Created) -> Result<(), Error> {
        self.proof.verify().map_err(|_| Error::SourceChanged)?;
        open_initial_project_import_ancestor(&self.project, &created.ancestor)
            .map_err(|_| Error::SourceChanged)?;
        Ok(())
    }
}

/// Root metadata identity is independent of physical artifact stems and temporary project paths.
pub(super) fn verify(
    created: &Created,
    expected_type: ProjectType,
    expected_metadata_name: &str,
    expected_uuid: Option<&str>,
) -> Result<Verified, Error> {
    let (_, config) = protocol::manifest(created)?;
    if config.configuration().project_type() != expected_type {
        return Err(Error::SourceChanged);
    }
    let project = config
        .into_project(created.root.clone())
        .map_err(|_| Error::SourceChanged)?;
    let source = EskaSource::open(project.clone(), ProjectScope::Standalone)
        .map_err(|_| Error::SourceChanged)?;
    let root = source
        .reader()
        .configuration()
        .declarations
        .first()
        .ok_or(Error::SourceChanged)?;
    if root.name != expected_metadata_name {
        return Err(Error::SourceChanged);
    }
    if let Some(expected_uuid) = expected_uuid {
        let expected = NativeUuid::parse(expected_uuid).map_err(|_| Error::SourceChanged)?;
        if root.uuid.key() != expected.key() {
            return Err(Error::SourceChanged);
        }
    }
    let candidate = AcceptedBase::open(&created.root.join(".eska/accepted"), &created.accepted)
        .map_err(|_| Error::SourceChanged)?;
    candidate
        .validate_source_root(source.reader())
        .map_err(|_| Error::SourceChanged)?;
    let ancestor = open_initial_project_import_ancestor(&project, &created.ancestor)
        .map_err(|_| Error::SourceChanged)?;
    let proof = prepare_source_import(project.source(), &ancestor, &ancestor, &candidate)
        .map_err(|_| Error::SourceChanged)?;
    if proof.working_revision() != ancestor.revision() {
        return Err(Error::SourceChanged);
    }
    proof.verify().map_err(|_| Error::SourceChanged)?;
    Ok(Verified { project, proof })
}
