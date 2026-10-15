//! Absence proofs use only complete names and the exact storage query-reference binding.

use eska::analysis_core::InputRequest;
use eska::project_core::source_format::{QueryDatasetKind, QueryLocated, QuerySlot};
use ide::{QuerySelectStatement, QueryTextModel, QueryTextStatement};
use std::collections::HashSet;

use super::{
    QueryCancellation, QueryConsumerCheckCoverage, QueryConsumerContext, QueryConsumerDiagnostics,
    QueryConsumerFinding, QueryConsumerIncomplete, QueryConsumerIncompleteReason as Reason,
    QueryConsumerLocation, QueryConsumerRequestError, QueryConsumerTarget, check_cancelled,
};

/// Shape incompleteness permits a partial result; cancellation refuses publication entirely.
enum OutputError {
    Incomplete(Reason),
    Refused(QueryConsumerRequestError),
}

/// Unsupported shapes preserve native facts but cannot produce a missing-field finding.
pub(super) fn check(
    context: QueryConsumerContext,
    model: QueryTextModel,
    request: &InputRequest,
    cancellation: &QueryCancellation,
) -> Result<QueryConsumerDiagnostics, QueryConsumerRequestError> {
    let mut coverage = QueryConsumerCheckCoverage {
        output_names_complete: false,
        output_types_complete: false,
        checked_bindings: 0,
        incomplete: Vec::new(),
    };
    let mut findings = Vec::new();
    let references = context
        .consumer
        .datasets()
        .iter()
        .filter_map(|dataset| dataset.query.as_ref())
        .chain(
            context
                .consumer
                .dynamic_lists()
                .iter()
                .filter_map(|list| list.query.as_ref()),
        )
        .filter(|query| query.source == context.query.source)
        .count();
    let output = match references {
        0 => Err(OutputError::Incomplete(Reason::UnboundQueryReference)),
        1 => output(&model, request, cancellation),
        _ => Err(OutputError::Incomplete(Reason::AmbiguousQueryReference)),
    };
    match output {
        Ok((select, names)) => {
            coverage.output_names_complete = true;
            coverage.output_types_complete = select.output_types_complete;
            dataset_fields(
                &context,
                &names,
                &mut findings,
                &mut coverage,
                request,
                cancellation,
            )?;
            form_fields(
                &context,
                &names,
                &mut findings,
                &mut coverage,
                request,
                cancellation,
            )?;
        }
        Err(OutputError::Incomplete(reason)) => incomplete(&mut coverage, reason, None),
        Err(OutputError::Refused(error)) => return Err(error),
    }
    for part in context.consumer.unhandled() {
        check_cancelled(request, cancellation)?;
        incomplete(
            &mut coverage,
            Reason::UnimplementedResourceParts,
            Some(location(&context, &part.node)),
        );
    }
    Ok(QueryConsumerDiagnostics {
        context,
        model,
        findings,
        coverage,
    })
}

/// One result set and unambiguous first-branch names are necessary for any absence proof.
fn output<'a>(
    model: &'a QueryTextModel,
    request: &InputRequest,
    cancellation: &QueryCancellation,
) -> Result<(&'a QuerySelectStatement, HashSet<String>), OutputError> {
    let select = output_shape(model).map_err(OutputError::Incomplete)?;
    let fields = &select
        .branches
        .first()
        .ok_or(OutputError::Incomplete(Reason::NoOutputRowset))?
        .fields;
    let mut names = HashSet::with_capacity(fields.len());
    for field in fields {
        check_cancelled(request, cancellation).map_err(OutputError::Refused)?;
        let name = field
            .name
            .as_deref()
            .ok_or(OutputError::Incomplete(Reason::OutputNamesIncomplete))?;
        if !names.insert(name_key(name)) {
            return Err(OutputError::Incomplete(Reason::AmbiguousOutputNames));
        }
    }
    Ok((select, names))
}

/// Native runtime issues and unresolved branch types cannot establish a usable output schema.
fn output_shape(model: &QueryTextModel) -> Result<&QuerySelectStatement, Reason> {
    if !model.syntax_complete {
        return Err(Reason::InvalidQuery);
    }
    if !model.temporary_table_model_complete {
        return Err(Reason::TemporaryTablePackage);
    }
    let mut selects = model
        .statements
        .iter()
        .filter_map(|statement| match statement {
            QueryTextStatement::Select(select) if select.into_table.is_none() => Some(select),
            QueryTextStatement::Select(_) | QueryTextStatement::Drop { .. } => None,
        });
    let select = selects.next().ok_or(Reason::NoOutputRowset)?;
    if selects.next().is_some() {
        return Err(Reason::MultipleOutputRowsets);
    }
    if !select.output_names_complete {
        return Err(Reason::OutputNamesIncomplete);
    }
    if model.diagnostics.iter().any(|issue| {
        issue.is_error() || matches!(issue, ide::QueryNativeIssue::AliasRequired { .. })
    }) {
        return Err(Reason::InvalidQueryMetadata);
    }
    if !model.native_expressions_supported {
        return Err(Reason::UnsupportedNativeExpression);
    }
    if select
        .branches
        .iter()
        .any(|branch| branch.fields.iter().any(|field| !field.type_complete))
    {
        return Err(Reason::OutputTypesIncomplete);
    }
    Ok(select)
}

/// A dataset with another query source or slot cannot inherit this query's output rowset.
fn dataset_fields(
    context: &QueryConsumerContext,
    names: &HashSet<String>,
    findings: &mut Vec<QueryConsumerFinding>,
    coverage: &mut QueryConsumerCheckCoverage,
    request: &InputRequest,
    cancellation: &QueryCancellation,
) -> Result<(), QueryConsumerRequestError> {
    for dataset in context.consumer.datasets().iter().filter(|dataset| {
        dataset.kind == QueryDatasetKind::Query
            && dataset.query.as_ref().is_some_and(|query| {
                query.source == context.query.source
                    && query.slot
                        == QuerySlot::Composition {
                            datasets: dataset.path.iter().map(|part| part.value.clone()).collect(),
                        }
            })
    }) {
        for field in &dataset.fields {
            check_cancelled(request, cancellation)?;
            check_field(
                context,
                names,
                &field.field.value,
                &field.field,
                QueryConsumerTarget::Dataset {
                    path: dataset.path.iter().map(|part| part.value.clone()).collect(),
                    data_path: field.data_path.value.clone(),
                },
                findings,
                coverage,
            );
        }
    }
    Ok(())
}

/// A manual list's direct UI field is distinct from its table binding or an unrelated attribute.
fn form_fields(
    context: &QueryConsumerContext,
    names: &HashSet<String>,
    findings: &mut Vec<QueryConsumerFinding>,
    coverage: &mut QueryConsumerCheckCoverage,
    request: &InputRequest,
    cancellation: &QueryCancellation,
) -> Result<(), QueryConsumerRequestError> {
    for list in context.consumer.dynamic_lists().iter().filter(|list| {
        list.manual_query.value
            && list.query.as_ref().is_some_and(|query| {
                query.source == context.query.source
                    && query.slot
                        == QuerySlot::DynamicList {
                            attribute: list.attribute.value.clone(),
                        }
            })
    }) {
        check_cancelled(request, cancellation)?;
        let list_name = name_key(&list.name.value);
        if context
            .consumer
            .dynamic_lists()
            .iter()
            .filter(|other| list_name == name_key(&other.name.value))
            .count()
            != 1
        {
            incomplete(
                coverage,
                Reason::AmbiguousDynamicList,
                Some(location(context, &list.name)),
            );
            continue;
        }
        if list
            .main_table
            .as_ref()
            .is_some_and(|table| !table.value.trim().is_empty())
        {
            incomplete(
                coverage,
                Reason::PlatformEnrichedDynamicList,
                Some(location(context, &list.name)),
            );
            continue;
        }
        for path in context.consumer.form_data_paths() {
            check_cancelled(request, cancellation)?;
            let Some((prefix, field)) = path.data_path.value.split_once('.') else {
                continue;
            };
            if list_name != name_key(prefix) {
                continue;
            }
            check_field(
                context,
                names,
                field,
                &path.data_path,
                QueryConsumerTarget::DynamicList {
                    attribute: list.attribute.value.clone(),
                    element: path.element.value.clone(),
                },
                findings,
                coverage,
            );
        }
    }
    Ok(())
}

/// Direct identifiers are checked case-insensitively using the native metadata-name semantics.
fn check_field(
    context: &QueryConsumerContext,
    names: &HashSet<String>,
    field: &str,
    binding: &QueryLocated<String>,
    target: QueryConsumerTarget,
    findings: &mut Vec<QueryConsumerFinding>,
    coverage: &mut QueryConsumerCheckCoverage,
) {
    let location = location(context, binding);
    let mut characters = field.chars();
    if !characters
        .next()
        .is_some_and(|first| first.is_alphabetic() || first == '_')
        || !characters.all(|character| character.is_alphanumeric() || character == '_')
    {
        incomplete(coverage, Reason::UnsupportedFieldPath, Some(location));
        return;
    }
    coverage.checked_bindings += 1;
    if !names.contains(&name_key(field)) {
        findings.push(QueryConsumerFinding {
            target,
            output_field: field.to_owned(),
            location,
        });
    }
}

/// Pinned native `stdx::case::eq_ignore_case` folds characters, without contextual final-sigma rules.
fn name_key(name: &str) -> String {
    name.chars().flat_map(char::to_lowercase).collect()
}

/// Native spans refer to reference XML; authoring precision remains exactly as supplied by the core.
fn location<T>(context: &QueryConsumerContext, binding: &QueryLocated<T>) -> QueryConsumerLocation {
    QueryConsumerLocation {
        source: context.consumer.source().clone(),
        native_resource: context.consumer.native_resource().clone(),
        native_range: binding.native_range.clone(),
        authoring: binding.authoring.clone(),
    }
}

/// Incompleteness is retained independently from findings, including clean supported bindings.
fn incomplete(
    coverage: &mut QueryConsumerCheckCoverage,
    reason: Reason,
    location: Option<QueryConsumerLocation>,
) {
    coverage
        .incomplete
        .push(QueryConsumerIncomplete { reason, location });
}
