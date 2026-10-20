//! Explicit full descriptor traversal reuses the same resolver and parser as the metadata tree.

use std::collections::BTreeSet;

use super::{
    Project,
    designer_source::{self, SourceError},
    metadata_model::{ObjectId, ProjectScope},
    metadata_parser::{self, LoadError, PropertiesMode},
};

/// A failed or unsupported descriptor cannot be presented as a completed structural check.
#[derive(Debug)]
pub enum ValidationError {
    Source(SourceError),
    Load(LoadError),
    Unsupported(ObjectId),
    TooManyDescriptors,
    Cancelled,
}

/// Validate every declared descriptor, including children absent from a lazily opened tree.
///
/// This checks ESKA's supported metadata structure, not platform compilation or opaque form data.
/// No cache or source file is written. Cancellation is checked before each bounded descriptor read.
///
/// # Errors
/// Rejects missing/unsafe descriptors, parser diagnostics, cancellation and excessive graphs.
pub fn validate(
    project: Project,
    scope: ProjectScope,
    cancelled: &dyn Fn() -> bool,
) -> Result<usize, ValidationError> {
    let source =
        designer_source::open_project(project, scope, false).map_err(ValidationError::Source)?;
    let mut pending = vec![source.root().id().clone()];
    let mut seen = BTreeSet::new();
    while let Some(id) = pending.pop() {
        if cancelled() {
            return Err(ValidationError::Cancelled);
        }
        if !seen.insert(id.clone()) {
            continue;
        }
        if seen.len() > 1_000_000 {
            return Err(ValidationError::TooManyDescriptors);
        }
        let parsed = metadata_parser::load(&source, &id, PropertiesMode::Summary)
            .map_err(ValidationError::Load)?;
        if !parsed.diagnostics.is_empty() {
            return Err(ValidationError::Unsupported(id));
        }
        pending.extend(parsed.references.into_iter().map(|reference| reference.id));
    }
    Ok(seen.len())
}
