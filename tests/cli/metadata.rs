use crate::support::TestDir;
use serde_json::{Value, json};
use std::{
    fs,
    io::Write,
    process::{Command, Stdio},
};

/// Execute the public JSON CLI with stdin, checking that no localized prose leaks into stdout.
fn call(
    root: &TestDir,
    locale: &str,
    operation: &str,
    input: Option<&Value>,
    success: bool,
) -> Value {
    let mut command = Command::new(env!("CARGO_BIN_EXE_eska"));
    command
        .current_dir(&root.0)
        .args(["--lang", locale, "metadata", operation]);
    if input.is_some() {
        command.arg("-");
    }
    let mut child = command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    if let Some(input) = input {
        child
            .stdin
            .take()
            .unwrap()
            .write_all(&serde_json::to_vec(input).unwrap())
            .unwrap();
    }
    let output = child.wait_with_output().unwrap();
    assert_eq!(
        output.status.success(),
        success,
        "{}",
        String::from_utf8_lossy(&output.stdout)
    );
    assert!(
        output.stderr.is_empty(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let result: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(result["schema_version"], 1);
    result
}

#[test]
fn metadata_json_schema_preview_apply_and_conflict_are_locale_independent() {
    let root = TestDir::new();
    fs::create_dir_all(root.0.join("src/Ext")).unwrap();
    fs::write(
        root.0.join("eska.toml"),
        "[project]\ntype='configuration'\n",
    )
    .unwrap();
    let path = root.0.join("src/Configuration.xml");
    let original = "\u{feff}<MetaDataObject xmlns='http://v8.1c.ru/8.3/MDClasses' xmlns:xsi='http://www.w3.org/2001/XMLSchema-instance' xmlns:xs='http://www.w3.org/2001/XMLSchema'><Configuration uuid='11111111-1111-1111-1111-111111111111'><Properties><Name>Demo</Name><Comment>A&amp;B</Comment><ScriptVariant>Russian</ScriptVariant><Unknown xsi:type='xs:string'>opaque</Unknown></Properties><ChildObjects/></Configuration></MetaDataObject>\r\n";
    fs::write(&path, original).unwrap();
    fs::write(
        root.0.join("src/Ext/ParentConfigurations.bin"),
        "{6,0,0,0,0,0}",
    )
    .unwrap();
    let ru = call(&root, "ru-RU", "inspect", None, true);
    let en = call(&root, "en-US", "inspect", None, true);
    assert_eq!(ru, en);
    let object = &ru["result"]["object"]["objectId"];
    let editing = &ru["result"]["editing"];
    let fields = editing["fields"].as_array().unwrap();
    assert!(!fields.iter().any(|field| {
        ["Name", "Unknown"].contains(&field["path"][0]["key"]["name"].as_str().unwrap())
    }));
    let field = fields
        .iter()
        .find(|field| field["path"][0]["key"]["name"] == "Comment")
        .unwrap();
    let request = json!({"schemaVersion":1,"objectId":object,"snapshot":editing["snapshot"],"path":field["path"],"change":{"kind":"text","value":"<New>"}});
    let checked = call(&root, "ru-RU", "check", Some(&request), true);
    assert_eq!(checked, call(&root, "en-US", "check", Some(&request), true));
    assert_eq!(checked["result"]["changes"].as_array().unwrap().len(), 1);
    assert_eq!(fs::read_to_string(&path).unwrap(), original);
    call(&root, "ru-RU", "apply", Some(&request), true);
    let expected = original.replace("A&amp;B", "&lt;New&gt;");
    assert_eq!(fs::read_to_string(&path).unwrap(), expected);
    let conflict = call(&root, "en-US", "apply", Some(&request), false);
    assert_eq!(conflict["error"]["kind"], "property_conflict");
    let current = call(&root, "ru-RU", "inspect", None, true);
    let field = fields
        .iter()
        .find(|field| field["path"][0]["key"]["name"] == "ScriptVariant")
        .unwrap();
    let invalid = json!({"schemaVersion":1,"objectId":object,"snapshot":current["result"]["editing"]["snapshot"],"path":field["path"],"change":{"kind":"text","value":"InventedByAI"}});
    assert_eq!(
        call(&root, "en-US", "apply", Some(&invalid), false)["error"]["kind"],
        "property_invalid"
    );
    assert_eq!(fs::read_to_string(&path).unwrap(), expected);
    assert!(!root.0.join(".eska").exists());
}
