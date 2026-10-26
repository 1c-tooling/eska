use super::*;

/// A derived analyzer-config path is not an input or prerequisite of the plain query editor.
#[test]
fn query_rpc_does_not_require_readable_derived_analyzer_configuration() {
    let directory = TestDir::new();
    fixture_query(&directory.0);
    std::fs::create_dir(directory.0.join("bsl-analyzer.toml")).unwrap();
    let mut client = Client::new("en-US");
    initialize(&mut client, "en-US");
    let workspace = client.open(&directory.0);
    let key = key(&workspace, 0);
    let opened = client.ok("query/open",with(&key,json!({"node":module(&workspace,0),"source":{"value":SOURCE,"encoding":"utf-8"},"version":"1"})));
    client.ok(
        "query/read",
        with(
            &binding(&key, &opened),
            json!({"at":point(INPUT,INPUT.find('"').unwrap())}),
        ),
    );
    client.finish();
}

/// Every read/plan checks current byte identity even when no filesystem event has been delivered.
#[test]
fn query_rpc_rejects_stale_versions_fingerprints_and_external_source_changes() {
    let directory = TestDir::new();
    fixture_query(&directory.0);
    let mut client = Client::new("en-US");
    initialize(&mut client, "en-US");
    let workspace = client.open(&directory.0);
    let key = key(&workspace, 0);
    let opened = client.ok("query/open", with(&key, json!({"node":module(&workspace,0),"source":{"value":SOURCE,"encoding":"utf-8"},"version":"1"})));
    let revision = binding(&key, &opened);
    let read = with(
        &revision,
        json!({"at":point(INPUT,INPUT.find('"').unwrap())}),
    );
    for (field, value, reason) in [
        ("version", "2", "stale_version"),
        ("snapshot", "stale", "stale_version"),
        ("contextFingerprint", "stale", "context_changed"),
    ] {
        let mut invalid = read.clone();
        invalid[field] = json!(value);
        assert_eq!(
            client.request("query/read", invalid)["error"]["data"]["details"]["reason"],
            reason
        );
    }
    std::fs::write(
        directory.0.join("src").join(SOURCE),
        INPUT.replace("outside", "external"),
    )
    .unwrap();
    assert_eq!(
        client.request("query/read", read.clone())["error"]["data"]["details"]["reason"],
        "conflict"
    );
    std::fs::write(directory.0.join("src").join(SOURCE), INPUT).unwrap();
    let manifest = directory.0.join("eska.toml");
    std::fs::write(
        &manifest,
        format!(
            "{}\n# changed\n",
            std::fs::read_to_string(&manifest).unwrap()
        ),
    )
    .unwrap();
    assert_eq!(
        client.request("query/read", read)["error"]["data"]["details"]["reason"],
        "context_changed"
    );
    client.ok(
        "query/close",
        with(
            &key,
            json!({"documentId":opened["documentId"],"generation":"9"}),
        ),
    );
    client.finish();
}

/// A policy change affects writable availability but preserves read access to the frozen query.
#[test]
fn query_rpc_read_only_support_policy_cannot_be_bypassed_by_a_plan() {
    let directory = TestDir::new();
    fixture_query(&directory.0);
    let mut client = Client::new("en-US");
    initialize(&mut client, "en-US");
    let workspace = client.open(&directory.0);
    let key = key(&workspace, 0);
    let opened = client.ok("query/open", with(&key, json!({"node":module(&workspace,0),"source":{"value":SOURCE,"encoding":"utf-8"},"version":"1"})));
    let revision = binding(&key, &opened);
    let root = "11111111-1111-1111-1111-111111111111";
    let rules = format!(
        "{{6,0,1,{root},0,{root},\"1\",\"Vendor\",\"Test\",1,0,0,{root},{root},0,0,0,1,0,0,0,1,0,1,0,1,1,1,1}}"
    );
    std::fs::write(directory.0.join("src/Ext/ParentConfigurations.bin"), rules).unwrap();
    let read = client.ok(
        "query/read",
        with(
            &revision,
            json!({"at":point(INPUT,INPUT.find('"').unwrap())}),
        ),
    );
    assert_eq!(read["writable"], false);
    let plan = client.request(
        "query/plan",
        with(
            &revision,
            json!({"query":read["query"]["identity"],"text":read["query"]["text"],"comments":[]}),
        ),
    );
    assert_eq!(plan["error"]["data"]["details"]["reason"], "read_only");
    client.finish();
}

/// Identical metadata IDs in different members cannot select another application's source or upload.
#[test]
fn query_rpc_isolates_application_members_and_their_retained_buffers() {
    let directory = TestDir::new();
    for name in ["first", "second"] {
        let member = directory.0.join(name);
        std::fs::create_dir_all(&member).unwrap();
        fixture_query(&member);
        std::fs::write(
            member.join("eska.toml"),
            format!("[project]\nname='{name}'\ntype='configuration'\n"),
        )
        .unwrap();
    }
    std::fs::write(directory.0.join("eska.toml"),"[workspace]\n[workspace.members.first]\nconfiguration='first'\n[workspace.members.second]\nconfiguration='second'\n").unwrap();
    let mut client = Client::new("en-US");
    initialize(&mut client, "en-US");
    let workspace=client.ok("workspace/open",json!({"start":{"value":directory.0,"encoding":"utf-8"},"selection":{"kind":"all"},"diskCache":false}));
    let first = key(&workspace, 0);
    let second = key(&workspace, 1);
    let open = with(
        &first,
        json!({"node":module(&workspace,0),"source":{"value":SOURCE,"encoding":"utf-8"},"version":"1","application":"second"}),
    );
    assert_eq!(
        client.request("query/open", open.clone())["error"]["data"]["details"]["reason"],
        "context_member_mismatch"
    );
    client.ok("query/open", with(&open, json!({"application":"first"})));
    let buffer = client.ok(
        "query/bufferBegin",
        with(&first, json!({"bytes":0,"snapshot":snapshot("")})),
    );
    let wrong = with(
        &second,
        json!({"node":module(&workspace,1),"source":{"value":SOURCE,"encoding":"utf-8"},"version":"1","application":"second","bufferId":buffer["bufferId"]}),
    );
    assert_eq!(
        client.request("query/open", wrong)["error"]["data"]["details"]["reason"],
        "unknown_buffer"
    );
    assert_eq!(
        client.request(
            "query/bufferClose",
            with(&second, json!({"bufferId":buffer["bufferId"]}))
        )["error"]["data"]["details"]["reason"],
        "unknown_buffer"
    );
    client.finish();
}
