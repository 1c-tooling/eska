//! Named configuration types resolve through native `GeneratedType` declarations, not name guesses.

use super::{TypeAlternative, qualified_name};
use crate::project_core::source_format::{
    Error, MD_NAMESPACE, MetadataNode, Namespace, Section, fragment,
    metadata_reference::READABLE_NAMESPACE, validation::ncname,
};

const CONFIGURATION_NAMESPACE: &str = "http://v8.1c.ru/8.1/data/enterprise/current-config";

/// Bare platform types and general type sets do not designate one named metadata object.
pub(in crate::project_core::source_format) fn named_target<'a>(
    alternative: &'a TypeAlternative,
    context: &[Namespace],
) -> Result<Option<&'a str>, Error> {
    let (namespace, name) = qualified_name(&alternative.name, context)?;
    Ok((namespace == CONFIGURATION_NAMESPACE && name.contains('.')).then_some(name))
}

/// Only declarations inside the actual metadata `InternalInfo` contribute generated type names.
pub(in crate::project_core::source_format) fn generated_names(
    node: &MetadataNode,
    context: &[Namespace],
) -> Result<Vec<String>, Error> {
    let mut names = Vec::new();
    for section in &node.sections {
        let Section::Preserved(value) = section else {
            continue;
        };
        let wrapped = fragment::wrap(value, context)?;
        let document = roxmltree::Document::parse(&wrapped)
            .map_err(|_| Error::root("source_fragment_xml_invalid"))?;
        let Some(root) = document.root_element().first_element_child() else {
            continue;
        };
        if !root.has_tag_name((MD_NAMESPACE, "InternalInfo")) {
            continue;
        }
        for declaration in root.children().filter(roxmltree::Node::is_element) {
            if !declaration.has_tag_name((READABLE_NAMESPACE, "GeneratedType")) {
                continue;
            }
            let name = declaration
                .attribute("name")
                .filter(|name| ncname(name))
                .ok_or_else(|| Error::root("source_type_definition_invalid"))?;
            names.push(name.into());
        }
    }
    Ok(names)
}
