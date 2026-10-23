use super::*;
use base64::{Engine, engine::general_purpose::STANDARD};

/// Read bounded binary chunks through independent framing, checking exact offsets and immutable handle identities.
fn source_bytes(client: &mut Client, binding: &Value, header: &Value, chunk: usize) -> Vec<u8> {
    let mut bytes = Vec::new();
    loop {
        let response = client.request(
            "source/snapshotRead",
            &source_args(
                binding,
                json!({"snapshotId":header["snapshotId"],
            "snapshot":header["snapshot"],"offset":bytes.len(),"bytes":chunk}),
            ),
        );
        assert_eq!(response["snapshotId"], header["snapshotId"]);
        assert_eq!(response["snapshot"], header["snapshot"]);
        assert_eq!(response["offset"], bytes.len());
        let value = STANDARD.decode(response["data"].as_str().unwrap()).unwrap();
        assert!(value.len() <= chunk);
        assert!(value.len() <= 131_072);
        bytes.extend_from_slice(&value);
        assert_eq!(response["nextOffset"], bytes.len());
        if response["complete"] == true {
            break;
        }
        assert_ne!(value, Vec::<u8>::new());
    }
    assert_eq!(header["bytes"], bytes.len());
    bytes
}

/// Only successful explicit close releases a captured source handle, independently of metadata generations.
fn close_source(client: &mut Client, binding: &Value, header: &Value) {
    assert_eq!(
        client.request(
            "source/snapshotClose",
            &source_args(binding, json!({"snapshotId":header["snapshotId"]}))
        )["released"],
        true
    );
}

/// Native overlays, binary bytes and machine access facts have one locale-independent source contract.
#[test]
fn source_snapshot_rpc_preserves_binary_and_native_bytes_in_both_locales() {
    let mut expected = None;
    for locale in ["ru-RU", "en-US"] {
        let (_directory, root, private) = source_fixture();
        let mut client = Client::new(&root, "en");
        let binding = source_connect_version(&mut client, &root, &private, locale, 17);
        let file = json!({"value":SOURCE,"encoding":"utf-8"});
        let disk = client.request(
            "source/snapshotOpen",
            &source_args(&binding, json!({"path":file})),
        );
        assert_eq!(disk["origin"], "disk");
        assert_eq!(disk["version"], Value::Null);
        assert_eq!(
            source_bytes(&mut client, &binding, &disk, 5),
            XML.as_bytes()
        );
        let original = fs::read(root.join("src").join(SOURCE)).unwrap();
        let dirty = "\u{feff}dirty 😀\r\n\"quoted\"\0";
        let editor = client.request(
            "source/snapshotOpen",
            &source_args(
                &binding,
                json!({"path":file,
            "native":{"version":"7","text":dirty,"userReadOnly":true}}),
            ),
        );
        assert_eq!(editor["origin"], "editor");
        assert_eq!(editor["version"], "7");
        assert_eq!(editor["snapshot"], snapshot(dirty));
        assert_eq!(editor["protection"]["userReadOnly"], true);
        assert_eq!(editor["protection"]["writable"], false);
        assert_eq!(
            source_bytes(&mut client, &binding, &editor, 3),
            dirty.as_bytes()
        );
        assert_eq!(fs::read(root.join("src").join(SOURCE)).unwrap(), original);
        let binary = b"\x00\xff\x01\xef\xbb\xbf\r\n";
        fs::write(root.join("src").join(SOURCE), binary).unwrap();
        let captured = client.request(
            "source/snapshotOpen",
            &source_args(&binding, json!({"path":file})),
        );
        assert_eq!(source_bytes(&mut client, &binding, &captured, 2), binary);
        assert_eq!(
            source_bytes(&mut client, &binding, &disk, 131_072),
            XML.as_bytes()
        );
        let machine = json!({"origin":editor["origin"],"snapshot":editor["snapshot"],"protection":editor["protection"]});
        if let Some(expected) = &expected {
            assert_eq!(expected, &machine);
        } else {
            expected = Some(machine);
        }
        for header in [&disk, &editor, &captured] {
            close_source(&mut client, &binding, header);
        }
        client.finish();
    }
}

/// Source ownership chooses the retained parent; readonly view handles cannot prolong its temporary write restriction.
#[test]
fn source_snapshot_rpc_prioritizes_the_query_owner_and_releases_access_independently() {
    let (_directory, root, private) = source_fixture();
    let mut client = Client::new(&root, "en");
    let binding = source_connect_version(&mut client, &root, &private, "en-US", 17);
    let dirty = XML
        .strip_prefix('\u{feff}')
        .unwrap()
        .replace("<other>keep</other>", "<other>unsaved parent 😀</other>");
    let owner = open_source(&mut client, &binding, &dirty, false);
    let file = json!({"value":SOURCE,"encoding":"utf-8"});
    let request = source_args(
        &binding,
        json!({"files":[{"path":file,"userReadOnly":false},
        {"path":{"value":"CommonForms/Queries.xml","encoding":"utf-8"},"userReadOnly":false}],"enforceSupport":false}),
    );
    let protected = client.request("source/protection", &request);
    for state in protected["files"].as_array().unwrap() {
        assert_eq!(state["writable"], false);
        assert_eq!(
            state["restrictions"][0]["id"],
            owner["protection"]["sourceRestriction"]
        );
    }
    let header = client.request(
        "source/snapshotOpen",
        &source_args(
            &binding,
            json!({"path":file,
        "native":{"version":"8","text":"later native text","userReadOnly":false}}),
        ),
    );
    assert_eq!(header["origin"], "querySession");
    assert_eq!(header["version"], "7");
    assert_eq!(
        source_bytes(&mut client, &binding, &header, 17),
        dirty.as_bytes()
    );
    client.request(
        "query/sessionClose",
        &source_args(&binding, json!({"querySessionId":owner["querySessionId"]})),
    );
    for state in client.request("source/protection", &request)["files"]
        .as_array()
        .unwrap()
    {
        assert_eq!(state["writable"], true);
        assert_eq!(state["restrictions"], json!([]));
    }
    assert_eq!(
        source_bytes(&mut client, &binding, &header, 131_072),
        dirty.as_bytes()
    );
    close_source(&mut client, &binding, &header);
    client.finish();
}

/// Reserved uploads and captured views share the 32 MiB budget; successful adoption consumes only its own upload.
#[test]
fn source_snapshot_rpc_shares_query_limits_and_adopts_large_uploads() {
    let (_directory, root, private) = source_fixture();
    let mut client = Client::new(&root, "en");
    let binding = source_connect_version(&mut client, &root, &private, "en-US", 17);
    let maximum = "x".repeat(8 * 1024 * 1024);
    fs::write(root.join("src").join(SOURCE), &maximum).unwrap();
    let path = json!({"value":SOURCE,"encoding":"utf-8"});
    let mut held = Vec::new();
    for _ in 0..4 {
        held.push(client.request(
            "source/snapshotOpen",
            &source_args(&binding, json!({"path":path})),
        ));
    }
    let limited = client.response(
        "source/snapshotOpen",
        &source_args(&binding, json!({"path":path})),
    );
    assert_eq!(limited["error"]["data"]["kind"], "resource_limit");
    let limited = client.response(
        "query/bufferBegin",
        &source_args(&binding, json!({"bytes":1,"snapshot":snapshot("x")})),
    );
    assert_eq!(limited["error"]["data"]["kind"], "resource_limit");
    close_source(&mut client, &binding, &held.remove(0));
    let begun = client.request(
        "source/bufferBegin",
        &source_args(
            &binding,
            json!({"bytes":maximum.len(),"snapshot":snapshot(&maximum)}),
        ),
    );
    let id = &begun["bufferId"];
    for offset in (0..maximum.len()).step_by(131_072) {
        client.request(
            "source/bufferAppend",
            &source_args(
                &binding,
                json!({"bufferId":id,"offset":offset,"text":&maximum[offset..offset+131_072]}),
            ),
        );
    }
    let view = client.request(
        "source/snapshotOpen",
        &source_args(
            &binding,
            json!({"path":path,
        "native":{"version":"2","bufferId":id,"userReadOnly":false}}),
        ),
    );
    assert_eq!(view["origin"], "editor");
    assert_eq!(view["bytes"], maximum.len());
    assert_eq!(view["snapshot"], snapshot(&maximum));
    assert_eq!(
        client.request(
            "source/bufferClose",
            &source_args(&binding, json!({"bufferId":id}))
        )["released"],
        false
    );
    close_source(&mut client, &binding, &view);
    for header in &held {
        close_source(&mut client, &binding, header);
    }
    let reserved = client.request(
        "query/bufferBegin",
        &source_args(
            &binding,
            json!({"bytes":maximum.len(),"snapshot":snapshot(&maximum)}),
        ),
    );
    client.request(
        "query/bufferClose",
        &source_args(&binding, json!({"bufferId":reserved["bufferId"]})),
    );
    client.finish();
}

/// Old clients cannot call the new API; captured handles allow stale generation cleanup without accepting foreign context.
#[test]
fn source_snapshot_rpc_negotiates_and_validates_contexts() {
    let (_directory, root, private) = source_fixture();
    let mut old = Client::new(&root, "en");
    let binding = source_connect_version(&mut old, &root, &private, "en-US", 16);
    for method in [
        "source/protection",
        "source/snapshotOpen",
        "source/snapshotRead",
        "source/snapshotClose",
        "source/bufferBegin",
        "source/bufferAppend",
        "source/bufferClose",
    ] {
        assert_eq!(
            old.response(method, &source_args(&binding, json!({})))["error"]["code"],
            -32601
        );
    }
    old.finish();
    let mut client = Client::new(&root, "en");
    let binding = source_connect_version(&mut client, &root, &private, "en-US", 17);
    let header = client.request(
        "source/snapshotOpen",
        &source_args(
            &binding,
            json!({"path":{"value":SOURCE,"encoding":"utf-8"}}),
        ),
    );
    let mut other = binding.clone();
    other["projectId"] = json!("foreign-project");
    let denied = client.response(
        "source/snapshotClose",
        &source_args(&other, json!({"snapshotId":header["snapshotId"]})),
    );
    assert_eq!(denied["error"]["data"]["kind"], "unknown_project");
    client.request(
        "metadata/refresh",
        &source_args(&binding, json!({"node":binding["node"]})),
    );
    assert_eq!(
        source_bytes(&mut client, &binding, &header, 131_072),
        XML.as_bytes()
    );
    close_source(&mut client, &binding, &header);
    client.finish();
}
