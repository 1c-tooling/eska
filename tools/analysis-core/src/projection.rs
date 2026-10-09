//! Parsed input readiness and the supported kernel projection are different facts.

use std::sync::Arc;

use eska::{
    analysis_core::{InputFailure, InputOrigin, InputRevision, InputState, OwnerInput},
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

/// Coordinates and model coverage belong to exactly one current owner revision.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OwnerModel {
    pub revision: InputRevision,
    pub origin: InputOrigin,
    pub source_sha256: Option<String>,
    pub descriptors: usize,
    /// Only the main descriptor is currently projected; nested descriptors remain visible here.
    pub modeled_descriptors: usize,
    pub state: ModelState,
}

/// Project only the declared main descriptor, retaining explicit failure and coverage states.
pub fn project(kind: MetadataKind, input: &OwnerInput) -> OwnerModel {
    let mut result = OwnerModel {
        revision: input.revision,
        origin: input.origin,
        source_sha256: input.source_sha256.clone(),
        descriptors: 0,
        modeled_descriptors: 0,
        state: ModelState::Pending,
    };
    result.state = match &input.state {
        InputState::Pending => ModelState::Pending,
        InputState::Unavailable(failure) => ModelState::Unavailable(failure.clone()),
        InputState::Ready(snapshot) => {
            result.descriptors = snapshot.documents.len();
            if kind == MetadataKind::Configuration {
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
        MetadataKind::CommonModule => adapter::common_module(root)
            .map(|model| CanonicalMetadata::CommonModule(Arc::new(model))),
        MetadataKind::DefinedType => {
            adapter::defined_type(root).map(|model| CanonicalMetadata::DefinedType(Arc::new(model)))
        }
        MetadataKind::Catalog
        | MetadataKind::Document
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
