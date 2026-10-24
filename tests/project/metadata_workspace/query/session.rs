use super::*;
use crate::backend::project::metadata_workspace::{
    ProjectSession, QueryEditorState, QueryOwnerFormat, QueryOwnerRestrictions, QuerySession,
    QuerySessionError, QuerySessionRestore,
};
use std::sync::Arc;

mod projection;
mod protection;

/// Recovery discovers only the declared owner branch; another IDE need not rebuild an expanded metadata tree first.
#[test]
fn query_session_restores_an_unexpanded_module_after_reopening_project() {
    let (directory, mut workspace, owner, _) = xml_fixture();
    let path = PathBuf::from("CommonForms/Queries/Ext/Form/Module.bsl");
    fs::create_dir_all(directory.0.join("src/CommonForms/Queries/Ext/Form")).unwrap();
    fs::write(directory.0.join("src").join(&path), INPUT).unwrap();
    let project = workspace.project_mut(&ProjectScope::Standalone).unwrap();
    project.children(&owner, TreeOptions::default()).unwrap();
    let NodeId::Object(id) = owner else {
        panic!("form owner")
    };
    let node = NodeId::Module {
        owner: id,
        role: ModuleRole::Module,
    };
    let input = INPUT.replace("// keep", "// dirty captured owner");
    let document = project
        .query_document(&node, &path, 7, Some(&input))
        .unwrap();
    let selected = identity(&document);
    let mut session = QuerySession::open(
        project,
        document,
        selected,
        None,
        QueryOwnerFormat::Source,
        QueryOwnerRestrictions::default(),
    )
    .unwrap();
    session
        .set_draft(session.revision(), Arc::from("SELECT recovered"))
        .unwrap();
    let checkpoint = saved(project, &session);
    session.close();
    drop(workspace);
    let mut fresh = MetadataWorkspace::open(&directory.0, &[], false).unwrap();
    let project = fresh.project_mut(&ProjectScope::Standalone).unwrap();
    assert!(
        project.node(&node).is_err(),
        "no UI has expanded the module owner"
    );
    let mut restored =
        QuerySession::restore(project, checkpoint, QueryOwnerFormat::Source).unwrap();
    assert!(!restored.resumed());
    assert_eq!(restored.document().input(), input);
    assert_eq!(restored.draft(), "SELECT recovered");
    restored
        .resume(
            project,
            editor(1, &snapshot(&input), true),
            QueryOwnerRestrictions::default(),
        )
        .unwrap();
    assert!(restored.resumed());
    assert_eq!(
        fs::read_to_string(directory.0.join("src").join(&path)).unwrap(),
        INPUT
    );
}

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
const fn editor(version: u64, snapshot: &str, dirty: bool) -> QueryEditorState<'_> {
    QueryEditorState {
        version,
        snapshot,
        is_dirty: dirty,
        user_read_only: false,
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
            editor(7, &owner_hash, true),
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
            editor(7, &owner_hash, true),
            &[],
            None
        ),
        Err(QuerySessionError::ApplyBusy)
    ));
    session
        .set_draft(session.revision(), Arc::from("SELECT newer"))
        .unwrap();
    assert!(matches!(
        session.applied(project, prepared.token, editor(8, &owner_hash, true)),
        Err(QuerySessionError::InvalidReceipt)
    ));
    let output_hash = snapshot(prepared.plan.output());
    session
        .applied(project, prepared.token, editor(8, &output_hash, true))
        .unwrap();
    assert_eq!(session.document().input(), prepared.plan.output());
    assert_eq!(session.base(), "SELECT 3;\n// пакет 😀\nSELECT 2");
    assert_eq!(session.draft(), "SELECT newer");
    assert_eq!(bytes(&directory.0), original);
    assert!(matches!(
        session.applied(project, prepared.token, editor(8, &output_hash, true)),
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
            editor(7, &hash, false),
            &[],
            None,
        )
        .unwrap();
    assert!(prepared.save_owner);
    let output_hash = snapshot(prepared.plan.output());
    assert!(
        session
            .applied(project, prepared.token, editor(8, &output_hash, false))
            .is_err()
    );
    fs::write(
        directory.0.join("src").join(&path),
        format!("\u{feff}{}", prepared.plan.output()),
    )
    .unwrap();
    session
        .applied(project, prepared.token, editor(8, &output_hash, false))
        .unwrap();
    assert_eq!(
        session.document().disk_snapshot(),
        snapshot(&format!("\u{feff}{}", prepared.plan.output()))
    );
    assert_eq!(session.base(), "SELECT saved");
    session.close();
    let dirty = input.replace("<other>keep</other>", "<other>unsaved</other>");
    let mut false_clean = xml_session(project, &node, &path, &dirty);
    assert!(
        false_clean
            .prepare(
                project,
                false_clean.revision(),
                editor(7, &snapshot(&dirty), false),
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
    let checkpoint = saved(project, &original);
    original.close();
    let mut recovered =
        QuerySession::restore(project, checkpoint, QueryOwnerFormat::Editor).unwrap();
    let owner_hash = snapshot(&input);
    let current = editor(1, &owner_hash, true);
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
    let clean = editor(2, &owner_hash, false);
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
                editor(7, &snapshot(input), false),
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
            editor(8, &snapshot(input), false),
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
            editor(8, &snapshot(input), false),
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
            editor(7, &original, false),
            &[],
            Some(crate::backend::project::query::LineEnding::CrLf),
        )
        .unwrap();
    assert_eq!(prepared.query.text, "ВЫБРАТЬ 2\nКАК Значение");
    let output = snapshot(prepared.plan.output());
    let interrupted = saved(project, &session);
    session
        .applied(project, prepared.token, editor(8, &output, true))
        .unwrap();
    assert_eq!(session.base(), "ВЫБРАТЬ 2\nКАК Значение");
    assert_eq!(session.draft(), "ВЫБРАТЬ 2\r\nКАК Значение");
    let acknowledged = saved(project, &session);
    session.close();
    let mut pending =
        QuerySession::restore(project, interrupted, QueryOwnerFormat::Source).unwrap();
    assert!(!pending.resumed());
    assert!(pending.applying());
    pending
        .resume(
            project,
            editor(1, &output, true),
            QueryOwnerRestrictions {
                user_read_only: false,
            },
        )
        .unwrap();
    assert_eq!(pending.base(), "ВЫБРАТЬ 2\nКАК Значение");
    assert_eq!(pending.draft(), "ВЫБРАТЬ 2\r\nКАК Значение");
    pending.close();
    let mut restored =
        QuerySession::restore(project, acknowledged, QueryOwnerFormat::Source).unwrap();
    restored
        .resume(
            project,
            editor(1, &output, true),
            QueryOwnerRestrictions {
                user_read_only: false,
            },
        )
        .unwrap();
    let no_op = restored
        .prepare(
            project,
            restored.revision(),
            editor(1, &output, true),
            &[],
            None,
        )
        .unwrap();
    assert!(no_op.plan.is_empty());
    restored
        .applied(project, no_op.token, editor(1, &output, true))
        .unwrap();
}

/// Ordinary native edits are allowed, but a changed version cannot overwrite either the owner or the draft.
#[test]
fn query_session_bsl_conflicts_preserve_draft_and_release_only_its_source_restriction() {
    let (directory, mut workspace, node, path) = setup();
    let project = workspace.project_mut(&ProjectScope::Standalone).unwrap();
    let document = project.query_document(&node, &path, 7, None).unwrap();
    let original_disk = fs::read(directory.0.join("src").join(&path)).unwrap();
    let selected = identity(&document);
    let restrictions = QueryOwnerRestrictions::default();
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
            editor(8, &hash, true),
            &[],
            None
        ),
        Err(QuerySessionError::Document(_))
    ));
    assert_eq!(session.draft(), "ВЫБРАТЬ 2\nКАК Значение");
    assert!(!session.resumed());
    assert_eq!(
        fs::read(directory.0.join("src").join(&path)).unwrap(),
        original_disk
    );
    session
        .resume(project, editor(7, &hash, false), restrictions)
        .unwrap();
    let prepared = session
        .prepare(
            project,
            session.revision(),
            editor(7, &hash, false),
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
            editor(7, &hash, false),
            QueryOwnerRestrictions {
                user_read_only: true,
            }
        ),
        Err(QuerySessionError::UserReadOnly)
    ));
    let restriction = session.source_restriction_id();
    assert!(restriction.is_some());
    assert_eq!(session.close(), restriction);
    assert_eq!(
        session.restrictions(),
        QueryOwnerRestrictions {
            user_read_only: true,
        }
    );
    assert_eq!(session.close(), None);
    assert!(matches!(
        session.set_draft(session.revision(), Arc::from("late")),
        Err(QuerySessionError::Closed)
    ));
}
