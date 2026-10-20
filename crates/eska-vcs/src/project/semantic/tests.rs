use gix::bstr::ByteSlice;

use super::{ChangeSet, ChangeStage, SemanticEventKind, descriptor_objects};
use crate::{
    project::{
        ProjectType,
        diff::{FileChange, ProjectDiff},
        metadata,
    },
    vcs::status::Change,
};

/// Workspace conversion retains both comparison edges and deterministic ordering.
#[test]
fn normalizes_workspace_edges() {
    let diff = ProjectDiff {
        files: vec![
            FileChange {
                path: b"src/B.bsl".as_bstr().to_owned(),
                index: None,
                worktree: Some(Change::Untracked),
            },
            FileChange {
                path: b"src/A.bsl".as_bstr().to_owned(),
                index: Some(Change::Modified),
                worktree: Some(Change::Modified),
            },
        ],
        display: Vec::new(),
    };

    let changes = ChangeSet::from_workspace(&diff);

    assert_eq!(changes.changes().len(), 3);
    assert_eq!(changes.changes()[0].path(), b"src/A.bsl".as_bstr());
    assert_eq!(changes.changes()[0].stage(), ChangeStage::Index);
    assert_eq!(changes.changes()[1].stage(), ChangeStage::Worktree);
    assert_eq!(changes.changes()[2].path(), b"src/B.bsl".as_bstr());
}

/// Descriptor parsing assigns independent stable identities to inline metadata objects.
#[test]
fn parses_descriptor_objects_and_property_signatures() {
    let source = br#"<MetaDataObject xmlns="http://v8.1c.ru/8.3/MDClasses"><Catalog><Properties><Name>Customers</Name></Properties><ChildObjects><Attribute><Properties><Name>Code</Name><Comment>value</Comment></Properties></Attribute></ChildObjects></Catalog></MetaDataObject>"#;
    let base = metadata::from_path(
        ProjectType::Configuration,
        b"Catalogs/Customers.xml".as_bstr(),
    )
    .expect("metadata path");

    let objects = descriptor_objects(source, &base).expect("valid descriptor");

    assert!(objects.contains_key("catalog:Customers"));
    assert!(objects.contains_key("catalog:Customers/attribute:Code"));
    assert!(
        objects["catalog:Customers/attribute:Code"]
            .properties
            .contains("value")
    );
}

/// Signatures preserve nested content and normalize attribute ordering and whitespace.
#[test]
fn xml_signatures_preserve_nested_structure() {
    let before = roxmltree::Document::parse(
        r#"<Properties b="2" a="1"><Name> Value </Name><Type><Kind>String</Kind></Type></Properties>"#,
    ).unwrap();
    let after = roxmltree::Document::parse(
        "<Properties a=\"1\" b=\"2\">\n<Name>Value</Name>\n<Type><Kind>String</Kind></Type>\n</Properties>",
    ).unwrap();
    let expected = "<Properties a=\"1\" b=\"2\"><Name>\"Value\"</><Type><Kind>\"String\"</></></>";
    assert_eq!(
        super::descriptor::xml_signature(before.root_element()),
        expected
    );
    assert_eq!(
        super::descriptor::xml_signature(after.root_element()),
        expected
    );
    let changed =
        roxmltree::Document::parse("<Properties><Type>String</Type></Properties>").unwrap();
    assert_ne!(
        super::descriptor::xml_signature(changed.root_element()),
        expected
    );
}

/// Namespace identities and literal text must not collapse into the same signature.
#[test]
fn xml_signatures_distinguish_namespaces_and_escaped_markup() {
    for (before, after) in [
        (
            "<Type xmlns='urn:one'>String</Type>",
            "<Type xmlns='urn:two'>String</Type>",
        ),
        (
            "<Type xmlns:x='urn:one' x:value='1'/>",
            "<Type xmlns:x='urn:two' x:value='1'/>",
        ),
        (
            "<Type>&lt;Kind&gt;String&lt;/Kind&gt;</Type>",
            "<Type><Kind>String</Kind></Type>",
        ),
    ] {
        let before = roxmltree::Document::parse(before).unwrap();
        let after = roxmltree::Document::parse(after).unwrap();
        assert_ne!(
            super::descriptor::xml_signature(before.root_element()),
            super::descriptor::xml_signature(after.root_element())
        );
    }
}

/// Prefix spelling, comments and processing instructions are not metadata values.
#[test]
fn xml_signatures_ignore_prefixes_and_non_data_nodes() {
    let before = roxmltree::Document::parse(
        "<a:Type xmlns:a='urn:type' a:value='1'><a:Kind>String</a:Kind></a:Type>",
    )
    .unwrap();
    let after = roxmltree::Document::parse("<b:Type xmlns:b='urn:type' b:value='1'><!-- comment --><?editor hint?><b:Kind>String</b:Kind></b:Type>").unwrap();
    assert_eq!(
        super::descriptor::xml_signature(before.root_element()),
        super::descriptor::xml_signature(after.root_element())
    );
}

/// Every event kind has an explicit stable machine name.
#[test]
fn semantic_event_names_are_unique() {
    let kinds = [
        SemanticEventKind::ObjectAdded,
        SemanticEventKind::ObjectRemoved,
        SemanticEventKind::ObjectChanged,
        SemanticEventKind::ModuleChanged,
        SemanticEventKind::MethodAdded,
        SemanticEventKind::MethodRemoved,
        SemanticEventKind::MethodChanged,
        SemanticEventKind::FunctionAdded,
        SemanticEventKind::FunctionRemoved,
        SemanticEventKind::FunctionChanged,
        SemanticEventKind::FormChanged,
        SemanticEventKind::MetadataAttributeChanged,
    ];
    let names: std::collections::BTreeSet<_> =
        kinds.into_iter().map(SemanticEventKind::as_str).collect();
    assert_eq!(names.len(), kinds.len());
}
