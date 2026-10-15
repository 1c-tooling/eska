//! Owner descriptors anchor current canonical values without XML text or a whole Configuration.

use std::{path::PathBuf, sync::Arc};

use base_db::{SourceDatabase, SourceRoot, SourceRootId};
use bsl_metadata::{
    AttributeType, DefinedType, MdoType, MetadataObject, MetadataResolver, QueryMetadataResolver,
    Register,
};
use salsa::Setter;
use vfs::{FileId, FileSet, VfsPath};

use super::FileQueryResolver;
use crate::{
    metadata::{
        CanonicalMetadata, CanonicalMetadataInput, DefinedTypeEntry, MdoEntry, MetadataListingData,
        RootKind, WorkspaceConfigsSnapshot,
    },
    RootDatabaseImpl,
};

/// Each real source-root input holds distinct descriptor IDs even when their relative paths agree.
fn database() -> (RootDatabaseImpl, Vec<CanonicalMetadataInput>) {
    let mut db = RootDatabaseImpl::new();
    let roots =
        ["base", "a", "b", "sibling"].map(|name| PathBuf::from("/query-context").join(name));
    let paths = roots
        .iter()
        .enumerate()
        .map(|(index, path)| ((index != 0).then(|| format!("extension-{index}")), path.clone()))
        .collect::<Vec<_>>();
    let mut provided = Vec::new();
    for (index, root) in roots.iter().enumerate() {
        let first = (index * 3) as u32;
        let mut files = FileSet::default();
        for (file, path) in [
            (FileId(first), "Catalogs/Items.xml"),
            (FileId(first + 1), "InformationRegisters/Rates.xml"),
            (FileId(first + 2), "DefinedTypes/Money.xml"),
        ] {
            files.insert(file, VfsPath::from(root.join(path)));
            db.set_file_source_root(file, SourceRootId(index as u32));
            db.set_file_unreadable(file);
        }
        db.set_source_root(SourceRootId(index as u32), SourceRoot::new_local(files));
        let mut listing = MetadataListingData::default();
        let mut slots = crate::metadata::CanonicalMetadataInputs::default();
        if index != 2 {
            listing.entries = vec![
                MdoEntry {
                    kind: MdoType::Catalog,
                    name: "Items".into(),
                    main: FileId(first),
                    predefined: None,
                },
                MdoEntry {
                    kind: MdoType::InformationRegister,
                    name: "Rates".into(),
                    main: FileId(first + 1),
                    predefined: None,
                },
            ];
            listing.defined_types =
                vec![DefinedTypeEntry { name: "Money".into(), main: FileId(first + 2) }];
            let values = [
                CanonicalMetadata::Object(Arc::new(MetadataObject::new(MdoType::Catalog, "Items"))),
                CanonicalMetadata::Register(Arc::new(
                    Register::builder()
                        .name("Rates")
                        .mdo_type(MdoType::InformationRegister)
                        .recorder_subordinate(index == 1)
                        .build(),
                )),
                CanonicalMetadata::DefinedType(Arc::new(
                    DefinedType::builder()
                        .uuid("70000000-0000-4000-8000-000000000001".parse().unwrap())
                        .name("Money")
                        .underlying_type(if index == 1 {
                            AttributeType::Boolean
                        } else {
                            AttributeType::String { length: Some(40) }
                        })
                        .build(),
                )),
            ];
            for (offset, value) in values.into_iter().enumerate() {
                let input =
                    CanonicalMetadataInput::new(&db, Some(Arc::new(value)), "current".into());
                slots.insert(FileId(first + offset as u32), input);
                if index == 1 {
                    provided.push(input);
                }
            }
        }
        db.set_metadata_listing(&root.to_string_lossy(), listing);
        db.metadata_listing(&root.to_string_lossy())
            .unwrap()
            .set_canonical_metadata(&mut db)
            .to(Arc::new(slots));
    }
    db.set_workspace_configs_snapshot(WorkspaceConfigsSnapshot {
        kinds: RootKind::from_labels(&paths),
        paths,
        canonical_paths: roots.to_vec(),
        closures: vec![vec![], vec![], vec![1], vec![]],
        topological_order: vec![0, 1, 2, 3],
        fingerprint: None,
        source_exclusions: Default::default(),
    });
    (db, provided)
}

/// Selected B inherits A while base and an unrelated sibling retain their own current values.
#[test]
fn file_query_resolver_uses_only_the_anchors_declared_visibility() {
    let (db, _) = database();
    let base = FileQueryResolver::new(&db, FileId(0));
    let selected = FileQueryResolver::new(&db, FileId(6));
    let sibling = FileQueryResolver::new(&db, FileId(9));
    assert_eq!(selected.resolve_defined_type("MONEY"), Some(AttributeType::Boolean));
    assert_eq!(
        base.resolve_defined_type("Money"),
        Some(AttributeType::String { length: Some(40) })
    );
    assert_eq!(sibling.resolve_defined_type("Money"), base.resolve_defined_type("Money"));
    assert!(selected.resolve_register(MdoType::InformationRegister, "Rates").is_some());
    assert!(!sibling
        .resolve_register(MdoType::InformationRegister, "Rates")
        .unwrap()
        .is_recorder_subordinate());
    assert!(selected.resolve_metadata_object(MdoType::Catalog, "Items").is_some());
    assert!(selected.resolve_metadata_object(MdoType::Catalog, "Missing").is_none());
    for raw in 0..12 {
        assert!(db.file_is_unread(FileId(raw)), "no XML fallback may read the descriptor");
    }
}

/// Pending or incompatible current A declarations cannot return the older matching base value.
#[test]
fn file_query_resolver_preserves_current_slot_failures_and_repair() {
    let (mut db, provided) = database();
    let saved = provided.iter().map(|input| input.value(&db).clone()).collect::<Vec<_>>();
    for input in &provided {
        input.set_value(&mut db).to(None);
    }
    let selected = FileQueryResolver::new(&db, FileId(6));
    assert!(selected.resolve_metadata_object(MdoType::Catalog, "Items").is_none());
    assert!(selected.resolve_register(MdoType::InformationRegister, "Rates").is_none());
    assert!(selected.resolve_defined_type("Money").is_none());
    assert!(FileQueryResolver::new(&db, FileId(0)).resolve_defined_type("Money").is_some());
    for input in &provided {
        input.set_value(&mut db).to(Some(Arc::new(CanonicalMetadata::Object(Arc::new(
            MetadataObject::new(MdoType::Catalog, "Wrong"),
        )))));
    }
    assert!(FileQueryResolver::new(&db, FileId(6))
        .resolve_metadata_object(MdoType::Catalog, "Items")
        .is_none());
    assert!(FileQueryResolver::new(&db, FileId(6))
        .resolve_register(MdoType::InformationRegister, "Rates")
        .is_none());
    assert!(FileQueryResolver::new(&db, FileId(6)).resolve_defined_type("Money").is_none());
    for (input, value) in provided.iter().zip(saved) {
        input.set_value(&mut db).to(value);
    }
    assert_eq!(
        FileQueryResolver::new(&db, FileId(6)).resolve_defined_type("Money"),
        Some(AttributeType::Boolean)
    );
}
