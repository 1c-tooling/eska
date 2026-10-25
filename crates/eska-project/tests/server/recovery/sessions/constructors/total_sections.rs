use super::*;

/// Every negotiated client receives the same native grammar guard and revision-bound section permissions.
#[test]
fn total_sections_rpc_enforces_one_native_section_in_both_locales() {
    for locale in ["ru-RU", "en-US"] {
        for minor in [27, 28, 30] {
            let (_directory, root, private) = source_fixture();
            let mut client = Client::new(&root, locale);
            let binding = source_connect_version(&mut client, &root, &private, locale, minor);
            let state = open_source(&mut client, &binding, XML, false);
            let text = "SELECT 1 AS A, 2 AS B FROM Catalog.Items AS I";
            let state = source_draft(&mut client, &binding, &state, text);
            let opened = client.request(
                "query/constructorOpen",
                &source_args(
                    &binding,
                    json!({"querySessionId":state["querySessionId"],"revision":state["revision"]}),
                ),
            );
            let intention = constructor_args(
                &binding,
                &opened,
                json!({"action":{"kind":"addTotals","statement":0,"after":null}}),
            );
            if minor < 28 {
                assert_eq!(
                    client.response("query/constructorAction", &intention)["error"]["code"],
                    -32601
                );
                continue;
            }
            assert_eq!(opened["constructor"]["statements"][0]["addTotals"], true);
            let added = client.request("query/constructorAction", &intention);
            let owner = &added["constructor"]["statements"][0];
            assert_eq!(owner["addTotals"], false);
            assert_eq!(owner["totalSections"].as_array().unwrap().len(), 1);
            assert_eq!(owner["totalSections"][0]["moveUp"], Value::Null);
            assert_eq!(owner["totalSections"][0]["moveDown"], Value::Null);
            let failed = client.response(
                "query/constructorAction",
                &constructor_args(
                    &binding,
                    &added,
                    json!({"action":{"kind":"addTotals","statement":0,"after":0}}),
                ),
            );
            assert_eq!(
                failed["error"]["data"]["details"]["reason"],
                "constructor_unsupported"
            );
            let read = client.request(
                "query/constructorModel",
                &constructor_args(&binding, &added, json!({})),
            );
            assert_eq!(read["constructor"], added["constructor"]);
            let removed = client.request(
                "query/constructorAction",
                &constructor_args(
                    &binding,
                    &added,
                    json!({"action":{"kind":"removeTotals","statement":0,"section":0}}),
                ),
            );
            assert_eq!(
                removed["constructor"]["statements"][0]["totalSections"],
                json!([])
            );
            assert_eq!(source_parts(&mut client, &binding, &removed)[3], text);
            client.request(
                "query/constructorCancel",
                &constructor_args(&binding, &removed, json!({})),
            );
            assert_eq!(
                fs::read_to_string(root.join("src").join(SOURCE)).unwrap(),
                XML
            );
            client.finish();
        }
    }
}

/// Current discovery describes the executable dialect, while older negotiations retain their documented DTO.
#[test]
fn native_total_section_capability_has_an_explicit_single_section_limit() {
    let (_directory, root, _) = source_fixture();
    let mut client = Client::new(&root, "en-US");
    let hello = client.request(
        "initialize",
        &json!({"apiVersion":{"major":1,"minor":30},"client":{"name":"totals-capability","version":"1"}}),
    );
    let capability = &hello["capabilities"]["queryConstructor"];
    assert_eq!(capability["totalSections"], true);
    assert_eq!(capability["maxTotalSections"], 1);
    assert!(capability["multipleTotals"].is_null());
    assert_eq!(capability["dcs"], true);
    client.finish();
}
