//! Recursive patterns must tolerate unrelated matches without discarding accepted declared-file changes.

use std::{fs, path::Path};

use serde_json::{Value, json};
use url::Url;

use super::{Client, initialize, notification, request, snapshot, support};

const OWNER: &str = "CommonModules/Tools/object.yaml";
const MODULE: &str = "CommonModules/Tools/Ext/Module.bsl";

/// A subscribed client acknowledges both registrations together before using the ordinary metadata methods.
fn start(root: &Path, lang: &str) -> (Client, Value) {
    let mut client = Client::with_options(root, lang, &[]);
    let mut init = initialize();
    init["params"]["capabilities"]["workspace"] =
        json!({"didChangeWatchedFiles":{"dynamicRegistration":true,"relativePatternSupport":true}});
    let capability = client.ask(&init)["result"]["capabilities"]["experimental"]["eska"].clone();
    assert_eq!(capability["watchedFiles"]["maxChanges"], Value::Null);
    assert_eq!(
        capability["watchedFiles"]["maxRequestBytes"],
        4 * 1024 * 1024
    );
    client.send(&notification("initialized"));
    let registration = client.receive();
    assert_eq!(registration["method"], "client/registerCapability");
    assert_eq!(
        registration["params"]["registrations"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    let ack = client.ask(&json!({"jsonrpc":"2.0","id":"eska/project-watch/1","result":null}));
    assert_eq!(ack["params"]["status"], "registered");
    (client, registration)
}

/// Only selected-source local file URIs participate; no path is probed on disk to classify an event.
fn uri(root: &Path, path: &str) -> String {
    Url::from_file_path(root.join("src").join(path))
        .unwrap()
        .to_string()
}

/// The standard notification receives one ordered input acknowledgement even if all matches are ignored.
fn watched(changes: &Value) -> Value {
    json!({"jsonrpc":"2.0","method":"workspace/didChangeWatchedFiles","params":{"changes":changes}})
}

/// Both UTF-8 and unread binary modules get literal basename patterns; preserved resources are excluded.
#[test]
fn saved_patterns_cover_declared_binary_modules_without_watching_resources() {
    for binary in [false, true] {
        let (_directory, root) = support::project("// saved\n", binary);
        let resource = root.join("src/Unrelated/Template.bin");
        fs::create_dir_all(resource.parent().unwrap()).unwrap();
        fs::write(&resource, [0xff]).unwrap();
        let mut previous = None;
        for lang in ["en-US", "ru-RU"] {
            let (mut client, registration) = start(&root, lang);
            let names = registration["params"]["registrations"][1]["registerOptions"]["watchers"]
                .as_array()
                .unwrap()
                .iter()
                .map(|watcher| watcher["globPattern"]["pattern"].as_str().unwrap())
                .collect::<Vec<_>>();
            assert_eq!(
                names,
                if binary {
                    vec!["**/Module.bin", "**/object.yaml"]
                } else {
                    vec!["**/Module.bsl", "**/object.yaml"]
                }
            );
            if let Some(previous) = &previous {
                assert_eq!(&registration, previous);
            }
            previous = Some(registration);
            client.finish();
        }
    }
}

/// A binary match remains explicitly unreadable and cannot reject another declared saved input's change.
#[test]
fn subscribed_binary_events_acknowledge_unreadable_input_without_poisoning_a_mixed_batch() {
    let (_directory, root) = support::project("// saved\n", true);
    let binary = "CommonModules/Tools/Ext/Module.bin";
    let file = root.join("src").join(binary);
    let saved = fs::read(&file).unwrap();
    let mut previous = None;
    for lang in ["en-US", "ru-RU"] {
        let (mut client, _) = start(&root, lang);
        let owner = client.ask(&request(
            json!("read"),
            "eska/metadata/read",
            json!({"snapshot":snapshot("0"),"source":OWNER}),
        ));
        assert_eq!(owner["result"]["snapshot"], snapshot("1"));
        fs::remove_file(&file).unwrap();
        let event = client.ask(&watched(
            &json!([{"uri":uri(&root,OWNER),"type":2},{"uri":uri(&root,binary),"type":3}]),
        ));
        assert_eq!(event["method"], "eska/inputsChanged");
        assert_eq!(event["params"]["snapshot"], snapshot("2"));
        assert_eq!(event["params"]["ignoredSources"], 0);
        assert_eq!(event["params"]["resources"]["total_source_bytes"], 0);
        let inputs = event["params"]["inputs"].as_array().unwrap();
        assert_eq!(inputs.len(), 2);
        let unchanged = inputs
            .iter()
            .find(|input| input["source"] == binary)
            .unwrap();
        assert_eq!(unchanged["invalidated"], false);
        assert_eq!(unchanged["input"]["state"]["input"]["kind"], "binary");
        let changed = inputs
            .iter()
            .find(|input| input["source"] == OWNER)
            .unwrap();
        assert_eq!(changed["invalidated"], true);
        if let Some(previous) = &previous {
            assert_eq!(&event, previous);
        }
        previous = Some(event);
        client.finish();
        fs::write(&file, &saved).unwrap();
    }
}

/// A batch larger than the manual cap remains atomic and coalesced; unknown matches cannot poison known inputs.
#[test]
fn subscribed_batches_ignore_unknown_matches_and_invalidate_all_known_inputs_in_both_locales() {
    let (_directory, root) = support::project("// saved\n", false);
    let owner = root.join("src").join(OWNER);
    let saved = fs::read(&owner).unwrap();
    let mut previous = None;
    for lang in ["en-US", "ru-RU"] {
        let (mut client, _) = start(&root, lang);
        client.ask(&request(
            json!("read"),
            "eska/metadata/read",
            json!({"snapshot":snapshot("0"),"source":OWNER}),
        ));
        let ready = client.ask(&request(
            json!("syntax"),
            "textDocument/diagnostic",
            json!({"textDocument":{"uri":uri(&root,MODULE)}}),
        ));
        assert_eq!(ready["result"]["eska"]["snapshot"], snapshot("2"));
        let mut trace = Vec::new();
        for invalid in [
            json!({"uri":"https://example.invalid/Module.bsl","type":2}),
            json!({"uri":Url::from_file_path(root.parent().unwrap().join("Other/Module.bsl")).unwrap().as_str(),"type":2}),
            json!({"uri":uri(&root,"Untracked/Ext/Module.bsl"),"type":4}),
        ] {
            let event = client.ask(&watched(
                &json!([{"uri":uri(&root,OWNER),"type":2},invalid]),
            ));
            assert_eq!(event["method"], "eska/inputRejected");
            assert_eq!(event["params"]["snapshot"], snapshot("2"));
            trace.push(event);
        }
        let mut events = vec![json!({"uri":uri(&root,OWNER),"type":2})];
        events.extend((0..210).map(|_| json!({"uri":uri(&root,MODULE),"type":2})));
        events.extend((0..250).map(
            |index| json!({"uri":uri(&root,&format!("Untracked-{index}/Ext/Module.bsl")),"type":1}),
        ));
        fs::remove_file(&owner).unwrap();
        let event = client.ask(&watched(&Value::Array(events)));
        assert_eq!(event["method"], "eska/inputsChanged");
        assert_eq!(event["params"]["snapshot"], snapshot("4"));
        assert_eq!(event["params"]["ignoredSources"], 250);
        assert_eq!(event["params"]["inputs"].as_array().unwrap().len(), 2);
        assert_eq!(event["params"]["resources"]["total_source_bytes"], 0);
        assert!(
            event["params"]["inputs"]
                .as_array()
                .unwrap()
                .iter()
                .all(|input| input["invalidated"] == true
                    && input["input"]["state"]["source_sha256"].is_null())
        );
        trace.push(event);
        let ignored = client.ask(&watched(
            &json!([{"uri":uri(&root,"Another/Ext/Module.bsl"),"type":1}]),
        ));
        assert_eq!(ignored["params"]["snapshot"], snapshot("4"));
        assert_eq!(ignored["params"]["inputs"], json!([]));
        assert_eq!(ignored["params"]["ignoredSources"], 1);
        trace.push(ignored);
        let stale = client.ask(&request(
            json!("stale"),
            "eska/metadata/read",
            json!({"snapshot":snapshot("2"),"source":OWNER}),
        ));
        assert_eq!(stale["error"]["code"], -32801);
        trace.push(stale);
        let missing = client.ask(&request(
            json!("missing"),
            "eska/metadata/read",
            json!({"snapshot":snapshot("4"),"source":OWNER}),
        ));
        assert_eq!(
            missing["result"]["metadata"]["state"]["input"]["failure"]["code"],
            "source_file_missing"
        );
        trace.push(missing);
        fs::write(&owner, &saved).unwrap();
        if let Some(previous) = &previous {
            assert_eq!(&trace, previous);
        }
        previous = Some(trace);
        client.finish();
    }
    assert_eq!(fs::read(owner).unwrap(), saved);
}
