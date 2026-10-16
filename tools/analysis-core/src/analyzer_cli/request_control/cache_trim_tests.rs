//! Language completion drops native memo values without dropping current editor authority.

use std::{
    fmt::Write as _,
    fs,
    path::{Path, PathBuf},
};

use base_db::SourceDatabase;
use eska::{
    analysis_core::InputRequest, project_core::source_format::SourcePath,
    tool_cli::localization::Locale,
};

use super::*;
use crate::{
    DiagnosticCode, DiagnosticsConfig, QueryCancellation,
    analyzer_cli::{completion, document_symbols},
    session::tests::{fixture, owner},
    test_support::TestDirectory,
};

const METHODS: usize = 2200;
const DRAFT: &str = "// Я😀 current draft\r\nProcedure Draft()\r\n    Value = 1;\r\n    Value = Value;\r\nEndProcedure\r\n";

struct Fixture {
    _directory: TestDirectory,
    root: PathBuf,
    paths: [SourcePath; 2],
    session: AnalysisSession,
    saved: String,
}

impl Fixture {
    /// Exceed the native sweep window while keeping a separate editor-owned parse hint resident.
    fn new() -> Self {
        let (directory, root) = fixture();
        let paths = [
            owner("CommonModules/Tools/Ext/Module.bsl"),
            owner("Catalogs/Sample/Ext/ObjectModule.bsl"),
        ];
        let mut saved = String::new();
        for method in 0..METHODS {
            writeln!(
                saved,
                "Procedure Method{method}()\n    Value = 1;\n    Value = Value;\nEndProcedure"
            )
            .unwrap();
        }
        fs::write(root.join(paths[0].as_path()), &saved).unwrap();
        let mut session = AnalysisSession::open(&root).unwrap();
        session
            .load_module_saved(&paths[0], session.revision(), &InputRequest::default())
            .unwrap();
        session
            .apply_module_overlay(
                &paths[1],
                session.revision(),
                37,
                DRAFT,
                &InputRequest::default(),
            )
            .unwrap();
        Self {
            _directory: directory,
            root,
            paths,
            session,
            saved,
        }
    }
}

#[derive(Clone, Copy)]
enum Outcome {
    Symbols,
    InvalidPosition,
    Cancelled,
    Deadline,
}

/// Heap hooks count owned messages, so a reduction proves eviction rather than fewer memo identities.
fn diagnostic_heap(session: &AnalysisSession) -> usize {
    let tables = session.kernel_memory().tables;
    let diagnostics = tables
        .iter()
        .filter(|table| {
            table
                .name
                .contains("Diagnostic<cfg_types::local_range::LocalRange>")
        })
        .collect::<Vec<_>>();
    assert!(!diagnostics.is_empty());
    diagnostics
        .into_iter()
        .map(|table| table.heap_bytes.unwrap())
        .sum()
}

/// Real successful and failed handlers execute their ordinary controlled request boundary.
fn run_handler(
    session: &mut AnalysisSession,
    root: &Path,
    path: &SourcePath,
    locale: Locale,
    outcome: Outcome,
) -> Result<Value, Value> {
    let settings =
        crate::analysis_settings::load(root, None, locale, crate::batch::BatchProfile::Syntax);
    let cancellation = RequestCancellation::default();
    let uri = url::Url::from_file_path(root.join(path.as_path())).unwrap();
    match outcome {
        Outcome::Symbols => {
            let params =
                serde_json::from_value(json!({"textDocument":{"uri":uri.as_str()}})).unwrap();
            document_symbols::pull(
                session,
                &params,
                &settings,
                &document_symbols::Options {
                    session_id: "connection",
                    request: &InputRequest::default(),
                    cancellation: &cancellation,
                    timeout: Duration::from_secs(300),
                    hierarchical: true,
                },
            )
        }
        Outcome::InvalidPosition => {
            let params = serde_json::from_value(json!({"textDocument":{"uri":uri.as_str()},"position":{"line":u32::MAX,"character":0}})).unwrap();
            completion::pull(
                session,
                &params,
                &settings,
                &completion::Options {
                    session_id: "connection",
                    request: InputRequest::default(),
                    cancellation: &cancellation,
                    timeout: Duration::from_secs(300),
                    capabilities: completion::Capabilities::default(),
                },
            )
        }
        Outcome::Cancelled | Outcome::Deadline => {
            let timeout = if matches!(outcome, Outcome::Deadline) {
                Duration::ZERO
            } else {
                Duration::from_secs(300)
            };
            let deadline = Deadline::start(timeout, cancellation.clone()).unwrap();
            if matches!(outcome, Outcome::Cancelled) {
                cancellation.cancel();
            }
            finish_language(
                session,
                "connection",
                Method::Language(language::Method::Diagnostic),
                &deadline,
                Ok(json!({"late":true})),
            )
        }
    }
}

/// Failed/stopped completions cannot publish late success or turn a refusal into empty findings.
fn assert_outcome(outcome: Outcome, result: Result<Value, Value>) {
    match outcome {
        Outcome::Symbols => assert_eq!(result.unwrap().as_array().unwrap().len(), METHODS),
        Outcome::InvalidPosition => {
            assert_eq!(result.unwrap_err()["data"]["kind"], "position_invalid");
        }
        Outcome::Cancelled => assert_eq!(result.unwrap_err()["data"]["kind"], "analysis_cancelled"),
        Outcome::Deadline => {
            let error = result.unwrap_err();
            assert_eq!(error["data"]["kind"], "analysis_deadline_exceeded");
            assert_eq!(error["data"]["retriggerRequest"], false);
        }
    }
}

/// Exclusive eviction preserves both saved and editor inputs and recomputes the same diagnostics.
#[test]
fn language_completion_trims_success_failure_and_stopped_queries_without_losing_editor_inputs() {
    for locale in [Locale::EnUs, Locale::RuRu] {
        for outcome in [
            Outcome::Symbols,
            Outcome::InvalidPosition,
            Outcome::Cancelled,
            Outcome::Deadline,
        ] {
            let Fixture {
                _directory,
                root,
                paths,
                mut session,
                saved,
            } = Fixture::new();
            let config = DiagnosticsConfig {
                only_enabled: Some(vec![DiagnosticCode::SelfAssign]),
                ..DiagnosticsConfig::default()
            };
            let before = paths.each_ref().map(|path| {
                session
                    .module_diagnostics(
                        path,
                        session.revision(),
                        &config,
                        &QueryCancellation::default(),
                    )
                    .unwrap()
            });
            assert_eq!(before[0].value.len(), METHODS);
            assert_eq!(before[1].value.len(), 1);
            let heap = diagnostic_heap(&session);
            assert!(heap > 0);
            let revision = session.revision();
            let resources = session.resources();
            let models = paths
                .each_ref()
                .map(|path| session.module(path).unwrap().clone());
            let files = paths
                .each_ref()
                .map(|path| session.selected_root.modules[path].file);
            let editor_hint = session.database.parse_snapshot(files[1]).unwrap();
            assert_outcome(
                outcome,
                run_handler(&mut session, &root, &paths[0], locale, outcome),
            );
            assert!(diagnostic_heap(&session) < heap);
            assert!(session.database.parse_snapshot(files[0]).is_none());
            let retained = session.database.parse_snapshot(files[1]).unwrap();
            assert_eq!(retained.text, editor_hint.text);
            assert_eq!(retained.parse, editor_hint.parse);
            assert_eq!(session.revision(), revision);
            assert_eq!(session.resources(), resources);
            for ((path, model), expected) in paths.each_ref().into_iter().zip(models).zip(before) {
                assert_eq!(session.module(path), Some(&model));
                assert_eq!(
                    session
                        .module_diagnostics(path, revision, &config, &QueryCancellation::default())
                        .unwrap(),
                    expected
                );
            }
            assert_eq!(
                fs::read(root.join(paths[0].as_path())).unwrap(),
                saved.as_bytes()
            );
            assert_eq!(
                session
                    .with_module_text(
                        &paths[1],
                        revision,
                        &QueryCancellation::default(),
                        str::to_owned
                    )
                    .unwrap()
                    .value,
                DRAFT
            );
        }
    }
}
