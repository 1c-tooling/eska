//! Actual Catalog/Document fields and direct tables for shared structural-profile checks.

use std::{fs, path::Path};

use super::{
    AcceptedBase, Child, MetadataNode, NativeUuid, ObjectSource, Property, PropertyName,
    PropertyValue, Section,
    child_creation::attribute_tests::attribute_xml,
    enum_test_support::{children, uuid},
};
use crate::test_support::TestDir;

const BOOLEAN: &str = "<v8:Type>xs:boolean</v8:Type>";

/// Both roots have accepted attributes and direct tables with independent identities and inert service bytes.
pub(super) fn fixture(
    test: &TestDir,
    readable: bool,
    frozen: &[NativeUuid],
    adjust: impl FnOnce(&Path),
) -> (std::path::PathBuf, AcceptedBase) {
    super::child_creation::attribute_tests::fixture(test, readable, frozen, |input| {
        for (path, table, first, second, first_name, second_name) in [
            (
                "Catalogs/Records.xml",
                13,
                21,
                22,
                "FirstColumn",
                "LastColumn",
            ),
            (
                "Documents/Entry.xml",
                14,
                41,
                42,
                "FirstDocumentColumn",
                "LastDocumentColumn",
            ),
        ] {
            let file = input.join(path);
            let native = fs::read_to_string(&file).unwrap();
            let table = format!(
                "<TabularSection uuid=\"{}\"><InternalInfo><FutureData>Retained table service bytes</FutureData></InternalInfo><Properties><Name>Rows</Name><Synonym/><Comment/></Properties><ChildObjects>{}{}</ChildObjects></TabularSection>",
                uuid(table).as_str(),
                attribute_xml(first, first_name, BOOLEAN),
                attribute_xml(second, second_name, BOOLEAN)
            );
            let fields = if first == 41 {
                attribute_xml(31, "FirstDocumentField", BOOLEAN)
                    + &attribute_xml(32, "LastDocumentField", BOOLEAN)
            } else {
                String::new()
            };
            fs::write(
                file,
                native.replace("</ChildObjects>", &(fields + &table + "</ChildObjects>")),
            )
            .unwrap();
        }
        adjust(input);
    })
}

/// Native parent identity selects one actual inline table and never a coincidental object kind.
pub(super) fn table(owner: &mut ObjectSource, index: u32) -> &mut MetadataNode {
    children(owner)
        .iter_mut()
        .find_map(|child| match child {
            Child::Inline { object } if object.uuid == uuid(index) => Some(object.as_mut()),
            _ => None,
        })
        .unwrap()
}

/// An accepted inline collection remains explicit even after its last column is removed.
pub(super) fn columns(table: &mut MetadataNode) -> &mut Vec<Child> {
    table
        .sections
        .iter_mut()
        .find_map(|section| match section {
            Section::Children(children) => Some(children),
            _ => None,
        })
        .unwrap()
}

/// Independent comments remain selectable when a neighboring structural operation is refused.
pub(super) fn comment(object: &mut MetadataNode, text: &str) {
    for section in &mut object.sections {
        if let Section::Properties(properties) = section {
            for property in properties {
                if let Property::Typed {
                    name: PropertyName::Comment,
                    value,
                } = property
                {
                    *value = PropertyValue::Text(text.into());
                }
            }
        }
    }
}

/// Fixture identity is unique across direct fields and table columns; displayed order is not its authority.
pub(super) fn member(node: &mut MetadataNode, id: u32) -> Option<&mut MetadataNode> {
    if node.uuid == uuid(id) {
        return Some(node);
    }
    for section in &mut node.sections {
        if let Section::Children(children) = section {
            for child in children {
                if let Child::Inline { object } = child
                    && let Some(found) = member(object, id)
                {
                    return Some(found);
                }
            }
        }
    }
    None
}

/// Rename changes one typed Name, retaining every other accepted field or service slot.
pub(super) fn rename(owner: &mut ObjectSource, id: u32, next: &str) {
    let object = member(&mut owner.document.descriptor.object, id).unwrap();
    for section in &mut object.sections {
        if let Section::Properties(properties) = section {
            for property in properties {
                if let Property::Typed {
                    name: PropertyName::Name,
                    value,
                } = property
                {
                    *value = PropertyValue::Text(next.into());
                }
            }
        }
    }
}
