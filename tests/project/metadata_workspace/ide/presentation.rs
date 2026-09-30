//! End-to-end reference/type presentation through an independent stdio client.

use super::{Client, TestDir, Value, context, fixture, json};
use std::fs;

/// Find an original property by its XML identity, retaining all source fields for assertions.
fn property<'a>(value: &'a Value, name: &str) -> &'a Value {
    value["properties"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["key"]["name"] == name)
        .unwrap()
}

/// A self-contained descriptor avoids depending on unrelated fixture contents.
fn descriptor(kind: &str, name: &str, properties: &str, children: &str) -> String {
    format!(
        r#"<MetaDataObject xmlns="http://v8.1c.ru/8.3/MDClasses" xmlns:v="http://v8.1c.ru/8.1/data/core" xmlns:t="http://www.w3.org/2001/XMLSchema-instance" xmlns:s="http://www.w3.org/2001/XMLSchema" xmlns:c="http://v8.1c.ru/8.1/data/enterprise/current-config" xmlns:r="http://v8.1c.ru/8.3/xcf/readable"><{kind} uuid="11111111-1111-1111-1111-111111111111"><Properties><Name>{name}</Name>{properties}</Properties><ChildObjects>{children}</ChildObjects></{kind}></MetaDataObject>"#
    )
}

/// Mix valid, missing and malformed targets without modifying shared fixture files.
fn write_reference_fixture(source: &std::path::Path) {
    fs::create_dir_all(source.join("CommonForms")).unwrap();
    fs::create_dir_all(source.join("Catalogs")).unwrap();
    fs::write(source.join("CommonForms/Main.xml"), descriptor("CommonForm", "Main",
        "<Synonym><v:item><v:lang>ru</v:lang><v:content>Форма отчета</v:content></v:item><v:item><v:lang>en</v:lang><v:content>Report form</v:content></v:item></Synonym>", "")).unwrap();
    fs::write(source.join("Catalogs/Companies.xml"), descriptor("Catalog", "Companies", "<Synonym><v:item><v:lang>ru</v:lang><v:content>Организации</v:content></v:item></Synonym>", "")).unwrap();
    let fields = r#"<Comment>CommonForm.Main</Comment><DefaultReportForm>CommonForm.Main</DefaultReportForm>
      <DefaultReportVariantForm>CommonForm.Absent</DefaultReportVariantForm>
      <DefaultSettingsForm>CommonForm.Broken</DefaultSettingsForm>
      <FillValue t:nil="true"/><MinValue t:nil="1"/><MaxValue t:nil="false"/>
      <EmptyString t:type="s:string"/><Content><r:Item t:type="r:MDObjectRef">Catalog.Companies</r:Item></Content>
      <Type><v:Type>c:CatalogRef.Companies</v:Type><v:Type>s:string</v:Type><v:StringQualifiers><v:Length>100</v:Length><v:AllowedLength>Variable</v:AllowedLength></v:StringQualifiers></Type>
      <Unknown><v:Type>c:CatalogRef.Companies</v:Type></Unknown>"#;
    fs::write(source.join("CommonForms/Broken.xml"), "<broken>").unwrap();
    fs::write(source.join("Configuration.xml"), descriptor("Configuration", "Test", fields,
        "<CommonForm>Main</CommonForm><CommonForm>Broken</CommonForm><Catalog>Companies</Catalog>")).unwrap();
}

/// No index or tree walk is needed to present a reference and navigate its declared target.
#[test]
fn process_semantic_properties_keep_raw_values_and_both_locales() {
    let directory = TestDir::new();
    fixture(&directory.0, "configuration");
    let source = directory.0.join("src");
    write_reference_fixture(&source);
    let mut expected = None;
    for locale in ["ru-RU", "en-US"] {
        let mut client = Client::new(locale);
        let handshake = client.initialize(locale);
        assert_eq!(handshake["capabilities"]["propertyPresentation"], true);
        let open = client.open(&directory.0);
        let args = context(
            &open,
            json!({"objectId":open["projects"][0]["root"]["objectId"]}),
        );
        let result = client.ok("metadata/properties", args);
        if let Some(expected) = &expected {
            assert_eq!(&result["properties"], expected);
        }
        expected = Some(result["properties"].clone());
        let reference = property(&result, "DefaultReportForm");
        assert_eq!(reference["value"]["text"], "CommonForm.Main");
        assert_eq!(
            reference["presentation"]["caption"]["ru-RU"],
            "Форма отчета"
        );
        assert_eq!(reference["presentation"]["caption"]["en-US"], "Report form");
        assert_eq!(
            reference["presentation"]["category"]["ru-RU"],
            "Общая форма"
        );
        assert_eq!(
            property(&result, "DefaultReportVariantForm")["presentation"]["status"],
            "missing"
        );
        assert_eq!(
            property(&result, "DefaultSettingsForm")["presentation"]["status"],
            "unavailable"
        );
        assert_eq!(
            property(&result, "FillValue")["presentation"]["caption"]["ru-RU"],
            "Не задано"
        );
        assert_eq!(
            property(&result, "MinValue")["presentation"]["kind"],
            "empty"
        );
        assert!(property(&result, "MaxValue").get("presentation").is_none());
        assert_eq!(
            property(&result, "EmptyString")["presentation"]["caption"]["en-US"],
            "Empty string"
        );
        assert!(property(&result, "Comment").get("presentation").is_none());
        assert!(property(&result, "Unknown").get("presentation").is_none());
        assert_eq!(
            property(&result, "Content")["value"]["fields"][0]["presentation"]["caption"]["ru-RU"],
            "Организации"
        );
        let types = &property(&result, "Type")["presentation"];
        assert_eq!(types["kind"], "types");
        assert_eq!(types["items"][0]["caption"]["ru-RU"], "Организации");
        assert_eq!(types["items"][1]["caption"]["ru-RU"], "Строка");
        assert!(
            types["items"][1]["detail"]["ru-RU"]
                .as_str()
                .unwrap()
                .contains("100")
        );
        assert_eq!(
            property(&result, "Type")["value"]["fields"]
                .as_array()
                .unwrap()
                .len(),
            3
        );
        let reveal = client.ok(
            "metadata/reveal",
            context(
                &open,
                json!({"objectId":reference["presentation"]["target"]}),
            ),
        );
        assert_eq!(
            reveal["ancestry"].as_array().unwrap().last().unwrap()["objectId"],
            reference["presentation"]["target"]
        );
        client.finish();
    }
}

/// Unsafe guesses and future type qualifiers remain visible through the original structure.
#[test]
fn process_unknown_type_shapes_and_foreign_annotations_stay_raw() {
    let directory = TestDir::new();
    fixture(&directory.0, "configuration");
    for fields in [
        r"<Type><v:Type>c:CommonFormManager.Main</v:Type></Type>",
        r"<Type><v:Type>s:string</v:Type><v:StringQualifiers><v:Length>100</v:Length><v:Future>keep</v:Future></v:StringQualifiers></Type>",
        r#"<Type><v:Type xmlns:c="urn:foreign">c:CatalogRef.Companies</v:Type></Type>"#,
        r"<Type><v:Type>s:string</v:Type><v:DateQualifiers><v:DateFractions>Date</v:DateFractions></v:DateQualifiers></Type>",
        r#"<Type><v:Type extra="keep">s:string</v:Type></Type>"#,
        r#"<Type xmlns:q="urn:foreign" q:nil="true"/>"#,
    ] {
        fs::write(
            directory.0.join("src/Configuration.xml"),
            descriptor("Configuration", "Test", fields, ""),
        )
        .unwrap();
        let mut client = Client::new("ru-RU");
        client.initialize("ru-RU");
        let open = client.open(&directory.0);
        let result = client.ok(
            "metadata/properties",
            context(
                &open,
                json!({"objectId":open["projects"][0]["root"]["objectId"]}),
            ),
        );
        assert!(
            property(&result, "Type").get("presentation").is_none(),
            "{result}"
        );
        client.finish();
    }
}
