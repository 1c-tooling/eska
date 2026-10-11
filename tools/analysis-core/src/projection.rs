//! Parsed input readiness and the supported kernel projection are different facts.

use std::sync::Arc;

use eska::{
    analysis_core::{InputFailure, InputOrigin, InputState, OwnerInput},
    project_core::{MetadataKind, source_format::SourcePath},
};
use eska_bsl_adapter::AdapterError;
use ide_db::metadata::CanonicalMetadata;

/// An unsuccessful current value never exposes the previous canonical model.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ModelState {
    Pending,
    Ready(Arc<CanonicalMetadata>),
    /// Configuration structure is validated, but is not a canonical value in the kernel API.
    StructureOnly,
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
    /// Only the main descriptor is currently projected; nested descriptors remain visible here.
    pub modeled_descriptors: usize,
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

/// Project only the declared main descriptor, retaining explicit failure and coverage states.
pub fn project(
    kind: MetadataKind,
    input: &OwnerInput,
    revision: crate::SessionRevision,
) -> OwnerModel {
    let mut result = OwnerModel {
        revision,
        origin: input.origin,
        source_sha256: input.source_sha256.clone(),
        descriptors: 0,
        parse_issues: 0,
        modeled_descriptors: 0,
        state: ModelState::Pending,
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
        | MetadataKind::Report => adapter::metadata_object(descriptor)
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
