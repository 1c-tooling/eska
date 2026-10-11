//! HTTP routes use their actual template and method parents, preserving native order.

#[cfg(test)]
mod tests;

use bsl_metadata::{
    HTTPService, HTTPServiceBuilder, HTTPServiceMethodBuilder, HTTPServiceURLTemplate,
    HTTPServiceURLTemplateBuilder,
};
use eska::project_core::{
    MetadataKind,
    metadata_parser::{ParsedDescriptor, ParsedObject},
};

use crate::{AdapterError, AdapterIssue, identity, object::children, properties::Properties};

/// Project routes and handlers without reading the service's module or materializing XML.
///
/// # Errors
/// Returns an incomplete descriptor, wrong kind, invalid identity or malformed property structure.
pub fn http_service(descriptor: &ParsedDescriptor) -> Result<HTTPService, AdapterError> {
    let root = identity::descriptor_root(descriptor)?;
    if root.metadata.kind() != MetadataKind::HTTPService {
        return Err(AdapterError {
            issue: AdapterIssue::WrongKind {
                actual: root.metadata.kind(),
            },
            range: root.range.clone(),
        });
    }
    let _uuid = identity::uuid(root)?;
    let fields = Properties::new(root)?;
    let mut builder = HTTPServiceBuilder::new()
        .name(root.metadata.name())
        .root_url(fields.text("RootURL")?.unwrap_or_default())
        // Match the native parser's logical URI; a declared body backlink is registered separately.
        .uri(format!(
            "HTTPServices/{}/Ext/Module.bsl",
            root.metadata.name()
        ));
    for child in children(descriptor, root)
        .filter(|child| child.metadata.kind() == MetadataKind::URLTemplate)
    {
        builder = builder.add_url_template(url_template(descriptor, child)?);
    }
    Ok(builder.build())
}

/// Equal method names in different templates retain their actual parent instead of being flattened.
fn url_template(
    descriptor: &ParsedDescriptor,
    object: &ParsedObject,
) -> Result<HTTPServiceURLTemplate, AdapterError> {
    let _uuid = identity::uuid(object)?;
    let fields = Properties::new(object)?;
    let mut builder = HTTPServiceURLTemplateBuilder::new()
        .name(object.metadata.name())
        .template(fields.text("Template")?.unwrap_or_default());
    for child in
        children(descriptor, object).filter(|child| child.metadata.kind() == MetadataKind::Method)
    {
        let _uuid = identity::uuid(child)?;
        let fields = Properties::new(child)?;
        builder = builder.add_method(
            HTTPServiceMethodBuilder::new()
                .name(child.metadata.name())
                .http_method(fields.text("HTTPMethod")?.unwrap_or_default())
                .handler(fields.text("Handler")?.unwrap_or_default())
                .build(),
        );
    }
    Ok(builder.build())
}
