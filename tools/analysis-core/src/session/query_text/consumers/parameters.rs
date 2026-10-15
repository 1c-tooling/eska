//! Explicit current DCS types use the ordinary adapter and native resolver, without a name/global fallback.

use eska::{
    analysis_core::InputRequest,
    project_core::{metadata_parser::LocatedProperty, source_format::QueryConsumerScope},
};

use super::{QueryCancellation, QueryConsumerContext, QueryConsumerRequestError, check_cancelled};

/// Every declaration participates in ambiguity even when its type or list-valued shape is unsupported.
pub(super) fn types(
    context: &QueryConsumerContext,
    request: &InputRequest,
    cancellation: &QueryCancellation,
) -> Result<ide::QueryParameterTypes, QueryConsumerRequestError> {
    let mut types = ide::QueryParameterTypes::default();
    for declaration in context.consumer.parameter_declarations() {
        check_cancelled(request, cancellation)?;
        if declaration.scope != QueryConsumerScope::Resource {
            continue;
        }
        let ty = if declaration
            .value_list_allowed
            .as_ref()
            .is_some_and(|flag| flag.value)
        {
            None
        } else {
            declaration.value_type.as_ref().and_then(|value| {
                eska_bsl_adapter::attribute_type(&LocatedProperty {
                    property: value.value.clone(),
                    range: value.native_range.clone(),
                })
                .ok()
            })
        };
        types.declare(&declaration.name.value, ty);
    }
    Ok(types)
}
