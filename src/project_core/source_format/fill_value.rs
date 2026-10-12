//! A named enum filling value has a verified target and a separate native type annotation.

use roxmltree::Node;
use serde::{Deserialize, Serialize};

use super::{
    Error, MetadataReference, Namespace,
    metadata_reference::{READABLE_NAMESPACE, XSI_NAMESPACE},
    validation::ncname,
};

#[cfg(test)]
mod tests;

/// Only named enum members are accepted; UUID pairs, empty references and other value types stay opaque.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EnumFillValue {
    pub reference: MetadataReference,
    pub native: EnumFillValueLayout,
}

/// Reconstruction prefixes contain no second copy of the editable metadata reference.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EnumFillValueLayout {
    pub xsi_prefix: String,
    pub type_prefix: String,
}

impl EnumFillValue {
    /// Check the closed target shape and both namespace bindings before native materialization.
    pub(super) fn validate(&self, context: &[Namespace]) -> Result<(), Error> {
        let parts = self.reference.as_str().split('.').collect::<Vec<_>>();
        if parts.len() != 4 || parts[0] != "Enum" || parts[2] != "EnumValue" {
            return Err(Error::root("source_metadata_reference_invalid"));
        }
        for (prefix, uri) in [
            (&self.native.xsi_prefix, XSI_NAMESPACE),
            (&self.native.type_prefix, READABLE_NAMESPACE),
        ] {
            if !ncname(prefix)
                || !context
                    .iter()
                    .any(|binding| binding.prefix.as_deref() == Some(prefix) && binding.uri == uri)
            {
                return Err(Error::root("source_metadata_reference_namespace_invalid"));
            }
        }
        Ok(())
    }
}

/// Promote only text with one resolved `DesignTimeRef` annotation and no local namespace envelope.
pub(super) fn decode(
    node: Node<'_, '_>,
    input: &str,
    context: &[Namespace],
) -> Option<EnumFillValue> {
    if node.attributes().len() != 1 || !node.children().all(|part| part.is_text()) {
        return None;
    }
    let head = input
        .get(node.range())?
        .strip_prefix('<')?
        .split_once('>')?
        .0;
    if head.contains("xmlns") {
        return None;
    }
    let mut fields = head.split_whitespace();
    if fields.next()? != "FillValue" {
        return None;
    }
    let attribute = fields.next()?;
    if fields.next().is_some() {
        return None;
    }
    let (attribute_name, _) = attribute.split_once('=')?;
    let (xsi_prefix, local) = attribute_name.split_once(':')?;
    if local != "type" {
        return None;
    }
    let (type_prefix, local) = node.attribute((XSI_NAMESPACE, "type"))?.split_once(':')?;
    if local != "DesignTimeRef" {
        return None;
    }
    let text: String = node.children().filter_map(|part| part.text()).collect();
    let result = EnumFillValue {
        reference: MetadataReference::parse(&text).ok()?,
        native: EnumFillValueLayout {
            xsi_prefix: xsi_prefix.into(),
            type_prefix: type_prefix.into(),
        },
    };
    result.validate(context).ok()?;
    Some(result)
}
