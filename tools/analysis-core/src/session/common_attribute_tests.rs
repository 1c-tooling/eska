//! Explicit current common membership reaches native fields without exposing hidden or stale contributions.

use super::*;
use crate::{
    QueryCancellation,
    session::tests::{owner, support::TestDirectory},
};
use std::{
    fs,
    path::{Path, PathBuf},
};

const MODULE: &str = "CommonModules/Tools/Ext/Module.bsl";
const ATTRIBUTE: &str = "CommonAttributes/Shared/object.yaml";
const CATALOG: &str = "Catalogs/Items/object.yaml";
const BODY: &str =
    "Procedure Run()\n Item = Catalogs.Items.CreateItem();\n Item.Shared;\nEndProcedure\n";

/// Each source tree is imported from this test's own native descriptors before opening a session.
fn project(directory: &Path, extension: bool, shared_uuid: &str, extra: &str) -> PathBuf {
    let root = crate::test_project::project(directory, BODY, false);
    let native = directory.join("native");
    let header = fs::read_to_string(native.join("Configuration.xml")).unwrap();
    let children = if extension {
        "<CommonAttribute>Shared</CommonAttribute>"
    } else {
        "<Catalog>Items</Catalog><CommonAttribute>Shared</CommonAttribute>"
    };
    let header = header.replace("</ChildObjects>", &format!("{children}</ChildObjects>"));
    let header = if extension {
        header.replace("<Name>SyntaxFixture</Name>", "<Name>Extension</Name><ObjectBelonging>Adopted</ObjectBelonging><ConfigurationExtensionPurpose>Patch</ConfigurationExtensionPurpose>")
            .replace("10000000-0000-4000-8000-000000000001", &shared_uuid.replacen("70000000", "10000000", 1))
    } else {
        header
    };
    fs::write(native.join("Configuration.xml"), header).unwrap();
    if extension {
        let path = native.join("CommonModules/Tools.xml");
        let text = fs::read_to_string(&path).unwrap().replace(
            "20000000-0000-4000-8000-000000000001",
            &shared_uuid.replacen("70000000", "20000000", 1),
        );
        fs::write(path, text).unwrap();
    }
    if !extension {
        fs::create_dir(native.join("Catalogs")).unwrap();
        fs::write(native.join("Catalogs/Items.xml"), "<MetaDataObject xmlns=\"http://v8.1c.ru/8.3/MDClasses\" version=\"2.20\"><Catalog uuid=\"80000000-0000-4000-8000-000000000001\"><Properties><Name>Items</Name></Properties></Catalog></MetaDataObject>").unwrap();
    }
    fs::create_dir(native.join("CommonAttributes")).unwrap();
    fs::write(native.join("CommonAttributes/Shared.xml"), format!(r#"<MetaDataObject xmlns="http://v8.1c.ru/8.3/MDClasses" xmlns:r="http://v8.1c.ru/8.3/xcf/readable" xmlns:t="http://v8.1c.ru/8.1/data/core" xmlns:x="http://www.w3.org/2001/XMLSchema" version="2.20"><CommonAttribute uuid="{shared_uuid}"><Properties><Name>Shared</Name><Type><t:Type>x:string</t:Type></Type><AutoUse>DontUse</AutoUse><DataSeparation>DontUse</DataSeparation><Content><r:Item><r:Metadata>Catalog.Items</r:Metadata><r:Use>Use</r:Use></r:Item></Content>{extra}</Properties></CommonAttribute></MetaDataObject>"#)).unwrap();
    let source = root.join("src-with-common-attributes");
    eska::project_core::source_format::import_designer(&native, &source).unwrap();
    let config = fs::read_to_string(root.join("eska.toml")).unwrap();
    let config = config.replace(
        "source_format='eska'",
        "source_format='eska'\nsource='src-with-common-attributes'",
    );
    fs::write(
        root.join("eska.toml"),
        if extension {
            config.replace("type='configuration'", "type='extension'")
        } else {
            config
        },
    )
    .unwrap();
    root
}

/// Per-file native resolution uses the selected module's real registered descriptor context.
fn object(session: &AnalysisSession) -> Option<Arc<bsl_metadata::MetadataObject>> {
    let module = session.selected_root.modules.get(&owner(MODULE)).unwrap();
    session.database.resolve_metadata_object_for_file(
        module.file,
        bsl_metadata::MdoType::Catalog,
        "Items",
    )
}

/// Exact current types are checked through the native model rather than the adapter as a second oracle.
fn field_type(session: &AnalysisSession) -> Option<bsl_metadata::AttributeType> {
    object(session)?
        .find_attribute("Shared")
        .map(|field| field.attr_type.clone())
}

/// Keep the native source, current source and instrumented database owned by one small standalone fixture.
fn standalone() -> (TestDirectory, PathBuf, AnalysisSession) {
    let directory = TestDirectory::new();
    let root = project(
        &directory.0,
        false,
        "70000000-0000-4000-8000-000000000001",
        "",
    );
    let source = root.join("src-with-common-attributes");
    let session =
        AnalysisSession::with_database(&source, RootDatabaseImpl::new_with_salsa_events()).unwrap();
    (directory, source, session)
}

/// Cold common-family preparation augments a ready object, and current failure cannot restore saved fields.
#[test]
fn current_common_attribute_type_membership_failure_and_close_reach_native_fields() {
    let (_directory, source, mut session) = standalone();
    let request = InputRequest::default();
    let module = owner(MODULE);
    let offset = u32::try_from(BODY.find("Shared").unwrap()).unwrap();
    session
        .load_module_saved(&module, session.revision(), &request)
        .unwrap();
    session
        .load_saved(&owner(CATALOG), session.revision(), &request)
        .unwrap();
    assert!(matches!(
        session.owner(&owner(ATTRIBUTE)).unwrap().state,
        ModelState::Pending
    ));
    session
        .prepare_completion_metadata(
            &module,
            session.revision(),
            offset,
            &request,
            &QueryCancellation::default(),
        )
        .unwrap();
    assert!(matches!(
        session.owner(&owner(CATALOG)).unwrap().state,
        ModelState::Ready(_)
    ));
    assert_eq!(
        field_type(&session),
        Some(bsl_metadata::AttributeType::String { length: None })
    );
    for locale in [crate::Locale::Ru, crate::Locale::En] {
        let result = session
            .module_completions(
                &module,
                session.revision(),
                offset,
                locale,
                &QueryCancellation::default(),
            )
            .unwrap();
        assert!(result.value.iter().any(|item| item.label == "Shared"));
    }
    let common = owner(ATTRIBUTE);
    let saved = fs::read_to_string(source.join(common.as_path())).unwrap();
    let before = fs::read(source.join(common.as_path())).unwrap();
    let changed = saved.replace("x:string", "x:boolean");
    session
        .apply_overlay(&common, session.revision(), 1, &changed, &request)
        .unwrap();
    assert_eq!(
        field_type(&session),
        Some(bsl_metadata::AttributeType::Boolean)
    );
    assert_eq!(
        session
            .invalidate_saved_inputs(std::slice::from_ref(&common), session.revision())
            .unwrap(),
        []
    );
    session
        .apply_overlay(&common, session.revision(), 2, "broken: [", &request)
        .unwrap();
    assert!(object(&session).is_none());
    assert!(matches!(
        session.prepare_completion_metadata(
            &module,
            session.revision(),
            offset,
            &request,
            &QueryCancellation::default()
        ),
        Err(crate::QueryError::Request(
            SessionRequestError::CompositionInputIncomplete(_)
        ))
    ));
    session
        .apply_overlay(
            &common,
            session.revision(),
            3,
            &saved.replace("<r:Use>Use</r:Use>", "<r:Use>DontUse</r:Use>"),
            &request,
        )
        .unwrap();
    assert!(object(&session).is_some());
    assert!(field_type(&session).is_none());
    session
        .close_overlay(&common, session.revision(), &request)
        .unwrap();
    assert_eq!(
        field_type(&session),
        Some(bsl_metadata::AttributeType::String { length: None })
    );
    assert_eq!(fs::read(source.join(common.as_path())).unwrap(), before);
}

/// Invalid ordinary controls remain a current model error with source coordinates and no inferred membership.
#[test]
fn unsupported_common_attribute_controls_do_not_grant_descriptor_or_field_completeness() {
    let directory = TestDirectory::new();
    let root = project(
        &directory.0,
        false,
        "70000000-0000-4000-8000-000000000001",
        "",
    );
    let source = root.join("src-with-common-attributes");
    let mut session = AnalysisSession::open(&source).unwrap();
    let request = InputRequest::default();
    session
        .load_semantic_inputs(session.revision(), &request)
        .unwrap();
    let common = owner(ATTRIBUTE);
    let saved = fs::read_to_string(source.join(common.as_path())).unwrap();
    for (index, text) in [
        saved.replace("<AutoUse>DontUse</AutoUse>", "<AutoUse>Use</AutoUse>"),
        saved.replace(
            "<DataSeparation>DontUse</DataSeparation>",
            "<DataSeparation>Use</DataSeparation>",
        ),
    ]
    .iter()
    .enumerate()
    {
        session
            .apply_overlay(
                &common,
                session.revision(),
                i64::try_from(index + 1).unwrap(),
                text,
                &request,
            )
            .unwrap();
        assert!(matches!(
            session.owner(&common).unwrap().state,
            ModelState::Invalid { .. }
        ));
        assert!(
            session
                .resolve(&common, session.revision())
                .unwrap()
                .is_none()
        );
        assert!(object(&session).is_none());
        assert!(!session.summary().descriptor_models_complete);
    }
}

/// Admit the three already imported source roots using the real configured visibility contract.
#[cfg(any(feature = "analyzer-cli", feature = "check-cli"))]
fn open_composition(base: &Path, selected: &Path, sibling: &Path) -> AnalysisSession {
    use crate::composition::admit;
    use eska::project_core::source_format::SourceReader;
    let source = |root: &Path| root.join("src-with-common-attributes");
    let config = selected.join("bsl-analyzer.toml");
    fs::write(&config, "[source]\nroot='../../base/project/src-with-common-attributes'\nextensions=[{name='Selected',path='src-with-common-attributes'},{name='Sibling',path='../../sibling/project/src-with-common-attributes'}]\n").unwrap();
    let settings = project_model::ProjectConfig::load_from_file(&config).unwrap();
    let readers = [base, selected, sibling]
        .map(|root| SourceReader::open(&source(root)).unwrap())
        .into_iter()
        .collect();
    let topology = admit(selected, &settings, readers, &source(selected)).unwrap();
    AnalysisSession::from_composition(topology, crate::SessionOptions::default()).unwrap()
}

/// Selected adoption composes explicit membership; hidden roots and failed base declarations cannot supply a substitute.
#[cfg(any(feature = "analyzer-cli", feature = "check-cli"))]
#[test]
fn common_attribute_selected_cfe_uses_current_base_and_adopted_targets_without_hidden_siblings() {
    use crate::composition::SourceKey;

    let directory = TestDirectory::new();
    let base = project(
        &directory.0.join("base"),
        false,
        "70000000-0000-4000-8000-000000000001",
        "",
    );
    let selected = project(
        &directory.0.join("selected"),
        true,
        "70000000-0000-4000-8000-000000000002",
        "<ObjectBelonging>Adopted</ObjectBelonging><ExtendedConfigurationObject>70000000-0000-4000-8000-000000000001</ExtendedConfigurationObject>",
    );
    let sibling = project(
        &directory.0.join("sibling"),
        true,
        "70000000-0000-4000-8000-000000000003",
        "",
    );
    let source = |root: &Path| root.join("src-with-common-attributes");
    let common = owner(ATTRIBUTE);
    let selected_path = source(&selected).join(common.as_path());
    let selected_text = fs::read_to_string(&selected_path)
        .unwrap()
        .replace("x:string", "x:boolean");
    fs::write(&selected_path, &selected_text).unwrap();
    let mut session = open_composition(&base, &selected, &sibling);
    let request = InputRequest::default();
    session
        .load_semantic_inputs(session.revision(), &request)
        .unwrap();
    assert_eq!(
        field_type(&session),
        Some(bsl_metadata::AttributeType::Boolean)
    );
    let root = session.root_ids().find(|id| id.index() == 0).unwrap();
    let hidden = session.root_ids().find(|id| id.index() == 2).unwrap();
    assert!(matches!(
        session.root_inputs(hidden).unwrap().owners[&common]
            .model
            .state,
        ModelState::Pending
    ));
    let key = SourceKey {
        root,
        source: common.clone(),
    };
    let base_text = fs::read_to_string(source(&base).join(common.as_path())).unwrap();
    session
        .apply_overlay_at(&key, session.revision(), 1, "broken: [", &request)
        .unwrap();
    assert!(object(&session).is_none());
    session
        .apply_overlay_at(&key, session.revision(), 2, &base_text, &request)
        .unwrap();
    assert_eq!(
        field_type(&session),
        Some(bsl_metadata::AttributeType::Boolean)
    );
    session
        .apply_overlay(
            &common,
            session.revision(),
            1,
            &selected_text.replace("<r:Use>Use</r:Use>", "<r:Use>DontUse</r:Use>"),
            &request,
        )
        .unwrap();
    assert!(object(&session).is_some());
    assert!(field_type(&session).is_none());
    session
        .close_overlay(&common, session.revision(), &request)
        .unwrap();
    assert_eq!(
        field_type(&session),
        Some(bsl_metadata::AttributeType::Boolean)
    );
    let wrong = selected_text.replace(
        "<ExtendedConfigurationObject>70000000",
        "<ExtendedConfigurationObject>90000000",
    );
    session
        .apply_overlay(&common, session.revision(), 2, &wrong, &request)
        .unwrap();
    assert!(object(&session).is_none());
    assert!(matches!(
        session.require_common_attribute_membership(
            session.selected_root.modules[&owner(MODULE)].file
        ),
        Err(crate::QueryError::Request(
            SessionRequestError::CompositionInputIncomplete(_)
        ))
    ));
    assert_eq!(fs::read_to_string(selected_path).unwrap(), selected_text);
    assert_eq!(
        fs::read_to_string(source(&base).join(common.as_path())).unwrap(),
        base_text
    );
}
