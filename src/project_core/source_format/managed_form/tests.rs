//! Layout changes retain native identities and scopes while rejecting ambiguous or unsafe relationships.

use super::{FormElement, FormElementKind, FormFixedSlot, FormId, ManagedFormLayout};

const XML: &str = r#"<Form xmlns="http://v8.1c.ru/8.3/xcf/logform" version="2.20">
  <AutoCommandBar name="" id="-1"><ChildItems><Button name="Run" id="10"><CommandName>Form.Command.Action</CommandName></Button></ChildItems></AutoCommandBar>
  <ChildItems>
    <UsualGroup name="First" id="1"><ChildItems><InputField name="Value" id="2"><ContextMenu name="Shared" id="6"/><ExtendedTooltip name="Shared" id="7"/></InputField></ChildItems></UsualGroup>
    <Table name="Rows" id="3"><AutoCommandBar name="RowsBar" id="8"/><SearchStringAddition name="Search" id="9"><AdditionSource><Item>Rows</Item></AdditionSource></SearchStringAddition><ChildItems><InputField name="Column" id="4"/></ChildItems></Table>
    <UsualGroup name="Second" id="5"/>
  </ChildItems>
  <Attributes><ConditionalAppearance/><Attribute name="Value" id="1"><Columns><Column name="Same" id="1"/></Columns><AdditionalColumns><Column name="Same" id="1"/></AdditionalColumns></Attribute><Attribute name="Other" id="2"><Columns><Column name="Same" id="1"/></Columns></Attribute></Attributes>
  <Commands><Command name="Action" id="1"/></Commands>
</Form>"#;

/// Synthetic native IDs are stable test handles, independent of any real configuration.
fn id(text: &str) -> FormId {
    FormId::parse(text).unwrap()
}

/// Decode a complete small graph with overlapping UI/data IDs and owned fixed nodes.
fn form() -> ManagedFormLayout {
    ManagedFormLayout::from_xml(XML).unwrap()
}

/// UI, attribute, command and immediate column scopes retain coincident native IDs.
#[test]
fn native_scopes_and_fixed_aliases_remain_distinct() {
    let form = form();
    assert_eq!(form.elements.len(), 11);
    assert_eq!(form.attributes[&id("1")].name, "Value");
    assert_eq!(form.commands[&id("1")].name, "Action");
    assert_eq!(form.elements[&id("1")].name, "First");
    assert_eq!(form.layout.roots, [id("1"), id("3"), id("5")]);
    assert_eq!(form.fixed.len(), 5);
    assert_eq!(form.addition_sources[&id("9")], id("3"));
    assert_eq!(form.local_commands[&id("10")], id("1"));
    form.validate_layout_change(&form).unwrap();
}

/// Reordering and moving ordinary nodes between visual groups preserves their fixed descendants.
#[test]
fn ordinary_layout_moves_preserve_identity_and_fixed_owners() {
    let accepted = form();
    let mut current = accepted.clone();
    current.layout.roots.reverse();
    current.layout.children.insert(id("1"), vec![]);
    current.layout.children.insert(id("5"), vec![id("2")]);
    current.validate_layout_change(&accepted).unwrap();
    assert_eq!(current.fixed, accepted.fixed);
}

/// Structurally valid data-column moves still require acceptance of their new table context.
#[test]
fn layout_moves_cannot_silently_change_the_nearest_table() {
    let accepted = form();
    let mut current = accepted.clone();
    current.layout.children.insert(id("3"), vec![]);
    current.layout.children.insert(id("5"), vec![id("4")]);
    current.validate().unwrap();
    assert_eq!(
        current.validate_layout_change(&accepted).unwrap_err().code,
        "source_form_data_context_changed"
    );
}

/// Valid native slots remain bound to their accepted owner and cannot be silently reassigned.
#[test]
fn layout_only_changes_do_not_replace_definitions_or_fixed_bindings() {
    let accepted = form();
    let mut current = accepted.clone();
    current
        .fixed
        .iter_mut()
        .find(|slot| slot.element == id("7"))
        .unwrap()
        .owner = Some(id("4"));
    current.validate().unwrap();
    assert_eq!(
        current.validate_layout_change(&accepted).unwrap_err().code,
        "source_form_identity_changed"
    );
    let mut current = accepted.clone();
    current.elements.get_mut(&id("2")).unwrap().name = "Renamed".into();
    assert_eq!(
        current.validate_layout_change(&accepted).unwrap_err().code,
        "source_form_identity_changed"
    );
}

/// Missing, duplicate and disconnected ordinary edges cannot hide behind a valid definition map.
#[test]
fn parentage_and_reachability_are_required() {
    let mut current = form();
    current.layout.roots.push(id("100"));
    assert_eq!(
        current.validate().unwrap_err().code,
        "source_form_element_missing"
    );
    let mut current = form();
    current.layout.children.insert(id("5"), vec![id("2")]);
    assert_eq!(
        current.validate().unwrap_err().code,
        "source_form_parent_duplicate"
    );
    let mut current = form();
    current.layout.children.remove(&id("1"));
    assert_eq!(
        current.validate().unwrap_err().code,
        "source_form_element_unreachable"
    );
}

/// A disconnected cycle has one incoming edge per node but no path to the form root.
#[test]
fn cycles_are_detected_without_recursive_traversal() {
    let mut current = form();
    current.layout.roots = vec![id("3")];
    current
        .layout
        .children
        .insert(id("1"), vec![id("2"), id("5")]);
    current.layout.children.insert(id("5"), vec![id("1")]);
    assert_eq!(
        current.validate().unwrap_err().code,
        "source_form_layout_cycle"
    );
}

/// Page, column and command containers retain their native child domains.
#[test]
fn class_nesting_and_fixed_slot_cardinality_are_checked() {
    let mut current = form();
    current.elements.get_mut(&id("1")).unwrap().kind = FormElementKind::Pages;
    assert_eq!(
        current.validate().unwrap_err().code,
        "source_form_nesting_invalid"
    );
    let mut current = form();
    current.elements.insert(
        id("20"),
        FormElement {
            kind: FormElementKind::ContextMenu,
            name: "Another".into(),
        },
    );
    current.fixed.push(FormFixedSlot {
        owner: Some(id("2")),
        element: id("20"),
    });
    assert_eq!(
        current.validate().unwrap_err().code,
        "source_form_fixed_slot_invalid"
    );
    let mut current = form();
    current.fixed.retain(|slot| slot.owner.is_some());
    assert_eq!(
        current.validate().unwrap_err().code,
        "source_form_fixed_slot_invalid"
    );
}

/// Proven local references resolve in the intended namespace; built-in or numeric command syntax stays opaque.
#[test]
fn references_do_not_acquire_authority_from_coincident_ids() {
    for (before, after) in [
        ("Form.Command.Action", "Form.Command.Value"),
        ("<Item>Rows</Item>", "<Item>First</Item>"),
    ] {
        assert_eq!(
            ManagedFormLayout::from_xml(&XML.replace(before, after))
                .unwrap_err()
                .code,
            "source_form_reference_missing"
        );
    }
    for value in ["Form.StandardCommand.Close", "1", "Other.Provider.Command"] {
        let current =
            ManagedFormLayout::from_xml(&XML.replace("Form.Command.Action", value)).unwrap();
        assert!(current.local_commands.is_empty());
    }
}

/// Numeric IDs remain canonical, distinct from UUIDs/names, without global value-scope uniqueness.
#[test]
fn invalid_and_duplicate_ids_are_rejected_in_their_own_scopes() {
    for invalid in [
        "",
        "01",
        "-2",
        "+1",
        "1.0",
        "0x1",
        "Table.Rows",
        "10000000-0000-4000-8000-000000000001",
    ] {
        assert_eq!(
            FormId::parse(invalid).unwrap_err().code,
            "source_form_id_invalid"
        );
    }
    assert_eq!(
        ManagedFormLayout::from_xml(
            &XML.replace("name=\"Second\" id=\"5\"", "name=\"Second\" id=\"1\"")
        )
        .unwrap_err()
        .code,
        "source_form_id_duplicate"
    );
    let duplicate = XML.replace(
        "<Column name=\"Same\" id=\"1\"/>",
        "<Column name=\"Same\" id=\"1\"/><Column name=\"Other\" id=\"1\"/>",
    );
    assert_eq!(
        ManagedFormLayout::from_xml(&duplicate).unwrap_err().code,
        "source_form_id_duplicate"
    );
}

/// Ordinary names are unambiguous even though fixed native aliases can repeat.
#[test]
fn ordinary_names_cannot_shadow_a_regular_reference_target() {
    assert_eq!(
        ManagedFormLayout::from_xml(&XML.replace("name=\"Second\"", "name=\"rows\""))
            .unwrap_err()
            .code,
        "source_form_name_ambiguous"
    );
    assert_eq!(
        ManagedFormLayout::from_xml(&XML.replace("name=\"Second\"", "name=\"\""))
            .unwrap_err()
            .code,
        "source_form_name_ambiguous"
    );
}

/// Unknown UI declarations, namespaces and versions remain raw resources without guessed layout semantics.
#[test]
fn unverified_native_profiles_and_malformed_xml_fail_explicitly() {
    for value in [
        XML.replace("UsualGroup", "FutureGroup"),
        XML.replace("version=\"2.20\"", "version=\"2.21\""),
        XML.replace(super::NAMESPACE, "urn:unknown"),
        XML.replace(
            "<UsualGroup name=\"Second\" id=\"5\"/>",
            "<UsualGroup name=\"Second\"/>",
        ),
    ] {
        assert_eq!(
            ManagedFormLayout::from_xml(&value).unwrap_err().code,
            "source_form_profile_unverified"
        );
    }
    assert_eq!(
        ManagedFormLayout::from_xml("<Form>").unwrap_err().code,
        "source_form_xml_invalid"
    );
}
