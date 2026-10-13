//! A deliberately narrow proof of the returned BSP structure's version field.

use std::collections::BTreeSet;

use super::{
    ContractError, ContractErrorKind, Node, descendants, error_at, function_statements,
    has_annotation, named_children, unwrapped,
};

/// Return the sole version expression of a recognized registration, or explicit absence.
pub(super) fn version_expression<'a>(
    nodes: &[Node<'a>],
    text: &str,
    offset: usize,
) -> Result<Option<Node<'a>>, ContractError> {
    let mut declarations = nodes.iter().copied().filter(|node| {
        matches!(node.kind(), "function_definition" | "procedure_definition")
            && node.child_by_field_name("name").is_some_and(|name| {
                text[name.byte_range()].to_lowercase() == "сведенияовнешнейобработке"
            })
    });
    let Some(function) = declarations.next() else {
        return Ok(None);
    };
    if let Some(other) = declarations.next() {
        return Err(error_at(
            ContractErrorKind::RegistrationAmbiguous,
            other,
            offset,
        ));
    }
    let returned = registration_target(function, nodes, text, offset)?;
    registered_value(function, returned, text, offset).map(Some)
}

/// Establish a single returned local structure before examining its writes.
fn registration_target<'a>(
    function: Node<'a>,
    nodes: &[Node<'a>],
    text: &str,
    offset: usize,
) -> Result<Node<'a>, ContractError> {
    let invalid = |node| error_at(ContractErrorKind::RegistrationUnsupported, node, offset);
    if function.kind() != "function_definition"
        || function.child_by_field_name("export").is_none()
        || has_annotation(function)
        || named_children(function)
            .iter()
            .any(|child| child.kind() == "ASYNC_KEYWORD")
        || function
            .child_by_field_name("parameters")
            .is_none_or(|parameters| {
                named_children(parameters)
                    .iter()
                    .any(|child| child.kind() == "parameter")
            })
    {
        return Err(invalid(function));
    }
    let statements = function_statements(function);
    let Some(last) = statements.last() else {
        return Err(invalid(function));
    };
    let Some(result) = last.child_by_field_name("result") else {
        return Err(invalid(*last));
    };
    let returned = unwrapped(result);
    if last.kind() != "return_statement" || returned.kind() != "identifier" {
        return Err(invalid(*last));
    }
    let target = text[returned.byte_range()].to_lowercase();
    let function_nodes = descendants(function);
    if function_nodes
        .iter()
        .filter(|node| node.kind() == "return_statement")
        .count()
        != 1
        || function_nodes
            .iter()
            .any(|node| node.kind() == "execute_statement")
        || shadows_version(&function_nodes, text)
        || nodes
            .iter()
            .filter(|node| node.kind() == "var_definition" && !inside_function(**node))
            .flat_map(|node| descendants(*node))
            .any(|node| {
                node.kind() == "identifier" && text[node.byte_range()].to_lowercase() == "версия"
                    || node.kind() == "identifier"
                        && text[node.byte_range()].to_lowercase() == target
            })
    {
        return Err(invalid(function));
    }
    Ok(returned)
}

/// Whitelist the constructor and direct writes, rejecting every unaccounted target reference.
fn registered_value<'a>(
    function: Node<'a>,
    returned: Node<'a>,
    text: &str,
    offset: usize,
) -> Result<Node<'a>, ContractError> {
    let invalid = |node| error_at(ContractErrorKind::RegistrationUnsupported, node, offset);
    let statements = function_statements(function);
    let target = text[returned.byte_range()].to_lowercase();
    let function_nodes = descendants(function);
    let mut initialized_at = None;
    let mut allowed = BTreeSet::from([returned.id()]);
    let mut version = None;
    for statement in &statements[..statements.len() - 1] {
        if statement.kind() == "assignment_statement" {
            let left = statement
                .child_by_field_name("left")
                .ok_or_else(|| invalid(*statement))?;
            let right = statement
                .child_by_field_name("right")
                .ok_or_else(|| invalid(*statement))?;
            if left.kind() == "identifier" && text[left.byte_range()].to_lowercase() == target {
                if initialized_at.is_some() || !is_constructor(right, text) {
                    return Err(invalid(*statement));
                }
                initialized_at = Some(statement.start_byte());
                allowed.insert(left.id());
            } else if let Some((receiver, field)) = member(left)
                && text[receiver.byte_range()].to_lowercase() == target
            {
                if initialized_at.is_none() {
                    return Err(invalid(*statement));
                }
                allowed.insert(receiver.id());
                if text[field.byte_range()].to_lowercase() == "версия"
                    && version.replace(right).is_some()
                {
                    return Err(invalid(*statement));
                }
            }
        } else if statement.kind() == "call_statement" {
            let children = named_children(*statement);
            if let [call] = children.as_slice()
                && let Some((receiver, method)) = member(*call)
                && text[receiver.byte_range()].to_lowercase() == target
            {
                let arguments = method
                    .child_by_field_name("arguments")
                    .map(named_children)
                    .unwrap_or_default();
                let [key, value] = arguments.as_slice() else {
                    return Err(invalid(*statement));
                };
                let key = literal_text(*key, text).ok_or_else(|| invalid(*statement))?;
                let name = method
                    .child_by_field_name("name")
                    .ok_or_else(|| invalid(*statement))?;
                if initialized_at.is_none()
                    || !matches!(
                        text[name.byte_range()].to_lowercase().as_str(),
                        "вставить" | "insert"
                    )
                {
                    return Err(invalid(*statement));
                }
                allowed.insert(receiver.id());
                if key.to_lowercase() == "версия" && version.replace(*value).is_some() {
                    return Err(invalid(*statement));
                }
            }
        }
    }
    // A returned structure cannot escape through an alias, argument or unknown mutator.
    // Writes inside a conditional or loop have no whitelist entry either.
    for node in function_nodes.iter().filter(|node| {
        node.kind() == "identifier" && text[node.byte_range()].to_lowercase() == target
    }) {
        if !allowed.contains(&node.id())
            || initialized_at.is_none_or(|start| node.start_byte() < start)
        {
            return Err(invalid(*node));
        }
    }
    version.ok_or_else(|| invalid(function))
}

/// Recognize a direct parameterless call, without a member access or local computation.
pub(super) fn is_version_call(expression: Node<'_>, text: &str) -> bool {
    let call = unwrapped(expression);
    call.kind() == "method_call"
        && call
            .child_by_field_name("name")
            .is_some_and(|name| text[name.byte_range()].to_lowercase() == "версия")
        && call
            .child_by_field_name("arguments")
            .is_some_and(|arguments| named_children(arguments).is_empty())
}

/// A new empty structure or the standard BSP factory establishes the target's origin.
fn is_constructor(expression: Node<'_>, text: &str) -> bool {
    let value = unwrapped(expression);
    if value.kind() == "new_expression" {
        return value.child_by_field_name("type").is_some_and(|kind| {
            matches!(
                text[kind.byte_range()].to_lowercase().as_str(),
                "структура" | "structure"
            )
        }) && value
            .child_by_field_name("arguments")
            .is_none_or(|arguments| named_children(arguments).is_empty());
    }
    member(value).is_some_and(|(receiver, method)| {
        text[receiver.byte_range()].to_lowercase() == "дополнительныеотчетыиобработки"
            && method.child_by_field_name("name").is_some_and(|name| {
                text[name.byte_range()].to_lowercase() == "сведенияовнешнейобработке"
            })
    })
}

/// Only a single identifier receiver is accepted; indexed and chained targets are opaque.
fn member(node: Node<'_>) -> Option<(Node<'_>, Node<'_>)> {
    if !matches!(node.kind(), "property_access" | "call_expression") {
        return None;
    }
    let children = named_children(node);
    let [access, member] = children.as_slice() else {
        return None;
    };
    if access.kind() != "access" {
        return None;
    }
    let receivers = named_children(*access);
    let [receiver] = receivers.as_slice() else {
        return None;
    };
    if receiver.kind() != "identifier" {
        return None;
    }
    Some((*receiver, *member))
}

/// Fixed insert keys and migration literals must not evaluate arbitrary expressions.
pub(super) fn literal_text<'a>(expression: Node<'_>, text: &'a str) -> Option<&'a str> {
    let literal = unwrapped(expression);
    (literal.kind() == "string")
        .then(|| &text[literal.byte_range()])?
        .strip_prefix('"')?
        .strip_suffix('"')
}

/// Local variable declarations/assignments may shadow the otherwise direct version call.
fn shadows_version(nodes: &[Node<'_>], text: &str) -> bool {
    nodes.iter().any(|node| {
        node.kind() == "assignment_statement"
            && node.child_by_field_name("left").is_some_and(|left| {
                left.kind() == "identifier" && text[left.byte_range()].to_lowercase() == "версия"
            })
            || matches!(node.kind(), "var_definition" | "parameter")
                && descendants(*node).iter().any(|child| {
                    child.kind() == "identifier"
                        && text[child.byte_range()].to_lowercase() == "версия"
                })
    })
}

/// Module variables have no enclosing function, even when wrapped by preprocessor nodes.
fn inside_function(mut node: Node<'_>) -> bool {
    while let Some(parent) = node.parent() {
        if matches!(
            parent.kind(),
            "function_definition" | "procedure_definition"
        ) {
            return true;
        }
        node = parent;
    }
    false
}

#[cfg(test)]
mod tests;
