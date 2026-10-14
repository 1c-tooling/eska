//! Addressed query reads expose declarations without scanning other resources or granting edit rights.

use std::collections::BTreeMap;

use super::{Error, FileRole, NativeUuid, SourcePath, SourceReader, read_text_limited};
use crate::project_core::source_format::{
    QueryReferenceDocument,
    layout::queries::{check_bindings, composition_template, invalid},
};

mod context;
pub use context::{QueryAuthoringRange, QueryContextView};

/// Plain text keeps explicit native ownership; actual dataset/attribute scope is read from its resource.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DeclaredQuery<'a> {
    pub owner: &'a SourcePath,
    pub native_descriptor: &'a SourcePath,
    pub native_resource: &'a SourcePath,
    pub uuid: &'a NativeUuid,
}

impl SourceReader {
    /// Enumerate declared plain files without opening their owner, resource or neighboring values.
    pub fn query_paths(&self) -> impl Iterator<Item = &SourcePath> {
        self.manifest
            .query_resources
            .iter()
            .flat_map(|resource| &resource.sources)
    }

    /// A missing physical value retains its declaration; filenames do not infer a native query slot.
    #[must_use]
    pub fn declared_query(&self, path: &SourcePath) -> Option<DeclaredQuery<'_>> {
        let resource = self
            .manifest
            .query_resources
            .iter()
            .find(|resource| resource.sources.contains(path))?;
        let owner = self.owner_for_native_main(&resource.descriptor)?;
        let uuid = self
            .owners
            .get(owner)?
            .descriptors
            .get(&resource.descriptor)?;
        Some(DeclaredQuery {
            owner,
            uuid,
            native_descriptor: &resource.descriptor,
            native_resource: &resource.path,
        })
    }

    /// Read only the exact declared plain file, preserving decoded characters and logical line endings.
    ///
    /// # Errors
    /// Returns an undeclared query, unsafe/missing file, encoding or tightened byte-limit failure.
    pub fn read_query_text(&self, path: &SourcePath, max_bytes: u64) -> Result<String, Error> {
        self.declared_query(path)
            .ok_or_else(|| Error::root("source_query_undeclared").in_field(path.as_str()))?;
        read_text_limited(&self.root, path, max_bytes)
    }

    /// Read the actual declared reference resource without loading any plain query values.
    ///
    /// Form references derive from the sole YAML body; source ranges refer to that generated XML.
    ///
    /// # Errors
    /// Returns an undeclared resource, bounded read, invalid reference or mismatched source set.
    pub fn read_query_resource(
        &self,
        native: &SourcePath,
        max_bytes: u64,
    ) -> Result<QueryReferenceDocument, Error> {
        let resource = self
            .manifest
            .query_resources
            .iter()
            .find(|resource| &resource.path == native)
            .ok_or_else(|| Error::root("source_query_undeclared"))?;
        let file = self
            .manifest
            .files
            .iter()
            .find(|file| &file.path == native)
            .ok_or_else(invalid)?;
        let xml = if matches!(file.role, FileRole::ManagedForm { .. }) {
            self.read_form_source(file.source_path(), max_bytes)?
                .to_xml()?
        } else {
            let owner = self
                .owner_for_native_main(&resource.descriptor)
                .ok_or_else(invalid)?;
            let text = self.read_owner_text(owner, max_bytes)?;
            if !composition_template(&self.parse_owner(owner, &text)?.source.document)? {
                return Err(invalid());
            }
            read_text_limited(&self.root, file.source_path(), max_bytes)?
        };
        if xml.len() as u64 > max_bytes {
            return Err(
                Error::root("source_document_too_large").in_field(file.source_path().as_str())
            );
        }
        let document = QueryReferenceDocument::from_xml(&xml)?;
        check_bindings(Some(resource), &document)?;
        Ok(document)
    }

    /// Native form reads resolve only their declared values and keep both reads and output bounded.
    pub(super) fn materialize_query_xml(
        &self,
        native: &SourcePath,
        xml: &str,
        max_bytes: u64,
    ) -> Result<String, Error> {
        let resource = self
            .manifest
            .query_resources
            .iter()
            .find(|resource| &resource.path == native);
        let document = QueryReferenceDocument::from_xml(xml)?;
        check_bindings(resource, &document)?;
        let mut values = BTreeMap::new();
        for reference in document.references() {
            values.insert(
                reference.source.clone(),
                self.read_query_text(&reference.source, max_bytes)?,
            );
        }
        let output = document.materialize(&values)?;
        if output.len() as u64 > max_bytes {
            return Err(Error::root("source_document_too_large").in_field(native.as_str()));
        }
        Ok(output)
    }
}
