use super::*;
use crate::backend::project::{
    metadata_edit::Replacement,
    metadata_workspace::{QueryProjectionTarget, SourceAccessRequest},
    query::{Affinity, byte_position},
};

/// Language projection keeps the dirty full owner, maps UTF-16 and owns no source-write lease after its session closes.
#[test]
fn query_projection_retains_exact_owner_and_maps_draft_and_unchanged_code() {
    let (directory, mut workspace, node, path) = setup();
    let project = workspace.project_mut(&ProjectScope::Standalone).unwrap();
    let input = INPUT.replace("// keep", "// dirty owner 😀");
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
    let draft = "ВЫБРАТЬ \"😀\" КАК Значение\nИЗ Справочник.Товары";
    session
        .set_draft(session.revision(), Arc::from(draft))
        .unwrap();
    let revision = session.revision();
    let projection = session
        .language_projection(project, revision, editor(7, &snapshot(&input), true))
        .unwrap();
    assert_eq!(projection.revision(), revision);
    assert_eq!(projection.draft(), draft);
    assert!(projection.owner().contains("\"\"😀\"\""));
    assert!(
        projection
            .owner()
            .contains("\r\n\t\t|ИЗ Справочник.Товары\"; // dirty owner 😀")
    );
    assert_eq!(projection.snapshot(), snapshot(projection.owner()));
    assert!(!session.applying());
    assert_eq!(session.revision(), revision);
    for offset in draft
        .char_indices()
        .map(|(offset, _)| offset)
        .chain([draft.len()])
    {
        let point = byte_position(draft, offset).unwrap();
        let projected = projection.owner_position(point, Affinity::After).unwrap();
        assert_eq!(projection.draft_position(projected), Some(point));
        assert_eq!(
            projection.target(projected),
            Some(QueryProjectionTarget::Draft(point))
        );
    }
    let tail = byte_position(
        projection.owner(),
        projection.owner().find("КонецПроцедуры").unwrap(),
    )
    .unwrap();
    let original = byte_position(&input, input.find("КонецПроцедуры").unwrap()).unwrap();
    assert_eq!(
        projection.target(tail),
        Some(QueryProjectionTarget::Owner(original))
    );
    session.close();
    assert!(
        project
            .source_protection(
                &[SourceAccessRequest {
                    path: path.clone(),
                    user_read_only: false
                }],
                true
            )
            .unwrap()[0]
            .writable()
    );
    assert_eq!(projection.draft(), draft);
    assert_eq!(
        fs::read_to_string(directory.0.join("src").join(path)).unwrap(),
        INPUT
    );
}

/// Completion edits are decoded centrally and cannot alter BSL syntax or a neighbouring expression.
#[test]
fn query_projection_translates_only_complete_in_literal_language_edits() {
    let (_directory, mut workspace, node, path) = setup();
    let project = workspace.project_mut(&ProjectScope::Standalone).unwrap();
    let document = project.query_document(&node, &path, 7, None).unwrap();
    let selected = identity(&document);
    let hash = document.snapshot().to_owned();
    let mut session = QuerySession::open(
        project,
        document,
        selected,
        None,
        QueryOwnerFormat::Source,
        QueryOwnerRestrictions::default(),
    )
    .unwrap();
    let draft = "ВЫБРАТЬ Товар.Ар КАК \"Имя\"";
    session
        .set_draft(session.revision(), Arc::from(draft))
        .unwrap();
    let projection = session
        .language_projection(project, session.revision(), editor(7, &hash, true))
        .unwrap();
    let start = projection.owner().find("Ар КАК").unwrap();
    let end = start + "Ар".len();
    let translated = projection
        .translate_edits(vec![Replacement {
            range: start..end,
            text: "Артикул".into(),
        }])
        .unwrap();
    assert_eq!(translated.original(), draft);
    assert_eq!(translated.output(), "ВЫБРАТЬ Товар.Артикул КАК \"Имя\"");
    assert_eq!(translated.replacements().len(), 1);
    assert_eq!(translated.replacements()[0].text, "тикул");
    assert_eq!(translated.replacements()[0].range.len(), 0);
    assert!(
        projection
            .translate_edits(vec![Replacement {
                range: 0..0,
                text: "outside".into()
            }])
            .is_err()
    );
    assert!(
        projection
            .translate_edits(vec![Replacement {
                range: start..end,
                text: "\"; ДругойКод = \"".into()
            }])
            .is_err()
    );
    assert!(
        projection
            .translate_edits(vec![
                Replacement {
                    range: start..end,
                    text: "X".into()
                },
                Replacement {
                    range: start..end,
                    text: "Y".into()
                }
            ])
            .is_err()
    );
    let quote = projection.owner().find("\"\"Имя").unwrap() + 1;
    assert!(
        projection
            .translate_edits(vec![Replacement {
                range: quote..quote,
                text: "X".into()
            }])
            .is_err()
    );
    assert!(projection.translate_edits(Vec::new()).unwrap().is_empty());
    assert_eq!(session.draft(), draft);
    assert!(!session.applying());
}

/// Analysis needs read permission, while stale buffers, XML sources and unresumed recovery cannot borrow a BSL projection.
#[test]
fn query_projection_checks_binding_without_granting_apply_permission() {
    let (_directory, mut workspace, node, path) = setup();
    let project = workspace.project_mut(&ProjectScope::Standalone).unwrap();
    let document = project.query_document(&node, &path, 7, None).unwrap();
    let selected = identity(&document);
    let hash = document.snapshot().to_owned();
    let session = QuerySession::open(
        project,
        document,
        selected,
        None,
        QueryOwnerFormat::Source,
        QueryOwnerRestrictions {
            user_read_only: true,
        },
    )
    .unwrap();
    assert!(
        session
            .language_projection(project, session.revision(), editor(7, &hash, false))
            .is_ok()
    );
    assert!(matches!(
        session.language_projection(project, session.revision() + 1, editor(7, &hash, false)),
        Err(QuerySessionError::StaleDraft)
    ));
    assert!(
        session
            .language_projection(project, session.revision(), editor(8, &hash, false))
            .is_err()
    );
    let checkpoint = saved(project, &session);
    drop(session);
    let restored = QuerySession::restore(project, checkpoint, QueryOwnerFormat::Source).unwrap();
    assert!(matches!(
        restored.language_projection(project, restored.revision(), editor(7, &hash, false)),
        Err(QuerySessionError::ResumeRequired)
    ));
    drop(restored);
    let (_xml_directory, mut xml_workspace, xml_node, xml_path) = xml_fixture();
    let project = xml_workspace
        .project_mut(&ProjectScope::Standalone)
        .unwrap();
    let mut xml = xml_session(project, &xml_node, &xml_path, XML);
    assert!(matches!(
        xml.language_projection(project, xml.revision(), editor(7, &snapshot(XML), false)),
        Err(QuerySessionError::Document(_))
    ));
    let checkpoint = saved(project, &xml);
    xml.close();
    let restored = QuerySession::restore(project, checkpoint, QueryOwnerFormat::Source).unwrap();
    assert!(!restored.resumed());
}
