//! Reference fixtures distinguish equal text with different XML semantics.

use super::*;

/// Bind a real type identity to the renamed metadata object.
fn catalog() -> ReferenceRename {
    ReferenceRename::new(
        &[("Catalog".to_owned(), "Old".to_owned())],
        "Новый",
        &[
            "CatalogRef.Old".to_owned(),
            "CatalogTabularSection.Old.Lines".to_owned(),
        ],
    )
    .expect("rename")
}

/// Apply only backend-produced ranges to inspect exact-byte preservation.
fn apply(input: &str, analysis: &XmlAnalysis) -> String {
    let mut candidate = input.to_owned();
    for change in analysis.replacements.iter().rev() {
        assert_eq!(input[change.range.clone()], change.before);
        candidate.replace_range(change.range.clone(), &change.after);
    }
    candidate
}

/// Equal-looking text only becomes an edit when its XML type proves the binding.
#[test]
fn changes_bound_references_and_preserves_unknown_text_entities_bom_and_crlf() {
    let input = format!(
        "\u{feff}<r xmlns:xsi='{XSI}' xmlns:xr='{XR}' xmlns:v='{CORE}' xmlns:cfg='{CFG}'>\r\n\t<a xsi:type='xr:MDObjectRef'>Catalog.Old.Attribute.Code</a>\r\n\t<b xsi:type='xr:DesignTimeRef'>Catalog.Old.First</b>\r\n\t<v:Type>cfg:CatalogRef.Old</v:Type><xr:GeneratedType name='CatalogTabularSection.Old.Lines'/>\r\n\t<text>Catalog.Old</text><text>Old &amp; label</text><text>Catalog.OldSuffix</text></r>"
    );
    let analysis = catalog().analyze_xml(&input, &[]).expect("analysis");
    assert_eq!(analysis.replacements.len(), 4);
    assert_eq!(analysis.uncertain.len(), 2);
    let output = apply(&input, &analysis);
    assert_eq!(
        output,
        input
            .replace(
                ">Catalog.Old.Attribute.Code<",
                ">Catalog.Новый.Attribute.Code<"
            )
            .replace(">Catalog.Old.First<", ">Catalog.Новый.First<")
            .replace(">cfg:CatalogRef.Old<", ">cfg:CatalogRef.Новый<")
            .replace(
                "name='CatalogTabularSection.Old.Lines'",
                "name='CatalogTabularSection.Новый.Lines'"
            )
    );
}

/// Prefixes alone cannot authorize a replacement after namespace rebinding.
#[test]
fn namespace_rebinding_prevents_lookalike_references_from_becoming_edits() {
    let input = format!(
        "<r xmlns:xsi='{XSI}' xmlns:xr='urn:foreign' xmlns:v='{CORE}' xmlns:cfg='urn:foreign'><a xsi:type='xr:MDObjectRef'>Catalog.Old</a><v:Type>cfg:CatalogRef.Old</v:Type><xr:GeneratedType name='CatalogRef.Old'/></r>"
    );
    let analysis = catalog().analyze_xml(&input, &[]).expect("analysis");
    assert!(analysis.replacements.is_empty());
    assert_eq!(analysis.uncertain.len(), 3);
}

/// Declaration coordinates cannot silently select an unrelated value or stale name.
#[test]
fn declaration_identity_is_supplied_by_the_resolver_and_must_still_match() {
    let input = format!(
        "<r xmlns='{MD}'><Properties><Name>Old</Name><Synonym>Old</Synonym></Properties></r>"
    );
    let parsed = Document::parse(&input).expect("XML");
    let range = parsed
        .descendants()
        .find(|node| node.has_tag_name((MD, "Name")))
        .expect("name")
        .range();
    let analysis = catalog()
        .analyze_xml(&input, std::slice::from_ref(&range))
        .expect("analysis");
    assert_eq!(
        apply(&input, &analysis),
        input.replace("<Name>Old</Name>", "<Name>Новый</Name>")
    );
    assert_eq!(analysis.uncertain.len(), 1);
    assert!(catalog().analyze_xml(&input, &[range, 0..1]).is_err());
}

/// An inline name belongs to its complete logical owner path.
#[test]
fn nested_identity_does_not_rename_equal_names_under_another_owner() {
    let rename = ReferenceRename::new(
        &[
            ("Catalog".to_owned(), "Owner".to_owned()),
            ("TabularSection".to_owned(), "Old".to_owned()),
        ],
        "New",
        &["CatalogTabularSection.Owner.Old".to_owned()],
    )
    .expect("rename");
    let input = format!(
        "<r xmlns:xr='{XR}' xmlns:xsi='{XSI}'><a xsi:type='xr:MDObjectRef'>Catalog.Owner.TabularSection.Old.Attribute.Code</a><a xsi:type='xr:MDObjectRef'>Catalog.Other.TabularSection.Old</a><xr:GeneratedType name='CatalogTabularSection.Owner.Old'/></r>"
    );
    let analysis = rename.analyze_xml(&input, &[]).expect("analysis");
    assert_eq!(analysis.replacements.len(), 2);
    assert_eq!(analysis.uncertain.len(), 1);
    assert!(apply(&input, &analysis).contains("Catalog.Other.TabularSection.Old"));
}

/// The same string may be a reference or a comment according to the metadata schema.
#[test]
fn metadata_property_domain_decides_whether_identical_text_is_a_reference() {
    let rename = ReferenceRename::new(&[("CommonForm".to_owned(), "Old".to_owned())], "New", &[])
        .expect("rename");
    let input = format!(
        "<MetaDataObject xmlns='{MD}'><Configuration><Properties><DefaultReportForm>CommonForm.Old</DefaultReportForm><Comment>CommonForm.Old</Comment></Properties></Configuration></MetaDataObject>"
    );
    let analysis = rename.analyze_xml(&input, &[]).expect("analysis");
    assert_eq!(analysis.replacements.len(), 1);
    assert_eq!(analysis.uncertain.len(), 1);
    assert!(apply(&input, &analysis).contains("<Comment>CommonForm.Old</Comment>"));
}

/// Case-insensitive matching still requires a complete logical path segment.
#[test]
fn generated_qnames_and_dump_names_respect_segment_boundaries_and_case() {
    let input = format!(
        "<r xmlns:d='{DUMP}' xmlns:v='{CORE}' xmlns:cfg='{CFG}'><d:Metadata name='catalog.old.Form.Main'/><d:Metadata name='Catalog.OldSuffix'/><v:Type>cfg:CatalogRef.OldSuffix</v:Type></r>"
    );
    let analysis = catalog().analyze_xml(&input, &[]).expect("analysis");
    assert_eq!(analysis.replacements.len(), 1);
    assert!(apply(&input, &analysis).contains("name='Catalog.Новый.Form.Main'"));
}

/// Rights object identities and module handler bindings differ from user text in the same files.
#[test]
fn role_object_and_handler_paths_are_confirmed_without_changing_restriction_text() {
    let input = format!(
        "<Rights xmlns='{RIGHTS}'><object><name>Catalog.Old</name><restriction><name>Catalog.Old</name></restriction></object></Rights>"
    );
    let analysis = catalog().analyze_xml(&input, &[]).expect("analysis");
    assert_eq!(analysis.replacements.len(), 1);
    assert_eq!(analysis.uncertain.len(), 1);
    let rename = ReferenceRename::new(&[("CommonModule".to_owned(), "Old".to_owned())], "New", &[])
        .expect("rename");
    for (class, property) in [
        ("EventSubscription", "Handler"),
        ("ScheduledJob", "MethodName"),
    ] {
        let input = format!(
            "<MetaDataObject xmlns='{MD}'><{class}><Properties><{property}>CommonModule.Old.Method</{property}><Comment>CommonModule.Old.Method</Comment></Properties></{class}></MetaDataObject>"
        );
        let analysis = rename.analyze_xml(&input, &[]).expect("analysis");
        assert_eq!(analysis.replacements.len(), 1);
        assert_eq!(analysis.uncertain.len(), 1);
    }
}

/// Unprefixed type names still bind through the in-scope default XML namespace.
#[test]
fn unprefixed_generated_type_uses_the_default_namespace() {
    let input = format!("<v:Type xmlns:v='{CORE}' xmlns='{CFG}'>CatalogRef.Old</v:Type>");
    let analysis = catalog().analyze_xml(&input, &[]).expect("analysis");
    assert_eq!(analysis.replacements.len(), 1);
    assert!(apply(&input, &analysis).contains(">CatalogRef.Новый<"));
}
