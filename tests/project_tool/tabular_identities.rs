//! The standalone tool shares native table identities, protected selection and locale-independent JSON.

use super::*;
use eska::project_core::source_format::{
    Child, NativeUuid, ObjectSource, Property, PropertyValue, Section,
};

const TYPE_ID: &str = "40000000-0000-4000-8000-000000000001";
const CHANGED_ID: &str = "40000000-0000-4000-8000-000000000099";

/// Explicit native service declarations belong to an accepted direct table of a catalog.
fn fixture(root: &Path) {
    fs::create_dir_all(root.join("native/Catalogs")).unwrap();
    fs::write(root.join("native/Configuration.xml"), "<MetaDataObject xmlns=\"http://v8.1c.ru/8.3/MDClasses\" version=\"2.20\"><Configuration uuid=\"10000000-0000-4000-8000-000000000001\"><Properties><Name>Sample</Name></Properties><ChildObjects><Catalog>Records</Catalog></ChildObjects></Configuration></MetaDataObject>").unwrap();
    fs::write(root.join("native/Catalogs/Records.xml"), format!("<MetaDataObject xmlns=\"http://v8.1c.ru/8.3/MDClasses\" xmlns:xr=\"http://v8.1c.ru/8.3/xcf/readable\" version=\"2.20\"><Catalog uuid=\"20000000-0000-4000-8000-000000000001\"><Properties><Name>Records</Name></Properties><ChildObjects><TabularSection uuid=\"30000000-0000-4000-8000-000000000001\"><InternalInfo><xr:GeneratedType name=\"CatalogTabularSection.Records.Rows\" category=\"TabularSection\"><xr:TypeId>{TYPE_ID}</xr:TypeId><xr:ValueId>40000000-0000-4000-8000-000000000002</xr:ValueId></xr:GeneratedType><xr:GeneratedType name=\"CatalogTabularSectionRow.Records.Rows\" category=\"TabularSectionRow\"><xr:TypeId>40000000-0000-4000-8000-000000000003</xr:TypeId><xr:ValueId>40000000-0000-4000-8000-000000000004</xr:ValueId></xr:GeneratedType></InternalInfo><Properties><Name>Rows</Name><Synonym/><Comment/></Properties><ChildObjects/></TabularSection></ChildObjects></Catalog></MetaDataObject>")).unwrap();
}

/// Changing a service slot cannot manufacture a new accepted identity or erase an independent editable value.
fn edit(root: &Path) -> String {
    let path = root.join("src/Catalogs/Records/object.yaml");
    let mut source = ObjectSource::from_yaml(&fs::read_to_string(&path).unwrap()).unwrap();
    let Section::Children(children) = &mut source.document.descriptor.object.sections[1] else {
        unreachable!()
    };
    let Child::Inline { object } = &mut children[0] else {
        unreachable!()
    };
    let Section::TabularSectionIdentities(ids) = &mut object.sections[0] else {
        unreachable!()
    };
    ids.table.type_id = NativeUuid::parse(CHANGED_ID).unwrap();
    let Section::Properties(properties) = &mut object.sections[1] else {
        unreachable!()
    };
    let Property::Typed { value, .. } = &mut properties[2] else {
        unreachable!()
    };
    *value = PropertyValue::Text("Independent comment".into());
    let text = format!(
        "# Keep service annotation\r\n{}",
        source.to_yaml().unwrap().replace('\n', "\r\n")
    );
    fs::write(path, &text).unwrap();
    text
}

/// Both envelopes and languages retain accepted UUIDs while recovery selects a permitted comment.
#[test]
fn standalone_prepare_restores_service_ids_without_touching_inputs_in_both_layouts_and_locales() {
    for layout in ["family", "readable"] {
        let test = TestDir::new();
        fixture(&test.0);
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
        let args = ["prepare", "--accepted-base-sha256", pin, "--format", "json"];
        let noop = success(&project, "ru", &args);
        assert_eq!(noop, success(&project, "en", &args));
        let selected = Path::new(noop["data"]["xml"].as_str().unwrap());
        let expected = fs::read(selected.join("Catalogs/Records.xml")).unwrap();
        fs::remove_dir_all(project.join(".eska/generations")).unwrap();
        let manifest = fs::read(project.join("src/source.json")).unwrap();
        let settings = fs::read(project.join("eska.toml")).unwrap();
        let native = fs::read(test.0.join("native/Catalogs/Records.xml")).unwrap();
        let accepted = fs::read(project.join(".eska/accepted/accepted.json")).unwrap();
        let authored = edit(&project);
        let mut refused = Vec::new();
        for locale in ["ru", "en"] {
            let output = tool(&project, locale, &args);
            assert_eq!(output.status.code(), Some(1));
            assert!(!output.stderr.is_empty());
            let report: Value = serde_json::from_slice(&output.stdout).unwrap();
            assert_eq!(report["error"]["code"], "source_changes_excluded");
            assert_eq!(report["error"]["diagnostics"].as_array().unwrap().len(), 1);
            refused.push(report);
        }
        assert_eq!(refused[0], refused[1]);
        assert!(!project.join(".eska/generations").exists());
        let args = [
            "prepare",
            "--accepted-base-sha256",
            pin,
            "--allow-exclusions",
            "--format",
            "json",
        ];
        let recovered = success(&project, "ru", &args);
        assert_eq!(recovered, success(&project, "en", &args));
        let output = fs::read_to_string(
            Path::new(recovered["data"]["xml"].as_str().unwrap()).join("Catalogs/Records.xml"),
        )
        .unwrap();
        assert!(output.contains(TYPE_ID));
        assert!(!output.contains(CHANGED_ID));
        assert_eq!(
            output,
            String::from_utf8(expected)
                .unwrap()
                .replace("<Comment/>", "<Comment>Independent comment</Comment>")
        );
        assert_eq!(
            fs::read_to_string(project.join("src/Catalogs/Records/object.yaml")).unwrap(),
            authored
        );
        assert_eq!(fs::read(project.join("src/source.json")).unwrap(), manifest);
        assert_eq!(fs::read(project.join("eska.toml")).unwrap(), settings);
        assert_eq!(
            fs::read(project.join(".eska/accepted/accepted.json")).unwrap(),
            accepted
        );
        assert_eq!(
            fs::read(test.0.join("native/Catalogs/Records.xml")).unwrap(),
            native
        );
    }
}
