//! The first framed signature request prepares its direct callee and retains the original argument coordinates.

mod fixture;

use super::*;
use crate::common_module_project;
use fixture::MemberCall;

/// Current parameters replace saved declarations while watched changes preserve editor input and caller coordinates.
fn current_signature(client: &mut Client, call: &MemberCall) -> Value {
    let uri = call.target_uri();
    let opened = client.ask(
        &json!({"jsonrpc":"2.0","method":"textDocument/didOpen","params":{
        "textDocument":{"uri":uri,"version":1,"languageId":"bsl","text":call.current}}}),
    );
    let revision = opened["params"]["snapshot"]["revision"].as_str().unwrap();
    let changed = client.ask(&call.pull(revision));
    call.verify(&changed["result"], revision);
    assert_eq!(
        changed["result"]["signatures"][0]["parameters"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    assert!(
        changed["result"]["signatures"][0]["label"]
            .as_str()
            .unwrap()
            .contains('😀')
    );
    let watched = client.ask(
        &json!({"jsonrpc":"2.0","method":"workspace/didChangeWatchedFiles","params":{
        "changes":[{"uri":uri,"type":2}]}}),
    );
    assert_eq!(watched["params"]["inputs"][0]["invalidated"], false);
    assert_eq!(client.ask(&call.pull(revision)), changed);
    changed
}

/// Broken current declarations never return an earlier signature; close rereads the actual saved body.
fn invalid_then_close(client: &mut Client, call: &MemberCall, first: &Value) -> (Value, Value) {
    let uri = call.target_uri();
    let invalid = client.ask(
        &json!({"jsonrpc":"2.0","method":"textDocument/didChange","params":{
        "textDocument":{"uri":uri,"version":2},"contentChanges":[{"text":"???"}]}}),
    );
    let unavailable =
        client.ask(&call.pull(invalid["params"]["snapshot"]["revision"].as_str().unwrap()));
    assert_eq!(
        unavailable["error"]["data"]["kind"],
        "signature_help_incomplete"
    );
    assert_eq!(
        unavailable["error"]["data"]["details"]["source_sha256"],
        hash(&call.body)
    );
    let closed = client.ask(
        &json!({"jsonrpc":"2.0","method":"textDocument/didClose","params":{
        "textDocument":{"uri":uri}}}),
    );
    let restored =
        client.ask(&call.pull(closed["params"]["snapshot"]["revision"].as_str().unwrap()));
    assert_eq!(
        restored["result"]["signatures"],
        first["result"]["signatures"]
    );
    assert_eq!(
        restored["result"]["eska"]["resources"],
        first["result"]["eska"]["resources"]
    );
    (unavailable, restored)
}

/// Exercise the same bounded preparation against independently declared common and manager modules.
fn cold_member_signature(manager: bool) {
    let call = MemberCall::new(manager);
    let mut previous = None;
    for lang in ["en-US", "ru-RU"] {
        let mut client = Client::with_options(&call.root, lang, &[]);
        let initialized = client.ask(&initialize_signature("plaintext", true));
        assert_eq!(
            initialized["result"]["capabilities"]["experimental"]["eska"]["signatureHelp"]["inputPreparation"],
            "direct-member-targets"
        );
        client.send(&notification("initialized"));
        let first = client.ask(&call.pull("0"));
        call.verify_first(&first);
        assert_eq!(client.ask(&call.pull(call.first_revision)), first);
        let changed = current_signature(&mut client, &call);
        let (unavailable, restored) = invalid_then_close(&mut client, &call, &first);
        let trace = (initialized, first, changed, unavailable, restored);
        if let Some(previous) = &previous {
            assert_eq!(&trace, previous);
        }
        previous = Some(trace);
        client.finish();
    }
    call.verify_saved();
}

/// Cold common-module calls load their declared body without a preceding completion or metadata request.
#[test]
fn first_common_module_signature_prepares_current_dependency_in_both_locales() {
    cold_member_signature(false);
}

/// Manager calls share canonical metadata and predefined resources with the same bounded preparation path.
#[test]
fn first_manager_signature_prepares_current_dependency_in_both_locales() {
    cold_member_signature(true);
}

/// Argument expressions preserve the no-metadata-read contract instead of discovering their incidental receivers.
#[test]
fn local_signature_never_prepares_the_common_module_mentioned_in_its_argument() {
    let prefix = "Procedure Local(Value)\nEndProcedure\nProcedure Run()\n Local(Utility.";
    let body = format!("{prefix});\nEndProcedure\n");
    let (directory, root) = support::project(&body, false);
    common_module_project::add_common_modules(&root, &directory.0.join("native"));
    for lang in ["en-US", "ru-RU"] {
        let mut client = Client::with_options(&root, lang, &[]);
        client.ask(&initialize_signature("plaintext", true));
        client.send(&notification("initialized"));
        let first = client.ask(&pull(
            &root,
            MODULE,
            "src-with-common-modules",
            "0",
            position(&body, prefix.len()),
        ));
        verify(&first["result"], &body, MODULE, "1", prefix.len(), true);
        assert!(
            first["result"]["signatures"][0]["label"]
                .as_str()
                .unwrap()
                .contains("Local(")
        );
        assert_eq!(
            first["result"]["eska"]["resources"]["metadata_source_bytes"],
            0
        );
        assert_eq!(
            first["result"]["eska"]["resources"]["module_text_bytes"],
            body.len()
        );
        client.finish();
    }
}
