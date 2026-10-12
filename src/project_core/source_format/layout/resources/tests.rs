use super::*;
use crate::{
    project_core::source_format::{
        AcceptedBase, FileOrigin, InputAuthority, LayoutPublication, NativeUuid, SourceReader,
        accept_source, export_designer, files::Inventory, import_designer,
        import_designer_resources, import_designer_separated, layout::migration::publication,
        prepare_accepted_resource_layout, prepare_descriptor_layout, prepare_protected,
        prepare_resource_layout,
    },
    test_support::TestDir,
};
use std::{fs, path::PathBuf};

/// Resource sets include nested `Ext` names, BOM/CRLF, binary bytes and preserved empty directories.
fn fixture(directory: &TestDir) -> PathBuf {
    let native = super::super::tests::native(directory);
    for (path, bytes) in [
        (
            "Ext/ManagedApplicationModule.bsl",
            b"\xef\xbb\xbf// Native module\r\n".as_slice(),
        ),
        (
            "Catalogs/Sample/Forms/Main/Ext/Form/Module.bsl",
            b"\xef\xbb\xbf// Form module\r\n",
        ),
        (
            "Catalogs/Sample/Forms/Main/Ext/Form/Items/Image/Picture.png",
            b"\x00\xffPNG",
        ),
        (
            "Catalogs/Sample/Templates/Main/Ext/Template/index.html",
            b"<img src=\"_files/Ext/Ext/icon.png\">\r\n",
        ),
        (
            "Catalogs/Sample/Templates/Main/Ext/Template/_files/Ext/Ext/icon.png",
            b"\x00\xffimage",
        ),
        ("Catalogs/Sample/Ext/Predefined.xml", b"<Predefined/>\r\n"),
        ("orphan/Ext/opaque.bin", b"\x00\xffunowned"),
    ] {
        let path = native.join(path);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, bytes).unwrap();
    }
    fs::create_dir_all(native.join("Catalogs/Sample/Ext/Help/Empty")).unwrap();
    native
}

/// Only the manifest's exact paths select authoring inputs; path construction supplies no identity.
fn path(value: &str) -> SourcePath {
    SourcePath::parse(value).unwrap()
}

/// New imports preserve every payload byte and resource set while binding modules to native owners.
#[test]
fn resource_import_preserves_bytes_native_output_and_reader_identities() {
    let directory = TestDir::new();
    let native = fixture(&directory);
    let before = Inventory::read(&native).unwrap();
    let source = directory.0.join("resources");
    let manifest = import_designer_resources(&native, &source).unwrap();
    assert_eq!(manifest.schema_version, RESOURCE_LAYOUT_VERSION);
    assert_eq!(manifest.input_revision, before.revision);
    for file in manifest.files.iter().filter(|file| file.is_payload()) {
        assert_eq!(
            fs::read(source.join(file.source_path().as_path())).unwrap(),
            fs::read(native.join(file.path.as_path())).unwrap()
        );
        assert!(!file.source_path().as_str().starts_with("Ext/"));
    }
    assert!(!source.join("Ext").exists());
    assert!(!source.join("Catalogs/Sample/Ext").exists());
    assert!(source.join("Catalogs/Sample/Help/Empty").is_dir());
    assert!(source.join("orphan/Ext/opaque.bin").is_file());
    assert!(
        source
            .join("Catalogs/Sample/Templates/Main/Template/_files/Ext/Ext/icon.png")
            .is_file()
    );
    let reader = SourceReader::open(&source).unwrap();
    let module = path("Catalogs/Sample/Forms/Main/Module.bsl");
    let identity = reader.declared_module(&module).unwrap();
    assert_eq!(
        identity.native_path,
        &path("Catalogs/Sample/Forms/Main/Ext/Form/Module.bsl")
    );
    assert_eq!(
        identity.owner,
        &path("Catalogs/Sample/Forms/Main/form.yaml")
    );
    assert_eq!(
        reader.read_module_text(&module, 1024).unwrap().as_bytes(),
        b"\xef\xbb\xbf// Form module\r\n"
    );
    assert_eq!(
        reader
            .declared_form(&path("Catalogs/Sample/Forms/Main.xml"))
            .unwrap()
            .dialog,
        Some(&path("Catalogs/Sample/Forms/Main/Form.xml"))
    );
    assert_eq!(
        reader.predefined_path(&path("Catalogs/Sample/object.yaml")),
        Some(&path("Catalogs/Sample/Predefined.xml"))
    );
    assert!(
        reader
            .declared_module(&path("orphan/Ext/opaque.bin"))
            .is_none()
    );
    let family = directory.0.join("family");
    import_designer(&native, &family).unwrap();
    let control = directory.0.join("control");
    let output = directory.0.join("output");
    export_designer(&family, &control).unwrap();
    export_designer(&source, &output).unwrap();
    assert_eq!(
        Inventory::read(&control).unwrap(),
        Inventory::read(&output).unwrap()
    );
    assert_eq!(Inventory::read(&native).unwrap(), before);
}

/// Both historical layouts migrate explicitly; separated YAML trivia and old trees stay untouched.
#[test]
fn resource_migration_preserves_yaml_and_is_repeatable_without_downgrading() {
    let directory = TestDir::new();
    let native = fixture(&directory);
    for version in [1, 2] {
        let source = directory.0.join(format!("source-{version}"));
        if version == 1 {
            import_designer(&native, &source).unwrap();
        } else {
            import_designer_separated(&native, &source).unwrap();
        }
        let owner = source.join("Catalogs/Sample/object.yaml");
        let original = format!(
            "# Authored comment\r\n{}",
            fs::read_to_string(&owner).unwrap()
        );
        fs::write(&owner, &original).unwrap();
        let before = Inventory::read(&source).unwrap();
        let plan = prepare_resource_layout(&source).unwrap();
        let destination = directory.0.join(format!("migrated-{version}"));
        assert_eq!(
            plan.publish(&destination).unwrap(),
            LayoutPublication::Created
        );
        assert_eq!(
            plan.publish(&destination).unwrap(),
            LayoutPublication::Unchanged
        );
        assert_eq!(Inventory::read(&source).unwrap(), before);
        if version == 2 {
            assert_eq!(
                fs::read_to_string(destination.join("Catalogs/Sample/object.yaml")).unwrap(),
                original
            );
        }
        let next = prepare_resource_layout(&destination).unwrap();
        assert!(next.is_empty());
        assert_eq!(
            next.publish(&destination).unwrap(),
            LayoutPublication::Unchanged
        );
        assert!(prepare_descriptor_layout(&destination).unwrap().is_empty());
        let first = directory.0.join(format!("first-{version}"));
        let second = directory.0.join(format!("second-{version}"));
        export_designer(&source, &first).unwrap();
        export_designer(&destination, &second).unwrap();
        assert_eq!(
            Inventory::read(&first).unwrap(),
            Inventory::read(&second).unwrap()
        );
    }
}

/// Renamed streamed payloads resume only their own receipt and reject changes before publication.
#[test]
fn resource_migration_recovers_streamed_payloads_and_rechecks_original_bytes() {
    let directory = TestDir::new();
    let source = directory.0.join("source");
    import_designer_separated(&fixture(&directory), &source).unwrap();
    let plan = prepare_resource_layout(&source).unwrap();
    let output = directory.0.join("interrupted");
    publication::interrupt(plan.publication_view(), &output);
    fs::write(output.join(super::super::PENDING_WRITE), b"partial payload").unwrap();
    fs::write(
        output.join("source.json"),
        plan.updated_text(&path("source.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(
        SourceReader::open(&output).err().unwrap().code,
        "source_layout_incomplete"
    );
    assert_eq!(plan.publish(&output).unwrap(), LayoutPublication::Resumed);
    fs::write(
        output.join("ManagedApplicationModule.bsl"),
        b"edited completed output",
    )
    .unwrap();
    assert_eq!(
        plan.publish(&output).unwrap_err().code,
        "source_destination_exists"
    );
    fs::write(
        source.join("Ext/ManagedApplicationModule.bsl"),
        b"changed input",
    )
    .unwrap();
    let fresh = directory.0.join("stale-output");
    assert_eq!(
        plan.publish(&fresh).unwrap_err().code,
        "source_input_changed"
    );
    assert!(!fresh.exists());
}

/// Canonical mappings cannot alias YAML, normalize nested asset names or enter old map versions.
#[test]
fn resource_layout_refuses_collisions_and_forged_or_legacy_mappings() {
    let directory = TestDir::new();
    let native = fixture(&directory);
    let source = directory.0.join("source");
    let manifest = import_designer_resources(&native, &source).unwrap();
    let mut invalid = manifest.clone();
    invalid.schema_version = DESCRIPTOR_LAYOUT_VERSION;
    assert_eq!(
        invalid.validate().unwrap_err().code,
        "source_layout_payload_invalid"
    );
    let mut invalid = manifest;
    invalid
        .files
        .iter_mut()
        .find(|file| file.path.as_str() == "Ext/ManagedApplicationModule.bsl")
        .unwrap()
        .role = FileRole::Relocated {
        source: path("foreign.bsl"),
    };
    assert_eq!(
        invalid.validate().unwrap_err().code,
        "source_layout_payload_invalid"
    );
    fs::write(
        native.join("Catalogs/Sample/Ext/object.yaml"),
        b"opaque collision",
    )
    .unwrap();
    let output = directory.0.join("collision");
    assert_eq!(
        import_designer_resources(&native, &output)
            .unwrap_err()
            .code,
        "source_path_collision"
    );
    assert!(!output.exists());
}

/// Trusted relocation preserves supplier authority/freeze, and changed modules map to source bytes.
#[test]
fn resource_base_migration_preserves_policy_and_source_map_authorities() {
    let directory = TestDir::new();
    let legacy = directory.0.join("legacy");
    import_designer(&fixture(&directory), &legacy).unwrap();
    let frozen = NativeUuid::parse("20000000-0000-4000-8000-000000000001").unwrap();
    let storage = directory.0.join("accepted");
    let reference = accept_source(&legacy, &storage, &[frozen]).unwrap();
    let old = AcceptedBase::open(&storage, &reference).unwrap();
    let before = Inventory::read(&storage).unwrap();
    let plan = prepare_accepted_resource_layout(&old).unwrap();
    let destination = directory.0.join("new-base");
    plan.publish(&destination).unwrap();
    let base = AcceptedBase::open(&destination, plan.accepted_reference()).unwrap();
    assert!(prepare_accepted_resource_layout(&base).unwrap().is_empty());
    assert!(
        prepare_accepted_resource_layout(&base)
            .unwrap()
            .source_changes()
            .is_empty()
    );
    assert_eq!(
        plan.publish(&destination).unwrap(),
        LayoutPublication::Unchanged
    );
    let source = directory.0.join("working");
    base.restore(&source).unwrap();
    assert!(
        base.module_permission(&path("ManagedApplicationModule.bsl"))
            .unwrap()
            .allowed
    );
    assert!(
        !base
            .module_permission(&path("Catalogs/Sample/Forms/Main/Module.bsl"))
            .unwrap()
            .allowed
    );
    fs::write(
        source.join("ManagedApplicationModule.bsl"),
        b"\xef\xbb\xbf// Current module\r\n",
    )
    .unwrap();
    let prepared =
        prepare_protected(&source, &base, &directory.0.join("generations"), true).unwrap();
    let map = prepared.source_map();
    assert_eq!(
        map.schema_version,
        crate::project_core::source_format::RELOCATED_SOURCE_MAP_VERSION
    );
    let module = map.file(&path("Ext/ManagedApplicationModule.bsl")).unwrap();
    assert!(
        matches!(&module.origin, FileOrigin::Payload { authority: InputAuthority::Working, input, module: true, .. } if input.path == path("ManagedApplicationModule.bsl"))
    );
    let bytes = fs::read(
        prepared
            .report()
            .xml
            .join("Ext/ManagedApplicationModule.bsl"),
    )
    .unwrap();
    let location = module.locate(&bytes, 3..bytes.len(), None).unwrap();
    assert_eq!(location.input.path, path("ManagedApplicationModule.bsl"));
    let mut forged_legacy = map.clone();
    forged_legacy.schema_version = crate::project_core::source_format::SOURCE_MAP_VERSION;
    assert_eq!(
        forged_legacy.validate().unwrap_err().code,
        "source_map_origin_invalid"
    );
    fs::write(
        source.join("ParentConfigurations.bin"),
        b"bypassed native rules",
    )
    .unwrap();
    fs::write(
        source.join("Catalogs/Sample/Forms/Main/Module.bsl"),
        b"bypassed frozen form",
    )
    .unwrap();
    let restored =
        prepare_protected(&source, &base, &directory.0.join("generations"), false).unwrap();
    assert_eq!(restored.report().diagnostics.len(), 2);
    assert!(
        matches!(&restored.source_map().file(&path("Catalogs/Sample/Forms/Main/Ext/Form/Module.bsl")).unwrap().origin, FileOrigin::Payload { authority: InputAuthority::Accepted, input, .. } if input.path == path("Catalogs/Sample/Forms/Main/Module.bsl"))
    );
    assert_eq!(Inventory::read(&storage).unwrap(), before);
}
