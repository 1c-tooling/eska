use super::*;
use crate::project::metadata_model::{MetadataValue, ValueIssue};

/// Wrap fragments in the same namespace envelope as Designer descriptors.
fn descriptor(body: &str) -> String {
    format!(
        "\u{feff}<m:MetaDataObject xmlns:m=\"{MD_NAMESPACE}\" xmlns:v=\"http://v8.1c.ru/8.1/data/core\" version=\"2.20\">\r\n{body}\r\n</m:MetaDataObject>"
    )
}

/// Emit an inline test object with valid required properties.
fn object(tag: &str, name: &str, children: &str) -> String {
    format!(
        "<m:{tag} uuid=\"shared\"><m:Properties><m:Name>{name}</m:Name></m:Properties><m:ChildObjects>{children}</m:ChildObjects></m:{tag}>"
    )
}

/// IDs, references, declared order and byte ranges remain independent of prefixes and BOM.
#[test]
fn summary_keeps_inline_objects_and_unopened_references() {
    let xml = descriptor(&object(
        "Catalog",
        "Клиенты",
        &format!(
            "{}<m:Form>Основная</m:Form>",
            object(
                "TabularSection",
                "Контакты",
                &object("Attribute", "Телефон", "")
            )
        ),
    ));
    let parsed = parse(&xml, None, PropertiesMode::Summary).unwrap();
    assert!(parsed.diagnostics.is_empty());
    assert_eq!(parsed.objects.len(), 3);
    assert_eq!(
        parsed.objects[2].metadata.id().as_str(),
        "catalog:Клиенты/tabular-section:Контакты/attribute:Телефон"
    );
    assert!(
        parsed
            .objects
            .iter()
            .all(|object| object.properties.is_none())
    );
    assert_eq!(
        parsed.references[0].id.as_str(),
        "catalog:Клиенты/form:Основная"
    );
    assert_eq!(parsed.objects[0].children.len(), 2);
    assert_eq!(
        &xml[parsed.references[0].range.clone()],
        "<m:Form>Основная</m:Form>"
    );
    assert!(xml[parsed.objects[2].range.clone()].starts_with("<m:Attribute"));
    let root = parse(
        &descriptor(&object(
            "Configuration",
            "Проект",
            "<m:Catalog>Клиенты</m:Catalog>",
        )),
        None,
        PropertiesMode::Summary,
    )
    .unwrap();
    assert_eq!(root.references[0].id.as_str(), "catalog:Клиенты");
    assert_eq!(root.references[0].owner.as_str(), "configuration:Проект");
}

/// Unknown qualified fields, repeated record values and decoded text are preserved.
#[test]
fn properties_keep_values_and_explicitly_mark_unsupported_shapes() {
    let xml = descriptor(
        r#"<m:Catalog uuid="id"><m:Properties><m:Name>Клиенты</m:Name><m:Synonym><v:item><v:lang>ru</v:lang><v:content>Клиенты &amp; партнёры</v:content></v:item></m:Synonym><m:Unknown xmlns:x="urn:extra" x:flag="yes"><x:Value>1</x:Value><x:Value>2</x:Value></m:Unknown><m:Text>A<!-- split --><![CDATA[B]]>&amp;C</m:Text><m:Mixed>before<m:X/>after</m:Mixed></m:Properties></m:Catalog>"#,
    );
    let parsed = parse(&xml, None, PropertiesMode::All).unwrap();
    let object = &parsed.objects[0];
    assert_eq!(object.synonyms[0].content, "Клиенты & партнёры");
    let properties = object.properties.as_ref().unwrap();
    assert_eq!(
        properties[2].property.qualifiers[0].0.namespace.as_deref(),
        Some("urn:extra")
    );
    let MetadataValue::Record(values) = &properties[2].property.value else {
        panic!("record")
    };
    assert_eq!(values.len(), 2);
    assert_eq!(values[0].key, values[1].key);
    assert_eq!(
        properties[3].property.value,
        MetadataValue::Text("AB&C".into())
    );
    assert_eq!(
        properties[4].property.value,
        MetadataValue::Unsupported(ValueIssue::MixedContent)
    );
    assert_eq!(parsed.diagnostics.len(), 1);
    assert!(xml[parsed.diagnostics[0].range.clone()].starts_with("<m:Mixed>"));
}

/// A bad child does not hide valid siblings or masquerade as an empty successful branch.
#[test]
fn local_errors_retain_supported_siblings() {
    let xml = descriptor(&object(
        "Catalog",
        "Клиенты",
        "<m:Future>Неизвестный</m:Future><x:Attribute xmlns:x='urn:foreign'>Чужой</x:Attribute><m:Attribute uuid='id'><m:Properties/></m:Attribute><m:Form>Одна</m:Form><m:Form>Одна</m:Form><m:Form>Другая</m:Form>",
    ));
    let parsed = parse(&xml, None, PropertiesMode::Summary).unwrap();
    assert_eq!(parsed.diagnostics.len(), 4);
    assert_eq!(parsed.references.len(), 2);
    assert!(matches!(
        &parsed.diagnostics[3].issue,
        ParseIssue::DuplicateIdentity(_)
    ));
    for fragment in ["<m:Future/>", "<m:Catalog/>", "<m:Catalog/><m:Catalog/>"] {
        assert!(matches!(
            parse(&descriptor(fragment), None, PropertiesMode::Summary),
            Err(ParseError::Invalid(_))
        ));
    }
}

/// Empty synonyms are valid; malformed or duplicate localized entries are not partial success.
#[test]
fn synonym_validation_also_runs_in_summary_mode() {
    for value in [
        "text",
        "<v:item/>",
        "<v:item>lost<v:lang>ru</v:lang><v:content>A</v:content></v:item>",
        "<v:item><v:lang>ru</v:lang><v:content>A</v:content><v:other/></v:item>",
    ] {
        let xml = descriptor(&format!(
            "<m:Catalog uuid='id'><m:Properties><m:Name>A</m:Name><m:Synonym>{value}</m:Synonym></m:Properties></m:Catalog>"
        ));
        let parsed = parse(&xml, None, PropertiesMode::Summary).unwrap();
        assert!(parsed.objects[0].synonyms.is_empty());
        assert_eq!(parsed.diagnostics.len(), 1);
    }
}

/// XML security and resource bounds apply before recursive metadata/value extraction.
#[test]
fn rejects_excessive_depth_size_nodes_and_dtd() {
    // The supported boundary must remain safe on the test runner's default stack.
    let boundary = format!("{}{}", "<n>".repeat(MAX_DEPTH), "</n>".repeat(MAX_DEPTH));
    assert!(matches!(
        parse(&boundary, None, PropertiesMode::All),
        Err(ParseError::Invalid(_))
    ));
    let xml = format!(
        "{}{}",
        "<n>".repeat(MAX_DEPTH + 1),
        "</n>".repeat(MAX_DEPTH + 1)
    );
    assert!(matches!(
        parse(&xml, None, PropertiesMode::All),
        Err(ParseError::TooDeep)
    ));
    assert!(matches!(
        parse(&" ".repeat(MAX_BYTES + 1), None, PropertiesMode::Summary),
        Err(ParseError::TooLarge)
    ));
    let xml = format!("<n>{}</n>", "<n/>".repeat(1_000_001));
    assert!(matches!(
        parse(&xml, None, PropertiesMode::Summary),
        Err(ParseError::Xml(_))
    ));
    for xml in [
        "<!DOCTYPE n [<!ENTITY x 'value'>]><n>&x;</n>",
        "<n>&#x1;</n>",
        "<n a='1' a='2'/>",
    ] {
        assert!(matches!(
            parse(xml, None, PropertiesMode::Summary),
            Err(ParseError::Xml(_))
        ));
    }
}

/// Every registry kind accepts the common identity envelope; placement belongs to the schema.
#[test]
fn all_metadata_kinds_share_the_descriptor_identity_contract() {
    for &kind in MetadataKind::ALL {
        let tag = format!("{kind:?}");
        let parsed = parse(
            &descriptor(&object(&tag, "Пример", "")),
            None,
            PropertiesMode::All,
        )
        .unwrap();
        assert!(parsed.diagnostics.is_empty());
        assert_eq!(parsed.objects[0].metadata.kind(), kind);
        assert_eq!(
            parsed.objects[0].metadata.id().as_str(),
            format!("{}:Пример", kind.as_str())
        );
    }
}
