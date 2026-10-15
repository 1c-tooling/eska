//! Experimental, reversible source documents, independent of IDE and platform runners.

mod attribute_profile;
#[cfg(test)]
mod attribute_test_support;
mod baseline;
mod child_creation;
mod child_deletion;
mod child_order;
mod child_rename;
mod composition;
mod coordinates;
mod document_paths;
mod edit;
#[cfg(test)]
mod enum_test_support;
mod export;
mod file_identity;
mod files;
mod fill_value;
mod fragment;
mod import;
mod initialization;
mod layout;
mod lexical;
mod managed_form;
mod metadata_reference;
mod model;
mod native_writer;
mod package;
mod project_migration;
mod project_update;
mod property;
mod protected;
mod queries;
mod readable;
mod reader;
mod root;
mod source_write;
mod structural_references;
mod synchronization;
mod tabular_identities;
mod tabular_profile;
#[cfg(test)]
mod tabular_test_support;
mod targets;
mod type_description;
mod validation;
mod xml;
mod yaml;

pub use baseline::{
    AcceptedBase, AcceptedLayoutPlan, AcceptedRef, BaseUpdateChange, BaseUpdateConflict,
    BaseUpdateConflictKind, BaseUpdatePlan, Operation, Permission, PermissionReason, SupportChange,
    accept_source, prepare_accepted_descriptor_layout, prepare_accepted_form_layout,
    prepare_accepted_query_layout, prepare_accepted_readable_layout,
    prepare_accepted_resource_layout, prepare_base_update,
};
pub use coordinates::{CoordinateError, TextCoordinates, TextPosition, TextRange};
pub use edit::{
    FormEdit, FormEditPreview, QueryEditPreview, SourceEditPlan, SourcePropertyEdit,
    SourceYamlEditPlan, prepare_form_edit, prepare_form_edit_text, prepare_module_edit,
    prepare_property_edit, prepare_property_edit_text, prepare_query_edit, prepare_query_edit_text,
    prepare_yaml_edit,
};
pub use export::export_designer;
pub use import::{
    import_designer, import_designer_forms, import_designer_queries, import_designer_readable,
    import_designer_resources, import_designer_separated,
};
pub use initialization::{ProjectInitializationPlan, prepare_project_initialization};
pub use layout::{
    DESCRIPTOR_LAYOUT_VERSION, DescriptorContext, LayoutChange, LayoutPublication,
    MANAGED_FORM_LAYOUT_VERSION, QUERY_LAYOUT_VERSION, QueryResource, RESOURCE_LAYOUT_VERSION,
    SourceLayoutPlan, prepare_descriptor_layout, prepare_form_layout, prepare_query_layout,
    prepare_readable_layout, prepare_resource_layout,
};
pub use managed_form::{
    FormDefinition, FormElement, FormElementKind, FormFixedSlot, FormId, FormLayout,
    FormLocalizedText, FormNamedValue, FormPropertyName, FormPropertyValue, ManagedFormLayout,
    ManagedFormSource,
};
pub use package::{FileEntry, FileRole, NativeDocument, ObjectSource, SourceManifest, SourcePath};
pub use protected::{
    COMPOSITION_SOURCE_MAP_VERSION, CREATED_SOURCE_MAP_VERSION, EXTERNAL_SOURCE_MAP_VERSION,
    Exclusion, ExclusionAction, FileOrigin, InputAuthority, InputFile,
    MANAGED_FORM_SOURCE_MAP_VERSION, MappedFile, OriginTexts, PreparedSource, ProtectedExport,
    ProtectedFailure, QUERY_SOURCE_MAP_VERSION, QueryOrigin, RELOCATED_SOURCE_MAP_VERSION,
    SOURCE_MAP_VERSION, SourceLocation, SourceMap, export_protected, prepare_protected,
};
pub use queries::{
    ExtractedQuerySource, NativeQuery, NativeQueryDocument, QUERY_REFERENCE_NAMESPACE,
    QueryConsumerCoverage, QueryConsumerScope, QueryConsumerView, QueryDataset, QueryDatasetField,
    QueryDatasetKind, QueryDynamicList, QueryFormDataPath, QueryLocated, QueryParameterDeclaration,
    QueryProfile, QueryReference, QueryReferenceDocument, QuerySlot, QueryUnhandledKind,
    QueryUnhandledPart, QueryXmlName,
};
pub use reader::{
    CurrentFormView, DeclaredForm, DeclaredModule, DeclaredOwner, DeclaredQuery,
    MetadataDeclaration, MetadataSnapshot, NativeMetadata, OwnerSnapshot, QueryAuthoringRange,
    QueryContextView, SourceReader,
};
pub use root::{
    EXTERNAL_LAYOUT_VERSION, ExternalKind, ExternalRoot, PROCESSING_PROFILE, REPORT_PROFILE,
};

pub use fill_value::{EnumFillValue, EnumFillValueLayout};
pub use metadata_reference::{MetadataReference, MetadataReferenceLayout, MetadataReferenceList};
pub use model::{
    Child, Descriptor, LineEnding, LocalizedText, MetadataNode, Namespace, NativeUuid, Property,
    PropertyName, PropertyValue, Section, XmlAttribute, XmlFragment, XmlProfile,
};
pub(crate) use project_migration::require_project_ready;

pub use project_migration::{
    PROJECT_LAYOUT_RECEIPT, ProjectLayoutPlan, WORKSPACE_LAYOUT_RECEIPT, WorkspaceAcceptedRefs,
    WorkspaceLayoutMember, WorkspaceLayoutPlan, prepare_form_project, prepare_form_workspace,
    prepare_query_project, prepare_query_workspace, prepare_readable_project,
    prepare_readable_workspace,
};
pub use project_update::{
    PROJECT_BASE_UPDATE_RECEIPT, ProjectBaseUpdatePlan, WORKSPACE_BASE_UPDATE_RECEIPT,
    WorkspaceBaseUpdateMember, WorkspaceBaseUpdatePlan, prepare_project_base_update,
    prepare_workspace_base_update,
};
pub use property::ExactUnsigned;
pub use tabular_identities::{GeneratedIdentity, TabularSectionIdentities};
pub use type_description::{
    DateQualifiers, NumberQualifiers, StringQualifiers, TypeAllowedLength, TypeAllowedSign,
    TypeAlternative, TypeAlternativeKind, TypeData, TypeDateField, TypeDateFractions,
    TypeDescription, TypeLayout, TypeNumberField, TypePart, TypeStringField,
};

/// The experimental source schema and conversion rules evolve independently.
pub const SCHEMA_VERSION: u32 = 1;
/// Named property values separate everyday editing from native reconstruction details.
pub const READABLE_SCHEMA_VERSION: u32 = 2;
/// The currently implemented conversion rules.
pub const RULES_VERSION: u32 = 1;
/// The first accepted ordinary-configuration XML profile.
pub const PROFILE: &str = "designer-2.20-configuration";
/// Designer's metadata namespace; `QName` values keep their own context separately.
pub const MD_NAMESPACE: &str = "http://v8.1c.ru/8.3/MDClasses";

/// A structured failure; consumers localize its presentation, never its code.
#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize)]
pub struct Error {
    pub code: &'static str,
    pub location: Vec<String>,
}

impl Error {
    /// Attach schema coordinates without retaining private document contents.
    pub(crate) fn at(code: &'static str, location: impl IntoIterator<Item = String>) -> Self {
        Self {
            code,
            location: location.into_iter().collect(),
        }
    }

    /// Construct a document-level failure.
    pub(crate) fn root(code: &'static str) -> Self {
        Self::at(code, [])
    }

    /// Prefix a schema coordinate while preserving the leaf failure code.
    pub(crate) fn in_field(mut self, field: impl Into<String>) -> Self {
        self.location.insert(0, field.into());
        self
    }
}

impl std::fmt::Display for Error {
    /// Keep core's fallback representation machine-facing.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.code)
    }
}

impl std::error::Error for Error {}

impl Descriptor {
    /// Materialize one validated descriptor using the observed native BOM and EOL policy.
    ///
    /// # Errors
    /// Returns source validation or namespace-materialization failures before writing a file.
    pub fn to_xml(&self) -> Result<String, Error> {
        self.validate()?;
        native_writer::encode(self)
    }

    /// Read a supported Designer descriptor without discarding unsupported property content.
    ///
    /// # Errors
    /// Returns a profile, XML, identity or unsupported-envelope failure before writing.
    pub fn from_xml(input: &str) -> Result<Self, Error> {
        xml::decode(input)
    }

    /// Decode one strict YAML document and validate the experimental source contract.
    ///
    /// # Errors
    /// Returns syntax, unsupported-profile, identity and schema failures before any writes.
    pub fn from_yaml(input: &str) -> Result<Self, Error> {
        let document = yaml::decode(input)?;
        let descriptor: Self =
            serde_json::from_value(document).map_err(|_| Error::root("source_schema_invalid"))?;
        descriptor.validate()?;
        Ok(descriptor)
    }

    /// Produce deterministic YAML; strings are quoted to preserve exact lexical values.
    ///
    /// # Errors
    /// Returns validation or serialization failures without changing the source document.
    pub fn to_yaml(&self) -> Result<String, Error> {
        self.validate()?;
        let value = serde_json::to_value(self).map_err(|_| Error::root("source_schema_invalid"))?;
        if self.schema_version == READABLE_SCHEMA_VERSION {
            yaml::encode_readable(&value)
        } else {
            yaml::encode(&value)
        }
    }
}

#[cfg(test)]
mod tests;

use synchronization::ProjectSettingsGuard;
pub(crate) use synchronization::verify_project_import_history;
pub use synchronization::{
    ImportAncestor, ImportAncestorRef, NativeProjectSourceImport, PROJECT_SOURCE_IMPORT_RECEIPT,
    ProjectSourceImportPlan, ProjectSourceImportRef, SourceImportChange, SourceImportConflict,
    SourceImportConflictKind, SourceImportPlan, SourceImportRecovery, capture_import_ancestor,
    open_initial_project_import_ancestor, prepare_project_native_source_import,
    prepare_project_source_import, prepare_source_import, project_import_ancestor,
    recover_project_source_import,
};
pub(crate) use synchronization::{
    PROJECT_SOURCE_IMPORT_DIRECTORY, initialize_project_import_ancestor,
};
