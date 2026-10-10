//! Metadata checks report current parsed failures without retaining YAML or inventing source coordinates.

#[cfg(test)]
mod tests;

use std::ops::Range;

use eska::{
    analysis_core::{InputFailure, InputOrigin, InputState},
    project_core::{
        MetadataKind,
        metadata_parser::ParseIssue,
        source_format::{CoordinateError, MetadataSnapshot, SourcePath, TextRange},
    },
};
use eska_bsl_adapter::AdapterIssue;

use crate::{AnalysisSession, ModelState, QueryCancellation, SessionRequestError, SessionRevision};

/// Domain causes keep their original fields; consumers choose machine codes and localized messages.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum MetadataDiagnosticIssue {
    Parse(ParseIssue),
    Adapter(AdapterIssue),
    UnsupportedKind(MetadataKind),
}

/// One current finding belongs to a physical YAML span and a logical native descriptor.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MetadataDiagnostic {
    pub native_path: SourcePath,
    pub byte_range: Range<usize>,
    pub range: TextRange,
    pub issue: MetadataDiagnosticIssue,
}

/// A successful document check can still have unsupported BSL models or unmodeled nested descriptors.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MetadataDiagnostics {
    pub revision: SessionRevision,
    pub source: SourcePath,
    pub source_sha256: String,
    pub origin: InputOrigin,
    pub findings: Vec<MetadataDiagnostic>,
    pub input_complete: bool,
    pub main_model_complete: bool,
    pub descriptor_models_complete: bool,
}

/// Failed or cancelled current input never becomes a clean report or a guessed file-level location.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum MetadataDiagnosticsError {
    Request(SessionRequestError),
    Pending,
    Unavailable(InputFailure),
    Cancelled,
    InconsistentCoordinates(CoordinateError),
}

impl AnalysisSession {
    /// Inspect only the current ready YAML, including every nested descriptor's recoverable issues.
    ///
    /// # Errors
    /// Returns stale/unknown, pending/unavailable input, cancellation or an uncaptured byte boundary.
    pub fn metadata_diagnostics(
        &self,
        path: &SourcePath,
        expected: SessionRevision,
        cancellation: &QueryCancellation,
    ) -> Result<MetadataDiagnostics, MetadataDiagnosticsError> {
        let view = self
            .metadata(path, expected)
            .map_err(MetadataDiagnosticsError::Request)?;
        check_cancelled(cancellation)?;
        let snapshot = match &view.input {
            InputState::Ready(snapshot) => snapshot,
            InputState::Pending => return Err(MetadataDiagnosticsError::Pending),
            InputState::Unavailable(failure) => {
                return Err(MetadataDiagnosticsError::Unavailable(failure.clone()));
            }
        };
        let mut findings = Vec::new();
        for document in &snapshot.documents {
            for diagnostic in &document.descriptor.diagnostics {
                check_cancelled(cancellation)?;
                findings.push(finding(
                    snapshot,
                    &document.path,
                    diagnostic.range.clone(),
                    MetadataDiagnosticIssue::Parse(diagnostic.issue.clone()),
                )?);
            }
        }
        if let Some(main) = snapshot.documents.first() {
            match &view.model.state {
                ModelState::Invalid { error, .. }
                    if error.issue != AdapterIssue::IncompleteDescriptor
                        || main.descriptor.diagnostics.is_empty() =>
                {
                    findings.push(finding(
                        snapshot,
                        &main.path,
                        error.range.clone(),
                        MetadataDiagnosticIssue::Adapter(error.issue.clone()),
                    )?);
                }
                ModelState::Unsupported(kind) => {
                    if let Some(root) = main.descriptor.objects.first() {
                        findings.push(finding(
                            snapshot,
                            &main.path,
                            root.range.clone(),
                            MetadataDiagnosticIssue::UnsupportedKind(*kind),
                        )?);
                    }
                }
                _ => {}
            }
        }
        check_cancelled(cancellation)?;
        Ok(MetadataDiagnostics {
            revision: expected,
            source: path.clone(),
            source_sha256: snapshot.source_sha256.clone(),
            origin: view.origin,
            findings,
            input_complete: view.model.parse_issues == 0,
            main_model_complete: matches!(
                view.model.state,
                ModelState::Ready(_) | ModelState::StructureOnly
            ),
            descriptor_models_complete: view.model.descriptors_complete(),
        })
    }
}

/// Resolve captured endpoints rather than rereading a changed disk file or rounding Unicode boundaries.
fn finding(
    snapshot: &MetadataSnapshot,
    native_path: &SourcePath,
    byte_range: Range<usize>,
    issue: MetadataDiagnosticIssue,
) -> Result<MetadataDiagnostic, MetadataDiagnosticsError> {
    let range = snapshot
        .coordinates
        .range(byte_range.clone())
        .map_err(MetadataDiagnosticsError::InconsistentCoordinates)?;
    Ok(MetadataDiagnostic {
        native_path: native_path.clone(),
        byte_range,
        range,
        issue,
    })
}

/// Short metadata walks share the request token without registering an unrelated kernel query.
fn check_cancelled(cancellation: &QueryCancellation) -> Result<(), MetadataDiagnosticsError> {
    if cancellation.is_cancelled() {
        Err(MetadataDiagnosticsError::Cancelled)
    } else {
        Ok(())
    }
}
