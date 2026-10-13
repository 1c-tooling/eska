//! CFE import and materialization use the standalone CLI's shared project model.

use super::*;

#[test]
/// Native root detection selects extension settings on both locales, preserving bytes and approval.
fn import_inspect_and_prepare_keep_the_extension_type() {
    let test = TestDir::new();
    native(&test.0);
    let descriptor = test.0.join("native/Configuration.xml");
    let original = fs::read_to_string(&descriptor).unwrap().replace(
        "<Properties>",
        "<Properties><ConfigurationExtensionPurpose>Patch</ConfigurationExtensionPurpose>",
    );
    fs::write(&descriptor, &original).unwrap();
    let mut pins = Vec::new();
    for (locale, destination) in [("ru", "extension-ru"), ("en", "extension-en")] {
        let imported = success(
            &test.0,
            locale,
            &[
                "import",
                "native",
                destination,
                "--name",
                "sample",
                "--layout",
                "readable",
                "--format",
                "json",
            ],
        );
        let pin = imported["data"]["accepted_base_sha256"].as_str().unwrap();
        pins.push(pin.to_owned());
        let root = test.0.join(destination);
        let inspected = success(&root, locale, &["inspect", "--format", "json"]);
        assert_eq!(inspected["data"]["type"], "extension");
        let prepared = success(
            &root,
            locale,
            &["prepare", "--accepted-base-sha256", pin, "--format", "json"],
        );
        let xml = Path::new(prepared["data"]["xml"].as_str().unwrap());
        assert_eq!(
            eska::project_core::native::project_type(
                &fs::read_to_string(xml.join("Configuration.xml")).unwrap()
            )
            .unwrap(),
            Some(eska::project_core::ProjectType::Extension)
        );
        assert_eq!(fs::read(&descriptor).unwrap(), original.as_bytes());
        assert_eq!(
            fs::read(xml.join("Module.bsl")).unwrap(),
            fs::read(test.0.join("native/Module.bsl")).unwrap()
        );
        assert!(
            fs::read_to_string(root.join("eska.toml"))
                .unwrap()
                .contains("type = \"extension\"")
        );
    }
    assert_eq!(pins[0], pins[1]);
}
