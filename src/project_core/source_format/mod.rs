//! Experimental, reversible source documents, independent of IDE and platform runners.

mod baseline;
mod composition;
mod coordinates;
mod edit;
mod export;
mod files;
mod fragment;
mod import;
mod model;
mod native_writer;
mod package;
mod protected;
mod reader;
mod validation;
mod xml;
mod yaml;

pub use baseline::{
    AcceptedBase, AcceptedRef, Operation, Permission, PermissionReason, accept_source,
};
pub use coordinates::{CoordinateError, TextCoordinates, TextPosition, TextRange};
pub use edit::{
    SourceEditPlan, SourcePropertyEdit, prepare_property_edit, prepare_property_edit_text,
};
pub use export::export_designer;
pub use import::import_designer;
pub use package::{FileEntry, FileRole, NativeDocument, ObjectSource, SourceManifest, SourcePath};
pub use protected::{
    Exclusion, ExclusionAction, FileOrigin, InputAuthority, InputFile, MappedFile, OriginTexts,
    PreparedSource, ProtectedExport, ProtectedFailure, SOURCE_MAP_VERSION, SourceLocation,
    SourceMap, export_protected, prepare_protected,
};
pub use reader::{
    DeclaredModule, DeclaredOwner, MetadataDeclaration, MetadataSnapshot, NativeMetadata,
    OwnerSnapshot, SourceReader,
};

pub use model::{
    Child, Descriptor, LineEnding, LocalizedText, MetadataNode, Namespace, NativeUuid, Property,
    PropertyName, PropertyValue, Section, XmlAttribute, XmlFragment, XmlProfile,
};

/// The experimental source schema and conversion rules evolve independently.
pub const SCHEMA_VERSION: u32 = 1;
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
        yaml::encode(&value)
    }
}

#[cfg(test)]
mod tests;
