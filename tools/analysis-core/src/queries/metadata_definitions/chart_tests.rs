//! Chart definitions retain real source identities without turning implicit rows into source objects.

use super::*;
use crate::session::tests::fixture;
use eska::{
    analysis_core::InputRequest,
    project_core::source_format::{NativeUuid, import_designer},
};
use std::fs;

#[path = "../../../tests/definition_chart_queries/fixture.rs"]
mod charts;

const CALLER: &str = "Catalogs/Sample/Ext/ObjectModule.bsl";

/// Only private native/source directories are expanded; all three chart models are explicitly selected.
fn session() -> (
    crate::session::tests::support::TestDirectory,
    AnalysisSession,
) {
    let (directory, _) = fixture();
    charts::extend(&directory.0.join("native"));
    let root = directory.0.join("chart-source");
    import_designer(&directory.0.join("native"), &root).unwrap();
    let mut session = AnalysisSession::open(&root).unwrap();
    for case in &charts::CASES {
        let owner =
            SourcePath::parse(&format!("{}/{}/object.yaml", case.folder, case.name)).unwrap();
        session
            .load_saved(&owner, session.revision(), &InputRequest::default())
            .unwrap();
        assert!(matches!(
            session.owner(&owner).unwrap().state,
            ModelState::Ready(_)
        ));
    }
    (directory, session)
}

/// Publish a current caller before choosing a byte inside the complete identifier.
fn query(
    session: &mut AnalysisSession,
    body: &str,
    field: &str,
    version: i64,
) -> Result<Option<MetadataLocation>, QueryError> {
    let caller = SourcePath::parse(CALLER).unwrap();
    session
        .apply_module_overlay(
            &caller,
            session.revision(),
            version,
            body,
            &InputRequest::default(),
        )
        .unwrap();
    let at = u32::try_from(body.rfind(field).unwrap() + field.chars().next().unwrap().len_utf8())
        .unwrap();
    session
        .module_metadata_definition(
            &caller,
            session.revision(),
            at,
            &QueryCancellation::default(),
        )
        .map(|result| result.value)
}

/// Declared roots, fields, tables/columns and both account flag levels match independent UUID-based locations.
#[test]
fn chart_members_and_real_implicit_row_flags_navigate_to_exact_source_declarations() {
    let (_directory, mut session) = session();
    let mut version = 1;
    for case in &charts::CASES {
        let owner =
            SourcePath::parse(&format!("{}/{}/object.yaml", case.folder, case.name)).unwrap();
        for (receiver, field, part) in [
            ("", "ExtraField", 1),
            ("", "Rows", 2),
            ("Object=Object.Rows.Add();", "Quantity", 3),
        ] {
            let body = charts::body(case, receiver, field);
            let target = query(&mut session, &body, field, version).unwrap().unwrap();
            version += 1;
            assert_eq!(
                target,
                session
                    .metadata_location(
                        &owner,
                        session.revision(),
                        &NativeUuid::parse(&charts::uuid(case, part)).unwrap(),
                        None
                    )
                    .unwrap()
            );
        }
    }
    let case = &charts::CASES[1];
    for (receiver, field, part) in [
        ("", "Accounting", 4),
        (
            "Object=Object.ExtDimensionTypes.Add();",
            "QuantityAccounting",
            5,
        ),
    ] {
        let body = charts::body(case, receiver, field);
        let target = query(&mut session, &body, field, version).unwrap().unwrap();
        version += 1;
        assert_eq!(target.uuid, charts::uuid(case, part));
    }
    for (case, receiver, field) in [
        (&charts::CASES[1], "", "ВидыСубконто"),
        (
            &charts::CASES[1],
            "Object=Object.ExtDimensionTypes.Add();",
            "ExtDimensionType",
        ),
        (
            &charts::CASES[1],
            "Object=Object.ExtDimensionTypes.Add();",
            "Accounting",
        ),
        (&charts::CASES[1], "", "QuantityAccounting"),
        (&charts::CASES[2], "", "БазовыеВидыРасчета"),
        (
            &charts::CASES[2],
            "Object=Object.BaseCalculationTypes.Add();",
            "CalculationType",
        ),
    ] {
        let body = charts::body(case, receiver, field);
        assert!(
            query(&mut session, &body, field, version)
                .unwrap()
                .is_none()
        );
        version += 1;
    }
}

/// Identical account/row flag names remain different declarations, scoped by their actual physical kinds.
#[test]
fn same_named_account_and_ext_dimension_flags_select_distinct_real_uuids() {
    let (_directory, mut session) = session();
    let case = &charts::CASES[1];
    let owner = SourcePath::parse("ChartsOfAccounts/Accounts/object.yaml").unwrap();
    let saved = fs::read_to_string(session.root().join(owner.as_path())).unwrap();
    let draft = saved.replace("QuantityAccounting", "Accounting");
    session
        .apply_overlay(
            &owner,
            session.revision(),
            7,
            &draft,
            &InputRequest::default(),
        )
        .unwrap();
    for (version, receiver, part) in [(1, "", 4), (2, "Object=Object.ExtDimensionTypes.Add();", 5)]
    {
        let body = charts::body(case, receiver, "Accounting");
        let target = query(&mut session, &body, "Accounting", version)
            .unwrap()
            .unwrap();
        let uuid = NativeUuid::parse(&charts::uuid(case, part)).unwrap();
        assert_eq!(
            target,
            session
                .metadata_location(&owner, session.revision(), &uuid, None)
                .unwrap()
        );
    }
}

/// A real row flag follows its editor rename, invalid source and missing close through one canonical slot.
#[test]
fn account_row_flag_navigation_preserves_current_editor_authority() {
    let (_directory, mut session) = session();
    let case = &charts::CASES[1];
    let owner = SourcePath::parse("ChartsOfAccounts/Accounts/object.yaml").unwrap();
    let file = session.root().join(owner.as_path());
    let saved = fs::read_to_string(&file).unwrap();
    let body = charts::body(
        case,
        "Object=Object.ExtDimensionTypes.Add();",
        "QuantityAccounting",
    );
    let first = query(&mut session, &body, "QuantityAccounting", 1)
        .unwrap()
        .unwrap();
    let draft = format!(
        "# Я😀 current chart\r\n{}",
        saved.replace("QuantityAccounting", "RowAccounting")
    );
    session
        .apply_overlay(
            &owner,
            session.revision(),
            7,
            &draft,
            &InputRequest::default(),
        )
        .unwrap();
    fs::remove_file(&file).unwrap();
    assert!(
        query(&mut session, &body, "QuantityAccounting", 2)
            .unwrap()
            .is_none()
    );
    let renamed = charts::body(
        case,
        "Object=Object.ВидыСубконто.Добавить();",
        "RowAccounting",
    );
    let current = query(&mut session, &renamed, "RowAccounting", 3)
        .unwrap()
        .unwrap();
    assert_eq!(current.uuid, first.uuid);
    assert_ne!(current.range, first.range);
    assert_ne!(current.source_sha256, first.source_sha256);
    session
        .apply_overlay(
            &owner,
            session.revision(),
            8,
            "broken: [\n",
            &InputRequest::default(),
        )
        .unwrap();
    assert!(
        query(&mut session, &renamed, "RowAccounting", 4)
            .unwrap()
            .is_none()
    );
    session
        .close_overlay(&owner, session.revision(), &InputRequest::default())
        .unwrap();
    assert!(
        query(&mut session, &body, "QuantityAccounting", 5)
            .unwrap()
            .is_none()
    );
    fs::write(&file, saved).unwrap();
    session
        .load_saved(&owner, session.revision(), &InputRequest::default())
        .unwrap();
    assert_eq!(
        query(&mut session, &body, "QuantityAccounting", 6)
            .unwrap()
            .unwrap()
            .uuid,
        first.uuid
    );
}

/// A declared table colliding with the implicit row or duplicate ordinary/account flag cannot choose an arbitrary UUID.
#[test]
fn ambiguous_chart_physical_kinds_and_implicit_row_parents_are_refused() {
    let (_directory, mut session) = session();
    let case = &charts::CASES[1];
    let owner = SourcePath::parse("ChartsOfAccounts/Accounts/object.yaml").unwrap();
    let saved = fs::read_to_string(session.root().join(owner.as_path())).unwrap();
    let parsed = session.inputs.source_reader().read_owner(&owner).unwrap();
    let range = parsed
        .declarations
        .iter()
        .find(|item| item.name == "Accounting")
        .unwrap()
        .name_range
        .clone();
    let mut ambiguous = saved.clone();
    ambiguous.replace_range(range, "ExtraField");
    let mut table = saved;
    let mut edits = [("Rows", "ВидыСубконто"), ("Quantity", "QuantityAccounting")].map(
        |(name, replacement)| {
            let range = parsed
                .declarations
                .iter()
                .find(|item| item.name == name)
                .unwrap()
                .name_range
                .clone();
            (range, replacement)
        },
    );
    edits.sort_unstable_by_key(|(range, _)| std::cmp::Reverse(range.start));
    for (range, replacement) in edits {
        table.replace_range(range, replacement);
    }
    for (version, draft, receiver, field) in [
        (1, ambiguous, "", "ExtraField"),
        (
            2,
            table,
            "Object=Object.ExtDimensionTypes.Add();",
            "QuantityAccounting",
        ),
    ] {
        session
            .apply_overlay(
                &owner,
                session.revision(),
                version,
                &draft,
                &InputRequest::default(),
            )
            .unwrap();
        assert!(matches!(
            session.owner(&owner).unwrap().state,
            ModelState::Ready(_)
        ));
        let body = charts::body(case, receiver, field);
        assert!(matches!(
            query(&mut session, &body, field, version),
            Ok(None) | Err(QueryError::UnsupportedNavigationTarget)
        ));
        let path = if version == 1 {
            vec![(
                eska::project_core::MetadataKind::Attribute,
                "ExtraField".into(),
            )]
        } else {
            vec![
                (
                    eska::project_core::MetadataKind::TabularSection,
                    "ВидыСубконто".into(),
                ),
                (
                    eska::project_core::MetadataKind::Attribute,
                    "QuantityAccounting".into(),
                ),
            ]
        };
        // Semantic ambiguity may refuse the receiver before navigation; physical confirmation must independently refuse too.
        assert_eq!(
            members::selected_uuid(
                &session,
                &owner,
                session.revision(),
                &NativeUuid::parse(&charts::uuid(case, 0)).unwrap(),
                &path
            ),
            Err(QueryError::UnsupportedNavigationTarget)
        );
    }
}
