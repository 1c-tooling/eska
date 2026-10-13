//! Syntax-only access to the authoritative version literal of an external object.

use std::ops::Range;

use tree_sitter::{Node, Parser};

use super::native_version::ProjectVersion;

/// Bound parser allocations independently of the surrounding metadata project.
pub const MAX_MODULE_BYTES: usize = 16 * 1024 * 1024;

/// An exact version value and original UTF-8 byte ranges, including any leading BOM.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ModuleVersion {
    pub version: ProjectVersion,
    pub function_range: Range<usize>,
    /// Only the numeric contents; the BSL string delimiters remain unchanged.
    pub value_range: Range<usize>,
}

/// A portable reason for refusing an unconfirmed external-version contract.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ContractErrorKind {
    ModuleTooLarge,
    ParserUnavailable,
    InvalidSyntax,
    MissingFunction,
    AmbiguousFunction,
    ExpectedFunction,
    NotExported,
    Parameters,
    AsyncFunction,
    AnnotatedFunction,
    ConditionalFunction,
    InvalidBody,
    InvalidVersion,
}

impl ContractErrorKind {
    /// Stable identifiers are independent of parser wording and presentation locale.
    #[must_use]
    pub const fn code(self) -> &'static str {
        match self {
            Self::ModuleTooLarge => "version_module_too_large",
            Self::ParserUnavailable => "version_module_parser_unavailable",
            Self::InvalidSyntax => "version_module_invalid_syntax",
            Self::MissingFunction => "version_function_missing",
            Self::AmbiguousFunction => "version_function_ambiguous",
            Self::ExpectedFunction => "version_function_expected",
            Self::NotExported => "version_function_not_exported",
            Self::Parameters => "version_function_parameters",
            Self::AsyncFunction => "version_function_async",
            Self::AnnotatedFunction => "version_function_annotated",
            Self::ConditionalFunction => "version_function_conditional",
            Self::InvalidBody => "version_function_invalid_body",
            Self::InvalidVersion => "version_function_invalid_version",
        }
    }
}

/// A diagnostic range always refers to the supplied text, rather than a normalized copy.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ContractError {
    pub kind: ContractErrorKind,
    pub range: Range<usize>,
}

/// Read one unconditional exported `Версия()` with a single string-literal return.
///
/// Only syntax is interpreted. Comments, strings and called members cannot declare the
/// function. Ordinary region markers are allowed; conditional and insertion scopes are not.
/// No execution, semantic index, source mutation or filesystem access occurs.
///
/// # Errors
/// Returns an original byte range for invalid syntax, an ambiguous declaration, an
/// unsupported signature/body or an invalid three/four-part version number.
pub fn inspect(input: &str) -> Result<ModuleVersion, ContractError> {
    if input.len() > MAX_MODULE_BYTES {
        return Err(ContractError {
            kind: ContractErrorKind::ModuleTooLarge,
            range: 0..input.len(),
        });
    }
    let text = input.strip_prefix('\u{feff}').unwrap_or(input);
    let offset = input.len() - text.len();
    let mut parser = Parser::new();
    parser
        .set_language(&tree_sitter_bsl::LANGUAGE.into())
        .map_err(|_| ContractError {
            kind: ContractErrorKind::ParserUnavailable,
            range: 0..input.len(),
        })?;
    let tree = parser.parse(text, None).ok_or(ContractError {
        kind: ContractErrorKind::ParserUnavailable,
        range: 0..input.len(),
    })?;
    let nodes = descendants(tree.root_node());
    if let Some(error) = nodes
        .iter()
        .find(|node| node.is_error() || node.is_missing())
    {
        return Err(error_at(ContractErrorKind::InvalidSyntax, *error, offset));
    }
    let mut declarations = nodes.iter().copied().filter(|node| {
        matches!(node.kind(), "function_definition" | "procedure_definition")
            && node
                .child_by_field_name("name")
                .is_some_and(|name| text[name.byte_range()].to_lowercase() == "версия")
    });
    let function = declarations.next().ok_or(ContractError {
        kind: ContractErrorKind::MissingFunction,
        range: input.len()..input.len(),
    })?;
    if let Some(other) = declarations.next() {
        return Err(error_at(
            ContractErrorKind::AmbiguousFunction,
            other,
            offset,
        ));
    }
    if function.kind() != "function_definition" {
        return Err(error_at(
            ContractErrorKind::ExpectedFunction,
            function,
            offset,
        ));
    }
    validate_signature(function, &nodes, text, offset)?;
    read_literal(function, input, offset)
}

/// Walk without recursive Rust calls, keeping parser child order and original node ranges.
fn descendants(root: Node<'_>) -> Vec<Node<'_>> {
    let mut cursor = root.walk();
    let mut result = Vec::new();
    loop {
        result.push(cursor.node());
        if cursor.goto_first_child() {
            continue;
        }
        while !cursor.goto_next_sibling() {
            if !cursor.goto_parent() {
                return result;
            }
        }
    }
}

/// Inspect structural children, excluding comments and other parser extras.
fn named_children(node: Node<'_>) -> Vec<Node<'_>> {
    let mut cursor = node.walk();
    node.named_children(&mut cursor)
        .filter(|child| !child.is_extra())
        .collect()
}

/// Regions group code; conditions and insertion/deletion markers change its availability.
fn conditional_scope(function: Node<'_>, nodes: &[Node<'_>], text: &str) -> bool {
    let mut ancestor = function.parent();
    while let Some(node) = ancestor {
        if node.kind() == "preprocessor"
            && named_children(node)
                .iter()
                .any(|child| child.kind() == "PREPROC_IF_KEYWORD")
        {
            return true;
        }
        ancestor = node.parent();
    }
    let mut depth = 0_usize;
    for node in nodes.iter().filter(|node| node.kind() == "preproc") {
        if node.start_byte() >= function.start_byte() {
            break;
        }
        match text[node.byte_range()].to_lowercase().as_str() {
            "#вставка" | "#insert" | "#удаление" | "#delete" => depth += 1,
            "#конецвставки" | "#endinsert" | "#конецудаления" | "#enddelete" =>
            {
                depth = depth.saturating_sub(1);
            }
            _ => {}
        }
    }
    depth > 0
}

/// Compiler annotations are siblings of their declaration in the published BSL grammar.
fn has_annotation(function: Node<'_>) -> bool {
    let mut previous = function.prev_named_sibling();
    while let Some(node) = previous {
        if node.is_extra() {
            previous = node.prev_named_sibling();
            continue;
        }
        return node.kind() == "preprocessor"
            && named_children(node)
                .iter()
                .any(|child| child.kind() == "annotation");
    }
    false
}

/// Refuse signatures whose callable form or availability is not proven by the contract.
fn validate_signature(
    function: Node<'_>,
    nodes: &[Node<'_>],
    text: &str,
    offset: usize,
) -> Result<(), ContractError> {
    let violation = if conditional_scope(function, nodes, text) {
        Some(ContractErrorKind::ConditionalFunction)
    } else if has_annotation(function) {
        Some(ContractErrorKind::AnnotatedFunction)
    } else if named_children(function)
        .iter()
        .any(|node| node.kind() == "ASYNC_KEYWORD")
    {
        Some(ContractErrorKind::AsyncFunction)
    } else if function
        .child_by_field_name("parameters")
        .is_none_or(|parameters| {
            named_children(parameters)
                .iter()
                .any(|node| node.kind() == "parameter")
        })
    {
        Some(ContractErrorKind::Parameters)
    } else if function.child_by_field_name("export").is_none() {
        Some(ContractErrorKind::NotExported)
    } else {
        None
    };
    violation.map_or(Ok(()), |kind| Err(error_at(kind, function, offset)))
}

/// The only allowed body statement is a return of one unparenthesized string literal.
fn read_literal(
    function: Node<'_>,
    input: &str,
    offset: usize,
) -> Result<ModuleVersion, ContractError> {
    let invalid = || error_at(ContractErrorKind::InvalidBody, function, offset);
    let statements: Vec<_> = named_children(function)
        .into_iter()
        .filter(|node| {
            !matches!(
                node.kind(),
                "FUNCTION_KEYWORD"
                    | "ENDFUNCTION_KEYWORD"
                    | "EXPORT_KEYWORD"
                    | "parameters"
                    | "identifier"
            )
        })
        .collect();
    let [statement] = statements.as_slice() else {
        return Err(invalid());
    };
    if statement.kind() != "return_statement" {
        return Err(invalid());
    }
    let mut literal = statement
        .child_by_field_name("result")
        .ok_or_else(invalid)?;
    while matches!(literal.kind(), "expression" | "const_expression") {
        let children = named_children(literal);
        let [child] = children.as_slice() else {
            return Err(invalid());
        };
        // An expression wrapper around parentheses is not the managed literal form.
        if literal.start_byte() != child.start_byte() || literal.end_byte() != child.end_byte() {
            return Err(invalid());
        }
        literal = *child;
    }
    if literal.kind() != "string" {
        return Err(invalid());
    }
    let literal_range = literal.start_byte() + offset..literal.end_byte() + offset;
    let raw = &input[literal_range.clone()];
    let value = raw
        .strip_prefix('"')
        .and_then(|value| value.strip_suffix('"'))
        .ok_or_else(invalid)?;
    let value_range = literal_range.start + 1..literal_range.end - 1;
    let version = ProjectVersion::parse(value).map_err(|_| ContractError {
        kind: ContractErrorKind::InvalidVersion,
        range: value_range.clone(),
    })?;
    Ok(ModuleVersion {
        version,
        function_range: function.start_byte() + offset..function.end_byte() + offset,
        value_range,
    })
}

/// Retain portable reasons and exact original byte offsets instead of parser human messages.
fn error_at(kind: ContractErrorKind, node: Node<'_>, offset: usize) -> ContractError {
    ContractError {
        kind,
        range: node.start_byte() + offset..node.end_byte() + offset,
    }
}

#[cfg(test)]
mod tests;
