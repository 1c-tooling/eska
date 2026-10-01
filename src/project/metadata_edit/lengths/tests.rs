use super::*;

/// Installed EDT validators have distinct code and description limits for these owners.
#[test]
fn owner_ranges_use_type_and_class() {
    for (class, property, kind, max) in [
        ("Catalog", "CodeLength", "String", 50),
        ("Catalog", "CodeLength", "Number", 38),
        ("ChartOfCalculationTypes", "CodeLength", "Number", 38),
        ("ChartOfCalculationTypes", "CodeLength", "String", 40),
        ("ChartOfCharacteristicTypes", "CodeLength", "", 50),
        ("ChartOfAccounts", "CodeLength", "", 50),
        ("ExchangePlan", "CodeLength", "", 50),
        ("Catalog", "DescriptionLength", "", 150),
        ("ChartOfCalculationTypes", "DescriptionLength", "", 150),
        ("ChartOfCharacteristicTypes", "DescriptionLength", "", 150),
        ("ChartOfAccounts", "DescriptionLength", "", 628),
        ("ExchangePlan", "DescriptionLength", "", 250),
        ("Task", "DescriptionLength", "", 150),
    ] {
        let xml = format!(
            "<{class} xmlns='{MD}'><Properties><CodeType>{kind}</CodeType><{property}>9</{property}></Properties></{class}>"
        );
        let doc = Document::parse(&xml).unwrap();
        let node = doc
            .descendants()
            .find(|node| node.has_tag_name((MD, property)))
            .unwrap();
        assert_eq!(range(node), Some((0, max)), "{class}.{property}");
    }
    for class in ["Document", "Task", "BusinessProcess"] {
        for (kind, max) in [("String", 50), ("Number", 38)] {
            let xml = format!(
                "<{class} xmlns='{MD}'><Properties><NumberType>{kind}</NumberType><NumberLength>9</NumberLength></Properties></{class}>"
            );
            let document = Document::parse(&xml).unwrap();
            let node = document
                .descendants()
                .find(|node| node.has_tag_name((MD, "NumberLength")))
                .unwrap();
            assert_eq!(range(node), Some((0, max)), "{class}.{kind}");
        }
    }
}

/// The edited tag can precede its dependency and change byte length; positions bind to its ancestor.
#[test]
fn incompatible_siblings_are_reported_without_implicit_corrections() {
    for (class, before, after, changed, dependency) in [
        (
            "Catalog",
            "<CodeType>String</CodeType><CodeLength>50</CodeLength>",
            "<CodeType>Number</CodeType><CodeLength>50</CodeLength>",
            "CodeType",
            "CodeLength",
        ),
        (
            "Document",
            "<NumberType>String</NumberType><NumberLength>50</NumberLength>",
            "<NumberType>Number</NumberType><NumberLength>50</NumberLength>",
            "NumberType",
            "NumberLength",
        ),
        (
            "Document",
            "<NumberLength>9</NumberLength><NumberType>String</NumberType><Autonumbering>true</Autonumbering>",
            "<NumberLength>0</NumberLength><NumberType>String</NumberType><Autonumbering>true</Autonumbering>",
            "NumberLength",
            "Autonumbering",
        ),
        (
            "Task",
            "<NumberLength>9</NumberLength><NumberType>String</NumberType><InputByString><Field>Task.Owner.StandardAttribute.Number</Field></InputByString>",
            "<NumberLength>0</NumberLength><NumberType>String</NumberType><InputByString><Field>Task.Owner.StandardAttribute.Number</Field></InputByString>",
            "NumberLength",
            "InputByString",
        ),
        (
            "Catalog",
            "<CodeLength>9</CodeLength><CodeType>String</CodeType><Autonumbering>true</Autonumbering>",
            "<CodeLength>0</CodeLength><CodeType>String</CodeType><Autonumbering>true</Autonumbering>",
            "CodeLength",
            "Autonumbering",
        ),
        (
            "Catalog",
            "<CheckUnique>false</CheckUnique><CodeLength>0</CodeLength><CodeType>String</CodeType>",
            "<CheckUnique>true</CheckUnique><CodeLength>0</CodeLength><CodeType>String</CodeType>",
            "CheckUnique",
            "CodeLength",
        ),
        (
            "Catalog",
            "<DescriptionLength>50</DescriptionLength><InputByString><Field>Catalog.Owner.StandardAttribute.Description</Field></InputByString>",
            "<DescriptionLength>0</DescriptionLength><InputByString><Field>Catalog.Owner.StandardAttribute.Description</Field></InputByString>",
            "DescriptionLength",
            "InputByString",
        ),
        (
            "ChartOfAccounts",
            "<CodeLength>9</CodeLength><OrderLength>9</OrderLength><AutoOrderByCode>true</AutoOrderByCode>",
            "<CodeLength>10</CodeLength><OrderLength>9</OrderLength><AutoOrderByCode>true</AutoOrderByCode>",
            "CodeLength",
            "OrderLength",
        ),
    ] {
        let before = format!(
            "<{class} xmlns='{MD}'><Properties><Name>Owner</Name>{before}</Properties></{class}>"
        );
        let after = format!(
            "<{class} xmlns='{MD}'><Properties><Name>Owner</Name>{after}</Properties></{class}>"
        );
        let old = Document::parse(&before).unwrap();
        let new = Document::parse(&after).unwrap();
        let node = old
            .descendants()
            .find(|node| node.has_tag_name((MD, changed)))
            .unwrap();
        assert!(
            matches!(validate_dependents(node, &new), Err(EditError::IncompatibleProperty(key)) if key.name == dependency),
            "{class}.{changed}"
        );
    }
}

/// Predefined payloads keep nested items; length zero means disabled, not an unlimited string.
#[test]
fn predefined_lengths_preserve_nested_values() {
    let xml = format!(
        "<PredefinedData xmlns='{PREDEF}'><Item><Code>1</Code><Description>A</Description><ChildItems><Item><Code>22</Code><Description>😀</Description></Item></ChildItems></Item></PredefinedData>"
    );
    assert!(
        PredefinedLengths {
            code: Some((2, true)),
            description: Some(2)
        }
        .validate(&xml)
        .is_ok()
    );
    for rule in [
        PredefinedLengths {
            code: Some((1, true)),
            description: None,
        },
        PredefinedLengths {
            code: None,
            description: Some(1),
        },
        PredefinedLengths {
            code: None,
            description: Some(0),
        },
    ] {
        assert!(rule.validate(&xml).is_err());
    }
    assert!(
        PredefinedLengths {
            code: Some((2, true)),
            description: None
        }
        .validate(&xml.replace(">22<", ">AB<"))
        .is_err()
    );
}

/// An absent autocomplete list has platform defaults; an explicit empty list does not.
#[test]
fn disabled_lengths_require_an_explicit_autocomplete_list() {
    for list in ["", "<InputByString/>"] {
        let before = format!(
            "<Catalog xmlns='{MD}'><Properties><Name>Owner</Name><DescriptionLength>50</DescriptionLength>{list}</Properties></Catalog>"
        );
        let after = before.replace(">50<", ">0<");
        let old = Document::parse(&before).unwrap();
        let new = Document::parse(&after).unwrap();
        let node = old
            .descendants()
            .find(|node| node.has_tag_name((MD, "DescriptionLength")))
            .unwrap();
        assert_eq!(validate_dependents(node, &new).is_ok(), !list.is_empty());
    }
}

/// A nested lookalike or an unbound namespace never inherits a document's numbering restrictions.
#[test]
fn numerator_inheritance_requires_the_document_property_context() {
    for (class, numerator, expected) in [
        ("Document", "DocumentNumerator.Shared", true),
        ("Document", "", false),
        ("Document", "  ", false),
        ("DocumentNumerator", "DocumentNumerator.Shared", false),
        ("Task", "DocumentNumerator.Shared", false),
    ] {
        let xml = format!(
            "<{class} xmlns='{MD}'><Properties><Numerator>{numerator}</Numerator><NumberLength>9</NumberLength><NumberType>String</NumberType><NumberAllowedLength>Variable</NumberAllowedLength><NumberPeriodicity>Year</NumberPeriodicity><CheckUnique>true</CheckUnique><Autonumbering>true</Autonumbering><Type><NumberLength>9</NumberLength></Type><NumberLength xmlns='other'>9</NumberLength></Properties></{class}>"
        );
        let document = Document::parse(&xml).unwrap();
        let properties = document.root_element().first_element_child().unwrap();
        for name in [
            "NumberLength",
            "NumberType",
            "NumberAllowedLength",
            "NumberPeriodicity",
            "CheckUnique",
        ] {
            assert_eq!(
                inherited_from_numerator(child(properties, name).unwrap()),
                expected,
                "{class}.{name}"
            );
        }
        assert!(!inherited_from_numerator(
            child(properties, "Autonumbering").unwrap()
        ));
        assert!(!inherited_from_numerator(
            child(properties, "Type")
                .unwrap()
                .first_element_child()
                .unwrap()
        ));
        assert!(!inherited_from_numerator(
            properties.last_element_child().unwrap()
        ));
    }
}

/// Clear one dependent flag even if another inherited XML inconsistency still needs correction.
#[test]
fn disabling_a_flag_does_not_require_fixing_unrelated_dependencies_first() {
    for (class, length, kind) in [
        ("Catalog", "CodeLength", "CodeType"),
        ("Document", "NumberLength", "NumberType"),
    ] {
        for flag in ["Autonumbering", "CheckUnique"] {
            let before = format!(
                "<{class} xmlns='{MD}'><Properties><{kind}>String</{kind}><{length}>0</{length}><Autonumbering>true</Autonumbering><CheckUnique>true</CheckUnique></Properties></{class}>"
            );
            let after = before.replace(
                &format!("<{flag}>true</{flag}>"),
                &format!("<{flag}>false</{flag}>"),
            );
            let old = Document::parse(&before).unwrap();
            let new = Document::parse(&after).unwrap();
            let node = old
                .descendants()
                .find(|node| node.has_tag_name((MD, flag)))
                .unwrap();
            validate_dependents(node, &new).unwrap();
        }
    }
}
