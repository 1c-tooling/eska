//! Dynamic-list attributes and UI `DataPath` properties retain their separate verified numeric scopes.

use roxmltree::Node;

use super::{
    Error, FORM, QueryConsumerCoverage, QueryConsumerScope, QueryConsumerView, QueryContextView,
    QueryDynamicList, QueryFormDataPath, SETTINGS, attribute, invalid, inventory, located, native,
    scalar,
};
use crate::project_core::source_format::{FormId, ManagedFormLayout, QuerySlot};

/// A validated form layout supplies real UI identities without resolving `DataPath` expressions.
pub(super) fn read(
    context: &QueryContextView,
    root: Node<'_, '_>,
    view: &mut QueryConsumerView,
) -> Result<(), Error> {
    let layout = ManagedFormLayout::from_xml(context.reference_xml())?;
    if let Some(attributes) = native::direct(root, FORM, "Attributes")? {
        for node in attributes.children().filter(Node::is_element) {
            if node.has_tag_name((FORM, "Attribute")) {
                dynamic_list(context, node, view)?;
            } else {
                inventory(
                    context,
                    view,
                    node,
                    QueryConsumerScope::Resource,
                    QueryConsumerCoverage::Incomplete,
                )?;
            }
        }
    }
    for node in root.descendants().filter(Node::is_element) {
        if node.tag_name().namespace() != Some(FORM)
            || node.attribute("id").is_none()
            || matches!(node.tag_name().name(), "Attribute" | "Column" | "Command")
        {
            continue;
        }
        let element = attribute(context, node, "id")?;
        let id = FormId::parse(&element.value)?;
        let declaration = layout.elements.get(&id).ok_or_else(invalid)?;
        if let Some(data_path) = native::direct(node, FORM, "DataPath")? {
            view.form_data_paths.push(QueryFormDataPath {
                element: located(context, id.clone(), element.native_range)?,
                element_kind: declaration.kind,
                element_name: attribute(context, node, "name")?,
                data_path: scalar(context, data_path)?,
            });
        }
        for child in node.children().filter(Node::is_element) {
            if !child.has_tag_name((FORM, "DataPath"))
                && !child.has_tag_name((FORM, "ChildItems"))
                && child.attribute("id").is_none()
            {
                inventory(
                    context,
                    view,
                    child,
                    QueryConsumerScope::FormElement {
                        element: id.clone(),
                    },
                    coverage(child),
                )?;
            }
        }
    }
    for child in root.children().filter(Node::is_element) {
        if !child.has_tag_name((FORM, "Attributes"))
            && !child.has_tag_name((FORM, "ChildItems"))
            && child.attribute("id").is_none()
        {
            inventory(
                context,
                view,
                child,
                QueryConsumerScope::Resource,
                coverage(child),
            )?;
        }
    }
    Ok(())
}

/// Only direct, namespace-checked `DynamicList` settings bind an attribute to saved query text.
fn dynamic_list(
    context: &QueryContextView,
    node: Node<'_, '_>,
    view: &mut QueryConsumerView,
) -> Result<(), Error> {
    let identity = attribute(context, node, "id")?;
    let id = FormId::parse(&identity.value)?;
    let scope = QueryConsumerScope::FormAttribute {
        attribute: id.clone(),
    };
    let Some(settings) = native::direct(node, FORM, "Settings")? else {
        return inventory(
            context,
            view,
            node,
            scope,
            QueryConsumerCoverage::Incomplete,
        );
    };
    if native::qualified_type(settings, FORM) != Ok("DynamicList") {
        return inventory(
            context,
            view,
            node,
            scope,
            QueryConsumerCoverage::Unsupported,
        );
    }
    let manual = scalar(
        context,
        native::direct(settings, FORM, "ManualQuery")?.ok_or_else(invalid)?,
    )?;
    let manual_query = match manual.value.as_str() {
        "true" => true,
        "false" => false,
        _ => return Err(invalid()),
    };
    let query = context
        .references()
        .iter()
        .find(|reference| {
            reference.slot
                == QuerySlot::DynamicList {
                    attribute: id.clone(),
                }
        })
        .cloned();
    if manual_query != query.is_some() {
        return Err(invalid());
    }
    let main_table = native::direct(settings, FORM, "MainTable")?
        .map(|node| scalar(context, node))
        .transpose()?;
    view.dynamic_lists.push(QueryDynamicList {
        attribute: located(context, id, identity.native_range)?,
        name: attribute(context, node, "name")?,
        manual_query: located(context, manual_query, manual.native_range)?,
        query,
        main_table,
    });
    for child in settings.children().filter(Node::is_element) {
        if !child.has_tag_name((FORM, "ManualQuery"))
            && !child.has_tag_name((FORM, "QueryText"))
            && !child.has_tag_name((FORM, "MainTable"))
        {
            inventory(context, view, child, scope.clone(), coverage(child))?;
        }
    }
    for child in node.children().filter(Node::is_element) {
        if !child.has_tag_name((FORM, "Settings")) {
            inventory(context, view, child, scope.clone(), coverage(child))?;
        }
    }
    Ok(())
}

/// Foreign namespaces are unsupported even when their local property name resembles a native one.
fn coverage(node: Node<'_, '_>) -> QueryConsumerCoverage {
    if matches!(node.tag_name().namespace(), Some(FORM | SETTINGS)) {
        QueryConsumerCoverage::Incomplete
    } else {
        QueryConsumerCoverage::Unsupported
    }
}
