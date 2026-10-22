use super::super::{
    json::{SemanticErrorDetailDocument, SemanticErrorDocument, change_name, semantic_error_code},
    raw::render_raw,
};

use super::{
    HumanState, SemanticHumanChange, SemanticHumanStage, append_semantic_event_groups,
    change_marker, human_state_title, raw_code, render_human,
};
use crate::{
    cli::localization::{Locale, Localizer},
    project::{
        ProjectType,
        diff::{DisplayChange, DisplayTarget, FileChange, ProjectDiff},
        metadata,
        semantic::{SemanticDiffError, SemanticEventKind, SourceLocation},
    },
    vcs::status::Change,
};

/// Machine names and raw codes are exhaustive and locale-independent.
#[test]
fn stable_change_representations_cover_every_state() {
    let values = [
        (Change::Added, "added", 'A', '+'),
        (Change::Modified, "modified", 'M', '✎'),
        (Change::Deleted, "deleted", 'D', '−'),
        (Change::TypeChanged, "type_changed", 'T', '↔'),
        (Change::Untracked, "untracked", '?', '?'),
        (Change::IntentToAdd, "intent_to_add", 'I', '◌'),
        (Change::Conflict, "conflict", 'U', '!'),
    ];
    for (change, name, code, marker) in values {
        assert_eq!(change_name(change), name);
        assert_eq!(raw_code(Some(change)), code);
        assert_eq!(change_marker(change), marker);
    }
    assert_eq!(raw_code(None), '.');
}

/// Fatal semantic JSON remains distinct from a successful incomplete analysis.
#[test]
fn semantic_error_document_has_a_stable_machine_code() {
    let error = SemanticDiffError::ProjectOutsideRepository {
        project: "/project".into(),
        repository: "/repository".into(),
    };
    let document = SemanticErrorDocument {
        schema_version: 4,
        kind: "semantic",
        status: "error",
        error: SemanticErrorDetailDocument {
            code: semantic_error_code(&error),
        },
    };

    assert_eq!(
        serde_json::to_value(document).expect("semantic error JSON"),
        serde_json::json!({
            "schema_version": 4,
            "kind": "semantic",
            "status": "error",
            "error": {"code": "project-outside-repository"}
        })
    );
}

/// Raw output keeps both comparison stages and deterministic path order.
#[test]
fn raw_output_has_two_state_columns() {
    let diff = ProjectDiff {
        files: vec![FileChange {
            path: BString::from("src/module.bsl"),
            index: Some(Change::Modified),
            worktree: Some(Change::Deleted),
        }],
        display: Vec::new(),
    };
    assert_eq!(render_raw(&diff), "MD\tsrc/module.bsl\n");
}

/// Workspace groups retain both stages while using the latest state marker.
#[test]
fn workspace_state_heading_describes_both_stages() {
    let state = HumanState::IndexAndWorktree {
        index: Change::Added,
        worktree: Change::Modified,
    };
    for (locale, expected) in [
        (Locale::RuRu, "Добавлены — индекс; Изменены — рабочая копия"),
        (Locale::EnUs, "Added — index; Modified — working tree"),
    ] {
        let localizer = Localizer::try_new(locale).unwrap();
        assert_eq!(human_state_title(state, &localizer), expected);
    }
    assert_eq!(change_marker(super::marker_change(state)), '✎');
}

/// Human output localizes Configurator identities, groups them and keeps other files last.
#[test]
fn human_output_groups_logical_metadata() {
    let catalog = metadata::from_path(
        ProjectType::Configuration,
        "Catalogs/Контрагенты.xml".as_bytes().as_bstr(),
    )
    .unwrap()
    .with_suffix(&[metadata::MetadataPart {
        kind: "attribute",
        name: Some("Реквизит1".to_owned()),
    }]);
    let diff = ProjectDiff {
        files: Vec::new(),
        display: vec![
            DisplayChange {
                target: DisplayTarget::Metadata(catalog),
                index: None,
                worktree: Some(Change::Modified),
            },
            DisplayChange {
                target: DisplayTarget::File("notes.txt".into()),
                index: None,
                worktree: Some(Change::Modified),
            },
        ],
    };

    for (locale, logical, other) in [
        (
            Locale::RuRu,
            "Справочник.Контрагенты.Реквизит.Реквизит1",
            "Прочие файлы",
        ),
        (
            Locale::EnUs,
            "Catalog.Контрагенты.Attribute.Реквизит1",
            "Other files",
        ),
    ] {
        let localizer = Localizer::try_new(locale).unwrap();
        let output = render_human(&diff, &localizer, false);
        assert!(output.contains(logical), "{output}");
        assert!(output.find(logical).unwrap() < output.find(other).unwrap());
        assert!(output.contains(&format!("    ✎ {logical}")), "{output}");
        assert!(!output.contains("\x1b["), "{output:?}");

        let styled = render_human(&diff, &localizer, true);
        assert!(styled.contains("\x1b[1;36m"), "{styled:?}");
        assert!(styled.contains("\x1b[1;33m✎\x1b[0m"), "{styled:?}");
        assert!(styled.contains(logical), "{styled}");
    }
}

/// Semantic groups reuse localized headings, counts, markers and the TTY color palette.
#[test]
fn semantic_groups_match_file_diff_presentation() {
    let localizer = Localizer::try_new(Locale::RuRu).unwrap();
    let changes = vec![
        SemanticHumanChange {
            object: "ОбщийМодуль.Обмен".to_owned(),
            target: "ОбщийМодуль.Обмен — Процедура.Выполнить (4, 2)".to_owned(),
            location: Some(SourceLocation { line: 4, column: 2 }),
            kind: SemanticEventKind::MethodChanged,
            stage: SemanticHumanStage::Worktree,
        },
        SemanticHumanChange {
            object: "ОбщийМодуль.Обмен".to_owned(),
            target: "ОбщийМодуль.Обмен — Функция.Значение (9, 1)".to_owned(),
            location: Some(SourceLocation { line: 9, column: 1 }),
            kind: SemanticEventKind::FunctionChanged,
            stage: SemanticHumanStage::Worktree,
        },
        SemanticHumanChange {
            object: "ОбщийМодуль.Новый".to_owned(),
            target: "ОбщийМодуль.Новый".to_owned(),
            location: None,
            kind: SemanticEventKind::ObjectAdded,
            stage: SemanticHumanStage::Index,
        },
    ];
    let mut plain = Vec::new();
    append_semantic_event_groups(&mut plain, changes, &localizer, false);
    let plain = plain.join("\n");
    assert!(
        plain.contains(
            "Изменён метод — в рабочей копии (2):\n    ✎ ОбщийМодуль.Обмен — Процедура.Выполнить (4, 2)"
        ),
        "{plain}"
    );
    assert!(
        plain.contains("Добавлен объект — в индексе (1):\n    + ОбщийМодуль.Новый"),
        "{plain}"
    );
    assert!(!plain.contains("\x1b["), "{plain:?}");

    let mut styled = Vec::new();
    append_semantic_event_groups(
        &mut styled,
        vec![SemanticHumanChange {
            object: "ОбщийМодуль.Обмен".to_owned(),
            target: "ОбщийМодуль.Обмен — Процедура.Выполнить (4, 2)".to_owned(),
            location: Some(SourceLocation { line: 4, column: 2 }),
            kind: SemanticEventKind::MethodChanged,
            stage: SemanticHumanStage::Worktree,
        }],
        &localizer,
        true,
    );
    let styled = styled.join("\n");
    assert!(styled.contains("\x1b[1;33m"), "{styled:?}");
    assert!(styled.contains("\x1b[1;33m✎\x1b[0m"), "{styled:?}");
}
use gix::bstr::{BString, ByteSlice};
