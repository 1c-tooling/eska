//! Schema-aware query discovery; XML parsing validates syntax, raw bytes supply positions.

use std::{collections::BTreeSet, ops::Range};

use roxmltree::{Document, Node};

use super::{Query, QueryError, QueryIdentity, SourceComment, mapping::MappedText};

const SCHEMA: &str = "http://v8.1c.ru/8.1/data-composition-system/schema";
const FORM: &str = "http://v8.1c.ru/8.3/xcf/logform";
const XSI: &str = "http://www.w3.org/2001/XMLSchema-instance";

/// Empty payloads still retain their schema kind for a logical owner check.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum XmlKind {
    DataComposition,
    Form,
}

/// Nested dataset paths must not amplify a small document into unbounded repeated identities.
#[derive(Default)]
struct Queries {
    values: Vec<Query>,
    identity_bytes: usize,
    mappings: usize,
}

impl Queries {
    /// Bound aggregate output, not only each individual scalar's map.
    fn push(&mut self, value: Query) -> Result<(), QueryError> {
        let bytes = match &value.identity {
            QueryIdentity::DataSet(path) => path.iter().map(String::len).sum(),
            QueryIdentity::DynamicList { name, id } => name.len() + id.len(),
            QueryIdentity::BslLiteral(_) => return Err(QueryError::UnsupportedStructure),
        };
        self.identity_bytes += bytes;
        self.mappings += value.map.spans().len();
        if self.values.len() == 1024
            || self.identity_bytes > super::MAX_SNAPSHOT_BYTES
            || self.mappings > 65_536
        {
            return Err(QueryError::TooLarge);
        }
        self.values.push(value);
        Ok(())
    }
}

/// Find saved query values in a single SCD/form payload, without inventing automatic queries.
///
/// # Errors
/// Rejects oversized, malformed or ambiguous XML before returning any query candidates.
pub fn read_xml(input: &str) -> Result<Vec<Query>, QueryError> {
    read_with_kind(input).map(|(_, values)| values)
}

/// Return the validated schema kind together with queries, including automatic forms with none.
pub fn read_with_kind(input: &str) -> Result<(XmlKind, Vec<Query>), QueryError> {
    super::check_size(input)?;
    crate::project::metadata_parser::check_envelope(input).map_err(|error| match error {
        crate::project::metadata_parser::ParseError::TooLarge
        | crate::project::metadata_parser::ParseError::TooDeep => QueryError::TooLarge,
        _ => QueryError::InvalidXml,
    })?;
    let document = Document::parse_with_options(
        input,
        roxmltree::ParsingOptions {
            nodes_limit: 250_000,
            ..Default::default()
        },
    )
    .map_err(|_| QueryError::InvalidXml)?;
    let root = document.root_element();
    let mut queries = Queries::default();
    let kind = if root.has_tag_name((SCHEMA, "DataCompositionSchema")) {
        data_sets(input, root, &[], &mut queries)?;
        XmlKind::DataComposition
    } else if root.has_tag_name((FORM, "Form")) {
        dynamic_lists(input, root, &mut queries)?;
        XmlKind::Form
    } else {
        return Err(QueryError::UnsupportedStructure);
    };
    Ok((kind, queries.values))
}

/// Dataset names are structural identity, including every union dataset ancestor.
fn data_sets(
    input: &str,
    parent: Node<'_, '_>,
    ancestors: &[String],
    queries: &mut Queries,
) -> Result<(), QueryError> {
    let mut names = BTreeSet::new();
    for data_set in parent
        .children()
        .filter(|node| node.has_tag_name((SCHEMA, "dataSet")))
    {
        let name = scalar(one(data_set, SCHEMA, "name")?)?;
        if name.is_empty() || !names.insert(name.clone()) {
            return Err(QueryError::UnsupportedStructure);
        }
        if ancestors.iter().map(String::len).sum::<usize>() + name.len() > 65_536 {
            return Err(QueryError::TooLarge);
        }
        let mut path = ancestors.to_vec();
        path.push(name);
        if schema_type(data_set, SCHEMA, "DataSetQuery") {
            queries.push(value(
                input,
                one(data_set, SCHEMA, "query")?,
                QueryIdentity::DataSet(path),
            )?)?;
        } else if schema_type(data_set, SCHEMA, "DataSetUnion") {
            data_sets(input, data_set, &path, queries)?;
        }
    }
    Ok(())
}

/// Only a manual dynamic list with an actual saved `QueryText` has editable source bytes.
fn dynamic_lists(input: &str, root: Node<'_, '_>, queries: &mut Queries) -> Result<(), QueryError> {
    let containers = children(root, FORM, "Attributes");
    if containers.len() > 1 {
        return Err(QueryError::UnsupportedStructure);
    }
    let Some(attributes) = containers.first() else {
        return Ok(());
    };
    let mut names = BTreeSet::new();
    let mut ids = BTreeSet::new();
    for attribute in attributes
        .children()
        .filter(|node| node.has_tag_name((FORM, "Attribute")))
    {
        let name = attribute.attribute("name").filter(|name| !name.is_empty());
        let id = attribute.attribute("id").filter(|id| !id.is_empty());
        let (Some(name), Some(id)) = (name, id) else {
            return Err(QueryError::UnsupportedStructure);
        };
        if !names.insert(name) || !ids.insert(id) {
            return Err(QueryError::UnsupportedStructure);
        }
        let settings = children(attribute, FORM, "Settings");
        if settings.len() > 1 {
            return Err(QueryError::UnsupportedStructure);
        }
        let Some(settings) = settings.first().copied() else {
            continue;
        };
        if !schema_type(settings, FORM, "DynamicList") {
            continue;
        }
        let manual = children(settings, FORM, "ManualQuery");
        if manual.len() > 1 {
            return Err(QueryError::UnsupportedStructure);
        }
        let Some(manual) = manual.first() else {
            continue;
        };
        match scalar(*manual)?.as_str() {
            "true" | "1" => {}
            "false" | "0" => continue,
            _ => return Err(QueryError::UnsupportedStructure),
        }
        queries.push(value(
            input,
            one(settings, FORM, "QueryText")?,
            QueryIdentity::DynamicList {
                name: name.to_owned(),
                id: id.to_owned(),
            },
        )?)?;
    }
    Ok(())
}

/// Resolve the `QName` in `xsi:type`, including prefixed and default namespaces.
fn schema_type(node: Node<'_, '_>, namespace: &str, name: &str) -> bool {
    let Some(value) = node.attribute((XSI, "type")) else {
        return false;
    };
    let (prefix, local) = value
        .split_once(':')
        .map_or((None, value), |(prefix, local)| (Some(prefix), local));
    local == name && node.lookup_namespace_uri(prefix) == Some(namespace)
}

/// Match only direct children in the expected namespace, never unrelated extension markup.
fn children<'a, 'input>(
    node: Node<'a, 'input>,
    namespace: &str,
    name: &str,
) -> Vec<Node<'a, 'input>> {
    node.children()
        .filter(|node| node.has_tag_name((namespace, name)))
        .collect()
}

/// Repeated scalar fields make ownership ambiguous and cannot be silently resolved by order.
fn one<'a, 'input>(
    node: Node<'a, 'input>,
    namespace: &str,
    name: &str,
) -> Result<Node<'a, 'input>, QueryError> {
    let matches = children(node, namespace, name);
    if let [node] = matches.as_slice() {
        Ok(*node)
    } else {
        Err(QueryError::UnsupportedStructure)
    }
}

/// Schema identity fields permit text and comments, but no nested values or processing instructions.
fn scalar(node: Node<'_, '_>) -> Result<String, QueryError> {
    if node
        .children()
        .any(|child| child.is_element() || child.is_pi())
    {
        return Err(QueryError::UnsupportedStructure);
    }
    Ok(node
        .children()
        .filter(Node::is_text)
        .filter_map(|text| text.text())
        .collect())
}

/// A hand-decoded value is accepted only if it agrees with the complete XML parser.
fn value(input: &str, node: Node<'_, '_>, identity: QueryIdentity) -> Result<Query, QueryError> {
    let expected = scalar(node)?;
    let inner = content_range(input, node.range())?;
    let mut mapped = MappedText::new(inner.end);
    let mut comments = Vec::new();
    let mut offset = inner.start;
    while offset < inner.end {
        let rest = &input[offset..inner.end];
        if rest.starts_with("<!--") {
            if comments.len() == super::MAX_SOURCE_COMMENTS {
                return Err(QueryError::TooLarge);
            }
            let end = offset + rest.find("-->").ok_or(QueryError::InvalidXml)? + 3;
            comments.push(SourceComment {
                source: offset..end,
                query_offset: mapped.text.len(),
                text: input[offset..end].to_owned(),
            });
            offset = end;
        } else if rest.starts_with("<![CDATA[") {
            let end = offset + rest.find("]]>").ok_or(QueryError::InvalidXml)?;
            decode_text(input, offset + 9..end, false, &mut mapped)?;
            offset = end + 3;
        } else {
            let end = offset + rest.find('<').unwrap_or(rest.len());
            if end == offset {
                return Err(QueryError::UnsupportedStructure);
            }
            decode_text(input, offset..end, true, &mut mapped)?;
            offset = end;
        }
    }
    let mapped = mapped.finish(inner.end);
    if expected != mapped.text {
        return Err(QueryError::UnsupportedStructure);
    }
    Ok(Query {
        identity,
        text: mapped.text,
        source: node.range(),
        map: mapped.map,
        comments,
    })
}

/// Locate markup boundaries without treating a quoted '>' attribute as the opening tag's end.
pub(super) fn content_range(input: &str, range: Range<usize>) -> Result<Range<usize>, QueryError> {
    let source = &input[range.clone()];
    if source.ends_with("/>") {
        return Ok(range.end - 2..range.end - 2);
    }
    let mut quote = None;
    let mut start = None;
    for (index, byte) in source.bytes().enumerate() {
        match (quote, byte) {
            (None, b'\'' | b'"') => quote = Some(byte),
            (Some(delimiter), current) if delimiter == current => quote = None,
            (None, b'>') => {
                start = Some(range.start + index + 1);
                break;
            }
            _ => {}
        }
    }
    let start = start.ok_or(QueryError::InvalidXml)?;
    let end = range.start + source.rfind("</").ok_or(QueryError::InvalidXml)?;
    Ok(start..end)
}

/// XML normalizes physical CR/CRLF but preserves character references to CR verbatim.
fn decode_text(
    input: &str,
    range: Range<usize>,
    entities: bool,
    mapped: &mut MappedText,
) -> Result<(), QueryError> {
    let mut offset = range.start;
    while offset < range.end {
        let rest = &input[offset..range.end];
        let current = rest.chars().next().ok_or(QueryError::InvalidXml)?;
        let (value, length, direct) = if current == '\r' {
            ('\n', if rest.starts_with("\r\n") { 2 } else { 1 }, false)
        } else if entities && current == '&' {
            let end = rest.find(';').ok_or(QueryError::InvalidXml)?;
            (entity(&rest[1..end])?, end + 1, false)
        } else {
            (current, current.len_utf8(), true)
        };
        mapped.push(value, offset..offset + length, direct)?;
        offset += length;
    }
    Ok(())
}

/// DTDs are disabled by the parser; only XML's built-ins and numeric references are accepted.
fn entity(name: &str) -> Result<char, QueryError> {
    let value = match name {
        "amp" => '&',
        "lt" => '<',
        "gt" => '>',
        "quot" => '"',
        "apos" => '\'',
        _ => {
            let value = name.strip_prefix("#x").map_or_else(
                || {
                    name.strip_prefix('#')
                        .and_then(|decimal| decimal.parse().ok())
                },
                |hex| u32::from_str_radix(hex, 16).ok(),
            );
            value
                .and_then(char::from_u32)
                .ok_or(QueryError::InvalidXml)?
        }
    };
    Ok(value)
}

#[cfg(test)]
mod tests;
