//! Tiny native register declarations are shared by library and framed process tests.

use std::{fs, path::Path};

/// Each flavour has distinct identities despite sharing its field names.
pub struct Case {
    pub folder: &'static str,
    pub kind: &'static str,
    pub name: &'static str,
    pub namespace: &'static str,
    pub index: u32,
}

/// A record of each supported register flavour uses the same scoped field contract.
pub const CASES: [Case; 4] = [
    Case {
        folder: "InformationRegisters",
        kind: "InformationRegister",
        name: "Rates",
        namespace: "InformationRegisters",
        index: 1,
    },
    Case {
        folder: "AccumulationRegisters",
        kind: "AccumulationRegister",
        name: "Balance",
        namespace: "РегистрыНакопления",
        index: 2,
    },
    Case {
        folder: "AccountingRegisters",
        kind: "AccountingRegister",
        name: "Ledger",
        namespace: "AccountingRegisters",
        index: 3,
    },
    Case {
        folder: "CalculationRegisters",
        kind: "CalculationRegister",
        name: "Payroll",
        namespace: "РегистрыРасчета",
        index: 4,
    },
];

/// Stable unique fixture identities distinguish root, dimension, resource and attribute.
pub fn uuid(case: &Case, part: u32) -> String {
    format!(
        "{:08x}-0000-4000-8000-{:012}",
        0x7100_0000 + part,
        case.index
    )
}

/// Add real-shaped typed declarations only to a test-owned native export before its import.
pub fn extend(native: &Path) {
    let file = native.join("Configuration.xml");
    let mut configuration = fs::read_to_string(&file).unwrap();
    for case in &CASES {
        let reference = format!("<{}>{}</{}>", case.kind, case.name, case.kind);
        if !configuration.contains(&reference) {
            configuration = configuration.replacen(
                "</ChildObjects>",
                &format!("{reference}</ChildObjects>"),
                1,
            );
        }
        let properties = match case.kind {
            "InformationRegister" => {
                "<InformationRegisterPeriodicity>Year</InformationRegisterPeriodicity><WriteMode>Independent</WriteMode>"
            }
            "AccumulationRegister" => "<RegisterType>Balance</RegisterType>",
            _ => "",
        };
        let root = uuid(case, 0);
        let dimension = uuid(case, 1);
        let resource = uuid(case, 2);
        let attribute = uuid(case, 3);
        let text = format!(
            "<MetaDataObject xmlns=\"http://v8.1c.ru/8.3/MDClasses\" xmlns:v8=\"http://v8.1c.ru/8.1/data/core\" xmlns:xs=\"http://www.w3.org/2001/XMLSchema\" version=\"2.20\"><{kind} uuid=\"{root}\"><Properties><Name>{name}</Name>{properties}</Properties><ChildObjects><Dimension uuid=\"{dimension}\"><Properties><Name>Key</Name><Type><v8:Type>xs:string</v8:Type></Type></Properties></Dimension><Resource uuid=\"{resource}\"><Properties><Name>Amount</Name><Type><v8:Type>xs:decimal</v8:Type></Type></Properties></Resource><Attribute uuid=\"{attribute}\"><Properties><Name>Note</Name><Type><v8:Type>xs:string</v8:Type></Type></Properties></Attribute></ChildObjects></{kind}></MetaDataObject>",
            kind = case.kind,
            name = case.name,
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

/// Field names remain separate from the receiver expression used by each scenario.
pub fn body(case: &Case, receiver: &str, field: &str) -> String {
    let prefix = format!(
        "Collection={};\r\nSet=Collection.{}.CreateRecordSet();\r\n",
        case.namespace, case.name
    );
    format!(
        "\u{feff}// Я😀 register member\r\nProcedure Probe()\r\n{prefix}{receiver}\r\nValue=Record.{field};\r\nEndProcedure\r\n"
    )
}
