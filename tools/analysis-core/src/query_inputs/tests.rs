//! Direct helper tests prove source admission, editor authority and native standalone syntax boundaries.

use crate::{query_creation_fixture as fixture, test_support as directory};

use std::{fs, path::PathBuf};

use ide::{DiagnosticCode, DiagnosticsConfig, Locale, METADATA_DEPENDENT_CODES};

use super::*;
use crate::SessionRevision;
use eska::project_core::source_format::import_designer_queries;

struct QueryFixture {
    _directory: directory::TestDirectory,
    root: PathBuf,
    reader: SourceReader,
    source: SourcePath,
    input: QueryTextInput,
}

impl QueryFixture {
    /// Own one complete query source in the configured playground, leaving the native input intact.
    fn new() -> Self {
        let directory = directory::TestDirectory::new();
        let native = fixture::native(&directory.0, "configuration");
        let root = directory.0.join("source");
        import_designer_queries(&native.input, &root).unwrap();
        let reader = SourceReader::open(&root).unwrap();
        let resource = SourcePath::parse(native.schema.to_str().unwrap()).unwrap();
        let source = reader
            .query_paths()
            .find(|source| reader.declared_query(source).unwrap().native_resource == &resource)
            .unwrap()
            .clone();
        let input = QueryTextInput::declare(&reader, &source).unwrap();
        Self {
            _directory: directory,
            root,
            reader,
            source,
            input,
        }
    }

    /// Replace only this test's selected plain query bytes, never descriptor or neighboring input.
    fn save(&self, text: &str) {
        fs::write(self.root.join(self.source.as_path()), text).unwrap();
    }

    /// Load through the declaration-aware reader with a host-supplied token and standard request.
    fn load(&mut self) -> QueryTextChange {
        self.input
            .load_saved(
                &self.reader,
                SessionRevision::default(),
                &InputRequest::default(),
            )
            .unwrap()
    }
}

/// Native aliases and both query profiles preserve actual declarations for all four artifact types.
#[test]
fn declarations_capture_all_four_native_owners_without_opening_plain_queries() {
    for kind in fixture::TYPES {
        let directory = directory::TestDirectory::new();
        let native = fixture::native(&directory.0, kind);
        let native_before = fixture::snapshot(&native.input);
        for path in [
            &native.root,
            &native.form,
            &native.dialog,
            &native.schema,
            &native.module,
        ] {
            assert!(native.input.join(path).is_file());
        }
        let root = directory.0.join("source");
        import_designer_queries(&native.input, &root).unwrap();
        let reader = SourceReader::open(&root).unwrap();
        assert_eq!(reader.configuration().path.as_str(), native.root_source);
        assert_eq!(
            reader.configuration().declarations[0].uuid.as_str(),
            fixture::ROOT_UUID
        );
        assert!(root.join(&native.form_source).is_file());
        assert!(root.join(native.module_source).is_file());
        let form_owner = SourcePath::parse(native.form_source.to_str().unwrap()).unwrap();
        assert_eq!(
            reader.declared_owner(&form_owner).unwrap().uuid.as_str(),
            fixture::FORM_UUID
        );
        let sources = reader.query_paths().cloned().collect::<Vec<_>>();
        for source in &sources {
            fs::remove_file(root.join(source.as_path())).unwrap();
        }
        for source in sources {
            let input = QueryTextInput::declare(&reader, &source).unwrap();
            let declared = reader.declared_query(&source).unwrap();
            assert_eq!(input.declaration().source, source);
            assert_eq!(&input.declaration().owner, declared.owner);
            assert_eq!(
                &input.declaration().native_descriptor,
                declared.native_descriptor
            );
            assert_eq!(
                &input.declaration().native_resource,
                declared.native_resource
            );
            assert_eq!(&input.declaration().uuid, declared.uuid);
            assert_eq!(input.snapshot().state, QueryTextState::Pending);
            assert_eq!(input.snapshot().origin, InputOrigin::Saved);
            assert!(input.snapshot().source_sha256.is_none());
            assert_eq!(input.bytes(), 0);
        }
        let undeclared = SourcePath::parse("Queries/Stray.sdbl").unwrap();
        assert!(
            matches!(QueryTextInput::declare(&reader, &undeclared), Err(QueryTextError::Undeclared(source)) if source == undeclared)
        );
        assert_eq!(fixture::snapshot(&native.input), native_before);
    }
}

/// Saved/editor bytes, hashes and reservations change together; failures cannot revive an earlier success.
#[test]
fn saved_overlay_close_release_and_host_revision_keep_exact_current_authority() {
    let mut case = QueryFixture::new();
    let saved = "\u{feff}ВЫБРАТЬ 1 КАК Код;\r\n// сохранённый\r\n";
    case.save(saved);
    let change = case.load();
    assert_eq!(change.previous_bytes, 0);
    assert_eq!(change.current_bytes, saved.len() as u64);
    let expected = Sha256::digest(saved.as_bytes())
        .iter()
        .fold(String::new(), |mut hash, byte| {
            write!(hash, "{byte:02x}").unwrap();
            hash
        });
    assert_eq!(
        case.input.snapshot().source_sha256.as_deref(),
        Some(expected.as_str())
    );
    let QueryTextState::Ready(text) = &case.input.snapshot().state else {
        panic!("saved text must be current");
    };
    assert_eq!(&**text, saved);
    let mut host = crate::AnalysisSession::open(&case.root).unwrap();
    let owner = case.reader.configuration().path.clone();
    host.load_saved(&owner, host.revision(), &InputRequest::default())
        .unwrap();
    let revision = host.revision();
    let overlay = "SELECT FROM\r\n";
    let change = case
        .input
        .apply_overlay(revision, 3, overlay, &InputRequest::default())
        .unwrap();
    assert_eq!(change.previous_bytes, saved.len() as u64);
    assert_eq!(change.current_bytes, overlay.len() as u64);
    assert_eq!(case.input.snapshot().revision, revision);
    assert_eq!(
        case.input.snapshot().origin,
        InputOrigin::Overlay {
            document_version: 3
        }
    );
    let before = case.input.snapshot().clone();
    for version in [2, 3] {
        assert_eq!(
            case.input
                .apply_overlay(revision, version, "SELECT 9", &InputRequest::default()),
            Err(QueryTextError::StaleDocumentVersion {
                current: 3,
                received: version
            })
        );
        assert_eq!(case.input.snapshot(), &before);
    }
    assert_eq!(
        case.input
            .load_saved(&case.reader, revision, &InputRequest::default()),
        Err(QueryTextError::OverlayActive {
            document_version: 3
        })
    );
    assert_eq!(
        case.input.release_saved(revision),
        Err(QueryTextError::OverlayActive {
            document_version: 3
        })
    );
    assert_eq!(case.input.snapshot(), &before);
    case.save("SELECT 2;\r\n");
    case.input
        .close_overlay(&case.reader, revision, &InputRequest::default())
        .unwrap();
    assert_eq!(case.input.snapshot().origin, InputOrigin::Saved);
    assert!(
        matches!(&case.input.snapshot().state, QueryTextState::Ready(text) if &**text == "SELECT 2;\r\n")
    );
    let change = case.input.release_saved(revision).unwrap();
    assert_eq!(change.current_bytes, 0);
    assert!(change.previous_bytes > 0);
    assert_eq!(case.input.snapshot().state, QueryTextState::Pending);
    assert!(case.input.snapshot().source_sha256.is_none());
}

/// A foreign composition refuses publication, while closing a deleted source accepts current unavailability.
#[test]
fn reader_mismatch_refuses_without_mutation_and_deleted_close_clears_editor_text() {
    let mut case = QueryFixture::new();
    case.save("SELECT 1");
    case.load();
    let other = QueryFixture::new();
    let before = case.input.snapshot().clone();
    assert_eq!(
        case.input.load_saved(
            &other.reader,
            SessionRevision::default(),
            &InputRequest::default()
        ),
        Err(QueryTextError::ReaderMismatch)
    );
    assert_eq!(case.input.snapshot(), &before);
    case.input
        .apply_overlay(
            SessionRevision::default(),
            1,
            "SELECT 3",
            &InputRequest::default(),
        )
        .unwrap();
    let before = case.input.snapshot().clone();
    assert_eq!(
        case.input.close_overlay(
            &other.reader,
            SessionRevision::default(),
            &InputRequest::default()
        ),
        Err(QueryTextError::ReaderMismatch)
    );
    assert_eq!(case.input.snapshot(), &before);
    fs::remove_file(case.root.join(case.source.as_path())).unwrap();
    let change = case
        .input
        .close_overlay(
            &case.reader,
            SessionRevision::default(),
            &InputRequest::default(),
        )
        .unwrap();
    assert_eq!(change.previous_bytes, "SELECT 3".len() as u64);
    assert_eq!(change.current_bytes, 0);
    assert_eq!(case.input.snapshot().origin, InputOrigin::Saved);
    assert!(case.input.snapshot().source_sha256.is_none());
    assert!(matches!(
        case.input.snapshot().state,
        QueryTextState::Unavailable(InputFailure::Source(_))
    ));
}

/// Tightened shared reservations and cancellation accept failed authority and release its stale source bytes.
#[test]
fn accepted_limit_and_cancel_failures_clear_current_bytes_hash_and_preserve_overlay_version() {
    let mut case = QueryFixture::new();
    case.save("SELECT 1");
    case.load();
    let request = InputRequest::default().with_source_budget(2, 16);
    let change = case
        .input
        .apply_overlay(SessionRevision::default(), 1, "SELECT 2", &request)
        .unwrap();
    assert_eq!(change.previous_bytes, "SELECT 1".len() as u64);
    assert_eq!(change.current_bytes, 0);
    assert_eq!(
        case.input.snapshot().state,
        QueryTextState::Unavailable(InputFailure::SourceBudgetExceeded { limit: 16 })
    );
    assert!(case.input.snapshot().source_sha256.is_none());
    assert_eq!(
        case.input.load_saved(
            &case.reader,
            SessionRevision::default(),
            &InputRequest::default()
        ),
        Err(QueryTextError::OverlayActive {
            document_version: 1
        })
    );
    let cancelled = InputRequest::default();
    cancelled.cancellation.cancel();
    case.input
        .apply_overlay(SessionRevision::default(), 2, "SELECT 3", &cancelled)
        .unwrap();
    assert_eq!(
        case.input.snapshot().state,
        QueryTextState::Unavailable(InputFailure::Cancelled)
    );
    assert_eq!(
        case.input.snapshot().origin,
        InputOrigin::Overlay {
            document_version: 2
        }
    );
    case.input
        .close_overlay(
            &case.reader,
            SessionRevision::default(),
            &InputRequest::default(),
        )
        .unwrap();
    let request = InputRequest {
        max_document_bytes: 1,
        ..InputRequest::default()
    };
    let change = case
        .input
        .load_saved(&case.reader, SessionRevision::default(), &request)
        .unwrap();
    assert_eq!(change.current_bytes, 0);
    assert_eq!(
        case.input.snapshot().state,
        QueryTextState::Unavailable(InputFailure::DocumentTooLarge { limit: 1 })
    );
    assert_eq!(case.input.bytes(), 0);
}

/// Native syntax uses verbatim query coordinates and the same codes/ranges in both locales.
#[test]
fn direct_native_syntax_matches_codes_ranges_and_explicit_source_only_result() {
    let mut case = QueryFixture::new();
    let text = "\u{feff}ВЫБРАТЬ\r\nИЗ\r\n";
    fs::remove_file(case.root.join(case.source.as_path())).unwrap();
    case.input
        .apply_overlay(
            SessionRevision::default(),
            1,
            text,
            &InputRequest::default(),
        )
        .unwrap();
    let mut codes = Vec::new();
    let mut messages = Vec::new();
    for locale in [Locale::Ru, Locale::En] {
        let config = DiagnosticsConfig {
            locale,
            only_enabled: Some(vec![DiagnosticCode::QueryParseError]),
            ..DiagnosticsConfig::default()
        };
        let native = ide::validate_query_text(&config, None, text);
        let result: QuerySyntaxResult = case
            .input
            .query_syntax(&config, &InputRequest::default())
            .unwrap();
        assert_eq!(result.scope, QuerySyntaxScope::SyntaxOnly);
        assert_eq!(serde_json::to_value(result.scope).unwrap(), "syntax_only");
        assert!(result.parse_enabled);
        assert_eq!(result.input_revision, case.input.snapshot().revision);
        assert_eq!(result.source, case.source);
        assert_eq!(
            Some(&result.source_sha256),
            case.input.snapshot().source_sha256.as_ref()
        );
        assert_eq!(
            result.origin,
            InputOrigin::Overlay {
                document_version: 1
            }
        );
        assert!(!result.diagnostics.is_empty());
        let positions = result
            .diagnostics
            .iter()
            .map(|diagnostic| (diagnostic.code, diagnostic.range))
            .collect::<Vec<_>>();
        assert_eq!(
            positions,
            native
                .iter()
                .map(|diagnostic| (diagnostic.code, diagnostic.range))
                .collect::<Vec<_>>()
        );
        for diagnostic in &result.diagnostics {
            assert!(
                text.get(
                    u32::from(diagnostic.range.start()) as usize
                        ..u32::from(diagnostic.range.end()) as usize
                )
                .is_some()
            );
        }
        codes.push(positions);
        messages.push(
            result
                .diagnostics
                .iter()
                .map(|diagnostic| diagnostic.message.clone())
                .collect::<Vec<_>>(),
        );
    }
    assert_eq!(codes[0], codes[1]);
    assert_ne!(messages[0], messages[1]);
}

/// Explicitly requested metadata diagnostics cannot override the standalone syntax-only admission.
#[test]
fn metadata_rules_are_disabled_even_when_only_enabled_takes_precedence() {
    let mut case = QueryFixture::new();
    case.input
        .apply_overlay(
            SessionRevision::default(),
            1,
            "SELECT Missing.Field FROM Catalog.Missing AS Missing",
            &InputRequest::default(),
        )
        .unwrap();
    let mut only = METADATA_DEPENDENT_CODES.to_vec();
    only.push(DiagnosticCode::QueryParseError);
    let config = DiagnosticsConfig {
        only_enabled: Some(only),
        ..DiagnosticsConfig::default()
    };
    let profile = syntax::syntax_profile(&config);
    for code in METADATA_DEPENDENT_CODES {
        assert!(profile.is_disabled(*code));
    }
    assert!(profile.is_disabled(DiagnosticCode::DuplicateAliasInQuery));
    let result = case
        .input
        .query_syntax(&config, &InputRequest::default())
        .unwrap();
    assert_eq!(result.scope, QuerySyntaxScope::SyntaxOnly);
    assert!(result.diagnostics.is_empty());
    let disabled = DiagnosticsConfig {
        only_enabled: Some(vec![]),
        ..config
    };
    let result = case
        .input
        .query_syntax(&disabled, &InputRequest::default())
        .unwrap();
    assert!(!result.parse_enabled);
    assert!(result.diagnostics.is_empty());
}

/// A stopped request discards completed native work, and unread input never yields clean diagnostics.
#[test]
fn cancellation_before_and_after_native_call_rejects_publication() {
    let mut case = QueryFixture::new();
    let config = DiagnosticsConfig {
        only_enabled: Some(vec![DiagnosticCode::QueryParseError]),
        ..DiagnosticsConfig::default()
    };
    assert!(matches!(
        case.input.query_syntax(&config, &InputRequest::default()),
        Err(QuerySyntaxError::Pending)
    ));
    let text = "SELECT FROM";
    case.input
        .apply_overlay(
            SessionRevision::default(),
            1,
            text,
            &InputRequest::default(),
        )
        .unwrap();
    let native = ide::validate_query_text(&config, None, text);
    let request = InputRequest::default();
    request.cancellation.cancel();
    assert!(matches!(
        case.input.query_syntax(&config, &request),
        Err(QuerySyntaxError::Cancelled)
    ));
    assert!(matches!(
        syntax::publish(&case.input, &config, text, native, &request),
        Err(QuerySyntaxError::Cancelled)
    ));
    case.input
        .apply_overlay(SessionRevision::default(), 2, text, &request)
        .unwrap();
    assert!(matches!(
        case.input.query_syntax(&config, &InputRequest::default()),
        Err(QuerySyntaxError::Unavailable(InputFailure::Cancelled))
    ));
}
