//! Clearing a bound value uses the same offline preparation contract in either layout or locale.

use super::*;
use eska::project_core::source_format::{
    AcceptedBase, AcceptedRef, NativeUuid, PropertyName, PropertyValue, prepare_property_edit,
};

/// Actual processes clear the authoritative list without requiring edits to readable native details.
#[test]
fn prepare_clears_bound_list_identically_in_both_locales() {
    for layout in ["family", "readable"] {
        let test = TestDir::new();
        native(&test.0);
        let input = test.0.join("native/Configuration.xml");
        let xml = fs::read_to_string(&input).unwrap()
            .replace("version=\"2.20\"", "xmlns:xr=\"http://v8.1c.ru/8.3/xcf/readable\" xmlns:xsi=\"http://www.w3.org/2001/XMLSchema-instance\" version=\"2.20\"")
            .replace("</Properties>", "<DefaultRoles><xr:Item xsi:type=\"xr:MDObjectRef\">Role.Full</xr:Item></DefaultRoles></Properties><ChildObjects><Role>Full</Role></ChildObjects>");
        fs::write(&input, xml).unwrap();
        fs::create_dir(test.0.join("native/Roles")).unwrap();
        fs::write(test.0.join("native/Roles/Full.xml"), "<MetaDataObject xmlns=\"http://v8.1c.ru/8.3/MDClasses\" version=\"2.20\"><Role uuid=\"20000000-0000-4000-8000-000000000001\"><Properties><Name>Full</Name></Properties></Role></MetaDataObject>").unwrap();
        let imported = success(
            &test.0,
            "en",
            &[
                "import", "native", "project", "--name", "sample", "--layout", layout, "--format",
                "json",
            ],
        );
        let pin = imported["data"]["accepted_base_sha256"].as_str().unwrap();
        let project = test.0.join("project");
        let source = project.join("src/configuration.yaml");
        let manifest = fs::read(project.join("src/source.json")).unwrap();
        let accepted = AcceptedBase::open(
            &project.join(".eska/accepted"),
            &AcceptedRef { sha256: pin.into() },
        )
        .unwrap();
        prepare_property_edit(
            &project.join("src"),
            &accepted,
            &NativeUuid::parse("10000000-0000-4000-8000-000000000001").unwrap(),
            PropertyName::DefaultRoles,
            PropertyValue::Empty,
        )
        .unwrap()
        .publish(&accepted)
        .unwrap();
        let authored = fs::read(&source).unwrap();
        let args = ["prepare", "--accepted-base-sha256", pin, "--format", "json"];
        let prepared = success(&project, "ru", &args);
        assert_eq!(prepared, success(&project, "en", &args));
        assert_eq!(prepared["data"]["exclusions"], serde_json::json!([]));
        let xml = fs::read_to_string(
            Path::new(prepared["data"]["xml"].as_str().unwrap()).join("Configuration.xml"),
        )
        .unwrap();
        assert!(xml.contains("<DefaultRoles/>"));
        assert_eq!(fs::read(source).unwrap(), authored);
        assert_eq!(fs::read(project.join("src/source.json")).unwrap(), manifest);
    }
}
