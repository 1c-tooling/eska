//! Web-service operations and parameter names retain source order and actual parents.

#[cfg(test)]
mod tests;

use bsl_metadata::{
    WebService, WebServiceBuilder, WebServiceOperation, WebServiceOperationBuilder,
    WebServiceParameter,
};
use eska::project_core::{
    MetadataKind,
    metadata_parser::{ParsedDescriptor, ParsedObject},
};

use crate::{AdapterError, AdapterIssue, identity, object::children, properties::Properties};

/// Project the native operation model without reading BSL or materializing XML.
///
/// # Errors
/// Returns an incomplete descriptor, wrong kind, invalid identity or malformed property structure.
pub fn web_service(descriptor: &ParsedDescriptor) -> Result<WebService, AdapterError> {
    let root = identity::descriptor_root(descriptor)?;
    if root.metadata.kind() != MetadataKind::WebService {
        return Err(AdapterError {
            issue: AdapterIssue::WrongKind {
                actual: root.metadata.kind(),
            },
            range: root.range.clone(),
        });
    }
    let _uuid = identity::uuid(root)?;
    let fields = Properties::new(root)?;
    let mut builder = WebServiceBuilder::new()
        .name(root.metadata.name())
        .namespace(fields.text("Namespace")?.unwrap_or_default())
        // Native URI is a logical spelling; the session links only declared source bodies.
        .uri(format!(
            "WebServices/{}/Ext/Module.bsl",
            root.metadata.name()
        ));
    for child in
        children(descriptor, root).filter(|child| child.metadata.kind() == MetadataKind::Operation)
    {
        builder = builder.add_operation(operation(descriptor, child)?);
    }
    Ok(builder.build())
}

/// Identical parameter names in distinct operations cannot be merged into one signature.
fn operation(
    descriptor: &ParsedDescriptor,
    object: &ParsedObject,
) -> Result<WebServiceOperation, AdapterError> {
    let _uuid = identity::uuid(object)?;
    let fields = Properties::new(object)?;
    let mut builder = WebServiceOperationBuilder::new()
        .name(object.metadata.name())
        .procedure_name(fields.text("ProcedureName")?.unwrap_or_default());
    for child in children(descriptor, object)
        .filter(|child| child.metadata.kind() == MetadataKind::Parameter)
    {
        let _uuid = identity::uuid(child)?;
        // Summary-only children cannot stand in for complete captured parameter properties.
        let _fields = Properties::new(child)?;
        builder = builder.add_parameter(WebServiceParameter::new(child.metadata.name()));
    }
    Ok(builder.build())
}
