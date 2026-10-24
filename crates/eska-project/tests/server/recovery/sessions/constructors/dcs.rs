use super::*;

/// Typed DCS edits are negotiated, private, revision-bound and identical across interface locales.
#[test]
fn dcs_rpc_negotiates_typed_fragments_and_preserves_the_xml_owner() {
    for locale in ["ru-RU", "en-US"] {
        for minor in [28, 29] {
            let (_directory, root, private) = source_fixture();
            let mut client = Client::new(&root, locale);
            let binding = source_connect_version(&mut client, &root, &private, locale, minor);
            let state = open_source(&mut client, &binding, XML, false);
            let text = "SELECT I.Ref AS Item {SELECT Item.*} FROM Catalog.Items AS I";
            let state = source_draft(&mut client, &binding, &state, text);
            let opened = client.request(
                "query/constructorOpen",
                &source_args(
                    &binding,
                    json!({"querySessionId":state["querySessionId"],"revision":state["revision"]}),
                ),
            );
            let args = constructor_args(
                &binding,
                &opened,
                json!({"action":{"kind":"setDcs","statement":0,"node":null,"target":"where","values":[{"kind":"condition","left":"I.Ref","comparison":"equal","right":"&Item"}],"enabled":true}}),
            );
            if minor < 29 {
                assert!(opened["constructor"]["statements"][0]["dcs"].is_null());
                assert_eq!(
                    client.response("query/constructorAction", &args)["error"]["code"],
                    -32601
                );
            } else {
                let dcs = &opened["constructor"]["statements"][0]["dcs"];
                assert_eq!(dcs["nodes"][0]["kind"], "select");
                assert_eq!(dcs["addWhere"], true);
                let changed = client.request("query/constructorAction", &args);
                let result = changed["constructor"]["text"].as_str().unwrap();
                assert!(result.contains("{WHERE I.Ref = &Item}"));
                assert_eq!(source_parts(&mut client, &binding, &changed)[3], text);
                assert_eq!(
                    client.response("query/constructorAction", &args)["error"]["data"]["details"]["reason"],
                    "stale_constructor"
                );
                let finished = client.request(
                    "query/constructorFinish",
                    &constructor_args(&binding, &changed, json!({"nativeSnapshot":snapshot(text)})),
                );
                assert_eq!(source_parts(&mut client, &binding, &finished)[3], result);
            }
            assert_eq!(
                fs::read_to_string(root.join("src").join(SOURCE)).unwrap(),
                XML
            );
            client.finish();
        }
    }
}
