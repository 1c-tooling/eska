//! Current form bindings provide observable semantic behavior without platform or XML export.

use std::{fmt::Write as _, fs, path::Path};

pub const BODY: &str = "\u{feff}// Current handler bindings.\r\n&НаКлиенте\r\nПроцедура SavedHandler()\r\nКонецПроцедуры\r\n&НаКлиенте\r\nПроцедура CurrentHandler()\r\nКонецПроцедуры\r\n";

/// Keep form contents rich enough to detect loss of element, column and event dependencies.
pub fn dialog(handler: &str) -> String {
    format!(
        "\u{feff}<Form xmlns=\"http://v8.1c.ru/8.3/xcf/logform\" xmlns:xs=\"http://www.w3.org/2001/XMLSchema\"><Events><Event name=\"OnOpen\">{handler}</Event></Events><ChildItems><UsualGroup name=\"Group\" id=\"1\"><ChildItems><InputField name=\"Value\" id=\"2\"><DataPath>Object.Code</DataPath></InputField></ChildItems></UsualGroup></ChildItems><Commands><Command name=\"Run\"><Action>{handler}</Action></Command></Commands><Attributes><Attribute name=\"Object\"><Type><Type>xs:string</Type></Type><MainAttribute>true</MainAttribute><Columns><Column name=\"Code\"><Type><Type>xs:string</Type></Type></Column></Columns></Attribute></Attributes></Form>\r\n"
    )
}

/// Import only this test's native fixture, optionally including explicitly unsupported dialogs.
pub fn add_forms(root: &Path, native: &Path, absent: bool) {
    let mut names = vec!["Dialog", "Neighbor"];
    if absent {
        names.extend(["Absent", "OrdinaryBinary"]);
    }
    let configuration = fs::read_to_string(native.join("Configuration.xml")).unwrap();
    let children = names.iter().fold(String::new(), |mut children, name| {
        write!(children, "<CommonForm>{name}</CommonForm>").unwrap();
        children
    });
    fs::write(
        native.join("Configuration.xml"),
        configuration.replace("</ChildObjects>", &(children + "</ChildObjects>")),
    )
    .unwrap();
    fs::create_dir(native.join("CommonForms")).unwrap();
    for (index, name) in names.iter().enumerate() {
        let form_type = if *name == "OrdinaryBinary" {
            "Ordinary"
        } else {
            "Managed"
        };
        fs::write(native.join(format!("CommonForms/{name}.xml")), format!("<MetaDataObject xmlns=\"http://v8.1c.ru/8.3/MDClasses\" version=\"2.20\"><CommonForm uuid=\"80000000-0000-4000-8000-00000000000{}\"><Properties><Name>{name}</Name><FormType>{form_type}</FormType></Properties></CommonForm></MetaDataObject>", index + 1)).unwrap();
        let directory = native.join(format!("CommonForms/{name}/Ext"));
        fs::create_dir_all(directory.join("Form")).unwrap();
        fs::write(directory.join("Form/Module.bsl"), BODY).unwrap();
        if matches!(*name, "Dialog" | "Neighbor") {
            fs::write(directory.join("Form.xml"), dialog("SavedHandler")).unwrap();
        } else if *name == "OrdinaryBinary" {
            fs::write(directory.join("Form.bin"), [0xff, 0x00]).unwrap();
        }
    }
    // A preserved stray file must not activate a form or become an editor input.
    fs::create_dir_all(native.join("CommonForms/Stray/Ext")).unwrap();
    fs::write(
        native.join("CommonForms/Stray/Ext/Form.xml"),
        dialog("StrayHandler"),
    )
    .unwrap();
    eska::project_core::source_format::import_designer(native, &root.join("src-with-forms"))
        .unwrap();
    let configuration = fs::read_to_string(root.join("eska.toml")).unwrap();
    fs::write(
        root.join("eska.toml"),
        configuration.replace(
            "source_format='eska'",
            "source_format='eska'\nsource='src-with-forms'",
        ),
    )
    .unwrap();
}

/// Equal short names and sibling dialogs exercise full identities without modifying user fixtures.
pub fn add_nested_forms(root: &Path, native: &Path, absent: bool) {
    let configuration = fs::read_to_string(native.join("Configuration.xml")).unwrap();
    fs::write(
        native.join("Configuration.xml"),
        configuration.replace(
            "</ChildObjects>",
            "<Catalog>Items</Catalog><Catalog>Orders</Catalog></ChildObjects>",
        ),
    )
    .unwrap();
    for (index, name) in ["Items", "Orders"].iter().enumerate() {
        let mut forms = vec!["Shared"];
        if *name == "Items" {
            forms.push("Other");
            if absent {
                forms.extend(["Absent", "Binary"]);
            }
        }
        let children = forms.iter().fold(String::new(), |mut children, form| {
            write!(children, "<Form>{form}</Form>").unwrap();
            children
        });
        fs::create_dir_all(native.join(format!("Catalogs/{name}/Forms"))).unwrap();
        fs::write(native.join(format!("Catalogs/{name}.xml")), format!("<MetaDataObject xmlns=\"http://v8.1c.ru/8.3/MDClasses\" version=\"2.20\"><Catalog uuid=\"90000000-0000-4000-8000-{:012}\"><Properties><Name>{name}</Name></Properties><ChildObjects>{children}</ChildObjects></Catalog></MetaDataObject>", index + 1)).unwrap();
        for (form_index, form) in forms.iter().enumerate() {
            let form_type = if *form == "Binary" {
                "Ordinary"
            } else {
                "Managed"
            };
            fs::write(native.join(format!("Catalogs/{name}/Forms/{form}.xml")), format!("<MetaDataObject xmlns=\"http://v8.1c.ru/8.3/MDClasses\" version=\"2.20\"><Form uuid=\"90000000-0000-4000-8000-{:012}\"><Properties><Name>{form}</Name><FormType>{form_type}</FormType></Properties></Form></MetaDataObject>", 10 * (index + 1) + form_index)).unwrap();
            let directory = native.join(format!("Catalogs/{name}/Forms/{form}/Ext"));
            fs::create_dir_all(directory.join("Form")).unwrap();
            fs::write(directory.join("Form/Module.bsl"), BODY).unwrap();
            if *form == "Binary" {
                fs::write(directory.join("Form.bin"), [0xff, 0x00]).unwrap();
            } else if *form != "Absent" {
                fs::write(
                    directory.join("Form.xml"),
                    dialog(if *name == "Orders" {
                        "CurrentHandler"
                    } else {
                        "SavedHandler"
                    }),
                )
                .unwrap();
            }
        }
    }
    fs::create_dir_all(native.join("Catalogs/Items/Forms/Stray/Ext")).unwrap();
    fs::write(
        native.join("Catalogs/Items/Forms/Stray/Ext/Form.xml"),
        dialog("StrayHandler"),
    )
    .unwrap();
    eska::project_core::source_format::import_designer(native, &root.join("src-with-nested-forms"))
        .unwrap();
    let configuration = fs::read_to_string(root.join("eska.toml")).unwrap();
    fs::write(
        root.join("eska.toml"),
        configuration.replace(
            "source_format='eska'",
            "source_format='eska'\nsource='src-with-nested-forms'",
        ),
    )
    .unwrap();
}
