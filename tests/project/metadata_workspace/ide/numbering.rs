//! API 1.10 advertises related property writes, including a complete multi-file preview.

use super::*;
use std::fs;

/// Declare one numerator and two documents with lexically different but related properties.
fn numbering_fixture(directory: &TestDir) -> Vec<(std::path::PathBuf, String)> {
    fixture(&directory.0, "configuration");
    let source = directory.0.join("src");
    let root = source.join("Configuration.xml");
    fs::write(&root, fs::read_to_string(&root).unwrap().replace("</ChildObjects>", "<DocumentNumerator>Shared</DocumentNumerator><Document>One</Document><Document>Two</Document></ChildObjects>")).unwrap();
    fs::create_dir_all(source.join("Ext")).unwrap();
    fs::write(source.join("Ext/ParentConfigurations.bin"), "{6,0,0,0,0,0}").unwrap();
    let mut files = Vec::new();
    for (tag, folder, name, length) in [
        ("DocumentNumerator", "DocumentNumerators", "Shared", 20),
        ("Document", "Documents", "One", 9),
        ("Document", "Documents", "Two", 9),
    ] {
        let path = source.join(format!("{folder}/{name}.xml"));
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        let extra = if tag == "Document" {
            "<Numerator>DocumentNumerator.Shared</Numerator><Autonumbering>false</Autonumbering><InputByString/>"
        } else {
            ""
        };
        let input = format!(
            "\u{feff}<MetaDataObject xmlns='http://v8.1c.ru/8.3/MDClasses' version='2.20'><{tag} uuid='11111111-1111-1111-1111-111111111111'><Properties><Name>{name}</Name><NumberType>String</NumberType><NumberLength>{length}</NumberLength><NumberAllowedLength>Variable</NumberAllowedLength><NumberPeriodicity>Year</NumberPeriodicity><CheckUnique>false</CheckUnique>{extra}</Properties></{tag}></MetaDataObject>\r\n"
        );
        fs::write(&path, &input).unwrap();
        files.push((path, input));
    }
    files
}

/// Older clients must not present a single-file editor for a field which now updates related descriptors.
#[test]
fn linked_numbering_is_hidden_from_old_clients_and_replayed_by_new_clients() {
    for minor in [8, 9, 10] {
        let directory = TestDir::new();
        let files = numbering_fixture(&directory);
        let mut client = Client::new("en-US");
        let initialized = client.ok("initialize", json!({"apiVersion":{"major":1,"minor":minor},"client":{"name":"numbering","version":"1"},"allowPropertyEdits":true}));
        assert_eq!(
            initialized["capabilities"]["linkedPropertyEdits"],
            minor >= 10
        );
        let mut open = client.open(&directory.0);
        let id = "document-numerator:Shared";
        let state = client.ok(
            "metadata/propertyEditing",
            context(&open, json!({"objectId":id})),
        );
        let fields = state["fields"].as_array().unwrap();
        assert_eq!(
            fields
                .iter()
                .any(|field| field["path"][0]["key"]["name"] == "NumberLength"),
            minor >= 10
        );
        if minor >= 10 {
            assert_eq!(state["linkedObjects"], 2);
            let field = fields
                .iter()
                .find(|field| field["path"][0]["key"]["name"] == "NumberLength")
                .unwrap();
            assert_eq!(field["linked"], true);
            let mut request = context(
                &open,
                json!({"objectId":id,"snapshot":state["snapshot"],"contextSnapshot":state["contextSnapshot"],"path":field["path"],"change":{"kind":"text","value":"30"}}),
            );
            let preview = client.ok("metadata/previewProperty", request.clone());
            assert_eq!(preview["files"].as_array().unwrap().len(), 3);
            let result = client.ok("metadata/updateProperty", request.clone());
            open["projects"][0]["generation"] = result["generation"].clone();
            for (path, input) in &files {
                assert_eq!(
                    fs::read_to_string(path).unwrap(),
                    input
                        .replace("<NumberLength>20", "<NumberLength>30")
                        .replace("<NumberLength>9", "<NumberLength>30")
                );
            }
            let state = &result["editing"];
            assert_eq!(state["undoLinked"], true);
            request = context(
                &open,
                json!({"objectId":id,"snapshot":state["snapshot"],"contextSnapshot":state["contextSnapshot"],"direction":"undo"}),
            );
            client.ok("metadata/undoProperty", request);
            for (path, input) in &files {
                assert_eq!(&fs::read_to_string(path).unwrap(), input);
            }
        }
        client.finish();
    }
}

/// Both locales return the same machine identity and property for a failed dependent document.
#[test]
fn linked_failure_identifies_the_related_document() {
    let directory = TestDir::new();
    let files = numbering_fixture(&directory);
    let (file, input) = &files[1];
    fs::write(file, input.replace("<NumberLength>9", "<NumberLength>50")).unwrap();
    let mut expected = None;
    for locale in ["ru-RU", "en-US"] {
        let mut client = Client::new(locale);
        client.ok("initialize", json!({"apiVersion":{"major":1,"minor":10},"client":{"name":"numbering-error","version":"1"},"allowPropertyEdits":true}));
        let open = client.open(&directory.0);
        let id = "document-numerator:Shared";
        let state = client.ok(
            "metadata/propertyEditing",
            context(&open, json!({"objectId":id})),
        );
        let field = state["fields"]
            .as_array()
            .unwrap()
            .iter()
            .find(|field| field["path"][0]["key"]["name"] == "NumberType")
            .unwrap();
        let result = client.request("metadata/updateProperty", context(&open, json!({"objectId":id,"snapshot":state["snapshot"],"contextSnapshot":state["contextSnapshot"],"path":field["path"],"change":{"kind":"text","value":"Number"}})));
        let error = &result["error"]["data"];
        assert_eq!(error["kind"], "property_dependency");
        assert_eq!(error["details"]["objectId"], "document:One");
        assert_eq!(error["details"]["objectName"], "One");
        assert_eq!(error["details"]["property"]["name"], "NumberLength");
        if let Some(expected) = &expected {
            assert_eq!(error, expected);
        }
        expected = Some(error.clone());
        client.finish();
    }
}
