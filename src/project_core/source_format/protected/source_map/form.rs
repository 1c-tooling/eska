//! Dialog coordinates follow pinned native bindings and exact selected YAML values, never short names.

use std::ops::Range;

use roxmltree::{Document, Node};
use serde_json::Value;
use yaml_edit::{AsYaml, YamlFile, YamlNode};

use super::{
    FileOrigin, InputAuthority, MappedFile, OriginTexts, SourceLocation, location::verify_bytes,
};
use crate::project_core::{
    MetadataKind,
    source_format::{
        Error, FormId, FormPropertyValue, ManagedFormSource, ObjectSource, managed_form::NAMESPACE,
        xml, yaml,
    },
};

const V8: &str = "http://v8.1c.ru/8.1/data/core";

/// Each hit addresses a sole editable value or a pinned definition in one captured form YAML.
struct Hit {
    path: Vec<String>,
    selected: Option<Value>,
}

/// Reconstruct permitted native output from accepted stencils before attributing current values.
pub(super) fn locate(
    file: &MappedFile,
    native: &[u8],
    range: &Range<usize>,
    texts: OriginTexts<'_>,
) -> Result<SourceLocation, Error> {
    let FileOrigin::ManagedForm {
        uuid,
        descriptor: _,
        accepted,
        working,
        uses_working_values,
        queries,
    } = &file.origin
    else {
        return Err(mismatch());
    };
    verify_bytes(texts.accepted.as_bytes(), accepted.bytes, &accepted.sha256)?;
    let (value, tree) = yaml::decode_with_tree(texts.accepted)?;
    let owner = ObjectSource::from_value(value.clone())?;
    if owner.document.descriptor.object.uuid != *uuid
        || !matches!(
            owner.document.descriptor.object.kind,
            MetadataKind::Form | MetadataKind::CommonForm
        )
    {
        return Err(mismatch());
    }
    let original = owner.form.ok_or_else(mismatch)?;
    let native =
        std::str::from_utf8(native).map_err(|_| Error::root("source_map_native_text_invalid"))?;
    if !native.is_char_boundary(range.start) || !native.is_char_boundary(range.end) {
        return Err(Error::root("source_map_span_invalid"));
    }
    let restored;
    let adjusted;
    let (text, range) = if queries.is_empty() {
        (native, range)
    } else {
        (restored, adjusted) = crate::project_core::source_format::queries::restore_native_queries(
            &original.to_xml()?,
            native,
            range,
        )?;
        (restored.as_str(), &adjusted)
    };
    let selected = ManagedFormSource::from_xml(text)?;
    let mut expected = original.clone();
    expected.layout = selected.layout.clone();
    expected.elements = selected.elements.clone();
    expected.properties = selected.properties.clone();
    expected.validate_change(&original)?;
    if expected.to_xml()? != text {
        return Err(mismatch());
    }
    let hit = hit(&original, &selected, text, range.start)?;
    let mut location = SourceLocation {
        authority: InputAuthority::Accepted,
        input: accepted.clone(),
        range: None,
        uuid: Some(uuid.clone()),
    };
    if let Some(selected) = hit.selected
        && value_at(&value, &hit.path) != Some(&selected)
    {
        if !uses_working_values {
            return Err(mismatch());
        }
        let input = working.as_ref().ok_or_else(mismatch)?;
        let text = texts
            .working
            .ok_or_else(|| Error::root("source_map_origin_text_missing"))?;
        verify_bytes(text.as_bytes(), input.bytes, &input.sha256)?;
        let (current, tree) = yaml::decode_with_tree(text)?;
        if value_at(&current, &hit.path) != Some(&selected) {
            return Err(mismatch());
        }
        location.authority = InputAuthority::Working;
        location.input = input.clone();
        location.range = Some(yaml_range(&tree, &hit.path)?);
    } else {
        location.range = Some(yaml_range(&tree, &hit.path)?);
    }
    Ok(location)
}

/// XML scopes distinguish UI identities from equal decimal Attribute/Command/column IDs.
fn ui_id(node: Node<'_, '_>, form: &ManagedFormSource) -> Option<FormId> {
    if node.tag_name().namespace() != Some(NAMESPACE) {
        return None;
    }
    let id = FormId::parse(node.attribute("id")?).ok()?;
    (form.elements.get(&id)?.kind.native_name() == node.tag_name().name()).then_some(id)
}

/// Property values outrank their enclosing definition; displaced nodes point to current layout entries.
fn hit(
    accepted: &ManagedFormSource,
    selected: &ManagedFormSource,
    text: &str,
    offset: usize,
) -> Result<Hit, Error> {
    let document = Document::parse_with_options(
        text,
        roxmltree::ParsingOptions {
            nodes_limit: 1_000_000,
            ..Default::default()
        },
    )
    .map_err(|_| Error::root("source_map_native_text_invalid"))?;
    let Some(node) = document
        .descendants()
        .rfind(|node| node.is_element() && node.range().contains(&offset))
    else {
        return Ok(Hit {
            path: vec!["form".into()],
            selected: None,
        });
    };
    for property in node.ancestors().filter(Node::is_element) {
        let Some(parent) = property.parent_element() else {
            continue;
        };
        let id = ui_id(parent, selected);
        let properties = if let Some(id) = &id {
            &selected.elements.get(id).ok_or_else(mismatch)?.properties
        } else if parent == document.root_element() {
            &selected.properties
        } else {
            continue;
        };
        if let Some((name, value)) = properties
            .iter()
            .find(|(name, _)| property.has_tag_name((NAMESPACE, name.as_str())))
        {
            let mut path = definition_path(id.as_ref());
            path.extend(["properties".into(), name.as_str().into()]);
            return property_hit(property, node, offset, path, value);
        }
    }
    if let Some(id) = node
        .ancestors()
        .filter(Node::is_element)
        .find_map(|node| ui_id(node, selected))
    {
        if let Some(current) = placement(selected, &id)
            && placement(accepted, &id).as_ref() != Some(&current)
        {
            return Ok(Hit {
                path: current,
                selected: Some(Value::String(id.as_str().into())),
            });
        }
        return Ok(Hit {
            path: definition_path(Some(&id)),
            selected: None,
        });
    }
    Ok(Hit {
        path: vec!["form".into()],
        selected: None,
    })
}

/// A localized text or formatted flag keeps a scalar coordinate when its native slot is identifiable.
fn property_hit(
    property: Node<'_, '_>,
    node: Node<'_, '_>,
    offset: usize,
    mut path: Vec<String>,
    value: &FormPropertyValue,
) -> Result<Hit, Error> {
    if let FormPropertyValue::Localized(localized) = value {
        if property
            .attributes()
            .any(|attribute| attribute.name() == "formatted" && attribute.range().contains(&offset))
            && let Some(formatted) = localized.formatted
        {
            path.push("formatted".into());
            return Ok(Hit {
                path,
                selected: Some(Value::Bool(formatted)),
            });
        }
        if let Some(item) = node
            .ancestors()
            .find(|node| node.has_tag_name((V8, "item")) && node.parent_element() == Some(property))
            && let Some(language) = item.children().find(|node| node.has_tag_name((V8, "lang")))
            && let Some(text) = localized.text.get(&xml::text(language))
        {
            path.extend(["text".into(), xml::text(language)]);
            return Ok(Hit {
                path,
                selected: Some(Value::String(text.clone())),
            });
        }
    }
    Ok(Hit {
        path,
        selected: Some(serde_json::to_value(value).map_err(|_| mismatch())?),
    })
}

/// UI identity addresses one definition; the form root has its own independent property map.
fn definition_path(id: Option<&FormId>) -> Vec<String> {
    let mut path = vec!["form".into()];
    if let Some(id) = id {
        path.extend(["elements".into(), id.as_str().into()]);
    }
    path
}

/// A layout entry identifies both its accepted parent scope and its current sequence position.
fn placement(form: &ManagedFormSource, id: &FormId) -> Option<Vec<String>> {
    let mut path = vec!["form".into(), "layout".into()];
    if let Some(index) = form.layout.roots.iter().position(|item| item == id) {
        path.extend(["roots".into(), index.to_string()]);
    } else {
        let (owner, index) = form.layout.children.iter().find_map(|(owner, children)| {
            children
                .iter()
                .position(|item| item == id)
                .map(|index| (owner, index))
        })?;
        path.extend(["children".into(), owner.as_str().into(), index.to_string()]);
    }
    Some(path)
}

/// Numeric UI/language mapping keys stay strings; only actual JSON arrays interpret indices.
fn value_at<'a>(value: &'a Value, path: &[String]) -> Option<&'a Value> {
    path.iter().try_fold(value, |value, key| {
        if let Some(items) = value.as_array() {
            items.get(key.parse::<usize>().ok()?)
        } else {
            value.get(key)
        }
    })
}

/// Dynamic form IDs and languages resolve through the validated CST, never a textual search.
fn yaml_range(tree: &YamlFile, path: &[String]) -> Result<Range<usize>, Error> {
    let root = tree
        .document()
        .and_then(|document| document.as_mapping())
        .ok_or_else(mismatch)?;
    let node = path
        .iter()
        .try_fold(YamlNode::Mapping(root), |node, key| {
            if let Some(items) = node.as_sequence() {
                items.get(key.parse::<usize>().ok()?)
            } else {
                node.as_mapping()?.get(key)
            }
        })
        .ok_or_else(mismatch)?;
    let range = node.as_node().ok_or_else(mismatch)?.text_range();
    Ok(usize::from(range.start())..usize::from(range.end()))
}

/// A mismatch cannot invent coordinates in accepted or working text with unrelated semantics.
fn mismatch() -> Error {
    Error::root("source_map_origin_mismatch")
}
