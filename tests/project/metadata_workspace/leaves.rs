use super::{fixture, object};
use crate::support::TestDir;
use eska::project::{
    configurator::{ChildrenState, TreeOptions},
    metadata_model::{MetadataKind, NodeId, ProjectScope},
    metadata_workspace::MetadataWorkspace,
};
use std::{fmt::Write, fs, path::PathBuf};

const LEAVES: &[&str] = &[
    "SessionParameter",
    "Role",
    "CommonAttribute",
    "EventSubscription",
    "ScheduledJob",
    "FunctionalOption",
    "FunctionalOptionsParameter",
    "DefinedType",
    "CommandGroup",
    "CommonTemplate",
    "CommonPicture",
    "XDTOPackage",
    "WSReference",
    "StyleItem",
    "Style",
    "Language",
    "Constant",
    "DocumentNumerator",
];

/// Add references only: leaf classification must not need any of their XML descriptors.
fn leaf_fixture() -> TestDir {
    let directory = TestDir::new();
    fixture(&directory.0, "configuration");
    let path = directory.0.join("src/Configuration.xml");
    let mut references = String::new();
    for tag in LEAVES {
        write!(references, "<{tag}>Leaf</{tag}>").unwrap();
    }
    let xml = fs::read_to_string(&path)
        .unwrap()
        .replace("</ChildObjects>", &format!("{references}</ChildObjects>"));
    fs::write(path, xml).unwrap();
    directory
}

/// Unparsed leaves are already empty, while real branches and their containing groups survive.
#[test]
fn schema_leaves_have_no_expander_without_loading_descriptors() {
    let directory = leaf_fixture();
    let mut workspace = MetadataWorkspace::open(&directory.0, &[], false).unwrap();
    let session = workspace.project_mut(&ProjectScope::Standalone).unwrap();
    let parses = session.cache_stats().parses;
    for tag in LEAVES {
        let kind = MetadataKind::from_xml_tag(tag).unwrap();
        let id = NodeId::Object(object(kind, "Leaf", None));
        let node = session.node(&id).unwrap();
        assert_eq!(node.state, ChildrenState::Empty, "{tag}");
        assert!(!node.can_expand(), "{tag}");
        let parent = node.parent.clone().unwrap();
        assert_eq!(
            session.node(&parent).unwrap().state,
            ChildrenState::NonEmpty
        );
        assert!(
            session
                .children(&parent, TreeOptions::default())
                .unwrap()
                .iter()
                .any(|child| child.id == id)
        );
        session.refresh(&id).unwrap();
        assert_eq!(
            session.node(&id).unwrap().state,
            ChildrenState::Empty,
            "refresh {tag}"
        );
    }
    assert_eq!(session.cache_stats().parses, parses);
    let catalog = NodeId::Object(object(MetadataKind::Catalog, "Контрагенты", None));
    assert_eq!(
        session.node(&catalog).unwrap().state,
        ChildrenState::Unloaded
    );
}

/// BSL additions/removals change the expander even before the constant was ever opened.
#[test]
fn constant_expander_tracks_both_module_roles_without_eager_xml_reads() {
    let directory = leaf_fixture();
    let xml = directory.0.join("src/Constants/Leaf.xml");
    fs::create_dir_all(xml.parent().unwrap()).unwrap();
    fs::write(&xml, "<MetaDataObject xmlns='http://v8.1c.ru/8.3/MDClasses'><Constant uuid='11111111-1111-1111-1111-111111111111'><Properties><Name>Leaf</Name></Properties></Constant></MetaDataObject>").unwrap();
    let mut workspace = MetadataWorkspace::open(&directory.0, &[], false).unwrap();
    let session = workspace.project_mut(&ProjectScope::Standalone).unwrap();
    let id = NodeId::Object(object(MetadataKind::Constant, "Leaf", None));
    let parses = session.cache_stats().parses;
    for role in ["ManagerModule", "ValueManagerModule"] {
        let path = PathBuf::from(format!("Constants/Leaf/Ext/{role}.bsl"));
        let full = directory.0.join("src").join(&path);
        fs::create_dir_all(full.parent().unwrap()).unwrap();
        fs::write(&full, "// module").unwrap();
        session.changed_paths(std::slice::from_ref(&path)).unwrap();
        assert_eq!(session.node(&id).unwrap().state, ChildrenState::Unloaded);
        assert_eq!(session.cache_stats().parses, parses);
        fs::remove_file(full).unwrap();
        session.changed_paths(&[path]).unwrap();
        assert_eq!(session.node(&id).unwrap().state, ChildrenState::Empty);
        assert_eq!(session.cache_stats().parses, parses);
    }
    let path = directory
        .0
        .join("src/Constants/Leaf/Ext/ValueManagerModule.bsl");
    fs::write(path, "// module").unwrap();
    session.refresh(&id).unwrap();
    assert!(
        !session
            .children(&id, TreeOptions::default())
            .unwrap()
            .is_empty()
    );
    assert_eq!(session.cache_stats().parses, parses + 1);
}
