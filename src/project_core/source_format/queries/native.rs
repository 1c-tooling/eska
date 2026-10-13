//! Qualified types and direct native scopes identify saved queries, including union dataset members.

use std::collections::BTreeSet;

use roxmltree::{Document, Node};

use super::{
    Error, FormId, NativeQuery, NativeQueryDocument, QUERY_REFERENCE_NAMESPACE, QueryProfile,
    QuerySlot, binding, references, stencil, unverified,
};
use crate::project_core::source_format::{validation, xml};

const DCS: &str = "http://v8.1c.ru/8.1/data-composition-system/schema";
const FORM: &str = "http://v8.1c.ru/8.3/xcf/logform";
const XSI: &str = "http://www.w3.org/2001/XMLSchema-instance";

/// Both native values and source references use the same actual parent/type scopes.
pub(super) struct SlotNode<'a, 'input> {
    pub(super) slot: QuerySlot,
    pub(super) node: Node<'a, 'input>,
}

/// Byte ranges remove every proven query node while leaving all unrelated native XML exactly once.
pub(super) fn decode(input: &str) -> Result<NativeQueryDocument, Error> {
    let document = parse(input)?;
    if document.descendants().filter(Node::is_element).any(|node| {
        node.namespaces()
            .any(|namespace| namespace.uri() == QUERY_REFERENCE_NAMESPACE)
    }) {
        return Err(references::invalid());
    }
    let queries = slots(document.root_element())?
        .into_iter()
        .map(|slot| {
            Ok(NativeQuery {
                slot: slot.slot,
                text: xml::text(slot.node),
                frame: stencil::Frame::from_node(slot.node, input)?,
            })
        })
        .collect::<Result<Vec<_>, Error>>()?;
    let mut parts = Vec::new();
    let mut cursor = 0;
    for query in &queries {
        parts.push(
            input
                .get(cursor..query.frame.range.start)
                .ok_or_else(binding)?
                .into(),
        );
        cursor = query.frame.range.end;
    }
    parts.push(input.get(cursor..).ok_or_else(binding)?.into());
    Ok(NativeQueryDocument {
        profile: profile(document.root_element())?,
        parts,
        queries,
    })
}

/// Shared bounded XML parsing does not grant file, language or supplier authority.
pub(super) fn parse(input: &str) -> Result<Document<'_>, Error> {
    if input.contains("<!DOCTYPE") || input.contains("<!ENTITY") {
        return Err(Error::root("source_xml_dtd_forbidden"));
    }
    Document::parse_with_options(
        input,
        roxmltree::ParsingOptions {
            nodes_limit: 1_000_000,
            ..Default::default()
        },
    )
    .map_err(|_| Error::root("source_query_xml_invalid"))
}

/// Native order and unique scopes are checked before either lexical representation is decoded.
pub(super) fn slots<'a, 'input>(
    root: Node<'a, 'input>,
) -> Result<Vec<SlotNode<'a, 'input>>, Error> {
    let mut queries = match profile(root)? {
        QueryProfile::Composition => composition(root)?,
        QueryProfile::ManagedForm => dynamic_lists(root)?,
    };
    queries.sort_by_key(|query| query.node.range().start);
    let mut slots = BTreeSet::new();
    let mut cursor = 0;
    for query in &queries {
        if !slots.insert(&query.slot) || query.node.range().start < cursor {
            return Err(binding());
        }
        cursor = query.node.range().end;
    }
    Ok(queries)
}

/// Classification stays separate from query count, so empty resources retain their actual class.
pub(super) fn profile(root: Node<'_, '_>) -> Result<QueryProfile, Error> {
    if root.has_tag_name((DCS, "DataCompositionSchema")) && root.attributes().len() == 0 {
        Ok(QueryProfile::Composition)
    } else if root.has_tag_name((FORM, "Form"))
        && root.attribute("version") == Some("2.20")
        && root.attributes().len() == 1
    {
        Ok(QueryProfile::ManagedForm)
    } else {
        Err(unverified())
    }
}

/// Full native dataset names are scoped by unions; sibling order is never their identity.
fn composition<'a, 'input>(root: Node<'a, 'input>) -> Result<Vec<SlotNode<'a, 'input>>, Error> {
    let mut pending = datasets(root, "dataSet")?
        .into_iter()
        .map(|node| (node, Vec::new()))
        .collect::<Vec<_>>();
    let mut queries = Vec::new();
    while let Some((node, mut path)) = pending.pop() {
        path.push(dataset_name(node)?);
        match qualified_type(node, DCS)? {
            "DataSetQuery" => {
                let query = direct(node, DCS, "query")?.ok_or_else(unverified)?;
                queries.push(SlotNode {
                    node: query,
                    slot: QuerySlot::Composition { datasets: path },
                });
            }
            "DataSetUnion" => {
                pending.extend(
                    datasets(node, "item")?
                        .into_iter()
                        .map(|item| (item, path.clone())),
                );
                if direct(node, DCS, "query")?.is_some() {
                    return Err(unverified());
                }
            }
            "DataSetObject" => {
                if direct(node, DCS, "query")?.is_some() {
                    return Err(unverified());
                }
            }
            _ => return Err(unverified()),
        }
    }
    Ok(queries)
}

/// Automatic lists without saved text do not create queries; inconsistent manual/text shapes refuse promotion.
fn dynamic_lists<'a, 'input>(root: Node<'a, 'input>) -> Result<Vec<SlotNode<'a, 'input>>, Error> {
    let Some(attributes) = direct(root, FORM, "Attributes")? else {
        return Ok(Vec::new());
    };
    let mut queries = Vec::new();
    let mut identities = BTreeSet::new();
    for attribute in attributes
        .children()
        .filter(|node| node.has_tag_name((FORM, "Attribute")))
    {
        let id = FormId::parse(attribute.attribute("id").ok_or_else(unverified)?)?;
        if id.as_str().starts_with('-') || !identities.insert(id.clone()) {
            return Err(binding());
        }
        let Some(settings) = direct(attribute, FORM, "Settings")? else {
            continue;
        };
        let Some(kind) = settings.attribute((XSI, "type")) else {
            if direct(settings, FORM, "QueryText")?.is_some() {
                return Err(unverified());
            }
            continue;
        };
        if kind.rsplit(':').next() != Some("DynamicList") {
            if direct(settings, FORM, "QueryText")?.is_some() {
                return Err(unverified());
            }
            continue;
        }
        if qualified_type(settings, FORM)? != "DynamicList" {
            return Err(unverified());
        }
        let manual = direct(settings, FORM, "ManualQuery")?.ok_or_else(unverified)?;
        let text = direct(settings, FORM, "QueryText")?;
        if manual.children().any(|child| child.is_element()) {
            return Err(unverified());
        }
        match (xml::text(manual).as_str(), text) {
            ("false", None) => {}
            ("true", Some(query)) => {
                queries.push(SlotNode {
                    node: query,
                    slot: QuerySlot::DynamicList { attribute: id },
                });
            }
            _ => return Err(unverified()),
        }
    }
    Ok(queries)
}

/// Native names are unique within their actual parent, including case-insensitive filename collision candidates.
fn datasets<'a, 'input>(node: Node<'a, 'input>, tag: &str) -> Result<Vec<Node<'a, 'input>>, Error> {
    let mut names = BTreeSet::new();
    node.children()
        .filter(|child| child.has_tag_name((DCS, tag)))
        .map(|child| {
            if !names.insert(dataset_name(child)?.to_lowercase()) {
                return Err(binding());
            }
            Ok(child)
        })
        .collect()
}

/// Dataset identifiers retain their exact Unicode spelling; empty or composite names cannot bind a query.
fn dataset_name(node: Node<'_, '_>) -> Result<String, Error> {
    let name = direct(node, DCS, "name")?.ok_or_else(unverified)?;
    if name.attributes().len() != 0 || name.children().any(|child| child.is_element()) {
        return Err(unverified());
    }
    let name = xml::text(name);
    if name.trim().is_empty() {
        return Err(binding());
    }
    Ok(name)
}

/// `QName` namespace identity comes from the node's scope, never from a familiar prefix string.
fn qualified_type<'a>(node: Node<'a, '_>, namespace: &str) -> Result<&'a str, Error> {
    let value = node.attribute((XSI, "type")).ok_or_else(unverified)?;
    let (prefix, name) = value
        .split_once(':')
        .map_or((None, value), |(prefix, name)| (Some(prefix), name));
    if !validation::ncname(name)
        || prefix.is_some_and(|prefix| !validation::ncname(prefix))
        || node.lookup_namespace_uri(prefix) != Some(namespace)
    {
        return Err(unverified());
    }
    Ok(name)
}

/// Duplicate direct slots are ambiguous even if their texts happen to be equal.
fn direct<'a, 'input>(
    node: Node<'a, 'input>,
    namespace: &str,
    tag: &str,
) -> Result<Option<Node<'a, 'input>>, Error> {
    let mut children = node
        .children()
        .filter(|child| child.has_tag_name((namespace, tag)));
    let first = children.next();
    if children.next().is_some() {
        return Err(binding());
    }
    Ok(first)
}
