use super::*;

/// Chunk only at UTF-8 boundaries and verify each acknowledged byte offset.
fn upload(client: &mut Client, key: &Value, text: &str) -> String {
    let opened = client.ok(
        "query/bufferBegin",
        with(key, json!({"bytes":text.len(),"snapshot":snapshot(text)})),
    );
    let id = opened["bufferId"].as_str().unwrap().to_owned();
    let mut offset = 0;
    while offset < text.len() {
        let mut end = (offset + 131_072).min(text.len());
        while !text.is_char_boundary(end) {
            end -= 1;
        }
        let result = client.ok(
            "query/bufferAppend",
            with(
                key,
                json!({"bufferId":id,"offset":offset,"text":text[offset..end]}),
            ),
        );
        assert_eq!(result["receivedBytes"], end);
        assert_eq!(result["ready"], end == text.len());
        offset = end;
    }
    id
}

/// A dirty owner larger than a frame is transferred intact without changing the frame limit or disk.
#[test]
fn query_rpc_uploads_large_unicode_owner_and_draft_in_bounded_frames() {
    let directory = TestDir::new();
    fixture_query(&directory.0);
    let before = bytes(&directory.0);
    let mut client = Client::new("ru-RU");
    initialize(&mut client, "ru-RU");
    let workspace = client.open(&directory.0);
    let key = key(&workspace, 0);
    let large = format!(
        "\u{feff}// {}\n{}",
        "x".repeat(1_100_000),
        INPUT.trim_start_matches('\u{feff}')
    );
    let owner_buffer = upload(&mut client, &key, &large);
    let opened = client.ok("query/open", with(&key, json!({"node":module(&workspace,0),"source":{"value":SOURCE,"encoding":"utf-8"},"version":"9","bufferId":owner_buffer})));
    let revision = binding(&key, &opened);
    let query = client.ok(
        "query/read",
        with(
            &revision,
            json!({"at":point(&large,large.find('"').unwrap())}),
        ),
    );
    let draft = format!("ВЫБРАТЬ \"😀\"\nИЗ Таблица // {}", "д".repeat(100_000));
    let draft_buffer = upload(&mut client, &key, &draft);
    let plan = client.ok(
        "query/plan",
        with(
            &revision,
            json!({"query":query["query"]["identity"],"bufferId":draft_buffer,"comments":[]}),
        ),
    );
    assert_eq!(plan["projectedQuery"]["text"], draft);
    assert_eq!(
        plan["projectedQuery"]["comments"][0]["text"],
        "\t// owner comment"
    );
    assert_eq!(plan["outputSnapshot"], snapshot(&apply(&large, &plan)));
    for id in [owner_buffer, draft_buffer] {
        assert_eq!(
            client.ok("query/bufferClose", with(&key, json!({"bufferId":id})))["released"],
            true
        );
    }
    client.finish();
    assert_eq!(bytes(&directory.0), before);
}

/// Out-of-order, oversized and corrupt appends cannot create a usable partial owner.
#[test]
fn query_rpc_rejects_partial_and_corrupt_uploads_without_losing_the_connection() {
    let directory = TestDir::new();
    fixture_query(&directory.0);
    let mut client = Client::new("en-US");
    initialize(&mut client, "en-US");
    let workspace = client.open(&directory.0);
    let key = key(&workspace, 0);
    let buffer = client.ok(
        "query/bufferBegin",
        with(&key, json!({"bytes":4,"snapshot":snapshot("test")})),
    );
    let id = &buffer["bufferId"];
    let open = with(
        &key,
        json!({"node":module(&workspace,0),"source":{"value":SOURCE,"encoding":"utf-8"},"version":"1","bufferId":id}),
    );
    assert_eq!(
        client.request("query/open", open.clone())["error"]["data"]["details"]["reason"],
        "unknown_buffer"
    );
    let invalid = client.request(
        "query/bufferAppend",
        with(&key, json!({"bufferId":id,"offset":1,"text":"t"})),
    );
    assert_eq!(
        invalid["error"]["data"]["details"]["reason"],
        "upload_offset"
    );
    let invalid = client.request(
        "query/bufferAppend",
        with(
            &key,
            json!({"bufferId":id,"offset":0,"text":"a".repeat(131_073)}),
        ),
    );
    assert_eq!(invalid["error"]["data"]["kind"], "resource_limit");
    client.ok(
        "query/bufferAppend",
        with(&key, json!({"bufferId":id,"offset":0,"text":"te"})),
    );
    let invalid = client.request(
        "query/bufferAppend",
        with(&key, json!({"bufferId":id,"offset":2,"text":"xx"})),
    );
    assert_eq!(
        invalid["error"]["data"]["details"]["reason"],
        "snapshot_mismatch"
    );
    assert_eq!(
        client.request("query/open", open)["error"]["data"]["details"]["reason"],
        "unknown_buffer"
    );
    assert_eq!(
        client.ok("query/bufferClose", with(&key, json!({"bufferId":id})))["released"],
        false
    );
    client.ok("project/info", json!({"sessionId":key["sessionId"]}));
    client.finish();
}

/// Reservations count toward the aggregate limit and remain releasable with a stale generation.
#[test]
fn query_rpc_bounds_retained_storage_and_releases_it_on_workspace_close() {
    let directory = TestDir::new();
    fixture_query(&directory.0);
    let mut client = Client::new("en-US");
    initialize(&mut client, "en-US");
    let workspace = client.open(&directory.0);
    let key = key(&workspace, 0);
    let begin = with(&key, json!({"bytes":8*1024*1024,"snapshot":"0".repeat(64)}));
    for _ in 0..8 {
        client.send(&json!({"jsonrpc":"2.0","method":"query/bufferBegin","params":begin}));
    }
    let ids: Vec<_> = (0..4)
        .map(|_| client.ok("query/bufferBegin", begin.clone())["bufferId"].clone())
        .collect();
    assert_eq!(
        client.request("query/bufferBegin", begin.clone())["error"]["data"]["kind"],
        "resource_limit"
    );
    for id in ids {
        let close = with(&key, json!({"bufferId":id,"generation":"42"}));
        assert_eq!(client.ok("query/bufferClose", close)["released"], true);
    }
    client.ok("query/bufferBegin", begin);
    client.ok("workspace/close", json!({"sessionId":key["sessionId"]}));
    let workspace = client.open(&directory.0);
    let new_key = super::key(&workspace, 0);
    client.ok(
        "query/bufferBegin",
        with(
            &new_key,
            json!({"bytes":8*1024*1024,"snapshot":"0".repeat(64)}),
        ),
    );
    client.finish();
}
