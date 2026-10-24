use super::*;

/// Sequential totals need explicit negotiation; indices remain bound to the private constructor revision.
#[test]
fn total_sections_rpc_preserves_pair_ownership_and_legacy_opaque_models() {
    for locale in ["ru-RU", "en-US"] {
        for minor in [27, 28] {
            let (_directory, root, private) = source_fixture();
            let mut client = Client::new(&root, locale);
            let binding = source_connect_version(&mut client, &root, &private, locale, minor);
            let state = open_source(&mut client, &binding, XML, false);
            let text = "SELECT 1 AS A, 2 AS B FROM Catalog.Items AS I TOTALS SUM(A) BY A TOTALS SUM(B) BY B";
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
                json!({"action":{"kind":"addTotals","statement":0,"after":0}}),
            );
            if minor < 28 {
                assert_eq!(
                    opened["constructor"]["statements"][0]["unavailable"],
                    "unsupported_construct"
                );
                assert_eq!(opened["constructor"]["statements"][0]["sources"], json!([]));
                assert_eq!(
                    client.response("query/constructorAction", &intention)["error"]["code"],
                    -32601
                );
                continue;
            }
            assert_eq!(
                opened["constructor"]["statements"][0]["totalSections"]
                    .as_array()
                    .unwrap()
                    .len(),
                2
            );
            let added = client.request("query/constructorAction", &intention);
            assert_eq!(added["constructor"]["totalFocus"], 1);
            let changed = client.request("query/constructorAction", &constructor_args(&binding, &added, json!({"action":{"kind":"composeTotal","statement":0,"section":2,"selection":null,"expression":{"kind":"overall"}}})));
            let owner = &changed["constructor"]["statements"][0];
            for section in owner["totalSections"].as_array().unwrap() {
                let fields = usize::try_from(section["fields"].as_u64().unwrap()).unwrap();
                let groups = usize::try_from(section["groups"].as_u64().unwrap()).unwrap();
                assert_eq!(owner["clauses"][fields]["kind"], "totals");
                assert_eq!(owner["clauses"][groups]["kind"], "totalGroups");
            }
            let removed = client.request(
                "query/constructorAction",
                &constructor_args(
                    &binding,
                    &changed,
                    json!({"action":{"kind":"removeTotals","statement":0,"section":1}}),
                ),
            );
            assert_eq!(
                removed["constructor"]["statements"][0]["totalSections"]
                    .as_array()
                    .unwrap()
                    .len(),
                2
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
