//! External body ownership follows its descriptor FileId even when physical and logical names differ.

use super::*;
use crate::metadata::MdoEntry;

const STEM: &str = "processing-native.project";
const NAME: &str = "ЛогическоеExternalOwner";

/// A registered descriptor identity selects its own current payload and cannot borrow a stem namesake.
#[test]
fn external_physical_stems_select_actual_declared_names_and_preserve_slot_guards() {
    for kind in
        [bsl_metadata::ExternalObjectKind::DataProcessor, bsl_metadata::ExternalObjectKind::Report]
    {
        let mut fixture = Fixture::new();
        let roots = fixture.roots(&[("external", RootKind::External(kind), &[])]);
        let root = &roots[0];
        fixture.file(root, 0, &format!("{STEM}.XML"), None);
        fixture.file(root, 1, &format!("{STEM}/Ext/ObjectModule.bsl"), Some(""));
        fixture.file(root, 2, "Decoy.xml", None);
        let current = object(kind.mdo_type(), NAME, "Current");
        let own = fixture.slot(Some(CanonicalMetadata::Object(Arc::clone(&current))));
        let decoy =
            fixture.slot(Some(CanonicalMetadata::Object(object(kind.mdo_type(), STEM, "Decoy"))));
        fixture.listing(
            root,
            MetadataListingData {
                entries: vec![entry(kind, NAME, 0), entry(kind, STEM, 2)],
                ..Default::default()
            },
            &[(0, own), (2, decoy)],
        );
        assert_eq!(fixture.module(1).mdo.as_ref(), Some(&current));
        update(&mut fixture, own, None);
        assert!(fixture.module(1).mdo.is_none());
        for value in [
            object(MdoType::DataProcessor, NAME, "WrongKind"),
            object(kind.mdo_type(), STEM, "WrongName"),
        ] {
            update(&mut fixture, own, Some(CanonicalMetadata::Object(value)));
            assert!(fixture.module(1).mdo.is_none());
        }
        update(&mut fixture, own, Some(CanonicalMetadata::Object(Arc::clone(&current))));
        assert_eq!(fixture.module(1).mdo.as_ref(), Some(&current));
        assert_eq!(fixture.database.salsa_family_churn("load_configuration").unwrap().0, 0);
        assert_eq!(fixture.database.salsa_family_churn("parse_mdo_query").unwrap().0, 0);
    }
}

/// Missing, outside-root and ambiguous descriptor enrollment never authorize a folder-name fallback.
#[test]
fn external_bodies_refuse_missing_and_ambiguous_own_descriptor_identities() {
    for kind in
        [bsl_metadata::ExternalObjectKind::DataProcessor, bsl_metadata::ExternalObjectKind::Report]
    {
        for case in ["missing", "unregistered", "outside", "duplicate-main", "namesake-main"] {
            let mut fixture = Fixture::new();
            let roots = fixture.roots(&[
                ("external", RootKind::External(kind), &[]),
                ("other", RootKind::External(kind), &[]),
            ]);
            let root = &roots[0];
            fixture.file(root, 1, &format!("{STEM}/Ext/ObjectModule.bsl"), Some(""));
            if case != "unregistered" {
                let descriptor_root = if case == "outside" { &roots[1] } else { root };
                fixture.file(descriptor_root, 0, &format!("{STEM}.xml"), None);
            }
            fixture.file(root, 2, "Decoy.xml", None);
            let current =
                fixture.slot(Some(CanonicalMetadata::Object(object(kind.mdo_type(), NAME, "Own"))));
            let decoy = fixture.slot(Some(CanonicalMetadata::Object(object(
                kind.mdo_type(),
                STEM,
                "Decoy",
            ))));
            let mut entries = vec![entry(kind, STEM, 2)];
            if case != "missing" {
                entries.insert(0, entry(kind, NAME, 0));
            }
            if case == "duplicate-main" {
                entries.push(entry(kind, "OtherName", 0));
            }
            if case == "namesake-main" {
                entries.push(entry(kind, NAME, 2));
            }
            fixture.listing(
                root,
                MetadataListingData { entries, ..Default::default() },
                &[(0, current), (2, decoy)],
            );
            assert!(fixture.module(1).mdo.is_none(), "{case}");
            assert_eq!(
                fixture.database.salsa_family_churn("load_configuration").unwrap().0,
                0,
                "{case}"
            );
            assert_eq!(
                fixture.database.salsa_family_churn("parse_mdo_query").unwrap().0,
                0,
                "{case}"
            );
        }
    }
}

/// The kind and logical name belong to the declared main identity, independently of its physical basename.
fn entry(kind: bsl_metadata::ExternalObjectKind, name: &str, main: u32) -> MdoEntry {
    MdoEntry { kind: kind.mdo_type(), name: name.into(), main: FileId(main), predefined: None }
}
