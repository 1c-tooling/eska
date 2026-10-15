//! Explicit data-parameter facts share the existing namespace-aware metadata value parser.

use roxmltree::Node;

use super::{
    DCS, Error, QueryConsumerScope, QueryConsumerView, QueryContextView, QueryParameterDeclaration,
    invalid, located, native, scalar,
};
use crate::project_core::metadata_parser::values;

/// A direct schema declaration never grants types to implicit or unrelated query parameters.
pub(super) fn read(
    context: &QueryContextView,
    node: Node<'_, '_>,
    scope: QueryConsumerScope,
    view: &mut QueryConsumerView,
) -> Result<(), Error> {
    let name = scalar(
        context,
        native::direct(node, DCS, "name")?.ok_or_else(invalid)?,
    )?;
    if name.value.trim().is_empty() {
        return Err(invalid());
    }
    let value_type = native::direct(node, DCS, "valueType")?
        .map(|node| {
            let mut diagnostics = Vec::new();
            let value = values::property(node, &mut diagnostics);
            located(context, value, node.range())
        })
        .transpose()?;
    let expression = native::direct(node, DCS, "expression")?
        .map(|node| scalar(context, node))
        .transpose()?;
    let value_list_allowed = native::direct(node, DCS, "valueListAllowed")?
        .map(|node| {
            let value = scalar(context, node)?;
            let flag = match value.value.as_str() {
                "true" => true,
                "false" => false,
                _ => return Err(invalid()),
            };
            located(context, flag, value.native_range)
        })
        .transpose()?;
    view.parameters.push(QueryParameterDeclaration {
        scope,
        name,
        value_type,
        expression,
        value_list_allowed,
    });
    Ok(())
}
