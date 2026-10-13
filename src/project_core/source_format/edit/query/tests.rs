//! Owned tests verify one-file changes, accepted policy and real filesystem publication failures.

use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};

use super::*;
use crate::{
    project_core::source_format::{
        NativeQueryDocument, NativeUuid, SourceReader, accept_source, export_designer,
        files::Inventory, import_designer_queries, prepare_protected, queries::test_support::*,
    },
    test_support::TestDir,
};

const TEXT: &str = "// keep \"<&>\"\r\nВЫБРАТЬ 1 КАК Число\r\nГДЕ 1 = 1;\r\n";

/// Pinned metadata UUIDs remain independent of filenames and overlapping form Attribute IDs.
fn uuid(prefix: char) -> NativeUuid {
    NativeUuid::parse(&format!("{prefix}0000000-0000-4000-8000-000000000001")).unwrap()
}

/// One optional supplier restriction is encoded in the original native policy before acceptance.
fn accepted(directory: &TestDir, frozen: &[NativeUuid], locked: bool) -> (PathBuf, AcceptedBase) {
    let native = fixture(directory);
    if locked {
        let original = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/eska-format/designer/Ext/ParentConfigurations.bin");
        let policy = fs::read_to_string(original).unwrap().replace(
            "\"SampleConfiguration\",3,",
            &format!("\"SampleConfiguration\",4,0,0,{0},{0},", uuid('7').as_str()),
        );
        fs::write(native.join("Ext/ParentConfigurations.bin"), policy).unwrap();
        fs::create_dir_all(native.join("Ext/ParentConfigurations")).unwrap();
        fs::write(
            native.join("Ext/ParentConfigurations/SampleConfiguration.cf"),
            b"synthetic opaque supplier CF",
        )
        .unwrap();
    }
    let root = directory.0.join("source");
    import_designer_queries(&native, &root).unwrap();
    for name in [LIST, QUERY] {
        fs::write(root.join(name), TEXT).unwrap();
    }
    let reference = accept_source(&root, &directory.0.join("accepted"), frozen).unwrap();
    let base = AcceptedBase::open(&directory.0.join("accepted"), &reference).unwrap();
    (root, base)
}

/// Only paths whose actual bytes differ count as effects of a single editor operation.
fn changed(before: &Inventory, after: &Inventory) -> Vec<String> {
    assert_eq!(before.directories, after.directories);
    assert_eq!(
        before.files.keys().collect::<Vec<_>>(),
        after.files.keys().collect::<Vec<_>>()
    );
    after
        .files
        .iter()
        .filter(|(path, entry)| before.files.get(*path) != Some(*entry))
        .map(|(path, _)| path.as_str().to_owned())
        .collect()
}

/// A condition edit changes one plain file and exactly one query property in its native resource.
#[test]
fn saved_and_unsaved_condition_edits_preserve_every_other_input_and_native_byte() {
    for (name, native) in [(LIST, DIALOG), (QUERY, TEMPLATE)] {
        let directory = TestDir::new();
        let (root, base) = accepted(&directory, &[], false);
        let before = Inventory::read(&root).unwrap();
        let control = directory.0.join("control");
        export_designer(&root, &control).unwrap();
        let updated = TEXT.replace("1 = 1", "1 = 2");
        let preview = prepare_query_edit_text(&base, &path(name), TEXT, &updated).unwrap();
        assert_eq!(preview.path(), &path(name));
        assert_eq!(preview.baseline(), base.reference());
        assert_eq!(preview.updated(), updated);
        assert!(!preview.is_empty());
        assert_eq!(Inventory::read(&root).unwrap(), before);
        let plan = prepare_query_edit(&root, &base, &path(name), &updated).unwrap();
        assert_eq!(plan.path(), preview.path());
        assert_eq!(plan.updated(), preview.updated());
        plan.publish(&base).unwrap();
        assert_eq!(changed(&before, &Inventory::read(&root).unwrap()), [name]);
        let prepared =
            prepare_protected(&root, &base, &directory.0.join("generations"), true).unwrap();
        assert_eq!(
            changed(
                &Inventory::read(&control).unwrap(),
                &Inventory::read(prepared.xml()).unwrap()
            ),
            [native]
        );
        let old = fs::read_to_string(control.join(native)).unwrap();
        let parsed = NativeQueryDocument::from_xml(&old).unwrap();
        let reader = SourceReader::open(&root).unwrap();
        let reference = reader.read_query_resource(&path(native), u64::MAX).unwrap();
        let slot = &reference
            .references()
            .iter()
            .find(|reference| reference.source == path(name))
            .unwrap()
            .slot;
        let mut values = parsed
            .queries()
            .iter()
            .map(|query| (query.slot.clone(), query.text.clone()))
            .collect::<BTreeMap<_, _>>();
        values.insert(slot.clone(), updated);
        assert_eq!(
            parsed.materialize(&values).unwrap(),
            fs::read_to_string(prepared.xml().join(native)).unwrap()
        );
    }
}

/// Equal requests preserve exact CRLF, comments, spelling and saved file metadata.
#[test]
fn equal_empty_and_editor_owned_values_keep_exact_bytes_without_neighbor_reads() {
    let directory = TestDir::new();
    let (root, base) = accepted(&directory, &[], false);
    let before = Inventory::read(&root).unwrap();
    let modified = fs::metadata(root.join(LIST)).unwrap().modified().unwrap();
    let preview = prepare_query_edit_text(&base, &path(LIST), TEXT, TEXT).unwrap();
    assert!(preview.is_empty());
    let plan = prepare_query_edit(&root, &base, &path(LIST), TEXT).unwrap();
    assert!(plan.is_empty());
    plan.publish(&base).unwrap();
    assert_eq!(
        fs::metadata(root.join(LIST)).unwrap().modified().unwrap(),
        modified
    );
    assert_eq!(Inventory::read(&root).unwrap(), before);
    fs::remove_file(root.join(QUERY)).unwrap();
    fs::write(root.join(RESOURCE), "broken unrelated resource").unwrap();
    let buffer = "// unsaved buffer\nSELECT &Value";
    let preview = prepare_query_edit_text(&base, &path(LIST), buffer, "").unwrap();
    assert_eq!(preview.updated(), "");
    assert!(!preview.is_empty());
    assert!(
        prepare_query_edit_text(&base, &path(LIST), "", "")
            .unwrap()
            .is_empty()
    );
    prepare_query_edit(&root, &base, &path(LIST), buffer)
        .unwrap()
        .publish(&base)
        .unwrap();
    assert_eq!(fs::read_to_string(root.join(LIST)).unwrap(), buffer);
}

/// Working maps and filenames cannot grant permission or defeat owner/ancestor restrictions.
#[test]
fn query_editor_uses_pinned_owners_native_support_and_inherited_freeze() {
    for (frozen, locked, name, code) in [
        (vec![uuid('7')], false, LIST, "source_edit_frozen"),
        (vec![uuid('8')], false, QUERY, "source_edit_frozen"),
        (vec![uuid('2')], false, LIST, "source_edit_frozen"),
        (vec![uuid('2')], false, QUERY, "source_edit_frozen"),
        (vec![uuid('1')], false, LIST, "source_edit_frozen"),
        (vec![], true, LIST, "source_edit_native_restricted"),
    ] {
        let directory = TestDir::new();
        let (root, base) = accepted(&directory, &frozen, locked);
        fs::write(root.join("source.json"), "{}").unwrap();
        fs::write(root.join(FORM), "working owner has no authority").unwrap();
        let before = Inventory::read(&root).unwrap();
        assert_eq!(
            prepare_query_edit_text(&base, &path(name), TEXT, "SELECT 2")
                .unwrap_err()
                .code,
            code
        );
        assert_eq!(
            prepare_query_edit(&root, &base, &path(name), "SELECT 2")
                .unwrap_err()
                .code,
            code
        );
        assert_eq!(Inventory::read(&root).unwrap(), before);
    }
    let directory = TestDir::new();
    let (root, base) = accepted(&directory, &[], false);
    let undeclared = "Catalogs/Sample/Forms/Main/Queries/unbound.sdbl";
    fs::write(root.join(undeclared), TEXT).unwrap();
    for name in [
        undeclared,
        FORM,
        RESOURCE,
        "Catalogs/Sample/Templates/QuerySchema/Queries/QUERY_Same.sdbl",
    ] {
        assert_eq!(
            prepare_query_edit_text(&base, &path(name), TEXT, "SELECT 2")
                .unwrap_err()
                .code,
            "source_query_undeclared"
        );
        assert_eq!(
            prepare_query_edit(&root, &base, &path(name), "SELECT 2")
                .unwrap_err()
                .code,
            "source_query_undeclared"
        );
    }
}

/// Current text may be repaired, but an unrepresentable replacement never reaches the writer.
#[test]
fn invalid_replacements_refuse_and_xml_character_repair_preserves_plain_syntax() {
    let directory = TestDir::new();
    let (root, base) = accepted(&directory, &[], false);
    let before = Inventory::read(&root).unwrap();
    for invalid in ["SELECT\0", "SELECT\u{1f}", "\u{fffe}", "\u{ffff}"] {
        assert_eq!(
            prepare_query_edit_text(&base, &path(LIST), TEXT, invalid)
                .unwrap_err()
                .code,
            "source_xml_character_invalid"
        );
        assert_eq!(
            prepare_query_edit(&root, &base, &path(LIST), invalid)
                .unwrap_err()
                .code,
            "source_xml_character_invalid"
        );
    }
    let oversized = " ".repeat(usize::try_from(files::MAX_TEXT_BYTES).unwrap() + 1);
    assert_eq!(
        prepare_query_edit_text(&base, &path(LIST), TEXT, &oversized)
            .unwrap_err()
            .code,
        "source_document_too_large"
    );
    assert_eq!(
        prepare_query_edit(&root, &base, &path(LIST), &oversized)
            .unwrap_err()
            .code,
        "source_document_too_large"
    );
    assert_eq!(Inventory::read(&root).unwrap(), before);
    let original = "damaged\0value";
    fs::write(root.join(LIST), original).unwrap();
    let updated = "not yet valid query syntax <>&\"\r\n\t";
    let preview = prepare_query_edit_text(&base, &path(LIST), original, updated).unwrap();
    assert_eq!(preview.updated(), updated);
    prepare_query_edit(&root, &base, &path(LIST), updated)
        .unwrap()
        .publish(&base)
        .unwrap();
    assert_eq!(fs::read_to_string(root.join(LIST)).unwrap(), updated);
}

/// A saved plan refuses current-byte conflicts, another pin and damaged baseline storage.
#[test]
fn query_plan_rechecks_bytes_baseline_pin_and_pinned_storage_before_publication() {
    let directory = TestDir::new();
    let (root, base) = accepted(&directory, &[], false);
    let plan = prepare_query_edit(&root, &base, &path(LIST), "SELECT 2").unwrap();
    fs::write(root.join(LIST), "concurrent write").unwrap();
    assert_eq!(
        plan.publish(&base).unwrap_err().code,
        "source_edit_conflict"
    );
    assert_eq!(
        fs::read_to_string(root.join(LIST)).unwrap(),
        "concurrent write"
    );
    let second = directory.0.join("other-base");
    let pin = accept_source(&root, &second, &[uuid('7')]).unwrap();
    let other = AcceptedBase::open(&second, &pin).unwrap();
    assert_eq!(
        plan.publish(&other).unwrap_err().code,
        "source_edit_baseline_changed"
    );
    fs::write(root.join(LIST), TEXT).unwrap();
    fs::write(directory.0.join("accepted/accepted.json"), "{}").unwrap();
    assert_eq!(
        plan.publish(&base).unwrap_err().code,
        "source_baseline_untrusted"
    );
    assert_eq!(fs::read_to_string(root.join(LIST)).unwrap(), TEXT);
}

/// Real aliases and read-only files are refused during preparation and again by the shared writer.
#[cfg(unix)]
#[test]
fn query_editor_refuses_symlinks_read_only_missing_and_non_utf8_files() {
    use std::os::unix::{fs::PermissionsExt, fs::symlink};
    let directory = TestDir::new();
    let (root, base) = accepted(&directory, &[], false);
    let file = root.join(LIST);
    let original = fs::read(&file).unwrap();
    let target = directory.0.join("target.sdbl");
    fs::write(&target, &original).unwrap();
    fs::remove_file(&file).unwrap();
    symlink(&target, &file).unwrap();
    assert_eq!(
        prepare_query_edit(&root, &base, &path(LIST), "SELECT 2")
            .unwrap_err()
            .code,
        "source_edit_unsafe_path"
    );
    assert_eq!(fs::read(&target).unwrap(), original);
    fs::remove_file(&file).unwrap();
    fs::write(&file, &original).unwrap();
    let plan = prepare_query_edit(&root, &base, &path(LIST), "SELECT 2").unwrap();
    fs::set_permissions(&file, fs::Permissions::from_mode(0o444)).unwrap();
    assert_eq!(
        prepare_query_edit(&root, &base, &path(LIST), "SELECT 2")
            .unwrap_err()
            .code,
        "source_edit_read_only"
    );
    assert_eq!(
        plan.publish(&base).unwrap_err().code,
        "source_edit_read_only"
    );
    fs::set_permissions(&file, fs::Permissions::from_mode(0o644)).unwrap();
    fs::write(&file, [0xff]).unwrap();
    assert_eq!(
        prepare_query_edit(&root, &base, &path(LIST), "SELECT 2")
            .unwrap_err()
            .code,
        "source_encoding_unsupported"
    );
    fs::remove_file(&file).unwrap();
    assert!(prepare_query_edit(&root, &base, &path(LIST), "SELECT 2").is_err());
    assert!(!file.exists());
}
