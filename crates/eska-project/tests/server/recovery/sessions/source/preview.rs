use super::*;

/// A rooted preview retains immutable bytes before metadata discovery, with no source write authorization.
#[test]
fn rooted_preview_rpc_restores_binary_views_without_a_manifest_in_both_locales() {
    for locale in ["ru-RU", "en-US"] {
        let (_directory, root, _) = source_fixture();
        fs::remove_file(root.join("eska.toml")).unwrap();
        let path = json!({"value":"src/preview.bin","encoding":"utf-8"});
        let bytes = b"\xff\x00\xef\xbb\xbf\r\n";
        fs::write(root.join("src/preview.bin"), bytes).unwrap();
        let mut client = Client::new(&root, "en");
        let hello = client.request(
            "initialize",
            &json!({"apiVersion":{"major":1,"minor":18},
            "client":{"name":"preview-test","version":"1"},"locale":locale}),
        );
        assert_eq!(
            hello["capabilities"]["sourceProtection"]["rootedPreviews"],
            true
        );
        let header = client.request("source/previewOpen", &json!({"path":path}));
        assert_eq!(header["origin"], "disk");
        assert_eq!(header["version"], Value::Null);
        assert_eq!(header["protection"]["writable"], false);
        assert_eq!(header["protection"]["supportUnknown"], true);
        assert_eq!(header["protection"]["objects"], json!([]));
        fs::write(root.join("src/preview.bin"), "changed").unwrap();
        let mut actual = Vec::new();
        loop {
            let part = client.request(
                "source/previewRead",
                &json!({"snapshotId":header["snapshotId"],
                "snapshot":header["snapshot"],"offset":actual.len(),"bytes":3}),
            );
            actual.extend(STANDARD.decode(part["data"].as_str().unwrap()).unwrap());
            assert_eq!(part["nextOffset"], actual.len());
            if part["complete"] == true {
                break;
            }
        }
        assert_eq!(actual, bytes);
        for path in [
            "../outside.bin",
            "src/../../outside.bin",
            "src",
            "missing.bin",
        ] {
            assert!(
                client
                    .response(
                        "source/previewOpen",
                        &json!({"path":{"value":path,"encoding":"utf-8"}})
                    )
                    .get("error")
                    .is_some()
            );
        }
        assert!(
            client
                .response(
                    "source/previewOpen",
                    &json!({"path":{"value":root,"encoding":"utf-8"}})
                )
                .get("error")
                .is_some()
        );
        assert_eq!(
            client.request(
                "source/previewClose",
                &json!({"snapshotId":header["snapshotId"]})
            )["released"],
            true
        );
        assert_eq!(
            client.request(
                "source/previewClose",
                &json!({"snapshotId":header["snapshotId"]})
            )["released"],
            false
        );
    }
}

/// Metadata and unclassified handles cannot release one another, and closing a workspace cannot recycle an old preview ID.
#[test]
fn rooted_previews_keep_separate_authority_and_never_reuse_ids() {
    let (_directory, root, private) = source_fixture();
    let mut client = Client::new(&root, "en");
    let binding = source_connect_version(&mut client, &root, &private, "en-US", 18);
    let header = client.request(
        "source/snapshotOpen",
        &source_args(
            &binding,
            json!({"path":{"value":SOURCE,"encoding":"utf-8"}}),
        ),
    );
    let path = json!({"value":format!("src/{SOURCE}"),"encoding":"utf-8"});
    let preview = client.request("source/previewOpen", &json!({"path":path}));
    assert_eq!(
        client.response(
            "source/previewClose",
            &json!({"snapshotId":header["snapshotId"]})
        )["error"]["data"]["details"]["reason"],
        "unknown_snapshot"
    );
    assert_eq!(
        client.response(
            "source/snapshotClose",
            &source_args(&binding, json!({"snapshotId":preview["snapshotId"]}))
        )["error"]["data"]["details"]["reason"],
        "unknown_snapshot"
    );
    assert_eq!(
        source_bytes(&mut client, &binding, &header, 100),
        XML.as_bytes()
    );
    client.request(
        "workspace/close",
        &json!({"sessionId":binding["sessionId"]}),
    );
    let next = client.request("source/previewOpen", &json!({"path":path}));
    assert_ne!(next["snapshotId"], preview["snapshotId"]);
    assert_eq!(
        client.request(
            "source/previewClose",
            &json!({"snapshotId":preview["snapshotId"]})
        )["released"],
        false
    );
    assert_eq!(
        client.request(
            "source/previewClose",
            &json!({"snapshotId":next["snapshotId"]})
        )["released"],
        true
    );
    let mut old = Client::new(&root, "en");
    let hello = old.request("initialize", &json!({"apiVersion":{"major":1,"minor":17},"client":{"name":"old-preview-test","version":"1"}}));
    assert_eq!(
        hello["capabilities"]["sourceProtection"]["rootedPreviews"],
        false
    );
    assert_eq!(
        old.response("source/previewOpen", &json!({"path":path}))["error"]["code"],
        -32601
    );
}

/// Unclassified views consume the existing shared query/source byte budget, rather than creating a second allowance.
#[test]
fn rooted_preview_uses_the_shared_source_snapshot_budget() {
    let (_directory, root, private) = source_fixture();
    fs::write(root.join("preview.bin"), vec![0xff; 8_388_608]).unwrap();
    let mut client = Client::new(&root, "en");
    let binding = source_connect_version(&mut client, &root, &private, "en-US", 18);
    let mut handles = Vec::new();
    for _ in 0..4 {
        handles.push(client.request(
            "source/previewOpen",
            &json!({"path":{"value":"preview.bin","encoding":"utf-8"}}),
        ));
    }
    assert_eq!(
        client.response(
            "query/bufferBegin",
            &source_args(&binding, json!({"bytes":1,"snapshot":"0".repeat(64)}))
        )["error"]["data"]["kind"],
        "resource_limit"
    );
    client.request(
        "source/previewClose",
        &json!({"snapshotId":handles[0]["snapshotId"]}),
    );
    let buffer = client.request(
        "query/bufferBegin",
        &source_args(&binding, json!({"bytes":1,"snapshot":"0".repeat(64)})),
    );
    assert_eq!(buffer["receivedBytes"], 0);
    client.request(
        "query/bufferClose",
        &source_args(&binding, json!({"bufferId":buffer["bufferId"]})),
    );
    for header in &handles[1..] {
        client.request(
            "source/previewClose",
            &json!({"snapshotId":header["snapshotId"]}),
        );
    }
}
