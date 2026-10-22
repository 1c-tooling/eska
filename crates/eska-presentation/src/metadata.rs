//! Shared Configurator notation for physical metadata ownership paths.

use crate::localization::Localizer;
use eska_project_core::project::metadata::MetadataPath;

/// Render a logical metadata identity in Configurator notation.
#[must_use]
pub fn render_metadata_path(path: &MetadataPath, localizer: &Localizer) -> String {
    path.parts
        .iter()
        .map(|part| {
            let kind = metadata_kind(part.kind, localizer);
            part.name
                .as_ref()
                .map_or_else(|| kind.clone(), |name| format!("{kind}.{name}"))
        })
        .collect::<Vec<_>>()
        .join(".")
}

/// Resolve the localized Configurator name of a stable metadata kind.
#[must_use]
pub fn metadata_kind(kind: &str, localizer: &Localizer) -> String {
    localizer.text(&format!("diff-metadata-{kind}"))
}
