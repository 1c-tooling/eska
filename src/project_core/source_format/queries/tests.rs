//! Native scope and byte-preservation checks use independent XML expectations, without query-language analysis.

use super::*;

const DCS: &str = "http://v8.1c.ru/8.1/data-composition-system/schema";
const FORM: &str = "http://v8.1c.ru/8.3/xcf/logform";
const XSI: &str = "http://www.w3.org/2001/XMLSchema-instance";

/// A small schema retains an unrelated field, a comment and query text without platform dependencies.
fn schema(content: &str) -> String {
    format!(
        "\u{feff}<DataCompositionSchema xmlns=\"{DCS}\" xmlns:xsi=\"{XSI}\"><!--keep--><dataSet xsi:type=\"DataSetQuery\"><name>Данные</name><field/><query>{content}</query></dataSet><settingsVariant/></DataCompositionSchema>\r\n"
    )
}

/// Saved query values are selected by native binding rather than sequence or filename.
fn values(document: &NativeQueryDocument) -> BTreeMap<QuerySlot, String> {
    document
        .queries()
        .iter()
        .map(|query| (query.slot.clone(), query.text.clone()))
        .collect()
}

/// The same dataset name in independent unions is valid; native order does not change those identities.
#[test]
fn query_bindings_use_full_dataset_scopes_and_qualified_type_names() {
    let input = format!(
        "<d:DataCompositionSchema xmlns:d=\"{DCS}\" xmlns:s=\"{XSI}\"><d:dataSet s:type=\"d:DataSetUnion\"><d:name>First</d:name><d:item s:type=\"d:DataSetQuery\"><d:name>Shared</d:name><d:query>SELECT 1</d:query></d:item></d:dataSet><d:dataSet s:type=\"d:DataSetUnion\"><d:name>Second</d:name><d:item s:type=\"d:DataSetQuery\"><d:name>Shared</d:name><d:query>SELECT 2</d:query></d:item></d:dataSet><d:dataSet s:type=\"d:DataSetObject\"><d:name>Object</d:name></d:dataSet></d:DataCompositionSchema>"
    );
    let document = NativeQueryDocument::from_xml(&input).unwrap();
    assert_eq!(document.to_xml().unwrap(), input);
    assert_eq!(document.queries().len(), 2);
    assert_eq!(
        document.queries()[0].slot,
        QuerySlot::Composition {
            datasets: vec!["First".into(), "Shared".into()]
        }
    );
    let mut changed = values(&document);
    changed.insert(document.queries()[1].slot.clone(), "SELECT 3".into());
    assert_eq!(
        document.materialize(&changed).unwrap(),
        input.replace(">SELECT 2<", ">SELECT 3<")
    );
    let swapped = input
        .replace(">First<", ">TEMP<")
        .replace(">Second<", ">First<")
        .replace(">TEMP<", ">Second<");
    let swapped = NativeQueryDocument::from_xml(&swapped).unwrap();
    assert_eq!(values(&swapped).len(), 2);
    assert_eq!(swapped.queries()[0].slot, document.queries()[1].slot);
}

/// UI IDs and unrelated `QueryText` nodes do not own a dynamic-list query.
#[test]
fn dynamic_lists_use_root_attribute_ids_and_ignore_automatic_lists_without_text() {
    let input = format!(
        "<Form xmlns=\"{FORM}\" xmlns:xsi=\"{XSI}\" version=\"2.20\"><ChildItems><Table name=\"Grid\" id=\"5\"/></ChildItems><Attributes><Attribute name=\"Rows\" id=\"5\"><Settings xsi:type=\"DynamicList\"><ManualQuery>true</ManualQuery><QueryText>ВЫБРАТЬ 1</QueryText><MainTable/></Settings></Attribute><Attribute name=\"Auto\" id=\"6\"><Settings xsi:type=\"DynamicList\"><ManualQuery>false</ManualQuery><MainTable/></Settings></Attribute></Attributes><Commands><Command name=\"Action\" id=\"5\"><QueryText>opaque</QueryText></Command></Commands></Form>"
    );
    let document = NativeQueryDocument::from_xml(&input).unwrap();
    assert_eq!(document.queries().len(), 1);
    assert_eq!(
        document.queries()[0].slot,
        QuerySlot::DynamicList {
            attribute: FormId::parse("5").unwrap()
        }
    );
    let mut changed = values(&document);
    changed.insert(document.queries()[0].slot.clone(), "ВЫБРАТЬ 2".into());
    assert_eq!(
        document.materialize(&changed).unwrap(),
        input.replace(">ВЫБРАТЬ 1<", ">ВЫБРАТЬ 2<")
    );
    let automatic = input.replace(
        "<ManualQuery>true</ManualQuery><QueryText>ВЫБРАТЬ 1</QueryText>",
        "<ManualQuery>false</ManualQuery>",
    );
    let automatic = NativeQueryDocument::from_xml(&automatic).unwrap();
    assert!(automatic.queries().is_empty());
    assert!(automatic.materialize(&BTreeMap::new()).is_ok());
}

/// Logical query EOL, comments, packages and XML characters round-trip without formatting the language.
#[test]
fn changed_text_preserves_packages_comments_quotes_entities_and_logical_eol() {
    let input =
        schema("// old\nSELECT &quot;x&quot;, &apos;y&apos;, A &lt; B, A &gt; B &amp; C&#xD;\n;");
    let document = NativeQueryDocument::from_xml(&input).unwrap();
    assert_eq!(document.to_xml().unwrap(), input);
    let text = "// комментарий\nВЫБРАТЬ \"&<>\";\r\nSELECT 'x'\n// end".to_owned();
    let changed = BTreeMap::from([(document.queries()[0].slot.clone(), text.clone())]);
    let output = document.materialize(&changed).unwrap();
    assert_eq!(
        output,
        schema(
            "// комментарий\nВЫБРАТЬ &quot;&amp;&lt;&gt;&quot;;&#xD;\nSELECT &apos;x&apos;\n// end"
        )
    );
    assert_eq!(
        NativeQueryDocument::from_xml(&output).unwrap().queries()[0].text,
        text
    );
}

/// Uniform native CRLF/CR are transport, while XML character references preserve actual query CR.
#[test]
fn native_query_transport_is_independent_from_decoded_query_eol() {
    for eol in ["\n", "\r\n", "\r"] {
        let input = schema(&format!("SELECT 1{eol}// old{eol}"));
        let document = NativeQueryDocument::from_xml(&input).unwrap();
        assert_eq!(document.queries()[0].text, "SELECT 1\n// old\n");
        assert_eq!(document.to_xml().unwrap(), input);
        let changed = BTreeMap::from([(
            document.queries()[0].slot.clone(),
            "SELECT 2\r\n// new\n".into(),
        )]);
        assert_eq!(
            document.materialize(&changed).unwrap(),
            schema(&format!("SELECT 2&#13;{eol}// new{eol}"))
        );
    }
}

/// CDATA termination and actual CR can be represented without inserting executable XML.
#[test]
fn cdata_and_empty_query_slots_preserve_their_native_inverse() {
    let input = schema("<![CDATA[SELECT \"<&>\"\r\n// old]]>");
    let document = NativeQueryDocument::from_xml(&input).unwrap();
    assert_eq!(document.to_xml().unwrap(), input);
    let text = "SELECT \"]]>&<\"\r\n// new".to_owned();
    let changed = BTreeMap::from([(document.queries()[0].slot.clone(), text.clone())]);
    let output = document.materialize(&changed).unwrap();
    assert_eq!(
        NativeQueryDocument::from_xml(&output).unwrap().queries()[0].text,
        text
    );
    assert!(output.contains("]]]]><![CDATA[>"));
    assert!(output.contains("]]>&#13;<![CDATA["));
    let input = schema("").replace("<query></query>", "<query />");
    let document = NativeQueryDocument::from_xml(&input).unwrap();
    assert_eq!(document.to_xml().unwrap(), input);
    let changed = BTreeMap::from([(document.queries()[0].slot.clone(), "SELECT 1".into())]);
    assert_eq!(
        document.materialize(&changed).unwrap(),
        input.replace("<query />", "<query >SELECT 1</query>")
    );
}

/// Missing and extra values cannot restore previous text implicitly or modify the immutable document.
#[test]
fn incomplete_values_and_invalid_xml_characters_refuse_materialization() {
    let input = schema("SELECT 1");
    let document = NativeQueryDocument::from_xml(&input).unwrap();
    assert_eq!(
        document.materialize(&BTreeMap::new()).unwrap_err().code,
        "source_query_binding_invalid"
    );
    let mut extra = values(&document);
    extra.insert(
        QuerySlot::Composition {
            datasets: vec!["Other".into()],
        },
        "SELECT 2".into(),
    );
    assert_eq!(
        document.materialize(&extra).unwrap_err().code,
        "source_query_binding_invalid"
    );
    let bad = BTreeMap::from([(document.queries()[0].slot.clone(), "SELECT \u{0000}".into())]);
    assert_eq!(
        document.materialize(&bad).unwrap_err().code,
        "source_xml_character_invalid"
    );
    assert_eq!(document.to_xml().unwrap(), input);
}

/// Familiar local type names do not authorize foreign namespaces, duplicate slots or inconsistent settings.
#[test]
fn unverified_profiles_and_ambiguous_native_query_scopes_refuse_decoding() {
    let valid = schema("SELECT 1");
    for input in [
        valid.replace(
            "xsi:type=\"DataSetQuery\"",
            "xmlns:foreign=\"urn:foreign\" xsi:type=\"foreign:DataSetQuery\"",
        ),
        valid.replace(
            "xsi:type=\"DataSetQuery\"",
            "xsi:type=\"missing:DataSetQuery\"",
        ),
        valid.replace("DataSetQuery", "UnknownDataSet"),
        valid.replace("<query>SELECT 1</query>", ""),
        valid.replace(
            "<query>SELECT 1</query>",
            "<query>SELECT 1</query><query>SELECT 1</query>",
        ),
        valid.replace("<field/>", "<name>Other</name>"),
    ] {
        assert!(NativeQueryDocument::from_xml(&input).is_err());
    }
    let duplicated = valid.replace("</DataCompositionSchema>", "<dataSet xsi:type=\"DataSetQuery\"><name>данные</name><query>SELECT 2</query></dataSet></DataCompositionSchema>");
    assert_eq!(
        NativeQueryDocument::from_xml(&duplicated).unwrap_err().code,
        "source_query_binding_invalid"
    );
    for inner in [
        "<ManualQuery>false</ManualQuery><QueryText>SELECT 1</QueryText>",
        "<ManualQuery>true</ManualQuery>",
        "<ManualQuery>1</ManualQuery><QueryText>SELECT 1</QueryText>",
        "<ManualQuery>true</ManualQuery><QueryText/><QueryText/>",
    ] {
        let input = format!(
            "<Form xmlns=\"{FORM}\" xmlns:xsi=\"{XSI}\" version=\"2.20\"><Attributes><Attribute id=\"1\" name=\"Rows\"><Settings xsi:type=\"DynamicList\">{inner}</Settings></Attribute></Attributes></Form>"
        );
        assert!(NativeQueryDocument::from_xml(&input).is_err());
        assert!(NativeQueryDocument::from_xml(&input.replace("2.20", "2.19")).is_err());
    }
}

/// A lexical shape without a proven inverse refuses extraction instead of discarding XML annotations.
#[test]
fn unsupported_lexical_nodes_and_dtd_refuse_promotion() {
    for content in [
        "SELECT <!--keep-->1",
        "SELECT <?keep value?>1",
        "SELECT <foreign/>",
        "SELECT&#10;1",
        "SELECT\r\n1\n// mixed",
        "SELECT \"x\", &quot;y&quot;",
    ] {
        assert_eq!(
            NativeQueryDocument::from_xml(&schema(content))
                .unwrap_err()
                .code,
            "source_query_lexical_unverified"
        );
    }
    let dtd = schema("SELECT 1").replacen(
        "<DataCompositionSchema",
        "<!DOCTYPE root><DataCompositionSchema",
        1,
    );
    assert_eq!(
        NativeQueryDocument::from_xml(&dtd).unwrap_err().code,
        "source_xml_dtd_forbidden"
    );
}
