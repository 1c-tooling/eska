use super::*;
use crate::project_core::source_format::{
    MetadataReference, MetadataReferenceLayout, MetadataReferenceList,
};

/// Appended scalar references keep transport and annotations in both actual wire value shapes.
#[test]
fn list_membership_edits_keep_a_valid_value_subtree() {
    let old = PropertyValue::ReferenceList(Box::new(MetadataReferenceList {
        items: vec![
            MetadataReference::parse("Catalog.Sample").unwrap(),
            MetadataReference::parse("Catalog.Other").unwrap(),
        ],
        native: MetadataReferenceLayout {
            item_prefix: "xr".into(),
            xsi_prefix: "xsi".into(),
            type_prefix: "xr".into(),
        },
    }));
    let mut new = old.clone();
    let PropertyValue::ReferenceList(value) = &mut new else {
        panic!("list")
    };
    value
        .items
        .push(MetadataReference::parse("Catalog.Other").unwrap());
    for readable in [false, true] {
        let value = if readable {
            super::super::super::readable::value_to_wire(&old).unwrap()
        } else {
            serde_json::to_value(&old).unwrap()
        };
        let input = yaml::encode(&serde_json::json!({"selected":value,"unrelated":"keep"}))
            .unwrap()
            .replace("\"Catalog.Sample\"", "'Catalog.Sample' # first")
            .replace("\"Catalog.Other\"", "'Catalog.Other' # second")
            .replace('\n', "\r\n");
        let updated = if readable {
            replace_direct(&input, &[Step::Key("selected")], &old, &new).unwrap()
        } else {
            replace(&input, &[Step::Key("selected")], &old, &new).unwrap()
        };
        let expected = if readable {
            super::super::super::readable::value_to_wire(&new).unwrap()
        } else {
            serde_json::to_value(&new).unwrap()
        };
        assert_eq!(
            yaml::decode(&updated).unwrap()["selected"],
            expected,
            "{updated}"
        );
        assert!(updated.contains("# first") && updated.contains("# second"));
    }
}

/// Removing an annotated member is refused; an ordinary trailing member has an exact line splice.
#[test]
fn membership_removal_preserves_neighbors_and_refuses_ambiguous_annotations() {
    use super::reference_value::membership;
    let old = [
        MetadataReference::parse("Role.First").unwrap(),
        MetadataReference::parse("Role.Second").unwrap(),
    ];
    let new = &old[..1];
    for eol in ["\n", "\r\n"] {
        let input = format!(
            "selected:{eol}  - 'Role.First' # first{eol}  - 'Role.Second'{eol}other: keep # neighbor{eol}"
        );
        assert_eq!(
            membership(&input, &[Step::Key("selected")], &old, new).unwrap(),
            input.replace(&format!("  - 'Role.Second'{eol}"), "")
        );
        let annotated = input.replace("'Role.Second'", "'Role.Second' # retained");
        assert!(membership(&annotated, &[Step::Key("selected")], &old, new).is_err());
    }
    assert!(
        membership(
            "selected: ['Role.First', 'Role.Second']\n",
            &[Step::Key("selected")],
            &old,
            new
        )
        .is_err()
    );
}
