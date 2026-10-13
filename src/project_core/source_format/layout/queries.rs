//! Only declared native forms and composition templates may bind separately authored query values.

use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
};

use serde::{Deserialize, Serialize};

use crate::project_core::MetadataKind;
use crate::project_core::source_format::{
    Error, FileRole, ManagedFormSource, NativeDocument, NativeQueryDocument, ObjectSource,
    QueryProfile, QueryReferenceDocument, SourceManifest, SourcePath, files, xml,
};

mod naming;
#[cfg(test)]
mod tests;

/// Native descriptor identity owns every declared source; scoped bindings live only in reference XML.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct QueryResource {
    pub path: SourcePath,
    pub descriptor: SourcePath,
    pub sources: Vec<SourcePath>,
}

/// Query ownership is a verified resource location, never inferred from its source filename.
pub(in crate::project_core::source_format) fn native_path(
    descriptor: &SourcePath,
    kind: MetadataKind,
) -> Result<SourcePath, Error> {
    if matches!(kind, MetadataKind::Form | MetadataKind::CommonForm) {
        return super::forms::native_path(descriptor);
    }
    let prefix = descriptor
        .as_str()
        .strip_suffix(".xml")
        .ok_or_else(invalid)?;
    SourcePath::parse(&format!("{prefix}/Ext/Template.xml"))
}

/// Source declarations cannot activate a foreign resource, alias another value or escape their owner.
pub(super) fn validate_map(manifest: &SourceManifest) -> Result<(), Error> {
    if manifest.schema_version < super::QUERY_LAYOUT_VERSION {
        return if manifest.query_resources.is_empty() {
            Ok(())
        } else {
            Err(invalid())
        };
    }
    let entries = manifest
        .files
        .iter()
        .map(|file| (&file.path, file))
        .collect::<BTreeMap<_, _>>();
    let contexts = manifest
        .descriptor_contexts
        .iter()
        .map(|context| (&context.path, context))
        .collect::<BTreeMap<_, _>>();
    let mut resources = BTreeSet::new();
    let mut sources = BTreeSet::new();
    for resource in &manifest.query_resources {
        let context = contexts.get(&resource.descriptor).ok_or_else(invalid)?;
        let file = entries.get(&resource.path).ok_or_else(invalid)?;
        let descriptor = entries.get(&resource.descriptor).ok_or_else(invalid)?;
        if !matches!(descriptor.role, FileRole::Descriptor { .. })
            || !matches!(
                context.metadata_kind,
                MetadataKind::Form
                    | MetadataKind::CommonForm
                    | MetadataKind::Template
                    | MetadataKind::CommonTemplate
            )
            || !resource.path.as_str().eq_ignore_ascii_case(
                native_path(&resource.descriptor, context.metadata_kind)?.as_str(),
            )
            || !resources.insert(&resource.path)
        {
            return Err(invalid().in_field(resource.path.as_str()));
        }
        let form = matches!(
            context.metadata_kind,
            MetadataKind::Form | MetadataKind::CommonForm
        );
        if form
            && !matches!(&file.role, FileRole::ManagedForm { descriptor, .. } if descriptor == &resource.descriptor)
            || !form && !file.is_payload()
        {
            return Err(invalid().in_field(resource.path.as_str()));
        }
        let prefix = format!(
            "{}/Queries/",
            resource
                .descriptor
                .as_str()
                .strip_suffix(".xml")
                .ok_or_else(invalid)?
        );
        for path in &resource.sources {
            if !path.as_str().starts_with(&prefix)
                || !path
                    .as_path()
                    .extension()
                    .is_some_and(|extension| extension.eq_ignore_ascii_case("sdbl"))
                || !sources.insert(path.as_str().to_lowercase())
            {
                return Err(invalid().in_field(path.as_str()));
            }
        }
    }
    Ok(())
}

/// A template's exact native type decides promotion; unrelated XML and binary resources remain opaque.
pub(in crate::project_core::source_format) fn composition_template(
    source: &NativeDocument,
) -> Result<bool, Error> {
    let input = source.descriptor.to_xml()?;
    let document = roxmltree::Document::parse(&input).map_err(|_| invalid())?;
    let namespace = "http://v8.1c.ru/8.3/MDClasses";
    let Some(object) = document.root_element().children().find(|node| {
        node.has_tag_name((namespace, "Template"))
            || node.has_tag_name((namespace, "CommonTemplate"))
    }) else {
        return Err(invalid());
    };
    let properties = object
        .children()
        .find(|node| node.has_tag_name((namespace, "Properties")))
        .ok_or_else(invalid)?;
    let mut types = properties
        .children()
        .filter(|node| node.has_tag_name((namespace, "TemplateType")));
    let Some(kind) = types.next() else {
        return Ok(false);
    };
    if types.next().is_some() || kind.children().any(|node| node.is_element()) {
        return Err(invalid());
    }
    Ok(xml::text(kind) == "DataCompositionSchema")
}

/// Promotion runs before publication; source XML keeps only references and plain values move once.
pub(in crate::project_core::source_format) fn extract(
    input: &Path,
    manifest: &mut SourceManifest,
    sources: &mut BTreeMap<SourcePath, ObjectSource>,
) -> Result<BTreeMap<SourcePath, String>, Error> {
    let locations = manifest
        .descriptor_contexts
        .iter()
        .filter(|context| {
            matches!(
                context.metadata_kind,
                MetadataKind::Form
                    | MetadataKind::CommonForm
                    | MetadataKind::Template
                    | MetadataKind::CommonTemplate
            )
        })
        .map(|context| {
            Ok((
                native_path(&context.path, context.metadata_kind)?
                    .as_str()
                    .to_ascii_lowercase(),
                context,
            ))
        })
        .collect::<Result<BTreeMap<_, _>, Error>>()?;
    let owners = manifest
        .files
        .iter()
        .filter_map(|file| match &file.role {
            FileRole::Descriptor { source, .. } => Some((&file.path, source)),
            _ => None,
        })
        .collect::<BTreeMap<_, _>>();
    let mut resources = Vec::new();
    let mut extra = BTreeMap::new();
    for file in &manifest.files {
        let Some(context) = locations.get(&file.path.as_str().to_ascii_lowercase()) else {
            continue;
        };
        let owner = owners.get(&context.path).ok_or_else(invalid)?;
        let source = sources.get_mut(*owner).ok_or_else(invalid)?;
        let form = source.form.is_some();
        if !form && !composition_template(&source.document)? {
            continue;
        }
        let original = if let Some(form) = &source.form {
            form.to_xml()?
        } else {
            files::read_text(&input.join(file.path.as_path()))?
        };
        let native = NativeQueryDocument::from_xml(&original)
            .map_err(|error| error.in_field(file.path.as_str()))?;
        if form && native.queries().is_empty() {
            continue;
        }
        let paths = naming::paths(&context.path, &native)?;
        let declared = paths.values().cloned().collect();
        let (xml, values) = native.extract(&paths)?.into_parts();
        if QueryReferenceDocument::from_xml(&xml)?.materialize(&values)? != original {
            return Err(invalid());
        }
        for (path, value) in values {
            if extra.insert(path, value).is_some() {
                return Err(invalid());
            }
        }
        if form {
            source.form = Some(ManagedFormSource::from_xml(&xml)?);
        } else if extra.insert(file.source_path().clone(), xml).is_some() {
            return Err(invalid());
        }
        resources.push(QueryResource {
            path: file.path.clone(),
            descriptor: context.path.clone(),
            sources: declared,
        });
    }
    manifest.query_resources = resources;
    manifest.schema_version = super::QUERY_LAYOUT_VERSION;
    Ok(extra)
}

/// A complete resource has exactly its declared plain values; inline or undeclared links never fall back.
pub(in crate::project_core::source_format) fn check_bindings(
    resource: Option<&QueryResource>,
    document: &QueryReferenceDocument,
) -> Result<(), Error> {
    let actual = document
        .references()
        .iter()
        .map(|reference| &reference.source)
        .collect::<BTreeSet<_>>();
    let expected = resource
        .into_iter()
        .flat_map(|resource| &resource.sources)
        .collect::<BTreeSet<_>>();
    let profile = if resource.is_none_or(|resource| {
        resource
            .path
            .as_path()
            .file_name()
            .is_some_and(|name| name.eq_ignore_ascii_case("Form.xml"))
    }) {
        QueryProfile::ManagedForm
    } else {
        QueryProfile::Composition
    };
    if document.profile() != profile || actual != expected {
        return Err(invalid());
    }
    Ok(())
}

/// Reverse-map corruption is distinct from malformed XML or missing physical values.
pub(in crate::project_core::source_format) fn invalid() -> Error {
    Error::root("source_query_map_invalid")
}
