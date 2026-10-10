//! Bidirectional sessions verify exact controls, compact saved-input patterns and registration lifecycle.

use std::fs;

use serde_json::{Value, json};
use url::Url;

use super::{Client, ending, initialize, notification, request, snapshot, support};

const ID: &str = "eska/project-watch/1";
const OWNER: &str = "CommonModules/Tools/object.yaml";

/// Opt in explicitly to both watcher flags; initialize itself must not emit a registration request.
fn opening(client: &mut Client, capability: Value) -> Value {
    let mut init = initialize();
    init["params"]["capabilities"]["workspace"] = json!({});
    init["params"]["capabilities"]["workspace"]["didChangeWatchedFiles"] = capability;
    client.ask(&init)["result"]["capabilities"]["experimental"]["eska"].clone()
}

/// Metadata requests act as ordered checkpoints without triggering file reads or revision changes.
fn owners(client: &mut Client, id: Value, revision: &str) -> Value {
    client.ask(&request(
        id,
        "eska/metadata/owners",
        json!({"snapshot":snapshot(revision),"offset":0,"limit":10}),
    ))
}

/// Standard client responses have no method, even when their ID equals a client request's ID.
fn reply(result: Value) -> Value {
    let mut response = json!({"jsonrpc":"2.0","id":ID});
    response["result"] = result;
    response
}

/// One post-initialized request contains exact controls and source-relative saved-input patterns.
fn registration(client: &mut Client, capability: &Value) -> Value {
    client.send(&notification("initialized"));
    let request = client.receive();
    assert_eq!(request["method"], "client/registerCapability");
    assert_eq!(request["id"], ID);
    let registered = &request["params"]["registrations"];
    assert_eq!(registered.as_array().unwrap().len(), 2);
    assert_eq!(registered[0]["method"], "workspace/didChangeWatchedFiles");
    let actual = registered[0]["registerOptions"]["watchers"]
        .as_array()
        .unwrap()
        .iter()
        .map(|watcher| {
            assert_eq!(watcher["kind"], 7);
            let pattern = watcher["globPattern"]["pattern"].as_str().unwrap();
            assert!(!pattern.contains(['*', '?', '[', ']', '{', '}', '/', '\\']));
            Url::parse(watcher["globPattern"]["baseUri"].as_str().unwrap())
                .unwrap()
                .join(pattern)
                .unwrap()
                .to_string()
        })
        .collect::<Vec<_>>();
    let expected = capability["projectChanges"]["inputs"]
        .as_array()
        .unwrap()
        .iter()
        .map(|input| input["uri"].as_str().unwrap().to_owned())
        .collect::<Vec<_>>();
    assert_eq!(actual, expected);
    let manifest = capability["projectChanges"]["inputs"]
        .as_array()
        .unwrap()
        .iter()
        .find(|input| input["kind"] == "source_manifest")
        .unwrap();
    let base = Url::parse(manifest["uri"].as_str().unwrap())
        .unwrap()
        .join(".")
        .unwrap();
    let saved = &registered[1];
    assert_eq!(saved["id"], "eska/project-saved-inputs");
    assert_eq!(saved["method"], "workspace/didChangeWatchedFiles");
    let patterns = saved["registerOptions"]["watchers"]
        .as_array()
        .unwrap()
        .iter()
        .map(|watcher| {
            assert_eq!(watcher["kind"], 7);
            assert_eq!(watcher["globPattern"]["baseUri"], base.as_str());
            watcher["globPattern"]["pattern"].as_str().unwrap()
        })
        .collect::<Vec<_>>();
    assert_eq!(patterns, ["**/Module.bsl", "**/object.yaml"]);
    request
}

/// Missing, false and nonboolean flags never make a client eligible or introduce extra output.
#[test]
fn manual_clients_are_not_registered_in_both_locales() {
    let (_directory, root) = support::project("// saved\n", false);
    for capability in [
        json!({}),
        json!({"dynamicRegistration":true}),
        json!({"relativePatternSupport":true}),
        json!({"dynamicRegistration":false,"relativePatternSupport":true}),
        json!({"dynamicRegistration":true,"relativePatternSupport":null}),
        json!({"dynamicRegistration":"true","relativePatternSupport":true}),
    ] {
        let mut previous = None;
        for lang in ["en-US", "ru-RU"] {
            let mut client = Client::with_options(&root, lang, &[]);
            let result = opening(&mut client, capability.clone());
            assert_eq!(result["watchRegistration"]["eligible"], false);
            client.send(&notification("initialized"));
            client.send(
                &json!({"jsonrpc":"2.0","id":null,"error":{"code":-32600,"message":"unrelated"}}),
            );
            let response = owners(&mut client, json!(ID), "0");
            assert!(response.get("result").is_some());
            if let Some(previous) = &previous {
                assert_eq!(&response, previous);
            }
            previous = Some(response);
            client.finish();
        }
    }
}

/// Relative base URIs preserve Unicode and glob characters in directories, including workspace roots.
#[test]
fn exact_control_inventory_is_registered_once_and_acknowledged_in_both_locales() {
    let (_directory, root) = support::project("// saved\n", false);
    let root_with_globs = root.parent().unwrap().join("Проект [x]{y}");
    fs::rename(&root, &root_with_globs).unwrap();
    for workspace in [false, true] {
        if workspace {
            fs::write(
                root_with_globs.parent().unwrap().join("eska.toml"),
                "[workspace]\nmembers=['Проект [x]{y}']\n",
            )
            .unwrap();
        }
        let mut previous = None;
        for lang in ["en-US", "ru-RU"] {
            let mut client = Client::with_options(&root_with_globs, lang, &[]);
            let capability = opening(
                &mut client,
                json!({"dynamicRegistration":true,"relativePatternSupport":true}),
            );
            assert_eq!(capability["watchRegistration"]["eligible"], true);
            assert_eq!(
                capability["watchRegistration"]["scope"],
                "project-control-inputs"
            );
            assert_eq!(capability["watchedFiles"]["dynamicRegistration"], true);
            assert_eq!(capability["watchedFiles"]["maxChanges"], Value::Null);
            assert_eq!(capability["watchedFiles"]["unknownSourceEvents"], "ignore");
            assert_eq!(
                capability["watchRegistration"]["savedInputs"]["registrationId"],
                "eska/project-saved-inputs"
            );
            assert_eq!(
                capability["projectChanges"]["inputs"]
                    .as_array()
                    .unwrap()
                    .len(),
                if workspace { 7 } else { 6 }
            );
            client.send(&reply(Value::Null));
            assert_eq!(
                owners(&mut client, json!("before-ready"), "0")["error"]["code"],
                -32002
            );
            let registered = registration(&mut client, &capability);
            client.send(&notification("initialized"));
            client.send(&json!({"jsonrpc":"2.0","id":7,"result":null}));
            assert!(owners(&mut client, json!(ID), "0").get("result").is_some());
            let ack = client.ask(&reply(Value::Null));
            assert_eq!(ack["method"], "eska/watchRegistration");
            assert_eq!(ack["params"]["status"], "registered");
            assert_eq!(ack["params"]["snapshot"], snapshot("0"));
            assert_eq!(
                ack["params"]["registrationId"],
                registered["params"]["registrations"][0]["id"]
            );
            assert_eq!(
                ack["params"]["registrationIds"],
                json!(["eska/project-control-files", "eska/project-saved-inputs"])
            );
            client.send(&reply(Value::Null));
            let fence = owners(&mut client, json!("after-duplicate"), "0");
            assert!(fence.get("result").is_some());
            let trace = vec![registered, ack, fence];
            if let Some(previous) = &previous {
                assert_eq!(&trace, previous);
            }
            previous = Some(trace);
            client.finish();
        }
    }
}

/// Pending and failed registrations neither block project work nor claim the watcher is installed.
#[test]
fn client_rejection_and_invalid_success_remain_explicit_without_blocking_inputs() {
    let (_directory, root) = support::project("// saved\n", false);
    for failure in [
        json!({"jsonrpc":"2.0","id":ID,"error":{"code":-32601,"message":"client detail","data":{"ignored":true}}}),
        reply(json!(true)),
    ] {
        let mut previous = None;
        for lang in ["en-US", "ru-RU"] {
            let mut client = Client::with_options(&root, lang, &[]);
            let capability = opening(
                &mut client,
                json!({"dynamicRegistration":true,"relativePatternSupport":true}),
            );
            registration(&mut client, &capability);
            let read = client.ask(&request(
                json!("read"),
                "eska/metadata/read",
                json!({"source":OWNER,"snapshot":snapshot("0")}),
            ));
            assert_eq!(read["result"]["snapshot"], snapshot("1"));
            let failed = client.ask(&failure);
            assert_eq!(failed["method"], "eska/watchRegistration");
            assert_eq!(failed["params"]["status"], "failed");
            assert_eq!(failed["params"]["snapshot"], snapshot("1"));
            assert_eq!(
                failed["params"]["error"],
                if failure.get("error").is_some() {
                    json!({"kind":"client_rejected_registration","code":-32601})
                } else {
                    json!({"kind":"registration_result_invalid","code":null})
                }
            );
            client.send(&reply(Value::Null));
            let fence = owners(&mut client, json!("after-failure"), "1");
            assert!(fence.get("result").is_some());
            let trace = vec![failed, fence];
            if let Some(previous) = &previous {
                assert_eq!(&trace, previous);
            }
            previous = Some(trace);
            client.finish();
        }
    }
}

/// A late registration result cannot revive a retired host, and shutdown never waits for a reply.
#[test]
fn unanswered_and_retired_registrations_do_not_hold_shutdown_open() {
    let (_directory, root) = support::project("// saved\n", false);
    for retire in [false, true] {
        for lang in ["en-US", "ru-RU"] {
            let mut client = Client::with_options(&root, lang, &[]);
            let capability = opening(
                &mut client,
                json!({"dynamicRegistration":true,"relativePatternSupport":true}),
            );
            registration(&mut client, &capability);
            if retire {
                let manifest = Url::from_file_path(root.join("src/source.json")).unwrap();
                let restart=client.ask(&json!({"jsonrpc":"2.0","method":"workspace/didChangeWatchedFiles","params":{"changes":[{"uri":manifest.as_str(),"type":2}]}}));
                assert_eq!(restart["method"], "eska/restartRequired");
                client.send(&reply(Value::Null));
                assert_eq!(
                    owners(&mut client, json!("retired"), "0")["error"]["data"]["kind"],
                    "project_restart_required"
                );
            }
            assert_eq!(client.ask(&ending()[0])["result"], Value::Null);
            client.send(&reply(Value::Null));
            client.send(&ending()[1]);
            assert!(client.child.wait().unwrap().success());
            assert!(
                eska::tool_cli::rpc_framing::read_frame(&mut client.output)
                    .unwrap()
                    .is_none()
            );
        }
    }
}
