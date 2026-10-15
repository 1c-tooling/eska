//! Typed consumer outcomes retain current locations and deliberately partial coverage.

use std::ops::Range;

use eska::project_core::source_format::{FormId, QueryAuthoringRange, SourcePath};

use crate::QueryConsumerContext;

/// An XML node span and a YAML enclosing node are distinct coordinate contracts.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct QueryConsumerLocation {
    pub source: SourcePath,
    pub native_resource: SourcePath,
    pub native_range: Range<usize>,
    pub authoring: QueryAuthoringRange,
}

/// Dataset ancestry and form numeric scopes disambiguate equal consumer names.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum QueryConsumerTarget {
    Dataset {
        path: Vec<String>,
        data_path: String,
    },
    DynamicList {
        attribute: FormId,
        element: FormId,
    },
}

/// This finding proves only that an explicit direct binding lacks a named query output field.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct QueryConsumerFinding {
    pub target: QueryConsumerTarget,
    pub output_field: String,
    pub location: QueryConsumerLocation,
}

/// Absence cannot be inferred from an incomplete rowset or from an unimplemented storage rule.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum QueryConsumerIncompleteReason {
    InvalidQuery,
    InvalidQueryMetadata,
    UnsupportedNativeExpression,
    NoOutputRowset,
    MultipleOutputRowsets,
    TemporaryTablePackage,
    OutputNamesIncomplete,
    OutputTypesIncomplete,
    AmbiguousOutputNames,
    UnboundQueryReference,
    AmbiguousQueryReference,
    AmbiguousDynamicList,
    PlatformEnrichedDynamicList,
    UnsupportedFieldPath,
    UnimplementedResourceParts,
}

/// Current unknown bindings remain locatable when an exact native/authoring range is available.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct QueryConsumerIncomplete {
    pub reason: QueryConsumerIncompleteReason,
    pub location: Option<QueryConsumerLocation>,
}

/// Explicit binding checks never grant completeness for expressions, settings or parameter types.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct QueryConsumerCheckCoverage {
    pub output_names_complete: bool,
    pub output_types_complete: bool,
    pub checked_bindings: usize,
    pub incomplete: Vec<QueryConsumerIncomplete>,
}

impl QueryConsumerCheckCoverage {
    /// Parameter occurrences are exposed by the model; owner declarations/types are not yet checked.
    #[must_use]
    pub const fn parameters_complete(&self) -> bool {
        false
    }

    /// Other DCS/form rules remain outside this explicit field-binding slice.
    #[must_use]
    pub const fn consumer_complete(&self) -> bool {
        false
    }

    /// Complete query/storage semantics require more than an output-name absence proof.
    #[must_use]
    pub const fn semantic_complete(&self) -> bool {
        false
    }
}

/// Facts and findings belong to one root-qualified generation and exact query/owner/resource inputs.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct QueryConsumerDiagnostics {
    pub context: QueryConsumerContext,
    pub model: ide::QueryTextModel,
    pub findings: Vec<QueryConsumerFinding>,
    pub coverage: QueryConsumerCheckCoverage,
}
