//! Checked byte splices preserve all working syntax outside selected incoming value subtrees.

use std::ops::Range;
use yaml_edit::{AsYaml, YamlFile, YamlNode};

/// Unsupported flow/block transitions or transport refuse before any candidate is written.
pub(super) fn splice(
    working: &str,
    incoming: &str,
    working_tree: &YamlFile,
    incoming_tree: &YamlFile,
    coordinates: &[Vec<String>],
) -> Option<String> {
    if [working, incoming].into_iter().any(mixed_transport) {
        return None;
    }
    let mut patches = Vec::new();
    for coordinate in coordinates {
        let old = range(working_tree, coordinate)?;
        let new = range(incoming_tree, coordinate)?;
        let replacement = reindent(
            working,
            incoming,
            &old,
            &new,
            block(incoming_tree, coordinate)?,
        )?;
        patches.push((old, replacement));
    }
    patches.sort_by_key(|(range, _)| range.start);
    if patches
        .windows(2)
        .any(|pair| pair[0].0.end > pair[1].0.start)
    {
        return None;
    }
    let mut output = working.to_owned();
    for (range, replacement) in patches.into_iter().rev() {
        output.replace_range(range, &replacement);
    }
    Some(output)
}

/// Mapping coordinates resolve actual nodes; no property text is searched or globally replaced.
fn at(tree: &YamlFile, coordinate: &[String]) -> Option<YamlNode> {
    let mut node = YamlNode::Mapping(tree.document()?.as_mapping()?);
    for key in coordinate {
        node = node.as_mapping()?.get(key)?;
    }
    Some(node)
}

/// Syntax ranges include a block collection's indentation but omit an inline key's separator.
fn range(tree: &YamlFile, coordinate: &[String]) -> Option<Range<usize>> {
    let range = at(tree, coordinate)?.as_node()?.text_range();
    Some(usize::from(range.start())..usize::from(range.end()))
}

/// A collection needs its own line when it replaces an inline empty sequence or mapping.
fn block(tree: &YamlFile, coordinate: &[String]) -> Option<bool> {
    let node = at(tree, coordinate)?;
    Some(
        node.as_mapping().is_some_and(|node| !node.is_flow_style())
            || node.as_sequence().is_some_and(|node| !node.is_flow_style()),
    )
}

/// Only indentation and newline transport change; quoted and literal scalar contents are rechecked.
fn reindent(
    working: &str,
    incoming: &str,
    old: &Range<usize>,
    new: &Range<usize>,
    block: bool,
) -> Option<String> {
    let original = working.get(old.clone())?;
    let text = incoming.get(new.clone())?.replace("\r\n", "\n");
    let mut replacement = if block {
        collection(working, incoming, old, new, &text)?
    } else {
        value(working, incoming, old, new, &text)?
    };
    super::super::super::super::yaml::preserve_transport(original, &mut replacement);
    if working.contains("\r\n") {
        replacement = replacement.replace("\r\n", "\n").replace('\n', "\r\n");
    }
    Some(replacement)
}

/// Non-collection values stay beside their original key, including literal scalar indicators.
fn value(
    working: &str,
    incoming: &str,
    old: &Range<usize>,
    new: &Range<usize>,
    text: &str,
) -> Option<String> {
    let from = indent(incoming, new.start)?;
    let to = indent(working, old.start)?;
    let mut lines = text.split('\n');
    let leading = working
        .get(old.clone())?
        .bytes()
        .take_while(|byte| *byte == b' ')
        .count();
    let mut replacement = " ".repeat(leading);
    replacement.push_str(lines.next()?.trim_start_matches(' '));
    for line in lines {
        replacement.push('\n');
        if line.is_empty() {
            continue;
        }
        replacement.push_str(&" ".repeat(to));
        replacement.push_str(line.strip_prefix(&" ".repeat(from))?);
    }
    Some(replacement)
}

/// Replacing `[]` with a block inserts a separator newline without rewriting its mapping key.
fn collection(
    working: &str,
    incoming: &str,
    old: &Range<usize>,
    new: &Range<usize>,
    text: &str,
) -> Option<String> {
    let prefix = working.get(..old.start)?.rsplit('\n').next()?;
    let on_line = prefix.bytes().all(|byte| byte == b' ');
    let to = if on_line {
        prefix.len()
            + working
                .get(old.clone())?
                .bytes()
                .take_while(|byte| *byte == b' ')
                .count()
    } else {
        indent(working, old.start)? + 2
    };
    let from = indent(incoming, new.start)? + text.bytes().take_while(|byte| *byte == b' ').count();
    let mut output = String::new();
    for (index, line) in text.split('\n').enumerate() {
        if index != 0 || !on_line {
            output.push('\n');
        }
        if line.is_empty() {
            continue;
        }
        let content = if index == 0 {
            line.trim_start_matches(' ')
        } else {
            line.strip_prefix(&" ".repeat(from))?
        };
        let indent = if index == 0 && on_line {
            to.checked_sub(prefix.len())?
        } else {
            to
        };
        output.push_str(&" ".repeat(indent));
        output.push_str(content);
    }
    Some(output)
}

/// The containing line establishes indentation even when a value follows its mapping key.
fn indent(text: &str, offset: usize) -> Option<usize> {
    let prefix = text.get(..offset)?;
    Some(
        prefix
            .rsplit('\n')
            .next()?
            .bytes()
            .take_while(|byte| *byte == b' ')
            .count(),
    )
}

/// A mixed document cannot establish a single transport for an inserted native subtree.
fn mixed_transport(text: &str) -> bool {
    text.contains("\r\n") && text.replace("\r\n", "").contains('\n')
}
