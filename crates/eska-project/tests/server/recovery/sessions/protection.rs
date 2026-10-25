use super::*;

/// The same backend rejects competing writers for every adapter; errors and restriction IDs are locale-independent.
#[test]
fn source_session_owns_backend_writes_until_its_exact_handle_is_closed() {
    let mut expected = None;
    for locale in ["ru-RU", "en-US"] {
        let (_directory, root, private) = source_fixture();
        let descriptor = root.join("src/CommonForms/Queries.xml");
        fs::write(
            &descriptor,
            fs::read_to_string(&descriptor)
                .unwrap()
                .replace("</Properties>", "<Comment>before</Comment></Properties>"),
        )
        .unwrap();
        let mut client = Client::new(&root, "en");
        let mut binding = source_connect_options(&mut client, &root, &private, locale, 16, true);
        let id = binding["node"]["objectId"].clone();
        let editing = client.request(
            "metadata/propertyEditing",
            &source_args(&binding, json!({"objectId":id})),
        );
        assert_eq!(editing["writable"], true);
        let comment = editing["fields"]
            .as_array()
            .unwrap()
            .iter()
            .find(|field| field["value"] == "before")
            .unwrap()["path"]
            .clone();
        let changed = client.request("metadata/updateProperty", &source_args(&binding, json!({"objectId":id,
            "snapshot":editing["snapshot"],"path":comment,"change":{"kind":"text","value":"after"}})));
        binding["generation"] = changed["generation"].clone();
        let editing = &changed["editing"];
        let owner = XML.strip_prefix('\u{feff}').unwrap();
        let session = open_source(&mut client, &binding, owner, false);
        assert_eq!(session["protection"]["sourceRestriction"], "1");
        assert_eq!(
            session["protection"]["ownerLock"],
            Value::Null,
            "backend ownership does not claim native input prevention"
        );
        let before = fs::read(&descriptor).unwrap();
        let mut machine = competing_writers(&mut client, &binding, editing, &comment);
        assert_eq!(fs::read(&descriptor).unwrap(), before);
        assert_eq!(
            fs::read_to_string(root.join("src").join(SOURCE)).unwrap(),
            XML
        );
        let released = release_source(&mut client, &binding, &session, owner);
        let undone = client.request(
            "metadata/undoProperty",
            &source_args(
                &binding,
                json!({"objectId":id,
            "snapshot":released["snapshot"],"direction":"undo"}),
            ),
        );
        assert!(undone["generation"].as_str().is_some());
        assert!(
            fs::read_to_string(&descriptor)
                .unwrap()
                .contains("<Comment>before</Comment>")
        );
        machine["restriction"] = session["protection"]["sourceRestriction"].clone();
        if let Some(expected) = &expected {
            assert_eq!(expected, &machine);
        } else {
            expected = Some(machine);
        }
        client.finish();
    }
}

/// Check real competing writers and return only stable machine evidence, with no localized captions.
fn competing_writers(
    client: &mut Client,
    binding: &Value,
    editing: &Value,
    comment: &Value,
) -> Value {
    let id = &binding["node"]["objectId"];
    let locked = client.request(
        "metadata/propertyEditing",
        &source_args(binding, json!({"objectId":id})),
    );
    assert_eq!(locked["writable"], false);
    let unknown = client.request(
        "query/sessionClose",
        &source_args(binding, json!({"querySessionId":"unrelated-session"})),
    );
    assert_eq!(unknown["released"], false);
    let still_locked = client.request(
        "metadata/propertyEditing",
        &source_args(binding, json!({"objectId":id})),
    );
    assert_eq!(still_locked["writable"], false);
    let update = client.response("metadata/updateProperty", &source_args(binding, json!({"objectId":id,
        "snapshot":editing["snapshot"],"path":comment,"change":{"kind":"text","value":"forbidden"}})));
    assert_eq!(update["error"]["data"]["kind"], "property_read_only");
    let undo = client.response(
        "metadata/undoProperty",
        &source_args(
            binding,
            json!({"objectId":id,
        "snapshot":editing["snapshot"],"direction":"undo"}),
        ),
    );
    assert_eq!(undo["error"]["data"]["kind"], "property_read_only");
    let rename = client.request(
        "metadata/renamePreview",
        &source_args(binding, json!({"objectId":id,"newName":"Renamed"})),
    );
    assert_eq!(rename["applyAvailable"], false);
    assert!(
        rename["plan"]["issues"]
            .as_array()
            .unwrap()
            .iter()
            .any(|issue| issue["reason"] == "source_session_active")
    );
    let blocked = client.response(
        "metadata/renameApply",
        &source_args(
            binding,
            json!({"objectId":id,"newName":"Renamed",
        "snapshot":rename["plan"]["snapshot"],"reviewedUncertain":true}),
        ),
    );
    assert_eq!(blocked["error"]["data"]["kind"], "rename_blocked");
    json!({"update":update["error"]["data"]["kind"],"undo":undo["error"]["data"]["kind"],
        "rename":blocked["error"]["data"]["kind"],"issues":rename["plan"]["issues"]})
}

/// The owner's plan still works; close and checkpoint removal each release their own backend restriction.
fn release_source(client: &mut Client, binding: &Value, session: &Value, owner: &str) -> Value {
    let id = &binding["node"]["objectId"];
    let prepared = client.request(
        "query/sessionPrepare",
        &source_args(
            binding,
            json!({"querySessionId":session["querySessionId"],
        "revision":session["revision"],"owner":native(7,owner,false,false),"comments":[]}),
        ),
    );
    assert_eq!(
        prepared["plan"]["noOp"], true,
        "the owning query can still prepare"
    );
    let aborted = client.request(
        "query/sessionAbort",
        &source_args(
            binding,
            json!({"querySessionId":session["querySessionId"],
        "applyToken":prepared["plan"]["applyToken"]}),
        ),
    );
    client.request(
        "query/sessionClose",
        &source_args(binding, json!({"querySessionId":session["querySessionId"]})),
    );
    let released = client.request(
        "metadata/propertyEditing",
        &source_args(binding, json!({"objectId":id})),
    );
    assert_eq!(released["writable"], true);
    let restored = client.request(
        "query/sessionRestore",
        &source_args(
            binding,
            json!({"id":ID,"revision":aborted["record"]["revision"],"format":"editor"}),
        ),
    );
    assert_eq!(restored["resumed"], false);
    assert_ne!(
        restored["protection"]["sourceRestriction"],
        session["protection"]["sourceRestriction"]
    );
    let locked = client.request(
        "metadata/propertyEditing",
        &source_args(binding, json!({"objectId":id})),
    );
    assert_eq!(locked["writable"], false);
    client.request(
        "query/recoveryRemove",
        &args(
            binding,
            json!({"id":ID,"revision":restored["record"]["revision"]}),
        ),
    );
    let released = client.request(
        "metadata/propertyEditing",
        &source_args(binding, json!({"objectId":id})),
    );
    assert_eq!(released["writable"], true);
    released
}
