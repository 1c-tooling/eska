//! Structured reports carry no native AST, source text or localized human prose.

use serde::ser::SerializeStruct as _;
use std::ops::Range;

use eska::analysis_core::InputRevision;
use eska::project_core::source_format::{SourcePath, TextRange};

use super::super::{QueryConsumerContext, QueryConsumerIncompleteReason, QueryConsumerTarget};
use crate::{QueryContextSource, SessionRevision, composition::RootId};

/// One generation qualifies every query, owner and resource origin/hash used by the report.
#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize)]
pub struct QueryConsumerReportContext {
    pub revision: SessionRevision,
    pub root: RootId,
    pub composition_sha256: Option<String>,
    pub query_revision: SessionRevision,
    pub owner_revision: InputRevision,
    pub resource_revision: Option<SessionRevision>,
    pub query: QueryContextSource,
    pub owner: QueryContextSource,
    pub resource: QueryContextSource,
}

impl From<QueryConsumerContext> for QueryConsumerReportContext {
    /// Discard parsed storage after copying its existing stamps; no new authority is introduced.
    fn from(value: QueryConsumerContext) -> Self {
        Self {
            revision: value.revision,
            root: value.root,
            composition_sha256: value.composition_sha256,
            query_revision: value.query_revision,
            owner_revision: value.owner_revision,
            resource_revision: value.resource_revision,
            query: value.query,
            owner: value.owner,
            resource: value.resource,
        }
    }
}

/// Exact XML and enclosing YAML spans are different contracts; unmapped spans have no coordinates.
#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize)]
#[serde(tag = "precision", rename_all = "snake_case")]
pub enum QueryConsumerAuthoringLocation {
    Exact {
        byte_range: Range<usize>,
        range: TextRange,
    },
    EnclosingNode {
        byte_range: Range<usize>,
        range: TextRange,
    },
    Unmapped,
}

/// The native span remains available without being mistaken for the physical authoring span.
#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize)]
pub struct QueryConsumerLocationReport {
    pub source: SourcePath,
    pub native_resource: SourcePath,
    pub native_range: Range<usize>,
    pub authoring: QueryConsumerAuthoringLocation,
}

/// A missing output field is proven only by the existing native/storage checks.
#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize)]
pub struct QueryConsumerDiagnosticReport {
    pub code: &'static str,
    pub target: QueryConsumerTarget,
    pub output_field: String,
    pub location: QueryConsumerLocationReport,
}

/// Incomplete checks retain their reason and any proven current source location.
#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize)]
pub struct QueryConsumerIncompleteReport {
    pub reason: QueryConsumerIncompleteReason,
    pub location: Option<QueryConsumerLocationReport>,
}

/// Partial field checks cannot grant full parameter, consumer or query semantic coverage.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct QueryConsumerCoverageReport {
    pub output_names_complete: bool,
    pub output_types_complete: bool,
    pub checked_bindings: usize,
    pub incomplete: Vec<QueryConsumerIncompleteReport>,
}

impl serde::Serialize for QueryConsumerCoverageReport {
    /// Unimplemented coverage is fixed by this contract and cannot be turned on by a caller.
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut value = serializer.serialize_struct("QueryConsumerCoverageReport", 7)?;
        value.serialize_field("output_names_complete", &self.output_names_complete)?;
        value.serialize_field("output_types_complete", &self.output_types_complete)?;
        value.serialize_field("checked_bindings", &self.checked_bindings)?;
        value.serialize_field("incomplete", &self.incomplete)?;
        value.serialize_field("parameters_complete", &false)?;
        value.serialize_field("consumer_complete", &false)?;
        value.serialize_field("semantic_complete", &false)?;
        value.end()
    }
}

/// IDE and checker serialize the identical machine data independently of their presentation locale.
#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize)]
pub struct QueryConsumerReport {
    pub context: QueryConsumerReportContext,
    pub findings: Vec<QueryConsumerDiagnosticReport>,
    pub coverage: QueryConsumerCoverageReport,
}

/// A failed current resource remains visibly unavailable without hiding independent query findings.
#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum QueryConsumerReportStatus {
    Checked { report: Box<QueryConsumerReport> },
    Unavailable { code: String, location: Vec<String> },
}
