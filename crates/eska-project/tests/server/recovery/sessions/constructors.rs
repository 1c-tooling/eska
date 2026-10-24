use super::*;

const DRAFT: &str = "SELECT A, // keep 😀\n B FROM T;\nDESTROY Temp";

/// Constructor identities are copied from replies; the client never implements parsing or source generation.
fn constructor_args(binding: &Value, state: &Value, extra: Value) -> Value {
    let mut value = source_args(
        binding,
        json!({"querySessionId":state["querySessionId"],
        "constructorId":state["constructor"]["id"],"constructorRevision":state["constructor"]["revision"]}),
    );
    let Value::Object(fields) = extra else {
        panic!("fixture extras")
    };
    value.as_object_mut().unwrap().extend(fields);
    value
}

/// A real generic recovery upload cannot bypass the canonical draft's exclusive constructor lock.
fn check_competing_checkpoint(client: &mut Client, binding: &Value, state: &Value) {
    let mut parts = source_parts(client, binding, state);
    parts[3] = "SELECT competing input".into();
    let upload = begin(
        client,
        binding,
        &parts,
        state["record"]["revision"].as_str(),
    );
    append_parts(client, binding, &upload, &parts);
    let result = client.response(
        "query/recoveryCommit",
        &args(binding, json!({"updateId":upload["updateId"]})),
    );
    assert_eq!(
        result["error"]["data"]["details"]["reason"],
        "constructor_busy"
    );
    let result = client.response(
        "query/recoveryRemove",
        &args(
            binding,
            json!({"id":ID,"revision":state["record"]["revision"]}),
        ),
    );
    assert_eq!(
        result["error"]["data"]["details"]["reason"],
        "constructor_busy"
    );
}

/// Model/actions/finish use the same journal and draft in either locale, while XML and its disk bytes remain untouched.
#[test]
fn constructor_rpc_owns_the_canonical_draft_and_publishes_only_on_explicit_finish() {
    for locale in ["en-US", "ru-RU"] {
        let (_directory, root, private) = source_fixture();
        let mut client = Client::new(&root, locale);
        let binding = source_connect_version(&mut client, &root, &private, locale, 21);
        let state = open_source(&mut client, &binding, XML, false);
        let state = source_draft(&mut client, &binding, &state, DRAFT);
        let opened = client.request(
            "query/constructorOpen",
            &source_args(
                &binding,
                json!({"querySessionId":state["querySessionId"],"revision":state["revision"]}),
            ),
        );
        assert_eq!(opened["constructorActive"], true);
        assert_eq!(opened["draftWritable"], false);
        assert_eq!(opened["constructor"]["text"], DRAFT);
        assert_eq!(
            opened["constructor"]["statements"][1]["unavailable"],
            "unsupported_statement"
        );
        check_competing_checkpoint(&mut client, &binding, &opened);
        let changed = client.request(
            "query/constructorAction",
            &constructor_args(
                &binding,
                &opened,
                json!({"action":{"kind":"append","statement":0,"clause":"fields","text":"C"}}),
            ),
        );
        let text = changed["constructor"]["text"].as_str().unwrap();
        assert!(text.ends_with(";\nDESTROY Temp"));
        assert!(text.contains("// keep 😀"));
        assert_eq!(source_parts(&mut client, &binding, &changed)[3], DRAFT);
        assert_eq!(changed["record"]["revision"], state["record"]["revision"]);
        let refused = client.response(
            "query/constructorFinish",
            &constructor_args(
                &binding,
                &changed,
                json!({"nativeSnapshot":snapshot("late native input")}),
            ),
        );
        assert_eq!(refused["error"]["data"]["details"]["reason"], "stale_draft");
        let staged = client.request(
            "query/constructorPlan",
            &constructor_args(
                &binding,
                &changed,
                json!({"nativeSnapshot":snapshot(DRAFT)}),
            ),
        );
        assert_eq!(staged["constructorActive"], true);
        assert_eq!(source_parts(&mut client, &binding, &staged)[3], DRAFT);
        assert_eq!(applied_text(DRAFT, &staged["draftPlan"]), text);
        let refused=client.response("query/constructorFinish",&constructor_args(&binding,&changed,json!({"nativeSnapshot":snapshot(DRAFT),"resultSnapshot":snapshot("different native result")})));
        assert_eq!(refused["error"]["data"]["details"]["reason"], "stale_draft");
        let finished = client.request(
            "query/constructorFinish",
            &constructor_args(
                &binding,
                &changed,
                json!({"nativeSnapshot":snapshot(DRAFT)}),
            ),
        );
        assert_eq!(finished["constructorActive"], false);
        assert_eq!(finished["draftWritable"], true);
        assert_eq!(applied_text(DRAFT, &finished["draftPlan"]), text);
        assert_eq!(source_parts(&mut client, &binding, &finished)[3], text);
        assert_eq!(finished["applyPending"], false);
        assert_eq!(
            fs::read_to_string(root.join("src").join(SOURCE)).unwrap(),
            XML
        );
        let reopened = client.request("query/constructorOpen", &source_args(&binding, json!({"querySessionId":finished["querySessionId"],"revision":finished["revision"]})));
        let changed = client.request(
            "query/constructorAction",
            &constructor_args(
                &binding,
                &reopened,
                json!({"action":{"kind":"append","statement":0,"clause":"order","text":"C"}}),
            ),
        );
        let cancelled = client.request(
            "query/constructorCancel",
            &constructor_args(&binding, &changed, json!({})),
        );
        assert_eq!(cancelled["released"], true);
        assert_eq!(source_parts(&mut client, &binding, &cancelled)[3], text);
        assert_eq!(
            cancelled["record"]["revision"],
            finished["record"]["revision"]
        );
    }
}

/// Destructive comment disposition and read-only rights are core decisions, not a native UI convention.
#[test]
fn constructor_rpc_requires_comment_decision_and_preserves_read_only() {
    let (_directory, root, private) = source_fixture();
    let mut client = Client::new(&root, "en-US");
    let binding = source_connect_version(&mut client, &root, &private, "en-US", 21);
    let state = open_source(&mut client, &binding, XML, false);
    let state = source_draft(&mut client, &binding, &state, DRAFT);
    let opened = client.request(
        "query/constructorOpen",
        &source_args(
            &binding,
            json!({"querySessionId":state["querySessionId"],"revision":state["revision"]}),
        ),
    );
    let action = json!({"kind":"remove","selection":{"statement":0,"clause":0,"item":1}});
    let refused = client.response(
        "query/constructorAction",
        &constructor_args(&binding, &opened, json!({"action":action})),
    );
    assert_eq!(
        refused["error"]["data"]["details"]["reason"],
        "constructor_comments_require_decision"
    );
    let mut action = action;
    action["comments"] = json!("keep");
    let kept = client.request(
        "query/constructorAction",
        &constructor_args(&binding, &opened, json!({"action":action})),
    );
    assert!(
        kept["constructor"]["text"]
            .as_str()
            .unwrap()
            .contains("// keep 😀")
    );
    client.request(
        "query/constructorCancel",
        &constructor_args(&binding, &kept, json!({})),
    );
    client.request(
        "query/sessionClose",
        &source_args(&binding, json!({"querySessionId":state["querySessionId"]})),
    );
    client.request(
        "query/recoveryRemove",
        &args(
            &binding,
            json!({"id":ID,"revision":state["record"]["revision"]}),
        ),
    );
    let readonly = open_source(&mut client, &binding, XML, true);
    let readonly = client.request(
        "query/constructorOpen",
        &source_args(
            &binding,
            json!({"querySessionId":readonly["querySessionId"],"revision":readonly["revision"]}),
        ),
    );
    assert_eq!(readonly["constructor"]["writable"], false);
    let rejected = client.response(
        "query/constructorAction",
        &constructor_args(
            &binding,
            &readonly,
            json!({"action":{"kind":"append","statement":0,"clause":"fields","text":"2"}}),
        ),
    );
    assert_eq!(
        rejected["error"]["data"]["details"]["reason"],
        "user_read_only"
    );
    let finished = client.request(
        "query/constructorFinish",
        &constructor_args(
            &binding,
            &readonly,
            json!({"nativeSnapshot":snapshot("SELECT 1")}),
        ),
    );
    assert_eq!(finished["draftPlan"]["noOp"], true);
    assert_eq!(finished["draftWritable"], false);
}

/// Negotiated 1.20 clients retain projection/editing support without access to the new exclusive-constructor lifecycle.
#[test]
fn constructor_rpc_requires_api_121() {
    let (_directory, root, private) = source_fixture();
    let mut client = Client::new(&root, "en-US");
    source_connect_version(&mut client, &root, &private, "en-US", 20);
    let refused = client.response("query/constructorOpen", &json!({}));
    assert_eq!(refused["error"]["code"], -32601);
}

/// Source-comment placement is explicit and maps a native UTF-16 cursor through ESKA, including supplementary characters.
#[test]
fn source_comment_rpc_maps_destinations_and_requires_explicit_placement() {
    let (_directory, root, private) = source_fixture();
    let mut client = Client::new(&root, "ru-RU");
    let binding = source_connect_version(&mut client, &root, &private, "ru-RU", 21);
    let owner = XML.replace("SELECT 1", "SELECT Long<!--anchor-->Name");
    let draft = "SELECT \"😀\" AS A";
    let state = open_source(&mut client, &binding, &owner, false);
    let state = source_draft(&mut client, &binding, &state, draft);
    let comments = client.request("query/sessionComments", &source_args(&binding, json!({"querySessionId":state["querySessionId"],
        "revision":state["revision"],"positions":[{"line":0,"character":10},{"line":0,"character":9}]})));
    assert_eq!(comments["needsDecision"], true);
    assert_eq!(comments["snapshot"], snapshot(draft));
    assert_eq!(comments["comments"][0]["text"], "<!--anchor-->");
    assert_eq!(comments["positions"][0]["queryOffset"], 12);
    assert!(
        comments["positions"][1]["queryOffset"].is_null(),
        "surrogate interiors are not byte anchors"
    );
    let plan = client.request("query/sessionPrepare", &source_args(&binding, json!({"querySessionId":state["querySessionId"],"revision":state["revision"],
        "owner":native(7,&owner,true,false),"comments":[{"comment":0,"queryOffset":comments["positions"][0]["queryOffset"]}]})));
    assert!(applied_text(&owner, &plan["plan"]).contains("😀<!--anchor-->\" AS A"));
    assert_eq!(
        fs::read_to_string(root.join("src").join(SOURCE)).unwrap(),
        XML
    );
}

/// BSL source-only comments can be retained only before a continuation line, never inside its string token.
#[test]
fn source_comment_rpc_filters_unsafe_bsl_destinations() {
    let (_directory, root, private) = source_fixture();
    fs::write(
        root.join("src/Ext/ManagedApplicationModule.bsl"),
        "// disk fixture\n",
    )
    .unwrap();
    let mut client = Client::new(&root, "en-US");
    let binding = source_connect_version(&mut client, &root, &private, "en-US", 21);
    let state = projections::open(&mut client, &binding);
    let comments = client.request("query/sessionComments", &source_args(&binding, json!({"querySessionId":state["querySessionId"],
        "revision":state["revision"],"positions":[{"line":0,"character":0},{"line":0,"character":5},{"line":1,"character":0}]})));
    assert_eq!(comments["lineStartsOnly"], true);
    assert!(comments["positions"][0]["queryOffset"].is_null());
    assert!(comments["positions"][1]["queryOffset"].is_null());
    assert_eq!(
        comments["positions"][2]["queryOffset"],
        "SELECT Т.Ар\n".len()
    );
}
