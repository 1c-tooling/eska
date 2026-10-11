//! The first language request resolves current direct dependencies without a prior metadata read.

use super::*;

const OWN: &str = "CommonModules/Tools/object.yaml";
const TARGET: &str = "Catalogs/Items/object.yaml";
const RESOURCE: &str = "Catalogs/Items/Ext/Predefined.xml";
const MODULE: &str = "CommonModules/Tools/Ext/Module.bsl";
const PREFIX: &str = "\u{feff}// Я😀\r\nProcedure Run()\r\n Value = Catalogs.Items.";

/// Enumerating current owner states never prepares another source dependency.
fn owners(client: &mut Client, revision: &str) -> Value {
    client.ask(&request(
        json!("owners"),
        "eska/metadata/owners",
        json!({"snapshot":snapshot(revision),"limit":200}),
    ))
}

/// Readiness and source reservations expose exactly which inputs the first completion prepared.
#[test]
fn cold_completion_prepares_direct_current_metadata_without_a_tree_read_in_both_locales() {
    let body = format!("{PREFIX}Saved;\r\nEndProcedure\r\n");
    let (directory, root) = support::project(&body, false);
    support::predefined_project::add_predefined(&root, &directory.0.join("native"));
    let source = root.join("src-with-predefined");
    let expected_bytes = [MODULE, OWN, TARGET, RESOURCE]
        .into_iter()
        .map(|path| fs::metadata(source.join(path)).unwrap().len())
        .sum::<u64>();
    let mut previous = None;
    for lang in ["en-US", "ru-RU"] {
        let mut client = Client::with_options(&root, lang, &[]);
        let init = client.ask(&initialize());
        assert_eq!(
            init["result"]["capabilities"]["experimental"]["eska"]["targetedCompletionMetadata"],
            true
        );
        client.send(&notification("initialized"));
        let cold = owners(&mut client, "0");
        assert_eq!(cold["result"]["resources"]["total_source_bytes"], 0);
        assert!(
            cold["result"]["owners"]
                .as_array()
                .unwrap()
                .iter()
                .all(|owner| owner["state"]["input"]["kind"] == "pending")
        );
        let ready = client.ask(&pull(&source, MODULE, "0", PREFIX));
        assert!(labels(&ready).contains("Saved"), "{ready}");
        assert_eq!(ready["result"]["eska"]["snapshot"], snapshot("4"));
        assert_eq!(ready["result"]["eska"]["position_byte"], PREFIX.len());
        assert_eq!(
            ready["result"]["eska"]["resources"]["total_source_bytes"],
            expected_bytes
        );
        assert_eq!(ready["result"]["isIncomplete"], true);
        let current = owners(&mut client, "4");
        for owner in current["result"]["owners"].as_array().unwrap() {
            let selected = [OWN, TARGET].contains(&owner["source"].as_str().unwrap());
            assert_eq!(
                owner["state"]["input"]["kind"] == "ready",
                selected,
                "{owner}"
            );
            if owner["source"] == TARGET {
                assert_eq!(owner["state"]["model"], "ready");
                assert_eq!(owner["state"]["forms"][0]["model"], "pending");
            }
        }
        let repeated = client.ask(&pull(&source, MODULE, "4", PREFIX));
        assert_eq!(ready, repeated);
        let stale = client.ask(&pull(&source, MODULE, "0", PREFIX));
        assert_eq!(stale["error"]["data"]["kind"], "stale_revision");
        assert_eq!(owners(&mut client, "4"), current);
        if let Some(previous) = &previous {
            assert_eq!(&current, previous);
        }
        previous = Some(current);
        client.finish();
    }
    assert_eq!(fs::read_to_string(source.join(MODULE)).unwrap(), body);
}

/// Saved failures and invalid editor input stay authoritative until their explicit current-input events.
#[test]
fn cold_completion_does_not_refresh_missing_saved_or_invalid_editor_metadata_implicitly() {
    let body = format!("{PREFIX}Saved;\r\nEndProcedure\r\n");
    let (directory, root) = support::project(&body, false);
    support::predefined_project::add_predefined(&root, &directory.0.join("native"));
    let source = root.join("src-with-predefined");
    let file = source.join(RESOURCE);
    let resource_uri = url::Url::from_file_path(&file).unwrap().to_string();
    let owner_uri = url::Url::from_file_path(source.join(TARGET))
        .unwrap()
        .to_string();
    let saved = fs::read(&file).unwrap();
    for lang in ["en-US", "ru-RU"] {
        fs::remove_file(&file).unwrap();
        let mut client = Client::with_options(&root, lang, &[]);
        client.ask(&initialize());
        client.send(&notification("initialized"));
        let missing = client.ask(&pull(&source, MODULE, "0", PREFIX));
        assert!(!labels(&missing).contains("Saved"));
        assert_eq!(missing["result"]["eska"]["snapshot"], snapshot("4"));
        let state = owners(&mut client, "4");
        let target = state["result"]["owners"]
            .as_array()
            .unwrap()
            .iter()
            .find(|owner| owner["source"] == TARGET)
            .unwrap();
        assert_eq!(
            target["state"]["model_issue"]["failure"]["code"],
            "source_file_missing"
        );
        fs::write(&file, &saved).unwrap();
        assert_eq!(client.ask(&pull(&source, MODULE, "4", PREFIX)), missing);
        let watched = client.ask(&json!({"jsonrpc":"2.0","method":"workspace/didChangeWatchedFiles","params":{"changes":[{"uri":resource_uri,"type":1}]}}));
        assert!(
            watched["params"]["inputs"][0]["invalidated"]
                .as_bool()
                .unwrap()
        );
        let ready = client.ask(&pull(
            &source,
            MODULE,
            watched["params"]["snapshot"]["revision"].as_str().unwrap(),
            PREFIX,
        ));
        assert!(labels(&ready).contains("Saved"));
        let opened = client.ask(&json!({"jsonrpc":"2.0","method":"textDocument/didOpen","params":{"textDocument":{"uri":owner_uri,"languageId":"yaml","version":1,"text":"broken: ["}}}));
        let invalid = client.ask(&pull(
            &source,
            MODULE,
            opened["params"]["snapshot"]["revision"].as_str().unwrap(),
            PREFIX,
        ));
        assert!(!labels(&invalid).contains("Saved"));
        assert_eq!(
            invalid["result"]["eska"]["snapshot"],
            opened["params"]["snapshot"]
        );
        let closed = client.ask(&json!({"jsonrpc":"2.0","method":"textDocument/didClose","params":{"textDocument":{"uri":owner_uri}}}));
        assert!(
            labels(&client.ask(&pull(
                &source,
                MODULE,
                closed["params"]["snapshot"]["revision"].as_str().unwrap(),
                PREFIX
            )))
            .contains("Saved")
        );
        client.finish();
    }
    assert_eq!(fs::read(file).unwrap(), saved);
}
