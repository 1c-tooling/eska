//! Move complete descriptor subtrees; loss of YAML comments or semantic bytes refuses the plan.

use std::collections::BTreeMap;

use yaml_edit::{AsYaml, SyntaxKind, YamlFile};

use crate::project_core::source_format::{
    Error, ManagedFormSource, NativeDocument, ObjectSource, SourcePath,
    yaml::{self, Step},
};

/// Block owners keep their exact existing prefix; flow owners use the checked concrete syntax insertion.
pub(super) fn attach_form(input: &str, form: &ManagedFormSource) -> Result<String, Error> {
    if input.contains("\r\n") && input.replace("\r\n", "").contains('\n') {
        return Err(unsupported());
    }
    let (value, tree) = yaml::decode_with_tree(input)?;
    let mut expected = ObjectSource::from_value(value)?;
    if expected.form.is_some() {
        return Err(unsupported());
    }
    expected.form = Some(form.clone());
    let body = form.to_yaml()?;
    let mapping = tree
        .document()
        .and_then(|document| document.as_mapping())
        .ok_or_else(unsupported)?;
    let output = if mapping.is_flow_style() {
        // A block CST inserted into a flow map is not reflowed by the library.
        // Its concrete closing delimiter anchors one JSON-compatible flow value.
        let close = mapping
            .as_node()
            .ok_or_else(unsupported)?
            .children_with_tokens()
            .find_map(|part| {
                part.into_token()
                    .filter(|token| token.kind() == SyntaxKind::RIGHT_BRACE)
            })
            .ok_or_else(unsupported)?;
        let value = serde_json::to_string(form).map_err(|_| unsupported())?;
        let mut output = input.to_owned();
        output.insert_str(
            usize::from(close.text_range().start()),
            &format!(", \"form\": {value}"),
        );
        output
    } else {
        let eol = if input.contains("\r\n") { "\r\n" } else { "\n" };
        let mut output = input.to_owned();
        if !output.ends_with('\n') {
            output.push_str(eol);
        }
        output.push_str("\"form\":");
        output.push_str(eol);
        for line in body.lines() {
            output.push_str("  ");
            output.push_str(line);
            output.push_str(eol);
        }
        output
    };
    if comments(input) != comments(&output) || ObjectSource::from_yaml(&output)? != expected {
        return Err(unsupported());
    }
    Ok(output)
}

/// Keep main-file trivia and each descriptor subtree without serializing authored property values.
pub(super) fn split(input: &str) -> Result<BTreeMap<SourcePath, String>, Error> {
    let (value, tree) = yaml::decode_with_tree(input)?;
    let source = ObjectSource::from_value(value)?;
    let mut result = BTreeMap::new();
    for (index, part) in source.nested.iter().enumerate() {
        let node =
            yaml::at(&tree, &[Step::Key("nested"), Step::Index(index)]).ok_or_else(unsupported)?;
        let child = skeleton()?;
        child
            .document()
            .and_then(|document| document.as_mapping())
            .ok_or_else(unsupported)?
            .set("document", node);
        let output = transport(input, &child.to_string());
        check(&output, part)?;
        result.insert(part.path.clone(), output);
    }
    let empty = skeleton()?;
    let empty = yaml::at(&empty, &[Step::Key("nested")]).ok_or_else(unsupported)?;
    tree.document()
        .and_then(|document| document.as_mapping())
        .ok_or_else(unsupported)?
        .set("nested", empty);
    let main = transport(input, &tree.to_string());
    check(&main, &source.document)?;
    result.insert(source.document.path, main);
    let mut retained = result
        .values()
        .flat_map(|text| comments(text))
        .collect::<Vec<_>>();
    retained.sort_unstable();
    if comments(input) != retained {
        return Err(unsupported());
    }
    Ok(result)
}

/// A new wrapper contributes no comments or descriptor values of its own.
fn skeleton() -> Result<YamlFile, Error> {
    yaml::decode_with_tree("document: {}\nnested: []\n").map(|(_, tree)| tree)
}

/// Manufactured wrapper tokens adopt the source transport EOL; values are checked afterwards.
fn transport(input: &str, output: &str) -> String {
    if input.contains("\r\n") {
        output.replace("\r\n", "\n").replace('\n', "\r\n")
    } else {
        output.into()
    }
}

/// Reindentation may not change XML fragments, literal values, identity or inline composition.
fn check(output: &str, expected: &NativeDocument) -> Result<(), Error> {
    let current = ObjectSource::from_yaml(output)?;
    if &current.document != expected || !current.nested.is_empty() {
        return Err(unsupported());
    }
    Ok(())
}

/// Compare exact comment tokens with multiplicity; a dropped or duplicated annotation is a failure.
fn comments(input: &str) -> Vec<&str> {
    let mut result = yaml_edit::lex(input)
        .into_iter()
        .filter_map(|(kind, text)| (kind == SyntaxKind::COMMENT).then_some(text))
        .collect::<Vec<_>>();
    result.sort_unstable();
    result
}

/// Unsupported trivia fails before any new output or accepted authority is claimed.
fn unsupported() -> Error {
    Error::root("source_layout_trivia_unsupported")
}
