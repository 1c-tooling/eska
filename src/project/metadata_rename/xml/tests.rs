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

/// Concrete value annotations use the same generated types as Type descriptions and keep every other attribute.
#[test]
fn generated_type_annotations_are_namespace_bound_and_byte_minimal() {
    let input = format!(
        "\u{feff}<r xmlns:s='{XSI}' xmlns:t='{CFG}'>\r\n<a id='keep' s:type='t:CatalogRef.Old'>00000000-0000-0000-0000-000000000000</a><b xmlns='{CFG}' s:type='CatalogRef.Old'/><c xmlns:t='urn:foreign' s:type='t:CatalogRef.Old'/><d type='t:CatalogRef.Old'/><e s:type='t:CatalogRef.OldSuffix'/></r>"
    );
    for source in [
        input.clone(),
        input.replace("</r>", &format!("{}\r\n</r>", " ".repeat(512 * 1024))),
    ] {
        let analysis = catalog().analyze_xml(&source, &[]).unwrap();
        assert_eq!(analysis.replacements.len(), 2);
        assert_eq!(analysis.uncertain.len(), 2);
        assert_eq!(
            apply(&source, &analysis),
            source
                .replacen(
                    "s:type='t:CatalogRef.Old'",
                    "s:type='t:CatalogRef.Новый'",
                    1
                )
                .replace("s:type='CatalogRef.Old'", "s:type='CatalogRef.Новый'")
        );
    }
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

/// Fragment inspection must agree with whole-document analysis even when prefixes and entity spellings vary.
#[test]
fn streaming_and_dom_reference_plans_match_byte_for_byte() {
    for input in [
        format!(
            "\u{feff}<?xml version='1.0'?><r xmlns:xsi='{XSI}' xmlns:xr='{XR}' xmlns:cfg='{CFG}' xmlns:v='{CORE}'><a xsi:type='xr:MDObjectRef'>Catalog.O&#108;d</a><v:Type><![CDATA[cfg:CatalogRef.Old]]></v:Type><xr:GeneratedType name='CatalogRef.Old'/><mixed>Old<child/>Old</mixed></r>\r\n"
        ),
        format!(
            "<r xmlns:xsi='{XSI}' xmlns:xr='{XR}'><a xsi:type='xr:MDObjectRef'>Catalog.Old</a><a xmlns:xr='urn:foreign' xsi:type='xr:MDObjectRef'>Catalog.Old</a><text>Old<!-- x -->Old</text><?instruction Old?></r>"
        ),
        format!(
            "<r xmlns:xml='http://www.w3.org/XML/1998/name&#115;pace' xmlns:xr='{XR}'><xr:GeneratedType name='CatalogRef.Old'/></r>"
        ),
        format!(
            "<MetaDataObject xmlns='{MD}'><Catalog><Properties><Name>Owner</Name><DefaultObjectForm>Catalog.Old.Form.Main</DefaultObjectForm></Properties><ChildObjects><Attribute><Properties><ChoiceForm>Catalog.Old.Form.Choice</ChoiceForm></Properties></Attribute></ChildObjects></Catalog></MetaDataObject>"
        ),
    ] {
        let dom = catalog().analyze_dom(&input, &[]).expect("DOM");
        let streaming = catalog()
            .analyze_streaming(&input)
            .unwrap_or_else(|error| panic!("{error:?}: {input}"));
        assert_eq!(dom.replacements, streaming.replacements, "{input}");
        let mut dom_uncertain = dom.uncertain;
        dom_uncertain.sort_by_key(|hit| hit.range.start);
        assert_eq!(dom_uncertain, streaming.uncertain, "{input}");
    }
}

/// Invalid content cannot pass merely because it does not contain the renamed identifier.
#[test]
fn streaming_rejects_malformed_xml_outside_candidate_fragments() {
    for input in [
        "<r><1invalid/></r>",
        "<r><a></r>",
        "<r/><s/>",
        "<r>bad ]]> text</r>",
        "<r><!-- -- --></r>",
        "<r>&unknown;</r>",
        "<r>&#0;</r>",
        "<r a='<'/>",
        "<r a='1' a='2'/>",
        "<r xmlns:a='urn:x' xmlns:b='urn:&#120;' a:x='1' b:x='2'/>",
        "<r xmlns:xml='urn:wrong'/>",
        "<r xmlns:p=''><p:a/></r>",
        "<r><missing:a/></r>",
        "<r>\u{0001}</r>",
        "\u{feff}\u{feff}<r/>",
        "<r><?XML version='1.0'?></r>",
        "<r><?xml version='1.0'?></r>",
        "<?xml version='1.0' version='1.0'?><r/>",
        "<?xml version='1.0' standalone='invalid'?><r/>",
        "<xmlns:r/>",
        "<r xmlns='http://www.w3.org/XML/1998/namespace'/>",
        "<!DOCTYPE r><r/>",
    ] {
        assert!(catalog().analyze_streaming(input).is_err(), "{input}");
    }
}

/// Large spreadsheets exceed the DOM node ceiling while their sparse references remain editable.
#[test]
fn large_xml_keeps_exact_ranges_without_allocating_a_dom_for_every_cell() {
    let mut input = format!("\u{feff}<r xmlns:xsi='{XSI}' xmlns:xr='{XR}'>\r\n");
    input.push_str(&"<n>0</n>".repeat(510_000));
    input.push_str("<a xsi:type='xr:MDObjectRef'>Catalog.Old</a>\r\n</r>");
    let result = catalog().analyze_xml(&input, &[]).expect("large XML");
    assert_eq!(result.replacements.len(), 1);
    assert!(result.uncertain.is_empty());
    assert_eq!(
        apply(&input, &result),
        input.replace("Catalog.Old", "Catalog.Новый")
    );
}
