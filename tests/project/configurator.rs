use std::{collections::BTreeMap, fs, path::Path};

use crate::support::TestDir;
use eska::{
    cli::localization::{Locale, Localizer},
    project::{
        configurator::{
            ChildrenState, ConfiguratorSchema, ConfiguratorTree, ModuleAvailability, TreeLabel,
            TreeOptions,
        },
        designer_source::{DesignerSource, open_projects},
        metadata_model::{
            CollectionKind, MetadataKind, MetadataObject, ModuleRole, NodeId, ObjectId,
        },
        metadata_parser::{self, PropertiesMode},
    },
};
use serde::Deserialize;
use serde_json::{Value, json};

/// Copy static XML fragments into an isolated runnable project, preserving their original bytes.
fn copy(source: &Path, destination: &Path) {
    fs::create_dir_all(destination).unwrap();
    for entry in fs::read_dir(source).unwrap() {
        let entry = entry.unwrap();
        if entry.file_type().unwrap().is_dir() {
            copy(&entry.path(), &destination.join(entry.file_name()));
        } else {
            fs::copy(entry.path(), destination.join(entry.file_name())).unwrap();
        }
    }
}

/// Open one test-owned project with a mandatory manifest; never edit a user's configuration.
fn project(case: &str) -> (TestDir, DesignerSource) {
    let directory = TestDir::new();
    copy(
        &Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/designer")
            .join(case),
        &directory.0.join("src"),
    );
    fs::write(
        directory.0.join("eska.toml"),
        format!("[project]\ntype='{case}'\n"),
    )
    .unwrap();
    let source = open_projects(&directory.0, &[], false).unwrap().remove(0);
    (directory, source)
}

/// Resolve only module paths for the descriptor's already-loaded owners, then project the tree.
fn tree(source: &DesignerSource, owner: &ObjectId) -> ConfiguratorTree {
    let schema = ConfiguratorSchema::for_source(source);
    let parsed = metadata_parser::load(source, owner, PropertiesMode::Summary).unwrap();
    let modules = parsed
        .objects
        .iter()
        .map(|object| {
            (
                object.metadata.id().clone(),
                ModuleAvailability::resolve(
                    source,
                    &schema,
                    object.metadata.id(),
                    object.metadata.kind(),
                )
                .unwrap(),
            )
        })
        .collect();
    ConfiguratorTree::build(&schema, &parsed, &modules).unwrap()
}

/// Test-only readable IDs preserve object identity independently of translated labels.
fn node_id(id: &NodeId) -> String {
    match id {
        NodeId::Object(id) => format!("object:{id}"),
        NodeId::Module { owner, role } => format!("module:{owner}#{}", role.as_str()),
        NodeId::Collection { owner, kind } => {
            let key = match kind {
                CollectionKind::Metadata(kind) => kind.as_str(),
                CollectionKind::Common => "common",
                CollectionKind::Modules => "modules",
                CollectionKind::Unsupported => "unsupported",
            };
            format!("group:{owner}#{key}")
        }
    }
}

/// Flatten a visible tree in preorder, with keys rather than localized UI strings.
fn snapshot(tree: &ConfiguratorTree, id: &NodeId, output: &mut Vec<Value>) {
    let node = tree.node(id).unwrap();
    let label = match &node.label {
        TreeLabel::Name(name) => format!("name:{name}"),
        TreeLabel::Key(key) => key.clone(),
    };
    output.push(json!([
        node_id(id),
        node.parent.as_ref().map(node_id),
        label,
        format!("{:?}", node.state),
        node.expanded_by_default
    ]));
    for child in tree.children(id, TreeOptions::default()).unwrap() {
        snapshot(tree, &child.id, output);
    }
}

/// Root types, nested attributes, all registers and existing modules have reviewed golden trees.
#[test]
fn configurator_golden_trees_cover_four_project_types_and_owner_shapes() {
    let mut actual = BTreeMap::new();
    for case in ["configuration", "extension", "report", "processing"] {
        let (_directory, source) = project(case);
        let projection = tree(&source, source.root().id());
        let mut rows = Vec::new();
        snapshot(&projection, projection.root(), &mut rows);
        actual.insert(case.to_owned(), rows);
        if case == "configuration" {
            for (kind, name) in [
                (MetadataKind::Catalog, "Контрагенты"),
                (MetadataKind::InformationRegister, "Курсы"),
                (MetadataKind::AccumulationRegister, "Остатки"),
                (MetadataKind::AccountingRegister, "Проводки"),
                (MetadataKind::CalculationRegister, "Начисления"),
            ] {
                let owner = MetadataObject::new(kind, name.into(), "id".into(), None).unwrap();
                let projection = tree(&source, owner.id());
                let mut rows = Vec::new();
                snapshot(&projection, projection.root(), &mut rows);
                actual.insert(kind.as_str().to_owned(), rows);
            }
        }
    }
    let expected: BTreeMap<String, Vec<Value>> =
        serde_json::from_str(include_str!("../fixtures/designer/expected-tree.json")).unwrap();
    assert_eq!(actual, expected);
}

/// View filtering preserves nested empty groups and keeps Documents with only numerators.
#[test]
fn configurator_filters_only_proven_empty_root_sections() {
    let (_directory, source) = project("configuration");
    let schema = ConfiguratorSchema::for_source(&source);
    let xml = "<MetaDataObject xmlns='http://v8.1c.ru/8.3/MDClasses'><Configuration uuid='id'><Properties><Name>ТестоваяКонфигурация</Name></Properties><ChildObjects><DocumentNumerator>Номер</DocumentNumerator></ChildObjects></Configuration></MetaDataObject>";
    // Match the checked root name, not a hard-coded directory or generated tree identity.
    let xml = xml.replace("ТестоваяКонфигурация", source.root().name());
    let parsed = metadata_parser::parse(&xml, None, PropertiesMode::Summary).unwrap();
    let projection = ConfiguratorTree::build(
        &schema,
        &parsed,
        &BTreeMap::from([(
            source.root().id().clone(),
            ModuleAvailability::Loaded(vec![]),
        )]),
    )
    .unwrap();
    let visible = projection
        .children(projection.root(), TreeOptions::default())
        .unwrap();
    assert_eq!(visible.len(), 1);
    assert!(matches!(
        visible[0].id,
        NodeId::Collection {
            kind: CollectionKind::Metadata(MetadataKind::Document),
            ..
        }
    ));
    let nested = projection
        .children(&visible[0].id, TreeOptions::default())
        .unwrap();
    assert_eq!(nested.len(), 2);
    assert_eq!(nested[1].state, ChildrenState::Empty);
    assert!(
        projection
            .children(
                projection.root(),
                TreeOptions {
                    hide_empty_root_sections: false
                }
            )
            .unwrap()
            .len()
            > 10
    );
}

/// Deferred/failed module lookup must not invent BSL nodes or be mistaken for empty content.
#[test]
fn configurator_keeps_unloaded_errors_and_hides_binary_only_modules() {
    let (_directory, source) = project("configuration");
    let owner = MetadataObject::new(
        MetadataKind::CommonModule,
        "ЗащищенныйМодуль".into(),
        "id".into(),
        None,
    )
    .unwrap();
    let projection = tree(&source, owner.id());
    assert!(
        projection
            .children(projection.root(), TreeOptions::default())
            .unwrap()
            .is_empty()
    );
    let schema = ConfiguratorSchema::for_source(&source);
    let parsed = metadata_parser::load(&source, owner.id(), PropertiesMode::Summary).unwrap();
    for (availability, state) in [
        (ModuleAvailability::Unloaded, ChildrenState::Unloaded),
        (ModuleAvailability::Failed, ChildrenState::Error),
    ] {
        let projection = ConfiguratorTree::build(
            &schema,
            &parsed,
            &BTreeMap::from([(owner.id().clone(), availability)]),
        )
        .unwrap();
        let children = projection
            .children(projection.root(), TreeOptions::default())
            .unwrap();
        assert_eq!(children.len(), 1);
        assert_eq!(children[0].state, state);
        assert!(children[0].expanded_by_default);
        assert!(children[0].children.is_empty());
    }
}

/// Names retain declaration order, even when alphabetical ordering would put them elsewhere.
#[test]
fn configurator_preserves_declaration_order_and_unknown_fragments() {
    let (_directory, source) = project("configuration");
    let schema = ConfiguratorSchema::for_source(&source);
    let xml = "<MetaDataObject xmlns='http://v8.1c.ru/8.3/MDClasses'><Catalog uuid='id'><Properties><Name>Владелец</Name></Properties><ChildObjects><Form>Я</Form><Future>Неизвестный</Future><Form>А</Form><Catalog>НеуместныйТип</Catalog></ChildObjects></Catalog></MetaDataObject>";
    let parsed = metadata_parser::parse(xml, None, PropertiesMode::Summary).unwrap();
    let projection = ConfiguratorTree::build(&schema, &parsed, &BTreeMap::new()).unwrap();
    let forms = projection
        .nodes()
        .find(|node| {
            matches!(
                node.id,
                NodeId::Collection {
                    kind: CollectionKind::Metadata(MetadataKind::Form),
                    ..
                }
            )
        })
        .unwrap();
    let children = projection
        .children(&forms.id, TreeOptions::default())
        .unwrap();
    assert_eq!(
        children.iter().map(|node| &node.label).collect::<Vec<_>>(),
        vec![&TreeLabel::Name("Я".into()), &TreeLabel::Name("А".into())]
    );
    let unsupported = projection
        .nodes()
        .find(|node| {
            matches!(
                node.id,
                NodeId::Collection {
                    kind: CollectionKind::Unsupported,
                    ..
                }
            )
        })
        .unwrap();
    assert_eq!(unsupported.state, ChildrenState::Error);
    assert_eq!(unsupported.diagnostics.len(), 1);
    assert_eq!(unsupported.children.len(), 1);
}

/// Translation snapshots are independent of IDs and both embedded locales contain every key.
#[test]
fn configurator_labels_exist_in_both_locales() {
    let expected: BTreeMap<String, BTreeMap<String, String>> = serde_json::from_str(include_str!(
        "../fixtures/designer/expected-tree-labels.json"
    ))
    .unwrap();
    for locale in [Locale::RuRu, Locale::EnUs] {
        let localizer = Localizer::try_new(locale).unwrap();
        for (key, values) in &expected {
            assert_eq!(localizer.text(key), values[locale.as_str()]);
        }
        for kind in MetadataKind::ALL {
            assert!(expected.contains_key(&format!("tree-collection-{}", kind.as_str())));
        }
    }
}

/// External descriptors cannot acquire a configuration object's manager module by filename.
#[test]
fn configurator_external_roots_expose_only_applicable_modules() {
    for case in ["report", "processing"] {
        let (directory, source) = project(case);
        let ext = directory.0.join("src/Ext");
        fs::create_dir_all(&ext).unwrap();
        fs::write(
            ext.join("ManagerModule.bsl"),
            "// Test-only inapplicable module.\n",
        )
        .unwrap();
        let projection = tree(&source, source.root().id());
        let roles: Vec<_> = projection
            .nodes()
            .filter_map(|node| match node.id {
                NodeId::Module { role, .. } => Some(role),
                _ => None,
            })
            .collect();
        assert_eq!(roles, vec![ModuleRole::Object]);
    }
}

/// Unmodeled external-source descendants remain explicit errors, not a successful empty tree.
#[test]
fn configurator_external_source_unknown_children_are_visible() {
    let (_directory, source) = project("configuration");
    let schema = ConfiguratorSchema::for_source(&source);
    let parsed = metadata_parser::parse("<MetaDataObject xmlns='http://v8.1c.ru/8.3/MDClasses'><ExternalDataSource uuid='id'><Properties><Name>Источник</Name></Properties><ChildObjects><Table>Таблица</Table></ChildObjects></ExternalDataSource></MetaDataObject>", None, PropertiesMode::Summary).unwrap();
    let projection = ConfiguratorTree::build(&schema, &parsed, &BTreeMap::new()).unwrap();
    let children = projection
        .children(projection.root(), TreeOptions::default())
        .unwrap();
    assert_eq!(children.len(), 1);
    assert_eq!(children[0].state, ChildrenState::Error);
    assert_eq!(children[0].diagnostics.len(), 1);
}

#[derive(Deserialize)]
struct SchemaCase {
    tag: String,
    collections: Vec<String>,
    modules: Vec<String>,
    #[serde(default)]
    direct: bool,
}

/// Emit class fixtures in reverse collection order to distinguish schema order from XML order.
fn schema_case_xml(case: &SchemaCase, cases: &BTreeMap<String, SchemaCase>) -> String {
    let mut children = String::new();
    for key in case.collections.iter().rev() {
        use std::fmt::Write;
        let tag = &cases[key].tag;
        write!(
            &mut children,
            "<{tag} uuid='id'><Properties><Name>Дочерний</Name></Properties></{tag}>"
        )
        .unwrap();
    }
    let xml = format!(
        "<MetaDataObject xmlns='http://v8.1c.ru/8.3/MDClasses'><{0} uuid='id'><Properties><Name>Пример</Name></Properties><ChildObjects>{children}</ChildObjects></{0}></MetaDataObject>",
        case.tag
    );
    xml
}

/// Synthetic per-class descriptors exercise all collection schemas and module applicability.
#[test]
fn configurator_schema_matrix_checks_every_kind_and_does_not_use_xml_group_order() {
    let (_directory, source) = project("configuration");
    let schema = ConfiguratorSchema::for_source(&source);
    let cases: BTreeMap<String, SchemaCase> =
        serde_json::from_str(include_str!("../fixtures/designer/schema-cases.json")).unwrap();
    assert_eq!(cases.len() + 1, MetadataKind::ALL.len());
    let all_roles = vec![
        ModuleRole::Command,
        ModuleRole::ExternalConnection,
        ModuleRole::Session,
        ModuleRole::OrdinaryApplication,
        ModuleRole::ManagedApplication,
        ModuleRole::ValueManager,
        ModuleRole::RecordSet,
        ModuleRole::Manager,
        ModuleRole::Object,
        ModuleRole::Module,
    ];
    for (key, case) in &cases {
        let xml = schema_case_xml(case, &cases);
        let parsed = metadata_parser::parse(&xml, None, PropertiesMode::Summary).unwrap();
        assert!(parsed.diagnostics.is_empty(), "{key}");
        let owner = parsed.objects[0].metadata.id();
        let modules = parsed
            .objects
            .iter()
            .map(|object| {
                (
                    object.metadata.id().clone(),
                    ModuleAvailability::Loaded(all_roles.clone()),
                )
            })
            .collect();
        let projection = ConfiguratorTree::build(&schema, &parsed, &modules).unwrap();
        let collections: Vec<_> = projection
            .children(projection.root(), TreeOptions::default())
            .unwrap()
            .iter()
            .filter_map(|node| match node.id {
                NodeId::Collection {
                    kind: CollectionKind::Metadata(kind),
                    ..
                } => Some(kind.as_str()),
                _ => None,
            })
            .collect();
        if case.direct {
            assert!(collections.is_empty(), "{key}");
            let direct: Vec<_> = projection
                .children(projection.root(), TreeOptions::default())
                .unwrap()
                .iter()
                .filter_map(|node| match &node.id {
                    NodeId::Object(id) => Some(id.clone()),
                    _ => None,
                })
                .collect();
            assert_eq!(direct, parsed.objects[0].children, "{key}");
        } else {
            assert_eq!(collections, case.collections, "{key}");
        }
        let module_group = NodeId::Collection {
            owner: owner.clone(),
            kind: CollectionKind::Modules,
        };
        let roles: Vec<_> = projection
            .children(&module_group, TreeOptions::default())
            .unwrap_or_default()
            .iter()
            .filter_map(|node| match node.id {
                NodeId::Module { role, .. } => Some(role.as_str()),
                _ => None,
            })
            .collect();
        assert_eq!(roles, case.modules, "{key}");
        if !roles.is_empty() {
            assert_eq!(
                projection.node(projection.root()).unwrap().children.first(),
                Some(&module_group)
            );
            assert!(projection.node(&module_group).unwrap().expanded_by_default);
        }
        assert!(
            !projection
                .nodes()
                .any(|node| node.state == ChildrenState::Error),
            "{key}"
        );
    }
}

/// Inspect only a root descriptor and its BSL paths on the user's large read-only fixture.
#[test]
#[ignore = "manual read-only tree smoke; requires ESKA_DESIGNER_PROJECT"]
fn manual_large_configurator_tree() {
    let project = std::env::var_os("ESKA_DESIGNER_PROJECT").expect("ESKA_DESIGNER_PROJECT");
    let source = open_projects(Path::new(&project), &[], false)
        .unwrap()
        .remove(0);
    let schema = ConfiguratorSchema::for_source(&source);
    let parsed =
        metadata_parser::load(&source, source.root().id(), PropertiesMode::Summary).unwrap();
    assert!(parsed.diagnostics.is_empty());
    let modules = BTreeMap::from([(
        source.root().id().clone(),
        ModuleAvailability::resolve(&source, &schema, source.root().id(), source.root().kind())
            .unwrap(),
    )]);
    let started = std::time::Instant::now();
    let projection = ConfiguratorTree::build(&schema, &parsed, &modules).unwrap();
    let elapsed = started.elapsed();
    assert_eq!(
        projection
            .nodes()
            .filter(|node| matches!(node.id, NodeId::Object(_)))
            .count(),
        parsed.objects.len() + parsed.references.len()
    );
    assert_eq!(
        projection
            .nodes()
            .filter(|node| node.state == ChildrenState::Unloaded)
            .count(),
        parsed.references.len()
    );
    assert!(
        !projection
            .nodes()
            .any(|node| node.state == ChildrenState::Error)
    );
    let mut sampled = std::collections::BTreeSet::new();
    for reference in &parsed.references {
        if sampled.insert(reference.kind) {
            let branch = tree(&source, &reference.id);
            assert!(
                !branch
                    .nodes()
                    .any(|node| node.state == ChildrenState::Error),
                "{}",
                reference.id
            );
        }
    }
    println!(
        "tree_build_ms={} references={} nodes={} sampled_kinds={}",
        elapsed.as_millis(),
        parsed.references.len(),
        projection.nodes().count(),
        sampled.len()
    );
}

/// External processing collections differ from the same kind inside a configuration.
#[test]
fn configurator_processing_schema_is_scoped_to_the_manifest_root() {
    for case in ["processing", "configuration"] {
        let (_directory, source) = project(case);
        let schema = ConfiguratorSchema::for_source(&source);
        let owner = if case == "processing" {
            source.root().id().clone()
        } else {
            MetadataObject::new(
                MetadataKind::DataProcessor,
                "Обработка".into(),
                "id".into(),
                None,
            )
            .unwrap()
            .id()
            .clone()
        };
        let actual = schema.owner_collections(&owner, MetadataKind::DataProcessor);
        let expected = if case == "processing" {
            vec![
                MetadataKind::Attribute,
                MetadataKind::TabularSection,
                MetadataKind::Form,
                MetadataKind::Template,
            ]
        } else {
            vec![
                MetadataKind::Attribute,
                MetadataKind::TabularSection,
                MetadataKind::Form,
                MetadataKind::Command,
                MetadataKind::Template,
            ]
        };
        assert_eq!(actual, expected);
    }
}

/// Exercise the standard Designer layout for the newly registered WebSocket client kind.
#[test]
fn configurator_websocket_client_resolves_descriptor_and_existing_module() {
    let (directory, source) = project("configuration");
    let base = directory.0.join("src/WebSocketClients");
    fs::create_dir_all(base.join("Соединение/Ext")).unwrap();
    fs::write(base.join("Соединение.xml"),
        "<MetaDataObject xmlns='http://v8.1c.ru/8.3/MDClasses'><WebSocketClient uuid='id'><Properties><Name>Соединение</Name></Properties></WebSocketClient></MetaDataObject>").unwrap();
    fs::write(base.join("Соединение/Ext/Module.bsl"), "// Test module.\n").unwrap();
    let owner = MetadataObject::new(
        MetadataKind::WebSocketClient,
        "Соединение".into(),
        "id".into(),
        None,
    )
    .unwrap();
    let projection = tree(&source, owner.id());
    let group = projection
        .children(projection.root(), TreeOptions::default())
        .unwrap();
    assert_eq!(group.len(), 1);
    assert!(group[0].expanded_by_default);
    assert_eq!(
        group[0].children,
        vec![NodeId::Module {
            owner: owner.id().clone(),
            role: ModuleRole::Module
        }]
    );
    fs::remove_file(base.join("Соединение/Ext/Module.bsl")).unwrap();
    fs::write(base.join("Соединение/Ext/Module.bin"), [0]).unwrap();
    let projection = tree(&source, owner.id());
    assert!(
        projection
            .children(projection.root(), TreeOptions::default())
            .unwrap()
            .is_empty()
    );
}
