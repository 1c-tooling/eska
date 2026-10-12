use crate::project_core::source_format::{
    Descriptor, PropertyName, PropertyValue, TypeAlternativeKind,
};

use super::super::{Step, replace_direct};

/// Alternative names and categories are editable leaves, while their slots and cardinality stay fixed.
#[test]
fn type_alternatives_keep_styles_and_refuse_structural_changes() {
    let native = r#"<MetaDataObject xmlns="http://v8.1c.ru/8.3/MDClasses" xmlns:v8="http://v8.1c.ru/8.1/data/core" xmlns:xs="http://www.w3.org/2001/XMLSchema" xmlns:cfg="http://v8.1c.ru/8.1/data/enterprise/current-config" version="2.20"><DefinedType uuid="10000000-0000-4000-8000-000000000001"><Properties><Name>Sample</Name><Type><v8:Type>xs:decimal</v8:Type><v8:TypeSet>cfg:DefinedType.Original</v8:TypeSet></Type></Properties></DefinedType></MetaDataObject>"#;
    let descriptor = Descriptor::from_xml(native).unwrap();
    let previous = descriptor.object.property(PropertyName::Type).unwrap();
    let mut next = previous.clone();
    let PropertyValue::TypeDescription(value) = &mut next else {
        panic!("typed description")
    };
    value.data.alternatives[0].name = "xs:string".into();
    value.data.alternatives[1].name = "cfg:DefinedType.Changed".into();
    value.data.alternatives[1].kind = TypeAlternativeKind::Type;
    let input = "Type:\r\n  alternatives:\r\n    - kind: type # first\r\n      name: 'xs:decimal' # builtin\r\n    - kind: type_set # set\r\n      name: 'cfg:DefinedType.Original' # named\r\nunrelated: 'keep'\r\n";
    let steps = [Step::Key("Type")];
    let updated = replace_direct(input, &steps, previous, &next).unwrap();
    assert_eq!(
        updated,
        input
            .replace("'xs:decimal'", "'xs:string'")
            .replace("'cfg:DefinedType.Original'", "'cfg:DefinedType.Changed'")
            .replace("kind: type_set # set", "kind: type # set")
    );
    assert_eq!(
        replace_direct(&updated, &steps, &next, &next).unwrap(),
        updated
    );
    let PropertyValue::TypeDescription(value) = &mut next else {
        panic!("typed description")
    };
    value
        .data
        .alternatives
        .push(value.data.alternatives[0].clone());
    assert_eq!(
        replace_direct(input, &steps, previous, &next)
            .unwrap_err()
            .code,
        "source_edit_value_unsupported"
    );
}
