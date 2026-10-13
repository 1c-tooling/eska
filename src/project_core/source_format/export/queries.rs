//! Every plain value belongs to the captured package, not a late filesystem fallback.

use std::collections::BTreeMap;

use crate::project_core::MetadataKind;
use crate::project_core::source_format::{
    Error, FileRole, ManagedFormSource, NativeDocument, QueryReferenceDocument, SourceManifest,
    SourcePath,
    layout::queries::{check_bindings, composition_template, invalid, native_path},
    validation,
};

/// Composition resources own their reference representation; form XML is rendered from its sole YAML.
#[derive(Default)]
pub(in crate::project_core::source_format) struct QueryInputs {
    pub resources: BTreeMap<SourcePath, QueryReferenceDocument>,
    pub values: BTreeMap<SourcePath, String>,
}

/// Baseline blobs and working trees use the same complete declaration and character checks.
pub(super) fn load(
    manifest: &SourceManifest,
    forms: &BTreeMap<SourcePath, ManagedFormSource>,
    documents: &BTreeMap<SourcePath, NativeDocument>,
    read: &mut impl FnMut(&SourcePath) -> Result<String, Error>,
) -> Result<QueryInputs, Error> {
    let mut input = QueryInputs::default();
    if manifest.schema_version < super::super::QUERY_LAYOUT_VERSION {
        return Ok(input);
    }
    let resources = manifest
        .query_resources
        .iter()
        .map(|resource| (&resource.path, resource))
        .collect::<BTreeMap<_, _>>();
    for (path, form) in forms {
        let xml = form.to_xml()?;
        let document = QueryReferenceDocument::from_xml(&xml)
            .map_err(|error| error.in_field(path.as_str()))?;
        check_bindings(resources.get(path).copied(), &document)
            .map_err(|error| error.in_field(path.as_str()))?;
    }
    let entries = manifest
        .files
        .iter()
        .map(|file| (&file.path, file))
        .collect::<BTreeMap<_, _>>();
    let native_entries = manifest
        .files
        .iter()
        .map(|file| (file.path.as_str().to_ascii_lowercase(), file))
        .collect::<BTreeMap<_, _>>();
    for document in documents.values() {
        let kind = document.descriptor.object.kind;
        if matches!(kind, MetadataKind::Template | MetadataKind::CommonTemplate)
            && composition_template(document)?
        {
            let native = native_path(&document.path, kind)?;
            let file = native_entries
                .get(&native.as_str().to_ascii_lowercase())
                .ok_or_else(invalid)?;
            if !resources.contains_key(&file.path) {
                return Err(invalid().in_field(file.path.as_str()));
            }
        }
    }
    for resource in &manifest.query_resources {
        let file = entries.get(&resource.path).ok_or_else(invalid)?;
        if file.is_payload() {
            if !composition_template(documents.get(&resource.descriptor).ok_or_else(invalid)?)? {
                return Err(invalid());
            }
            let xml = read(file.source_path())?;
            let document = QueryReferenceDocument::from_xml(&xml)
                .map_err(|error| error.in_field(file.source_path().as_str()))?;
            check_bindings(Some(resource), &document)
                .map_err(|error| error.in_field(file.source_path().as_str()))?;
            input.resources.insert(resource.path.clone(), document);
        }
        for path in &resource.sources {
            let text = read(path).map_err(|error| error.in_field(path.as_str()))?;
            validation::xml_text(&text).map_err(|error| error.in_field(path.as_str()))?;
            if input.values.insert(path.clone(), text).is_some() {
                return Err(invalid());
            }
        }
    }
    Ok(input)
}

impl QueryInputs {
    /// Native output is complete before a directory is claimed; all source markers are removed.
    pub(super) fn materialize(
        &self,
        manifest: &SourceManifest,
        xml: &mut BTreeMap<SourcePath, String>,
    ) -> Result<(), Error> {
        if manifest.schema_version < super::super::QUERY_LAYOUT_VERSION {
            return Ok(());
        }
        let resources = manifest
            .query_resources
            .iter()
            .map(|resource| (&resource.path, resource))
            .collect::<BTreeMap<_, _>>();
        for file in &manifest.files {
            if !matches!(file.role, FileRole::ManagedForm { .. }) {
                continue;
            }
            let document =
                QueryReferenceDocument::from_xml(xml.get(&file.path).ok_or_else(invalid)?)?;
            check_bindings(resources.get(&file.path).copied(), &document)?;
            xml.insert(
                file.path.clone(),
                document
                    .render(|reference| self.values.get(&reference.source).map(String::as_str))?,
            );
        }
        for (path, document) in &self.resources {
            xml.insert(
                path.clone(),
                document
                    .render(|reference| self.values.get(&reference.source).map(String::as_str))?,
            );
        }
        Ok(())
    }
}
