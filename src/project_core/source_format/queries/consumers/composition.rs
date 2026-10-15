//! Explicit dataset fields retain full union ancestry and never infer output schema from query text.

use std::collections::BTreeSet;

use roxmltree::Node;

use super::{
    DCS, Error, QueryConsumerCoverage, QueryConsumerScope, QueryConsumerView, QueryContextView,
    QueryDataset, QueryDatasetField, QueryDatasetKind, SETTINGS, XSI, invalid, inventory, located,
    native, parameters, scalar,
};
use crate::project_core::source_format::QuerySlot;

/// Existing native helpers validate direct dataset scopes before this read-only storage projection.
pub(super) fn read(
    context: &QueryContextView,
    root: Node<'_, '_>,
    view: &mut QueryConsumerView,
) -> Result<(), Error> {
    let mut pending: Vec<_> = native::datasets(root, "dataSet")?
        .into_iter()
        .rev()
        .map(|node| (node, vec![]))
        .collect();
    let mut handled = BTreeSet::new();
    while let Some((node, mut path)) = pending.pop() {
        let name = native::direct(node, DCS, "name")?.ok_or_else(invalid)?;
        path.push(located(context, native::dataset_name(node)?, name.range())?);
        let datasets: Vec<_> = path.iter().map(|name| name.value.clone()).collect();
        let kind = match native::qualified_type(node, DCS)? {
            "DataSetQuery" => QueryDatasetKind::Query,
            "DataSetUnion" => QueryDatasetKind::Union,
            "DataSetObject" => QueryDatasetKind::Object,
            _ => return Err(invalid()),
        };
        let query = context
            .references()
            .iter()
            .find(|reference| {
                reference.slot
                    == QuerySlot::Composition {
                        datasets: datasets.clone(),
                    }
            })
            .cloned();
        if (kind == QueryDatasetKind::Query) != query.is_some() {
            return Err(invalid());
        }
        let fields = fields(context, node, &datasets, view)?;
        if kind == QueryDatasetKind::Union {
            pending.extend(
                native::datasets(node, "item")?
                    .into_iter()
                    .rev()
                    .map(|item| (item, path.clone())),
            );
        }
        view.datasets.push(QueryDataset {
            path,
            kind,
            query,
            fields,
        });
        handled.insert(node.range().start);
        for child in node.children().filter(Node::is_element) {
            if child.has_tag_name((DCS, "name"))
                || child.has_tag_name((DCS, "field"))
                || child.has_tag_name((DCS, "query"))
                || (kind == QueryDatasetKind::Union && child.has_tag_name((DCS, "item")))
            {
                continue;
            }
            inventory(
                context,
                view,
                child,
                QueryConsumerScope::Dataset {
                    datasets: datasets.clone(),
                },
                coverage(child),
            )?;
        }
    }
    for child in root.children().filter(Node::is_element) {
        if child.has_tag_name((DCS, "parameter")) {
            parameters::read(context, child, QueryConsumerScope::Resource, view)?;
        }
        if !handled.contains(&child.range().start) {
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

/// Only the observed `DataSetFieldField` shape has explicit bindings; other field types remain inventory.
fn fields(
    context: &QueryContextView,
    dataset: Node<'_, '_>,
    datasets: &[String],
    view: &mut QueryConsumerView,
) -> Result<Vec<QueryDatasetField>, Error> {
    let mut result = Vec::new();
    let mut paths = BTreeSet::new();
    for node in dataset
        .children()
        .filter(|node| node.has_tag_name((DCS, "field")))
    {
        if native::qualified_type(node, DCS) != Ok("DataSetFieldField") {
            inventory(
                context,
                view,
                node,
                QueryConsumerScope::Dataset {
                    datasets: datasets.to_vec(),
                },
                QueryConsumerCoverage::Unsupported,
            )?;
            continue;
        }
        if node
            .attributes()
            .any(|attribute| attribute.namespace() != Some(XSI) || attribute.name() != "type")
        {
            return Err(invalid());
        }
        let data_path = scalar(
            context,
            native::direct(node, DCS, "dataPath")?.ok_or_else(invalid)?,
        )?;
        let field = scalar(
            context,
            native::direct(node, DCS, "field")?.ok_or_else(invalid)?,
        )?;
        if data_path.value.trim().is_empty()
            || field.value.trim().is_empty()
            || !paths.insert(data_path.value.clone())
        {
            return Err(invalid());
        }
        for child in node.children().filter(Node::is_element) {
            if !child.has_tag_name((DCS, "dataPath")) && !child.has_tag_name((DCS, "field")) {
                inventory(
                    context,
                    view,
                    child,
                    QueryConsumerScope::Dataset {
                        datasets: datasets.to_vec(),
                    },
                    coverage(child),
                )?;
            }
        }
        result.push(QueryDatasetField { data_path, field });
    }
    Ok(result)
}

/// Familiar native containers remain incomplete; foreign namespaces never acquire native semantics.
fn coverage(node: Node<'_, '_>) -> QueryConsumerCoverage {
    if matches!(node.tag_name().namespace(), Some(DCS | SETTINGS)) {
        QueryConsumerCoverage::Incomplete
    } else {
        QueryConsumerCoverage::Unsupported
    }
}
