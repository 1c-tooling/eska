//! Deterministic cooperative cancellation inside a running tracked query, without timing guesses.

use std::{
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver, SyncSender},
    },
    time::Duration,
};

use base_db::RootQueryDb;
use eska::analysis_core::InputRequest;
use ide_db::RootDatabaseImpl;
use salsa::Database;

use super::*;
use crate::{QueryCancellation, session::tests::fixture};

/// Gates are test-only untracked signals; identity is stable for one controlled computation.
#[derive(Clone, Debug)]
struct Gate(Arc<Signals>);

impl PartialEq for Gate {
    /// Signal identity, rather than the state of channels, is the tracked input value.
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
}

impl Eq for Gate {}

#[derive(Debug)]
struct Signals {
    entered: SyncSender<()>,
    resume: Mutex<Receiver<()>>,
    passed_checkpoint: AtomicBool,
}

#[salsa::input]
struct GateInput {
    #[returns(ref)]
    gate: Gate,
}

/// Cancellation arrives while this tracked body runs; actual parsing supplies the next checkpoint.
#[salsa::tracked(returns(copy))]
fn gated_parse(db: &dyn RootQueryDb, input: GateInput, file: vfs::FileId) -> bool {
    let gate = input.gate(db);
    gate.0.entered.send(()).unwrap();
    gate.0
        .resume
        .lock()
        .unwrap()
        .recv_timeout(Duration::from_secs(10))
        .unwrap();
    let _ = db.parse(file);
    gate.0.passed_checkpoint.store(true, Ordering::Release);
    true
}

/// Keep one known body ready without loading or asserting complete project coverage.
fn ready(session: &mut AnalysisSession) -> SourcePath {
    let path = SourcePath::parse("CommonModules/Tools/Ext/Module.bsl").unwrap();
    session
        .load_module_saved(&path, session.revision(), &InputRequest::default())
        .unwrap();
    path
}

/// A new query can use the unchanged input after cancellation or an unrelated panic.
fn syntax(session: &AnalysisSession, path: &SourcePath, token: &QueryCancellation) {
    let config = DiagnosticsConfig {
        only_enabled: Some(vec![crate::DiagnosticCode::ParseError]),
        ..DiagnosticsConfig::default()
    };
    assert!(
        session
            .module_diagnostics(path, session.revision(), &config, token)
            .unwrap()
            .value
            .is_empty()
    );
}

/// The running tracked query unwinds before its parser checkpoint, rather than finishing silently.
#[test]
fn cancelling_running_tracked_query_preserves_inputs_and_allows_later_updates() {
    let (_directory, root) = fixture();
    let mut session = AnalysisSession::open(&root).unwrap();
    let path = ready(&mut session);
    let revision = session.revision();
    let before = session.module(&path).unwrap().clone();
    let token = QueryCancellation::default();
    let request = token.clone();
    let (entered, started) = mpsc::sync_channel(1);
    let (resume, receiver) = mpsc::sync_channel(1);
    let gate = Gate(Arc::new(Signals {
        entered,
        resume: Mutex::new(receiver),
        passed_checkpoint: AtomicBool::new(false),
    }));
    let probe = GateInput::new(&session.database, gate.clone());
    let worker_path = path.clone();
    let worker = std::thread::spawn(move || {
        let result = session.query_module(&worker_path, revision, &request, |analysis, file| {
            Ok(gated_parse(analysis.database(), probe, file))
        });
        (result, session)
    });
    started.recv_timeout(Duration::from_secs(10)).unwrap();
    token.cancel();
    resume.send(()).unwrap();
    let (result, mut session) = worker.join().unwrap();
    assert_eq!(result, Err(QueryError::Cancelled));
    assert!(!gate.0.passed_checkpoint.load(Ordering::Acquire));
    assert_eq!(session.revision(), revision);
    assert_eq!(session.module(&path), Some(&before));
    assert!(token.is_cancelled());
    syntax(&session, &path, &QueryCancellation::default());
    session
        .apply_module_overlay(
            &path,
            revision,
            1,
            "Процедура ПослеОтмены()\nКонецПроцедуры\n",
            &InputRequest::default(),
        )
        .unwrap();
    assert_ne!(session.revision(), revision);
    syntax(&session, &path, &QueryCancellation::default());
}

/// Reusing a token concurrently is rejected; cancellation reaches only its registered handle.
#[test]
fn token_registration_is_exclusive_and_does_not_cancel_another_handle() {
    let database = RootDatabaseImpl::new();
    let neighbor = database.clone();
    let first = database.cancellation_token();
    let second = neighbor.cancellation_token();
    let token = QueryCancellation::default();
    let other = QueryCancellation::default();
    let registration = token.register(first.clone()).unwrap();
    let neighbor_registration = other.register(second.clone()).unwrap();
    assert!(matches!(
        token.register(second.clone()),
        Err(QueryError::CancellationTokenInUse)
    ));
    token.cancel();
    assert!(first.is_cancelled());
    assert!(!second.is_cancelled());
    assert!(!other.is_cancelled());
    drop(registration);
    assert!(matches!(token.register(second), Err(QueryError::Cancelled)));
    drop(neighbor_registration);
    let later = neighbor.cancellation_token();
    let registration = other.register(later.clone()).unwrap();
    drop(registration);
    other.cancel();
    assert!(!later.is_cancelled());
}

/// Kernel cancellation catches do not swallow unrelated panics or leak a registered database handle.
#[test]
fn unrelated_panic_is_rethrown_and_registration_is_removed() {
    let (_directory, root) = fixture();
    let mut session = AnalysisSession::open(&root).unwrap();
    let path = ready(&mut session);
    let revision = session.revision();
    let token = QueryCancellation::default();
    let panic = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        session.query_module::<()>(&path, revision, &token, |_, _| {
            panic!("unrelated_test_failure")
        })
    }));
    assert!(panic.is_err());
    assert!(!token.is_cancelled());
    assert_eq!(session.revision(), revision);
    syntax(&session, &path, &token);
    session
        .close_module_overlay(&path, revision, &InputRequest::default())
        .unwrap();
    syntax(&session, &path, &token);
}
