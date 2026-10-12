//! Unknown list envelopes stay whole native fragments instead of partly editable records.

use roxmltree::Node;

use super::{
    MetadataReference, MetadataReferenceLayout, MetadataReferenceList, READABLE_NAMESPACE,
    XSI_NAMESPACE,
};
use crate::project_core::source_format::{
    Namespace,
    xml::{plain_element_name, typed_elements},
};

/// Promote only homogeneous `xr:Item` lists with one namespace-resolved `MDObjectRef` annotation.
pub(in crate::project_core::source_format) fn decode_list(
    node: Node<'_, '_>,
    input: &str,
    context: &[Namespace],
) -> Option<MetadataReferenceList> {
    plain_element_name(node, input)?;
    let mut result: Option<MetadataReferenceList> = None;
    for child in typed_elements(node)? {
        if !child.has_tag_name((READABLE_NAMESPACE, "Item"))
            || child.attributes().len() != 1
            || !child.children().all(|part| part.is_text())
        {
            return None;
        }
        let head = input
            .get(child.range())?
            .strip_prefix('<')?
            .split_once('>')?
            .0;
        let mut fields = head.split_whitespace();
        let (item_prefix, local) = fields.next()?.split_once(':')?;
        if local != "Item" {
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
        let type_name = child.attribute((XSI_NAMESPACE, "type"))?;
        let (type_prefix, local) = type_name.split_once(':')?;
        if local != "MDObjectRef" {
            return None;
        }
        let layout = MetadataReferenceLayout {
            item_prefix: item_prefix.into(),
            xsi_prefix: xsi_prefix.into(),
            type_prefix: type_prefix.into(),
        };
        let text: String = child.children().filter_map(|part| part.text()).collect();
        let value = MetadataReference::parse(&text).ok()?;
        let list = result.get_or_insert_with(|| MetadataReferenceList {
            items: Vec::new(),
            native: layout.clone(),
        });
        if list.native != layout {
            return None;
        }
        list.items.push(value);
    }
    let result = result?;
    result.validate(context).ok()?;
    Some(result)
}
