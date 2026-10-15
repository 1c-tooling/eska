//! Pure current owner/resource inputs retain reference identity and explicit coordinate precision.

use std::ops::Range;

use super::{SourcePath, SourceReader, check_bindings, composition_template, invalid};
use crate::project_core::source_format::{
    EXTERNAL_LAYOUT_VERSION, Error, FileRole, NativeUuid, QUERY_LAYOUT_VERSION, QueryConsumerView,
    QueryProfile, QueryReference, QueryReferenceDocument,
};

#[cfg(test)]
mod tests;

/// Authoring ranges belong to the supplied resource bytes, never to accepted or generated files.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum QueryAuthoringRange {
    /// A native-reference XML subrange is identical to the source XML bytes.
    Exact(Range<usize>),
    /// The exact enclosing YAML scalar is known; escapes/folding have no decoded offset map yet.
    EnclosingNode(Range<usize>),
    /// Generated values or ranges spanning several fragments have no proven single source node.
    Unmapped,
}

/// An immutable resource view validates storage bindings, without query semantics or edit authority.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct QueryContextView {
    owner: SourcePath,
    descriptor: SourcePath,
    source: SourcePath,
    native: SourcePath,
    uuid: NativeUuid,
    xml: String,
    document: QueryReferenceDocument,
    form_locations: Option<Vec<(Range<usize>, Range<usize>)>>,
}

impl QueryContextView {
    /// Project proven native storage consumers, retaining unknown parts and incomplete coverage.
    ///
    /// # Errors
    /// Rejects ambiguous or malformed supported fields and invalid current coordinate mappings.
    /// Query text, metadata semantics and decoded YAML subranges are never inferred.
    pub fn consumer_view(&self) -> Result<QueryConsumerView, Error> {
        crate::project_core::source_format::queries::consumer_view(self)
    }

    /// Return the physical descriptor owner checked against the reader's frozen manifest.
    #[must_use]
    pub const fn owner(&self) -> &SourcePath {
        &self.owner
    }

    /// Return the exact native descriptor, including its validated nested scope.
    #[must_use]
    pub const fn native_descriptor(&self) -> &SourcePath {
        &self.descriptor
    }

    /// Return the sole authoring resource path to which all mapped ranges refer.
    #[must_use]
    pub const fn source(&self) -> &SourcePath {
        &self.source
    }

    /// Return the declared native resource identity without inferring a physical filename.
    #[must_use]
    pub const fn native_resource(&self) -> &SourcePath {
        &self.native
    }

    /// Return the descriptor UUID verified in the supplied current owner text.
    #[must_use]
    pub const fn uuid(&self) -> &NativeUuid {
        &self.uuid
    }

    /// Resource purpose comes from its checked native root rather than a query filename.
    #[must_use]
    pub const fn profile(&self) -> QueryProfile {
        self.document.profile()
    }

    /// Return only explicit links and scoped slots; plain query values have not been read.
    #[must_use]
    pub fn references(&self) -> &[QueryReference] {
        self.document.references()
    }

    /// These bounded bytes contain reference markup, not materialized query values for the platform.
    #[must_use]
    pub fn reference_xml(&self) -> &str {
        &self.xml
    }

    /// Map only proven byte ranges in these exact reference bytes to the current authoring resource.
    ///
    /// # Errors
    /// Rejects reversed, out-of-bounds or non-UTF-8-boundary ranges instead of guessing a source span.
    pub fn locate_reference_range(
        &self,
        range: Range<usize>,
    ) -> Result<QueryAuthoringRange, Error> {
        if range.start > range.end || self.xml.get(range.clone()).is_none() {
            return Err(Error::root("source_location_invalid"));
        }
        let Some(locations) = &self.form_locations else {
            return Ok(QueryAuthoringRange::Exact(range));
        };
        let mut matches = locations.iter().filter(|(native, _)| {
            native.start <= range.start
                && range.end <= native.end
                && (!range.is_empty() || range.start < native.end)
        });
        let Some((_, authoring)) = matches.next() else {
            return Ok(QueryAuthoringRange::Unmapped);
        };
        if matches.next().is_some() {
            return Ok(QueryAuthoringRange::Unmapped);
        }
        Ok(QueryAuthoringRange::EnclosingNode(authoring.clone()))
    }
}

impl SourceReader {
    /// Validate exact current owner/resource inputs without opening any file or resolving plain values.
    ///
    /// # Errors
    /// Rejects unsupported layouts, oversized inputs, wrong owner identities/types or inconsistent links.
    /// A mapped form uses the same authoring bytes for owner and resource; neither may shadow the other.
    pub fn parse_query_context(
        &self,
        native_resource: &SourcePath,
        owner_text: &str,
        resource_text: &str,
        max_bytes: u64,
    ) -> Result<QueryContextView, Error> {
        if !matches!(
            self.manifest.schema_version,
            QUERY_LAYOUT_VERSION | EXTERNAL_LAYOUT_VERSION
        ) {
            return Err(Error::root("source_query_context_layout_unsupported"));
        }
        if owner_text.len() as u64 > max_bytes || resource_text.len() as u64 > max_bytes {
            return Err(Error::root("source_document_too_large"));
        }
        let resource = self
            .manifest
            .query_resources
            .iter()
            .find(|resource| &resource.path == native_resource)
            .ok_or_else(|| Error::root("source_query_undeclared"))?;
        let file = self
            .manifest
            .files
            .iter()
            .find(|file| &file.path == native_resource)
            .ok_or_else(invalid)?;
        let owner = self
            .owner_for_native_main(&resource.descriptor)
            .ok_or_else(invalid)?;
        if owner == file.source_path() && owner_text != resource_text {
            return Err(Error::root("source_query_context_input_mismatch"));
        }
        let snapshot = self.parse_owner(owner, owner_text)?;
        let descriptor = std::iter::once(&snapshot.source.document)
            .chain(&snapshot.source.nested)
            .find(|document| document.path == resource.descriptor)
            .ok_or_else(invalid)?;
        let uuid = descriptor.descriptor.object.uuid.clone();
        let expected_profile = if matches!(file.role, FileRole::ManagedForm { .. }) {
            QueryProfile::ManagedForm
        } else {
            QueryProfile::Composition
        };
        let (xml, form_locations) = if expected_profile == QueryProfile::ManagedForm {
            let form = self.parse_form_source(file.source_path(), resource_text)?;
            let context = form.query_reference_context(resource_text, max_bytes)?;
            (context.xml, Some(context.locations))
        } else {
            if !composition_template(descriptor)? {
                return Err(invalid());
            }
            (resource_text.to_owned(), None)
        };
        let document = QueryReferenceDocument::from_xml(&xml)?;
        if document.profile() != expected_profile {
            return Err(invalid());
        }
        check_bindings(Some(resource), &document)?;
        Ok(QueryContextView {
            owner: owner.clone(),
            descriptor: resource.descriptor.clone(),
            source: file.source_path().clone(),
            native: native_resource.clone(),
            uuid,
            xml,
            document,
            form_locations,
        })
    }
}
