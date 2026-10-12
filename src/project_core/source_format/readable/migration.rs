//! Migration annotations follow concrete legacy fields, never a coincidental scalar spelling.

use std::{collections::BTreeMap, ops::Range};

use yaml_edit::{AsYaml, SyntaxKind, YamlFile, YamlNode};

use super::super::{
    Child, Error, MetadataNode, ObjectSource, Property, PropertyValue, Section, document_paths,
    yaml::{self, Step},
};

mod types;

/// A field has a concrete new address; unchanged value subtrees can retain their authored syntax.
struct Binding {
    before: Vec<Step>,
    after: Vec<Step>,
    copy: bool,
}

/// Resolve ranges once; several comments on a property do not repeatedly walk its CST path.
struct Target {
    source: Range<usize>,
    offset: usize,
}

/// Keep value syntax where the wire shape agrees, then attach remaining annotations to their field.
pub(super) fn transfer(
    input: &str,
    before: &ObjectSource,
    expected: &ObjectSource,
    canonical: &str,
) -> Result<String, Error> {
    if input.contains("\r\n") && input.replace("\r\n", "").contains('\n') {
        return Err(unsupported());
    }
    let (_, original) = yaml::decode_with_tree(input)?;
    let (_, updated) = yaml::decode_with_tree(canonical)?;
    let bindings = bindings(before);
    let mut copied = Vec::new();
    for binding in &bindings {
        if binding.copy {
            let node = yaml::at(&original, &binding.before).ok_or_else(unsupported)?;
            copied.push(span(&node)?);
            copy(&updated, &binding.after, node)?;
        }
    }
    let output = transport(input, &updated.to_string());
    verify(&output, expected)?;
    let (_, updated) = yaml::decode_with_tree(&output)?;
    let targets = targets(&bindings, &original, &updated, &output)?;
    let mut insertions = BTreeMap::<usize, String>::new();
    let content = content_span(input)?;
    for comment in original.comments() {
        let range = comment.text_range();
        let range = usize::from(range.start())..usize::from(range.end());
        if copied.iter().any(|copied| encloses(copied, &range)) {
            continue;
        }
        let target = targets
            .iter()
            .filter(|target| encloses(&target.source, &range))
            .min_by_key(|target| target.source.len());
        let offset = if range.end <= content.start {
            0
        } else if range.start >= content.end
            && input[line_start(input, range.start)?..range.start]
                .trim()
                .is_empty()
        {
            output.len()
        } else if let Some(target) = target {
            target.offset
        } else {
            return Err(unsupported());
        };
        let indent = output
            .get(offset..)
            .ok_or_else(unsupported)?
            .bytes()
            .take_while(|byte| *byte == b' ')
            .count();
        let annotation = insertions.entry(offset).or_default();
        if offset == output.len() && !output.ends_with('\n') && annotation.is_empty() {
            annotation.push('\n');
        }
        annotation.push_str(&" ".repeat(indent));
        annotation.push_str(comment.text());
        annotation.push('\n');
    }
    let mut annotated = output;
    for (offset, annotation) in insertions.into_iter().rev() {
        annotated.insert_str(offset, &transport(input, &annotation));
    }
    if comments(input) != comments(&annotated) {
        return Err(unsupported());
    }
    verify(&annotated, expected)?;
    Ok(annotated)
}

/// A factory must not change literal content or publish an invalid manufactured YAML tree.
fn verify(output: &str, expected: &ObjectSource) -> Result<(), Error> {
    if ObjectSource::from_yaml(output).map_err(|_| unsupported())? != *expected {
        return Err(unsupported());
    }
    Ok(())
}

/// Every known entry maps to a concrete new key or child line, including removed wrappers.
fn targets(
    bindings: &[Binding],
    original: &YamlFile,
    updated: &YamlFile,
    output: &str,
) -> Result<Vec<Target>, Error> {
    bindings
        .iter()
        .map(|binding| {
            let before = yaml::at(original, &binding.before).ok_or_else(unsupported)?;
            let after = yaml::at(updated, &binding.after).ok_or_else(unsupported)?;
            Ok(Target {
                source: entry_span(&before).ok_or_else(unsupported)?,
                offset: line_start(output, entry_span(&after).ok_or_else(unsupported)?.start)?,
            })
        })
        .collect()
}

/// File header/footer comments stay outside metadata even if the CST includes them in a mapping.
fn content_span(input: &str) -> Result<Range<usize>, Error> {
    let mut offset = 0;
    let mut start = None;
    let mut end = 0;
    for (kind, text) in yaml_edit::lex(input) {
        let next = offset + text.len();
        if !matches!(
            kind,
            SyntaxKind::WHITESPACE
                | SyntaxKind::NEWLINE
                | SyntaxKind::INDENT
                | SyntaxKind::COMMENT
                | SyntaxKind::BOM
                | SyntaxKind::EOF
                | SyntaxKind::DOC_START
                | SyntaxKind::DOC_END
        ) {
            start.get_or_insert(offset);
            end = next;
        }
        offset = next;
    }
    Ok(start.ok_or_else(unsupported)?..end)
}

/// Only schema coordinates are rewritten; declarations and preserved/typed classification agree.
fn bindings(source: &ObjectSource) -> Vec<Binding> {
    let mut result = Vec::new();
    add(&mut result, vec![], vec![], false);
    add(
        &mut result,
        vec![Step::Key("nested")],
        vec![Step::Key("nested")],
        false,
    );
    for (index, part) in std::iter::once(&source.document)
        .chain(&source.nested)
        .enumerate()
    {
        let mut root = document_paths::root_steps(index);
        root.pop();
        let wrapper = &root[..root.len() - 1];
        add(&mut result, wrapper.to_vec(), wrapper.to_vec(), false);
        if part.descriptor.schema_version == super::super::READABLE_SCHEMA_VERSION {
            add(&mut result, root.clone(), root, true);
            continue;
        }
        add(&mut result, root.clone(), root.clone(), false);
        for key in ["schema_version", "rules_version", "profile"] {
            let path = append(&root, [Step::Key(key)]);
            add(&mut result, path.clone(), path, key != "schema_version");
        }
        add(
            &mut result,
            append(&root, [Step::Key("xml")]),
            append(&root, [Step::Key("native_xml")]),
            true,
        );
        root.push(Step::Key("object"));
        node_bindings(&mut result, &part.descriptor.object, &root, &root);
        let mut wrapper = root;
        wrapper.truncate(wrapper.len() - 2);
        add(
            &mut result,
            append(&wrapper, [Step::Key("path")]),
            append(&wrapper, [Step::Key("path")]),
            true,
        );
    }
    result
}

/// Native attributes/namespaces and identities retain complete source values and annotations.
fn node_bindings(result: &mut Vec<Binding>, node: &MetadataNode, before: &[Step], after: &[Step]) {
    add(result, before.to_vec(), after.to_vec(), false);
    for key in ["kind", "uuid"] {
        add(
            result,
            append(before, [Step::Key(key)]),
            append(after, [Step::Key(key)]),
            true,
        );
    }
    for key in ["namespaces", "attributes"] {
        add(
            result,
            append(before, [Step::Key(key)]),
            append(after, [Step::Key("native"), Step::Key(key)]),
            true,
        );
    }
    add(
        result,
        append(before, [Step::Key("sections")]),
        append(after, [Step::Key("native"), Step::Key("sections")]),
        false,
    );
    for (index, section) in node.sections.iter().enumerate() {
        let old = append(before, [Step::Key("sections"), Step::Index(index)]);
        let new = append(
            after,
            [
                Step::Key("native"),
                Step::Key("sections"),
                Step::Index(index),
            ],
        );
        add(result, old.clone(), new.clone(), false);
        add(
            result,
            append(&old, [Step::Key("kind")]),
            append(&new, [Step::Key("kind")]),
            true,
        );
        match section {
            Section::Properties(items) => property_bindings(result, items, &old, &new, after),
            Section::Children(items) => child_bindings(result, items, &old, after),
            Section::Preserved(_) => add(
                result,
                append(&old, [Step::Key("items")]),
                append(&new, [Step::Key("items")]),
                true,
            ),
        }
    }
}

/// Scalar wrappers lose no annotations; their surviving leaf keeps quoting and localized order.
fn property_bindings(
    result: &mut Vec<Binding>,
    properties: &[Property],
    old: &[Step],
    new: &[Step],
    owner: &[Step],
) {
    add(
        result,
        append(old, [Step::Key("items")]),
        append(owner, [Step::Key("properties")]),
        false,
    );
    for (index, property) in properties.iter().enumerate() {
        let before = append(old, [Step::Key("items"), Step::Index(index)]);
        match property {
            Property::Typed { name, value } => {
                let after = append(owner, [Step::Key("properties"), Step::Key(name.as_str())]);
                add(result, before.clone(), after.clone(), false);
                let value_path = append(&before, [Step::Key("value")]);
                if let PropertyValue::TypeDescription(value) = value {
                    let content = append(&value_path, [Step::Key("value")]);
                    add(result, value_path, after.clone(), false);
                    add(result, content.clone(), after.clone(), false);
                    types::data(
                        result,
                        &value.data,
                        &append(&content, [Step::Key("data")]),
                        &after,
                    );
                    types::layout(
                        result,
                        &value.native,
                        &append(&content, [Step::Key("native")]),
                        &append(
                            new,
                            [Step::Key("items"), Step::Index(index), Step::Key("layout")],
                        ),
                    );
                    continue;
                }
                if let PropertyValue::ReferenceList(value) = value {
                    let content = append(&value_path, [Step::Key("value")]);
                    add(result, value_path, after.clone(), false);
                    add(result, content.clone(), after.clone(), false);
                    let items = append(&content, [Step::Key("items")]);
                    add(result, items.clone(), after.clone(), false);
                    for index in 0..value.items.len() {
                        add(
                            result,
                            append(&items, [Step::Index(index)]),
                            append(&after, [Step::Index(index)]),
                            true,
                        );
                    }
                    let old_native = append(&content, [Step::Key("native")]);
                    let new_native = append(
                        new,
                        [
                            Step::Key("items"),
                            Step::Index(index),
                            Step::Key("references"),
                        ],
                    );
                    add(result, old_native.clone(), new_native.clone(), false);
                    for key in ["item_prefix", "xsi_prefix", "type_prefix"] {
                        add(
                            result,
                            append(&old_native, [Step::Key(key)]),
                            append(&new_native, [Step::Key(key)]),
                            true,
                        );
                    }
                    continue;
                }
                let leaf = match value {
                    PropertyValue::Empty | PropertyValue::Nil => value_path,
                    _ => append(&value_path, [Step::Key("value")]),
                };
                add(result, leaf, after, true);
            }
            Property::Preserved { .. } => {
                let after = append(new, [Step::Key("items"), Step::Index(index)]);
                add(result, before.clone(), after.clone(), false);
                add(
                    result,
                    append(&before, [Step::Key("fragment")]),
                    append(&after, [Step::Key("fragment")]),
                    true,
                );
            }
        }
    }
}

/// Inline recursion uses original identity and order; reference and opaque child shapes stay intact.
fn child_bindings(result: &mut Vec<Binding>, children: &[Child], old: &[Step], owner: &[Step]) {
    let before = append(old, [Step::Key("items")]);
    let after = append(owner, [Step::Key("children")]);
    add(result, before.clone(), after.clone(), false);
    for (index, child) in children.iter().enumerate() {
        let before = append(&before, [Step::Index(index)]);
        let after = append(&after, [Step::Index(index)]);
        add(result, before.clone(), after.clone(), false);
        if let Child::Inline { object } = child {
            add(
                result,
                append(&before, [Step::Key("kind")]),
                append(&after, [Step::Key("kind")]),
                true,
            );
            node_bindings(
                result,
                object,
                &append(&before, [Step::Key("object")]),
                &append(&after, [Step::Key("object")]),
            );
        } else {
            let keys: &[&'static str] = if matches!(child, Child::Reference { .. }) {
                &["kind", "metadata_kind", "name"]
            } else {
                &["kind", "fragment"]
            };
            for key in keys {
                add(
                    result,
                    append(&before, [Step::Key(key)]),
                    append(&after, [Step::Key(key)]),
                    true,
                );
            }
        }
    }
}

/// Concrete schema steps remain reusable without a general migration or plugin abstraction.
fn append(path: &[Step], suffix: impl IntoIterator<Item = Step>) -> Vec<Step> {
    path.iter().cloned().chain(suffix).collect()
}

/// Register a single concrete source/destination field relation before mutating the output tree.
fn add(result: &mut Vec<Binding>, before: Vec<Step>, after: Vec<Step>, copy: bool) {
    result.push(Binding {
        before,
        after,
        copy,
    });
}

/// Factory reindentation is accepted only when a full model comparison proves exact values.
fn copy(tree: &YamlFile, steps: &[Step], value: YamlNode) -> Result<(), Error> {
    let Some((step, parent)) = steps.split_last() else {
        return Err(unsupported());
    };
    let parent = yaml::at(tree, parent).ok_or_else(unsupported)?;
    match step {
        Step::Key(key) => parent
            .as_mapping()
            .ok_or_else(unsupported)?
            .set(*key, value),
        Step::Index(index) => {
            let scalar = value.as_scalar().ok_or_else(unsupported)?;
            if !parent
                .as_sequence()
                .ok_or_else(unsupported)?
                .set(*index, yaml_edit::ScalarValue::from_scalar(scalar))
            {
                return Err(unsupported());
            }
        }
    }
    Ok(())
}

/// Inline and leading field comments use the same semantic entry before conversion removes wrappers.
fn entry_span(node: &YamlNode) -> Option<Range<usize>> {
    let node = node.as_node()?;
    let entry = node.ancestors().find(|parent| {
        matches!(
            parent.kind(),
            SyntaxKind::MAPPING_ENTRY | SyntaxKind::SEQUENCE_ENTRY
        )
    });
    let range = entry.as_ref().unwrap_or(node).text_range();
    Some(usize::from(range.start())..usize::from(range.end()))
}

/// Copy accounting includes only tokens in the value, not annotations on its removed wrapper.
fn span(node: &YamlNode) -> Result<Range<usize>, Error> {
    let range = node.as_node().ok_or_else(unsupported)?.text_range();
    Ok(usize::from(range.start())..usize::from(range.end()))
}

/// Complete containment prevents assigning a comment token to two adjacent fields.
const fn encloses(outer: &Range<usize>, inner: &Range<usize>) -> bool {
    outer.start <= inner.start && inner.end <= outer.end
}

/// A leading annotation is inserted before the key/dash line, with that line's indentation.
fn line_start(text: &str, offset: usize) -> Result<usize, Error> {
    Ok(text
        .get(..offset)
        .ok_or_else(unsupported)?
        .rfind('\n')
        .map_or(0, |index| index + 1))
}

/// Compare exact tokens with multiplicity; quotes/literal '#' are not annotations.
fn comments(input: &str) -> Vec<&str> {
    let mut result = yaml_edit::lex(input)
        .into_iter()
        .filter_map(|(kind, text)| (kind == SyntaxKind::COMMENT).then_some(text))
        .collect::<Vec<_>>();
    result.sort_unstable();
    result
}

/// Manufactured tokens adopt one transport convention without touching scalar escape spellings.
fn transport(input: &str, text: &str) -> String {
    if input.contains("\r\n") {
        text.replace("\r\n", "\n").replace('\n', "\r\n")
    } else {
        text.into()
    }
}

/// Ambiguous associations or unsafe reindentation fail before any publication.
fn unsupported() -> Error {
    Error::root("source_layout_trivia_unsupported")
}
