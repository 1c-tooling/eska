//! Lexical holes retain XML outside proven UI definitions and actual ordered collection slots.

use std::{collections::BTreeMap, ops::Range};

use roxmltree::{Document, Node};

use super::{ChildFrame, Error, ManagedFormSource, NativeForm, Part, Stencil, binding};
use crate::project_core::source_format::{FormElementKind, FormId, ManagedFormLayout, Namespace};

struct Hole {
    range: Range<usize>,
    part: Part,
}

/// The accepted read model supplies identities; original byte ranges supply only their inverse slots.
pub(super) fn decode(input: &str) -> Result<ManagedFormSource, Error> {
    let model = ManagedFormLayout::from_xml(input)?;
    let document = Document::parse(input).map_err(|_| binding())?;
    let root = document.root_element();
    let root_scope = namespaces(root);
    let mut elements = BTreeMap::new();
    for node in root.descendants().filter(Node::is_element) {
        let Some(id) = ui_id(node)? else {
            continue;
        };
        let scope = namespaces(node);
        elements.insert(
            id,
            Stencil {
                namespaces: (scope != root_scope).then_some(scope),
                parts: parts(node, input, node.range(), true)?,
            },
        );
    }
    let source = ManagedFormSource {
        schema_version: 1,
        elements: model.elements,
        layout: model.layout,
        native: NativeForm {
            namespaces: root_scope,
            root: parts(root, input, 0..input.len(), false)?,
            elements,
        },
    };
    if source.to_xml()? != input {
        return Err(Error::root("source_form_lexical_unverified"));
    }
    Ok(source)
}

/// Every opaque descendant retains the same effective scope after a layout move.
pub(super) fn check_scopes(source: &ManagedFormSource, xml: &str) -> Result<(), Error> {
    let document = Document::parse(xml).map_err(|_| binding())?;
    let root = document.root_element();
    if namespaces(root) != source.native.namespaces {
        return Err(binding());
    }
    for node in root.descendants().filter(Node::is_element) {
        let Some(id) = ui_id(node)? else {
            continue;
        };
        let stencil = source.native.elements.get(&id).ok_or_else(binding)?;
        let expected = stencil
            .namespaces
            .as_ref()
            .unwrap_or(&source.native.namespaces);
        if &namespaces(node) != expected {
            return Err(Error::root("source_form_namespace_context_changed").in_field(id.as_str()));
        }
    }
    Ok(())
}

/// Data/command IDs never acquire UI ownership merely because their numeric spelling coincides.
fn ui_id(node: Node<'_, '_>) -> Result<Option<FormId>, Error> {
    if node.tag_name().namespace() == Some(super::super::NAMESPACE)
        && FormElementKind::native(node.tag_name().name()).is_some()
        && let Some(id) = node.attribute("id")
    {
        FormId::parse(id).map(Some)
    } else {
        Ok(None)
    }
}

/// Scope order is lexical trivia; the prefix-to-URI bindings are the relevant invariant.
fn namespaces(node: Node<'_, '_>) -> Vec<Namespace> {
    let mut result: Vec<_> = node
        .namespaces()
        .map(|namespace| Namespace {
            prefix: namespace.name().map(str::to_owned),
            uri: namespace.uri().into(),
        })
        .collect();
    result.sort_by(|left, right| left.prefix.cmp(&right.prefix));
    result
}

/// Flat chunks exclude UI children, class/name/ID values and the contents of ordered slots.
fn parts(
    node: Node<'_, '_>,
    input: &str,
    range: Range<usize>,
    definition: bool,
) -> Result<Vec<Part>, Error> {
    let mut holes = Vec::new();
    if definition {
        for (attribute, part) in [("name", Part::Name), ("id", Part::Id)] {
            let attribute = node
                .attributes()
                .find(|value| value.namespace().is_none() && value.name() == attribute)
                .ok_or_else(binding)?;
            holes.push(Hole {
                range: attribute.range_value(),
                part,
            });
        }
        class_holes(node, input, &mut holes)?;
    }
    for child in node.children().filter(Node::is_element) {
        let part = if child.has_tag_name((super::super::NAMESPACE, "ChildItems")) {
            Some(Part::Children {
                frame: child_frame(child, input)?,
            })
        } else {
            ui_id(child)?.map(|id| Part::Element { id })
        };
        if let Some(part) = part {
            holes.push(Hole {
                range: child.range(),
                part,
            });
        }
    }
    holes.sort_by_key(|hole| hole.range.start);
    let mut cursor = range.start;
    let mut result = Vec::new();
    for hole in holes {
        if hole.range.start < cursor || hole.range.end > range.end {
            return Err(binding());
        }
        if cursor < hole.range.start {
            result.push(Part::Xml {
                value: slice(input, cursor..hole.range.start)?.into(),
            });
        }
        cursor = hole.range.end;
        result.push(hole.part);
    }
    if cursor < range.end {
        result.push(Part::Xml {
            value: slice(input, cursor..range.end)?.into(),
        });
    }
    Ok(result)
}

/// Both native tag occurrences consume the same typed class, retaining the original namespace prefix.
fn class_holes(node: Node<'_, '_>, input: &str, holes: &mut Vec<Hole>) -> Result<(), Error> {
    let range = node.range();
    let (qname, opening) = opening_name(input, range.start)?;
    let prefix = qname
        .rsplit_once(':')
        .map_or_else(String::new, |(prefix, _)| format!("{prefix}:"));
    holes.push(Hole {
        range: opening,
        part: Part::Class {
            prefix: prefix.clone(),
        },
    });
    let text = slice(input, range.clone())?;
    if !text.ends_with("/>") {
        let start = range.start + text.rfind("</").ok_or_else(binding)? + 2;
        let closing = start..start + qname.len();
        if slice(input, closing.clone())? != qname {
            return Err(binding());
        }
        holes.push(Hole {
            range: closing,
            part: Part::Class { prefix },
        });
    }
    Ok(())
}

/// Existing empty slots can expand; absent slots never gain authority from a generic XML insertion.
fn child_frame(node: Node<'_, '_>, input: &str) -> Result<ChildFrame, Error> {
    let range = node.range();
    let text = slice(input, range.clone())?;
    let (qname, _) = opening_name(input, range.start)?;
    if let Some(prefix) = text.strip_suffix("/>") {
        return Ok(ChildFrame {
            open: format!("{prefix}>"),
            close: format!("</{qname}>"),
            gaps: vec![String::new()],
            empty: Some(text.into()),
        });
    }
    let start = opening_end(input, range.clone())?;
    let end = range.start + text.rfind("</").ok_or_else(binding)?;
    let mut cursor = start;
    let mut gaps = Vec::new();
    for child in node.children().filter(Node::is_element) {
        if ui_id(child)?.is_none() {
            return Err(binding());
        }
        let child = child.range();
        gaps.push(slice(input, cursor..child.start)?.into());
        cursor = child.end;
    }
    gaps.push(slice(input, cursor..end)?.into());
    Ok(ChildFrame {
        open: slice(input, range.start..start)?.into(),
        close: slice(input, end..range.end)?.into(),
        gaps,
        empty: None,
    })
}

/// Native `QName` ranges end before attributes or closing punctuation, never by a guessed class suffix.
fn opening_name(input: &str, start: usize) -> Result<(&str, Range<usize>), Error> {
    let tail = input.get(start + 1..).ok_or_else(binding)?;
    let length = tail
        .find(|character: char| character.is_ascii_whitespace() || matches!(character, '/' | '>'))
        .ok_or_else(binding)?;
    Ok((&tail[..length], start + 1..start + 1 + length))
}

/// All native parser offsets must remain ordered UTF-8 boundaries in the exact input text.
fn slice(input: &str, range: Range<usize>) -> Result<&str, Error> {
    input.get(range).ok_or_else(binding)
}

/// Quoted greater-than signs belong to attributes, not to the start tag's closing punctuation.
fn opening_end(input: &str, range: Range<usize>) -> Result<usize, Error> {
    let mut quote = None;
    for (offset, character) in slice(input, range.clone())?.char_indices() {
        match (quote, character) {
            (None, '>') => return Ok(range.start + offset + 1),
            (None, '\'' | '"') => quote = Some(character),
            (Some(delimiter), value) if delimiter == value => quote = None,
            _ => {}
        }
    }
    Err(binding())
}
