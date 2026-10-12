//! Fresh inline objects cannot reuse accepted metadata or native generated type/value identities.

use std::collections::BTreeSet;

use super::super::{
    AcceptedBase, Child, Error, MD_NAMESPACE, MetadataNode, Namespace, NativeUuid, Section,
    composition, fragment, metadata_reference::READABLE_NAMESPACE, validation,
};

/// Legacy opaque declarations reserve their known native UUID slots without changing their source classification.
pub(super) fn accepted_services(base: &AcceptedBase) -> Result<BTreeSet<String>, Error> {
    let mut result = BTreeSet::new();
    for document in base.package.documents.values() {
        services(
            &document.descriptor.object,
            &document.descriptor.xml.namespaces,
            &mut result,
        )?;
    }
    Ok(result)
}

/// Every identity in a closed new subtree is fresh, including columns and its four explicit generated slots.
pub(super) fn validate(
    base: &AcceptedBase,
    node: &MetadataNode,
    services: &BTreeSet<String>,
) -> Result<(), Error> {
    let mut identities = BTreeSet::new();
    composition::identities(node, &mut identities)?;
    for identity in identities {
        if services.contains(&identity) || base.contains_object(&NativeUuid::parse(&identity)?) {
            return Err(Error::root("native_uuid_duplicate"));
        }
    }
    Ok(())
}

/// Only actual generated declaration UUID slots count; arbitrary UUID text in resources is not native identity.
fn services(
    node: &MetadataNode,
    inherited: &[Namespace],
    result: &mut BTreeSet<String>,
) -> Result<(), Error> {
    let context = validation::scope(inherited, &node.namespaces)?;
    for section in &node.sections {
        match section {
            Section::TabularSectionIdentities(identities) => {
                result.extend(identities.uuids().into_iter().map(NativeUuid::key));
            }
            Section::Preserved(value) => {
                let wrapped = fragment::wrap(value, &context)?;
                let document = roxmltree::Document::parse(&wrapped)
                    .map_err(|_| Error::root("source_fragment_xml_invalid"))?;
                let Some(root) = document.root_element().first_element_child() else {
                    continue;
                };
                if !root.has_tag_name((MD_NAMESPACE, "InternalInfo")) {
                    continue;
                }
                for declaration in root
                    .children()
                    .filter(|node| node.has_tag_name((READABLE_NAMESPACE, "GeneratedType")))
                {
                    for slot in declaration.children().filter(|node| {
                        node.has_tag_name((READABLE_NAMESPACE, "TypeId"))
                            || node.has_tag_name((READABLE_NAMESPACE, "ValueId"))
                    }) {
                        if let Some(value) = slot.text()
                            && let Ok(uuid) = NativeUuid::parse(value.trim())
                        {
                            result.insert(uuid.key());
                        }
                    }
                }
            }
            Section::Children(children) => {
                for child in children {
                    if let Child::Inline { object } = child {
                        services(object, &context, result)?;
                    }
                }
            }
            Section::Properties(_) => {}
        }
    }
    Ok(())
}
