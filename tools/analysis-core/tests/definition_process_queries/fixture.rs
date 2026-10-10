//! Process, task and exchange declarations are shared by library and stdio tests.

use std::{fs, path::Path};

/// Factory and collection spellings come from the existing platform catalogue and kernel.
pub struct Case {
    pub folder: &'static str,
    pub kind: &'static str,
    pub name: &'static str,
    pub namespace: &'static str,
    pub factory: &'static str,
    pub index: u32,
}

pub const CASES: [Case; 3] = [
    Case {
        folder: "BusinessProcesses",
        kind: "BusinessProcess",
        name: "Flow",
        namespace: "BusinessProcesses",
        factory: "CreateBusinessProcess",
        index: 1,
    },
    Case {
        folder: "Tasks",
        kind: "Task",
        name: "Work",
        namespace: "Задачи",
        factory: "СоздатьЗадачу",
        index: 2,
    },
    Case {
        folder: "ExchangePlans",
        kind: "ExchangePlan",
        name: "Sync",
        namespace: "ExchangePlans",
        factory: "CreateNode",
        index: 3,
    },
];

/// Unique native identities distinguish both each object kind and every scoped field.
pub fn uuid(case: &Case, part: u32) -> String {
    format!(
        "{:08x}-0000-4000-8000-{:012}",
        0x8200_0000 + part,
        case.index
    )
}

/// Extend only the test's private native export before a fresh import into another owned directory.
pub fn extend(native: &Path) {
    let file = native.join("Configuration.xml");
    let mut configuration = fs::read_to_string(&file).unwrap();
    for case in &CASES {
        let reference = format!("<{}>{}</{}>", case.kind, case.name, case.kind);
        configuration =
            configuration.replacen("</ChildObjects>", &format!("{reference}</ChildObjects>"), 1);
        let addressing = if case.kind == "Task" {
            format!(
                "<AddressingAttribute uuid=\"{}\"><Properties><Name>Assignee</Name><Type><v8:Type>xs:boolean</v8:Type></Type></Properties></AddressingAttribute>",
                uuid(case, 4)
            )
        } else {
            String::new()
        };
        let text = format!(
            r#"<MetaDataObject xmlns="http://v8.1c.ru/8.3/MDClasses" xmlns:v8="http://v8.1c.ru/8.1/data/core" xmlns:xs="http://www.w3.org/2001/XMLSchema" version="2.20"><{kind} uuid="{root}"><Properties><Name>{name}</Name><CodeLength>9</CodeLength><NumberLength>12</NumberLength></Properties><ChildObjects><Attribute uuid="{field}"><Properties><Name>ExtraField</Name><Type><v8:Type>xs:string</v8:Type></Type></Properties></Attribute>{addressing}<TabularSection uuid="{table}"><Properties><Name>Rows</Name></Properties><ChildObjects><Attribute uuid="{column}"><Properties><Name>Quantity</Name><Type><v8:Type>xs:decimal</v8:Type></Type></Properties></Attribute></ChildObjects></TabularSection></ChildObjects></{kind}></MetaDataObject>"#,
            kind = case.kind,
            name = case.name,
            root = uuid(case, 0),
            field = uuid(case, 1),
            table = uuid(case, 2),
            column = uuid(case, 3)
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

/// A typed factory is independent of the selected member expression, with Unicode transport coordinates.
pub fn body(case: &Case, receiver: &str, field: &str) -> String {
    format!(
        "\u{feff}// Я😀 current process fields\r\nProcedure Probe()\r\nObject={}.{}.{}();\r\n{receiver}\r\nValue=Object.{field};\r\nEndProcedure\r\n",
        case.namespace, case.name, case.factory
    )
}
