//! Verified inline attributes share structural gates without interpreting their native scalar defaults.

use super::{
    Error, MD_NAMESPACE, MetadataNode, Namespace, Property, PropertyName, PropertyValue, Section,
    fragment,
};

/// The observed 8.3.27 attribute domains do not own child descriptors or generated identity slots.
pub(super) fn attribute(node: &MetadataNode, context: &[Namespace]) -> Result<bool, Error> {
    let [Section::Properties(properties)] = node.sections.as_slice() else {
        return Ok(false);
    };
    let [
        Property::Typed {
            name: PropertyName::Name,
            value: PropertyValue::Text(_),
        },
        Property::Typed {
            name: PropertyName::Synonym,
            value: synonym,
        },
        Property::Typed {
            name: PropertyName::Comment,
            value: comment,
        },
        Property::Typed {
            name: PropertyName::Type,
            value: PropertyValue::TypeDescription(_),
        },
        remaining @ ..,
    ] = properties.as_slice()
    else {
        return Ok(false);
    };
    if !node.attributes.is_empty()
        || !node.namespaces.is_empty()
        || !matches!(synonym, PropertyValue::Empty | PropertyValue::Localized(_))
        || !matches!(comment, PropertyValue::Empty | PropertyValue::Text(_))
    {
        return Ok(false);
    }
    for property in remaining {
        let name = match property {
            Property::Typed { name, .. } => name.as_str().to_owned(),
            Property::Preserved { fragment } => {
                if !fragment.namespaces.is_empty() {
                    return Ok(false);
                }
                let wrapped = fragment::wrap(fragment, context)?;
                let document = roxmltree::Document::parse(&wrapped)
                    .map_err(|_| Error::root("source_fragment_xml_invalid"))?;
                let mut elements = document
                    .root_element()
                    .children()
                    .filter(roxmltree::Node::is_element);
                let Some(element) = elements.next() else {
                    return Ok(false);
                };
                if elements.next().is_some() || element.tag_name().namespace() != Some(MD_NAMESPACE)
                {
                    return Ok(false);
                }
                element.tag_name().name().to_owned()
            }
        };
        if !matches!(
            name.as_str(),
            "PasswordMode"
                | "Format"
                | "EditFormat"
                | "ToolTip"
                | "MarkNegatives"
                | "Mask"
                | "MultiLine"
                | "ExtendedEdit"
                | "MinValue"
                | "MaxValue"
                | "FillFromFillingValue"
                | "FillValue"
                | "FillChecking"
                | "ChoiceFoldersAndItems"
                | "ChoiceParameterLinks"
                | "ChoiceParameters"
                | "QuickChoice"
                | "CreateOnInput"
                | "ChoiceForm"
                | "LinkByType"
                | "ChoiceHistoryOnInput"
                | "Use"
                | "Indexing"
                | "FullTextSearch"
                | "DataHistory"
        ) {
            return Ok(false);
        }
    }
    Ok(true)
}
