use super::*;

/// Missing optional positions and commas inside nested expressions remain separate positional values.
#[test]
fn virtual_arguments_are_structured_and_unchanged_forms_are_exact() {
    let original = "select R.QuantityBalance from AccumulationRegister.Stock.Balance( , Item IN (&A, &B)) as R // keep 😀\r\n";
    let mut value = constructor(original);
    let clause = &value.model().statements[0].clauses[1];
    let expression = clause.items[0].visual(value.text(), clause).unwrap();
    assert!(
        matches!(&expression, Expression::VirtualSource { parameters, .. } if parameters == &["", "Item IN (&A, &B)"])
    );
    apply(
        &mut value,
        &Action::Compose {
            statement: 0,
            selection: Some(selection(1, 0)),
            expression,
        },
    );
    assert_eq!(value.text(), original);
    let binding = &value.model().statements[0].source_bindings(value.text())[0];
    assert_eq!(binding.table, "AccumulationRegister.Stock.Balance");
    assert_eq!(binding.parameters.as_ref().unwrap()[0], "");
    apply(
        &mut value,
        &Action::Compose {
            statement: 0,
            selection: Some(selection(1, 0)),
            expression: Expression::VirtualSource {
                table: binding.table.clone(),
                alias: "S".into(),
                parameters: vec!["&Date".into(), "Item IN (&A, &B)".into()],
            },
        },
    );
    assert!(
        value
            .text()
            .contains("Balance(&Date, Item IN (&A, &B)) AS S")
    );
    assert!(value.text().contains("select S.QuantityBalance"));
    assert!(value.text().contains("// keep 😀\r\n"));
}

/// An argument cannot inject another positional value, source alias or package statement.
#[test]
fn virtual_argument_rejection_leaves_text_and_revision_unchanged() {
    let mut value = constructor("ВЫБРАТЬ 1");
    for parameters in [
        vec!["&A, &B"],
        vec!["&A); ВЫБРАТЬ 9"],
        vec!["// скрыть )"],
        vec!["1", "2", "3"],
    ] {
        let before = value.text().to_owned();
        let revision = value.revision();
        assert!(
            value
                .apply(
                    revision,
                    &Action::Compose {
                        statement: 0,
                        selection: None,
                        expression: Expression::VirtualSource {
                            table: "РегистрНакопления.Запасы.Остатки".into(),
                            alias: String::new(),
                            parameters: parameters.into_iter().map(str::to_owned).collect()
                        }
                    }
                )
                .is_err()
        );
        assert_eq!(value.text(), before);
        assert_eq!(value.revision(), revision);
    }
    apply(
        &mut value,
        &Action::Compose {
            statement: 0,
            selection: None,
            expression: Expression::VirtualSource {
                table: "РегистрНакопления.Запасы.Остатки".into(),
                alias: "О".into(),
                parameters: vec![String::new(), "Товар = &Товар".into()],
            },
        },
    );
    assert!(
        value
            .text()
            .contains("ИЗ РегистрНакопления.Запасы.Остатки(, Товар = &Товар) КАК О")
    );
}
