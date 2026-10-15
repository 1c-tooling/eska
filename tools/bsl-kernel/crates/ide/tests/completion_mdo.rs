use ide::{Analysis, CompletionItem, CompletionItemKind};
use ide_db::base_db::{SourceDatabase, SourceRoot, SourceRootId};
use ide_db::metadata::{CanonicalMetadataInput, MdoEntry, MetadataListingData};
use ide_db::RootDatabaseImpl;
use salsa::Setter;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use test_fixture::Fixture;
use vfs::{FileId, FileSet};

/// Dependency discovery uses native root ownership and context without reading the target object.
#[test]
fn direct_completion_dependency_discovery_preserves_native_names_and_kinds() {
    for (access, kind, name) in [
        ("Catalogs.Items.$0", bsl_metadata::MdoType::Catalog, "Items"),
        ("сПрАвОчНиКи.Товары.Sa$0", bsl_metadata::MdoType::Catalog, "Товары"),
        ("ChartsOfAccounts.Accounts.$0", bsl_metadata::MdoType::ChartOfAccounts, "Accounts"),
        (
            "ChartsOfCalculationTypes.Calculations.$0",
            bsl_metadata::MdoType::ChartOfCalculationTypes,
            "Calculations",
        ),
        (
            "ChartsOfCharacteristicTypes.Characteristics.$0",
            bsl_metadata::MdoType::ChartOfCharacteristicTypes,
            "Characteristics",
        ),
    ] {
        let fixture = format!("//- /test.bsl\nProcedure Run()\n Value = {access};\nEndProcedure\n");
        let (analysis, file, offset) = setup_with_config(&fixture);
        assert_eq!(
            analysis.completion_metadata_target(file, offset),
            Some((kind, name.to_owned())),
            "{access}"
        );
    }
}

/// Collection names, non-code tokens, local shadowing and client-only code do not admit an object dependency.
#[test]
fn direct_completion_dependency_discovery_rejects_unclaimed_or_unavailable_roots() {
    for (path, body) in [
        ("/test.bsl", "Procedure Run()\n Value = Catalogs.$0;\nEndProcedure\n"),
        ("/test.bsl", "Procedure Run()\n Value = Metadata.Catalogs.$0;\nEndProcedure\n"),
        ("/test.bsl", "Procedure Run(Catalogs)\n Value = Catalogs.Items.$0;\nEndProcedure\n"),
        ("/test.bsl", "Procedure Run()\n Value = \"Catalogs.Items.$0\";\nEndProcedure\n"),
        ("/test.bsl", "Procedure Run()\n // Catalogs.Items.$0\nEndProcedure\n"),
        (
            "/Catalogs/Items/Forms/Main/Ext/Form/Module.bsl",
            "&AtClient\nProcedure Run()\n Value = Catalogs.Items.$0;\nEndProcedure\n",
        ),
    ] {
        let fixture = format!("//- {path}\n{body}");
        let (analysis, file, offset) = setup_with_config(&fixture);
        assert_eq!(analysis.completion_metadata_target(file, offset), None, "{body}");
    }
}

/// Cold references retain the exact native namespace reached through aliases and transparent callees.
#[test]
fn cold_reference_dependency_discovery_follows_exact_reaching_values() {
    for body in [
        "Procedure Run()\n Item=Catalogs.Справочник1.CreateItem();\n Alias=Item;\n Result=Alias.$0Реквизит1;\nEndProcedure\n",
        "Procedure Run()\n Result=(Catalogs.Справочник1.CreateItem()).$0Реквизит1;\nEndProcedure\n",
        "Procedure Run(Flag)\n If Flag Then\n Item=Catalogs.Справочник1.CreateItem();\n Alias=Item;\n Result=Alias.$0Реквизит1;\n EndIf;\nEndProcedure\n",
    ] {
        let (analysis, file, offset) = setup_with_pending_catalog(body);
        assert_eq!(
            analysis.completion_metadata_target(file, offset),
            Some((bsl_metadata::MdoType::Catalog, "Справочник1".into())),
            "{body}"
        );
    }
}

/// Overwrites, shadowing, cycles and unrelated calls cannot select a cold descriptor for this receiver.
#[test]
fn cold_reference_dependency_discovery_rejects_unproven_owners() {
    for body in [
        "Procedure Run()\n Item=Catalogs.Справочник1.CreateItem();\n Item=1;\n Result=Item.$0Реквизит1;\nEndProcedure\n",
        "Procedure Run(Catalogs)\n Item=Catalogs.Справочник1.CreateItem();\n Result=Item.$0Реквизит1;\nEndProcedure\n",
        "Procedure Run()\n Result=Echo(Catalogs.Справочник1.CreateItem()).$0Реквизит1;\nEndProcedure\n",
        "Procedure Run()\n Other=Catalogs.Справочник1.CreateItem();\n Result=Missing.$0Реквизит1;\nEndProcedure\n",
        "Procedure Other()\n Item=Catalogs.Справочник1.CreateItem();\nEndProcedure\nProcedure Run()\n Result=Item.$0Реквизит1;\nEndProcedure\n",
        "Procedure Run()\n Other=Catalogs.Справочник1.CreateItem();\n Item=Item;\n Result=Item.$0Реквизит1;\nEndProcedure\n",
        "Procedure Run()\n Other=Catalogs.Справочник1.CreateItem();\n Item=Alias;\n Alias=Item;\n Result=Alias.$0Реквизит1;\nEndProcedure\n",
    ] {
        let (analysis, file, offset) = setup_with_pending_catalog(body);
        assert_eq!(analysis.completion_metadata_target(file, offset), None, "{body}");
    }
}

/// Source-order reaching writes do not prove which branch supplies a value read after the conditional.
#[test]
fn cold_reference_dependency_discovery_does_not_choose_a_branch() {
    for body in [
        "Procedure Run(Flag)\n If Flag Then\n Item=Catalogs.Справочник1.CreateItem();\n Else\n Item=Catalogs.Справочник2.CreateItem();\n EndIf;\n Result=Item.$0Реквизит1;\nEndProcedure\n",
        "Procedure Run(Flag)\n If Flag Then\n Item=Catalogs.Справочник1.CreateItem();\n EndIf;\n Result=Item.$0Реквизит1;\nEndProcedure\n",
    ] {
        let (analysis, file, offset) = setup_with_pending_catalog(body);
        assert_eq!(analysis.completion_metadata_target(file, offset), None, "{body}");
    }
}

/// Pending canonical slots prohibit XML fallback, reproducing first-request discovery before host preparation.
fn setup_with_pending_catalog(body: &str) -> (Analysis, FileId, u32) {
    let (analysis, file, offset) = setup_with_config(&format!("//- /test.bsl\n{body}"));
    let mut db = analysis.database().clone();
    // A live analysis snapshot would prevent Salsa's exclusive input publication below.
    drop(analysis);
    let root = designer_fixture_path();
    let root = root.to_string_lossy();
    let entries = ["Справочник1", "Справочник2"]
        .into_iter()
        .enumerate()
        .map(|(index, name)| MdoEntry {
            kind: bsl_metadata::MdoType::Catalog,
            name: name.into(),
            main: FileId(90 + index as u32),
            predefined: None,
        })
        .collect();
    db.set_metadata_listing(
        &root,
        MetadataListingData { entries, ..MetadataListingData::default() },
    );
    let listing = db.metadata_listing(&root).expect("registered listing");
    let slots = [FileId(90), FileId(91)]
        .into_iter()
        .map(|file| (file, CanonicalMetadataInput::new(&db, None, "pending".into())))
        .collect();
    listing.set_canonical_metadata(&mut db).to(Arc::new(slots));
    assert!(ide_db::metadata::resolve_metadata_object(
        &db,
        listing,
        bsl_metadata::MdoType::Catalog,
        "Справочник1".into()
    )
    .is_none());
    (Analysis::from_database(db), file, offset)
}

/// A direct manager dependency uses the same native case-insensitive structural module index as completion.
#[test]
fn direct_completion_manager_discovery_selects_only_the_named_declared_body() {
    let (analysis,file,offset)=setup_with_config("//- /test.bsl\nProcedure Run()\n Value = сПрАвОчНиКи.iTeMs.$0;\nEndProcedure\n//- /Catalogs/Items/Ext/ManagerModule.bsl\nProcedure SavedMethod() Export\nEndProcedure\n//- /Catalogs/Neighbor/Ext/ManagerModule.bsl\nProcedure NeighborMethod() Export\nEndProcedure\n");
    let target = analysis.completion_manager_module(file, offset).unwrap();
    assert_ne!(target, file);
    let items = analysis.completions(file, offset, None, ide::Locale::En);
    assert!(items.iter().any(|item| item.label == "SavedMethod"));
    assert!(!items.iter().any(|item| item.label == "NeighborMethod"));
    let root = analysis.database().file_source_root_input(file).source_root_id(analysis.database());
    assert!(analysis
        .database()
        .source_root_input(root)
        .root(analysis.database())
        .file_set()
        .path_for_file(&target)
        .unwrap()
        .as_path()
        .ends_with("Catalogs/Items/Ext/ManagerModule.bsl"));
}

/// No admitted object or no manager declaration means no manager-body dependency.
#[test]
fn direct_completion_manager_discovery_rejects_shadowing_non_object_and_missing_manager() {
    for body in [
        "Procedure Run(Catalogs)\n Value=Catalogs.Items.$0;\nEndProcedure\n",
        "Procedure Run()\n Value=Catalogs.$0;\nEndProcedure\n",
        "Procedure Run()\n Value=Catalogs.Absent.$0;\nEndProcedure\n",
    ] {
        let text=format!("//- /test.bsl\n{body}//- /Catalogs/Items/Ext/ManagerModule.bsl\nProcedure SavedMethod() Export\nEndProcedure\n");
        let (analysis, file, offset) = setup_with_config(&text);
        assert_eq!(analysis.completion_manager_module(file, offset), None, "{body}");
    }
}

fn designer_fixture_path() -> PathBuf {
    PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/../bsl-metadata/fixtures/designer"))
}

fn setup_with_config(fixture_text: &str) -> (Analysis, FileId, u32) {
    setup_with_config_path(fixture_text, &designer_fixture_path())
}

fn setup_with_config_path(fixture_text: &str, config_path: &Path) -> (Analysis, FileId, u32) {
    let (fixture_text, test_path, cursor_offset) = extract_cursor(fixture_text);
    let fixture = Fixture::parse(&fixture_text);

    let mut db = RootDatabaseImpl::new();
    let source_root_id = SourceRootId(0);

    let mut file_set = FileSet::default();
    for (file_id, file) in &fixture.files {
        file_set.insert(*file_id, file.path.clone());
    }
    db.set_source_root(source_root_id, SourceRoot::new_local(file_set));

    for (file_id, file) in &fixture.files {
        db.set_file_source_root(*file_id, source_root_id);
        db.set_file_text(*file_id, &file.content);
    }

    db.set_all_config_paths(vec![(None, config_path.to_path_buf())]);

    let test_file = fixture
        .files
        .iter()
        .find(|(_, f)| f.path.as_path().to_string_lossy().ends_with(&test_path))
        .map(|(id, _)| *id)
        .expect("cursor-bearing file not found");

    (Analysis::from_database(db), test_file, cursor_offset)
}

fn extract_cursor(fixture_text: &str) -> (String, String, u32) {
    let abs_idx = fixture_text.find("$0").expect("fixture must contain $0 cursor marker");
    let prefix = &fixture_text[..abs_idx];
    let last_header_start = prefix.rfind("//- ").expect("cursor must be inside a //- file");
    let header_end =
        prefix[last_header_start..].find('\n').expect("//- header must end with newline")
            + last_header_start;
    let path_line = &prefix[last_header_start + 4..header_end];
    let file_offset_in_prefix = header_end + 1;
    let cursor_in_file = (abs_idx - file_offset_in_prefix) as u32;
    let cleaned = fixture_text.replacen("$0", "", 1);
    (cleaned, path_line.to_string(), cursor_in_file)
}

fn complete(fixture: &str) -> Vec<CompletionItem> {
    let (analysis, file_id, offset) = setup_with_config(fixture);
    analysis.completions(file_id, offset, None, ide::Locale::Ru)
}

fn complete_with_config_path(fixture: &str, config_path: &Path) -> Vec<CompletionItem> {
    let (analysis, file_id, offset) = setup_with_config_path(fixture, config_path);
    analysis.completions(file_id, offset, None, ide::Locale::Ru)
}

fn has_label(items: &[CompletionItem], label: &str) -> bool {
    items.iter().any(|i| i.label == label)
}

fn item_with_label<'a>(items: &'a [CompletionItem], label: &str) -> Option<&'a CompletionItem> {
    items.iter().find(|i| i.label == label)
}

fn write_collision_catalog_fixture(root: &Path) {
    std::fs::create_dir_all(root.join("Catalogs")).expect("create Catalogs directory");
    std::fs::create_dir_all(root.join("CommonModules")).expect("create CommonModules directory");
    std::fs::write(
        root.join("Configuration.xml"),
        r#"<?xml version="1.0" encoding="UTF-8"?>
<MetaDataObject xmlns="http://v8.1c.ru/8.3/MDClasses" version="2.10">
    <Configuration uuid="11111111-1111-1111-1111-111111111111">
        <Properties>
            <Name>CollisionConfig</Name>
        </Properties>
        <ChildObjects>
            <CommonModule>ПервыйОбщийМодуль</CommonModule>
            <Catalog>КоллизияМетодов</Catalog>
        </ChildObjects>
    </Configuration>
</MetaDataObject>"#,
    )
    .expect("write synthetic Configuration.xml");
    std::fs::write(
        root.join("CommonModules/ПервыйОбщийМодуль.xml"),
        r#"<?xml version="1.0" encoding="UTF-8"?>
<MetaDataObject xmlns="http://v8.1c.ru/8.3/MDClasses" version="2.10">
    <CommonModule uuid="44444444-4444-4444-4444-444444444444">
        <Properties>
            <Name>ПервыйОбщийМодуль</Name>
            <Global>false</Global>
            <Server>true</Server>
            <ClientManagedApplication>false</ClientManagedApplication>
            <ClientOrdinaryApplication>false</ClientOrdinaryApplication>
            <ExternalConnection>false</ExternalConnection>
            <ServerCall>false</ServerCall>
            <Privileged>false</Privileged>
            <ReturnValuesReuse>DontUse</ReturnValuesReuse>
        </Properties>
    </CommonModule>
</MetaDataObject>"#,
    )
    .expect("write synthetic CommonModule XML");
    std::fs::write(
        root.join("Catalogs/КоллизияМетодов.xml"),
        r#"<?xml version="1.0" encoding="UTF-8"?>
<MetaDataObject xmlns="http://v8.1c.ru/8.3/MDClasses" xmlns:v8="http://v8.1c.ru/8.1/data/core" version="2.10">
    <Catalog uuid="22222222-2222-2222-2222-222222222222">
        <Properties>
            <Name>КоллизияМетодов</Name>
            <CodeLength>9</CodeLength>
            <DescriptionLength>25</DescriptionLength>
        </Properties>
        <ChildObjects>
            <Attribute uuid="33333333-3333-3333-3333-333333333333">
                <Properties>
                    <Name>Записать</Name>
                    <Type>
                        <v8:Type>xs:string</v8:Type>
                        <v8:StringQualifiers>
                            <v8:Length>10</v8:Length>
                        </v8:StringQualifiers>
                    </Type>
                </Properties>
            </Attribute>
        </ChildObjects>
    </Catalog>
</MetaDataObject>"#,
    )
    .expect("write synthetic catalog XML");
}

fn write_accumulation_register_with_recorder_fixture(root: &Path) {
    std::fs::create_dir_all(root.join("AccumulationRegisters"))
        .expect("create AccumulationRegisters directory");
    std::fs::create_dir_all(root.join("Documents")).expect("create Documents directory");
    std::fs::create_dir_all(root.join("CommonModules")).expect("create CommonModules directory");
    std::fs::write(
        root.join("Configuration.xml"),
        r#"<?xml version="1.0" encoding="UTF-8"?>
<MetaDataObject xmlns="http://v8.1c.ru/8.3/MDClasses" version="2.10">
    <Configuration uuid="11111111-1111-1111-1111-111111111111">
        <Properties>
            <Name>RegisterConfig</Name>
        </Properties>
        <ChildObjects>
            <CommonModule>ПервыйОбщийМодуль</CommonModule>
            <AccumulationRegister>Остатки</AccumulationRegister>
            <Document>Поступление</Document>
        </ChildObjects>
    </Configuration>
</MetaDataObject>"#,
    )
    .expect("write synthetic Configuration.xml");
    std::fs::write(
        root.join("CommonModules/ПервыйОбщийМодуль.xml"),
        r#"<?xml version="1.0" encoding="UTF-8"?>
<MetaDataObject xmlns="http://v8.1c.ru/8.3/MDClasses" version="2.10">
    <CommonModule uuid="44444444-4444-4444-4444-444444444444">
        <Properties>
            <Name>ПервыйОбщийМодуль</Name>
            <Global>false</Global>
            <Server>true</Server>
            <ClientManagedApplication>false</ClientManagedApplication>
            <ClientOrdinaryApplication>false</ClientOrdinaryApplication>
            <ExternalConnection>false</ExternalConnection>
            <ServerCall>false</ServerCall>
            <Privileged>false</Privileged>
            <ReturnValuesReuse>DontUse</ReturnValuesReuse>
        </Properties>
    </CommonModule>
</MetaDataObject>"#,
    )
    .expect("write synthetic CommonModule XML");
    std::fs::write(
        root.join("AccumulationRegisters/Остатки.xml"),
        r#"<?xml version="1.0" encoding="UTF-8"?>
<MetaDataObject xmlns="http://v8.1c.ru/8.3/MDClasses" xmlns:v8="http://v8.1c.ru/8.1/data/core" version="2.10">
    <AccumulationRegister uuid="22222222-2222-2222-2222-222222222222">
        <Properties>
            <Name>Остатки</Name>
        </Properties>
        <ChildObjects>
            <Dimension uuid="33333333-3333-3333-3333-333333333333">
                <Properties>
                    <Name>Номенклатура</Name>
                    <Type>
                        <v8:Type>xs:string</v8:Type>
                    </Type>
                </Properties>
            </Dimension>
            <Dimension uuid="33333333-3333-3333-3333-444444444444">
                <Properties>
                    <Name>Цена</Name>
                    <Type>
                        <v8:Type>xs:decimal</v8:Type>
                        <v8:NumberQualifiers>
                            <v8:Digits>15</v8:Digits>
                            <v8:FractionDigits>2</v8:FractionDigits>
                        </v8:NumberQualifiers>
                    </Type>
                </Properties>
            </Dimension>
        </ChildObjects>
    </AccumulationRegister>
</MetaDataObject>"#,
    )
    .expect("write synthetic accumulation register XML");
    std::fs::write(
        root.join("Documents/Поступление.xml"),
        r#"<?xml version="1.0" encoding="UTF-8"?>
<MetaDataObject xmlns="http://v8.1c.ru/8.3/MDClasses" xmlns:xr="http://v8.1c.ru/8.3/xcf/readable" xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance" version="2.10">
    <Document uuid="55555555-5555-5555-5555-555555555555">
        <Properties>
            <Name>Поступление</Name>
            <RegisterRecords>
                <xr:Item xsi:type="xr:MDObjectRef">AccumulationRegister.Остатки</xr:Item>
            </RegisterRecords>
        </Properties>
    </Document>
</MetaDataObject>"#,
    )
    .expect("write synthetic document XML");
}

const CATALOG_REF_MODULE: &str = r#"//- /CommonModules/ПервыйОбщийМодуль/Ext/Module.bsl
// Возвращаемое значение:
//   СправочникСсылка.Справочник1
Функция Ссылка() Экспорт
    Возврат Неопределено;
КонецФункции

"#;

const CATALOG_OBJECT_MODULE: &str = r#"//- /CommonModules/ПервыйОбщийМодуль/Ext/Module.bsl
// Возвращаемое значение:
//   СправочникОбъект.Справочник1
Функция Объект() Экспорт
    Возврат Неопределено;
КонецФункции

"#;

#[test]
fn completion_after_dot_on_catalog_ref_includes_custom_attributes() {
    let items = complete(&format!(
        r#"{CATALOG_REF_MODULE}
//- /test.bsl
Функция Тест()
    Спр = ПервыйОбщийМодуль.Ссылка();
    Спр.$0
КонецФункции
"#
    ));

    assert!(!items.is_empty(), "expected completion items after dot on CatalogRef; got empty");

    for attr in &["Реквизит1", "Реквизит2", "Реквизит3"] {
        assert!(
            has_label(&items, attr),
            "custom attribute {attr} must appear; labels: {:?}",
            items.iter().map(|i| i.label.as_str()).collect::<Vec<_>>()
        );
        let item = item_with_label(&items, attr).unwrap();
        assert_eq!(
            item.kind,
            CompletionItemKind::Field,
            "{attr} must have kind Field, got {:?}",
            item.kind
        );
    }
}

#[test]
fn completion_after_dot_on_catalog_ref_includes_standard_code_attribute() {
    let items = complete(&format!(
        r#"{CATALOG_REF_MODULE}
//- /test.bsl
Функция Тест()
    Спр = ПервыйОбщийМодуль.Ссылка();
    Спр.$0
КонецФункции
"#
    ));

    assert!(
        has_label(&items, "Код"),
        "standard attribute Код must appear; labels: {:?}",
        items.iter().map(|i| i.label.as_str()).collect::<Vec<_>>()
    );
    let item = item_with_label(&items, "Код").unwrap();
    assert_eq!(item.kind, CompletionItemKind::Field, "Код must have kind Field");
}

#[test]
fn completion_after_dot_on_catalog_ref_includes_tabular_section_with_detail() {
    let items = complete(&format!(
        r#"{CATALOG_REF_MODULE}
//- /test.bsl
Функция Тест()
    Спр = ПервыйОбщийМодуль.Ссылка();
    Спр.$0
КонецФункции
"#
    ));

    assert!(
        has_label(&items, "ТабличнаяЧасть1"),
        "ТабличнаяЧасть1 must appear; labels: {:?}",
        items.iter().map(|i| i.label.as_str()).collect::<Vec<_>>()
    );
    let item = item_with_label(&items, "ТабличнаяЧасть1").unwrap();
    assert_eq!(item.kind, CompletionItemKind::Field, "ТабличнаяЧасть1 must have kind Field");
    let detail = item.detail.as_deref().unwrap_or("");
    assert!(
        detail.contains("ТабличнаяЧасть"),
        "detail for ТабличнаяЧасть1 must contain 'ТабличнаяЧасть', got: {detail:?}"
    );
}

#[test]
fn completion_after_dot_on_tabular_section_shows_platform_methods_regression() {
    let items = complete(&format!(
        r#"{CATALOG_OBJECT_MODULE}
//- /test.bsl
Функция Тест()
    Спр = ПервыйОбщийМодуль.Объект();
    Спр.ТабличнаяЧасть1.$0
КонецФункции
"#
    ));

    assert!(
        !items.is_empty(),
        "platform TS methods must be offered after ТабличнаяЧасть1 dot; got empty"
    );
    for method in &["Добавить", "НайтиСтроки", "Количество"] {
        assert!(
            has_label(&items, method),
            "platform method {method} must appear on TabularSection receiver; labels: {:?}",
            items.iter().map(|i| i.label.as_str()).collect::<Vec<_>>()
        );
    }
}

#[test]
fn completion_after_add_returns_row_shows_columns_and_line_number() {
    let items = complete(&format!(
        r#"{CATALOG_OBJECT_MODULE}
//- /test.bsl
Функция Тест()
    Спр = ПервыйОбщийМодуль.Объект();
    Стр = Спр.ТабличнаяЧасть1.Добавить();
    Стр.$0
КонецФункции
"#
    ));

    assert!(!items.is_empty(), "completion on TabularSectionRow must not be empty");
    let labels: Vec<&str> = items.iter().map(|i| i.label.as_str()).collect();

    for col in &["Реквизит1", "Реквизит2"] {
        assert!(has_label(&items, col), "row column {col} must appear; labels: {:?}", labels);
    }
    assert!(
        has_label(&items, "НомерСтроки"),
        "НомерСтроки must appear on TabularSectionRow; labels: {:?}",
        labels
    );
}

#[test]
fn completion_dedup_user_attribute_collides_with_platform_method() {
    let temp_dir = tempfile::tempdir().expect("create synthetic config tempdir");
    write_collision_catalog_fixture(temp_dir.path());
    let object_module_path = temp_dir.path().join("Catalogs/КоллизияМетодов/Ext/ObjectModule.bsl");

    let items = complete_with_config_path(
        &format!(
            r#"//- {}
Функция Тест()
    ЭтотОбъект.$0
КонецФункции
"#,
            object_module_path.display()
        ),
        temp_dir.path(),
    );

    let matching: Vec<_> = items.iter().filter(|item| item.label == "Записать").collect();
    assert_eq!(
        matching.len(),
        1,
        "MDO field must deduplicate platform method label; labels: {:?}",
        items.iter().map(|i| i.label.as_str()).collect::<Vec<_>>()
    );
    assert_eq!(matching[0].kind, CompletionItemKind::Field);
}

#[test]
fn completion_filter_by_english_name() {
    let items = complete(&format!(
        r#"{CATALOG_REF_MODULE}
//- /test.bsl
Функция Тест()
    Спр = ПервыйОбщийМодуль.Ссылка();
    Спр.$0
КонецФункции
"#
    ));

    let kod = item_with_label(&items, "Код").expect("Код must appear in completion list");
    let ft = kod.filter_text.as_deref().unwrap_or("");
    assert!(
        ft.to_lowercase().contains("code") || ft.to_lowercase().contains("код"),
        "filter_text must contain 'Код' or 'Code' to allow bilingual filtering; got: {ft:?}"
    );
}

#[test]
fn completion_after_dot_on_unknown_receiver_returns_no_mdo_fields() {
    let items = complete(
        r#"//- /test.bsl
Процедура Тест()
    НеизвестныйСимвол.$0
КонецПроцедуры
"#,
    );

    for item in &items {
        assert_ne!(
            item.kind,
            CompletionItemKind::Field,
            "unknown receiver must not produce MDO Field items; got: {:?}",
            item
        );
        assert!(
            !item.label.is_empty(),
            "completion items must have non-empty labels; got {:?}",
            item
        );
    }
}

#[test]
fn completion_object_manager_fast_path_does_not_show_mdo_fields() {
    let items = complete(
        r#"//- /test.bsl
Функция Тест()
    Справочники.Справочник1.$0
КонецФункции
"#,
    );

    assert!(!items.is_empty(), "ObjectManager must offer platform manager methods; got empty");
    for item in &items {
        assert_ne!(
            item.kind,
            CompletionItemKind::Field,
            "ObjectManager completion must not contain Field items; found: {:?}",
            item
        );
    }
}

#[test]
fn completion_after_dot_on_information_register_ref_shows_dimensions() {
    let items = complete(
        r#"//- /CommonModules/ПервыйОбщийМодуль/Ext/Module.bsl
// Возвращаемое значение:
//   РегистрСведенийКлючЗаписи.РегистрСведений1
Функция Запись() Экспорт
    Возврат Неопределено;
КонецФункции

//- /test.bsl
Функция Тест()
    З = ПервыйОбщийМодуль.Запись();
    З.$0
КонецФункции
"#,
    );

    assert!(
        has_label(&items, "Справочник1"),
        "dimension Справочник1 must appear on InformationRegisterRef receiver; labels: {:?}",
        items.iter().map(|i| i.label.as_str()).collect::<Vec<_>>()
    );
    let item = item_with_label(&items, "Справочник1").unwrap();
    assert_eq!(
        item.kind,
        CompletionItemKind::Field,
        "register dimension must have kind Field, got {:?}",
        item.kind
    );
}

#[test]
fn completion_filter_recorder_detail_contains_wrapper_and_recorder_type() {
    let temp_dir = tempfile::tempdir().expect("create synthetic register config tempdir");
    write_accumulation_register_with_recorder_fixture(temp_dir.path());

    let items = complete_with_config_path(
        r#"//- /test.bsl
Процедура Тест()
    Н = РегистрыНакопления.Остатки.СоздатьНаборЗаписей();
    Н.Отбор.$0
КонецПроцедуры
"#,
        temp_dir.path(),
    );

    let recorder = item_with_label(&items, "Регистратор").expect("Регистратор must complete");
    let detail = recorder.detail.as_deref().unwrap_or("");
    assert!(
        detail.contains("ЭлементОтбора →") && detail.contains("ДокументСсылка.Поступление"),
        "Регистратор detail must show wrapper and recorder type, got: {detail:?}",
    );
}

#[test]
fn completion_bare_ident_in_object_module_offers_mdo_attribute_with_real_type() {
    let module_path =
        designer_fixture_path().join("DataProcessors/ТестоваяОбработка/Ext/ObjectModule.bsl");
    let items = complete(&format!(
        r#"//- {}
Процедура Тест()
    Адр$0
КонецПроцедуры
"#,
        module_path.display(),
    ));

    let attr = item_with_label(&items, "АдресСайта").expect("MDO attribute must complete");
    assert_eq!(attr.detail.as_deref(), Some("Строка"));
}

#[test]
fn completion_bare_ident_in_form_marks_regular_form_attribute() {
    let module_path = designer_fixture_path()
        .join("Catalogs/рдт_Рецептура/Forms/ФормаЭлемента/Ext/Form/Module.bsl");
    let items = complete(&format!(
        r#"//- {}
Процедура Тест()
    Перес$0
КонецПроцедуры
"#,
        module_path.display(),
    ));

    let attr = item_with_label(&items, "Пересчитать").expect("form attribute must complete");
    let detail = attr.detail.as_deref().unwrap_or("");
    assert!(
        detail.contains("(реквизит формы)"),
        "regular form attribute detail must carry marker, got: {detail:?}",
    );
}

#[test]
fn hover_filter_dimension_price_shows_wrapper_and_value_type() {
    let temp_dir = tempfile::tempdir().expect("create synthetic register config tempdir");
    write_accumulation_register_with_recorder_fixture(temp_dir.path());
    let (analysis, file_id, offset) = setup_with_config_path(
        r#"//- /test.bsl
Процедура Тест()
    Н = РегистрыНакопления.Остатки.СоздатьНаборЗаписей();
    Э = Н.Отбор.Ц$0ена;
КонецПроцедуры
"#,
        temp_dir.path(),
    );

    let hover = analysis.hover(file_id, offset, ide::Locale::Ru).expect("hover must resolve");
    assert!(
        hover.markup.contains("ЭлементОтбора → Число"),
        "hover must show filter wrapper and value type, got:\n{}",
        hover.markup,
    );
}

#[test]
fn completion_document_object_offers_additional_properties_and_movements() {
    let items = complete(
        r#"//- /test.bsl
Функция Тест()
    Док = Документы.Документ1.СоздатьДокумент();
    Док.$0
КонецФункции
"#,
    );

    assert!(
        has_label(&items, "ДополнительныеСвойства"),
        "DocumentObject completion must offer ДополнительныеСвойства; labels: {:?}",
        items.iter().map(|i| i.label.as_str()).collect::<Vec<_>>()
    );
    assert!(
        has_label(&items, "Движения"),
        "DocumentObject completion must offer Движения; labels: {:?}",
        items.iter().map(|i| i.label.as_str()).collect::<Vec<_>>()
    );
    assert!(has_label(&items, "ОбменДанными"), "DocumentObject completion must offer ОбменДанными",);
}

#[test]
fn completion_chain_through_additional_properties_offers_structure_methods() {
    let items = complete(
        r#"//- /test.bsl
Функция Тест()
    Док = Документы.Документ1.СоздатьДокумент();
    Док.ДополнительныеСвойства.$0
КонецФункции
"#,
    );

    assert!(
        has_label(&items, "Вставить"),
        "Структура.Вставить must complete on ДополнительныеСвойства chain; labels: {:?}",
        items.iter().map(|i| i.label.as_str()).collect::<Vec<_>>()
    );
}

#[test]
fn completion_catalog_object_offers_additional_properties() {
    let items = complete(
        r#"//- /test.bsl
Функция Тест()
    Эл = Справочники.Справочник1.СоздатьЭлемент();
    Эл.$0
КонецФункции
"#,
    );

    assert!(
        has_label(&items, "ДополнительныеСвойства"),
        "CatalogObject completion must offer ДополнительныеСвойства; labels: {:?}",
        items.iter().map(|i| i.label.as_str()).collect::<Vec<_>>()
    );
    assert!(has_label(&items, "ВерсияДанных"), "CatalogObject completion must offer ВерсияДанных",);
}

#[test]
fn completion_object_module_top_level_offers_implicit_attribute() {
    let object_module_path =
        designer_fixture_path().join("Catalogs/Справочник1/Ext/ObjectModule.bsl");
    let items = complete_with_config_path(
        &format!(
            r#"//- {}
Функция Тест()
    Рек$0
КонецФункции
"#,
            object_module_path.display()
        ),
        &designer_fixture_path(),
    );
    let req = items.iter().find(|i| i.label == "Реквизит1").unwrap_or_else(|| {
        panic!(
            "Реквизит1 (ObjectModule implicit attribute) must surface; labels: {:?}",
            items.iter().map(|i| i.label.as_str()).collect::<Vec<_>>()
        )
    });
    assert_eq!(
        req.kind,
        CompletionItemKind::Field,
        "implicit ObjectModule attribute must render as Field, not as a platform shape"
    );
}

fn write_catalog_with_metadata_attribute(root: &Path) {
    std::fs::create_dir_all(root.join("Catalogs")).expect("create Catalogs directory");
    std::fs::write(
        root.join("Configuration.xml"),
        r#"<?xml version="1.0" encoding="UTF-8"?>
<MetaDataObject xmlns="http://v8.1c.ru/8.3/MDClasses" version="2.10">
    <Configuration uuid="11111111-1111-1111-1111-111111111111">
        <Properties>
            <Name>HbkCollisionConfig</Name>
        </Properties>
        <ChildObjects>
            <Catalog>СправочникСМетаданными</Catalog>
        </ChildObjects>
    </Configuration>
</MetaDataObject>"#,
    )
    .expect("write synthetic Configuration.xml");
    std::fs::write(
        root.join("Catalogs/СправочникСМетаданными.xml"),
        r#"<?xml version="1.0" encoding="UTF-8"?>
<MetaDataObject xmlns="http://v8.1c.ru/8.3/MDClasses" xmlns:v8="http://v8.1c.ru/8.1/data/core" version="2.10">
    <Catalog uuid="22222222-2222-2222-2222-222222222222">
        <Properties>
            <Name>СправочникСМетаданными</Name>
            <CodeLength>9</CodeLength>
            <DescriptionLength>25</DescriptionLength>
        </Properties>
        <ChildObjects>
            <Attribute uuid="33333333-3333-3333-3333-333333333333">
                <Properties>
                    <Name>Метаданные</Name>
                    <Type>
                        <v8:Type>xs:string</v8:Type>
                        <v8:StringQualifiers>
                            <v8:Length>20</v8:Length>
                        </v8:StringQualifiers>
                    </Type>
                </Properties>
            </Attribute>
        </ChildObjects>
    </Catalog>
</MetaDataObject>"#,
    )
    .expect("write synthetic catalog XML");
}

#[test]
fn completion_object_module_attribute_shadows_hbk_global() {
    let temp_dir = tempfile::tempdir().expect("create synthetic HBK-collision config tempdir");
    write_catalog_with_metadata_attribute(temp_dir.path());
    let object_module_path =
        temp_dir.path().join("Catalogs/СправочникСМетаданными/Ext/ObjectModule.bsl");
    let items = complete_with_config_path(
        &format!(
            r#"//- {}
Функция Тест()
    Мет$0
КонецФункции
"#,
            object_module_path.display()
        ),
        temp_dir.path(),
    );
    let meta = items.iter().find(|i| i.label == "Метаданные").unwrap_or_else(|| {
        panic!(
            "Метаданные (workspace attribute) must surface at top of ObjectModule; labels: {:?}",
            items.iter().map(|i| i.label.as_str()).collect::<Vec<_>>()
        )
    });
    assert_eq!(
        meta.kind,
        CompletionItemKind::Field,
        "workspace attribute must shadow HBK Global-context property; kind = {:?}",
        meta.kind,
    );
    let leaked_hbk = items.iter().any(|i| {
        i.label == "Метаданные"
            && i.kind == CompletionItemKind::Property
            && i.detail.as_deref().is_some_and(|d| d.contains("ОбъектМетаданныхКонфигурация"))
    });
    assert!(
        !leaked_hbk,
        "HBK Метаданные property leaked despite workspace attribute shadow; labels/kinds: {:?}",
        items
            .iter()
            .filter(|i| i.label == "Метаданные")
            .map(|i| (i.label.clone(), i.kind))
            .collect::<Vec<_>>()
    );
}

#[test]
fn collection_objects_hidden_in_client_form_method() {
    let items = complete(
        r#"//- /Catalogs/Товары/Forms/ФормаЭлемента/Ext/Form/Module.bsl
&НаКлиенте
Процедура Сохранить()
    Справочники.$0
КонецПроцедуры
"#,
    );
    assert!(
        items.is_empty(),
        "a client method must not offer catalog names behind the unavailable collection root; got: {:?}",
        items.iter().map(|i| i.label.as_str()).collect::<Vec<_>>()
    );
}

#[test]
fn collection_objects_offered_in_server_form_method() {
    let items = complete(
        r#"//- /Catalogs/Товары/Forms/ФормаЭлемента/Ext/Form/Module.bsl
&НаСервере
Процедура Записать()
    Справочники.$0
КонецПроцедуры
"#,
    );
    assert!(
        has_label(&items, "Справочник1"),
        "the server method must keep the catalog suggestions; got: {:?}",
        items.iter().map(|i| i.label.as_str()).collect::<Vec<_>>()
    );
}

#[test]
fn collection_objects_restored_under_server_conditional() {
    let items = complete(
        r#"//- /Catalogs/Товары/Forms/ФормаЭлемента/Ext/Form/Module.bsl
&НаКлиенте
Процедура Сохранить()
    #Если Сервер Тогда
    Справочники.$0
    #КонецЕсли
КонецПроцедуры
"#,
    );
    assert!(
        has_label(&items, "Справочник1"),
        "`#Если Сервер` narrowing must restore the suggestions, mirroring the diagnostic; got: {:?}",
        items.iter().map(|i| i.label.as_str()).collect::<Vec<_>>()
    );
}

#[test]
fn english_collection_root_hidden_in_client_form_method() {
    let items = complete(
        r#"//- /Catalogs/Товары/Forms/ФормаЭлемента/Ext/Form/Module.bsl
&НаКлиенте
Процедура Сохранить()
    Catalogs.$0
КонецПроцедуры
"#,
    );
    assert!(
        items.is_empty(),
        "the English collection root must be hidden in a client method too; got: {:?}",
        items.iter().map(|i| i.label.as_str()).collect::<Vec<_>>()
    );
}

#[test]
fn object_members_hidden_in_client_form_method() {
    let items = complete(
        r#"//- /Catalogs/Товары/Forms/ФормаЭлемента/Ext/Form/Module.bsl
&НаКлиенте
Процедура Сохранить()
    Справочники.Справочник1.$0
КонецПроцедуры
"#,
    );
    assert!(
        items.is_empty(),
        "manager members sit behind the flagged collection root and must be hidden in a client method; got: {:?}",
        items.iter().map(|i| i.label.as_str()).collect::<Vec<_>>()
    );
}

#[test]
fn assigned_collection_root_still_offers_its_objects() {
    let items = complete(
        r#"//- /test.bsl
Функция Тест()
    Справочники = Новый Структура("Код", 1);
    Справочники.$0
КонецФункции
"#,
    );
    // The assignment writes to a Global-context property and declares no local, so
    // the name never became a `Структура`. Offering that structure's members would
    // promise an object the code does not have; the illegal write is reported by
    // `GlobalPropertyNotWritable` instead.
    assert!(
        has_label(&items, "Справочник1"),
        "the name still denotes the collection; got: {:?}",
        items.iter().map(|i| i.label.as_str()).collect::<Vec<_>>()
    );
    assert!(
        !has_label(&items, "Вставить"),
        "no local of type Структура exists here; got: {:?}",
        items.iter().map(|i| i.label.as_str()).collect::<Vec<_>>()
    );
}

#[test]
fn typed_collection_local_keeps_object_suggestions_in_client() {
    let items = complete(
        r#"//- /Catalogs/Товары/Forms/ФормаЭлемента/Ext/Form/Module.bsl
&НаСервере
Функция ПолучитьСправочники()
    Возврат Справочники;
КонецФункции

&НаКлиенте
Процедура Тест()
    Справочники = ПолучитьСправочники();
    Справочники.$0
КонецПроцедуры
"#,
    );
    assert!(
        !has_label(&items, "Справочник1"),
        "the assignment declares no local, so the name is the collection again — and a \
         client method may not touch it; got: {:?}",
        items.iter().map(|i| i.label.as_str()).collect::<Vec<_>>()
    );
}

#[test]
fn assigned_root_in_server_conditional_still_offers_objects() {
    let items = complete(
        r#"//- /Catalogs/Товары/Forms/ФормаЭлемента/Ext/Form/Module.bsl
&НаСервере
Процедура Тест()
    Справочники = Новый Структура("Код", 1);
    #Если Сервер Тогда
    Справочники.$0
    #КонецЕсли
КонецПроцедуры
"#,
    );
    assert!(
        has_label(&items, "Справочник1"),
        "an assignment claims nothing, and the server branch admits the collection; got: {:?}",
        items.iter().map(|i| i.label.as_str()).collect::<Vec<_>>()
    );
}

#[test]
fn shadowed_root_before_assignment_offers_no_objects() {
    let items = complete(
        r#"//- /Catalogs/Товары/Forms/ФормаЭлемента/Ext/Form/Module.bsl
&НаКлиенте
Процедура Тест()
    Справочники.$0
    Справочники = Новый Структура("Код", 1);
КонецПроцедуры
"#,
    );
    assert!(
        !has_label(&items, "Справочник1"),
        "an assignment anywhere in the body claims the name for the whole body; got: {:?}",
        items.iter().map(|i| i.label.as_str()).collect::<Vec<_>>()
    );
}

#[test]
fn metadata_collection_hidden_behind_typed_local_in_client() {
    let items = complete(
        r#"//- /Catalogs/Товары/Forms/ФормаЭлемента/Ext/Form/Module.bsl
&НаСервере
Функция ПолучитьМетаданные()
    Возврат Метаданные;
КонецФункции

&НаКлиенте
Процедура Тест()
    Метаданные = ПолучитьМетаданные();
    Метаданные.Справочники.$0
КонецПроцедуры
"#,
    );
    assert!(
        !has_label(&items, "Справочник1"),
        "the collection property has its own availability mask and is unreachable on the client even through a variable; got: {:?}",
        items.iter().map(|i| i.label.as_str()).collect::<Vec<_>>()
    );
}

#[test]
fn metadata_collection_objects_offered_in_server_method() {
    let items = complete(
        r#"//- /Catalogs/Товары/Forms/ФормаЭлемента/Ext/Form/Module.bsl
&НаСервере
Процедура Записать()
    Метаданные.Справочники.$0
КонецПроцедуры
"#,
    );
    assert!(
        has_label(&items, "Справочник1"),
        "the server method must keep the second-level suggestions; got: {:?}",
        items.iter().map(|i| i.label.as_str()).collect::<Vec<_>>()
    );
}

#[test]
fn optional_typed_collection_local_keeps_objects() {
    let items = complete(
        r#"//- /Catalogs/Товары/Forms/ФормаЭлемента/Ext/Form/Module.bsl
&НаСервере
Функция ПолучитьСправочники()
    Если Истина Тогда
        Возврат Справочники;
    КонецЕсли;
    Возврат Неопределено;
КонецФункции

&НаКлиенте
Процедура Тест()
    Справочники = ПолучитьСправочники();
    Справочники.$0
КонецПроцедуры
"#,
    );
    assert!(
        !has_label(&items, "Справочник1"),
        "the assignment declares no local, so the name is the collection again — and a \
         client method may not touch it; got: {:?}",
        items.iter().map(|i| i.label.as_str()).collect::<Vec<_>>()
    );
}

#[test]
fn optional_metadata_local_hides_server_collection() {
    let items = complete(
        r#"//- /Catalogs/Товары/Forms/ФормаЭлемента/Ext/Form/Module.bsl
&НаСервере
Функция ПолучитьМетаданные()
    Если Истина Тогда
        Возврат Метаданные;
    КонецЕсли;
    Возврат Неопределено;
КонецФункции

&НаКлиенте
Процедура Тест()
    Метаданные = ПолучитьМетаданные();
    Метаданные.$0
КонецПроцедуры
"#,
    );
    assert!(
        !has_label(&items, "Справочники"),
        "the server-only collection property must stay hidden behind a nullable metadata local on the client; got: {:?}",
        items.iter().map(|i| i.label.as_str()).collect::<Vec<_>>()
    );
}

#[test]
fn shadowed_root_before_assignment_with_prefix_offers_no_objects() {
    let items = complete(
        r#"//- /Catalogs/Товары/Forms/ФормаЭлемента/Ext/Form/Module.bsl
&НаКлиенте
Процедура Тест()
    Справочники.С$0
    Справочники = Новый Структура("Код", 1);
КонецПроцедуры
"#,
    );
    assert!(
        !has_label(&items, "Справочник1"),
        "before the first assignment the name still denotes the global, and the client gate applies; got: {:?}",
        items.iter().map(|i| i.label.as_str()).collect::<Vec<_>>()
    );
}

#[test]
fn metadata_read_before_assignment_stays_root_gated() {
    let items = complete(
        r#"//- /Catalogs/Товары/Forms/ФормаЭлемента/Ext/Form/Module.bsl
&НаКлиенте
Процедура Тест()
    Метаданные.О$0
    Метаданные = Новый Структура;
КонецПроцедуры
"#,
    );
    assert!(
        items.is_empty(),
        "before the first assignment the read is the global `Метаданные`, unavailable on the client; got: {:?}",
        items.iter().map(|i| i.label.as_str()).collect::<Vec<_>>()
    );
}

#[test]
fn collection_rhs_before_first_assignment_stays_root_gated() {
    let items = complete(
        r#"//- /Catalogs/Товары/Forms/ФормаЭлемента/Ext/Form/Module.bsl
&НаКлиенте
Процедура Тест()
    Справочники = Справочники.С$0
КонецПроцедуры
"#,
    );
    assert!(
        items.is_empty(),
        "inside its own right-hand side the name still denotes the unavailable global; got: {:?}",
        items.iter().map(|i| i.label.as_str()).collect::<Vec<_>>()
    );
}

#[test]
fn assignment_does_not_take_the_name_from_a_collection() {
    // Assigning to a Global-context property does not create a local: the platform
    // refuses the write instead ("property is not writable"), so the name keeps
    // denoting the collection throughout the body — before and after the assignment
    // alike. Only a DECLARED owner (parameter, `Перем`, form attribute, method,
    // common module) takes the name.
    for (label, fixture) in [
        (
            "read before the assignment",
            r#"//- /Catalogs/Товары/Forms/ФормаЭлемента/Ext/Form/Module.bsl
&НаСервере
Процедура Тест()
    Справочники.С$0
    Справочники = Новый Структура("Код", 1);
КонецПроцедуры
"#,
        ),
        (
            "read after the assignment",
            r#"//- /Catalogs/Товары/Forms/ФормаЭлемента/Ext/Form/Module.bsl
&НаСервере
Процедура Тест()
    Справочники = Новый Структура("Код", 1);
    Справочники.С$0
КонецПроцедуры
"#,
        ),
    ] {
        let items = complete(fixture);
        assert!(
            has_label(&items, "Справочник1"),
            "{label}: the name still denotes the platform collection; got: {:?}",
            items.iter().map(|i| i.label.as_str()).collect::<Vec<_>>()
        );
    }
}

#[test]
fn assignment_of_unknown_value_still_leaves_the_collection() {
    let items = complete(
        r#"//- /test.bsl
Функция Тест()
    Справочники = НеизвестнаяФункция();
    Справочники.С$0
КонецФункции
"#,
    );
    assert!(
        has_label(&items, "Справочник1"),
        "an assignment of an unknown value declares no local either; got: {:?}",
        items.iter().map(|i| i.label.as_str()).collect::<Vec<_>>()
    );
}

#[test]
fn conditional_module_metadata_assignment_does_not_supply_mdo_objects() {
    let items = complete(
        r#"//- /Catalogs/Товары/Forms/ФормаЭлемента/Ext/Form/Module.bsl
Перем Метаданные;
&НаСервере
Процедура Тест()
    Если Ложь Тогда
        Метаданные = Metadata;
    КонецЕсли;
    Метаданные.Справочники.$0
КонецПроцедуры
"#,
    );
    assert!(
        !has_label(&items, "Справочник1"),
        "a body assignment to a module variable is not a flow-typed local write; got: {:?}",
        items.iter().map(|i| i.label.as_str()).collect::<Vec<_>>()
    );
}

#[test]
fn assigned_collection_shadows_same_named_method() {
    let items = complete(
        r#"//- /Catalogs/Товары/Forms/ФормаЭлемента/Ext/Form/Module.bsl
&НаСервере
Функция ПолучитьСправочники()
    Возврат Catalogs;
КонецФункции

Процедура Справочники()
КонецПроцедуры

&НаКлиенте
Процедура Тест()
    Справочники = ПолучитьСправочники();
    Справочники.$0
КонецПроцедуры
"#,
    );
    assert!(
        has_label(&items, "Справочник1"),
        "a same-named method cannot be assigned to — the write creates an implicit local, which inference flow-types; got: {:?}",
        items.iter().map(|i| i.label.as_str()).collect::<Vec<_>>()
    );
}

#[test]
fn assigned_local_over_method_beats_same_named_object_member() {
    let temp_dir = tempfile::tempdir().expect("tempdir");
    write_catalog_with_metadata_attribute(temp_dir.path());
    let object_module_path =
        temp_dir.path().join("Catalogs/СправочникСМетаданными/Ext/ObjectModule.bsl");
    let items = complete_with_config_path(
        &format!(
            "//- {}\nФункция Метаданные()\nКонецФункции\n\nФункция Тест()\n    Метаданные = Metadata;\n    Метаданные.Справочники.$0\nКонецФункции\n",
            object_module_path.display()
        ),
        temp_dir.path(),
    );
    assert!(
        has_label(&items, "СправочникСМетаданными"),
        "the method wins resolution, so the assignment creates a flow-typed implicit local despite the same-named object member; got: {:?}",
        items.iter().map(|i| i.label.as_str()).collect::<Vec<_>>()
    );
}
