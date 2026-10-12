//! Shared enum fixtures serve structural changes through the same public source/base boundaries.

use std::{fs, path::PathBuf};

use super::{
    AcceptedBase, Child, LocalizedText, MetadataNode, NativeUuid, ObjectSource, Property,
    PropertyName, PropertyValue, Section, accept_source, import_designer, import_designer_readable,
    yaml,
};
use crate::{project_core::MetadataKind, test_support::TestDir};

/// Use distinct native identities for the accepted tree and newly authored values.
pub(super) fn uuid(index: u32) -> NativeUuid {
    NativeUuid::parse(&format!("20000000-0000-4000-8000-{index:012}")).unwrap()
}

/// Native evidence keeps BOM and CRLF, without introducing a source mirror into the repository.
pub(super) fn native(kind: &str, index: u32, name: &str, children: &str) -> String {
    format!(
        "\u{feff}<?xml version=\"1.0\" encoding=\"UTF-8\"?>\r\n<MetaDataObject xmlns=\"http://v8.1c.ru/8.3/MDClasses\" xmlns:v8=\"http://v8.1c.ru/8.1/data/core\" version=\"2.20\"><{kind} uuid=\"{}\"><Properties><Name>{name}</Name><Synonym/><Comment/></Properties><ChildObjects>{children}</ChildObjects></{kind}></MetaDataObject>\r\n",
        uuid(index).as_str()
    )
}

/// The profile is the observed enum-value shape; all values have a single authoring copy.
pub(super) fn value(index: u32, name: &str) -> Child {
    Child::Inline {
        object: Box::new(MetadataNode {
            kind: MetadataKind::EnumValue,
            uuid: uuid(index),
            namespaces: Vec::new(),
            attributes: Vec::new(),
            sections: vec![Section::Properties(vec![
                Property::Typed {
                    name: PropertyName::Name,
                    value: PropertyValue::Text(name.into()),
                },
                Property::Typed {
                    name: PropertyName::Synonym,
                    value: PropertyValue::Localized(vec![LocalizedText {
                        language: "ru".into(),
                        text: format!("Значение {name}"),
                    }]),
                },
                Property::Typed {
                    name: PropertyName::Comment,
                    value: PropertyValue::Empty,
                },
            ])],
        }),
    }
}

/// Every input, accepted blob and generation remains within the test's own playground directory.
pub(super) fn fixture(
    test: &TestDir,
    readable: bool,
    frozen: &[NativeUuid],
) -> (PathBuf, AcceptedBase) {
    fixture_with(test, readable, frozen, |_| {})
}

/// Adverse native profiles are introduced only in a private fixture before its independent acceptance.
pub(super) fn fixture_with(
    test: &TestDir,
    readable: bool,
    frozen: &[NativeUuid],
    before_import: impl FnOnce(&std::path::Path),
) -> (PathBuf, AcceptedBase) {
    let input = test.0.join("native");
    fs::create_dir_all(input.join("Enums")).unwrap();
    fs::write(
        input.join("Configuration.xml"),
        native(
            "Configuration",
            1,
            "Sample",
            "<Enum>Status</Enum><Enum>Other</Enum>",
        ),
    )
    .unwrap();
    let children = [("First", 11), ("Second", 12)].map(|(name, index)| {
        format!("<EnumValue uuid=\"{}\"><Properties><Name>{name}</Name><Synonym/><Comment/></Properties></EnumValue>", uuid(index).as_str())
    }).concat();
    fs::write(
        input.join("Enums/Status.xml"),
        native("Enum", 2, "Status", &children),
    )
    .unwrap();
    fs::write(
        input.join("Enums/Other.xml"),
        native("Enum", 3, "Other", ""),
    )
    .unwrap();
    before_import(&input);
    let source = test.0.join("source");
    if readable {
        import_designer_readable(&input, &source).unwrap();
    } else {
        import_designer(&input, &source).unwrap();
    }
    let accepted = test.0.join("accepted");
    let pin = accept_source(&source, &accepted, frozen).unwrap();
    (source, AcceptedBase::open(&accepted, &pin).unwrap())
}

/// Write raw authored values so negative tests exercise the protected selector rather than an editor gate.
pub(super) fn edit(source: &std::path::Path, change: impl FnOnce(&mut ObjectSource)) -> String {
    let path = source.join("Enums/Status/object.yaml");
    let mut owner = ObjectSource::from_yaml(&fs::read_to_string(&path).unwrap()).unwrap();
    change(&mut owner);
    let raw = serde_json::to_value(&owner).unwrap();
    let text = format!(
        "# Keep authored annotation\r\n{}",
        yaml::encode(&raw).unwrap().replace('\n', "\r\n")
    );
    fs::write(path, &text).unwrap();
    text
}

/// The existing native collection is edited once; its service slot has no duplicate editable list.
pub(super) fn children(owner: &mut ObjectSource) -> &mut Vec<Child> {
    owner
        .document
        .descriptor
        .object
        .sections
        .iter_mut()
        .find_map(|section| {
            if let Section::Children(children) = section {
                Some(children)
            } else {
                None
            }
        })
        .unwrap()
}
