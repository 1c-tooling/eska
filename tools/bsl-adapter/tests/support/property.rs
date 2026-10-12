//! Typed scalar fixtures select native property identity instead of its serialized XML shape.

use eska::project_core::source_format::{ObjectSource, Property, PropertyName, Section};

/// Replace exactly one declared scalar while leaving nested/native fixture data authoritative.
pub fn replace(input: &str, name: PropertyName, replacement: Property) -> String {
    let mut source = ObjectSource::from_yaml(input).unwrap();
    let target = source
        .document
        .descriptor
        .object
        .sections
        .iter_mut()
        .filter_map(|section| {
            if let Section::Properties(items) = section {
                Some(items)
            } else {
                None
            }
        })
        .flatten()
        .find(
            |property| matches!(property, Property::Typed {name: current, ..} if *current == name),
        )
        .unwrap();
    *target = replacement;
    assert_ne!(ObjectSource::from_yaml(input).unwrap(), source);
    serde_json::to_string(&source).unwrap()
}
