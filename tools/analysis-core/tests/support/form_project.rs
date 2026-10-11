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
