//! Both process consumers check exactly the same current metadata bytes.

use eska::project_core::source_format::{
    Child, Namespace, Property, PropertyName, PropertyValue, Section, SourcePath, SourceReader,
    XmlFragment,
};
use std::path::Path;

pub const OWNER: &str = "CommonModules/Tools/object.yaml";

/// A non-BMP prefix and compact YAML-compatible JSON distinguish UTF-16 from bytes and Unicode scalars.
pub fn text(root: &Path, parse_issue: bool) -> String {
    let source = root.join("src");
    let path = SourcePath::parse(OWNER).unwrap();
    let mut owner = SourceReader::open(&source)
        .unwrap()
        .read_owner(&path)
        .unwrap()
        .source;
    let properties = owner
        .document
        .descriptor
        .object
        .sections
        .iter_mut()
        .find_map(|section| {
            if let Section::Properties(properties) = section {
                Some(properties)
            } else {
                None
            }
        })
        .unwrap();
    properties.insert(
        0,
        Property::Typed {
            name: PropertyName::Comment,
            value: PropertyValue::Text("Я😀 current coordinates".into()),
        },
    );
    if parse_issue {
        owner
            .document
            .descriptor
            .object
            .sections
            .push(Section::Children(vec![Child::Preserved {
                fragment: XmlFragment {
                    xml: "<future:Child/>".into(),
                    namespaces: vec![Namespace {
                        prefix: Some("future".into()),
                        uri: "urn:future".into(),
                    }],
                },
            }]));
    }
    let text = serde_json::to_string(&owner).unwrap();
    let text = if parse_issue {
        text
    } else {
        text.replace("<Server>true</Server>", "<Server>Future</Server>")
    };
    format!("# Я😀 current check\r\n{text}")
}
