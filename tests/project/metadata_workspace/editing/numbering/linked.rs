use super::*;
use eska::project::metadata_workspace::{EditingSnapshot, HistoryOperation, ProjectSession};

/// Keep a second linked document, a different numerator, and unlinked content in the same declared source.
fn linked_fixture() -> (TestDir, ObjectId, PathBuf, String) {
    let (directory, id, path, input) =
        fixture(MetadataKind::Document, "Document", "Documents", true);
    let source = directory.0.join("src");
    let root = source.join("Configuration.xml");
    fs::write(&root, fs::read_to_string(&root).unwrap().replace("</ChildObjects>", "<Document>Second</Document><Document>Unrelated</Document><DocumentNumerator>Другой</DocumentNumerator></ChildObjects>")).unwrap();
    for name in ["Shared", "Другой"] {
        selector_file(
            &source,
            &format!("DocumentNumerators/{name}.xml"),
            "DocumentNumerator",
            name,
            "<NumberType>String</NumberType><NumberLength>20</NumberLength><NumberAllowedLength>Fixed</NumberAllowedLength><NumberPeriodicity>Month</NumberPeriodicity><CheckUnique>true</CheckUnique>",
            "",
        );
    }
    for (name, reference) in [("Second", "DocumentNumerator.sHARED"), ("Unrelated", "")] {
        selector_file(
            &source,
            &format!("Documents/{name}.xml"),
            "Document",
            name,
            &format!(
                "<NumberType>String</NumberType><NumberLength>9</NumberLength><NumberAllowedLength>Variable</NumberAllowedLength><NumberPeriodicity>Year</NumberPeriodicity><CheckUnique>false</CheckUnique><Autonumbering>false</Autonumbering><InputByString/><Numerator>{reference}</Numerator>"
            ),
            "",
        );
    }
    (directory, id, path, input)
}

/// Use only addresses and context tokens returned by inspect, as an IDE or AI client would.
fn change(
    project: &mut ProjectSession,
    id: &ObjectId,
    state: &EditingSnapshot,
    name: &str,
    value: &str,
) -> Result<eska::project::metadata_workspace::RefreshReport, PropertyEditError> {
    project.update_property(
        id,
        &state.properties.snapshot,
        state.context_snapshot.as_deref(),
        &field(&state.properties.fields, name).path,
        &PropertyChange::Text {
            value: value.into(),
        },
    )
}

/// Document assignment copies all five settings in one descriptor; Unicode names retain their declared spelling.
#[test]
fn assigning_numerator_copies_settings_and_replays_exact_bytes() {
    let (directory, id, file, input) = linked_fixture();
    let mut workspace = MetadataWorkspace::open(&directory.0, &[], false).unwrap();
    let project = workspace.project_mut(&ProjectScope::Standalone).unwrap();
    let state = project.property_editing(&id).unwrap();
    change(
        project,
        &id,
        &state,
        "Numerator",
        "DocumentNumerator.Другой",
    )
    .unwrap();
    let expected = input
        .replace("DocumentNumerator.Shared", "DocumentNumerator.Другой")
        .replace(
            "<NumberLength>9</NumberLength>",
            "<NumberLength>20</NumberLength>",
        )
        .replace(
            "<NumberAllowedLength>Variable</NumberAllowedLength>",
            "<NumberAllowedLength>Fixed</NumberAllowedLength>",
        )
        .replace(
            "<NumberPeriodicity>Year</NumberPeriodicity>",
            "<NumberPeriodicity>Month</NumberPeriodicity>",
        )
        .replace(
            "<CheckUnique>false</CheckUnique>",
            "<CheckUnique>true</CheckUnique>",
        );
    assert_eq!(fs::read_to_string(&file).unwrap(), expected);
    for (undo, expected) in [(true, &input), (false, &expected)] {
        let state = project.property_editing(&id).unwrap();
        assert_eq!(
            if undo { state.undo } else { state.redo },
            Some(HistoryOperation::Linked)
        );
        project
            .undo_property(
                &id,
                &state.properties.snapshot,
                state.context_snapshot.as_deref(),
                undo,
            )
            .unwrap();
        assert_eq!(&fs::read_to_string(&file).unwrap(), expected);
    }
}

/// A numerator edit propagates only the edited parameter, preserving other mismatches and unrelated documents.
#[test]
fn numerator_change_and_history_are_one_atomic_multi_document_operation() {
    let (directory, _, file, input) = linked_fixture();
    let source = directory.0.join("src");
    let id = object(MetadataKind::DocumentNumerator, "Shared", None);
    let mut workspace = MetadataWorkspace::open(&directory.0, &[], false).unwrap();
    let project = workspace.project_mut(&ProjectScope::Standalone).unwrap();
    let state = project.property_editing(&id).unwrap();
    assert_eq!(state.linked_objects, 2);
    let number = field(&state.properties.fields, "NumberLength");
    let delta = PropertyChange::Text { value: "30".into() };
    let plan = project
        .preview_property(
            &id,
            &state.properties.snapshot,
            state.context_snapshot.as_deref(),
            &number.path,
            &delta,
        )
        .unwrap();
    assert_eq!(plan.files().filter(|file| !file.plan.is_empty()).count(), 3);
    assert!(plan.files().all(|file| file.plan.replacements().len() == 1));
    let originals: Vec<_> = plan
        .files()
        .map(|file| {
            (
                file.path.clone(),
                fs::read(source.join(&file.path)).unwrap(),
            )
        })
        .collect();
    let unrelated = fs::read(source.join("Documents/Unrelated.xml")).unwrap();
    change(project, &id, &state, "NumberLength", "30").unwrap();
    assert_eq!(
        fs::read_to_string(&file).unwrap(),
        input.replace(
            "<NumberLength>9</NumberLength>",
            "<NumberLength>30</NumberLength>"
        )
    );
    assert_eq!(
        fs::read(source.join("Documents/Unrelated.xml")).unwrap(),
        unrelated
    );
    let state = project.property_editing(&id).unwrap();
    project
        .undo_property(
            &id,
            &state.properties.snapshot,
            state.context_snapshot.as_deref(),
            true,
        )
        .unwrap();
    for (path, input) in &originals {
        assert_eq!(fs::read(source.join(path)).unwrap(), *input);
    }
    let state = project.property_editing(&id).unwrap();
    project
        .undo_property(
            &id,
            &state.properties.snapshot,
            state.context_snapshot.as_deref(),
            false,
        )
        .unwrap();
    assert_eq!(
        fs::read_to_string(file).unwrap(),
        input.replace(
            "<NumberLength>9</NumberLength>",
            "<NumberLength>30</NumberLength>"
        )
    );
    assert!(!directory.0.join(".eska/metadata/rename").exists());
}

/// Missing tokens, changed dependencies and newly linked documents all reject before touching any candidate.
#[test]
fn numerator_requires_fresh_dependency_context_even_without_watcher_events() {
    let (directory, _, file, original) = linked_fixture();
    let source = directory.0.join("src");
    let id = object(MetadataKind::DocumentNumerator, "Shared", None);
    let mut workspace = MetadataWorkspace::open(&directory.0, &[], false).unwrap();
    let project = workspace.project_mut(&ProjectScope::Standalone).unwrap();
    let initial = project.property_editing(&id).unwrap();
    let delta = PropertyChange::Text { value: "30".into() };
    assert!(matches!(
        project.update_property(
            &id,
            &initial.properties.snapshot,
            None,
            &field(&initial.properties.fields, "NumberLength").path,
            &delta
        ),
        Err(PropertyEditError::Edit(EditError::ContextRequired))
    ));
    let unrelated = source.join("Documents/Unrelated.xml");
    let original_unrelated = fs::read_to_string(&unrelated).unwrap();
    fs::write(
        &unrelated,
        original_unrelated.replace(
            "<Numerator></Numerator>",
            "<Numerator>DocumentNumerator.Shared</Numerator>",
        ),
    )
    .unwrap();
    assert!(matches!(
        change(project, &id, &initial, "NumberLength", "30"),
        Err(PropertyEditError::Edit(EditError::Conflict))
    ));
    assert_eq!(fs::read_to_string(file).unwrap(), original);
    let state = project.property_editing(&id).unwrap();
    assert_eq!(state.linked_objects, 3);
    change(project, &id, &state, "NumberLength", "30").unwrap();
    let state = project.property_editing(&id).unwrap();
    fs::write(
        &unrelated,
        fs::read_to_string(&unrelated)
            .unwrap()
            .replace("<Autonumbering>false", "<Autonumbering>true"),
    )
    .unwrap();
    assert!(matches!(
        project.undo_property(
            &id,
            &state.properties.snapshot,
            state.context_snapshot.as_deref(),
            true
        ),
        Err(PropertyEditError::Edit(EditError::Conflict))
    ));
    assert_eq!(
        project.property_editing(&id).unwrap().undo,
        Some(HistoryOperation::Linked)
    );
}

/// A malformed or incompatible linked descriptor must stop the entire update before the numerator changes.
#[test]
fn invalid_linked_document_never_leaves_partial_numbering_changes() {
    for invalid in [
        "<NumberLength>bad</NumberLength>",
        "<NumberLength>50</NumberLength>",
    ] {
        let (directory, _, file, input) = linked_fixture();
        let source = directory.0.join("src");
        fs::write(
            &file,
            input.replace("<NumberLength>9</NumberLength>", invalid),
        )
        .unwrap();
        let numerator = source.join("DocumentNumerators/Shared.xml");
        let before = fs::read(&numerator).unwrap();
        let mut workspace = MetadataWorkspace::open(&directory.0, &[], false).unwrap();
        let project = workspace.project_mut(&ProjectScope::Standalone).unwrap();
        let id = object(MetadataKind::DocumentNumerator, "Shared", None);
        let state = project.property_editing(&id).unwrap();
        assert!(change(project, &id, &state, "NumberType", "Number").is_err());
        assert_eq!(fs::read(&numerator).unwrap(), before);
        assert_eq!(
            fs::read_to_string(&file).unwrap(),
            input.replace("<NumberLength>9</NumberLength>", invalid)
        );
        assert!(project.property_editing(&id).unwrap().undo.is_none());
    }
}

/// Failure to establish linked context hides only the dependent writers, retaining independent fields.
#[test]
fn unreadable_dependency_retains_independent_editors() {
    let (directory, id, file, _) = linked_fixture();
    fs::write(
        directory.0.join("src/DocumentNumerators/Shared.xml"),
        "broken",
    )
    .unwrap();
    let mut workspace = MetadataWorkspace::open(&directory.0, &[], false).unwrap();
    let project = workspace.project_mut(&ProjectScope::Standalone).unwrap();
    let state = project.property_editing(&id).unwrap();
    assert!(state.context_snapshot.is_none());
    assert!(state.properties.read_only_properties.iter().any(|item| item.key.name == "Numerator" && item.reason == "linked_context_unavailable"));
    change(project, &id, &state, "Autonumbering", "true").unwrap();
    assert!(
        fs::read_to_string(file)
            .unwrap()
            .contains("<Autonumbering>true</Autonumbering>")
    );
}

/// An incompatible filling value blocks propagation until the user clears it explicitly.
#[test]
fn linked_number_type_rejects_a_documents_incompatible_fill_value() {
    let (directory, _, file, input) = linked_fixture();
    let standard = "<StandardAttributes xmlns:xr='http://v8.1c.ru/8.3/xcf/readable' xmlns:xsi='http://www.w3.org/2001/XMLSchema-instance' xmlns:xs='http://www.w3.org/2001/XMLSchema'><xr:StandardAttribute name='Number'><xr:FillValue xsi:type='xs:string'>ABC</xr:FillValue></xr:StandardAttribute></StandardAttributes>";
    let before = input.replace("</Properties>", &format!("{standard}</Properties>"));
    fs::write(&file, &before).unwrap();
    let mut workspace = MetadataWorkspace::open(&directory.0, &[], false).unwrap();
    let project = workspace.project_mut(&ProjectScope::Standalone).unwrap();
    let id = object(MetadataKind::DocumentNumerator, "Shared", None);
    let state = project.property_editing(&id).unwrap();
    assert!(
        matches!(change(project, &id, &state, "NumberType", "Number"), Err(PropertyEditError::Related { object_id, name, error: EditError::IncompatibleProperty(key) }) if key.name == "FillValue" && object_id == object(MetadataKind::Document, "Probe", None) && name == "Probe")
    );
    assert_eq!(fs::read_to_string(file).unwrap(), before);
    assert!(project.property_editing(&id).unwrap().undo.is_none());
}

/// Read-only dependent descriptors reject both a preview and apply, including before staging.
#[cfg(unix)]
#[test]
fn linked_property_preflight_respects_dependent_file_permissions() {
    use std::os::unix::fs::PermissionsExt;
    let (directory, _, file, input) = linked_fixture();
    fs::set_permissions(&file, fs::Permissions::from_mode(0o444)).unwrap();
    let mut workspace = MetadataWorkspace::open(&directory.0, &[], false).unwrap();
    let project = workspace.project_mut(&ProjectScope::Standalone).unwrap();
    let id = object(MetadataKind::DocumentNumerator, "Shared", None);
    let state = project.property_editing(&id).unwrap();
    let preview = project.preview_property(
        &id,
        &state.properties.snapshot,
        state.context_snapshot.as_deref(),
        &field(&state.properties.fields, "NumberLength").path,
        &PropertyChange::Text { value: "30".into() },
    );
    assert!(matches!(
        preview,
        Err(PropertyEditError::Related {
            error: EditError::ReadOnly,
            ..
        })
    ));
    assert!(matches!(
        change(project, &id, &state, "NumberLength", "30"),
        Err(PropertyEditError::Related {
            error: EditError::ReadOnly,
            ..
        })
    ));
    fs::set_permissions(&file, fs::Permissions::from_mode(0o644)).unwrap();
    assert_eq!(fs::read_to_string(file).unwrap(), input);
    assert!(!directory.0.join(".eska/metadata/rename").exists());
}

/// A temporarily unreadable dependency disables linked replay without deleting its saved history.
#[test]
fn linked_history_survives_unavailable_dependency_context() {
    let (directory, _, file, _) = linked_fixture();
    let mut workspace = MetadataWorkspace::open(&directory.0, &[], false).unwrap();
    let project = workspace.project_mut(&ProjectScope::Standalone).unwrap();
    let id = object(MetadataKind::DocumentNumerator, "Shared", None);
    let state = project.property_editing(&id).unwrap();
    change(project, &id, &state, "NumberLength", "30").unwrap();
    let saved = fs::read(&file).unwrap();
    fs::write(&file, "broken").unwrap();
    let state = project.property_editing(&id).unwrap();
    assert!(state.context_snapshot.is_none());
    assert!(state.undo.is_none());
    fs::write(&file, saved).unwrap();
    let state = project.property_editing(&id).unwrap();
    assert_eq!(state.undo, Some(HistoryOperation::Linked));
    project
        .undo_property(
            &id,
            &state.properties.snapshot,
            state.context_snapshot.as_deref(),
            true,
        )
        .unwrap();
}
