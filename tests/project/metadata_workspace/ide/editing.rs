//! Protocol negotiation keeps the documented API 1.6 editor schema compatible.

use super::*;
use std::fs;

/// New selector variants appear only after negotiating the supporting protocol minor.
#[test]
fn editor_schema_and_mutation_responses_respect_the_negotiated_minor() {
    let directory = TestDir::new();
    fixture(&directory.0, "configuration");
    let path = directory.0.join("src/Configuration.xml");
    fs::write(
        &path,
        fs::read_to_string(&path).unwrap().replacen(
            "</Properties>",
            "<Comment/><DefaultReportForm/></Properties>",
            1,
        ),
    )
    .unwrap();
    fs::create_dir_all(directory.0.join("src/Ext")).unwrap();
    fs::write(
        directory.0.join("src/Ext/ParentConfigurations.bin"),
        "{6,0,0,0,0,0}",
    )
    .unwrap();
    for minor in [6, 7] {
        let mut client = Client::new("en-US");
        client.ok("initialize",json!({"apiVersion":{"major":1,"minor":minor},"client":{"name":"editing-test","version":"1"},"allowPropertyEdits":true}));
        let open = client.open(&directory.0);
        let id = &open["projects"][0]["root"]["objectId"];
        let editing = client.ok(
            "metadata/propertyEditing",
            context(&open, json!({"objectId":id})),
        );
        let fields = editing["fields"].as_array().unwrap();
        assert_eq!(
            fields
                .iter()
                .any(|field| field["schema"]["kind"] == "reference"),
            minor == 7
        );
        let field = fields
            .iter()
            .find(|field| field["path"][0]["key"]["name"] == "Comment")
            .unwrap();
        let result = client.ok("metadata/updateProperty",context(&open,json!({"objectId":id,"snapshot":editing["snapshot"],"path":field["path"],"change":{"kind":"text","value":format!("minor {minor}")}})));
        assert_eq!(
            result["editing"]["fields"]
                .as_array()
                .unwrap()
                .iter()
                .any(|field| field["schema"]["kind"] == "reference"),
            minor == 7
        );
        client.finish();
    }
}
