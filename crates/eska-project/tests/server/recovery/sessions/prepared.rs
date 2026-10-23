use super::*;

/// Recovery intent chunks remain available before any metadata session, on the exact acknowledged revision.
fn prepared_part(client: &mut Client, binding: &Value, state: &Value, part: &str) -> String {
    let header = &state["record"];
    let mut text = String::new();
    loop {
        let reply = client.request(
            "query/recoveryPreparedRead",
            &args(
                binding,
                json!({"id":ID,
            "revision":header["revision"],"part":part,"offset":text.len()}),
            ),
        );
        let next = reply["text"].as_str().unwrap();
        assert!(next.len() <= 131_072);
        text.push_str(next);
        assert_eq!(reply["nextOffset"], text.len());
        if reply["complete"] == true {
            break;
        }
        assert_ne!(next, "");
    }
    assert_eq!(header["prepared"]["parts"][part]["bytes"], text.len());
    assert_eq!(
        header["prepared"]["parts"][part]["snapshot"],
        snapshot(&text)
    );
    text
}

/// An older adapter must preserve an unfinished intent instead of treating it as an ordinary clean record.
fn check_old_adapter(root: &Path, private: &Path, revision: &Value) {
    let before = fs::read(private.join(format!("{ID}.json"))).unwrap();
    let mut old = Client::new(root, "en");
    initialize(&mut old, "en-US");
    let binding = old.request(
        "query/recoveryOpen",
        &json!({"directory":{"value":private,"encoding":"utf-8"}}),
    );
    assert_eq!(binding["records"], json!([]));
    assert_eq!(binding["damaged"], json!([ID]));
    let input = args(&binding, json!({"id":ID,"revision":revision}));
    assert_eq!(
        old.response("query/recoveryPreparedRead", &input)["error"]["code"],
        -32601
    );
    assert_eq!(
        old.response("query/recoveryRemove", &input)["error"]["data"]["details"]["reason"],
        "pending_apply"
    );
    assert_eq!(
        fs::read(private.join(format!("{ID}.json"))).unwrap(),
        before
    );
    old.finish();
}

/// A fresh handle cannot bypass a known owner's still-unreconciled durable intent.
fn check_pending_owner_open(client: &mut Client, binding: &Value, input: &str) {
    let document = client.request(
        "query/open",
        &source_args(
            binding,
            json!({"node":binding["node"],
        "source":{"value":SOURCE,"encoding":"utf-8"},"version":"1","text":input}),
        ),
    );
    let blocked=client.response("query/sessionOpen",&source_args(binding,json!({"documentId":document["documentId"],
        "id":"10000000-0000-4000-8000-000000000002","query":{"kind":"dynamicList","name":"List","id":"1"},
        "format":"editor","userReadOnly":false})));
    assert_eq!(
        blocked["error"]["data"]["details"],
        json!({"reason":"pending_apply","id":ID})
    );
    client.request(
        "query/close",
        &source_args(binding, json!({"documentId":document["documentId"]})),
    );
    let other_id = "10000000-0000-4000-8000-000000000002";
    let mut values = source_parts(client, binding, &json!({"record":binding["records"][0]}));
    let mut metadata: Value = serde_json::from_str(&values[0]).unwrap();
    metadata["id"] = json!(other_id);
    values[0] = serde_json::to_string(&metadata).unwrap();
    let upload = client.request(
        "query/recoveryBegin",
        &args(
            binding,
            json!({"id":other_id,"expectedRevision":null,"parts":descriptions(&values)}),
        ),
    );
    append_parts(client, binding, &upload, &values);
    let stored = client.request(
        "query/recoveryCommit",
        &args(binding, json!({"updateId":upload["updateId"]})),
    );
    let blocked = client.response(
        "query/sessionRestore",
        &source_args(
            binding,
            json!({"id":other_id,
        "revision":stored["revision"],"format":"editor"}),
        ),
    );
    assert_eq!(
        blocked["error"]["data"]["details"],
        json!({"reason":"pending_apply","id":ID})
    );
    client.request(
        "query/recoveryRemove",
        &args(
            binding,
            json!({"id":other_id,"revision":stored["revision"]}),
        ),
    );
}

/// Fresh counters must reconcile only an original or exactly applied owner, leaving a newer draft intact.
fn reconcile(
    client: &mut Client,
    binding: &Value,
    input: &str,
    output: &str,
    applied: bool,
    dirty: bool,
) -> Value {
    check_pending_owner_open(client, binding, input);
    let restored = client.request(
        "query/sessionRestore",
        &source_args(
            binding,
            json!({"id":ID,
        "revision":binding["records"][0]["revision"],"format":"editor"}),
        ),
    );
    assert_eq!(restored["resumed"], false);
    assert_eq!(restored["applyPending"], true);
    assert_eq!(prepared_part(client, binding, &restored, "output"), output);
    assert_eq!(
        prepared_part(client, binding, &restored, "appliedDraft"),
        "SELECT applied 😀"
    );
    let request = source_args(
        binding,
        json!({"querySessionId":restored["querySessionId"],"revision":restored["revision"],
        "owner":native(1,output,dirty,false),"comments":[]}),
    );
    assert_eq!(
        client.response("query/sessionPrepare", &request)["error"]["data"]["details"]["reason"],
        "resume_required"
    );
    let mut wrong = request.clone();
    wrong["owner"]["snapshot"] = json!(snapshot("unrelated owner"));
    assert_eq!(
        client.response("query/sessionResume", &wrong)["error"]["data"]["details"]["reason"],
        "invalid_receipt"
    );
    let current = if applied { output } else { input };
    if dirty {
        let mut false_clean = request.clone();
        false_clean["owner"]["isDirty"] = json!(false);
        assert_eq!(
            client.response("query/sessionResume", &false_clean)["error"]["data"]["details"]["reason"],
            "conflict"
        );
    }
    let state = client.request(
        "query/sessionResume",
        &source_args(
            binding,
            json!({"querySessionId":restored["querySessionId"],
        "revision":restored["revision"],"owner":native(1,current,dirty,false)}),
        ),
    );
    assert_eq!(state["applyPending"], false);
    assert_eq!(state["resumed"], true);
    let texts = source_parts(client, binding, &state);
    assert_eq!(texts[1], current);
    assert_eq!(
        texts[2],
        if applied {
            "SELECT applied 😀"
        } else {
            "SELECT 1"
        }
    );
    assert_eq!(texts[3], "SELECT newer 😀");
    assert!(state["record"].get("prepared").is_none());
    state
}

/// Kill real standalone processes before editing, after dirty editing, after saving, and between clean edit/save.
#[test]
fn standalone_prepared_intent_reconciles_each_crash_boundary_without_replaying_source_edits() {
    for mode in 0..4 {
        let (_directory, root, private) = source_fixture();
        let input = XML.strip_prefix('\u{feff}').unwrap().replace(
            "<other>keep</other>",
            if mode == 1 {
                "<other>dirty 😀</other>"
            } else {
                "<other>keep</other>"
            },
        );
        let mut first = Client::new(&root, "en");
        let binding = source_connect_version(&mut first, &root, &private, "en-US", 16);
        let state = open_source(&mut first, &binding, &input, false);
        let state = source_draft(&mut first, &binding, &state, "SELECT applied 😀");
        let prepared = first.request(
            "query/sessionPrepare",
            &source_args(
                &binding,
                json!({"querySessionId":state["querySessionId"],
            "revision":state["revision"],"owner":native(7,&input,mode==1,false),"comments":[]}),
            ),
        );
        let output = applied_text(&input, &prepared["plan"]);
        let latest = source_draft(&mut first, &binding, &prepared, "SELECT newer 😀");
        let path = private.join(format!("{ID}.json"));
        let disk: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        assert_eq!(disk["storageVersion"], 2);
        assert_eq!(
            snapshot(&fs::read_to_string(&path).unwrap()),
            latest["record"]["revision"]
        );
        assert_eq!(
            fs::read_to_string(root.join("src").join(SOURCE)).unwrap(),
            XML
        );
        if mode == 2 {
            fs::write(root.join("src").join(SOURCE), format!("\u{feff}{output}")).unwrap();
        }
        first.child.kill().unwrap();
        first.child.wait().unwrap();
        check_old_adapter(&root, &private, &latest["record"]["revision"]);
        let owner_before = fs::read(root.join("src").join(SOURCE)).unwrap();
        let mut client = Client::new(&root, "ru");
        let binding = source_connect_version(&mut client, &root, &private, "ru-RU", 16);
        let state = reconcile(
            &mut client,
            &binding,
            &input,
            &output,
            mode != 0,
            mode == 1 || mode == 3,
        );
        assert_eq!(
            fs::read(root.join("src").join(SOURCE)).unwrap(),
            owner_before
        );
        let disk: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        assert_eq!(disk["storageVersion"], 1);
        assert!(disk.get("prepared").is_none());
        client.request(
            "query/recoveryRemove",
            &args(
                &binding,
                json!({"id":ID,"revision":state["record"]["revision"]}),
            ),
        );
        client.finish();
    }
}

/// An intent can be inspected and explicitly discarded with a broken manifest; ordinary uploads cannot erase it.
#[test]
fn standalone_prepared_intent_survives_generic_drafts_and_is_readable_without_metadata() {
    let (_directory, root, private) = source_fixture();
    let input = XML.strip_prefix('\u{feff}').unwrap();
    let draft = format!("SELECT 2\n// {}", "😀 unfinished ".repeat(12_000));
    let mut first = Client::new(&root, "en");
    let binding = source_connect_version(&mut first, &root, &private, "en-US", 16);
    let state = open_source(&mut first, &binding, input, false);
    let state = source_draft(&mut first, &binding, &state, &draft);
    let prepared = first.request(
        "query/sessionPrepare",
        &source_args(
            &binding,
            json!({"querySessionId":state["querySessionId"],
        "revision":state["revision"],"owner":native(7,input,false,false),"comments":[]}),
        ),
    );
    let output = applied_text(input, &prepared["plan"]);
    first.child.kill().unwrap();
    first.child.wait().unwrap();
    fs::write(root.join("eska.toml"), "[broken").unwrap();
    let mut client = Client::new(&root, "ru");
    client.request("initialize",&json!({"apiVersion":{"major":1,"minor":16},"client":{"name":"private-recovery","version":"1"},"locale":"ru-RU"}));
    let binding = client.request(
        "query/recoveryOpen",
        &json!({"directory":{"value":private,"encoding":"utf-8"}}),
    );
    let state = json!({"record":binding["records"][0]});
    assert_eq!(
        prepared_part(&mut client, &binding, &state, "output"),
        output
    );
    assert_eq!(
        prepared_part(&mut client, &binding, &state, "appliedDraft"),
        draft
    );
    let mut values = source_parts(&mut client, &binding, &state);
    values[3] = "SELECT latest\r\n// private 😀\0".into();
    let updated = save(
        &mut client,
        &binding,
        &values,
        state["record"]["revision"].as_str(),
    );
    let state = json!({"record":updated});
    assert_eq!(source_parts(&mut client, &binding, &state)[3], values[3]);
    assert_eq!(
        prepared_part(&mut client, &binding, &state, "output"),
        output
    );
    let mut forged = values;
    let mut metadata: Value = serde_json::from_str(&forged[0]).unwrap();
    metadata["ownerVersion"] = json!("999");
    forged[0] = serde_json::to_string(&metadata).unwrap();
    let upload = begin(
        &mut client,
        &binding,
        &forged,
        state["record"]["revision"].as_str(),
    );
    append_parts(&mut client, &binding, &upload, &forged);
    assert_eq!(
        client.response(
            "query/recoveryCommit",
            &args(&binding, json!({"updateId":upload["updateId"]}))
        )["error"]["data"]["details"]["reason"],
        "recovery_conflict"
    );
    let remove = args(
        &binding,
        json!({"id":ID,"revision":state["record"]["revision"]}),
    );
    assert_eq!(
        client.response("query/recoveryRemove", &remove)["error"]["data"]["details"]["reason"],
        "pending_apply"
    );
    let mut discard = remove;
    discard["discardPrepared"] = json!(true);
    assert_eq!(
        client.request("query/recoveryRemove", &discard)["released"],
        true
    );
    assert!(!private.join(format!("{ID}.json")).exists());
    assert_eq!(
        fs::read_to_string(root.join("src").join(SOURCE)).unwrap(),
        XML
    );
    client.finish();
}
