use super::*;
use crate::{
    project_core::source_format::{
        Child, Property, PropertyName, PropertyValue, Section, XmlFragment, import_designer,
    },
    test_support::TestDir,
};
use std::{fs, path::PathBuf};

/// Import the public corpus into this test's exclusively owned playground directory.
fn project() -> (TestDir, PathBuf, MetadataInputs) {
    let test = TestDir::new();
    let root = test.0.join("source");
    import_designer(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/eska-format/designer"),
        &root,
    )
    .unwrap();
    let inputs = MetadataInputs::open(&root).unwrap();
    (test, root, inputs)
}

/// The corpus catalog includes inline attributes, a table and a separate form descriptor.
fn catalog() -> SourcePath {
    SourcePath::parse("Catalogs/Sample/object.yaml").unwrap()
}

/// Return a current parsed model, failing when a test accidentally queries unavailable input.
fn ready(inputs: &MetadataInputs, path: &SourcePath) -> Arc<MetadataSnapshot> {
    let InputState::Ready(snapshot) = &inputs.owner(path).unwrap().state else {
        panic!("ready input")
    };
    Arc::clone(snapshot)
}

/// Load every declared owner through the same request used by batch consumers.
fn load_all(inputs: &mut MetadataInputs) {
    let paths = inputs.owner_paths().cloned().collect::<Vec<_>>();
    for path in paths {
        inputs
            .load_saved(&path, inputs.revision(), &InputRequest::default())
            .unwrap();
    }
}

#[test]
fn unread_missing_and_failed_owners_do_not_become_successful_empty_inputs() {
    let (_test, root, mut inputs) = project();
    assert_eq!(inputs.summary().pending, 3);
    assert!(!inputs.summary().metadata_input_complete);
    fs::remove_file(root.join(catalog().as_path())).unwrap();
    inputs
        .load_saved(&catalog(), inputs.revision(), &InputRequest::default())
        .unwrap();
    let InputState::Unavailable(InputFailure::Source(error)) =
        &inputs.owner(&catalog()).unwrap().state
    else {
        panic!("missing input")
    };
    assert_eq!(error.code, "source_file_missing");
    assert_eq!(inputs.summary().unavailable, 1);
    assert_eq!(inputs.summary().pending, 2);
    assert!(inputs.owner(&catalog()).unwrap().source_sha256.is_none());
    assert!(!inputs.summary().metadata_input_complete);
}

#[test]
fn saved_and_overlay_bytes_have_identical_values_hashes_and_coordinates() {
    let (_test, root, mut inputs) = project();
    load_all(&mut inputs);
    assert!(inputs.summary().metadata_input_complete);
    let saved = ready(&inputs, &catalog());
    let text = fs::read_to_string(root.join(catalog().as_path())).unwrap();
    assert_eq!(saved.source_bytes, text.len() as u64);
    inputs
        .apply_overlay(
            &catalog(),
            inputs.revision(),
            4,
            &text,
            &InputRequest::default(),
        )
        .unwrap();
    let overlay = ready(&inputs, &catalog());
    assert_eq!(saved, overlay);
    assert_eq!(
        Some(&overlay.source_sha256),
        inputs.owner(&catalog()).unwrap().source_sha256.as_ref()
    );
    assert_eq!(inputs.manifest_sha256(), overlay.manifest_sha256);
    assert!(inputs.summary().metadata_input_complete);
    assert!(!root.join("Configuration.xml").exists());
}

/// A host reservation bounds both saved and editor reads, clearing current models on refusal.
#[test]
fn source_budget_refusal_is_distinct_from_a_document_limit_and_releases_current_input() {
    let (_test, root, mut inputs) = project();
    let path = catalog();
    inputs
        .load_saved(&path, inputs.revision(), &InputRequest::default())
        .unwrap();
    let saved = ready(&inputs, &path);
    let text = fs::read_to_string(root.join(path.as_path())).unwrap();
    let request = InputRequest::default().with_source_budget(0, 20);
    inputs
        .load_saved(&path, inputs.revision(), &request)
        .unwrap();
    assert_eq!(
        inputs.owner(&path).unwrap().state,
        InputState::Unavailable(InputFailure::SourceBudgetExceeded { limit: 20 })
    );
    let request =
        InputRequest::default().with_source_budget(text.len() as u64 - 1, text.len() as u64);
    inputs
        .apply_overlay(&path, inputs.revision(), 1, &text, &request)
        .unwrap();
    assert!(matches!(
        inputs.owner(&path).unwrap().state,
        InputState::Unavailable(InputFailure::SourceBudgetExceeded { .. })
    ));
    let request = InputRequest {
        max_document_bytes: 2,
        ..InputRequest::default()
    }
    .with_source_budget(10, 20);
    inputs
        .apply_overlay(&path, inputs.revision(), 2, &text, &request)
        .unwrap();
    assert_eq!(
        inputs.owner(&path).unwrap().state,
        InputState::Unavailable(InputFailure::DocumentTooLarge { limit: 2 })
    );
    inputs
        .close_overlay(&path, inputs.revision(), &InputRequest::default())
        .unwrap();
    assert_eq!(ready(&inputs, &path), saved);
    assert_eq!(fs::read_to_string(root.join(path.as_path())).unwrap(), text);
}

#[test]
fn invalid_overlay_replaces_current_model_and_keeps_old_snapshot_only_for_its_holder() {
    let (_test, root, mut inputs) = project();
    inputs
        .load_saved(&catalog(), inputs.revision(), &InputRequest::default())
        .unwrap();
    let saved = ready(&inputs, &catalog());
    let text = fs::read_to_string(root.join(catalog().as_path())).unwrap();
    inputs
        .apply_overlay(
            &catalog(),
            inputs.revision(),
            1,
            "document: [",
            &InputRequest::default(),
        )
        .unwrap();
    let current = inputs.owner(&catalog()).unwrap();
    assert!(
        matches!(&current.state, InputState::Unavailable(InputFailure::Source(error)) if error.code == "source_yaml_invalid")
    );
    assert!(current.source_sha256.is_some());
    assert_ne!(
        current.source_sha256.as_deref(),
        Some(saved.source_sha256.as_str())
    );
    assert_eq!(
        saved.documents[0].descriptor.objects[0].metadata.name(),
        "Sample"
    );
    assert_eq!(
        fs::read_to_string(root.join(catalog().as_path())).unwrap(),
        text
    );
    inputs
        .close_overlay(&catalog(), inputs.revision(), &InputRequest::default())
        .unwrap();
    assert_eq!(ready(&inputs, &catalog()), saved);
}

#[test]
fn stale_request_and_document_versions_cannot_replace_a_newer_draft() {
    let (_test, root, mut inputs) = project();
    let text = fs::read_to_string(root.join(catalog().as_path())).unwrap();
    let initial = inputs.revision();
    inputs
        .apply_overlay(&catalog(), initial, 8, &text, &InputRequest::default())
        .unwrap();
    let current = inputs.owner(&catalog()).unwrap().clone();
    assert!(matches!(
        inputs.apply_overlay(&catalog(), initial, 9, "broken", &InputRequest::default()),
        Err(UpdateError::StaleRevision { .. })
    ));
    for version in [7, 8] {
        assert_eq!(
            inputs.apply_overlay(
                &catalog(),
                inputs.revision(),
                version,
                "broken",
                &InputRequest::default()
            ),
            Err(UpdateError::StaleDocumentVersion {
                current: 8,
                received: version
            })
        );
    }
    assert_eq!(inputs.owner(&catalog()).unwrap(), &current);
}

#[test]
fn filesystem_refresh_preserves_editor_overlay_until_explicit_close() {
    let (_test, root, mut inputs) = project();
    let text = fs::read_to_string(root.join(catalog().as_path())).unwrap();
    let draft = format!("# UTF-8 «draft»\r\n{text}");
    inputs
        .apply_overlay(
            &catalog(),
            inputs.revision(),
            2,
            &draft,
            &InputRequest::default(),
        )
        .unwrap();
    let overlay = ready(&inputs, &catalog());
    fs::remove_file(root.join(catalog().as_path())).unwrap();
    assert_eq!(
        inputs.load_saved(&catalog(), inputs.revision(), &InputRequest::default()),
        Err(UpdateError::OverlayActive {
            document_version: 2
        })
    );
    assert!(Arc::ptr_eq(&overlay, &ready(&inputs, &catalog())));
    inputs
        .close_overlay(&catalog(), inputs.revision(), &InputRequest::default())
        .unwrap();
    assert!(matches!(
        inputs.owner(&catalog()).unwrap().state,
        InputState::Unavailable(_)
    ));
}

#[test]
fn one_owner_update_keeps_unrelated_models_and_revisions() {
    let (_test, root, mut inputs) = project();
    load_all(&mut inputs);
    let module = SourcePath::parse("CommonModules/Tools/object.yaml").unwrap();
    let previous = inputs.owner(&module).unwrap().clone();
    let snapshot = ready(&inputs, &module);
    let text = fs::read_to_string(root.join(catalog().as_path())).unwrap();
    inputs
        .apply_overlay(
            &catalog(),
            inputs.revision(),
            1,
            &format!("# changed\n{text}"),
            &InputRequest::default(),
        )
        .unwrap();
    assert_eq!(inputs.owner(&module).unwrap(), &previous);
    assert!(Arc::ptr_eq(&snapshot, &ready(&inputs, &module)));
    assert_ne!(previous.revision, inputs.revision());
}

#[test]
fn disk_and_overlay_limits_both_invalidate_current_model_before_parsing() {
    let (_test, root, mut inputs) = project();
    inputs
        .load_saved(&catalog(), inputs.revision(), &InputRequest::default())
        .unwrap();
    let text = fs::read_to_string(root.join(catalog().as_path())).unwrap();
    let request = InputRequest {
        max_document_bytes: 2,
        ..InputRequest::default()
    };
    inputs
        .load_saved(&catalog(), inputs.revision(), &request)
        .unwrap();
    assert_eq!(
        inputs.owner(&catalog()).unwrap().state,
        InputState::Unavailable(InputFailure::DocumentTooLarge { limit: 2 })
    );
    inputs
        .apply_overlay(&catalog(), inputs.revision(), 1, &text, &request)
        .unwrap();
    assert_eq!(
        inputs.owner(&catalog()).unwrap().state,
        InputState::Unavailable(InputFailure::DocumentTooLarge { limit: 2 })
    );
    assert!(inputs.owner(&catalog()).unwrap().source_sha256.is_none());
    assert!(!inputs.summary().metadata_input_complete);
}

#[test]
fn shared_cancellation_invalidates_current_model_and_cannot_be_reset() {
    let (_test, root, mut inputs) = project();
    inputs
        .load_saved(&catalog(), inputs.revision(), &InputRequest::default())
        .unwrap();
    let text = fs::read_to_string(root.join(catalog().as_path())).unwrap();
    let request = InputRequest::default();
    request.cancellation.clone().cancel();
    inputs
        .apply_overlay(&catalog(), inputs.revision(), 1, &text, &request)
        .unwrap();
    assert_eq!(
        inputs.owner(&catalog()).unwrap().state,
        InputState::Unavailable(InputFailure::Cancelled)
    );
    inputs
        .close_overlay(&catalog(), inputs.revision(), &request)
        .unwrap();
    assert_eq!(
        inputs.owner(&catalog()).unwrap().state,
        InputState::Unavailable(InputFailure::Cancelled)
    );
    assert!(request.cancellation.is_cancelled());
}

#[test]
fn recoverable_projection_issues_remain_visible_and_prevent_input_completeness() {
    let (_test, root, mut inputs) = project();
    load_all(&mut inputs);
    let reader = SourceReader::open(&root).unwrap();
    let mut family = reader.read_owner(&catalog()).unwrap().source;
    let node = &mut family.document.descriptor.object;
    let Section::Properties(properties) = &mut node.sections[1] else {
        panic!("properties")
    };
    properties.push(Property::Preserved {
        fragment: XmlFragment {
            xml: "<Mixed>before<Value/>after</Mixed>".into(),
            namespaces: vec![],
        },
    });
    let Section::Children(children) = &mut node.sections[2] else {
        panic!("children")
    };
    children.push(Child::Preserved {
        fragment: XmlFragment {
            xml: "<FutureChild xmlns='urn:future'/>".into(),
            namespaces: vec![],
        },
    });
    let text = family.to_yaml().unwrap();
    inputs
        .apply_overlay(
            &catalog(),
            inputs.revision(),
            1,
            &text,
            &InputRequest::default(),
        )
        .unwrap();
    assert_eq!(
        ready(&inputs, &catalog()).documents[0]
            .descriptor
            .diagnostics
            .len(),
        2
    );
    assert_eq!(inputs.summary().parse_issues, 2);
    assert_eq!(inputs.summary().ready, 3);
    assert!(!inputs.summary().metadata_input_complete);
}

#[test]
fn declared_owner_rules_and_native_identities_apply_to_overlays() {
    let (_test, root, mut inputs) = project();
    let unknown = SourcePath::parse("Catalogs/Unlisted/object.yaml").unwrap();
    assert_eq!(
        inputs.apply_overlay(
            &unknown,
            inputs.revision(),
            1,
            "anything",
            &InputRequest::default()
        ),
        Err(UpdateError::UnknownOwner(unknown))
    );
    assert_eq!(inputs.revision(), InputRevision::default());
    let reader = SourceReader::open(&root).unwrap();
    let mut family = reader.read_owner(&catalog()).unwrap().source;
    let Section::Properties(properties) = &mut family.document.descriptor.object.sections[1] else {
        panic!("properties")
    };
    let property = properties
        .iter_mut()
        .find(|property| {
            matches!(
                property,
                Property::Typed {
                    name: PropertyName::Name,
                    ..
                }
            )
        })
        .unwrap();
    *property = Property::Typed {
        name: PropertyName::Name,
        value: PropertyValue::Text("Renamed".into()),
    };
    inputs
        .apply_overlay(
            &catalog(),
            inputs.revision(),
            1,
            &family.to_yaml().unwrap(),
            &InputRequest::default(),
        )
        .unwrap();
    assert!(
        matches!(&inputs.owner(&catalog()).unwrap().state, InputState::Unavailable(InputFailure::Source(error)) if error.code == "source_declaration_mismatch")
    );
}

#[cfg(unix)]
#[test]
fn saved_input_never_follows_an_owner_alias() {
    let (test, root, mut inputs) = project();
    let path = root.join(catalog().as_path());
    let external = test.0.join("external.yaml");
    fs::rename(&path, &external).unwrap();
    std::os::unix::fs::symlink(&external, &path).unwrap();
    inputs
        .load_saved(&catalog(), inputs.revision(), &InputRequest::default())
        .unwrap();
    assert!(
        matches!(&inputs.owner(&catalog()).unwrap().state, InputState::Unavailable(InputFailure::Source(error)) if error.code == "source_entry_unsupported")
    );
}

#[test]
fn exhausted_revision_rejects_update_before_accepting_new_bytes() {
    let (_test, _root, mut inputs) = project();
    inputs.revision = InputRevision(u64::MAX);
    let previous = inputs.owner(&catalog()).unwrap().clone();
    assert_eq!(
        inputs.apply_overlay(
            &catalog(),
            inputs.revision(),
            1,
            "invalid",
            &InputRequest::default()
        ),
        Err(UpdateError::RevisionExhausted)
    );
    assert_eq!(inputs.owner(&catalog()).unwrap(), &previous);
}
