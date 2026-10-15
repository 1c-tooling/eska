//! Machine reports map existing current ranges once, without retaining a second source buffer.

use eska::{
    analysis_core::{InputFailure, InputRequest, InputState},
    project_core::source_format::{QueryAuthoringRange, SourcePath, TextCoordinates},
};

use super::{
    AnalysisSession, QueryCancellation, QueryConsumerContext, QueryConsumerLocation,
    QueryConsumerRequestError, SessionRevision, check_cancelled, mismatch,
};

mod model;
pub use model::{
    QueryConsumerAuthoringLocation, QueryConsumerCoverageReport, QueryConsumerDiagnosticReport,
    QueryConsumerIncompleteReport, QueryConsumerLocationReport, QueryConsumerReport,
    QueryConsumerReportContext, QueryConsumerReportStatus,
};

impl AnalysisSession {
    /// Both transports prepare the same current inputs and classify only recoverable storage failures.
    pub(crate) fn check_query_consumer_report(
        &mut self,
        path: &SourcePath,
        request: &InputRequest,
        cancellation: &QueryCancellation,
    ) -> Result<QueryConsumerReportStatus, QueryConsumerRequestError> {
        let result = self
            .prepare_query_consumers(path, self.revision(), request)
            .and_then(|()| {
                self.query_consumer_report(path, self.revision(), request, cancellation)
            });
        match result {
            Ok(report) => Ok(QueryConsumerReportStatus::Checked {
                report: Box::new(report),
            }),
            Err(error) => unavailable(&error).ok_or(error),
        }
    }

    /// Project proven bindings to source-stamped, locale-independent byte and UTF-16 locations.
    ///
    /// # Errors
    /// Refuses stale or unavailable inputs, cancellation and uncaptured source boundaries.
    pub fn query_consumer_report(
        &self,
        path: &SourcePath,
        expected: SessionRevision,
        request: &InputRequest,
        cancellation: &QueryCancellation,
    ) -> Result<QueryConsumerReport, QueryConsumerRequestError> {
        let result = self.query_consumer_diagnostics(path, expected, request, cancellation)?;
        let locations = result
            .findings
            .iter()
            .map(|finding| &finding.location)
            .chain(
                result
                    .coverage
                    .incomplete
                    .iter()
                    .filter_map(|part| part.location.as_ref()),
            );
        let offsets = locations
            .filter_map(|location| authoring_range(&location.authoring))
            .flat_map(|range| [range.start, range.end])
            .collect::<Vec<_>>();
        let coordinates = self.consumer_coordinates(&result.context, &offsets)?;
        let coordinates = match &coordinates {
            Coordinates::Owner(value) => *value,
            Coordinates::Resource(value) => value,
        };
        let findings = result
            .findings
            .into_iter()
            .map(|finding| {
                check_cancelled(request, cancellation)?;
                Ok(QueryConsumerDiagnosticReport {
                    code: "query_consumer_output_field_missing",
                    target: finding.target,
                    output_field: finding.output_field,
                    location: location(finding.location, coordinates)?,
                })
            })
            .collect::<Result<_, QueryConsumerRequestError>>()?;
        let incomplete = result
            .coverage
            .incomplete
            .into_iter()
            .map(|part| {
                check_cancelled(request, cancellation)?;
                Ok(QueryConsumerIncompleteReport {
                    reason: part.reason,
                    location: part
                        .location
                        .map(|value| location(value, coordinates))
                        .transpose()?,
                })
            })
            .collect::<Result<_, QueryConsumerRequestError>>()?;
        check_cancelled(request, cancellation)?;
        Ok(QueryConsumerReport {
            context: result.context.into(),
            findings,
            coverage: QueryConsumerCoverageReport {
                output_names_complete: result.coverage.output_names_complete,
                output_types_complete: result.coverage.output_types_complete,
                checked_bindings: result.coverage.checked_bindings,
                incomplete,
            },
        })
    }

    /// Forms borrow captured endpoints; distinct XML resources scan only their one ready current text.
    fn consumer_coordinates<'a>(
        &'a self,
        context: &QueryConsumerContext,
        offsets: &[usize],
    ) -> Result<Coordinates<'a>, QueryConsumerRequestError> {
        if context.resource.source == context.owner.source {
            let input = self
                .selected_root
                .inputs
                .owner(&context.owner.source)
                .ok_or_else(mismatch)?;
            let InputState::Ready(snapshot) = &input.state else {
                return Err(mismatch());
            };
            if snapshot.source_sha256 != context.resource.source_sha256
                || input.origin != context.resource.origin
            {
                return Err(mismatch());
            }
            Ok(Coordinates::Owner(&snapshot.coordinates))
        } else {
            let input = self
                .metadata_resource(&context.resource.source)
                .ok_or_else(mismatch)?;
            let crate::MetadataResourceState::Ready(text) = &input.state else {
                return Err(mismatch());
            };
            if input.source_sha256.as_ref() != Some(&context.resource.source_sha256)
                || input.origin != context.resource.origin
            {
                return Err(mismatch());
            }
            TextCoordinates::new(text, offsets)
                .map(Coordinates::Resource)
                .map_err(|_| mismatch())
        }
    }
}

/// Input validity is distinct from a native invariant or a cancellation of the active request.
fn unavailable(error: &QueryConsumerRequestError) -> Option<QueryConsumerReportStatus> {
    use crate::QueryContextRequestError;
    if let QueryConsumerRequestError::Request(
        crate::SessionRequestError::CompositionInputIncomplete(key),
    ) = error
    {
        return Some(QueryConsumerReportStatus::Unavailable {
            code: "analysis_composition_incomplete".into(),
            location: vec![key.root.index().to_string(), key.source.as_str().into()],
        });
    }
    let QueryConsumerRequestError::Context(error) = error else {
        return None;
    };
    let (code, location) = match error {
        QueryContextRequestError::Pending(key) => (
            "query_consumer_context_pending",
            vec![key.root.index().to_string(), key.source.as_str().into()],
        ),
        QueryContextRequestError::Source(error) => (error.code, error.location.clone()),
        QueryContextRequestError::Unavailable { source, failure } => {
            let mut location = vec![
                source.root.index().to_string(),
                source.source.as_str().into(),
            ];
            let code = match failure {
                InputFailure::Source(error) => {
                    location.extend(error.location.iter().cloned());
                    error.code
                }
                InputFailure::Cancelled => "analysis_cancelled",
                InputFailure::DocumentTooLarge { .. } => "source_document_too_large",
                InputFailure::SourceBudgetExceeded { .. } => "analysis_source_budget_exceeded",
            };
            (code, location)
        }
        QueryContextRequestError::Request(_) | QueryContextRequestError::Cancelled => return None,
    };
    Some(QueryConsumerReportStatus::Unavailable {
        code: code.into(),
        location,
    })
}

/// Source text is owned only by the input controller, never by this report adapter.
enum Coordinates<'a> {
    Owner(&'a TextCoordinates),
    Resource(TextCoordinates),
}

/// Precision is retained verbatim; an unknown authoring location never acquires a guessed range.
const fn authoring_range(value: &QueryAuthoringRange) -> Option<&std::ops::Range<usize>> {
    match value {
        QueryAuthoringRange::Exact(range) | QueryAuthoringRange::EnclosingNode(range) => {
            Some(range)
        }
        QueryAuthoringRange::Unmapped => None,
    }
}

/// Byte and UTF-16 ranges refer to the identical current physical source and explicit precision.
fn location(
    value: QueryConsumerLocation,
    coordinates: &TextCoordinates,
) -> Result<QueryConsumerLocationReport, QueryConsumerRequestError> {
    let authoring = match value.authoring {
        QueryAuthoringRange::Exact(byte_range) => QueryConsumerAuthoringLocation::Exact {
            range: coordinates
                .range(byte_range.clone())
                .map_err(|_| mismatch())?,
            byte_range,
        },
        QueryAuthoringRange::EnclosingNode(byte_range) => {
            QueryConsumerAuthoringLocation::EnclosingNode {
                range: coordinates
                    .range(byte_range.clone())
                    .map_err(|_| mismatch())?,
                byte_range,
            }
        }
        QueryAuthoringRange::Unmapped => QueryConsumerAuthoringLocation::Unmapped,
    };
    Ok(QueryConsumerLocationReport {
        source: value.source,
        native_resource: value.native_resource,
        native_range: value.native_range,
        authoring,
    })
}
