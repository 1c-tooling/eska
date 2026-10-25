use super::*;

/// Adapter intentions address the core's exact node index rather than reparsing braces.
fn change(value: &mut Constructor, target: DcsKind, values: Vec<DcsValue>, enabled: bool) {
    let node = value.dcs_model().unwrap().statements[0]
        .nodes
        .iter()
        .find(|node| node.kind == target)
        .map(|node| node.index);
    apply(
        value,
        &Action::SetDcs {
            statement: 0,
            node,
            target,
            values,
            enabled,
            comments: None,
        },
    );
}

/// Field values have the same DTO in Russian and English query spelling.
fn field(name: &str) -> DcsValue {
    DcsValue::Field {
        expression: name.into(),
        children: true,
        alias: String::new(),
    }
}

/// Extension lists are inserted at proven positions; only projection aliases can enter DCS SELECT.
#[test]
fn dcs_lists_are_typed_bilingual_scoped_and_lossless() {
    for (input, select, condition) in [
        (
            "SELECT T.Ref AS Item FROM Catalog.Items AS T WHERE T.Active = TRUE ORDER BY Item",
            "SELECT",
            "WHERE",
        ),
        (
            "ВЫБРАТЬ T.Ref КАК Item ИЗ Справочник.Товары КАК T ГДЕ T.Active = ИСТИНА УПОРЯДОЧИТЬ ПО Item",
            "ВЫБРАТЬ",
            "ГДЕ",
        ),
    ] {
        let mut value = constructor(input);
        assert!(value.finish().unwrap().is_empty());
        change(&mut value, DcsKind::Select, vec![field("Item")], true);
        change(
            &mut value,
            DcsKind::Where,
            vec![
                field("T.Ref"),
                DcsValue::Condition {
                    left: "T.Date".into(),
                    comparison: Comparison::GreaterOrEqual,
                    right: "&Begin".into(),
                },
            ],
            true,
        );
        assert!(value.text().contains(&format!("{{{select} Item.*}}")));
        assert!(
            value
                .text()
                .contains(&format!("{{{condition} T.Ref.*, T.Date >= &Begin}}"))
        );
        let revision = value.revision();
        change(&mut value, DcsKind::Select, vec![field("Item")], true);
        assert_eq!(value.revision(), revision);
        let text = value.text().to_owned();
        let node = value.dcs_model().unwrap().statements[0]
            .nodes
            .iter()
            .find(|node| node.kind == DcsKind::Select)
            .unwrap()
            .index;
        assert_eq!(
            value
                .apply(
                    revision,
                    &Action::SetDcs {
                        statement: 0,
                        node: Some(node),
                        target: DcsKind::Select,
                        values: vec![field("T.Ref")],
                        enabled: true,
                        comments: None
                    }
                )
                .unwrap_err(),
            Error::InvalidExpression
        );
        assert_eq!(value.text(), text);
        change(&mut value, DcsKind::Select, Vec::new(), false);
        assert!(!value.text().contains(&format!("{{{select}")));
    }
    let value = constructor("SELECT 1 AS A UNION ALL SELECT 2 AS A");
    assert!(value.dcs_model().unwrap().statements[0].add_select);
    assert!(!value.dcs_model().unwrap().statements[1].add_select);
}

/// The real buhmfo pattern combines optional joins with nested virtual-table filter blocks and renamed fields.
#[test]
fn dcs_real_buhmfo_optional_source_and_virtual_arguments_keep_unedited_bytes() {
    let input = "ВЫБРАТЬ Н.Ссылка КАК Номенклатура\r\n{ВЫБРАТЬ Номенклатура.*}\r\nИЗ Справочник.Номенклатура КАК Н\r\n{ЛЕВОЕ СОЕДИНЕНИЕ РегистрСведений.ЦеныНоменклатуры.СрезПоследних({&Дата}, {(Номенклатура).*, (ТипЦен).*}) КАК Цены ПО Н.Ссылка = Цены.Номенклатура}\r\n{ГДЕ Н.Ссылка.* КАК Номенклатура}";
    let mut value = constructor(input);
    assert!(value.finish().unwrap().is_empty());
    let nodes = &value.dcs_model().unwrap().statements[0].nodes;
    assert!(nodes.iter().all(|node| node.editable), "{nodes:?}");
    assert_eq!(nodes.len(), 5);
    change(
        &mut value,
        DcsKind::Parameter,
        vec![DcsValue::Parameter {
            name: "Период".into(),
        }],
        true,
    );
    assert_eq!(value.text(), input.replace("{&Дата}", "{&Период}"));
    change(
        &mut value,
        DcsKind::VirtualFilter,
        vec![field("Номенклатура")],
        true,
    );
    assert!(
        value
            .text()
            .contains("СрезПоследних({&Период}, {Номенклатура.*})")
    );
    let before = value.text().to_owned();
    change(&mut value, DcsKind::OptionalSource, Vec::new(), false);
    assert_eq!(
        value.text(),
        before
            .replace("{ЛЕВОЕ СОЕДИНЕНИЕ", "ЛЕВОЕ СОЕДИНЕНИЕ")
            .replace("Цены.Номенклатура}", "Цены.Номенклатура")
    );
    change(&mut value, DcsKind::OptionalSource, Vec::new(), true);
    assert_eq!(value.text(), before);
    change(&mut value, DcsKind::Where, vec![field("Н.Ссылка")], true);
    assert!(
        value
            .text()
            .contains("Цены.Номенклатура}\r\n{ГДЕ Н.Ссылка.*}")
    );
}

/// Unknown macros, wrong slots, comments and stale actions never silently delete or reinterpret text.
#[test]
fn dcs_refuses_unknown_positions_and_preserves_comments_atomically() {
    let input = "SELECT 1 AS A {SELECT /* keep */ A} {CHARACTERISTICS TYPE(Catalog.Items)}";
    let mut value = constructor(input);
    let view = value.dcs_model().unwrap();
    assert!(
        view.statements[0]
            .nodes
            .iter()
            .any(|node| node.kind == DcsKind::Opaque && !node.editable)
    );
    let node = view.statements[0]
        .nodes
        .iter()
        .find(|node| node.kind == DcsKind::Select)
        .unwrap();
    let mut action = Action::SetDcs {
        statement: 0,
        node: Some(node.index),
        target: DcsKind::Select,
        values: vec![field("A")],
        enabled: true,
        comments: None,
    };
    assert_eq!(
        value.apply(value.revision(), &action).unwrap_err(),
        Error::CommentsRequireDecision
    );
    assert_eq!(value.text(), input);
    if let Action::SetDcs { comments, .. } = &mut action {
        *comments = Some(CommentDecision::Keep);
    }
    apply(&mut value, &action);
    assert!(value.text().contains("/* keep */"));
    assert!(
        value
            .text()
            .ends_with("{CHARACTERISTICS TYPE(Catalog.Items)}")
    );
    assert_eq!(value.apply(1, &action).unwrap_err(), Error::StaleRevision);
    let text = value.text().to_owned();
    if let Action::SetDcs { values, .. } = &mut action {
        *values = vec![DcsValue::Condition {
            left: "A".into(),
            comparison: Comparison::Equal,
            right: "1; SELECT 2".into(),
        }];
    }
    assert!(value.apply(value.revision(), &action).is_err());
    assert_eq!(value.text(), text);
    let wrong = constructor("SELECT 1 AS A FROM Catalog.Items {SELECT A}");
    assert!(
        wrong.dcs_model().unwrap().statements[0]
            .nodes
            .iter()
            .all(|node| !node.editable)
    );
    let unknown = constructor("SELECT T.Ref AS A FROM Catalog.Items AS T {WHERE T.Ref = {Macro}} ");
    assert!(
        unknown.dcs_model().unwrap().statements[0]
            .nodes
            .iter()
            .all(|node| !node.editable)
    );
}

/// Conditional date arguments are signature-driven and ordinary source syntax cannot acquire a DCS parameter editor.
#[test]
fn dcs_parameters_follow_virtual_signatures_and_source_scope() {
    let mut value = constructor(
        "SELECT T.Amount FROM AccumulationRegister.Stock.Turnovers(&Begin, &End, , ) AS T",
    );
    let view = value.dcs_model().unwrap();
    assert_eq!(
        view.statements[0]
            .nodes
            .iter()
            .filter(|node| node.kind == DcsKind::Parameter)
            .count(),
        2
    );
    change(
        &mut value,
        DcsKind::Parameter,
        vec![DcsValue::Parameter {
            name: "Start".into(),
        }],
        true,
    );
    assert!(value.text().contains("Turnovers({&Start}, &End, , )"));
    change(
        &mut value,
        DcsKind::Parameter,
        vec![DcsValue::Parameter {
            name: "Start".into(),
        }],
        false,
    );
    assert!(value.text().contains("Turnovers(&Start, &End, , )"));
    let plain = constructor("SELECT F(&Begin) AS A");
    assert!(plain.dcs_model().unwrap().statements[0].nodes.is_empty());
    let nested = constructor(
        "SELECT X.A FROM (SELECT T.Ref AS A {SELECT A.*} FROM Catalog.Items AS T) AS X",
    );
    assert!(nested.dcs_model().unwrap().statements[0].nodes.is_empty());
    assert_eq!(nested.dcs_model().unwrap().statements[1].nodes.len(), 1);
}

/// A new WHERE extension belongs after the optional source wrapper, never inside its join condition.
#[test]
fn dcs_where_insertion_follows_the_optional_join_wrapper() {
    let mut value = constructor(
        "SELECT T.Ref AS Item FROM Catalog.Items AS T {LEFT JOIN Catalog.Prices AS P ON T.Ref = P.Item}",
    );
    change(&mut value, DcsKind::Where, vec![field("T.Ref")], true);
    assert!(value.text().contains("P.Item}\n{WHERE T.Ref.*}"));
}

/// A removed extension wrapper is also a lexical separator in compact valid source text.
#[test]
fn dcs_removing_braces_cannot_merge_neighboring_query_tokens() {
    let mut value = constructor("SELECT A{SELECT A}FROM Catalog.Items AS T");
    change(&mut value, DcsKind::Select, Vec::new(), false);
    assert_eq!(value.text(), "SELECT A FROM Catalog.Items AS T");
    let mut value = constructor(
        "SELECT T.Ref AS A FROM Catalog.Items AS T{LEFT JOIN Catalog.Prices AS P ON T.Ref = P.Item}WHERE T.Ref = &Item",
    );
    change(&mut value, DcsKind::OptionalSource, Vec::new(), false);
    assert!(value.text().contains("AS T LEFT JOIN"));
    assert!(value.text().contains("P.Item WHERE T.Ref"));
}

/// An unaliased CASE still ends at its original END token, including Cyrillic UTF-8 boundaries.
#[test]
fn dcs_where_after_unaliased_case_keeps_original_token_boundaries() {
    for input in [
        "SELECT CASE WHEN TRUE THEN 1 ELSE 0 END",
        "ВЫБРАТЬ ВЫБОР КОГДА ИСТИНА ТОГДА 1 ИНАЧЕ 0 КОНЕЦ",
    ] {
        let mut value = constructor(input);
        change(
            &mut value,
            DcsKind::Where,
            vec![DcsValue::Condition {
                left: "1".into(),
                comparison: Comparison::Equal,
                right: "1".into(),
            }],
            true,
        );
        assert!(value.text().starts_with(input));
        assert!(value.text().contains("1 = 1}"));
    }
}

/// The full original buhmfo query includes CASE, optional ON(TRUE), compound joins and parenthesized filter paths.
#[test]
fn dcs_inspects_the_original_buhmfo_price_query_without_regenerating_case() {
    let input = include_str!("fixtures/buhmfo-price-query.sdbl");
    let mut value = constructor(input);
    let view = value.dcs_model().unwrap();
    assert!(
        view.statements[0].nodes.iter().all(|node| node.editable),
        "{:?}",
        view.statements[0].nodes
    );
    assert_eq!(
        view.statements[0]
            .nodes
            .iter()
            .find(|node| node.kind == DcsKind::Select)
            .unwrap()
            .values
            .len(),
        6
    );
    assert!(value.finish().unwrap().is_empty());
    let select = view.statements[0]
        .nodes
        .iter()
        .find(|node| node.kind == DcsKind::Select)
        .unwrap();
    assert!(
        value
            .apply(
                value.revision(),
                &Action::SetDcs {
                    statement: 0,
                    node: Some(select.index),
                    target: DcsKind::Select,
                    values: select.values.clone(),
                    enabled: true,
                    comments: None
                }
            )
            .unwrap()
            .is_empty()
    );
    change(&mut value, DcsKind::OptionalSource, Vec::new(), false);
    assert!(value.text().contains("КОНЕЦ КАК СтавкаНДС"));
    change(&mut value, DcsKind::OptionalSource, Vec::new(), true);
    assert_eq!(value.text(), input);
}
