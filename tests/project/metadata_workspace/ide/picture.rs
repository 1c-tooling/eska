use std::fs;

use base64::{Engine as _, engine::general_purpose::STANDARD};

use super::{Client, TestDir, Value, context, fixture, json};

/// Expand only common pictures using returned identities, independent of display language.
fn picture_id(client: &mut Client, open: &Value) -> Value {
    let children = client.ok(
        "metadata/children",
        context(open, json!({"node":open["projects"][0]["root"]})),
    );
    let common = children["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|node| node["id"]["collection"]["kind"] == "common")
        .unwrap();
    let children = client.ok(
        "metadata/children",
        context(open, json!({"node":common["id"]})),
    );
    let pictures = children["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|node| node["id"]["collection"]["metadataKind"] == "common-picture")
        .unwrap();
    let children = client.ok(
        "metadata/children",
        context(open, json!({"node":pictures["id"]})),
    );
    children["nodes"][0]["id"]["objectId"].clone()
}

/// Picture transport is optional, locale independent and resilient to payload failures.
#[test]
fn process_picture_properties_preserve_bytes_and_recover_on_refresh() {
    let directory = TestDir::new();
    fixture(&directory.0, "configuration");
    let root = directory.0.join("src/Configuration.xml");
    fs::write(
        &root,
        fs::read_to_string(&root).unwrap().replace(
            "<ChildObjects>",
            "<ChildObjects><CommonPicture>Icon</CommonPicture>",
        ),
    )
    .unwrap();
    let ext = directory.0.join("src/CommonPictures/Icon/Ext");
    fs::create_dir_all(ext.join("Picture")).unwrap();
    fs::write(directory.0.join("src/CommonPictures/Icon.xml"), "<MetaDataObject xmlns=\"http://v8.1c.ru/8.3/MDClasses\"><CommonPicture uuid=\"11111111-1111-1111-1111-111111111111\"><Properties><Name>Icon</Name></Properties></CommonPicture></MetaDataObject>").unwrap();
    fs::write(ext.join("Picture.xml"), "<ExtPicture xmlns=\"http://v8.1c.ru/8.3/xcf/extrnprops\" xmlns:xr=\"http://v8.1c.ru/8.3/xcf/readable\"><Picture><xr:Abs>Picture.svg</xr:Abs></Picture></ExtPicture>").unwrap();
    let image = b"<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"32\" height=\"16\"/>";
    let mut expected = None;
    for locale in ["ru-RU", "en-US"] {
        fs::write(ext.join("Picture/Picture.svg"), image).unwrap();
        let mut client = Client::new(locale);
        let parameters = json!({"apiVersion":{"major":1,"minor":9},"client":{"name":"pictures","version":"1"},"locale":locale});
        let rejected = client.request("initialize", parameters.clone());
        assert_eq!(rejected["error"]["data"]["kind"], "unsupported_version");
        assert_eq!(
            rejected["error"]["data"]["details"]["supported"]["minor"],
            8
        );
        let mut parameters = parameters;
        parameters["apiVersion"]["minor"] = json!(4);
        let handshake = client.ok("initialize", parameters);
        assert_eq!(handshake["capabilities"]["picturePreview"], true);
        let open = client.open(&directory.0);
        let id = picture_id(&mut client, &open);
        let args = context(&open, json!({"objectId":id}));
        let result = client.ok("metadata/properties", args.clone());
        assert_eq!(result["picture"]["status"], "ready");
        assert_eq!(result["picture"]["mimeType"], "image/svg+xml");
        assert_eq!(
            STANDARD
                .decode(result["picture"]["data"].as_str().unwrap())
                .unwrap(),
            image
        );
        if let Some(expected) = &expected {
            assert_eq!(&result["picture"], expected);
        }
        expected = Some(result["picture"].clone());
        fs::remove_file(ext.join("Picture/Picture.svg")).unwrap();
        let missing = client.ok("metadata/properties", args.clone());
        assert_eq!(missing["picture"], json!({"status":"missing"}));
        assert_eq!(missing["properties"], result["properties"]);
        fs::write(ext.join("Picture/Picture.svg"), image).unwrap();
        assert_eq!(
            client.ok("metadata/properties", args)["picture"],
            result["picture"]
        );
        let root = client.ok(
            "metadata/properties",
            context(
                &open,
                json!({"objectId":open["projects"][0]["root"]["objectId"]}),
            ),
        );
        assert!(root.get("picture").is_none());
        client.finish();
    }
}
