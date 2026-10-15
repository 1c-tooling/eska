//! Pure current owner/resource inputs retain reference identity and explicit coordinate precision.

use std::{ops::Range, sync::Arc};

use super::{SourcePath, SourceReader, check_bindings, composition_template, invalid};
use crate::project_core::source_format::{
    EXTERNAL_LAYOUT_VERSION, Error, FileRole, MD_NAMESPACE, MetadataSnapshot, NativeUuid,
    ObjectSource, QUERY_LAYOUT_VERSION, QueryConsumerView, QueryProfile, QueryReference,
    QueryReferenceDocument,
};
use crate::project_core::{MetadataKind, metadata_model::MetadataValue};

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
    xml: Arc<str>,
    document: QueryReferenceDocument,
    form_locations: Option<Vec<(Range<usize>, Range<usize>)>>,
}

/// A unified form has one current authoring input, even when several consumers need its XML view.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CurrentFormView {
    reference_xml: Arc<str>,
    query_context: Result<Option<QueryContextView>, Error>,
}

impl CurrentFormView {
    /// Native form metadata needs reference markup, never separately stored query values.
    #[must_use]
    pub fn reference_xml(&self) -> &str {
        &self.reference_xml
    }

    /// A current reference error stays explicit without discarding otherwise valid form metadata.
    ///
    /// # Errors
    /// Returns the current reference or binding failure; no previous context is retained.
    pub fn query_context(&self) -> Result<Option<&QueryContextView>, &Error> {
        self.query_context.as_ref().map(Option::as_ref)
    }
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
        self.query_context_from_parts(
            native_resource,
            owner,
            &uuid,
            xml.into(),
            form_locations,
            expected_profile,
        )
    }

    /// Capture the validated current form while its authoring bytes are available at the parse boundary.
    pub(in crate::project_core::source_format::reader) fn current_form_view(
        &self,
        owner: &SourcePath,
        source: &ObjectSource,
        text: &str,
    ) -> Result<Option<Arc<CurrentFormView>>, Error> {
        let Some(form) = &source.form else {
            return Ok(None);
        };
        let resource = self
            .manifest
            .query_resources
            .iter()
            .find(|resource| resource.descriptor == source.document.path);
        let Some(resource) = resource else {
            return Ok(Some(Arc::new(CurrentFormView {
                reference_xml: form.to_xml()?.into(),
                query_context: Ok(None),
            })));
        };
        let (reference_xml, query_context) = match form.query_reference_context(text, u64::MAX) {
            Ok(context) => {
                let xml: Arc<str> = context.xml.into();
                let query_context = self
                    .query_context_from_parts(
                        &resource.path,
                        owner,
                        &source.document.descriptor.object.uuid,
                        Arc::clone(&xml),
                        Some(context.locations),
                        QueryProfile::ManagedForm,
                    )
                    .map(Some);
                (xml, query_context)
            }
            Err(error) => (form.to_xml()?.into(), Err(error)),
        };
        Ok(Some(Arc::new(CurrentFormView {
            reference_xml,
            query_context,
        })))
    }

    /// Bind a distinct current DCS resource to an already parsed current owner without any file I/O.
    ///
    /// # Errors
    /// Rejects wrong manifests, owner/native identities, unloaded or invalid template properties,
    /// oversized inputs and inconsistent reference bindings. Unified forms use their captured view.
    pub fn parse_query_context_from_snapshot(
        &self,
        native_resource: &SourcePath,
        owner: &MetadataSnapshot,
        current_resource_xml: &str,
        max_bytes: u64,
    ) -> Result<QueryContextView, Error> {
        if !matches!(
            self.manifest.schema_version,
            QUERY_LAYOUT_VERSION | EXTERNAL_LAYOUT_VERSION
        ) {
            return Err(Error::root("source_query_context_layout_unsupported"));
        }
        if owner.source_bytes > max_bytes || current_resource_xml.len() as u64 > max_bytes {
            return Err(Error::root("source_document_too_large"));
        }
        let resource = self
            .manifest
            .query_resources
            .iter()
            .find(|resource| &resource.path == native_resource)
            .ok_or_else(|| Error::root("source_query_undeclared"))?;
        if owner.manifest_sha256 != self.manifest_sha256
            || self.owner_for_native_main(&resource.descriptor) != Some(&owner.path)
        {
            return Err(Error::root("source_query_context_input_mismatch"));
        }
        let document = owner
            .documents
            .iter()
            .find(|document| document.path == resource.descriptor)
            .ok_or_else(invalid)?;
        let object = document.descriptor.objects.first().ok_or_else(invalid)?;
        if !document.descriptor.diagnostics.is_empty()
            || !matches!(
                object.metadata.kind(),
                MetadataKind::Template | MetadataKind::CommonTemplate
            )
        {
            return Err(invalid());
        }
        let properties = object.properties.as_deref().ok_or_else(invalid)?;
        let mut types = properties.iter().filter(|field| {
            field.property.key.namespace.as_deref() == Some(MD_NAMESPACE)
                && field.property.key.name == "TemplateType"
        });
        if !matches!(types.next().map(|field| &field.property.value),
            Some(MetadataValue::Text(value)) if value == "DataCompositionSchema")
            || types.next().is_some()
        {
            return Err(invalid());
        }
        let uuid = NativeUuid::parse(object.metadata.uuid())?;
        self.query_context_from_parts(
            native_resource,
            &owner.path,
            &uuid,
            Arc::from(current_resource_xml),
            None,
            QueryProfile::Composition,
        )
    }

    /// Both current-input routes enforce the same frozen native resource and query declarations.
    fn query_context_from_parts(
        &self,
        native_resource: &SourcePath,
        owner: &SourcePath,
        uuid: &NativeUuid,
        xml: Arc<str>,
        form_locations: Option<Vec<(Range<usize>, Range<usize>)>>,
        profile: QueryProfile,
    ) -> Result<QueryContextView, Error> {
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
        let declared_owner = self
            .owner_for_native_main(&resource.descriptor)
            .ok_or_else(invalid)?;
        let declared_uuid = self
            .owners
            .get(declared_owner)
            .and_then(|entry| entry.descriptors.get(&resource.descriptor))
            .ok_or_else(invalid)?;
        let is_form = matches!(file.role, FileRole::ManagedForm { .. });
        if owner != declared_owner || uuid != declared_uuid {
            return Err(Error::root("source_identity_mismatch"));
        }
        if is_form != (profile == QueryProfile::ManagedForm) {
            return Err(invalid());
        }
        let document = QueryReferenceDocument::from_xml(&xml)?;
        if document.profile() != profile {
            return Err(invalid());
        }
        check_bindings(Some(resource), &document)?;
        Ok(QueryContextView {
            owner: owner.clone(),
            descriptor: resource.descriptor.clone(),
            source: file.source_path().clone(),
            native: native_resource.clone(),
            uuid: uuid.clone(),
            xml,
            document,
            form_locations,
        })
    }
}
