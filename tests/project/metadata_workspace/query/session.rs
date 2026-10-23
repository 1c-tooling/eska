use super::*;
use crate::backend::project::metadata_workspace::{
    ProjectSession, QueryEditorState, QueryOwnerFormat, QueryOwnerRestrictions, QuerySession,
    QuerySessionError, QuerySessionRestore,
};
use std::sync::Arc;

const XML: &str = "\u{feff}<Form xmlns='http://v8.1c.ru/8.3/xcf/logform' xmlns:xsi='http://www.w3.org/2001/XMLSchema-instance'>\r\n<Attributes><Attribute name='List' id='1'><Settings xsi:type='DynamicList'><ManualQuery>true</ManualQuery><QueryText>SELECT 1;\n// пакет 😀\nSELECT 2</QueryText></Settings></Attribute></Attributes><other>keep</other>\r\n</Form>";

/// Native-editor fixtures share the real declared metadata and support setup, with an independent XML payload.
fn xml_fixture() -> (TestDir, MetadataWorkspace, NodeId, PathBuf) {
    let (directory, _, _, _) = setup();
    let source = directory.0.join("src");
    let root = source.join("Configuration.xml");
    fs::write(
        &root,
        fs::read_to_string(&root).unwrap().replace(
            "<ChildObjects>",
            "<ChildObjects><CommonForm>Queries</CommonForm>",
        ),
    )
    .unwrap();
    fs::create_dir_all(source.join("CommonForms/Queries/Ext")).unwrap();
    fs::write(source.join("CommonForms/Queries.xml"), "<MetaDataObject xmlns='http://v8.1c.ru/8.3/MDClasses'><CommonForm uuid='22222222-2222-2222-2222-222222222222'><Properties><Name>Queries</Name></Properties></CommonForm></MetaDataObject>").unwrap();
    let path = PathBuf::from("CommonForms/Queries/Ext/Form.xml");
    fs::write(source.join(&path), XML).unwrap();
    let workspace = MetadataWorkspace::open(&directory.0, &[], false).unwrap();
    (
        directory,
        workspace,
        NodeId::Object(object(MetadataKind::CommonForm, "Queries", None)),
        path,
    )
}

/// Capture a native model's exact version/hash; its encoding signature is deliberately outside that model.
const fn editor(
    version: u64,
    snapshot: &str,
    dirty: bool,
    owner_lock: Option<u64>,
) -> QueryEditorState<'_> {
    QueryEditorState {
        version,
        snapshot,
        is_dirty: dirty,
        user_read_only: false,
        owner_lock,
        application: None,
    }
}

/// Open a checked XML session from the actual native owner text, without an IDE or analyzer.
fn xml_session(
    project: &mut ProjectSession,
    node: &NodeId,
    path: &Path,
    text: &str,
) -> QuerySession {
    let document = project.query_document(node, path, 7, Some(text)).unwrap();
    let identity = document.xml_queries().unwrap().remove(0).identity;
    QuerySession::open(
        project,
        document,
        identity,
        None,
        QueryOwnerFormat::Editor,
        QueryOwnerRestrictions::default(),
    )
    .unwrap()
}

/// Recovery data is a remembered binding, not a live document handle or automatic resume permission.
fn saved(project: &ProjectSession, session: &QuerySession) -> QuerySessionRestore {
    let document = session.document();
    let (descriptor, descriptor_snapshot) = document.descriptor();
    let [owner_text, base, draft] = session.texts();
    QuerySessionRestore {
        root: project.project().root().to_owned(),
        scope: project.scope().clone(),
        node: document.node().clone(),
        path: document.path().to_owned(),
        descriptor: descriptor.to_owned(),
        descriptor_snapshot: descriptor_snapshot.to_owned(),
        disk_snapshot: document.disk_snapshot().to_owned(),
        owner_snapshot: document.snapshot().to_owned(),
        owner_version: document.version(),
        context_fingerprint: session.context_fingerprint().to_owned(),
        application: session.application().map(str::to_owned),
        identity: session.identity().clone(),
        owner_text,
        base,
        draft,
        writable: document.writable(),
        prepared: session.prepared(),
    }
}

/// A dirty owner stays unsaved, the whole package is retained, and a newer draft survives an older apply receipt.
#[test]
fn query_session_owns_dirty_xml_draft_and_native_application_receipt() {
    let (directory, mut workspace, node, path) = xml_fixture();
    let project = workspace.project_mut(&ProjectScope::Standalone).unwrap();
    let input = XML
        .strip_prefix('\u{feff}')
        .unwrap()
        .replace("<other>keep</other>", "<other>unsaved</other>");
    let mut session = xml_session(project, &node, &path, &input);
    let original = bytes(&directory.0);
    session
        .set_draft(
            session.revision(),
            Arc::from("SELECT 3;\n// пакет 😀\nSELECT 2"),
        )
        .unwrap();
    let owner_hash = snapshot(&input);
    let prepared = session
        .prepare(
            project,
            session.revision(),
            editor(7, &owner_hash, true, None),
            &[],
            Some(crate::backend::project::query::LineEnding::CrLf),
        )
        .unwrap();
    assert!(!prepared.save_owner);
    assert_eq!(bytes(&directory.0), original);
    assert!(prepared.plan.output().contains("<other>unsaved</other>"));
    assert_eq!(prepared.query.text, "SELECT 3;\n// пакет 😀\nSELECT 2");
    assert!(matches!(
        session.prepare(
            project,
            session.revision(),
            editor(7, &owner_hash, true, None),
            &[],
            None
        ),
        Err(QuerySessionError::ApplyBusy)
    ));
    session
        .set_draft(session.revision(), Arc::from("SELECT newer"))
        .unwrap();
    assert!(matches!(
        session.applied(project, prepared.token, editor(8, &owner_hash, true, None)),
        Err(QuerySessionError::InvalidReceipt)
    ));
    let output_hash = snapshot(prepared.plan.output());
    session
        .applied(project, prepared.token, editor(8, &output_hash, true, None))
        .unwrap();
    assert_eq!(session.document().input(), prepared.plan.output());
    assert_eq!(session.base(), "SELECT 3;\n// пакет 😀\nSELECT 2");
    assert_eq!(session.draft(), "SELECT newer");
    assert_eq!(bytes(&directory.0), original);
    assert!(matches!(
        session.applied(project, prepared.token, editor(8, &output_hash, true, None)),
        Err(QuerySessionError::UnknownApply)
    ));
}

/// Clean native saves keep the disk signature; a falsely clean model cannot publish unrelated unsaved bytes.
#[test]
fn query_session_checks_clean_model_and_exact_saved_xml_bytes() {
    let (directory, mut workspace, node, path) = xml_fixture();
    let project = workspace.project_mut(&ProjectScope::Standalone).unwrap();
    let input = XML.strip_prefix('\u{feff}').unwrap();
    let mut session = xml_session(project, &node, &path, input);
    session
        .set_draft(session.revision(), Arc::from("SELECT saved"))
        .unwrap();
    let hash = snapshot(input);
    let prepared = session
        .prepare(
            project,
            session.revision(),
            editor(7, &hash, false, None),
            &[],
            None,
        )
        .unwrap();
    assert!(prepared.save_owner);
    let output_hash = snapshot(prepared.plan.output());
    assert!(
        session
            .applied(
                project,
                prepared.token,
                editor(8, &output_hash, false, None)
            )
            .is_err()
    );
    fs::write(
        directory.0.join("src").join(&path),
        format!("\u{feff}{}", prepared.plan.output()),
    )
    .unwrap();
    session
        .applied(
            project,
            prepared.token,
            editor(8, &output_hash, false, None),
        )
        .unwrap();
    assert_eq!(
        session.document().disk_snapshot(),
        snapshot(&format!("\u{feff}{}", prepared.plan.output()))
    );
    assert_eq!(session.base(), "SELECT saved");
    let dirty = input.replace("<other>keep</other>", "<other>unsaved</other>");
    let mut false_clean = xml_session(project, &node, &path, &dirty);
    assert!(
        false_clean
            .prepare(
                project,
                false_clean.revision(),
                editor(7, &snapshot(&dirty), false, None),
                &[],
                None
            )
            .is_err()
    );
    assert!(!false_clean.resumed());
}

/// Restart counters are process-local; a restored draft requires explicit resume and the same complete owner/context.
#[test]
fn query_session_recovery_is_paused_and_rebinds_only_matching_owner_state() {
    let (directory, mut workspace, node, path) = xml_fixture();
    let project = workspace.project_mut(&ProjectScope::Standalone).unwrap();
    let input = XML
        .strip_prefix('\u{feff}')
        .unwrap()
        .replace("<other>keep</other>", "<other>unsaved</other>");
    let mut original = xml_session(project, &node, &path, &input);
    original
        .set_draft(original.revision(), Arc::from("SELECT recovered"))
        .unwrap();
    let mut recovered =
        QuerySession::restore(project, saved(project, &original), QueryOwnerFormat::Editor)
            .unwrap();
    let owner_hash = snapshot(&input);
    let current = editor(1, &owner_hash, true, None);
    assert!(!recovered.resumed());
    assert!(matches!(
        recovered.prepare(project, recovered.revision(), current, &[], None),
        Err(QuerySessionError::ResumeRequired)
    ));
    recovered
        .resume(project, current, QueryOwnerRestrictions::default())
        .unwrap();
    assert_eq!(recovered.document().version(), 1);
    assert_eq!(recovered.draft(), "SELECT recovered");
    fs::write(
        directory.0.join("src").join(&path),
        format!("\u{feff}{input}"),
    )
    .unwrap();
    assert!(
        recovered
            .prepare(project, recovered.revision(), current, &[], None)
            .is_err()
    );
    assert!(!recovered.resumed());
    assert!(
        recovered
            .resume(project, current, QueryOwnerRestrictions::default())
            .is_err()
    );
    let clean = editor(2, &owner_hash, false, None);
    recovered
        .resume(project, clean, QueryOwnerRestrictions::default())
        .unwrap();
    let prepared = recovered
        .prepare(project, recovered.revision(), clean, &[], None)
        .unwrap();
    assert!(prepared.save_owner);
}

/// Full descriptors and manifests are independent guards; a failed preflight preserves all three texts.
#[test]
fn query_session_conflicts_pause_without_changing_owner_base_or_draft() {
    let (directory, mut workspace, node, path) = xml_fixture();
    let project = workspace.project_mut(&ProjectScope::Standalone).unwrap();
    let input = XML.strip_prefix('\u{feff}').unwrap();
    let mut session = xml_session(project, &node, &path, input);
    session
        .set_draft(session.revision(), Arc::from("SELECT keep draft"))
        .unwrap();
    let before = session.texts();
    let descriptor = directory.0.join("src/CommonForms/Queries.xml");
    let original = fs::read_to_string(&descriptor).unwrap();
    fs::write(&descriptor, format!("{original}\n<!-- external -->")).unwrap();
    assert!(
        session
            .prepare(
                project,
                session.revision(),
                editor(7, &snapshot(input), false, None),
                &[],
                None
            )
            .is_err()
    );
    assert!(!session.resumed());
    assert_eq!(session.texts(), before);
    fs::write(descriptor, original).unwrap();
    session
        .resume(
            project,
            editor(8, &snapshot(input), false, None),
            QueryOwnerRestrictions::default(),
        )
        .unwrap();
    let manifest = directory.0.join("eska.toml");
    let original = fs::read_to_string(&manifest).unwrap();
    fs::write(&manifest, format!("{original}\n# changed context\n")).unwrap();
    assert!(matches!(
        session.prepare(
            project,
            session.revision(),
            editor(8, &snapshot(input), false, None),
            &[],
            None
        ),
        Err(QuerySessionError::ContextChanged)
    ));
    assert_eq!(session.texts(), before);
}

/// A BSL receipt records the decoded LF baseline even when the exact editable draft used CRLF.
#[test]
fn query_session_bsl_can_reapply_and_restore_after_normalizing_draft_line_endings() {
    let (_directory, mut workspace, node, path) = setup();
    let project = workspace.project_mut(&ProjectScope::Standalone).unwrap();
    let document = project.query_document(&node, &path, 7, None).unwrap();
    let selected = identity(&document);
    let mut session = QuerySession::open(
        project,
        document,
        selected,
        None,
        QueryOwnerFormat::Source,
        QueryOwnerRestrictions {
            user_read_only: false,
            owner_lock: Some(19),
        },
    )
    .unwrap();
    session
        .set_draft(session.revision(), Arc::from("ВЫБРАТЬ 2\r\nКАК Значение"))
        .unwrap();
    let original = session.document().snapshot().to_owned();
    let prepared = session
        .prepare(
            project,
            session.revision(),
            editor(7, &original, false, Some(19)),
            &[],
            Some(crate::backend::project::query::LineEnding::CrLf),
        )
        .unwrap();
    assert_eq!(prepared.query.text, "ВЫБРАТЬ 2\nКАК Значение");
    let output = snapshot(prepared.plan.output());
    let mut pending =
        QuerySession::restore(project, saved(project, &session), QueryOwnerFormat::Source).unwrap();
    assert!(!pending.resumed());
    assert!(pending.applying());
    pending
        .resume(
            project,
            editor(1, &output, true, Some(20)),
            QueryOwnerRestrictions {
                user_read_only: false,
                owner_lock: Some(20),
            },
        )
        .unwrap();
    assert_eq!(pending.base(), "ВЫБРАТЬ 2\nКАК Значение");
    assert_eq!(pending.draft(), "ВЫБРАТЬ 2\r\nКАК Значение");
    session
        .applied(project, prepared.token, editor(8, &output, true, Some(19)))
        .unwrap();
    assert_eq!(session.base(), "ВЫБРАТЬ 2\nКАК Значение");
    assert_eq!(session.draft(), "ВЫБРАТЬ 2\r\nКАК Значение");
    let mut restored =
        QuerySession::restore(project, saved(project, &session), QueryOwnerFormat::Source).unwrap();
    restored
        .resume(
            project,
            editor(1, &output, true, Some(20)),
            QueryOwnerRestrictions {
                user_read_only: false,
                owner_lock: Some(20),
            },
        )
        .unwrap();
    let no_op = restored
        .prepare(
            project,
            restored.revision(),
            editor(1, &output, true, Some(20)),
            &[],
            None,
        )
        .unwrap();
    assert!(no_op.plan.is_empty());
    restored
        .applied(project, no_op.token, editor(1, &output, true, Some(20)))
        .unwrap();
}

/// Headless receipts model a verified adapter contract; they do not prove native input prevention in any IDE.
#[test]
fn query_session_bsl_requires_its_lock_and_release_preserves_independent_user_restrictions() {
    let (_directory, mut workspace, node, path) = setup();
    let project = workspace.project_mut(&ProjectScope::Standalone).unwrap();
    let document = project.query_document(&node, &path, 7, None).unwrap();
    let selected = identity(&document);
    assert!(matches!(
        QuerySession::open(
            project,
            document.clone(),
            selected.clone(),
            None,
            QueryOwnerFormat::Source,
            QueryOwnerRestrictions::default()
        ),
        Err(QuerySessionError::OwnerLockRequired)
    ));
    let restrictions = QueryOwnerRestrictions {
        user_read_only: false,
        owner_lock: Some(19),
    };
    let mut session = QuerySession::open(
        project,
        document,
        selected,
        None,
        QueryOwnerFormat::Source,
        restrictions,
    )
    .unwrap();
    session
        .set_draft(session.revision(), Arc::from("ВЫБРАТЬ 2\nКАК Значение"))
        .unwrap();
    let hash = session.document().snapshot().to_owned();
    assert!(matches!(
        session.prepare(
            project,
            session.revision(),
            editor(7, &hash, false, None),
            &[],
            None
        ),
        Err(QuerySessionError::OwnerLockRequired)
    ));
    session
        .resume(project, editor(7, &hash, false, Some(19)), restrictions)
        .unwrap();
    let prepared = session
        .prepare(
            project,
            session.revision(),
            editor(7, &hash, false, Some(19)),
            &[],
            None,
        )
        .unwrap();
    assert!(
        !prepared.save_owner,
        "BSL Apply never asks its adapter to save the module"
    );
    session.abort_apply(prepared.token).unwrap();
    assert!(matches!(
        session.resume(
            project,
            editor(7, &hash, false, Some(19)),
            QueryOwnerRestrictions {
                user_read_only: true,
                owner_lock: Some(19)
            }
        ),
        Err(QuerySessionError::UserReadOnly)
    ));
    assert_eq!(session.close(), Some(19));
    assert_eq!(
        session.restrictions(),
        QueryOwnerRestrictions {
            user_read_only: true,
            owner_lock: None
        }
    );
    assert_eq!(session.close(), None);
    assert!(matches!(
        session.set_draft(session.revision(), Arc::from("late")),
        Err(QuerySessionError::Closed)
    ));
}
