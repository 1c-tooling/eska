//! Canonical headers use the verified source identity and never recover from unrelated native XML.

use std::fs;

use eska::project_core::source_format::{NativeUuid, SourceReader};

use super::{tests::fixture, *};

/// Both admission routes retain the same source UUID and the native serialized configuration shape.
#[test]
fn canonical_configuration_header_retains_exact_source_identity_across_reopen() {
    let (_directory, root) = fixture();
    let reader = SourceReader::open(&root).unwrap();
    let declaration = &reader.configuration().declarations[0];
    let uuid: bsl_metadata::Uuid = declaration.uuid.as_str().parse().unwrap();
    let name = declaration.name.clone();
    let header_path = root.join(reader.configuration().path.as_path());
    let saved = fs::read(&header_path).unwrap();
    for from_reader in [false, true, false] {
        let session = if from_reader {
            AnalysisSession::from_reader(
                SourceReader::open(&root).unwrap(),
                SessionOptions::default(),
            )
        } else {
            AnalysisSession::open(&root)
        }
        .unwrap();
        let configuration = session.database.configuration_for_root(&root).unwrap();
        assert_eq!(configuration.uuid(), &uuid);
        assert_eq!(configuration.name(), name);
        let serialized = serde_json::to_value(&*configuration).unwrap();
        assert_eq!(serialized["uuid"], uuid.to_string());
        assert_eq!(serialized["name"], name);
        assert!(serialized["metadataObjects"].as_array().unwrap().is_empty());
        assert!(serialized["commonModules"].as_array().unwrap().is_empty());
        let restored: Configuration = serde_json::from_value(serialized.clone()).unwrap();
        assert_eq!(restored.uuid(), &uuid);
        assert_eq!(serde_json::to_value(restored).unwrap(), serialized);
        assert_eq!(fs::read(&header_path).unwrap(), saved);
    }
}

/// A changed identity or malformed current header refuses reopening until the actual source is repaired.
#[test]
fn invalid_or_stale_source_header_cannot_recover_an_old_or_native_configuration_identity() {
    let (_directory, root) = fixture();
    let reader = SourceReader::open(&root).unwrap();
    let declaration = &reader.configuration().declarations[0];
    let uuid: bsl_metadata::Uuid = declaration.uuid.as_str().parse().unwrap();
    let header_path = root.join(reader.configuration().path.as_path());
    let saved = fs::read_to_string(&header_path).unwrap();
    let mut changed = reader.configuration().source.clone();
    changed.document.descriptor.object.uuid =
        NativeUuid::parse("90000000-0000-4000-8000-000000000099").unwrap();
    let stale_identity = changed.to_yaml().unwrap();
    let native = b"<MetaDataObject><Configuration uuid=\"90000000-0000-4000-8000-000000000098\"><Properties><Name>UnrelatedNative</Name></Properties></Configuration></MetaDataObject>";
    fs::write(root.join("Configuration.xml"), native).unwrap();
    for text in [&stale_identity, "not: [valid"] {
        fs::write(&header_path, text).unwrap();
        assert!(matches!(
            AnalysisSession::open(&root),
            Err(SessionError::Source(_))
        ));
        fs::write(&header_path, &saved).unwrap();
        let session = AnalysisSession::open(&root).unwrap();
        let configuration = session.database.configuration_for_root(&root).unwrap();
        assert_eq!(configuration.uuid(), &uuid);
        assert_eq!(configuration.name(), declaration.name);
        assert_eq!(fs::read(root.join("Configuration.xml")).unwrap(), native);
    }
    assert_eq!(fs::read_to_string(&header_path).unwrap(), saved);
}
