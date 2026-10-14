//! Inline native composition changes retain independent owner edits under the original support pin.

use std::{fs, path::Path};

use super::*;
use crate::{
    project_core::source_format::{
        AcceptedBase, Child, ImportAncestor, LayoutPublication, MetadataNode, Operation,
        PermissionReason, Property, PropertyName, PropertyValue, Section, capture_import_ancestor,
        enum_test_support, prepare_protected, prepare_source_import,
    },
    test_support::TestDir,
};

const OWNER: &str = "Enums/Status/object.yaml";

/// A readable native owner permits creation without changing the approved reverse declaration map.
struct Fixture {
    test: TestDir,
    source: std::path::PathBuf,
    incoming: std::path::PathBuf,
    base: AcceptedBase,
    ancestor: ImportAncestor,
}

impl Fixture {
    /// Freeze remains external to both captured ancestors and every incoming descriptor.
    fn new(frozen: bool) -> Self {
        let test = TestDir::new();
        let frozen = if frozen {
            vec![enum_test_support::uuid(2)]
        } else {
            Vec::new()
        };
        let (source, base) = enum_test_support::fixture(&test, true, &frozen);
        let ancestor = capture(&source, &test.0.join("ancestor"));
        let incoming = test.0.join("incoming-source");
        ancestor.restore(&incoming).unwrap();
        Self {
            test,
            source,
            incoming,
            base,
            ancestor,
        }
    }

    /// Publication verifies a captured incoming store independently of its original source folder.
    fn plan(&self) -> SourceImportPlan {
        let incoming = capture(&self.incoming, &self.test.0.join("incoming"));
        prepare_source_import(&self.source, &self.ancestor, &incoming, &self.base).unwrap()
    }
}

/// Typed source encoding keeps the readable envelope rather than writing its legacy internal model.
fn edit(root: &Path, change: impl FnOnce(&mut ObjectSource)) {
    let path = root.join(OWNER);
    let mut owner = ObjectSource::from_yaml(&fs::read_to_string(&path).unwrap()).unwrap();
    change(&mut owner);
    fs::write(path, owner.to_yaml().unwrap()).unwrap();
}

/// The common ancestor and incoming snapshot never carry accepted support or freeze rules.
fn capture(source: &Path, store: &Path) -> ImportAncestor {
    let reference = capture_import_ancestor(source, store).unwrap();
    ImportAncestor::open(store, &reference).unwrap()
}

/// Properties are changed by their native typed identity, not a guessed serialized array position.
fn comment(node: &mut MetadataNode, text: &str) {
    for section in &mut node.sections {
        if let Section::Properties(properties) = section {
            for property in properties {
                if let Property::Typed {
                    name: PropertyName::Comment,
                    value,
                } = property
                {
                    *value = PropertyValue::Text(text.into());
                }
            }
        }
    }
}

/// Independent incoming inline additions/deletions/renames and a local owner value share one file.
#[test]
fn inline_changes_merge_with_local_properties_and_keep_working_trivia() {
    for operation in ["add", "delete", "rename"] {
        let fixture = Fixture::new(false);
        edit(&fixture.source, |owner| {
            comment(&mut owner.document.descriptor.object, "Локальная заметка");
        });
        let local = fs::read_to_string(fixture.source.join(OWNER)).unwrap();
        let local = format!(
            "# Keep local owner annotation\r\n{}",
            local.replace('\n', "\r\n")
        );
        fs::write(fixture.source.join(OWNER), &local).unwrap();
        edit(&fixture.incoming, |owner| {
            let children = enum_test_support::children(owner);
            match operation {
                "add" => children.push(enum_test_support::value(100, "Third")),
                "delete" => {
                    children.remove(1);
                }
                _ => {
                    let Child::Inline { object } = &mut children[1] else {
                        unreachable!()
                    };
                    for section in &mut object.sections {
                        if let Section::Properties(properties) = section {
                            for property in properties {
                                if let Property::Typed {
                                    name: PropertyName::Name,
                                    value,
                                } = property
                                {
                                    *value = PropertyValue::Text("Renamed".into());
                                }
                            }
                        }
                    }
                }
            }
        });
        let base = files::Inventory::read(fixture.base.storage_root()).unwrap();
        let plan = fixture.plan();
        assert!(
            plan.conflicts().is_empty(),
            "{operation}: {:?}",
            plan.conflicts()
        );
        let output = fixture.test.0.join("output");
        assert_eq!(
            plan.publish_source(&output).unwrap(),
            LayoutPublication::Created
        );
        assert_eq!(
            plan.publish_source(&output).unwrap(),
            LayoutPublication::Unchanged
        );
        let merged = fs::read_to_string(output.join(OWNER)).unwrap();
        assert!(merged.starts_with("# Keep local owner annotation\r\n"));
        assert!(!merged.replace("\r\n", "").contains('\n'));
        assert!(merged.contains("Локальная заметка"));
        let prepared = prepare_protected(
            &output,
            &fixture.base,
            &fixture.test.0.join("generations"),
            true,
        )
        .unwrap();
        let xml = fs::read_to_string(prepared.xml().join("Enums/Status.xml")).unwrap();
        assert!(xml.contains("Локальная заметка"));
        assert_eq!(xml.contains("Third"), operation == "add");
        assert_eq!(xml.contains("Renamed"), operation == "rename");
        assert_eq!(xml.contains("Second"), operation == "add");
        assert_eq!(
            fs::read_to_string(fixture.source.join(OWNER)).unwrap(),
            local
        );
        assert_eq!(
            files::Inventory::read(fixture.base.storage_root()).unwrap(),
            base
        );
        assert_eq!(plan.approved_reference(), fixture.base.reference());
    }
}

/// A local edit inside a deleted or renamed collection stays a conflict with a precise coordinate.
#[test]
fn overlapping_values_and_deletion_against_child_edits_are_not_selected() {
    for deleted in [false, true] {
        let fixture = Fixture::new(false);
        edit(&fixture.source, |owner| {
            if deleted {
                let Child::Inline { object } = &mut enum_test_support::children(owner)[1] else {
                    unreachable!()
                };
                comment(object, "local child");
            } else {
                comment(&mut owner.document.descriptor.object, "local owner");
            }
        });
        edit(&fixture.incoming, |owner| {
            if deleted {
                enum_test_support::children(owner).remove(1);
            } else {
                comment(&mut owner.document.descriptor.object, "incoming owner");
            }
        });
        let plan = fixture.plan();
        assert_eq!(plan.conflicts().len(), 1);
        let conflict = &plan.conflicts()[0];
        assert_eq!(conflict.kind, SourceImportConflictKind::BothModified);
        assert_eq!(
            conflict.coordinate,
            if deleted {
                vec!["document", "descriptor", "object", "children"]
            } else {
                vec!["document", "descriptor", "object", "properties", "Comment"]
            }
        );
        assert_eq!(conflict.uuid, Some(enum_test_support::uuid(2)));
        let json = serde_json::to_value(conflict).unwrap();
        assert_eq!(
            json["coordinate"].as_array().unwrap().len(),
            conflict.coordinate.len()
        );
        let output = fixture.test.0.join("output");
        assert_eq!(
            plan.publish_source(&output).unwrap_err().code,
            "source_import_conflicts"
        );
        assert!(!output.exists());
    }
}

/// A syntactically valid merged descriptor cannot create children through a frozen parent.
#[test]
fn merged_inputs_still_obey_the_original_frozen_parent() {
    let fixture = Fixture::new(true);
    edit(&fixture.source, |owner| {
        comment(&mut owner.document.descriptor.object, "local");
    });
    edit(&fixture.incoming, |owner| {
        enum_test_support::children(owner).push(enum_test_support::value(100, "Third"));
    });
    let plan = fixture.plan();
    assert!(!plan.conflicts().is_empty());
    assert!(
        plan.conflicts()
            .iter()
            .all(|conflict| conflict.kind == SourceImportConflictKind::PermissionDenied)
    );
    assert!(plan.conflicts().iter().any(|conflict| {
        conflict.exclusion.as_ref().is_some_and(|excluded| {
            excluded.operation == Operation::Create
                && excluded.reason == PermissionReason::EskaFrozen
        })
    }));
    assert!(
        !fixture
            .base
            .permission(&enum_test_support::uuid(100), Operation::Module)
            .allowed
    );
    assert!(plan.publish_source(&fixture.test.0.join("output")).is_err());
}

/// A replacement subtree cannot silently discard authored annotations or import unrelated new trivia.
#[test]
fn refuses_comment_loss_and_incoming_trivia_outside_selected_values() {
    for incoming_comment in [false, true] {
        let fixture = Fixture::new(false);
        edit(&fixture.source, |owner| {
            comment(&mut owner.document.descriptor.object, "local");
        });
        edit(&fixture.incoming, |owner| {
            enum_test_support::children(owner).push(enum_test_support::value(100, "Third"));
        });
        let path = if incoming_comment {
            fixture.incoming.join(OWNER)
        } else {
            fixture.source.join(OWNER)
        };
        let text = fs::read_to_string(&path).unwrap();
        let text = if incoming_comment {
            format!("# Incoming annotation must survive\n{text}")
        } else {
            text.replace(
                "\"children\":\n",
                "\"children\": # Child annotation must survive\n",
            )
            .replace(
                "\"kind\": \"inline\"",
                "\"kind\": \"inline\" # Keep child annotation",
            )
        };
        fs::write(path, text).unwrap();
        let plan = fixture.plan();
        assert_eq!(plan.conflicts().len(), 1);
        assert_eq!(
            plan.conflicts()[0].kind,
            SourceImportConflictKind::BothModified
        );
        assert!(plan.publish_source(&fixture.test.0.join("output")).is_err());
    }
}

/// Generated merged bytes remain bound to every original source, ancestry and accepted snapshot.
#[test]
fn merged_candidate_cannot_publish_after_an_input_change() {
    let fixture = Fixture::new(false);
    edit(&fixture.source, |owner| {
        comment(&mut owner.document.descriptor.object, "local");
    });
    edit(&fixture.incoming, |owner| {
        enum_test_support::children(owner).push(enum_test_support::value(100, "Third"));
    });
    let plan = fixture.plan();
    assert!(plan.conflicts().is_empty());
    fs::write(fixture.source.join(OWNER), "changed after preview").unwrap();
    let output = fixture.test.0.join("output");
    assert_eq!(
        plan.publish_source(&output).unwrap_err().code,
        "source_input_changed"
    );
    assert!(!output.exists());
}

/// An empty inline collection becomes a block without losing local comments or CRLF transport.
#[test]
fn empty_children_merge_into_a_block_with_local_owner_values() {
    let fixture = Fixture::new(false);
    for source in [&fixture.source, &fixture.incoming] {
        edit(source, |owner| enum_test_support::children(owner).clear());
    }
    let ancestor = capture(&fixture.source, &fixture.test.0.join("empty-ancestor"));
    let base_path = fixture.test.0.join("empty-accepted");
    let pin = crate::project_core::source_format::accept_source(&fixture.source, &base_path, &[])
        .unwrap();
    let base = AcceptedBase::open(&base_path, &pin).unwrap();
    edit(&fixture.source, |owner| {
        comment(&mut owner.document.descriptor.object, "local");
    });
    let path = fixture.source.join(OWNER);
    let text = format!(
        "# Keep CRLF\r\n{}",
        fs::read_to_string(&path).unwrap().replace('\n', "\r\n")
    );
    fs::write(&path, &text).unwrap();
    edit(&fixture.incoming, |owner| {
        enum_test_support::children(owner).push(enum_test_support::value(100, "Third"));
    });
    let incoming = capture(&fixture.incoming, &fixture.test.0.join("empty-incoming"));
    let plan = prepare_source_import(&fixture.source, &ancestor, &incoming, &base).unwrap();
    assert!(plan.conflicts().is_empty(), "{:?}", plan.conflicts());
    let output = fixture.test.0.join("output");
    plan.publish_source(&output).unwrap();
    let merged = fs::read_to_string(output.join(OWNER)).unwrap();
    assert!(merged.contains("Third"));
    assert!(merged.contains("local"));
    assert!(merged.starts_with("# Keep CRLF\r\n"));
    assert!(!merged.replace("\r\n", "").contains('\n'));
    prepare_protected(&output, &base, &fixture.test.0.join("generations"), true).unwrap();
}

/// Empty-state maps and literal values keep their neighboring localized values and authored styles.
#[test]
fn scalar_shape_changes_merge_without_rewriting_local_property_bytes() {
    for flow in [false, true] {
        let fixture = Fixture::new(false);
        edit(&fixture.source, |owner| {
            for section in &mut owner.document.descriptor.object.sections {
                if let Section::Properties(properties) = section {
                    for property in properties {
                        if let Property::Typed {
                            name: PropertyName::Synonym,
                            value,
                        } = property
                        {
                            *value = PropertyValue::Localized(vec![
                                crate::project_core::source_format::LocalizedText {
                                    language: "ru".into(),
                                    text: "local synonym".into(),
                                },
                            ]);
                        }
                    }
                }
            }
        });
        edit(&fixture.incoming, |owner| {
            comment(&mut owner.document.descriptor.object, "incoming comment");
        });
        if flow {
            for root in [&fixture.source, &fixture.incoming] {
                let path = root.join(OWNER);
                let value = yaml::decode(&fs::read_to_string(&path).unwrap()).unwrap();
                fs::write(path, serde_json::to_string(&value).unwrap()).unwrap();
            }
        }
        let path = fixture.source.join(OWNER);
        let local = format!(
            "# Keep local property syntax\r\n{}",
            fs::read_to_string(&path).unwrap().replace('\n', "\r\n")
        );
        fs::write(&path, &local).unwrap();
        let plan = fixture.plan();
        assert!(
            plan.conflicts().is_empty(),
            "flow={flow}: {:?}",
            plan.conflicts()
        );
        let output = fixture.test.0.join("output");
        plan.publish_source(&output).unwrap();
        let merged = fs::read_to_string(output.join(OWNER)).unwrap();
        assert!(merged.contains("local synonym"));
        assert!(merged.contains("incoming comment"));
        assert!(merged.starts_with("# Keep local property syntax\r\n"));
        assert!(!merged.replace("\r\n", "").contains('\n'));
        prepare_protected(
            &output,
            &fixture.base,
            &fixture.test.0.join("generations"),
            true,
        )
        .unwrap();
    }
}
