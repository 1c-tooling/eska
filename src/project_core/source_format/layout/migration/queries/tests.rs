//! Owned query migrations prove native fidelity before testing project and workspace orchestration.

mod projects;

use super::*;
use crate::{
    project_core::source_format::{
        AcceptedBase, FileRole, LayoutPublication, ManagedFormSource, NativeQueryDocument,
        NativeUuid, ObjectSource, Operation, SourceReader, accept_source, export_designer,
        import_designer, import_designer_forms, import_designer_readable,
        import_designer_resources, import_designer_separated, prepare_accepted_query_layout,
        prepare_descriptor_layout, prepare_form_layout, prepare_protected, prepare_query_layout,
        prepare_readable_layout, prepare_resource_layout, queries::test_support::*, yaml,
    },
    test_support::TestDir,
};
use std::{
    fs,
    path::{Path, PathBuf},
};

/// Every legacy import derives paths from its actual map rather than assuming resource placement.
fn legacy(test: &TestDir, kind: u32, frozen: &[NativeUuid]) -> (PathBuf, AcceptedBase) {
    let native = fixture(test);
    fs::write(
        native.join(DIALOG),
        form().replace("</Form>", "<ReadOnly>false</ReadOnly></Form>"),
    )
    .unwrap();
    let source = test.0.join("source");
    let import = match kind {
        1 => import_designer,
        2 => import_designer_separated,
        3 => import_designer_resources,
        4 => import_designer_readable,
        _ => import_designer_forms,
    };
    import(&native, &source).unwrap();
    let pin = accept_source(&source, &test.0.join("accepted"), frozen).unwrap();
    (
        source,
        AcceptedBase::open(&test.0.join("accepted"), &pin).unwrap(),
    )
}

/// Modify old inline values through their actual native slots, before plain-file migration exists.
fn edit_inline(source: &Path, native: &str, marker: &str) {
    let reader = SourceReader::open(source).unwrap();
    let file = reader
        .manifest()
        .files
        .iter()
        .find(|file| file.path == path(native))
        .unwrap();
    let owner = source.join(file.source_path().as_path());
    let mut object = if matches!(file.role, FileRole::ManagedForm { .. }) {
        Some(ObjectSource::from_yaml(&fs::read_to_string(&owner).unwrap()).unwrap())
    } else {
        None
    };
    let xml = object.as_ref().map_or_else(
        || fs::read_to_string(&owner).unwrap(),
        |object| object.form.as_ref().unwrap().to_xml().unwrap(),
    );
    let document = NativeQueryDocument::from_xml(&xml).unwrap();
    let values = document
        .queries()
        .iter()
        .map(|query| {
            (
                query.slot.clone(),
                if query.text.is_empty() {
                    String::new()
                } else {
                    format!("{}\n// {marker}", query.text)
                },
            )
        })
        .collect();
    let updated = document.materialize(&values).unwrap();
    if let Some(object) = &mut object {
        object.form = Some(ManagedFormSource::from_xml(&updated).unwrap());
        fs::write(&owner, object.to_yaml().unwrap()).unwrap();
    } else {
        fs::write(&owner, updated).unwrap();
    }
}

/// Pinned values never acquire current query edits, while protected native output keeps the old decision.
#[test]
fn working_queries_and_pinned_values_migrate_independently_with_the_same_freeze_and_native_output()
{
    for kind in [1, 3, 5] {
        let test = TestDir::new();
        let frozen = NativeUuid::parse("20000000-0000-4000-8000-000000000001").unwrap();
        let (source, base) = legacy(&test, kind, std::slice::from_ref(&frozen));
        edit_inline(&source, DIALOG, "working list");
        edit_inline(&source, TEMPLATE, "working schema");
        let before = files::Inventory::read(&source).unwrap();
        let pinned_before = files::Inventory::read(&test.0.join("accepted")).unwrap();
        let old_output =
            prepare_protected(&source, &base, &test.0.join("old-generations"), false).unwrap();
        let plan = prepare_query_layout(&source).unwrap();
        let accepted = prepare_accepted_query_layout(&base).unwrap();
        assert_eq!(accepted.previous_reference(), base.reference());
        assert_ne!(accepted.accepted_reference(), base.reference());
        let destination = test.0.join("new-source");
        plan.publish(&destination).unwrap();
        let new_base = test.0.join("new-base");
        accepted.publish(&new_base).unwrap();
        assert!(AcceptedBase::open(&new_base, base.reference()).is_err());
        let migrated = AcceptedBase::open(&new_base, accepted.accepted_reference()).unwrap();
        for id in [
            "70000000-0000-4000-8000-000000000001",
            "80000000-0000-4000-8000-000000000001",
        ] {
            let id = NativeUuid::parse(id).unwrap();
            let mut expected =
                serde_json::to_value(base.permission(&id, Operation::Resource)).unwrap();
            expected["baseline"] = migrated.reference().sha256.clone().into();
            assert_eq!(
                serde_json::to_value(migrated.permission(&id, Operation::Resource)).unwrap(),
                expected
            );
        }
        let restored = test.0.join("restored");
        migrated.restore(&restored).unwrap();
        let pinned_reader = SourceReader::open(&restored).unwrap();
        let current_reader = SourceReader::open(&destination).unwrap();
        for name in pinned_reader.query_paths() {
            assert!(
                !pinned_reader
                    .read_query_text(name, u64::MAX)
                    .unwrap()
                    .contains("working")
            );
            let pinned = pinned_reader.read_query_text(name, u64::MAX).unwrap();
            let current = current_reader.read_query_text(name, u64::MAX).unwrap();
            if pinned.is_empty() {
                assert_eq!(current, pinned);
            } else {
                assert!(current.contains("working"));
            }
        }
        let new_output = prepare_protected(
            &destination,
            &migrated,
            &test.0.join("new-generations"),
            false,
        )
        .unwrap();
        assert_eq!(
            files::Inventory::read(old_output.xml()).unwrap(),
            files::Inventory::read(new_output.xml()).unwrap()
        );
        let no_op = prepare_accepted_query_layout(&migrated).unwrap();
        assert!(no_op.is_empty());
        assert_eq!(no_op.accepted_reference(), migrated.reference());
        assert_eq!(files::Inventory::read(&source).unwrap(), before);
        assert_eq!(
            files::Inventory::read(&test.0.join("accepted")).unwrap(),
            pinned_before
        );
    }
}

/// Mask only opaque root XML values so every unrelated byte remains directly comparable.
fn mask_query_stencil(input: &str) -> String {
    let (value, tree) = yaml::decode_with_tree(input).unwrap();
    let parts = value["form"]["native"]["root"].as_array().unwrap();
    let mut ranges = parts
        .iter()
        .enumerate()
        .filter(|(_, part)| part["kind"] == "xml")
        .map(|(index, _)| {
            yaml::range_at(
                &tree,
                &[
                    yaml::Step::Key("form"),
                    yaml::Step::Key("native"),
                    yaml::Step::Key("root"),
                    yaml::Step::Index(index),
                    yaml::Step::Key("value"),
                ],
            )
            .unwrap()
        })
        .collect::<Vec<_>>();
    ranges.sort_by_key(|range| std::cmp::Reverse(range.start));
    let mut text = input.to_owned();
    for range in ranges {
        text.replace_range(range, "\"<opaque XML>\"");
    }
    text
}

/// Existing map 4 body fields, flow/block syntax, comments and uppercase booleans survive extraction.
#[test]
fn query_migration_splices_only_native_root_values_and_keeps_form_annotations_and_properties() {
    for flow in [false, true] {
        let test = TestDir::new();
        let (source, _) = legacy(&test, 5, &[]);
        let file = source.join(FORM);
        let input = fs::read_to_string(&file).unwrap();
        let input = if flow {
            format!(
                "# flow owner\n{}\n",
                serde_json::to_string(&yaml::decode(&input).unwrap()).unwrap()
            )
        } else {
            format!("# body owner\n{input}").replace(
                "\"ReadOnly\": false",
                "\"ReadOnly\": FALSE # preserve field note",
            )
        };
        fs::write(&file, &input).unwrap();
        let before = ObjectSource::from_yaml(&input).unwrap();
        let plan = prepare_query_layout(&source).unwrap();
        let updated = plan.updated_text(&path(FORM)).unwrap();
        let after = ObjectSource::from_yaml(updated).unwrap();
        assert_eq!(before.document, after.document);
        assert_eq!(before.nested, after.nested);
        let old = before.form.unwrap();
        let new = after.form.unwrap();
        assert_eq!(old.properties, new.properties);
        assert_eq!(old.elements, new.elements);
        assert_eq!(old.layout, new.layout);
        assert_eq!(mask_query_stencil(&input), mask_query_stencil(updated));
    }
}

/// Source/base receipts are exclusive and resumable for the new plain files and changed blobs too.
#[test]
fn query_layout_publication_resumes_and_refuses_conflicting_output_or_stale_inputs() {
    let test = TestDir::new();
    let (source, base) = legacy(&test, 3, &[]);
    let plan = prepare_query_layout(&source).unwrap();
    let accepted = prepare_accepted_query_layout(&base).unwrap();
    let target = test.0.join("new-source");
    let new_base = test.0.join("new-base");
    super::super::publication::interrupt(plan.publication_view(), &target);
    super::super::publication::interrupt(accepted.publication_view(), &new_base);
    assert_eq!(plan.publish(&target).unwrap(), LayoutPublication::Resumed);
    assert_eq!(
        accepted.publish(&new_base).unwrap(),
        LayoutPublication::Resumed
    );
    fs::write(target.join(QUERY), "foreign value").unwrap();
    assert!(plan.publish(&target).is_err());
    edit_inline(&source, TEMPLATE, "concurrent edit");
    let absent = test.0.join("absent");
    assert_eq!(
        plan.publish(&absent).unwrap_err().code,
        "source_input_changed"
    );
    assert!(!absent.exists());
}

/// Opaque resources stay opaque, empty composition schemas are declared, and invalid promotion refuses.
#[test]
fn query_migration_covers_empty_schemas_and_refuses_invalid_resources_or_existing_plain_paths() {
    let test = TestDir::new();
    let native = fixture(&test);
    fs::write(
        native.join(TEMPLATE),
        format!(
            "<DataCompositionSchema xmlns=\"{DCS}\"><settingsVariant/></DataCompositionSchema>"
        ),
    )
    .unwrap();
    let source = test.0.join("source");
    import_designer_forms(&native, &source).unwrap();
    let plan = prepare_query_layout(&source).unwrap();
    assert_eq!(plan.manifest().query_resources.len(), 2);
    assert!(
        plan.manifest()
            .query_resources
            .iter()
            .find(|resource| resource.path == path(TEMPLATE))
            .unwrap()
            .sources
            .is_empty()
    );
    let destination = test.0.join("queries");
    plan.publish(&destination).unwrap();
    let old_output = test.0.join("old-xml");
    let new_output = test.0.join("new-xml");
    export_designer(&source, &old_output).unwrap();
    export_designer(&destination, &new_output).unwrap();
    assert_eq!(
        files::Inventory::read(&old_output).unwrap(),
        files::Inventory::read(&new_output).unwrap()
    );
    fs::write(
        source.join(RESOURCE),
        "<DataCompositionSchema xmlns=\"wrong\"/>",
    )
    .unwrap();
    let before = files::Inventory::read(&source).unwrap();
    assert!(prepare_query_layout(&source).is_err());
    assert_eq!(files::Inventory::read(&source).unwrap(), before);
    let test = TestDir::new();
    let native = fixture(&test);
    fs::create_dir_all(native.join(QUERY).parent().unwrap()).unwrap();
    fs::write(native.join(QUERY), "unrelated native payload").unwrap();
    let source = test.0.join("source");
    import_designer_forms(&native, &source).unwrap();
    let before = files::Inventory::read(&source).unwrap();
    assert_eq!(
        prepare_query_layout(&source).unwrap_err().code,
        "source_path_collision"
    );
    assert_eq!(files::Inventory::read(&source).unwrap(), before);
}

/// Filling an empty query after migration changes only plain text, retaining the pinned empty-node recipe.
#[test]
fn migrated_empty_query_can_be_filled_without_rewriting_bindings_or_triggering_strict_exclusions() {
    use crate::project_core::source_format::{FormId, QuerySlot, prepare_query_edit};
    let test = TestDir::new();
    let (source, base) = legacy(&test, 5, &[]);
    let plan = prepare_query_layout(&source).unwrap();
    let accepted = prepare_accepted_query_layout(&base).unwrap();
    let destination = test.0.join("new-source");
    let new_base = test.0.join("new-base");
    plan.publish(&destination).unwrap();
    accepted.publish(&new_base).unwrap();
    let base = AcceptedBase::open(&new_base, accepted.accepted_reference()).unwrap();
    let empty = path("Catalogs/Sample/Forms/Main/Queries/Attribute_6.sdbl");
    assert_eq!(
        fs::read_to_string(destination.join(empty.as_path())).unwrap(),
        ""
    );
    let owner = fs::read(destination.join(FORM)).unwrap();
    let control = test.0.join("control");
    export_designer(&destination, &control).unwrap();
    let updated = "SELECT \"<&>\"\n// filled value";
    prepare_query_edit(&destination, &base, &empty, updated)
        .unwrap()
        .publish(&base)
        .unwrap();
    assert_eq!(fs::read(destination.join(FORM)).unwrap(), owner);
    let prepared =
        prepare_protected(&destination, &base, &test.0.join("generations"), true).unwrap();
    assert!(prepared.diagnostics().is_empty());
    let parsed =
        NativeQueryDocument::from_xml(&fs::read_to_string(control.join(DIALOG)).unwrap()).unwrap();
    let mut values = parsed
        .queries()
        .iter()
        .map(|query| (query.slot.clone(), query.text.clone()))
        .collect::<BTreeMap<_, _>>();
    values.insert(
        QuerySlot::DynamicList {
            attribute: FormId::parse("6").unwrap(),
        },
        updated.into(),
    );
    assert_eq!(
        fs::read_to_string(prepared.xml().join(DIALOG)).unwrap(),
        parsed.materialize(&values).unwrap()
    );
}

/// Independent before/after native trees include every resource byte and empty directory.
#[test]
fn every_legacy_layout_extracts_single_values_without_changing_native_evidence_or_noop_bytes() {
    for kind in 1..=5 {
        let test = TestDir::new();
        let (source, _) = legacy(&test, kind, &[]);
        let before = files::Inventory::read(&source).unwrap();
        let control = test.0.join("control");
        export_designer(&source, &control).unwrap();
        let plan = prepare_query_layout(&source).unwrap();
        assert_eq!(plan.manifest().schema_version, 5);
        assert_eq!(plan.manifest().query_resources.len(), 2);
        assert!(!plan.is_empty());
        let destination = test.0.join("queries");
        assert_eq!(
            plan.publish(&destination).unwrap(),
            LayoutPublication::Created
        );
        assert_eq!(
            plan.publish(&destination).unwrap(),
            LayoutPublication::Unchanged
        );
        let reader = SourceReader::open(&destination).unwrap();
        assert_eq!(reader.query_paths().count(), 4);
        let output = test.0.join("output");
        export_designer(&destination, &output).unwrap();
        assert_eq!(
            files::Inventory::read(&output).unwrap(),
            files::Inventory::read(&control).unwrap()
        );
        assert_eq!(files::Inventory::read(&source).unwrap(), before);
        let no_op = prepare_query_layout(&destination).unwrap();
        assert!(no_op.is_empty());
        for target in [
            prepare_descriptor_layout,
            prepare_resource_layout,
            prepare_readable_layout,
            prepare_form_layout,
        ] {
            assert!(target(&destination).unwrap().is_empty());
        }
    }
}
