//! Chart fixtures declare real flags beside the implicit row that contains them in BSL.

use std::{fs, path::Path};

/// Factories and collection names come from the existing platform/kernel catalogue.
pub struct Case {
    pub folder: &'static str,
    pub kind: &'static str,
    pub name: &'static str,
    pub namespace: &'static str,
    pub factory: &'static str,
    pub properties: &'static str,
    pub index: u32,
}

pub const CASES: [Case; 3] = [
    Case {
        folder: "ChartsOfCharacteristicTypes",
        kind: "ChartOfCharacteristicTypes",
        name: "Kinds",
        namespace: "ПланыВидовХарактеристик",
        factory: "СоздатьЭлемент",
        properties: "",
        index: 1,
    },
    Case {
        folder: "ChartsOfAccounts",
        kind: "ChartOfAccounts",
        name: "Accounts",
        namespace: "ChartsOfAccounts",
        factory: "CreateAccount",
        properties: "<ExtDimensionTypes>ChartOfCharacteristicTypes.Kinds</ExtDimensionTypes>",
        index: 2,
    },
    Case {
        folder: "ChartsOfCalculationTypes",
        kind: "ChartOfCalculationTypes",
        name: "Calculations",
        namespace: "ПланыВидовРасчета",
        factory: "СоздатьВидРасчета",
        properties: "<DependenceOnCalculationTypes>OnActionPeriod</DependenceOnCalculationTypes>",
        index: 3,
    },
];

/// Distinct real identities cover root, attribute, section, column and both flag levels.
pub fn uuid(case: &Case, part: u32) -> String {
    format!(
        "{:08x}-0000-4000-8000-{:012}",
        0x8300_0000 + part,
        case.index
    )
}

/// Expand only a test's own native fixture before importing its canonical sources.
pub fn extend(native: &Path) {
    let file = native.join("Configuration.xml");
    let mut configuration = fs::read_to_string(&file).unwrap();
    for case in &CASES {
        let reference = format!("<{}>{}</{}>", case.kind, case.name, case.kind);
        configuration =
            configuration.replacen("</ChildObjects>", &format!("{reference}</ChildObjects>"), 1);
        let flags = if case.kind == "ChartOfAccounts" {
            format!(
                "<AccountingFlag uuid=\"{}\"><Properties><Name>Accounting</Name><Type><v8:Type>xs:boolean</v8:Type></Type></Properties></AccountingFlag><ExtDimensionAccountingFlag uuid=\"{}\"><Properties><Name>QuantityAccounting</Name><Type><v8:Type>xs:boolean</v8:Type></Type></Properties></ExtDimensionAccountingFlag>",
                uuid(case, 4),
                uuid(case, 5)
            )
        } else {
            String::new()
        };
        let text = format!(
            r#"<MetaDataObject xmlns="http://v8.1c.ru/8.3/MDClasses" xmlns:v8="http://v8.1c.ru/8.1/data/core" xmlns:xs="http://www.w3.org/2001/XMLSchema" version="2.20"><{kind} uuid="{root}"><Properties><Name>{name}</Name><CodeLength>9</CodeLength><DescriptionLength>100</DescriptionLength>{properties}</Properties><ChildObjects><Attribute uuid="{field}"><Properties><Name>ExtraField</Name><Type><v8:Type>xs:string</v8:Type></Type></Properties></Attribute>{flags}<TabularSection uuid="{table}"><Properties><Name>Rows</Name></Properties><ChildObjects><Attribute uuid="{column}"><Properties><Name>Quantity</Name><Type><v8:Type>xs:decimal</v8:Type></Type></Properties></Attribute></ChildObjects></TabularSection></ChildObjects></{kind}></MetaDataObject>"#,
            kind = case.kind,
            name = case.name,
            root = uuid(case, 0),
            field = uuid(case, 1),
            table = uuid(case, 2),
            column = uuid(case, 3),
            properties = case.properties
        );
        fs::create_dir_all(native.join(case.folder)).unwrap();
        fs::write(
            native.join(case.folder).join(format!("{}.xml", case.name)),
            text,
        )
        .unwrap();
    }
    fs::write(file, configuration).unwrap();
}

/// Unicode/newlines exercise current byte/UTF-16 coordinates independently of the selected field.
pub fn body(case: &Case, receiver: &str, field: &str) -> String {
    format!(
        "\u{feff}// Я😀 chart members\r\nProcedure Probe()\r\nObject={}.{}.{}();\r\n{receiver}\r\nValue=Object.{field};\r\nEndProcedure\r\n",
        case.namespace, case.name, case.factory
    )
}
