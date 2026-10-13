//! Opt-in preparation of independent native controls for the installed-platform acceptance.

use std::{
    fs,
    path::{Path, PathBuf},
};

use eska::project::{
    source_format::{
        AcceptedBase, AcceptedRef, Descriptor, NativeUuid, ObjectSource, Operation,
        PermissionReason, Property, PropertyName, PropertyValue, Section, export_protected,
        prepare_property_edit,
    },
    support::Reason,
};

/// Preserve the observed property vocabulary while constructing the independent XML control.
fn comment(descriptor: &mut Descriptor, text: &str) {
    for section in &mut descriptor.object.sections {
        if let Section::Properties(items) = section {
            for item in items {
                if let Property::Typed {
                    name: PropertyName::Comment,
                    value,
                } = item
                {
                    *value = PropertyValue::Text(text.into());
                    return;
                }
            }
        }
    }
    panic!("selected native object must have Comment");
}

/// Prefer a local module, while retaining a genuinely native restricted counterpart.
fn select(root: &Path, base: &AcceptedBase) -> ((PathBuf, Descriptor), (PathBuf, Descriptor)) {
    let mut modules = fs::read_dir(root.join("CommonModules"))
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "xml"))
        .collect::<Vec<_>>();
    modules.sort();
    let mut allowed = Vec::new();
    let mut restricted = None;
    for path in modules {
        let descriptor = Descriptor::from_xml(&fs::read_to_string(&path).unwrap()).unwrap();
        if descriptor.object.property(PropertyName::Comment).is_none() {
            continue;
        }
        let permission = base.permission(
            &descriptor.object.uuid,
            Operation::Property(PropertyName::Comment),
        );
        let module = path.with_extension("").join("Ext/Module.bsl");
        if permission.allowed && module.is_file() {
            allowed.push((
                permission.native_reason != Reason::OwnObject,
                path,
                descriptor,
            ));
        } else if permission.reason == PermissionReason::NativeRestriction && restricted.is_none() {
            restricted = Some((path, descriptor));
        }
    }
    allowed.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.cmp(&b.1)));
    let (_, path, descriptor) = allowed.into_iter().next().expect("real editable module");
    (
        (path, descriptor),
        restricted.expect("real vendor-restricted module"),
    )
}

/// Find the authoritative owner path from the complete imported native file map.
fn owner(source: &Path, native: &Path) -> PathBuf {
    let manifest: eska::project::source_format::SourceManifest =
        serde_json::from_str(&fs::read_to_string(source.join("source.json")).unwrap()).unwrap();
    let entry = manifest
        .files
        .iter()
        .find(|entry| entry.path.as_path() == native)
        .unwrap();
    match &entry.role {
        eska::project::source_format::FileRole::OptionalModule { .. } => {
            panic!("configuration input cannot declare an external module slot")
        }
        eska::project::source_format::FileRole::Descriptor { source, .. } => {
            source.as_path().into()
        }
        eska::project::source_format::FileRole::Preserved
        | eska::project::source_format::FileRole::Relocated { .. }
        | eska::project::source_format::FileRole::ManagedForm { .. } => {
            panic!("selected file must be a native descriptor")
        }
    }
}

/// A raw edit simulates bypassing the editor; the protected exporter must exclude it.
fn bypass(source: &Path, native: &Path, uuid: &NativeUuid) {
    let path = source.join(owner(source, native));
    let mut object = ObjectSource::from_yaml(&fs::read_to_string(&path).unwrap()).unwrap();
    assert_eq!(object.document.descriptor.object.uuid, *uuid);
    comment(
        &mut object.document.descriptor,
        "ESKA forbidden edit acceptance",
    );
    fs::write(path, object.to_yaml().unwrap()).unwrap();
}

/// Require an explicit independent pin and exclusively claim a new playground stage.
fn acceptance_paths() -> (PathBuf, PathBuf, AcceptedBase) {
    let root = PathBuf::from(std::env::var_os("ESKA_TEST_ROOT").unwrap())
        .canonicalize()
        .unwrap();
    let input = PathBuf::from(std::env::var_os("ESKA_FORMAT_ORIGINAL").unwrap())
        .canonicalize()
        .unwrap();
    let base_path = PathBuf::from(std::env::var_os("ESKA_FORMAT_BASE").unwrap())
        .canonicalize()
        .unwrap();
    let output = PathBuf::from(std::env::var_os("ESKA_FORMAT_OUTPUT").unwrap());
    assert!(input.starts_with(&root) && base_path.starts_with(&root));
    assert!(
        output
            .parent()
            .unwrap()
            .canonicalize()
            .unwrap()
            .starts_with(&root)
    );
    assert!(!output.exists());
    fs::create_dir(&output).unwrap();
    let base = AcceptedBase::open(
        &base_path,
        &AcceptedRef {
            sha256: std::env::var("ESKA_FORMAT_REFERENCE").unwrap(),
        },
    )
    .unwrap();
    (input, output, base)
}

#[test]
#[ignore = "Real C1 edit preparation; requires explicit playground input, independent base pin and new output"]
fn real_playground_edit_acceptance() {
    let (input, output, base) = acceptance_paths();
    let source = output.join("source");
    base.restore(&source).unwrap();
    println!("accepted source restored");
    let ((allowed_path, mut allowed), (restricted_path, restricted)) = select(&input, &base);
    let allowed_native = allowed_path.strip_prefix(&input).unwrap();
    let restricted_native = restricted_path.strip_prefix(&input).unwrap();
    let text = "ESKA property acceptance";
    let plan = prepare_property_edit(
        &source,
        &base,
        &allowed.object.uuid,
        PropertyName::Comment,
        PropertyValue::Text(text.into()),
    )
    .unwrap();
    plan.publish(&base).unwrap();
    println!("accepted property edit published");
    assert!(
        prepare_property_edit(
            &source,
            &base,
            &restricted.object.uuid,
            PropertyName::Comment,
            PropertyValue::Text("forbidden".into())
        )
        .is_err()
    );
    let module = allowed_native.with_extension("").join("Ext/Module.bsl");
    let original_module = fs::read(input.join(&module)).unwrap();
    let mut changed_module = original_module.clone();
    let eol = if original_module.windows(2).any(|pair| pair == b"\r\n") {
        "\r\n"
    } else {
        "\n"
    };
    changed_module.extend_from_slice(format!("{eol}// ESKA module acceptance{eol}").as_bytes());
    fs::write(source.join(&module), &changed_module).unwrap();
    bypass(&source, restricted_native, &restricted.object.uuid);
    let source_before = super::baseline::tree(&source);
    let store = output.join("generations");
    let strict = export_protected(&source, &base, &store, true).unwrap_err();
    assert_eq!(strict.error.code, "source_changes_excluded");
    assert!(!store.exists());
    println!("strict export refused the native restricted edit");
    let report = export_protected(&source, &base, &store, false).unwrap();
    assert!(!report.diagnostics.is_empty());
    assert!(
        report
            .diagnostics
            .iter()
            .all(|entry| entry.uuid.as_ref() == Some(&restricted.object.uuid))
    );
    assert_eq!(super::baseline::tree(&source), source_before);
    let actual =
        Descriptor::from_xml(&fs::read_to_string(report.xml.join(allowed_native)).unwrap())
            .unwrap();
    assert_eq!(
        actual.object.property(PropertyName::Comment),
        Some(&PropertyValue::Text(text.into()))
    );
    let actual =
        Descriptor::from_xml(&fs::read_to_string(report.xml.join(restricted_native)).unwrap())
            .unwrap();
    assert_eq!(
        actual.object.property(PropertyName::Comment),
        restricted.object.property(PropertyName::Comment)
    );
    assert_eq!(fs::read(report.xml.join(&module)).unwrap(), changed_module);
    let control = output.join("control");
    fs::create_dir(&control).unwrap();
    super::copy_tree(&input, &control.join("src"));
    comment(&mut allowed, text);
    fs::write(
        control.join("src").join(allowed_native),
        allowed.to_xml().unwrap(),
    )
    .unwrap();
    fs::write(control.join("src").join(&module), changed_module).unwrap();
    let converted = output.join("converted");
    fs::create_dir(&converted).unwrap();
    super::copy_tree(&report.xml, &converted.join("src"));
    let details = serde_json::json!({"schema_version":1,"baseline":std::env::var("ESKA_FORMAT_REFERENCE").unwrap(),
        "generation":report.generation,"xml":report.xml,"control":control,"converted":converted,
        "property_file":allowed_native,"property_uuid":allowed.object.uuid,"module":module,
        "restricted_file":restricted_native,"restricted_uuid":restricted.object.uuid,
        "exclusions":report.diagnostics});
    fs::write(
        output.join("preparation.json"),
        serde_json::to_vec_pretty(&details).unwrap(),
    )
    .unwrap();
    println!("{details}");
}
