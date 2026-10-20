//! Designer XML descriptor parsing and structural property comparisons.

use super::{
    SemanticEvent, SemanticEventKind, SemanticFallback, SemanticFallbackReason, SemanticObject,
    SnapshotChange, emit, fallback_object, record_fallback, semantic_object_from_path,
};
use crate::{
    project::{
        ProjectType,
        metadata::{self, MetadataPart, MetadataPath},
    },
    vcs::status::Change,
};
use gix::bstr::BStr;
use std::collections::{BTreeMap, BTreeSet};

const MD_NAMESPACE: &str = "http://v8.1c.ru/8.3/MDClasses";

/// Compare logical objects and their metadata properties inside one descriptor.
pub(super) fn analyze_descriptor(
    project_type: ProjectType,
    source_path: &BStr,
    snapshot: &SnapshotChange<'_>,
    fallback: Option<SemanticObject>,
    events: &mut BTreeSet<SemanticEvent>,
    fallbacks: &mut BTreeSet<SemanticFallback>,
) {
    let base = metadata::from_path(project_type, source_path);
    let before_objects = base.as_ref().and_then(|base| {
        snapshot
            .before
            .and_then(|contents| descriptor_objects(contents, base))
    });
    let after_objects = base.as_ref().and_then(|base| {
        snapshot
            .after
            .and_then(|contents| descriptor_objects(contents, base))
    });
    if snapshot.before.is_some() && before_objects.is_none()
        || snapshot.after.is_some() && after_objects.is_none()
    {
        record_fallback(fallbacks, SemanticFallbackReason::DescriptorParse, snapshot);
    }

    if let (Some(before), Some(after)) = (&before_objects, &after_objects) {
        compare_descriptor_objects(before, after, snapshot, events);
        return;
    }

    let parsed = match snapshot.change {
        Change::Added | Change::Untracked | Change::IntentToAdd => after_objects,
        Change::Deleted => before_objects,
        Change::Modified | Change::TypeChanged | Change::Conflict => None,
    };
    if let Some(objects) = parsed {
        emit_descriptor_lifecycle(objects, snapshot, events);
        return;
    }

    let object = fallback.or_else(|| {
        fallback_object(
            project_type,
            source_path,
            snapshot.before.or(snapshot.after),
        )
    });
    emit_descriptor_fallback(object, snapshot, events);
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct DescriptorObject {
    pub(super) object: SemanticObject,
    pub(super) properties: String,
}

/// Emit precise lifecycle and property changes from two parsed descriptors.
fn compare_descriptor_objects(
    before: &BTreeMap<String, DescriptorObject>,
    after: &BTreeMap<String, DescriptorObject>,
    snapshot: &SnapshotChange<'_>,
    events: &mut BTreeSet<SemanticEvent>,
) {
    let mut keys: BTreeSet<_> = before.keys().cloned().collect();
    keys.extend(after.keys().cloned());
    for key in keys {
        match (before.get(&key), after.get(&key)) {
            (None, Some(current)) => emit(
                events,
                SemanticEventKind::ObjectAdded,
                snapshot.stage,
                current.object.clone(),
                None,
                snapshot.project_path.clone(),
            ),
            (Some(previous), None) => emit(
                events,
                SemanticEventKind::ObjectRemoved,
                snapshot.stage,
                previous.object.clone(),
                None,
                snapshot.project_path.clone(),
            ),
            (Some(previous), Some(current)) if previous.properties != current.properties => {
                emit(
                    events,
                    SemanticEventKind::ObjectChanged,
                    snapshot.stage,
                    current.object.clone(),
                    None,
                    snapshot.project_path.clone(),
                );
                emit(
                    events,
                    SemanticEventKind::MetadataAttributeChanged,
                    snapshot.stage,
                    current.object.clone(),
                    None,
                    snapshot.project_path.clone(),
                );
                if matches!(current.object.metadata_type, "form" | "common-form") {
                    emit(
                        events,
                        SemanticEventKind::FormChanged,
                        snapshot.stage,
                        current.object.clone(),
                        None,
                        snapshot.project_path.clone(),
                    );
                }
            }
            _ => {}
        }
    }
}

/// Emit all objects from a parsed added or removed descriptor.
fn emit_descriptor_lifecycle(
    objects: BTreeMap<String, DescriptorObject>,
    snapshot: &SnapshotChange<'_>,
    events: &mut BTreeSet<SemanticEvent>,
) {
    let kind = match snapshot.change {
        Change::Added | Change::Untracked | Change::IntentToAdd => SemanticEventKind::ObjectAdded,
        Change::Deleted => SemanticEventKind::ObjectRemoved,
        Change::Modified | Change::TypeChanged | Change::Conflict => return,
    };
    for descriptor in objects.into_values() {
        emit(
            events,
            kind,
            snapshot.stage,
            descriptor.object,
            None,
            snapshot.project_path.clone(),
        );
    }
}

/// Emit a conservative owner-level event when a descriptor is not parseable at both endpoints.
fn emit_descriptor_fallback(
    object: Option<SemanticObject>,
    snapshot: &SnapshotChange<'_>,
    events: &mut BTreeSet<SemanticEvent>,
) {
    let Some(object) = object else {
        return;
    };
    let kind = match snapshot.change {
        Change::Added | Change::Untracked | Change::IntentToAdd => SemanticEventKind::ObjectAdded,
        Change::Deleted => SemanticEventKind::ObjectRemoved,
        Change::Modified | Change::TypeChanged | Change::Conflict => {
            SemanticEventKind::ObjectChanged
        }
    };
    emit(
        events,
        kind,
        snapshot.stage,
        object,
        None,
        snapshot.project_path.clone(),
    );
}

/// Parse object identities and property payloads without depending on formatting elsewhere.
pub(super) fn descriptor_objects(
    contents: &[u8],
    base: &MetadataPath,
) -> Option<BTreeMap<String, DescriptorObject>> {
    let text = std::str::from_utf8(contents)
        .ok()?
        .trim_start_matches('\u{feff}');
    let document = roxmltree::Document::parse_with_options(
        text,
        roxmltree::ParsingOptions {
            nodes_limit: 1_000_000,
            ..Default::default()
        },
    )
    .ok()?;
    let root = document.root_element();
    if !root.has_tag_name((MD_NAMESPACE, "MetaDataObject")) {
        return None;
    }
    let mut roots = root.children().filter(roxmltree::Node::is_element);
    let object = roots.next()?;
    if roots.next().is_some() || object.tag_name().namespace() != Some(MD_NAMESPACE) {
        return None;
    }
    let mut result = BTreeMap::new();
    collect_descriptor_snapshots(object, base, &mut result)?;
    Some(result)
}

/// Recursively retain each supported inline object as an independent semantic identity.
fn collect_descriptor_snapshots(
    node: roxmltree::Node<'_, '_>,
    logical_path: &MetadataPath,
    output: &mut BTreeMap<String, DescriptorObject>,
) -> Option<()> {
    let metadata_type = metadata::kind_from_tag(node.tag_name().name())?;
    let properties = node
        .children()
        .find(|child| child.has_tag_name((MD_NAMESPACE, "Properties")))?;
    let name = properties
        .children()
        .find(|child| child.has_tag_name((MD_NAMESPACE, "Name")))?
        .text()?
        .to_owned();
    let mut path = logical_path.clone();
    if let Some(last) = path.parts.last_mut() {
        last.kind = metadata_type;
        last.name = Some(name);
    }
    let object = semantic_object_from_path(&path)?;
    output.insert(
        object.id.clone(),
        DescriptorObject {
            object,
            properties: xml_signature(properties),
        },
    );
    if let Some(children) = node
        .children()
        .find(|child| child.has_tag_name((MD_NAMESPACE, "ChildObjects")))
    {
        for child in children.children().filter(roxmltree::Node::is_element) {
            let Some(kind) = metadata::kind_from_tag(child.tag_name().name()) else {
                continue;
            };
            let Some(child_name) = child
                .children()
                .find(|item| item.has_tag_name((MD_NAMESPACE, "Properties")))
                .and_then(|item| {
                    item.children()
                        .find(|property| property.has_tag_name((MD_NAMESPACE, "Name")))
                })
                .and_then(|item| item.text())
            else {
                continue;
            };
            let nested = path.with_suffix(&[MetadataPart {
                kind,
                name: Some(child_name.to_owned()),
            }]);
            collect_descriptor_snapshots(child, &nested, output)?;
        }
    }
    Some(())
}

/// Build a formatting-independent signature of one XML subtree.
pub(super) fn xml_signature(node: roxmltree::Node<'_, '_>) -> String {
    let mut signature = String::new();
    append_xml_signature(node, &mut signature);
    signature
}

/// Append directly to one buffer so ancestors do not copy their descendants' signatures.
fn append_xml_signature(node: roxmltree::Node<'_, '_>, signature: &mut String) {
    use std::fmt::Write as _;
    signature.push('<');
    append_xml_name(
        node.tag_name().namespace(),
        node.tag_name().name(),
        signature,
    );
    let mut attributes: Vec<_> = node.attributes().collect();
    attributes.sort_by_key(|attribute| (attribute.namespace(), attribute.name()));
    for attribute in attributes {
        signature.push(' ');
        append_xml_name(attribute.namespace(), attribute.name(), signature);
        write!(signature, "={:?}", attribute.value()).expect("writing to String cannot fail");
    }
    signature.push('>');
    let mut text = String::new();
    for child in node.children() {
        if child.is_text() {
            text.push_str(child.text().unwrap_or_default());
        } else if child.is_element() {
            append_xml_text(&text, signature);
            text.clear();
            append_xml_signature(child, signature);
        }
    }
    append_xml_text(&text, signature);
    signature.push_str("</>");
}

/// Expanded names retain namespace identity independently of the document's chosen prefixes.
fn append_xml_name(namespace: Option<&str>, name: &str, signature: &mut String) {
    use std::fmt::Write as _;
    if let Some(namespace) = namespace {
        write!(signature, "{{{namespace:?}}}").expect("writing to String cannot fail");
    }
    signature.push_str(name);
}

/// Quote text so escaped markup cannot impersonate an element; formatting stays insignificant.
fn append_xml_text(text: &str, signature: &mut String) {
    use std::fmt::Write as _;
    let text = text.trim();
    if !text.is_empty() {
        write!(signature, "{text:?}").expect("writing to String cannot fail");
    }
}
