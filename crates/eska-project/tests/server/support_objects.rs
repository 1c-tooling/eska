use super::*;

/// Object display is a versioned, bounded, locale-independent read without file permissions or writes.
#[test]
fn support_object_protocol_is_bounded_and_negotiated() {
    let fixture = support::TestDir::new();
    let root = fixture.0.join("project");
    create::create(&root, &ProjectConfig::new(ProjectType::Configuration)).unwrap();
    let descriptor = root.join("src/Configuration.xml");
    fs::write(&descriptor, "<MetaDataObject xmlns=\"http://v8.1c.ru/8.3/MDClasses\"><Configuration uuid=\"root\"><Properties><Name>Пример</Name></Properties><ChildObjects/></Configuration></MetaDataObject>").unwrap();
    let before = fs::read(&descriptor).unwrap();
    let mut snapshots = Vec::new();
    for minor in [24, 25] {
        for locale in ["ru", "en"] {
            let mut client = Client::new(&root, locale);
            let hello = client.request("initialize", &json!({"apiVersion":{"major":1,"minor":minor},"client":{"name":"support-objects","version":"1"}}));
            assert_eq!(
                hello["capabilities"]["supportObjects"],
                if minor == 25 {
                    json!(true)
                } else {
                    Value::Null
                }
            );
            let opened = client.request("workspace/open", &json!({"start":{"value":root,"encoding":"utf-8"},"selection":{"kind":"current"},"diskCache":false}));
            let project = &opened["projects"][0];
            let id = &project["root"]["objectId"];
            let mut params = json!({"sessionId":opened["sessionId"],"projectId":project["projectId"],"generation":project["generation"],"objectIds":[id]});
            if minor < 25 {
                assert_eq!(
                    client.response("metadata/supportObjects", &params)["error"]["code"],
                    -32601
                );
            } else {
                let result = client.request("metadata/supportObjects", &params);
                assert_eq!(result["objects"][0]["objectId"], *id);
                assert_eq!(result["objects"][0]["state"], "unknown");
                assert!(result.get("files").is_none());
                assert_eq!(result["freshness"], "current");
                snapshots
                    .push(json!({"objects":result["objects"],"diagnostics":result["diagnostics"]}));
                for invalid in [json!([]), json!(vec![id; 129]), json!([12])] {
                    params["objectIds"] = invalid;
                    assert_eq!(
                        client.response("metadata/supportObjects", &params)["error"]["code"],
                        -32602
                    );
                }
                params["objectIds"] = json!([id]);
                params["generation"] = json!("999999");
                assert_eq!(
                    client.response("metadata/supportObjects", &params)["error"]["data"]["kind"],
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
