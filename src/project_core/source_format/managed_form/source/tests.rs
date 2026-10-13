//! Lossless dialog stencils preserve unknown content and fail before unsafe graph expansion.

use super::{ManagedFormSource, Part};
use crate::project_core::source_format::{FormId, ManagedFormLayout};

/// The shared synthetic graph has independent data scopes and multiple fixed descendants.
fn xml() -> String {
    super::super::tests::XML.replace(
        "<UsualGroup name=\"Second\" id=\"5\"/>",
        "<UsualGroup name=\"Second\" id=\"5\"><ChildItems/></UsualGroup>",
    )
}

/// Native decimal identities address the same real slots in XML, source and validation.
fn id(value: &str) -> FormId {
    FormId::parse(value).unwrap()
}

/// Move a field between two existing collection slots without reassigning any fixed owner.
fn moved(source: &ManagedFormSource) -> ManagedFormSource {
    let mut result = source.clone();
    result.layout.children.insert(id("1"), vec![]);
    result.layout.children.insert(id("5"), vec![id("2")]);
    result
}

/// XML declarations, BOM/CRLF, foreign content and formatted titles survive source serialization exactly.
#[test]
fn source_round_trip_keeps_all_unpromoted_bytes() {
    let body = xml()
        .replace("<InputField name=\"Value\" id=\"2\">", "<InputField name=\"Value\" id=\"2\"><Title formatted=\"false\"><foreign:Item xmlns:foreign=\"urn:test\"><![CDATA[Я 😀 < &]]></foreign:Item></Title><!-- opaque -->")
        .replace("<ChildItems/>", "<ChildItems note=\"a > b\"></ChildItems>")
        .replace("</Form>", "<CommandInterface><CommandBar><Item/></CommandBar></CommandInterface></Form>");
    let input = format!("\u{feff}<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n{body}\n")
        .replace('\n', "\r\n");
    let source = ManagedFormSource::from_xml(&input).unwrap();
    let text = source.to_yaml().unwrap();
    let restored = ManagedFormSource::from_yaml(&text).unwrap();
    assert_eq!(source, restored);
    assert_eq!(restored.to_xml().unwrap(), input);
    assert_eq!(
        restored.layout_snapshot().unwrap(),
        ManagedFormLayout::from_xml(&input).unwrap()
    );
    assert!(!text.contains("name=\\\"First\\\""));
}

/// Reordering native traversal changes fixed enumeration order, never its actual owner bindings.
#[test]
fn reordered_xml_retains_fixed_bindings_and_native_ids() {
    let accepted = ManagedFormSource::from_xml(&xml()).unwrap();
    let mut current = accepted.clone();
    current.layout.roots.reverse();
    current.validate_layout_change(&accepted).unwrap();
    let rendered = current.to_xml().unwrap();
    let current = ManagedFormLayout::from_xml(&rendered).unwrap();
    let accepted = accepted.layout_snapshot().unwrap();
    assert_ne!(current.fixed, accepted.fixed);
    current.validate_layout_change(&accepted).unwrap();
}

/// Child-count changes preserve all owner gaps once, including comments around a moved definition.
#[test]
fn moves_expand_existing_empty_slots_without_losing_trivia() {
    let input = xml()
        .replace(
            "<ChildItems><InputField name=\"Value\"",
            "<ChildItems><!-- before -->\n<InputField name=\"Value\"",
        )
        .replace(
            "</InputField></ChildItems></UsualGroup>",
            "</InputField>\n<!-- after --></ChildItems></UsualGroup>",
        );
    let accepted = ManagedFormSource::from_xml(&input).unwrap();
    let current = moved(&accepted);
    current.validate_layout_change(&accepted).unwrap();
    let rendered = current.to_xml().unwrap();
    assert_eq!(rendered.matches("<!-- before -->").count(), 1);
    assert_eq!(rendered.matches("<!-- after -->").count(), 1);
    assert_eq!(current.native, accepted.native);
    assert_eq!(
        ManagedFormLayout::from_xml(&rendered)
            .unwrap()
            .layout
            .children[&id("5")],
        [id("2")]
    );
}

/// A known container without an inverse collection slot cannot receive silently dropped children.
#[test]
fn absent_native_child_slots_are_not_manufactured() {
    let accepted = ManagedFormSource::from_xml(super::super::tests::XML).unwrap();
    let current = moved(&accepted);
    assert_eq!(
        current.to_xml().unwrap_err().code,
        "source_form_layout_slot_invalid"
    );
}

/// Effective prefixes inside preserved XML remain meaningful when an ordinary element moves.
#[test]
fn moves_across_different_namespace_scopes_are_refused() {
    let input = xml()
        .replace(
            "name=\"First\" id=\"1\"",
            "name=\"First\" id=\"1\" xmlns:q=\"urn:first\"",
        )
        .replace(
            "name=\"Second\" id=\"5\"",
            "name=\"Second\" id=\"5\" xmlns:q=\"urn:second\"",
        )
        .replace(
            "<ContextMenu name=\"Shared\"",
            "<q:Payload>q:Preserved</q:Payload><ContextMenu name=\"Shared\"",
        );
    let accepted = ManagedFormSource::from_xml(&input).unwrap();
    assert_eq!(
        moved(&accepted).to_xml().unwrap_err().code,
        "source_form_namespace_context_changed"
    );
}

/// The accepted layout contract keeps opaque XML and names fixed even if their standalone encoding remains valid.
#[test]
fn accepted_layout_checks_pin_native_stencils_and_definitions() {
    let accepted = ManagedFormSource::from_xml(&xml()).unwrap();
    let mut current = accepted.clone();
    current.elements.get_mut(&id("1")).unwrap().name = "NewName".into();
    assert_eq!(
        current.validate_layout_change(&accepted).unwrap_err().code,
        "source_form_identity_changed"
    );
    let mut current = accepted.clone();
    let Part::Xml { value } = current.native.root.first_mut().unwrap() else {
        panic!("root prefix")
    };
    value.insert_str(0, "<!-- altered native -->");
    current.validate().unwrap();
    assert_eq!(
        current.validate_layout_change(&accepted).unwrap_err().code,
        "source_form_binding_invalid"
    );
}

/// Cycles and duplicate parents fail in preflight rather than growing an unbounded render stack.
#[test]
fn rendering_checks_graph_before_expansion() {
    let mut current = ManagedFormSource::from_xml(&xml()).unwrap();
    current.layout.roots = vec![id("3")];
    current
        .layout
        .children
        .insert(id("1"), vec![id("2"), id("5")]);
    current.layout.children.insert(id("5"), vec![id("1")]);
    assert_eq!(
        current.to_xml().unwrap_err().code,
        "source_form_layout_cycle"
    );
    let mut current = ManagedFormSource::from_xml(&xml()).unwrap();
    current.layout.children.insert(id("5"), vec![id("2")]);
    assert_eq!(
        current.to_xml().unwrap_err().code,
        "source_form_parent_duplicate"
    );
}

/// Typed identity holes cannot be replaced with hidden native literals or duplicated transport fields.
#[test]
fn wire_schema_rejects_substituted_bindings_and_ambiguous_keys() {
    let mut source = ManagedFormSource::from_xml(&xml()).unwrap();
    source
        .native
        .elements
        .get_mut(&id("1"))
        .unwrap()
        .parts
        .retain(|part| !matches!(part, Part::Name));
    assert_eq!(
        source.validate().unwrap_err().code,
        "source_form_binding_invalid"
    );
    let source = ManagedFormSource::from_xml(&xml()).unwrap();
    let text = source.to_yaml().unwrap();
    assert_eq!(
        ManagedFormSource::from_yaml(&format!("schema_version: 1\n{text}"))
            .unwrap_err()
            .code,
        "source_yaml_key_duplicate"
    );
    assert_eq!(
        ManagedFormSource::from_yaml(&format!("unknown: true\n{text}"))
            .unwrap_err()
            .code,
        "source_form_schema_invalid"
    );
}

/// An unproven native ID lexeme is refused instead of being normalized by a supposedly lossless import.
#[test]
fn lexical_identity_normalization_is_explicitly_unverified() {
    let input = xml().replace("name=\"First\" id=\"1\"", "name=\"First\" id=\"&#49;\"");
    ManagedFormLayout::from_xml(&input).unwrap();
    assert_eq!(
        ManagedFormSource::from_xml(&input).unwrap_err().code,
        "source_form_lexical_unverified"
    );
}
