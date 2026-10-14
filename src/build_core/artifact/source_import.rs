//! Native build staging delegates selection and guarded source publication to project core.

use super::{PreparedArtifact, UnpackError};
use crate::project_core::{
    Project,
    metadata_model::ProjectScope,
    source_format::{
        AcceptedRef, Error, ImportAncestor, LayoutPublication, NativeProjectSourceImport,
        ProjectSourceImportPlan, prepare_project_native_source_import,
    },
};

/// Platform/staging failures and project decisions retain their original structured categories.
#[derive(Debug)]
pub enum ArtifactSourceImportError {
    Artifact(UnpackError),
    Project(Error),
}

/// Borrowing the artifact keeps its verified native input alive until preview/application ends.
pub struct PreparedArtifactSourceImport<'a> {
    artifact: &'a PreparedArtifact,
    incoming: NativeProjectSourceImport,
}

impl PreparedArtifact {
    /// Convert verified native staging into the selected project's existing source layout and plan.
    ///
    /// The common ancestor and support pin are independently supplied. Preparation never changes
    /// the project, selects conflict bytes, approves supplier rules or changes the accepted map.
    ///
    /// # Errors
    /// Returns altered staging, wrong scope/identity/type, unsafe projection or project failures.
    pub fn prepare_source_import(
        &self,
        project: &Project,
        scope: &ProjectScope,
        expected: &AcceptedRef,
        ancestor: &ImportAncestor,
        cancelled: &dyn Fn() -> bool,
    ) -> Result<PreparedArtifactSourceImport<'_>, ArtifactSourceImportError> {
        self.verify_cancellable(cancelled)
            .map_err(ArtifactSourceImportError::Artifact)?;
        let staging = self
            .sources()
            .parent()
            .ok_or_else(|| {
                ArtifactSourceImportError::Artifact(UnpackError::UnsafePath(self.sources()))
            })?
            .join("source-import");
        let incoming = prepare_project_native_source_import(
            project,
            scope,
            expected,
            ancestor,
            &self.sources(),
            &staging,
            cancelled,
        )
        .map_err(ArtifactSourceImportError::Project)?;
        self.verify_cancellable(cancelled)
            .map_err(ArtifactSourceImportError::Artifact)?;
        Ok(PreparedArtifactSourceImport {
            artifact: self,
            incoming,
        })
    }
}

impl PreparedArtifactSourceImport<'_> {
    /// Project core owns preview, conflict selection, incoming ancestry and recovery identity.
    #[must_use]
    pub const fn plan(&self) -> &ProjectSourceImportPlan {
        self.incoming.plan()
    }

    /// Recheck native proof and delegate mutation to the same guarded project publication.
    ///
    /// # Errors
    /// Returns changed native staging, conflicts, stale project inputs, cancellation or I/O.
    pub fn publish(
        &self,
        cancelled: &dyn Fn() -> bool,
    ) -> Result<LayoutPublication, ArtifactSourceImportError> {
        self.artifact
            .verify_cancellable(cancelled)
            .map_err(ArtifactSourceImportError::Artifact)?;
        self.plan()
            .publish_cancellable(cancelled)
            .map_err(ArtifactSourceImportError::Project)
    }
}
