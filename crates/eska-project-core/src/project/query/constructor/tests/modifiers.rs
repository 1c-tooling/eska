use super::*;
use crate::project::query::constructor::Modifiers;

/// Import/reapply must preserve original flag order, comments, BOM and CRLF exactly.
#[test]
fn modifier_roundtrip_is_byte_exact_and_tails_do_not_become_fields() {
    let text = "\u{feff}SELECT ALLOWED /* flag 😀 */ DISTINCT TOP 0 T.Code AS N\r\nFROM Catalog.Items AS T\r\nFOR /* lock */ UPDATE Catalog.Items\r\nORDER BY N\r\nAUTOORDER\r\nTOTALS COUNT(N) AS N BY OVERALL";
    let mut value = constructor(text);
    assert!(value.model().statements[0].unavailable.is_none());
    let view = value.modifiers(0).unwrap();
    assert!(view.value.allowed && view.value.distinct && view.value.auto_order);
    assert_eq!(view.value.top, Some(0));
    assert_eq!(view.value.for_update, Some(vec!["Catalog.Items".into()]));
    let plan = value
        .apply(
            1,
            &Action::SetModifiers {
                statement: 0,
                modifiers: view.value,
            },
        )
        .unwrap();
    assert!(plan.is_empty());
    assert_eq!(value.text(), text);
    assert_eq!(value.revision(), 1);
    assert_eq!(value.model().statements[0].clauses[0].items.len(), 1);
}

/// Subsequent ordinary edits respect the definition lock and result autoorder boundaries.
#[test]
fn modifying_flags_and_adding_clauses_keeps_native_control_order() {
    let mut value = constructor("SELECT T.Code AS N FROM Catalog.Items AS T");
    let modifiers = Modifiers {
        distinct: true,
        allowed: true,
        top: Some(u32::MAX),
        auto_order: true,
        for_update: Some(vec!["Catalog.Items".into()]),
    };
    apply(
        &mut value,
        &Action::SetModifiers {
            statement: 0,
            modifiers,
        },
    );
    assert_eq!(value.revision(), 2);
    for (clause, text) in [
        (ClauseKind::Conditions, "T.Code = 1"),
        (ClauseKind::Order, "N"),
        (ClauseKind::Totals, "SUM(N) AS N"),
    ] {
        apply(
            &mut value,
            &Action::Append {
                statement: 0,
                clause,
                text: text.into(),
                join: JoinKind::Inner,
            },
        );
    }
    let input = value.text();
    assert!(input.find("WHERE").unwrap() < input.find("FOR UPDATE").unwrap());
    assert!(input.find("FOR UPDATE").unwrap() < input.find("ORDER BY").unwrap());
    assert!(input.find("ORDER BY").unwrap() < input.find("AUTOORDER").unwrap());
    assert!(input.find("AUTOORDER").unwrap() < input.find("TOTALS").unwrap());
    assert_eq!(value.modifiers(0).unwrap().value.top, Some(u32::MAX));
    assert!(value.model().statements[0].unavailable.is_none());
}

/// Native locking names are metadata paths, and a conversion cannot silently discard incompatible controls.
#[test]
fn invalid_lock_targets_and_operator_conversions_are_atomic() {
    let mut value = constructor("SELECT T.Code FROM Catalog.Items AS T");
    let mut options = value.modifiers(0).unwrap().value;
    options.for_update = Some(vec!["T".into()]);
    assert_eq!(
        value
            .apply(
                1,
                &Action::SetModifiers {
                    statement: 0,
                    modifiers: options.clone()
                }
            )
            .unwrap_err(),
        Error::InvalidExpression
    );
    assert_eq!(value.revision(), 1);
    options.for_update = Some(vec!["Catalog.Items".into()]);
    apply(
        &mut value,
        &Action::SetModifiers {
            statement: 0,
            modifiers: options,
        },
    );
    let before = value.text().to_owned();
    let revision = value.revision();
    assert_eq!(
        value
            .apply(
                revision,
                &Action::SetInto {
                    statement: 0,
                    name: Some("Tmp".into()),
                    comments: None
                }
            )
            .unwrap_err(),
        Error::Unsupported
    );
    assert_eq!(value.text(), before);
    assert_eq!(
        value
            .apply(
                revision,
                &Action::AddUnion {
                    statement: 0,
                    all: true
                }
            )
            .unwrap_err(),
        Error::Unsupported
    );
    assert_eq!(value.text(), before);
    let action = serde_json::json!({"kind":"setModifiers","statement":0,"modifiers":{"distinct":false,"allowed":false,"top":4_294_967_296u64,"autoOrder":false,"forUpdate":null}});
    assert!(serde_json::from_value::<Action>(action).is_err());
}

/// Source aliases can change while explicit lock paths stay stable; changing a locked table is refused.
#[test]
fn locked_sources_retain_path_ownership_during_alias_edits() {
    let mut value = constructor("SELECT T.Code FROM Catalog.Items AS T FOR UPDATE Catalog.Items");
    apply(
        &mut value,
        &Action::Compose {
            statement: 0,
            selection: Some(selection(1, 0)),
            expression: Expression::Source {
                table: "Catalog.Items".into(),
                alias: "Renamed".into(),
            },
        },
    );
    assert!(value.text().contains("Renamed.Code"));
    assert!(value.text().contains("FOR UPDATE Catalog.Items"));
    let before = value.text().to_owned();
    assert_eq!(
        value
            .apply(
                value.revision(),
                &Action::Compose {
                    statement: 0,
                    selection: Some(selection(1, 0)),
                    expression: Expression::Source {
                        table: "Catalog.Other".into(),
                        alias: "Renamed".into()
                    }
                }
            )
            .unwrap_err(),
        Error::Unsupported
    );
    assert_eq!(value.text(), before);
}

/// Autoordering is scope-wide for UNION, while TOP and DISTINCT remain branch-local.
#[test]
fn shared_autoorder_and_branch_prefixes_preserve_union_projection() {
    let mut value = constructor("SELECT 1 AS N UNION ALL SELECT 2 AS N");
    let mut options = value.modifiers(0).unwrap().value;
    options.auto_order = true;
    options.top = Some(1);
    apply(
        &mut value,
        &Action::SetModifiers {
            statement: 0,
            modifiers: options,
        },
    );
    assert!(value.text().starts_with("SELECT TOP 1"));
    assert!(value.text().contains("UNION ALL SELECT 2"));
    assert!(value.text().trim_end().ends_with("AUTOORDER"));
    assert!(value.modifiers(1).unwrap().value.auto_order);
    assert_eq!(value.modifiers(1).unwrap().value.top, None);
    apply(
        &mut value,
        &Action::AddUnion {
            statement: 0,
            all: true,
        },
    );
    assert_eq!(value.text().matches("AUTOORDER").count(), 1);
    assert!(
        value
            .model()
            .statements
            .iter()
            .all(|owner| owner.unavailable.is_none())
    );
}
