use super::*;
use crate::project::query::Affinity;

/// Keep fixture namespaces explicit so local names alone never establish a query owner.
fn scd(content: &str) -> String {
    format!(
        "\u{feff}<DataCompositionSchema xmlns='{SCHEMA}' xmlns:xsi='{XSI}' xmlns:s='{SCHEMA}'>{content}</DataCompositionSchema>"
    )
}

/// Forms use a different namespace and an explicit manual-query setting.
fn form(manual: &str, value: &str) -> String {
    format!(
        "<Form xmlns='{FORM}' xmlns:xsi='{XSI}'><Attributes><Attribute name='Список' id='1'><Settings xsi:type='DynamicList'><ManualQuery>{manual}</ManualQuery><QueryText>{value}</QueryText><MainTable>Catalog.Test</MainTable></Settings></Attribute></Attributes></Form>"
    )
}

#[test]
fn decodes_entities_cdata_crlf_comments_and_non_bmp_with_exact_coordinates() {
    let input = scd(
        "<dataSet xsi:type='s:DataSetQuery'><name>Набор</name><query ext='>'><!--before-->ВЫБРАТЬ &quot;😀&quot;, A &lt; 2 &amp; B&#13;\r\n<![CDATA[{ГДЕ X < 3}]]><!--after--></query></dataSet>",
    );
    let queries = read_xml(&input).expect("XML");
    let [query] = queries.as_slice() else {
        panic!("one query")
    };
    assert_eq!(
        query.identity,
        QueryIdentity::DataSet(vec!["Набор".to_owned()])
    );
    assert_eq!(query.text, "ВЫБРАТЬ \"😀\", A < 2 & B\r\n{ГДЕ X < 3}");
    assert_eq!(query.comments.len(), 2);
    for (byte, character) in query.text.char_indices() {
        let source = query
            .map
            .query_to_source(&query.text, byte, Affinity::After)
            .expect("mapped start");
        assert_eq!(
            query.map.source_to_query(&input, source),
            Some(byte),
            "{character}"
        );
    }
    let entity = input.find("&quot;").expect("entity");
    assert_eq!(query.map.source_to_query(&input, entity + 1), None);
    let comment = query.comments[0].source.start + 1;
    assert_eq!(query.map.source_to_query(&input, comment), None);
}

#[test]
fn dataset_paths_and_namespaces_distinguish_union_children_and_unrelated_extensions() {
    let input = scd(
        "<dataSet xsi:type='DataSetUnion'><name>Общий</name><dataSet xsi:type='DataSetQuery'><name>Первый</name><query>SELECT 1; SELECT 2</query></dataSet><dataSet xsi:type='DataSetObject'><name>Объект</name></dataSet></dataSet><x:dataSet xmlns:x='urn:extension'><x:query>ignore</x:query></x:dataSet>",
    );
    let queries = read_xml(&input).expect("union");
    assert_eq!(queries.len(), 1);
    assert_eq!(
        queries[0].identity,
        QueryIdentity::DataSet(vec!["Общий".to_owned(), "Первый".to_owned()])
    );
    assert_eq!(queries[0].text, "SELECT 1; SELECT 2");
    let wrong_type = scd(
        "<dataSet xmlns:x='urn:other' xsi:type='x:DataSetQuery'><name>A</name><query>ignore</query></dataSet>",
    );
    assert_eq!(read_xml(&wrong_type).expect("foreign type"), []);
}

#[test]
fn manual_dynamic_list_requires_saved_query_and_never_synthesizes_main_table() {
    let queries = read_xml(&form("true", "ВЫБРАТЬ &#x1F600;\n\n{ПОЛЕ}")).expect("manual");
    assert_eq!(
        queries[0].identity,
        QueryIdentity::DynamicList {
            name: "Список".to_owned(),
            id: "1".to_owned()
        }
    );
    assert_eq!(queries[0].text, "ВЫБРАТЬ 😀\n\n{ПОЛЕ}");
    assert_eq!(read_xml(&form("false", "unused")).expect("automatic"), []);
    assert_eq!(
        read_xml(&form("true", "x").replace("<QueryText>x</QueryText>", "")),
        Err(QueryError::UnsupportedStructure)
    );
    assert_eq!(
        read_xml(&form("false", "").replace("<QueryText></QueryText>", ""))
            .expect("automatic without query"),
        []
    );
}

#[test]
fn rejects_duplicate_identity_nested_values_dtd_and_wrong_payload_roots() {
    for content in [
        "<dataSet xsi:type='DataSetQuery'><name>A</name><query>x</query><query>y</query></dataSet>",
        "<dataSet xsi:type='DataSetQuery'><name>A</name><query><child/></query></dataSet>",
        "<dataSet xsi:type='DataSetQuery'><name>A</name><query>x</query></dataSet><dataSet xsi:type='DataSetQuery'><name>A</name><query>y</query></dataSet>",
    ] {
        assert_eq!(
            read_xml(&scd(content)),
            Err(QueryError::UnsupportedStructure)
        );
    }
    assert_eq!(
        read_xml("<!DOCTYPE r [<!ENTITY x 'x'>]><r>&x;</r>"),
        Err(QueryError::InvalidXml)
    );
    assert_eq!(
        read_xml("<Form xmlns='urn:wrong'/>"),
        Err(QueryError::UnsupportedStructure)
    );
}

#[test]
fn empty_and_split_cdata_values_have_valid_read_maps() {
    for content in [
        "<query/>",
        "<query></query>",
        "<query><![CDATA[]]></query>",
        "<query><!--keep--></query>",
    ] {
        let input = scd(&format!(
            "<dataSet xsi:type='DataSetQuery'><name>A</name>{content}</dataSet>"
        ));
        let query = read_xml(&input).expect("empty").remove(0);
        assert_eq!(query.text, "");
        let source = query
            .map
            .query_to_source(&query.text, 0, Affinity::After)
            .expect("empty insertion");
        assert_eq!(query.map.source_to_query(&input, source), Some(0));
    }
    let input = form("1", "A<!--keep-->B<![CDATA[&]]>&amp;");
    let query = read_xml(&input).expect("split").remove(0);
    assert_eq!(query.text, "AB&&");
    assert_eq!(query.comments[0].query_offset, 1);
}
