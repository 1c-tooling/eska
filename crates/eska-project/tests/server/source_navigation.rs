use super::*;

/// Source navigation is a negotiated read-only method with locale-independent nodes and generation checks.
#[test]
fn source_navigation_protocol_is_lazy_versioned_and_locale_independent() {
    let fixture = support::TestDir::new();
    let root = fixture.0.join("project");
    create::create(&root, &ProjectConfig::new(ProjectType::Configuration)).unwrap();
    let descriptor = root.join("src/Configuration.xml");
    fs::write(&descriptor, "<MetaDataObject xmlns=\"http://v8.1c.ru/8.3/MDClasses\"><Configuration uuid=\"root\"><Properties><Name>Пример</Name></Properties><ChildObjects/></Configuration></MetaDataObject>").unwrap();
    let before = fs::read(&descriptor).unwrap();
    let mut snapshots = Vec::new();
    for minor in [23, 24] {
        for locale in ["ru", "en"] {
            let mut client = Client::new(&root, locale);
            let hello = client.request("initialize", &json!({"apiVersion":{"major":1,"minor":minor},"client":{"name":"source-navigation","version":"1"}}));
            assert_eq!(
                hello["capabilities"]["sourceNavigation"],
                if minor == 24 {
                    json!(true)
                } else {
                    Value::Null
                }
            );
            let opened = client.request("workspace/open", &json!({"start":{"value":root,"encoding":"utf-8"},"selection":{"kind":"current"},"diskCache":false}));
            let project = &opened["projects"][0];
            let context = json!({"sessionId":opened["sessionId"],"projectId":project["projectId"],"generation":project["generation"]});
            let mut params = context.clone();
            params["path"] = json!({"value":"Configuration.xml","encoding":"utf-8"});
            if minor < 24 {
                assert_eq!(
                    client.response("metadata/revealSource", &params)["error"]["code"],
                    -32601
                );
            } else {
                let result = client.request("metadata/revealSource", &params);
                assert_eq!(result["source"]["path"], params["path"]);
                assert_eq!(result["nodes"][0]["id"], project["root"]);
                snapshots.push(json!({"source":result["source"],"nodes":result["nodes"]}));
                params["path"]["value"] = json!("missing.bsl");
                let missing = client.request("metadata/revealSource", &params);
                assert_eq!(missing["source"], Value::Null);
                assert_eq!(missing["nodes"], json!([]));
                params["path"]["value"] = json!("../eska.toml");
                assert!(
                    client
                        .response("metadata/revealSource", &params)
                        .get("error")
                        .is_some()
                );
                params["path"]["encoding"] = json!("unknown");
                assert_eq!(
                    client.response("metadata/revealSource", &params)["error"]["code"],
                    -32602
                );
                let mut status = context.clone();
                status["action"] = json!("status");
                assert_eq!(
                    client.request("metadata/index", &status)["progress"]["state"],
                    "not_started"
                );
                let mut refresh = context.clone();
                refresh["node"] = project["root"].clone();
                client.request("metadata/refresh", &refresh);
                params["path"] = json!({"value":"Configuration.xml","encoding":"utf-8"});
                assert_eq!(
                    client.response("metadata/revealSource", &params)["error"]["data"]["kind"],
                    "stale_generation"
                );
            }
            client.finish();
        }
    }
    assert_eq!(snapshots[0], snapshots[1]);
    assert_eq!(fs::read(descriptor).unwrap(), before);
    assert!(!root.join(".eska").exists());
}
