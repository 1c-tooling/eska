//! The native service writer stores full u64 seconds; JSON must never round their bounds or values.

use super::*;
use std::fs;

/// Exercise both service classes through version negotiation, validation, exact writes and history.
#[test]
fn service_age_domains_preserve_full_unsigned_precision_and_legacy_schemas() {
    for locale in ["ru-RU", "en-US"] {
        for minor in [10, 11] {
            let directory = TestDir::new();
            fixture(&directory.0, "configuration");
            let source = directory.0.join("src");
            let root = source.join("Configuration.xml");
            fs::write(
                &root,
                fs::read_to_string(&root).unwrap().replace(
                    "</ChildObjects>",
                    "<HTTPService>Age</HTTPService><WebService>Age</WebService></ChildObjects>",
                ),
            )
            .unwrap();
            fs::create_dir_all(source.join("Ext")).unwrap();
            fs::write(source.join("Ext/ParentConfigurations.bin"), "{6,0,0,0,0,0}").unwrap();
            let mut client = Client::new(locale);
            let initialized = client.ok("initialize", json!({"apiVersion":{"major":1,"minor":minor},"client":{"name":"unsigned","version":"1"},"locale":locale,"allowPropertyEdits":true}));
            assert_eq!(
                initialized["capabilities"]["unsignedIntegerProperties"],
                minor >= 11
            );
            for (index, tag) in ["HTTPService", "WebService"].iter().enumerate() {
                let file = source.join(format!("{tag}s/Age.xml"));
                fs::create_dir_all(file.parent().unwrap()).unwrap();
                fs::write(file, format!("\u{feff}<MetaDataObject xmlns='http://v8.1c.ru/8.3/MDClasses'><{tag} uuid='22222222-2222-2222-2222-22222222222{index}'><Properties><Name>Age</Name><SessionMaxAge>20</SessionMaxAge></Properties></{tag}></MetaDataObject>\r\n")).unwrap();
            }
            let mut open = client.open(&directory.0);
            for (tag, id) in [
                ("HTTPService", "http-service:Age"),
                ("WebService", "web-service:Age"),
            ] {
                let file = source.join(format!("{tag}s/Age.xml"));
                let original = fs::read_to_string(&file).unwrap();
                let state = client.ok(
                    "metadata/propertyEditing",
                    context(&open, json!({"objectId":id})),
                );
                let field = state["fields"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .find(|field| field["path"][0]["key"]["name"] == "SessionMaxAge");
                if minor < 11 {
                    assert!(field.is_none());
                    assert!(
                        state["readOnlyProperties"]
                            .as_array()
                            .unwrap()
                            .iter()
                            .any(|item| item["key"]["name"] == "SessionMaxAge"
                                && item["reason"] == "client_version")
                    );
                    continue;
                }
                let field = field.unwrap();
                assert_eq!(
                    field["schema"],
                    json!({"kind":"unsignedInteger","min":"0","max":"18446744073709551615"})
                );
                for value in ["-1", "18446744073709551616", "1.5", "1e6"] {
                    let error = client.request("metadata/updateProperty", context(&open, json!({"objectId":id,"snapshot":state["snapshot"],"path":field["path"],"change":{"kind":"text","value":value}})));
                    assert_eq!(error["error"]["data"]["kind"], "property_invalid");
                    assert_eq!(fs::read_to_string(&file).unwrap(), original);
                }
                for value in ["0", "9007199254740993", "18446744073709551615"] {
                    let result = client.ok("metadata/updateProperty", context(&open, json!({"objectId":id,"snapshot":state["snapshot"],"path":field["path"],"change":{"kind":"text","value":value}})));
                    open["projects"][0]["generation"] = result["generation"].clone();
                    assert_eq!(
                        fs::read_to_string(&file).unwrap(),
                        original.replace("<SessionMaxAge>20", &format!("<SessionMaxAge>{value}"))
                    );
                    assert_eq!(result["editing"]["fields"][0]["value"], value);
                    assert_eq!(result["editing"]["fields"][0]["schema"], field["schema"]);
                    let undone = client.ok("metadata/undoProperty", context(&open, json!({"objectId":id,"snapshot":result["editing"]["snapshot"],"direction":"undo"})));
                    open["projects"][0]["generation"] = undone["generation"].clone();
                    assert_eq!(fs::read_to_string(&file).unwrap(), original);
                }
            }
            client.finish();
        }
    }
}
