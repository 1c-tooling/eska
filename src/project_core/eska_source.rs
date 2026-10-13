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
