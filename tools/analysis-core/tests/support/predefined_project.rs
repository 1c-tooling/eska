//! Declared predefined data shares exact bytes across resident and process tests.

use std::{fmt::Write as _, fs, path::Path};

/// Each resource includes an ordered child, a native UUID and BOM/CRLF with non-BMP text.
pub fn data(name: &str) -> String {
    format!(
        "\u{feff}<PredefinedData xmlns=\"http://v8.1c.ru/8.3/xcf/predef\"><!-- Я😀 --><Item id=\"91000000-0000-4000-8000-000000000010\"><Name>{name}</Name><ChildItems><Item id=\"91000000-0000-4000-8000-000000000011\"><Name>Child</Name></Item></ChildItems></Item></PredefinedData>\r\n"
    )
}

/// Extend only this owned native tree, then import a new project source with all four families.
pub fn add_predefined(root: &Path, native: &Path) {
    let mut children = String::new();
    for (index, (folder, kind, name, predefined)) in [
        ("Catalogs", "Catalog", "Items", Some("Saved")),
        ("Catalogs", "Catalog", "Neighbor", Some("NeighborItem")),
        ("Catalogs", "Catalog", "Absent", None),
        (
            "ChartsOfAccounts",
            "ChartOfAccounts",
            "Accounts",
            Some("Account"),
        ),
        (
            "ChartsOfCalculationTypes",
            "ChartOfCalculationTypes",
            "Calculations",
            Some("Calculation"),
        ),
        (
            "ChartsOfCharacteristicTypes",
            "ChartOfCharacteristicTypes",
            "Characteristics",
            Some("Characteristic"),
        ),
    ]
    .into_iter()
    .enumerate()
    {
        write!(children, "<{kind}>{name}</{kind}>").unwrap();
        fs::create_dir_all(native.join(folder)).unwrap();
        let forms = if name == "Items" {
            "<ChildObjects><Form>Shared</Form></ChildObjects>"
        } else {
            ""
        };
        fs::write(native.join(format!("{folder}/{name}.xml")), format!("<MetaDataObject xmlns=\"http://v8.1c.ru/8.3/MDClasses\" version=\"2.20\"><{kind} uuid=\"91000000-0000-4000-8000-{:012}\"><Properties><Name>{name}</Name></Properties>{forms}</{kind}></MetaDataObject>", index + 1)).unwrap();
        if let Some(item_name) = predefined {
            let directory = native.join(format!("{folder}/{name}/Ext"));
            fs::create_dir_all(&directory).unwrap();
            fs::write(directory.join("Predefined.xml"), data(item_name)).unwrap();
        }
    }
    let configuration = fs::read_to_string(native.join("Configuration.xml")).unwrap();
    fs::write(
        native.join("Configuration.xml"),
        configuration.replace("</ChildObjects>", &(children + "</ChildObjects>")),
    )
    .unwrap();
    let form = native.join("Catalogs/Items/Forms/Shared");
    fs::create_dir_all(form.join("Ext/Form")).unwrap();
    fs::write(form.with_extension("xml"), "<MetaDataObject xmlns=\"http://v8.1c.ru/8.3/MDClasses\" version=\"2.20\"><Form uuid=\"91000000-0000-4000-8000-000000000020\"><Properties><Name>Shared</Name><FormType>Managed</FormType></Properties></Form></MetaDataObject>").unwrap();
    fs::write(form.join("Ext/Form.xml"), "<Form/>").unwrap();
    fs::write(form.join("Ext/Form/Module.bsl"), "// Current form body.\n").unwrap();
    fs::create_dir_all(native.join("Catalogs/Stray/Ext")).unwrap();
    fs::write(
        native.join("Catalogs/Stray/Ext/Predefined.xml"),
        data("Stray"),
    )
    .unwrap();
    eska::project_core::source_format::import_designer(native, &root.join("src-with-predefined"))
        .unwrap();
    let configuration = fs::read_to_string(root.join("eska.toml")).unwrap();
    fs::write(
        root.join("eska.toml"),
        configuration.replace(
            "source_format='eska'",
            "source_format='eska'\nsource='src-with-predefined'",
        ),
    )
    .unwrap();
}
