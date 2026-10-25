use super::*;
use base64::{Engine, engine::general_purpose::STANDARD};

const PATH: &str = "Ext/ManagedApplicationModule.bsl";
const OWNER: &str = "// 😀\r\nQ = \"SELECT 1\";\r\nTail = 42;\r\n";
const DRAFT: &str = "SELECT Т.Ар\nFROM Catalog.Товары AS Т";

/// A projection starts from the common BSL session; its module file is intentionally different from the dirty owner.
pub(super) fn open(client: &mut Client, binding: &Value) -> Value {
    let root = client.request("metadata/root", &source_args(binding, json!({})));
    let node = json!({"kind":"module","owner":root["node"]["id"]["objectId"],"role":"managed-application"});
    let doc = client.request(
        "query/open",
        &source_args(
            binding,
            json!({"node":node,
        "source":{"value":PATH,"encoding":"utf-8"},"version":"7","text":OWNER}),
        ),
    );
    let query = client.request("query/read", &source_args(binding, json!({"documentId":doc["documentId"],
        "version":doc["version"],"snapshot":doc["snapshot"],"contextFingerprint":doc["contextFingerprint"],"at":{"line":1,"character":8}})));
    let state = client.request(
        "query/sessionOpen",
        &source_args(
            binding,
            json!({"documentId":doc["documentId"],
        "id":ID,"query":query["query"]["identity"],"format":"editor","userReadOnly":false}),
        ),
    );
    source_draft(client, binding, &state, DRAFT)
}

/// Tiny binary chunks deliberately split UTF-8 scalars and are checked against the complete captured hash.
fn body(client: &mut Client, binding: &Value, view: &Value) -> String {
    let mut bytes = Vec::new();
    loop {
        let chunk = client.request(
            "query/projectionRead",
            &source_args(
                binding,
                json!({"projectionId":view["projectionId"],
            "snapshot":view["snapshot"],"offset":bytes.len(),"bytes":7}),
            ),
        );
        bytes.extend(STANDARD.decode(chunk["data"].as_str().unwrap()).unwrap());
        assert_eq!(chunk["nextOffset"], bytes.len());
        if chunk["eof"] == true {
            break;
        }
    }
    let text = String::from_utf8(bytes).unwrap();
    assert_eq!(snapshot(&text), view["snapshot"]);
    text
}

/// The portable RPC maps positions and edits without an apply intent, source write or draft/checkpoint mutation.
#[test]
fn query_projection_rpc_maps_unsaved_bsl_and_never_writes_or_retains_its_source_lease() {
    for locale in ["en-US", "ru-RU"] {
        let (_directory, root, private) = source_fixture();
        fs::write(root.join("src").join(PATH), OWNER.replace("42", "41")).unwrap();
        let disk = fs::read(root.join("src").join(PATH)).unwrap();
        let mut client = Client::new(&root, locale);
        let binding = source_connect_version(&mut client, &root, &private, locale, 20);
        let state = open(&mut client, &binding);
        let view = client.request(
            "query/projectionOpen",
            &source_args(
                &binding,
                json!({"querySessionId":state["querySessionId"],
            "revision":state["revision"],"owner":native(7,OWNER,true,false)}),
            ),
        );
        assert_eq!(view["readOnly"], true);
        assert_eq!(view["revision"], state["revision"]);
        assert_eq!(view["draftSnapshot"], snapshot(DRAFT));
        let text = body(&mut client, &binding, &view);
        assert!(text.starts_with("// 😀\r\nQ = \"SELECT Т.Ар"));
        assert!(text.ends_with(";\r\nTail = 42;\r\n"));
        let mapped = client.request("query/projectionMap", &source_args(&binding, json!({"projectionId":view["projectionId"],
            "snapshot":view["snapshot"],"direction":"draftToOwner","positions":[{"line":0,"character":11}]})));
        assert_eq!(
            mapped["positions"][0],
            json!({"target":"owner","position":{"line":1,"character":16}})
        );
        let mapped = client.request("query/projectionMap", &source_args(&binding, json!({"projectionId":view["projectionId"],
            "snapshot":view["snapshot"],"direction":"ownerToDraft","positions":[{"line":1,"character":16}]})));
        assert_eq!(
            mapped["positions"][0],
            json!({"target":"draft","position":{"line":0,"character":11}})
        );
        let edits = client.request("query/projectionEdits", &source_args(&binding, json!({"projectionId":view["projectionId"],
            "snapshot":view["snapshot"],"edits":[{"range":{"start":{"line":1,"character":14},"end":{"line":1,"character":16}},"newText":"Артикул"}]})));
        assert_eq!(edits["inputSnapshot"], snapshot(DRAFT));
        assert_eq!(edits["replacements"][0]["text"], "тикул");
        assert_eq!(
            edits["outputSnapshot"],
            snapshot(&DRAFT.replace("Ар", "Артикул"))
        );
        let current = client.request(
            "query/sessionGet",
            &source_args(&binding, json!({"querySessionId":state["querySessionId"]})),
        );
        assert_eq!(current["revision"], state["revision"]);
        assert_eq!(current["record"]["revision"], state["record"]["revision"]);
        assert_eq!(current["applyPending"], false);
        assert_eq!(source_parts(&mut client, &binding, &current)[3], DRAFT);
        client.request(
            "query/sessionClose",
            &source_args(&binding, json!({"querySessionId":state["querySessionId"]})),
        );
        assert_eq!(body(&mut client, &binding, &view), text);
        assert_eq!(fs::read(root.join("src").join(PATH)).unwrap(), disk);
        client.request(
            "query/projectionClose",
            &source_args(&binding, json!({"projectionId":view["projectionId"]})),
        );
        let closed = client.response(
            "query/projectionRead",
            &source_args(
                &binding,
                json!({"projectionId":view["projectionId"],
            "snapshot":view["snapshot"],"offset":0,"bytes":7}),
            ),
        );
        assert_eq!(
            closed["error"]["data"]["details"]["reason"],
            "unknown_projection"
        );
        client.finish();
    }
}

/// Old clients cannot opt into projection by guessing methods; immutable views remain bounded and namespace-checked.
#[test]
fn query_projection_rpc_requires_negotiation_and_bounds_live_handles() {
    let (_directory, root, private) = source_fixture();
    fs::write(root.join("src").join(PATH), OWNER).unwrap();
    let mut old = Client::new(&root, "en");
    source_connect_version(&mut old, &root, &private, "en-US", 19);
    assert_eq!(
        old.response("query/projectionOpen", &json!({}))["error"]["code"],
        -32601
    );
    old.finish();
    let mut client = Client::new(&root, "en");
    let binding = source_connect_version(&mut client, &root, &private, "en-US", 20);
    let state = open(&mut client, &binding);
    let args = source_args(
        &binding,
        json!({"querySessionId":state["querySessionId"],"revision":state["revision"],"owner":native(7,OWNER,true,false)}),
    );
    let mut views = Vec::new();
    for _ in 0..16 {
        views.push(client.request("query/projectionOpen", &args));
    }
    let failed = client.response("query/projectionOpen", &args);
    assert_eq!(
        failed["error"]["data"]["details"]["reason"],
        "projection_limit"
    );
    let wrong = client.response(
        "query/projectionMap",
        &source_args(
            &binding,
            json!({"projectionId":views[0]["projectionId"],
        "snapshot":"wrong","direction":"ownerToDraft","positions":[]}),
        ),
    );
    assert_eq!(
        wrong["error"]["data"]["details"]["reason"],
        "projection_snapshot_mismatch"
    );
    let outside = client.response("query/projectionEdits", &source_args(&binding, json!({"projectionId":views[0]["projectionId"],
        "snapshot":views[0]["snapshot"],"edits":[{"range":{"start":{"line":0,"character":0},"end":{"line":0,"character":0}},"newText":"unsafe"}]})));
    assert_eq!(
        outside["error"]["data"]["details"]["reason"],
        "invalid_position"
    );
    for view in views {
        client.request(
            "query/projectionClose",
            &source_args(&binding, json!({"projectionId":view["projectionId"]})),
        );
    }
    client.finish();
}
