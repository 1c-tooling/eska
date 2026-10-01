//! API 1.9 adds reviewed structural writes while preserving the read-only default and older clients.

use super::*;
use std::fs;

/// A compact source is writable and contains an uncertain literal which must stay untouched.
fn rename_source() -> (TestDir, String) {
    let directory = TestDir::new();
    fs::create_dir_all(directory.0.join("src/Ext")).unwrap();
    fs::write(
        directory.0.join("eska.toml"),
        "[project]\ntype='configuration'\n",
    )
    .unwrap();
    fs::write(
        directory.0.join("src/Ext/ParentConfigurations.bin"),
        "{6,0,0,0,0,0}",
    )
    .unwrap();
    let input = "\u{feff}<MetaDataObject xmlns='http://v8.1c.ru/8.3/MDClasses'><Configuration uuid='11111111-1111-1111-1111-111111111111'><Properties><Name>Demo</Name><Comment>Demo</Comment></Properties><ChildObjects/></Configuration></MetaDataObject>\r\n".to_owned();
    fs::write(directory.0.join("src/Configuration.xml"), &input).unwrap();
    (directory, input)
}

/// Notifications migrate the old identity before the caller receives fresh properties and undo state.
#[test]
fn ide_rename_preview_apply_and_history_are_locale_independent() {
    for locale in ["ru-RU", "en-US"] {
        let (directory, input) = rename_source();
        let mut client = Client::new(locale);
        let initialized = client.ok("initialize", json!({"apiVersion":{"major":1,"minor":9},"client":{"name":"rename-test","version":"1"},"allowPropertyEdits":true}));
        assert_eq!(initialized["capabilities"]["metadataRename"], true);
        let mut open = client.open(&directory.0);
        let schema = client.ok(
            "metadata/propertyEditing",
            context(&open, json!({"objectId":"configuration:Demo"})),
        );
        assert_eq!(schema["renameAvailable"], true);
        assert_eq!(schema["undoRename"], false);
        let preview = client.ok(
            "metadata/renamePreview",
            context(
                &open,
                json!({"objectId":"configuration:Demo","newName":"Новая"}),
            ),
        );
        assert_eq!(preview["applyAvailable"], true);
        let mut apply = context(
            &open,
            json!({"objectId":"configuration:Demo","newName":"Новая","snapshot":preview["plan"]["snapshot"],"reviewedUncertain":false}),
        );
        let rejected = client.request("metadata/renameApply", apply.clone());
        assert_eq!(rejected["error"]["data"]["kind"], "rename_review_required");
        assert_eq!(
            fs::read_to_string(directory.0.join("src/Configuration.xml")).unwrap(),
            input
        );
        apply["reviewedUncertain"] = json!(true);
        let result = client.ok("metadata/renameApply", apply);
        assert_eq!(
            result["renamed"],
            json!({"from":"configuration:Demo","to":"configuration:Новая","descendantFrom":"configuration:Demo/","descendantTo":"configuration:Новая/"})
        );
        assert_eq!(result["editing"]["undoRename"], true);
        assert!(
            client
                .events
                .iter()
                .any(|event| event["method"] == "metadata/changed"
                    && event["params"]["renamed"] == result["renamed"])
        );
        assert_eq!(
            fs::read_to_string(directory.0.join("src/Configuration.xml")).unwrap(),
            input.replace("<Name>Demo</Name>", "<Name>Новая</Name>")
        );
        open["projects"][0]["generation"] = result["generation"].clone();
        let result = client.ok("metadata/undoProperty", context(&open,json!({"objectId":"configuration:Новая","snapshot":result["editing"]["snapshot"],"direction":"undo"})));
        assert_eq!(
            result["renamed"],
            json!({"from":"configuration:Новая","to":"configuration:Demo","descendantFrom":"configuration:Новая/","descendantTo":"configuration:Demo/"})
        );
        assert_eq!(result["editing"]["redoRename"], true);
        assert_eq!(
            fs::read_to_string(directory.0.join("src/Configuration.xml")).unwrap(),
            input
        );
        open["projects"][0]["generation"] = result["generation"].clone();
        let result = client.ok("metadata/undoProperty", context(&open,json!({"objectId":"configuration:Demo","snapshot":result["editing"]["snapshot"],"direction":"redo"})));
        assert_eq!(result["renamed"]["to"], "configuration:Новая");
        client.finish();
    }
}

/// The new capability never upgrades the default read-only connection or an older editing client.
#[test]
fn ide_rename_requires_minor_nine_and_write_opt_in() {
    let (directory, input) = rename_source();
    for (minor, allow) in [(8, true), (9, false)] {
        let mut client = Client::new("en-US");
        let initialized = client.ok("initialize", json!({"apiVersion":{"major":1,"minor":minor},"client":{"name":"rename-test","version":"1"},"allowPropertyEdits":allow}));
        assert_eq!(initialized["capabilities"]["metadataRename"], false);
        let open = client.open(&directory.0);
        let schema = client.ok(
            "metadata/propertyEditing",
            context(&open, json!({"objectId":"configuration:Demo"})),
        );
        if minor < 9 {
            assert!(schema.get("renameAvailable").is_none());
        }
        let result = client.request(if allow {"metadata/renamePreview"} else {"metadata/renameApply"}, context(&open,json!({"objectId":"configuration:Demo","newName":"New","snapshot":"unreviewed","reviewedUncertain":true})));
        if allow {
            assert_eq!(result["error"]["code"], -32601);
        } else {
            assert_eq!(result["error"]["data"]["kind"], "property_read_only");
        }
        assert_eq!(
            fs::read_to_string(directory.0.join("src/Configuration.xml")).unwrap(),
            input
        );
        client.finish();
    }
}

/// Cancelling an active journal operation must not hide a committed write behind a cancelled response.
#[test]
fn rename_cancellation_after_staging_preserves_the_committed_result() {
    let (directory, input) = rename_source();
    fs::create_dir_all(directory.0.join("src/payload")).unwrap();
    // Hashing a real payload after staging leaves a deterministic observation window for the pipe reader.
    fs::write(
        directory.0.join("src/payload/data.bin"),
        vec![71; 32 * 1024 * 1024],
    )
    .unwrap();
    let mut client = Client::new("en-US");
    client.ok("initialize",json!({"apiVersion":{"major":1,"minor":9},"client":{"name":"cancel-rename","version":"1"},"allowPropertyEdits":true}));
    let open = client.open(&directory.0);
    let preview = client.ok(
        "metadata/renamePreview",
        context(
            &open,
            json!({"objectId":"configuration:Demo","newName":"New"}),
        ),
    );
    client.serial += 1;
    let id = client.serial;
    client.send(&json!({"jsonrpc":"2.0","id":id,"method":"metadata/renameApply","params":context(&open,json!({"objectId":"configuration:Demo","newName":"New","snapshot":preview["plan"]["snapshot"],"reviewedUncertain":true}))}));
    let ready = directory.0.join(".eska/metadata/rename/ready");
    let deadline = Instant::now() + Duration::from_secs(10);
    while !ready.exists() {
        assert!(Instant::now() < deadline, "rename staging was not observed");
        thread::sleep(Duration::from_millis(1));
    }
    client.send(&json!({"jsonrpc":"2.0","method":"$/cancelRequest","params":{"id":id}}));
    let result = client.receive();
    assert_eq!(result["id"], id);
    assert!(result.get("error").is_none(), "{result}");
    assert_eq!(result["result"]["renamed"]["to"], "configuration:New");
    assert_eq!(
        fs::read_to_string(directory.0.join("src/Configuration.xml")).unwrap(),
        input.replace("<Name>Demo</Name>", "<Name>New</Name>")
    );
    client.finish();
}
