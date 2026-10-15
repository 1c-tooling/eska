//! Consumer facts retain native identity, current authoring coordinates and incomplete coverage.

use std::ops::Range;

use crate::project_core::metadata_model::MetadataProperty;
use crate::project_core::source_format::{
    FormElementKind, FormId, NativeUuid, QueryAuthoringRange, QueryProfile, QueryReference,
    SourcePath,
};

/// Decoded values retain their enclosing native node; decoded subranges are not implied.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct QueryLocated<T> {
    pub value: T,
    pub native_range: Range<usize>,
    pub authoring: QueryAuthoringRange,
}

/// Native dataset purpose is independent of its query language or output types.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum QueryDatasetKind {
    Query,
    Union,
    Object,
}

/// An explicit native field binding does not prove that a query supplies the field.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct QueryDatasetField {
    pub data_path: QueryLocated<String>,
    pub field: QueryLocated<String>,
}

/// Full union ancestry identifies a dataset; equal names in separate scopes remain separate.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct QueryDataset {
    pub path: Vec<QueryLocated<String>>,
    pub kind: QueryDatasetKind,
    pub query: Option<QueryReference>,
    pub fields: Vec<QueryDatasetField>,
}

/// Explicit schema parameters retain source types and expressions without claiming runtime completeness.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct QueryParameterDeclaration {
    pub scope: QueryConsumerScope,
    pub name: QueryLocated<String>,
    pub value_type: Option<QueryLocated<MetadataProperty>>,
    pub expression: Option<QueryLocated<String>>,
    pub value_list_allowed: Option<QueryLocated<bool>>,
}

/// Attribute identity belongs to the root Attributes collection rather than the UI element map.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct QueryDynamicList {
    pub attribute: QueryLocated<FormId>,
    pub name: QueryLocated<String>,
    pub manual_query: QueryLocated<bool>,
    pub query: Option<QueryReference>,
    pub main_table: Option<QueryLocated<String>>,
}

/// A direct UI `DataPath` is captured without inferring an attribute or metadata target from its text.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct QueryFormDataPath {
    pub element: QueryLocated<FormId>,
    pub element_kind: FormElementKind,
    pub element_name: QueryLocated<String>,
    pub data_path: QueryLocated<String>,
}

/// Expanded names identify native parts by their actual namespace rather than a familiar prefix.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct QueryXmlName {
    pub namespace: Option<String>,
    pub local: String,
}

/// Only actual native scopes identify unhandled parts; no expression dependency is inferred.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum QueryConsumerScope {
    Resource,
    Dataset { datasets: Vec<String> },
    FormAttribute { attribute: FormId },
    FormElement { element: FormId },
}

/// Known container names classify inventory only, without interpreting their values or expressions.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum QueryUnhandledKind {
    Parameter,
    DatasetLink,
    CalculatedField,
    TotalField,
    Settings,
    Other,
}

/// Missing semantics and unknown native shapes stay distinguishable from supported storage facts.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum QueryConsumerCoverage {
    Incomplete,
    Unsupported,
}

/// The whole current subtree remains locatable; inventory never means that the subtree was analyzed.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct QueryUnhandledPart {
    pub scope: QueryConsumerScope,
    pub kind: QueryUnhandledKind,
    pub coverage: QueryConsumerCoverage,
    pub node: QueryLocated<QueryXmlName>,
}

/// A pure storage read view grants neither edit authority nor query/consumer semantic completeness.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct QueryConsumerView {
    pub(super) owner: SourcePath,
    pub(super) descriptor: SourcePath,
    pub(super) source: SourcePath,
    pub(super) native: SourcePath,
    pub(super) uuid: NativeUuid,
    pub(super) profile: QueryProfile,
    pub(super) datasets: Vec<QueryDataset>,
    pub(super) parameters: Vec<QueryParameterDeclaration>,
    pub(super) dynamic_lists: Vec<QueryDynamicList>,
    pub(super) form_data_paths: Vec<QueryFormDataPath>,
    pub(super) unhandled: Vec<QueryUnhandledPart>,
}

impl QueryConsumerView {
    /// Return the physical owner verified before the supplied resource was parsed.
    #[must_use]
    pub const fn owner(&self) -> &SourcePath {
        &self.owner
    }

    /// Return the declared native descriptor without assuming a physical artifact stem.
    #[must_use]
    pub const fn native_descriptor(&self) -> &SourcePath {
        &self.descriptor
    }

    /// Every authoring range belongs to this exact supplied source resource.
    #[must_use]
    pub const fn source(&self) -> &SourcePath {
        &self.source
    }

    /// Return the validated native resource identity for downstream consumer admission.
    #[must_use]
    pub const fn native_resource(&self) -> &SourcePath {
        &self.native
    }

    /// Return the current owner UUID without approving ancestry or published history.
    #[must_use]
    pub const fn uuid(&self) -> &NativeUuid {
        &self.uuid
    }

    /// The resource root selects composition or managed-form storage rules.
    #[must_use]
    pub const fn profile(&self) -> QueryProfile {
        self.profile
    }

    /// Return explicit datasets in native order, including their full union scopes.
    #[must_use]
    pub fn datasets(&self) -> &[QueryDataset] {
        &self.datasets
    }

    /// Only explicit native declarations are listed; implicit virtual-table parameters are not invented.
    #[must_use]
    pub fn parameter_declarations(&self) -> &[QueryParameterDeclaration] {
        &self.parameters
    }

    /// Automatic lists retain their explicit settings but supply no saved query reference.
    #[must_use]
    pub fn dynamic_lists(&self) -> &[QueryDynamicList] {
        &self.dynamic_lists
    }

    /// Return only direct `DataPath` properties attached to validated UI element identities.
    #[must_use]
    pub fn form_data_paths(&self) -> &[QueryFormDataPath] {
        &self.form_data_paths
    }

    /// Unknown shapes and unimplemented semantics remain explicit even when there are no diagnostics.
    #[must_use]
    pub fn unhandled(&self) -> &[QueryUnhandledPart] {
        &self.unhandled
    }

    /// Storage facts never establish metadata, expression or output-field semantic completeness.
    #[must_use]
    pub const fn consumer_complete(&self) -> bool {
        false
    }
}
