//! The native completion path owns root binding and context checks; names need no channel reads.

use super::integration_service_tests::fixture;
use super::*;
use crate::session::tests::{owner, support::TestDirectory};
use crate::{CompletionItem, Locale, QueryCancellation};

/// Query at the actual collection dot, using the same current text as the native semantic facade.
fn completions(
    session: &AnalysisSession,
    module: &SourcePath,
    body: &str,
    marker: &str,
    locale: Locale,
) -> Vec<CompletionItem> {
    let offset = u32::try_from(body.find(marker).unwrap() + marker.len()).unwrap();
    session
        .module_completions(
            module,
            session.revision(),
            offset,
            locale,
            &QueryCancellation::default(),
        )
        .unwrap()
        .value
}

/// Declared names and localized details are independent of whether channel payloads have been activated.
#[test]
fn integration_names_complete_in_both_spellings_and_locales_without_loading_yaml() {
    let directory = TestDirectory::new();
    let root = crate::test_project::project(&directory.0, "// Current body.\n", false);
    fixture::add_integration_services(&root, &directory.0.join("native"));
    let mut session = AnalysisSession::open(&root.join("src-with-integration")).unwrap();
    let module = owner("IntegrationServices/Bus/Ext/Module.bsl");
    let request = InputRequest::default();
    for (version, collection) in [
        "Metadata.IntegrationServices.",
        "Метаданные.СервисыИнтеграции.",
        "mEtAdAtA.iNtEgRaTiOnSeRvIcEs.",
    ]
    .into_iter()
    .enumerate()
    {
        let body = format!("Procedure Run()\n Value = {collection}Bus;\nEndProcedure\n");
        session
            .apply_module_overlay(
                &module,
                session.revision(),
                i64::try_from(version).unwrap() + 1,
                &body,
                &request,
            )
            .unwrap();
        for locale in [Locale::Ru, Locale::En] {
            let items = completions(&session, &module, &body, collection, locale);
            let names = items
                .iter()
                .map(|item| item.label.as_str())
                .collect::<std::collections::BTreeSet<_>>();
            assert_eq!(names, ["Bus", "Neighbor"].into_iter().collect());
            let detail = match locale {
                Locale::Ru => "Сервис интеграции",
                Locale::En => "Integration service",
            };
            assert!(
                items
                    .iter()
                    .all(|item| item.detail.as_deref() == Some(detail))
            );
        }
        assert!(matches!(
            session
                .owner(&owner("IntegrationServices/Bus/object.yaml"))
                .unwrap()
                .state,
            ModelState::Pending
        ));
        assert!(matches!(
            session
                .owner(&owner("IntegrationServices/Neighbor/object.yaml"))
                .unwrap()
                .state,
            ModelState::Pending
        ));
    }
}

/// A matching spelling in a local parameter, ordinary value, comment or string cannot claim global metadata names.
#[test]
fn integration_completion_preserves_native_shadowing_and_noncode_boundaries() {
    let directory = TestDirectory::new();
    let root = crate::test_project::project(&directory.0, "// Current body.\n", false);
    fixture::add_integration_services(&root, &directory.0.join("native"));
    let mut session = AnalysisSession::open(&root.join("src-with-integration")).unwrap();
    let module = owner("IntegrationServices/Bus/Ext/Module.bsl");
    let marker = "Metadata.IntegrationServices.";
    for (version, body) in [
        "Procedure Run(Metadata)\n Value = Metadata.IntegrationServices.Bus;\nEndProcedure\n",
        "Procedure Run()\n Metadata = New Structure;\n Value = Metadata.IntegrationServices.Bus;\nEndProcedure\n",
        "Procedure Run()\n // Metadata.IntegrationServices.Bus\nEndProcedure\n",
        "Procedure Run()\n Value = \"Metadata.IntegrationServices.Bus\";\nEndProcedure\n",
    ].into_iter().enumerate() {
        session.apply_module_overlay(&module, session.revision(), i64::try_from(version).unwrap() + 1, body, &InputRequest::default()).unwrap();
        for locale in [Locale::Ru, Locale::En] {
            let items = completions(&session, &module, body, marker, locale);
            assert!(!items.iter().any(|item| ["Bus", "Neighbor"].contains(&item.label.as_str())), "{body}: {items:?}");
        }
    }
}
