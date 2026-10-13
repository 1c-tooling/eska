//! Accepted table profiles shared by actual removal and rename consumers.

use super::{
    AcceptedBase, Child, GeneratedIdentity, MetadataNode, NativeUuid, Property, PropertyName,
    PropertyValue, Section, TabularSectionIdentities,
    attribute_test_support::fixture as field_fixture,
    child_creation::attribute_tests::attribute_xml, enum_test_support::uuid,
};
use crate::project_core::MetadataKind;
use crate::test_support::TestDir;
use std::{fs, path::Path};
const BOOLEAN: &str = "<v8:Type>xs:boolean</v8:Type>";

/// Native table/row declarations derive their names from one parent/table Name and have four unique UUID slots.
pub(super) fn table_xml(kind: &str, parent: &str, index: u32, name: &str, columns: &str) -> String {
    let declarations = [("TabularSection", 1000), ("TabularSectionRow", 1002)]
        .map(|(role, offset)| {
            let type_id = uuid(index * 10 + offset);
            let value_id = uuid(index * 10 + offset + 1);
            format!(
                "<xr:GeneratedType name=\"{kind}{role}.{parent}.{name}\" category=\"{role}\"><xr:TypeId>{}</xr:TypeId><xr:ValueId>{}</xr:ValueId></xr:GeneratedType>",
                type_id.as_str(), value_id.as_str()
            )
        })
        .concat();
    format!(
        "<TabularSection uuid=\"{}\"><InternalInfo>{declarations}</InternalInfo><Properties><Name>{name}</Name><Synonym/><Comment/><ToolTip/><FillChecking>DontCheck</FillChecking>{}<LineNumberLength>5</LineNumberLength></Properties><ChildObjects>{columns}</ChildObjects></TabularSection>",
        uuid(index).as_str(),
        if kind == "Catalog" {
            "<Use>ForItem</Use>"
        } else {
            ""
        }
    )
}

/// Known service declarations replace only this test's inert table data before independently pinning it.
pub(super) fn fixture(
    test: &TestDir,
    readable: bool,
    frozen: &[NativeUuid],
    adjust: impl FnOnce(&Path),
) -> (std::path::PathBuf, AcceptedBase) {
    field_fixture(test, readable, frozen, |input| {
        for (path, kind, parent, index, first, last) in [
            ("Catalogs/Records.xml", "Catalog", "Records", 13, 21, 22),
            ("Documents/Entry.xml", "Document", "Entry", 14, 41, 42),
        ] {
            let file = input.join(path);
            let native = fs::read_to_string(&file).unwrap();
            let start = native
                .find(&format!(
                    "<TabularSection uuid=\"{}\"",
                    uuid(index).as_str()
                ))
                .unwrap();
            let end = start
                + native[start..].find("</TabularSection>").unwrap()
                + "</TabularSection>".len();
            let columns = attribute_xml(
                first,
                if first == 21 {
                    "FirstColumn"
                } else {
                    "FirstDocumentColumn"
                },
                BOOLEAN,
            ) + &attribute_xml(
                last,
                if last == 22 {
                    "LastColumn"
                } else {
                    "LastDocumentColumn"
                },
                BOOLEAN,
            );
            let mut changed = native.clone();
            changed.replace_range(
                start..end,
                &table_xml(kind, parent, index, "Rows", &columns),
            );
            if index == 13 {
                let start = changed
                    .find(&format!("<TabularSection uuid=\"{}\"", uuid(13).as_str()))
                    .unwrap();
                let end = start
                    + changed[start..].find("</TabularSection>").unwrap()
                    + "</TabularSection>".len();
                changed.insert_str(
                    end,
                    &table_xml(
                        kind,
                        parent,
                        15,
                        "RemainingRows",
                        &attribute_xml(23, "ExtraColumn", BOOLEAN),
                    ),
                );
            }
            fs::write(file, changed).unwrap();
        }
        adjust(input);
    })
}

/// Every authored name has one typed value; native declaration names follow the real parent.
pub(super) fn fresh_table(index: u32, name: &str, columns: Vec<Child>) -> Child {
    Child::Inline {
        object: Box::new(MetadataNode {
            kind: MetadataKind::TabularSection,
            uuid: uuid(index),
            namespaces: Vec::new(),
            attributes: Vec::new(),
            sections: vec![
                Section::TabularSectionIdentities(TabularSectionIdentities {
                    prefix: "xr".into(),
                    table: GeneratedIdentity {
                        type_id: uuid(index + 1000),
                        value_id: uuid(index + 1001),
                    },
                    row: GeneratedIdentity {
                        type_id: uuid(index + 1002),
                        value_id: uuid(index + 1003),
                    },
                }),
                Section::Properties(vec![
                    Property::Typed {
                        name: PropertyName::Name,
                        value: PropertyValue::Text(name.into()),
                    },
                    Property::Typed {
                        name: PropertyName::Synonym,
                        value: PropertyValue::Empty,
                    },
                    Property::Typed {
                        name: PropertyName::Comment,
                        value: PropertyValue::Empty,
                    },
                ]),
                Section::Children(columns),
            ],
        }),
    }
}
