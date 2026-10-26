use super::*;

/// Inspecting inconsistent branches is lossless; only explicit alignment fills their absent positions.
#[test]
fn mismatched_matrix_is_read_only_until_explicit_atomic_alignment() {
    let text = "\u{feff}SELECT 1 AS N\r\nUNION ALL\r\nSELECT 2 AS N, // keep 😀\r\n3 AS Extra\r\nUNION\r\nSELECT 4 AS N";
    let mut value = constructor(text);
    let matrix = value.union_matrix(1).unwrap();
    assert!(matrix.can_align);
    assert_eq!(matrix.branches, vec![0, 1, 2]);
    assert_eq!(matrix.columns.len(), 2);
    assert!(matrix.columns[1].cells[0].is_none());
    assert_eq!(value.text(), text);
    assert_eq!(value.revision(), 1);
    apply(&mut value, &Action::AlignUnion { statement: 1 });
    assert_eq!(value.revision(), 2);
    assert_eq!(
        value.model().union_mismatches(value.text()),
        Vec::<usize>::new()
    );
    assert_eq!(value.text().matches("NULL AS Extra").count(), 2);
    assert_eq!(value.text().matches("// keep 😀").count(), 1);
    assert!(value.text().contains("\r\n"));
    let before = value.text().to_owned();
    apply(&mut value, &Action::AlignUnion { statement: 0 });
    assert_eq!(value.revision(), 2);
    assert_eq!(value.text(), before);
}

/// Nested UNIONs align inside their own scope; wildcard output cannot be guessed without semantic services.
#[test]
fn alignment_respects_nested_scope_and_refuses_wildcard_expansion() {
    let mut value =
        constructor("SELECT Q.N FROM (SELECT 1 AS N UNION ALL SELECT 2 AS N, 3 AS Extra) AS Q");
    let matrix = value.union_matrix(1).unwrap();
    assert_eq!(matrix.scope, 1);
    apply(&mut value, &Action::AlignUnion { statement: 1 });
    assert!(value.text().starts_with("SELECT Q.N FROM ("));
    assert!(value.text().contains("NULL AS Extra"));
    let mut value = constructor("SELECT T.* FROM T UNION SELECT 1 AS N, 2 AS Other");
    let before = value.text().to_owned();
    assert!(!value.union_matrix(0).unwrap().can_align);
    assert_eq!(
        value
            .apply(1, &Action::AlignUnion { statement: 0 })
            .unwrap_err(),
        Error::Unsupported
    );
    assert_eq!(value.text(), before);
    assert_eq!(value.revision(), 1);
}
