//! The opt-in source layout stays offline, locale-free and independently pinned through the CLI.

use super::*;
use eska::project_core::source_format::{AcceptedBase, AcceptedRef, ObjectSource, SourceReader};

/// Copy the native fixture only into this test's unique playground directory.
fn copy_tree(input: &Path, output: &Path) {
    fs::create_dir(output).unwrap();
    for entry in fs::read_dir(input).unwrap() {
        let entry = entry.unwrap();
        let destination = output.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            copy_tree(&entry.path(), &destination);
        } else {
            fs::copy(entry.path(), destination).unwrap();
        }
    }
}

/// The selected layout creates separate named YAML and resources without changing native bytes.
#[test]
fn readable_import_inspect_and_prepare_work_in_both_locales() {
    let test = TestDir::new();
    copy_tree(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/eska-format/designer"),
        &test.0.join("native"),
    );
    // The fixture omits the supplier CF; this own-input scenario has no native support rules.
    fs::remove_file(test.0.join("native/Ext/ParentConfigurations.bin")).unwrap();
    let before = fs::read(test.0.join("native/Configuration.xml")).unwrap();
    let module_before = fs::read(test.0.join("native/CommonModules/Tools/Ext/Module.bsl")).unwrap();
    let mut pins = Vec::new();
    for (locale, destination) in [("ru", "readable-ru"), ("en", "readable-en")] {
        let report = success(
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
                "--freeze",
                "20000000-0000-4000-8000-000000000001",
                "--format",
                "json",
            ],
        );
        assert_eq!(report["data"]["source_layout"], "readable");
        let pin = report["data"]["accepted_base_sha256"].as_str().unwrap();
        pins.push(pin.to_owned());
        let root = test.0.join(destination);
        AcceptedBase::open(
            &root.join(".eska/accepted"),
            &AcceptedRef { sha256: pin.into() },
        )
        .unwrap();
        let reader = SourceReader::open(&root.join("src")).unwrap();
        assert_eq!(reader.manifest().schema_version, 3);
        for owner in reader.owner_paths() {
            let snapshot = reader.read_owner(owner).unwrap();
            assert_eq!(snapshot.source.document.descriptor.schema_version, 2);
            assert!(snapshot.source.nested.is_empty());
        }
        let yaml = fs::read_to_string(root.join("src/configuration.yaml")).unwrap();
        let object = ObjectSource::from_yaml(&yaml).unwrap();
        assert_eq!(object.document.descriptor.schema_version, 2);
        assert!(yaml.contains("\"Version\": \"1.2.3.004\""));
        assert_eq!(
            fs::read(root.join("src/CommonModules/Tools/Module.bsl")).unwrap(),
            module_before
        );
        assert!(
            root.join("src/Catalogs/Sample/Forms/Main/form.yaml")
                .is_file()
        );
        assert!(!root.join("src/Ext").exists());
        let inspected = success(&root, locale, &["inspect", "--format", "json"]);
        assert_eq!(inspected["data"]["metadata"]["name"], "SampleConfiguration");
        let prepared = success(
            &root,
            locale,
            &["prepare", "--accepted-base-sha256", pin, "--format", "json"],
        );
        assert_eq!(prepared["data"]["baseline"], pin);
        assert!(
            prepared["data"]["exclusions"]
                .as_array()
                .unwrap()
                .is_empty()
        );
        let xml = Path::new(prepared["data"]["xml"].as_str().unwrap());
        assert_eq!(fs::read(xml.join("Configuration.xml")).unwrap(), before);
        assert_eq!(
            fs::read(xml.join("CommonModules/Tools/Ext/Module.bsl")).unwrap(),
            module_before
        );
        assert!(!root.join(".git").exists());
        assert!(!root.join("build").exists());
    }
    assert_eq!(pins[0], pins[1]);
    assert_eq!(
        fs::read(test.0.join("native/Configuration.xml")).unwrap(),
        before
    );
    assert_eq!(
        fs::read(test.0.join("native/CommonModules/Tools/Ext/Module.bsl")).unwrap(),
        module_before
    );
}

/// Omission remains family layout; an explicit selection has the same bytes and candidate pin.
#[test]
fn default_and_explicit_family_layouts_remain_equivalent() {
    let test = TestDir::new();
    native(&test.0);
    let default_pin = import(&test.0, "default", "sample");
    let explicit = success(
        &test.0,
        "en",
        &[
            "import", "native", "family", "--name", "sample", "--layout", "family", "--format",
            "json",
        ],
    );
    assert_eq!(explicit["data"]["source_layout"], "family");
    assert_eq!(explicit["data"]["accepted_base_sha256"], default_pin);
    for path in [
        "src/source.json",
        "src/configuration.yaml",
        "eska.toml",
        ".gitattributes",
    ] {
        assert_eq!(
            fs::read(test.0.join("default").join(path)).unwrap(),
            fs::read(test.0.join("family").join(path)).unwrap()
        );
    }
    let reader = SourceReader::open(&test.0.join("default/src")).unwrap();
    assert_eq!(reader.manifest().schema_version, 1);
    assert_eq!(
        reader
            .configuration()
            .source
            .document
            .descriptor
            .schema_version,
        1
    );
}

/// Layout help is localized; unknown choices fail parsing before destination ownership is claimed.
#[test]
fn layout_help_and_invalid_selection_work_in_both_locales() {
    let test = TestDir::new();
    let mut errors = Vec::new();
    for (locale, phrase) in [("ru", "Структура исходников"), ("en", "Source layout")]
    {
        let help = tool(&test.0, locale, &["import", "--help"]);
        assert_eq!(help.status.code(), Some(0));
        assert!(help.stderr.is_empty());
        let help_text = String::from_utf8_lossy(&help.stdout);
        assert!(help_text.contains("--layout"));
        assert!(help_text.contains(phrase));
        let output = tool(
            &test.0,
            locale,
            &[
                "import",
                "missing-native",
                "refused",
                "--name",
                "sample",
                "--layout",
                "future",
                "--format",
                "json",
            ],
        );
        assert_eq!(output.status.code(), Some(1));
        let value: Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(value["error"]["code"], "arguments_invalid");
        assert_eq!(value["command"], Value::Null);
        assert!(!output.stderr.is_empty());
        assert!(!test.0.join("refused").exists());
        errors.push(value);
    }
    assert_eq!(errors[0], errors[1]);
}
