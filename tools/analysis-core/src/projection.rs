//! Parsed input readiness and the supported kernel projection are different facts.

use std::sync::Arc;

use eska::{
    analysis_core::{InputFailure, InputOrigin, InputState, OwnerInput},
    project_core::{
        MetadataKind,
        source_format::{CurrentFormView, SourcePath},
    },
};
use eska_bsl_adapter::AdapterError;
use ide_db::metadata::CanonicalMetadata;

/// An unsuccessful current value never exposes the previous canonical model.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ModelState {
    Pending,
    Ready(Arc<CanonicalMetadata>),
    /// Validated source structure has no canonical value in the kernel API.
    StructureOnly,
    /// Current source is available, while canonical projection is outside the admitted scope.
    SourceOnly,
    Unsupported(MetadataKind),
    Unavailable(InputFailure),
    Invalid {
        /// The edited owner YAML; the adapter range is measured in this current document.
        source: SourcePath,
        error: AdapterError,
    },
}

/// Tree/property consumers receive the same current YAML snapshot as the semantic adapter.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MetadataView {
    pub revision: crate::SessionRevision,
    pub source: SourcePath,
    pub origin: InputOrigin,
    pub source_sha256: Option<String>,
    /// Unsupported BSL projection does not hide valid metadata properties from the IDE tree.
    pub input: InputState,
    pub model: OwnerModel,
}

/// Coordinates and model coverage belong to exactly one current owner revision.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OwnerModel {
    pub revision: crate::SessionRevision,
    pub origin: InputOrigin,
    pub source_sha256: Option<String>,
    pub descriptors: usize,
    pub parse_issues: usize,
    /// Count current canonical main and nested descriptors, excluding opaque resources.
    pub modeled_descriptors: usize,
    pub state: ModelState,
    pub forms: Vec<FormModel>,
}

/// A nested form keeps its full native identity and the revision of its own dependencies.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FormModel {
    pub native_path: SourcePath,
    pub dialog: Option<SourcePath>,
    pub revision: crate::SessionRevision,
    pub state: ModelState,
}

impl OwnerModel {
    /// This predicate covers parsed descriptors only, never BSL or opaque resource semantics.
    pub(super) const fn descriptors_complete(&self) -> bool {
        matches!(self.state, ModelState::Ready(_) | ModelState::StructureOnly)
            && self.parse_issues == 0
            && self.descriptors == self.modeled_descriptors
    }
}

/// Preserve parsed source coordinates and readiness without invoking any semantic adapter.
pub fn source_only(input: &OwnerInput, revision: crate::SessionRevision) -> OwnerModel {
    let (descriptors, parse_issues) = match &input.state {
        InputState::Ready(snapshot) => (
            snapshot.documents.len(),
            snapshot
                .documents
                .iter()
                .map(|document| document.descriptor.diagnostics.len())
                .sum(),
        ),
        _ => (0, 0),
    };
    OwnerModel {
        revision,
        origin: input.origin,
        source_sha256: input.source_sha256.clone(),
        descriptors,
        parse_issues,
        modeled_descriptors: 0,
        state: source_state(input),
        forms: Vec::new(),
    }
}

/// Missing or rejected current bytes remain unavailable rather than an empty source-only model.
pub fn source_state(input: &OwnerInput) -> ModelState {
    match &input.state {
        InputState::Ready(_) => ModelState::SourceOnly,
        InputState::Pending => ModelState::Pending,
        InputState::Unavailable(failure) => ModelState::Unavailable(failure.clone()),
    }
}

/// Project only the declared main descriptor, retaining explicit failure and coverage states.
pub fn project(
    kind: MetadataKind,
    input: &OwnerInput,
    revision: crate::SessionRevision,
    resource: Option<(&SourcePath, &crate::MetadataResourceInput)>,
) -> OwnerModel {
    let mut result = OwnerModel {
        revision,
        origin: input.origin,
        source_sha256: input.source_sha256.clone(),
        descriptors: 0,
        parse_issues: 0,
        modeled_descriptors: 0,
        state: ModelState::Pending,
        forms: Vec::new(),
    };
    result.state = match &input.state {
        InputState::Pending => ModelState::Pending,
        InputState::Unavailable(failure) => ModelState::Unavailable(failure.clone()),
        InputState::Ready(snapshot) => {
            result.descriptors = snapshot.documents.len();
            result.parse_issues = snapshot
                .documents
                .iter()
                .map(|document| document.descriptor.diagnostics.len())
                .sum();
            if kind == MetadataKind::Configuration {
                result.modeled_descriptors = 1;
                ModelState::StructureOnly
            } else if let Some(document) = snapshot.documents.first() {
                if eska_bsl_adapter::is_structural_descriptor_kind(kind) {
                    match eska_bsl_adapter::structural_descriptor(&document.descriptor) {
                        Ok(()) => {
                            result.modeled_descriptors = 1;
                            ModelState::StructureOnly
                        }
                        Err(error) => ModelState::Invalid {
                            source: snapshot.path.clone(),
                            error,
                        },
                    }
                } else if matches!(
                    kind,
                    MetadataKind::Role
                        | MetadataKind::CommonForm
                        | MetadataKind::Form
                        | MetadataKind::Catalog
                        | MetadataKind::ChartOfAccounts
                        | MetadataKind::ChartOfCalculationTypes
                        | MetadataKind::ChartOfCharacteristicTypes
                ) {
                    let state = resource_state(
                        &snapshot.path,
                        &document.descriptor,
                        kind,
                        resource,
                        snapshot.form.as_deref().map(CurrentFormView::reference_xml),
                    );
                    if matches!(state, ModelState::Ready(_)) {
                        result.modeled_descriptors = 1;
                    }
                    state
                } else {
                    match canonical(kind, &document.descriptor) {
                        Some(Ok(value)) => {
                            result.modeled_descriptors = 1;
                            ModelState::Ready(Arc::new(value))
                        }
                        Some(Err(error)) => ModelState::Invalid {
                            source: snapshot.path.clone(),
                            error,
                        },
                        None => ModelState::Unsupported(kind),
                    }
                }
            } else {
                ModelState::Invalid {
                    source: snapshot.path.clone(),
                    error: AdapterError {
                        issue: eska_bsl_adapter::AdapterIssue::MissingRoot,
                        range: 0..0,
                    },
                }
            }
        }
    };
    result
}

/// Only a current declared descriptor and its own dialog can produce a nested form value.
pub fn form_state(
    input: &OwnerInput,
    descriptor: Option<&eska::project_core::source_format::NativeMetadata>,
    native_path: &SourcePath,
    resource: Option<(&SourcePath, &crate::MetadataResourceInput)>,
) -> ModelState {
    match &input.state {
        InputState::Pending => ModelState::Pending,
        InputState::Unavailable(failure) => ModelState::Unavailable(failure.clone()),
        InputState::Ready(snapshot) => descriptor.map_or_else(
            || resource_failure(native_path, "analysis_form_descriptor_undeclared"),
            |document| {
                resource_state(
                    &snapshot.path,
                    &document.descriptor,
                    MetadataKind::Form,
                    resource,
                    snapshot.form.as_deref().map(CurrentFormView::reference_xml),
                )
            },
        ),
    }
}

/// A declared resource is an explicit dependency; its failure cannot become absent/default data.
fn resource_state(
    source: &SourcePath,
    descriptor: &eska::project_core::metadata_parser::ParsedDescriptor,
    kind: MetadataKind,
    resource: Option<(&SourcePath, &crate::MetadataResourceInput)>,
    current_form: Option<&str>,
) -> ModelState {
    use crate::{MetadataResourceKind, MetadataResourceState};
    if let Some(issue) = descriptor.diagnostics.first() {
        return ModelState::Invalid {
            source: source.clone(),
            error: AdapterError {
                issue: eska_bsl_adapter::AdapterIssue::IncompleteDescriptor,
                range: issue.range.clone(),
            },
        };
    }
    let current = match resource {
        None => None,
        Some((path, input)) => {
            let expected = match kind {
                MetadataKind::Role => MetadataResourceKind::RoleRights,
                MetadataKind::CommonForm | MetadataKind::Form => MetadataResourceKind::FormDialog,
                _ => MetadataResourceKind::PredefinedData,
            };
            if input.kind != expected {
                return resource_failure(path, "analysis_metadata_resource_kind_mismatch");
            }
            match &input.state {
                MetadataResourceState::Pending => return ModelState::Pending,
                MetadataResourceState::Unavailable(failure) => {
                    return ModelState::Unavailable(failure.clone());
                }
                MetadataResourceState::Ready(text) => Some((path, text)),
            }
        }
    };
    let current = if matches!(kind, MetadataKind::CommonForm | MetadataKind::Form) {
        current_form
            .map(|text| (source, text))
            .or_else(|| current.map(|(path, text)| (path, text.as_ref())))
    } else {
        current.map(|(path, text)| (path, text.as_ref()))
    };
    let value = if kind == MetadataKind::Role {
        let data = match current {
            None => bsl_metadata::RoleData::default(),
            Some((path, text)) => match bsl_metadata::xml_parser::parse_rights_xml(text) {
                Ok(data) => data,
                Err(_) => return resource_failure(path, "analysis_role_rights_invalid"),
            },
        };
        eska_bsl_adapter::role(descriptor, data).map(|role| CanonicalMetadata::Role(Arc::new(role)))
    } else if matches!(kind, MetadataKind::CommonForm | MetadataKind::Form) {
        let Some((path, text)) = current else {
            // An absent/binary dialog does not contain the managed form's current bindings.
            return resource_failure(source, "analysis_form_dialog_undeclared");
        };
        let Ok(data) = bsl_metadata::xml_parser::parse_form_xml(text) else {
            return resource_failure(path, "analysis_form_dialog_invalid");
        };
        eska_bsl_adapter::form(descriptor, data).map(|form| CanonicalMetadata::Form(Arc::new(form)))
    } else {
        let items = match current {
            None => Vec::new(),
            Some((path, text)) => match bsl_metadata::xml_parser::try_parse_predefined_xml(text) {
                Ok(items) => items,
                Err(_) => return resource_failure(path, "analysis_predefined_data_invalid"),
            },
        };
        eska_bsl_adapter::metadata_object(descriptor, items)
            .map(|model| CanonicalMetadata::Object(Arc::new(model)))
    };
    match value {
        Ok(value) => ModelState::Ready(Arc::new(value)),
        Err(error) => ModelState::Invalid {
            source: source.clone(),
            error,
        },
    }
}

/// Resource failures have their own source and never manufacture a range in the owner YAML.
fn resource_failure(path: &SourcePath, code: &'static str) -> ModelState {
    ModelState::Unavailable(InputFailure::Source(
        eska::project_core::source_format::Error {
            code,
            location: vec![path.as_str().to_owned()],
        },
    ))
}

/// Match only implemented adapters; a parsed unknown family is not an empty successful model.
fn canonical(
    kind: MetadataKind,
    descriptor: &eska::project_core::metadata_parser::ParsedDescriptor,
) -> Option<Result<CanonicalMetadata, AdapterError>> {
    use eska_bsl_adapter as adapter;
    if let Some(issue) = descriptor.diagnostics.first() {
        return Some(Err(AdapterError {
            issue: adapter::AdapterIssue::IncompleteDescriptor,
            range: issue.range.clone(),
        }));
    }
    let Some(root) = descriptor.objects.first() else {
        return Some(Err(AdapterError {
            issue: adapter::AdapterIssue::MissingRoot,
            range: 0..0,
        }));
    };
    Some(match kind {
        MetadataKind::IntegrationService => adapter::integration_service(descriptor)
            .map(|model| CanonicalMetadata::IntegrationService(Arc::new(model))),
        MetadataKind::WebService => adapter::web_service(descriptor)
            .map(|model| CanonicalMetadata::WebService(Arc::new(model))),
        MetadataKind::HTTPService => adapter::http_service(descriptor)
            .map(|model| CanonicalMetadata::HTTPService(Arc::new(model))),
        MetadataKind::Subsystem => adapter::subsystem(descriptor)
            .map(|model| CanonicalMetadata::Subsystem(Arc::new(model))),
        MetadataKind::ScheduledJob => adapter::scheduled_job(root)
            .map(|model| CanonicalMetadata::ScheduledJob(Arc::new(model))),
        MetadataKind::EventSubscription => adapter::event_subscription(root)
            .map(|model| CanonicalMetadata::EventSubscription(Arc::new(model))),
        MetadataKind::CommonModule => adapter::common_module(root)
            .map(|model| CanonicalMetadata::CommonModule(Arc::new(model))),
        MetadataKind::SessionParameter => adapter::session_parameter(root)
            .map(|model| CanonicalMetadata::SessionParameter(Arc::new(model))),
        MetadataKind::CommonAttribute => adapter::common_attribute(root)
            .map(|model| CanonicalMetadata::CommonAttribute(Arc::new(model))),
        MetadataKind::CommonCommand => adapter::common_command(root)
            .map(|model| CanonicalMetadata::CommonCommand(Arc::new(model))),
        MetadataKind::FilterCriterion => adapter::filter_criterion(root)
            .map(|model| CanonicalMetadata::FilterCriterion(Arc::new(model))),
        MetadataKind::DefinedType => {
            adapter::defined_type(root).map(|model| CanonicalMetadata::DefinedType(Arc::new(model)))
        }
        MetadataKind::Catalog
        | MetadataKind::Document
        | MetadataKind::BusinessProcess
        | MetadataKind::Task
        | MetadataKind::ExchangePlan
        | MetadataKind::ChartOfCharacteristicTypes
        | MetadataKind::ChartOfAccounts
        | MetadataKind::ChartOfCalculationTypes
        | MetadataKind::DataProcessor
        | MetadataKind::Report => adapter::metadata_object(descriptor, Vec::new())
            .map(|model| CanonicalMetadata::Object(Arc::new(model))),
        MetadataKind::Constant => adapter::constant_object(descriptor)
            .map(|model| CanonicalMetadata::Object(Arc::new(model))),
        MetadataKind::Enum => {
            adapter::enum_object(descriptor).map(|model| CanonicalMetadata::Object(Arc::new(model)))
        }
        MetadataKind::InformationRegister
        | MetadataKind::AccumulationRegister
        | MetadataKind::AccountingRegister
        | MetadataKind::CalculationRegister => {
            adapter::register(descriptor).map(|model| CanonicalMetadata::Register(Arc::new(model)))
        }
        _ => return None,
    })
}
