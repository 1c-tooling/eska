//! References are checked independently from filesystem ownership and supplier decisions.

use super::*;
use crate::project_core::source_format::SourcePath;

const DCS: &str = "http://v8.1c.ru/8.1/data-composition-system/schema";
const FORM: &str = "http://v8.1c.ru/8.3/xcf/logform";
const XSI: &str = "http://www.w3.org/2001/XMLSchema-instance";

/// The surrounding native schema stays opaque while its declared query owns one plain source.
fn schema(value: &str) -> String {
    format!(
        "\u{feff}<DataCompositionSchema xmlns=\"{DCS}\" xmlns:xsi=\"{XSI}\"><!--keep--><dataSet xsi:type=\"DataSetQuery\"><name>Данные</name><query>{value}</query><field/></dataSet></DataCompositionSchema>\r\n"
    )
}

/// The caller supplies portable paths, without a file-naming convention hidden in the codec.
fn paths(document: &NativeQueryDocument) -> BTreeMap<QuerySlot, SourcePath> {
    document
        .queries()
        .iter()
        .enumerate()
        .map(|(index, query)| {
            (
                query.slot.clone(),
                SourcePath::parse(&format!("Queries/Запрос_{index}.sdbl")).unwrap(),
            )
        })
        .collect()
}

/// Extraction removes the only query value from XML, including packages, comments and XML escapes.
#[test]
fn plain_sources_hold_the_sole_query_value_and_resolve_without_native_markup() {
    let input = schema("// query comment\r\nВЫБРАТЬ &quot;&amp;&lt;&gt;&quot;;&#xD;\r\nSELECT 2");
    let native = NativeQueryDocument::from_xml(&input).unwrap();
    let path = SourcePath::parse("Queries/Имя & 'запрос'.sdbl").unwrap();
    let paths = BTreeMap::from([(native.queries()[0].slot.clone(), path.clone())]);
    let source = native.extract(&paths).unwrap();
    assert_eq!(
        source.queries()[&path],
        "// query comment\nВЫБРАТЬ \"&<>\";\r\nSELECT 2"
    );
    assert!(!source.xml().contains("query comment"));
    assert!(!source.xml().contains("SELECT 2"));
    assert!(source.xml().contains("<!--keep-->"));
    assert!(
        source
            .xml()
            .contains("source=\"Queries/Имя &amp; 'запрос'.sdbl\"")
    );
    let (xml, values) = source.into_parts();
    let references = QueryReferenceDocument::from_xml(&xml).unwrap();
    assert_eq!(references.references()[0].source, path);
    assert!(xml[references.references()[0].source_range()].starts_with("<query>"));
    let output = references.materialize(&values).unwrap();
    assert_eq!(output, input);
    assert!(!output.contains(QUERY_REFERENCE_NAMESPACE));
    assert!(NativeQueryDocument::from_xml(&xml).is_err());
}

/// Native EOL, character references, qualified names, CDATA and empty tags have a closed inverse recipe.
#[test]
fn persisted_reference_recipes_preserve_native_lexical_shapes_and_changed_values() {
    for input in [
        schema("SELECT 'x', &quot;y&quot;&#xd;\n// old"),
        schema("SELECT 1\r// old"),
        schema("<![CDATA[SELECT \"<&>\"\r\n// old]]>"),
        schema("").replace("<query></query>", "<query />"),
        schema("").replace("<query></query>", "<query/>"),
        schema(""),
        format!(
            "<d:DataCompositionSchema xmlns:d=\"{DCS}\" xmlns:s=\"{XSI}\"><d:dataSet s:type=\"d:DataSetQuery\"><d:name>N</d:name><d:query>SELECT 1</d:query></d:dataSet></d:DataCompositionSchema>"
        ),
    ] {
        let native = NativeQueryDocument::from_xml(&input).unwrap();
        let source = native.clone().extract(&paths(&native)).unwrap();
        let references = QueryReferenceDocument::from_xml(source.xml()).unwrap();
        assert_eq!(references.materialize(source.queries()).unwrap(), input);
        let text = "// new\nSELECT \"]]>&<\", 'x';\r\nВЫБРАТЬ 2".to_owned();
        let values = BTreeMap::from([(references.references()[0].source.clone(), text.clone())]);
        let output = references.materialize(&values).unwrap();
        assert_eq!(
            NativeQueryDocument::from_xml(&output).unwrap().queries()[0].text,
            text
        );
        assert!(!output.contains(QUERY_REFERENCE_NAMESPACE));
        assert_eq!(
            output,
            native
                .materialize(&BTreeMap::from([(native.queries()[0].slot.clone(), text)]))
                .unwrap()
        );
    }
}

/// Scoped names can change around stable references; identity still comes from the current native scope.
#[test]
fn reference_bindings_follow_dataset_scopes_including_union_members_and_renames() {
    let input = format!(
        "<DataCompositionSchema xmlns=\"{DCS}\" xmlns:xsi=\"{XSI}\"><dataSet xsi:type=\"DataSetUnion\"><name>Union</name><item xsi:type=\"DataSetQuery\"><name>Same</name><query>SELECT 1</query></item></dataSet><dataSet xsi:type=\"DataSetQuery\"><name>Same</name><query>SELECT 2</query></dataSet></DataCompositionSchema>"
    );
    let native = NativeQueryDocument::from_xml(&input).unwrap();
    let source = native.clone().extract(&paths(&native)).unwrap();
    let renamed = source
        .xml()
        .replace("<name>Union</name>", "<name>Renamed</name>");
    let references = QueryReferenceDocument::from_xml(&renamed).unwrap();
    assert_eq!(
        references.references()[0].slot,
        QuerySlot::Composition {
            datasets: vec!["Renamed".into(), "Same".into()]
        }
    );
    assert_eq!(
        references.materialize(source.queries()).unwrap(),
        input.replace("<name>Union</name>", "<name>Renamed</name>")
    );
    let duplicated = source.xml().replace("Запрос_1.sdbl", "Запрос_0.sdbl");
    assert_eq!(
        QueryReferenceDocument::from_xml(&duplicated)
            .unwrap_err()
            .code,
        "source_query_reference_invalid"
    );
}

/// Root attributes alone own saved list queries; unrelated query text and automatic lists stay opaque.
#[test]
fn list_references_keep_attribute_scope_and_ignore_unrelated_query_text() {
    let input = format!(
        "<Form xmlns=\"{FORM}\" xmlns:xsi=\"{XSI}\" version=\"2.20\"><ChildItems><Table name=\"Grid\" id=\"5\"/></ChildItems><Attributes><Attribute name=\"Rows\" id=\"5\"><Settings xsi:type=\"DynamicList\"><ManualQuery>true</ManualQuery><QueryText>SELECT 1</QueryText></Settings></Attribute><Attribute name=\"Auto\" id=\"6\"><Settings xsi:type=\"DynamicList\"><ManualQuery>false</ManualQuery></Settings></Attribute></Attributes><Commands><Command id=\"5\"><QueryText>opaque</QueryText></Command></Commands></Form>"
    );
    let native = NativeQueryDocument::from_xml(&input).unwrap();
    let source = native.clone().extract(&paths(&native)).unwrap();
    let references = QueryReferenceDocument::from_xml(source.xml()).unwrap();
    assert_eq!(references.references().len(), 1);
    assert_eq!(
        references.references()[0].slot,
        QuerySlot::DynamicList {
            attribute: FormId::parse("5").unwrap()
        }
    );
    assert!(source.xml().contains("<QueryText>opaque</QueryText>"));
    assert_eq!(references.materialize(source.queries()).unwrap(), input);
    let automatic = input.replace(
        "<ManualQuery>true</ManualQuery><QueryText>SELECT 1</QueryText>",
        "<ManualQuery>false</ManualQuery>",
    );
    let source = NativeQueryDocument::from_xml(&automatic)
        .unwrap()
        .extract(&BTreeMap::new())
        .unwrap();
    assert_eq!(source.xml(), automatic);
    assert!(source.queries().is_empty());
    assert_eq!(
        QueryReferenceDocument::from_xml(source.xml())
            .unwrap()
            .materialize(source.queries())
            .unwrap(),
        automatic
    );
}

/// Missing, extra or colliding bindings cannot fall back to old values or overwrite another plain source.
#[test]
fn extraction_and_resolution_require_exact_portable_source_sets() {
    let native = NativeQueryDocument::from_xml(&schema("SELECT 1")).unwrap();
    assert!(native.clone().extract(&BTreeMap::new()).is_err());
    let mut extra = paths(&native);
    extra.insert(
        QuerySlot::Composition {
            datasets: vec!["Other".into()],
        },
        SourcePath::parse("Queries/Other.sdbl").unwrap(),
    );
    assert!(native.clone().extract(&extra).is_err());
    let wrong = BTreeMap::from([(
        native.queries()[0].slot.clone(),
        SourcePath::parse("Queries/Wrong.txt").unwrap(),
    )]);
    assert!(native.clone().extract(&wrong).is_err());
    let source = native.clone().extract(&paths(&native)).unwrap();
    let reference = QueryReferenceDocument::from_xml(source.xml()).unwrap();
    assert!(reference.materialize(&BTreeMap::new()).is_err());
    let mut extra = source.queries().clone();
    extra.insert(
        SourcePath::parse("Queries/Other.sdbl").unwrap(),
        "SELECT 2".into(),
    );
    assert!(reference.materialize(&extra).is_err());
    let bad = BTreeMap::from([(
        reference.references()[0].source.clone(),
        "SELECT \u{0000}".into(),
    )]);
    assert_eq!(
        reference.materialize(&bad).unwrap_err().code,
        "source_xml_character_invalid"
    );
    for path in ["../Outside.sdbl", "/Outside.sdbl", "Queries\\Wrong.sdbl"] {
        assert!(
            QueryReferenceDocument::from_xml(&source.xml().replace("Queries/Запрос_0.sdbl", path))
                .is_err()
        );
    }
    assert_eq!(
        reference.materialize(source.queries()).unwrap(),
        schema("SELECT 1")
    );
}

/// A marker has one protocol version, known recipe fields and no hidden or duplicate text.
#[test]
fn malformed_or_inline_reference_markup_refuses_parsing() {
    let native = NativeQueryDocument::from_xml(&schema("SELECT 1")).unwrap();
    let source = native.clone().extract(&paths(&native)).unwrap();
    for input in [
        source
            .xml()
            .replace(QUERY_REFERENCE_NAMESPACE, "urn:eska:query:2"),
        source
            .xml()
            .replace("coding=\"text\"", "coding=\"unknown\""),
        source
            .xml()
            .replace("coding=\"text\"", "coding=\"cdata\" quotes=\"true\""),
        source.xml().replace(" eol=\"lf\"", ""),
        source.xml().replace("eol=\"lf\"", "eol=\"mixed\""),
        source.xml().replace("cr=\"13\"", "cr=\"unknown\""),
        source.xml().replace("quotes=\"false\"", "quotes=\"1\""),
        source.xml().replace("empty=\"false\"", "empty=\"maybe\""),
        source
            .xml()
            .replace("empty=\"false\"", "empty=\"false\" extra=\"value\""),
        source.xml().replace("<eska:query", "SELECT 1<eska:query"),
        source
            .xml()
            .replace("<eska:query", "<!--query--><eska:query"),
        source
            .xml()
            .replace("<eska:query", "<?query old?><eska:query"),
        schema("SELECT 1"),
    ] {
        assert!(
            QueryReferenceDocument::from_xml(&input).is_err(),
            "accepted {input}"
        );
    }
    let cdata = source
        .xml()
        .replace("coding=\"text\"", "coding=\"cdata\"")
        .replace("quotes=\"false\"", "quotes=\"true\"");
    assert!(QueryReferenceDocument::from_xml(&cdata).is_err());
    let whitespace = source
        .xml()
        .replace("<eska:query", " \n\t<eska:query")
        .replace("/></query>", "/>\n </query>");
    assert_eq!(
        QueryReferenceDocument::from_xml(&whitespace)
            .unwrap()
            .materialize(source.queries())
            .unwrap(),
        schema("SELECT 1")
    );
}

/// Protocol namespaces may exist only on declared local markers, so native output cannot retain them.
#[test]
fn reference_namespace_cannot_hide_in_opaque_native_gaps() {
    let input = schema("SELECT 1");
    let native = NativeQueryDocument::from_xml(&input).unwrap();
    let source = native.clone().extract(&paths(&native)).unwrap();
    for xml in [input, source.xml().into()] {
        let root_namespace = xml.replace(
            "xmlns:xsi=",
            &format!("xmlns:unused=\"{QUERY_REFERENCE_NAMESPACE}\" xmlns:xsi="),
        );
        assert!(NativeQueryDocument::from_xml(&root_namespace).is_err());
        assert!(QueryReferenceDocument::from_xml(&root_namespace).is_err());
        let opaque_marker = xml.replace(
            "<field/>",
            &format!("<field xmlns:q=\"{QUERY_REFERENCE_NAMESPACE}\"/>"),
        );
        assert!(NativeQueryDocument::from_xml(&opaque_marker).is_err());
        assert!(QueryReferenceDocument::from_xml(&opaque_marker).is_err());
    }
}

/// Case aliases are rejected even when native slots and case-sensitive source spellings differ.
#[test]
fn query_source_paths_cannot_alias_each_other_by_case() {
    let input = schema("SELECT 1").replace("</DataCompositionSchema>", "<dataSet xsi:type=\"DataSetQuery\"><name>Other</name><query>SELECT 2</query></dataSet></DataCompositionSchema>");
    let native = NativeQueryDocument::from_xml(&input).unwrap();
    let source = native.clone().extract(&paths(&native)).unwrap();
    let aliased = source
        .xml()
        .replace("Queries/Запрос_1.sdbl", "queries/запрос_0.sdbl");
    assert_eq!(
        QueryReferenceDocument::from_xml(&aliased).unwrap_err().code,
        "source_query_reference_invalid"
    );
    let collision = BTreeMap::from([
        (
            native.queries()[0].slot.clone(),
            SourcePath::parse("Queries/Text.sdbl").unwrap(),
        ),
        (
            native.queries()[1].slot.clone(),
            SourcePath::parse("queries/text.sdbl").unwrap(),
        ),
    ]);
    assert_eq!(
        native.extract(&collision).unwrap_err().code,
        "source_query_reference_invalid"
    );
}
