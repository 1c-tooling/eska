//! Namespace-aware storage consumers use the same checked native parser and current read-view locations.

use std::ops::Range;

use roxmltree::Node;

use super::native;
use crate::project_core::source_format::{Error, QueryContextView, QueryProfile, xml};

mod composition;
mod form;
mod model;
#[cfg(test)]
mod tests;

pub use model::{
    QueryConsumerCoverage, QueryConsumerScope, QueryConsumerView, QueryDataset, QueryDatasetField,
    QueryDatasetKind, QueryDynamicList, QueryFormDataPath, QueryLocated, QueryUnhandledKind,
    QueryUnhandledPart, QueryXmlName,
};

const DCS: &str = "http://v8.1c.ru/8.1/data-composition-system/schema";
const FORM: &str = "http://v8.1c.ru/8.3/xcf/logform";
const SETTINGS: &str = "http://v8.1c.ru/8.1/data-composition-system/settings";
const XSI: &str = "http://www.w3.org/2001/XMLSchema-instance";

/// Traversal borrows current reference XML and never opens query values, a baseline or another reader.
pub(in crate::project_core::source_format) fn consumer_view(
    context: &QueryContextView,
) -> Result<QueryConsumerView, Error> {
    let document = native::parse(context.reference_xml())?;
    let mut view = QueryConsumerView {
        owner: context.owner().clone(),
        descriptor: context.native_descriptor().clone(),
        source: context.source().clone(),
        native: context.native_resource().clone(),
        uuid: context.uuid().clone(),
        profile: context.profile(),
        datasets: vec![],
        dynamic_lists: vec![],
        form_data_paths: vec![],
        unhandled: vec![],
    };
    match context.profile() {
        QueryProfile::Composition => composition::read(context, document.root_element(), &mut view),
        QueryProfile::ManagedForm => form::read(context, document.root_element(), &mut view),
    }
    .map_err(|error| error.in_field(context.source().as_str()))?;
    Ok(view)
}

/// Every published native span is checked by the current read view before acquiring authoring coordinates.
fn located<T>(
    context: &QueryContextView,
    value: T,
    native_range: Range<usize>,
) -> Result<QueryLocated<T>, Error> {
    Ok(QueryLocated {
        authoring: context.locate_reference_range(native_range.clone())?,
        value,
        native_range,
    })
}

/// Only a direct scalar node is decoded; its whole XML node remains the proven native location.
fn scalar(context: &QueryContextView, node: Node<'_, '_>) -> Result<QueryLocated<String>, Error> {
    if node.attributes().len() != 0 || node.children().any(|child| child.is_element()) {
        return Err(invalid());
    }
    located(context, xml::text(node), node.range())
}

/// Attribute values keep their actual XML byte range instead of being found by equal text.
fn attribute(
    context: &QueryContextView,
    node: Node<'_, '_>,
    name: &str,
) -> Result<QueryLocated<String>, Error> {
    let value = node
        .attributes()
        .find(|attribute| attribute.namespace().is_none() && attribute.name() == name)
        .ok_or_else(invalid)?;
    located(context, value.value().into(), value.range_value())
}

/// Unhandled subtrees retain actual namespace, scope and explicit unsupported/incomplete coverage.
fn inventory(
    context: &QueryContextView,
    view: &mut QueryConsumerView,
    node: Node<'_, '_>,
    scope: QueryConsumerScope,
    coverage: QueryConsumerCoverage,
) -> Result<(), Error> {
    let namespace = node.tag_name().namespace();
    let local = node.tag_name().name();
    let kind = match (namespace, local) {
        (Some(DCS), "parameter") => QueryUnhandledKind::Parameter,
        (Some(DCS), "dataSetLink") => QueryUnhandledKind::DatasetLink,
        (Some(DCS), "calculatedField") => QueryUnhandledKind::CalculatedField,
        (Some(DCS), "totalField") => QueryUnhandledKind::TotalField,
        (Some(DCS), "settingsVariant") | (Some(FORM), "ListSettings") | (Some(SETTINGS), _) => {
            QueryUnhandledKind::Settings
        }
        _ => QueryUnhandledKind::Other,
    };
    view.unhandled.push(QueryUnhandledPart {
        scope,
        kind,
        coverage,
        node: located(
            context,
            QueryXmlName {
                namespace: namespace.map(str::to_owned),
                local: local.into(),
            },
            node.range(),
        )?,
    });
    Ok(())
}

/// A malformed supported shape refuses publication instead of supplying an empty or guessed field.
fn invalid() -> Error {
    Error::root("source_query_consumer_invalid")
}
