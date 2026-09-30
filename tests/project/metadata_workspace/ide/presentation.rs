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

/// Real accounting exports use untyped selectors, field paths and typed data references together.
#[test]
fn process_accounting_selectors_and_nested_references() {
    let directory = TestDir::new();
    fixture(&directory.0, "configuration");
    let source = directory.0.join("src");
    write_reference_fixture(&source);
    for (folder, kind, name) in [
        ("Styles", "Style", "Dark"),
        ("Languages", "Language", "Russian"),
        ("CommonPictures", "CommonPicture", "Logo"),
        ("SettingsStorages", "SettingsStorage", "Reports"),
        ("Enums", "Enum", "Status"),
    ] {
        fs::create_dir_all(source.join(folder)).unwrap();
        let children = if kind == "Enum" {
            "<EnumValue uuid='22222222-2222-2222-2222-222222222222'><Properties><Name>Active</Name></Properties></EnumValue>"
        } else {
            ""
        };
        fs::write(
            source.join(folder).join(format!("{name}.xml")),
            descriptor(kind, name, "", children),
        )
        .unwrap();
    }
    let fields = r"<DefaultStyle>Style.Dark</DefaultStyle><DefaultLanguage>Language.Russian</DefaultLanguage>
        <ReportsVariantsStorage>SettingsStorage.Reports</ReportsVariantsStorage>
        <AuxiliaryReportForm>CommonForm.Main</AuxiliaryReportForm><DefaultInterface/>
        <Picture><r:Ref>CommonPicture.Logo</r:Ref></Picture>
        <InputByString><r:Field>Catalog.Companies.StandardAttribute.Description</r:Field></InputByString>
        <FillValue t:type='r:DesignTimeRef'>Enum.Status.EnumValue.Active</FillValue>
        <Empty t:type='r:DesignTimeRef'>Catalog.Companies.EmptyRef</Empty>
        <Comment>Language.Russian</Comment><Other><r:Ref>CommonPicture.Logo</r:Ref></Other>";
    fs::write(source.join("Configuration.xml"), descriptor("Configuration", "Test", fields,
        "<Style>Dark</Style><Language>Russian</Language><SettingsStorage>Reports</SettingsStorage><CommonForm>Main</CommonForm><Catalog>Companies</Catalog><CommonPicture>Logo</CommonPicture><Enum>Status</Enum>")).unwrap();
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
    for (key, target) in [
        ("DefaultStyle", "style:Dark"),
        ("DefaultLanguage", "language:Russian"),
        ("ReportsVariantsStorage", "settings-storage:Reports"),
        ("AuxiliaryReportForm", "common-form:Main"),
        ("FillValue", "enum:Status/enum-value:Active"),
    ] {
        assert_eq!(
            property(&result, key)["presentation"]["target"],
            target,
            "{key}"
        );
        let reveal = client.ok(
            "metadata/reveal",
            context(&open, json!({"objectId":target})),
        );
        assert_eq!(
            reveal["ancestry"].as_array().unwrap().last().unwrap()["objectId"],
            target
        );
    }
    let picture = &property(&result, "Picture")["value"]["fields"][0];
    assert_eq!(picture["presentation"]["target"], "common-picture:Logo");
    let standard = &property(&result, "InputByString")["value"]["fields"][0]["presentation"];
    assert_eq!(standard["target"], "catalog:Companies");
    assert_eq!(standard["caption"]["ru-RU"], "Наименование");
    assert_eq!(standard["detail"]["ru-RU"], "Организации");
    assert_eq!(
        property(&result, "Empty")["presentation"]["caption"]["en-US"],
        "Empty reference"
    );
    assert_eq!(
        property(&result, "DefaultInterface")["presentation"]["kind"],
        "empty"
    );
    assert!(property(&result, "Comment").get("presentation").is_none());
    assert!(
        property(&result, "Other")["value"]["fields"][0]
            .get("presentation")
            .is_none()
    );
    client.finish();
}

/// Design-time predefined names resolve nested groups and never select an ambiguous match.
#[test]
fn process_predefined_references_are_lazy_and_unambiguous() {
    let directory = TestDir::new();
    fixture(&directory.0, "configuration");
    let source = directory.0.join("src");
    write_reference_fixture(&source);
    fs::create_dir_all(source.join("Catalogs/Companies/Ext")).unwrap();
    fs::write(source.join("Catalogs/Companies/Ext/Predefined.xml"), r#"<PredefinedData xmlns="http://v8.1c.ru/8.3/xcf/predef"><Item><Name>Group</Name><ChildItems><Item><Name>Main</Name><Description>Основная организация</Description></Item><Item><Name>Duplicate</Name></Item></ChildItems></Item><Item><Name>Duplicate</Name></Item></PredefinedData>"#).unwrap();
    let fields = r"<FillValue t:type='r:DesignTimeRef'>Catalog.Companies.Main</FillValue>
        <Missing t:type='r:DesignTimeRef'>Catalog.Companies.Absent</Missing>
        <Ambiguous t:type='r:DesignTimeRef'>Catalog.Companies.Duplicate</Ambiguous>";
    fs::write(
        source.join("Configuration.xml"),
        descriptor(
            "Configuration",
            "Test",
            fields,
            "<Catalog>Companies</Catalog>",
        ),
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
    let target = "catalog:Companies/predefined-item:Group/predefined-item:Main";
    let value = &property(&result, "FillValue")["presentation"];
    assert_eq!(value["target"], target);
    assert_eq!(value["caption"]["ru-RU"], "Основная организация");
    assert_eq!(
        property(&result, "Missing")["presentation"]["status"],
        "missing"
    );
    assert_eq!(
        property(&result, "Ambiguous")["presentation"]["status"],
        "unavailable"
    );
    let reveal = client.ok(
        "metadata/reveal",
        context(&open, json!({"objectId":target})),
    );
    assert_eq!(
        reveal["ancestry"].as_array().unwrap().last().unwrap()["objectId"],
        target
    );
    let item = client.ok(
        "metadata/properties",
        context(&open, json!({"objectId":target})),
    );
    assert_eq!(
        property(&item, "Description")["value"]["text"],
        "Основная организация"
    );
    client.finish();
}

/// `QName` type sets and singleton types coexist without losing primitive constraints or namespace checks.
#[test]
fn process_type_sets_and_typed_enum_values_are_readable() {
    let directory = TestDir::new();
    fixture(&directory.0, "configuration");
    let source = directory.0.join("src");
    write_reference_fixture(&source);
    for (folder, tag, name) in [
        ("DefinedTypes", "DefinedType", "Price"),
        (
            "ChartsOfCharacteristicTypes",
            "ChartOfCharacteristicTypes",
            "Options",
        ),
        ("Constants", "Constant", "Setting"),
        ("BusinessProcesses", "BusinessProcess", "Flow"),
    ] {
        fs::create_dir_all(source.join(folder)).unwrap();
        fs::write(
            source.join(folder).join(format!("{name}.xml")),
            descriptor(tag, name, "", ""),
        )
        .unwrap();
    }
    let fields = r"<Type><v:TypeSet>c:DefinedType.Price</v:TypeSet><v:TypeSet>c:Characteristic.Options</v:TypeSet>
        <v:TypeSet>c:AnyIBRef</v:TypeSet><v:TypeSet>c:DocumentRef</v:TypeSet>
        <v:Type>c:ConstantValueManager.Setting</v:Type><v:Type>c:BusinessProcessRoutePointRef.Flow</v:Type>
        <v:Type>s:string</v:Type><v:StringQualifiers><v:Length>20</v:Length></v:StringQualifiers></Type>
        <FillValue xmlns:e='http://v8.1c.ru/8.1/data/enterprise' t:type='e:AccountType'>Active</FillValue>
        <Other xmlns:e='urn:foreign' t:type='e:AccountType'>Active</Other>
        <XDTOValueType>s:int</XDTOValueType><Picture><r:Ref>StdPicture.Print</r:Ref></Picture>";
    fs::write(source.join("Configuration.xml"), descriptor("Configuration", "Test", fields,
        "<DefinedType>Price</DefinedType><ChartOfCharacteristicTypes>Options</ChartOfCharacteristicTypes><Constant>Setting</Constant><BusinessProcess>Flow</BusinessProcess>")).unwrap();
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
    let items = property(&result, "Type")["presentation"]["items"]
        .as_array()
        .unwrap();
    assert_eq!(items.len(), 7);
    assert_eq!(items[0]["target"], "defined-type:Price");
    assert_eq!(items[1]["target"], "chart-of-characteristic-types:Options");
    assert_eq!(
        items[2]["caption"]["ru-RU"],
        "Любая ссылка информационной базы"
    );
    assert_eq!(items[3]["caption"]["en-US"], "Any document");
    assert!(items[3].get("target").is_none());
    assert_eq!(items[4]["target"], "constant:Setting");
    assert_eq!(items[5]["target"], "business-process:Flow");
    assert!(items[6]["detail"]["ru-RU"].as_str().unwrap().contains("20"));
    assert_eq!(
        property(&result, "FillValue")["value"]["caption"]["ru-RU"],
        "Активный"
    );
    assert!(property(&result, "Other")["value"].get("caption").is_none());
    assert_eq!(
        property(&result, "XDTOValueType")["value"]["caption"]["en-US"],
        "Integer (32-bit)"
    );
    assert_eq!(
        property(&result, "Picture")["value"]["fields"][0]["value"]["caption"]["ru-RU"],
        "Печать"
    );
    client.finish();
}

/// Inline command references use owner XML, while handler links keep the procedure name visible.
#[test]
fn process_inline_commands_and_procedure_selectors() {
    let directory = TestDir::new();
    fixture(&directory.0, "configuration");
    let source = directory.0.join("src");
    write_reference_fixture(&source);
    let command = "<Command uuid='22222222-2222-2222-2222-222222222222'><Properties><Name>Open</Name><Group>FormNavigationPanelImportant</Group></Properties></Command>";
    fs::write(
        source.join("Catalogs/Companies.xml"),
        descriptor("Catalog", "Companies", "", command),
    )
    .unwrap();
    fs::create_dir_all(source.join("CommonModules")).unwrap();
    fs::write(
        source.join("CommonModules/Actions.xml"),
        descriptor("CommonModule", "Actions", "", ""),
    )
    .unwrap();
    fs::create_dir_all(source.join("EventSubscriptions")).unwrap();
    fs::write(
        source.join("EventSubscriptions/OnWrite.xml"),
        descriptor(
            "EventSubscription",
            "OnWrite",
            "<Handler>CommonModule.Actions.AfterWrite</Handler>",
            "",
        ),
    )
    .unwrap();
    fs::write(source.join("Configuration.xml"), descriptor("Configuration", "Test", "<Content><r:Item t:type='r:MDObjectRef'>Catalog.Companies.Command.Open</r:Item></Content>",
        "<Catalog>Companies</Catalog><CommonModule>Actions</CommonModule><EventSubscription>OnWrite</EventSubscription>")).unwrap();
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
    let target = "catalog:Companies/command:Open";
    assert_eq!(
        property(&result, "Content")["value"]["fields"][0]["presentation"]["target"],
        target
    );
    let command = client.ok(
        "metadata/properties",
        context(&open, json!({"objectId":target})),
    );
    assert_eq!(property(&command, "Name")["value"]["text"], "Open");
    let handler = client.ok(
        "metadata/properties",
        context(&open, json!({"objectId":"event-subscription:OnWrite"})),
    );
    assert_eq!(
        property(&handler, "Handler")["presentation"]["target"],
        "common-module:Actions"
    );
    assert_eq!(
        property(&handler, "Handler")["presentation"]["caption"]["ru-RU"],
        "AfterWrite"
    );
    client.finish();
}
