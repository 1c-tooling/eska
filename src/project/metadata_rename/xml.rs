//! Namespace-aware XML reference analysis never treats arbitrary text as a metadata path.

use std::{collections::BTreeMap, ops::Range};

use roxmltree::{Document, Node};

use super::{NameError, RenameReplacement, UncertainReference, same_name, validate_name};
use crate::project::metadata_model::MetadataKind;

const MD: &str = "http://v8.1c.ru/8.3/MDClasses";
const XR: &str = "http://v8.1c.ru/8.3/xcf/readable";
const CORE: &str = "http://v8.1c.ru/8.1/data/core";
const CFG: &str = "http://v8.1c.ru/8.1/data/enterprise/current-config";
const XSI: &str = "http://www.w3.org/2001/XMLSchema-instance";
const DUMP: &str = "http://v8.1c.ru/8.3/xcf/dumpinfo";
const RIGHTS: &str = "http://v8.1c.ru/8.2/roles";

/// Immutable semantic substitutions derived from a resolved object and its generated types.
#[derive(Debug)]
pub struct ReferenceRename {
    old_path: String,
    new_path: String,
    old_name: String,
    new_name: String,
    generated: BTreeMap<String, String>,
}

/// Analysis is read-only; source publication must still verify the complete project snapshot.
#[derive(Debug, Default)]
pub struct XmlAnalysis {
    pub replacements: Vec<RenameReplacement>,
    pub uncertain: Vec<UncertainReference>,
}

impl ReferenceRename {
    /// Cheap lexical filtering precedes XML parsing; numeric entities require decoded inspection.
    pub(crate) fn may_mention(&self, input: &str) -> bool {
        input.contains("&#") || self.mentions_name(input)
    }

    /// Match whole identifiers using the same case rules as semantic path comparison.
    fn mentions_name(&self, value: &str) -> bool {
        value
            .split(|character: char| !character.is_alphanumeric() && character != '_')
            .any(|token| same_name(token, &self.old_name))
    }

    /// Construct semantic paths from resolved Designer tags, never from a filename guess.
    ///
    /// `generated` contains actual generated-type names read from the renamed object's subtree.
    ///
    /// # Errors
    /// Rejects empty ancestry or an invalid destination identifier.
    pub fn new(
        ancestry: &[(String, String)],
        new_name: &str,
        generated: &[String],
    ) -> Result<Self, NameError> {
        validate_name(new_name)?;
        let (_, old_name) = ancestry.last().ok_or(NameError::Empty)?;
        let old_path = ancestry
            .iter()
            .flat_map(|(kind, name)| [kind.as_str(), name.as_str()])
            .collect::<Vec<_>>()
            .join(".");
        let mut new_path = old_path[..old_path.len() - old_name.len()].to_owned();
        new_path.push_str(new_name);
        let generated = generated
            .iter()
            .filter_map(|name| {
                let mut parts: Vec<_> = name.split('.').collect();
                // Generated types omit kind tokens after the first family: CatalogTabularSection.A.B.
                if parts.len() <= ancestry.len()
                    || !ancestry
                        .iter()
                        .zip(parts.iter().skip(1))
                        .all(|((_, expected), actual)| same_name(expected, actual))
                {
                    return None;
                }
                parts[ancestry.len()] = new_name;
                Some((name.clone(), parts.join(".")))
            })
            .collect();
        Ok(Self {
            old_path,
            new_path,
            old_name: old_name.clone(),
            new_name: new_name.to_owned(),
            generated,
        })
    }

    /// Inspect text and attributes using their resolved namespaces and writer contracts.
    ///
    /// Declaration ranges must come from the parsed target/parent identities, not client input.
    /// They refer to element ranges of Properties/Name or the parent's `ChildObjects` declaration.
    ///
    /// # Errors
    /// Rejects unbounded, invalid XML and declaration ranges that no longer name the target.
    pub fn analyze_xml(
        &self,
        input: &str,
        declarations: &[Range<usize>],
    ) -> Result<XmlAnalysis, crate::project::metadata_edit::EditError> {
        use crate::project::metadata_edit::EditError;
        crate::project::metadata_parser::check_envelope(input)
            .map_err(|_| EditError::InvalidXml)?;
        let document = Document::parse_with_options(
            input,
            roxmltree::ParsingOptions {
                nodes_limit: 1_000_000,
                ..Default::default()
            },
        )
        .map_err(|_| EditError::InvalidXml)?;
        let mut result = XmlAnalysis::default();
        let mut seen = 0;
        for node in document.descendants().filter(Node::is_element) {
            if declarations.contains(&node.range()) {
                if node.children().any(|item| item.is_element())
                    || node.text() != Some(&self.old_name)
                    || node.children().filter(Node::is_text).count() != 1
                {
                    return Err(EditError::Conflict);
                }
                seen += 1;
                self.collect_text(input, node, Some(self.new_name.clone()), &mut result);
            } else {
                let replacement = self.text_reference(node);
                self.collect_text(input, node, replacement, &mut result);
            }
            for attribute in node.attributes() {
                let replacement = if attribute.namespace().is_none() && attribute.name() == "name" {
                    if node.has_tag_name((XR, "GeneratedType")) {
                        self.generated_reference(attribute.value())
                    } else if node.has_tag_name((DUMP, "Metadata")) {
                        self.path_reference(attribute.value())
                    } else {
                        None
                    }
                } else {
                    None
                };
                self.collect(
                    input,
                    attribute.range_value(),
                    attribute.value(),
                    replacement,
                    &mut result,
                );
            }
        }
        if seen != declarations.len() {
            return Err(EditError::Conflict);
        }
        result.replacements.sort_by_key(|change| change.range.start);
        let mut candidate = input.to_owned();
        for change in result.replacements.iter().rev() {
            candidate.replace_range(change.range.clone(), &change.after);
        }
        Document::parse(&candidate).map_err(|_| EditError::InvalidXml)?;
        Ok(result)
    }

    /// Replace a complete metadata prefix only at a dot boundary, including its descendants.
    fn path_reference(&self, value: &str) -> Option<String> {
        let segments = self.old_path.split('.').count();
        let mut end = value.len();
        for (count, (offset, _)) in value.match_indices('.').enumerate() {
            if count + 1 == segments {
                end = offset;
                break;
            }
        }
        same_name(&value[..end], &self.old_path)
            .then(|| format!("{}{}", self.new_path, &value[end..]))
    }

    /// Exact generated-type identities come from `InternalInfo`, not an invented family list.
    fn generated_reference(&self, value: &str) -> Option<String> {
        self.generated
            .iter()
            .find_map(|(before, after)| same_name(value, before).then(|| after.clone()))
    }

    /// Recognize typed references and reviewed unannotated metadata property domains.
    fn text_reference(&self, node: Node<'_, '_>) -> Option<String> {
        if node.children().any(|child| child.is_element()) {
            return None;
        }
        let value = node.text()?;
        if !self.mentions_name(value) {
            return None;
        }
        if node.has_tag_name((CORE, "Type")) || node.has_tag_name((CORE, "TypeSet")) {
            let (prefix, local) = value
                .split_once(':')
                .map_or((None, value), |(prefix, local)| (Some(prefix), local));
            if node.lookup_namespace_uri(prefix) == Some(CFG) {
                return self.generated_reference(local).map(|renamed| {
                    prefix.map_or_else(|| renamed.clone(), |prefix| format!("{prefix}:{renamed}"))
                });
            }
            return None;
        }
        if let Some(annotation) = node.attribute((XSI, "type")) {
            let (prefix, local) = annotation
                .split_once(':')
                .map_or((None, annotation), |(prefix, local)| (Some(prefix), local));
            if node.lookup_namespace_uri(prefix) == Some(XR)
                && matches!(local, "MDObjectRef" | "DesignTimeRef")
            {
                return self.path_reference(value);
            }
        }
        let role_object = node.has_tag_name((RIGHTS, "name"))
            && node
                .parent_element()
                .is_some_and(|parent| parent.has_tag_name((RIGHTS, "object")));
        if node.has_tag_name((XR, "DataPath")) || role_object || metadata_reference_property(node) {
            return self.path_reference(value);
        }
        None
    }

    /// Mixed text/comment nodes stay uncertain because one replacement cannot preserve their structure.
    fn collect_text(
        &self,
        input: &str,
        node: Node<'_, '_>,
        replacement: Option<String>,
        result: &mut XmlAnalysis,
    ) {
        let mut texts = node.children().filter(Node::is_text);
        let Some(_) = texts.next() else {
            return;
        };
        let replacement = if texts.next().is_none() {
            replacement
        } else {
            None
        };
        for text in node.children().filter(Node::is_text) {
            self.collect(
                input,
                text.range(),
                text.text().unwrap_or_default(),
                replacement.clone(),
                result,
            );
        }
    }

    /// Preserve all bytes outside the selected field; unproven matches never enter replacements.
    fn collect(
        &self,
        input: &str,
        range: Range<usize>,
        value: &str,
        replacement: Option<String>,
        result: &mut XmlAnalysis,
    ) {
        if let Some(after) = replacement {
            if value != after {
                result.replacements.push(RenameReplacement {
                    before: input[range.clone()].to_owned(),
                    after: after
                        .replace('&', "&amp;")
                        .replace('<', "&lt;")
                        .replace('>', "&gt;")
                        .replace('"', "&quot;")
                        .replace('\'', "&apos;"),
                    range,
                });
            }
        } else if self.mentions_name(value) {
            result.uncertain.push(UncertainReference {
                range,
                text: value.to_owned(),
                reason: "unverified_xml_context",
            });
        }
    }
}

/// Unannotated fields need their actual class/domain binding; matching a property name alone is unsafe.
fn metadata_reference_property(node: Node<'_, '_>) -> bool {
    let Some(properties) = node
        .parent()
        .filter(|parent| parent.has_tag_name((MD, "Properties")))
    else {
        return false;
    };
    let Some(object) = properties
        .parent()
        .filter(|object| object.tag_name().namespace() == Some(MD))
    else {
        return false;
    };
    let class = object.tag_name().name();
    if node.tag_name().namespace() == Some(MD)
        && matches!(
            (class, node.tag_name().name()),
            ("EventSubscription", "Handler") | ("ScheduledJob", "MethodName")
        )
    {
        return true;
    }
    let owner = object
        .parent_element()
        .filter(|parent| parent.has_tag_name((MD, "ChildObjects")))
        .and_then(|parent| parent.parent_element());
    let class = if matches!(
        class,
        "Attribute" | "Dimension" | "Resource" | "Form" | "Command" | "TabularSection"
    ) {
        format!(
            "{}{class}",
            owner.map_or("", |parent| parent.tag_name().name())
        )
    } else {
        class.to_owned()
    };
    include_str!("../metadata_edit/fields.tsv")
        .lines()
        .skip(1)
        .any(|line| {
            let columns: Vec<_> = line.split('\t').collect();
            columns.len() == 6
                && columns[0] == class
                && Some(columns[1]) == node.tag_name().namespace()
                && columns[2] == node.tag_name().name()
                && (MetadataKind::from_xml_tag(columns[3]).is_ok()
                    || crate::project::metadata_edit::references::target(columns[3]).is_some())
        })
}

#[cfg(test)]
mod tests;
