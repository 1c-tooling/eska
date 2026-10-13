//! File-based integration with the shared parser, adapter and real Salsa resolvers.

use std::{fs, path::PathBuf, sync::Arc};

use bsl_metadata::{AttributeType, RegisterPeriodicity};
use eska::{
    analysis_core::{Cancellation, InputFailure, InputOrigin},
    project_core::source_format::{SourceReader, import_designer},
};
use ide_db::metadata::config_index;

use super::*;

#[test]
/// Both direct and selected-reader entry points refuse an uncomposed extension before kernel setup.
fn extension_analysis_requires_an_admitted_base_context() {
    let (directory, source) = fixture();
    let native = directory.0.join("native");
    let path = native.join("Configuration.xml");
    let text = fs::read_to_string(&path).unwrap().replace(
        "<Properties>",
        "<Properties><ConfigurationExtensionPurpose>Customization</ConfigurationExtensionPurpose>",
    );
    fs::write(path, text).unwrap();
    let extension = directory.0.join("extension");
    import_designer(&native, &extension).unwrap();
    assert!(SourceReader::open(&source).is_ok());
    for selected in [false, true] {
        let opened = if selected {
            AnalysisSession::from_reader(
                SourceReader::open(&extension).unwrap(),
                SessionOptions::default(),
            )
        } else {
            AnalysisSession::open(&extension)
        };
        assert!(matches!(
            opened,
            Err(SessionError::UnsupportedProjectType(
                eska::project_core::ProjectType::Extension
            ))
        ));
    }
}

#[path = "../../../bsl-adapter/tests/support/mod.rs"]
pub mod support;

/// Keep fixtures exclusively inside the declared playground.
pub fn fixture() -> (support::TestDirectory, PathBuf) {
    let directory = support::TestDirectory::new();
    let native = directory.0.join("native");
    copy_tree(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/eska-format/designer"),
        &native,
    );
    // The generic round-trip fixture has deliberately untyped fields; this fixture tests inference.
    let catalog_path = native.join("Catalogs/Sample.xml");
    let catalog = fs::read_to_string(&catalog_path)
        .unwrap()
        .replace(
            "xmlns:v8=",
            "xmlns:xs=\"http://www.w3.org/2001/XMLSchema\" xmlns:v8=",
        )
        .replace(
            "<Name>Description</Name>",
            "<Name>Description</Name><Type><v8:Type>xs:string</v8:Type></Type>",
        )
        .replace(
            "<Name>Count</Name>",
            "<Name>Count</Name><Type><v8:Type>xs:decimal</v8:Type></Type>",
        );
    fs::write(catalog_path, catalog).unwrap();
    fs::write(native.join("Catalogs/Sample/Ext/ObjectModule.bsl"), "\u{feff}Процедура Проверка()\r\n\tОбъект = Справочники.Sample.СоздатьЭлемент();\r\n\tОбъект.Description = \"fixture\";\r\nКонецПроцедуры\r\n").unwrap();
    fs::write(
        native.join("Catalogs/Sample/Ext/ManagerModule.bin"),
        [0xff, 0xfe],
    )
    .unwrap();
    let mut configuration = fs::read_to_string(native.join("Configuration.xml")).unwrap();
    let extra = [
        (
            "Catalogs",
            "Catalog",
            "Other",
            "1",
            "<ChildObjects><Attribute uuid=\"30000000-0000-4000-8000-000000000009\"><Properties><Name>OtherField</Name><Type><v8:Type>xs:string</v8:Type></Type></Properties></Attribute></ChildObjects>",
        ),
        ("Constants", "Constant", "Flag", "2", ""),
        (
            "Enums",
            "Enum",
            "Choice",
            "3",
            "<ChildObjects><EnumValue uuid=\"30000000-0000-4000-8000-000000000008\"><Properties><Name>First</Name></Properties></EnumValue></ChildObjects>",
        ),
        ("DefinedTypes", "DefinedType", "TextType", "4", ""),
        (
            "InformationRegisters",
            "InformationRegister",
            "Rates",
            "5",
            "",
        ),
        ("Roles", "Role", "Reader", "6", ""),
    ];
    for (folder, kind, name, suffix, children) in extra {
        configuration = configuration.replacen(
            "</ChildObjects>",
            &format!("<{kind}>{name}</{kind}></ChildObjects>"),
            1,
        );
        fs::create_dir_all(native.join(folder)).unwrap();
        let properties = match kind {
            "Constant" | "DefinedType" => "<Type><v8:Type>xs:string</v8:Type></Type>",
            "InformationRegister" => {
                "<InformationRegisterPeriodicity>Year</InformationRegisterPeriodicity>"
            }
            _ => "",
        };
        let text = format!(
            "\u{feff}<MetaDataObject xmlns=\"http://v8.1c.ru/8.3/MDClasses\" xmlns:v8=\"http://v8.1c.ru/8.1/data/core\" xmlns:xs=\"http://www.w3.org/2001/XMLSchema\" version=\"2.20\"><{kind} uuid=\"a0000000-0000-4000-8000-00000000000{suffix}\"><Properties><Name>{name}</Name>{properties}</Properties>{children}</{kind}></MetaDataObject>\r\n"
        );
        fs::write(native.join(folder).join(format!("{name}.xml")), text).unwrap();
    }
    fs::write(native.join("Configuration.xml"), configuration).unwrap();
    let source = directory.0.join("source");
    import_designer(&native, &source).unwrap();
    (directory, source)
}

/// Keep an actually unmodeled family available after roles gain their own canonical adapter.
pub fn fixture_with_unmodeled_family() -> (support::TestDirectory, PathBuf) {
    let (directory, _) = fixture();
    let native = directory.0.join("native");
    let configuration = fs::read_to_string(native.join("Configuration.xml")).unwrap();
    fs::write(
        native.join("Configuration.xml"),
        configuration.replace(
            "<Role>Reader</Role>",
            "<CommonPicture>Reader</CommonPicture>",
        ),
    )
    .unwrap();
    let role = fs::read_to_string(native.join("Roles/Reader.xml")).unwrap();
    fs::create_dir(native.join("CommonPictures")).unwrap();
    fs::write(
        native.join("CommonPictures/Reader.xml"),
        role.replace("<Role ", "<CommonPicture ")
            .replace("</Role>", "</CommonPicture>"),
    )
    .unwrap();
    fs::remove_file(native.join("Roles/Reader.xml")).unwrap();
    let source = directory.0.join("source-with-picture");
    import_designer(&native, &source).unwrap();
    (directory, source)
}

/// Copy only the tiny public fixture, never an existing playground project.
fn copy_tree(source: &Path, destination: &Path) {
    fs::create_dir(destination).unwrap();
    for entry in fs::read_dir(source).unwrap() {
        let entry = entry.unwrap();
        if entry.file_type().unwrap().is_dir() {
            copy_tree(&entry.path(), &destination.join(entry.file_name()));
        } else {
            fs::copy(entry.path(), destination.join(entry.file_name())).unwrap();
        }
    }
}

/// Fully load metadata inputs; unsupported families still remain explicitly unsupported.
pub(super) fn load(session: &mut AnalysisSession) {
    let paths = session.owner_paths().cloned().collect::<Vec<_>>();
    for path in paths {
        session
            .load_saved(&path, session.revision(), &InputRequest::default())
            .unwrap();
    }
}

/// Keep the owner identity explicit in each scenario.
pub fn owner(value: &str) -> SourcePath {
    SourcePath::parse(value).unwrap()
}

/// Rename one declared attribute using its exact YAML coordinates.
pub fn draft(root: &Path, path: &SourcePath, field: &str, value: &str) -> String {
    let reader = SourceReader::open(root).unwrap();
    let parsed = reader.read_owner(path).unwrap();
    let range = parsed
        .declarations
        .iter()
        .find(|node| node.name == field)
        .unwrap()
        .name_range
        .clone();
    let mut text = fs::read_to_string(root.join(path.as_path())).unwrap();
    text.replace_range(range, value);
    text
}

/// Inspect a kernel-resolved object rather than only the adapter's cached status.
fn object(session: &AnalysisSession, path: &SourcePath) -> Arc<bsl_metadata::MetadataObject> {
    let Some(CanonicalMetadata::Object(value)) = session.resolve(path, session.revision()).unwrap()
    else {
        panic!("expected object, current state: {:?}", session.owner(path));
    };
    value
}

/// Opening composes stable identities without reading missing owners or stray native XML.
#[test]
fn opening_is_lazy_and_resolves_only_declared_slots() {
    let (_directory, root) = fixture();
    let path = owner("Catalogs/Sample/object.yaml");
    fs::remove_file(root.join(path.as_path())).unwrap();
    fs::write(
        root.join("Catalogs/Sample.xml"),
        "untrusted native fallback",
    )
    .unwrap();
    let mut session = AnalysisSession::open(&root).unwrap();
    assert_eq!(
        session.native_path(&path).unwrap().as_str(),
        "Catalogs/Sample.xml"
    );
    assert_eq!(session.summary().pending, 9);
    assert!(
        session
            .resolve(&path, session.revision())
            .unwrap()
            .is_none()
    );
    session
        .load_saved(&path, session.revision(), &InputRequest::default())
        .unwrap();
    assert!(matches!(
        session.owner(&path).unwrap().state,
        ModelState::Unavailable(_)
    ));
    assert!(
        session
            .resolve(&path, session.revision())
            .unwrap()
            .is_none()
    );
}

/// All main adapters reach canonical resolvers; nested descriptors still retain explicit coverage.
#[test]
fn supported_main_models_and_nested_coverage_stay_distinct() {
    let (_directory, root) = fixture();
    let mut session = AnalysisSession::open(&root).unwrap();
    load(&mut session);
    let constant = object(&session, &owner("Constants/Flag/object.yaml"));
    assert_eq!(
        constant.constant_type,
        Some(AttributeType::String { length: None })
    );
    let enumeration = object(&session, &owner("Enums/Choice/object.yaml"));
    assert_eq!(enumeration.enum_values[0].name, "First");
    let Some(CanonicalMetadata::Register(register)) = session
        .resolve(
            &owner("InformationRegisters/Rates/object.yaml"),
            session.revision(),
        )
        .unwrap()
    else {
        panic!("register projection");
    };
    assert_eq!(register.periodicity(), Some(RegisterPeriodicity::Year));
    assert_eq!(register.virtual_tables(), ["СрезПервых", "СрезПоследних"]);
    assert!(matches!(
        session
            .resolve(
                &owner("DefinedTypes/TextType/object.yaml"),
                session.revision()
            )
            .unwrap(),
        Some(CanonicalMetadata::DefinedType(_))
    ));
    assert!(matches!(
        session
            .resolve(
                &owner("CommonModules/Tools/object.yaml"),
                session.revision()
            )
            .unwrap(),
        Some(CanonicalMetadata::CommonModule(_))
    ));
    let role = owner("Roles/Reader/object.yaml");
    assert!(matches!(
        session.owner(&role).unwrap().state,
        ModelState::Ready(_)
    ));
    assert!(matches!(
        session.resolve(&role, session.revision()).unwrap(),
        Some(CanonicalMetadata::Role(_))
    ));
    let summary = session.summary();
    assert!(summary.inputs.metadata_input_complete);
    assert_eq!(summary.ready_models, 8);
    assert_eq!(summary.unsupported, 0);
    assert!(summary.main_models_complete);
    assert!(!summary.descriptor_models_complete);
    assert!(summary.descriptors > summary.modeled_descriptors);
}

/// Drafts change current names and preserve both retained old models and saved bytes.
#[test]
fn saved_draft_invalid_and_close_use_one_input_path() {
    let (_directory, root) = fixture();
    let mut session = AnalysisSession::open(&root).unwrap();
    let path = owner("Catalogs/Sample/object.yaml");
    let saved_bytes = fs::read(root.join(path.as_path())).unwrap();
    session
        .load_saved(&path, session.revision(), &InputRequest::default())
        .unwrap();
    let saved = object(&session, &path);
    let text = draft(&root, &path, "Description", "LiveField");
    session
        .apply_overlay(
            &path,
            session.revision(),
            1,
            &text,
            &InputRequest::default(),
        )
        .unwrap();
    assert!(
        object(&session, &path)
            .attributes
            .iter()
            .any(|field| field.name == "LiveField")
    );
    assert!(
        saved
            .attributes
            .iter()
            .any(|field| field.name == "Description")
    );
    assert_eq!(
        session.owner(&path).unwrap().origin,
        InputOrigin::Overlay {
            document_version: 1
        }
    );
    assert_eq!(fs::read(root.join(path.as_path())).unwrap(), saved_bytes);
    session
        .apply_overlay(
            &path,
            session.revision(),
            2,
            "broken: [",
            &InputRequest::default(),
        )
        .unwrap();
    assert!(
        session
            .resolve(&path, session.revision())
            .unwrap()
            .is_none()
    );
    session
        .close_overlay(&path, session.revision(), &InputRequest::default())
        .unwrap();
    assert_eq!(*object(&session, &path), *saved);
    assert_eq!(fs::read(root.join(path.as_path())).unwrap(), saved_bytes);
}

/// Adapter errors refer to current YAML; a syntactically valid unsupported type is not ready.
#[test]
fn adapter_failure_and_rejected_requests_never_restore_an_old_value() {
    let (_directory, root) = fixture();
    let mut session = AnalysisSession::open(&root).unwrap();
    let path = owner("Catalogs/Other/object.yaml");
    session
        .load_saved(&path, session.revision(), &InputRequest::default())
        .unwrap();
    let stale = session.revision();
    let text = fs::read_to_string(root.join(path.as_path()))
        .unwrap()
        .replace("xs:string", "xs:UnknownType");
    session
        .apply_overlay(&path, stale, 1, &text, &InputRequest::default())
        .unwrap();
    let ModelState::Invalid { error, .. } = &session.owner(&path).unwrap().state else {
        panic!("adapter error");
    };
    assert!(text[error.range.clone()].contains("UnknownType"));
    let current = session.revision();
    assert!(session.resolve(&path, current).unwrap().is_none());
    assert!(matches!(
        session.resolve(&path, stale),
        Err(SessionRequestError::StaleRevision { .. })
    ));
    assert!(matches!(
        session.apply_overlay(&path, current, 1, &text, &InputRequest::default()),
        Err(SessionRequestError::Metadata(
            UpdateError::StaleDocumentVersion { .. }
        ))
    ));
    assert!(matches!(
        session.load_saved(&path, current, &InputRequest::default()),
        Err(SessionRequestError::Metadata(
            UpdateError::OverlayActive { .. }
        ))
    ));
    assert_eq!(session.revision(), current);
    assert!(session.resolve(&path, current).unwrap().is_none());
}

/// Cancelled or bounded-out accepted reads remove canonical values, including after disk refresh.
#[test]
fn cancellation_and_limits_clear_the_current_kernel_model() {
    let (_directory, root) = fixture();
    let mut session = AnalysisSession::open(&root).unwrap();
    let path = owner("Catalogs/Other/object.yaml");
    session
        .load_saved(&path, session.revision(), &InputRequest::default())
        .unwrap();
    let cancellation = Cancellation::default();
    cancellation.cancel();
    let request = InputRequest {
        max_document_bytes: 1,
        cancellation,
        ..InputRequest::default()
    };
    session
        .load_saved(&path, session.revision(), &request)
        .unwrap();
    assert!(matches!(
        session.owner(&path).unwrap().state,
        ModelState::Unavailable(InputFailure::Cancelled)
    ));
    assert!(
        session
            .resolve(&path, session.revision())
            .unwrap()
            .is_none()
    );
    let request = InputRequest {
        max_document_bytes: 1,
        cancellation: Cancellation::default(),
        ..InputRequest::default()
    };
    session
        .load_saved(&path, session.revision(), &request)
        .unwrap();
    assert!(matches!(
        session.owner(&path).unwrap().state,
        ModelState::Unavailable(InputFailure::DocumentTooLarge { limit: 1 })
    ));
    assert!(
        session
            .resolve(&path, session.revision())
            .unwrap()
            .is_none()
    );
}

/// Execution counters demonstrate that changing one owner does not rerun its neighbor's resolver.
#[test]
fn unrelated_owner_changes_preserve_the_index_and_neighbor_query() {
    let (_directory, root) = fixture();
    let mut session =
        AnalysisSession::with_database(&root, RootDatabaseImpl::new_with_salsa_events()).unwrap();
    let path = owner("Catalogs/Sample/object.yaml");
    let other = owner("Catalogs/Other/object.yaml");
    session
        .load_saved(&path, session.revision(), &InputRequest::default())
        .unwrap();
    session
        .load_saved(&other, session.revision(), &InputRequest::default())
        .unwrap();
    let neighbor = object(&session, &other);
    let index = Arc::clone(config_index(&session.database, session.listing));
    let count = session
        .database
        .salsa_family_churn("resolve_metadata_object")
        .unwrap()
        .0;
    let text = draft(&root, &path, "Description", "LiveField");
    session
        .apply_overlay(
            &path,
            session.revision(),
            1,
            &text,
            &InputRequest::default(),
        )
        .unwrap();
    assert!(Arc::ptr_eq(&neighbor, &object(&session, &other)));
    assert!(Arc::ptr_eq(
        &index,
        config_index(&session.database, session.listing)
    ));
    assert_eq!(
        session
            .database
            .salsa_family_churn("resolve_metadata_object")
            .unwrap()
            .0,
        count
    );
    assert!(
        object(&session, &path)
            .attributes
            .iter()
            .any(|field| field.name == "LiveField")
    );
    assert_eq!(
        session
            .database
            .salsa_family_churn("parse_mdo_query")
            .unwrap()
            .0,
        0
    );
}

/// IDE metadata remains available for a valid unsupported family and shares current draft failure.
#[test]
fn metadata_views_share_current_snapshots_without_requiring_bsl_projection() {
    let (_directory, root) = fixture_with_unmodeled_family();
    let mut session = AnalysisSession::open(&root).unwrap();
    let path = owner("CommonPictures/Reader/object.yaml");
    session
        .load_saved(&path, session.revision(), &InputRequest::default())
        .unwrap();
    let saved = session.metadata(&path, session.revision()).unwrap();
    let eska::analysis_core::InputState::Ready(snapshot) = &saved.input else {
        panic!("readable picture metadata");
    };
    assert_eq!(
        snapshot.documents[0].descriptor.objects[0].metadata.name(),
        "Reader"
    );
    assert!(matches!(saved.model.state, ModelState::Unsupported(_)));
    let text = fs::read_to_string(root.join(path.as_path()))
        .unwrap()
        .replace('\n', "\r\n");
    session
        .apply_overlay(
            &path,
            session.revision(),
            1,
            &text,
            &InputRequest::default(),
        )
        .unwrap();
    let overlay = session.metadata(&path, session.revision()).unwrap();
    assert!(matches!(
        overlay.origin,
        InputOrigin::Overlay {
            document_version: 1
        }
    ));
    assert_eq!(overlay.source_sha256, overlay.model.source_sha256);
    assert!(matches!(
        overlay.input,
        eska::analysis_core::InputState::Ready(_)
    ));
    assert!(matches!(
        session.metadata(&path, saved.revision),
        Err(SessionRequestError::StaleRevision { .. })
    ));
    session
        .apply_overlay(
            &path,
            session.revision(),
            2,
            "broken: [",
            &InputRequest::default(),
        )
        .unwrap();
    let invalid = session.metadata(&path, session.revision()).unwrap();
    assert!(matches!(
        invalid.input,
        eska::analysis_core::InputState::Unavailable(_)
    ));
    assert!(matches!(invalid.model.state, ModelState::Unavailable(_)));
    assert!(matches!(
        saved.input,
        eska::analysis_core::InputState::Ready(_)
    ));
}
