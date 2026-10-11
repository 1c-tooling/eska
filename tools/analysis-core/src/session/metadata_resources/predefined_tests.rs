//! Declared reader sidecars do not become versioned inputs before a canonical consumer is connected.

use std::fs;

use ide_db::metadata::CanonicalMetadata;

use super::*;
use crate::session::tests::{owner, support::TestDirectory};

/// The reader's structural API alone cannot activate an unprojected resource or a disk fallback.
#[test]
fn predefined_reader_does_not_activate_unconnected_session_inputs() {
    let directory = TestDirectory::new();
    let project = crate::test_project::project(&directory.0, "// Current shared body.\n", false);
    let native = directory.0.join("native");
    let configuration = fs::read_to_string(native.join("Configuration.xml")).unwrap();
    fs::write(
        native.join("Configuration.xml"),
        configuration.replace("</ChildObjects>", "<Catalog>Items</Catalog></ChildObjects>"),
    )
    .unwrap();
    fs::create_dir_all(native.join("Catalogs/Items/Ext")).unwrap();
    fs::write(native.join("Catalogs/Items.xml"), "<MetaDataObject xmlns=\"http://v8.1c.ru/8.3/MDClasses\" version=\"2.20\"><Catalog uuid=\"90000000-0000-4000-8000-000000000001\"><Properties><Name>Items</Name></Properties></Catalog></MetaDataObject>").unwrap();
    let resource = owner("Catalogs/Items/Ext/Predefined.xml");
    fs::write(native.join(resource.as_path()), "<PredefinedData><Item id=\"90000000-0000-4000-8000-000000000002\"><Name>Saved</Name></Item></PredefinedData>").unwrap();
    let source = project.join("source-with-predefined");
    eska::project_core::source_format::import_designer(&native, &source).unwrap();
    let reader = SourceReader::open(&source).unwrap();
    assert_eq!(reader.predefined_paths().count(), 1);
    assert_eq!(
        reader.predefined_owner(&resource),
        Some(&owner("Catalogs/Items/object.yaml"))
    );
    let mut session =
        AnalysisSession::from_reader(reader, crate::SessionOptions::default()).unwrap();
    let request = InputRequest::default();
    session
        .load_semantic_inputs(session.revision(), &request)
        .unwrap();
    assert_eq!(session.resource_paths().count(), 0);
    assert!(session.metadata_resource(&resource).is_none());
    let revision = session.revision();
    let resources = session.resources();
    assert_eq!(
        session.apply_resource_overlay(&resource, revision, 1, "<PredefinedData/>", &request),
        Err(SessionRequestError::UnknownMetadataResource(resource))
    );
    assert_eq!(session.revision(), revision);
    assert_eq!(session.resources(), resources);
    let Some(CanonicalMetadata::Object(model)) = session
        .resolve(&owner("Catalogs/Items/object.yaml"), revision)
        .unwrap()
    else {
        panic!("current catalog model")
    };
    assert!(model.predefined_items.is_empty());
}
