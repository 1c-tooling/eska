//! Current storage consumers borrow owner authority rather than a saved YAML or XML reconstruction.

use eska::{
    analysis_core::{InputFailure, InputOrigin, InputRequest, InputRevision, InputState},
    project_core::source_format::{Error, MetadataSnapshot, QueryConsumerView, SourcePath},
};

use crate::{
    AnalysisSession, MetadataResourceState, QueryTextState, SessionRequestError, SessionRevision,
    composition::{RootId, SourceKey},
};

#[cfg(test)]
mod tests;

/// One current source identity never claims that its bytes have complete consumer semantics.
#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize)]
pub struct QueryContextSource {
    pub source: SourcePath,
    pub source_sha256: String,
    pub origin: InputOrigin,
}

/// The global generation binds the exact query, owner and storage resource used by this read view.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct QueryConsumerContext {
    pub revision: SessionRevision,
    pub root: RootId,
    pub composition_sha256: Option<String>,
    pub query_revision: SessionRevision,
    /// This existing metadata-controller token is qualified by `root` and the global generation.
    pub owner_revision: InputRevision,
    /// None means the resource is the same owner document and shares `owner_revision`.
    pub resource_revision: Option<SessionRevision>,
    pub query: QueryContextSource,
    pub owner: QueryContextSource,
    pub resource: QueryContextSource,
    pub consumer: QueryConsumerView,
}

/// Current failures refuse the read view; they never reveal a previous source or consumer value.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum QueryContextRequestError {
    Request(SessionRequestError),
    Pending(SourceKey),
    Unavailable {
        source: SourceKey,
        failure: InputFailure,
    },
    Source(Error),
    Cancelled,
}

impl AnalysisSession {
    /// Prepare exactly one query, its current owner and its distinct composition resource.
    ///
    /// No neighboring query, BSL body, foreign metadata or saved replacement of an overlay is read.
    /// # Errors
    /// Rejects stale/unknown input or cancellation; accepted input failures remain current for inspection.
    pub fn prepare_query_context(
        &mut self,
        path: &SourcePath,
        expected: SessionRevision,
        request: &InputRequest,
    ) -> Result<(), QueryContextRequestError> {
        self.validate_revision(expected)
            .map_err(QueryContextRequestError::Request)?;
        check_cancelled(request)?;
        let declaration = self.query_declaration(path).cloned().ok_or_else(|| {
            QueryContextRequestError::Request(SessionRequestError::UnknownQuery(path.clone()))
        })?;
        if self.query_input(path).is_some_and(|input| {
            input.origin == InputOrigin::Saved
                && matches!(
                    input.state,
                    QueryTextState::Pending | QueryTextState::Unavailable(InputFailure::Cancelled)
                )
        }) {
            self.load_query_saved(path, self.revision(), request)
                .map_err(QueryContextRequestError::Request)?;
        }
        check_cancelled(request)?;
        if self
            .selected_root
            .inputs
            .owner(&declaration.owner)
            .is_some_and(|input| {
                input.origin == InputOrigin::Saved
                    && matches!(
                        input.state,
                        InputState::Pending | InputState::Unavailable(InputFailure::Cancelled)
                    )
            })
        {
            self.load_saved(&declaration.owner, self.revision(), request)
                .map_err(QueryContextRequestError::Request)?;
        }
        check_cancelled(request)?;
        let resource = resource_source(self, &declaration.native_resource)?;
        if resource != declaration.owner {
            let input = self.metadata_resource(&resource).ok_or_else(|| {
                QueryContextRequestError::Request(SessionRequestError::UnknownMetadataResource(
                    resource.clone(),
                ))
            })?;
            if input.origin == InputOrigin::Saved
                && matches!(
                    input.state,
                    MetadataResourceState::Pending
                        | MetadataResourceState::Unavailable(InputFailure::Cancelled)
                )
            {
                self.load_resource_saved(&resource, self.revision(), request)
                    .map_err(QueryContextRequestError::Request)?;
            }
        }
        check_cancelled(request)
    }

    /// Read actual current storage bindings without granting query/output/parameter semantic coverage.
    ///
    /// Unified forms use their captured owner view; separate DCS input uses the same resource controller.
    /// # Errors
    /// Refuses stale, pending, failed or mismatched current input, invalid references or cancellation.
    pub fn query_context(
        &self,
        path: &SourcePath,
        expected: SessionRevision,
        request: &InputRequest,
    ) -> Result<QueryConsumerContext, QueryContextRequestError> {
        self.validate_revision(expected)
            .map_err(QueryContextRequestError::Request)?;
        check_cancelled(request)?;
        let query = self.selected_root.query_inputs.get(path).ok_or_else(|| {
            QueryContextRequestError::Request(SessionRequestError::UnknownQuery(path.clone()))
        })?;
        let declaration = query.declaration();
        let query_input = query.snapshot();
        match &query_input.state {
            QueryTextState::Pending => return Err(pending(self, path)),
            QueryTextState::Unavailable(failure) => return Err(unavailable(self, path, failure)),
            QueryTextState::Ready(text) => check_size(self, path, text.len() as u64, request)?,
        }
        let owner = self
            .selected_root
            .inputs
            .owner(&declaration.owner)
            .ok_or_else(mismatch)?;
        let snapshot = match &owner.state {
            InputState::Pending => return Err(pending(self, &declaration.owner)),
            InputState::Unavailable(failure) => {
                return Err(unavailable(self, &declaration.owner, failure));
            }
            InputState::Ready(snapshot) => snapshot,
        };
        check_size(self, &declaration.owner, snapshot.source_bytes, request)?;
        if owner.source_sha256.as_deref() != Some(snapshot.source_sha256.as_str()) {
            return Err(mismatch());
        }
        let resource_path = resource_source(self, &declaration.native_resource)?;
        let owner_source = source_stamp(
            &declaration.owner,
            owner.source_sha256.as_deref(),
            owner.origin,
        )?;
        let (consumer, resource, resource_revision) = if resource_path == declaration.owner {
            let context = snapshot
                .form
                .as_ref()
                .ok_or_else(mismatch)?
                .query_context()
                .map_err(|error| QueryContextRequestError::Source(error.clone()))?
                .ok_or_else(mismatch)?;
            (
                context
                    .consumer_view()
                    .map_err(QueryContextRequestError::Source)?,
                owner_source.clone(),
                None,
            )
        } else {
            composition_context(
                self,
                &declaration.native_resource,
                snapshot,
                &resource_path,
                request,
            )?
        };
        check_cancelled(request)?;
        Ok(QueryConsumerContext {
            revision: expected,
            root: self.selected_root_id(),
            composition_sha256: self.composition_sha256().map(str::to_owned),
            query_revision: query_input.revision,
            owner_revision: owner.revision,
            resource_revision,
            query: source_stamp(
                path,
                query_input.source_sha256.as_deref(),
                query_input.origin,
            )?,
            owner: owner_source,
            resource,
            consumer,
        })
    }
}

/// A distinct current XML input is parsed only against its exact current owner snapshot.
fn composition_context(
    session: &AnalysisSession,
    native: &SourcePath,
    owner: &MetadataSnapshot,
    resource: &SourcePath,
    request: &InputRequest,
) -> Result<
    (
        QueryConsumerView,
        QueryContextSource,
        Option<SessionRevision>,
    ),
    QueryContextRequestError,
> {
    let input = session.metadata_resource(resource).ok_or_else(mismatch)?;
    if input.kind != crate::MetadataResourceKind::QueryComposition {
        return Err(mismatch());
    }
    let text = match &input.state {
        MetadataResourceState::Pending => return Err(pending(session, resource)),
        MetadataResourceState::Unavailable(failure) => {
            return Err(unavailable(session, resource, failure));
        }
        MetadataResourceState::Ready(text) => text,
    };
    check_size(session, resource, text.len() as u64, request)?;
    let context = session
        .selected_root
        .inputs
        .source_reader()
        .parse_query_context_from_snapshot(native, owner, text, request.source_limit())
        .map_err(QueryContextRequestError::Source)?;
    Ok((
        context
            .consumer_view()
            .map_err(QueryContextRequestError::Source)?,
        source_stamp(resource, input.source_sha256.as_deref(), input.origin)?,
        Some(input.revision),
    ))
}

/// Manifest resource identity, rather than a query filename, selects the current authoring path.
fn resource_source(
    session: &AnalysisSession,
    native: &SourcePath,
) -> Result<SourcePath, QueryContextRequestError> {
    session
        .selected_root
        .inputs
        .source_reader()
        .manifest()
        .files
        .iter()
        .find(|file| &file.path == native)
        .map(|file| file.source_path().clone())
        .ok_or_else(mismatch)
}

/// Exact source fingerprints are required even when a clean result has no storage findings.
fn source_stamp(
    path: &SourcePath,
    hash: Option<&str>,
    origin: InputOrigin,
) -> Result<QueryContextSource, QueryContextRequestError> {
    Ok(QueryContextSource {
        source: path.clone(),
        source_sha256: hash.ok_or_else(mismatch)?.into(),
        origin,
    })
}

/// One root-qualified failure cannot be confused with an equal path in a different source root.
fn unavailable(
    session: &AnalysisSession,
    path: &SourcePath,
    failure: &InputFailure,
) -> QueryContextRequestError {
    QueryContextRequestError::Unavailable {
        source: session.selected_source(path),
        failure: failure.clone(),
    }
}

/// Unread input remains distinguishable from readable malformed storage.
fn pending(session: &AnalysisSession, path: &SourcePath) -> QueryContextRequestError {
    QueryContextRequestError::Pending(session.selected_source(path))
}

/// Declaration inconsistencies never select an unrelated owner or a saved resource fallback.
const fn mismatch() -> QueryContextRequestError {
    QueryContextRequestError::Request(SessionRequestError::QueryReaderMismatch)
}

/// Passive views honor the caller's current per-document bound without reserving derived XML again.
fn check_size(
    session: &AnalysisSession,
    path: &SourcePath,
    bytes: u64,
    request: &InputRequest,
) -> Result<(), QueryContextRequestError> {
    if bytes > request.source_limit() {
        Err(unavailable(session, path, &request.limit_failure()))
    } else {
        Ok(())
    }
}

/// Pure parsing can only be interrupted at its publication boundary, matching existing storage views.
fn check_cancelled(request: &InputRequest) -> Result<(), QueryContextRequestError> {
    if request.cancellation.is_cancelled() {
        Err(QueryContextRequestError::Cancelled)
    } else {
        Ok(())
    }
}
