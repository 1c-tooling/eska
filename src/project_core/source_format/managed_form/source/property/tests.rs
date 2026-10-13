//! Typed property edits retain scope, representation and immutable inverse bindings.

use super::{FormLocalizedText, FormPropertyName as Name, FormPropertyValue as Value};
use crate::project_core::source_format::{FormId, ManagedFormSource};

/// A native field has every accepted property, including empty tooltip content and a formatted title.
fn xml() -> String {
    super::super::super::tests::XML
        .replace("version=\"2.20\"", "xmlns:v8=\"http://v8.1c.ru/8.1/data/core\" version=\"2.20\"")
        .replace("<InputField name=\"Value\" id=\"2\">", "<InputField name=\"Value\" id=\"2\"><Title formatted=\"false\"><v8:item><v8:lang>ru</v8:lang><v8:content>Исходный Я 😀 &amp; &lt;</v8:content></v8:item></Title><ToolTip><v8:item><v8:lang>ru</v8:lang><v8:content/></v8:item></ToolTip><Visible>false</Visible><ReadOnly>true</ReadOnly><Enabled>false</Enabled>")
}

/// UI and data namespaces may share a decimal spelling without sharing a property scope.
fn id(value: &str) -> FormId {
    FormId::parse(value).unwrap()
}

/// Localized value changes retain their original item keys and explicit formatted-flag presence.
fn localized(value: &mut Value) -> &mut FormLocalizedText {
    let Value::Localized(value) = value else {
        panic!("localized fixture")
    };
    value
}

/// All accepted values appear once in authoring and disappear from pinned opaque fragments.
#[test]
fn promoted_properties_have_one_authoritative_value_and_exact_noop() {
    let input = xml();
    let source = ManagedFormSource::from_xml(&input).unwrap();
    let properties = &source.elements[&id("2")].properties;
    assert_eq!(properties.len(), 5);
    assert_eq!(properties[&Name::Visible], Value::Bool(false));
    assert_eq!(properties[&Name::ReadOnly], Value::Bool(true));
    let Value::Localized(title) = &properties[&Name::Title] else {
        panic!("title")
    };
    assert_eq!(title.text["ru"], "Исходный Я 😀 & <");
    assert_eq!(title.formatted, Some(false));
    let text = source.to_yaml().unwrap();
    assert_eq!(text.matches("Исходный Я 😀").count(), 1);
    assert_eq!(
        ManagedFormSource::from_yaml(&text)
            .unwrap()
            .to_xml()
            .unwrap(),
        input
    );
}

/// Boolean, formatted and localized edits use typed values while leaving all native frames unchanged.
#[test]
fn property_changes_keep_slots_identity_and_xml_text_semantics() {
    let accepted = ManagedFormSource::from_xml(&xml()).unwrap();
    let mut current = accepted.clone();
    let properties = &mut current.elements.get_mut(&id("2")).unwrap().properties;
    for (name, value) in [
        (Name::Visible, true),
        (Name::ReadOnly, false),
        (Name::Enabled, true),
    ] {
        properties.insert(name, Value::Bool(value));
    }
    let title = localized(properties.get_mut(&Name::Title).unwrap());
    title.formatted = Some(true);
    title
        .text
        .insert("ru".into(), "<b>Новый Я 😎</b>\r\n&\"".into());
    localized(properties.get_mut(&Name::ToolTip).unwrap())
        .text
        .insert("ru".into(), "Подсказка".into());
    current.validate_change(&accepted).unwrap();
    assert_eq!(
        current.validate_layout_change(&accepted).unwrap_err().code,
        "source_form_identity_changed"
    );
    let output = current.to_xml().unwrap();
    assert!(output.contains("<Visible>true</Visible>"));
    assert!(output.contains("<ReadOnly>false</ReadOnly>"));
    assert!(output.contains("<Title formatted=\"true\">"));
    assert!(output.contains("&lt;b&gt;Новый Я 😎&lt;/b&gt;&#13;\n&amp;\""));
    let reread = ManagedFormSource::from_xml(&output).unwrap();
    assert_eq!(reread.elements[&id("2")], current.elements[&id("2")]);
}

/// Root and element properties remain distinct even when their caption text happens to be equal.
#[test]
fn coincident_root_and_element_titles_do_not_share_authority() {
    let caption =
        "<Title><v8:item><v8:lang>ru</v8:lang><v8:content>Same</v8:content></v8:item></Title>";
    let input = xml()
        .replace(
            "<AutoCommandBar name=\"\"",
            &format!("{caption}<AutoCommandBar name=\"\""),
        )
        .replace(
            "<UsualGroup name=\"Second\" id=\"5\"/>",
            &format!("<UsualGroup name=\"Second\" id=\"5\">{caption}</UsualGroup>"),
        );
    let accepted = ManagedFormSource::from_xml(&input).unwrap();
    let mut current = accepted.clone();
    localized(current.properties.get_mut(&Name::Title).unwrap())
        .text
        .insert("ru".into(), "Root only".into());
    current.validate_change(&accepted).unwrap();
    let output = current.to_xml().unwrap();
    assert_eq!(output.matches(">Same<").count(), 1);
    assert_eq!(output.matches(">Root only<").count(), 1);
}

/// Missing fields/languages/flags and type changes cannot manufacture a new native inverse slot.
#[test]
fn property_binding_shape_is_immutable() {
    let accepted = ManagedFormSource::from_xml(&xml()).unwrap();
    let mut current = accepted.clone();
    current
        .elements
        .get_mut(&id("2"))
        .unwrap()
        .properties
        .remove(&Name::Visible);
    assert_eq!(
        current.to_xml().unwrap_err().code,
        "source_form_property_binding_invalid"
    );
    let mut current = accepted.clone();
    let properties = &mut current.elements.get_mut(&id("2")).unwrap().properties;
    localized(properties.get_mut(&Name::Title).unwrap())
        .text
        .insert("en".into(), "New language".into());
    assert_eq!(
        current.to_xml().unwrap_err().code,
        "source_form_property_binding_invalid"
    );
    let mut current = accepted.clone();
    localized(
        current
            .elements
            .get_mut(&id("2"))
            .unwrap()
            .properties
            .get_mut(&Name::Title)
            .unwrap(),
    )
    .formatted = None;
    assert_eq!(
        current.to_xml().unwrap_err().code,
        "source_form_property_binding_invalid"
    );
    let mut current = accepted;
    current
        .elements
        .get_mut(&id("2"))
        .unwrap()
        .properties
        .insert(Name::Title, Value::Bool(true));
    assert_eq!(
        current.to_xml().unwrap_err().code,
        "source_form_property_binding_invalid"
    );
}

/// Unverified localized/lexical/attribute shapes stay wholly opaque and never gain duplicate editable values.
#[test]
fn unsupported_property_shapes_are_preserved_and_uneditable() {
    for (before, after) in [
        ("<Visible>false</Visible>", "<Visible>0</Visible>"),
        (
            "<Visible>false</Visible>",
            "<Visible custom=\"true\">false</Visible>",
        ),
        (
            "<Visible>false</Visible>",
            "<Visible>false</Visible><Visible>true</Visible>",
        ),
    ] {
        let input = xml().replace(before, after);
        let source = ManagedFormSource::from_xml(&input).unwrap();
        assert!(
            !source.elements[&id("2")]
                .properties
                .contains_key(&Name::Visible)
        );
        assert_eq!(source.to_xml().unwrap(), input);
    }
    let input = xml().replace("Исходный Я 😀 &amp; &lt;", "<![CDATA[Unverified < &]]>");
    let source = ManagedFormSource::from_xml(&input).unwrap();
    assert!(
        !source.elements[&id("2")]
            .properties
            .contains_key(&Name::Title)
    );
    assert_eq!(source.to_xml().unwrap(), input);
}

/// Empty formatted titles have a flag binding but no implicit locale or text item.
#[test]
fn empty_formatted_titles_keep_absence_distinct_from_an_item() {
    let input = xml().replace(
        "<UsualGroup name=\"Second\" id=\"5\"/>",
        "<UsualGroup name=\"Second\" id=\"5\"><Title formatted=\"false\"/></UsualGroup>",
    );
    let accepted = ManagedFormSource::from_xml(&input).unwrap();
    let mut current = accepted.clone();
    let value = localized(
        current
            .elements
            .get_mut(&id("5"))
            .unwrap()
            .properties
            .get_mut(&Name::Title)
            .unwrap(),
    );
    assert!(value.text.is_empty());
    value.formatted = Some(true);
    current.validate_change(&accepted).unwrap();
    assert!(
        current
            .to_xml()
            .unwrap()
            .contains("<Title formatted=\"true\"/>")
    );
}

/// Invalid XML characters fail semantic output validation without mutating any accepted data.
#[test]
fn invalid_xml_text_is_refused_before_materialization() {
    let mut current = ManagedFormSource::from_xml(&xml()).unwrap();
    localized(
        current
            .elements
            .get_mut(&id("2"))
            .unwrap()
            .properties
            .get_mut(&Name::Title)
            .unwrap(),
    )
    .text
    .insert("ru".into(), "Invalid\0text".into());
    assert_eq!(
        current.to_xml().unwrap_err().code,
        "source_form_xml_invalid"
    );
}

/// Published body v1 remains readable without implicitly promoting its opaque properties into a new schema.
#[test]
fn legacy_body_schema_stays_readable_and_cannot_smuggle_v2_fields() {
    let text = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/docs/research/eska-format/managed-form-body.example.yaml"
    ));
    let source = ManagedFormSource::from_yaml(text).unwrap();
    assert_eq!(source.schema_version, 1);
    assert_eq!(
        ManagedFormSource::from_yaml(&source.to_yaml().unwrap()).unwrap(),
        source
    );
    assert_eq!(
        ManagedFormSource::from_yaml(&format!("properties: {{}}\n{text}"))
            .unwrap_err()
            .code,
        "source_form_schema_invalid"
    );
    let mut current = ManagedFormSource::from_xml(&xml()).unwrap();
    current.schema_version = 1;
    assert_eq!(
        current.to_xml().unwrap_err().code,
        "source_form_schema_invalid"
    );
}
