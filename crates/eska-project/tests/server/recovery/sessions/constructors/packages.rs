use super::*;

/// Package mutations, visibility and dependency failures require explicit negotiation and preserve the canonical draft until finish.
#[test]
fn package_rpc_is_negotiated_revision_bound_and_independent_of_locale() {
    for locale in ["ru-RU", "en-US"] {
        for minor in [25, 26] {
            let (_directory, root, private) = source_fixture();
            let mut client = Client::new(&root, locale);
            let binding = source_connect_version(&mut client, &root, &private, locale, minor);
            let state = open_source(&mut client, &binding, XML, false);
            let draft =
                "SELECT I.Ref AS A INTO Temp FROM Catalog.Items AS I; SELECT T.A FROM Temp AS T";
            let state = source_draft(&mut client, &binding, &state, draft);
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
                json!({"action":{"kind":"setInto","statement":0,"name":"Renamed"}}),
            );
            if minor < 26 {
                assert_eq!(
                    opened["constructor"]["statements"][0]["unavailable"],
                    "unsupported_construct"
                );
                assert_eq!(opened["constructor"]["statements"][0]["sources"], json!([]));
                assert_eq!(
                    client.response("query/constructorAction", &intention)["error"]["code"],
                    -32601
                );
                assert_eq!(
                    client.response(
                        "query/constructorCatalog",
                        &constructor_args(&binding, &opened, json!({"query":{"atStatement":1}}))
                    )["error"]["code"],
                    -32601
                );
                continue;
            }
            let page = client.request(
                "query/constructorCatalog",
                &constructor_args(&binding, &opened, json!({"query":{"atStatement":1}})),
            );
            assert_eq!(page["catalog"]["temporaryTables"][0]["name"], "Temp");
            let changed = client.request("query/constructorAction", &intention);
            assert_eq!(
                changed["constructor"]["statements"][0]["package"]["destination"],
                "Renamed"
            );
            assert!(
                changed["constructor"]["text"]
                    .as_str()
                    .unwrap()
                    .contains("FROM Renamed AS T")
            );
            let bad = client.response(
                "query/constructorAction",
                &constructor_args(
                    &binding,
                    &changed,
                    json!({"action":{"kind":"moveStatement","statement":0,"to":1}}),
                ),
            );
            assert_eq!(
                bad["error"]["data"]["details"]["reason"],
                "constructor_invalid_expression"
            );
            let read = client.request(
                "query/constructorModel",
                &constructor_args(&binding, &changed, json!({})),
            );
            assert_eq!(read["constructor"], changed["constructor"]);
            let added = client.request("query/constructorAction", &constructor_args(&binding, &changed, json!({"action":{"kind":"addStatement","statement":1,"operation":"destroy","name":"Renamed"}})));
            assert_eq!(added["constructor"]["focus"], 2);
            assert_eq!(source_parts(&mut client, &binding, &added)[3], draft);
            client.request(
                "query/constructorFinish",
                &constructor_args(&binding, &added, json!({"nativeSnapshot":snapshot(draft)})),
            );
            assert_eq!(
                fs::read_to_string(root.join("src").join(SOURCE)).unwrap(),
                XML
            );
            client.finish();
        }
    }
}
