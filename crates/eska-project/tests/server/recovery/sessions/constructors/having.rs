use super::*;

/// HAVING has an explicit clause/capability boundary and keeps the source/checkpoint unchanged until finish.
#[test]
fn having_rpc_is_typed_and_fenced_for_older_clients_in_both_locales() {
    for locale in ["ru-RU", "en-US"] {
        for minor in [26, 27] {
            let (_directory, root, private) = source_fixture();
            let mut client = Client::new(&root, locale);
            let binding = source_connect_version(&mut client, &root, &private, locale, minor);
            let state = open_source(&mut client, &binding, XML, false);
            let text = "SELECT SUM(1) AS Total HAVING SUM(1)>0";
            let state = source_draft(&mut client, &binding, &state, text);
            let opened = client.request(
                "query/constructorOpen",
                &source_args(
                    &binding,
                    json!({"querySessionId":state["querySessionId"],"revision":state["revision"]}),
                ),
            );
            let action = constructor_args(
                &binding,
                &opened,
                json!({"action":{"kind":"compose","statement":0,"selection":null,"expression":{"kind":"having","left":"COUNT(*)","comparison":"greater","right":"0"}}}),
            );
            if minor < 27 {
                assert_eq!(
                    opened["constructor"]["statements"][0]["unavailable"],
                    "unsupported_construct"
                );
                assert_eq!(
                    client.response("query/constructorAction", &action)["error"]["code"],
                    -32601
                );
                continue;
            }
            let changed = client.request("query/constructorAction", &action);
            let clauses = changed["constructor"]["statements"][0]["clauses"]
                .as_array()
                .unwrap();
            let clause = clauses.iter().find(|c| c["kind"] == "having").unwrap();
            assert_eq!(clause["items"][0]["visual"]["kind"], "having");
            assert_eq!(clause["items"].as_array().unwrap().len(), 2);
            assert!(
                changed["constructor"]["text"]
                    .as_str()
                    .unwrap()
                    .contains("AND COUNT(*) > 0")
            );
            assert_eq!(source_parts(&mut client, &binding, &changed)[3], text);
            client.request(
                "query/constructorCancel",
                &constructor_args(&binding, &changed, json!({})),
            );
            assert_eq!(
                fs::read_to_string(root.join("src").join(SOURCE)).unwrap(),
                XML
            );
            client.finish();
        }
    }
}
