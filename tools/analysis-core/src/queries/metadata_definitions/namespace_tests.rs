//! Typed common-module namespaces stay distinct from nearer BSL declarations and typed locals.

use eska::analysis_core::InputRequest;

use super::{tests::query, *};
use crate::session::tests::{draft, fixture};

const CALLER: &str = "Catalogs/Sample/Ext/ObjectModule.bsl";
const OWNER: &str = "CommonModules/Tools/object.yaml";
const TARGET: &str = "CommonModules/Tools/Ext/Module.bsl";

/// Mixed-case namespace references bind to ready metadata even while the target module is unread.
#[test]
fn common_module_namespace_uses_current_metadata_without_loading_its_body() {
    let (_directory, root) = fixture();
    let mut session = AnalysisSession::open(&root).unwrap();
    let caller = SourcePath::parse(CALLER).unwrap();
    let owner = SourcePath::parse(OWNER).unwrap();
    let request = InputRequest::default();
    let body = "\u{feff}// Я😀\r\nПроцедура Тест()\r\nToOlS.Цель();\r\nКонецПроцедуры\r\n";
    session
        .apply_module_overlay(&caller, session.revision(), 1, body, &request)
        .unwrap();
    let at = u32::try_from(body.find("ToOlS").unwrap() + 1).unwrap();
    assert!(query(&session, &caller, at).value.is_none());
    session
        .load_saved(&owner, session.revision(), &request)
        .unwrap();
    let first = query(&session, &caller, at).value.unwrap();
    assert_eq!(first.source, owner);
    assert_eq!(first.native_path.as_str(), "CommonModules/Tools.xml");
    assert_eq!(first.uuid.as_str(), "60000000-0000-4000-8000-000000000001");
    let target = SourcePath::parse(TARGET).unwrap();
    assert!(matches!(
        session.module(&target).unwrap().state,
        crate::ModuleState::Pending
    ));
    let saved = std::fs::read_to_string(root.join(OWNER)).unwrap();
    let editor = format!("# Я😀\r\n{}", saved.replace('\n', "\r\n"));
    session
        .apply_overlay(&owner, session.revision(), 7, &editor, &request)
        .unwrap();
    let current = query(&session, &caller, at).value.unwrap();
    assert_ne!(current.source_sha256, first.source_sha256);
    assert_eq!(current.range.start.line, first.range.start.line + 1);
    std::fs::remove_file(root.join(OWNER)).unwrap();
    assert_eq!(query(&session, &caller, at).value, Some(current));
    session
        .apply_overlay(&owner, session.revision(), 8, "broken: [\n", &request)
        .unwrap();
    assert!(query(&session, &caller, at).value.is_none());
    std::fs::write(root.join(OWNER), saved).unwrap();
    session
        .close_overlay(&owner, session.revision(), &request)
        .unwrap();
    assert_eq!(
        query(&session, &caller, at).value.unwrap().source_sha256,
        first.source_sha256
    );
}

/// A same-named explicit local, parameter or method keeps its BSL definition rather than gaining YAML ownership.
#[test]
fn common_module_namespace_is_not_guessed_from_shadowed_spelling() {
    let (_directory, root) = fixture();
    let mut session = AnalysisSession::open(&root).unwrap();
    let caller = SourcePath::parse(CALLER).unwrap();
    let owner = SourcePath::parse(OWNER).unwrap();
    let request = InputRequest::default();
    session
        .load_saved(&owner, session.revision(), &request)
        .unwrap();
    for (version, body) in [
        "Процедура Тест()\nПерем Tools;\nTools=Новый Структура;\nTools.Цель();\nКонецПроцедуры\n",
        "Процедура Тест(Tools)\nTools.Цель();\nКонецПроцедуры\n",
        "Процедура Tools()\nКонецПроцедуры\nTools();\n",
        "Процедура Тест()\nТекст=\"Tools.Цель()\";\nКонецПроцедуры\n",
    ]
    .into_iter()
    .enumerate()
    {
        session
            .apply_module_overlay(
                &caller,
                session.revision(),
                i64::try_from(version + 1).unwrap(),
                body,
                &request,
            )
            .unwrap();
        let at = u32::try_from(body.rfind("Tools").unwrap() + 1).unwrap();
        assert!(query(&session, &caller, at).value.is_none(), "{body}");
    }
    let alias = "Процедура Тест()\nToolsAlias=Tools;\nToolsAlias.Цель();\nКонецПроцедуры\n";
    session
        .apply_module_overlay(&caller, session.revision(), 9, alias, &request)
        .unwrap();
    let at = u32::try_from(alias.rfind("ToolsAlias").unwrap() + 1).unwrap();
    assert!(query(&session, &caller, at).value.is_none());
}

/// Changing an opening owner declaration is unavailable; released inputs cannot confirm the old namespace either.
#[test]
fn common_module_namespace_refuses_renamed_and_pending_current_yaml() {
    let (_directory, root) = fixture();
    let mut session = AnalysisSession::open(&root).unwrap();
    let caller = SourcePath::parse(CALLER).unwrap();
    let owner = SourcePath::parse(OWNER).unwrap();
    let request = InputRequest::default();
    let body = "Процедура Тест()\nTools.Цель();\nКонецПроцедуры\n";
    session
        .apply_module_overlay(&caller, session.revision(), 1, body, &request)
        .unwrap();
    session
        .load_saved(&owner, session.revision(), &request)
        .unwrap();
    let at = u32::try_from(body.find("Tools").unwrap() + 1).unwrap();
    assert!(query(&session, &caller, at).value.is_some());
    let renamed = draft(&root, &owner, "Tools", "Renamed");
    session
        .apply_overlay(&owner, session.revision(), 1, &renamed, &request)
        .unwrap();
    assert!(matches!(
        session.owner(&owner).unwrap().state,
        ModelState::Unavailable(_)
    ));
    assert!(query(&session, &caller, at).value.is_none());
    session
        .close_overlay(&owner, session.revision(), &request)
        .unwrap();
    assert!(query(&session, &caller, at).value.is_some());
    session
        .invalidate_saved_inputs(&[owner], session.revision())
        .unwrap();
    assert!(query(&session, &caller, at).value.is_none());
}
