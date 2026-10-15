//! Current YAML coordinates survive transport-free queries without a second source authority.

use std::fs;

use eska::{
    analysis_core::InputRequest,
    project_core::source_format::{MD_NAMESPACE, ObjectSource, Property, Section, TextPosition},
};

use super::*;
use crate::{ModelState, session::tests::fixture};

/// Independently count a fixture prefix; these fixtures use LF or CRLF rather than lone CR.
fn position(text: &str, byte: usize) -> TextPosition {
    let prefix = &text[..byte];
    let start = prefix.rfind('\n').map_or(0, |index| index + 1);
    TextPosition {
        line: u32::try_from(prefix.bytes().filter(|&value| value == b'\n').count()).unwrap(),
        character: u32::try_from(prefix[start..].encode_utf16().count()).unwrap(),
    }
}

/// Inline and external objects share their physical owner; unsaved bytes replace saved coordinates.
#[test]
fn metadata_locations_follow_current_owner_text_and_never_read_changed_disk() {
    let (_directory, root) = fixture();
    let path = SourcePath::parse("Catalogs/Sample/object.yaml").unwrap();
    let uuid = NativeUuid::parse("50000000-0000-4000-8000-000000000001").unwrap();
    let key = PropertyKey {
        namespace: Some(MD_NAMESPACE.into()),
        name: "Name".into(),
    };
    let saved = fs::read_to_string(root.join(path.as_path())).unwrap();
    let mut session = AnalysisSession::open(&root).unwrap();
    assert_eq!(
        session.metadata_location(&path, session.revision(), &uuid, None),
        Err(MetadataLocationError::Pending)
    );
    session
        .load_saved(&path, session.revision(), &InputRequest::default())
        .unwrap();
    let revision = session.revision();
    let before = session.resources();
    let object = session
        .metadata_location(&path, revision, &uuid, None)
        .unwrap();
    let name = session
        .metadata_location(&path, revision, &uuid, Some(&key))
        .unwrap();
    assert!(saved[object.byte_range].contains(uuid.as_str()));
    assert!(saved[name.byte_range.clone()].contains("Count"));
    assert_eq!(name.range.start, position(&saved, name.byte_range.start));
    assert_eq!(name.range.end, position(&saved, name.byte_range.end));
    assert_eq!(name.native_path.as_str(), "Catalogs/Sample.xml");
    let form = NativeUuid::parse("70000000-0000-4000-8000-000000000001").unwrap();
    let form = session
        .metadata_location(&path, revision, &form, None)
        .unwrap();
    assert_eq!(form.source, path);
    assert_eq!(form.native_path.as_str(), "Catalogs/Sample/Forms/Main.xml");
    assert!(saved[form.byte_range].contains("Main"));
    assert_eq!(session.resources(), before);
    let draft = format!(
        "# Count and Name are only comment text Я😀\r\n{}",
        saved.replace('\n', "\r\n")
    );
    session
        .apply_overlay(&path, revision, 7, &draft, &InputRequest::default())
        .unwrap();
    let editor = session
        .metadata_location(&path, session.revision(), &uuid, Some(&key))
        .unwrap();
    assert_eq!(
        editor.range.start,
        position(&draft, editor.byte_range.start)
    );
    assert_eq!(editor.range.end, position(&draft, editor.byte_range.end));
    assert_ne!(editor.source_sha256, name.source_sha256);
    assert_eq!(
        editor.origin,
        InputOrigin::Overlay {
            document_version: 7
        }
    );
    assert!(matches!(
        session.metadata_location(&path, revision, &uuid, None),
        Err(MetadataLocationError::Request(_))
    ));
    fs::remove_file(root.join(path.as_path())).unwrap();
    assert_eq!(
        session
            .metadata_location(&path, session.revision(), &uuid, Some(&key))
            .unwrap(),
        editor
    );
    session
        .apply_overlay(
            &path,
            session.revision(),
            8,
            "broken: [\n",
            &InputRequest::default(),
        )
        .unwrap();
    assert!(matches!(
        session.metadata_location(&path, session.revision(), &uuid, None),
        Err(MetadataLocationError::Unavailable(_))
    ));
    fs::write(root.join(path.as_path()), &saved).unwrap();
    session
        .close_overlay(&path, session.revision(), &InputRequest::default())
        .unwrap();
    let restored = session
        .metadata_location(&path, session.revision(), &uuid, Some(&key))
        .unwrap();
    assert_eq!(restored.range, name.range);
    assert_eq!(restored.byte_range, name.byte_range);
    assert_eq!(restored.source_sha256, name.source_sha256);
    assert_eq!(restored.origin, InputOrigin::Saved);
}

/// Unsupported BSL families keep locations; duplicate properties invalidate the current input.
#[test]
fn metadata_locations_keep_unsupported_families_and_reject_invalid_current_properties() {
    let (_directory, root) = crate::session::tests::fixture_with_unmodeled_family();
    let mut session = AnalysisSession::open(&root).unwrap();
    let role = SourcePath::parse("CommonAttributes/Reader/object.yaml").unwrap();
    session
        .load_saved(&role, session.revision(), &InputRequest::default())
        .unwrap();
    assert!(matches!(
        session.owner(&role).unwrap().state,
        ModelState::Unsupported(_)
    ));
    let uuid = NativeUuid::parse("a0000000-0000-4000-8000-000000000006").unwrap();
    assert!(
        session
            .metadata_location(&role, session.revision(), &uuid, None)
            .is_ok()
    );
    let foreign = PropertyKey {
        namespace: Some("urn:foreign".into()),
        name: "Name".into(),
    };
    assert_eq!(
        session.metadata_location(&role, session.revision(), &uuid, Some(&foreign)),
        Err(MetadataLocationError::PropertyNotFound)
    );
    let missing = NativeUuid::parse("00000000-0000-4000-8000-000000000000").unwrap();
    assert_eq!(
        session.metadata_location(&role, session.revision(), &missing, None),
        Err(MetadataLocationError::ObjectNotFound)
    );
    let path = SourcePath::parse("CommonModules/Tools/object.yaml").unwrap();
    let mut owner =
        ObjectSource::from_yaml(&fs::read_to_string(root.join(path.as_path())).unwrap()).unwrap();
    for section in &mut owner.document.descriptor.object.sections {
        if let Section::Properties(properties) = section {
            let server = properties.iter().find(|property| matches!(property, Property::Typed { name, .. } if name.as_str() == "Server")).unwrap().clone();
            properties.push(server);
        }
    }
    // JSON is flow-style YAML; bypass the writer's validation to send a malformed editor input.
    let draft = serde_json::to_string(&owner).unwrap();
    session
        .apply_overlay(
            &path,
            session.revision(),
            1,
            &draft,
            &InputRequest::default(),
        )
        .unwrap();
    let uuid = NativeUuid::parse("60000000-0000-4000-8000-000000000001").unwrap();
    let key = PropertyKey {
        namespace: Some(MD_NAMESPACE.into()),
        name: "Server".into(),
    };
    assert!(matches!(
        session.metadata_location(&path, session.revision(), &uuid, Some(&key)),
        Err(MetadataLocationError::Unavailable(InputFailure::Source(error)))
            if error.code == "source_property_duplicate"
    ));
    assert!(matches!(
        session.metadata_location(&path, session.revision(), &uuid, None),
        Err(MetadataLocationError::Unavailable(_))
    ));
}
