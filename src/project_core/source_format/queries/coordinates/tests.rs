//! Coordinate expectations cover native transport forms independently of lexical-token implementation.

use std::collections::BTreeMap;

use crate::project_core::source_format::{
    NativeQueryDocument, SourcePath, queries::restore_native_queries,
};

const DCS: &str = "http://v8.1c.ru/8.1/data-composition-system/schema";
const XSI: &str = "http://www.w3.org/2001/XMLSchema-instance";

/// Two saved queries expose offsets after earlier variable-length XML replacements.
fn schema(text: &str) -> String {
    format!(
        "\u{feff}<DataCompositionSchema xmlns=\"{DCS}\" xmlns:xsi=\"{XSI}\"><dataSet xsi:type=\"DataSetQuery\"><name>A</name><query>{text}</query></dataSet><dataSet xsi:type=\"DataSetQuery\"><name>B</name><query>SELECT 2</query></dataSet><!--tail--></DataCompositionSchema>\r\n"
    )
}

/// Text and CDATA produce the same plain coordinates for Unicode, normalized EOL and split delimiters.
#[test]
fn native_entities_and_split_cdata_address_decoded_utf8_without_per_character_storage() {
    let value = "ВЫБРАТЬ \"<&>\", ']]>';\r\n// end";
    for seed in [
        "SELECT &quot;x&quot;&#xd;\r\n// old",
        "<![CDATA[SELECT \"x\"\r\n// old]]>",
    ] {
        let original = NativeQueryDocument::from_xml(&schema(seed)).unwrap();
        let values = original
            .queries()
            .iter()
            .map(|query| (query.slot.clone(), value.to_owned()))
            .collect::<BTreeMap<_, _>>();
        let native = original.materialize(&values).unwrap();
        let parsed = NativeQueryDocument::from_xml(&native).unwrap();
        let query = &parsed.queries()[0];
        let needle = if seed.starts_with("<![CDATA[") {
            "]]]]><![CDATA[>"
        } else {
            "]]&gt;"
        };
        for (needle, decoded) in [("ВЫБРАТЬ", "ВЫБРАТЬ"), (needle, "]]>"), ("&#13;", "\r")]
        {
            let needle = if !seed.starts_with("<![CDATA[") && needle == "&#13;" {
                "&#xd;"
            } else {
                needle
            };
            let start = native.find(needle).unwrap();
            let span = start..start + needle.len();
            let actual = query.decoded_range(&native, &span).unwrap().unwrap();
            assert_eq!(&value[actual], decoded);
        }
        let start = native.find("ВЫБРАТЬ").unwrap();
        assert!(
            query
                .decoded_range(&native, &(start + 1..start + 2))
                .is_err()
        );
        assert_eq!(
            query
                .decoded_range(&native, &(start..start))
                .unwrap()
                .unwrap(),
            0..0
        );
        let changed = native.replace("ВЫБРАТЬ", "SELECT");
        assert!(query.decoded_range(&changed, &(start..start)).is_err());
    }
    let empty = schema("").replace("<query></query>", "<query />");
    let parsed = NativeQueryDocument::from_xml(&empty).unwrap();
    let query = &parsed.queries()[0];
    assert_eq!(
        query.decoded_range(&empty, &query.native_range()).unwrap(),
        Some(0..0)
    );
}

/// A self-closing query must not claim coordinates in adjacent XML or another query.
#[test]
fn empty_query_coordinates_do_not_escape_their_native_slot() {
    let native = schema("").replace("<query></query>", "<query />");
    let parsed = NativeQueryDocument::from_xml(&native).unwrap();
    let query = &parsed.queries()[0];
    let slot = query.native_range();
    for range in [
        0..slot.start,
        slot.start - 1..slot.end,
        slot.start..slot.end + 1,
        parsed.queries()[1].native_range(),
    ] {
        assert_eq!(query.decoded_range(&native, &range).unwrap(), None);
    }
    assert_eq!(query.decoded_range(&native, &slot).unwrap(), Some(0..0));
}

/// Gap locations handle exact query-node endpoints and points after multiple earlier replacements.
#[test]
fn restoring_reference_nodes_preserves_gap_offsets_and_exact_end_boundaries() {
    let native = schema("SELECT 1");
    let parsed = NativeQueryDocument::from_xml(&native).unwrap();
    let paths = parsed
        .queries()
        .iter()
        .enumerate()
        .map(|(index, query)| {
            (
                query.slot.clone(),
                SourcePath::parse(&format!("Queries/{index}.sdbl")).unwrap(),
            )
        })
        .collect::<BTreeMap<_, _>>();
    let source = parsed.clone().extract(&paths).unwrap();
    let start = native.find("<!--tail-->").unwrap();
    let (restored, range) =
        restore_native_queries(source.xml(), &native, &(start..start + "<!--tail-->".len()))
            .unwrap();
    assert_eq!(restored, source.xml());
    assert_eq!(&restored[range], "<!--tail-->");
    let node = parsed.queries()[0].native_range();
    let (_, range) = restore_native_queries(source.xml(), &native, &(node.end..node.end)).unwrap();
    let reference =
        crate::project_core::source_format::QueryReferenceDocument::from_xml(source.xml()).unwrap();
    assert_eq!(
        range,
        reference.references()[0].source_range().end..reference.references()[0].source_range().end
    );
}
