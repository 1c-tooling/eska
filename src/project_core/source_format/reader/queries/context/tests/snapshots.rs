//! Current form and DCS views retain one authoring authority after the filesystem disappears.

use super::*;
use crate::project_core::{
    metadata_parser::PropertiesMode,
    source_format::{MetadataSnapshot, import_designer_forms},
};

/// Use declared native ownership rather than inferring a descriptor from a resource filename.
fn snapshot(reader: &SourceReader, native: &str, text: &str) -> MetadataSnapshot {
    let resource = reader
        .manifest()
        .query_resources
        .iter()
        .find(|resource| resource.path == path(native))
        .unwrap();
    let owner = reader.owner_for_native_main(&resource.descriptor).unwrap();
    reader
        .parse_metadata(owner, text, PropertiesMode::All)
        .unwrap()
}

/// A changed body and exact YAML coordinates survive without a second form/query file read.
#[test]
fn unified_form_snapshot_shares_current_reference_bytes_and_authoring_maps() {
    let (_directory, _, root) = source();
    let reader = SourceReader::open(&root).unwrap();
    let (text, _) = inputs(&reader, DIALOG);
    let changed = format!("# Я😀\r\n{}", text.replace("Grid", "CurrentGrid"));
    let metadata = snapshot(&reader, DIALOG, &changed);
    fs::remove_dir_all(&root).unwrap();
    let form = metadata.form.as_ref().unwrap();
    let context = form.query_context().unwrap().unwrap();
    assert!(form.reference_xml().contains("CurrentGrid"));
    assert!(!form.reference_xml().contains("SELECT"));
    assert!(Arc::ptr_eq(&form.reference_xml, &context.xml));
    let pure = reader
        .parse_query_context(&path(DIALOG), &changed, &changed, 1_000_000)
        .unwrap();
    assert_eq!(context, &pure);
    let location = context
        .locate_reference_range(context.references()[0].source_range())
        .unwrap();
    let QueryAuthoringRange::EnclosingNode(range) = location else {
        panic!("a native preserved fragment must keep its current YAML scalar");
    };
    assert!(changed[range.clone()].contains("Attribute"));
    let captured = metadata.coordinates.range(range.clone()).unwrap();
    for (offset, position) in [(range.start, captured.start), (range.end, captured.end)] {
        let prefix = &changed[..offset];
        let column = prefix.rsplit('\n').next().unwrap().encode_utf16().count();
        assert_eq!(
            position.line as usize,
            prefix.bytes().filter(|&byte| byte == b'\n').count()
        );
        assert_eq!(position.character as usize, column);
    }
}

/// An unsupported current link cannot resurrect a previous good query context or hide form metadata.
#[test]
fn invalid_form_query_bindings_are_explicit_inside_the_current_snapshot() {
    let (_directory, _, root) = source();
    let reader = SourceReader::open(&root).unwrap();
    let (text, _) = inputs(&reader, DIALOG);
    let previous = snapshot(&reader, DIALOG, &text);
    assert!(previous.form.as_ref().unwrap().query_context().is_ok());
    let changed = text.replace("Queries/Attribute_5.sdbl", "Queries/Foreign.sdbl");
    assert_ne!(text, changed);
    let current = snapshot(&reader, DIALOG, &changed);
    let form = current.form.as_ref().unwrap();
    assert!(form.reference_xml().contains("Queries/Foreign.sdbl"));
    assert!(form.query_context().is_err());
    assert_ne!(previous.source_sha256, current.source_sha256);
    assert_eq!(previous.documents, current.documents);
}

/// Earlier unified forms have current metadata without inventing a declared standalone query context.
#[test]
fn form_layout_four_retains_literal_queries_without_a_context_claim() {
    let directory = crate::test_support::TestDir::new();
    let native = crate::project_core::source_format::queries::test_support::fixture(&directory);
    let root = directory.0.join("forms");
    import_designer_forms(&native, &root).unwrap();
    let reader = SourceReader::open(&root).unwrap();
    let text = fs::read_to_string(root.join(FORM)).unwrap();
    let current = reader
        .parse_metadata(&path(FORM), &text, PropertiesMode::All)
        .unwrap();
    fs::remove_dir_all(&root).unwrap();
    let form = current.form.as_ref().unwrap();
    assert!(form.reference_xml().contains("SELECT"));
    assert_eq!(form.query_context().unwrap(), None);
}

/// A current parsed owner can bind current DCS bytes after every source and plain value has gone.
#[test]
fn dcs_context_from_snapshot_matches_the_supplied_text_route_without_io() {
    let (_directory, _, root) = source();
    let reader = SourceReader::open(&root).unwrap();
    let (owner, resource) = inputs(&reader, TEMPLATE);
    let current = snapshot(&reader, TEMPLATE, &owner);
    let changed = resource.replace("<name>Union</name>", "<name>Текущий😀</name>");
    fs::remove_dir_all(&root).unwrap();
    let view = reader
        .parse_query_context_from_snapshot(&path(TEMPLATE), &current, &changed, 1_000_000)
        .unwrap();
    let previous_api = reader
        .parse_query_context(&path(TEMPLATE), &owner, &changed, 1_000_000)
        .unwrap();
    assert_eq!(view, previous_api);
    assert_eq!(view.reference_xml(), changed);
    assert_eq!(
        view.references()[0].slot,
        QuerySlot::Composition {
            datasets: vec!["Текущий😀".into(), "Same".into()]
        }
    );
}

/// A different snapshot, template purpose, identity, unloaded property set or limit grants no context.
#[test]
fn dcs_snapshot_binding_rejects_mismatched_or_incomplete_current_owners() {
    let (_directory, _, root) = source();
    let reader = SourceReader::open(&root).unwrap();
    let (owner, resource) = inputs(&reader, TEMPLATE);
    let current = snapshot(&reader, TEMPLATE, &owner);
    for mutation in 0..5 {
        let mut wrong = current.clone();
        match mutation {
            0 => wrong.manifest_sha256 = "f".repeat(64),
            1 => wrong.path = path(FORM),
            2 => {
                wrong
                    .documents
                    .iter_mut()
                    .find(|document| document.path.as_str().contains("QuerySchema"))
                    .unwrap()
                    .descriptor
                    .objects[0]
                    .metadata
                    .uuid = "80000000-0000-4000-8000-000000000099".into();
            }
            3 => {
                wrong
                    .documents
                    .iter_mut()
                    .find(|document| document.path.as_str().contains("QuerySchema"))
                    .unwrap()
                    .descriptor
                    .objects[0]
                    .properties = None;
            }
            _ => {
                wrong = snapshot(
                    &reader,
                    TEMPLATE,
                    &owner.replace("DataCompositionSchema", "BinaryData"),
                );
            }
        }
        assert!(
            reader
                .parse_query_context_from_snapshot(&path(TEMPLATE), &wrong, &resource, 1_000_000)
                .is_err(),
            "mutation {mutation}"
        );
    }
    assert_eq!(
        reader
            .parse_query_context_from_snapshot(&path(TEMPLATE), &current, &resource, 1)
            .unwrap_err()
            .code,
        "source_document_too_large"
    );
    assert!(
        reader
            .parse_query_context_from_snapshot(
                &path(TEMPLATE),
                &current,
                &resource.replace("Queries/Query_Same.sdbl", "Queries/Foreign.sdbl"),
                1_000_000
            )
            .is_err()
    );
    assert_eq!(
        reader
            .parse_query_context_from_snapshot(
                &path("Undeclared.xml"),
                &current,
                &resource,
                1_000_000
            )
            .unwrap_err()
            .code,
        "source_query_undeclared"
    );
}

/// DCS input loading preserves malformed current bytes and never touches neighboring owner/query files.
#[test]
fn raw_dcs_input_is_declared_bounded_and_independent_of_neighbor_files() {
    let (directory, _, root) = source();
    let reader = SourceReader::open(&root).unwrap();
    let malformed = "\u{feff}<broken Я😀\r\n";
    fs::write(root.join(RESOURCE), malformed).unwrap();
    let native_resource = reader
        .manifest()
        .query_resources
        .iter()
        .find(|resource| resource.path == path(TEMPLATE))
        .unwrap();
    let owner = reader
        .owner_for_native_main(&native_resource.descriptor)
        .unwrap();
    fs::remove_file(root.join(owner.as_path())).unwrap();
    for query in reader.query_paths() {
        fs::remove_file(root.join(query.as_path())).unwrap();
    }
    assert_eq!(
        reader
            .read_query_resource_text(&path(RESOURCE), 1_000_000)
            .unwrap(),
        malformed
    );
    assert_eq!(
        reader
            .read_query_resource_text(&path(RESOURCE), 1)
            .unwrap_err()
            .code,
        "source_document_too_large"
    );
    for undeclared in [FORM, TEMPLATE, "Undeclared.xml"] {
        assert_eq!(
            reader
                .read_query_resource_text(&path(undeclared), 1_000_000)
                .unwrap_err()
                .code,
            "source_query_undeclared"
        );
    }
    fs::remove_file(root.join(RESOURCE)).unwrap();
    assert!(
        reader
            .read_query_resource_text(&path(RESOURCE), 1_000_000)
            .is_err()
    );
    #[cfg(unix)]
    {
        let outside = directory.0.join("outside.xml");
        fs::write(&outside, "outside").unwrap();
        std::os::unix::fs::symlink(&outside, root.join(RESOURCE)).unwrap();
        assert!(
            reader
                .read_query_resource_text(&path(RESOURCE), 1_000_000)
                .is_err()
        );
        assert_eq!(fs::read_to_string(outside).unwrap(), "outside");
    }
}
