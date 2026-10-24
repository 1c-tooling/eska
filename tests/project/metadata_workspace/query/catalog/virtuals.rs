use super::*;

/// Corresponding accounts, numbered subaccounts and unbalanced debit/credit turnover fields stay table-specific.
#[test]
fn accounting_virtual_catalog_uses_assigned_subaccounts_and_resource_balance_flags() {
    let (directory, _, _, _) = setup();
    let root = directory.0.join("src/Configuration.xml");
    fs::write(
        &root,
        fs::read_to_string(&root).unwrap().replace(
            "</ChildObjects>",
            "<ChartOfAccounts>Accounts</ChartOfAccounts></ChildObjects>",
        ),
    )
    .unwrap();
    calculation_descriptor(
        &directory.0,
        "ChartOfAccounts",
        "Accounts",
        "<MaxExtDimensionCount>3</MaxExtDimensionCount>",
        "",
    );
    fs::write(directory.0.join("src/AccountingRegisters/Проводки.xml"), "<MetaDataObject xmlns='http://v8.1c.ru/8.3/MDClasses'><AccountingRegister uuid='11111111-1111-1111-1111-111111111111'><Properties><Name>Проводки</Name><Correspondence>1</Correspondence><PeriodAdjustmentLength>1</PeriodAdjustmentLength><ChartOfAccounts>ChartOfAccounts.Accounts</ChartOfAccounts></Properties><ChildObjects><Resource uuid='33333333-3333-3333-3333-333333333333'><Properties><Name>Quantity</Name><Balance>false</Balance></Properties></Resource></ChildObjects></AccountingRegister></MetaDataObject>").unwrap();
    let workspace = MetadataWorkspace::open(&directory.0, &[], false).unwrap();
    let project = &workspace.projects()[0];
    let request = |suffix: &str| QueryCatalogRequest {
        table: Some(format!("AccountingRegister.Проводки{suffix}")),
        virtual_tables: true,
        ..Default::default()
    };
    let debit_credit = project
        .query_catalog(None, &request(".DrCrTurnovers"))
        .unwrap();
    for name in [
        "QuantityTurnoverDr",
        "QuantityTurnoverCr",
        "ExtDimensionDr3",
        "ExtDimensionCr3",
        "PeriodAdjustment",
    ] {
        assert!(
            debit_credit
                .fields
                .iter()
                .any(|field| field.english == name),
            "{name}"
        );
    }
    assert!(
        !debit_credit
            .fields
            .iter()
            .any(|field| field.english == "QuantityTurnover")
    );
    let turnover = project.query_catalog(None, &request(".Turnovers")).unwrap();
    assert_eq!(turnover.parameters.len(), 8);
    for name in [
        "BalancedAccount",
        "BalancedExtDimension3",
        "QuantityBalancedTurnoverDr",
    ] {
        assert!(
            turnover.fields.iter().any(|field| field.english == name),
            "{name}"
        );
    }
    let records = project
        .query_catalog(None, &request(".RecordsWithExtDimensions"))
        .unwrap();
    for name in ["PointInTime", "ExtDimensionTypeDr3", "QuantityDr"] {
        assert!(
            records.fields.iter().any(|field| field.english == name),
            "{name}"
        );
    }
    let physical = project.query_catalog(None, &request("")).unwrap();
    assert!(
        !physical
            .fields
            .iter()
            .any(|field| field.english.starts_with("ExtDimension"))
    );
}

/// Schedule resources and base-register columns come from their declared relations, with explicit positional signatures.
#[test]
fn calculation_virtual_catalog_resolves_schedule_resources_and_related_base_registers() {
    let (directory, _, _, _) = setup();
    let root = directory.0.join("src/Configuration.xml");
    fs::write(&root, fs::read_to_string(&root).unwrap().replace("</ChildObjects>", "<CalculationRegister>Main</CalculationRegister><CalculationRegister>Wages</CalculationRegister><ChartOfCalculationTypes>MainTypes</ChartOfCalculationTypes><ChartOfCalculationTypes>BaseTypes</ChartOfCalculationTypes><InformationRegister>Schedule</InformationRegister></ChildObjects>")).unwrap();
    calculation_descriptor(
        &directory.0,
        "CalculationRegister",
        "Main",
        "<ChartOfCalculationTypes>ChartOfCalculationTypes.MainTypes</ChartOfCalculationTypes><BasePeriodUse>true</BasePeriodUse><ActionPeriodUse>true</ActionPeriodUse><Schedule>InformationRegister.Schedule</Schedule>",
        "<Dimension uuid='11111111-1111-1111-1111-111111111111'><Properties><Name>Person</Name></Properties></Dimension>",
    );
    calculation_descriptor(
        &directory.0,
        "CalculationRegister",
        "Wages",
        "<ChartOfCalculationTypes>ChartOfCalculationTypes.BaseTypes</ChartOfCalculationTypes>",
        "<Dimension uuid='22222222-2222-2222-2222-222222222222'><Properties><Name>Employee</Name></Properties></Dimension><Resource uuid='33333333-3333-3333-3333-333333333333'><Properties><Name>Amount</Name></Properties></Resource>",
    );
    calculation_descriptor(
        &directory.0,
        "ChartOfCalculationTypes",
        "MainTypes",
        "<DependenceOnCalculationTypes>ActionPeriod</DependenceOnCalculationTypes><BaseCalculationTypes><Item>ChartOfCalculationTypes.BaseTypes</Item></BaseCalculationTypes>",
        "",
    );
    calculation_descriptor(
        &directory.0,
        "ChartOfCalculationTypes",
        "BaseTypes",
        "<DependenceOnCalculationTypes>DontUse</DependenceOnCalculationTypes>",
        "",
    );
    calculation_descriptor(
        &directory.0,
        "InformationRegister",
        "Schedule",
        "",
        "<Resource uuid='44444444-4444-4444-4444-444444444444'><Properties><Name>Hours</Name></Properties></Resource>",
    );
    let workspace = MetadataWorkspace::open(&directory.0, &[], false).unwrap();
    let project = &workspace.projects()[0];
    let request = |suffix: &str| QueryCatalogRequest {
        table: Some(format!("CalculationRegister.Main{suffix}")),
        virtual_tables: true,
        ..Default::default()
    };
    let page = project.query_catalog(None, &request("")).unwrap();
    assert!(
        page.tables
            .iter()
            .any(|table| table.english == "CalculationRegister.Main.BaseWages")
    );
    let base = project.query_catalog(None, &request(".BaseWages")).unwrap();
    assert!(base.found);
    assert_eq!(
        base.parameters
            .iter()
            .map(|value| value.required)
            .collect::<Vec<_>>(),
        [true, true, false, false]
    );
    for name in [
        "Person",
        "EmployeeDimension",
        "AmountBase",
        "CalculationType",
    ] {
        assert!(
            base.fields.iter().any(|field| field.english == name),
            "{name}"
        );
    }
    assert!(
        !project
            .query_catalog(None, &request(".BaseMain"))
            .unwrap()
            .found
    );
    let schedule = project
        .query_catalog(None, &request(".ScheduleData"))
        .unwrap();
    for name in [
        "HoursBasePeriod",
        "HoursActivePeriod",
        "HoursRegistrationPeriod",
        "HoursActualActionPeriod",
    ] {
        assert!(
            schedule.fields.iter().any(|field| field.english == name),
            "{name}"
        );
    }
    assert!(!schedule.fields.iter().any(|field| field.english == "Hours"));
}

/// Small Designer descriptors exercise real declared identities; files never escape this test's owned playground.
fn calculation_descriptor(root: &Path, kind: &str, name: &str, properties: &str, children: &str) {
    let folder = match kind {
        "ChartOfAccounts" => "ChartsOfAccounts",
        "CalculationRegister" => "CalculationRegisters",
        "InformationRegister" => "InformationRegisters",
        "ChartOfCalculationTypes" => "ChartsOfCalculationTypes",
        _ => panic!("fixture kind"),
    };
    let folder = root.join("src").join(folder);
    fs::create_dir_all(&folder).unwrap();
    fs::write(folder.join(format!("{name}.xml")), format!("<MetaDataObject xmlns='http://v8.1c.ru/8.3/MDClasses'><{kind} uuid='99999999-9999-9999-9999-999999999999'><Properties><Name>{name}</Name>{properties}</Properties><ChildObjects>{children}</ChildObjects></{kind}></MetaDataObject>")).unwrap();
}

/// A derived table exposes its own first projection and expands qualified wildcards using the same application metadata.
#[test]
fn nested_catalog_projects_aliases_and_wildcards_without_client_parsing() {
    use crate::backend::project::query::constructor::{Constructor, Selection};
    let (_directory, workspace, _, _) = setup();
    let constructor = Constructor::new("SELECT Q.Ref FROM (SELECT I.*, I.ИНН AS Tax FROM Catalog.Контрагенты AS I UNION ALL SELECT X.*, X.ИНН AS Other FROM Catalog.Контрагенты AS X) AS Q".into()).unwrap();
    let page = workspace.projects()[0]
        .query_constructor_catalog(
            &constructor,
            None,
            &QueryCatalogRequest {
                source: Some(Selection {
                    statement: 0,
                    clause: 1,
                    item: 0,
                }),
                qualifier: Some("Q".into()),
                ..Default::default()
            },
        )
        .unwrap();
    assert!(page.fields.iter().any(|field| field.english == "Q.Ref"));
    assert!(page.fields.iter().any(|field| field.english == "Q.Tax"));
    assert!(!page.fields.iter().any(|field| field.english == "Q.Other"));
}

/// Virtual choices depend on the register type and period setting rather than a universal client inventory.
#[test]
fn virtual_catalog_uses_real_register_availability_resources_and_parameter_positions() {
    let (directory, workspace, _, _) = setup();
    let path = directory.0.join("src/AccumulationRegisters/Остатки.xml");
    let descriptor = "<MetaDataObject xmlns='http://v8.1c.ru/8.3/MDClasses'><AccumulationRegister uuid='11111111-1111-1111-1111-111111111111'><Properties><Name>Остатки</Name><RegisterType>Balance</RegisterType></Properties><ChildObjects><Dimension uuid='22222222-2222-2222-2222-222222222222'><Properties><Name>Товар</Name></Properties></Dimension><Resource uuid='33333333-3333-3333-3333-333333333333'><Properties><Name>Количество</Name></Properties></Resource></ChildObjects></AccumulationRegister></MetaDataObject>";
    fs::write(&path, descriptor).unwrap();
    let before = bytes(&directory.0);
    let project = &workspace.projects()[0];
    let page = project
        .query_catalog(
            None,
            &QueryCatalogRequest {
                table: Some("AccumulationRegister.Остатки".into()),
                virtual_tables: true,
                ..Default::default()
            },
        )
        .unwrap();
    assert_eq!(page.tables.len(), 3);
    let fields = project
        .query_catalog(
            None,
            &QueryCatalogRequest {
                table: Some("AccumulationRegister.Остатки.BalanceAndTurnovers".into()),
                qualifier: Some("О".into()),
                virtual_tables: true,
                ..Default::default()
            },
        )
        .unwrap();
    assert!(fields.found);
    assert_eq!(
        fields
            .parameters
            .iter()
            .map(|value| value.key)
            .collect::<Vec<_>>(),
        [
            "beginPeriod",
            "endPeriod",
            "periodicity",
            "complementMethod",
            "condition"
        ]
    );
    assert!(
        fields
            .fields
            .iter()
            .any(|field| field.english == "О.КоличествоOpeningBalance")
    );
    assert!(
        fields
            .fields
            .iter()
            .any(|field| field.english == "О.КоличествоTurnover")
    );
    assert!(
        !fields
            .fields
            .iter()
            .any(|field| field.english == "О.Количество")
    );
    assert_eq!(bytes(&directory.0), before);
    fs::write(&path, descriptor.replace(">Balance<", ">Turnovers<")).unwrap();
    let page = project
        .query_catalog(
            None,
            &QueryCatalogRequest {
                table: Some("AccumulationRegister.Остатки".into()),
                virtual_tables: true,
                ..Default::default()
            },
        )
        .unwrap();
    assert_eq!(page.tables.len(), 1);
    let missing = project
        .query_catalog(
            None,
            &QueryCatalogRequest {
                table: Some("AccumulationRegister.Остатки.Balance".into()),
                virtual_tables: true,
                ..Default::default()
            },
        )
        .unwrap();
    assert!(!missing.found);
}
