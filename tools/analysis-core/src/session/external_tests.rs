//! Canonical external scopes supply real current self/form types and retain absent-base incompleteness.

use std::fs;

use eska::{
    analysis_core::{InputRequest, MetadataInputs},
    project_core::ProjectType,
};
use hir::DefDatabase;
use ide_db::{
    RootDatabaseImpl,
    metadata::{CanonicalMetadata, RootKind},
};

use super::{tests::support::TestDirectory, *};
use crate::{Locale, QueryCancellation, QueryError};

#[path = "../../tests/support/external_project.rs"]
mod fixture;

/// Own and form module consumers receive external metadata without a base, fake Configuration or disk XML.
#[test]
fn canonical_external_owner_and_form_have_real_types_and_complete_current_models() {
    for (project_type, external_kind) in [
        (
            ProjectType::Processing,
            bsl_metadata::ExternalObjectKind::DataProcessor,
        ),
        (
            ProjectType::Report,
            bsl_metadata::ExternalObjectKind::Report,
        ),
    ] {
        let directory = TestDirectory::new();
        let root = fixture::project(&directory.0, project_type);
        let paths = fixture::paths(&root);
        let mut session = AnalysisSession::with_inputs(
            MetadataInputs::open(&root.join("src")).unwrap(),
            RootDatabaseImpl::new_with_salsa_events(),
            SessionOptions::default(),
            AnalysisScope::CanonicalExternal,
        )
        .unwrap();
        assert_eq!(session.scope(), AnalysisScope::CanonicalExternal);
        let roots = session.database.workspace_configs_snapshot();
        assert_eq!(roots.kinds, [RootKind::External(external_kind)]);
        assert!(!roots.has_base());
        let request = InputRequest::default();
        session.load_pending_metadata_inputs(&request).unwrap();
        assert_eq!(
            session.resource_owner(&paths.dialog),
            Some(&paths.form_owner)
        );
        assert!(session.metadata_resource(&paths.dialog).is_some());
        let resolved = session
            .resolve(&paths.owner, session.revision())
            .unwrap()
            .unwrap();
        let CanonicalMetadata::Object(owner) = &resolved else {
            panic!("external object")
        };
        assert_eq!(owner.mdo_type, external_kind.mdo_type());
        assert_eq!(owner.attributes[0].name, "Сохраненный");
        for (source, body, anchor) in [
            (&paths.object_module, fixture::OBJECT_BODY, "ЭтотОбъект."),
            (&paths.form_module, fixture::FORM_BODY, "Объект."),
        ] {
            session
                .load_module_saved(source, session.revision(), &request)
                .unwrap();
            let offset = u32::try_from(body.find(anchor).unwrap() + anchor.len()).unwrap();
            session
                .prepare_completion_metadata(
                    source,
                    session.revision(),
                    offset,
                    &request,
                    &QueryCancellation::default(),
                )
                .unwrap();
            let result = session
                .module_completions(
                    source,
                    session.revision(),
                    offset,
                    Locale::Ru,
                    &QueryCancellation::default(),
                )
                .unwrap();
            assert!(
                result.value.iter().any(|item| item.label == "Сохраненный"),
                "{}",
                source.as_str()
            );
            assert!(result.value.iter().any(|item| item.label == "Строки"));
            if source == &paths.object_module {
                assert!(result.value.iter().any(|item| item.label == "Метаданные"));
            }
        }
        assert_module_types(&session, &paths, external_kind);
        assert!(session.summary().main_models_complete);
        assert!(session.summary().descriptor_models_complete);
        assert_eq!(
            session
                .database
                .salsa_family_churn("load_configuration")
                .unwrap()
                .0,
            0
        );
        assert!(!root.join("src/Configuration.xml").exists());
        assert!(!root.join(".eska").exists());
    }
}

/// Current metadata overlays update both self/form field completion, while invalid inputs clear owners.
#[test]
fn canonical_external_metadata_saved_dirty_invalid_close_parity() {
    for kind in [ProjectType::Processing, ProjectType::Report] {
        let directory = TestDirectory::new();
        let root = fixture::project(&directory.0, kind);
        let paths = fixture::paths(&root);
        let owner_path = root.join("src").join(paths.owner.as_path());
        let saved = fs::read_to_string(&owner_path).unwrap();
        let current = saved.replace("Сохраненный", "Текущий");
        let mut session = AnalysisSession::from_reader_with_scope(
            SourceReader::open(&root.join("src")).unwrap(),
            SessionOptions::default(),
            AnalysisScope::CanonicalExternal,
        )
        .unwrap();
        let request = InputRequest::default();
        session.load_pending_metadata_inputs(&request).unwrap();
        let original = session
            .resolve(&paths.owner, session.revision())
            .unwrap()
            .unwrap();
        assert_current_fields(&mut session, &paths, "Сохраненный", "Текущий");
        session
            .apply_overlay(&paths.owner, session.revision(), 1, &current, &request)
            .unwrap();
        let changed = session
            .resolve(&paths.owner, session.revision())
            .unwrap()
            .unwrap();
        let CanonicalMetadata::Object(object) = &changed else {
            panic!("external object")
        };
        assert_eq!(object.attributes[0].name, "Текущий");
        assert_ne!(original, changed);
        assert_current_fields(&mut session, &paths, "Текущий", "Сохраненный");
        assert_eq!(fs::read_to_string(&owner_path).unwrap(), saved);
        session
            .apply_overlay(&paths.owner, session.revision(), 2, "not: [valid", &request)
            .unwrap();
        assert!(
            session
                .resolve(&paths.owner, session.revision())
                .unwrap()
                .is_none()
        );
        assert!(!session.summary().main_models_complete);
        assert_current_fields(&mut session, &paths, "", "Текущий");
        session
            .close_overlay(&paths.owner, session.revision(), &request)
            .unwrap();
        assert_eq!(
            session
                .resolve(&paths.owner, session.revision())
                .unwrap()
                .unwrap(),
            original
        );
        assert_eq!(fs::read_to_string(&owner_path).unwrap(), saved);
        assert_current_fields(&mut session, &paths, "Сохраненный", "Текущий");
    }
}

/// An own canonical object never proves full-project semantics or a missing owning-base target.
#[test]
fn canonical_external_full_semantics_and_foreign_targets_require_configuration_context() {
    for kind in [ProjectType::Processing, ProjectType::Report] {
        let directory = TestDirectory::new();
        let root = fixture::project(&directory.0, kind);
        let paths = fixture::paths(&root);
        let mut session = AnalysisSession::from_reader_with_scope(
            SourceReader::open(&root.join("src")).unwrap(),
            SessionOptions::default(),
            AnalysisScope::CanonicalExternal,
        )
        .unwrap();
        let request = InputRequest::default();
        assert_eq!(
            session.load_semantic_inputs(session.revision(), &request),
            Err(SessionRequestError::ConfigurationContextRequired),
        );
        assert_eq!(session.summary().ready_models, 0);
        session.load_pending_metadata_inputs(&request).unwrap();
        session
            .load_module_saved(&paths.object_module, session.revision(), &request)
            .unwrap();
        assert!(matches!(
            session.module_diagnostics(
                &paths.object_module,
                session.revision(),
                &crate::DiagnosticsConfig::default(),
                &QueryCancellation::default()
            ),
            Err(QueryError::Request(
                SessionRequestError::ConfigurationContextRequired
            )),
        ));
        let foreign = "Процедура Проба()\n    Справочники.Товары.\nКонецПроцедуры\n";
        session
            .apply_module_overlay(
                &paths.object_module,
                session.revision(),
                1,
                foreign,
                &request,
            )
            .unwrap();
        let offset = u32::try_from(foreign.find("Товары.").unwrap() + "Товары.".len()).unwrap();
        assert!(matches!(
            session.prepare_completion_metadata(
                &paths.object_module,
                session.revision(),
                offset,
                &request,
                &QueryCancellation::default()
            ),
            Err(QueryError::Request(
                SessionRequestError::ConfigurationContextRequired
            )),
        ));
        assert!(matches!(
            session.module_completions(
                &paths.object_module,
                session.revision(),
                offset,
                Locale::Ru,
                &QueryCancellation::default()
            ),
            Err(QueryError::Request(
                SessionRequestError::ConfigurationContextRequired
            ))
        ));
        assert!(!session.summary().metadata_context_complete);
    }
}

/// Case changes in a real external main-attribute qualified name retain ownership; foreign names and kinds do not.
#[test]
fn canonical_external_ownership_uses_native_case_folding_for_mixed_names_and_cold_form_qnames() {
    for kind in [ProjectType::Processing, ProjectType::Report] {
        let directory = TestDirectory::new();
        let root = fixture::project(&directory.0, kind);
        let paths = fixture::paths(&root);
        let physical = root.join("src").join(paths.dialog.as_path());
        let saved = fs::read_to_string(&physical).unwrap();
        let varied = fixture::NAME.to_uppercase();
        let current = saved.replace(
            &format!("Object.{}", fixture::NAME),
            &format!("Object.{varied}"),
        );
        assert_ne!(current, saved);
        fs::write(&physical, &current).unwrap();
        let mut session = AnalysisSession::open(&root.join("src")).unwrap();
        let own_kind = session.external_kind.unwrap().mdo_type();
        assert_ne!(varied, fixture::NAME);
        assert_eq!(
            session.require_own_external_target(own_kind, &varied),
            Ok(())
        );
        assert!(matches!(
            session.require_own_external_target(own_kind, "ДругойExternalOwner"),
            Err(QueryError::Request(
                SessionRequestError::ConfigurationContextRequired
            ))
        ));
        assert!(matches!(
            session.require_own_external_target(bsl_metadata::MdoType::DataProcessor, &varied),
            Err(QueryError::Request(
                SessionRequestError::ConfigurationContextRequired
            ))
        ));
        let request = InputRequest::default();
        session
            .load_saved(&paths.owner, session.revision(), &request)
            .unwrap();
        assert!(matches!(
            session.owner(&paths.form_owner).unwrap().state,
            ModelState::Pending
        ));
        assert!(matches!(
            session.metadata_resource(&paths.dialog).unwrap().state,
            crate::MetadataResourceState::Pending
        ));
        session
            .load_module_saved(&paths.form_module, session.revision(), &request)
            .unwrap();
        let offset =
            u32::try_from(fixture::FORM_BODY.find("Объект.").unwrap() + "Объект.".len()).unwrap();
        session
            .prepare_completion_metadata(
                &paths.form_module,
                session.revision(),
                offset,
                &request,
                &QueryCancellation::default(),
            )
            .unwrap();
        let result = session
            .module_completions(
                &paths.form_module,
                session.revision(),
                offset,
                Locale::Ru,
                &QueryCancellation::default(),
            )
            .unwrap();
        assert!(result.value.iter().any(|item| item.label == "Сохраненный"));
        assert!(!result.metadata_context_complete);
        let module = session.database.module_metadata(hir::ModuleId {
            file_id: session.modules[&paths.form_module].file,
        });
        assert_eq!(
            module
                .form
                .as_ref()
                .unwrap()
                .main_attribute()
                .unwrap()
                .attr_type,
            bsl_metadata::AttributeType::Ref {
                mdo_type: own_kind,
                name: varied
            }
        );
        assert_eq!(
            fs::read_to_string(session.root().join(paths.dialog.as_path())).unwrap(),
            current
        );
    }
}

/// Native field cards and canonical YAML definitions resolve actual self and form main-attribute receivers.
#[test]
fn canonical_external_self_and_form_field_hover_and_definition_use_current_metadata() {
    for kind in [ProjectType::Processing, ProjectType::Report] {
        let directory = TestDirectory::new();
        let root = fixture::project(&directory.0, kind);
        let paths = fixture::paths(&root);
        let mut session = AnalysisSession::open(&root.join("src")).unwrap();
        let request = InputRequest::default();
        for (source, receiver) in [
            (&paths.object_module, "ЭтотОбъект"),
            (&paths.form_module, "Объект"),
        ] {
            let text = format!(
                "Процедура Проба()\n    Значение = {receiver}.Сохраненный;\nКонецПроцедуры\n"
            );
            session
                .apply_module_overlay(source, session.revision(), 1, &text, &request)
                .unwrap();
            let offset = u32::try_from(text.find("Сохраненный").unwrap()).unwrap();
            session
                .prepare_symbol_hover_metadata(
                    source,
                    session.revision(),
                    offset,
                    &request,
                    &QueryCancellation::default(),
                )
                .unwrap();
            for locale in [Locale::Ru, Locale::En] {
                let hovered = session
                    .module_symbol_hover(
                        source,
                        session.revision(),
                        offset,
                        locale,
                        &QueryCancellation::default(),
                    )
                    .unwrap();
                assert!(!hovered.metadata_context_complete);
                let card = hovered.value.expect("real external field hover");
                assert_eq!(card.kind, crate::HoverKind::Field);
                assert_eq!(card.symbol, "Сохраненный");
                assert!(
                    card.return_type
                        .as_deref()
                        .is_some_and(|label| label.contains(if locale == Locale::Ru {
                            "Булево"
                        } else {
                            "Boolean"
                        })),
                    "{card:?}"
                );
            }
            let target = session
                .module_metadata_definition(
                    source,
                    session.revision(),
                    offset,
                    &QueryCancellation::default(),
                )
                .unwrap();
            assert!(!target.metadata_context_complete);
            let target = target.value.expect("real external field definition");
            assert_eq!(target.source, paths.owner);
            assert_eq!(target.uuid.as_str(), "20000000-0000-4000-8000-000000000001");
            let exact = session
                .metadata_location(
                    &paths.owner,
                    session.revision(),
                    &eska::project_core::source_format::NativeUuid::parse(
                        "20000000-0000-4000-8000-000000000001",
                    )
                    .unwrap(),
                    None,
                )
                .unwrap();
            assert_eq!(target, exact);
        }
        assert!(!root.join(".eska").exists());
    }
}

/// Ready and editor-owned inputs are reused; changed root fields flow into both existing native consumers.
fn assert_current_fields(
    session: &mut AnalysisSession,
    paths: &fixture::Paths,
    present: &str,
    absent: &str,
) {
    let request = InputRequest::default();
    for (source, text, anchor) in [
        (&paths.object_module, fixture::OBJECT_BODY, "ЭтотОбъект."),
        (&paths.form_module, fixture::FORM_BODY, "Объект."),
    ] {
        if matches!(
            session.module(source).unwrap().state,
            crate::ModuleState::Pending
        ) {
            session
                .load_module_saved(source, session.revision(), &request)
                .unwrap();
        }
        let offset = u32::try_from(text.find(anchor).unwrap() + anchor.len()).unwrap();
        session
            .prepare_completion_metadata(
                source,
                session.revision(),
                offset,
                &request,
                &QueryCancellation::default(),
            )
            .unwrap();
        let result = session
            .module_completions(
                source,
                session.revision(),
                offset,
                Locale::Ru,
                &QueryCancellation::default(),
            )
            .unwrap();
        assert!(!result.metadata_context_complete);
        if !present.is_empty() {
            assert!(
                result.value.iter().any(|item| item.label == present),
                "{}",
                source.as_str()
            );
        }
        assert!(
            !result.value.iter().any(|item| item.label == absent),
            "{}",
            source.as_str()
        );
    }
}

/// The actual native module query retains the external object and its form main-attribute type.
fn assert_module_types(
    session: &AnalysisSession,
    paths: &fixture::Paths,
    external_kind: bsl_metadata::ExternalObjectKind,
) {
    let object_file = session.modules[&paths.object_module].file;
    let form_file = session.modules[&paths.form_module].file;
    let object = session.database.module_metadata(hir::ModuleId {
        file_id: object_file,
    });
    assert_eq!(
        object.mdo.as_ref().unwrap().mdo_type,
        external_kind.mdo_type()
    );
    let form = session
        .database
        .module_metadata(hir::ModuleId { file_id: form_file });
    assert_eq!(
        form.form
            .as_ref()
            .unwrap()
            .main_attribute()
            .unwrap()
            .attr_type,
        bsl_metadata::AttributeType::Ref {
            mdo_type: external_kind.mdo_type(),
            name: fixture::NAME.into()
        }
    );
}
