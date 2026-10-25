use super::*;

const BSL_PATH: &str = "Ext/ManagedApplicationModule.bsl";
const BSL: &str = "// dirty owner 😀\r\nx = \"SELECT 1\r\n|AS Value\";\r\ny = 42;\r\n";

/// API 1.19 applies only the exact captured native revision and never asks the adapter to save a BSL module.
#[test]
fn bsl_session_protected_snapshot_conflicts_and_explicit_unsaved_receipt() {
    for locale in ["en-US", "ru-RU"] {
        let (_directory, root, private) = source_fixture();
        let disk = root.join("src").join(BSL_PATH);
        fs::write(&disk, BSL.replace("dirty owner", "disk owner")).unwrap();
        let original = fs::read(&disk).unwrap();
        let mut client = Client::new(&root, locale);
        let binding = source_connect_version(&mut client, &root, &private, locale, 19);
        let root_node = client.request("metadata/root", &source_args(&binding, json!({})));
        let module = json!({"kind":"module","owner":root_node["node"]["id"]["objectId"],"role":"managed-application"});
        let document = client.request(
            "query/open",
            &source_args(
                &binding,
                json!({"node":module,
            "source":{"value":BSL_PATH,"encoding":"utf-8"},"version":"7","text":BSL}),
            ),
        );
        let query = client.request(
            "query/read",
            &source_args(
                &binding,
                json!({"documentId":document["documentId"],
            "version":document["version"],"snapshot":document["snapshot"],"contextFingerprint":document["contextFingerprint"],"at":{"line":1,"character":7}}),
            ),
        );
        let state = client.request(
            "query/sessionOpen",
            &source_args(
                &binding,
                json!({"documentId":document["documentId"],
            "id":ID,"query":query["query"]["identity"],"format":"editor","userReadOnly":false}),
            ),
        );
        assert_eq!(state["protection"]["ownerLock"], Value::Null);
        assert_eq!(
            state["protection"]["ownerProtection"],
            "protectedViewWithConflicts"
        );
        assert_eq!(state["draftWritable"], true);
        let state = reject_native(
            &mut client,
            &binding,
            &state,
            &native(7, &BSL.replace("42", "43"), true, false),
        );
        let state = source_draft(&mut client, &binding, &state, "SELECT 2\nAS Value");
        let state = reject_native(&mut client, &binding, &state, &native(8, BSL, true, false));
        assert_eq!(
            source_parts(&mut client, &binding, &state)[3],
            "SELECT 2\nAS Value"
        );
        let prepared = client.request("query/sessionPrepare", &source_args(&binding, json!({"querySessionId":state["querySessionId"],
            "revision":state["revision"],"owner":native(7,BSL,true,false),"comments":[],"lineEnding":"crlf"})));
        assert_eq!(prepared["plan"]["saveOwner"], false);
        assert!(prepared["record"]["prepared"].is_object());
        let output = applied_text(BSL, &prepared["plan"]);
        assert!(output.starts_with("// dirty owner 😀\r\nx = "));
        assert!(output.ends_with(";\r\ny = 42;\r\n"));
        let applied = client.request(
            "query/sessionApplied",
            &source_args(
                &binding,
                json!({"querySessionId":state["querySessionId"],
            "applyToken":prepared["plan"]["applyToken"],"owner":native(8,&output,true,false)}),
            ),
        );
        assert_eq!(source_parts(&mut client, &binding, &applied)[1], output);
        assert_eq!(fs::read(&disk).unwrap(), original);
        let closed = client.request(
            "query/sessionClose",
            &source_args(&binding, json!({"querySessionId":state["querySessionId"]})),
        );
        assert_eq!(
            closed["releasedSourceRestriction"],
            state["protection"]["sourceRestriction"]
        );
        assert_eq!(closed["releasedOwnerLock"], Value::Null);
        client.finish();
    }
}

/// A conflicting version or hash pauses the session without consuming its draft or granting a new owner binding.
fn reject_native(client: &mut Client, binding: &Value, state: &Value, owner: &Value) -> Value {
    let changed = client.response(
        "query/sessionPrepare",
        &source_args(
            binding,
            json!({"querySessionId":state["querySessionId"],
        "revision":state["revision"],"owner":owner,"comments":[]}),
        ),
    );
    assert_eq!(
        changed["error"]["data"]["details"]["reason"],
        "stale_version"
    );
    client.request(
        "query/sessionResume",
        &source_args(
            binding,
            json!({"querySessionId":state["querySessionId"],
        "revision":state["revision"],"owner":native(7,BSL,true,false)}),
        ),
    )
}
