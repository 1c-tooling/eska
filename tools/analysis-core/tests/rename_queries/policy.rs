//! Working metadata cannot grant module write permission from a frozen or replaced accepted base.

use super::*;

/// Supplier/freeze authority remains pinned independently of current editor YAML identity.
#[test]
fn frozen_module_policy_denies_prepare_and_rename_even_after_yaml_identity_edit() {
    let (_directory, root, reference) = project(BODY, true);
    let saved = fs::read(root.join("src").join(MODULE)).unwrap();
    for locale in ["en-US", "ru-RU"] {
        let mut client = Client::with_options(
            &root,
            locale,
            &["--accepted-base-sha256", &reference.sha256],
        );
        client.ask(&initialize_rename(true));
        client.send(&notification("initialized"));
        open(&mut client, &root, BODY, 1);
        let yaml_path = root.join("src/CommonModules/Tools/object.yaml");
        let yaml = fs::read_to_string(&yaml_path).unwrap().replace(
            "20000000-0000-4000-8000-000000000001",
            "30000000-0000-4000-8000-000000000001",
        );
        client.ask(&json!({"jsonrpc":"2.0","method":"textDocument/didOpen","params":{
            "textDocument":{"uri":url::Url::from_file_path(&yaml_path).unwrap().as_str(),"version":1,"languageId":"yaml","text":yaml}}}));
        for replacement in [None, Some("Свежий")] {
            let refused = client.ask(&query(
                &root,
                BODY,
                BODY.find("Аргумент").unwrap(),
                "2",
                replacement,
            ));
            assert_eq!(refused["error"]["data"]["kind"], "rename_permission_denied");
            assert_eq!(
                refused["error"]["data"]["details"]["permission"]["reason"],
                "eska_frozen"
            );
            assert_eq!(
                refused["error"]["data"]["details"]["permission"]["baseline"],
                reference.sha256
            );
        }
        client.finish();
    }
    assert_eq!(fs::read(root.join("src").join(MODULE)).unwrap(), saved);
}

/// Advertised plans still verify accepted storage on each request rather than trusting initialize.
#[test]
fn local_rename_refuses_replaced_accepted_storage_without_changing_the_open_input() {
    let (_directory, root, reference) = project(BODY, false);
    let manifest = root.join(".eska/accepted/accepted.json");
    let saved = fs::read(&manifest).unwrap();
    for locale in ["en-US", "ru-RU"] {
        let mut client = Client::with_options(
            &root,
            locale,
            &["--accepted-base-sha256", &reference.sha256],
        );
        client.ask(&initialize_rename(true));
        client.send(&notification("initialized"));
        let opened = open(&mut client, &root, BODY, 1);
        fs::write(&manifest, b"{}").unwrap();
        for replacement in [None, Some("Свежий")] {
            let refused = client.ask(&query(
                &root,
                BODY,
                BODY.find("Аргумент").unwrap(),
                "1",
                replacement,
            ));
            assert!(
                refused["error"]["data"]["kind"]
                    .as_str()
                    .unwrap()
                    .starts_with("source_baseline_"),
                "{refused}"
            );
        }
        fs::write(&manifest, &saved).unwrap();
        let plan = client.ask(&query(
            &root,
            BODY,
            BODY.find("Аргумент").unwrap(),
            "1",
            Some("Свежий"),
        ));
        assert!(plan.get("result").is_some(), "{plan}");
        assert_eq!(
            plan["result"]["eska"]["resources"],
            opened["params"]["resources"]
        );
        client.finish();
    }
    assert_eq!(fs::read(&manifest).unwrap(), saved);
}
