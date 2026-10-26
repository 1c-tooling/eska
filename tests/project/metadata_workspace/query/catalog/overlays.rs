use super::*;

/// Build independent base/extension members and a foreign application with colliding metadata names.
fn application_fixture(root: &Path) {
    for (name, kind) in [
        ("base", "configuration"),
        ("first", "extension"),
        ("second", "extension"),
        ("foreign", "configuration"),
    ] {
        let member = root.join(name);
        fs::create_dir_all(&member).unwrap();
        fixture(&member, kind);
        fs::write(
            member.join("eska.toml"),
            format!("[project]\nname='{name}'\ntype='{kind}'\n"),
        )
        .unwrap();
        fs::create_dir_all(member.join("src/Ext")).unwrap();
        fs::write(
            member.join("src/Ext/ParentConfigurations.bin"),
            "{6,0,0,0,0,0}",
        )
        .unwrap();
    }
    fs::write(root.join("eska.toml"), "[workspace]\n[workspace.members.main]\nconfiguration='base'\nextensions = [{path='first'}, {path='second'}]\n[workspace.members.other]\nconfiguration='foreign'\n").unwrap();
    descriptor(
        root,
        "base",
        MetadataKind::ChartOfAccounts,
        "Accounts",
        "<MaxExtDimensionCount>3</MaxExtDimensionCount>",
        "",
    );
    descriptor(
        root,
        "first",
        MetadataKind::ChartOfAccounts,
        "Accounts",
        "<ObjectBelonging>Adopted</ObjectBelonging><MaxExtDimensionCount>4</MaxExtDimensionCount>",
        "",
    );
    descriptor(
        root,
        "second",
        MetadataKind::ChartOfAccounts,
        "Accounts",
        "<ObjectBelonging>Adopted</ObjectBelonging><MaxExtDimensionCount>2</MaxExtDimensionCount>",
        "",
    );
    descriptor(
        root,
        "base",
        MetadataKind::AccountingRegister,
        "Ledger",
        "<Correspondence>true</Correspondence><ChartOfAccounts>ChartOfAccounts.Accounts</ChartOfAccounts>",
        &field("Resource", "Amount", "<Balance>true</Balance>"),
    );
    descriptor(
        root,
        "first",
        MetadataKind::AccountingRegister,
        "Ledger",
        "<ObjectBelonging>Adopted</ObjectBelonging>",
        &field("Resource", "Amount", "<Balance>false</Balance>"),
    );
    descriptor(
        root,
        "second",
        MetadataKind::AccountingRegister,
        "Ledger",
        "<ObjectBelonging>Adopted</ObjectBelonging>",
        &field("Dimension", "Company", "<Balance>true</Balance>"),
    );
    descriptor(
        root,
        "foreign",
        MetadataKind::ChartOfAccounts,
        "Accounts",
        "<MaxExtDimensionCount>9</MaxExtDimensionCount>",
        "",
    );
    calculation_schemas(root);
}

/// Calculation dependencies deliberately cross base and extension descriptors.
fn calculation_schemas(root: &Path) {
    descriptor(
        root,
        "base",
        MetadataKind::CalculationRegister,
        "Pay",
        "<ChartOfCalculationTypes>ChartOfCalculationTypes.MainTypes</ChartOfCalculationTypes><BasePeriodUse>true</BasePeriodUse><ActionPeriodUse>true</ActionPeriodUse><Schedule>InformationRegister.Calendar</Schedule>",
        "",
    );
    descriptor(
        root,
        "first",
        MetadataKind::CalculationRegister,
        "Pay",
        "<ObjectBelonging>Adopted</ObjectBelonging>",
        &field("Dimension", "Person", ""),
    );
    descriptor(
        root,
        "base",
        MetadataKind::InformationRegister,
        "Calendar",
        "",
        &field("Resource", "Hours", ""),
    );
    descriptor(
        root,
        "second",
        MetadataKind::InformationRegister,
        "Calendar",
        "<ObjectBelonging>Adopted</ObjectBelonging>",
        &field("Resource", "Days", ""),
    );
    // A prefixed property inherited from the envelope must remain valid after composition.
    descriptor(
        root,
        "base",
        MetadataKind::ChartOfCalculationTypes,
        "MainTypes",
        "<DependenceOnCalculationTypes>ActionPeriod</DependenceOnCalculationTypes><BaseCalculationTypes><v8:Item>ChartOfCalculationTypes.BaseTypes</v8:Item></BaseCalculationTypes>",
        "",
    );
    descriptor(
        root,
        "base",
        MetadataKind::CalculationRegister,
        "Wages",
        "<ChartOfCalculationTypes>ChartOfCalculationTypes.BaseTypes</ChartOfCalculationTypes>",
        &field("Dimension", "Employee", ""),
    );
    descriptor(
        root,
        "second",
        MetadataKind::CalculationRegister,
        "Wages",
        "<ObjectBelonging>Adopted</ObjectBelonging>",
        &field("Resource", "Bonus", ""),
    );
}

/// Declare a real standalone descriptor; only this test-owned project is mutated.
fn descriptor(
    root: &Path,
    member: &str,
    kind: MetadataKind,
    name: &str,
    properties: &str,
    children: &str,
) {
    let root = root.join(member).join("src");
    let configuration = root.join("Configuration.xml");
    let tag = kind.designer_tag();
    let declaration = format!("<{tag}>{name}</{tag}>");
    let input = fs::read_to_string(&configuration).unwrap();
    if !input.contains(&declaration) {
        fs::write(
            &configuration,
            input.replace("</ChildObjects>", &format!("{declaration}</ChildObjects>")),
        )
        .unwrap();
    }
    let folder = root.join(kind.collection_folder().unwrap());
    fs::create_dir_all(&folder).unwrap();
    fs::write(folder.join(format!("{name}.xml")), format!("<MetaDataObject xmlns='http://v8.1c.ru/8.3/MDClasses' xmlns:v8='http://v8.1c.ru/8.1/data/core'><{tag} uuid='11111111-1111-1111-1111-111111111111'><Properties><Name>{name}</Name>{properties}</Properties><ChildObjects>{children}</ChildObjects></{tag}></MetaDataObject>")).unwrap();
}

/// Named children may inherit or override individual field properties across borrowed objects.
fn field(tag: &str, name: &str, properties: &str) -> String {
    format!(
        "<{tag} uuid='22222222-2222-2222-2222-222222222222'><Properties><Name>{name}</Name>{properties}</Properties></{tag}>"
    )
}

/// Effective owner flags, fields and related schemas use every selected member once, in configured order.
#[test]
fn query_catalog_composes_borrowed_owners_and_related_schemas_across_application_sources() {
    let directory = TestDir::new();
    application_fixture(&directory.0);
    let workspace = MetadataWorkspace::open(&directory.0, &[], false).unwrap();
    let project = workspace
        .projects()
        .iter()
        .find(|p| p.project().root().ends_with("first"))
        .unwrap();
    let before = bytes(&directory.0);
    let catalog = |table: &str| {
        project
            .query_catalog(
                Some("main"),
                &QueryCatalogRequest {
                    table: Some(table.into()),
                    virtual_tables: true,
                    ..Default::default()
                },
            )
            .unwrap()
    };
    let physical = catalog("AccountingRegister.Ledger");
    let names: Vec<_> = physical.fields.iter().map(|f| f.english.as_str()).collect();
    assert!(physical.found && !physical.limited_context);
    for name in ["AmountDr", "AmountCr", "Company", "AccountDr"] {
        assert!(names.contains(&name), "{name}");
    }
    assert!(!names.contains(&"Amount"));
    let virtuals = catalog("РегистрБухгалтерии.Ledger.ОборотыДтКт");
    let names: Vec<_> = virtuals.fields.iter().map(|f| f.english.as_str()).collect();
    for name in [
        "AmountTurnoverDr",
        "AmountTurnoverCr",
        "ExtDimensionDr2",
        "Company",
    ] {
        assert!(names.contains(&name), "{name}");
    }
    for name in ["AmountTurnover", "ExtDimensionDr3", "ExtDimensionDr9"] {
        assert!(!names.contains(&name), "{name}");
    }
    assert_eq!(virtuals.parameters.len(), 8);
    let schedule = catalog("CalculationRegister.Pay.ScheduleData");
    for name in ["HoursBasePeriod", "DaysActivePeriod", "Person"] {
        assert!(schedule.fields.iter().any(|f| f.english == name), "{name}");
    }
    let base = catalog("CalculationRegister.Pay.BaseWages");
    for name in ["EmployeeDimension", "BonusBase", "Person"] {
        assert!(base.fields.iter().any(|f| f.english == name), "{name}");
    }
    assert!(!catalog("CalculationRegister.Pay.BaseUnknown").found);
    assert_eq!(bytes(&directory.0), before);
}

/// Unknown relations contribute no invented columns; an explicitly declared missing chart is a hard failure.
#[test]
fn query_catalog_related_sources_preserve_missing_descriptor_errors_and_limited_context() {
    let directory = TestDir::new();
    application_fixture(&directory.0);
    let workspace = MetadataWorkspace::open(&directory.0, &[], false).unwrap();
    let project = &workspace.projects()[0];
    let request = QueryCatalogRequest {
        table: Some("AccountingRegister.Ledger.RecordsWithExtDimensions".into()),
        virtual_tables: true,
        ..Default::default()
    };
    fs::remove_file(directory.0.join("base/src/ChartsOfAccounts/Accounts.xml")).unwrap();
    assert!(project.query_catalog(Some("main"), &request).is_err());
    descriptor(
        &directory.0,
        "base",
        MetadataKind::AccountingRegister,
        "Ledger",
        "<ChartOfAccounts>ChartOfAccounts.Unknown</ChartOfAccounts>",
        "",
    );
    let page = project.query_catalog(Some("main"), &request).unwrap();
    assert!(page.found);
    assert!(
        !page
            .fields
            .iter()
            .any(|f| f.english.starts_with("ExtDimensionDr"))
    );
    let separate = TestDir::new();
    fixture(&separate.0, "extension");
    let workspace = MetadataWorkspace::open(&separate.0, &[], false).unwrap();
    assert!(
        workspace.projects()[0]
            .query_catalog(None, &QueryCatalogRequest::default())
            .unwrap()
            .limited_context
    );
}
