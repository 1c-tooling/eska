//! Manifest-selected YAML reading and protected materialization without Git or platform access.

use std::path::Path;

use super::source_format::{
    AcceptedBase, AcceptedRef, Error, PreparedSource, ProtectedFailure, SourceReader,
    prepare_protected,
};
use super::{Project, SourceFormat, metadata_model::ProjectScope};

/// A selected project's exact source format; derivative XML never participates in reading.
pub struct EskaSource {
    project: Project,
    scope: ProjectScope,
    reader: SourceReader,
}

impl EskaSource {
    /// Read the selected manifest and configuration without opening the accepted base or payloads.
    ///
    /// # Errors
    /// Returns an explicit format/profile mismatch or the YAML reader's structured failures.
    pub fn open(project: Project, scope: ProjectScope) -> Result<Self, Error> {
        let SourceFormat::Eska { accepted_base } = project.configuration().source_format() else {
            return Err(Error::root("source_format_mismatch"));
        };
        super::source_format::require_project_ready(project.root())?;
        let base = project.root().join(accepted_base.as_path());
        if base.starts_with(project.source()) || project.source().starts_with(&base) {
            return Err(Error::root("source_baseline_overlap"));
        }
        let reader = SourceReader::open(project.source())?;
        if reader.project_type()? != project.configuration().project_type() {
            return Err(Error::root("source_profile_unsupported"));
        }
        super::source_format::require_project_ready(project.root())?;
        Ok(Self {
            project,
            scope,
            reader,
        })
    }

    /// Return the discovered project, including its member-specific accepted-base path.
    #[must_use]
    pub const fn project(&self) -> &Project {
        &self.project
    }

    /// Retain workspace scope without encoding it into native UUIDs or source paths.
    #[must_use]
    pub const fn scope(&self) -> &ProjectScope {
        &self.scope
    }

    /// Read owner-local disk or overlay snapshots using the single shared YAML implementation.
    #[must_use]
    pub const fn reader(&self) -> &SourceReader {
        &self.reader
    }

    /// Transfer the validated composition to an analysis host without a second filesystem open.
    #[must_use]
    pub fn into_reader(self) -> SourceReader {
        self.reader
    }

    /// Open current editor bindings under an independently supplied support pin without advancing it.
    ///
    /// # Errors
    /// Returns unresolved import, unsafe or changed composition, invalid pin or baseline failures.
    pub fn edit_authority(&self, expected: &AcceptedRef) -> Result<AcceptedBase, Error> {
        self.edit_authority_cancellable(expected, &|| false)
    }

    /// Check the same project guard, saved composition and pin with caller-owned cooperative cancellation.
    ///
    /// # Errors
    /// Returns the same authority failures or `source_read_cancelled` when stopped.
    pub fn edit_authority_cancellable(
        &self,
        expected: &AcceptedRef,
        cancelled: &dyn Fn() -> bool,
    ) -> Result<AcceptedBase, Error> {
        super::source_format::require_project_ready(self.project.root())?;
        let SourceFormat::Eska { accepted_base } = self.project.configuration().source_format()
        else {
            return Err(Error::root("source_format_mismatch"));
        };
        let authority = AcceptedBase::open_cancellable(
            &self.project.root().join(accepted_base.as_path()),
            expected,
            cancelled,
        )?
        .bind_source_cancellable(self.project.source(), cancelled)?;
        super::source_format::require_project_ready(self.project.root())?;
        Ok(authority.with_project_guard(self.project.root()))
    }

    /// Materialize saved inputs using a baseline digest supplied independently by the caller.
    ///
    /// The configured path locates bytes; it does not approve their policy. Every invocation
    /// verifies the pinned base and current complete source composition before publishing XML.
    ///
    /// # Errors
    /// Returns untrusted/missing baseline or protected-export failures without XML fallback.
    pub fn prepare(
        &self,
        expected: &AcceptedRef,
        generations: &Path,
        strict: bool,
    ) -> Result<PreparedSource, ProtectedFailure> {
        super::source_format::require_project_ready(self.project.root())?;
        let SourceFormat::Eska { accepted_base } = self.project.configuration().source_format()
        else {
            return Err(Error::root("source_format_mismatch").into());
        };
        let base =
            AcceptedBase::open(&self.project.root().join(accepted_base.as_path()), expected)?;
        prepare_protected(self.project.source(), &base, generations, strict)
    }
}

#[cfg(test)]
mod tests;
