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
    assert!(!root.0.join(".eska/metadata/rename").exists());
    assert!(root.0.join(".eska/metadata/write.lock").is_file());
}

/// AI callers receive the same closed reference domain in either locale; arbitrary names cannot write.
#[test]
fn metadata_reference_choices_validate_the_public_json_contract() {
    let root = TestDir::new();
    fs::create_dir_all(root.0.join("src/Ext")).unwrap();
    fs::create_dir_all(root.0.join("src/CommonForms")).unwrap();
    fs::write(
        root.0.join("eska.toml"),
        "[project]\ntype='configuration'\n",
    )
    .unwrap();
    let path = root.0.join("src/Configuration.xml");
    let original = "<MetaDataObject xmlns='http://v8.1c.ru/8.3/MDClasses'><Configuration uuid='11111111-1111-1111-1111-111111111111'><Properties><Name>Demo</Name><DefaultReportForm/></Properties><ChildObjects><CommonForm>Report</CommonForm></ChildObjects></Configuration></MetaDataObject>";
    fs::write(&path, original).unwrap();
    fs::write(
        root.0.join("src/Ext/ParentConfigurations.bin"),
        "{6,0,0,0,0,0}",
    )
    .unwrap();
    fs::write(root.0.join("src/CommonForms/Report.xml"), "<MetaDataObject xmlns='http://v8.1c.ru/8.3/MDClasses'><CommonForm uuid='22222222-2222-2222-2222-222222222222'><Properties><Name>Report</Name></Properties></CommonForm></MetaDataObject>").unwrap();
    let state = call(&root, "ru-RU", "inspect", None, true);
    let editing = &state["result"]["editing"];
    let field = &editing["fields"][0];
    assert_eq!(field["schema"]["kind"], "reference");
    let query = json!({"schemaVersion":1,"objectId":state["result"]["object"]["objectId"],"path":field["path"]});
    let choices = call(&root, "ru-RU", "choices", Some(&query), true);
    assert_eq!(choices, call(&root, "en-US", "choices", Some(&query), true));
    assert_eq!(
        choices["result"]["choices"][0]["value"],
        "CommonForm.Report"
    );
    let mut request = query;
    request["snapshot"] = editing["snapshot"].clone();
    request["change"] = json!({"kind":"text","value":"CommonForm.InventedByAI"});
    assert_eq!(
        call(&root, "en-US", "apply", Some(&request), false)["error"]["kind"],
        "property_invalid"
    );
    assert_eq!(fs::read_to_string(&path).unwrap(), original);
    request["change"]["value"] = choices["result"]["choices"][0]["value"].clone();
    call(&root, "ru-RU", "apply", Some(&request), true);
    assert_eq!(
        fs::read_to_string(&path).unwrap(),
        original.replace(
            "<DefaultReportForm/>",
            "<DefaultReportForm>CommonForm.Report</DefaultReportForm>"
        )
    );
}

/// Value schemas, typed writes and rejected invented types use the same locale-independent contract.
#[test]
fn metadata_value_schema_and_typed_changes_are_locale_independent() {
    let root = TestDir::new();
    fs::create_dir_all(root.0.join("src/Ext")).unwrap();
    fs::create_dir_all(root.0.join("src/Catalogs")).unwrap();
    fs::write(
        root.0.join("eska.toml"),
        "[project]\ntype='configuration'\n",
    )
    .unwrap();
    fs::write(
        root.0.join("src/Ext/ParentConfigurations.bin"),
        "{6,0,0,0,0,0}",
    )
    .unwrap();
    fs::write(root.0.join("src/Configuration.xml"), "<MetaDataObject xmlns='http://v8.1c.ru/8.3/MDClasses'><Configuration uuid='11111111-1111-1111-1111-111111111111'><Properties><Name>Demo</Name></Properties><ChildObjects><Catalog>Value</Catalog></ChildObjects></Configuration></MetaDataObject>").unwrap();
    let file = root.0.join("src/Catalogs/Value.xml");
    let original = "<MetaDataObject xmlns='http://v8.1c.ru/8.3/MDClasses' xmlns:v='http://v8.1c.ru/8.1/data/core' xmlns:xs='http://www.w3.org/2001/XMLSchema' xmlns:s='http://www.w3.org/2001/XMLSchema-instance'><Catalog uuid='22222222-2222-2222-2222-222222222222'><Properties><Name>Value</Name></Properties><ChildObjects><Attribute uuid='33333333-3333-3333-3333-333333333333'><Properties><Name>Target</Name><Type><v:Type>xs:boolean</v:Type></Type><FillValue s:nil='true'/></Properties></Attribute></ChildObjects></Catalog></MetaDataObject>";
    fs::write(&file, original).unwrap();
    let inspect = |locale| {
        let output = Command::new(env!("CARGO_BIN_EXE_eska"))
            .current_dir(&root.0)
            .args([
                "--lang",
                locale,
                "metadata",
                "inspect",
                "--object",
                "catalog:Value/attribute:Target",
            ])
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stdout)
        );
        assert!(output.stderr.is_empty());
        serde_json::from_slice::<Value>(&output.stdout).unwrap()
    };
    let ru = inspect("ru-RU");
    assert_eq!(ru, inspect("en-US"));
    let editing = &ru["result"]["editing"];
    let field = editing["fields"]
        .as_array()
        .unwrap()
        .iter()
        .find(|field| field["schema"]["kind"] == "value")
        .unwrap();
    assert!(field["schema"]["key"].is_null());
    assert_eq!(
        field["schema"]["types"][0]["constraints"]["kind"],
        "boolean"
    );
    let mut request = json!({"schemaVersion":1,"objectId":"catalog:Value/attribute:Target","snapshot":editing["snapshot"],"path":field["path"],"change":{"kind":"value","key":field["schema"]["types"][0]["key"],"value":"true"}});
    assert_eq!(
        call(&root, "ru-RU", "check", Some(&request), true),
        call(&root, "en-US", "check", Some(&request), true)
    );
    assert_eq!(fs::read_to_string(&file).unwrap(), original);
    request["change"]["key"]["name"] = json!("InventedByAI");
    assert_eq!(
        call(&root, "en-US", "apply", Some(&request), false)["error"]["kind"],
        "property_invalid"
    );
    request["change"]["key"]["name"] = json!("boolean");
    call(&root, "ru-RU", "apply", Some(&request), true);
    assert_eq!(
        fs::read_to_string(&file).unwrap(),
        original.replace(
            "<FillValue s:nil='true'/>",
            "<FillValue s:type=\"xs:boolean\">true</FillValue>"
        )
    );
    let current = inspect("en-US");
    let editing = &current["result"]["editing"];
    let field = editing["fields"]
        .as_array()
        .unwrap()
        .iter()
        .find(|field| field["schema"]["kind"] == "dataType")
        .unwrap();
    let change = json!({"schemaVersion":1,"objectId":"catalog:Value/attribute:Target","snapshot":editing["snapshot"],"path":field["path"],"change":{"kind":"dataType","key":{"namespace":"http://www.w3.org/2001/XMLSchema","name":"string"}}});
    let failure = call(&root, "en-US", "check", Some(&change), false);
    assert_eq!(failure, call(&root, "ru-RU", "apply", Some(&change), false));
    assert_eq!(failure["error"]["kind"], "property_dependency");
    assert_eq!(failure["error"]["details"]["property"]["name"], "FillValue");
    assert_eq!(current, inspect("en-US"));
    assert!(!root.0.join(".eska/metadata/rename").exists());
    assert!(root.0.join(".eska/metadata/write.lock").is_file());
}

/// Structural inspection uses one JSON contract in both languages and leaves source bytes untouched.
#[test]
fn metadata_rename_preview_is_locale_independent_and_never_applies_edits() {
    let root = TestDir::new();
    fs::create_dir_all(root.0.join("src")).unwrap();
    fs::write(
        root.0.join("eska.toml"),
        "[project]\ntype='configuration'\n",
    )
    .unwrap();
    let path = root.0.join("src/Configuration.xml");
    let original = "\u{feff}<MetaDataObject xmlns='http://v8.1c.ru/8.3/MDClasses'><Configuration uuid='11111111-1111-1111-1111-111111111111'><Properties><Name>Demo</Name><Comment>Demo</Comment></Properties><ChildObjects/></Configuration></MetaDataObject>\r\n";
    fs::write(&path, original).unwrap();
    let request = json!({"schemaVersion":1,"objectId":"configuration:Demo","newName":"Новая"});
    let ru = call(&root, "ru-RU", "rename-preview", Some(&request), true);
    let en = call(&root, "en-US", "rename-preview", Some(&request), true);
    assert_eq!(ru, en);
    assert_eq!(ru["result"]["applyAvailable"], false);
    assert_eq!(ru["result"]["plan"]["newObjectId"], "configuration:Новая");
    assert_eq!(
        ru["result"]["plan"]["files"][0]["replacements"][0]["after"],
        "Новая"
    );
    assert_eq!(
        ru["result"]["plan"]["files"][0]["uncertain"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    let mut invalid = request.clone();
    invalid["newName"] = json!("../bad");
    assert_eq!(
        call(&root, "en-US", "rename-preview", Some(&invalid), false)["error"]["kind"],
        "rename_invalid_name"
    );
    let mut injected = request;
    injected["path"] = json!("other.xml");
    assert_eq!(
        call(&root, "ru-RU", "rename-preview", Some(&injected), false)["error"]["kind"],
        "invalid_request"
    );
    assert_eq!(fs::read_to_string(path).unwrap(), original);
}

/// AI writes must follow a reviewed snapshot, and uncertain literal text is never changed automatically.
#[test]
fn metadata_rename_apply_requires_review_and_rejects_external_changes() {
    let root = TestDir::new();
    fs::create_dir_all(root.0.join("src/Ext")).unwrap();
    fs::write(
        root.0.join("eska.toml"),
        "[project]\ntype='configuration'\n",
    )
    .unwrap();
    fs::write(
        root.0.join("src/Ext/ParentConfigurations.bin"),
        "{6,0,0,0,0,0}",
    )
    .unwrap();
    let path = root.0.join("src/Configuration.xml");
    let original = "\u{feff}<MetaDataObject xmlns='http://v8.1c.ru/8.3/MDClasses'><Configuration uuid='11111111-1111-1111-1111-111111111111'><Properties><Name>Demo</Name><Comment>Demo</Comment></Properties><ChildObjects/></Configuration></MetaDataObject>\r\n";
    fs::write(&path, original).unwrap();
    let preview = json!({"schemaVersion":1,"objectId":"configuration:Demo","newName":"Новая"});
    let state = call(&root, "ru-RU", "rename-preview", Some(&preview), true);
    assert_eq!(state["result"]["applyAvailable"], true);
    let mut request = preview.clone();
    request["snapshot"] = state["result"]["plan"]["snapshot"].clone();
    request["reviewedUncertain"] = json!(false);
    let ru = call(&root, "ru-RU", "rename-apply", Some(&request), false);
    assert_eq!(ru["error"]["kind"], "rename_review_required");
    assert_eq!(
        ru,
        call(&root, "en-US", "rename-apply", Some(&request), false)
    );
    request["reviewedUncertain"] = json!(true);
    fs::write(root.0.join("src/added.bsl"), "// Demo").unwrap();
    assert_eq!(
        call(&root, "en-US", "rename-apply", Some(&request), false)["error"]["kind"],
        "property_conflict"
    );
    assert_eq!(fs::read_to_string(&path).unwrap(), original);
    let state = call(&root, "en-US", "rename-preview", Some(&preview), true);
    request["snapshot"] = state["result"]["plan"]["snapshot"].clone();
    let result = call(&root, "ru-RU", "rename-apply", Some(&request), true);
    assert_eq!(result["result"]["objectId"], "configuration:Новая");
    assert_eq!(
        fs::read_to_string(&path).unwrap(),
        original.replace("<Name>Demo</Name>", "<Name>Новая</Name>")
    );
    assert_eq!(
        call(&root, "en-US", "inspect", None, true)["result"]["object"]["objectId"],
        "configuration:Новая"
    );
    let status = call(
        &root,
        "en-US",
        "rename-recovery",
        Some(&json!({"schemaVersion":1,"action":"inspect"})),
        true,
    );
    assert_eq!(status["result"]["pending"], false);
}

/// AI callers see disabled prerequisites and newly enabled ranges through the same JSON schema.
#[test]
fn metadata_hierarchy_schema_changes_with_its_prerequisite_in_both_locales() {
    let root = TestDir::new();
    fs::create_dir_all(root.0.join("src/Ext")).unwrap();
    fs::create_dir_all(root.0.join("src/Catalogs")).unwrap();
    fs::write(
        root.0.join("eska.toml"),
        "[project]\ntype='configuration'\n",
    )
    .unwrap();
    fs::write(
        root.0.join("src/Ext/ParentConfigurations.bin"),
        "{6,0,0,0,0,0}",
    )
    .unwrap();
    fs::write(root.0.join("src/Configuration.xml"), "<MetaDataObject xmlns='http://v8.1c.ru/8.3/MDClasses'><Configuration uuid='11111111-1111-1111-1111-111111111111'><Properties><Name>Demo</Name></Properties><ChildObjects><Catalog>Levels</Catalog></ChildObjects></Configuration></MetaDataObject>").unwrap();
    let path = root.0.join("src/Catalogs/Levels.xml");
    let original = "\u{feff}<MetaDataObject xmlns='http://v8.1c.ru/8.3/MDClasses'><Catalog uuid='22222222-2222-2222-2222-222222222222'><Properties><Name>Levels</Name><Hierarchical>false</Hierarchical><LimitLevelCount>true</LimitLevelCount><LevelCount>2</LevelCount></Properties><ChildObjects/></Catalog></MetaDataObject>\r\n";
    fs::write(&path, original).unwrap();
    let inspect = |locale| {
        let output = Command::new(env!("CARGO_BIN_EXE_eska"))
            .current_dir(&root.0)
            .args([
                "--lang",
                locale,
                "metadata",
                "inspect",
                "--object",
                "catalog:Levels",
            ])
            .output()
            .unwrap();
        assert!(output.status.success());
        assert!(output.stderr.is_empty());
        serde_json::from_slice::<Value>(&output.stdout).unwrap()
    };
    let before = inspect("ru-RU");
    assert_eq!(before, inspect("en-US"));
    assert!(
        before["result"]["editing"]["readOnlyProperties"]
            .as_array()
            .unwrap()
            .iter()
            .any(|field| field["key"]["name"] == "LevelCount"
                && field["reason"] == "hierarchy_disabled")
    );
    let editing = &before["result"]["editing"];
    let field = editing["fields"]
        .as_array()
        .unwrap()
        .iter()
        .find(|field| field["path"][0]["key"]["name"] == "Hierarchical")
        .unwrap();
    let request = json!({"schemaVersion":1,"objectId":"catalog:Levels","snapshot":editing["snapshot"],"path":field["path"],"change":{"kind":"text","value":"true"}});
    call(&root, "ru-RU", "apply", Some(&request), true);
    let after = inspect("en-US");
    assert_eq!(after, inspect("ru-RU"));
    let editing = &after["result"]["editing"];
    let field = editing["fields"]
        .as_array()
        .unwrap()
        .iter()
        .find(|field| field["path"][0]["key"]["name"] == "LevelCount")
        .unwrap();
    assert_eq!(field["schema"], json!({"kind":"integer","min":2,"max":10}));
    let invalid = json!({"schemaVersion":1,"objectId":"catalog:Levels","snapshot":editing["snapshot"],"path":field["path"],"change":{"kind":"text","value":"11"}});
    let error = call(&root, "en-US", "apply", Some(&invalid), false);
    assert_eq!(error["error"]["kind"], "property_invalid");
    assert_eq!(error, call(&root, "ru-RU", "check", Some(&invalid), false));
    assert_eq!(
        fs::read_to_string(&path).unwrap(),
        original.replace("<Hierarchical>false", "<Hierarchical>true")
    );
}

/// AI clients discover broad value types lazily without being allowed to invent a platform type.
#[test]
fn choice_parameter_schema_types_and_publication_share_the_json_contract() {
    let root = TestDir::new();
    fs::create_dir_all(root.0.join("src/Ext")).unwrap();
    fs::create_dir_all(root.0.join("src/Catalogs")).unwrap();
    fs::write(
        root.0.join("eska.toml"),
        "[project]\ntype='configuration'\n",
    )
    .unwrap();
    fs::write(
        root.0.join("src/Ext/ParentConfigurations.bin"),
        "{6,0,0,0,0,0}",
    )
    .unwrap();
    fs::write(root.0.join("src/Configuration.xml"),"<MetaDataObject xmlns='http://v8.1c.ru/8.3/MDClasses'><Configuration uuid='11111111-1111-1111-1111-111111111111'><Properties><Name>Demo</Name></Properties><ChildObjects><Catalog>Goods</Catalog></ChildObjects></Configuration></MetaDataObject>").unwrap();
    let file = root.0.join("src/Catalogs/Goods.xml");
    let original = "<MetaDataObject xmlns='http://v8.1c.ru/8.3/MDClasses' xmlns:r='http://v8.1c.ru/8.3/xcf/readable' xmlns:a='http://v8.1c.ru/8.2/managed-application/core' xmlns:x='http://www.w3.org/2001/XMLSchema' xmlns:s='http://www.w3.org/2001/XMLSchema-instance'><Catalog uuid='22222222-2222-2222-2222-222222222222'><Properties><Name>Goods</Name><StandardAttributes><r:StandardAttribute name='Parent'><r:ChoiceParameters><a:item name='Filter.Owner'><a:value s:type='x:decimal'>1</a:value></a:item></r:ChoiceParameters></r:StandardAttribute></StandardAttributes></Properties><ChildObjects/></Catalog></MetaDataObject>";
    fs::write(&file, original).unwrap();
    let inspect = |locale| {
        let out = Command::new(env!("CARGO_BIN_EXE_eska"))
            .current_dir(&root.0)
            .args([
                "--lang",
                locale,
                "metadata",
                "inspect",
                "--object",
                "catalog:Goods",
            ])
            .output()
            .unwrap();
        assert!(out.status.success());
        assert!(out.stderr.is_empty());
        serde_json::from_slice::<Value>(&out.stdout).unwrap()
    };
    let state = inspect("ru-RU");
    assert_eq!(state, inspect("en-US"));
    let editing = &state["result"]["editing"];
    let field = editing["fields"]
        .as_array()
        .unwrap()
        .iter()
        .find(|f| f["schema"]["domain"] == "choiceParameter")
        .unwrap();
    let number = field["schema"]["types"]
        .as_array()
        .unwrap()
        .iter()
        .find(|t| t["constraints"]["kind"] == "number")
        .unwrap();
    assert!(number["constraints"].get("digits").is_none());
    let query = json!({"schemaVersion":1,"objectId":"catalog:Goods","path":field["path"]});
    let types = call(&root, "ru-RU", "types", Some(&query), true);
    assert_eq!(types, call(&root, "en-US", "types", Some(&query), true));
    assert_eq!(types["result"]["choices"].as_array().unwrap().len(), 5);
    let key = json!({"namespace":"http://v8.1c.ru/8.1/data/enterprise/current-config","name":"CatalogRef.Goods"});
    let mut query = query;
    query["key"] = key.clone();
    let choices = call(&root, "en-US", "choices", Some(&query), true);
    assert_eq!(choices, call(&root, "ru-RU", "choices", Some(&query), true));
    assert_eq!(
        choices["result"]["choices"][0]["value"],
        "Catalog.Goods.EmptyRef"
    );
    let mut request = json!({"schemaVersion":1,"objectId":"catalog:Goods","path":field["path"],"snapshot":editing["snapshot"],"change":{"kind":"value","key":key,"value":"Catalog.Goods.Invented"}});
    assert_eq!(
        call(&root, "ru-RU", "check", Some(&request), false),
        call(&root, "en-US", "apply", Some(&request), false)
    );
    assert_eq!(fs::read_to_string(&file).unwrap(), original);
    request["change"]["value"] = json!("Catalog.Goods.EmptyRef");
    assert_eq!(
        call(&root, "ru-RU", "check", Some(&request), true),
        call(&root, "en-US", "check", Some(&request), true)
    );
    call(&root, "en-US", "apply", Some(&request), true);
    assert_eq!(
        fs::read_to_string(file).unwrap(),
        original.replace(
            "s:type='x:decimal'>1",
            "s:type=\"r:DesignTimeRef\">Catalog.Goods.EmptyRef"
        )
    );
}
