//! A cold hover loads its own direct callee before the native positional card is queried.

mod fixture;

use super::*;
use fixture::MemberHover;

/// Documentation and type follow the target overlay, including a saved event that cannot replace it.
fn current_card(client: &mut Client, call: &MemberHover, string: &str) -> Value {
    let uri = call.target_uri();
    let opened = client.ask(
        &json!({"jsonrpc":"2.0","method":"textDocument/didOpen","params":{
        "textDocument":{"uri":uri,"version":1,"languageId":"bsl","text":call.current}}}),
    );
    let revision = opened["params"]["snapshot"]["revision"].as_str().unwrap();
    let changed = client.ask(&call.pull(revision, false));
    call.verify(&changed["result"], revision);
    let contents = changed["result"]["contents"]["value"].as_str().unwrap();
    assert!(contents.contains(string) && contents.contains("Current purpose Я😀"));
    let watched = client.ask(
        &json!({"jsonrpc":"2.0","method":"workspace/didChangeWatchedFiles","params":{
        "changes":[{"uri":uri,"type":2}]}}),
    );
    assert_eq!(watched["params"]["inputs"][0]["invalidated"], false);
    assert_eq!(client.ask(&call.pull(revision, false)), changed);
    changed
}

/// Broken current input removes an old card; closing it rereads the actual saved callee.
fn invalid_then_close(client: &mut Client, call: &MemberHover, first: &Value) -> (Value, Value) {
    let uri = call.target_uri();
    let invalid = client.ask(
        &json!({"jsonrpc":"2.0","method":"textDocument/didChange","params":{
        "textDocument":{"uri":uri,"version":2},"contentChanges":[{"text":"???"}]}}),
    );
    let unavailable = client.ask(&call.pull(
        invalid["params"]["snapshot"]["revision"].as_str().unwrap(),
        false,
    ));
    assert_eq!(unavailable["error"]["data"]["kind"], "hover_incomplete");
    assert_eq!(
        unavailable["error"]["data"]["details"]["source_sha256"],
        hash(&call.body)
    );
    let closed = client.ask(
        &json!({"jsonrpc":"2.0","method":"textDocument/didClose","params":{
        "textDocument":{"uri":uri}}}),
    );
    let restored = client.ask(&call.pull(
        closed["params"]["snapshot"]["revision"].as_str().unwrap(),
        false,
    ));
    assert_eq!(restored["result"]["contents"], first["result"]["contents"]);
    assert_eq!(
        restored["result"]["eska"]["resources"],
        first["result"]["eska"]["resources"]
    );
    (unavailable, restored)
}

/// The wire boundary preserves localization, independent target authority and exact caller coordinates.
fn cold_member_hover(manager: bool) {
    let call = MemberHover::new(manager);
    let mut previous = None;
    for (lang, string) in [("en-US", "String"), ("ru-RU", "Строка")] {
        let mut client = Client::with_options(&call.root, lang, &[]);
        let initialized = client.ask(&initialize_hover(json!(["plaintext"])));
        assert_eq!(
            initialized["result"]["capabilities"]["experimental"]["eska"]["hover"]["inputPreparation"],
            "direct-member-callees"
        );
        client.send(&notification("initialized"));
        let first = client.ask(&call.pull("0", false));
        call.verify_first(&first);
        assert_eq!(client.ask(&call.pull(call.first_revision, false)), first);
        let edge = client.ask(&call.pull(call.first_revision, true));
        call.verify(&edge["result"], call.first_revision);
        assert_eq!(edge["result"]["contents"], first["result"]["contents"]);
        assert_eq!(
            edge["result"]["eska"]["resources"],
            first["result"]["eska"]["resources"]
        );
        let changed = current_card(&mut client, &call, string);
        let (unavailable, restored) = invalid_then_close(&mut client, &call, &first);
        let trace = [initialized, first, edge, changed, unavailable, restored].map(machine);
        if let Some(previous) = &previous {
            assert_eq!(&trace, previous);
        }
        previous = Some(trace);
        client.finish();
    }
    call.verify_saved();
}

/// The first hover of a common-module callee needs no earlier metadata or completion request.
#[test]
fn first_common_module_hover_prepares_current_dependency_in_both_locales() {
    cold_member_hover(false);
}

/// A manager callee shares its canonical metadata and predefined resource with existing preparation.
#[test]
fn first_manager_hover_prepares_current_dependency_in_both_locales() {
    cold_member_hover(true);
}
